//! Right panel: tool tabs (Files · Git · Ports) and the active tool.

use nsh_core::prelude::*;

use super::super::state::Tool;
use super::super::style::{
    FOCUS_BAR, HAIRLINE, badge, body, hrow, meta, mono, selected_state, spacer,
};
use super::super::tokens::{color, row, space, tint};
use super::{Desk, git, ports};
use crate::model::{FileRow, Mark, Workspace};

/// Tools column width (canvas: 300).
const TOOLS_W: f32 = 300.0;
/// File tree indent per level (components.md: 12–14px).
const INDENT: f32 = space::SP_3;

/// Tool tabs + the active tool.
pub(super) fn tools(ws: &Workspace, desk: Desk) -> View {
    let (files, git_data) = (ws.files.clone(), ws.git.clone());
    view()
        .flex_none()
        .w(px(TOOLS_W))
        .bg(color::BG_1)
        .border_l(px(HAIRLINE))
        .border_color(color::LINE)
        .overflow_hidden()
        .child(tab_bar(desk, ws.git.changes.len()))
        .child(dynamic(move || match desk.tool.get() {
            Tool::Files => file_tree(&files),
            Tool::Git => git::git(&git_data, desk),
            Tool::Ports => ports::ports(desk),
        }))
}

fn tab_bar(desk: Desk, unstaged: usize) -> View {
    view()
        .flex_none()
        .flex_row()
        .h(px(row::ROW_MD))
        .px(px(space::SP_1))
        .border_b(px(HAIRLINE))
        .border_color(color::LINE)
        .role(AccessibilityRole::Tablist)
        .child(dynamic(move || {
            let active = desk.tool.get();
            let counts = [
                None,
                Some(desk.staged.get().len() + unstaged),
                Some(desk.live_ports()),
            ];
            Tool::ALL
                .into_iter()
                .zip(counts)
                .map(|(t, n)| {
                    tool_tab(t, t == active, n).on_press(move |_: &PressEvent| desk.tool.set(t))
                })
                .collect::<Vec<_>>()
        }))
        .child(spacer())
        .child(dynamic(move || {
            hrow(0.0)
                .pr(px(space::SP_2))
                .child(badge(desk.tool.get().chord(), color::FG_DIM))
        }))
}

/// Tool tab: text; active = `fg` + 2px accent bottom bar.
fn tool_tab(tool: Tool, active: bool, count: Option<usize>) -> View {
    let fg = if active { color::FG } else { color::FG_MUTED };
    hrow(space::SP_1)
        .h_full()
        .px(px(space::SP_3))
        .role(AccessibilityRole::Tab)
        .label(tool.label())
        .accessibility_state(selected_state(active))
        .when(active, |t| {
            t.border_b(px(FOCUS_BAR)).border_b_color(color::ACCENT)
        })
        .child(body(tool.label(), fg).font_weight(FontWeight(500)))
        .when_some(count, |t, n| t.child(badge(n.to_string(), color::FG_DIM)))
}

/// Files tool: a small tree with git marks.
fn file_tree(rows: &[FileRow]) -> View {
    view()
        .py(px(space::SP_2))
        .child(
            hrow(0.0)
                .h(px(row::ROW_SM - 2.0))
                .px(px(space::SP_3))
                .child(meta("build-01:~/src/api")),
        )
        .children(rows.iter().map(file_row))
}

fn file_row(f: &FileRow) -> View {
    let caret = match f.dir {
        Some(true) => "▾",
        Some(false) => "▸",
        None => " ",
    };
    let mark = match (f.mark, f.dir.is_some()) {
        (None, _) => None,
        (Some(_), true) => Some(("●", tint::DIFF_MODIFIED)),
        (Some(Mark::Modified), false) => Some(("M", tint::DIFF_MODIFIED)),
        (Some(Mark::Added), false) => Some(("A", tint::DIFF_ADDED)),
        (Some(Mark::Deleted), false) => Some(("D", tint::DIFF_DELETED)),
    };
    hrow(space::SP_1)
        .h(px(row::ROW_XS))
        .pl(px(space::SP_3 + INDENT * f32::from(f.depth)))
        .pr(px(space::SP_3))
        .child(meta(caret))
        .child(mono(
            f.name.clone(),
            if f.dir.is_some() {
                color::FG
            } else {
                color::FG_TERM
            },
        ))
        .child(spacer())
        .when_some(mark, |r, (m, hue)| r.child(meta(m).text_color(hue)))
}
