//! The worker side of pairing: `lattice add-deck <invite>` (specs/01-pairing.md, rules 3–10).
//!
//! The worker dials the deck named by the invite (the transport pins its NodeId), proves the
//! invite secret, and on `PairOk` adds the deck to its allowlist before finishing the stream.

use std::net::SocketAddr;
use std::path::Path;
use std::time::Duration;

use lattice_key::NodeId;
use lattice_sync_proto::{Decoded, FrameDecoder, Invite, Message, encode};
use lattice_transport::{Conn, RecvStream, SendStream, Transport, TransportError};

use crate::allowlist::{Allowlist, one_line};

/// Bound on the whole exchange (specs/01-pairing.md, rule 10; proposed).
pub(crate) const PAIR_TIMEOUT: Duration = Duration::from_secs(10);

/// Bytes read from the pairing stream per call.
const READ_CHUNK: usize = 4 * 1024;

/// Pairing failed. Never holds the secret or peer bytes.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PairError {
    /// The invite expired before the worker dialed (rule 3).
    #[error("invite expired")]
    Expired,
    /// The deck refused, with its code (`pair.bad_secret`, `pair.expired`, `pair.used`).
    #[error("deck refused: {0}")]
    Denied(String),
    /// Dial or stream failure, `transport.identity_mismatch` included (rule 4).
    #[error("transport: {}", .0.code())]
    Transport(TransportError),
    /// The deck broke the pairing exchange.
    #[error("protocol: {0}")]
    Protocol(&'static str),
    /// No answer within `PAIR_TIMEOUT` (10 s).
    #[error("timed out")]
    Timeout,
    /// The allowlist could not be written.
    #[error("allowlist: {0}")]
    Allowlist(String),
}

impl PairError {
    /// Stable code for logs and the CLI.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Expired => "pair.expired",
            Self::Denied(_) => "pair.denied",
            Self::Transport(e) => e.code(),
            Self::Protocol(code) => code,
            Self::Timeout => "pair.timeout",
            Self::Allowlist(_) => "pair.allowlist",
        }
    }
}

impl From<TransportError> for PairError {
    fn from(e: TransportError) -> Self {
        Self::Transport(e)
    }
}

/// A deck the worker now serves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairedDeck {
    /// The deck's NodeId: the one the invite named and TLS pinned.
    pub id: NodeId,
    /// Its display name, from `PairOk`.
    pub name: String,
}

/// What the worker tells the deck about itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerCard {
    /// Display name (`build-01`).
    pub name: String,
    /// Where the deck reaches `latticed`.
    pub addr: SocketAddr,
}

/// Runs the worker side of pairing over `transport`, which must already know the invite's
/// address (`QuicTransport::add_peer`). On success the deck is in the allowlist at `allowlist`.
///
/// `now` is the current Unix time, in seconds.
///
/// # Errors
///
/// [`PairError`]; nothing is written to the allowlist then.
pub async fn add_deck<T: Transport>(
    transport: &T,
    invite: &Invite,
    me: &WorkerCard,
    allowlist: &Path,
    now: u64,
) -> Result<PairedDeck, PairError> {
    if invite.expired_at(now) {
        return Err(PairError::Expired);
    }
    tokio::time::timeout(PAIR_TIMEOUT, exchange(transport, invite, me, allowlist))
        .await
        .map_err(|_| PairError::Timeout)?
}

async fn exchange<T: Transport>(
    transport: &T,
    invite: &Invite,
    me: &WorkerCard,
    allowlist: &Path,
) -> Result<PairedDeck, PairError> {
    let conn = transport.connect(invite.deck()).await?;
    let (mut send, mut recv) = conn.open_bi().await?;
    let request = Message::PairRequest {
        secret: invite.secret().clone(),
        name: me.name.clone(),
        addr: me.addr,
    };
    let mut wire = Vec::new();
    encode(&request, &mut wire).map_err(|e| PairError::Protocol(e.code()))?;
    send.write_all(&wire).await?;
    let mut reader = Reader::default();
    let name = match reader.next(&mut recv).await? {
        Message::PairOk { name } => one_line(&name),
        Message::PairDenied { code } => return Err(PairError::Denied(code)),
        _ => return Err(PairError::Protocol("pair.unexpected_message")),
    };
    let deck = PairedDeck {
        id: invite.deck(),
        name,
    };
    save(allowlist, &deck).await?;
    tracing::info!(deck = %deck.id, name = %deck.name, "deck added to the allowlist");
    // Rule 9: the deck dials us only after this finish, so the allowlist is on disk first.
    send.finish()?;
    reader.until_end(&mut recv).await;
    drop(conn);
    Ok(deck)
}

/// Adds `deck` to the allowlist file: read, add, write to a temporary file, rename.
async fn save(path: &Path, deck: &PairedDeck) -> Result<(), PairError> {
    let err = |e: std::io::Error| PairError::Allowlist(e.kind().to_string());
    let text = match tokio::fs::read_to_string(path).await {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(err(e)),
    };
    let mut list = Allowlist::parse(&text);
    list.add(deck.id, &deck.name);
    let tmp = path.with_extension("tmp");
    tokio::fs::write(&tmp, list.to_text()).await.map_err(err)?;
    tokio::fs::rename(&tmp, path).await.map_err(err)
}

/// Sync frames from one stream.
#[derive(Default)]
struct Reader {
    dec: FrameDecoder,
}

impl Reader {
    async fn next<R: RecvStream>(&mut self, recv: &mut R) -> Result<Message, PairError> {
        let mut buf = vec![0u8; READ_CHUNK];
        loop {
            match self.dec.next() {
                Ok(Some(Decoded::Message(msg))) => return Ok(msg),
                Ok(Some(Decoded::Unsupported)) => {
                    return Err(PairError::Protocol("pair.unexpected_message"));
                }
                Ok(None) => {}
                Err(e) => return Err(PairError::Protocol(e.code())),
            }
            match recv.read(&mut buf).await? {
                Some(n) => self.dec.push(buf.get(..n).unwrap_or_default()),
                None => return Err(PairError::Transport(TransportError::Closed)),
            }
        }
    }

    /// Reads until the peer finishes or the stream breaks; the bytes do not matter.
    async fn until_end<R: RecvStream>(&mut self, recv: &mut R) {
        let mut buf = vec![0u8; READ_CHUNK];
        while let Ok(Some(_)) = recv.read(&mut buf).await {}
    }
}

#[cfg(test)]
mod tests;
