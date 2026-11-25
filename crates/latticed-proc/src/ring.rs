//! Raw output ring buffer per process. Sans-IO core (verification.md scope).
//!
//! Invariants: `start_off ≤ end_off`; `end_off - start_off ≤ capacity`;
//! a read below `start_off` returns `Gap` exactly when bytes were evicted.

#![allow(dead_code, reason = "L1 skeleton: used by the server once implemented")]

/// Result of reading from an offset.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Read<'a> {
    /// Bytes from the requested offset (may be split in two at the wrap).
    Data(&'a [u8], &'a [u8]),
    /// The offset was evicted; resume at `new_start`.
    Gap { new_start: u64 },
    /// Nothing new yet.
    Empty,
}

/// Fixed-capacity byte ring addressed by absolute offsets.
///
/// TBD: capacity per process (04 says 4–16 MiB).
#[derive(Debug)]
pub(crate) struct Ring {
    buf: Vec<u8>,
    start_off: u64,
    end_off: u64,
}

impl Ring {
    /// An empty ring of `capacity` bytes.
    pub(crate) fn with_capacity(capacity: usize) -> Self {
        Self {
            buf: vec![0; capacity],
            start_off: 0,
            end_off: 0,
        }
    }

    /// Oldest offset still kept.
    pub(crate) fn start_off(&self) -> u64 {
        self.start_off
    }

    /// Offset after the newest byte.
    pub(crate) fn end_off(&self) -> u64 {
        self.end_off
    }

    /// Appends PTY output, evicting the oldest bytes when full.
    #[allow(
        clippy::todo,
        unused_variables,
        reason = "L1 skeleton: body lands with its contract test"
    )]
    pub(crate) fn push(&mut self, bytes: &[u8]) {
        // write at end_off % cap, wrapping; keep only the last `cap` bytes when input is larger
        // end_off += len; start_off = max(start_off, end_off - cap)
        todo!()
    }

    /// Reads from absolute offset `from`.
    #[allow(
        clippy::todo,
        unused_variables,
        reason = "L1 skeleton: body lands with its contract test"
    )]
    pub(crate) fn read_from(&self, from: u64) -> Read<'_> {
        // from < start_off: Gap { new_start: start_off }
        // from >= end_off: Empty
        // else Data(two slices covering from..end_off)
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn concat(r: Read<'_>) -> Vec<u8> {
        match r {
            Read::Data(a, b) => [a, b].concat(),
            other => panic!("expected data, got {other:?}"),
        }
    }

    #[test]
    #[ignore = "contract: lands with Ring"]
    fn reads_back_what_was_pushed_across_the_wrap() {
        let mut r = Ring::with_capacity(4);
        r.push(b"abc");
        r.push(b"de");
        assert_eq!((r.start_off(), r.end_off()), (1, 5));
        assert_eq!(concat(r.read_from(1)), b"bcde");
    }

    #[test]
    #[ignore = "contract: lands with Ring"]
    fn gap_exactly_when_evicted() {
        let mut r = Ring::with_capacity(4);
        r.push(b"abcdef");
        assert_eq!(r.read_from(1), Read::Gap { new_start: 2 });
        assert_eq!(concat(r.read_from(2)), b"cdef");
        assert_eq!(r.read_from(6), Read::Empty);
    }
}
