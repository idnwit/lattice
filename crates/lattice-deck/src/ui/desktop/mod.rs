//! Desktop workspace (artboard "Desktop / Web — Advanced workspace").
//!
//! Title bar · worker rail + worker panel · tabs and splits · tools · status bar.

mod center;
mod git;
mod ports;
mod sidebar;
mod tools;

use nsh_core::prelude::*;

use super::components::{ButtonKind, button, state_glyph};
use super::state::Desk;
use super::style::{HAIRLINE, body, hrow, meta, spacer, typed};
use super::tokens::{color, row, space, state, typo};
use crate::model::{Verdict, Workspace};

/// Title bar height (canvas: 36).
const TITLE_BAR: f32 = row::ROW_MD + space::SP_1;
/// Command search field width (canvas: 420).
const SEARCH_W: f32 = 420.0;
/// Brand mark: 8×15 accent block.
const BRAND_MARK: (f32, f32) = (space::SP_2, 15.0);

/// The whole desktop workspace, with its local state.
pub(crate) fn workspace(ws: &Workspace) -> View {
    let desk = Desk::new(ws);
    bind_approval_keys(desk);
    view()
        .flex_1()
        .bg(color::BG_0)
        .child(title_bar(ws))
        .child(
            view()
                .flex_1()
                .flex_row()
                .min_h(px(0.0))
                .child(sidebar::sidebar(ws, desk))
                .child(center::center(ws, desk))
                .child(tools::tools(ws, desk)),
        )
        .child(status_bar(ws, desk))
}

/// `Y` / `A` / `N` answer the approval from anywhere in the window.
/// A focused text field keeps the letters it types.
fn bind_approval_keys(desk: Desk) {
    for (key, verdict) in [
        ("y", Verdict::Allow),
        ("a", Verdict::Always),
        ("n", Verdict::Deny),
    ] {
        on_key(key, move |_| desk.answer(verdict));
    }
}

fn title_bar(ws: &Workspace) -> View {
    let brand = typed(text("lattice"), typo::MONO_UI)
        .text_size(px(typo::BODY.size))
        .font_weight(FontWeight(typo::HEADING.weight));
    hrow(space::SP_3)
        .flex_none()
        .h(px(TITLE_BAR))
        .px(px(space::SP_3))
        .bg(color::BG_1)
        .border_b(px(HAIRLINE))
        .border_color(color::LINE)
        .child(
            hrow(space::SP_2)
                .child(
                    view()
                        .w(px(BRAND_MARK.0))
                        .h(px(BRAND_MARK.1))
                        .bg(color::ACCENT),
                )
                .child(brand.text_color(color::FG)),
        )
        .child(meta(format!("/ {}", ws.account)))
        .child(spacer())
        .child(search_field())
        .child(spacer())
        .child(
            hrow(space::SP_2)
                .child(state_glyph(lattice_deck_core::Status::Online))
                .child(meta(ws.sync.clone()).text_color(color::FG_MUTED)),
        )
        .child(button(
            ButtonKind::Secondary,
            "Add worker",
            None,
            row::ROW_XS + 2.0,
        ))
}

fn search_field() -> View {
    hrow(space::SP_2)
        .w(px(SEARCH_W))
        .h(px(row::ROW_XS + 2.0))
        .px(px(space::SP_3))
        .rounded(px(super::tokens::radius::R_3))
        .border(px(HAIRLINE))
        .border_color(color::LINE)
        .bg(color::BG_0)
        .role(AccessibilityRole::Search)
        .child(body("Run command, jump to session or worker…", color::FG_DIM).flex_1())
        .child(meta("⌘K"))
}

fn status_bar(ws: &Workspace, desk: Desk) -> View {
    let bar = ws.status.clone();
    let tabs: Vec<String> = ws.tabs.iter().map(|t| t.name.clone()).collect();
    let muted = |s: String| meta(s).text_color(color::FG_MUTED);
    hrow(space::SP_4)
        .flex_none()
        .h(px(row::ROW_XS))
        .px(px(space::SP_3))
        .bg(color::BG_1)
        .border_t(px(HAIRLINE))
        .border_color(color::LINE)
        .child(
            hrow(space::SP_2)
                .child(state_glyph(bar.status))
                .child(muted(format!("{} · {}", bar.worker, bar.link))),
        )
        .child(
            text(move || {
                let i = desk.tab.get();
                let name = tabs.get(i).cloned().unwrap_or_default();
                format!("{name} › tab {} › pane {}.1", i + 1, i + 1)
            })
            .map(|t| typed(t, typo::MONO_META).text_color(color::FG_MUTED)),
        )
        .child(
            typed(
                text(move || format!("⇄ {} ports", desk.live_ports())),
                typo::MONO_META,
            )
            .text_color(color::ACCENT),
        )
        .child(dynamic(move || waiting_note(desk.verdict.get().is_none())))
        .child(spacer())
        .child(meta(bar.info.clone()))
}

/// `▍1 agent waiting` while the approval waits; nothing after.
fn waiting_note(waiting: bool) -> Option<View> {
    waiting.then(|| {
        hrow(space::SP_1)
            .child(state_glyph(lattice_deck_core::Status::Waiting))
            .child(meta("1 agent waiting").text_color(state::WAITING))
    })
}
