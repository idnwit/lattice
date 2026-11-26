//! Replay guard, one entry per node (01-transport.md, Relay rule 7).

use lattice_key::NodeId;
use std::collections::BTreeMap;

/// Nonces kept for one `ts`: several `REGISTER`s in the same second (rule 7).
const MAX_NONCES_PER_TS: usize = 4;

/// What one node last registered with.
#[derive(Debug)]
struct Seen {
    ts: u64,
    nonces: Vec<[u8; 16]>,
}

/// `node_id → highest accepted ts and its nonces`. One entry per key, however fast it
/// registers, so a single key can never fill the relay's memory.
#[derive(Debug, Default)]
pub(crate) struct ReplayGuard {
    by_node: BTreeMap<NodeId, Seen>,
}

impl ReplayGuard {
    /// True when `(ts, nonce)` may pass: a higher `ts`, or the same `ts` with a new nonce.
    pub(crate) fn admits(&self, node: NodeId, ts: u64, nonce: [u8; 16]) -> bool {
        match self.by_node.get(&node) {
            None => true,
            Some(seen) if ts > seen.ts => true,
            Some(seen) => {
                ts == seen.ts
                    && !seen.nonces.contains(&nonce)
                    && seen.nonces.len() < MAX_NONCES_PER_TS
            }
        }
    }

    /// Records an accepted `REGISTER`. Call only after [`ReplayGuard::admits`].
    pub(crate) fn record(&mut self, node: NodeId, ts: u64, nonce: [u8; 16]) {
        let seen = self.by_node.entry(node).or_insert(Seen {
            ts,
            nonces: Vec::new(),
        });
        if ts > seen.ts {
            seen.ts = ts;
            seen.nonces.clear();
        }
        seen.nonces.push(nonce);
    }

    /// True when `node` has an entry.
    pub(crate) fn knows(&self, node: NodeId) -> bool {
        self.by_node.contains_key(&node)
    }

    /// Entries held.
    pub(crate) fn len(&self) -> usize {
        self.by_node.len()
    }

    /// Drops entries whose `ts` is no longer fresh: a replay of them is stale too.
    pub(crate) fn expire(&mut self, fresh: impl Fn(u64) -> bool) {
        self.by_node.retain(|_, seen| fresh(seen.ts));
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
    fn an_older_ts_is_a_replay_even_with_a_new_nonce() {
        let mut g = ReplayGuard::default();
        g.record(id(1), 100, [1; 16]);
        let cases = [
            ("same ts, same nonce", 100, [1; 16], false),
            ("same ts, new nonce", 100, [2; 16], true),
            ("older ts, new nonce", 99, [3; 16], false),
            ("newer ts", 101, [1; 16], true),
        ];
        for (name, ts, nonce, want) in cases {
            assert_eq!(g.admits(id(1), ts, nonce), want, "case={name}");
        }
        assert!(g.admits(id(2), 1, [1; 16]), "another node is independent");
    }

    #[test]
    fn one_ts_holds_a_bounded_number_of_nonces() {
        let mut g = ReplayGuard::default();
        for n in 0..4u8 {
            assert!(g.admits(id(1), 100, [n; 16]));
            g.record(id(1), 100, [n; 16]);
        }
        assert!(!g.admits(id(1), 100, [9; 16]));
        g.record(id(1), 101, [9; 16]);
        assert_eq!(g.len(), 1, "one entry per node");
        assert!(
            g.admits(id(1), 101, [8; 16]),
            "a newer ts resets the nonces"
        );
    }

    #[test]
    fn a_stale_entry_is_forgotten() {
        let mut g = ReplayGuard::default();
        g.record(id(1), 100, [1; 16]);
        g.expire(|ts| ts >= 100);
        assert!(g.knows(id(1)));
        g.expire(|ts| ts > 100);
        assert!(!g.knows(id(1)));
    }
}
