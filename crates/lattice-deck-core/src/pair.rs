//! The deck side of pairing (specs/01-pairing.md, Invite flow; D47).
//!
//! [`PendingInvite`] is the sans-IO check of one invite. [`accept_pair`] serves the worker's
//! callback over a transport: it reads `PairRequest`, checks the secret, answers `PairOk`,
//! and saves the worker through the caller only on the worker's `PairDone` (rule 9).
//!
//! No timers here: the caller passes a `PAIR_TIMEOUT` sleep for each exchange and bounds the
//! whole wait by the invite's expiry, so the crate runs on any executor.

use std::future::{Future, poll_fn};
use std::net::SocketAddr;
use std::pin::pin;
use std::task::Poll;

use lattice_key::NodeId;
use lattice_sync_proto::{
    Decoded, FrameDecoder, INVITE_TTL_SECS, Invite, Message, PairSecret, encode, one_line_name,
};
use lattice_transport::{CloseReason, Conn, RecvStream, SendStream, Transport, TransportError};

use crate::link::DeckError;

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

/// Progress of [`accept_pair`], for the status line (specs/01-pairing.md, UI rule 4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairStep {
    /// A worker connected: its callback is being served (`pairing…`).
    Serving,
    /// That callback did not pair; the invite waits for the next (`waiting for worker…`).
    Waiting,
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
/// For each connection, bounded by one `timeout()` (`PAIR_TIMEOUT`, rule 10): read
/// `PairRequest`, [`PendingInvite::check`] it at `now()`, answer `PairOk{deck_name}`, and
/// wait for `PairDone`. Only then `save` the worker and finish the stream (rule 9). Then wait
/// up to one more `timeout()` for the worker's finish, so ours reached it.
///
/// A connection that does not pair is closed as refused, and the next one is served while
/// the invite is open. `on_step` hears each connection start ([`PairStep::Serving`]) and each
/// one that did not pair ([`PairStep::Waiting`]).
///
/// `timeout` makes a fresh `PAIR_TIMEOUT` sleep on the caller's executor: this crate has no
/// timers. The caller bounds the whole call by the invite's expiry.
///
/// # Errors
///
/// [`DeckError::Remote`] with a code: `pair.expired` once the invite expired; the failure of
/// the connection that used the invite up (`pair.timeout`, `pair.unconfirmed`); or the code
/// of `save`. [`DeckError::Transport`] when the transport closes.
pub async fn accept_pair<T: Transport, F: Future<Output = ()>>(
    transport: &T,
    pending: &mut PendingInvite,
    deck_name: &str,
    now: impl Fn() -> u64,
    mut save: impl FnMut(&PairedWorker) -> Result<(), String>,
    mut on_step: impl FnMut(PairStep),
    timeout: impl Fn() -> F,
) -> Result<PairedWorker, DeckError> {
    loop {
        let (peer, conn) = transport.accept().await?;
        on_step(PairStep::Serving);
        let served = within(serve(&conn, peer, pending, deck_name, &now), timeout())
            .await
            .unwrap_or(Err(Served("pair.timeout")));
        let code = match served {
            Ok((worker, mut exchange)) => {
                // Dropping `exchange` unpaired closes the connection as refused.
                save(&worker).map_err(DeckError::Remote)?;
                exchange.confirm();
                // The worker finishes once it read our finish. Paired either way: a lost
                // finish only costs this wait.
                if within(exchange.drain(), timeout()).await.is_none() {
                    tracing::debug!(worker = %worker.id, "no finish from the worker in time");
                }
                tracing::info!(worker = %worker.id, name = %worker.name, "worker paired");
                return Ok(worker);
            }
            Err(Served(code)) => code,
        };
        conn.close(CloseReason::Refused);
        tracing::debug!(%peer, code, "pairing connection did not pair");
        if pending.is_used() {
            return Err(DeckError::Remote(code.to_owned()));
        }
        if pending.invite.expired_at(now()) {
            return Err(DeckError::Remote(PairDeny::Expired.code().to_owned()));
        }
        on_step(PairStep::Waiting);
    }
}

/// Why one connection did not pair: a stable code.
struct Served(&'static str);

/// Serves one connection up to `PairDone`. The caller saves the worker and confirms.
async fn serve<'c, C: Conn>(
    conn: &'c C,
    peer: NodeId,
    pending: &mut PendingInvite,
    deck_name: &str,
    now: &impl Fn() -> u64,
) -> Result<(PairedWorker, Exchange<'c, C>), Served> {
    let dropped = |e: TransportError| Served(e.code());
    let (send, recv) = conn.accept_bi().await.map_err(dropped)?;
    let mut exchange = Exchange {
        conn,
        send,
        recv,
        dec: FrameDecoder::new(),
        paired: false,
    };
    let Some(Message::PairRequest { secret, name, addr }) = exchange.read().await else {
        return Err(Served("pair.unexpected_message"));
    };
    if let Err(deny) = pending.check(&secret, now()) {
        let denied = Message::PairDenied {
            code: deny.code().to_owned(),
        };
        exchange.write(&denied).await.map_err(dropped)?;
        exchange.finish();
        // Rule 8: let the worker read the denial; the drop below closes as refused.
        exchange.drain().await;
        return Err(Served(deny.code()));
    }
    let worker = PairedWorker {
        id: peer,
        addr,
        name: one_line_name(&name),
    };
    let ok = Message::PairOk {
        name: deck_name.to_owned(),
    };
    exchange.write(&ok).await.map_err(dropped)?;
    match exchange.read().await {
        Some(Message::PairDone) => Ok((worker, exchange)),
        _ => Err(Served("pair.unconfirmed")),
    }
}

/// The pairing stream of one connection.
///
/// Dropped before [`Exchange::confirm`], it closes the connection as refused (rule 10). Its
/// `Drop` runs before the streams drop, so a cancelled exchange never ends with the clean
/// finish a dropped send stream would make.
struct Exchange<'c, C: Conn> {
    conn: &'c C,
    send: C::Send,
    recv: C::Recv,
    dec: FrameDecoder,
    paired: bool,
}

impl<C: Conn> Exchange<'_, C> {
    /// Next message, or `None` at the end of the stream or on any error.
    async fn read(&mut self) -> Option<Message> {
        let mut buf = vec![0u8; READ_CHUNK];
        loop {
            match self.dec.next() {
                Ok(Some(Decoded::Message(msg))) => return Some(msg),
                Ok(None) => {}
                Ok(Some(Decoded::Unsupported)) | Err(_) => return None,
            }
            let n = self.recv.read(&mut buf).await.ok()??;
            self.dec.push(buf.get(..n)?);
        }
    }

    async fn write(&mut self, msg: &Message) -> Result<(), TransportError> {
        let mut wire = Vec::new();
        encode(msg, &mut wire).map_err(|e| TransportError::Io(e.code().to_owned()))?;
        self.send.write_all(&wire).await
    }

    fn finish(&mut self) {
        // A stream that cannot finish is gone already; the peer sees that instead.
        drop(self.send.finish());
    }

    /// The worker is saved: finish cleanly and keep the connection open (rule 9).
    fn confirm(&mut self) {
        self.paired = true;
        self.finish();
    }

    /// Reads until the peer ends its stream, or the stream breaks.
    async fn drain(&mut self) {
        let mut buf = vec![0u8; READ_CHUNK];
        while let Ok(Some(_)) = self.recv.read(&mut buf).await {}
    }
}

impl<C: Conn> Drop for Exchange<'_, C> {
    fn drop(&mut self) {
        if !self.paired {
            self.conn.close(CloseReason::Refused);
        }
    }
}

/// `work`, or `None` once `deadline` fires first. Executor-agnostic.
async fn within<T>(work: impl Future<Output = T>, deadline: impl Future<Output = ()>) -> Option<T> {
    let mut work = pin!(work);
    let mut deadline = pin!(deadline);
    poll_fn(|cx| {
        if let Poll::Ready(v) = work.as_mut().poll(cx) {
            return Poll::Ready(Some(v));
        }
        deadline.as_mut().poll(cx).map(|()| None)
    })
    .await
}

#[cfg(test)]
mod tests;
