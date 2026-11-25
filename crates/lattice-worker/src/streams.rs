//! Stream ids for processes (02-sync.md, Examples).

use lattice_proc_ipc::ProcId;
use lattice_sync_proto::StreamId;

/// Which stream of a process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcChannel {
    /// `proc/<id>/raw`: raw PTY bytes, `Log` kind.
    Raw,
}

/// A process stream: `(process, channel)` ⇄ `proc/<id>/<channel>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcStream {
    proc: ProcId,
    channel: ProcChannel,
}

impl ProcStream {
    /// The stream for `channel` of `proc`.
    pub fn new(proc: ProcId, channel: ProcChannel) -> Self {
        Self { proc, channel }
    }

    /// The process.
    pub fn proc(&self) -> ProcId {
        self.proc
    }

    /// Its wire id.
    #[allow(
        clippy::todo,
        reason = "L1 skeleton: body lands with its contract test"
    )]
    pub fn to_stream_id(&self) -> StreamId {
        // format "proc/{raw id}/raw" and parse; the format is valid by construction
        todo!()
    }

    /// Parses a wire id; `None` when it is not a process stream.
    #[allow(
        clippy::todo,
        unused_variables,
        reason = "L1 skeleton: body lands with its contract test"
    )]
    pub fn parse(id: &StreamId) -> Option<Self> {
        // split on '/': ["proc", decimal id, "raw"]; anything else None
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "contract: lands with ProcStream"]
    fn proc_stream_id_roundtrips() {
        let s = ProcStream::new(ProcId::from_raw(42), ProcChannel::Raw);
        assert_eq!(s.to_stream_id().as_str(), "proc/42/raw");
        assert_eq!(ProcStream::parse(&s.to_stream_id()), Some(s));
        assert_eq!(ProcStream::parse(&"user/meta".parse().unwrap()), None);
    }
}
