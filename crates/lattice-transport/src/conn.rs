//! The `Transport` / `Conn` traits (01-transport.md, Transport trait).

use crate::path::PathInfo;
use bytes::Bytes;
use lattice_key::NodeId;
use std::future::Future;
use std::net::SocketAddr;

/// Error from any transport operation. Never holds peer bytes.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TransportError {
    /// The peer is unreachable on every path.
    #[error("peer unreachable")]
    Unreachable,
    /// The peer's key did not match the dialed NodeId.
    #[error("peer identity mismatch")]
    IdentityMismatch,
    /// The connection or stream closed.
    #[error("closed")]
    Closed,
    /// The peer closed the connection because it does not allow this node
    /// ([`CloseReason::Refused`]).
    #[error("refused by peer")]
    Refused,
    /// A datagram exceeds the path limit.
    #[error("datagram too large")]
    DatagramTooLarge,
    /// Socket or endpoint failure.
    #[error("io: {0}")]
    Io(String),
}

impl TransportError {
    /// Stable code for logs and the UI.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Unreachable => "transport.unreachable",
            Self::IdentityMismatch => "transport.identity_mismatch",
            Self::Closed => "transport.closed",
            Self::Refused => "transport.refused",
            Self::DatagramTooLarge => "transport.datagram_too_large",
            Self::Io(_) => "transport.io",
        }
    }
}

/// Why a node closes a connection on purpose. The peer sees it as a [`TransportError`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseReason {
    /// The peer is not allowed here (specs/01-pairing.md, rule 12). The peer's streams fail
    /// with [`TransportError::Refused`].
    Refused,
}

/// A node's home relay: where it registers and where peers reach it first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HomeRelay {
    addr: SocketAddr,
    relay_id: NodeId,
}

impl HomeRelay {
    /// A relay at `addr` whose key is `relay_id` (signed into `REGISTER`).
    pub fn new(addr: SocketAddr, relay_id: NodeId) -> Self {
        Self { addr, relay_id }
    }

    /// UDP address of the relay.
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// Relay key; `REGISTER` signs `relay_id || nonce || ts`.
    pub fn relay_id(&self) -> NodeId {
        self.relay_id
    }
}

/// Dials and accepts authenticated connections by NodeId.
pub trait Transport: Send + Sync {
    /// Connection type.
    type Conn: Conn;

    /// Opens (or reuses) the one connection to `peer`. Relay first; direct later.
    fn connect(
        &self,
        peer: NodeId,
    ) -> impl Future<Output = Result<Self::Conn, TransportError>> + Send;

    /// Waits for the next inbound connection and its verified NodeId.
    fn accept(&self) -> impl Future<Output = Result<(NodeId, Self::Conn), TransportError>> + Send;

    /// This node's id.
    fn local_id(&self) -> NodeId;

    /// Closes every open connection with the generic close, then waits until each peer was
    /// told, within `SHUTDOWN_WAIT` (01-transport.md, Transport trait rules 3–4). A process
    /// calls it before it exits: packets still queued, a stream finish included, are lost
    /// otherwise, and the peer waits for its idle timeout.
    fn shutdown(&self) -> impl Future<Output = ()> + Send;
}

/// One QUIC connection to one peer (D4: one per peer pair).
pub trait Conn: Send + Sync {
    /// Send half of a bi stream.
    type Send: SendStream;
    /// Receive half of a bi stream.
    type Recv: RecvStream;

    /// Opens a bi stream (control, per-stream data, input, blobs).
    fn open_bi(
        &self,
    ) -> impl Future<Output = Result<(Self::Send, Self::Recv), TransportError>> + Send;

    /// Accepts a bi stream the peer opened.
    fn accept_bi(
        &self,
    ) -> impl Future<Output = Result<(Self::Send, Self::Recv), TransportError>> + Send;

    /// Sends an unreliable datagram (v2 fast state).
    ///
    /// # Errors
    ///
    /// [`TransportError::DatagramTooLarge`] above the path limit.
    fn send_datagram(&self, bytes: Bytes) -> Result<(), TransportError>;

    /// Receives the next datagram.
    fn recv_datagram(&self) -> impl Future<Output = Result<Bytes, TransportError>> + Send;

    /// Current path and RTT, for the UI (`direct · 18ms`).
    fn path(&self) -> PathInfo;

    /// Verified id of the peer.
    fn remote_id(&self) -> NodeId;

    /// Closes the connection now, for every handle to it, and tells the peer `reason`.
    /// Dropping a `Conn` does not close it: the transport may keep it for reuse (D4).
    fn close(&self, reason: CloseReason);
}

/// Write half of a reliable stream.
pub trait SendStream: Send {
    /// Writes all bytes, waiting on flow control.
    fn write_all(
        &mut self,
        bytes: &[u8],
    ) -> impl Future<Output = Result<(), TransportError>> + Send;

    /// Ends the stream cleanly.
    ///
    /// # Errors
    ///
    /// [`TransportError::Closed`] when already closed.
    fn finish(&mut self) -> Result<(), TransportError>;
}

/// Read half of a reliable stream.
pub trait RecvStream: Send {
    /// Reads into `buf`. `Ok(None)` at end of stream.
    fn read(
        &mut self,
        buf: &mut [u8],
    ) -> impl Future<Output = Result<Option<usize>, TransportError>> + Send;
}
