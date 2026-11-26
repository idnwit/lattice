//! The caller side of reducers: [`Caller`], [`RawTicket`], [`Ticket`] (06-apps.md, Interface 3).
//!
//! A ticket is a one-shot slot shared with a [`Resolver`]. The link owner resolves it when the
//! `ActionResult` arrives; dropping the resolver settles it [`CallError::Unknown`]. Std only: a
//! `Mutex` and a `Waker`, no runtime. A ticket is a `Future`, and [`Ticket::try_take`] reads it
//! without one.

use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::task::{Context, Poll, Waker};

use serde::de::DeserializeOwned;

use crate::dispatch::{AppError, BAD_REPLY};

/// Sends encoded actions of one app instance. Generated `XApiClient<C>` calls it.
pub trait Caller {
    /// Sends `action` (the instance is the caller's) and returns a ticket for its outcome.
    fn call(&self, action: Vec<u8>) -> RawTicket;
}

/// How a call ended, before typing: the encoded `Res`, or why not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RawCallError {
    /// `ActionResult` with an error.
    Refused(AppError),
    /// No link: nothing was sent.
    NotConnected,
    /// The link ended before the outcome arrived.
    Unknown,
}

/// How a typed call failed (06-apps.md, Interface 3). It never resends.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CallError<E> {
    /// The app refused: its typed error. The draft rolled back.
    #[error("{0}")]
    App(E),
    /// The platform refused (`app.bad_action`, `worker.no_instance`, …): the wire code.
    #[error("refused: {0}")]
    Platform(String),
    /// No link: nothing was sent.
    #[error("not connected")]
    NotConnected,
    /// The link ended while waiting; read the state to know.
    #[error("outcome unknown")]
    Unknown,
}

/// One-shot slot between a ticket and its resolver.
#[derive(Debug, Default)]
struct Slot {
    outcome: Option<Result<Vec<u8>, RawCallError>>,
    waker: Option<Waker>,
}

type Shared = Arc<Mutex<Slot>>;

/// Locks the slot. A poisoned lock still holds a whole `Slot`: every write is one assignment.
fn lock(slot: &Shared) -> MutexGuard<'_, Slot> {
    slot.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The untyped outcome of one call: a `Future` of the encoded `Res`.
#[derive(Debug)]
pub struct RawTicket(Shared);

/// Settles one [`RawTicket`]. Dropped unresolved, it settles [`RawCallError::Unknown`].
#[derive(Debug)]
pub struct Resolver(Option<Shared>);

impl RawTicket {
    /// A pending ticket and the resolver that settles it.
    pub fn pair() -> (Self, Resolver) {
        let slot = Shared::default();
        (Self(slot.clone()), Resolver(Some(slot)))
    }

    /// A ticket already settled with `outcome` (e.g. `NotConnected`).
    pub fn ready(outcome: Result<Vec<u8>, RawCallError>) -> Self {
        let (ticket, resolver) = Self::pair();
        resolver.resolve(outcome);
        ticket
    }

    /// The outcome, once settled; `None` while pending or after it was taken.
    pub fn try_take(&mut self) -> Option<Result<Vec<u8>, RawCallError>> {
        lock(&self.0).outcome.take()
    }
}

impl Future for RawTicket {
    type Output = Result<Vec<u8>, RawCallError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut slot = lock(&self.0);
        match slot.outcome.take() {
            Some(outcome) => Poll::Ready(outcome),
            None => {
                slot.waker = Some(cx.waker().clone());
                Poll::Pending
            }
        }
    }
}

impl Resolver {
    /// Settles the ticket and wakes its task.
    pub fn resolve(mut self, outcome: Result<Vec<u8>, RawCallError>) {
        if let Some(slot) = self.0.take() {
            settle(&slot, outcome);
        }
    }
}

impl Drop for Resolver {
    fn drop(&mut self) {
        if let Some(slot) = self.0.take() {
            settle(&slot, Err(RawCallError::Unknown));
        }
    }
}

fn settle(slot: &Shared, outcome: Result<Vec<u8>, RawCallError>) {
    let waker = {
        let mut slot = lock(slot);
        slot.outcome = Some(outcome);
        slot.waker.take()
    };
    if let Some(w) = waker {
        w.wake();
    }
}

/// The typed outcome of one reducer call: `Res`, or a [`CallError`] with the app's `E`.
pub struct Ticket<R, E> {
    raw: RawTicket,
    types: PhantomData<fn() -> (R, E)>,
}

impl<R, E> std::fmt::Debug for Ticket<R, E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Ticket").field(&self.raw).finish()
    }
}

impl<R, E> Ticket<R, E> {
    /// Types a raw ticket.
    pub fn new(raw: RawTicket) -> Self {
        Self {
            raw,
            types: PhantomData,
        }
    }
}

impl<R: DeserializeOwned, E: DeserializeOwned> Ticket<R, E> {
    /// The outcome, once settled; `None` while pending or after it was taken.
    pub fn try_take(&mut self) -> Option<Result<R, CallError<E>>> {
        self.raw.try_take().map(typed)
    }
}

impl<R: DeserializeOwned, E: DeserializeOwned> Future for Ticket<R, E> {
    type Output = Result<R, CallError<E>>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.raw).poll(cx).map(typed)
    }
}

/// Decodes `Res` or the app error; bytes that do not decode are [`BAD_REPLY`].
fn typed<R: DeserializeOwned, E: DeserializeOwned>(
    raw: Result<Vec<u8>, RawCallError>,
) -> Result<R, CallError<E>> {
    let bad = || CallError::Platform(BAD_REPLY.to_owned());
    match raw {
        Ok(bytes) => lattice_state::decode(&bytes).map_err(|_| bad()),
        Err(RawCallError::Refused(e)) if e.is_platform() => {
            Err(CallError::Platform(e.into_parts().0))
        }
        Err(RawCallError::Refused(e)) => Err(lattice_state::decode(e.detail())
            .map(CallError::App)
            .unwrap_or_else(|_| bad())),
        Err(RawCallError::NotConnected) => Err(CallError::NotConnected),
        Err(RawCallError::Unknown) => Err(CallError::Unknown),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::task::Wake;

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    enum Oops {
        Full,
    }

    type T = Ticket<u32, Oops>;

    /// Name, raw outcome, typed outcome.
    type Case = (
        &'static str,
        Result<Vec<u8>, RawCallError>,
        Result<u32, CallError<Oops>>,
    );

    fn pending() -> (T, Resolver) {
        let (raw, resolver) = RawTicket::pair();
        (Ticket::new(raw), resolver)
    }

    #[test]
    fn a_ticket_settles_each_way() {
        let ok = lattice_state::encode(&7u32).unwrap();
        let app = AppError::of(&Oops::Full);
        let cases: [Case; 5] = [
            ("ok", Ok(ok), Ok(7)),
            (
                "app",
                Err(RawCallError::Refused(app)),
                Err(CallError::App(Oops::Full)),
            ),
            (
                "platform",
                Err(RawCallError::Refused(AppError::platform(
                    "worker.no_instance",
                ))),
                Err(CallError::Platform("worker.no_instance".to_owned())),
            ),
            (
                "not connected",
                Err(RawCallError::NotConnected),
                Err(CallError::NotConnected),
            ),
            (
                "unknown",
                Err(RawCallError::Unknown),
                Err(CallError::Unknown),
            ),
        ];
        for (name, outcome, want) in cases {
            let (mut ticket, resolver) = pending();
            assert_eq!(ticket.try_take(), None, "case={name}: pending first");
            resolver.resolve(outcome);
            assert_eq!(ticket.try_take(), Some(want), "case={name}");
            assert_eq!(ticket.try_take(), None, "case={name}: taken once");
        }
    }

    #[test]
    fn dropping_the_resolver_settles_unknown() {
        let (mut ticket, resolver) = pending();
        drop(resolver);
        assert_eq!(ticket.try_take(), Some(Err(CallError::Unknown)));
    }

    #[test]
    fn bytes_that_do_not_decode_are_a_bad_reply() {
        let bad = Some(Err(CallError::Platform(BAD_REPLY.to_owned())));
        let mut ticket: T = Ticket::new(RawTicket::ready(Ok(vec![
            0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
        ])));
        assert_eq!(ticket.try_take(), bad);
        let detail = AppError::from_wire("full".to_owned(), vec![9]);
        let mut ticket: T = Ticket::new(RawTicket::ready(Err(RawCallError::Refused(detail))));
        assert_eq!(ticket.try_take(), bad);
    }

    struct Count(AtomicUsize);

    impl Wake for Count {
        fn wake(self: Arc<Self>) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn resolving_wakes_the_polling_task() {
        let count = Arc::new(Count(AtomicUsize::new(0)));
        let waker = Waker::from(count.clone());
        let mut cx = Context::from_waker(&waker);
        let (mut ticket, resolver) = pending();
        assert!(Pin::new(&mut ticket).poll(&mut cx).is_pending());
        resolver.resolve(Ok(lattice_state::encode(&3u32).unwrap()));
        assert_eq!(count.0.load(Ordering::SeqCst), 1);
        assert_eq!(Pin::new(&mut ticket).poll(&mut cx), Poll::Ready(Ok(3)));
    }
}
