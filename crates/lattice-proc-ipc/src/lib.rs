//! Local IPC between `latticed` and the frozen process daemon `latticed-proc`.
//!
//! Owner spec: `docs/architecture/04-worker.md` (Local IPC).
//! This crate is the v1 wire contract. It is frozen: v1 semantics never change,
//! later versions only add optional tags. It is sans-IO and depends on std only.
//!
//! Frame: `u32 len | u16 type | u16 flags | TLV payload (u16 tag, u32 len, bytes)`.
//! Compatibility: `Hello{version, features}` first. Unknown tags are ignored.
//! Unknown types get `Error{unsupported}`. Golden v1 fixtures guard the format
//! (`.rules/verification.md`).
//!
//! # Usage
//!
//! ```no_run
//! use lattice_proc_ipc::{FrameDecoder, Request, encode_request};
//!
//! let mut wire = Vec::new();
//! encode_request(&Request::List, &mut wire);
//! let mut dec = FrameDecoder::new();
//! dec.push(&wire);
//! let frame = dec.next_frame();
//! ```
//!
//! # Key items
//!
//! - [`FrameDecoder`]: incremental frame splitter for a byte stream.
//! - [`Request`] / [`Reply`]: v1 messages; [`encode_request`], [`decode_request`],
//!   [`encode_reply`], [`decode_reply`].
//! - [`JournalId`], [`EntryKind`]: one journal per app instance; entries are process output and exits.
//! - [`ProcId`], [`Offset`]: process ids and journal offsets shared by both daemons.
//! - [`Frame`], [`FrameError`]: one frame and the errors that end a connection.

#![deny(clippy::indexing_slicing, clippy::arithmetic_side_effects)]
#![deny(clippy::cast_possible_truncation)]

mod frame;
mod ids;
mod message;

pub use frame::{Frame, FrameDecoder, FrameError, MAX_FRAME_LEN};
pub use ids::{EntryKind, ErrorCode, ExitStatus, JournalId, Offset, ProcId};
pub use message::{
    DEFAULT_JOURNAL_BUDGET, JournalInfo, MAX_META_LEN, MIN_JOURNAL_BUDGET, OpenSpec, ProcInfo,
    Reply, Request, SpawnSpec, decode_reply, decode_request, encode_reply, encode_request,
};

use std::path::PathBuf;

/// Protocol version this crate speaks in `Hello`.
pub const PROTOCOL_VERSION: u16 = 1;

/// Env var that overrides the socket path (tests, custom setups).
pub const SOCKET_ENV: &str = "LATTICE_PROC_SOCK";

/// The proc socket path both daemons agree on (04-worker.md, Local IPC).
///
/// `$LATTICE_PROC_SOCK` if set, else `$XDG_RUNTIME_DIR/lattice/proc.sock`,
/// else `$TMPDIR/lattice/proc.sock` (macOS), else `/tmp/lattice/proc.sock`.
pub fn default_socket_path() -> PathBuf {
    if let Some(path) = std::env::var_os(SOCKET_ENV).filter(|p| !p.is_empty()) {
        return PathBuf::from(path);
    }
    let base = ["XDG_RUNTIME_DIR", "TMPDIR"]
        .iter()
        .find_map(|var| std::env::var_os(var).filter(|p| !p.is_empty()))
        .map_or_else(|| PathBuf::from("/tmp"), PathBuf::from);
    base.join("lattice").join("proc.sock")
}
