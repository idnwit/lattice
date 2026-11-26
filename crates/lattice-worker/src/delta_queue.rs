//! The last deltas of one instance, for decks that resume (06-apps.md, Sync to viewers).

use std::collections::VecDeque;

/// Deltas kept per instance. A reader older than this gets the full state (D31).
pub(crate) const DELTA_QUEUE_LEN: usize = 1024;

/// What a resuming reader gets.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Resume<'a> {
    /// The deltas after the reader's version, oldest first. Empty when it is current.
    Deltas(Vec<(u64, &'a [u8])>),
    /// The reader is outside the queue (or has no version): send the full state.
    FullState,
}

/// The first version of every `State` stream in one run of `latticed` (06-apps.md, Sync to
/// viewers rules 10–12). `latticed` makes it from the clock at boot; the runner never reads one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct BootEpoch(u64);

impl BootEpoch {
    /// Low bits left for commits: a boot may commit 2^20 deltas per stream per millisecond it
    /// runs before its versions could reach the next boot's epoch.
    pub const SHIFT: u32 = 20;

    /// The epoch of a boot at `unix_ms`: `unix_ms << 20`, saturating (past the year 2527).
    pub fn from_unix_millis(unix_ms: u64) -> Self {
        Self(unix_ms.saturating_mul(1 << Self::SHIFT))
    }

    /// The first version (before any commit).
    pub fn base(self) -> u64 {
        self.0
    }
}

/// Ring of the newest committed deltas with their versions.
#[derive(Debug)]
pub(crate) struct DeltaQueue {
    deltas: VecDeque<(u64, Vec<u8>)>,
    version: u64,
}

impl DeltaQueue {
    /// An empty queue at version `epoch` (06-apps.md, Sync to viewers rule 10).
    pub(crate) fn new(epoch: BootEpoch) -> Self {
        Self {
            deltas: VecDeque::new(),
            version: epoch.base(),
        }
    }

    /// The version of the newest delta (the epoch before any commit).
    pub(crate) fn version(&self) -> u64 {
        self.version
    }

    /// Records a committed delta; returns its version. Drops the oldest past [`DELTA_QUEUE_LEN`].
    pub(crate) fn push(&mut self, delta: Vec<u8>) -> u64 {
        // One delta per commit: a u64 cannot overflow by counting.
        self.version = self.version.saturating_add(1);
        self.deltas.push_back((self.version, delta));
        while self.deltas.len() > DELTA_QUEUE_LEN {
            self.deltas.pop_front();
        }
        self.version
    }

    /// What a reader holding `have` needs next.
    pub(crate) fn resume(&self, have: Option<u64>) -> Resume<'_> {
        let Some(have) = have else {
            return Resume::FullState;
        };
        if have > self.version {
            return Resume::FullState;
        }
        // Version of the oldest kept delta; with an empty queue, the next one.
        let oldest = self
            .deltas
            .front()
            .map_or(self.version.saturating_add(1), |(v, _)| *v);
        if have.saturating_add(1) < oldest {
            return Resume::FullState;
        }
        let missing = self
            .deltas
            .iter()
            .filter(|(v, _)| *v > have)
            .map(|(v, d)| (*v, d.as_slice()))
            .collect();
        Resume::Deltas(missing)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resume_inside_the_queue_sends_only_missing_deltas() {
        let mut q = DeltaQueue::new(BootEpoch::from_unix_millis(0));
        q.push(b"a".to_vec());
        q.push(b"b".to_vec());
        q.push(b"c".to_vec());
        assert_eq!(
            q.resume(Some(1)),
            Resume::Deltas(vec![(2, &b"b"[..]), (3, &b"c"[..])])
        );
        assert_eq!(q.resume(Some(3)), Resume::Deltas(Vec::new()));
    }

    #[test]
    fn resume_older_than_the_queue_gets_full_state() {
        let mut q = DeltaQueue::new(BootEpoch::from_unix_millis(0));
        for _ in 0..=DELTA_QUEUE_LEN {
            q.push(Vec::new());
        }
        assert_eq!(q.resume(Some(0)), Resume::FullState);
        assert_eq!(q.resume(None), Resume::FullState);
        assert!(matches!(q.resume(Some(1)), Resume::Deltas(_)));
    }

    #[test]
    fn a_reader_ahead_of_the_writer_gets_full_state() {
        let mut q = DeltaQueue::new(BootEpoch::from_unix_millis(0));
        assert_eq!(q.resume(Some(0)), Resume::Deltas(Vec::new()));
        assert_eq!(q.resume(Some(1)), Resume::FullState);
        assert_eq!(q.push(b"a".to_vec()), 1);
        assert_eq!(q.version(), 1);
        assert_eq!(q.resume(Some(0)), Resume::Deltas(vec![(1, &b"a"[..])]));
    }

    #[test]
    fn a_version_from_an_earlier_boot_gets_the_full_state() {
        // Boot 1 commits three deltas; a deck holds the newest.
        let mut before = DeltaQueue::new(BootEpoch::from_unix_millis(1));
        for d in [b"a", b"b", b"c"] {
            before.push(d.to_vec());
        }
        let held = before.version();
        // Boot 2 (1 ms later) commits as many again: its versions never meet the old ones.
        let mut after = DeltaQueue::new(BootEpoch::from_unix_millis(2));
        for d in [b"x", b"y", b"z"] {
            after.push(d.to_vec());
        }
        assert!(after.version() > held, "{} > {held}", after.version());
        assert_eq!(after.resume(Some(held)), Resume::FullState);
    }

    #[test]
    fn the_epoch_is_the_boot_millis_shifted_and_saturates() {
        assert_eq!(BootEpoch::from_unix_millis(0).base(), 0);
        assert_eq!(BootEpoch::from_unix_millis(3).base(), 3 << 20);
        assert_eq!(BootEpoch::from_unix_millis(u64::MAX).base(), u64::MAX);
        let q = DeltaQueue::new(BootEpoch::from_unix_millis(7));
        assert_eq!(q.version(), 7 << 20);
        assert_eq!(q.resume(Some(7 << 20)), Resume::Deltas(Vec::new()));
    }
}
