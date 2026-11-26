//! Contract tests of the apps runner.

use super::*;
use crate::test_app::{echo_hash, echo_id, echo_installed};
use lattice_app::AppError;
use lattice_app::{InstancePhase, ProcRef};
use lattice_proc_ipc::{EntryKind, ExitStatus, JournalId, Offset, ProcId};

fn runner() -> AppsRunner {
    let mut r = AppsRunner::new(BootEpoch::from_unix_millis(0));
    r.install(echo_hash(), echo_installed());
    r
}

fn create(r: &mut AppsRunner) -> (Vec<AppsEffect>, InstanceId) {
    let action = AppsAction::Create {
        app: echo_id(),
        package: Some(echo_hash()),
        params: Vec::new(),
        session: "s".to_owned(),
    };
    let (effects, res) = r.on_apps_action(action);
    (effects, res.unwrap().unwrap())
}

fn journal(iid: InstanceId) -> JournalId {
    JournalId::try_new(iid.raw().to_be_bytes().to_vec()).unwrap()
}

fn exit(iid: InstanceId) -> Reply {
    Reply::Entry {
        journal: journal(iid),
        off: Offset::from_raw(0),
        entry: EntryKind::Exit {
            proc: ProcId::from_raw(1),
            status: ExitStatus::Code(0),
        },
    }
}

fn is_drop(e: &AppsEffect) -> bool {
    matches!(e, AppsEffect::Proc(Request::Drop { .. }))
}

#[test]
fn create_opens_a_journal_before_it_spawns() {
    let (effects, iid) = create(&mut runner());
    assert!(matches!(
        effects.first(),
        Some(AppsEffect::Proc(Request::Open(_)))
    ));
    assert!(matches!(
        effects.get(1),
        Some(AppsEffect::Proc(Request::Spawn(_)))
    ));
    assert_eq!(iid, InstanceId::from_raw(1));
}

#[test]
fn every_spawn_names_the_terminal_the_emulator_implements() {
    // 04-worker.md, Child env rule 3.
    let (effects, iid) = create(&mut runner());
    let Some(AppsEffect::Proc(Request::Spawn(spec))) = effects.get(1) else {
        panic!("a spawn: {effects:?}");
    };
    let iid = iid.to_string();
    let want: [(&[u8], &[u8]); 3] = [
        (b"TERM", b"xterm-256color"),
        (b"COLORTERM", b"truecolor"),
        (b"LATTICE_INSTANCE_ID", iid.as_bytes()),
    ];
    for (name, value) in want {
        let got = spec.env().iter().find(|(k, _)| k.as_slice() == name);
        assert_eq!(
            got.map(|(_, v)| v.as_slice()),
            Some(value),
            "case={}",
            String::from_utf8_lossy(name)
        );
    }
}

#[test]
fn create_of_an_unknown_app_opens_nothing() {
    let action = AppsAction::Create {
        app: AppId::try_new("test.missing").unwrap(),
        package: Some(echo_hash()),
        params: Vec::new(),
        session: "s".to_owned(),
    };
    let (effects, res) = runner().on_apps_action(action);
    assert_eq!(res, Err("worker.package_app_mismatch".to_owned()));
    assert!(effects.is_empty());
    let unknown = AppsAction::Create {
        app: echo_id(),
        package: Some(lattice_app::PackageHash::from_bytes([9; 32])),
        params: Vec::new(),
        session: "s".to_owned(),
    };
    assert_eq!(
        runner().on_apps_action(unknown).1,
        Err("worker.unknown_package".to_owned())
    );
    let none = AppsAction::Create {
        app: echo_id(),
        package: None,
        params: Vec::new(),
        session: "s".to_owned(),
    };
    assert_eq!(
        runner().on_apps_action(none).1,
        Err("worker.no_package".to_owned())
    );
}

#[test]
fn close_drops_the_journal_only_after_every_exit() {
    let mut r = runner();
    let (_, iid) = create(&mut r);
    let (effects, _) = r.on_apps_action(AppsAction::Close { iid });
    assert!(!effects.iter().any(is_drop));
    assert_eq!(
        r.records().first().map(InstanceRecord::phase),
        Some(InstancePhase::Closing)
    );
    assert!(r.on_proc_reply(exit(iid)).iter().any(is_drop));
}

#[test]
fn finish_closes_the_instance_without_a_request() {
    let mut r = runner();
    let (_, iid) = create(&mut r);
    assert!(r.on_proc_reply(exit(iid)).iter().any(is_drop));
    let dropped = Reply::Dropped {
        journal: journal(iid),
    };
    assert!(
        r.on_proc_reply(dropped)
            .iter()
            .any(|e| matches!(e, AppsEffect::AppsChanged { .. }))
    );
    assert!(r.records().is_empty());
}

#[test]
fn replay_rebuilds_state_and_runs_no_effects() {
    let mut r = runner();
    let meta = JournalMeta::new(echo_id(), Some(echo_hash()), Vec::new(), "s".to_owned()).encode();
    let range = (Offset::from_raw(0), Offset::from_raw(3));
    let info = JournalInfo::new(journal(InstanceId::from_raw(4)), meta, range, Vec::new());
    let effects = r.restore(vec![info]);
    assert!(
        effects
            .iter()
            .all(|e| matches!(e, AppsEffect::Proc(Request::Subscribe { .. })))
    );
    let out = Reply::Entry {
        journal: journal(InstanceId::from_raw(4)),
        off: Offset::from_raw(0),
        entry: EntryKind::Out {
            proc: ProcId::from_raw(1),
            bytes: b"abc".to_vec(),
        },
    };
    let effects = r.on_proc_reply(out);
    // The entry ends at the listed end: replay is done and `resumed` runs live (Echo writes).
    let procs: Vec<_> = effects
        .iter()
        .filter(|e| matches!(e, AppsEffect::Proc(_)))
        .collect();
    assert_eq!(
        procs,
        vec![&resumed_write()],
        "only the effect of `resumed`"
    );
}

/// What Echo's `resumed` sends: `resumed` to process 1.
fn resumed_write() -> AppsEffect {
    AppsEffect::Proc(Request::Input {
        id: ProcId::from_raw(1),
        bytes: b"resumed".to_vec(),
    })
}

fn out_entry(iid: InstanceId, off: u64, bytes: &[u8]) -> Reply {
    Reply::Entry {
        journal: journal(iid),
        off: Offset::from_raw(off),
        entry: EntryKind::Out {
            proc: ProcId::from_raw(1),
            bytes: bytes.to_vec(),
        },
    }
}

#[test]
fn resumed_runs_once_when_replay_reaches_the_listed_end() {
    let mut r = runner();
    let iid = InstanceId::from_raw(4);
    let meta = JournalMeta::new(echo_id(), Some(echo_hash()), Vec::new(), "s".to_owned()).encode();
    let range = (Offset::from_raw(10), Offset::from_raw(15));
    r.restore(vec![JournalInfo::new(
        journal(iid),
        meta,
        range,
        Vec::new(),
    )]);
    let count = |effects: &[AppsEffect]| effects.iter().filter(|e| **e == resumed_write()).count();
    assert_eq!(
        count(&r.on_proc_reply(out_entry(iid, 10, b"abc"))),
        0,
        "still replaying"
    );
    assert_eq!(
        count(&r.on_proc_reply(out_entry(iid, 13, b"de"))),
        1,
        "reached 15"
    );
    assert_eq!(
        count(&r.on_proc_reply(out_entry(iid, 15, b"f"))),
        0,
        "live: once only"
    );
}

#[test]
fn a_new_instance_and_an_empty_journal_resume_at_once() {
    let resumed = || crate::test_app::RESUMED.with(std::cell::Cell::get);
    let before = resumed();
    let mut r = runner();
    create(&mut r);
    assert_eq!(resumed() - before, 1);
    // An empty listed journal (start == end) resumes at restore.
    let meta = JournalMeta::new(echo_id(), Some(echo_hash()), Vec::new(), "s".to_owned()).encode();
    let range = (Offset::from_raw(3), Offset::from_raw(3));
    let info = JournalInfo::new(journal(InstanceId::from_raw(5)), meta, range, Vec::new());
    r.restore(vec![info]);
    assert_eq!(resumed() - before, 2);
}

#[test]
fn instances_never_see_each_other() {
    let mut r = runner();
    let (_, a) = create(&mut r);
    let (_, b) = create(&mut r);
    let (effects, res) = r.on_instance_action(a, &[0, 0]);
    assert!(res.is_ok());
    let touches_b = |e: &AppsEffect| matches!(e, AppsEffect::Delta { iid, .. } if *iid == b);
    assert!(!effects.iter().any(touches_b));
    let (_, kill) = r.on_apps_action(AppsAction::Kill { iid: b });
    assert!(kill.is_ok());
}

/// In-memory `latticed-proc`: captures requests and answers like the daemon would.
#[derive(Default)]
struct FakeProc {
    requests: Vec<Request>,
    next_proc: u64,
}

impl FakeProc {
    /// Takes the `Proc` effects; returns the other effects and the replies, in order.
    fn run(&mut self, effects: Vec<AppsEffect>) -> (Vec<AppsEffect>, Vec<Reply>) {
        let mut rest = Vec::new();
        let mut replies = Vec::new();
        for e in effects {
            let AppsEffect::Proc(req) = e else {
                rest.push(e);
                continue;
            };
            match &req {
                Request::Spawn(_) => {
                    self.next_proc += 1;
                    replies.push(Reply::Spawned {
                        id: ProcId::from_raw(self.next_proc),
                        pid: 1000,
                    });
                }
                Request::Drop { journal } => replies.push(Reply::Dropped {
                    journal: journal.clone(),
                }),
                _ => {}
            }
            self.requests.push(req);
        }
        (rest, replies)
    }
}

fn is_changed(e: &AppsEffect) -> bool {
    matches!(e, AppsEffect::AppsChanged { .. })
}

fn entry(iid: InstanceId, off: u64, entry: EntryKind) -> Reply {
    Reply::Entry {
        journal: journal(iid),
        off: Offset::from_raw(off),
        entry,
    }
}

/// Feeds replies to the runner; returns every effect it produced.
fn feed(r: &mut AppsRunner, replies: Vec<Reply>) -> Vec<AppsEffect> {
    replies
        .into_iter()
        .flat_map(|x| r.on_proc_reply(x))
        .collect()
}

#[test]
fn instance_lifecycle_with_a_fake_proc_daemon() {
    let mut r = runner();
    let mut proc = FakeProc::default();
    let (effects, iid) = create(&mut r);
    let (rest, replies) = proc.run(effects);
    assert!(matches!(
        proc.requests.as_slice(),
        [
            Request::Open(_),
            Request::Spawn(_),
            Request::Subscribe { .. }
        ]
    ));
    assert!(matches!(
        rest.as_slice(),
        [AppsEffect::AppsChanged { version: 1, .. }]
    ));

    let changed = feed(&mut r, replies);
    assert!(matches!(
        changed.as_slice(),
        [AppsEffect::AppsChanged { version: 2, .. }]
    ));
    let rec = r.records();
    assert_eq!(
        rec.first().map(InstanceRecord::procs),
        Some(&[ProcRef::from_raw(1)][..])
    );
    output_then_exit_then_drop(r, proc, iid);
}

/// Second half of the lifecycle: output → delta, exit → finish → `Drop`, `Dropped` → gone.
fn output_then_exit_then_drop(mut r: AppsRunner, mut proc: FakeProc, iid: InstanceId) {
    let out = EntryKind::Out {
        proc: ProcId::from_raw(1),
        bytes: b"hello".to_vec(),
    };
    let effects = r.on_proc_reply(entry(iid, 0, out));
    let want = AppsEffect::Delta {
        iid,
        version: 1,
        bytes: lattice_state::encode(&5u64).unwrap(),
    };
    assert_eq!(effects, vec![want]);

    let exit = EntryKind::Exit {
        proc: ProcId::from_raw(1),
        status: ExitStatus::Code(0),
    };
    let (rest, replies) = proc.run(r.on_proc_reply(entry(iid, 5, exit)));
    assert!(matches!(proc.requests.last(), Some(Request::Drop { .. })));
    assert!(
        !proc
            .requests
            .iter()
            .any(|q| matches!(q, Request::Signal { .. })),
        "no live process left to signal"
    );
    assert!(rest.iter().any(is_changed));
    assert!(feed(&mut r, replies).iter().any(is_changed));
    assert!(r.records().is_empty());
    assert_eq!(r.state_sync(iid, None), None);
}

#[test]
fn close_hangs_up_live_processes_and_kill_escalates() {
    let mut r = runner();
    let mut proc = FakeProc::default();
    let (effects, iid) = create(&mut r);
    let (_, replies) = proc.run(effects);
    feed(&mut r, replies);

    let (effects, res) = r.on_apps_action(AppsAction::Close { iid });
    assert_eq!(res, Ok(None));
    let hup = Request::Signal {
        id: ProcId::from_raw(1),
        signo: 1,
    };
    assert!(effects.contains(&AppsEffect::Proc(hup)));
    assert!(!effects.iter().any(is_drop));

    let (effects, _) = r.on_apps_action(AppsAction::Kill { iid });
    let kill = Request::Signal {
        id: ProcId::from_raw(1),
        signo: 9,
    };
    assert!(effects.contains(&AppsEffect::Proc(kill)));
    let exit = EntryKind::Exit {
        proc: ProcId::from_raw(1),
        status: ExitStatus::Signal(9),
    };
    assert!(r.on_proc_reply(entry(iid, 0, exit)).iter().any(is_drop));
}

#[test]
fn a_process_spawned_while_closing_gets_the_close_signal() {
    let mut r = runner();
    let (_, iid) = create(&mut r);
    assert_eq!(r.on_apps_action(AppsAction::Close { iid }).1, Ok(None));
    let effects = r.on_proc_reply(Reply::Spawned {
        id: ProcId::from_raw(4),
        pid: 1,
    });
    let hup = Request::Signal {
        id: ProcId::from_raw(4),
        signo: 1,
    };
    assert!(effects.contains(&AppsEffect::Proc(hup)));
    assert!(!effects.iter().any(is_drop));
}

#[test]
fn a_failed_spawn_releases_the_close_flow() {
    let mut r = runner();
    let (_, iid) = create(&mut r);
    assert_eq!(r.on_apps_action(AppsAction::Close { iid }).1, Ok(None));
    let failed = Reply::Error {
        code: lattice_proc_ipc::ErrorCode::SpawnFailed,
        msg: String::new(),
    };
    assert!(r.on_proc_reply(failed).iter().any(is_drop));
}

/// 06-apps.md, Instances rules 10-11: a failed spawn leaves the instance listed as `failed`;
/// nothing drops it until a `close`.
#[test]
fn a_failed_spawn_keeps_the_instance_listed_until_a_close() {
    let mut r = runner();
    let (_, iid) = create(&mut r);
    let failed = Reply::Error {
        code: lattice_proc_ipc::ErrorCode::SpawnFailed,
        msg: String::new(),
    };
    assert!(!r.on_proc_reply(failed).iter().any(is_drop));
    let phase = |r: &AppsRunner| {
        r.records()
            .iter()
            .find(|x| x.iid() == iid)
            .map(InstanceRecord::phase)
    };
    assert_eq!(phase(&r), Some(InstancePhase::Failed));
    let (effects, res) = r.on_apps_action(AppsAction::Close { iid });
    assert_eq!(res, Ok(None));
    assert!(effects.iter().any(is_drop), "the user's close drops it");
}

#[test]
fn resume_sends_missing_deltas_or_the_full_state() {
    let mut r = runner();
    let (_, iid) = create(&mut r);
    let out = EntryKind::Out {
        proc: ProcId::from_raw(1),
        bytes: b"ab".to_vec(),
    };
    r.on_proc_reply(entry(iid, 0, out));
    let two = lattice_state::encode(&2u64).unwrap();
    assert_eq!(
        r.state_sync(iid, Some(0)),
        Some(StateSync::Deltas(vec![(1, two.clone())]))
    );
    assert_eq!(
        r.state_sync(iid, None),
        Some(StateSync::Full {
            version: 1,
            bytes: two
        })
    );
}

#[test]
fn errors_of_actions_are_stable_codes() {
    let mut r = runner();
    let missing = InstanceId::from_raw(9);
    let (_, res) = r.on_apps_action(AppsAction::Close { iid: missing });
    assert_eq!(res, Err("worker.no_instance".to_owned()));
    let (_, res) = r.on_instance_action(missing, &[0, 0]);
    assert_eq!(res, Err(AppError::platform("worker.no_instance")));
    let (_, iid) = create(&mut r);
    let (effects, res) = r.on_instance_action(iid, &[0, 1, 7]);
    assert_eq!(
        res.map_err(|e| e.code().to_owned()),
        Err("refused".to_owned())
    );
    assert!(effects.is_empty());
    let bad_params = AppsAction::Create {
        app: echo_id(),
        package: Some(echo_hash()),
        params: vec![1],
        session: "s".to_owned(),
    };
    let (effects, res) = r.on_apps_action(bad_params);
    assert_eq!(res, Err("worker.bad_params".to_owned()));
    assert!(effects.is_empty());
}

#[test]
fn restore_marks_an_undecodable_meta_failed_and_continues_ids() {
    let mut r = runner();
    let range = (Offset::from_raw(0), Offset::from_raw(0));
    let info = JournalInfo::new(
        journal(InstanceId::from_raw(7)),
        vec![0xff],
        range,
        Vec::new(),
    );
    assert!(r.restore(vec![info]).is_empty());
    assert_eq!(
        r.records().first().map(InstanceRecord::phase),
        Some(InstancePhase::Failed)
    );
    let (_, iid) = create(&mut r);
    assert_eq!(iid, InstanceId::from_raw(8));
}
