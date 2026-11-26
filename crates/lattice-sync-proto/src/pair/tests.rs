use super::*;
use lattice_key::{NodeKey, SecretKeyBytes};

fn deck() -> NodeId {
    NodeKey::from_secret(SecretKeyBytes::new([5; 32])).node_id()
}

fn invite(addr: &str) -> Invite {
    Invite::new(
        deck(),
        addr.parse().unwrap(),
        PairSecret::new([7; 32]),
        1_800_000_000,
    )
}

#[test]
fn invite_code_roundtrips() {
    for addr in ["192.168.1.20:51234", "[fe80::1]:7441"] {
        let inv = invite(addr);
        let code = inv.encode();
        assert!(code.starts_with(INVITE_PREFIX), "case={addr}");
        assert!(code.len() <= MAX_CODE_LEN, "case={addr}");
        assert_eq!(Invite::decode(&code), Ok(inv.clone()), "case={addr}");
        let shouted = format!("  {}\n", code.to_uppercase());
        assert_eq!(Invite::decode(&shouted), Ok(inv), "case={addr} upper");
    }
}

#[test]
fn bad_invite_codes_are_refused() {
    let good = invite("10.0.0.2:9").encode();
    let body = &good[INVITE_PREFIX.len()..];
    let cases = [
        ("empty", String::new(), InviteError::Prefix),
        (
            "other version",
            format!("lattice2:{body}"),
            InviteError::Prefix,
        ),
        (
            "not base32",
            format!("{INVITE_PREFIX}{body}!"),
            InviteError::Malformed,
        ),
        (
            "truncated",
            good[..good.len() - 8].to_owned(),
            InviteError::Malformed,
        ),
        (
            "trailing bytes",
            format!("{good}aaaa"),
            InviteError::Malformed,
        ),
        (
            "too long",
            format!("{INVITE_PREFIX}{}", "a".repeat(600)),
            InviteError::Malformed,
        ),
    ];
    for (name, code, want) in cases {
        assert_eq!(Invite::decode(&code), Err(want), "case={name}");
    }
}

#[test]
fn base32_matches_rfc4648_vectors() {
    // RFC 4648 section 10, lower case, padding removed.
    let cases: [(&[u8], &str); 6] = [
        (b"f", "my"),
        (b"fo", "mzxq"),
        (b"foo", "mzxw6"),
        (b"foob", "mzxw6yq"),
        (b"fooba", "mzxw6ytb"),
        (b"foobar", "mzxw6ytboi"),
    ];
    for (bytes, text) in cases {
        let mut out = String::new();
        base32_encode(bytes, &mut out);
        assert_eq!(out, text, "case={text}");
        assert_eq!(base32_decode(text).as_deref(), Some(bytes), "case={text}");
    }
}

#[test]
fn secrets_compare_by_value_and_never_print() {
    let a = PairSecret::new([1; 32]);
    let mut other = [1; 32];
    other[31] = 2;
    assert!(a.matches(&PairSecret::new([1; 32])));
    assert!(!a.matches(&PairSecret::new(other)));
    let shown = format!("{a:?} {:?}", invite("10.0.0.2:9"));
    assert!(!shown.contains("[1, 1"), "shown={shown}");
    assert!(shown.contains("redacted"), "shown={shown}");
}

#[test]
fn an_invite_expires_at_its_deadline() {
    let inv = invite("10.0.0.2:9");
    assert!(!inv.expired_at(inv.expires() - 1));
    assert!(inv.expired_at(inv.expires()));
}

#[test]
fn error_codes_are_stable_and_namespaced() {
    assert_eq!(InviteError::Prefix.code(), "pair.invite_prefix");
    assert_eq!(InviteError::Malformed.code(), "pair.invite_malformed");
}
