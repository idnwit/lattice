//! The magic socket: one UDP socket, many paths (01-transport.md, Paths).
//!
//! M2.3 carries the relay path only. Direct paths, probes and duplicate sending
//! land with hole punching (ROADMAP M2.5).

mod vaddr;

use crate::tls;
use lattice_key::{NodeId, NodeKey};
use lattice_relay_proto::{ClientEvent, Datagram, RelayClient, SEND_HEADER_LEN, classify};
use lattice_transport::{HomeRelay, PathKind};
use quinn::udp::{RecvMeta, Transmit};
use quinn::{AsyncUdpSocket, UdpPoller};
use std::fmt;
use std::future::Future;
use std::io::{self, IoSliceMut};
use std::net::SocketAddr;
use std::ops::Range;
use std::pin::Pin;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, Weak};
use std::task::{Context, Poll, ready};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::io::ReadBuf;
use tokio::sync::{Notify, watch};
use vaddr::VirtualAddrs;

/// Datagrams dropped in one `poll_recv` before it yields to other tasks, so junk on the
/// socket cannot starve the runtime (01-transport.md, Paths rule 10; proposed).
const RECV_BUDGET: usize = 64;

/// `AsyncUdpSocket` that hides relay vs direct from quinn.
///
/// - Outgoing: virtual peer address → NodeId → `SEND` via `RelayClient` (Paths rule 2).
/// - Incoming: demux by first byte; `RECV` is handed to quinn as coming from the
///   peer's virtual address (rule 3). Anything else is dropped (rule 10).
/// - Registration and keepalive run in a task beside it and never stop (rule 4).
#[derive(Debug)]
pub(crate) struct MagicSocket {
    udp: tokio::net::UdpSocket,
    /// The socket is IPv6: quinn then sees every peer as v4-mapped (quinn `ensure_ipv6`).
    v6: bool,
    start: Instant,
    /// The home relay: its address and its registration FSM, replaced together.
    relay: Mutex<Home>,
    vaddrs: Mutex<VirtualAddrs>,
    registered: watch::Sender<bool>,
    /// Wakes the registrar early (NAT rebinding: register now).
    wake: Arc<Notify>,
}

impl MagicSocket {
    /// Wraps a bound UDP socket that registers with `relay`.
    pub(crate) fn new(udp: tokio::net::UdpSocket, relay: HomeRelay) -> Self {
        let v6 = udp.local_addr().is_ok_and(|a| a.is_ipv6());
        Self {
            udp,
            v6,
            start: Instant::now(),
            relay: Mutex::new(Home::new(relay)),
            vaddrs: Mutex::new(VirtualAddrs::default()),
            registered: watch::Sender::new(false),
            wake: Arc::new(Notify::new()),
        }
    }

    /// Starts registration and keepalive. The task ends once the socket is dropped.
    pub(crate) fn spawn_registrar(self: &Arc<Self>, key: Arc<NodeKey>) {
        let sock = Arc::downgrade(self);
        let wake = self.wake.clone();
        tokio::spawn(registrar(sock, wake, key));
    }

    /// Moves the registration to `relay` (specs/10-settings.md, rule 35): a fresh
    /// `RelayClient` registers there at once. Datagrams from the old relay are dropped from
    /// now on; its binding expires on its own (01-transport.md, Relay rule 9).
    pub(crate) fn set_home_relay(&self, relay: HomeRelay) {
        *lock(&self.relay) = Home::new(relay);
        self.registered.send_replace(false);
        self.wake.notify_one();
    }

    /// The address quinn knows `peer` by, in the socket's family. `None` when no
    /// address is free (Paths rule 9).
    pub(crate) fn virtual_addr(&self, peer: NodeId) -> Option<SocketAddr> {
        let addr = lock(&self.vaddrs).addr_of(peer)?;
        Some(match addr {
            SocketAddr::V4(a) if self.v6 => {
                SocketAddr::new(std::net::IpAddr::V6(a.ip().to_ipv6_mapped()), a.port())
            }
            other => other,
        })
    }

    /// The path packets to `peer` take now. Relay only until hole punching (M2.5).
    pub(crate) fn path_of(&self, _peer: NodeId) -> PathKind {
        PathKind::Relay
    }

    /// Waits until the relay registration holds, at most `limit`. True when registered.
    pub(crate) async fn wait_registered(&self, limit: Duration) -> bool {
        let mut rx = self.registered.subscribe();
        tokio::time::timeout(limit, rx.wait_for(|r| *r))
            .await
            .is_ok_and(|r| r.is_ok())
    }

    fn now(&self) -> Duration {
        self.start.elapsed()
    }

    /// Sends a due `REGISTER` or `KEEPALIVE`. Returns when the registrar must run again.
    async fn drive_relay(&self, key: &NodeKey) -> Duration {
        let now = self.now();
        let mut out = Vec::new();
        let relay_addr = {
            let mut home = lock(&self.relay);
            let addr = home.addr;
            let c = &mut home.client;
            if c.poll_register(now) {
                match tls::random_nonce() {
                    Some(nonce) => c.register(key, nonce, unix_secs(), &mut out),
                    // The retry is already scheduled (Relay rule 13).
                    None => tracing::error!("os rng failed: REGISTER skipped"),
                }
            } else {
                c.poll_keepalive(now, &mut out);
            }
            self.registered.send_if_modified(|r| {
                let was = std::mem::replace(r, c.is_registered());
                was != *r
            });
            addr
        };
        if !out.is_empty()
            && let Err(e) = self.udp.send_to(&out, relay_addr).await
        {
            // UDP is best effort: the next deadline retries.
            tracing::debug!(error = %e, "relay send failed");
        }
        lock(&self.relay).client.next_deadline().unwrap_or(now)
    }

    /// What quinn sees of one received datagram: the virtual source and the packet range.
    fn demux(&self, src: SocketAddr, bytes: &[u8]) -> Option<(SocketAddr, Range<usize>)> {
        let event = {
            let mut home = lock(&self.relay);
            if !same_endpoint(src, home.addr) || classify(bytes) != Datagram::Relay {
                // Direct QUIC needs a validated path (M2.5). Rule 10: drop.
                return None;
            }
            home.client.on_datagram(self.now(), bytes)?
        };
        match event {
            ClientEvent::Packet { src, packet } => Some((self.virtual_addr(src)?, packet)),
            ClientEvent::Registered { observed } => {
                tracing::debug!(observed = %observed.addr(), "relay registered");
                self.registered.send_replace(true);
                None
            }
            ClientEvent::AddrChanged { observed } => {
                // The token stays until the new REG_OK (Relay rule 16).
                tracing::debug!(observed = %observed.addr(), "nat rebinding: re-registering");
                self.wake.notify_one();
                None
            }
            ClientEvent::RegisterDue => {
                // COOKIE (Relay rule 22): the registrar sends the REGISTER with the cookie.
                self.wake.notify_one();
                None
            }
            // Hole punching lands in M2.5.
            ClientEvent::PunchGo { .. } => None,
        }
    }
}

/// The home relay's address and its registration FSM.
#[derive(Debug)]
struct Home {
    addr: SocketAddr,
    client: RelayClient,
}

impl Home {
    fn new(relay: HomeRelay) -> Self {
        Self {
            addr: relay.addr(),
            client: RelayClient::new(relay.relay_id()),
        }
    }
}

async fn registrar(sock: Weak<MagicSocket>, wake: Arc<Notify>, key: Arc<NodeKey>) {
    loop {
        let (start, next) = {
            let Some(s) = sock.upgrade() else { return };
            (s.start, s.drive_relay(&key).await)
        };
        // The socket is not held while sleeping, so dropping the endpoint ends this task.
        let at = tokio::time::Instant::from_std(start + next);
        tokio::select! {
            () = tokio::time::sleep_until(at) => {}
            () = wake.notified() => {}
        }
    }
}

impl AsyncUdpSocket for MagicSocket {
    fn create_io_poller(self: Arc<Self>) -> Pin<Box<dyn UdpPoller>> {
        Box::pin(Writable {
            sock: self,
            fut: None,
        })
    }

    fn try_send(&self, transmit: &Transmit<'_>) -> io::Result<()> {
        let Some(peer) = lock(&self.vaddrs).node_at(transmit.destination) else {
            // Not a peer address: nothing can carry it. Drop, like a lost datagram.
            return Ok(());
        };
        let mut out = Vec::with_capacity(SEND_HEADER_LEN.saturating_add(transmit.contents.len()));
        let relay_addr = {
            let home = lock(&self.relay);
            if home
                .client
                .wrap_send(peer, transmit.contents, &mut out)
                .is_none()
            {
                // Not registered yet: quinn retransmits once the registration holds.
                return Ok(());
            }
            home.addr
        };
        self.udp.try_send_to(&out, relay_addr).map(drop)
    }

    fn poll_recv(
        &self,
        cx: &mut Context<'_>,
        bufs: &mut [IoSliceMut<'_>],
        meta: &mut [RecvMeta],
    ) -> Poll<io::Result<usize>> {
        let (Some(buf), Some(m)) = (bufs.first_mut(), meta.first_mut()) else {
            return Poll::Ready(Ok(0));
        };
        for _ in 0..RECV_BUDGET {
            let mut rb = ReadBuf::new(buf);
            let src = ready!(self.udp.poll_recv_from(cx, &mut rb))?;
            let len = rb.filled().len();
            let Some((from, packet)) = buf.get(..len).and_then(|b| self.demux(src, b)) else {
                continue;
            };
            let n = packet.len();
            buf.copy_within(packet, 0);
            *m = RecvMeta {
                addr: from,
                len: n,
                stride: n,
                ecn: None,
                dst_ip: None,
            };
            return Poll::Ready(Ok(1));
        }
        cx.waker().wake_by_ref();
        Poll::Pending
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        self.udp.local_addr()
    }
}

type WritableFut = Pin<Box<dyn Future<Output = io::Result<()>> + Send + Sync>>;

/// Write readiness for one quinn task (same pattern as quinn's tokio runtime).
struct Writable {
    sock: Arc<MagicSocket>,
    fut: Option<WritableFut>,
}

impl fmt::Debug for Writable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Writable").finish_non_exhaustive()
    }
}

impl UdpPoller for Writable {
    fn poll_writable(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        let fut = this.fut.get_or_insert_with(|| {
            let sock = this.sock.clone();
            Box::pin(async move { sock.udp.writable().await })
        });
        let res = fut.as_mut().poll(cx);
        if res.is_ready() {
            this.fut = None;
        }
        res
    }
}

/// Wall clock for `REGISTER.ts` (01, Relay wire format). Before 1970: 0, so the relay
/// refuses it as stale (fail closed).
fn unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Same IP (v4-mapped equals v4) and port.
fn same_endpoint(a: SocketAddr, b: SocketAddr) -> bool {
    a.ip().to_canonical() == b.ip().to_canonical() && a.port() == b.port()
}

/// Locks `m`. Poisoning is ignored: every critical section is a short, panic-free call.
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}
