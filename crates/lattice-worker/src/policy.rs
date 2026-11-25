//! Who may connect (03-identity-security.md, Access control rule 1).

use lattice_key::NodeId;
use std::collections::BTreeSet;

/// Accepts a connection only from listed NodeIds.
///
/// L1 stand-in for the signed device list. Replaced by `lattice-identity`
/// once pairing lands.
#[derive(Debug, Clone)]
pub struct PeerPolicy {
    allowed: BTreeSet<NodeId>,
}

impl PeerPolicy {
    /// Allows exactly these nodes.
    pub fn allow_list(nodes: Vec<NodeId>) -> Self {
        Self {
            allowed: nodes.into_iter().collect(),
        }
    }

    /// True when `peer` may connect.
    pub fn permits(&self, peer: &NodeId) -> bool {
        self.allowed.contains(peer)
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
}
