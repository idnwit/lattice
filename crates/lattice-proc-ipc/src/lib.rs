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
//! - [`ProcId`], [`Offset`]: ids and ring-buffer offsets shared by both daemons.
//! - [`Frame`], [`FrameError`]: one frame and the errors that end a connection.

#![deny(clippy::indexing_slicing, clippy::arithmetic_side_effects)]
#![deny(clippy::cast_possible_truncation)]

mod frame;
mod message;

pub use frame::{Frame, FrameDecoder, FrameError};
pub use message::{
    ErrorCode, ExitStatus, Offset, ProcId, ProcInfo, Reply, Request, SpawnSpec, decode_reply,
    decode_request, encode_reply, encode_request,
};

/// Protocol version this crate speaks in `Hello`.
pub const PROTOCOL_VERSION: u16 = 1;
