//! L1 transport contract: how two nodes connect, as seen from above L1.
//!
//! Owner spec: `docs/architecture/01-transport.md`.
//! Nothing above L1 imports `quinn` types. Hosts (`lattice-worker`,
//! `lattice-deck-core`) use only the traits here. `lattice-quic` implements
//! them over the magic socket.
//!
//! This crate also owns the sans-IO path-selection FSM that decides relay vs
//! direct. It runs on an injected clock, so tests are deterministic
//! (`.rules/verification.md`).
//!
//! # Usage
//!
//! ```no_run
//! use lattice_transport::{Conn, Transport};
//!
//! async fn ping<T: Transport>(t: &T, peer: lattice_key::NodeId) -> Result<(), lattice_transport::TransportError> {
//!     let conn = t.connect(peer).await?;
//!     let _path = conn.path(); // "relay · 64ms" until the direct path validates
//!     Ok(())
//! }
//! ```
//!
//! # Key items
//!
//! - [`Transport`], [`Conn`], [`SendStream`], [`RecvStream`]: the L1 boundary.
//! - [`PathInfo`], [`PathKind`]: what the status bar shows.
//! - [`PathSelector`]: relay-first, then direct, with 2 s silence fallback.
//! - [`HomeRelay`]: where a node registers.

#![deny(clippy::indexing_slicing, clippy::arithmetic_side_effects)]
#![deny(clippy::cast_possible_truncation)]

mod conn;
mod path;

pub use conn::{Conn, HomeRelay, RecvStream, SendStream, Transport, TransportError};
pub use path::{
    DIRECT_SILENCE_FALLBACK, DUPLICATE_WINDOW, PathEvent, PathInfo, PathKind, PathSelector,
};
