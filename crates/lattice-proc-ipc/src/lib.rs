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

use std::ffi::OsString;
use std::path::PathBuf;

/// Protocol version this crate speaks in `Hello`.
pub const PROTOCOL_VERSION: u16 = 1;

/// Env var that overrides the socket path (tests, custom setups).
pub const SOCKET_ENV: &str = "LATTICE_PROC_SOCK";

/// Name of the socket file in the runtime dir.
const SOCKET_FILE: &str = "proc.sock";

/// The per-user runtime dir both daemons share (specs/01-pairing.md, Files). It holds
/// `proc.sock` and `worker.addr`.
///
/// `$XDG_RUNTIME_DIR/lattice` when set, else `$TMPDIR/lattice-<uid>`, else
/// `/tmp/lattice-<uid>`. `uid` is the caller's real user id: one dir per user, even where
/// `$TMPDIR` is shared.
pub fn runtime_dir(uid: u32) -> PathBuf {
    let var = |name| std::env::var_os(name);
    runtime_dir_in(var("XDG_RUNTIME_DIR"), var("TMPDIR"), uid)
}

/// [`runtime_dir`] from the two env values.
fn runtime_dir_in(xdg: Option<OsString>, tmp: Option<OsString>, uid: u32) -> PathBuf {
    let set = |v: Option<OsString>| v.filter(|v| !v.is_empty()).map(PathBuf::from);
    if let Some(xdg) = set(xdg) {
        return xdg.join("lattice");
    }
    set(tmp)
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(format!("lattice-{uid}"))
}

/// The proc socket path both daemons agree on (04-worker.md, Local IPC):
/// `$LATTICE_PROC_SOCK` if set, else `proc.sock` in [`runtime_dir`].
pub fn default_socket_path(uid: u32) -> PathBuf {
    if let Some(path) = std::env::var_os(SOCKET_ENV).filter(|p| !p.is_empty()) {
        return PathBuf::from(path);
    }
    runtime_dir(uid).join(SOCKET_FILE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_runtime_dir_is_per_user() {
        let os = |s: &str| Some(OsString::from(s));
        let cases = [
            (
                "xdg wins",
                os("/run/user/1000"),
                os("/tmp/x"),
                "/run/user/1000/lattice",
            ),
            (
                "tmpdir",
                None,
                os("/var/folders/ab/T/"),
                "/var/folders/ab/T/lattice-1000",
            ),
            ("empty xdg", os(""), os("/tmp/x"), "/tmp/x/lattice-1000"),
            ("nothing", None, None, "/tmp/lattice-1000"),
            ("empty tmpdir", None, os(""), "/tmp/lattice-1000"),
        ];
        for (name, xdg, tmp, want) in cases {
            assert_eq!(
                runtime_dir_in(xdg, tmp, 1000),
                PathBuf::from(want),
                "case={name}"
            );
        }
    }
}
