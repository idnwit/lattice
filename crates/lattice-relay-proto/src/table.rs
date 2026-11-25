//! Relay registration table (01-transport.md, Relay rules 1–2).

use crate::codec::Token;
use lattice_key::NodeId;
use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::time::Duration;

/// One live registration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code, reason = "L1 skeleton: read by the engine once implemented")]
pub(crate) struct Registration {
    node_id: NodeId,
    addr: SocketAddr,
    expires: Duration,
}

#[allow(dead_code, reason = "L1 skeleton: read by the engine once implemented")]
impl Registration {
    /// Registered node.
    pub(crate) fn node_id(&self) -> NodeId {
        self.node_id
    }

    /// Source address bound to the token.
    pub(crate) fn addr(&self) -> SocketAddr {
        self.addr
    }
}

/// `token → (node_id, src addr, expiry)`. Deterministic: `BTreeMap`, injected time.
#[derive(Debug, Default)]
#[allow(dead_code, reason = "L1 skeleton: read once the table is implemented")]
pub(crate) struct TokenTable {
    by_token: BTreeMap<Token, Registration>,
    by_node: BTreeMap<NodeId, Token>,
}

#[allow(
    dead_code,
    reason = "L1 skeleton: called by the engine once implemented"
)]
impl TokenTable {
    /// An empty table.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Adds or refreshes `node_id` at `addr` until `now + ttl`. Returns its token.
    ///
    /// A node re-registering from a new address replaces its old binding.
    #[allow(
        clippy::todo,
        unused_variables,
        reason = "L1 skeleton: body lands with its contract test"
    )]
    pub(crate) fn register(
        &mut self,
        node_id: NodeId,
        addr: SocketAddr,
        token: Token,
        now: Duration,
        ttl: Duration,
    ) {
        // drop the node's previous token, if any
        // insert token → Registration { node_id, addr, expires: now + ttl } and node → token
        todo!()
    }

    /// The registration for `token`, only when `src` is its bound address (anti-spoof).
    #[allow(
        clippy::todo,
        unused_variables,
        reason = "L1 skeleton: body lands with its contract test"
    )]
    pub(crate) fn authorize(
        &self,
        token: Token,
        src: SocketAddr,
        now: Duration,
    ) -> Option<&Registration> {
        // lookup token; reject when expired or addr != src
        todo!()
    }

    /// Where `node_id` is reachable now.
    #[allow(
        clippy::todo,
        unused_variables,
        reason = "L1 skeleton: body lands with its contract test"
    )]
    pub(crate) fn lookup_node(&self, node_id: NodeId, now: Duration) -> Option<&Registration> {
        // by_node → by_token; skip expired
        todo!()
    }

    /// Extends a registration on keepalive.
    #[allow(
        clippy::todo,
        unused_variables,
        reason = "L1 skeleton: body lands with its contract test"
    )]
    pub(crate) fn refresh(&mut self, token: Token, now: Duration, ttl: Duration) {
        // set expires = now + ttl when the token exists
        todo!()
    }

    /// Drops expired entries. Call on a timer.
    #[allow(
        clippy::todo,
        unused_variables,
        reason = "L1 skeleton: body lands with its contract test"
    )]
    pub(crate) fn expire(&mut self, now: Duration) {
        // remove every registration with expires <= now from both maps
        todo!()
    }
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
    #[ignore = "contract: lands with TokenTable"]
    fn send_accepted_only_from_the_tokens_bound_address() {
        let mut t = TokenTable::new();
        let tok = Token::from_bytes([1; 8]);
        t.register(node(1), addr(1000), tok, Duration::ZERO, TTL);
        assert!(t.authorize(tok, addr(1000), Duration::ZERO).is_some());
        assert!(t.authorize(tok, addr(1001), Duration::ZERO).is_none());
    }

    #[test]
    #[ignore = "contract: lands with TokenTable"]
    fn expired_token_is_refused() {
        let mut t = TokenTable::new();
        let tok = Token::from_bytes([2; 8]);
        t.register(node(2), addr(1000), tok, Duration::ZERO, TTL);
        assert!(t.authorize(tok, addr(1000), TTL).is_none());
        t.expire(TTL);
        assert!(t.lookup_node(node(2), TTL).is_none());
    }

    #[test]
    #[ignore = "contract: lands with TokenTable"]
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
