//! v1 requests and replies (04-worker.md, Local IPC table).

use crate::frame::FrameError;
use crate::ids::{EntryKind, ErrorCode, ExitStatus, JournalId, Offset, ProcId};

/// Default journal budget when `latticed` has no better value: 16 MiB.
pub const DEFAULT_JOURNAL_BUDGET: u64 = 16 << 20;

/// Smallest journal budget: the size of one `Exit` entry.
pub const MIN_JOURNAL_BUDGET: u64 = 8;

/// Longest `meta` accepted by `Open`: 64 KiB.
pub const MAX_META_LEN: usize = 64 << 10;

/// Everything `Open` needs to create a journal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenSpec {
    journal: JournalId,
    /// App id, package hash, params, encoded by `latticed`. Opaque here.
    meta: Vec<u8>,
    /// Byte budget; past it the oldest entries drop.
    budget: u64,
}

impl OpenSpec {
    /// Builds an open request.
    ///
    /// # Errors
    ///
    /// [`FrameError::BadValue`] when `budget` is below [`MIN_JOURNAL_BUDGET`] or
    /// `meta` is longer than [`MAX_META_LEN`].
    pub fn try_new(journal: JournalId, meta: Vec<u8>, budget: u64) -> Result<Self, FrameError> {
        if budget < MIN_JOURNAL_BUDGET {
            return Err(FrameError::BadValue(tag::BUDGET));
        }
        if meta.len() > MAX_META_LEN {
            return Err(FrameError::BadValue(tag::META));
        }
        Ok(Self {
            journal,
            meta,
            budget,
        })
    }

    /// The journal to create.
    pub fn journal(&self) -> &JournalId {
        &self.journal
    }

    /// The opaque meta.
    pub fn meta(&self) -> &[u8] {
        &self.meta
    }

    /// Byte budget.
    pub fn budget(&self) -> u64 {
        self.budget
    }
}

/// Everything `Spawn` needs. Fields other than the journal are opaque to `latticed-proc`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpawnSpec {
    journal: JournalId,
    argv: Vec<Vec<u8>>,
    /// Env vars merged onto the daemon's env; latticed injects `LATTICE_INSTANCE_ID`, `LATTICE_PROC_ID`.
    env: Vec<(Vec<u8>, Vec<u8>)>,
    cwd: Vec<u8>,
    cols: u16,
    rows: u16,
}

impl SpawnSpec {
    /// Builds a spawn request into `journal`. `argv` must not be empty.
    ///
    /// # Errors
    ///
    /// [`FrameError::BadValue`] when `argv` is empty or a size is zero.
    /// An empty `cwd` means the daemon's cwd.
    pub fn try_new(
        journal: JournalId,
        argv: Vec<Vec<u8>>,
        cwd: Vec<u8>,
        cols: u16,
        rows: u16,
    ) -> Result<Self, FrameError> {
        if argv.is_empty() {
            return Err(FrameError::BadValue(tag::ARGV));
        }
        if cols == 0 {
            return Err(FrameError::BadValue(tag::COLS));
        }
        if rows == 0 {
            return Err(FrameError::BadValue(tag::ROWS));
        }
        Ok(Self {
            journal,
            argv,
            env: Vec::new(),
            cwd,
            cols,
            rows,
        })
    }

    /// Adds one env var. A later var with the same name wins.
    ///
    /// # Errors
    ///
    /// [`FrameError::BadValue`] when `name` is empty or holds `=` or NUL.
    pub fn with_env(mut self, name: Vec<u8>, value: Vec<u8>) -> Result<Self, FrameError> {
        if name.is_empty() || name.iter().any(|b| *b == b'=' || *b == 0) {
            return Err(FrameError::BadValue(tag::ENV));
        }
        self.env.push((name, value));
        Ok(self)
    }

    /// The journal that receives output and exit.
    pub fn journal(&self) -> &JournalId {
        &self.journal
    }

    /// Program and arguments. Never empty.
    pub fn argv(&self) -> &[Vec<u8>] {
        &self.argv
    }

    /// Env vars, in order.
    pub fn env(&self) -> &[(Vec<u8>, Vec<u8>)] {
        &self.env
    }

    /// Working directory; empty means inherit.
    pub fn cwd(&self) -> &[u8] {
        &self.cwd
    }

    /// PTY size `(cols, rows)`.
    pub fn size(&self) -> (u16, u16) {
        (self.cols, self.rows)
    }
}

/// One process of a journal, in a `Journals` reply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcInfo {
    id: ProcId,
    pid: u32,
    cols: u16,
    rows: u16,
    exit: Option<ExitStatus>,
}

impl ProcInfo {
    /// A process record as `latticed-proc` lists it.
    pub fn new(id: ProcId, pid: u32, size: (u16, u16), exit: Option<ExitStatus>) -> Self {
        Self {
            id,
            pid,
            cols: size.0,
            rows: size.1,
            exit,
        }
    }

    /// The process id.
    pub fn id(&self) -> ProcId {
        self.id
    }

    /// OS pid.
    pub fn pid(&self) -> u32 {
        self.pid
    }

    /// PTY size `(cols, rows)`.
    pub fn size(&self) -> (u16, u16) {
        (self.cols, self.rows)
    }

    /// Exit status once it ended.
    pub fn exit(&self) -> Option<ExitStatus> {
        self.exit
    }
}

/// One entry of a `Journals` reply: what `latticed` needs to rebuild a runner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JournalInfo {
    journal: JournalId,
    meta: Vec<u8>,
    start_off: Offset,
    end_off: Offset,
    procs: Vec<ProcInfo>,
}

impl JournalInfo {
    /// A journal record as `latticed-proc` lists it.
    pub fn new(
        journal: JournalId,
        meta: Vec<u8>,
        range: (Offset, Offset),
        procs: Vec<ProcInfo>,
    ) -> Self {
        Self {
            journal,
            meta,
            start_off: range.0,
            end_off: range.1,
            procs,
        }
    }

    /// The journal id (the instance id).
    pub fn journal(&self) -> &JournalId {
        &self.journal
    }

    /// The opaque meta set at `Open`.
    pub fn meta(&self) -> &[u8] {
        &self.meta
    }

    /// Kept range: `[start_off, end_off)`.
    pub fn range(&self) -> (Offset, Offset) {
        (self.start_off, self.end_off)
    }

    /// Its processes.
    pub fn procs(&self) -> &[ProcInfo] {
        &self.procs
    }
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
    /// Create a journal for a new app instance.
    Open(OpenSpec),
    /// Start a child on a new PTY; its output goes into the named journal.
    Spawn(SpawnSpec),
    /// List journals with their processes.
    List,
    /// Stream journal entries from `from_off`.
    Subscribe {
        /// Target journal.
        journal: JournalId,
        /// First offset wanted.
        from_off: Offset,
    },
    /// Stop streaming.
    Unsubscribe {
        /// Target journal.
        journal: JournalId,
    },
    /// Drop every entry before `upto` (a clean point).
    Trim {
        /// Target journal.
        journal: JournalId,
        /// First offset kept.
        upto: Offset,
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
    /// Delete a journal. Every process of it must have exited.
    Drop {
        /// Target journal.
        journal: JournalId,
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
    /// Answer to `Open`.
    Opened {
        /// The new journal.
        journal: JournalId,
    },
    /// Answer to `Spawn`.
    Spawned {
        /// New process.
        id: ProcId,
        /// OS pid.
        pid: u32,
    },
    /// Answer to `List`.
    Journals(Vec<JournalInfo>),
    /// One journal entry at `off`.
    Entry {
        /// Source journal.
        journal: JournalId,
        /// Offset of the entry.
        off: Offset,
        /// The entry.
        entry: EntryKind,
    },
    /// The subscriber lagged; entries before `new_start` were dropped.
    Gap {
        /// Source journal.
        journal: JournalId,
        /// Oldest offset still kept.
        new_start: Offset,
    },
    /// Answer to `Trim`.
    Trimmed {
        /// Target journal.
        journal: JournalId,
        /// Oldest offset now kept.
        start_off: Offset,
    },
    /// Answer to `Drop`.
    Dropped {
        /// The deleted journal.
        journal: JournalId,
    },
    /// Request failed.
    Error {
        /// Stable error code.
        code: ErrorCode,
        /// Human text. Never secrets.
        msg: String,
    },
}

mod codec;
mod decode;

pub(crate) use codec::tag;
pub use codec::{encode_reply, encode_request};
pub use decode::{decode_reply, decode_request};
