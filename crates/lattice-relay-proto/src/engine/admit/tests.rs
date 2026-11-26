//! `admit` unit tests: cookies and the rate limit (rules 18–20).

use super::*;
use std::net::Ipv6Addr;

fn addr(last: u8, port: u16) -> SocketAddr {
    SocketAddr::from(([192, 0, 2, last], port))
}

#[test]
fn a_cookie_is_bound_to_its_address_and_two_buckets() {
    let c = Cookies::new([7; 32]);
    let t = 1_000_020; // bucket 33_334 starts at 1_000_020
    let cookie = c.issue(addr(1, 1), t);
    assert!(c.valid(addr(1, 1), t, cookie));
    assert!(c.valid(addr(1, 1), t + 59, cookie), "next bucket");
    assert!(!c.valid(addr(1, 1), t + 60, cookie), "two buckets on");
    assert!(!c.valid(addr(1, 2), t, cookie), "other port");
    assert!(!c.valid(addr(2, 1), t, cookie), "other ip");
    assert!(!c.valid(addr(1, 1), t, [0; 16]), "zero is no cookie");
    assert!(
        !Cookies::new([8; 32]).valid(addr(1, 1), t, cookie),
        "other secret"
    );
    let mapped = SocketAddr::new(
        Ipv6Addr::from([0, 0, 0, 0, 0, 0xFFFF, 0xC000, 0x0201]).into(),
        1,
    );
    assert!(c.valid(mapped, t, cookie), "v4-mapped is the same source");
}

#[test]
fn a_flooding_source_is_rate_limited() {
    let mut r = RateLimit::default();
    let ip = addr(1, 1).ip();
    let t0 = Duration::from_secs(10);
    let passed = (0..30).filter(|_| r.allow(ip, t0)).count();
    assert_eq!(passed, 20, "burst");
    assert!(!r.allow(ip, t0 + Duration::from_millis(50)));
    assert!(
        r.allow(ip, t0 + Duration::from_millis(100)),
        "10 per second refill"
    );
    assert!(
        r.allow(addr(2, 1).ip(), t0),
        "another source has its own bucket"
    );
}

fn v6(s: &str) -> IpAddr {
    s.parse().unwrap_or(IpAddr::V6(Ipv6Addr::UNSPECIFIED))
}

#[test]
fn an_ipv6_slash_64_is_one_source() {
    let mut r = RateLimit::default();
    let (a, b) = (v6("2001:db8:1:2::1"), v6("2001:db8:1:2:ffff::9"));
    let t = Duration::ZERO;
    for _ in 0..10 {
        assert!(r.allow(a, t));
        assert!(r.allow(b, t));
    }
    assert!(!r.allow(a, t), "one /64, one burst");
    assert!(
        r.allow(v6("2001:db8:1:3::1"), t),
        "another /64 is independent"
    );

    let v4 = addr(1, 1).ip();
    let mapped = v6("::ffff:192.0.2.1");
    for _ in 0..10 {
        assert!(r.allow(v4, t));
        assert!(r.allow(mapped, t));
    }
    assert!(!r.allow(v4, t), "v4-mapped spends the IPv4 bucket");
    assert!(!r.allow(mapped, t), "v4-mapped spends the IPv4 bucket");
}

#[test]
fn an_ipv6_slash_48_has_its_own_budget() {
    let mut r = RateLimit::default();
    let t = Duration::ZERO;
    let mut passed = 0;
    for net in 0..20u16 {
        let ip = IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 1, net, 0, 0, 0, 1));
        passed += (0..20).filter(|_| r.allow(ip, t)).count();
    }
    assert_eq!(passed, 200, "the /48 burst caps 20 full /64 bursts");
    assert!(
        r.allow(v6("2001:db8:2::1"), t),
        "another /48 has its own budget"
    );
    let fresh = Ipv6Addr::new(0x2001, 0xdb8, 1, 99, 0, 0, 0, 1);
    assert!(!r.allow(IpAddr::V6(fresh), t), "a new /64 in a spent /48");
    assert!(
        r.allow(IpAddr::V6(fresh), t + Duration::from_millis(10)),
        "100 per second refill"
    );

    let mut v4 = RateLimit::default();
    let mut passed = 0;
    for last in 0..15u8 {
        passed += (0..20).filter(|_| v4.allow(addr(last, 1).ip(), t)).count();
    }
    assert_eq!(passed, 300, "IPv4 has only the per-address tier");
}

#[test]
fn a_full_source_table_evicts_the_idle_most_source() {
    let mut r = RateLimit::default();
    r.set_max_sources(2);
    let (a, b, c) = (addr(1, 1).ip(), addr(2, 1).ip(), addr(3, 1).ip());
    let ms = Duration::from_millis;
    assert_eq!((0..20).filter(|_| r.allow(a, ms(0))).count(), 20);
    assert_eq!((0..20).filter(|_| r.allow(b, ms(10))).count(), 20);
    assert!(
        r.allow(a, ms(100)),
        "a is used again, so b is now the idle-most"
    );
    assert_eq!(
        r.sources.by_use.len(),
        r.sources(),
        "a refresh leaves no stale entry"
    );
    assert!(r.allow(c, ms(100)), "a new source is never locked out");
    assert_eq!(r.sources(), 2, "the table stays bounded");
    assert_eq!(r.sources.by_use.len(), r.sources(), "index and map agree");
    assert!(
        !r.allow(a, ms(130)),
        "insertion order would have evicted a; LRU keeps its spent bucket"
    );
    assert!(
        r.allow(b, ms(130)),
        "the idle-most source was evicted: it has a full bucket again"
    );
}

fn in_48(net: u16) -> IpAddr {
    IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 1, net, 0, 0, 0, 1))
}

#[test]
fn a_refused_register_creates_no_source_state() {
    let mut r = RateLimit::default();
    r.set_max_sources(30);
    let t = Duration::ZERO;
    let tracked = addr(1, 1).ip();
    assert!(r.allow(tracked, t));
    for net in 0..10u16 {
        assert_eq!((0..20).filter(|_| r.allow(in_48(net), t)).count(), 20);
    }
    assert!(!r.allow(in_48(10), t), "the /48 is exhausted");
    let (sources, wide) = (r.sources(), r.slash48.buckets.len());
    for net in 11..60u16 {
        assert!(!r.allow(in_48(net), t), "net={net}");
    }
    assert_eq!(r.sources(), sources, "no source bucket for a refused /64");
    assert_eq!(r.slash48.buckets.len(), wide);
    assert_eq!(r.sources.by_use.len(), sources);
    assert!(!r.sources.buckets.contains_key(&source_key(in_48(11))));
    assert!(
        r.sources.buckets.contains_key(&source_key(tracked)),
        "not evicted"
    );
}

#[test]
fn a_refusal_in_one_tier_charges_neither() {
    let mut r = RateLimit::default();
    let t = Duration::ZERO;
    assert_eq!(
        (0..40).filter(|_| r.allow(in_48(0), t)).count(),
        20,
        "the /64 burst"
    );
    // 20 of the /48's 200 are spent; the 20 refusals above must not have spent more.
    let mut others = 0;
    for net in 1..100u16 {
        others += (0..20).filter(|_| r.allow(in_48(net), t)).count();
    }
    assert_eq!(others, 180, "the /48 admits exactly its remaining budget");
}

#[test]
fn full_buckets_are_forgotten() {
    let mut r = RateLimit::default();
    let t = Duration::ZERO;
    assert!(r.allow(addr(1, 1).ip(), t));
    assert!(r.allow(v6("2001:db8::1"), t));
    r.expire(t + Duration::from_millis(5));
    assert_eq!(r.sources(), 2, "not refilled yet");
    assert_eq!(r.slash48.buckets.len(), 1, "not refilled yet");
    r.expire(t + Duration::from_millis(100));
    assert_eq!(r.sources(), 0);
    assert!(r.slash48.buckets.is_empty() && r.slash48.by_use.is_empty());
    assert!(r.sources.by_use.is_empty());
}

/// Independent oracle for rule 19: the expected bytes come from Python's `blake3` 1.0.11,
/// not from this crate. Input = `bucket 8 BE || v4-mapped ip 16 || port 2 BE`, 26 bytes:
///
/// ```text
/// python3 -m venv venv && ./venv/bin/pip install blake3
/// ./venv/bin/python -c 'import blake3,struct,ipaddress
/// key=bytes(range(32)); b=1_700_000_000//30
/// for ip,p in [("::ffff:192.0.2.1",4433),("2001:db8::1",443)]:
///     d=struct.pack(">Q",b)+ipaddress.IPv6Address(ip).packed+struct.pack(">H",p)
///     print(blake3.blake3(d,key=key).digest()[:16].hex())'
/// ```
#[test]
fn the_cookie_matches_the_rule_19_vector() {
    let mut secret = [0u8; 32];
    for (i, b) in (0u8..).zip(secret.iter_mut()) {
        *b = i;
    }
    let c = Cookies::new(secret);
    let unix = 1_700_000_000;
    let cases = [
        (
            "ipv4",
            SocketAddr::from(([192, 0, 2, 1], 4433)),
            0x7407_af79_3352_14f0_4b48_6abb_6944_e0b8_u128,
        ),
        (
            "ipv6",
            SocketAddr::new(v6("2001:db8::1"), 443),
            0xbdc0_e7c6_b439_0d22_1b7b_2112_8408_29cd_u128,
        ),
    ];
    for (name, src, want) in cases {
        assert_eq!(c.issue(src, unix), want.to_be_bytes(), "case={name}");
    }
}
