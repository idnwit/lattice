//! One instance inside the apps runner: its runner, processes, close flow, and effects.

use std::collections::BTreeSet;

use lattice_app::{
    Effect, InstanceId, InstancePhase, InstanceRecord, JournalMeta, ProcRef, Signal,
};
use lattice_proc_ipc::{JournalId, Offset, ProcId, Request, SpawnSpec};

use super::AppsEffect;
use crate::delta_queue::DeltaQueue;
use crate::runner::{DynRunner, Input, Mode, Step};
use lattice_package::SpawnTemplate;

use crate::spawn::{DEFAULT_SIZE, resolve};

/// `SIGHUP`: sent by `close` and `finish` (06-apps.md, Instances rule 5).
pub(super) const SIGHUP: i32 = 1;
/// `SIGINT`.
const SIGINT: i32 = 2;
/// `SIGKILL`: sent by `kill`.
pub(super) const SIGKILL: i32 = 9;
/// `SIGTERM`.
const SIGTERM: i32 = 15;

/// What one call into the apps runner produced, shared by every instance it touched.
#[derive(Debug, Default)]
pub(super) struct Out {
    /// Effects for the host, in order.
    pub(super) effects: Vec<AppsEffect>,
    /// `Spawn` requests sent, oldest first; `Spawned` replies answer them in this order.
    pub(super) spawned: Vec<InstanceId>,
    /// The `worker/apps` records changed.
    pub(super) records_dirty: bool,
}

impl Out {
    fn proc(&mut self, req: Request) {
        self.effects.push(AppsEffect::Proc(req));
    }
}

/// One live instance.
pub(super) struct Instance {
    pub(super) iid: InstanceId,
    pub(super) journal: JournalId,
    pub(super) meta: JournalMeta,
    /// `Running`, `Degraded`, or `Failed`; `Closing` is derived from `closing`.
    pub(super) phase: InstancePhase,
    /// `None` when the instance failed to build or start.
    pub(super) runner: Option<Box<dyn DynRunner>>,
    pub(super) deltas: DeltaQueue,
    /// The app's `[[spawn]]` templates (manifest).
    pub(super) spawn: Vec<SpawnTemplate>,
    /// Entries below this offset are replayed (restore); `None` when live.
    pub(super) replay_until: Option<Offset>,
    /// Processes not yet seen exiting.
    live: BTreeSet<ProcId>,
    /// Every process ever seen, live or not.
    known: BTreeSet<ProcId>,
    /// `Spawn` requests without a `Spawned` reply yet.
    pending: u32,
    /// The signal of the close flow, once started.
    closing: Option<i32>,
    drop_sent: bool,
    /// PTY size for the next spawn: the last size the runner asked for.
    size: (u16, u16),
}

impl Instance {
    /// A new instance with no process yet.
    pub(super) fn new(
        iid: InstanceId,
        journal: JournalId,
        meta: JournalMeta,
        runner: Option<Box<dyn DynRunner>>,
    ) -> Self {
        let phase = if runner.is_some() {
            InstancePhase::Running
        } else {
            InstancePhase::Failed
        };
        Self {
            iid,
            journal,
            meta,
            phase,
            runner,
            deltas: DeltaQueue::default(),
            spawn: Vec::new(),
            replay_until: None,
            live: BTreeSet::new(),
            known: BTreeSet::new(),
            pending: 0,
            closing: None,
            drop_sent: false,
            size: DEFAULT_SIZE,
        }
    }

    /// Its `worker/apps` row.
    pub(super) fn record(&self) -> InstanceRecord {
        let mut r = InstanceRecord::new(self.iid, &self.meta);
        let phase = if self.closing.is_some() {
            InstancePhase::Closing
        } else {
            self.phase
        };
        r.set_phase(phase);
        r.set_procs(self.live.iter().map(|p| proc_ref(*p)).collect());
        r
    }

    /// Records a committed delta and runs (or, in replay, drops) the step's effects.
    pub(super) fn commit(&mut self, step: Step, entry: Option<Offset>, out: &mut Out) {
        if let Some(delta) = step.delta {
            let version = self.deltas.push(delta.clone());
            out.effects.push(AppsEffect::Delta {
                iid: self.iid,
                version,
                bytes: delta,
            });
        }
        for effect in step.effects {
            self.run_effect(effect, entry, out);
        }
    }

    /// Feeds one input to the runner; a failed instance ignores it. A runner that dies on it
    /// (a WASM trap) is dropped and the instance becomes `failed`.
    pub(super) fn feed(&mut self, input: Input<'_>, mode: Mode, out: &mut Out) -> Option<Step> {
        let runner = self.runner.as_mut()?;
        let step = runner.step(input, mode);
        if runner.is_dead() {
            tracing::warn!(iid = %self.iid, "runner died; instance failed");
            self.runner = None;
            self.phase = InstancePhase::Failed;
            out.records_dirty = true;
        }
        Some(step)
    }

    fn run_effect(&mut self, effect: Effect, entry: Option<Offset>, out: &mut Out) {
        match effect {
            Effect::Spawn {
                template,
                cwd,
                size,
            } => self.spawn(&template, &cwd, size, out),
            Effect::Write { proc, bytes } => out.proc(Request::Input {
                id: proc_id(proc),
                bytes,
            }),
            Effect::Resize { proc, cols, rows } => {
                self.size = (cols, rows);
                out.proc(Request::Resize {
                    id: proc_id(proc),
                    cols,
                    rows,
                });
            }
            Effect::Signal { proc, signal } => out.proc(Request::Signal {
                id: proc_id(proc),
                signo: signo(signal),
            }),
            Effect::Trim => match entry {
                Some(upto) => out.proc(Request::Trim {
                    journal: self.journal.clone(),
                    upto,
                }),
                None => tracing::debug!(iid = %self.iid, "trim outside a journal entry ignored"),
            },
            Effect::Finish => self.close(SIGHUP, out),
        }
    }

    /// Resolves a `[[spawn]]` template into `Spawn`. Never while closing.
    fn spawn(&mut self, template: &str, cwd: &str, size: Option<(u16, u16)>, out: &mut Out) {
        if self.closing.is_some() {
            return;
        }
        let Some(t) = self.spawn.iter().find(|t| t.name == template) else {
            tracing::warn!(iid = %self.iid, template, "no spawn template");
            return;
        };
        if let Some(size) = size {
            self.size = size;
        }
        let (argv, cwd) = resolve(t, cwd);
        // 04-worker.md, Structured events: LATTICE_PROC_ID is assigned by `latticed-proc`
        // and unknown before `Spawned`, so only LATTICE_INSTANCE_ID is set (TBD in 04).
        let spec = SpawnSpec::try_new(self.journal.clone(), argv, cwd, self.size.0, self.size.1)
            .and_then(|s| {
                s.with_env(
                    b"LATTICE_INSTANCE_ID".to_vec(),
                    self.iid.to_string().into_bytes(),
                )
            });
        match spec {
            Ok(spec) => {
                self.pending = self.pending.saturating_add(1);
                out.spawned.push(self.iid);
                out.proc(Request::Spawn(spec));
            }
            Err(e) => tracing::warn!(iid = %self.iid, code = e.code(), "bad spawn template"),
        }
    }

    /// Starts (or escalates) the close flow: signal every live process, drop when none is left.
    pub(super) fn close(&mut self, signo: i32, out: &mut Out) {
        let escalate = signo == SIGKILL && self.closing != Some(SIGKILL);
        if self.closing.is_none() || escalate {
            self.closing = Some(signo);
            out.records_dirty = true;
            for id in &self.live {
                out.proc(Request::Signal { id: *id, signo });
            }
        }
        self.drop_if_done(out);
    }

    /// Sends `Drop` once the close flow runs and every process has exited.
    pub(super) fn drop_if_done(&mut self, out: &mut Out) {
        if self.closing.is_some() && self.live.is_empty() && self.pending == 0 && !self.drop_sent {
            self.drop_sent = true;
            out.proc(Request::Drop {
                journal: self.journal.clone(),
            });
        }
    }

    /// `Spawned` answered one of our `Spawn` requests.
    pub(super) fn on_spawned(&mut self, id: ProcId, out: &mut Out) {
        if !self.known.insert(id) {
            return; // already adopted from one of its journal entries
        }
        self.pending = self.pending.saturating_sub(1);
        self.live.insert(id);
        out.records_dirty = true;
        if let Some(signo) = self.closing {
            out.proc(Request::Signal { id, signo });
        }
    }

    /// `Spawn` failed: no process will come.
    pub(super) fn on_spawn_failed(&mut self, out: &mut Out) {
        self.pending = self.pending.saturating_sub(1);
        if self.closing.is_none() {
            self.phase = InstancePhase::Failed;
        }
        out.records_dirty = true;
        self.drop_if_done(out);
    }

    /// An entry names `id`: adopt it if new (restore, or an entry before `Spawned`).
    pub(super) fn saw(&mut self, id: ProcId, out: &mut Out) {
        if self.known.insert(id) {
            self.pending = self.pending.saturating_sub(1);
            self.live.insert(id);
            out.records_dirty = true;
        }
    }

    /// A process `latticed-proc` listed for this journal at restore.
    pub(super) fn listed(&mut self, id: ProcId, exited: bool) {
        self.known.insert(id);
        if !exited {
            self.live.insert(id);
        }
    }

    /// An `Exit` entry for `id`.
    pub(super) fn exited(&mut self, id: ProcId, out: &mut Out) {
        self.saw(id, out);
        if self.live.remove(&id) {
            out.records_dirty = true;
        }
    }

    /// Replay hit a gap: the state is partial.
    pub(super) fn degrade(&mut self, out: &mut Out) {
        if self.phase == InstancePhase::Running {
            self.phase = InstancePhase::Degraded;
            out.records_dirty = true;
        }
    }

    /// The mode for an entry at `off`: replay below the listed end.
    pub(super) fn mode_at(&self, off: Offset) -> Mode {
        match self.replay_until {
            Some(end) if off < end => Mode::Replay,
            Some(_) | None => Mode::Live,
        }
    }

    /// Replay covered the journal up to `upto`: at or past the listed end it is done, and
    /// the runner gets `resumed` (06-apps.md, Runner rule 4).
    pub(super) fn replayed_to(&mut self, upto: Offset, out: &mut Out) {
        if self.replay_until.is_some_and(|end| upto >= end) {
            self.replay_until = None;
            self.resume(out);
        }
    }

    /// Calls `resumed` live and runs its effects.
    pub(super) fn resume(&mut self, out: &mut Out) {
        if let Some(step) = self.feed(Input::Resumed, Mode::Live, out) {
            self.commit(step, None, out);
        }
    }
}

/// The runner's name for a process.
pub(super) fn proc_ref(id: ProcId) -> ProcRef {
    ProcRef::from_raw(id.raw())
}

/// The daemon's id of a process the runner names.
fn proc_id(proc: ProcRef) -> ProcId {
    ProcId::from_raw(proc.raw())
}

/// Signal numbers; identical on Linux and macOS.
fn signo(signal: Signal) -> i32 {
    match signal {
        Signal::Interrupt => SIGINT,
        Signal::Hangup => SIGHUP,
        Signal::Terminate => SIGTERM,
    }
}

/// The journal of an instance: its id as 8 big-endian bytes (04-worker.md, Journal).
pub(super) fn journal_of(iid: InstanceId) -> Option<JournalId> {
    JournalId::try_new(iid.raw().to_be_bytes().to_vec()).ok()
}

/// The instance of a journal; `None` when it is not 8 bytes.
pub(super) fn iid_of(journal: &JournalId) -> Option<InstanceId> {
    let raw: [u8; 8] = journal.as_bytes().try_into().ok()?;
    Some(InstanceId::from_raw(u64::from_be_bytes(raw)))
}
