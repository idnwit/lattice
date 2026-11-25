//! Relay server engine: one datagram in, what to send out.

use crate::codec::Token;
use crate::table::TokenTable;
use lattice_key::NodeId;
use std::net::SocketAddr;
use std::time::Duration;

/// One datagram to send.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transmit {
    dst: SocketAddr,
    bytes: Vec<u8>,
}

impl Transmit {
    /// Where to send.
    pub fn dst(&self) -> SocketAddr {
        self.dst
    }

    /// Datagram bytes.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// What the UDP loop does with one input datagram.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineOutput {
    /// Drop silently: unknown, unauthorized, or QUIC sent to the relay.
    Drop,
    /// Answer the sender (`REG_OK`, `PONG`).
    Reply(Transmit),
    /// `SEND` rewritten as `RECV` to the destination.
    Forward(Transmit),
    /// `PUNCH_GO` to both sides.
    Punch([Transmit; 2]),
}

/// Sans-IO relay. Owns the token table. Never reads packet contents.
#[derive(Debug)]
#[allow(dead_code, reason = "L1 skeleton: read once the engine is implemented")]
pub struct RelayEngine {
    relay_id: NodeId,
    ttl: Duration,
    table: TokenTable,
}

impl RelayEngine {
    /// A relay with key `relay_id` granting registrations for `ttl`.
    ///
    /// TBD: default registration ttl (08-infrastructure.md)?
    pub fn new(relay_id: NodeId, ttl: Duration) -> Self {
        Self {
            relay_id,
            ttl,
            table: TokenTable::new(),
        }
    }

    /// Handles one datagram from `src` at `now`.
    ///
    /// `fresh_token` is a random token from the driver's RNG; used only when
    /// this datagram creates a registration (keeps the engine deterministic).
    #[allow(
        clippy::todo,
        unused_variables,
        reason = "L1 skeleton: body lands with its contract test"
    )]
    pub fn handle(
        &mut self,
        now: Duration,
        src: SocketAddr,
        bytes: &[u8],
        fresh_token: Token,
    ) -> EngineOutput {
        // classify; anything but a decodable relay message: Drop
        // REGISTER: verify sig over relay_id || nonce || ts (TBD: ts freshness window?);
        //   register in table; Reply(REG_OK { token, observed: src, ttl })
        // KEEPALIVE: authorize(token, src); refresh; Reply(PONG { observed: src })
        // SEND: authorize(token, src) else Drop; lookup dst else Drop;
        //   Forward(RECV { src: sender node, packet }) to dst addr
        // PUNCH_REQ: authorize; lookup dst; Punch(PUNCH_GO to both, shared nonce)
        //   TBD: where the punch nonce comes from (driver RNG like fresh_token?)
        todo!()
    }

    /// Drops expired registrations. Call on a timer.
    #[allow(
        clippy::todo,
        unused_variables,
        reason = "L1 skeleton: body lands with its contract test"
    )]
    pub fn on_tick(&mut self, now: Duration) {
        // table.expire(now)
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::{RelayMsg, decode, encode};
    use lattice_key::{NodeKey, SecretKeyBytes};

    fn key(seed: u8) -> NodeKey {
        NodeKey::from_secret(SecretKeyBytes::new([seed; 32]))
    }

    fn register(engine: &mut RelayEngine, node: &NodeKey, src: SocketAddr, tok: Token) -> Token {
        let (nonce, ts) = ([3; 16], 1u64);
        let mut signed = engine.relay_id.as_bytes().to_vec();
        signed.extend_from_slice(&nonce);
        signed.extend_from_slice(&ts.to_be_bytes());
        let msg = RelayMsg::Register {
            node_id: node.node_id(),
            nonce,
            ts,
            sig: node.sign(&signed),
        };
        let mut wire = Vec::new();
        encode(&msg, &mut wire);
        let EngineOutput::Reply(t) = engine.handle(Duration::ZERO, src, &wire, tok) else {
            panic!("expected REG_OK");
        };
        let Ok(RelayMsg::RegOk { token, .. }) = decode(t.bytes()) else {
            panic!("expected REG_OK")
        };
        token
    }

    #[test]
    #[ignore = "contract: lands with RelayEngine::handle"]
    fn send_is_forwarded_as_recv_to_the_registered_peer() {
        let mut engine = RelayEngine::new(key(0).node_id(), Duration::from_secs(60));
        let (a, b) = (key(1), key(2));
        let (a_addr, b_addr) = (
            SocketAddr::from(([192, 0, 2, 1], 1)),
            SocketAddr::from(([192, 0, 2, 2], 2)),
        );
        let tok_a = register(&mut engine, &a, a_addr, Token::from_bytes([1; 8]));
        register(&mut engine, &b, b_addr, Token::from_bytes([2; 8]));
        let mut wire = Vec::new();
        encode(
            &RelayMsg::Send {
                token: tok_a,
                dst: b.node_id(),
                packet: &[0x40, 9],
            },
            &mut wire,
        );
        let out = engine.handle(Duration::ZERO, a_addr, &wire, Token::from_bytes([9; 8]));
        let EngineOutput::Forward(t) = out else {
            panic!("expected Forward, got {out:?}")
        };
        assert_eq!(t.dst(), b_addr);
        assert_eq!(
            decode(t.bytes()),
            Ok(RelayMsg::Recv {
                src: a.node_id(),
                packet: &[0x40, 9]
            })
        );
    }

    #[test]
    #[ignore = "contract: lands with RelayEngine::handle"]
    fn send_is_dropped_after_the_registration_expires() {
        let ttl = Duration::from_secs(60);
        let mut engine = RelayEngine::new(key(0).node_id(), ttl);
        let (a, b) = (key(1), key(2));
        let a_addr = SocketAddr::from(([192, 0, 2, 1], 1));
        let b_addr = SocketAddr::from(([192, 0, 2, 2], 2));
        let tok_a = register(&mut engine, &a, a_addr, Token::from_bytes([1; 8]));
        register(&mut engine, &b, b_addr, Token::from_bytes([2; 8]));
        engine.on_tick(ttl);
        let mut wire = Vec::new();
        let send = RelayMsg::Send {
            token: tok_a,
            dst: b.node_id(),
            packet: &[0x40],
        };
        encode(&send, &mut wire);
        let out = engine.handle(ttl, a_addr, &wire, Token::from_bytes([9; 8]));
        assert_eq!(out, EngineOutput::Drop);
    }

    #[test]
    #[ignore = "contract: lands with RelayEngine::handle"]
    fn quic_packets_sent_to_the_relay_are_dropped() {
        let mut engine = RelayEngine::new(key(0).node_id(), Duration::from_secs(60));
        let src = SocketAddr::from(([192, 0, 2, 1], 1));
        assert_eq!(
            engine.handle(
                Duration::ZERO,
                src,
                &[0xC0, 1, 2],
                Token::from_bytes([1; 8])
            ),
            EngineOutput::Drop
        );
    }
}
