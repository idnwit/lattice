//! When to dial a lost worker again (specs/02-connectivity.md, Reconnect rules 8–10). Sans-IO:
//! the caller sleeps for the delay it gets.

use std::time::Duration;

/// First wait after a link ends (rule 8, proposed).
pub const RECONNECT_FIRST: Duration = Duration::from_millis(500);
/// Longest wait between dials (rule 8, proposed).
pub const RECONNECT_MAX: Duration = Duration::from_secs(8);
/// A dial that opens nothing in this time failed (rule 9, proposed).
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// The wait before each dial: [`RECONNECT_FIRST`], doubled per failed dial, capped at
/// [`RECONNECT_MAX`]; [`Backoff::reset`] once a link opens (rule 10).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Backoff {
    failed: u32,
}

impl Backoff {
    /// The wait before the next dial; counts that dial as failed until [`Backoff::reset`].
    pub fn next_delay(&mut self) -> Duration {
        // 2^failed times the first wait; past the cap the shift saturates there anyway.
        let factor = 1u32.checked_shl(self.failed).unwrap_or(u32::MAX);
        self.failed = self.failed.saturating_add(1);
        RECONNECT_FIRST
            .checked_mul(factor)
            .map_or(RECONNECT_MAX, |d| d.min(RECONNECT_MAX))
    }

    /// A link opened: the next loss waits [`RECONNECT_FIRST`] again.
    pub fn reset(&mut self) {
        self.failed = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waits_double_from_the_first_and_stop_at_the_cap() {
        let mut b = Backoff::default();
        let got: Vec<u128> = (0..8).map(|_| b.next_delay().as_millis()).collect();
        assert_eq!(got, [500, 1000, 2000, 4000, 8000, 8000, 8000, 8000]);
    }

    #[test]
    fn every_wait_stays_within_first_and_max_even_after_many_failures() {
        let mut b = Backoff::default();
        for n in 0..10_000 {
            let d = b.next_delay();
            assert!(
                (RECONNECT_FIRST..=RECONNECT_MAX).contains(&d),
                "case={n} {d:?}"
            );
        }
    }

    #[test]
    fn an_opened_link_resets_the_wait() {
        let mut b = Backoff::default();
        b.next_delay();
        b.next_delay();
        b.reset();
        assert_eq!(b.next_delay(), RECONNECT_FIRST);
    }
}
