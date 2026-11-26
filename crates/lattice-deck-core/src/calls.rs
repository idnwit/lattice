//! Typed app calls over a worker link (06-apps.md, Interface rule 3). Sans-IO.
//!
//! A [`LinkCaller`] is a [`Caller`] bound to one instance. It queues each encoded action with
//! the [`Resolver`] of its ticket; the link owner sends the queue
//! ([`crate::WorkerLink::flush_calls`]) and resolves tickets as `ActionResult`s arrive.
//!
//! Rules:
//! - A call made while the link is down settles `NotConnected` at once: nothing was sent.
//! - When the link ends, queued calls settle `NotConnected` and calls in flight `Unknown`.
//!   A ticket never resends.

use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use lattice_app::{AppError, Caller, InstanceId, RawCallError, RawTicket, Resolver};
use lattice_sync_proto::{ActionId, Message};

/// One call waiting to be sent.
#[derive(Debug)]
pub(crate) struct Outbound {
    pub(crate) iid: InstanceId,
    pub(crate) payload: Vec<u8>,
    pub(crate) resolver: Resolver,
}

#[derive(Debug)]
struct Queue {
    open: bool,
    outbound: VecDeque<Outbound>,
}

/// Calls queued by [`LinkCaller`]s of one link, shared with them.
#[derive(Debug, Clone)]
pub(crate) struct CallQueue(Arc<Mutex<Queue>>);

impl CallQueue {
    /// An open, empty queue.
    pub(crate) fn new() -> Self {
        Self(Arc::new(Mutex::new(Queue {
            open: true,
            outbound: VecDeque::new(),
        })))
    }

    /// Locks the queue. A poisoned lock still holds a whole queue: every write is one call.
    fn lock(&self) -> MutexGuard<'_, Queue> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn push(&self, iid: InstanceId, payload: Vec<u8>) -> RawTicket {
        let mut queue = self.lock();
        if !queue.open {
            return RawTicket::ready(Err(RawCallError::NotConnected));
        }
        let (ticket, resolver) = RawTicket::pair();
        queue.outbound.push_back(Outbound {
            iid,
            payload,
            resolver,
        });
        ticket
    }

    /// Takes every queued call, oldest first.
    pub(crate) fn drain(&self) -> Vec<Outbound> {
        self.lock().outbound.drain(..).collect()
    }

    /// The link ended: queued calls settle `NotConnected`; later calls too.
    pub(crate) fn close(&self) {
        let queued = {
            let mut queue = self.lock();
            queue.open = false;
            std::mem::take(&mut queue.outbound)
        };
        for call in queued {
            call.resolver.resolve(Err(RawCallError::NotConnected));
        }
    }
}

/// Tickets of calls in flight, by action id.
#[derive(Debug, Default)]
pub(crate) struct Pending {
    waiting: BTreeMap<ActionId, Resolver>,
}

impl Pending {
    /// Tracks a sent call.
    pub(crate) fn insert(&mut self, id: ActionId, resolver: Resolver) {
        self.waiting.insert(id, resolver);
    }

    /// Settles the ticket an `ActionResult` answers; other messages are ignored.
    pub(crate) fn on_message(&mut self, msg: &Message) {
        let Message::ActionResult {
            id,
            error,
            result,
            detail,
        } = msg
        else {
            return;
        };
        let Some(resolver) = self.waiting.remove(id) else {
            return;
        };
        resolver.resolve(match error {
            None => Ok(result.clone()),
            Some(code) => Err(RawCallError::Refused(AppError::from_wire(
                code.clone(),
                detail.clone(),
            ))),
        });
    }

    /// The link ended: every call in flight settles `Unknown` (its resolver drops).
    pub(crate) fn clear(&mut self) {
        self.waiting.clear();
    }
}

/// A [`Caller`] for one instance over a worker link. Cheap to clone.
///
/// Get one from [`crate::WorkerLink::caller`] and wrap it in a generated client:
/// `TerminalApiClient::new(link.caller(iid))`.
#[derive(Debug, Clone)]
pub struct LinkCaller {
    queue: CallQueue,
    iid: InstanceId,
}

impl LinkCaller {
    pub(crate) fn new(queue: CallQueue, iid: InstanceId) -> Self {
        Self { queue, iid }
    }
}

impl Caller for LinkCaller {
    fn call(&self, action: Vec<u8>) -> RawTicket {
        self.queue.push(self.iid, action)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing, reason = "tests index fixed vectors")]

    use super::*;

    fn result(id: u64, error: Option<&str>, result: Vec<u8>, detail: Vec<u8>) -> Message {
        Message::ActionResult {
            id: ActionId::new(id),
            error: error.map(str::to_owned),
            result,
            detail,
        }
    }

    #[test]
    fn queued_calls_resolve_from_their_action_results() {
        let queue = CallQueue::new();
        let caller = LinkCaller::new(queue.clone(), InstanceId::from_raw(3));
        let mut ok = caller.call(vec![1]);
        let mut refused = caller.call(vec![2]);
        let mut pending = Pending::default();
        for (n, call) in (1..).zip(queue.drain()) {
            assert_eq!(call.iid, InstanceId::from_raw(3));
            pending.insert(ActionId::new(n), call.resolver);
        }
        assert_eq!(ok.try_take(), None);
        pending.on_message(&result(2, Some("bad_size"), Vec::new(), vec![1]));
        pending.on_message(&result(1, None, vec![7], Vec::new()));
        pending.on_message(&result(9, None, Vec::new(), Vec::new()));
        assert_eq!(ok.try_take(), Some(Ok(vec![7])));
        let want = AppError::from_wire("bad_size".to_owned(), vec![1]);
        assert_eq!(refused.try_take(), Some(Err(RawCallError::Refused(want))));
    }

    #[test]
    fn link_end_settles_queued_not_connected_and_in_flight_unknown() {
        let queue = CallQueue::new();
        let caller = LinkCaller::new(queue.clone(), InstanceId::from_raw(1));
        let mut in_flight = caller.call(vec![1]);
        let mut pending = Pending::default();
        let sent = queue.drain().pop().unwrap();
        pending.insert(ActionId::new(1), sent.resolver);
        let mut queued = caller.call(vec![2]);
        queue.close();
        pending.clear();
        assert_eq!(in_flight.try_take(), Some(Err(RawCallError::Unknown)));
        assert_eq!(queued.try_take(), Some(Err(RawCallError::NotConnected)));
        let mut late = caller.call(vec![3]);
        assert_eq!(late.try_take(), Some(Err(RawCallError::NotConnected)));
    }
}
