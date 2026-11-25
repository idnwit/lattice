//! Git tool: commit form, staged and unstaged changes, history.

use nsh_core::prelude::*;

use super::super::components::{ButtonKind, button, change_row};
use super::super::style::{HAIRLINE, RING_STROKE, body, hrow, label, meta, mono, spacer, typed};
use super::super::tokens::{color, radius, row, space, typo};
use super::Desk;
use crate::model::{Change, Commit, Git};

/// HEAD ring size (tokens.md: 7–8px).
const RING: f32 = 7.0;

/// The Git tool. Staged files and history come from state.
pub(super) fn git(g: &Git, desk: Desk) -> View {
    let changes = g.changes.clone();
    view()
        .child(commit_form(g, desk))
        .child(dynamic(move || {
            let staged = desk.staged.get();
            view()
                .child(section("Staged", Some(staged.len())))
                .children(staged.iter().map(change_row))
        }))
        .child(section("Changes", Some(changes.len())).mt(px(space::SP_1)))
        .children(changes.iter().map(|c: &Change| change_row(c)))
        .child(section("History", None).mt(px(space::SP_2)))
        .child(dynamic(move || {
            desk.history
                .get()
                .iter()
                .enumerate()
                .map(|(i, c)| commit_row(c, i == 0))
                .collect::<Vec<_>>()
        }))
}

fn commit_form(g: &Git, desk: Desk) -> View {
    view()
        .gap(px(space::SP_3))
        .px(px(space::SP_3))
        .py(px(space::SP_3))
        .child(
            hrow(space::SP_2)
                .child(body("⎇", color::FG))
                .child(mono(g.branch.clone(), color::FG))
                .child(spacer())
                .child(
                    typed(
                        text(move || format!("↑{} ↓0", desk.ahead.get())),
                        typo::MONO_META,
                    )
                    .text_color(color::FG_DIM),
                ),
        )
        .child(
            view()
                .gap(px(space::SP_1))
                .child(
                    typed(text("Commit message"), typo::MONO_META)
                        .font_family(typo::BODY.family)
                        .text_color(color::FG_MUTED),
                )
                .child(message_input(desk)),
        )
        .child(
            hrow(space::SP_2)
                .child(dynamic(move || {
                    commit_button(desk.staged.get().len(), desk)
                }))
                .child(
                    button(ButtonKind::Secondary, "Push", None, row::ROW_SM)
                        .on_press(move |_: &PressEvent| desk.ahead.set(0)),
                ),
        )
}

/// The commit message: a real text field. Enter commits.
fn message_input(desk: Desk) -> Input {
    typed(nsh_core::element::input(), typo::BODY)
        .h(px(row::ROW_MD - 2.0))
        .px(px(space::SP_2))
        .rounded(px(radius::R_3))
        .border(px(HAIRLINE))
        .border_color(color::LINE_STRONG)
        .focus(|s| s.border_color(color::ACCENT))
        .bg(color::BG_0)
        .text_color(color::FG)
        .caret_color(color::ACCENT)
        .label("Commit message")
        .placeholder("Message")
        .value(move || desk.message.get())
        .on_change(move |t: &str| desk.message.set(t.to_owned()))
        .on_submit(move |_: &str| desk.commit())
}

/// `Commit n files ⌘↵`: the one primary button here.
fn commit_button(staged: usize, desk: Desk) -> View {
    let noun = if staged == 1 { "file" } else { "files" };
    button(
        ButtonKind::Primary,
        &format!("Commit {staged} {noun}"),
        Some("⌘↵"),
        row::ROW_SM,
    )
    .flex_1()
    .when(staged == 0, |b| b.opacity(DISABLED_OPACITY))
    .on_press(move |_: &PressEvent| desk.commit())
}

/// Opacity of a button with nothing to do.
const DISABLED_OPACITY: f32 = 0.4;

fn section(title: &str, count: Option<usize>) -> View {
    hrow(space::SP_2)
        .h(px(row::ROW_SM - 2.0))
        .px(px(space::SP_3))
        .child(label(title.to_owned()))
        .when_some(count, |s, n| s.child(meta(n.to_string())))
}

fn commit_row(c: &Commit, head: bool) -> View {
    // HEAD gets a brighter ring; accent stays reserved for focus.
    let ring = if head { color::FG } else { color::FG_DIM };
    hrow(space::SP_2)
        .h(px(row::ROW_XS + 2.0))
        .px(px(space::SP_3))
        .child(
            view()
                .size(px(RING))
                .rounded_full()
                .border(px(RING_STROKE))
                .border_color(ring),
        )
        .child(body(c.message.clone(), color::FG).flex_1().truncate())
        .child(meta(c.sha.clone()))
}
