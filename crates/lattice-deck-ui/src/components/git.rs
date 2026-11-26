//! Git tool (right panel): commit form, staged and unstaged changes, history.

use std::rc::Rc;

use nsh_core::prelude::*;

use super::controls::{ButtonKind, button};
use crate::style::{
    DISABLED_OPACITY, HAIRLINE, RING_STROKE, body, hrow, label, meta, mono, spacer, typed,
};
use crate::tokens::{color, radius, row, space, tint, typo};

/// HEAD ring size (tokens.md: 7–8px).
const RING: f32 = 7.0;

/// Git mark of a changed path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    /// `M`.
    Modified,
    /// `A`.
    Added,
    /// `D` (path struck through).
    Deleted,
}

impl Mark {
    /// Letter and hue (`diff/*` tint).
    pub(crate) fn glyph(self) -> (&'static str, Rgba) {
        match self {
            Self::Modified => ("M", tint::DIFF_MODIFIED),
            Self::Added => ("A", tint::DIFF_ADDED),
            Self::Deleted => ("D", tint::DIFF_DELETED),
        }
    }
}

/// A changed file.
#[derive(Debug, Clone, PartialEq)]
pub struct Change {
    /// Path.
    pub path: String,
    /// Line delta (`+42 −9`).
    pub delta: String,
    /// Git mark.
    pub mark: Mark,
}

/// A commit; the first one of a history is HEAD.
#[derive(Debug, Clone, PartialEq)]
pub struct Commit {
    /// Subject line.
    pub message: String,
    /// Short hash.
    pub sha: String,
}

/// Props of [`git_tool`]. Reactive fields take values, signals, or closures.
pub struct GitTool {
    /// Branch name.
    pub branch: String,
    /// Commits ahead of the remote.
    pub ahead: Reactive<u32>,
    /// Draft commit message.
    pub message: Reactive<String>,
    /// Staged changes.
    pub staged: Reactive<Vec<Change>>,
    /// Unstaged changes.
    pub changes: Vec<Change>,
    /// Recent commits, newest first.
    pub history: Reactive<Vec<Commit>>,
    /// Hears each edit of the message.
    pub on_message: Rc<dyn Fn(String)>,
    /// Hears `Commit` (button or Enter in the message).
    pub on_commit: Rc<dyn Fn()>,
    /// Hears `Push`.
    pub on_push: Rc<dyn Fn()>,
}

/// The Git tool.
pub fn git_tool(props: GitTool) -> View {
    let (staged, history) = (props.staged.clone(), props.history.clone());
    let changes = props.changes.clone();
    view()
        .child(commit_form(&props))
        .child(dynamic(move || {
            let staged = staged.get();
            view()
                .child(section("Staged", Some(staged.len())))
                .children(staged.iter().map(change_row))
        }))
        .child(section("Changes", Some(changes.len())).mt(px(space::SP_1)))
        .children(changes.iter().map(change_row))
        .child(section("History", None).mt(px(space::SP_2)))
        .child(dynamic(move || {
            history
                .get()
                .iter()
                .enumerate()
                .map(|(i, c)| commit_row(c, i == 0))
                .collect::<Vec<_>>()
        }))
}

fn commit_form(props: &GitTool) -> View {
    let ahead = props.ahead.clone();
    let staged = props.staged.clone();
    let (on_commit, on_push) = (props.on_commit.clone(), props.on_push.clone());
    view()
        .gap(px(space::SP_3))
        .px(px(space::SP_3))
        .py(px(space::SP_3))
        .child(
            hrow(space::SP_2)
                .child(body("⎇", color::FG))
                .child(mono(props.branch.clone(), color::FG))
                .child(spacer())
                .child(
                    typed(
                        text(move || format!("↑{} ↓0", ahead.get())),
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
                .child(message_input(props)),
        )
        .child(
            hrow(space::SP_2)
                .child(dynamic(move || {
                    commit_button(staged.get().len(), on_commit.clone())
                }))
                .child(
                    button(ButtonKind::Secondary, "Push", None, row::ROW_SM)
                        .on_press(move |_: &PressEvent| on_push()),
                ),
        )
}

/// The commit message: a real text field. Enter commits.
fn message_input(props: &GitTool) -> Input {
    let (message, on_message, on_commit) = (
        props.message.clone(),
        props.on_message.clone(),
        props.on_commit.clone(),
    );
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
        .value(move || message.get())
        .on_change(move |t: &str| on_message(t.to_owned()))
        .on_submit(move |_: &str| on_commit())
}

/// `Commit n files ⌘↵`: the one primary button here.
fn commit_button(staged: usize, on_commit: Rc<dyn Fn()>) -> View {
    let noun = if staged == 1 { "file" } else { "files" };
    button(
        ButtonKind::Primary,
        &format!("Commit {staged} {noun}"),
        Some("⌘↵"),
        row::ROW_SM,
    )
    .flex_1()
    .when(staged == 0, |b| b.opacity(DISABLED_OPACITY))
    .on_press(move |_: &PressEvent| on_commit())
}

/// A section head: uppercase title and an optional count.
pub(crate) fn section(title: &str, count: Option<usize>) -> View {
    hrow(space::SP_2)
        .h(px(row::ROW_SM - 2.0))
        .px(px(space::SP_3))
        .child(label(title.to_owned()))
        .when_some(count, |s, n| s.child(meta(n.to_string())))
}

/// Git change row: path · delta · mark.
pub(crate) fn change_row(change: &Change) -> View {
    let (mark, hue) = change.mark.glyph();
    let deleted = change.mark == Mark::Deleted;
    let path = mono(
        change.path.clone(),
        if deleted { color::FG_MUTED } else { color::FG },
    );
    hrow(space::SP_2)
        .h(px(row::ROW_XS + space::SP_1 / 2.0))
        .px(px(space::SP_3))
        .child(path.when(deleted, Styled::line_through).flex_1())
        .child(meta(change.delta.clone()))
        .child(mono(mark, hue).w(px(space::SP_4)).text_center())
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
