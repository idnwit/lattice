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

#[test]
fn base32_decoding_is_canonical_only() {
    // `f` is `my`: the last 2 bits of `y` are spare and must be zero.
    let cases = [
        ("one byte", "my", Some(&b"f"[..])),
        ("non-zero spare bits", "mz", None),
        ("length 1 mod 8", "m", None),
        ("length 3 mod 8", "mzx", None),
        ("length 6 mod 8", "mzxw6y", None),
        ("padding", "my======", None),
    ];
    for (name, text, want) in cases {
        assert_eq!(base32_decode(text).as_deref(), want, "case={name}");
    }
}

#[test]
fn an_invite_code_with_stray_bits_is_malformed() {
    // The first of these whose code ends with spare bits (not every payload length does).
    let inv = ["10.0.0.2:9", "10.0.0.2:300", "10.0.0.2:20000"]
        .into_iter()
        .map(invite)
        .find(|i| !((i.encode().len() - INVITE_PREFIX.len()) * 5).is_multiple_of(8))
        .expect("one payload length leaves spare bits");
    let good = inv.encode();
    // Set the lowest spare bit of the last symbol: same bytes, another spelling.
    let last = good.as_bytes()[good.len() - 1];
    let v = BASE32.iter().position(|&s| s == last).unwrap();
    let mut stray = good[..good.len() - 1].to_owned();
    stray.push(char::from(BASE32[v | 1]));
    assert_ne!(stray, good);
    assert_eq!(Invite::decode(&stray), Err(InviteError::Malformed));
    assert_eq!(Invite::decode(&good), Ok(inv));
}

#[test]
fn peer_names_drop_controls_and_format_chars_and_are_capped() {
    let long = "x".repeat(NAME_MAX_CHARS + 10);
    let capped = "x".repeat(NAME_MAX_CHARS);
    let cases = [
        ("plain", "build-01", "build-01"),
        ("newline", "evil\nname", "evil name"),
        ("bidi override", "gpu\u{202e}10-ddab", "gpu10-ddab"),
        ("zero width and bom", "\u{feff}a\u{200b}b\u{2066}", "ab"),
        ("soft hyphen", "a\u{ad}b", "ab"),
        ("tag characters", "x\u{e0041}y", "xy"),
        ("trimmed", "  deck \r", "deck"),
        ("capped", long.as_str(), capped.as_str()),
        ("multibyte kept", "máy-chủ", "máy-chủ"),
    ];
    for (name, input, want) in cases {
        assert_eq!(one_line_name(input), want, "case={name}");
    }
}
