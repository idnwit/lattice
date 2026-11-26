//! Desktop workspace fixtures (artboard "Desktop / Web — Advanced workspace").

use super::panes::{agent_pane, logs_pane, migrate_pane, psql_pane, shell_pane, web_pane};
use super::{approval, column, leaf, row};
use crate::app::model::{
    AgentRow, Change, Commit, FileRow, Git, Mark, PortRow, RailItem, Session, StatusBar, Tab,
    Worker, Workspace,
};
use lattice_deck_ui::components::Status;

fn agent(name: &str, meta: &str, status: Status) -> AgentRow {
    AgentRow {
        name: name.to_owned(),
        meta: meta.to_owned(),
        status,
    }
}

fn session(
    name: &str,
    ports: Option<u32>,
    tabs: u32,
    status: Status,
    agents: Vec<AgentRow>,
) -> Session {
    Session {
        name: name.to_owned(),
        ports,
        tabs,
        status,
        agents,
    }
}

fn rail(mono: &str, status: Option<Status>, selected: bool) -> RailItem {
    RailItem {
        mono: mono.to_owned(),
        status,
        selected,
    }
}

fn change(path: &str, delta: &str, mark: Mark) -> Change {
    Change {
        path: path.to_owned(),
        delta: delta.to_owned(),
        mark,
    }
}

fn commit(message: &str, sha: &str) -> Commit {
    Commit {
        message: message.to_owned(),
        sha: sha.to_owned(),
    }
}

/// The desktop workspace mock (1440×900).
pub(crate) fn workspace() -> Workspace {
    Workspace {
        account: "personal".to_owned(),
        sync: "synced".to_owned(),
        rail: vec![
            rail("∗", None, false),
            rail("B1", Some(Status::Online), true),
            rail("G2", Some(Status::Relay), false),
            rail("MM", Some(Status::Offline), false),
        ],
        worker: build_01(),
        tabs: tabs(),
        files: files(),
        git: git(),
        ports: ports(),
        approval: approval(),
        status: StatusBar {
            worker: "build-01".to_owned(),
            status: Status::Online,
            link: "direct · 18ms".to_owned(),
            info: "utf-8 · 132×41 · zsh".to_owned(),
        },
    }
}

fn tabs() -> Vec<Tab> {
    let api = row(vec![
        (1.35, leaf(shell_pane())),
        (
            1.0,
            column(vec![(1.0, leaf(agent_pane())), (1.0, leaf(logs_pane()))]),
        ),
    ]);
    let db = column(vec![(2.0, leaf(psql_pane())), (1.0, leaf(migrate_pane()))]);
    vec![
        Tab {
            name: "api-server".to_owned(),
            status: Status::Online,
            split: api,
        },
        Tab {
            name: "web".to_owned(),
            status: Status::Online,
            split: leaf(web_pane()),
        },
        Tab {
            name: "db-shell".to_owned(),
            status: Status::Error,
            split: db,
        },
    ]
}

fn build_01() -> Worker {
    Worker {
        name: "build-01".to_owned(),
        os: "linux · x86_64 · 16 cores".to_owned(),
        link: "direct · 18ms".to_owned(),
        status: Status::Online,
        cpu_pct: 42,
        mem: "11/32G".to_owned(),
        mem_pct: 35,
        sessions: vec![
            session("api-server", Some(2), 3, Status::Online, vec![]),
            session(
                "refactor-auth",
                None,
                1,
                Status::Waiting,
                vec![agent("claude-code", "approve", Status::Waiting)],
            ),
            session(
                "web-e2e",
                Some(1),
                2,
                Status::Online,
                vec![agent("codex", "done 2m", Status::Online)],
            ),
        ],
    }
}

fn git() -> Git {
    Git {
        branch: "feat/relay-fallback".to_owned(),
        ahead: 2,
        message: "peer: survive NAT rebinding on relay".to_owned(),
        staged: vec![
            change("peer/handshake.rs", "+42 −9", Mark::Modified),
            change("peer/relay.rs", "+118", Mark::Added),
        ],
        changes: vec![
            change("web/src/peers.tsx", "+6 −2", Mark::Modified),
            change("Cargo.lock", "+14 −3", Mark::Modified),
            change("legacy/stun.rs", "−203", Mark::Deleted),
        ],
        history: vec![
            commit("peer: add relay reconnect backoff", "a3f9c21"),
            commit("session: persist layout to cloud", "91be0d4"),
            commit("agentd: stream tool calls over p2p", "4c02e7a"),
            commit("pair: qr + short code flow", "e17d5b0"),
        ],
    }
}

fn port(remote: u16, local: u16, label: &str, meta: &str, on: bool) -> PortRow {
    PortRow {
        remote,
        local,
        label: label.to_owned(),
        meta: meta.to_owned(),
        on,
        conflict: false,
    }
}

fn ports() -> Vec<PortRow> {
    vec![
        port(3000, 3000, "vite dev", "auto-detected", true),
        port(5432, 15432, "postgres", "pinned", true),
        port(9229, 9229, "node inspector", "auto-detected", false),
        port(6006, 6006, "tensorboard", "gpu-02 · relay", false),
        PortRow {
            conflict: true,
            ..port(8080, 8080, "api", "local :8080 in use", false)
        },
    ]
}

fn file(name: &str, depth: u8, dir: Option<bool>, mark: Option<Mark>) -> FileRow {
    FileRow {
        name: name.to_owned(),
        depth,
        dir,
        mark,
    }
}

fn files() -> Vec<FileRow> {
    vec![
        file("api", 0, Some(true), None),
        file("peer", 1, Some(true), Some(Mark::Modified)),
        file("handshake.rs", 2, None, Some(Mark::Modified)),
        file("relay.rs", 2, None, Some(Mark::Added)),
        file("mod.rs", 2, None, None),
        file("session", 1, Some(false), None),
        file("web", 1, Some(false), Some(Mark::Modified)),
        file("Cargo.toml", 1, None, None),
        file("Cargo.lock", 1, None, Some(Mark::Modified)),
        file("README.md", 1, None, None),
    ]
}
