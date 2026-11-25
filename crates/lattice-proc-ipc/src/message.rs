//! v1 requests and replies (04-worker.md, Local IPC table).

use crate::frame::{Frame, FrameError};

/// Process id assigned by `latticed-proc`. Stable for the life of the record.
///
/// TBD: width of the id on the wire (u32 or u64)?
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProcId(u64);

impl ProcId {
    /// Wraps a raw id read from the wire.
    pub fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    /// The raw id.
    #[allow(dead_code, reason = "L1 skeleton: read by the codec once implemented")]
    pub(crate) fn raw(self) -> u64 {
        self.0
    }
}

impl std::fmt::Display for ProcId {
    /// Decimal id, as used in stream ids (`proc/<id>/raw`).
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Absolute byte offset into a process's output since spawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Offset(u64);

impl Offset {
    /// Wraps a raw offset.
    pub fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    /// The raw offset.
    #[allow(dead_code, reason = "L1 skeleton: read by the codec once implemented")]
    pub(crate) fn raw(self) -> u64 {
        self.0
    }
}

/// How a child ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitStatus {
    /// Exited with a code.
    Code(i32),
    /// Killed by a signal.
    Signal(i32),
}

/// Stable error codes of `Error{code, msg}` replies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    /// Unknown message type (forward compatibility rule).
    Unsupported,
    /// Frame or field could not be parsed.
    Malformed,
    /// No process with that id.
    NoSuchProc,
    /// `Spawn` failed (exec, PTY, or fork error).
    SpawnFailed,
}

/// Everything `Spawn` needs. All fields are opaque to `latticed-proc`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpawnSpec {
    argv: Vec<Vec<u8>>,
    /// Env vars; latticed injects `LATTICE_PROC_ID`, `LATTICE_EVENTS_SOCK` (setter lands with spawn).
    #[allow(dead_code, reason = "L1 skeleton: read by the codec once implemented")]
    env: Vec<(Vec<u8>, Vec<u8>)>,
    cwd: Vec<u8>,
    cols: u16,
    rows: u16,
    tag: Vec<u8>,
}

impl SpawnSpec {
    /// Builds a spawn request. `argv` must not be empty.
    ///
    /// # Errors
    ///
    /// [`FrameError::BadValue`] when `argv` is empty or a size is zero.
    #[allow(
        clippy::todo,
        unused_variables,
        reason = "L1 skeleton: body lands with its contract test"
    )]
    pub fn try_new(
        argv: Vec<Vec<u8>>,
        cwd: Vec<u8>,
        cols: u16,
        rows: u16,
    ) -> Result<Self, FrameError> {
        // reject empty argv, zero cols, zero rows
        // env starts empty, tag starts empty; set them with with_env / with_tag
        todo!()
    }

    /// Sets the opaque tag latticed uses to find its own records after restart.
    pub fn with_tag(mut self, tag: Vec<u8>) -> Self {
        self.tag = tag;
        self
    }
}

/// One entry of a `Procs` reply.
///
/// Constructor and accessors land with `List` in latticed-proc and the worker's restart recovery.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code, reason = "L1 skeleton: read by the codec once implemented")]
pub struct ProcInfo {
    id: ProcId,
    pid: u32,
    tag: Vec<u8>,
    cols: u16,
    rows: u16,
    start_off: Offset,
    end_off: Offset,
    exit: Option<ExitStatus>,
}

/// `latticed` → `latticed-proc`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    /// First message. Carries version and feature bits.
    Hello {
        /// Protocol version.
        version: u16,
        /// Feature bits (`GetFd`, ...).
        features: u64,
    },
    /// Start a child on a new PTY.
    Spawn(SpawnSpec),
    /// List process records.
    List,
    /// Stream output from `from_off`.
    Subscribe {
        /// Target process.
        id: ProcId,
        /// First offset wanted.
        from_off: Offset,
    },
    /// Stop streaming.
    Unsubscribe {
        /// Target process.
        id: ProcId,
    },
    /// Write bytes to the PTY.
    Input {
        /// Target process.
        id: ProcId,
        /// Raw bytes.
        bytes: Vec<u8>,
    },
    /// Resize the PTY.
    Resize {
        /// Target process.
        id: ProcId,
        /// Columns.
        cols: u16,
        /// Rows.
        rows: u16,
    },
    /// Send a signal to the child.
    Signal {
        /// Target process.
        id: ProcId,
        /// Signal number.
        signo: i32,
    },
    /// Forget an exited record.
    Close {
        /// Target process.
        id: ProcId,
    },
}

/// `latticed-proc` → `latticed`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reply {
    /// Answer to `Hello`.
    Hello {
        /// Protocol version.
        version: u16,
        /// Feature bits (`GetFd`, ...).
        features: u64,
    },
    /// Answer to `Spawn`.
    Spawned {
        /// Target process.
        id: ProcId,
        /// OS pid.
        pid: u32,
    },
    /// Answer to `List`.
    Procs(Vec<ProcInfo>),
    /// Output bytes starting at `off`.
    Output {
        /// Target process.
        id: ProcId,
        /// Offset of the first byte.
        off: Offset,
        /// Raw bytes.
        bytes: Vec<u8>,
    },
    /// The child exited.
    Exited {
        /// Target process.
        id: ProcId,
        /// How it ended.
        status: ExitStatus,
    },
    /// The subscriber lagged; bytes before `new_start` were evicted.
    Gap {
        /// Target process.
        id: ProcId,
        /// Oldest offset still kept.
        new_start: Offset,
    },
    /// Request failed.
    Error {
        /// Stable error code.
        code: ErrorCode,
        /// Human text. Never secrets.
        msg: String,
    },
}

/// Appends one framed request to `out`.
#[allow(
    clippy::todo,
    unused_variables,
    clippy::ptr_arg,
    reason = "L1 skeleton: body lands with its contract test"
)]
pub fn encode_request(req: &Request, out: &mut Vec<u8>) {
    // reserve header; write one TLV per field with the v1 tag table
    // patch the header length once the payload size is known
    todo!()
}

/// Parses a request frame. Unknown tags are ignored.
///
/// # Errors
///
/// [`FrameError`] on a missing or malformed field. An unknown type returns
/// `Ok(None)`: the server answers `Error{Unsupported}` and keeps the connection.
#[allow(
    clippy::todo,
    unused_variables,
    reason = "L1 skeleton: body lands with its contract test"
)]
pub fn decode_request(frame: &Frame) -> Result<Option<Request>, FrameError> {
    // match frame.msg_type() against the v1 table; unknown: Ok(None)
    // collect known tags from frame.tlvs(), skip unknown tags
    // build the variant; a missing required tag: MissingTag
    todo!()
}

/// Appends one framed reply to `out`.
#[allow(
    clippy::todo,
    unused_variables,
    clippy::ptr_arg,
    reason = "L1 skeleton: body lands with its contract test"
)]
pub fn encode_reply(reply: &Reply, out: &mut Vec<u8>) {
    // same layout as encode_request, reply type codes
    todo!()
}

/// Parses a reply frame. Same rules as [`decode_request`].
///
/// # Errors
///
/// [`FrameError`] on a missing or malformed field.
#[allow(
    clippy::todo,
    unused_variables,
    reason = "L1 skeleton: body lands with its contract test"
)]
pub fn decode_reply(frame: &Frame) -> Result<Option<Reply>, FrameError> {
    // mirror of decode_request for replies
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FrameDecoder;

    fn roundtrip(req: &Request) -> Request {
        let mut wire = Vec::new();
        encode_request(req, &mut wire);
        let mut dec = FrameDecoder::new();
        dec.push(&wire);
        let frame = dec.next_frame().unwrap().unwrap();
        decode_request(&frame).unwrap().unwrap()
    }

    #[test]
    #[ignore = "contract: lands with encode_request / decode_request"]
    fn every_request_roundtrips() {
        let spec = SpawnSpec::try_new(vec![b"zsh".to_vec()], b"/".to_vec(), 80, 24).unwrap();
        let cases = [
            Request::Hello {
                version: crate::PROTOCOL_VERSION,
                features: 0,
            },
            Request::Spawn(spec.with_tag(b"t".to_vec())),
            Request::List,
            Request::Subscribe {
                id: ProcId::from_raw(1),
                from_off: Offset::from_raw(7),
            },
            Request::Input {
                id: ProcId::from_raw(1),
                bytes: b"ls\r".to_vec(),
            },
            Request::Resize {
                id: ProcId::from_raw(1),
                cols: 120,
                rows: 40,
            },
        ];
        for case in cases {
            assert_eq!(roundtrip(&case), case, "case={case:?}");
        }
    }

    #[test]
    #[ignore = "contract: lands with decode_request"]
    fn unknown_type_decodes_to_none() {
        let mut dec = FrameDecoder::new();
        // len 0, type 0xFFFF, flags 0 (byte order TBD in frame.rs).
        dec.push(&[0, 0, 0, 0, 0xFF, 0xFF, 0, 0]);
        let frame = dec.next_frame().unwrap().unwrap();
        assert_eq!(decode_request(&frame), Ok(None));
    }

    #[test]
    #[ignore = "contract: lands with SpawnSpec::try_new"]
    fn spawn_spec_rejects_empty_argv() {
        assert!(SpawnSpec::try_new(Vec::new(), b"/".to_vec(), 80, 24).is_err());
    }

    #[test]
    #[ignore = "contract: lands with encode_reply / decode_reply"]
    fn gap_and_exit_replies_roundtrip() {
        let cases = [
            Reply::Gap {
                id: ProcId::from_raw(2),
                new_start: Offset::from_raw(4096),
            },
            Reply::Exited {
                id: ProcId::from_raw(2),
                status: ExitStatus::Code(0),
            },
            Reply::Error {
                code: ErrorCode::Unsupported,
                msg: String::new(),
            },
        ];
        for case in cases {
            let mut wire = Vec::new();
            encode_reply(&case, &mut wire);
            let mut dec = FrameDecoder::new();
            dec.push(&wire);
            let frame = dec.next_frame().unwrap().unwrap();
            assert_eq!(
                decode_reply(&frame).unwrap().unwrap(),
                case,
                "case={case:?}"
            );
        }
    }
}
