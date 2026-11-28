use proptest::prelude::*;

use super::*;
use crate::resolve::Source;

fn read_request(bytes: &[u8]) -> Result<Option<Incoming>, ControlError> {
    let mut r = RequestReader::new();
    r.push(bytes);
    r.next()
}

fn every_request() -> Vec<Request> {
    vec![
        Request::AddDeck {
            code: "lattice1:abc".to_owned(),
            name: Some("w".to_owned()),
        },
        Request::ConfigList,
        Request::ConfigGet {
            key: "relay.addr".to_owned(),
        },
        Request::ConfigSet {
            key: "relay.addr".to_owned(),
            value: "none".to_owned(),
        },
        Request::ConfigReset {
            key: "name".to_owned(),
        },
        Request::ConfigReplace {
            toml: "name = \"a\"\n".to_owned(),
            base_hash: file_hash(b""),
        },
    ]
}

#[test]
fn every_request_roundtrips() {
    let all = every_request();
    assert_eq!(all.len(), usize::try_from(REQUEST_TAGS).unwrap());
    for req in all {
        let mut wire = Vec::new();
        encode_request(&req, &mut wire).unwrap();
        assert_eq!(wire.first(), Some(&CONTROL_VERSION));
        // Byte by byte: a partial frame waits for more.
        let mut r = RequestReader::new();
        for (i, b) in wire.iter().enumerate() {
            let got = r.next().unwrap();
            assert_eq!(got, None, "case={req:?} at={i}");
            r.push(&[*b]);
        }
        assert_eq!(
            r.next().unwrap(),
            Some(Incoming::Request(req.clone())),
            "case={req:?}"
        );
    }
}

#[test]
fn replies_roundtrip() {
    let cases = [
        Reply::Config {
            entries: vec![Entry::new("name".to_owned(), "w".to_owned(), Source::File)],
        },
        Reply::failed("config.unknown_key"),
    ];
    for reply in cases {
        let mut wire = Vec::new();
        encode_reply(&reply, &mut wire).unwrap();
        let mut r = ReplyReader::new();
        r.push(&wire);
        assert_eq!(r.next().unwrap(), Some(reply.clone()), "case={reply:?}");
    }
}

/// specs/11-control-socket.md, Invariant 2 (codec half; the daemon half is in `latticed`).
#[test]
fn an_unknown_control_request_is_unsupported() {
    // Tag 6 (the first unknown one), then a 2-byte varint tag 200.
    for body in [&[6u8][..], &[0xC8, 0x01, 0xFF]] {
        let mut wire = vec![CONTROL_VERSION];
        wire.extend_from_slice(&u32::try_from(body.len()).unwrap().to_be_bytes());
        wire.extend_from_slice(body);
        assert_eq!(
            read_request(&wire),
            Ok(Some(Incoming::Unsupported)),
            "case={body:?}"
        );
    }
}

#[test]
fn another_version_byte_is_reported() {
    assert_eq!(
        read_request(&[2, 0, 0, 0, 1, 1]),
        Ok(Some(Incoming::BadVersion))
    );
    assert_eq!(read_request(&[]), Ok(None));
}

/// specs/11-control-socket.md, Invariant 3 (codec half).
#[test]
fn an_oversized_control_frame_fails_on_its_prefix() {
    let len = u32::try_from(MAX_CONTROL_FRAME + 1).unwrap();
    let mut wire = vec![CONTROL_VERSION];
    wire.extend_from_slice(&len.to_be_bytes());
    // No body byte has arrived yet.
    assert_eq!(read_request(&wire), Err(ControlError::TooLong));
    let mut out = Vec::new();
    let big = Request::ConfigReplace {
        toml: "x".repeat(MAX_CONTROL_FRAME),
        base_hash: [0; 32],
    };
    assert_eq!(encode_request(&big, &mut out), Err(ControlError::TooLong));
    assert!(out.is_empty(), "out unchanged on error");
}

#[test]
fn trailing_bytes_are_malformed() {
    let mut wire = Vec::new();
    encode_request(&Request::ConfigList, &mut wire).unwrap();
    // Grow the declared length by one and add a junk byte.
    wire[4] += 1;
    wire.push(0);
    assert_eq!(read_request(&wire), Err(ControlError::Malformed));
}

#[test]
fn add_deck_debug_never_shows_the_code() {
    let req = Request::AddDeck {
        code: "lattice1:SECRET".to_owned(),
        name: None,
    };
    assert!(!format!("{req:?}").contains("SECRET"));
}

proptest! {
    /// specs/11-control-socket.md, Invariant 1.
    #[test]
    fn control_decode_never_panics(bytes in proptest::collection::vec(any::<u8>(), 0..512)) {
        let mut req = RequestReader::new();
        req.push(&bytes);
        let _req = req.next();
        let mut versioned = RequestReader::new();
        versioned.push(&[CONTROL_VERSION]);
        versioned.push(&bytes);
        let _versioned = versioned.next();
        let mut reply = ReplyReader::new();
        reply.push(&bytes);
        let _reply = reply.next();
    }
}
