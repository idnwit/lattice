//! Wire messages (02-sync.md, Wire messages table).

use crate::stream::{Have, Seq, StreamId, StreamInfo};
use lattice_key::{NodeId, Signature};
use serde::{Deserialize, Serialize};

/// Reader-chosen id that matches an `Action` to its `ActionResult`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ActionId(u64);

impl ActionId {
    /// Wraps a raw id.
    pub fn new(raw: u64) -> Self {
        Self(raw)
    }
}

/// How a subscription starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SubscribeMode {
    /// From `have` to now, then live.
    Live,
    /// Replay history only.
    Replay,
}

/// One change in a writer's append-only log.
///
/// TBD: signatures cover batches of changes (02, Model). Batch shape?
/// Constructor and accessors land with the worker's raw stream and the deck's reader.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[allow(
    dead_code,
    reason = "L1 skeleton: read once sync streams are implemented"
)]
pub struct Change {
    stream: StreamId,
    writer: NodeId,
    seq: Seq,
    prev_hash: [u8; 32],
    ts: u64,
    payload: Vec<u8>,
    sig: Option<Signature>,
}

/// Every sync message. Variant order is the wire tag: append only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Message {
    /// Opens a session, negotiates versions.
    Hello {
        /// Sender id.
        node_id: NodeId,
        /// Device-list chain head (TBD: hash type, lands with lattice-identity).
        device_list_head: Option<[u8; 32]>,
        /// Supported protocol versions.
        versions: Vec<u16>,
    },
    /// Discover streams under a prefix.
    List {
        /// Only ids under this prefix; `None` lists all.
        prefix: Option<StreamId>,
    },
    /// Answer to `List`.
    Streams(Vec<StreamInfo>),
    /// Start or resume a stream.
    Subscribe {
        /// Stream to read.
        stream: StreamId,
        /// What the reader already has.
        have: Have,
        /// Live or replay.
        mode: SubscribeMode,
    },
    /// Full state when `have` is older than compaction or empty.
    Snapshot {
        /// Stream.
        stream: StreamId,
        /// Writers and seqs the snapshot covers.
        covers: Have,
        /// Kind-specific snapshot bytes.
        payload: Vec<u8>,
    },
    /// A live or replayed change.
    Change(Change),
    /// Flow control; lets the owner coalesce per reader.
    Ack {
        /// Stream.
        stream: StreamId,
        /// Reader cursor.
        have: Have,
    },
    /// Reader fell behind `Log` retention.
    Gap {
        /// Stream.
        stream: StreamId,
        /// First seq still kept.
        new_start: Seq,
    },
    /// Stop a stream.
    Unsubscribe {
        /// Stream.
        stream: StreamId,
    },
    /// Request a state change (input, resize, approve…).
    Action {
        /// Target stream.
        target: StreamId,
        /// Reader-chosen id.
        id: ActionId,
        /// Action name from the stream schema.
        name: String,
        /// Action payload.
        payload: Vec<u8>,
    },
    /// Outcome of an action; effects arrive as changes.
    ActionResult {
        /// Matching action id.
        id: ActionId,
        /// `None` on success, else a stable error code.
        error: Option<String>,
    },
    /// Fetch content-addressed bytes.
    BlobRequest {
        /// blake3 hash.
        hash: [u8; 32],
    },
    /// Error that keeps the connection open (e.g. `unsupported`).
    Error {
        /// Stable error code.
        code: String,
    },
}
