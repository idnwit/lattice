//! Relay codec properties (01-transport.md, Relay invariants 1 and 3).

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test harness helpers: a failure should abort the test"
)]

use lattice_key::{NodeId, NodeKey, SecretKeyBytes, Signature};
use lattice_relay_proto::{ObservedAddr, RelayMsg, Token, decode, encode};
use proptest::prelude::*;
use proptest::test_runner::{Config, RngSeed};
use std::net::{IpAddr, SocketAddr};

fn config() -> Config {
    Config {
        cases: 2048,
        rng_seed: RngSeed::Fixed(0x2e1a_7000),
        ..Config::default()
    }
}

fn node_id() -> impl Strategy<Value = NodeId> {
    any::<[u8; 32]>().prop_map(|s| NodeKey::from_secret(SecretKeyBytes::new(s)).node_id())
}

fn token() -> impl Strategy<Value = Token> {
    any::<[u8; 8]>().prop_map(Token::from_bytes)
}

fn addr() -> impl Strategy<Value = ObservedAddr> {
    (any::<IpAddr>(), any::<u16>())
        .prop_map(|(ip, port)| ObservedAddr::new(SocketAddr::new(ip, port)))
}

/// Owned form of a message, so the strategy can hold the tail bytes.
#[derive(Debug, Clone)]
enum Owned {
    Register(NodeId, [u8; 16], u64, [u8; 64]),
    RegOk(Token, ObservedAddr, u16, [u8; 16]),
    Keepalive(Token),
    Pong(ObservedAddr),
    Send(Token, NodeId, Vec<u8>),
    Recv(NodeId, Vec<u8>),
    PunchReq(Token, NodeId, Vec<u8>),
    PunchGo(NodeId, Vec<u8>, [u8; 16]),
}

impl Owned {
    fn msg(&self) -> RelayMsg<'_> {
        match self {
            Self::Register(node_id, nonce, ts, sig) => RelayMsg::Register {
                node_id: *node_id,
                nonce: *nonce,
                ts: *ts,
                sig: Signature::from_bytes(*sig),
            },
            Self::RegOk(token, observed, ttl, nonce) => RelayMsg::RegOk {
                token: *token,
                observed: *observed,
                ttl: *ttl,
                nonce: *nonce,
            },
            Self::Keepalive(token) => RelayMsg::Keepalive { token: *token },
            Self::Pong(observed) => RelayMsg::Pong {
                observed: *observed,
            },
            Self::Send(token, dst, packet) => RelayMsg::Send {
                token: *token,
                dst: *dst,
                packet,
            },
            Self::Recv(src, packet) => RelayMsg::Recv { src: *src, packet },
            Self::PunchReq(token, dst, candidates) => RelayMsg::PunchReq {
                token: *token,
                dst: *dst,
                candidates,
            },
            Self::PunchGo(peer, candidates, punch_nonce) => RelayMsg::PunchGo {
                peer: *peer,
                candidates,
                punch_nonce: *punch_nonce,
            },
        }
    }
}

fn message() -> impl Strategy<Value = Owned> {
    let tail = || proptest::collection::vec(any::<u8>(), 0..1500);
    prop_oneof![
        (
            node_id(),
            any::<[u8; 16]>(),
            any::<u64>(),
            any::<[u8; 64]>()
        )
            .prop_map(|(n, nonce, ts, sig)| Owned::Register(n, nonce, ts, sig)),
        (token(), addr(), any::<u16>(), any::<[u8; 16]>())
            .prop_map(|(t, a, ttl, n)| Owned::RegOk(t, a, ttl, n)),
        token().prop_map(Owned::Keepalive),
        addr().prop_map(Owned::Pong),
        (token(), node_id(), tail()).prop_map(|(t, n, p)| Owned::Send(t, n, p)),
        (node_id(), tail()).prop_map(|(n, p)| Owned::Recv(n, p)),
        (token(), node_id(), tail()).prop_map(|(t, n, c)| Owned::PunchReq(t, n, c)),
        (node_id(), tail(), any::<[u8; 16]>())
            .prop_map(|(n, c, nonce)| Owned::PunchGo(n, c, nonce)),
    ]
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn every_message_roundtrips(m in message()) {
        let msg = m.msg();
        let mut wire = Vec::new();
        encode(&msg, &mut wire);
        prop_assert_eq!(decode(&wire), Ok(msg));
    }

    #[test]
    fn decode_never_panics(bytes in proptest::collection::vec(any::<u8>(), 0..256)) {
        drop(decode(&bytes));
    }

    #[test]
    fn decode_never_panics_on_truncated_messages(m in message(), cut in any::<prop::sample::Index>()) {
        let mut wire = Vec::new();
        encode(&m.msg(), &mut wire);
        drop(decode(&wire[..cut.index(wire.len() + 1)]));
    }
}
