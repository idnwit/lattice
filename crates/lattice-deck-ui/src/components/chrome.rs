//! Window chrome: title bar and status bar pieces (desktop).

use nsh_core::prelude::*;

use super::controls::{ButtonKind, button};
use super::glyph::{Status, state_glyph};
use crate::style::{HAIRLINE, body, hrow, meta, spacer, typed};
use crate::tokens::{color, radius, row, space, state, typo};

/// Title bar height (canvas: 36).
const TITLE_BAR: f32 = row::ROW_MD + space::SP_1;
/// Command search field width (canvas: 420).
const SEARCH_W: f32 = 420.0;
/// Brand mark: 8×15 accent block.
const BRAND_MARK: (f32, f32) = (space::SP_2, 15.0);

/// Title bar: brand, `/ account`, the `⌘K` search field, sync state, `Add worker`.
pub fn title_bar(account: &str, sync: &str) -> View {
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
        .child(meta(format!("/ {account}")))
        .child(spacer())
        .child(search_field())
        .child(spacer())
        .child(link_indicator(Status::Online, sync))
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
        .rounded(px(radius::R_3))
        .border(px(HAIRLINE))
        .border_color(color::LINE)
        .bg(color::BG_0)
        .role(AccessibilityRole::Search)
        .child(body("Run command, jump to session or worker…", color::FG_DIM).flex_1())
        .child(meta("⌘K"))
}

/// Status bar: a `row/xs` strip on `bg/1`. Add items as children; put a
/// [`spacer`] before right-aligned ones.
pub fn status_bar() -> View {
    hrow(space::SP_4)
        .flex_none()
        .h(px(row::ROW_XS))
        .px(px(space::SP_3))
        .bg(color::BG_1)
        .border_t(px(HAIRLINE))
        .border_color(color::LINE)
}

/// Sync and link indicator: state glyph + muted meta text (`build-01 · direct · 18ms`).
pub fn link_indicator(status: Status, text: &str) -> View {
    hrow(space::SP_2)
        .child(state_glyph(status))
        .child(meta(text.to_owned()).text_color(color::FG_MUTED))
}

/// `▍n agents waiting` in `waiting`.
pub fn waiting_note(count: usize) -> View {
    let noun = if count == 1 { "agent" } else { "agents" };
    hrow(space::SP_1)
        .child(state_glyph(Status::Waiting))
        .child(meta(format!("{count} {noun} waiting")).text_color(state::WAITING))
}
