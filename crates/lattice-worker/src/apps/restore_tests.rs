//! Restore across a `latticed` restart (06-apps.md, Restore; Sync to viewers rules 10–12).

use super::*;
use crate::test_app::{echo_hash, echo_id, echo_installed};
use lattice_app::InstancePhase;
use lattice_proc_ipc::{JournalId, ProcId};

/// One boot of `latticed`: a runner at `epoch_ms` with Echo installed.
fn boot(epoch_ms: u64) -> AppsRunner {
    let mut r = AppsRunner::new(BootEpoch::from_unix_millis(epoch_ms));
    r.install(echo_hash(), echo_installed());
    r
}

fn journal(iid: InstanceId) -> JournalId {
    JournalId::try_new(iid.raw().to_be_bytes().to_vec()).unwrap()
}

fn out(iid: InstanceId, off: u64, bytes: &[u8]) -> Reply {
    Reply::Entry {
        journal: journal(iid),
        off: Offset::from_raw(off),
        entry: EntryKind::Out {
            proc: ProcId::from_raw(1),
            bytes: bytes.to_vec(),
        },
    }
}

/// What `latticed-proc` lists for the instance after a restart: Echo's meta, `start..end`.
fn listed(iid: InstanceId, start: u64, end: u64) -> JournalInfo {
    let meta = JournalMeta::new(echo_id(), Some(echo_hash()), Vec::new(), "s".to_owned()).encode();
    let range = (Offset::from_raw(start), Offset::from_raw(end));
    JournalInfo::new(journal(iid), meta, range, Vec::new())
}

fn create(r: &mut AppsRunner) -> InstanceId {
    let action = AppsAction::Create {
        app: echo_id(),
        package: Some(echo_hash()),
        params: Vec::new(),
        session: "s".to_owned(),
    };
    r.on_apps_action(action).1.unwrap().unwrap()
}

fn full(r: &AppsRunner, iid: InstanceId) -> (u64, Vec<u8>) {
    match r.state_sync(iid, None) {
        Some(StateSync::Full { version, bytes }) => (version, bytes),
        other => panic!("a reader with no version gets the full state: {other:?}"),
    }
}

#[test]
fn a_resume_from_before_a_restart_gets_the_full_state() {
    // Boot 1: three outputs. The deck holds the newest version of both streams.
    let mut first = boot(1_000);
    let iid = create(&mut first);
    let entries = [(0, &b"ab"[..]), (2, b"c"), (3, b"d")];
    for (off, bytes) in entries {
        first.on_proc_reply(out(iid, off, bytes));
    }
    let (held, before) = full(&first, iid);
    let (apps_held, _) = first.apps_state();

    // Boot 2, one second later: the same journal replays to the same state.
    let mut second = boot(2_000);
    second.restore(vec![listed(iid, 0, 4)]);
    for (off, bytes) in entries {
        second.on_proc_reply(out(iid, off, bytes));
    }
    let (version, after) = full(&second, iid);
    assert_eq!(after, before, "the instance keeps its state");
    assert!(version > held, "version={version} held={held}");

    // Sync to viewers rule 11: never deltas against a version of another boot.
    assert_eq!(
        second.state_sync(iid, Some(held)),
        Some(StateSync::Full {
            version,
            bytes: after
        })
    );
    let (apps_version, _) = second.apps_state();
    assert!(
        apps_version > apps_held,
        "worker/apps: {apps_version} > {apps_held}"
    );
}

#[test]
fn versions_of_a_boot_start_at_its_epoch() {
    let mut r = boot(5);
    let base = BootEpoch::from_unix_millis(5).base();
    assert_eq!(base, 5 << BootEpoch::SHIFT);
    let iid = create(&mut r);
    let (created, _) = full(&r, iid);
    assert!(created >= base, "created={created} base={base}");
    let effects = r.on_proc_reply(out(iid, 0, b"x"));
    let delta = effects.iter().find_map(|e| match e {
        AppsEffect::Delta { version, .. } => Some(*version),
        _ => None,
    });
    assert_eq!(delta, Some(created + 1), "deltas count up from the epoch");
    assert!(r.apps_state().0 > base);
}

#[test]
fn a_gap_during_replay_marks_the_instance_degraded() {
    let mut r = boot(1);
    let iid = InstanceId::from_raw(3);
    r.restore(vec![listed(iid, 10, 20)]);
    assert_eq!(
        r.records().first().map(InstanceRecord::phase),
        Some(InstancePhase::Running)
    );
    let gap = Reply::Gap {
        journal: journal(iid),
        new_start: Offset::from_raw(15),
    };
    let effects = r.on_proc_reply(gap);
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, AppsEffect::AppsChanged { .. })),
        "the records change: {effects:?}"
    );
    assert_eq!(
        r.records().first().map(InstanceRecord::phase),
        Some(InstancePhase::Degraded)
    );
}

#[test]
fn a_clean_point_trims_live_and_never_in_replay() {
    let is_trim = |e: &AppsEffect| matches!(e, AppsEffect::Proc(Request::Trim { .. }));
    // Replay: the trim already happened before the restart; it is dropped like every effect.
    let mut r = boot(1);
    let iid = InstanceId::from_raw(3);
    r.restore(vec![listed(iid, 0, 10)]);
    assert!(!r.on_proc_reply(out(iid, 0, b"clear")).iter().any(is_trim));
    // Live: the host trims the journal up to the entry's offset.
    let effects = r.on_proc_reply(out(iid, 10, b"clear"));
    let want = AppsEffect::Proc(Request::Trim {
        journal: journal(iid),
        upto: Offset::from_raw(10),
    });
    assert!(effects.contains(&want), "effects={effects:?}");
}
