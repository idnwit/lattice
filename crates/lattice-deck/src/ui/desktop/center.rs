//! Center: terminal tabs and the active tab's split tree.

use nsh_core::prelude::*;

use super::super::approval::verdict_buttons;
use super::super::components::{pane_header, settle, terminal_tab};
use super::super::style::{HAIRLINE, body, hrow, spacer};
use super::super::terminal::{prompt_input, term_view};
use super::super::tokens::{color, row, space, tint};
use super::Desk;
use crate::model::{Approval, Pane, Split, Tab, TermLine, Tone, Verdict, Workspace};
use crate::shell;

/// Tabs and splits. The active tab picks the split tree.
pub(super) fn center(ws: &Workspace, desk: Desk) -> View {
    let tabs = ws.tabs.clone();
    let approval = ws.approval.clone();
    view()
        .flex_1()
        .min_w(px(0.0))
        .child(tab_bar(ws.tabs.clone(), desk))
        .child(
            view()
                .flex_1()
                .min_h(px(0.0))
                .bg(color::LINE)
                .child(dynamic(move || {
                    let i = desk.tab.get();
                    let cx = Cx {
                        desk,
                        tab: i,
                        approval: approval.clone(),
                    };
                    tabs.get(i).map(|t| split(&t.split, &cx))
                })),
        )
}

/// What panes need besides their own data.
struct Cx {
    desk: Desk,
    tab: usize,
    approval: Approval,
}

fn tab_bar(tabs: Vec<Tab>, desk: Desk) -> View {
    view()
        .flex_none()
        .flex_row()
        .h(px(row::ROW_MD))
        .bg(color::BG_1)
        .border_b(px(HAIRLINE))
        .border_color(color::LINE)
        .role(AccessibilityRole::Tablist)
        .child(dynamic(move || {
            let active = desk.tab.get();
            tabs.iter()
                .enumerate()
                .map(|(i, t)| {
                    terminal_tab(t, i == active).on_press(move |_: &PressEvent| desk.tab.set(i))
                })
                .collect::<Vec<_>>()
        }))
        .child(
            hrow(0.0)
                .w(px(row::ROW_MD))
                .justify_center()
                .label("New tab")
                .child(body("+", color::FG_MUTED)),
        )
        .child(spacer())
        .child(
            hrow(space::SP_2)
                .pr(px(space::SP_3))
                .child(body("◫", color::FG_MUTED).label("Split right"))
                .child(body("⊟", color::FG_MUTED).label("Split down")),
        )
}

/// A split tree; gutters are 1px of `line` showing through.
fn split(node: &Split, cx: &Cx) -> View {
    match node {
        Split::Leaf(p) => pane(p, cx),
        Split::Row(children) => view()
            .flex_1()
            .flex_row()
            .gap(px(HAIRLINE))
            .children(children.iter().map(|c| weighted(c, cx))),
        Split::Column(children) => view()
            .flex_1()
            .gap(px(HAIRLINE))
            .children(children.iter().map(|c| weighted(c, cx))),
    }
}

fn weighted((weight, node): &(f32, Split), cx: &Cx) -> View {
    view()
        .flex_grow(*weight)
        .flex_shrink(1.0)
        .basis(px(0.0))
        .min_w(px(0.0))
        .min_h(px(0.0))
        .child(split(node, cx))
}

fn frame(p: &Pane) -> View {
    view()
        .flex_1()
        .min_w(px(0.0))
        .bg(color::BG_0)
        .label(format!("Pane {}", p.id))
        // Focused pane: 1px inset accent ring. Nothing else glows.
        .when(p.focused, |v| {
            v.inset_ring(px(HAIRLINE)).inset_ring_color(color::ACCENT)
        })
}

fn pane(p: &Pane, cx: &Cx) -> View {
    if p.focused {
        shell_pane(p, cx)
    } else if p.asks {
        agent_pane(p, cx)
    } else {
        frame(p)
            .child(pane_header(p, p.status, None))
            .child(term_view(&p.lines, p.density))
    }
}

/// The focused shell: its lines live in state and a prompt takes commands.
fn shell_pane(p: &Pane, cx: &Cx) -> View {
    let (desk, tab, density) = (cx.desk, cx.tab, p.density);
    let draft = signal(String::new());
    let lines = move || desk.shells.get().get(tab).cloned().unwrap_or_default();
    frame(p)
        .child(pane_header(p, p.status, None))
        .child(dynamic(move || term_view(&lines(), density)))
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

/// The agent pane: the CLI's inline approval prompt while it waits, then
/// the answer.
fn agent_pane(p: &Pane, cx: &Cx) -> View {
    let (pane, approval, desk) = (p.clone(), cx.approval.clone(), cx.desk);
    frame(p).child(dynamic(move || {
        let answered = desk.verdict.get();
        let note = answered.is_none().then_some("needs approval");
        let lines = [pane.lines.clone(), approval_lines(&approval, answered)].concat();
        view()
            .flex_1()
            .child(pane_header(&pane, settle(pane.status, answered), note))
            .child(term_view(&lines, pane.density))
            .when(answered.is_none(), |v| v.child(approval_bar(desk)))
    }))
}

fn approval_lines(approval: &Approval, answered: Option<Verdict>) -> Vec<TermLine> {
    let line = |spans: &[(&str, Tone)]| TermLine {
        spans: spans
            .iter()
            .map(|(t, tone)| crate::model::Span {
                text: (*t).to_owned(),
                tone: *tone,
            })
            .collect(),
        cursor: false,
    };
    match answered {
        None => vec![
            line(&[
                ("▍", Tone::Waiting),
                (" Allow: ", Tone::Term),
                (&approval.command, Tone::Bright),
            ]),
            line(&[
                ("▍", Tone::Waiting),
                (" [y] yes  [a] always  [n] no", Tone::Term),
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

/// Buttons under the agent's prompt (`waiting/bg` row variant).
fn approval_bar(desk: Desk) -> View {
    view()
        .flex_none()
        .px(px(space::SP_3))
        .py(px(space::SP_2))
        .bg(tint::WAITING_BG)
        .child(verdict_buttons(row::ROW_SM, move |v| desk.answer(v)))
}
