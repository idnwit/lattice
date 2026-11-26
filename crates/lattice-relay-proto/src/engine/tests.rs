//! `engine` unit tests.

use super::*;
use crate::codec::{RelayMsg, decode, encode};
use lattice_key::{NodeKey, SecretKeyBytes};

fn key(seed: u8) -> NodeKey {
    NodeKey::from_secret(SecretKeyBytes::new([seed; 32]))
}

/// Fresh wall clock for `ts = 1`.
fn at(mono: Duration) -> Now {
    Now::new(mono, 1)
}

fn register_wire(relay: NodeId, node: &NodeKey, nonce: [u8; 16], ts: u64) -> Vec<u8> {
    let msg = RelayMsg::Register {
        node_id: node.node_id(),
        nonce,
        ts,
        sig: node.sign(&register_signed(relay, nonce, ts)),
    };
    let mut wire = Vec::new();
    encode(&msg, &mut wire);
    wire
}

fn register(engine: &mut RelayEngine, node: &NodeKey, src: SocketAddr, tok: Token) -> Token {
    let wire = register_wire(engine.relay_id, node, [3; 16], 1);
    let EngineOutput::Reply(t) = engine.handle(at(Duration::ZERO), src, &wire, tok) else {
        panic!("expected REG_OK");
    };
    let Ok(RelayMsg::RegOk { token, .. }) = decode(t.bytes()) else {
        panic!("expected REG_OK")
    };
    token
}

#[test]
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
    let out = engine.handle(at(Duration::ZERO), a_addr, &wire, Token::from_bytes([9; 8]));
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
fn send_is_dropped_after_the_registration_expires() {
    let ttl = Duration::from_secs(60);
    let mut engine = RelayEngine::new(key(0).node_id(), ttl);
    let (a, b) = (key(1), key(2));
    let a_addr = SocketAddr::from(([192, 0, 2, 1], 1));
    let b_addr = SocketAddr::from(([192, 0, 2, 2], 2));
    let tok_a = register(&mut engine, &a, a_addr, Token::from_bytes([1; 8]));
    register(&mut engine, &b, b_addr, Token::from_bytes([2; 8]));
    engine.on_tick(at(ttl));
    let mut wire = Vec::new();
    let send = RelayMsg::Send {
        token: tok_a,
        dst: b.node_id(),
        packet: &[0x40],
    };
    encode(&send, &mut wire);
    let out = engine.handle(at(ttl), a_addr, &wire, Token::from_bytes([9; 8]));
    assert_eq!(out, EngineOutput::Drop);
}

#[test]
fn quic_packets_sent_to_the_relay_are_dropped() {
    let mut engine = RelayEngine::new(key(0).node_id(), Duration::from_secs(60));
    let src = SocketAddr::from(([192, 0, 2, 1], 1));
    assert_eq!(
        engine.handle(
            at(Duration::ZERO),
            src,
            &[0xC0, 1, 2],
            Token::from_bytes([1; 8])
        ),
        EngineOutput::Drop
    );
}

#[test]
fn replayed_register_is_dropped() {
    let mut engine = RelayEngine::new(key(0).node_id(), DEFAULT_TTL);
    let a = key(1);
    let (home, thief) = (
        SocketAddr::from(([192, 0, 2, 1], 1)),
        SocketAddr::from(([198, 51, 100, 9], 9)),
    );
    let wire = register_wire(engine.relay_id, &a, [5; 16], 1);
    let first = engine.handle(at(Duration::ZERO), home, &wire, Token::from_bytes([1; 8]));
    assert!(matches!(first, EngineOutput::Reply(_)));
    let replay = engine.handle(
        at(Duration::from_secs(1)),
        thief,
        &wire,
        Token::from_bytes([2; 8]),
    );
    assert_eq!(replay, EngineOutput::Drop);
    let reg = engine
        .table
        .lookup_node(a.node_id(), Duration::from_secs(1));
    assert_eq!(reg.map(|r| r.addr()), Some(home));
}

#[test]
fn stale_register_is_dropped() {
    let mut engine = RelayEngine::new(key(0).node_id(), DEFAULT_TTL);
    let a = key(1);
    let src = SocketAddr::from(([192, 0, 2, 1], 1));
    let now = Now::new(Duration::ZERO, 1_000);
    let cases: [(u64, u8, bool); 4] = [
        (969, 1, false),
        (970, 2, true),
        (1_030, 3, true),
        (1_031, 4, false),
    ];
    for (ts, n, fresh) in cases {
        let wire = register_wire(engine.relay_id, &a, [n; 16], ts);
        let out = engine.handle(now, src, &wire, Token::from_bytes([n; 8]));
        assert_eq!(matches!(out, EngineOutput::Reply(_)), fresh, "case=ts {ts}");
    }
}

#[test]
fn register_with_a_bad_signature_is_dropped() {
    let mut engine = RelayEngine::new(key(0).node_id(), DEFAULT_TTL);
    let src = SocketAddr::from(([192, 0, 2, 1], 1));
    // Signed for another relay.
    let wire = register_wire(key(7).node_id(), &key(1), [1; 16], 1);
    let out = engine.handle(at(Duration::ZERO), src, &wire, Token::from_bytes([1; 8]));
    assert_eq!(out, EngineOutput::Drop);
}

#[test]
fn keepalive_gets_a_pong_and_extends_the_registration() {
    let mut engine = RelayEngine::new(key(0).node_id(), DEFAULT_TTL);
    let a = key(1);
    let src = SocketAddr::from(([192, 0, 2, 1], 1));
    let tok = register(&mut engine, &a, src, Token::from_bytes([1; 8]));
    let mut wire = Vec::new();
    encode(&RelayMsg::Keepalive { token: tok }, &mut wire);
    let t80 = Duration::from_secs(80);
    let EngineOutput::Reply(t) = engine.handle(at(t80), src, &wire, Token::from_bytes([9; 8]))
    else {
        panic!("expected PONG");
    };
    assert_eq!(
        decode(t.bytes()),
        Ok(RelayMsg::Pong {
            observed: ObservedAddr::new(src)
        })
    );
    engine.on_tick(at(DEFAULT_TTL + Duration::from_secs(1)));
    assert!(engine.table.lookup_node(a.node_id(), DEFAULT_TTL).is_some());
}

fn addr(last: u8, port: u16) -> SocketAddr {
    SocketAddr::from(([192, 0, 2, last], port))
}

fn send_wire(token: Token, dst: NodeId) -> Vec<u8> {
    let mut wire = Vec::new();
    let msg = RelayMsg::Send {
        token,
        dst,
        packet: &[0x40],
    };
    encode(&msg, &mut wire);
    wire
}

/// Rule 11: a rebinding node learns its new address, but the binding stays put.
#[test]
fn keepalive_from_a_new_address_gets_its_pong_but_no_rebind() {
    let mut engine = RelayEngine::new(key(0).node_id(), DEFAULT_TTL);
    let (a, src, other) = (key(1), addr(1, 1), addr(1, 2));
    let tok = register(&mut engine, &a, src, Token::from_bytes([1; 8]));
    let mut wire = Vec::new();
    encode(&RelayMsg::Keepalive { token: tok }, &mut wire);
    let t80 = Duration::from_secs(80);
    let EngineOutput::Reply(t) = engine.handle(at(t80), other, &wire, Token::from_bytes([9; 8]))
    else {
        panic!("expected PONG");
    };
    assert_eq!(t.dst(), other);
    let pong = RelayMsg::Pong {
        observed: ObservedAddr::new(other),
    };
    assert_eq!(decode(t.bytes()), Ok(pong));
    assert_eq!(
        engine.table.lookup_node(a.node_id(), t80).map(|r| r.addr()),
        Some(src)
    );
    assert!(
        engine.table.lookup_node(a.node_id(), DEFAULT_TTL).is_none(),
        "not extended"
    );
}

#[test]
fn keepalive_with_an_unknown_token_is_dropped() {
    let mut engine = RelayEngine::new(key(0).node_id(), DEFAULT_TTL);
    let mut wire = Vec::new();
    encode(
        &RelayMsg::Keepalive {
            token: Token::from_bytes([4; 8]),
        },
        &mut wire,
    );
    let out = engine.handle(
        at(Duration::ZERO),
        addr(1, 1),
        &wire,
        Token::from_bytes([9; 8]),
    );
    assert_eq!(out, EngineOutput::Drop);
}

/// Invariant 4, through the engine.
#[test]
fn send_from_another_address_is_dropped() {
    let mut engine = RelayEngine::new(key(0).node_id(), DEFAULT_TTL);
    let (a, b) = (key(1), key(2));
    let tok_a = register(&mut engine, &a, addr(1, 1), Token::from_bytes([1; 8]));
    register(&mut engine, &b, addr(2, 2), Token::from_bytes([2; 8]));
    let wire = send_wire(tok_a, b.node_id());
    let out = engine.handle(
        at(Duration::ZERO),
        addr(3, 3),
        &wire,
        Token::from_bytes([9; 8]),
    );
    assert_eq!(out, EngineOutput::Drop);
}

/// Rule 12: the sender is live; only the destination is gone.
#[test]
fn send_to_an_expired_destination_is_dropped() {
    let mut engine = RelayEngine::new(key(0).node_id(), DEFAULT_TTL);
    let (a, b) = (key(1), key(2));
    register(&mut engine, &b, addr(2, 2), Token::from_bytes([2; 8]));
    let late = Duration::from_secs(80);
    let a_wire = register_wire(engine.relay_id, &a, [4; 16], 1);
    let out = engine.handle(at(late), addr(1, 1), &a_wire, Token::from_bytes([1; 8]));
    assert!(matches!(out, EngineOutput::Reply(_)));
    let after_b = DEFAULT_TTL + Duration::from_secs(1);
    engine.on_tick(at(after_b));
    assert!(engine.table.lookup_node(a.node_id(), after_b).is_some());
    let out = engine.handle(
        at(after_b),
        addr(1, 1),
        &send_wire(Token::from_bytes([1; 8]), b.node_id()),
        Token::from_bytes([9; 8]),
    );
    assert_eq!(out, EngineOutput::Drop);
}

/// Rule 9.
#[test]
fn a_new_register_replaces_the_old_token_and_address() {
    let mut engine = RelayEngine::new(key(0).node_id(), DEFAULT_TTL);
    let (a, b) = (key(1), key(2));
    let old = register(&mut engine, &a, addr(1, 1), Token::from_bytes([1; 8]));
    register(&mut engine, &b, addr(2, 2), Token::from_bytes([2; 8]));
    let wire = register_wire(engine.relay_id, &a, [8; 16], 1);
    let out = engine.handle(
        at(Duration::ZERO),
        addr(1, 5),
        &wire,
        Token::from_bytes([3; 8]),
    );
    assert!(matches!(out, EngineOutput::Reply(_)));
    let old_send = engine.handle(
        at(Duration::ZERO),
        addr(1, 1),
        &send_wire(old, b.node_id()),
        Token::from_bytes([9; 8]),
    );
    assert_eq!(old_send, EngineOutput::Drop);
    let reg = engine.table.lookup_node(a.node_id(), Duration::ZERO);
    assert_eq!(reg.map(|r| r.addr()), Some(addr(1, 5)));
}

#[test]
fn a_token_collision_is_dropped_and_the_nonce_stays_usable() {
    let mut engine = RelayEngine::new(key(0).node_id(), DEFAULT_TTL);
    let tok = Token::from_bytes([1; 8]);
    register(&mut engine, &key(1), addr(1, 1), tok);
    let wire = register_wire(engine.relay_id, &key(2), [5; 16], 1);
    let clash = engine.handle(at(Duration::ZERO), addr(2, 2), &wire, tok);
    assert_eq!(clash, EngineOutput::Drop);
    let retry = engine.handle(
        at(Duration::ZERO),
        addr(2, 2),
        &wire,
        Token::from_bytes([2; 8]),
    );
    assert!(matches!(retry, EngineOutput::Reply(_)));
}

/// Rule 8.
#[test]
fn a_full_nonce_cache_drops_new_registers_until_entries_expire() {
    let mut engine = RelayEngine::new(key(0).node_id(), DEFAULT_TTL);
    engine.max_seen = 1;
    register(&mut engine, &key(1), addr(1, 1), Token::from_bytes([1; 8]));
    let wire = register_wire(engine.relay_id, &key(2), [5; 16], 1);
    let full = engine.handle(
        at(Duration::ZERO),
        addr(2, 2),
        &wire,
        Token::from_bytes([2; 8]),
    );
    assert_eq!(full, EngineOutput::Drop);
    // ts = 1 is stale at wall 32: the first nonce is forgotten.
    engine.on_tick(Now::new(Duration::from_secs(31), 32));
    let wire = register_wire(engine.relay_id, &key(2), [5; 16], 32);
    let now = Now::new(Duration::from_secs(31), 32);
    let out = engine.handle(now, addr(2, 2), &wire, Token::from_bytes([2; 8]));
    assert!(matches!(out, EngineOutput::Reply(_)));
}

/// Invariant 5: the nonce lives as long as its ts is fresh on the wall clock, whatever mono does.
#[test]
fn a_replay_after_a_wall_clock_step_back_is_dropped() {
    let mut engine = RelayEngine::new(key(0).node_id(), DEFAULT_TTL);
    let a = key(1);
    let wire = register_wire(engine.relay_id, &a, [5; 16], 1_030);
    let first = engine.handle(
        Now::new(Duration::ZERO, 1_000),
        addr(1, 1),
        &wire,
        Token::from_bytes([1; 8]),
    );
    assert!(matches!(first, EngineOutput::Reply(_)));
    // 120 s of monotonic time pass, but the wall clock stepped back: ts is still fresh.
    let later = Now::new(Duration::from_secs(120), 1_050);
    engine.on_tick(later);
    let replay = engine.handle(later, addr(9, 9), &wire, Token::from_bytes([2; 8]));
    assert_eq!(replay, EngineOutput::Drop);
}
