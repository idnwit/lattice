//! Path selection between relay and direct (01-transport.md, Paths).
//!
//! Sans-IO. Time is a `Duration` since node start, passed in by the driver.

use std::fmt;
use std::time::Duration;

/// Fall back to relay when the direct path is silent this long (01, Paths rule 4).
pub const DIRECT_SILENCE_FALLBACK: Duration = Duration::from_secs(2);

/// Send on both paths this long after a switch (01, Paths rule 5).
///
/// TBD: spec says 1–2 s. Upper bound used until measured.
pub const DUPLICATE_WINDOW: Duration = Duration::from_secs(2);

/// Which path carries packets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathKind {
    /// Via the relay (`SEND` / `RECV`).
    Relay,
    /// Straight to the peer's address.
    Direct,
}

/// Path and smoothed RTT, as shown in the status bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PathInfo {
    kind: PathKind,
    rtt: Duration,
}

impl PathInfo {
    /// A path of `kind` with smoothed `rtt`.
    pub fn new(kind: PathKind, rtt: Duration) -> Self {
        Self { kind, rtt }
    }

    /// The active path.
    pub fn kind(&self) -> PathKind {
        self.kind
    }

    /// Smoothed RTT.
    pub fn rtt(&self) -> Duration {
        self.rtt
    }
}

impl fmt::Display for PathInfo {
    /// `direct · 18ms` / `relay · 64ms` (components.md, Worker card).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let kind = match self.kind {
            PathKind::Relay => "relay",
            PathKind::Direct => "direct",
        };
        write!(f, "{kind} · {}ms", self.rtt.as_millis())
    }
}

/// Output of the FSM for the socket driver.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathEvent {
    /// Switched path. The driver resets cwnd and starts duplicate sending.
    Switched(PathKind),
    /// Duplicate window ended; send on the active path only.
    DuplicateEnded,
}

/// Relay-first path FSM for one peer. Never flaps (verification.md scope).
#[derive(Debug)]
#[allow(dead_code, reason = "L1 skeleton: read once the FSM is implemented")]
pub struct PathSelector {
    active: PathKind,
    last_direct_rx: Option<Duration>,
    duplicate_until: Option<Duration>,
}

impl Default for PathSelector {
    fn default() -> Self {
        Self::new()
    }
}

impl PathSelector {
    /// Starts on the relay path (connect is instant via relay).
    pub fn new() -> Self {
        Self {
            active: PathKind::Relay,
            last_direct_rx: None,
            duplicate_until: None,
        }
    }

    /// The path outgoing packets use now.
    pub fn active(&self) -> PathKind {
        self.active
    }

    /// True while packets go on both paths.
    #[allow(
        clippy::todo,
        unused_variables,
        reason = "L1 skeleton: body lands with its contract test"
    )]
    pub fn duplicating(&self, now: Duration) -> bool {
        // duplicate_until is set and now < duplicate_until
        todo!()
    }

    /// A punch probe round-trip succeeded both ways at `now`.
    #[allow(
        clippy::todo,
        unused_variables,
        reason = "L1 skeleton: body lands with its contract test"
    )]
    pub fn on_probe_validated(&mut self, now: Duration) -> Option<PathEvent> {
        // record direct rx at now
        // already Direct: None
        // switch to Direct, open the duplicate window, return Switched(Direct)
        todo!()
    }

    /// Any packet arrived on the direct path at `now`.
    #[allow(
        clippy::todo,
        unused_variables,
        reason = "L1 skeleton: body lands with its contract test"
    )]
    pub fn on_direct_rx(&mut self, now: Duration) {
        // record last direct rx; never switches by itself (probe validation does)
        todo!()
    }

    /// Advances time. Call at [`PathSelector::next_deadline`] or on any event.
    #[allow(
        clippy::todo,
        unused_variables,
        reason = "L1 skeleton: body lands with its contract test"
    )]
    pub fn on_tick(&mut self, now: Duration, relay_alive: bool) -> Option<PathEvent> {
        // Direct and relay alive and now - last_direct_rx >= DIRECT_SILENCE_FALLBACK:
        //   switch to Relay, open duplicate window, return Switched(Relay)
        // duplicate window passed: clear it, return DuplicateEnded
        // else None
        todo!()
    }

    /// Next time the driver must call [`PathSelector::on_tick`].
    #[allow(
        clippy::todo,
        unused_variables,
        reason = "L1 skeleton: body lands with its contract test"
    )]
    pub fn next_deadline(&self) -> Option<Duration> {
        // min of (last_direct_rx + DIRECT_SILENCE_FALLBACK when Direct, duplicate_until)
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn ms(v: u64) -> Duration {
        Duration::from_millis(v)
    }

    #[test]
    fn path_info_displays_like_the_status_bar() {
        let relay = PathInfo::new(PathKind::Relay, ms(64));
        assert_eq!(
            PathInfo::new(PathKind::Direct, ms(18)).to_string(),
            "direct · 18ms"
        );
        assert_eq!(relay.to_string(), "relay · 64ms");
        assert_eq!(relay.rtt(), ms(64));
    }

    #[test]
    #[ignore = "contract: lands with PathSelector"]
    fn starts_on_relay_and_goes_direct_after_probe() {
        let mut p = PathSelector::new();
        assert_eq!(p.active(), PathKind::Relay);
        let ev = p.on_probe_validated(ms(100));
        assert_eq!(ev, Some(PathEvent::Switched(PathKind::Direct)));
        assert!(p.duplicating(ms(100)));
    }

    #[test]
    #[ignore = "contract: lands with PathSelector"]
    fn relay_fallback_after_two_seconds_of_direct_silence() {
        let mut p = PathSelector::new();
        p.on_probe_validated(ms(0));
        let just_before = DIRECT_SILENCE_FALLBACK - ms(1);
        let switched = Some(PathEvent::Switched(PathKind::Relay));
        assert_ne!(p.on_tick(just_before, true), switched);
        assert_eq!(p.active(), PathKind::Direct);
        assert_eq!(p.on_tick(DIRECT_SILENCE_FALLBACK, true), switched);
        assert!(p.duplicating(DIRECT_SILENCE_FALLBACK + DUPLICATE_WINDOW - ms(1)));
    }

    #[test]
    #[ignore = "contract: lands with PathSelector"]
    fn direct_rx_postpones_fallback_and_the_deadline() {
        let mut p = PathSelector::new();
        p.on_probe_validated(ms(0));
        p.on_direct_rx(ms(1_500));
        let want = DUPLICATE_WINDOW.min(ms(1_500) + DIRECT_SILENCE_FALLBACK);
        assert_eq!(p.next_deadline(), Some(want));
        assert_ne!(
            p.on_tick(ms(2_000), true),
            Some(PathEvent::Switched(PathKind::Relay))
        );
        assert_eq!(p.active(), PathKind::Direct);
    }

    #[test]
    #[ignore = "contract: lands with PathSelector"]
    fn no_fallback_while_relay_is_dead() {
        let mut p = PathSelector::new();
        p.on_probe_validated(ms(0));
        p.on_tick(ms(5_000), false);
        assert_eq!(p.active(), PathKind::Direct);
    }
}
