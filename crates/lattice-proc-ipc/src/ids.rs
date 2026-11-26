//! Ids, offsets, and small value types shared by both daemons (04-worker.md, Journal).

/// Process id assigned by `latticed-proc`. Stable for the life of the record. `u64` on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProcId(u64);

impl ProcId {
    /// Wraps a raw id read from the wire.
    pub fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    /// The raw id.
    pub fn raw(self) -> u64 {
        self.0
    }
}

impl std::fmt::Display for ProcId {
    /// Decimal id.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Journal id: the app instance id (`iid`), chosen by `latticed`. Opaque to `latticed-proc`.
///
/// 1 to [`JournalId::MAX_LEN`] bytes. `latticed` uses the instance id as 8 bytes big-endian.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct JournalId(Vec<u8>);

impl JournalId {
    /// Longest id accepted on the wire.
    pub const MAX_LEN: usize = 64;

    /// Builds an id. It must not be empty.
    ///
    /// # Errors
    ///
    /// [`crate::FrameError::BadValue`] when `raw` is empty or too long.
    pub fn try_new(raw: Vec<u8>) -> Result<Self, crate::FrameError> {
        if raw.is_empty() || raw.len() > Self::MAX_LEN {
            return Err(crate::FrameError::BadValue(crate::message::tag::JOURNAL));
        }
        Ok(Self(raw))
    }

    /// The raw bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

/// Absolute byte offset into a journal since `Open`. Only grows, also across `Trim`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Offset(u64);

impl Offset {
    /// Wraps a raw offset.
    pub fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    /// The raw offset.
    pub fn raw(self) -> u64 {
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

/// One journal entry (04-worker.md, Journal). Nothing else is ever stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryKind {
    /// Output read from a PTY of the instance.
    Out {
        /// The process that wrote it.
        proc: ProcId,
        /// Raw bytes.
        bytes: Vec<u8>,
    },
    /// A process of the instance exited.
    Exit {
        /// The process.
        proc: ProcId,
        /// How it ended.
        status: ExitStatus,
    },
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
    /// No journal with that id.
    NoSuchJournal,
    /// `Open` named a journal that exists.
    JournalExists,
    /// `Drop` while a process of the journal still lives.
    Busy,
    /// `Spawn` failed (exec, PTY, or fork error).
    SpawnFailed,
}
