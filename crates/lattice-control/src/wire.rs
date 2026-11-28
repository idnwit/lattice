//! Control socket messages and framing (specs/11-control-socket.md, rules 6–8, Messages).
//!
//! The CLI sends one version byte, then one frame; `latticed` answers one frame. A frame is
//! a big-endian `u32` length, then a postcard body of at most [`MAX_CONTROL_FRAME`] bytes.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::resolve::Entry;

/// The version byte the CLI sends first (rule 6).
pub const CONTROL_VERSION: u8 = 1;

/// Longest frame body: 64 KiB (rule 7; proposed). A longer one closes the connection.
pub const MAX_CONTROL_FRAME: usize = 64 * 1024;

/// Bytes of the big-endian `u32` length prefix.
const LEN_PREFIX: usize = 4;

/// Number of [`Request`] variants: a postcard tag at or above it is an unknown request
/// type (rule 8). Bump when appending a variant.
const REQUEST_TAGS: u32 = 6;

/// A CLI request. Variant order is the wire tag: append only.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Request {
    /// `lattice add-deck` (specs/01-pairing.md, CLI). `code` holds the invite secret.
    AddDeck {
        /// The invite code, at most 1024 bytes. Never logged.
        code: String,
        /// The worker name for this pairing.
        name: Option<String>,
    },
    /// `lattice config list`.
    ConfigList,
    /// `lattice config get <key>`.
    ConfigGet {
        /// Dotted key name.
        key: String,
    },
    /// `lattice config set <key> <value>`.
    ConfigSet {
        /// Dotted key name.
        key: String,
        /// The new value, in text form.
        value: String,
    },
    /// `lattice config reset <key>`.
    ConfigReset {
        /// Dotted key name.
        key: String,
    },
    /// `lattice config edit`: replace the file when it still hashes to `base_hash`.
    ConfigReplace {
        /// The new file text.
        toml: String,
        /// blake3 of the file the CLI read ([`file_hash`]).
        base_hash: [u8; 32],
    },
}

impl fmt::Debug for Request {
    // The invite code is a secret (.rules/engineering.md, Security): never printed.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AddDeck { name, .. } => f
                .debug_struct("AddDeck")
                .field("code", &"<redacted>")
                .field("name", name)
                .finish(),
            Self::ConfigList => f.write_str("ConfigList"),
            Self::ConfigGet { key } => f.debug_struct("ConfigGet").field("key", key).finish(),
            Self::ConfigSet { key, value } => f
                .debug_struct("ConfigSet")
                .field("key", key)
                .field("value", value)
                .finish(),
            Self::ConfigReset { key } => f.debug_struct("ConfigReset").field("key", key).finish(),
            Self::ConfigReplace { .. } => f.write_str("ConfigReplace"),
        }
    }
}

/// The reply of `latticed`. Variant order is the wire tag: append only.
///
/// `Paired` (the `AddDeck` reply) is appended with pairing (ROADMAP M2.9).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Reply {
    /// Resolved entries: all of them, or the one the request named.
    Config {
        /// Key, value, and source of each entry.
        entries: Vec<Entry>,
    },
    /// The request failed with a stable code (`config.unknown_key`, `control.unsupported`).
    Failed {
        /// The code.
        code: String,
    },
}

impl Reply {
    /// `Failed{code}`.
    pub fn failed(code: &str) -> Self {
        Self::Failed {
            code: code.to_owned(),
        }
    }
}

/// The connection must close: framing is lost.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ControlError {
    /// A declared frame body above [`MAX_CONTROL_FRAME`].
    #[error("control frame too long")]
    TooLong,
    /// Bytes that do not decode.
    #[error("malformed control frame")]
    Malformed,
}

impl ControlError {
    /// Stable code for logs (proposed).
    pub fn code(&self) -> &'static str {
        match self {
            Self::TooLong => "control.too_long",
            Self::Malformed => "control.malformed",
        }
    }
}

/// blake3 of the file bytes: the `base_hash` of `ConfigReplace`.
pub fn file_hash(bytes: &[u8]) -> [u8; 32] {
    *blake3::hash(bytes).as_bytes()
}

/// Appends the version byte and the frame of `req`: all the CLI sends.
///
/// # Errors
///
/// [`ControlError::TooLong`] when the body exceeds [`MAX_CONTROL_FRAME`]; `out` is
/// unchanged on error.
pub fn encode_request(req: &Request, out: &mut Vec<u8>) -> Result<(), ControlError> {
    let start = out.len();
    out.push(CONTROL_VERSION);
    encode_frame(req, out).inspect_err(|_| out.truncate(start))
}

/// Appends the frame of `reply`.
///
/// # Errors
///
/// [`ControlError::TooLong`] when the body exceeds [`MAX_CONTROL_FRAME`]; `out` is
/// unchanged on error.
pub fn encode_reply(reply: &Reply, out: &mut Vec<u8>) -> Result<(), ControlError> {
    encode_frame(reply, out)
}

fn encode_frame<T: Serialize>(msg: &T, out: &mut Vec<u8>) -> Result<(), ControlError> {
    let body = postcard::to_allocvec(msg).map_err(|_| ControlError::Malformed)?;
    if body.len() > MAX_CONTROL_FRAME {
        return Err(ControlError::TooLong);
    }
    let len = u32::try_from(body.len()).map_err(|_| ControlError::TooLong)?;
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(&body);
    Ok(())
}

/// What `latticed` read from a connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Incoming {
    /// A known request.
    Request(Request),
    /// An unknown request type: reply `Failed{control.unsupported}` (rule 8).
    Unsupported,
    /// Another version byte: reply `Failed{control.version}` (rule 6).
    BadVersion,
}

/// Reads the version byte and one request frame. Sans-IO: push bytes, then call
/// [`RequestReader::next`] until it gives a result.
#[derive(Debug, Default)]
pub struct RequestReader {
    frames: FrameBuf,
    version_seen: bool,
}

impl RequestReader {
    /// An empty reader.
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends bytes read from the socket.
    pub fn push(&mut self, bytes: &[u8]) {
        self.frames.buf.extend_from_slice(bytes);
    }

    /// The request, `Ok(None)` while more bytes are needed.
    ///
    /// A declared length above [`MAX_CONTROL_FRAME`] fails as soon as the prefix is in,
    /// before the body is read (rule 7).
    ///
    /// # Errors
    ///
    /// [`ControlError`] when the connection must close.
    #[allow(clippy::should_implement_trait, reason = "fallible; not an Iterator")]
    pub fn next(&mut self) -> Result<Option<Incoming>, ControlError> {
        if !self.version_seen {
            let Some(&version) = self.frames.buf.first() else {
                return Ok(None);
            };
            if version != CONTROL_VERSION {
                return Ok(Some(Incoming::BadVersion));
            }
            self.frames.buf.remove(0);
            self.version_seen = true;
        }
        let Some(body) = self.frames.body()? else {
            return Ok(None);
        };
        let tag = read_varint_u32(body).ok_or(ControlError::Malformed)?;
        if tag >= REQUEST_TAGS {
            return Ok(Some(Incoming::Unsupported));
        }
        decode_exact(body).map(|r| Some(Incoming::Request(r)))
    }
}

/// Reads one reply frame (the CLI side).
#[derive(Debug, Default)]
pub struct ReplyReader {
    frames: FrameBuf,
}

impl ReplyReader {
    /// An empty reader.
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends bytes read from the socket.
    pub fn push(&mut self, bytes: &[u8]) {
        self.frames.buf.extend_from_slice(bytes);
    }

    /// The reply, `Ok(None)` while more bytes are needed.
    ///
    /// # Errors
    ///
    /// [`ControlError`] for a frame that is too long or does not decode, an unknown
    /// reply type included.
    #[allow(clippy::should_implement_trait, reason = "fallible; not an Iterator")]
    pub fn next(&mut self) -> Result<Option<Reply>, ControlError> {
        match self.frames.body()? {
            Some(body) => decode_exact(body).map(Some),
            None => Ok(None),
        }
    }
}

/// Buffered bytes of one frame.
#[derive(Debug, Default)]
struct FrameBuf {
    buf: Vec<u8>,
}

impl FrameBuf {
    /// The whole body once it is in.
    fn body(&self) -> Result<Option<&[u8]>, ControlError> {
        let Some((prefix, rest)) = self.buf.split_first_chunk::<LEN_PREFIX>() else {
            return Ok(None);
        };
        let len =
            usize::try_from(u32::from_be_bytes(*prefix)).map_err(|_| ControlError::TooLong)?;
        if len > MAX_CONTROL_FRAME {
            return Err(ControlError::TooLong);
        }
        Ok(rest.get(..len))
    }
}

/// Decodes a body with no trailing bytes.
fn decode_exact<'a, T: Deserialize<'a>>(body: &'a [u8]) -> Result<T, ControlError> {
    match postcard::take_from_bytes::<T>(body) {
        Ok((msg, [])) => Ok(msg),
        _ => Err(ControlError::Malformed),
    }
}

/// Postcard's enum tag: a LEB128 `u32` varint (at most 5 bytes).
fn read_varint_u32(bytes: &[u8]) -> Option<u32> {
    let mut value: u32 = 0;
    for (i, &b) in bytes.iter().take(5).enumerate() {
        let shift = u32::try_from(i).ok()?.checked_mul(7)?;
        let part = u32::from(b & 0x7F).checked_shl(shift)?;
        value |= part;
        if b & 0x80 == 0 {
            return Some(value);
        }
    }
    None
}

#[cfg(test)]
mod tests;
