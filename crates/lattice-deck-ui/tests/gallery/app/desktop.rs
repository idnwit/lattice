//! Desktop parts of the canvas mock (artboard "Desktop / Web — Advanced
//! workspace"): title bar, sessions panel, tools panel, status bar. Each
//! is library components wired to the gallery's local state.

use std::rc::Rc;

use lattice_deck_ui::components::{
    GitTool, PortsTool, ToolTab, file_tree, git_tool, link_indicator, ports_tool, status_bar,
    tool_tabs, waiting_note, worker_panel, worker_rail,
};
use lattice_deck_ui::style::{meta, spacer, typed};
use lattice_deck_ui::tokens::{color, typo};
use nsh_core::prelude::*;

use super::model::Workspace;
use super::state::{Desk, Tool};

/// Sessions panel: worker rail + worker panel.
pub(crate) fn sessions(ws: &Workspace, desk: Desk) -> View {
    view()
        .flex_1()
        .flex_row()
        .child(worker_rail(
            &ws.rail,
            move || desk.verdict.get().is_none(),
            || {},
        ))
        .child(worker_panel(
            &ws.worker,
            desk.session,
            desk.verdict,
            move |i| desk.session.set(i),
        ))
}

/// Tools panel: tool tabs + the active tool.
pub(crate) fn tools(ws: &Workspace, desk: Desk) -> View {
    let (files, git) = (ws.files.clone(), ws.git.clone());
    let unstaged = ws.git.changes.len();
    let tabs = move || {
        let counts = [
            None,
            Some(desk.staged.get().len() + unstaged),
            Some(desk.live_ports()),
        ];
        Tool::ALL
            .into_iter()
            .zip(counts)
            .map(|(t, count)| ToolTab {
                label: t.label().to_owned(),
                count,
                chord: t.chord().to_owned(),
            })
            .collect::<Vec<_>>()
    };
    let active = move || {
        Tool::ALL
            .iter()
            .position(|t| *t == desk.tool.get())
            .unwrap_or(0)
    };
    view()
        .flex_1()
        .child(tool_tabs(tabs, active, move |i| {
            if let Some(t) = Tool::ALL.get(i) {
                desk.tool.set(*t);
            }
        }))
        .child(dynamic(move || match desk.tool.get() {
            Tool::Files => file_tree("build-01:~/src/api", &files),
            Tool::Git => git_tool(git_props(&git.branch, &git.changes, desk)),
            Tool::Ports => ports_tool(PortsTool {
                session: "api-server".to_owned(),
                ports: desk.ports.into_reactive(),
                on_toggle: Rc::new(move |i| desk.toggle_port(i)),
                on_set_all: Rc::new(move |on| desk.set_all_ports(on)),
            }),
        }))
}

fn git_props(branch: &str, changes: &[lattice_deck_ui::components::Change], desk: Desk) -> GitTool {
    GitTool {
        branch: branch.to_owned(),
        ahead: desk.ahead.into_reactive(),
        message: desk.message.into_reactive(),
        staged: desk.staged.into_reactive(),
        changes: changes.to_vec(),
        history: desk.history.into_reactive(),
        on_message: Rc::new(move |m| desk.message.set(m)),
        on_commit: Rc::new(move || desk.commit()),
        on_push: Rc::new(move || desk.ahead.set(0)),
    }
}

/// Status bar: link, tab crumb, live ports, waiting agents, terminal info.
pub(crate) fn status(ws: &Workspace, desk: Desk) -> View {
    let bar = ws.status.clone();
    let tabs: Vec<String> = ws.tabs.iter().map(|t| t.name.clone()).collect();
    let crumb = move || {
        let i = desk.tab.get();
        let name = tabs.get(i).cloned().unwrap_or_default();
        format!("{name} › tab {} › pane {}.1", i + 1, i + 1)
    };
    status_bar()
        .child(link_indicator(
            bar.status,
            &format!("{} · {}", bar.worker, bar.link),
        ))
        .child(typed(text(crumb), typo::MONO_META).text_color(color::FG_MUTED))
        .child(
            typed(
                text(move || format!("⇄ {} ports", desk.live_ports())),
                typo::MONO_META,
            )
            .text_color(color::ACCENT),
        )
        .child(dynamic(move || {
            desk.verdict.get().is_none().then(|| waiting_note(1))
        }))
        .child(spacer())
        .child(meta(bar.info.clone()))
}
