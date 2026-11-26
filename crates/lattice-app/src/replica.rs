//! A deck's copy of one instance state (06-apps.md, Sync to viewers). Sans-IO.
//!
//! Lives in the contract so the deck host (`lattice-deck-core`, which feeds it from the link)
//! and viewers (`lattice-deck-ui`, which read it) share one type. It logs nothing: the host
//! that feeds it logs a [`ReplicaEvent::Resync`].

use lattice_state::Synced;
use serde::{Deserialize, Serialize};

/// Outcome of feeding one message of a state stream. Crosses the viewer ABI (D43).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReplicaEvent {
    /// The state changed; the viewer re-renders.
    Changed,
    /// The delta did not follow the held version, or did not apply: resubscribe without a version.
    Resync,
}

/// Full state plus version, kept current with deltas.
#[derive(Debug, Default)]
pub struct Replica<S: Synced + Default> {
    state: S,
    version: Option<u64>,
}

impl<S: Synced + Default> Replica<S> {
    /// The version to send in `Subscribe`; `None` before the first full state.
    pub fn version(&self) -> Option<u64> {
        self.version
    }

    /// The state the viewer renders.
    pub fn state(&self) -> &S {
        &self.state
    }

    /// Replaces the state with a full state at `version`.
    ///
    /// Bytes that do not decode reset the replica (no version) and ask for a resync.
    pub fn on_full_state(&mut self, version: u64, bytes: &[u8]) -> ReplicaEvent {
        match lattice_state::decode::<S>(bytes) {
            Ok(state) => {
                self.state = state;
                self.version = Some(version);
                ReplicaEvent::Changed
            }
            Err(_) => {
                self.state = S::default();
                self.version = None;
                ReplicaEvent::Resync
            }
        }
    }

    /// Applies one delta.
    pub fn on_delta(&mut self, version: u64, bytes: &[u8]) -> ReplicaEvent {
        // 06-apps.md, Sync to viewers rule 6: only the next version applies.
        let next = self.version.and_then(|v| v.checked_add(1));
        if next != Some(version) {
            return ReplicaEvent::Resync;
        }
        let applied = lattice_state::decode::<S::Change>(bytes).and_then(|c| self.state.apply(c));
        match applied {
            Ok(()) => {
                self.version = Some(version);
                ReplicaEvent::Changed
            }
            Err(_) => {
                // Rule 8: a delta that fails to apply is a bug. The host logs the resync.
                self.version = None;
                ReplicaEvent::Resync
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lattice_state::{Draft, Value, encode};

    /// A runner-side value after `set(n)` and commit, with the delta it produced.
    fn committed(n: u32) -> (Value<u32>, Vec<u8>) {
        let mut v = Value::new(0);
        v.set(n);
        let delta = encode(&Draft(&v)).unwrap();
        v.commit();
        (v, delta)
    }

    #[test]
    fn delta_out_of_order_asks_for_resync() {
        let mut r = Replica::<Value<u32>>::default();
        let full = encode(&Value::new(1u32)).unwrap();
        assert_eq!(r.on_full_state(5, &full), ReplicaEvent::Changed);
        let (_, delta) = committed(2);
        assert_eq!(r.on_delta(7, &delta), ReplicaEvent::Resync);
        assert_eq!(r.version(), Some(5));
    }

    #[test]
    fn full_state_then_deltas_equal_the_runner_state() {
        let mut r = Replica::<Value<u32>>::default();
        assert_eq!(r.version(), None);
        r.on_full_state(0, &encode(&Value::new(0u32)).unwrap());
        let (runner, delta) = committed(9);
        assert_eq!(r.on_delta(1, &delta), ReplicaEvent::Changed);
        assert_eq!(r.state().get(), runner.get());
        assert_eq!(r.version(), Some(1));
    }

    #[test]
    fn bad_bytes_ask_for_a_resync_without_a_version() {
        let mut r = Replica::<Value<u32>>::default();
        assert_eq!(r.on_full_state(1, &[0xff]), ReplicaEvent::Resync);
        assert_eq!(r.version(), None);
        r.on_full_state(1, &encode(&Value::new(0u32)).unwrap());
        assert_eq!(r.on_delta(2, &[0xff]), ReplicaEvent::Resync);
        assert_eq!(r.version(), None);
        let (_, delta) = committed(3);
        assert_eq!(
            r.on_delta(3, &delta),
            ReplicaEvent::Resync,
            "no version held"
        );
    }
}
