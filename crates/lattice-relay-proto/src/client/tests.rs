//! `client` unit tests.

use super::*;
use crate::codec::{RelayMsg, encode};
use lattice_key::SecretKeyBytes;
use std::net::SocketAddr;

fn id(seed: u8) -> NodeId {
    node(seed).node_id()
}

fn node(seed: u8) -> NodeKey {
    NodeKey::from_secret(SecretKeyBytes::new([seed; 32]))
}

const NONCE: [u8; 16] = [7; 16];

fn reg_ok(nonce: [u8; 16], token: u8) -> Vec<u8> {
    let observed = ObservedAddr::new(SocketAddr::from(([203, 0, 113, 5], 4000)));
    let mut wire = Vec::new();
    encode(
        &RelayMsg::RegOk {
            token: Token::from_bytes([token; 8]),
            observed,
            ttl: 90,
            nonce,
        },
        &mut wire,
    );
    wire
}

fn pong(port: u16) -> Vec<u8> {
    let mut wire = Vec::new();
    let observed = ObservedAddr::new(SocketAddr::from(([203, 0, 113, 5], port)));
    encode(&RelayMsg::Pong { observed }, &mut wire);
    wire
}

/// A client that sent `REGISTER` with `nonce` at `now`.
fn sent(c: &mut RelayClient, nonce: [u8; 16]) {
    c.register(&node(1), nonce, 1, &mut Vec::new());
}

fn token_of(c: &RelayClient) -> Option<Token> {
    let mut out = Vec::new();
    c.wrap_send(id(9), &[0x40], &mut out)?;
    match crate::decode(&out) {
        Ok(RelayMsg::Send { token, .. }) => Some(token),
        _ => None,
    }
}

#[test]
fn keepalive_every_25_seconds_after_registration() {
    let mut c = RelayClient::new(id(0));
    assert!(!c.is_registered());
    sent(&mut c, NONCE);
    let Some(ClientEvent::Registered { observed }) =
        c.on_datagram(Duration::ZERO, &reg_ok(NONCE, 1))
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
fn register_signs_relay_id_nonce_and_ts() {
    let (relay, node) = (id(0), node(1));
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
fn send_needs_registration() {
    let mut c = RelayClient::new(id(0));
    let mut out = Vec::new();
    assert_eq!(c.wrap_send(id(1), &[0x40], &mut out), None);
    sent(&mut c, NONCE);
    c.on_datagram(Duration::ZERO, &reg_ok(NONCE, 1));
    assert_eq!(c.wrap_send(id(1), &[0x40], &mut out), Some(()));
    assert_eq!(out.len(), crate::SEND_HEADER_LEN + 1);
}

/// Rule 15: two REGISTERs in flight; only the last one's REG_OK counts, in either order.
#[test]
fn only_the_reg_ok_of_the_last_register_is_accepted() {
    for late_first in [true, false] {
        let mut c = RelayClient::new(id(0));
        sent(&mut c, [1; 16]);
        sent(&mut c, [2; 16]);
        let (stale, live) = (reg_ok([1; 16], 1), reg_ok([2; 16], 2));
        let order = if late_first {
            [&stale, &live]
        } else {
            [&live, &stale]
        };
        for wire in order {
            c.on_datagram(Duration::ZERO, wire);
        }
        assert_eq!(
            token_of(&c),
            Some(Token::from_bytes([2; 8])),
            "case=late_first {late_first}"
        );
    }
}

#[test]
fn a_reg_ok_without_a_register_is_ignored() {
    let mut c = RelayClient::new(id(0));
    assert_eq!(c.on_datagram(Duration::ZERO, &reg_ok(NONCE, 1)), None);
    assert!(!c.is_registered());
}

#[test]
fn client_reregisters_after_sixty_seconds_of_silence() {
    let mut c = RelayClient::new(id(0));
    assert!(c.poll_register(Duration::ZERO));
    assert!(!c.poll_register(Duration::from_secs(4)));
    assert!(c.poll_register(REGISTER_RETRY));
    sent(&mut c, NONCE);
    c.on_datagram(REGISTER_RETRY, &reg_ok(NONCE, 1));
    assert!(!c.poll_register(REGISTER_RETRY));
    let silent_until = REGISTER_RETRY + SILENCE_LIMIT;
    assert_eq!(c.next_deadline(), Some(REGISTER_RETRY + KEEPALIVE_INTERVAL));
    let mut out = Vec::new();
    assert!(c.poll_keepalive(REGISTER_RETRY + KEEPALIVE_INTERVAL, &mut out));
    assert!(c.is_registered());
    assert!(!c.poll_register(silent_until - Duration::from_secs(1)));
    assert!(c.poll_register(silent_until));
    assert!(!c.is_registered());
    assert_eq!(c.wrap_send(id(1), &[0x40], &mut out), None);
}

#[test]
fn pong_from_the_same_address_keeps_the_registration_alive() {
    let mut c = RelayClient::new(id(0));
    sent(&mut c, NONCE);
    c.on_datagram(Duration::ZERO, &reg_ok(NONCE, 1));
    assert_eq!(c.on_datagram(Duration::from_secs(50), &pong(4000)), None);
    assert!(!c.poll_register(Duration::from_secs(100)));
    assert!(c.is_registered());
}

/// Rule 16.
#[test]
fn a_pong_with_a_new_address_reregisters_at_once() {
    let mut c = RelayClient::new(id(0));
    sent(&mut c, NONCE);
    c.on_datagram(Duration::ZERO, &reg_ok(NONCE, 1));
    let t30 = Duration::from_secs(30);
    let moved = c.on_datagram(t30, &pong(4001));
    assert!(matches!(moved, Some(ClientEvent::AddrChanged { .. })));
    assert!(c.next_deadline() <= Some(t30), "register is due now");
    assert!(c.poll_register(t30));
    sent(&mut c, [8; 16]);
    c.on_datagram(t30, &reg_ok([8; 16], 2));
    assert_eq!(token_of(&c), Some(Token::from_bytes([2; 8])));
    assert!(!c.poll_register(t30));
}

/// Rule 16: a spoofed PONG never drops the token; repeats cost one REGISTER per retry.
#[test]
fn a_spoofed_pong_keeps_the_token_and_is_rate_limited() {
    let mut c = RelayClient::new(id(0));
    sent(&mut c, NONCE);
    c.on_datagram(Duration::ZERO, &reg_ok(NONCE, 1));
    let t = Duration::from_secs(10);
    c.on_datagram(t, &pong(4001));
    assert!(c.poll_register(t));
    c.on_datagram(t, &pong(4002));
    assert!(!c.poll_register(t + Duration::from_secs(1)));
    assert!(c.poll_register(t + REGISTER_RETRY));
    assert_eq!(token_of(&c), Some(Token::from_bytes([1; 8])));
}

#[test]
fn recv_yields_the_packet_range() {
    let mut c = RelayClient::new(id(0));
    let mut wire = Vec::new();
    encode(
        &RelayMsg::Recv {
            src: id(1),
            packet: &[0x40, 7, 8],
        },
        &mut wire,
    );
    let Some(ClientEvent::Packet { src, packet }) = c.on_datagram(Duration::ZERO, &wire) else {
        panic!("expected Packet");
    };
    assert_eq!((src, wire.get(packet)), (id(1), Some(&[0x40, 7, 8][..])));
}
