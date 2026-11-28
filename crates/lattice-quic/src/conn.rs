//! `Transport` and `Conn` over quinn.

use crate::config::{Mode, QuicConfig};
use crate::socket::MagicSocket;
use crate::tls::{self, Identity, Mtu};
use bytes::Bytes;
use lattice_key::NodeId;
use lattice_transport::{
    CloseReason, Conn, HomeRelay, PathInfo, PathKind, RecvStream, SendStream, Transport,
    TransportError,
};
use quinn::rustls::pki_types::CertificateDer;
use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

/// Application close code for [`CloseReason::Refused`] (01-transport.md, Transport trait).
/// Code 0 stays the generic close.
const CLOSE_REFUSED: quinn::VarInt = quinn::VarInt::from_u32(1);

/// The generic close: what [`Transport::shutdown`] sends.
const CLOSE_GENERIC: quinn::VarInt = quinn::VarInt::from_u32(0);

/// Longest wait of [`Transport::shutdown`] for the peers to be told (01-transport.md,
/// Transport trait rule 3; proposed).
const SHUTDOWN_WAIT: std::time::Duration = std::time::Duration::from_secs(1);

/// Relay mode: how long `connect` waits for the relay registration before it fails
/// with `transport.unreachable` (01-transport.md, Connect sequence rule 3).
const REGISTER_WAIT: std::time::Duration = std::time::Duration::from_secs(5);

/// Moves a relay-mode transport to another home relay (specs/10-settings.md, rule 35).
///
/// Holds no strong reference: the transport stays owned by its host.
#[derive(Debug, Clone)]
pub struct HomeRelayHandle(std::sync::Weak<MagicSocket>);

impl HomeRelayHandle {
    /// Registers at `relay` from now on, on the same UDP socket. Connections through the
    /// old relay stop carrying packets. Returns `false` when the transport is gone.
    pub fn set(&self, relay: HomeRelay) -> bool {
        let Some(socket) = self.0.upgrade() else {
            return false;
        };
        tracing::info!(relay = %relay.addr(), "home relay changed: registering there");
        socket.set_home_relay(relay);
        true
    }
}

/// A bound node: one UDP socket and one quinn endpoint.
///
/// Relay mode dials every peer at its virtual address through the magic socket.
/// Direct mode dials addresses from [`QuicTransport::add_peer`].
#[derive(Debug)]
pub struct QuicTransport {
    endpoint: quinn::Endpoint,
    local_id: NodeId,
    identity: Identity,
    /// Address book for direct mode.
    peers: Mutex<BTreeMap<NodeId, SocketAddr>>,
    /// Live connections, reused by `connect` (D4: one per peer pair).
    conns: Mutex<BTreeMap<NodeId, quinn::Connection>>,
    /// Relay mode only.
    socket: Option<Arc<MagicSocket>>,
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
        let identity = Identity::new(Arc::new(cfg.key), Mtu::Discover);
        let endpoint = quinn::Endpoint::server(identity.server_config()?, cfg.bind)
            .map_err(|e| TransportError::Io(e.to_string()))?;
        tracing::info!(addr = ?endpoint.local_addr().ok(), "quic direct-only endpoint bound");
        Ok(Self::with(endpoint, local_id, identity, None))
    }

    fn with(
        endpoint: quinn::Endpoint,
        local_id: NodeId,
        identity: Identity,
        socket: Option<Arc<MagicSocket>>,
    ) -> Self {
        Self {
            endpoint,
            local_id,
            identity,
            peers: Mutex::new(BTreeMap::new()),
            conns: Mutex::new(BTreeMap::new()),
            socket,
        }
    }

    /// Magic socket at `cfg.bind`, registered with `relay` (01-transport.md, Paths).
    ///
    /// Relay path only; direct paths and their cwnd reset land with hole punching (M2.5).
    async fn bind_relay(cfg: QuicConfig, relay: HomeRelay) -> Result<Self, TransportError> {
        let io = |e: std::io::Error| TransportError::Io(e.to_string());
        let udp = tokio::net::UdpSocket::bind(cfg.bind).await.map_err(io)?;
        let socket = Arc::new(MagicSocket::new(udp, relay));
        let key = Arc::new(cfg.key);
        let local_id = key.node_id();
        let identity = Identity::new(key.clone(), Mtu::RelaySafe);
        let endpoint = quinn::Endpoint::new_with_abstract_socket(
            quinn::EndpointConfig::default(),
            Some(identity.server_config()?),
            socket.clone(),
            Arc::new(quinn::TokioRuntime),
        )
        .map_err(io)?;
        socket.spawn_registrar(key);
        tracing::info!(addr = ?endpoint.local_addr().ok(), relay = %relay.addr(), "quic relay endpoint bound");
        Ok(Self::with(endpoint, local_id, identity, Some(socket)))
    }

    /// [`Transport::shutdown`], callable without the trait (binaries that only bind).
    ///
    /// Closes every connection with the generic close, then waits for the peers to be told,
    /// at most `SHUTDOWN_WAIT` (01-transport.md, Transport trait rules 3–4).
    pub async fn shutdown(&self) {
        // Connections closed before (refused) keep their code: quinn sends each close once.
        self.endpoint.close(CLOSE_GENERIC, b"shutdown");
        if tokio::time::timeout(SHUTDOWN_WAIT, self.endpoint.wait_idle())
            .await
            .is_err()
        {
            tracing::debug!("shutdown: peers not told within the wait");
        }
    }

    /// A handle that moves the relay registration to another home relay while this
    /// transport serves. `None` in direct-only mode, which has no relay to move.
    pub fn home_relay_handle(&self) -> Option<HomeRelayHandle> {
        let socket = self.socket.as_ref()?;
        Some(HomeRelayHandle(Arc::downgrade(socket)))
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

    /// Keeps `conn` for reuse and forgets closed ones, so refused peers do not pile up.
    fn remember(&self, peer: NodeId, conn: &quinn::Connection) {
        let mut conns = lock(&self.conns);
        conns.retain(|_, c| c.close_reason().is_none());
        conns.insert(peer, conn.clone());
    }

    /// Where quinn dials `peer`. Relay mode waits for the registration first.
    async fn dial_addr(&self, peer: NodeId) -> Result<SocketAddr, TransportError> {
        let Some(socket) = &self.socket else {
            return lock(&self.peers)
                .get(&peer)
                .copied()
                .ok_or(TransportError::Unreachable);
        };
        if !socket.wait_registered(REGISTER_WAIT).await {
            tracing::debug!(?peer, "relay not registered: connect fails");
            return Err(TransportError::Unreachable);
        }
        socket.virtual_addr(peer).ok_or(TransportError::Unreachable)
    }

    fn conn(&self, inner: quinn::Connection, peer: NodeId) -> QuicConn {
        QuicConn {
            inner,
            remote: peer,
            socket: self.socket.clone(),
        }
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
            return Ok(self.conn(inner, peer));
        }
        let addr = self.dial_addr(peer).await?;
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
        Ok(self.conn(inner, peer))
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
            return Ok((peer, self.conn(inner, peer)));
        }
    }

    fn local_id(&self) -> NodeId {
        self.local_id
    }

    async fn shutdown(&self) {
        QuicTransport::shutdown(self).await;
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
        E::ApplicationClosed(ref close) if close.error_code == CLOSE_REFUSED => {
            TransportError::Refused
        }
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
    /// Relay mode: the socket that knows the active path.
    socket: Option<Arc<MagicSocket>>,
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
        let kind = self
            .socket
            .as_ref()
            .map_or(PathKind::Direct, |s| s.path_of(self.remote));
        PathInfo::new(kind, self.inner.rtt())
    }

    fn remote_id(&self) -> NodeId {
        self.remote
    }

    fn close(&self, reason: CloseReason) {
        match reason {
            CloseReason::Refused => self.inner.close(CLOSE_REFUSED, b"refused"),
        }
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
