//! Style helpers over tokens, and layout sizes from the canvas that are
//! not tokens (yet).

use nsh_core::prelude::*;

use super::tokens::{TypeStyle, color, typo};
use crate::model::Tone;

/// Badge and count text size (components.md: "mono 10px"). Not a token.
pub(crate) const MONO_BADGE: f32 = 10.0;
/// Stroke of hollow ring glyphs (tokens.md: 1.5–2px).
pub(crate) const RING_STROKE: f32 = 1.5;
/// Hairline: dividers, borders and the focus ring.
pub(crate) const HAIRLINE: f32 = 1.0;
/// Active tab and tool-tab bar (tokens.md "Elevation and focus").
pub(crate) const FOCUS_BAR: f32 = 2.0;

/// Applies a type style to a text or view.
pub(crate) fn typed<E: Styled + Element>(el: E, t: TypeStyle) -> E {
    el.font_family(t.family)
        .text_size(px(t.size))
        .line_height(px(t.line))
        .font_weight(FontWeight(t.weight))
        .when(t.tracking_em > 0.0, |e| {
            e.letter_spacing(px(t.size * t.tracking_em))
        })
        .when(t.upper, Styled::uppercase)
}

/// `mono/ui` text in `color`.
pub(crate) fn mono(s: impl Into<SharedString>, c: Rgba) -> Text {
    typed(text(s.into()), typo::MONO_UI).text_color(c)
}

/// `mono/meta` text (`fg/dim` by default).
pub(crate) fn meta(s: impl Into<SharedString>) -> Text {
    typed(text(s.into()), typo::MONO_META).text_color(color::FG_DIM)
}

/// Mono badge text (counts, chords).
pub(crate) fn badge(s: impl Into<SharedString>, c: Rgba) -> Text {
    typed(text(s.into()), typo::MONO_META)
        .text_size(px(MONO_BADGE))
        .text_color(c)
}

/// `body` text in `color`.
pub(crate) fn body(s: impl Into<SharedString>, c: Rgba) -> Text {
    typed(text(s.into()), typo::BODY).text_color(c)
}

/// `label` text: uppercase section heads.
pub(crate) fn label(s: impl Into<SharedString>) -> Text {
    typed(text(s.into()), typo::LABEL).text_color(color::FG_MUTED)
}

/// A horizontal row with centered items and a gap.
pub(crate) fn hrow(gap: f32) -> View {
    view().flex_row().items_center().gap(px(gap))
}

/// A flexible spacer.
pub(crate) fn spacer() -> View {
    view().flex_1()
}

/// The color of a terminal tone.
pub(crate) fn tone_color(tone: Tone) -> Rgba {
    use super::tokens::{ANSI, state};
    match tone {
        Tone::Term => color::FG_TERM,
        Tone::Bright => color::FG,
        Tone::Muted => color::FG_MUTED,
        Tone::Dim => color::FG_DIM,
        Tone::Accent => color::ACCENT,
        Tone::Waiting => state::WAITING,
        Tone::Ansi(n) => ANSI.get(usize::from(n)).copied().unwrap_or(color::FG_TERM),
    }
}

/// Accessibility state of a selectable item (tab, row, switch).
pub(crate) fn selected_state(on: bool) -> AccessibilityState {
    AccessibilityState {
        selected: on,
        ..AccessibilityState::default()
    }
}
