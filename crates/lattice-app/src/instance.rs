//! `worker/apps` records and actions (06-apps.md, Instances).

use serde::{Deserialize, Serialize};

use crate::ids::{AppId, InstanceId, PackageHash, ProcRef};

/// Lifecycle of an instance. Each phase maps to a design-system glyph (TBD).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InstancePhase {
    /// Package loading, `start` running.
    Starting,
    /// Runner live.
    Running,
    /// Replay hit a gap; state is partial.
    Degraded,
    /// Runner trapped, exceeded limits, or could not restore.
    Failed,
    /// `close`, `kill`, or `finish`; waiting for processes to exit.
    Closing,
}

/// One row of the `worker/apps` state.
///
/// Built by the worker's apps runner; read by the deck's instance list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstanceRecord {
    iid: InstanceId,
    app: AppId,
    /// `None` for a built-in native app (M1).
    package: Option<PackageHash>,
    /// TBD: session id type (lands with the session model on the worker).
    session: String,
    phase: InstancePhase,
    procs: Vec<ProcRef>,
}

impl InstanceRecord {
    /// A record of a new instance, in phase `Starting` with no process yet.
    pub fn new(iid: InstanceId, meta: &JournalMeta) -> Self {
        Self {
            iid,
            app: meta.app.clone(),
            package: meta.package,
            session: meta.session.clone(),
            phase: InstancePhase::Starting,
            procs: Vec::new(),
        }
    }

    /// The instance id.
    pub fn iid(&self) -> InstanceId {
        self.iid
    }

    /// The app it runs.
    pub fn app(&self) -> &AppId {
        &self.app
    }

    /// Its package; `None` for a built-in app.
    pub fn package(&self) -> Option<PackageHash> {
        self.package
    }

    /// The owning session.
    pub fn session(&self) -> &str {
        &self.session
    }

    /// The lifecycle phase.
    pub fn phase(&self) -> InstancePhase {
        self.phase
    }

    /// Its live processes.
    pub fn procs(&self) -> &[ProcRef] {
        &self.procs
    }

    /// Moves the instance to `phase`.
    pub fn set_phase(&mut self, phase: InstancePhase) {
        self.phase = phase;
    }

    /// Replaces the list of live processes.
    pub fn set_procs(&mut self, procs: Vec<ProcRef>) {
        self.procs = procs;
    }

    /// Encodes the full `worker/apps` state: every record, in instance order (postcard).
    ///
    /// M1 syncs `worker/apps` as full states only, never deltas (06-apps.md, Instances rule 4).
    pub fn encode_list(records: &[Self]) -> Vec<u8> {
        #[allow(
            clippy::expect_used,
            reason = "postcard writes ints, strings, bytes, options, enums, and sized seqs without fail"
        )]
        lattice_state::encode(records).expect("records encode as postcard")
    }

    /// Decodes a full `worker/apps` state. `None` when the bytes are not one whole list.
    pub fn decode_list(bytes: &[u8]) -> Option<Vec<Self>> {
        lattice_state::decode(bytes).ok()
    }
}

/// Actions on the `worker/apps` stream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AppsAction {
    /// Start an instance. `ActionResult` carries the new [`InstanceId`].
    Create {
        /// App to run.
        app: AppId,
        /// Package; `None` for a built-in app.
        package: Option<PackageHash>,
        /// Encoded params, validated against the manifest.
        params: Vec<u8>,
        /// Owning session.
        session: String,
    },
    /// `SIGHUP` every process, wait for exit, then delete the instance and its journal.
    Close {
        /// Target instance.
        iid: InstanceId,
    },
    /// `SIGKILL` every process of the instance (tab context menu, `Force kill`).
    Kill {
        /// Target instance.
        iid: InstanceId,
    },
}

/// What `latticed` stores in a journal at `Open`, to rebuild the runner after a restart.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JournalMeta {
    app: AppId,
    package: Option<PackageHash>,
    params: Vec<u8>,
    session: String,
}

impl JournalMeta {
    /// Builds the meta of a new instance.
    pub fn new(app: AppId, package: Option<PackageHash>, params: Vec<u8>, session: String) -> Self {
        Self {
            app,
            package,
            params,
            session,
        }
    }

    /// The app to run.
    pub fn app(&self) -> &AppId {
        &self.app
    }

    /// Its package; `None` for a built-in app.
    pub fn package(&self) -> Option<PackageHash> {
        self.package
    }

    /// Encoded params of `create`, as the runner decodes them.
    pub fn params(&self) -> &[u8] {
        &self.params
    }

    /// The owning session.
    pub fn session(&self) -> &str {
        &self.session
    }

    /// Encodes for `Open{meta}` (postcard, D19).
    pub fn encode(&self) -> Vec<u8> {
        #[allow(
            clippy::expect_used,
            reason = "postcard writes strings, bytes, options, and arrays without fail"
        )]
        lattice_state::encode(self).expect("journal meta encodes as postcard")
    }

    /// Decodes the meta of a listed journal.
    ///
    /// # Errors
    ///
    /// `None` when the bytes do not decode; the worker marks that instance `failed`.
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        lattice_state::decode(bytes).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta() -> JournalMeta {
        JournalMeta::new(
            AppId::try_new("dev.lattice.terminal").unwrap(),
            None,
            b"params".to_vec(),
            "api-server".to_owned(),
        )
    }

    #[test]
    fn journal_meta_roundtrips() {
        let m = meta();
        assert_eq!(JournalMeta::decode(&m.encode()), Some(m));
        assert_eq!(JournalMeta::decode(b"\xff"), None);
    }

    #[test]
    fn a_new_record_starts_with_no_process() {
        let r = InstanceRecord::new(InstanceId::from_raw(1), &meta());
        assert_eq!(r.iid(), InstanceId::from_raw(1));
        assert_eq!(r.phase(), InstancePhase::Starting);
        assert!(r.procs().is_empty());
        assert_eq!(r.app().as_str(), "dev.lattice.terminal");
        assert_eq!(r.session(), "api-server");
        assert_eq!(r.package(), None);
        assert_eq!(meta().session(), "api-server");
    }

    #[test]
    fn record_list_roundtrips() {
        let mut r = InstanceRecord::new(InstanceId::from_raw(1), &meta());
        r.set_phase(InstancePhase::Running);
        r.set_procs(vec![ProcRef::from_raw(3)]);
        let list = vec![r, InstanceRecord::new(InstanceId::from_raw(2), &meta())];
        assert_eq!(
            InstanceRecord::decode_list(&InstanceRecord::encode_list(&list)),
            Some(list)
        );
        assert_eq!(InstanceRecord::decode_list(b"\xff"), None);
    }
}
