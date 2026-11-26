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
//! Two modes:
//!
//! - **Relay** ([`QuicConfig::new`]): relay first, then direct. Skeleton.
//! - **Direct-only** ([`QuicConfig::direct`], ROADMAP M1.5): plain UDP to
//!   addresses from [`QuicTransport::add_peer`]. Binds loopback only.
//!   [`QuicConfig::direct_lan`] binds any address, for hosts that check every
//!   peer themselves (the worker's allowlist, M1.6).
//!
//! TLS 1.3 is bound to the node key: each side presents its ed25519 NodeId as
//! an RFC 7250 raw public key. The dialer pins the NodeId it dials; the
//! acceptor requires a client key and reports it as the peer NodeId. ALPN
//! `lattice/1`.
//!
//! # Usage
//!
//! ```no_run
//! # async fn run(worker: lattice_key::NodeKey, deck: lattice_key::NodeKey) -> Result<(), Box<dyn std::error::Error>> {
//! use lattice_quic::{QuicConfig, QuicTransport};
//! use lattice_transport::Transport;
//!
//! let bind = "127.0.0.1:0".parse()?;
//! let worker = QuicTransport::bind(QuicConfig::direct(worker, bind)?).await?;
//! let deck = QuicTransport::bind(QuicConfig::direct(deck, bind)?).await?;
//! deck.add_peer(worker.local_id(), worker.local_addr()?);
//! let conn = deck.connect(worker.local_id()).await?;
//! # Ok(()) }
//! ```
//!
//! # Key items
//!
//! - [`QuicTransport`], [`QuicConn`]: `Transport` / `Conn` implementations.
//! - [`QuicConfig`]: node key, mode (relay or direct-only), bind address.
//! - [`ConfigError`]: refused configs (non-loopback direct bind).
//! - [`advertise_ip`]: the local IP a peer reaches us at.
//! - `MagicSocket` (internal, skeleton): relay + direct + probes on one UDP socket.

mod advertise;
mod config;
mod conn;
mod socket;
mod tls;

pub use advertise::{ADVERTISE_ENV, advertise_ip};
pub use config::{ConfigError, QuicConfig};
pub use conn::{QuicConn, QuicRecv, QuicSend, QuicTransport};
