//! Approval components (components.md › Approval card): the card, its
//! answer buttons, and the note an answer leaves.

use nsh_core::prelude::*;

use super::controls::{ButtonKind, button};
use super::glyph::{Status, state_glyph};
use crate::style::{HAIRLINE, body, hrow, mono};
use crate::tokens::{color, radius, space, tint, typo};

/// An agent approval request.
#[derive(Debug, Clone, PartialEq)]
pub struct Approval {
    /// Agent name.
    pub agent: String,
    /// Where it comes from (`pane 2 · wants to run`).
    pub origin: String,
    /// The command.
    pub command: String,
}

/// How the user answered an approval.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// Allow once (`Y`).
    Allow,
    /// Always allow this command prefix (`A`).
    Always,
    /// Deny (`N`).
    Deny,
}

impl Verdict {
    /// Every verdict with its key chord, in button order.
    pub const KEYS: [(&'static str, Verdict); 3] = [
        ("y", Verdict::Allow),
        ("a", Verdict::Always),
        ("n", Verdict::Deny),
    ];

    /// Past-tense word shown after the answer.
    pub fn done(self) -> &'static str {
        match self {
            Self::Allow => "allowed",
            Self::Always => "always allowed",
            Self::Deny => "denied",
        }
    }
}

/// A waiting state once the approval is answered: running again, or idle after a deny.
pub fn settle(status: Status, answered: Option<Verdict>) -> Status {
    match (status, answered) {
        (Status::Waiting, Some(Verdict::Deny)) => Status::Idle,
        (Status::Waiting, Some(_)) => Status::Agent,
        (s, _) => s,
    }
}

/// Approval card: waiting bar, title, command, Allow / Always / Deny.
///
/// `answer` hears the choice. Buttons are `height` tall (`row/touch` on mobile).
pub fn approval_card(
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

/// `Allow Y` (primary), `Always A` (secondary), `Deny N` (ghost, bordered).
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
pub fn verdict_note(approval: &Approval, verdict: Verdict) -> View {
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

/// A bar of verdict buttons on `waiting/bg`, under an agent's inline prompt.
pub fn approval_bar(height: f32, answer: impl Fn(Verdict) + Copy + 'static) -> View {
    view()
        .flex_none()
        .px(px(space::SP_3))
        .py(px(space::SP_2))
        .bg(tint::WAITING_BG)
        .child(verdict_buttons(height, answer))
}
