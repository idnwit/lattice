//! Virtual peer addresses: what quinn sees instead of a real path (01-transport.md, Paths rules 8–9).

use lattice_key::NodeId;
use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

/// `240.0.0.0/4`: class E, never routed (RFC 1112).
const PREFIX: u32 = 0xF000_0000;
const HOST_MASK: u32 = 0x0FFF_FFFF;

/// Most peers one socket maps (01-transport.md, Paths rule 9; proposed). A `RECV` can name
/// any NodeId, so the map is bounded: past this, new peers are dropped.
pub(crate) const MAX_PEERS: usize = 4_096;
/// Ports tried after a taken address before the peer is dropped (rule 9; proposed).
const MAX_PROBES: u16 = 16;

/// Two-way map `NodeId ↔ virtual address`. Entries are never removed: a peer keeps its
/// address for the life of the socket, so quinn's view of a connection never changes.
#[derive(Debug, Default)]
pub(crate) struct VirtualAddrs {
    by_node: BTreeMap<NodeId, SocketAddr>,
    by_addr: BTreeMap<SocketAddr, NodeId>,
}

impl VirtualAddrs {
    /// The address of `node`, assigned on first use. `None` when the map is full or
    /// [`MAX_PROBES`] ports after the derived one are taken.
    pub(crate) fn addr_of(&mut self, node: NodeId) -> Option<SocketAddr> {
        if let Some(addr) = self.by_node.get(&node) {
            return Some(*addr);
        }
        if self.by_node.len() >= MAX_PEERS {
            return None;
        }
        let mut addr = derive(node);
        // Rule 9: a taken address moves to the next port up (port 0 skipped), a bounded number of times.
        for _ in 0..=MAX_PROBES {
            if !self.by_addr.contains_key(&addr) {
                self.by_node.insert(node, addr);
                self.by_addr.insert(addr, node);
                return Some(addr);
            }
            let port = match addr.port().wrapping_add(1) {
                0 => 1,
                p => p,
            };
            addr.set_port(port);
        }
        None
    }

    /// The peer behind a virtual address. Accepts the v4-mapped form an IPv6 socket uses.
    pub(crate) fn node_at(&self, addr: SocketAddr) -> Option<NodeId> {
        let canon = SocketAddr::new(addr.ip().to_canonical(), addr.port());
        self.by_addr.get(&canon).copied()
    }
}

/// Rule 8: IP from `node_id[0..4]` under the prefix, port from `node_id[4..6]`.
fn derive(node: NodeId) -> SocketAddr {
    let b = node.as_bytes();
    let ip = u32::from_be_bytes([b[0], b[1], b[2], b[3]]) & HOST_MASK | PREFIX;
    let port = match u16::from_be_bytes([b[4], b[5]]) {
        0 => 1,
        p => p,
    };
    SocketAddr::new(IpAddr::V4(Ipv4Addr::from(ip)), port)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lattice_key::{NodeKey, SecretKeyBytes};
    use std::net::Ipv6Addr;

    fn id(seed: u8) -> NodeId {
        NodeKey::from_secret(SecretKeyBytes::new([seed; 32])).node_id()
    }

    #[test]
    fn virtual_address_follows_the_node_id_under_class_e() {
        let node = id(1);
        let b = node.as_bytes();
        let mut v = VirtualAddrs::default();
        let addr = v.addr_of(node).unwrap();
        let SocketAddr::V4(a) = addr else {
            panic!("expected IPv4, got {addr}");
        };
        let o = a.ip().octets();
        assert_eq!(o[0] >> 4, 0xF, "class E");
        assert_eq!(
            [o[0] & 0x0F, o[1], o[2], o[3]],
            [b[0] & 0x0F, b[1], b[2], b[3]]
        );
        assert_eq!(a.port(), u16::from_be_bytes([b[4], b[5]]).max(1));
        assert_eq!(v.addr_of(node), Some(addr), "stable");
        assert_eq!(v.node_at(addr), Some(node));
    }

    #[test]
    fn a_v4_mapped_address_finds_the_same_peer() {
        let mut v = VirtualAddrs::default();
        let addr = v.addr_of(id(2)).unwrap();
        let SocketAddr::V4(a) = addr else { panic!() };
        let mapped = SocketAddr::new(IpAddr::V6(a.ip().to_ipv6_mapped()), a.port());
        assert_eq!(v.node_at(mapped), Some(id(2)));
        assert_eq!(
            v.node_at(SocketAddr::new(Ipv6Addr::LOCALHOST.into(), 1)),
            None
        );
    }

    #[test]
    fn a_taken_address_moves_to_the_next_port() {
        let mut v = VirtualAddrs::default();
        let first = v.addr_of(id(3)).unwrap();
        // Plant another node on the address id(4) would derive.
        let want = derive(id(4));
        v.by_addr.insert(want, id(3));
        let got = v.addr_of(id(4)).unwrap();
        assert_eq!(got.ip(), want.ip());
        assert_ne!(got, want);
        assert_ne!(got, first);
        assert_eq!(v.node_at(got), Some(id(4)));
    }

    fn many(n: u16) -> NodeId {
        let mut secret = [0u8; 32];
        secret[..2].copy_from_slice(&n.to_be_bytes());
        NodeKey::from_secret(SecretKeyBytes::new(secret)).node_id()
    }

    #[test]
    fn probing_a_crowded_address_is_bounded() {
        let mut v = VirtualAddrs::default();
        let base = derive(id(5));
        for k in 0..=MAX_PROBES {
            let mut a = base;
            a.set_port(base.port().wrapping_add(k).max(1));
            v.by_addr.insert(a, id(6));
        }
        assert_eq!(v.addr_of(id(5)), None);
    }

    #[test]
    fn a_full_map_drops_new_peers_and_keeps_known_ones() {
        let mut v = VirtualAddrs::default();
        let mut n = 0u16;
        while v.by_node.len() < MAX_PEERS {
            v.addr_of(many(n));
            n += 1;
        }
        let known = v.addr_of(many(0));
        assert!(known.is_some());
        assert_eq!(v.addr_of(many(n)), None);
    }
}
