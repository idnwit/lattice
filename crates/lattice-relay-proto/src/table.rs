//! Relay registration table (01-transport.md, Relay rules 1–2).

use crate::codec::Token;
use lattice_key::NodeId;
use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::time::Duration;

/// One live registration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Registration {
    node_id: NodeId,
    addr: SocketAddr,
    expires: Duration,
}

impl Registration {
    /// Registered node.
    pub(crate) fn node_id(&self) -> NodeId {
        self.node_id
    }

    /// Source address bound to the token.
    pub(crate) fn addr(&self) -> SocketAddr {
        self.addr
    }

    fn live(&self, now: Duration) -> bool {
        now < self.expires
    }
}

/// `token → (node_id, src addr, expiry)`. Deterministic: `BTreeMap`, injected time.
#[derive(Debug, Default)]
pub(crate) struct TokenTable {
    by_token: BTreeMap<Token, Registration>,
    by_node: BTreeMap<NodeId, Token>,
}

impl TokenTable {
    /// An empty table.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// True when `token` is bound to any registration, live or not yet expired out.
    pub(crate) fn contains(&self, token: Token) -> bool {
        self.by_token.contains_key(&token)
    }

    /// Adds or refreshes `node_id` at `addr` until `now + ttl`.
    ///
    /// A node re-registering from a new address replaces its old binding.
    pub(crate) fn register(
        &mut self,
        node_id: NodeId,
        addr: SocketAddr,
        token: Token,
        now: Duration,
        ttl: Duration,
    ) {
        if let Some(old) = self.by_node.insert(node_id, token) {
            self.by_token.remove(&old);
        }
        let expires = now.saturating_add(ttl);
        self.by_token.insert(
            token,
            Registration {
                node_id,
                addr,
                expires,
            },
        );
    }

    /// True when `node` has a registration, live or not yet expired out.
    pub(crate) fn knows(&self, node: NodeId) -> bool {
        self.by_node.contains_key(&node)
    }

    /// Registered nodes.
    pub(crate) fn nodes(&self) -> usize {
        self.by_node.len()
    }

    /// The registration for `token`, only when `src` is its bound address (anti-spoof).
    ///
    /// Compares IP and port only: IPv6 flow info may change between datagrams.
    pub(crate) fn authorize(
        &self,
        token: Token,
        src: SocketAddr,
        now: Duration,
    ) -> Option<&Registration> {
        self.live(token, now).filter(|r| same_endpoint(r.addr, src))
    }

    /// The live registration for `token`, from any address.
    pub(crate) fn live(&self, token: Token, now: Duration) -> Option<&Registration> {
        self.by_token.get(&token).filter(|r| r.live(now))
    }

    /// Where `node_id` is reachable now.
    pub(crate) fn lookup_node(&self, node_id: NodeId, now: Duration) -> Option<&Registration> {
        let token = self.by_node.get(&node_id)?;
        self.by_token.get(token).filter(|r| r.live(now))
    }

    /// Extends a registration on keepalive.
    pub(crate) fn refresh(&mut self, token: Token, now: Duration, ttl: Duration) {
        if let Some(r) = self.by_token.get_mut(&token) {
            r.expires = now.saturating_add(ttl);
        }
    }

    /// Drops expired entries. Call on a timer.
    pub(crate) fn expire(&mut self, now: Duration) {
        let by_node = &mut self.by_node;
        self.by_token.retain(|token, r| {
            let keep = r.live(now);
            if !keep && by_node.get(&r.node_id) == Some(token) {
                by_node.remove(&r.node_id);
            }
            keep
        });
    }
}

/// Same IP (v4-mapped equals v4) and port.
fn same_endpoint(a: SocketAddr, b: SocketAddr) -> bool {
    a.ip().to_canonical() == b.ip().to_canonical() && a.port() == b.port()
}

#[cfg(test)]
mod tests {
    use super::*;
    use lattice_key::{NodeKey, SecretKeyBytes};

    fn node(seed: u8) -> NodeId {
        NodeKey::from_secret(SecretKeyBytes::new([seed; 32])).node_id()
    }

    fn addr(port: u16) -> SocketAddr {
        SocketAddr::from(([192, 0, 2, 1], port))
    }

    const TTL: Duration = Duration::from_secs(60);

    #[test]
    fn send_accepted_only_from_the_tokens_bound_address() {
        let mut t = TokenTable::new();
        let tok = Token::from_bytes([1; 8]);
        t.register(node(1), addr(1000), tok, Duration::ZERO, TTL);
        assert!(t.authorize(tok, addr(1000), Duration::ZERO).is_some());
        assert!(t.authorize(tok, addr(1001), Duration::ZERO).is_none());
    }

    #[test]
    fn expired_token_is_refused() {
        let mut t = TokenTable::new();
        let tok = Token::from_bytes([2; 8]);
        t.register(node(2), addr(1000), tok, Duration::ZERO, TTL);
        assert!(t.authorize(tok, addr(1000), TTL).is_none());
        t.expire(TTL);
        assert!(t.lookup_node(node(2), TTL).is_none());
    }

    #[test]
    fn keepalive_refresh_extends_the_registration() {
        let mut t = TokenTable::new();
        let tok = Token::from_bytes([3; 8]);
        t.register(node(3), addr(1000), tok, Duration::ZERO, TTL);
        t.refresh(tok, TTL - Duration::from_secs(1), TTL);
        let reg = t
            .authorize(tok, addr(1000), TTL)
            .map(|r| (r.node_id(), r.addr()));
        assert_eq!(reg, Some((node(3), addr(1000))));
    }
}
