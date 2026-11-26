//! The deck's worker list file `deck/workers` (specs/01-pairing.md, Files). Sans-IO.
//!
//! One worker per line: `<node id> <ip:port> <name>`, in pairing order: the last line is
//! the most recently paired worker, the M1 active one.

use std::net::SocketAddr;

use lattice_key::NodeId;
use lattice_sync_proto::one_line_name;

use crate::pair::PairedWorker;

/// First line written to a new file.
const HEADER: &str = "# Workers this deck paired with (specs/01-pairing.md). One per line: <node id> <ip:port> <name>";

/// The paired workers, oldest first.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkerList {
    workers: Vec<PairedWorker>,
}

impl WorkerList {
    /// Parses the file. A line that does not parse is skipped and logged.
    pub fn parse(text: &str) -> Self {
        let mut list = Self::default();
        for (n, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            match parse_line(line) {
                Some(w) => list.add(w),
                None => tracing::warn!(line = n.saturating_add(1), "worker list line skipped"),
            }
        }
        list
    }

    /// The file text, header first.
    pub fn to_text(&self) -> String {
        let mut out = String::from(HEADER);
        out.push('\n');
        for w in &self.workers {
            out.push_str(&format!("{} {} {}\n", w.id, w.addr, w.name));
        }
        out
    }

    /// Adds `worker` last; a worker paired again moves last with its new address and name.
    pub fn add(&mut self, worker: PairedWorker) {
        self.workers.retain(|w| w.id != worker.id);
        self.workers.push(PairedWorker {
            name: one_line_name(&worker.name),
            ..worker
        });
    }

    /// The paired workers, oldest first.
    pub fn workers(&self) -> &[PairedWorker] {
        &self.workers
    }

    /// The most recently paired worker: the M1 active worker (specs/01-pairing.md, UI).
    pub fn latest(&self) -> Option<&PairedWorker> {
        self.workers.last()
    }

    /// The worker named `key`: its name, its full NodeId, or a NodeId prefix of 4+ digits.
    pub fn find(&self, key: &str) -> Option<&PairedWorker> {
        self.workers
            .iter()
            .rev()
            .find(|w| w.name == key || (key.len() >= 4 && w.id.to_string().starts_with(key)))
    }
}

fn parse_line(line: &str) -> Option<PairedWorker> {
    let mut parts = line.splitn(3, char::is_whitespace);
    let id = parts.next()?.parse::<NodeId>().ok()?;
    let addr = parts.next()?.parse::<SocketAddr>().ok()?;
    let name = parts.next().unwrap_or_default().trim().to_owned();
    Some(PairedWorker { id, addr, name })
}

/// Two-letter rail monogram: first and last character of the name, upper case
/// (specs/01-pairing.md, Data; `build-01` → `B1`).
pub fn monogram(name: &str) -> String {
    let mut chars = name.chars().filter(|c| c.is_alphanumeric());
    let first = chars.next();
    let last = chars.next_back().or(first);
    first
        .into_iter()
        .chain(last)
        .flat_map(char::to_uppercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use lattice_key::{NodeKey, SecretKeyBytes};

    fn worker(seed: u8, name: &str) -> PairedWorker {
        PairedWorker {
            id: NodeKey::from_secret(SecretKeyBytes::new([seed; 32])).node_id(),
            addr: "192.168.1.5:7441".parse().unwrap(),
            name: name.to_owned(),
        }
    }

    #[test]
    fn worker_list_text_roundtrips_in_pairing_order() {
        let mut list = WorkerList::default();
        list.add(worker(1, "build-01"));
        list.add(worker(2, "gpu 02"));
        let parsed = WorkerList::parse(&list.to_text());
        assert_eq!(parsed, list);
        assert_eq!(parsed.latest().map(|w| w.name.as_str()), Some("gpu 02"));
    }

    #[test]
    fn pairing_again_moves_a_worker_last() {
        let mut list = WorkerList::default();
        list.add(worker(1, "build-01"));
        list.add(worker(2, "gpu-02"));
        list.add(worker(1, "build-01b"));
        let names: Vec<&str> = list.workers().iter().map(|w| w.name.as_str()).collect();
        assert_eq!(names, ["gpu-02", "build-01b"]);
    }

    #[test]
    fn workers_are_found_by_name_or_id_prefix() {
        let mut list = WorkerList::default();
        list.add(worker(1, "build-01"));
        let id = list.workers()[0].id.to_string();
        assert!(list.find("build-01").is_some());
        assert!(list.find(&id[..6]).is_some());
        assert!(list.find(&id[..3]).is_none(), "a short prefix is ambiguous");
        assert!(list.find("nope").is_none());
    }

    #[test]
    fn bad_lines_are_skipped_and_names_stay_one_line() {
        let mut list = WorkerList::default();
        list.add(worker(1, "evil\nname"));
        let text = format!("{}garbage line\n\n", list.to_text());
        let parsed = WorkerList::parse(&text);
        assert_eq!(parsed.workers().len(), 1);
        assert_eq!(parsed.workers()[0].name, "evil name");
    }

    #[test]
    fn monograms_take_the_first_and_last_character() {
        let cases = [("build-01", "B1"), ("gpu-02", "G2"), ("x", "XX"), ("", "")];
        for (name, want) in cases {
            assert_eq!(monogram(name), want, "case={name}");
        }
    }
}
