//! The active worker's link state, and the worker rail built from the worker list
//! (specs/02-connectivity.md, Link states; specs/01-pairing.md, UI rule 1). Sans-IO.

use lattice_deck_core::{WorkerList, monogram};
use lattice_deck_ui::components::{RailItem, Status};
use lattice_transport::{PathInfo, PathKind};

/// What the link to the active worker is doing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LinkState {
    /// No paired worker.
    NoWorker,
    /// Dialing `worker`.
    Connecting { worker: String },
    /// Linked to `worker` over `path`.
    Up { worker: String, path: PathInfo },
    /// The link to `worker` ended or did not open; `code` says why. The deck dials again.
    Lost { worker: String, code: String },
}

impl LinkState {
    /// The state glyph: shape + hue.
    pub(crate) fn status(&self) -> Status {
        match self {
            Self::NoWorker => Status::Offline,
            Self::Connecting { .. } => Status::Idle,
            Self::Up { path, .. } => match path.kind() {
                PathKind::Direct => Status::Online,
                PathKind::Relay => Status::Relay,
            },
            Self::Lost { .. } => Status::Error,
        }
    }

    /// The status bar text (`build-01 · direct · 3ms`).
    pub(crate) fn text(&self) -> String {
        match self {
            Self::NoWorker => "no worker".to_owned(),
            Self::Connecting { worker } => format!("{worker} · connecting"),
            Self::Up { worker, path } => format!("{worker} · {path}"),
            Self::Lost { worker, code } => format!("{worker} · reconnecting · {code}"),
        }
    }
}

/// One rail entry per paired worker, in pairing order. The latest is the active one: it is
/// selected and shows the link state; the others show offline (M1 links one worker).
pub(crate) fn rail_items(workers: &WorkerList, link: &LinkState) -> Vec<RailItem> {
    let active = workers.latest().map(|w| w.id);
    workers
        .workers()
        .iter()
        .map(|w| {
            let selected = Some(w.id) == active;
            RailItem {
                mono: monogram(&w.name),
                status: Some(if selected {
                    link.status()
                } else {
                    Status::Offline
                }),
                selected,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use lattice_deck_core::PairedWorker;
    use lattice_key::{NodeKey, SecretKeyBytes};
    use std::time::Duration;

    fn list(names: &[&str]) -> WorkerList {
        let mut list = WorkerList::default();
        for (i, name) in names.iter().enumerate() {
            let seed = u8::try_from(i + 1).unwrap();
            list.add(PairedWorker {
                id: NodeKey::from_secret(SecretKeyBytes::new([seed; 32])).node_id(),
                addr: "172.16.55.6:7441".parse().unwrap(),
                name: (*name).to_owned(),
            });
        }
        list
    }

    fn up(kind: PathKind) -> LinkState {
        LinkState::Up {
            worker: "build-01".to_owned(),
            path: PathInfo::new(kind, Duration::from_millis(3)),
        }
    }

    #[test]
    fn link_states_map_to_shape_and_text() {
        let lost = LinkState::Lost {
            worker: "build-01".to_owned(),
            code: "transport.closed".to_owned(),
        };
        let connecting = LinkState::Connecting {
            worker: "build-01".to_owned(),
        };
        let cases = [
            (LinkState::NoWorker, Status::Offline, "no worker"),
            (connecting, Status::Idle, "build-01 · connecting"),
            (
                up(PathKind::Direct),
                Status::Online,
                "build-01 · direct · 3ms",
            ),
            (up(PathKind::Relay), Status::Relay, "build-01 · relay · 3ms"),
            (
                lost,
                Status::Error,
                "build-01 · reconnecting · transport.closed",
            ),
        ];
        for (state, status, text) in cases {
            assert_eq!(
                (state.status(), state.text().as_str()),
                (status, text),
                "case={state:?}"
            );
        }
    }

    #[test]
    fn the_rail_has_one_entry_per_paired_worker_and_the_latest_is_active() {
        let rail = rail_items(&list(&["build-01", "gpu-02"]), &up(PathKind::Direct));
        let got: Vec<(&str, Option<Status>, bool)> = rail
            .iter()
            .map(|r| (r.mono.as_str(), r.status, r.selected))
            .collect();
        let want = [
            ("B1", Some(Status::Offline), false),
            ("G2", Some(Status::Online), true),
        ];
        assert_eq!(got, want);
    }

    #[test]
    fn no_paired_worker_means_an_empty_rail() {
        assert!(rail_items(&WorkerList::default(), &LinkState::NoWorker).is_empty());
    }
}
