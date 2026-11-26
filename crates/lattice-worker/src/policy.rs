//! Who may connect (03-identity-security.md, Access control rule 1).

use std::collections::BTreeSet;
use std::path::PathBuf;

use lattice_key::NodeId;

use crate::allowlist::Allowlist;

/// Which NodeIds may connect.
///
/// L1 stand-in for the signed device list. Replaced by `lattice-identity` in M2.6.
#[derive(Debug, Clone)]
pub struct PeerPolicy {
    mode: Mode,
}

#[derive(Debug, Clone)]
enum Mode {
    /// Only these nodes.
    List(BTreeSet<NodeId>),
    /// The decks in this allowlist file, read on every connection
    /// (specs/01-pairing.md, rules 12–13). A missing file allows no one.
    File(PathBuf),
}

impl PeerPolicy {
    /// Allows exactly these nodes.
    pub fn allow_list(nodes: Vec<NodeId>) -> Self {
        Self {
            mode: Mode::List(nodes.into_iter().collect()),
        }
    }

    /// Allows the decks listed in the allowlist file at `path` (`worker/decks`). The file is
    /// read on every connection, so `lattice add-deck` takes effect without a restart.
    pub fn allow_file(path: PathBuf) -> Self {
        Self {
            mode: Mode::File(path),
        }
    }

    /// True when `peer` may connect.
    pub async fn permits(&self, peer: &NodeId) -> bool {
        match &self.mode {
            Mode::List(allowed) => allowed.contains(peer),
            Mode::File(path) => match tokio::fs::read_to_string(path).await {
                Ok(text) => Allowlist::parse(&text).contains(peer),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
                Err(e) => {
                    tracing::warn!(error = %e, "allowlist unreadable; connection refused");
                    false
                }
            },
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

    #[tokio::test]
    async fn only_listed_nodes_may_connect() {
        let policy = PeerPolicy::allow_list(vec![id(1)]);
        assert!(policy.permits(&id(1)).await);
        assert!(!policy.permits(&id(2)).await);
    }

    #[tokio::test]
    async fn the_file_policy_follows_the_file_as_it_changes() {
        let path = std::env::temp_dir().join(format!("lattice-policy-{}", std::process::id()));
        drop(std::fs::remove_file(&path));
        let policy = PeerPolicy::allow_file(path.clone());
        assert!(!policy.permits(&id(1)).await, "no file allows no one");
        let mut list = Allowlist::default();
        list.add(id(1), "deck");
        std::fs::write(&path, list.to_text()).unwrap();
        assert!(policy.permits(&id(1)).await);
        assert!(!policy.permits(&id(2)).await);
        std::fs::remove_file(&path).unwrap();
    }
}
