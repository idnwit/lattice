//! Approval components: the card, its answer buttons, and the note an
//! answer leaves (components.md › Approval card).

use nsh_core::prelude::*;

use super::components::{ButtonKind, button, state_glyph};
use super::style::{HAIRLINE, body, hrow, mono};
use super::tokens::{color, radius, space, tint, typo};
use crate::model::{Approval, Verdict};
use lattice_deck_core::Status;

/// Approval card: waiting bar, title, command, Allow / Always / Deny.
///
/// `answer` hears the choice. Buttons are `height` tall (`row/touch` on
/// mobile).
pub(crate) fn approval_card(
    approval: &Approval,
    height: f32,
    answer: impl Fn(Verdict) + Copy + 'static,
) -> View {
    view()
        .flex_none()
        .gap(px(space::SP_2))
        .px(px(space::SP_3))
        .py(px(space::SP_3))
        .rounded(px(radius::R_12))
        .border(px(HAIRLINE))
        .border_color(color::LINE_STRONG)
        .bg(tint::WAITING_BG)
        .role(AccessibilityRole::Alert)
        .child(
            hrow(space::SP_2)
                .child(state_glyph(Status::Waiting))
                .child(
                    body(approval.agent.clone(), color::FG)
                        .font_weight(FontWeight(typo::HEADING.weight)),
                )
                .child(body(approval.origin.clone(), color::FG_MUTED)),
        )
        .child(mono(approval.command.clone(), color::FG))
        .child(verdict_buttons(height, answer))
}

/// `Allow Y` (primary), `Always A` (secondary), `Deny N` (ghost).
pub(crate) fn verdict_buttons(height: f32, answer: impl Fn(Verdict) + Copy + 'static) -> View {
    let pick = |kind, text, chord, v: Verdict| {
        button(kind, text, Some(chord), height)
            .flex_1()
            .on_press(move |_: &PressEvent| answer(v))
    };
    hrow(space::SP_2)
        .child(pick(ButtonKind::Primary, "Allow", "Y", Verdict::Allow))
        .child(pick(ButtonKind::Secondary, "Always", "A", Verdict::Always))
        .child(
            pick(ButtonKind::Ghost, "Deny", "N", Verdict::Deny)
                .border(px(HAIRLINE))
                .border_color(color::LINE_STRONG),
        )
}

/// What an answered approval leaves: glyph + `allowed: <command>`.
pub(crate) fn verdict_note(approval: &Approval, verdict: Verdict) -> View {
    let status = if verdict == Verdict::Deny {
        Status::Error
    } else {
        Status::Online
    };
    hrow(space::SP_2)
        .flex_none()
        .px(px(space::SP_3))
        .py(px(space::SP_2))
        .rounded(px(radius::R_6))
        .border(px(HAIRLINE))
        .border_color(color::LINE_STRONG)
        .bg(color::BG_2)
        .role(AccessibilityRole::Alert)
        .child(state_glyph(status))
        .child(
            mono(
                format!("{}: {}", verdict.done(), approval.command),
                color::FG,
            )
            .flex_1()
            .truncate(),
        )
}
