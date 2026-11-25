//! Mobile input: the extra-keys row and the command field.

use nsh_core::prelude::*;

use super::super::state::Phone;
use super::super::style::HAIRLINE;
use super::super::style::typed;
use super::super::terminal::prompt_input;
use super::super::tokens::{color, row, space, typo};
use super::icon_button;
use crate::model::{Density, ExtraKey};

/// Input bar height (canvas: 56).
const INPUT_BAR: f32 = row::ROW_TOUCH + space::SP_3;

/// Extra keys. `ctrl` latches; the others type a marker into the input.
pub(super) fn extra_keys(keys: Vec<ExtraKey>, phone: Phone) -> View {
    view()
        .flex_none()
        .flex_row()
        // The top border sits outside the 44pt keys.
        .h(px(row::ROW_TOUCH + HAIRLINE))
        .gap(px(HAIRLINE))
        .bg(color::LINE)
        .border_t(px(HAIRLINE))
        .border_color(color::LINE)
        .child(dynamic(move || {
            let latched = phone.ctrl.get();
            keys.iter()
                .map(|k| extra_key(k, k.modifier && latched, phone))
                .collect::<Vec<_>>()
        }))
}

fn extra_key(key: &ExtraKey, latched: bool, phone: Phone) -> View {
    let (bg, fg) = if latched {
        (color::BG_3, color::ACCENT)
    } else {
        (color::BG_1, color::FG)
    };
    let name = if latched {
        format!("{}, latched", key.name)
    } else {
        key.name.clone()
    };
    let (label, modifier) = (key.label.clone(), key.modifier);
    view()
        .flex_1()
        .items_center()
        .justify_center()
        .bg(bg)
        .role(AccessibilityRole::KeyboardKey)
        .label(name)
        .on_press(move |_: &PressEvent| phone.key(&label, modifier))
        .child(typed(text(key.label.clone()), typo::MONO_TERM).text_color(fg))
}

/// The command field and `↵`. Enter or `↵` runs the draft.
pub(super) fn input_bar(phone: Phone) -> View {
    let field = view()
        .flex_1()
        .h(px(row::ROW_KEY))
        .px(px(space::SP_3))
        .justify_center()
        .rounded(px(row::ROW_KEY / 2.0))
        .border(px(HAIRLINE))
        .border_color(color::LINE_STRONG)
        .bg(color::BG_0)
        .child(prompt_input(
            "Command",
            phone.draft,
            Density::Regular,
            move |cmd| phone.run(&cmd),
        ));
    view()
        .flex_none()
        .flex_row()
        .items_center()
        .gap(px(space::SP_2))
        .h(px(INPUT_BAR))
        .px(px(space::SP_2))
        .bg(color::BG_1)
        .child(field)
        .child(
            icon_button("↵", "Send")
                .rounded_full()
                .bg(color::BG_3)
                .on_press(move |_: &PressEvent| phone.run(&phone.draft.get_untracked())),
        )
}
