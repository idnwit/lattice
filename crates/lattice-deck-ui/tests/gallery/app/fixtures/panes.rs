//! Desktop pane fixtures: one split per tab.

use super::term::{CYAN, GREEN, PURPLE, RED, YELLOW, command, line, plain, prompt};
use crate::app::model::{Density, Line, Pane, Tone};
use lattice_deck_ui::components::Status;

pub(super) fn pane(
    id: &str,
    worker: &str,
    status: Status,
    context: &str,
    lines: Vec<Line>,
) -> Pane {
    Pane {
        id: id.to_owned(),
        worker: worker.to_owned(),
        status,
        context: context.to_owned(),
        agent: None,
        asks: false,
        focused: false,
        density: Density::Compact,
        lines,
    }
}

pub(super) fn shell_pane() -> Pane {
    let user = "giang@build-01";
    let branch = "(feat/relay-fallback*)";
    let mut lines = vec![prompt(user, branch), command("cargo test peer::")];
    lines.extend(cargo_output());
    lines.extend([
        prompt(user, branch),
        command("pnpm --dir web dev"),
        line(&[
            ("  ", Tone::Text),
            ("VITE v6.1.0", CYAN),
            ("  ready in 412 ms", Tone::Text),
        ]),
        line(&[
            ("  ➜  Local:   ", Tone::Text),
            ("http://localhost:3000/", CYAN),
            ("  ⇄ forwarded", Tone::Dim),
        ]),
        plain(""),
        prompt(user, branch),
    ]);
    Pane {
        focused: true,
        density: Density::Regular,
        ..pane("1.1", "build-01", Status::Online, "~/src/api · zsh", lines)
    }
}

fn cargo_output() -> Vec<Line> {
    let ok = |name: &str| line(&[(name, Tone::Text), ("ok", GREEN)]);
    vec![
        line(&[
            ("   ", Tone::Text),
            ("Compiling", GREEN),
            (" lattice-peer v0.4.2 (/src/api/peer)", Tone::Text),
        ]),
        line(&[
            ("   ", Tone::Text),
            ("Compiling", GREEN),
            (" api v0.4.2 (/src/api)", Tone::Text),
        ]),
        line(&[
            ("    ", Tone::Text),
            ("Finished", GREEN),
            (" test [unoptimized + debuginfo] in 7.92s", Tone::Text),
        ]),
        line(&[
            ("     ", Tone::Text),
            ("Running", GREEN),
            (" unittests src/lib.rs", Tone::Text),
        ]),
        plain(""),
        plain("running 6 tests"),
        ok("test peer::handshake::direct_ok ........ "),
        ok("test peer::handshake::nat_symmetric .... "),
        ok("test peer::handshake::relay_fallback ... "),
        ok("test peer::relay::reconnect_backoff .... "),
        ok("test peer::relay::ttl_expiry ........... "),
        line(&[
            ("test peer::stun::legacy_probe .......... ", Tone::Text),
            ("ignored", Tone::Dim),
        ]),
        plain(""),
        line(&[
            ("test result: ", Tone::Text),
            ("ok", GREEN),
            (". 5 passed; 0 failed; 1 ignored", Tone::Text),
        ]),
        plain(""),
    ]
}

pub(super) fn agent_pane() -> Pane {
    let tool = |verb: &str, rest: Vec<(&str, Tone)>| {
        let mut runs = vec![("  ⎿", Tone::Dim), (verb, Tone::Text)];
        runs.extend(rest);
        line(&runs)
    };
    let lines = vec![
        line(&[
            ("◆", PURPLE),
            (
                " Task: make relay fallback survive NAT rebinding",
                Tone::Text,
            ),
        ]),
        tool(
            " Read  ",
            vec![
                ("peer/handshake.rs ", Tone::Text),
                ("(412 lines)", Tone::Dim),
            ],
        ),
        tool(
            " Edit  ",
            vec![
                ("peer/handshake.rs ", Tone::Text),
                ("+42", GREEN),
                (" ", Tone::Text),
                ("−9", RED),
            ],
        ),
        tool(
            " Write ",
            vec![("peer/relay.rs ", Tone::Text), ("+118", GREEN)],
        ),
        tool(
            " Run   ",
            vec![("cargo test peer:: ", Tone::Text), ("5 passed", GREEN)],
        ),
        plain(""),
    ];
    Pane {
        agent: Some("claude-code".to_owned()),
        asks: true,
        ..pane("1.2", "build-01", Status::Waiting, "", lines)
    }
}

pub(super) fn logs_pane() -> Pane {
    let log = |time: &str, src: &str, level: &str, tone: Tone, msg: &str| {
        line(&[
            (time, Tone::Dim),
            (src, Tone::Muted),
            (level, tone),
            (msg, Tone::Muted),
        ])
    };
    let lines = vec![
        log(
            "10:14:02",
            " lattice-agentd ",
            "INFO",
            GREEN,
            "  peer up via relay sg-1",
        ),
        log(
            "10:14:02",
            " lattice-agentd ",
            "INFO",
            GREEN,
            "  session ml-eval attached",
        ),
        log(
            "10:14:09",
            " lattice-agentd ",
            "WARN",
            YELLOW,
            "  hole-punch failed, symmetric NAT",
        ),
        log(
            "10:14:09",
            " lattice-agentd ",
            "INFO",
            GREEN,
            "  staying on relay (rtt 64ms)",
        ),
        log(
            "10:15:31",
            " eval-sweep     ",
            "INFO",
            GREEN,
            "  run 14/40 · acc 0.913",
        ),
        log(
            "10:15:58",
            " eval-sweep     ",
            "INFO",
            GREEN,
            "  run 15/40 · acc 0.917",
        ),
    ];
    pane("1.3", "gpu-02", Status::Relay, "relay · journalctl", lines)
}

/// Tab 2: the web dev server, one pane.
pub(super) fn web_pane() -> Pane {
    let user = "giang@build-01";
    let branch = "(feat/relay-fallback*)";
    let lines = vec![
        prompt(user, branch),
        command("pnpm --dir web dev"),
        line(&[
            ("  ", Tone::Text),
            ("VITE v6.1.0", CYAN),
            ("  ready in 412 ms", Tone::Text),
        ]),
        line(&[
            ("  ➜  Local:   ", Tone::Text),
            ("http://localhost:3000/", CYAN),
        ]),
        line(&[
            ("10:16:40", Tone::Dim),
            (" [vite] ", CYAN),
            ("hmr update /src/peers.tsx", Tone::Text),
        ]),
        line(&[
            ("10:16:52", Tone::Dim),
            (" [vite] ", CYAN),
            ("page reload src/main.tsx", Tone::Text),
        ]),
        plain(""),
        prompt(user, branch),
    ];
    Pane {
        focused: true,
        density: Density::Regular,
        ..pane(
            "2.1",
            "build-01",
            Status::Online,
            "~/src/api/web · node",
            lines,
        )
    }
}

/// Tab 3, pane 1: a psql session whose last query failed.
pub(super) fn psql_pane() -> Pane {
    let lines = vec![
        line(&[
            ("api_dev=# ", GREEN),
            ("select count(*) from peers;", Tone::Text),
        ]),
        plain(" count"),
        plain("-------"),
        plain("    42"),
        plain("(1 row)"),
        plain(""),
        line(&[
            ("api_dev=# ", GREEN),
            ("select * from relay_tokens;", Tone::Text),
        ]),
        line(&[
            ("ERROR:", RED),
            ("  relation \"relay_tokens\" does not exist", Tone::Text),
        ]),
        plain(""),
    ];
    Pane {
        focused: true,
        density: Density::Regular,
        ..pane("3.1", "build-01", Status::Error, "~/src/api · psql", lines)
    }
}

/// Tab 3, pane 2: the migration that should have made the table.
pub(super) fn migrate_pane() -> Pane {
    let lines = vec![
        command("sqlx migrate info"),
        line(&[("20261101_peers.sql      ", Tone::Text), ("applied", GREEN)]),
        line(&[
            ("20261118_relay_tokens.sql ", Tone::Text),
            ("pending", YELLOW),
        ]),
    ];
    pane("3.2", "build-01", Status::Online, "~/src/api · zsh", lines)
}
