//! Mobile terminal fixtures (artboard "Mobile — Terminal").

use super::approval;
use super::panes::{agent_pane, logs_pane, migrate_pane, psql_pane, web_pane};
use super::term::{CYAN, GREEN, command, line, plain, prompt};
use crate::app::model::{ExtraKey, Line, MobileTab, MobileTerminal, Page, Pane, Tone};
use lattice_deck_ui::components::Status;

/// The mobile terminal mock (390×844).
pub(crate) fn mobile_terminal() -> MobileTerminal {
    MobileTerminal {
        worker: "build-01".to_owned(),
        status: Status::Online,
        link: "direct 18ms".to_owned(),
        ports: 2,
        tabs: vec![
            tab(
                "api-server",
                Status::Online,
                vec![
                    shell_page(),
                    page(&agent_pane(), "claude-code"),
                    page(&logs_pane(), "journalctl"),
                ],
            ),
            tab("web", Status::Online, vec![page(&web_pane(), "node")]),
            tab(
                "db-shell",
                Status::Error,
                vec![page(&psql_pane(), "psql"), page(&migrate_pane(), "zsh")],
            ),
        ],
        approval: approval(),
        keys: extra_keys(),
        input: "git st".to_owned(),
    }
}

fn tab(name: &str, status: Status, pages: Vec<Page>) -> MobileTab {
    MobileTab {
        name: name.to_owned(),
        status,
        pages,
    }
}

/// A desktop pane as a mobile page.
fn page(pane: &Pane, process: &str) -> Page {
    Page {
        process: process.to_owned(),
        status: pane.status,
        lines: pane.lines.clone(),
    }
}

fn shell_page() -> Page {
    Page {
        process: "zsh".to_owned(),
        status: Status::Online,
        lines: mobile_lines(),
    }
}

fn mobile_lines() -> Vec<Line> {
    let user = "build-01";
    let branch = "(feat/relay*)";
    let ok = |name: &str| line(&[(name, Tone::Text), ("ok", GREEN)]);
    vec![
        prompt(user, branch),
        command("cargo test peer::"),
        line(&[
            ("    ", Tone::Text),
            ("Finished", GREEN),
            (" test in 7.92s", Tone::Text),
        ]),
        ok("test handshake::direct_ok ... "),
        ok("test handshake::nat_sym ..... "),
        ok("test handshake::relay_fb .... "),
        ok("test relay::reconnect ....... "),
        ok("test relay::ttl_expiry ...... "),
        plain(""),
        line(&[
            ("test result: ", Tone::Text),
            ("ok", GREEN),
            (". 5 passed; 0 failed", Tone::Text),
        ]),
        plain(""),
        prompt(user, branch),
        command("pnpm --dir web dev"),
        line(&[
            ("  ", Tone::Text),
            ("VITE v6.1.0", CYAN),
            (" ready in 412 ms", Tone::Text),
        ]),
        line(&[
            ("  ➜ Local: ", Tone::Text),
            ("localhost:3000", CYAN),
            (" ⇄", Tone::Dim),
        ]),
        plain(""),
        prompt(user, branch),
    ]
}

fn extra_keys() -> Vec<ExtraKey> {
    [
        ("esc", "Escape", false),
        ("tab", "Tab", false),
        ("ctrl", "Control", true),
        ("alt", "Alt", false),
        ("←", "Left", false),
        ("↑", "Up", false),
        ("↓", "Down", false),
        ("→", "Right", false),
    ]
    .into_iter()
    .map(|(label, name, modifier)| ExtraKey {
        label: label.to_owned(),
        name: name.to_owned(),
        modifier,
    })
    .collect()
}
