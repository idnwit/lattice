//! Stream addressing and cursors (02-sync.md, Model).

use lattice_key::NodeId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::str::FromStr;

/// Stream path, e.g. `proc/<id>/raw`, `worker/metrics`, `user/meta`.
///
/// TBD: allowed characters and max length of a stream id.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct StreamId(String);

/// A stream id failed validation.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid stream id")]
pub struct StreamIdError;

impl StreamId {
    /// The path text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for StreamId {
    type Err = StreamIdError;

    #[allow(
        clippy::todo,
        unused_variables,
        reason = "L1 skeleton: body lands with its contract test"
    )]
    fn from_str(s: &str) -> Result<Self, StreamIdError> {
        // non-empty segments separated by '/', no leading or trailing '/'
        // character set and length limit: TBD (see type doc)
        todo!()
    }
}

impl TryFrom<String> for StreamId {
    type Error = StreamIdError;

    fn try_from(s: String) -> Result<Self, StreamIdError> {
        s.parse()
    }
}

impl From<StreamId> for String {
    fn from(id: StreamId) -> String {
        id.0
    }
}

/// Stream kind (02-sync.md, Stream kinds).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StreamKind {
    /// Single writer, append entries (raw PTY bytes, hook events).
    Log,
    /// Single writer, diffs + full snapshot (terminal screen).
    State,
    /// Single writer, batches of points (metrics).
    Series,
    /// Multi writer CRDT (user meta).
    Doc,
}

/// Per-writer sequence number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Seq(u64);

impl Seq {
    /// Wraps a raw sequence number.
    pub fn new(raw: u64) -> Self {
        Self(raw)
    }
}

/// Reader cursor: highest seq seen per writer (version vector).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Have(BTreeMap<NodeId, Seq>);

impl Have {
    /// Empty cursor: "send me everything" (or a snapshot).
    pub fn empty() -> Self {
        Self::default()
    }

    /// Highest seq seen from `writer`.
    pub fn get(&self, writer: &NodeId) -> Option<Seq> {
        self.0.get(writer).copied()
    }

    /// Records `seq` from `writer`; never moves backwards.
    #[allow(
        clippy::todo,
        unused_variables,
        reason = "L1 skeleton: body lands with its contract test"
    )]
    pub fn advance(&mut self, writer: NodeId, seq: Seq) {
        // insert max(current, seq)
        todo!()
    }
}

/// One entry of a `Streams` reply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[allow(
    dead_code,
    reason = "L1 skeleton: accessors land with the deck's stream list"
)]
pub struct StreamInfo {
    id: StreamId,
    owner: NodeId,
    kind: StreamKind,
    schema: String,
}

impl StreamInfo {
    /// Describes a stream.
    pub fn new(id: StreamId, owner: NodeId, kind: StreamKind, schema: String) -> Self {
        Self {
            id,
            owner,
            kind,
            schema,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "contract: lands with StreamId::from_str"]
    fn stream_id_accepts_paths_and_rejects_empty_segments() {
        for ok in ["proc/1/raw", "worker/metrics", "user/meta"] {
            assert_eq!(
                ok.parse::<StreamId>().map(|s| s.as_str().to_owned()),
                Ok(ok.into()),
                "case={ok}"
            );
        }
        for bad in ["", "/proc", "proc/", "proc//raw"] {
            assert_eq!(bad.parse::<StreamId>(), Err(StreamIdError), "case={bad}");
        }
    }

    #[test]
    #[ignore = "contract: lands with Have::advance"]
    fn have_never_moves_backwards() {
        let w =
            lattice_key::NodeKey::from_secret(lattice_key::SecretKeyBytes::new([1; 32])).node_id();
        let mut have = Have::empty();
        have.advance(w, Seq::new(5));
        have.advance(w, Seq::new(3));
        assert_eq!(have.get(&w), Some(Seq::new(5)));
    }
}
