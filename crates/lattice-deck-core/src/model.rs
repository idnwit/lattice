//! Deck-side model of workers, sessions, tabs, panes (03-sessions.md, Model).

use crate::status::Status;
use lattice_key::NodeId;
use lattice_transport::PathInfo;
use std::fmt;
use std::str::FromStr;

/// Pane id `<tab>.<pane>`, 1-based (03, Behavior rule 5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PaneId {
    tab: u16,
    pane: u16,
}

/// A pane id failed to parse.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid pane id")]
pub struct PaneIdError;

impl PaneId {
    /// Builds `tab.pane`. Both must be ≥ 1.
    ///
    /// # Errors
    ///
    /// [`PaneIdError`] on zero.
    #[allow(
        clippy::todo,
        unused_variables,
        reason = "L1 skeleton: body lands with its contract test"
    )]
    pub fn try_new(tab: u16, pane: u16) -> Result<Self, PaneIdError> {
        // reject 0 in either part
        todo!()
    }
}

impl fmt::Display for PaneId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.tab, self.pane)
    }
}

impl FromStr for PaneId {
    type Err = PaneIdError;

    #[allow(
        clippy::todo,
        unused_variables,
        reason = "L1 skeleton: body lands with its contract test"
    )]
    fn from_str(s: &str) -> Result<Self, PaneIdError> {
        // split once on '.', parse both as u16, then try_new
        todo!()
    }
}

/// A paired worker as the deck sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerEntry {
    id: NodeId,
    name: String,
    path: Option<PathInfo>,
}

impl WorkerEntry {
    /// A worker named `name` (e.g. `build-01`), not yet linked.
    pub fn new(id: NodeId, name: String) -> Self {
        Self {
            id,
            name,
            path: None,
        }
    }

    /// Display name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Link state: `Online` direct, `Relay`, or `Offline` without a link.
    #[allow(
        clippy::todo,
        reason = "L1 skeleton: body lands with its contract test"
    )]
    pub fn status(&self) -> Status {
        // None → Offline; Some(Direct) → Online; Some(Relay) → Relay
        todo!()
    }
}

/// Everything views render. Updated by link events; read by the UI.
///
/// Sessions, tabs, and split trees land with the session spec work (specs/03-sessions.md)
/// and the layout engine (07-layouts.md).
#[derive(Debug, Default)]
pub struct DeckState {
    workers: Vec<WorkerEntry>,
}

impl DeckState {
    /// Empty state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Paired workers in rail order.
    pub fn workers(&self) -> &[WorkerEntry] {
        &self.workers
    }

    /// Adds or renames a worker.
    #[allow(
        clippy::todo,
        unused_variables,
        reason = "L1 skeleton: body lands with its contract test"
    )]
    pub fn upsert_worker(&mut self, worker: WorkerEntry) {
        // replace the entry with the same id, else append
        todo!()
    }

    /// Records a new path or loss (`None`) for worker `id`.
    #[allow(
        clippy::todo,
        unused_variables,
        reason = "L1 skeleton: body lands with its contract test"
    )]
    pub fn set_link(&mut self, id: NodeId, path: Option<PathInfo>) {
        // find the worker; set path
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lattice_key::{NodeKey, SecretKeyBytes};
    use lattice_transport::PathKind;
    use std::time::Duration;

    #[test]
    #[ignore = "contract: lands with PaneId"]
    fn pane_id_parses_tab_dot_pane() {
        assert_eq!("1.2".parse::<PaneId>(), PaneId::try_new(1, 2));
        assert_eq!(
            PaneId::try_new(1, 2).map(|p| p.to_string()),
            Ok("1.2".to_string())
        );
        for bad in ["", "1", "0.1", "1.0", "a.b", "1.2.3"] {
            assert_eq!(bad.parse::<PaneId>(), Err(PaneIdError), "case={bad}");
        }
    }

    #[test]
    #[ignore = "contract: lands with DeckState"]
    fn worker_status_follows_its_link() {
        let id = NodeKey::from_secret(SecretKeyBytes::new([1; 32])).node_id();
        let mut st = DeckState::new();
        st.upsert_worker(WorkerEntry::new(id, "build-01".into()));
        assert_eq!(st.workers()[0].name(), "build-01");
        assert_eq!(st.workers()[0].status(), Status::Offline);
        st.set_link(
            id,
            Some(PathInfo::new(PathKind::Relay, Duration::from_millis(64))),
        );
        assert_eq!(st.workers()[0].status(), Status::Relay);
        st.set_link(
            id,
            Some(PathInfo::new(PathKind::Direct, Duration::from_millis(18))),
        );
        assert_eq!(st.workers()[0].status(), Status::Online);
    }
}
