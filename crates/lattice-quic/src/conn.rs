//! `Transport` and `Conn` over quinn.

use bytes::Bytes;
use lattice_key::{NodeId, NodeKey};
use lattice_transport::{
    Conn, HomeRelay, PathInfo, RecvStream, SendStream, Transport, TransportError,
};
use std::net::SocketAddr;

/// Everything needed to bind a node's transport.
#[derive(Debug)]
#[allow(dead_code, reason = "L1 skeleton: read once bind is implemented")]
pub struct QuicConfig {
    key: NodeKey,
    relay: HomeRelay,
    bind: SocketAddr,
}

impl QuicConfig {
    /// Node key, home relay, and local UDP bind address.
    pub fn new(key: NodeKey, relay: HomeRelay, bind: SocketAddr) -> Self {
        Self { key, relay, bind }
    }
}

/// A bound node: one UDP socket, one quinn endpoint, relay registration running.
#[derive(Debug)]
#[allow(dead_code, reason = "L1 skeleton: read once bind is implemented")]
pub struct QuicTransport {
    endpoint: quinn::Endpoint,
    local_id: NodeId,
}

impl QuicTransport {
    /// Binds the socket, starts relay registration, and builds the endpoint.
    ///
    /// # Errors
    ///
    /// [`TransportError::Io`] when the socket cannot bind.
    #[allow(
        clippy::todo,
        unused_variables,
        reason = "L1 skeleton: body lands with its contract test"
    )]
    pub async fn bind(cfg: QuicConfig) -> Result<Self, TransportError> {
        // bind tokio UdpSocket at cfg.bind; wrap in MagicSocket
        // rustls client + server configs with ed25519 raw public keys (RFC 7250) from cfg.key
        // cap max UDP payload so packet + SEND_HEADER_LEN fits the relay path (rule 7)
        // congestion: wrapper ControllerFactory that resets cwnd on PathEvent::Switched (rule 6)
        // Endpoint::new_with_abstract_socket(..); spawn relay register + keepalive task
        todo!()
    }
}

impl Transport for QuicTransport {
    type Conn = QuicConn;

    #[allow(
        clippy::todo,
        unused_variables,
        reason = "L1 skeleton: body lands with its contract test"
    )]
    async fn connect(&self, peer: NodeId) -> Result<QuicConn, TransportError> {
        // reuse a live connection to peer (D4), else endpoint.connect(virtual_addr(peer))
        // verify the peer's raw public key == peer, else IdentityMismatch
        // in parallel: PUNCH_REQ → probes → PathSelector (connect sequence 4–5)
        todo!()
    }

    #[allow(
        clippy::todo,
        reason = "L1 skeleton: body lands with its contract test"
    )]
    async fn accept(&self) -> Result<(NodeId, QuicConn), TransportError> {
        // endpoint.accept(); complete handshake; NodeId from the peer's raw public key
        // access control is the host's job (device list), not L1's
        todo!()
    }

    fn local_id(&self) -> NodeId {
        self.local_id
    }
}

/// One connection to a peer.
#[derive(Debug)]
#[allow(dead_code, reason = "L1 skeleton: read once Conn is implemented")]
pub struct QuicConn {
    inner: quinn::Connection,
    remote: NodeId,
}

impl Conn for QuicConn {
    type Send = QuicSend;
    type Recv = QuicRecv;

    #[allow(
        clippy::todo,
        reason = "L1 skeleton: body lands with its contract test"
    )]
    async fn open_bi(&self) -> Result<(QuicSend, QuicRecv), TransportError> {
        // inner.open_bi(); map errors to TransportError
        todo!()
    }

    #[allow(
        clippy::todo,
        reason = "L1 skeleton: body lands with its contract test"
    )]
    async fn accept_bi(&self) -> Result<(QuicSend, QuicRecv), TransportError> {
        // inner.accept_bi()
        todo!()
    }

    #[allow(
        clippy::todo,
        unused_variables,
        reason = "L1 skeleton: body lands with its contract test"
    )]
    fn send_datagram(&self, bytes: Bytes) -> Result<(), TransportError> {
        // inner.send_datagram(bytes); TooLarge → DatagramTooLarge
        todo!()
    }

    #[allow(
        clippy::todo,
        reason = "L1 skeleton: body lands with its contract test"
    )]
    async fn recv_datagram(&self) -> Result<Bytes, TransportError> {
        // inner.read_datagram()
        todo!()
    }

    #[allow(
        clippy::todo,
        reason = "L1 skeleton: body lands with its contract test"
    )]
    fn path(&self) -> PathInfo {
        // active PathKind from the socket's selector for remote + inner.rtt()
        todo!()
    }

    fn remote_id(&self) -> NodeId {
        self.remote
    }
}

/// Send half of a quinn bi stream.
#[derive(Debug)]
#[allow(dead_code, reason = "L1 skeleton: read once streams are implemented")]
pub struct QuicSend(quinn::SendStream);

impl SendStream for QuicSend {
    #[allow(
        clippy::todo,
        unused_variables,
        reason = "L1 skeleton: body lands with its contract test"
    )]
    async fn write_all(&mut self, bytes: &[u8]) -> Result<(), TransportError> {
        // self.0.write_all(bytes)
        todo!()
    }

    #[allow(
        clippy::todo,
        reason = "L1 skeleton: body lands with its contract test"
    )]
    fn finish(&mut self) -> Result<(), TransportError> {
        // self.0.finish()
        todo!()
    }
}

/// Receive half of a quinn bi stream.
#[derive(Debug)]
#[allow(dead_code, reason = "L1 skeleton: read once streams are implemented")]
pub struct QuicRecv(quinn::RecvStream);

impl RecvStream for QuicRecv {
    #[allow(
        clippy::todo,
        unused_variables,
        reason = "L1 skeleton: body lands with its contract test"
    )]
    async fn read(&mut self, buf: &mut [u8]) -> Result<Option<usize>, TransportError> {
        // self.0.read(buf)
        todo!()
    }
}
