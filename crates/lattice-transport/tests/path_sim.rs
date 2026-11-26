//! Deterministic simulation of `PathSelector` on a logical clock
//! (01-transport.md, Path invariants; .rules/verification.md).

use lattice_transport::{
    DIRECT_SILENCE_FALLBACK, DUPLICATE_WINDOW, PathEvent, PathKind, PathSelector,
};
use proptest::prelude::*;
use proptest::test_runner::{Config, RngSeed};
use std::time::Duration;

#[derive(Debug, Clone, Copy)]
enum Ev {
    Probe,
    DirectRx,
    Tick { relay_alive: bool },
}

fn step() -> impl Strategy<Value = (u64, Ev)> {
    let ev = prop_oneof![
        Just(Ev::Probe),
        Just(Ev::DirectRx),
        any::<bool>().prop_map(|relay_alive| Ev::Tick { relay_alive }),
    ];
    (0u64..1_500, ev)
}

fn config() -> Config {
    Config {
        cases: 1024,
        rng_seed: RngSeed::Fixed(0x9a7b_5e1e),
        ..Config::default()
    }
}

/// The spec rules, restated over the event history only (never the FSM's fields).
#[derive(Debug)]
struct Model {
    active: PathKind,
    last_rx: Option<Duration>,
    dup_until: Option<Duration>,
}

impl Model {
    fn expect(&mut self, now: Duration, ev: Ev) -> Option<PathEvent> {
        let want = match ev {
            Ev::Probe => {
                self.last_rx = Some(now);
                (self.active == PathKind::Relay).then_some(PathEvent::Switched(PathKind::Direct))
            }
            Ev::DirectRx => {
                self.last_rx = Some(now);
                None
            }
            Ev::Tick { relay_alive } => {
                let silent = self
                    .last_rx
                    .is_some_and(|rx| now - rx >= DIRECT_SILENCE_FALLBACK);
                if self.active == PathKind::Direct && relay_alive && silent {
                    Some(PathEvent::Switched(PathKind::Relay))
                } else if self.dup_until.is_some_and(|u| now >= u) {
                    Some(PathEvent::DuplicateEnded)
                } else {
                    None
                }
            }
        };
        match want {
            Some(PathEvent::Switched(to)) => {
                self.active = to;
                self.dup_until = Some(now + DUPLICATE_WINDOW);
            }
            Some(PathEvent::DuplicateEnded) => self.dup_until = None,
            None => {}
        }
        want
    }
}

proptest! {
    #![proptest_config(config())]

    /// Path invariants 1–3, plus liveness: after a quiet tick no deadline is in the past.
    #[test]
    fn switches_only_on_probe_or_direct_silence(steps in proptest::collection::vec(step(), 1..200)) {
        let mut p = PathSelector::new();
        let mut m = Model { active: PathKind::Relay, last_rx: None, dup_until: None };
        let mut now = Duration::ZERO;
        for (dt, ev) in steps {
            now += Duration::from_millis(dt);
            let got = match ev {
                Ev::Probe => p.on_probe_validated(now),
                Ev::DirectRx => {
                    p.on_direct_rx(now);
                    None
                }
                Ev::Tick { relay_alive } => p.on_tick(now, relay_alive),
            };
            let want = m.expect(now, ev);
            prop_assert_eq!(got, want, "ev={:?} now={:?}", ev, now);
            prop_assert_eq!(p.active(), m.active);
            prop_assert_eq!(p.duplicating(now), m.dup_until.is_some_and(|u| now < u));
            if matches!(ev, Ev::Tick { .. }) && got.is_none() {
                prop_assert!(p.next_deadline().is_none_or(|d| d > now), "deadline in the past");
            }
        }
    }
}
