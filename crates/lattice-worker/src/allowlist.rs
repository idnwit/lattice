//! The worker's allowlist file `worker/decks` (specs/01-pairing.md, Files). Sans-IO.
//!
//! One deck per line: `<node id> <name>`. `#` lines and blank lines are ignored. M1 stand-in
//! for the signed device list (M2.6).

use std::collections::BTreeMap;

use lattice_key::NodeId;

/// First line written to a new file.
const HEADER: &str =
    "# Decks this worker serves (specs/01-pairing.md). One per line: <node id> <name>";

/// The decks a worker serves, with their names.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Allowlist {
    decks: BTreeMap<NodeId, String>,
}

impl Allowlist {
    /// Parses the file. A line that does not parse is skipped and logged, so one bad edit
    /// never locks every deck out.
    pub fn parse(text: &str) -> Self {
        let mut decks = BTreeMap::new();
        for (n, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (id, name) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
            match id.parse::<NodeId>() {
                Ok(id) => {
                    decks.insert(id, name.trim().to_owned());
                }
                Err(e) => tracing::warn!(
                    line = n.saturating_add(1),
                    code = e.code(),
                    "allowlist line skipped"
                ),
            }
        }
        Self { decks }
    }

    /// The file text, header first, decks in NodeId order.
    pub fn to_text(&self) -> String {
        let mut out = String::from(HEADER);
        out.push('\n');
        for (id, name) in &self.decks {
            out.push_str(&format!("{id} {name}\n"));
        }
        out
    }

    /// Adds `deck`, or renames it when it is already listed.
    pub fn add(&mut self, deck: NodeId, name: &str) {
        self.decks.insert(deck, one_line(name));
    }

    /// True when `deck` is listed.
    pub fn contains(&self, deck: &NodeId) -> bool {
        self.decks.contains_key(deck)
    }

    /// The listed decks.
    pub fn decks(&self) -> impl Iterator<Item = &NodeId> {
        self.decks.keys()
    }
}

/// A name a peer sent, safe for a one-line record: control characters become spaces.
pub(crate) fn one_line(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .trim()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use lattice_key::{NodeKey, SecretKeyBytes};

    fn id(seed: u8) -> NodeId {
        NodeKey::from_secret(SecretKeyBytes::new([seed; 32])).node_id()
    }

    #[test]
    fn allowlist_text_roundtrips() {
        let mut list = Allowlist::default();
        list.add(id(1), "minh-mbp");
        list.add(id(2), "office deck");
        let parsed = Allowlist::parse(&list.to_text());
        assert_eq!(parsed, list);
        assert!(parsed.contains(&id(1)) && parsed.contains(&id(2)));
        assert!(!parsed.contains(&id(3)));
    }

    #[test]
    fn comments_blank_and_bad_lines_are_skipped() {
        let text = format!(
            "# header\n\n{}\nnot-a-key deck\n  {} spaced  \n",
            id(1),
            id(2)
        );
        let list = Allowlist::parse(&text);
        assert_eq!(list.decks().count(), 2);
        assert!(list.contains(&id(1)) && list.contains(&id(2)));
    }

    #[test]
    fn names_from_peers_stay_on_one_line() {
        let mut list = Allowlist::default();
        list.add(id(1), "evil\nffff deck\r");
        let text = list.to_text();
        assert_eq!(text.lines().count(), 2, "text={text:?}");
        assert_eq!(Allowlist::parse(&text), list);
    }
}
