//! The relay protocol: one UDP socket, first byte demultiplexes.
//!
//! Owner spec: `docs/architecture/01-transport.md` (Relay wire format) and
//! `docs/architecture/08-infrastructure.md` (Relay).
//! `0x00–0x3F` is a relay message. QUIC packets always set `0x40` or `0x80`
//! (RFC 7983). The relay forwards opaque QUIC packets. It never decrypts and
//! never decides who may connect.
//!
//! Everything here is sans-IO. `lattice-relay` drives [`RelayEngine`] from a
//! UDP loop. `lattice-quic` drives [`RelayClient`] inside the magic socket.
//!
//! # Usage
//!
//! ```no_run
//! use lattice_relay_proto::{Datagram, classify, decode};
//!
//! fn on_datagram(bytes: &[u8]) {
//!     match classify(bytes) {
//!         Datagram::Relay => { let _msg = decode(bytes); }
//!         Datagram::Quic => { /* hand to quinn */ }
//!         Datagram::Unknown => { /* drop silently (rule 5) */ }
//!     }
//! }
//! ```
//!
//! # Key items
//!
//! - [`RelayMsg`], [`encode`], [`decode`], [`classify`]: the wire format.
//! - [`RelayEngine`]: server side; owns the token table (`token → (node_id, src addr, expiry)`)
//!   and turns one datagram into what to send.
//! - [`RelayClient`]: client side; register, keepalive, wrap `SEND`, unwrap `RECV`.

#![deny(clippy::indexing_slicing, clippy::arithmetic_side_effects)]
#![deny(clippy::cast_possible_truncation)]

mod client;
mod codec;
mod engine;
mod table;

pub use client::{ClientEvent, KEEPALIVE_INTERVAL, RelayClient};
pub use codec::{
    CodecError, Datagram, MsgKind, ObservedAddr, RelayMsg, SEND_HEADER_LEN, Token, classify,
    decode, encode,
};
pub use engine::{EngineOutput, RelayEngine, Transmit};
