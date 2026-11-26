//! The deck side of pairing (specs/01-pairing.md, Invite flow; D47).
//!
//! [`PendingInvite`] is the sans-IO check of one invite. [`accept_pair`] serves the worker's
//! callback over a transport: it reads `PairRequest`, checks the secret, saves the worker
//! through the caller, answers, and waits for the worker to finish (rule 9).
//!
//! No timers here: the caller bounds the wait (the invite's expiry, `PAIR_TIMEOUT`), so the
//! crate runs on any executor.

use std::net::SocketAddr;

use lattice_key::NodeId;
use lattice_sync_proto::{
    Decoded, FrameDecoder, INVITE_TTL_SECS, Invite, Message, PairSecret, encode,
};
use lattice_transport::{Conn, RecvStream, SendStream, Transport, TransportError};

use crate::link::DeckError;
use crate::workers::one_line;

/// Bytes read from a pairing stream per call.
const READ_CHUNK: usize = 4 * 1024;

/// Why the deck refused a `PairRequest` (rule 8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PairDeny {
    /// The secret is not the invite's. The invite stays open.
    #[error("bad secret")]
    BadSecret,
    /// The invite expired.
    #[error("expired")]
    Expired,
    /// The invite was used already.
    #[error("used")]
    Used,
}

impl PairDeny {
    /// Stable code, sent in `PairDenied`.
    pub fn code(&self) -> &'static str {
        match self {
            Self::BadSecret => "pair.bad_secret",
            Self::Expired => "pair.expired",
            Self::Used => "pair.used",
        }
    }
}

/// The deck's one open invite (rule 2). Sans-IO.
#[derive(Debug, Clone)]
pub struct PendingInvite {
    invite: Invite,
    used: bool,
}

impl PendingInvite {
    /// A new invite from `deck` at `addr`, made at `now` (Unix seconds), with `secret`
    /// from the OS RNG. It expires [`INVITE_TTL_SECS`] later.
    pub fn new(deck: NodeId, addr: SocketAddr, secret: PairSecret, now: u64) -> Self {
        let expires = now.saturating_add(INVITE_TTL_SECS);
        Self {
            invite: Invite::new(deck, addr, secret, expires),
            used: false,
        }
    }

    /// The invite, to show as `lattice add-deck <code>`.
    pub fn invite(&self) -> &Invite {
        &self.invite
    }

    /// The command the user copies to the worker. Holds the secret: never log it.
    pub fn command(&self) -> String {
        format!("lattice add-deck {}", self.invite.encode())
    }

    /// Checks one `PairRequest` secret at `now`. Success uses the invite up (single use).
    ///
    /// # Errors
    ///
    /// [`PairDeny`]; only [`PairDeny::BadSecret`] leaves the invite usable.
    pub fn check(&mut self, secret: &PairSecret, now: u64) -> Result<(), PairDeny> {
        if self.used {
            return Err(PairDeny::Used);
        }
        if self.invite.expired_at(now) {
            return Err(PairDeny::Expired);
        }
        if !self.invite.secret().matches(secret) {
            return Err(PairDeny::BadSecret);
        }
        self.used = true;
        Ok(())
    }

    /// True once a worker paired with it.
    pub fn is_used(&self) -> bool {
        self.used
    }
}

/// A worker the deck paired with: its NodeId comes from TLS, never from a message (rule 6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairedWorker {
    /// The worker's NodeId, from the handshake.
    pub id: NodeId,
    /// Where `latticed` listens.
    pub addr: SocketAddr,
    /// Its display name.
    pub name: String,
}

/// Accepts pairing connections on `transport` until one pairs, or the invite is spent.
///
/// For each connection: read `PairRequest`, [`PendingInvite::check`] it at `now()`, call
/// `save` before answering `PairOk{deck_name}` (rule 7), then wait for the worker to finish
/// its stream (rule 9). A bad secret gets `PairDenied` and the next connection is served.
///
/// # Errors
///
/// [`DeckError::Remote`] with `pair.expired` or `pair.used` once the invite cannot pair;
/// [`DeckError::Transport`] when the transport closes; [`DeckError::Remote`] with the code of
/// `save` when saving failed (the worker gets `PairDenied{pair.save_failed}`).
pub async fn accept_pair<T: Transport>(
    transport: &T,
    pending: &mut PendingInvite,
    deck_name: &str,
    now: impl Fn() -> u64,
    mut save: impl FnMut(&PairedWorker) -> Result<(), String>,
) -> Result<PairedWorker, DeckError> {
    loop {
        let (peer, conn) = transport.accept().await?;
        match serve(&conn, peer, pending, deck_name, &now, &mut save).await {
            Ok(Some(worker)) => return Ok(worker),
            Ok(None) => {}
            Err(Served::Fatal(e)) => return Err(e),
            Err(Served::Dropped(code)) => {
                tracing::debug!(%peer, code, "pairing connection dropped");
            }
        }
        if pending.is_used() || pending.invite.expired_at(now()) {
            let deny = if pending.is_used() {
                PairDeny::Used
            } else {
                PairDeny::Expired
            };
            return Err(DeckError::Remote(deny.code().to_owned()));
        }
    }
}

/// How one connection ended without pairing.
enum Served {
    /// The invite cannot pair any more, or saving failed.
    Fatal(DeckError),
    /// That connection broke; serve the next.
    Dropped(&'static str),
}

async fn serve<C: Conn>(
    conn: &C,
    peer: NodeId,
    pending: &mut PendingInvite,
    deck_name: &str,
    now: &impl Fn() -> u64,
    save: &mut impl FnMut(&PairedWorker) -> Result<(), String>,
) -> Result<Option<PairedWorker>, Served> {
    let dropped = |e: TransportError| Served::Dropped(e.code());
    let (mut send, mut recv) = conn.accept_bi().await.map_err(dropped)?;
    let mut dec = FrameDecoder::new();
    let Some(Message::PairRequest { secret, name, addr }) = read(&mut dec, &mut recv).await else {
        return Err(Served::Dropped("pair.unexpected_message"));
    };
    let reply = match pending.check(&secret, now()) {
        Ok(()) => {
            let worker = PairedWorker {
                id: peer,
                addr,
                name: one_line(&name),
            };
            if let Err(code) = save(&worker) {
                let denied = Message::PairDenied {
                    code: "pair.save_failed".to_owned(),
                };
                drop(write(&mut send, &denied).await);
                return Err(Served::Fatal(DeckError::Remote(code)));
            }
            let ok = Message::PairOk {
                name: deck_name.to_owned(),
            };
            write(&mut send, &ok).await.map_err(dropped)?;
            // Rule 9: the worker finishes after writing its allowlist; dial only then.
            while read(&mut dec, &mut recv).await.is_some() {}
            tracing::info!(worker = %worker.id, name = %worker.name, "worker paired");
            return Ok(Some(worker));
        }
        Err(deny) => Message::PairDenied {
            code: deny.code().to_owned(),
        },
    };
    write(&mut send, &reply).await.map_err(dropped)?;
    drop(send.finish());
    Ok(None)
}

/// Next message, or `None` at the end of the stream or on any error.
async fn read<R: RecvStream>(dec: &mut FrameDecoder, recv: &mut R) -> Option<Message> {
    let mut buf = vec![0u8; READ_CHUNK];
    loop {
        match dec.next() {
            Ok(Some(Decoded::Message(msg))) => return Some(msg),
            Ok(None) => {}
            Ok(Some(Decoded::Unsupported)) | Err(_) => return None,
        }
        let n = recv.read(&mut buf).await.ok()??;
        dec.push(buf.get(..n)?);
    }
}

async fn write<S: SendStream>(send: &mut S, msg: &Message) -> Result<(), TransportError> {
    let mut wire = Vec::new();
    encode(msg, &mut wire).map_err(|e| TransportError::Io(e.code().to_owned()))?;
    send.write_all(&wire).await
}

#[cfg(test)]
mod tests;
