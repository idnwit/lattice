//! Byte layout of relay messages (01-transport.md, Relay wire format).

use lattice_key::{NodeId, Signature};
use std::net::{IpAddr, Ipv6Addr, SocketAddr};

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

/// Highest first byte of a relay message (RFC 7983 split).
const RELAY_MAX: u8 = 0x3F;

/// Classifies a datagram by its first byte.
pub fn classify(bytes: &[u8]) -> Datagram {
    match bytes.first() {
        None => Datagram::Unknown,
        Some(&b) if b <= RELAY_MAX => Datagram::Relay,
        Some(_) => Datagram::Quic,
    }
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

impl MsgKind {
    fn from_byte(b: u8) -> Option<Self> {
        Some(match b {
            0x01 => Self::Register,
            0x02 => Self::RegOk,
            0x03 => Self::Keepalive,
            0x04 => Self::Pong,
            0x10 => Self::Send,
            0x11 => Self::Recv,
            0x20 => Self::PunchReq,
            0x21 => Self::PunchGo,
            _ => return None,
        })
    }
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
    pub(crate) fn to_bytes(self) -> [u8; 8] {
        self.0
    }
}

/// An address as the relay saw it: 16-byte IPv6 (v4-mapped) + 2-byte port.
///
/// A v4-mapped IPv6 address decodes as IPv4, so v4 and v4-mapped compare equal after a roundtrip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObservedAddr(SocketAddr);

impl ObservedAddr {
    /// Wraps an address in its wire form: v4-mapped becomes IPv4; IPv6 flow and scope drop.
    pub fn new(addr: SocketAddr) -> Self {
        Self(SocketAddr::new(addr.ip().to_canonical(), addr.port()))
    }

    /// The address.
    pub fn addr(self) -> SocketAddr {
        self.0
    }
}

/// One decoded relay message. Packet bytes borrow from the datagram.
///
/// TBD: encoding of hole-punch candidate lists (M2.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelayMsg<'a> {
    /// Node → relay. `sig = ed25519(relay_id || nonce || ts)`.
    Register {
        /// Registering node.
        node_id: NodeId,
        /// Fresh random nonce.
        nonce: [u8; 16],
        /// Node wall clock, seconds since the UNIX epoch.
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
        /// Registration lifetime in seconds.
        ttl: u16,
        /// Nonce of the `REGISTER` this answers (rule 15).
        nonce: [u8; 16],
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
pub fn encode(msg: &RelayMsg<'_>, out: &mut Vec<u8>) {
    match msg {
        RelayMsg::Register {
            node_id,
            nonce,
            ts,
            sig,
        } => {
            out.push(MsgKind::Register as u8);
            out.extend_from_slice(node_id.as_bytes());
            out.extend_from_slice(nonce);
            out.extend_from_slice(&ts.to_be_bytes());
            out.extend_from_slice(&sig.to_bytes());
        }
        RelayMsg::RegOk {
            token,
            observed,
            ttl,
            nonce,
        } => {
            out.push(MsgKind::RegOk as u8);
            out.extend_from_slice(&token.to_bytes());
            put_addr(*observed, out);
            out.extend_from_slice(&ttl.to_be_bytes());
            out.extend_from_slice(nonce);
        }
        RelayMsg::Keepalive { token } => {
            out.push(MsgKind::Keepalive as u8);
            out.extend_from_slice(&token.to_bytes());
        }
        RelayMsg::Pong { observed } => {
            out.push(MsgKind::Pong as u8);
            put_addr(*observed, out);
        }
        RelayMsg::Send { token, dst, packet } => {
            put_token_node(MsgKind::Send, *token, *dst, packet, out);
        }
        RelayMsg::Recv { src, packet } => {
            out.push(MsgKind::Recv as u8);
            out.extend_from_slice(src.as_bytes());
            out.extend_from_slice(packet);
        }
        RelayMsg::PunchReq {
            token,
            dst,
            candidates,
        } => put_token_node(MsgKind::PunchReq, *token, *dst, candidates, out),
        RelayMsg::PunchGo {
            peer,
            candidates,
            punch_nonce,
        } => {
            out.push(MsgKind::PunchGo as u8);
            out.extend_from_slice(peer.as_bytes());
            out.extend_from_slice(candidates);
            out.extend_from_slice(punch_nonce);
        }
    }
}

/// `[kind][token 8][node_id 32][tail]`: the shape of `SEND` and `PUNCH_REQ`.
fn put_token_node(kind: MsgKind, token: Token, node: NodeId, tail: &[u8], out: &mut Vec<u8>) {
    out.push(kind as u8);
    out.extend_from_slice(&token.to_bytes());
    out.extend_from_slice(node.as_bytes());
    out.extend_from_slice(tail);
}

/// Decodes one relay datagram. Never panics on any input (fuzz target).
///
/// # Errors
///
/// [`CodecError`] on unknown type, short input, or a bad NodeId.
pub fn decode(bytes: &[u8]) -> Result<RelayMsg<'_>, CodecError> {
    let (&kind, body) = bytes.split_first().ok_or(CodecError::Truncated)?;
    let kind = MsgKind::from_byte(kind).ok_or(CodecError::UnknownType)?;
    let mut r = Reader(body);
    Ok(match kind {
        MsgKind::Register => RelayMsg::Register {
            node_id: r.node_id()?,
            nonce: r.array()?,
            ts: u64::from_be_bytes(r.array()?),
            sig: Signature::from_bytes(r.array()?),
        },
        MsgKind::RegOk => RelayMsg::RegOk {
            token: r.token()?,
            observed: r.addr()?,
            ttl: u16::from_be_bytes(r.array()?),
            nonce: r.array()?,
        },
        MsgKind::Keepalive => RelayMsg::Keepalive { token: r.token()? },
        MsgKind::Pong => RelayMsg::Pong {
            observed: r.addr()?,
        },
        MsgKind::Send => RelayMsg::Send {
            token: r.token()?,
            dst: r.node_id()?,
            packet: r.0,
        },
        MsgKind::Recv => RelayMsg::Recv {
            src: r.node_id()?,
            packet: r.0,
        },
        MsgKind::PunchReq => RelayMsg::PunchReq {
            token: r.token()?,
            dst: r.node_id()?,
            candidates: r.0,
        },
        MsgKind::PunchGo => {
            let peer = r.node_id()?;
            let (candidates, punch_nonce) =
                r.0.split_last_chunk::<16>().ok_or(CodecError::Truncated)?;
            RelayMsg::PunchGo {
                peer,
                candidates,
                punch_nonce: *punch_nonce,
            }
        }
    })
}

/// Bytes `RECV` puts before the QUIC packet: type 1 + src 32.
pub(crate) const RECV_HEADER_LEN: usize = 33;

fn put_addr(addr: ObservedAddr, out: &mut Vec<u8>) {
    let ip = match addr.0.ip() {
        IpAddr::V4(v4) => v4.to_ipv6_mapped(),
        IpAddr::V6(v6) => v6,
    };
    out.extend_from_slice(&ip.octets());
    out.extend_from_slice(&addr.0.port().to_be_bytes());
}

/// Checked reads over the fields after the type byte.
struct Reader<'a>(&'a [u8]);

impl Reader<'_> {
    fn array<const N: usize>(&mut self) -> Result<[u8; N], CodecError> {
        let (head, rest) = self
            .0
            .split_first_chunk::<N>()
            .ok_or(CodecError::Truncated)?;
        self.0 = rest;
        Ok(*head)
    }

    fn node_id(&mut self) -> Result<NodeId, CodecError> {
        NodeId::try_from_bytes(self.array()?).map_err(|_| CodecError::BadNodeId)
    }

    fn token(&mut self) -> Result<Token, CodecError> {
        self.array().map(Token::from_bytes)
    }

    fn addr(&mut self) -> Result<ObservedAddr, CodecError> {
        let ip = Ipv6Addr::from(self.array::<16>()?);
        let port = u16::from_be_bytes(self.array()?);
        Ok(ObservedAddr::new(SocketAddr::new(IpAddr::V6(ip), port)))
    }
}

#[cfg(test)]
mod tests;
