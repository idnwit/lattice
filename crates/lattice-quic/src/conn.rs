//! `Transport` and `Conn` over quinn.

use crate::config::{Mode, QuicConfig};
use crate::tls::{self, Identity};
use bytes::Bytes;
use lattice_key::NodeId;
use lattice_transport::{
    Conn, HomeRelay, PathInfo, PathKind, RecvStream, SendStream, Transport, TransportError,
};
use quinn::rustls::pki_types::CertificateDer;
use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

/// A bound node: one UDP socket and one quinn endpoint.
///
/// Relay mode also runs relay registration (skeleton). Direct mode dials
/// addresses from [`QuicTransport::add_peer`].
#[derive(Debug)]
pub struct QuicTransport {
    endpoint: quinn::Endpoint,
    local_id: NodeId,
    identity: Identity,
    /// Address book for direct mode.
    peers: Mutex<BTreeMap<NodeId, SocketAddr>>,
    /// Live connections, reused by `connect` (D4: one per peer pair).
    conns: Mutex<BTreeMap<NodeId, quinn::Connection>>,
}

impl QuicTransport {
    /// Binds the socket and builds the endpoint.
    ///
    /// # Errors
    ///
    /// [`TransportError::Io`] when the socket cannot bind.
    pub async fn bind(cfg: QuicConfig) -> Result<Self, TransportError> {
        match cfg.mode {
            Mode::Direct => Self::bind_direct(cfg),
            Mode::Relay(relay) => Self::bind_relay(cfg, relay).await,
        }
    }

    /// Plain UDP socket at `cfg.bind`, quinn's own socket driver.
    fn bind_direct(cfg: QuicConfig) -> Result<Self, TransportError> {
        let local_id = cfg.key.node_id();
        let identity = Identity::new(cfg.key);
        let endpoint = quinn::Endpoint::server(identity.server_config()?, cfg.bind)
            .map_err(|e| TransportError::Io(e.to_string()))?;
        tracing::info!(addr = ?endpoint.local_addr().ok(), "quic direct-only endpoint bound");
        Ok(Self {
            endpoint,
            local_id,
            identity,
            peers: Mutex::new(BTreeMap::new()),
            conns: Mutex::new(BTreeMap::new()),
        })
    }

    #[allow(
        clippy::todo,
        unused_variables,
        clippy::unused_async,
        reason = "L1 skeleton: relay mode lands with its contract test"
    )]
    async fn bind_relay(cfg: QuicConfig, relay: HomeRelay) -> Result<Self, TransportError> {
        // bind tokio UdpSocket at cfg.bind; wrap in MagicSocket
        // tls::Identity as in direct mode (raw public keys, RFC 7250)
        // cap max UDP payload so packet + SEND_HEADER_LEN fits the relay path (rule 7)
        // congestion: wrapper ControllerFactory that resets cwnd on PathEvent::Switched (rule 6)
        // Endpoint::new_with_abstract_socket(..); spawn relay register + keepalive task
        // connect: virtual_addr(peer); PUNCH_REQ → probes → PathSelector (connect sequence 4–5)
        todo!()
    }

    /// Records where `peer` listens. Direct mode dials only known peers.
    pub fn add_peer(&self, peer: NodeId, addr: SocketAddr) {
        lock(&self.peers).insert(peer, addr);
    }

    /// The bound UDP address (resolves a `:0` bind).
    ///
    /// # Errors
    ///
    /// [`TransportError::Io`] when the socket is gone.
    pub fn local_addr(&self) -> Result<SocketAddr, TransportError> {
        self.endpoint
            .local_addr()
            .map_err(|e| TransportError::Io(e.to_string()))
    }

    /// A still-open connection to `peer`, if any. Drops closed ones.
    fn live_conn(&self, peer: NodeId) -> Option<quinn::Connection> {
        let mut conns = lock(&self.conns);
        match conns.get(&peer) {
            Some(c) if c.close_reason().is_none() => Some(c.clone()),
            Some(_) => {
                conns.remove(&peer);
                None
            }
            None => None,
        }
    }

    fn remember(&self, peer: NodeId, conn: &quinn::Connection) {
        lock(&self.conns).insert(peer, conn.clone());
    }

    fn peer_addr(&self, peer: NodeId) -> Option<SocketAddr> {
        lock(&self.peers).get(&peer).copied()
    }
}

/// Locks `m`. Poisoning is ignored: every critical section is one map call.
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Transport for QuicTransport {
    type Conn = QuicConn;

    async fn connect(&self, peer: NodeId) -> Result<QuicConn, TransportError> {
        if let Some(inner) = self.live_conn(peer) {
            return Ok(QuicConn::new(inner, peer));
        }
        let addr = self.peer_addr(peer).ok_or(TransportError::Unreachable)?;
        let mismatch = Arc::new(AtomicBool::new(false));
        let cfg = self.identity.client_config(peer, mismatch.clone())?;
        let connecting = self
            .endpoint
            .connect_with(cfg, addr, tls::SERVER_NAME)
            .map_err(|e| TransportError::Io(e.to_string()))?;
        let inner = match connecting.await {
            Ok(inner) => inner,
            Err(_) if mismatch.load(Ordering::SeqCst) => {
                tracing::warn!(?peer, "dialed peer presented another key");
                return Err(TransportError::IdentityMismatch);
            }
            Err(e) => return Err(conn_err(e)),
        };
        // Two racing connects may both dial; the last one is kept for reuse.
        self.remember(peer, &inner);
        tracing::debug!(?peer, "quic connected");
        Ok(QuicConn::new(inner, peer))
    }

    async fn accept(&self) -> Result<(NodeId, QuicConn), TransportError> {
        loop {
            let incoming = self.endpoint.accept().await.ok_or(TransportError::Closed)?;
            let inner = match incoming.await {
                Ok(inner) => inner,
                Err(e) => {
                    // One failed handshake must not stop the accept loop.
                    tracing::debug!(code = conn_err(e).code(), "inbound handshake failed");
                    continue;
                }
            };
            let Some(peer) = peer_node_id(&inner) else {
                // Unreachable with our verifier; refuse rather than guess.
                inner.close(quinn::VarInt::from_u32(0), b"identity");
                continue;
            };
            // Access control is the host's job (device list), not L1's.
            self.remember(peer, &inner);
            tracing::debug!(?peer, "quic accepted");
            return Ok((peer, QuicConn::new(inner, peer)));
        }
    }

    fn local_id(&self) -> NodeId {
        self.local_id
    }
}

/// The verified client key of an accepted connection.
fn peer_node_id(conn: &quinn::Connection) -> Option<NodeId> {
    let certs = conn
        .peer_identity()?
        .downcast::<Vec<CertificateDer<'static>>>()
        .ok()?;
    tls::node_id_from_spki(certs.first()?)
}

fn conn_err(e: quinn::ConnectionError) -> TransportError {
    use quinn::ConnectionError as E;
    match e {
        E::TimedOut => TransportError::Unreachable,
        E::LocallyClosed | E::ApplicationClosed(_) | E::ConnectionClosed(_) | E::Reset => {
            TransportError::Closed
        }
        E::VersionMismatch | E::TransportError(_) | E::CidsExhausted => {
            TransportError::Io(e.to_string())
        }
    }
}

/// One connection to a peer.
#[derive(Debug, Clone)]
pub struct QuicConn {
    inner: quinn::Connection,
    remote: NodeId,
}

impl QuicConn {
    fn new(inner: quinn::Connection, remote: NodeId) -> Self {
        Self { inner, remote }
    }
}

impl Conn for QuicConn {
    type Send = QuicSend;
    type Recv = QuicRecv;

    async fn open_bi(&self) -> Result<(QuicSend, QuicRecv), TransportError> {
        let (tx, rx) = self.inner.open_bi().await.map_err(conn_err)?;
        Ok((QuicSend(tx), QuicRecv(rx)))
    }

    async fn accept_bi(&self) -> Result<(QuicSend, QuicRecv), TransportError> {
        let (tx, rx) = self.inner.accept_bi().await.map_err(conn_err)?;
        Ok((QuicSend(tx), QuicRecv(rx)))
    }

    fn send_datagram(&self, bytes: Bytes) -> Result<(), TransportError> {
        use quinn::SendDatagramError as E;
        self.inner.send_datagram(bytes).map_err(|e| match e {
            E::TooLarge => TransportError::DatagramTooLarge,
            E::ConnectionLost(e) => conn_err(e),
            E::UnsupportedByPeer | E::Disabled => TransportError::Io(e.to_string()),
        })
    }

    async fn recv_datagram(&self) -> Result<Bytes, TransportError> {
        self.inner.read_datagram().await.map_err(conn_err)
    }

    fn path(&self) -> PathInfo {
        // Direct mode only; relay mode reads the socket's PathSelector (TBD).
        PathInfo::new(PathKind::Direct, self.inner.rtt())
    }

    fn remote_id(&self) -> NodeId {
        self.remote
    }
}

/// Send half of a quinn bi stream.
#[derive(Debug)]
pub struct QuicSend(quinn::SendStream);

impl SendStream for QuicSend {
    async fn write_all(&mut self, bytes: &[u8]) -> Result<(), TransportError> {
        self.0.write_all(bytes).await.map_err(|e| match e {
            quinn::WriteError::ConnectionLost(e) => conn_err(e),
            _ => TransportError::Closed,
        })
    }

    fn finish(&mut self) -> Result<(), TransportError> {
        self.0.finish().map_err(|_| TransportError::Closed)
    }
}

/// Receive half of a quinn bi stream.
#[derive(Debug)]
pub struct QuicRecv(quinn::RecvStream);

impl RecvStream for QuicRecv {
    async fn read(&mut self, buf: &mut [u8]) -> Result<Option<usize>, TransportError> {
        self.0.read(buf).await.map_err(|e| match e {
            quinn::ReadError::ConnectionLost(e) => conn_err(e),
            _ => TransportError::Closed,
        })
    }
}
