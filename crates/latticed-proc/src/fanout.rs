//! Subscriber cursors per journal. A slow subscriber never blocks the PTY reader.
//!
//! The journal is the only per-subscriber backlog: a client whose output buffer
//! is full simply stops advancing. If the journal drops entries past its cursor,
//! its next read is `Gap{new_start}` and it continues from there.

use crate::journal::{Journal, Read};

/// Index of a connected client in the server's table.
pub(crate) type ClientIdx = usize;

/// Who reads a journal and from where.
#[derive(Debug, Default)]
pub(crate) struct Fanout {
    cursors: Vec<(ClientIdx, u64)>,
}

impl Fanout {
    /// Starts streaming to `client` from `from_off`. Replaces an existing cursor.
    pub(crate) fn subscribe(&mut self, client: ClientIdx, from_off: u64) {
        self.unsubscribe(client);
        self.cursors.push((client, from_off));
    }

    /// Stops streaming to `client`.
    pub(crate) fn unsubscribe(&mut self, client: ClientIdx) {
        self.cursors.retain(|(c, _)| *c != client);
    }

    /// Hands each subscriber what comes next while `emit(client, read)` accepts it.
    ///
    /// `emit` returns `false` when that client has no room; its cursor stays and
    /// the other subscribers continue. Nothing here waits on a client.
    pub(crate) fn pump(
        &mut self,
        journal: &Journal,
        mut emit: impl FnMut(ClientIdx, &Read<'_>) -> bool,
    ) {
        for (client, cursor) in &mut self.cursors {
            loop {
                let read = journal.read_from(*cursor);
                let next = match read {
                    Read::Entry { next, .. } => next,
                    Read::Gap { new_start } => new_start,
                    Read::Empty => break,
                };
                if !emit(*client, &read) {
                    break;
                }
                *cursor = next.max(*cursor);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lattice_proc_ipc::{EntryKind, JournalId, ProcId};

    fn out(bytes: &[u8]) -> EntryKind {
        EntryKind::Out {
            proc: ProcId::from_raw(1),
            bytes: bytes.to_vec(),
        }
    }

    /// Collects what `pump` hands out; client 2 accepts at most `room2` items.
    fn pump(f: &mut Fanout, j: &Journal, room2: usize) -> Vec<(ClientIdx, String)> {
        let mut got = Vec::new();
        f.pump(j, |c, r| {
            if c == 2 && got.iter().filter(|(c, _)| *c == 2).count() >= room2 {
                return false;
            }
            let s = match r {
                Read::Entry { off, .. } => format!("entry@{off}"),
                Read::Gap { new_start } => format!("gap@{new_start}"),
                Read::Empty => unreachable!(),
            };
            got.push((c, s));
            true
        });
        got
    }

    #[test]
    fn slow_subscriber_never_blocks_the_others() {
        let mut j = Journal::new(JournalId::try_new(b"i".to_vec()).unwrap(), Vec::new(), 8);
        let mut f = Fanout::default();
        f.subscribe(1, 0);
        f.subscribe(2, 0);
        j.push(out(b"abc"));
        j.push(out(b"de"));
        let got = pump(&mut f, &j, 0);
        assert_eq!(got, vec![(1, "entry@0".into()), (1, "entry@3".into())]);
        j.push(out(b"fghi"));
        let got = pump(&mut f, &j, 2);
        let want = [(1, "entry@5"), (2, "gap@3"), (2, "entry@3")];
        assert_eq!(got, want.map(|(c, s)| (c, s.to_string())).to_vec());
    }

    #[test]
    fn resubscribe_replaces_the_cursor_and_unsubscribe_stops() {
        let mut j = Journal::new(JournalId::try_new(b"i".to_vec()).unwrap(), Vec::new(), 64);
        let mut f = Fanout::default();
        j.push(out(b"abc"));
        f.subscribe(1, 0);
        f.subscribe(1, 3);
        assert!(pump(&mut f, &j, 0).is_empty());
        j.push(out(b"d"));
        f.unsubscribe(1);
        assert!(pump(&mut f, &j, 0).is_empty());
    }
}
