//! Client side of the relay: registration, keepalive, `SEND` / `RECV`.

use crate::codec::{ObservedAddr, Token};
use lattice_key::{NodeId, NodeKey};
use std::ops::Range;
use std::time::Duration;

/// Keepalive period. Many home NATs drop UDP mappings after ~30 s (01, Relay rule 3).
pub const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(25);

/// What a relay datagram meant for this node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientEvent {
    /// `REG_OK`: registered; the relay sees us at `observed`.
    Registered {
        /// Our public address as the relay saw it.
        observed: ObservedAddr,
    },
    /// `PONG` showed a new observed address (NAT rebinding).
    AddrChanged {
        /// New public address.
        observed: ObservedAddr,
    },
    /// `RECV`: a QUIC packet from `src`, at `packet` within the datagram.
    Packet {
        /// Sending node.
        src: NodeId,
        /// Byte range of the QUIC packet in the input datagram.
        packet: Range<usize>,
    },
    /// `PUNCH_GO`: start probing `peer`.
    PunchGo {
        /// Peer to probe.
        peer: NodeId,
        /// Shared probe nonce.
        punch_nonce: [u8; 16],
    },
}

/// Registration FSM for one home relay. Never stops registering (01, Paths rule 4).
#[derive(Debug)]
#[allow(dead_code, reason = "L1 skeleton: read once the FSM is implemented")]
pub struct RelayClient {
    relay_id: NodeId,
    token: Option<Token>,
    observed: Option<ObservedAddr>,
    next_keepalive: Option<Duration>,
}

impl RelayClient {
    /// A client for the relay whose key is `relay_id`. Starts unregistered.
    pub fn new(relay_id: NodeId) -> Self {
        Self {
            relay_id,
            token: None,
            observed: None,
            next_keepalive: None,
        }
    }

    /// True once `REG_OK` arrived.
    pub fn is_registered(&self) -> bool {
        self.token.is_some()
    }

    /// Builds a signed `REGISTER`. `nonce` and `ts` come from the driver.
    #[allow(
        clippy::todo,
        unused_variables,
        clippy::ptr_arg,
        reason = "L1 skeleton: body lands with its contract test"
    )]
    pub fn register(&self, key: &NodeKey, nonce: [u8; 16], ts: u64, out: &mut Vec<u8>) {
        // sign relay_id || nonce || ts with key; encode RelayMsg::Register into out
        todo!()
    }

    /// Handles a relay datagram from the relay address.
    #[allow(
        clippy::todo,
        unused_variables,
        reason = "L1 skeleton: body lands with its contract test"
    )]
    pub fn on_datagram(&mut self, now: Duration, bytes: &[u8]) -> Option<ClientEvent> {
        // decode; ignore errors (drop silently)
        // REG_OK: store token + observed, schedule keepalive at now + KEEPALIVE_INTERVAL
        // PONG: observed changed → AddrChanged
        // RECV: Packet { src, range of packet }
        // PUNCH_GO: PunchGo
        todo!()
    }

    /// Writes a `KEEPALIVE` into `out` when due. Returns true when written.
    #[allow(
        clippy::todo,
        unused_variables,
        clippy::ptr_arg,
        reason = "L1 skeleton: body lands with its contract test"
    )]
    pub fn poll_keepalive(&mut self, now: Duration, out: &mut Vec<u8>) -> bool {
        // registered and now >= next_keepalive: encode KEEPALIVE, reschedule, true
        // TBD: re-register when no PONG for how long?
        todo!()
    }

    /// Wraps a QUIC packet for `dst` in a `SEND`. `None` until registered.
    #[allow(
        clippy::todo,
        unused_variables,
        clippy::ptr_arg,
        reason = "L1 skeleton: body lands with its contract test"
    )]
    pub fn wrap_send(&self, dst: NodeId, packet: &[u8], out: &mut Vec<u8>) -> Option<()> {
        // token? else None; encode SEND { token, dst, packet } into out
        todo!()
    }

    /// Next time [`RelayClient::poll_keepalive`] must run.
    #[allow(
        clippy::todo,
        unused_variables,
        reason = "L1 skeleton: body lands with its contract test"
    )]
    pub fn next_deadline(&self) -> Option<Duration> {
        // next_keepalive
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::{RelayMsg, encode};
    use lattice_key::SecretKeyBytes;
    use std::net::SocketAddr;

    fn id(seed: u8) -> NodeId {
        NodeKey::from_secret(SecretKeyBytes::new([seed; 32])).node_id()
    }

    fn reg_ok() -> Vec<u8> {
        let observed = ObservedAddr::new(SocketAddr::from(([203, 0, 113, 5], 4000)));
        let mut wire = Vec::new();
        encode(
            &RelayMsg::RegOk {
                token: Token::from_bytes([1; 8]),
                observed,
                ttl: 60,
            },
            &mut wire,
        );
        wire
    }

    #[test]
    #[ignore = "contract: lands with RelayClient"]
    fn keepalive_every_25_seconds_after_registration() {
        let mut c = RelayClient::new(id(0));
        assert!(!c.is_registered());
        let Some(ClientEvent::Registered { observed }) = c.on_datagram(Duration::ZERO, &reg_ok())
        else {
            panic!("expected Registered");
        };
        assert_eq!(observed.addr(), SocketAddr::from(([203, 0, 113, 5], 4000)));
        assert!(c.is_registered());
        assert_eq!(c.next_deadline(), Some(KEEPALIVE_INTERVAL));
        let mut out = Vec::new();
        assert!(!c.poll_keepalive(Duration::from_secs(24), &mut out));
        assert!(c.poll_keepalive(KEEPALIVE_INTERVAL, &mut out));
    }

    #[test]
    #[ignore = "contract: lands with RelayClient"]
    fn register_signs_relay_id_nonce_and_ts() {
        let (relay, node) = (id(0), NodeKey::from_secret(SecretKeyBytes::new([1; 32])));
        let mut out = Vec::new();
        RelayClient::new(relay).register(&node, [7; 16], 42, &mut out);
        let Ok(RelayMsg::Register {
            node_id,
            nonce,
            ts,
            sig,
        }) = crate::decode(&out)
        else {
            panic!("expected REGISTER");
        };
        let mut signed = relay.as_bytes().to_vec();
        signed.extend_from_slice(&nonce);
        signed.extend_from_slice(&ts.to_be_bytes());
        assert_eq!((node_id, ts), (node.node_id(), 42));
        assert_eq!(node_id.verify(&signed, &sig), Ok(()));
    }

    #[test]
    #[ignore = "contract: lands with RelayClient"]
    fn send_needs_registration() {
        let mut c = RelayClient::new(id(0));
        let mut out = Vec::new();
        assert_eq!(c.wrap_send(id(1), &[0x40], &mut out), None);
        c.on_datagram(Duration::ZERO, &reg_ok());
        assert_eq!(c.wrap_send(id(1), &[0x40], &mut out), Some(()));
        assert_eq!(out.len(), crate::SEND_HEADER_LEN + 1);
    }
}
