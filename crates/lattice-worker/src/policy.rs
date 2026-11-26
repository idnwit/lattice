//! Who may connect (03-identity-security.md, Access control rule 1).

use lattice_key::NodeId;
use std::collections::BTreeSet;

/// Which NodeIds may connect.
///
/// L1 stand-in for the signed device list. Replaced by `lattice-identity`
/// once pairing lands.
#[derive(Debug, Clone)]
pub struct PeerPolicy {
    mode: Mode,
}

#[derive(Debug, Clone)]
enum Mode {
    /// Only these nodes.
    List(BTreeSet<NodeId>),
    /// Any node: the M1 localhost experiment only.
    AnyLoopback,
}

impl PeerPolicy {
    /// Allows exactly these nodes.
    pub fn allow_list(nodes: Vec<NodeId>) -> Self {
        Self {
            mode: Mode::List(nodes.into_iter().collect()),
        }
    }

    /// Allows any node. **Experiment only**: use it only with a transport bound to loopback
    /// (`lattice-quic` direct mode refuses other binds), so every peer is a local process.
    /// Never ship it: access control (03-identity-security.md) replaces it with pairing.
    pub fn allow_any_loopback() -> Self {
        Self {
            mode: Mode::AnyLoopback,
        }
    }

    /// True when `peer` may connect.
    pub fn permits(&self, peer: &NodeId) -> bool {
        match &self.mode {
            Mode::List(allowed) => allowed.contains(peer),
            Mode::AnyLoopback => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lattice_key::{NodeKey, SecretKeyBytes};

    fn id(seed: u8) -> NodeId {
        NodeKey::from_secret(SecretKeyBytes::new([seed; 32])).node_id()
    }

    #[test]
    fn only_listed_nodes_may_connect() {
        let policy = PeerPolicy::allow_list(vec![id(1)]);
        assert!(policy.permits(&id(1)));
        assert!(!policy.permits(&id(2)));
    }

    #[test]
    fn the_loopback_experiment_policy_permits_any_node() {
        assert!(PeerPolicy::allow_any_loopback().permits(&id(2)));
    }
}
