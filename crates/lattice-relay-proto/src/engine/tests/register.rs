//! Registration defenses: capacity, replay across clock steps, cookies, floods, reply sizes
//! (rules 7, 8, 11, 17–23).

use super::*;

/// Rule 8.
#[test]
fn a_full_relay_drops_only_unknown_nodes() {
    let mut engine = engine(DEFAULT_TTL);
    engine.max_nodes = 1;
    register(&mut engine, &key(1), addr(1, 1), Token::from_bytes([1; 8]));
    let stranger = register_wire(&engine, &key(2), [5; 16], 1, addr(2, 2), 1);
    let full = engine.handle(
        at(Duration::ZERO),
        addr(2, 2),
        &stranger,
        Token::from_bytes([2; 8]),
    );
    assert_eq!(full, EngineOutput::Drop);
    let again = register_wire(&engine, &key(1), [6; 16], 1, addr(1, 1), 1);
    let known = engine.handle(
        at(Duration::ZERO),
        addr(1, 1),
        &again,
        Token::from_bytes([3; 8]),
    );
    assert!(is_reg_ok(&known), "a known node still re-registers");
}

/// Invariant 5: the nonce lives as long as its ts is fresh on the wall clock, whatever mono does.
#[test]
fn a_replay_after_a_wall_clock_step_back_is_dropped() {
    let mut engine = engine(DEFAULT_TTL);
    let a = key(1);
    let reg = register_wire(&engine, &a, [5; 16], 1_030, addr(1, 1), 1_020);
    let first = engine.handle(
        Now::new(Duration::ZERO, 1_020),
        addr(1, 1),
        &reg,
        Token::from_bytes([1; 8]),
    );
    assert!(is_reg_ok(&first));
    // 120 s of monotonic time pass and the registration expires, but the wall clock stepped
    // back: ts is still fresh and the signed cookie is still valid (next bucket).
    let later = Now::new(Duration::from_secs(120), 1_050);
    engine.on_tick(later);
    assert!(!engine.table.knows(a.node_id()), "registration expired");
    let out = engine.handle(later, addr(1, 1), &reg, Token::from_bytes([2; 8]));
    assert_eq!(out, EngineOutput::Drop);
}

/// Invariant 5, rule 17: an on-path attacker swaps its own valid cookie into a victim's
/// REGISTER and sends it from its own address. The signature covers the cookie, so it fails.
#[test]
fn a_swapped_cookie_breaks_the_signature() {
    let mut engine = engine(DEFAULT_TTL);
    let victim = key(1);
    let (home, attacker) = (addr(1, 1), SocketAddr::from(([198, 51, 100, 9], 9)));
    let attacker_cookie = engine.cookies.issue(attacker, 1);
    let steal = |engine: &mut RelayEngine, captured: &[u8], name: &str| {
        let forged = swap_cookie(captured, attacker_cookie);
        let out = engine.handle(
            at(Duration::ZERO),
            attacker,
            &forged,
            Token::from_bytes([9; 8]),
        );
        assert!(!is_reg_ok(&out), "case={name}");
    };

    // The victim's first REGISTER: zero cookie, answered by a COOKIE, never recorded.
    let first = wire(engine.relay_id, &victim, [1; 16], 1, [0; 16]);
    let out = engine.handle(at(Duration::ZERO), home, &first, Token::from_bytes([1; 8]));
    assert!(!is_reg_ok(&out));
    steal(&mut engine, &first, "zero cookie");
    assert!(!engine.table.knows(victim.node_id()), "case=zero cookie");

    // The victim's REGISTER with its own valid cookie, after it registered.
    let second = register_wire(&engine, &victim, [2; 16], 1, home, 1);
    let out = engine.handle(at(Duration::ZERO), home, &second, Token::from_bytes([2; 8]));
    assert!(is_reg_ok(&out));
    let captured = register_wire(&engine, &victim, [3; 16], 1, home, 1);
    steal(&mut engine, &captured, "victim's cookie");
    let reg = engine.table.lookup_node(victim.node_id(), Duration::ZERO);
    assert_eq!(reg.map(|r| r.addr()), Some(home), "case=victim's cookie");
}

/// Invariant 7.
#[test]
fn register_without_a_valid_cookie_gets_one_and_changes_nothing() {
    let mut engine = engine(DEFAULT_TTL);
    let a = key(1);
    let src = addr(1, 1);
    for (name, cookie) in [("none", [0; 16]), ("forged", [9; 16])] {
        let w = wire(engine.relay_id, &a, [5; 16], 1, cookie);
        let out = engine.handle(at(Duration::ZERO), src, &w, Token::from_bytes([1; 8]));
        let EngineOutput::Reply(t) = out else {
            panic!("expected COOKIE, case={name}");
        };
        assert_eq!(t.dst(), src);
        let want = RelayMsg::Cookie {
            cookie: engine.cookies.issue(src, 1),
            nonce: [5; 16],
        };
        assert_eq!(decode(t.bytes()), Ok(want), "case={name}");
    }
    assert!(!engine.table.knows(a.node_id()));
    assert_eq!(engine.guard.len(), 0);
    // The same REGISTER with the cookie now registers.
    let w = register_wire(&engine, &a, [5; 16], 1, src, 1);
    assert!(is_reg_ok(&engine.handle(
        at(Duration::ZERO),
        src,
        &w,
        Token::from_bytes([1; 8])
    )));
}

/// Invariant 8: one key from many real addresses keeps one replay entry and one registration,
/// so even a relay with room for two nodes still admits another one.
#[test]
fn a_register_flood_from_one_key_never_blocks_another_node() {
    let mut engine = engine(DEFAULT_TTL);
    engine.max_nodes = 2;
    let flooder = key(1);
    let now = Now::new(Duration::ZERO, 1_000);
    for n in 0..1_000u32 {
        let [_, _, hi, lo] = n.to_be_bytes();
        let src = SocketAddr::from(([10, 0, hi, lo], 1));
        let ts = 970 + u64::from(n % 61);
        let mut nonce = [0u8; 16];
        nonce[..4].copy_from_slice(&n.to_be_bytes());
        let w = register_wire(&engine, &flooder, nonce, ts, src, 1_000);
        engine.handle(now, src, &w, Token::from_bytes([lo, hi, 0, 0, 0, 0, 0, 1]));
    }
    assert_eq!(engine.guard.len(), 1, "one entry per key");
    assert_eq!(engine.table.nodes(), 1, "one registration per key");
    let honest = register_wire(&engine, &key(2), [7; 16], 1_000, addr(2, 2), 1_000);
    let out = engine.handle(now, addr(2, 2), &honest, Token::from_bytes([7; 8]));
    assert!(is_reg_ok(&out));
}

/// Invariant 8, rule 20, through the engine: past its burst a source gets no `REG_OK`.
#[test]
fn a_source_over_its_rate_gets_no_reg_ok() {
    let mut engine = engine(DEFAULT_TTL);
    let (a, src) = (key(1), addr(1, 1));
    let now = |ms| Now::new(Duration::from_millis(ms), 1_000);
    // A higher ts each time, so only the rate limit can refuse.
    let mut try_at = |ms: u64, ts: u64| {
        let w = register_wire(&engine, &a, [1; 16], ts, src, 1_000);
        engine.handle(now(ms), src, &w, Token::from_bytes(ts.to_be_bytes()))
    };
    let passed = (0..21).filter(|&n| is_reg_ok(&try_at(0, 975 + n))).count();
    assert_eq!(passed, 20, "burst 20");
    assert_eq!(try_at(50, 996), EngineOutput::Drop, "over the rate");
    assert!(is_reg_ok(&try_at(100, 997)), "refilled after 100 ms");
    let other = register_wire(&engine, &key(2), [2; 16], 1_000, addr(2, 2), 1_000);
    let out = engine.handle(now(50), addr(2, 2), &other, Token::from_bytes([2; 8]));
    assert!(is_reg_ok(&out), "another source is not limited");
}

fn keepalive_wire(token: Token) -> Vec<u8> {
    let mut wire = Vec::new();
    encode(&RelayMsg::Keepalive { token }, &mut wire);
    wire
}

/// Rules 11 and 23: a 9-byte `KEEPALIVE` with a live token, from any address, gets nothing.
#[test]
fn a_short_keepalive_gets_no_pong() {
    let mut engine = engine(DEFAULT_TTL);
    let tok = register(&mut engine, &key(1), addr(1, 1), Token::from_bytes([1; 8]));
    let mut short = keepalive_wire(tok);
    short.truncate(9);
    for (name, src) in [("bound address", addr(1, 1)), ("victim", addr(7, 7))] {
        let out = engine.handle(at(Duration::ZERO), src, &short, Token::from_bytes([9; 8]));
        assert_eq!(out, EngineOutput::Drop, "case={name}");
    }
    let full = engine.handle(
        at(Duration::ZERO),
        addr(7, 7),
        &keepalive_wire(tok),
        Token::from_bytes([9; 8]),
    );
    assert!(
        matches!(full, EngineOutput::Reply(_)),
        "a full one still gets its PONG"
    );
}

/// Rule 23, invariant 9: measured on the engine's real replies, not on the layout table.
#[test]
fn no_reply_is_bigger_than_its_request() {
    let mut engine = engine(DEFAULT_TTL);
    let (a, src) = (key(1), addr(1, 1));
    let v6 = SocketAddr::new("2001:db8::1".parse().unwrap_or(src.ip()), 9);
    let size = |out: &EngineOutput| match out {
        EngineOutput::Reply(t) => t.bytes().len(),
        other => panic!("expected a reply, got {other:?}"),
    };
    let mut cases = Vec::new();

    let bare = wire(engine.relay_id, &a, [1; 16], 1, [0; 16]);
    let out = engine.handle(at(Duration::ZERO), v6, &bare, Token::from_bytes([1; 8]));
    cases.push(("COOKIE", size(&out), bare.len(), 33));

    let reg = register_wire(&engine, &a, [2; 16], 1, v6, 1);
    let out = engine.handle(at(Duration::ZERO), v6, &reg, Token::from_bytes([2; 8]));
    assert!(is_reg_ok(&out));
    cases.push(("REG_OK", size(&out), reg.len(), 45));

    let ka = keepalive_wire(Token::from_bytes([2; 8]));
    let out = engine.handle(at(Duration::ZERO), v6, &ka, Token::from_bytes([3; 8]));
    cases.push(("PONG", size(&out), ka.len(), 19));

    for (name, reply, request, want) in cases {
        assert_eq!(reply, want, "case={name}");
        assert!(reply <= request, "case={name}: {reply} > {request}");
    }
    assert_eq!(reg.len(), 137, "REGISTER size in rule 21");
}
