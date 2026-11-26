//! Stream addressing and cursors (02-sync.md, Model).

use lattice_key::NodeId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::str::FromStr;

/// Max length of a stream id, in bytes (02-sync.md, Model; proposed).
pub const MAX_STREAM_ID_LEN: usize = 255;

/// Stream path, e.g. `worker/apps`, `inst/<iid>/state`, `user/meta`.
///
/// Valid ids (02-sync.md, Model; proposed): at most [`MAX_STREAM_ID_LEN`] bytes,
/// non-empty segments of `[a-z0-9._-]` separated by `/`, no leading or
/// trailing `/`.
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

    fn from_str(s: &str) -> Result<Self, StreamIdError> {
        if s.len() > MAX_STREAM_ID_LEN {
            return Err(StreamIdError);
        }
        // `split` yields an empty segment for "", a leading, trailing, or doubled '/'.
        let valid = s.split('/').all(|seg| {
            !seg.is_empty()
                && seg
                    .bytes()
                    .all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'.' | b'_' | b'-'))
        });
        if valid {
            Ok(Self(s.to_owned()))
        } else {
            Err(StreamIdError)
        }
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
    /// Single writer, append entries.
    Log,
    /// Single writer, deltas + full state (app instance state, `worker/apps`).
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
    pub fn advance(&mut self, writer: NodeId, seq: Seq) {
        let slot = self.0.entry(writer).or_insert(seq);
        *slot = (*slot).max(seq);
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
    fn stream_id_accepts_paths_and_rejects_empty_segments() {
        for ok in ["inst/1/state", "worker/apps", "user/meta", "a.b_c-d/0"] {
            assert_eq!(
                ok.parse::<StreamId>().map(|s| s.as_str().to_owned()),
                Ok(ok.into()),
                "case={ok}"
            );
        }
        for bad in ["", "/proc", "proc/", "proc//raw", "/"] {
            assert_eq!(bad.parse::<StreamId>(), Err(StreamIdError), "case={bad}");
        }
    }

    #[test]
    fn stream_id_rejects_other_characters() {
        for bad in ["Inst/1", "inst/1 ", "inst/\u{e9}", "inst\\1", "inst/1:2"] {
            assert_eq!(bad.parse::<StreamId>(), Err(StreamIdError), "case={bad}");
        }
    }

    #[test]
    fn stream_id_max_length_is_255_bytes() {
        let max = "a".repeat(MAX_STREAM_ID_LEN);
        assert!(max.parse::<StreamId>().is_ok());
        let over = "a".repeat(MAX_STREAM_ID_LEN + 1);
        assert_eq!(over.parse::<StreamId>(), Err(StreamIdError));
    }

    #[test]
    fn stream_id_is_validated_when_deserialized() {
        // postcard string: varint length 1, then "/".
        assert!(postcard::from_bytes::<StreamId>(&[1, b'/']).is_err());
        let ok = postcard::to_allocvec(&"worker/apps".parse::<StreamId>().unwrap()).unwrap();
        assert_eq!(
            postcard::from_bytes::<StreamId>(&ok).unwrap().as_str(),
            "worker/apps"
        );
    }

    #[test]
    fn have_never_moves_backwards() {
        let w =
            lattice_key::NodeKey::from_secret(lattice_key::SecretKeyBytes::new([1; 32])).node_id();
        let mut have = Have::empty();
        have.advance(w, Seq::new(5));
        have.advance(w, Seq::new(3));
        assert_eq!(have.get(&w), Some(Seq::new(5)));
    }
}
