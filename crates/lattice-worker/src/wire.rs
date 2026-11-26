//! How `State` streams map onto sync messages (02-sync.md, Wire messages; 06-apps.md, Sync to viewers).
//!
//! A `State` stream has one writer, the worker. Its version is the writer's `seq`:
//!
//! - full state → `Snapshot{stream, covers: {worker: version}, payload}`;
//! - delta → `Change{stream, writer: worker, seq: version, prev_hash: 0, ts: 0, payload, sig: None}`.
//!
//! `prev_hash`, `ts`, and `sig` are unused for `State` streams in M1 (TBD: signing, 02-sync.md).

use lattice_key::{NodeId, Signature};
use lattice_sync_proto::{Change, Have, Message, Seq, StreamId};
use serde::Serialize;

/// The full state of `stream` at `version`.
pub(crate) fn snapshot(
    stream: StreamId,
    worker: NodeId,
    version: u64,
    payload: Vec<u8>,
) -> Message {
    let mut covers = Have::empty();
    covers.advance(worker, Seq::new(version));
    Message::Snapshot {
        stream,
        covers,
        payload,
    }
}

/// One delta of `stream` at `version`. `None` only if `Change`'s wire layout drifted (a bug).
pub(crate) fn delta(
    stream: StreamId,
    worker: NodeId,
    version: u64,
    payload: Vec<u8>,
) -> Option<Message> {
    let fields = ChangeFields {
        stream,
        writer: worker,
        seq: Seq::new(version),
        prev_hash: [0; 32],
        ts: 0,
        payload,
        sig: None,
    };
    let bytes = lattice_state::encode(&fields).ok()?;
    lattice_state::decode::<Change>(&bytes)
        .ok()
        .map(Message::Change)
}

/// The raw number of a `seq`.
///
/// TODO(sync-proto): replace with a `Seq` accessor once it exists (postcard: a `u64` newtype is its varint).
pub(crate) fn seq_raw(seq: Seq) -> u64 {
    lattice_state::encode(&seq)
        .ok()
        .and_then(|b| lattice_state::decode::<u64>(&b).ok())
        .unwrap_or(0)
}

/// The fields of [`Change`], in its serde order.
///
/// TODO(sync-proto): replace with `Change::new` once it exists. `Change` has private fields and
/// no constructor yet, so it is built by decoding this same-shaped struct (postcard is positional).
#[derive(Serialize)]
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

    fn id(seed: u8) -> NodeId {
        NodeKey::from_secret(SecretKeyBytes::new([seed; 32])).node_id()
    }

    #[test]
    fn a_delta_becomes_a_change_with_the_version_as_seq() {
        let stream: StreamId = "inst/1/state".parse().unwrap();
        let msg = delta(stream.clone(), id(1), 7, b"d".to_vec()).unwrap();
        let Message::Change(change) = msg else {
            panic!("not a change: {msg:?}");
        };
        // Recompute from scratch: the same fields encode to the same bytes.
        let want = ChangeFields {
            stream,
            writer: id(1),
            seq: Seq::new(7),
            prev_hash: [0; 32],
            ts: 0,
            payload: b"d".to_vec(),
            sig: None,
        };
        assert_eq!(
            lattice_state::encode(&change).unwrap(),
            lattice_state::encode(&want).unwrap()
        );
    }

    #[test]
    fn a_snapshot_covers_the_worker_at_the_version() {
        let msg = snapshot("worker/apps".parse().unwrap(), id(1), 3, Vec::new());
        let Message::Snapshot { covers, .. } = msg else {
            panic!("not a snapshot");
        };
        assert_eq!(covers.get(&id(1)), Some(Seq::new(3)));
    }
}
