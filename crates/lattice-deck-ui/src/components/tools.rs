//! Tool tabs (components.md › Tool tabs): the right panel's tab bar.

use std::rc::Rc;

use nsh_core::prelude::*;

use crate::style::{FOCUS_BAR, HAIRLINE, badge, body, hrow, selected_state, spacer};
use crate::tokens::{color, row, space};

/// One tool tab.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolTab {
    /// Label (`Git`).
    pub label: String,
    /// Optional count badge.
    pub count: Option<usize>,
    /// Chord that opens it (`⌘⇧G`), shown at the bar's end while active.
    pub chord: String,
}

/// Tool tab bar. `active` is the active index; `on_select` hears a press.
pub fn tool_tabs(
    tabs: impl IntoReactive<Vec<ToolTab>>,
    active: impl IntoReactive<usize>,
    on_select: impl Fn(usize) + 'static,
) -> View {
    let (tabs, active) = (tabs.into_reactive(), active.into_reactive());
    let on_select: Rc<dyn Fn(usize)> = Rc::new(on_select);
    let (chord_tabs, chord_active) = (tabs.clone(), active.clone());
    view()
        .flex_none()
        .flex_row()
        .h(px(row::ROW_MD))
        .px(px(space::SP_1))
        .border_b(px(HAIRLINE))
        .border_color(color::LINE)
        .role(AccessibilityRole::Tablist)
        .child(dynamic(move || {
            let active = active.get();
            tabs.get()
                .iter()
                .enumerate()
                .map(|(i, t)| {
                    let on_select = on_select.clone();
                    tool_tab(t, i == active).on_press(move |_: &PressEvent| on_select(i))
                })
                .collect::<Vec<_>>()
        }))
        .child(spacer())
        .child(dynamic(move || {
            let chord = chord_tabs
                .get()
                .get(chord_active.get())
                .map(|t| t.chord.clone())
                .unwrap_or_default();
            hrow(0.0)
                .pr(px(space::SP_2))
                .child(badge(chord, color::FG_DIM))
        }))
}

/// Tool tab: text; active = `fg` + 2px accent bottom bar.
fn tool_tab(tab: &ToolTab, active: bool) -> View {
    let fg = if active { color::FG } else { color::FG_MUTED };
    hrow(space::SP_1)
        .h_full()
        .px(px(space::SP_3))
        .role(AccessibilityRole::Tab)
        .label(tab.label.clone())
        .accessibility_state(selected_state(active))
        .when(active, |t| {
            t.border_b(px(FOCUS_BAR)).border_b_color(color::ACCENT)
        })
        .child(body(tab.label.clone(), fg).font_weight(FontWeight(500)))
        .when_some(tab.count, |t, n| {
            t.child(badge(n.to_string(), color::FG_DIM))
        })
}
