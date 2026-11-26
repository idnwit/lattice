//! Pane frame, pane header, and the close confirmation
//! (components.md › Pane header; specs/03-sessions.md, Behavior 13).

use std::rc::Rc;

use nsh_core::prelude::*;

use crate::components::{ButtonKind, Status, button, segmented, state_glyph};
use crate::style::{HAIRLINE, body, hrow, meta, spacer};
use crate::tokens::{color, radius, row, space, state, typo};

/// A pane: `bg/0`, named `Pane <name>`. Focused: 1px inset accent ring.
/// Nothing else glows.
pub fn pane_frame(name: &str, focused: impl IntoReactive<bool>) -> View {
    let focused = focused.into_reactive();
    view()
        .flex_1()
        .min_w(px(0.0))
        .min_h(px(0.0))
        .bg(color::BG_0)
        .label(format!("Pane {name}"))
        .inset_ring(move || px(if focused.get() { HAIRLINE } else { 0.0 }))
        .inset_ring_color(color::ACCENT)
}

/// What a pane header shows.
#[derive(Debug, Clone, PartialEq)]
pub struct PaneHeader {
    /// Pane id (`1.2`), shown at the right unless a note replaces it.
    pub id: String,
    /// Pane state glyph.
    pub status: Status,
    /// Main text (the worker, `build-01`).
    pub title: String,
    /// Context after it (`~/src/api · zsh`); empty for none.
    pub context: String,
    /// Agent name, when an agent runs here (adds the TTY / Visual control).
    pub agent: Option<String>,
    /// A note in `waiting` instead of the id (`needs approval`, `closing`).
    pub note: Option<String>,
    /// The pane has focus: the title is `fg`, else `fg/muted`.
    pub focused: bool,
}

impl PaneHeader {
    /// A header with `id`, `status` and `title` only.
    pub fn new(id: impl Into<String>, status: Status, title: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            status,
            title: title.into(),
            context: String::new(),
            agent: None,
            note: None,
            focused: false,
        }
    }
}

/// Pane header: glyph, title, context or agent, note or id. No close
/// button (`⌘W`).
pub fn pane_header(h: &PaneHeader) -> View {
    let fg = if h.focused {
        color::FG
    } else {
        color::FG_MUTED
    };
    let right = match &h.note {
        Some(note) => meta(note.clone()).text_color(state::WAITING),
        None => meta(h.id.clone()),
    };
    hrow(space::SP_2)
        .flex_none()
        .h(px(row::ROW_SM))
        .px(px(space::SP_3))
        .bg(color::BG_1)
        .child(state_glyph(h.status))
        .child(meta(h.title.clone()).text_color(fg))
        .when(!h.context.is_empty(), |r| r.child(meta(h.context.clone())))
        .when_some(h.agent.as_ref(), |r, a| {
            r.child(meta(format!("agent: {a}")).text_color(state::AGENT))
        })
        .child(spacer())
        .child(right)
        .when(h.agent.is_some(), |r| {
            r.child(segmented(&["TTY", "Visual"], 0))
        })
}

/// Props of [`confirm_card`].
pub struct Confirm {
    /// Question (`Close pane 1.2?`).
    pub title: String,
    /// What follows (`The instance stops and its history is deleted.`).
    pub detail: String,
    /// The destructive verb (`Close`).
    pub action: String,
    /// Button height (`row/touch` on mobile).
    pub height: f32,
    /// Hears the action (button or Enter).
    pub on_confirm: Rc<dyn Fn()>,
    /// Hears `Cancel` (button or Escape).
    pub on_cancel: Rc<dyn Fn()>,
}

/// An inline confirmation for a destructive action: error glyph (shape +
/// hue), title, detail, a danger button and `Cancel`. While it shows,
/// Enter confirms and Escape cancels.
pub fn confirm_card(c: Confirm) -> View {
    let Confirm {
        title,
        detail,
        action,
        height,
        on_confirm,
        on_cancel,
    } = c;
    let (enter, escape) = (on_confirm.clone(), on_cancel.clone());
    on_key("enter", move |_| enter());
    on_key("escape", move |_| escape());
    view()
        .flex_none()
        .gap(px(space::SP_2))
        .m(px(space::SP_3))
        .p(px(space::SP_3))
        .rounded(px(radius::R_6))
        .border(px(HAIRLINE))
        .border_color(color::LINE_STRONG)
        .bg(color::BG_2)
        .role(AccessibilityRole::Alert)
        .child(
            hrow(space::SP_2)
                .child(state_glyph(Status::Error))
                .child(body(title, color::FG).font_weight(FontWeight(typo::HEADING.weight))),
        )
        .child(body(detail, color::FG_MUTED))
        .child(
            hrow(space::SP_2)
                .child(
                    button(ButtonKind::Danger, &action, Some("↵"), height)
                        .flex_1()
                        .on_press(move |_: &PressEvent| on_confirm()),
                )
                .child(
                    button(ButtonKind::Ghost, "Cancel", Some("esc"), height)
                        .flex_1()
                        .on_press(move |_: &PressEvent| on_cancel()),
                ),
        )
}
