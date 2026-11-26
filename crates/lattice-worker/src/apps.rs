//! The apps runner: instances, journals, effects (06-apps.md, Instances and Restore). Sans-IO.
//!
//! Design notes (M1):
//!
//! - Journal id = `iid` as 8 big-endian bytes. Budget: [`JOURNAL_BUDGET`].
//! - Apps are packages (D44): [`AppsRunner::install`] adds one by hash; `create` names one.
//! - `Spawn` effects resolve through the package manifest's `[[spawn]]` templates.
//! - `Spawned` carries no journal id. `latticed-proc` answers requests in order on one
//!   socket, so the runner matches each `Spawned` (or `Error{SpawnFailed}`) to the oldest
//!   `Spawn` still due. An entry that names a process not seen yet adopts it.
//! - Restore takes the live processes from `List` (`ProcInfo` without an exit status).
//! - `worker/apps` is synced as full states: every change emits one
//!   [`AppsEffect::AppsChanged`] with the whole encoded record list and a new version.
//! - Restore runs before any deck connects: it emits only `Proc` effects; deltas go to the queues.
//! - `resumed` runs once replay reaches the listed `end_off` (right away when the journal is
//!   empty), and right after `start` for a new instance. Its effects run live.

use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;

use lattice_app::{
    AppError, AppId, AppsAction, BAD_REPLY, InstanceId, InstanceRecord, JournalMeta, PackageHash,
};
use lattice_package::Manifest;
use lattice_proc_ipc::{EntryKind, ErrorCode, JournalInfo, Offset, OpenSpec, Reply, Request};

use crate::delta_queue::{BootEpoch, Resume};
use crate::runner::{DynRunner, Input, Mode, Step};

mod instance;
use instance::{Instance, Out, SIGHUP, SIGKILL, iid_of, journal_of, proc_ref};

/// Journal size of an `Exit` entry (04-worker.md, Journal sizing); `Out` counts its bytes.
const EXIT_SIZE: u64 = 8;

/// Byte budget of a new journal (proposed: the manifest's `journal_mb = 16`; 04-worker.md TBD).
pub(crate) const JOURNAL_BUDGET: u64 = lattice_proc_ipc::DEFAULT_JOURNAL_BUDGET;

/// App id shown for a listed journal whose meta does not decode (its instance is `failed`).
const UNKNOWN_APP: &str = "lattice.unknown";

/// What the host must do after the apps runner handled an input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppsEffect {
    /// Send a request to `latticed-proc`.
    Proc(Request),
    /// A committed delta of an instance, to fan out to its subscribed decks.
    Delta {
        /// The instance.
        iid: InstanceId,
        /// Version of this delta.
        version: u64,
        /// Encoded delta.
        bytes: Vec<u8>,
    },
    /// The `worker/apps` records changed: the full encoded record list at `version`.
    AppsChanged {
        /// Version of the `worker/apps` state.
        version: u64,
        /// [`InstanceRecord::encode_list`] of every record.
        bytes: Vec<u8>,
    },
}

/// What a deck resuming `inst/<iid>/state` gets (06-apps.md, Sync to viewers rules 4–5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StateSync {
    /// The full state at `version`.
    Full {
        /// Version of the state.
        version: u64,
        /// Encoded state.
        bytes: Vec<u8>,
    },
    /// The missing deltas, oldest first. Empty when the deck is current.
    Deltas(Vec<(u64, Vec<u8>)>),
}

/// Builds a runner from encoded params: a WASM package (`lattice_wasm::WasmApp::instantiate`)
/// or, in tests, a native adapter (`Arc::new(hosted::<XApiRunner<App>>)`).
///
/// `None` when the params do not decode; a failed `start` shows as the [`Step`]'s error.
pub type RunnerFactory =
    Arc<dyn Fn(&[u8], Mode) -> Option<(Box<dyn DynRunner>, Step)> + Send + Sync>;

/// Turns verified package bytes into an [`Installed`] package (`latticed`: decode, compile the
/// wasm runner, write the cache). The host checks the hash before calling it.
///
/// # Errors
///
/// A stable code (`package.decode`, `wasm.compile`, ...) when the package cannot run here.
pub type PackageLoader =
    Arc<dyn Fn(&PackageHash, &[u8]) -> Result<Installed, String> + Send + Sync>;

/// One installed package: its manifest and how to build its runner.
#[derive(Clone)]
pub struct Installed {
    manifest: Manifest,
    runner: RunnerFactory,
}

impl Installed {
    /// A package whose runner `runner` builds.
    pub fn new(manifest: Manifest, runner: RunnerFactory) -> Self {
        Self { manifest, runner }
    }

    /// The manifest.
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }
}

impl std::fmt::Debug for Installed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Installed")
            .field("app", &self.manifest.id)
            .finish_non_exhaustive()
    }
}

/// All instances on this worker.
pub struct AppsRunner {
    packages: BTreeMap<PackageHash, Installed>,
    instances: BTreeMap<InstanceId, Instance>,
    next_iid: u64,
    /// Instances of the `Spawn` requests still waiting for `Spawned`, oldest first.
    spawn_queue: VecDeque<InstanceId>,
    apps_version: u64,
    /// First version of every state stream in this boot (06-apps.md, Sync to viewers rule 10).
    epoch: BootEpoch,
}

impl std::fmt::Debug for AppsRunner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppsRunner")
            .field("packages", &self.packages.values().collect::<Vec<_>>())
            .field("instances", &self.instances.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl AppsRunner {
    /// A runner with no package installed: the worker ships no app (D44).
    ///
    /// `epoch` is this boot's first version (`latticed` passes [`BootEpoch::from_unix_millis`]
    /// of its start), so versions after a restart exceed all earlier ones (06-apps.md, Sync to
    /// viewers rules 10–12).
    pub fn new(epoch: BootEpoch) -> Self {
        Self {
            packages: BTreeMap::new(),
            instances: BTreeMap::new(),
            next_iid: 1,
            spawn_queue: VecDeque::new(),
            apps_version: epoch.base(),
            epoch,
        }
    }

    /// Installs a verified package under its hash (from the cache at start, or fetched).
    pub fn install(&mut self, hash: PackageHash, package: Installed) {
        tracing::info!(app = package.manifest.id.as_str(), %hash, "package installed");
        self.packages.insert(hash, package);
    }

    /// True when the package is installed.
    pub fn has_package(&self, hash: &PackageHash) -> bool {
        self.packages.contains_key(hash)
    }

    /// The package `meta` names, if installed and of the same app.
    fn package_of(&self, meta: &JournalMeta) -> Result<&Installed, String> {
        let hash = meta
            .package()
            .ok_or_else(|| "worker.no_package".to_owned())?;
        let p = self
            .packages
            .get(&hash)
            .ok_or_else(|| "worker.unknown_package".to_owned())?;
        if p.manifest.id != *meta.app() {
            return Err("worker.package_app_mismatch".to_owned());
        }
        Ok(p)
    }

    /// Restore: one runner per listed journal, built from its meta (04-worker.md, `latticed`).
    pub fn restore(&mut self, journals: Vec<JournalInfo>) -> Vec<AppsEffect> {
        let mut out = Out::default();
        for info in journals {
            let Some(iid) = iid_of(info.journal()) else {
                tracing::warn!("journal id is not an instance id; skipped");
                continue;
            };
            let mut inst = self.rebuild(iid, &info);
            for p in info.procs() {
                inst.listed(p.id(), p.exit().is_some());
            }
            if inst.runner.is_some() {
                out.effects.push(AppsEffect::Proc(Request::Subscribe {
                    journal: info.journal().clone(),
                    from_off: info.range().0,
                }));
                if inst.replay_until.is_none() {
                    inst.resume(&mut out);
                }
            }
            tracing::info!(%iid, phase = ?inst.phase, "instance restored");
            self.next_iid = self.next_iid.max(iid.raw().saturating_add(1));
            self.instances.insert(iid, inst);
        }
        self.apps_version = self.apps_version.saturating_add(1);
        self.spawn_queue.extend(out.spawned);
        out.effects
    }

    /// Builds one restored instance: `new` and `start` in replay, then replay to `end_off`.
    fn rebuild(&self, iid: InstanceId, info: &JournalInfo) -> Instance {
        let meta = JournalMeta::decode(info.meta());
        let built = meta.as_ref().and_then(|m| {
            let package = self.package_of(m).ok()?;
            let (runner, step) = (package.runner)(m.params(), Mode::Replay)?;
            step.error
                .is_none()
                .then(|| (runner, step, package.manifest.spawn.clone()))
        });
        let meta = meta.unwrap_or_else(|| {
            #[allow(clippy::expect_used, reason = "a constant reverse-DNS id")]
            let app = AppId::try_new(UNKNOWN_APP).expect("valid app id");
            JournalMeta::new(app, None, Vec::new(), String::new())
        });
        let Some((runner, step, spawn)) = built else {
            return Instance::new(iid, info.journal().clone(), meta, None, self.epoch);
        };
        let mut inst = Instance::new(iid, info.journal().clone(), meta, Some(runner), self.epoch);
        inst.spawn = spawn;
        let (start, end) = info.range();
        inst.replay_until = (start < end).then_some(end);
        if let Some(delta) = step.delta {
            inst.deltas.push(delta);
        }
        inst
    }

    /// The `worker/apps` records, in instance order.
    pub fn records(&self) -> Vec<InstanceRecord> {
        self.instances.values().map(Instance::record).collect()
    }

    /// The `worker/apps` state: its version and [`InstanceRecord::encode_list`] of the records.
    pub(crate) fn apps_state(&self) -> (u64, Vec<u8>) {
        (
            self.apps_version,
            InstanceRecord::encode_list(&self.records()),
        )
    }

    /// What a deck holding `have` of `inst/<iid>/state` needs; `None` for an unknown instance.
    pub fn state_sync(&self, iid: InstanceId, have: Option<u64>) -> Option<StateSync> {
        let inst = self.instances.get(&iid)?;
        Some(match inst.deltas.resume(have) {
            Resume::Deltas(d) => {
                StateSync::Deltas(d.into_iter().map(|(v, b)| (v, b.to_vec())).collect())
            }
            Resume::FullState => StateSync::Full {
                version: inst.deltas.version(),
                bytes: inst
                    .runner
                    .as_ref()
                    .map(|r| r.snapshot())
                    .unwrap_or_default(),
            },
        })
    }

    /// A deck action on `worker/apps`. Returns the effects and the result for `ActionResult`.
    pub fn on_apps_action(
        &mut self,
        action: AppsAction,
    ) -> (Vec<AppsEffect>, Result<Option<InstanceId>, String>) {
        let mut out = Out::default();
        let res = match action {
            AppsAction::Create {
                app,
                package,
                params,
                session,
            } => self
                .create(JournalMeta::new(app, package, params, session), &mut out)
                .map(Some),
            AppsAction::Close { iid } => self.close(iid, SIGHUP, &mut out).map(|()| None),
            AppsAction::Kill { iid } => self.close(iid, SIGKILL, &mut out).map(|()| None),
        };
        (self.finish(out), res)
    }

    /// Validates the package and params, opens the journal, then runs `start` live
    /// (06-apps.md, Instances 2–3). The package must be installed: the host fetches it first.
    fn create(&mut self, meta: JournalMeta, out: &mut Out) -> Result<InstanceId, String> {
        let package = self.package_of(&meta)?;
        let spawn = package.manifest.spawn.clone();
        let (runner, step) = (package.runner)(meta.params(), Mode::Live)
            .ok_or_else(|| "worker.bad_params".to_owned())?;
        if let Some(code) = step.error {
            return Err(code);
        }
        let iid = InstanceId::from_raw(self.next_iid);
        let journal = journal_of(iid).ok_or_else(|| "worker.bad_journal".to_owned())?;
        let open = OpenSpec::try_new(journal.clone(), meta.encode(), JOURNAL_BUDGET)
            .map_err(|e| e.code().to_owned())?;
        self.next_iid = self.next_iid.saturating_add(1);
        out.effects.push(AppsEffect::Proc(Request::Open(open)));
        let mut inst = Instance::new(iid, journal.clone(), meta, Some(runner), self.epoch);
        inst.spawn = spawn;
        inst.commit(step, None, out);
        inst.resume(out);
        out.effects.push(AppsEffect::Proc(Request::Subscribe {
            journal,
            from_off: Offset::from_raw(0),
        }));
        out.records_dirty = true;
        tracing::info!(%iid, "instance created");
        self.instances.insert(iid, inst);
        Ok(iid)
    }

    fn close(&mut self, iid: InstanceId, signo: i32, out: &mut Out) -> Result<(), String> {
        let inst = self
            .instances
            .get_mut(&iid)
            .ok_or_else(|| "worker.no_instance".to_owned())?;
        inst.close(signo, out);
        Ok(())
    }

    /// A deck action on `inst/<iid>/state`, encoded for that app. Returns the effects and
    /// the reply for `ActionResult`: the encoded `Res`, or the app or platform error.
    pub fn on_instance_action(
        &mut self,
        iid: InstanceId,
        action: &[u8],
    ) -> (Vec<AppsEffect>, Result<Vec<u8>, AppError>) {
        let mut out = Out::default();
        let res = match self.instances.get_mut(&iid) {
            None => Err(AppError::platform("worker.no_instance")),
            Some(inst) => match inst.feed(Input::Action(action), Mode::Live, &mut out) {
                None => Err(AppError::platform("worker.instance_failed")),
                Some(mut step) => {
                    let reply = step
                        .reply
                        .take()
                        .unwrap_or_else(|| Err(AppError::platform(BAD_REPLY)));
                    if reply.is_ok() {
                        inst.commit(step, None, &mut out);
                    }
                    reply
                }
            },
        };
        (self.finish(out), res)
    }

    /// A reply from `latticed-proc` (journal entries, gaps, spawns, drops).
    pub fn on_proc_reply(&mut self, reply: Reply) -> Vec<AppsEffect> {
        let mut out = Out::default();
        match reply {
            Reply::Entry {
                journal,
                off,
                entry,
            } => {
                if let Some(inst) = iid_of(&journal).and_then(|i| self.instances.get_mut(&i)) {
                    on_entry(inst, off, entry, &mut out);
                }
            }
            Reply::Gap { journal, new_start } => {
                if let Some(inst) = iid_of(&journal).and_then(|i| self.instances.get_mut(&i)) {
                    on_gap(inst, new_start, &mut out);
                }
            }
            Reply::Spawned { id, .. } => {
                if let Some(inst) = self.next_spawn() {
                    inst.on_spawned(id, &mut out);
                }
            }
            Reply::Error { code, msg } => self.on_error(code, &msg, &mut out),
            Reply::Dropped { journal } => {
                if let Some(iid) = iid_of(&journal)
                    && self.instances.remove(&iid).is_some()
                {
                    tracing::info!(%iid, "instance dropped");
                    out.records_dirty = true;
                }
            }
            Reply::Hello { .. }
            | Reply::Opened { .. }
            | Reply::Trimmed { .. }
            | Reply::Journals(_) => {}
        }
        self.finish(out)
    }

    /// The instance of the oldest `Spawn` still due, if it still exists.
    fn next_spawn(&mut self) -> Option<&mut Instance> {
        let iid = self.spawn_queue.pop_front()?;
        self.instances.get_mut(&iid)
    }

    fn on_error(&mut self, code: ErrorCode, msg: &str, out: &mut Out) {
        tracing::warn!(?code, msg, "proc daemon error");
        if code == ErrorCode::SpawnFailed
            && let Some(inst) = self.next_spawn()
        {
            inst.on_spawn_failed(out);
        }
    }

    /// Queues the spawns of this call and adds one `AppsChanged` when the records changed.
    fn finish(&mut self, out: Out) -> Vec<AppsEffect> {
        let Out {
            mut effects,
            spawned,
            records_dirty,
        } = out;
        self.spawn_queue.extend(spawned);
        if records_dirty {
            self.apps_version = self.apps_version.saturating_add(1);
            effects.push(AppsEffect::AppsChanged {
                version: self.apps_version,
                bytes: InstanceRecord::encode_list(&self.records()),
            });
        }
        effects
    }
}

/// One journal entry: replay below the restore end, else live; `resumed` once replay
/// reaches the end; then the close check.
fn on_entry(inst: &mut Instance, off: Offset, entry: EntryKind, out: &mut Out) {
    inst.replayed_to(off, out);
    let mode = inst.mode_at(off);
    let (step, size) = match entry {
        EntryKind::Out { proc, bytes } => {
            inst.saw(proc, out);
            let size = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
            let step = inst.feed(Input::Output(proc_ref(proc), &bytes), mode, out);
            (step, size)
        }
        EntryKind::Exit { proc, status } => {
            inst.exited(proc, out);
            let status = match status {
                lattice_proc_ipc::ExitStatus::Code(c) => lattice_app::ExitStatus::Code(c),
                lattice_proc_ipc::ExitStatus::Signal(s) => lattice_app::ExitStatus::Signal(s),
            };
            (
                inst.feed(Input::Exit(proc_ref(proc), status), mode, out),
                EXIT_SIZE,
            )
        }
    };
    if let Some(step) = step {
        inst.commit(step, Some(off), out);
    }
    inst.replayed_to(Offset::from_raw(off.raw().saturating_add(size)), out);
    inst.drop_if_done(out);
}

/// History before `new_start` was dropped: `on_gap`, and the instance is `degraded`.
fn on_gap(inst: &mut Instance, new_start: Offset, out: &mut Out) {
    let mode = inst.mode_at(new_start);
    if let Some(step) = inst.feed(Input::Gap, mode, out) {
        inst.commit(step, None, out);
    }
    inst.degrade(out);
    inst.replayed_to(new_start, out);
}

#[cfg(test)]
mod restore_tests;
#[cfg(test)]
mod tests;
