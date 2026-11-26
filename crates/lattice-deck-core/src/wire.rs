//! `State` stream messages as a deck reads them (06-apps.md, Sync to viewers). Sans-IO.
//!
//! The worker is the single writer of `worker/apps` and `inst/<iid>/state`. Its `seq` is the
//! stream version: a full state arrives as `Snapshot{covers: {worker: version}}`, a delta as
//! `Change{writer: worker, seq: version, payload}`.

use lattice_app::{Replica, ReplicaEvent};
use lattice_key::{NodeId, Signature};
use lattice_state::Synced;
use lattice_sync_proto::{Change, Message, Seq, StreamId};
use serde::{Deserialize, Serialize};

/// One update of a `State` stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StateUpdate {
    /// The full state at `version`.
    Full {
        /// Version of the state.
        version: u64,
        /// Encoded state.
        bytes: Vec<u8>,
    },
    /// One delta at `version`.
    Delta {
        /// Version of this delta.
        version: u64,
        /// Encoded change.
        bytes: Vec<u8>,
    },
}

impl StateUpdate {
    /// Reads a `Snapshot` or `Change` written by `worker`.
    ///
    /// `None` for any other message, or a state from another writer.
    pub fn from_message(msg: Message, worker: &NodeId) -> Option<(StreamId, Self)> {
        match msg {
            Message::Snapshot {
                stream,
                covers,
                payload,
            } => {
                let version = seq_raw(covers.get(worker)?);
                Some((
                    stream,
                    Self::Full {
                        version,
                        bytes: payload,
                    },
                ))
            }
            Message::Change(change) => {
                let f = change_fields(&change)?;
                (f.writer == *worker).then(|| {
                    let delta = Self::Delta {
                        version: seq_raw(f.seq),
                        bytes: f.payload,
                    };
                    (f.stream, delta)
                })
            }
            _ => None,
        }
    }
}

impl StateUpdate {
    /// Feeds this update to `replica`. A resync is logged here: the replica logs nothing.
    pub fn apply_to<S: Synced + Default>(&self, replica: &mut Replica<S>) -> ReplicaEvent {
        let event = match self {
            Self::Full { version, bytes } => replica.on_full_state(*version, bytes),
            Self::Delta { version, bytes } => replica.on_delta(*version, bytes),
        };
        if event == ReplicaEvent::Resync {
            tracing::warn!(kind = self.kind(), held = ?replica.version(), "state update asks for a resync");
        }
        event
    }

    fn kind(&self) -> &'static str {
        match self {
            Self::Full { .. } => "full",
            Self::Delta { .. } => "delta",
        }
    }
}

/// The raw number of a `seq`.
///
/// TODO(sync-proto): replace with a `Seq` accessor once it exists (postcard: a `u64` newtype is its varint).
fn seq_raw(seq: Seq) -> u64 {
    lattice_state::encode(&seq)
        .ok()
        .and_then(|b| lattice_state::decode::<u64>(&b).ok())
        .unwrap_or(0)
}

/// Reads the fields of a [`Change`].
///
/// TODO(sync-proto): replace with `Change` accessors once they exist. Its fields are private,
/// so they are read by re-decoding it as this same-shaped struct (postcard is positional).
fn change_fields(change: &Change) -> Option<ChangeFields> {
    let bytes = lattice_state::encode(change).ok()?;
    lattice_state::decode(&bytes).ok()
}

/// The fields of [`Change`], in its serde order.
#[derive(Serialize, Deserialize)]
struct ChangeFields {
    stream: StreamId,
    writer: NodeId,
    seq: Seq,
    prev_hash: [u8; 32],
    ts: u64,
    payload: Vec<u8>,
    sig: Option<Signature>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use lattice_key::{NodeKey, SecretKeyBytes};
    use lattice_sync_proto::Have;

    fn id(seed: u8) -> NodeId {
        NodeKey::from_secret(SecretKeyBytes::new([seed; 32])).node_id()
    }

    fn change(writer: NodeId, seq: u64) -> Message {
        let f = ChangeFields {
            stream: "inst/1/state".parse().unwrap(),
            writer,
            seq: Seq::new(seq),
            prev_hash: [0; 32],
            ts: 0,
            payload: b"d".to_vec(),
            sig: None,
        };
        let c: Change = lattice_state::decode(&lattice_state::encode(&f).unwrap()).unwrap();
        Message::Change(c)
    }

    #[test]
    fn updates_route_to_full_state_and_delta() {
        use lattice_state::{Draft, Value, encode};
        let mut r = Replica::<Value<u32>>::default();
        let full = StateUpdate::Full {
            version: 0,
            bytes: encode(&Value::new(4u32)).unwrap(),
        };
        assert_eq!(full.apply_to(&mut r), ReplicaEvent::Changed);
        let mut runner = Value::new(4u32);
        runner.set(5);
        let bytes = encode(&Draft(&runner)).unwrap();
        let delta = StateUpdate::Delta { version: 1, bytes };
        assert_eq!(delta.apply_to(&mut r), ReplicaEvent::Changed);
        assert_eq!(*r.state().get(), 5);
        assert_eq!(
            delta.apply_to(&mut r),
            ReplicaEvent::Resync,
            "same version twice"
        );
    }

    #[test]
    fn a_change_from_the_worker_is_a_delta_at_its_seq() {
        let (stream, update) = StateUpdate::from_message(change(id(1), 9), &id(1)).unwrap();
        assert_eq!(stream.as_str(), "inst/1/state");
        let want = StateUpdate::Delta {
            version: 9,
            bytes: b"d".to_vec(),
        };
        assert_eq!(update, want);
        assert_eq!(StateUpdate::from_message(change(id(2), 9), &id(1)), None);
    }

    #[test]
    fn a_snapshot_is_the_full_state_at_the_worker_seq() {
        let mut covers = Have::empty();
        covers.advance(id(1), Seq::new(4));
        let msg = Message::Snapshot {
            stream: "worker/apps".parse().unwrap(),
            covers,
            payload: vec![0],
        };
        let want = StateUpdate::Full {
            version: 4,
            bytes: vec![0],
        };
        assert_eq!(
            StateUpdate::from_message(msg.clone(), &id(1)).map(|(_, u)| u),
            Some(want)
        );
        assert_eq!(StateUpdate::from_message(msg, &id(2)), None);
        let other = Message::Error {
            code: String::new(),
        };
        assert_eq!(StateUpdate::from_message(other, &id(1)), None);
    }
}
