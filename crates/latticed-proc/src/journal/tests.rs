#![allow(
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    reason = "test oracle on small bounded values"
)]

use super::*;
use lattice_proc_ipc::{ExitStatus, ProcId};
use proptest::prelude::*;
use proptest::test_runner::{Config, RngSeed};

fn out(proc: u64, bytes: &[u8]) -> EntryKind {
    EntryKind::Out {
        proc: ProcId::from_raw(proc),
        bytes: bytes.to_vec(),
    }
}

fn exit(proc: u64) -> EntryKind {
    EntryKind::Exit {
        proc: ProcId::from_raw(proc),
        status: ExitStatus::Code(0),
    }
}

fn journal(budget: u64) -> Journal {
    Journal::new(
        JournalId::try_new(b"i1".to_vec()).unwrap(),
        Vec::new(),
        budget,
    )
}

fn drain(j: &Journal, mut from: u64) -> Vec<EntryKind> {
    let mut got = Vec::new();
    while let Read::Entry { entry, next, .. } = j.read_from(from) {
        got.push(entry.clone());
        from = next;
    }
    got
}

#[test]
fn two_procs_interleave_in_read_order() {
    let mut j = journal(1024);
    j.push(out(1, b"a"));
    j.push(out(2, b"b"));
    j.push(out(1, b"c"));
    j.push(exit(2));
    let got = drain(&j, 0);
    assert_eq!(got.len(), 4);
    assert_eq!(got.first(), Some(&out(1, b"a")));
    assert_eq!(got.get(1), Some(&out(2, b"b")));
}

#[test]
fn gap_exactly_when_over_budget() {
    let mut j = journal(4);
    j.push(out(1, b"abc"));
    j.push(out(1, b"de"));
    assert_eq!(j.read_from(0), Read::Gap { new_start: 3 });
    assert_eq!(drain(&j, 3), vec![out(1, b"de")]);
    assert_eq!(j.read_from(5), Read::Empty);
}

#[test]
fn trim_drops_history_and_offsets_keep_growing() {
    let mut j = journal(1024);
    j.push(out(1, b"old"));
    j.push(out(1, b"clear"));
    assert_eq!(j.trim(3), 3);
    assert_eq!(j.read_from(0), Read::Gap { new_start: 3 });
    j.push(out(1, b"new"));
    assert_eq!((j.start_off(), j.end_off()), (3, 11));
    assert_eq!(drain(&j, 3), vec![out(1, b"clear"), out(1, b"new")]);
}

#[test]
fn oversized_out_keeps_only_its_tail() {
    let mut j = journal(8);
    j.push(out(1, b"ab"));
    j.push(out(1, b"0123456789"));
    assert_eq!((j.start_off(), j.end_off()), (4, 12));
    assert_eq!(j.read_from(2), Read::Gap { new_start: 4 });
    assert_eq!(drain(&j, 4), vec![out(1, b"23456789")]);
}

#[test]
fn empty_out_is_ignored() {
    let mut j = journal(8);
    j.push(out(1, b""));
    assert_eq!(
        (j.start_off(), j.end_off(), j.read_from(0)),
        (0, 0, Read::Empty)
    );
}

/// Independent oracle: the whole history, recomputed from scratch after every step.
#[derive(Debug)]
struct Model {
    budget: u64,
    all: Vec<(u64, EntryKind)>,
    end: u64,
    trim_floor: u64,
}

impl Model {
    /// Kept = the longest suffix of all entries that fits the budget, minus trimmed ones.
    fn kept(&self) -> Vec<(u64, EntryKind)> {
        let mut sum = 0;
        let mut k = self.all.len();
        while k > 0 && sum + size(&self.all[k - 1].1) <= self.budget {
            sum += size(&self.all[k - 1].1);
            k -= 1;
        }
        let tail = self.all[k..].iter();
        tail.filter(|(off, _)| *off >= self.trim_floor)
            .cloned()
            .collect()
    }

    fn start(&self) -> u64 {
        self.kept().first().map_or(self.end, |(off, _)| *off)
    }
}

#[derive(Debug, Clone)]
enum Op {
    Out(u64, Vec<u8>),
    Exit(u64),
    Trim(u64),
    Read(u64),
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        4 => (1u64..3, proptest::collection::vec(any::<u8>(), 1..=16)).prop_map(|(p, b)| Op::Out(p, b)),
        1 => (1u64..3).prop_map(Op::Exit),
        1 => (0u64..400).prop_map(Op::Trim),
        2 => (0u64..400).prop_map(Op::Read),
    ]
}

fn check_read(j: &Journal, m: &Model, from: u64) {
    let want = if from < m.start() {
        Read::Gap {
            new_start: m.start(),
        }
    } else {
        let kept = m.kept();
        match kept.iter().find(|(off, _)| *off >= from) {
            Some((off, e)) => {
                assert_eq!(
                    j.read_from(from),
                    Read::Entry {
                        off: *off,
                        entry: e,
                        next: off + size(e)
                    }
                );
                return;
            }
            None => Read::Empty,
        }
    };
    assert_eq!(j.read_from(from), want, "from={from}");
}

proptest! {
    #![proptest_config(Config { cases: 512, rng_seed: RngSeed::Fixed(0x10a7_7ce5), ..Config::default() })]

    #[test]
    fn journal_matches_the_history_oracle(budget in 16u64..64, ops in proptest::collection::vec(op(), 1..80)) {
        let mut j = journal(budget);
        let mut m = Model { budget, all: Vec::new(), end: 0, trim_floor: 0 };
        let (mut last_start, mut last_end) = (0, 0);
        for op in ops {
            match op {
                Op::Out(p, b) => {
                    m.all.push((m.end, out(p, &b)));
                    m.end += b.len() as u64;
                    j.push(out(p, &b));
                }
                Op::Exit(p) => {
                    m.all.push((m.end, exit(p)));
                    m.end += EXIT_SIZE;
                    j.push(exit(p));
                }
                Op::Trim(upto) => {
                    m.trim_floor = m.trim_floor.max(upto.min(m.end));
                    prop_assert_eq!(j.trim(upto), m.start());
                }
                Op::Read(from) => check_read(&j, &m, from),
            }
            let kept: u64 = m.kept().iter().map(|(_, e)| size(e)).sum();
            prop_assert!(kept <= budget);
            prop_assert_eq!((j.start_off(), j.end_off()), (m.start(), m.end));
            prop_assert!(j.start_off() <= j.end_off());
            prop_assert!(j.start_off() >= last_start && j.end_off() >= last_end);
            (last_start, last_end) = (j.start_off(), j.end_off());
            let want: Vec<_> = m.kept().into_iter().map(|(_, e)| e).collect();
            prop_assert_eq!(drain(&j, j.start_off()), want);
            if j.start_off() > 0 {
                prop_assert_eq!(j.read_from(j.start_off() - 1), Read::Gap { new_start: j.start_off() });
            }
        }
    }
}
