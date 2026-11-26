//! Stream ids served by the worker (02-sync.md, Examples; 06-apps.md, Sync to viewers).

use lattice_app::InstanceId;
use lattice_sync_proto::StreamId;

/// A stream the worker owns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerStream {
    /// `worker/apps`: instance records; target of `create`, `close`, `kill`.
    Apps,
    /// `inst/<iid>/state`: one instance's state; target of its app actions.
    State(InstanceId),
}

impl WorkerStream {
    /// Its wire id.
    pub fn to_stream_id(&self) -> StreamId {
        let text = match self {
            Self::Apps => "worker/apps".to_owned(),
            Self::State(iid) => format!("inst/{iid}/state"),
        };
        #[allow(
            clippy::expect_used,
            reason = "lowercase words and a decimal number form a valid stream id"
        )]
        text.parse()
            .expect("worker stream ids are valid by construction")
    }

    /// Parses a wire id; `None` when the worker does not own it.
    pub fn parse(id: &StreamId) -> Option<Self> {
        let mut parts = id.as_str().split('/');
        let stream = match (parts.next(), parts.next(), parts.next()) {
            (Some("worker"), Some("apps"), None) => Self::Apps,
            (Some("inst"), Some(iid), Some("state")) => Self::State(parse_iid(iid)?),
            _ => return None,
        };
        parts.next().is_none().then_some(stream)
    }
}

/// A canonical decimal instance id: no sign, no leading zero.
fn parse_iid(text: &str) -> Option<InstanceId> {
    if text.len() > 1 && text.starts_with('0') {
        return None;
    }
    if !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse().ok().map(InstanceId::from_raw)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_stream_ids_roundtrip() {
        let s = WorkerStream::State(InstanceId::from_raw(42));
        assert_eq!(s.to_stream_id().as_str(), "inst/42/state");
        assert_eq!(WorkerStream::parse(&s.to_stream_id()), Some(s));
        assert_eq!(
            WorkerStream::parse(&"worker/apps".parse().unwrap()),
            Some(WorkerStream::Apps)
        );
        for other in [
            "user/meta",
            "inst/x/state",
            "inst/07/state",
            "inst/1/state/more",
            "worker/apps/x",
        ] {
            assert_eq!(
                WorkerStream::parse(&other.parse().unwrap()),
                None,
                "case={other}"
            );
        }
    }
}
