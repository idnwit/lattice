//! Per-instance journal: ordered process output and exits. Sans-IO core (verification.md scope).
//!
//! Owner spec: `docs/architecture/04-worker.md` (Journal).
//! Invariants: `start_off ≤ end_off`; kept bytes ≤ budget after every push;
//! offsets only grow, also across `trim`; a read below `start_off` returns `Gap`
//! exactly when entries were dropped or trimmed.

#![deny(
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation
)]

use std::collections::VecDeque;

use lattice_proc_ipc::{EntryKind, JournalId};

/// Budget size of an `Exit` entry.
pub(crate) const EXIT_SIZE: u64 = 8;

/// Result of reading from an offset.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Read<'a> {
    /// The entry that starts at the requested offset, and the offset after it.
    Entry {
        /// Offset of this entry.
        off: u64,
        /// The entry.
        entry: &'a EntryKind,
        /// Offset of the next entry.
        next: u64,
    },
    /// The offset was dropped; resume at `new_start`.
    Gap {
        /// Oldest offset still kept.
        new_start: u64,
    },
    /// Nothing new yet.
    Empty,
}

/// Budget size of an entry: `Out` its byte count, `Exit` [`EXIT_SIZE`].
pub(crate) fn size(entry: &EntryKind) -> u64 {
    match entry {
        EntryKind::Out { bytes, .. } => u64::try_from(bytes.len()).unwrap_or(u64::MAX),
        EntryKind::Exit { .. } => EXIT_SIZE,
    }
}

/// Ordered entries of one app instance, addressed by absolute byte offsets.
///
/// An entry's size is [`size`]. The budget is at least [`EXIT_SIZE`] (`OpenSpec` enforces it).
#[derive(Debug)]
pub(crate) struct Journal {
    id: JournalId,
    meta: Vec<u8>,
    budget: u64,
    entries: VecDeque<(u64, EntryKind)>,
    kept: u64,
    start_off: u64,
    end_off: u64,
}

impl Journal {
    /// An empty journal with a byte `budget`.
    pub(crate) fn new(id: JournalId, meta: Vec<u8>, budget: u64) -> Self {
        Self {
            id,
            meta,
            budget,
            entries: VecDeque::new(),
            kept: 0,
            start_off: 0,
            end_off: 0,
        }
    }

    /// The journal id.
    pub(crate) fn id(&self) -> &JournalId {
        &self.id
    }

    /// The opaque meta set at `Open`.
    pub(crate) fn meta(&self) -> &[u8] {
        &self.meta
    }

    /// Oldest offset still kept.
    pub(crate) fn start_off(&self) -> u64 {
        self.start_off
    }

    /// Offset after the newest entry.
    pub(crate) fn end_off(&self) -> u64 {
        self.end_off
    }

    /// Appends an entry, dropping the oldest ones while over budget.
    ///
    /// An empty `Out` is ignored (it has no offset of its own). An `Out` larger
    /// than the whole budget keeps only its last `budget` bytes, at the offset
    /// those bytes had in the stream.
    pub(crate) fn push(&mut self, entry: EntryKind) {
        let len = size(&entry);
        if len == 0 {
            return;
        }
        let entry = match entry {
            EntryKind::Out { proc, bytes } if len > self.budget => {
                let cut = usize::try_from(len.saturating_sub(self.budget)).unwrap_or(usize::MAX);
                self.entries.clear();
                self.kept = 0;
                self.end_off = self.end_off.saturating_add(len.saturating_sub(self.budget));
                let bytes = bytes.get(cut..).map(<[u8]>::to_vec).unwrap_or_default();
                EntryKind::Out { proc, bytes }
            }
            other => other,
        };
        let len = size(&entry);
        self.entries.push_back((self.end_off, entry));
        self.end_off = self.end_off.saturating_add(len);
        self.kept = self.kept.saturating_add(len);
        while self.kept > self.budget && self.entries.len() > 1 {
            self.pop_front();
        }
        self.start_off = self.front_off();
    }

    /// Drops every entry that starts before `upto`. Returns the new `start_off`.
    pub(crate) fn trim(&mut self, upto: u64) -> u64 {
        let upto = upto.min(self.end_off);
        while self.entries.front().is_some_and(|(off, _)| *off < upto) {
            self.pop_front();
        }
        self.start_off = self.front_off().max(self.start_off);
        self.start_off
    }

    /// Reads the entry at absolute offset `from`, or the first one after it.
    pub(crate) fn read_from(&self, from: u64) -> Read<'_> {
        if from < self.start_off {
            return Read::Gap {
                new_start: self.start_off,
            };
        }
        let i = self.entries.partition_point(|(off, _)| *off < from);
        match self.entries.get(i) {
            Some((off, entry)) => Read::Entry {
                off: *off,
                entry,
                next: off.saturating_add(size(entry)),
            },
            None => Read::Empty,
        }
    }

    fn pop_front(&mut self) {
        if let Some((_, e)) = self.entries.pop_front() {
            self.kept = self.kept.saturating_sub(size(&e));
        }
    }

    /// Offset of the oldest kept entry, or `end_off` when none is kept.
    fn front_off(&self) -> u64 {
        self.entries.front().map_or(self.end_off, |(off, _)| *off)
    }
}

#[cfg(test)]
mod tests;
