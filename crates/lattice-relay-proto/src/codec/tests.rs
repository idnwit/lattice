//! `codec` unit tests.

use super::*;

#[test]
fn relay_and_quic_split_on_first_byte() {
    let cases: [(&[u8], Datagram); 5] = [
        (&[], Datagram::Unknown),
        (&[0x00], Datagram::Relay),
        (&[0x3F], Datagram::Relay),
        (&[0x40], Datagram::Quic),
        (&[0xC3], Datagram::Quic),
    ];
    for (bytes, want) in cases {
        assert_eq!(classify(bytes), want, "case={bytes:?}");
    }
}

#[test]
fn keepalive_roundtrips() {
    let msg = RelayMsg::Keepalive {
        token: Token::from_bytes([1; 8]),
    };
    let mut wire = Vec::new();
    encode(&msg, &mut wire);
    assert_eq!(wire.len(), 9);
    assert_eq!(decode(&wire), Ok(msg));
}

#[test]
fn error_codes_are_stable_and_namespaced() {
    let all = [
        CodecError::UnknownType,
        CodecError::Truncated,
        CodecError::BadNodeId,
    ];
    let want = ["relay.unknown_type", "relay.truncated", "relay.bad_node_id"];
    assert_eq!(all.map(|e| e.code()), want);
}

#[test]
fn unknown_relay_type_is_rejected_not_panicking() {
    assert_eq!(decode(&[0x3F, 1, 2]), Err(CodecError::UnknownType));
    assert_eq!(
        decode(&[MsgKind::Send as u8, 1]),
        Err(CodecError::Truncated)
    );
}

fn id(seed: u8) -> NodeId {
    use lattice_key::{NodeKey, SecretKeyBytes};
    NodeKey::from_secret(SecretKeyBytes::new([seed; 32])).node_id()
}

/// Concatenates byte slices: the hand-written oracle for each layout.
fn cat(parts: &[&[u8]]) -> Vec<u8> {
    parts.concat()
}

/// 01-transport.md, Relay invariant 6. Built by hand from the spec table, not from `encode`.
#[test]
#[allow(
    clippy::too_many_lines,
    reason = "one table row per message type, kept together"
)]
fn every_message_has_its_spec_layout() {
    let (a, b) = (id(1), id(2));
    let tok = Token::from_bytes([0xAA; 8]);
    let v4 = ObservedAddr::new(SocketAddr::from(([203, 0, 113, 5], 0x1234)));
    let v4_wire: &[u8] = &[
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xFF, 0xFF, 203, 0, 113, 5, 0x12, 0x34,
    ];
    let sig = Signature::from_bytes([0x55; 64]);
    let cases: Vec<(&str, RelayMsg<'_>, Vec<u8>)> = vec![
        (
            "register",
            RelayMsg::Register {
                node_id: a,
                nonce: [7; 16],
                ts: 0x0102_0304_0506_0708,
                sig,
            },
            cat(&[
                &[0x01],
                a.as_bytes(),
                &[7; 16],
                &[1, 2, 3, 4, 5, 6, 7, 8],
                &[0x55; 64],
            ]),
        ),
        (
            "reg_ok",
            RelayMsg::RegOk {
                token: tok,
                observed: v4,
                ttl: 0x005A,
                nonce: [9; 16],
            },
            cat(&[&[0x02], &[0xAA; 8], v4_wire, &[0x00, 0x5A], &[9; 16]]),
        ),
        (
            "keepalive",
            RelayMsg::Keepalive { token: tok },
            cat(&[&[0x03], &[0xAA; 8]]),
        ),
        (
            "pong",
            RelayMsg::Pong { observed: v4 },
            cat(&[&[0x04], v4_wire]),
        ),
        (
            "send",
            RelayMsg::Send {
                token: tok,
                dst: b,
                packet: &[0x40, 1],
            },
            cat(&[&[0x10], &[0xAA; 8], b.as_bytes(), &[0x40, 1]]),
        ),
        (
            "recv",
            RelayMsg::Recv {
                src: a,
                packet: &[0x40, 1],
            },
            cat(&[&[0x11], a.as_bytes(), &[0x40, 1]]),
        ),
        (
            "punch_req",
            RelayMsg::PunchReq {
                token: tok,
                dst: b,
                candidates: &[3, 4],
            },
            cat(&[&[0x20], &[0xAA; 8], b.as_bytes(), &[3, 4]]),
        ),
        (
            "punch_go",
            RelayMsg::PunchGo {
                peer: b,
                candidates: &[3, 4],
                punch_nonce: [6; 16],
            },
            cat(&[&[0x21], b.as_bytes(), &[3, 4], &[6; 16]]),
        ),
    ];
    for (name, msg, want) in cases {
        let mut wire = Vec::new();
        encode(&msg, &mut wire);
        assert_eq!(wire, want, "case={name}");
        assert_eq!(decode(&want), Ok(msg), "case={name}");
    }
    assert_eq!(SEND_HEADER_LEN, 1 + 8 + 32);
    assert_eq!(RECV_HEADER_LEN, 1 + 32);
}

#[test]
fn bytes_after_a_fixed_size_message_are_ignored() {
    let msg = RelayMsg::Keepalive {
        token: Token::from_bytes([1; 8]),
    };
    let mut wire = Vec::new();
    encode(&msg, &mut wire);
    wire.extend_from_slice(&[0xEE, 0xEE]);
    assert_eq!(decode(&wire), Ok(msg));
}
