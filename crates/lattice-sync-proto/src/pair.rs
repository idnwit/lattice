//! Pairing contract (specs/01-pairing.md, Invite flow; D47): the invite and its secret.
//!
//! The deck makes an [`Invite`] and shows it as `lattice add-deck <code>`. The worker decodes
//! it, dials the deck, and proves the [`PairSecret`] in `Message::PairRequest`.

use std::fmt;
use std::net::SocketAddr;
use std::time::Duration;

use lattice_key::NodeId;
use serde::{Deserialize, Serialize};

/// Prefix of an invite code; the `1` is the format version.
pub const INVITE_PREFIX: &str = "lattice1:";

/// Bytes of a [`PairSecret`].
pub const PAIR_SECRET_LEN: usize = 32;

/// Lifetime of an invite, in seconds (specs/01-pairing.md, rule 1: 5 minutes).
pub const INVITE_TTL_SECS: u64 = 5 * 60;

/// Bound on one pairing connection's exchange, on each side (specs/01-pairing.md, rule 10;
/// proposed).
pub const PAIR_TIMEOUT: Duration = Duration::from_secs(10);

/// RFC 4648 base32 alphabet, lower case.
const BASE32: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";

/// Longest code [`Invite::decode`] reads. An IPv6 invite is about 140 characters.
const MAX_CODE_LEN: usize = 512;

/// The one-time secret of an invite. `Debug` never shows it.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairSecret([u8; PAIR_SECRET_LEN]);

impl PairSecret {
    /// Wraps secret bytes from the OS RNG.
    pub fn new(bytes: [u8; PAIR_SECRET_LEN]) -> Self {
        Self(bytes)
    }

    /// Compares in constant time, so timing does not leak how many bytes match.
    pub fn matches(&self, other: &PairSecret) -> bool {
        self.0
            .iter()
            .zip(other.0.iter())
            .fold(0u8, |acc, (a, b)| acc | (a ^ b))
            == 0
    }
}

impl fmt::Debug for PairSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PairSecret(<redacted>)")
    }
}

/// A deck's one-time offer to pair (specs/01-pairing.md, Invite code).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Invite {
    deck: NodeId,
    addr: SocketAddr,
    secret: PairSecret,
    expires: u64,
}

/// An invite code that does not decode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum InviteError {
    /// No `lattice1:` prefix: another format or another version.
    #[error("not a lattice1 invite")]
    Prefix,
    /// Not base32, too long, or a malformed payload.
    #[error("malformed invite")]
    Malformed,
}

impl InviteError {
    /// Stable code for logs and the CLI.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Prefix => "pair.invite_prefix",
            Self::Malformed => "pair.invite_malformed",
        }
    }
}

impl Invite {
    /// An invite from `deck` at `addr`, valid until `expires` (Unix seconds).
    pub fn new(deck: NodeId, addr: SocketAddr, secret: PairSecret, expires: u64) -> Self {
        Self {
            deck,
            addr,
            secret,
            expires,
        }
    }

    /// The deck to dial and pin.
    pub fn deck(&self) -> NodeId {
        self.deck
    }

    /// Where the deck listens.
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// The secret to prove in `PairRequest`.
    pub fn secret(&self) -> &PairSecret {
        &self.secret
    }

    /// Expiry, Unix seconds.
    pub fn expires(&self) -> u64 {
        self.expires
    }

    /// True once `now` (Unix seconds) reached the expiry.
    pub fn expired_at(&self, now: u64) -> bool {
        now >= self.expires
    }

    /// `lattice1:` + base32 of the postcard payload. Holds the secret: never log it.
    pub fn encode(&self) -> String {
        // An invite has no variable-length field that can fail to encode.
        let bytes = postcard::to_allocvec(self).unwrap_or_default();
        let mut out = String::with_capacity(
            INVITE_PREFIX
                .len()
                .saturating_add(bytes.len().saturating_mul(2)),
        );
        out.push_str(INVITE_PREFIX);
        base32_encode(&bytes, &mut out);
        out
    }

    /// Parses a code made by [`Invite::encode`]. Case and surrounding spaces are ignored.
    ///
    /// # Errors
    ///
    /// [`InviteError`] when the prefix, the base32, or the payload is wrong.
    pub fn decode(code: &str) -> Result<Self, InviteError> {
        let code = code.trim();
        if code.len() > MAX_CODE_LEN {
            return Err(InviteError::Malformed);
        }
        let prefix = code.get(..INVITE_PREFIX.len()).ok_or(InviteError::Prefix)?;
        if !prefix.eq_ignore_ascii_case(INVITE_PREFIX) {
            return Err(InviteError::Prefix);
        }
        let body = code.get(INVITE_PREFIX.len()..).unwrap_or_default();
        let bytes = base32_decode(body).ok_or(InviteError::Malformed)?;
        let (invite, rest) =
            postcard::take_from_bytes::<Invite>(&bytes).map_err(|_| InviteError::Malformed)?;
        if !rest.is_empty() {
            return Err(InviteError::Malformed);
        }
        Ok(invite)
    }
}

/// RFC 4648 base32, lower case, no padding.
fn base32_encode(bytes: &[u8], out: &mut String) {
    let (mut acc, mut bits) = (0u32, 0u32);
    for &b in bytes {
        acc = acc.wrapping_shl(8) | u32::from(b);
        bits = bits.wrapping_add(8);
        while bits >= 5 {
            bits = bits.wrapping_sub(5);
            out.push(symbol(acc.wrapping_shr(bits)));
        }
    }
    if bits > 0 {
        out.push(symbol(acc.wrapping_shl(5u32.wrapping_sub(bits))));
    }
}

fn symbol(v: u32) -> char {
    // `v & 0x1f` is below 32: always in the table.
    let i = usize::try_from(v & 0x1f).unwrap_or_default();
    BASE32.get(i).map_or('a', |&c| char::from(c))
}

/// Longest peer-sent name kept, in characters (specs/01-pairing.md, Data; proposed).
pub const NAME_MAX_CHARS: usize = 64;

/// Unicode format characters (general category Cf, Unicode 16): bidi overrides, zero-width
/// characters, tags. Inclusive ranges.
const FORMAT_CHARS: &[(char, char)] = &[
    ('\u{00ad}', '\u{00ad}'),
    ('\u{0600}', '\u{0605}'),
    ('\u{061c}', '\u{061c}'),
    ('\u{06dd}', '\u{06dd}'),
    ('\u{070f}', '\u{070f}'),
    ('\u{0890}', '\u{0891}'),
    ('\u{08e2}', '\u{08e2}'),
    ('\u{180e}', '\u{180e}'),
    ('\u{200b}', '\u{200f}'),
    ('\u{202a}', '\u{202e}'),
    ('\u{2060}', '\u{2064}'),
    ('\u{2066}', '\u{206f}'),
    ('\u{feff}', '\u{feff}'),
    ('\u{fff9}', '\u{fffb}'),
    ('\u{110bd}', '\u{110bd}'),
    ('\u{110cd}', '\u{110cd}'),
    ('\u{13430}', '\u{1343f}'),
    ('\u{1bca0}', '\u{1bca3}'),
    ('\u{1d173}', '\u{1d17a}'),
    ('\u{e0001}', '\u{e0001}'),
    ('\u{e0020}', '\u{e007f}'),
];

fn is_format(c: char) -> bool {
    FORMAT_CHARS.iter().any(|&(lo, hi)| (lo..=hi).contains(&c))
}

/// A name a peer sent, safe for a one-line record and a terminal (specs/01-pairing.md, Data):
/// control characters become spaces, format characters go, then it is trimmed and cut to
/// [`NAME_MAX_CHARS`].
pub fn one_line_name(name: &str) -> String {
    let clean: String = name
        .chars()
        .filter(|&c| !is_format(c))
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let capped: String = clean.trim().chars().take(NAME_MAX_CHARS).collect();
    capped.trim_end().to_owned()
}

/// Inverse of [`base32_encode`]. Canonical only: `None` on any other character, on a length
/// that leaves a partial symbol (1, 3 or 6 symbols mod 8), or on non-zero spare bits.
fn base32_decode(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len());
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in text.bytes() {
        let lower = c.to_ascii_lowercase();
        let v = BASE32.iter().position(|&s| s == lower)?;
        acc = acc.wrapping_shl(5) | u32::try_from(v).ok()?;
        bits = bits.wrapping_add(5);
        if bits >= 8 {
            bits = bits.wrapping_sub(8);
            out.push(u8::try_from(acc.wrapping_shr(bits) & 0xff).ok()?);
        }
    }
    // `bits` is below 8 here. 5 or more spare bits: a whole symbol carries no byte.
    let spare = acc & 1u32.wrapping_shl(bits).wrapping_sub(1);
    (bits < 5 && spare == 0).then_some(out)
}

#[cfg(test)]
mod tests;
