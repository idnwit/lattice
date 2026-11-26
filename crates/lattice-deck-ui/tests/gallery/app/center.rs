//! Desktop workspace of the canvas mock: tabs and the active tab's split
//! tree, from the library's tab bar, split view and pane pieces.

use lattice_deck_ui::components::{
    Approval, Line, Tone, Verdict, approval_bar, prompt_input, settle, styled_lines,
};
use lattice_deck_ui::tokens::{color, row, space};
use lattice_deck_ui::workspace::{
    AppChoice, PaneHeader, TabBar, TabItem, pane_frame, pane_header, split_view,
};
use nsh_core::prelude::*;

use super::model::{Pane, Run, Workspace};
use super::shell;
use super::state::Desk;

/// Tabs and splits. The active tab picks the split tree. `Y` / `A` / `N`
/// answer the approval from anywhere in the window; a focused text field
/// keeps the letters it types.
pub(crate) fn center(ws: &Workspace, desk: Desk) -> View {
    for (key, verdict) in Verdict::KEYS {
        on_key(key, move |_| desk.answer(verdict));
    }
    let items: Vec<TabItem> = ws
        .tabs
        .iter()
        .map(|t| TabItem {
            name: t.name.clone(),
            status: t.status,
            panes: t.split.leaves().len(),
        })
        .collect();
    let (tabs, approval) = (ws.tabs.clone(), ws.approval.clone());
    view()
        .flex_1()
        .min_w(px(0.0))
        .child(
            TabBar::new(Reactive::Static(items), desk.tab)
                .apps(vec![
                    AppChoice::new("dev.lattice.terminal", "Terminal"),
                    AppChoice::new("dev.lattice.echo", "Echo"),
                ])
                .on_select(move |i| desk.tab.set(i))
                // The mock has no instances: a pick or a split changes nothing.
                .on_split(|_| {})
                .render(),
        )
        .child(
            view()
                .flex_1()
                .min_h(px(0.0))
                .bg(color::LINE)
                .child(dynamic(move || {
                    let tab = desk.tab.get();
                    let approval = approval.clone();
                    tabs.get(tab)
                        .map(|t| split_view(&t.split, &|p: &Pane| pane(p, desk, tab, &approval)))
                })),
        )
}

fn header(p: &Pane, status: lattice_deck_ui::components::Status, note: Option<&str>) -> View {
    pane_header(&PaneHeader {
        id: p.id.clone(),
        status,
        title: p.worker.clone(),
        context: p.context.clone(),
        agent: p.agent.clone(),
        note: note.map(str::to_owned),
        focused: p.focused,
    })
}

fn pane(p: &Pane, desk: Desk, tab: usize, approval: &Approval) -> View {
    if p.focused {
        shell_pane(p, desk, tab)
    } else if p.asks {
        agent_pane(p, desk, approval)
    } else {
        pane_frame(&p.id, false)
            .child(header(p, p.status, None))
            .child(styled_lines(&p.lines, p.density))
    }
}

/// The focused shell: its lines live in state and a prompt takes commands.
fn shell_pane(p: &Pane, desk: Desk, tab: usize) -> View {
    let density = p.density;
    let draft = signal(String::new());
    let lines = move || desk.shells.get().get(tab).cloned().unwrap_or_default();
    pane_frame(&p.id, true)
        .child(header(p, p.status, None))
        .child(dynamic(move || styled_lines(&lines(), density)))
        .child(
            view()
                .flex_none()
                .px(px(space::SP_3))
                .pb(px(space::SP_2))
                .child(prompt_input("Terminal input", draft, density, move |cmd| {
                    desk.shells
                        .update(|s| s.get_mut(tab).into_iter().for_each(|l| shell::run(l, &cmd)));
                    draft.set(String::new());
                })),
        )
}

/// The agent pane: the CLI's inline approval prompt while it waits, then the answer.
fn agent_pane(p: &Pane, desk: Desk, approval: &Approval) -> View {
    let (pane, approval) = (p.clone(), approval.clone());
    pane_frame(&p.id, false).child(dynamic(move || {
        let answered = desk.verdict.get();
        let note = answered.is_none().then_some("needs approval");
        let lines = [pane.lines.clone(), approval_lines(&approval, answered)].concat();
        view()
            .flex_1()
            .child(header(&pane, settle(pane.status, answered), note))
            .child(styled_lines(&lines, pane.density))
            .when(answered.is_none(), |v| {
                v.child(approval_bar(row::ROW_SM, move |v| desk.answer(v)))
            })
    }))
}

fn approval_lines(approval: &Approval, answered: Option<Verdict>) -> Vec<Line> {
    let line = |runs: &[(&str, Tone)]| {
        Line::new(runs.iter().map(|(t, tone)| Run::new(*t, *tone)).collect())
    };
    match answered {
        None => vec![
            line(&[
                ("▍", Tone::Waiting),
                (" Allow: ", Tone::Text),
                (&approval.command, Tone::Bright),
            ]),
            line(&[
                ("▍", Tone::Waiting),
                (" [y] yes  [a] always  [n] no", Tone::Text),
            ]),
        ],
        Some(Verdict::Deny) => vec![shell::note(
            "✗",
            &format!("denied: {}", approval.command),
            Tone::Ansi(1),
        )],
        Some(v) => vec![shell::note(
            "✓",
            &format!("{}: {}", v.done(), approval.command),
            Tone::Ansi(2),
        )],
    }
}
