//! The magic socket: one UDP socket, many paths (01-transport.md, Paths).

use lattice_key::NodeId;
use lattice_relay_proto::RelayClient;
use lattice_transport::{HomeRelay, PathSelector};
use quinn::udp::{RecvMeta, Transmit};
use quinn::{AsyncUdpSocket, UdpPoller};
use std::collections::BTreeMap;
use std::io::{self, IoSliceMut};
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::Mutex;
use std::task::{Context, Poll};

/// `AsyncUdpSocket` that hides relay vs direct from quinn.
///
/// - Outgoing: virtual peer address → NodeId → direct address if active,
///   else wrapped in `SEND` via `RelayClient` (rules 1–2).
/// - Incoming: demux by first byte (`relay_proto::classify`); `RECV` and direct
///   packets are handed to quinn as coming from the peer's virtual address (rule 3).
/// - Duplicates on both paths during the switch window (rule 5).
///
/// TBD: virtual address scheme per NodeId (e.g. an IPv6 ULA derived from the key?).
#[derive(Debug)]
#[allow(dead_code, reason = "L1 skeleton: read once the socket is implemented")]
pub(crate) struct MagicSocket {
    udp: tokio::net::UdpSocket,
    relay_addr: SocketAddr,
    relay: Mutex<RelayClient>,
    paths: Mutex<BTreeMap<NodeId, PathSelector>>,
}

#[allow(
    dead_code,
    reason = "L1 skeleton: built by QuicTransport::bind once implemented"
)]
impl MagicSocket {
    /// Wraps a bound UDP socket that registers with `relay`.
    pub(crate) fn new(udp: tokio::net::UdpSocket, relay: HomeRelay) -> Self {
        Self {
            udp,
            relay_addr: relay.addr(),
            relay: Mutex::new(RelayClient::new(relay.relay_id())),
            paths: Mutex::new(BTreeMap::new()),
        }
    }
}

impl AsyncUdpSocket for MagicSocket {
    #[allow(
        clippy::todo,
        reason = "L1 skeleton: body lands with its contract test"
    )]
    fn create_io_poller(self: Arc<Self>) -> Pin<Box<dyn UdpPoller>> {
        // poller over udp writability (same pattern as quinn's tokio runtime)
        todo!()
    }

    #[allow(
        clippy::todo,
        unused_variables,
        reason = "L1 skeleton: body lands with its contract test"
    )]
    fn try_send(&self, transmit: &Transmit) -> io::Result<()> {
        // map transmit.destination (virtual) → NodeId → path selector
        // direct: send to the direct addr; relay: wrap_send and send to the relay
        // duplicating: do both
        todo!()
    }

    #[allow(
        clippy::todo,
        unused_variables,
        reason = "L1 skeleton: body lands with its contract test"
    )]
    fn poll_recv(
        &self,
        cx: &mut Context<'_>,
        bufs: &mut [IoSliceMut<'_>],
        meta: &mut [RecvMeta],
    ) -> Poll<io::Result<usize>> {
        // loop: poll_recv_from the udp socket
        // relay message: feed RelayClient (REG_OK, PONG, PUNCH_GO handled here); RECV → copy packet out,
        //   meta.addr = virtual addr of src
        // direct QUIC packet: known peer addr → meta.addr = virtual addr; mark direct rx in path selector
        // probes: answer / validate; never surface to quinn
        todo!()
    }

    #[allow(
        clippy::todo,
        reason = "L1 skeleton: body lands with its contract test"
    )]
    fn local_addr(&self) -> io::Result<SocketAddr> {
        // the real bound address of udp
        todo!()
    }
}
