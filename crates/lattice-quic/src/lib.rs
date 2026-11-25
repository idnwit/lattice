//! QUIC over our own UDP paths: the L1 adapter.
//!
//! Owner spec: `docs/architecture/01-transport.md` (Paths, Connect sequence).
//! Upstream `quinn` runs over `MagicSocket`, our `AsyncUdpSocket`. QUIC sees one
//! stable virtual address per peer NodeId; the socket picks relay or direct
//! under it with `lattice_transport::PathSelector`.
//!
//! This crate is the only place that names `quinn` types (01: nothing above L1
//! imports them). Hosts use [`QuicTransport`] through `lattice_transport::Transport`.
//!
//! # Usage
//!
//! ```no_run
//! # async fn run(key: lattice_key::NodeKey, relay: lattice_transport::HomeRelay) -> Result<(), lattice_transport::TransportError> {
//! use lattice_quic::{QuicConfig, QuicTransport};
//!
//! let cfg = QuicConfig::new(key, relay, "0.0.0.0:0".parse().unwrap());
//! let transport = QuicTransport::bind(cfg).await?;
//! # Ok(()) }
//! ```
//!
//! # Key items
//!
//! - [`QuicTransport`], [`QuicConn`]: `Transport` / `Conn` implementations.
//! - [`QuicConfig`]: node key, home relay, bind address.
//! - `MagicSocket` (internal): relay + direct + probes on one UDP socket.

mod conn;
mod socket;

pub use conn::{QuicConfig, QuicConn, QuicRecv, QuicSend, QuicTransport};
