//! Invite command and the Add worker dialog (components.md › Invite command;
//! patterns.md › Add worker; D47).

use std::rc::Rc;

use nsh_core::prelude::*;

use super::controls::{ButtonKind, button};
use super::glyph::{Status, state_glyph};
use crate::style::{HAIRLINE, body, hrow, meta, mono, spacer, typed};
use crate::tokens::{color, radius, row, space, typo};

/// Dialog width (patterns.md › Add worker: 480px).
const DIALOG_W: f32 = 480.0;
/// Stacking order of dialogs: above menus and the tab bar.
pub(crate) const DIALOG_Z: i32 = 20;
/// Keystroke that closes the dialog.
const CLOSE_KEY: &str = "escape";

/// Invite command: `❯ lattice add-deck lattice1:…` in mono with `Copy` beside it.
///
/// `command` holds a pairing secret: show it, never log it.
pub(crate) fn invite_command(command: &str, on_copy: impl Fn() + 'static) -> View {
    hrow(space::SP_3)
        .items_start()
        .p(px(space::SP_3))
        .rounded(px(radius::R_3))
        .border(px(HAIRLINE))
        .border_color(color::LINE)
        .bg(color::BG_0)
        .child(
            hrow(space::SP_2)
                .items_start()
                .flex_1()
                .min_w(px(0.0))
                .child(mono("❯", color::ACCENT))
                .child(
                    mono(command.to_owned(), color::FG)
                        .flex_1()
                        .min_w(px(0.0))
                        .selectable(true)
                        .label("Invite command"),
                ),
        )
        .child(
            button(ButtonKind::Primary, "Copy", None, row::ROW_SM)
                .flex_none()
                .on_press(move |_: &PressEvent| on_copy()),
        )
}

/// What the Add worker dialog shows.
#[derive(Clone, PartialEq, Eq)]
pub struct AddWorker {
    /// `lattice add-deck <invite>`; `None` while the invite is made, or when making it failed.
    pub command: Option<String>,
    /// Status glyph: waiting, idle (pairing), online (paired), offline (expired), error.
    pub status: Status,
    /// Status text (`waiting for worker…`, `paired · build-01`, an error code).
    pub status_text: String,
    /// Shows `New invite` (the invite expired or failed).
    pub renew: bool,
    /// Invite lifetime in whole minutes, for "Expires in N min." (the caller's
    /// `INVITE_TTL_SECS`).
    pub ttl_mins: u64,
}

impl std::fmt::Debug for AddWorker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The command holds the pairing secret (.rules/engineering.md, Security).
        f.debug_struct("AddWorker")
            .field("command", &self.command.as_ref().map(|_| "<redacted>"))
            .field("status", &self.status)
            .field("status_text", &self.status_text)
            .field("renew", &self.renew)
            .field("ttl_mins", &self.ttl_mins)
            .finish()
    }
}

/// The dialog's handlers.
#[derive(Clone)]
pub struct AddWorkerActions {
    /// `Copy` pressed.
    pub on_copy: Rc<dyn Fn()>,
    /// Close pressed or `Escape`.
    pub on_close: Rc<dyn Fn()>,
    /// `New invite` pressed.
    pub on_renew: Rc<dyn Fn()>,
}

/// Add worker dialog over the workspace: `bg/1` + `line/strong`, 480 wide, `r/6`.
///
/// Absolute and centered in its nearest `relative` parent. Binds `Escape` to close while it
/// lives.
pub fn add_worker_dialog(dialog: &AddWorker, actions: &AddWorkerActions) -> View {
    let close = actions.on_close.clone();
    on_key(CLOSE_KEY, move |_| close());
    view()
        .absolute()
        .top(px(0.0))
        .left(px(0.0))
        .right(px(0.0))
        .bottom(px(0.0))
        .items_center()
        .justify_center()
        .z_index(DIALOG_Z)
        .child(
            view()
                .w(px(DIALOG_W))
                .gap(px(space::SP_3))
                .p(px(space::SP_4))
                .rounded(px(radius::R_6))
                .border(px(HAIRLINE))
                .border_color(color::LINE_STRONG)
                .bg(color::BG_1)
                .role(AccessibilityRole::Dialog)
                .label("Add worker")
                .child(header(actions.on_close.clone()))
                .child(body("Run this on the worker:", color::FG_MUTED))
                .when_some(dialog.command.as_deref(), |v, cmd| {
                    let copy = actions.on_copy.clone();
                    v.child(invite_command(cmd, move || copy()))
                })
                .child(
                    hrow(space::SP_2)
                        .child(state_glyph(dialog.status))
                        .child(meta(dialog.status_text.clone()).text_color(color::FG_MUTED)),
                )
                .child(meta(format!(
                    "Expires in {} min. One use.",
                    dialog.ttl_mins
                )))
                .when(dialog.renew, |v| {
                    let renew = actions.on_renew.clone();
                    v.child(
                        hrow(0.0).justify_end().child(
                            button(ButtonKind::Secondary, "New invite", None, row::ROW_SM)
                                .on_press(move |_: &PressEvent| renew()),
                        ),
                    )
                }),
        )
}

fn header(on_close: Rc<dyn Fn()>) -> View {
    let title = typed(text("Add worker"), typo::HEADING).text_color(color::FG);
    hrow(space::SP_2).child(title).child(spacer()).child(
        hrow(0.0)
            .justify_center()
            .size(px(row::ROW_SM))
            .rounded(px(radius::R_3))
            .role(AccessibilityRole::Button)
            .label("Close")
            .hover(|s| s.bg(color::BG_2))
            .on_press(move |_: &PressEvent| on_close())
            .child(body("×", color::FG_MUTED)),
    )
}
