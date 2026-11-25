//! Byte layout of relay messages (01-transport.md, Relay wire format).

use lattice_key::{NodeId, Signature};
use std::net::SocketAddr;

/// Bytes a `SEND` adds before the QUIC packet: type 1 + token 8 + dst 32.
/// quinn's max UDP payload is capped so packet + header fits (01, Paths rule 7).
pub const SEND_HEADER_LEN: usize = 41;

/// What the first byte of a datagram says it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Datagram {
    /// `0x00–0x3F`: relay message.
    Relay,
    /// `0x40` or `0x80` bit set: QUIC packet.
    Quic,
    /// Empty datagram.
    Unknown,
}

/// Classifies a datagram by its first byte.
#[allow(
    clippy::todo,
    unused_variables,
    reason = "L1 skeleton: body lands with its contract test"
)]
pub fn classify(bytes: &[u8]) -> Datagram {
    // empty: Unknown; first byte <= 0x3F: Relay; else Quic
    todo!()
}

/// Relay message type byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum MsgKind {
    /// `REGISTER`
    Register = 0x01,
    /// `REG_OK`
    RegOk = 0x02,
    /// `KEEPALIVE`
    Keepalive = 0x03,
    /// `PONG`
    Pong = 0x04,
    /// `SEND`
    Send = 0x10,
    /// `RECV`
    Recv = 0x11,
    /// `PUNCH_REQ`
    PunchReq = 0x20,
    /// `PUNCH_GO`
    PunchGo = 0x21,
}

/// Registration token: binds a node to its source address on the relay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Token([u8; 8]);

impl Token {
    /// Wraps 8 token bytes.
    pub fn from_bytes(bytes: [u8; 8]) -> Self {
        Self(bytes)
    }

    /// The raw token.
    #[allow(dead_code, reason = "L1 skeleton: read by encode once implemented")]
    pub(crate) fn to_bytes(self) -> [u8; 8] {
        self.0
    }
}

/// An address as the relay saw it: 16-byte IPv6 (v4-mapped) + 2-byte port.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObservedAddr(SocketAddr);

impl ObservedAddr {
    /// Wraps an address.
    pub fn new(addr: SocketAddr) -> Self {
        Self(addr)
    }

    /// The address.
    pub fn addr(self) -> SocketAddr {
        self.0
    }
}

/// One decoded relay message. Packet bytes borrow from the datagram.
///
/// TBD: units of `ts` (ms since UNIX epoch?) and `ttl` (seconds?).
/// TBD: encoding of hole-punch candidate lists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelayMsg<'a> {
    /// Node → relay. `sig = ed25519(relay_id || nonce || ts)`.
    Register {
        /// Registering node.
        node_id: NodeId,
        /// Fresh random nonce.
        nonce: [u8; 16],
        /// Node clock.
        ts: u64,
        /// Signature by `node_id`.
        sig: Signature,
    },
    /// Relay → node.
    RegOk {
        /// Token for later messages.
        token: Token,
        /// Source address the relay saw.
        observed: ObservedAddr,
        /// Registration lifetime.
        ttl: u16,
    },
    /// Node → relay, every 25 s.
    Keepalive {
        /// Registration token.
        token: Token,
    },
    /// Relay → node. Detects NAT rebinding.
    Pong {
        /// Source address the relay saw.
        observed: ObservedAddr,
    },
    /// Node → relay: forward `packet` to `dst`.
    Send {
        /// Sender token, bound to its source address.
        token: Token,
        /// Destination node.
        dst: NodeId,
        /// Opaque QUIC packet.
        packet: &'a [u8],
    },
    /// Relay → node: `packet` from `src`.
    Recv {
        /// Source node.
        src: NodeId,
        /// Opaque QUIC packet.
        packet: &'a [u8],
    },
    /// Node → relay: start hole punching with `dst`.
    PunchReq {
        /// Sender token.
        token: Token,
        /// Peer to punch to.
        dst: NodeId,
        /// Raw candidate list (format TBD).
        candidates: &'a [u8],
    },
    /// Relay → both nodes: punch now.
    PunchGo {
        /// The other side.
        peer: NodeId,
        /// Its raw candidate list (format TBD).
        candidates: &'a [u8],
        /// Shared probe nonce.
        punch_nonce: [u8; 16],
    },
}

/// Error from decoding a relay datagram. Never holds peer bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodecError {
    /// Unknown type byte in the relay range: drop silently (rule 5).
    UnknownType,
    /// Datagram shorter than its fixed fields.
    Truncated,
    /// A NodeId field is not a valid key.
    BadNodeId,
}

impl CodecError {
    /// Stable code for logs.
    pub fn code(&self) -> &'static str {
        match self {
            Self::UnknownType => "relay.unknown_type",
            Self::Truncated => "relay.truncated",
            Self::BadNodeId => "relay.bad_node_id",
        }
    }
}

/// Appends the wire form of `msg` to `out`.
#[allow(
    clippy::todo,
    unused_variables,
    clippy::ptr_arg,
    reason = "L1 skeleton: body lands with its contract test"
)]
pub fn encode(msg: &RelayMsg<'_>, out: &mut Vec<u8>) {
    // push MsgKind byte, then fixed fields in table order, then the tail slice
    todo!()
}

/// Decodes one relay datagram. Never panics on any input (fuzz target).
///
/// # Errors
///
/// [`CodecError`] on unknown type, short input, or a bad NodeId.
#[allow(
    clippy::todo,
    unused_variables,
    reason = "L1 skeleton: body lands with its contract test"
)]
pub fn decode(bytes: &[u8]) -> Result<RelayMsg<'_>, CodecError> {
    // split first byte; map to MsgKind or UnknownType
    // read fixed fields with checked splits (Truncated); NodeId::try_from_bytes (BadNodeId)
    // remaining bytes are the packet / candidates tail
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "contract: lands with classify"]
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
    #[ignore = "contract: lands with encode / decode"]
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
    #[ignore = "contract: lands with decode"]
    fn unknown_relay_type_is_rejected_not_panicking() {
        assert_eq!(decode(&[0x3F, 1, 2]), Err(CodecError::UnknownType));
        assert_eq!(
            decode(&[MsgKind::Send as u8, 1]),
            Err(CodecError::Truncated)
        );
    }
}
