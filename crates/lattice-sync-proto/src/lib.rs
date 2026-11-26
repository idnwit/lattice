//! Sync wire protocol between nodes: messages and their framing.
//!
//! Owner spec: `docs/architecture/02-sync.md` (Wire messages).
//! Messages ride the control stream and per-stream channels of one QUIC
//! connection (`01-transport.md`, Channel layout). Payload encoding is
//! postcard (D19). Each frame is a big-endian `u32` length prefix + postcard
//! body of at most [`MAX_FRAME_LEN`] bytes.
//!
//! This crate owns the wire shape only. Folding changes into state,
//! compaction, and stream kinds' `apply` belong to `lattice-sync` (L2 core,
//! not yet created).
//!
//! # Usage
//!
//! ```no_run
//! use lattice_sync_proto::{Decoded, FrameDecoder, Message, encode};
//!
//! let mut wire = Vec::new();
//! encode(&Message::Unsubscribe { stream: "inst/1/state".parse().unwrap() }, &mut wire).unwrap();
//! let mut dec = FrameDecoder::new();
//! dec.push(&wire);
//! if let Ok(Some(Decoded::Message(_msg))) = dec.next() {}
//! ```
//!
//! # Key items
//!
//! - [`Message`]: every wire message in 02-sync.
//! - [`StreamId`], [`StreamKind`], [`Seq`], [`Have`]: stream addressing and cursors.
//! - [`encode`], [`FrameDecoder`], [`Decoded`]: framing; unknown types decode
//!   to [`Decoded::Unsupported`] so the connection stays open (02, rule 5).

#![deny(clippy::indexing_slicing, clippy::arithmetic_side_effects)]
#![deny(clippy::cast_possible_truncation)]

mod frame;
mod message;
mod stream;

pub use frame::{Decoded, FrameDecoder, FrameError, MAX_FRAME_LEN, encode};
pub use message::{ActionId, Change, Message, SubscribeMode};
pub use stream::{Have, MAX_STREAM_ID_LEN, Seq, StreamId, StreamIdError, StreamInfo, StreamKind};

/// Sync protocol version sent in `Hello`.
pub const PROTOCOL_VERSION: u16 = 1;
