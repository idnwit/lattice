//! Controls (components.md): button, toggle, segmented control, port chip,
//! progress meter, icon button.

use nsh_core::prelude::*;

use crate::style::{DISABLED_OPACITY, HAIRLINE, badge, body, hrow};
use crate::tokens::{color, radius, row, space, state, tint, typo};

/// Meter track height (components.md: 3px).
const METER: f32 = 3.0;
/// Load at which a meter turns `waiting` (components.md: ≥ 85%).
const METER_HOT: u8 = 85;
/// Toggle track size (components.md: 30×16).
const TOGGLE: (f32, f32) = (30.0, 16.0);
/// Gap between track and knob.
const TOGGLE_PAD: f32 = 2.0;

/// Button variants (components.md › Button).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonKind {
    /// Accent fill. One per surface.
    Primary,
    /// `bg/2` fill, `line/strong` border.
    Secondary,
    /// Transparent, no border.
    Ghost,
    /// `danger/bg` fill, `danger/border` border, `error` text: destructive verbs.
    Danger,
}

/// A button with a verb label and an optional chord hint, `height` tall.
///
/// Add `.on_press(..)` for its action.
pub fn button(kind: ButtonKind, text: &str, chord: Option<&str>, height: f32) -> View {
    let (fill, fg, weight) = match kind {
        ButtonKind::Primary => (color::ACCENT, color::ACCENT_INK, 600),
        ButtonKind::Secondary => (color::BG_2, color::FG, 500),
        ButtonKind::Ghost => (transparent_black().into(), color::FG, 500),
        ButtonKind::Danger => (tint::DANGER_BG, state::ERROR, 500),
    };
    let border = match kind {
        ButtonKind::Secondary => Some(color::LINE_STRONG),
        ButtonKind::Danger => Some(tint::DANGER_BORDER),
        ButtonKind::Primary | ButtonKind::Ghost => None,
    };
    let chord_fg = if kind == ButtonKind::Primary {
        fg
    } else {
        color::FG_DIM
    };
    hrow(space::SP_2)
        .justify_center()
        .h(px(height))
        .px(px(space::SP_3))
        .rounded(px(radius::R_3))
        .bg(fill)
        .when_some(border, |b, c| b.border(px(HAIRLINE)).border_color(c))
        .role(AccessibilityRole::Button)
        .label(text.to_owned())
        .child(body(text.to_owned(), fg).font_weight(FontWeight(weight)))
        .when_some(chord, |b, c| b.child(badge(c.to_owned(), chord_fg)))
}

/// A square tap target with a glyph, `row/touch` (44) on a side.
pub(crate) fn icon_button(glyph: &str, name: &str) -> View {
    view()
        .flex_none()
        .size(px(row::ROW_TOUCH))
        .items_center()
        .justify_center()
        .rounded(px(radius::R_6))
        .role(AccessibilityRole::Button)
        .label(name.to_owned())
        .child(body(glyph.to_owned(), color::FG).text_size(px(typo::TITLE.size)))
}

/// Port chip: `⇄ n` in accent, thin accent border.
pub(crate) fn port_chip(count: u32) -> View {
    view()
        .px(px(space::SP_1))
        .rounded(px(radius::R_3))
        .border(px(HAIRLINE))
        .border_color(color::ACCENT_BORDER)
        .child(badge(format!("⇄ {count}"), color::ACCENT))
}

/// Segmented control; `selected` is the chosen index.
pub(crate) fn segmented(items: &[&str], selected: usize) -> View {
    hrow(HAIRLINE)
        .p(px(HAIRLINE))
        .border(px(HAIRLINE))
        .border_color(color::LINE)
        .rounded(px(radius::R_3))
        .children(items.iter().enumerate().map(|(i, item)| {
            let on = i == selected;
            view()
                .px(px(space::SP_1))
                .rounded(px(radius::R_3))
                .when(on, |s| s.bg(color::BG_3))
                .child(badge(
                    (*item).to_owned(),
                    if on { color::FG } else { color::FG_MUTED },
                ))
        }))
}

/// Progress meter: caption over a 3px bar. At 85% and above it turns `waiting`.
pub(crate) fn meter(caption: &str, pct: u8) -> View {
    let fill = if pct >= METER_HOT {
        state::WAITING
    } else {
        color::FG_MUTED
    };
    view()
        .flex_1()
        .gap(px(space::SP_1))
        .child(badge(caption.to_owned(), color::FG_MUTED))
        .child(
            view()
                .h(px(METER))
                .bg(color::LINE)
                .child(view().h_full().w(relative(f32::from(pct) / 100.0)).bg(fill)),
        )
}

/// Switch: 30×16 track, 12px knob. On: accent track, knob right.
///
/// `name` names the target (role `switch`). A disabled switch is dimmed.
pub(crate) fn toggle(on: bool, enabled: bool, name: String) -> View {
    let track = if on {
        color::ACCENT
    } else {
        color::LINE_STRONG
    };
    view()
        .flex_none()
        .flex_row()
        .w(px(TOGGLE.0))
        .h(px(TOGGLE.1))
        .p(px(TOGGLE_PAD))
        .rounded_full()
        .bg(track)
        .when(on, |t| t.justify_end())
        .when(!enabled, |t| t.opacity(DISABLED_OPACITY))
        .role(AccessibilityRole::Switch)
        .label(name)
        .accessibility_state(crate::style::selected_state(on))
        .child(
            view()
                .size(px(TOGGLE.1 - 2.0 * TOGGLE_PAD))
                .rounded_full()
                .bg(color::FG),
        )
}
