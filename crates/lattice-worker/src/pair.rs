//! The worker side of pairing: `latticed add-deck` (specs/01-pairing.md, rules 3–10).
//!
//! The worker dials the deck named by the invite (the transport pins its NodeId) and proves
//! the invite secret. On `PairOk` it writes the deck to its allowlist, sends `PairDone`, and
//! reports paired once the deck finishes its stream: the deck saved the worker then.

use std::net::SocketAddr;
use std::path::Path;

use lattice_key::NodeId;
use lattice_sync_proto::{
    Decoded, FrameDecoder, Invite, Message, PAIR_TIMEOUT, encode, one_line_name,
};
use lattice_transport::{CloseReason, Conn, RecvStream, SendStream, Transport, TransportError};
use tokio::time::{Instant, timeout_at};

use crate::decks_file;

/// Bytes read from the pairing stream per call.
const READ_CHUNK: usize = 4 * 1024;

/// Longest deck code kept from `PairDenied`.
const DENY_CODE_MAX: usize = 32;

/// Pairing failed. Never holds the secret or peer bytes.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PairError {
    /// The invite expired before the worker dialed (rule 3).
    #[error("invite expired")]
    Expired,
    /// The deck refused, with its code reduced to `[a-z._]` (rule 8).
    #[error("deck refused: {0}")]
    Denied(String),
    /// Dial or stream failure, `transport.identity_mismatch` included (rule 4).
    #[error("transport: {}", .0.code())]
    Transport(TransportError),
    /// The deck broke the pairing exchange.
    #[error("protocol: {0}")]
    Protocol(&'static str),
    /// No answer within `PAIR_TIMEOUT` (rule 10).
    #[error("timed out")]
    Timeout,
    /// After `PairDone` the deck closed or reset instead of finishing (rule 10).
    #[error("the deck did not confirm")]
    Unconfirmed,
    /// The allowlist could not be written.
    #[error("allowlist: {0}")]
    Allowlist(String),
}

impl PairError {
    /// Stable code for logs and the CLI. A deck's denial keeps its code when it is one
    /// of rule 8's.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Expired => "pair.expired",
            Self::Denied(code) => deny_code(code),
            Self::Transport(e) => e.code(),
            Self::Protocol(code) => code,
            Self::Timeout => "pair.timeout",
            Self::Unconfirmed => "pair.unconfirmed",
            Self::Allowlist(_) => "pair.allowlist",
        }
    }
}

/// Rule 8's codes as themselves; any other denial is `pair.denied`.
fn deny_code(code: &str) -> &'static str {
    match code {
        "pair.bad_secret" => "pair.bad_secret",
        "pair.expired" => "pair.expired",
        "pair.used" => "pair.used",
        _ => "pair.denied",
    }
}

/// A peer's code, reduced to a short `[a-z._]` token for messages.
fn sanitize_code(code: &str) -> String {
    code.chars()
        .filter(|c| c.is_ascii_lowercase() || *c == '.' || *c == '_')
        .take(DENY_CODE_MAX)
        .collect()
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
/// address (`QuicTransport::add_peer`). On success the deck is in the allowlist at `allowlist`
/// and the deck confirmed it saved this worker.
///
/// `now` is the current Unix time, in seconds. The whole exchange is bounded by
/// `PAIR_TIMEOUT`.
///
/// # Errors
///
/// [`PairError`]. The allowlist is untouched unless the deck answered `PairOk`. After that
/// write, a failure ([`PairError::Timeout`], [`PairError::Unconfirmed`]) keeps the line: the
/// deck does not list this worker, so the line is harmless (rule 10).
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
    let deadline = Instant::now() + PAIR_TIMEOUT;
    let conn = timeout_at(deadline, transport.connect(invite.deck()))
        .await
        .map_err(|_| PairError::Timeout)??;
    let outcome = timeout_at(deadline, exchange(&conn, invite, me, allowlist))
        .await
        .unwrap_or(Err(PairError::Timeout));
    if outcome.is_err() {
        // Rule 10: a failed exchange never ends with a clean finish.
        conn.close(CloseReason::Refused);
    }
    outcome
}

async fn exchange<C: Conn>(
    conn: &C,
    invite: &Invite,
    me: &WorkerCard,
    allowlist: &Path,
) -> Result<PairedDeck, PairError> {
    let (mut send, mut recv) = conn.open_bi().await?;
    let request = Message::PairRequest {
        secret: invite.secret().clone(),
        name: me.name.clone(),
        addr: me.addr,
    };
    write(&mut send, &request).await?;
    let mut reader = Reader::default();
    let name = match reader.next(&mut recv).await? {
        Message::PairOk { name } => one_line_name(&name),
        Message::PairDenied { code } => return Err(PairError::Denied(sanitize_code(&code))),
        _ => return Err(PairError::Protocol("pair.unexpected_message")),
    };
    let deck = PairedDeck {
        id: invite.deck(),
        name,
    };
    save(allowlist, &deck).await?;
    tracing::info!(deck = %deck.id, name = %deck.name, "deck added to the allowlist");
    // Rule 9: the deck saves this worker on `PairDone`, then finishes its stream.
    write(&mut send, &Message::PairDone).await?;
    if !finished(&mut recv).await {
        return Err(PairError::Unconfirmed);
    }
    // Lets the deck stop waiting: its finish reached us.
    send.finish()?;
    Ok(deck)
}

async fn write<S: SendStream>(send: &mut S, msg: &Message) -> Result<(), PairError> {
    let mut wire = Vec::new();
    encode(msg, &mut wire).map_err(|e| PairError::Protocol(e.code()))?;
    Ok(send.write_all(&wire).await?)
}

/// Adds `deck` to the allowlist file, off the async threads (specs/01-pairing.md, Files).
async fn save(path: &Path, deck: &PairedDeck) -> Result<(), PairError> {
    let (path, id, name) = (path.to_owned(), deck.id, deck.name.clone());
    tokio::task::spawn_blocking(move || decks_file::add(&path, id, &name))
        .await
        .map_err(|_| PairError::Allowlist("join".to_owned()))?
        .map_err(|e| PairError::Allowlist(e.kind().to_string()))
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
}

/// True when the peer finishes its stream with nothing more sent; false on more bytes, a
/// reset, or a closed connection.
async fn finished<R: RecvStream>(recv: &mut R) -> bool {
    let mut buf = [0u8; 64];
    matches!(recv.read(&mut buf).await, Ok(None))
}

#[cfg(test)]
mod tests;
