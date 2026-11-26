//! Mobile components (patterns.md › Mobile): app bar, tab pills, pane
//! switcher, extra keys, input bar. Every tap target is at least
//! `row/touch` (44).

use std::rc::Rc;

use nsh_core::prelude::*;

use super::controls::icon_button;
use super::glyph::{Status, state_glyph};
use super::lines::{Density, prompt_input};
use crate::style::{HAIRLINE, badge, hrow, meta, mono, selected_state, spacer, typed};
use crate::tokens::{color, row, space, state, typo};

/// App bar height (canvas: 52).
const APP_BAR: f32 = row::ROW_TOUCH + space::SP_2;
/// Page dot: active 18×4, others 6×4.
const PAGE_DOT: (f32, f32, f32) = (18.0, 6.0, 4.0);
/// Input bar height (canvas: 56).
const INPUT_BAR: f32 = row::ROW_TOUCH + space::SP_3;

/// A key in the extra-keys row.
#[derive(Debug, Clone, PartialEq)]
pub struct ExtraKey {
    /// Key label (`esc`).
    pub label: String,
    /// Accessible name (`Escape`).
    pub name: String,
    /// A modifier that latches (`ctrl`).
    pub modifier: bool,
}

/// A tab pill: name, state, page count.
#[derive(Debug, Clone, PartialEq)]
pub struct TabPill {
    /// Tab name.
    pub name: String,
    /// Busiest pane state.
    pub status: Status,
    /// Panes in it (`▦n`, shown while active).
    pub pages: usize,
}

/// Props of [`app_bar`].
pub struct AppBar {
    /// Title (the active tab's name).
    pub title: Reactive<String>,
    /// Link state.
    pub status: Status,
    /// `build-01 · direct 18ms`.
    pub link: String,
    /// Live port forwards (`⇄ n`).
    pub ports: u32,
    /// Hears `☰` (sessions, left region).
    pub on_menu: Rc<dyn Fn()>,
    /// Hears `⎇` (tools, right region).
    pub on_tools: Rc<dyn Fn()>,
}

/// App bar: `☰`, title over link, port chip, `⎇`.
pub fn app_bar(props: AppBar) -> View {
    let AppBar {
        title,
        status,
        link,
        ports,
        on_menu,
        on_tools,
    } = props;
    let title = typed(text(move || title.get()), typo::MONO_UI)
        .text_size(px(typo::HEADING.size))
        .font_weight(FontWeight(typo::HEADING.weight));
    let chip = hrow(space::SP_1)
        .h(px(row::ROW_TOUCH))
        .px(px(space::SP_3))
        .role(AccessibilityRole::Button)
        .label(format!("Ports, {ports} live"))
        .child(
            hrow(0.0)
                .h(px(row::ROW_MD))
                .px(px(space::SP_3))
                .rounded(px(row::ROW_MD / 2.0))
                .border(px(HAIRLINE))
                .border_color(color::ACCENT_BORDER)
                .child(mono(format!("⇄ {ports}"), color::ACCENT)),
        );
    let link = hrow(space::SP_2)
        .child(state_glyph(status))
        .child(meta(link).text_color(color::FG_MUTED));
    hrow(space::SP_1)
        .flex_none()
        .h(px(APP_BAR))
        .px(px(space::SP_1))
        .bg(color::BG_1)
        .border_b(px(HAIRLINE))
        .border_color(color::LINE)
        .child(icon_button("☰", "Open sessions").on_press(move |_: &PressEvent| on_menu()))
        .child(
            view()
                .flex_1()
                .min_w(px(0.0))
                .child(title.text_color(color::FG))
                .child(link),
        )
        .child(chip)
        .child(
            icon_button("⎇", "Tools: files, git, ports").on_press(move |_: &PressEvent| on_tools()),
        )
}

/// Tab strip: pills, then `+`. `on_select` hears a pill press.
pub fn tab_strip(
    tabs: Vec<TabPill>,
    active: impl IntoReactive<usize>,
    on_select: impl Fn(usize) + 'static,
) -> View {
    let active = active.into_reactive();
    let on_select: Rc<dyn Fn(usize)> = Rc::new(on_select);
    hrow(space::SP_2)
        .flex_none()
        .h(px(row::ROW_TOUCH))
        .px(px(space::SP_2))
        .border_b(px(HAIRLINE))
        .border_color(color::LINE)
        .overflow_hidden()
        .role(AccessibilityRole::Tablist)
        .child(dynamic(move || {
            let active = active.get();
            tabs.iter()
                .enumerate()
                .map(|(i, t)| {
                    let on_select = on_select.clone();
                    tab_pill(t, i == active).on_press(move |_: &PressEvent| on_select(i))
                })
                .collect::<Vec<_>>()
        }))
        .child(spacer())
        .child(icon_button("+", "New tab"))
}

/// Tab pill. The pill is 32 tall inside a 44pt tap target.
pub(crate) fn tab_pill(tab: &TabPill, active: bool) -> View {
    let fg = if active { color::FG } else { color::FG_MUTED };
    let pill = hrow(space::SP_2)
        .h(px(row::ROW_MD))
        .px(px(space::SP_3))
        .rounded(px(row::ROW_MD / 2.0))
        .border(px(HAIRLINE))
        .border_color(if active { color::ACCENT } else { color::LINE })
        .when(active, |t| t.bg(color::BG_3))
        .child(state_glyph(tab.status))
        .child(mono(tab.name.clone(), fg))
        .when(active, |t| {
            t.child(badge(format!("▦{}", tab.pages), color::FG_DIM))
        });
    hrow(0.0)
        .flex_none()
        .h(px(row::ROW_TOUCH))
        .role(AccessibilityRole::Tab)
        .label(tab.name.clone())
        .accessibility_state(selected_state(active))
        .child(pill)
}

/// `pane n/m · process`, page dots, `swipe ⇆`. A tap hears `on_next`.
///
/// `pages` holds each page's state; `current` is the shown page.
pub fn pane_switcher(
    pages: &[Status],
    current: usize,
    process: &str,
    on_next: impl Fn() + 'static,
) -> View {
    let count = pages.len();
    hrow(space::SP_2)
        .flex_none()
        .h(px(row::ROW_TOUCH))
        .px(px(space::SP_3))
        .bg(color::BG_1)
        .border_b(px(HAIRLINE))
        .border_color(color::LINE)
        .role(AccessibilityRole::Button)
        .label("Next pane")
        .on_press(move |_: &PressEvent| on_next())
        .child(meta(format!("pane {}/{count}", current + 1)).text_color(color::FG))
        .child(meta(format!("· {process}")).text_color(color::FG_MUTED))
        .child(spacer())
        .child(
            hrow(space::SP_1).children(
                pages
                    .iter()
                    .enumerate()
                    .map(|(i, s)| page_dot(*s, i == current)),
            ),
        )
        .child(meta("swipe ⇆"))
}

/// One page dot. The current page is longer and accent; others take
/// their pane's state hue (shape: short bar).
fn page_dot(status: Status, current: bool) -> View {
    let (w, hue) = match (current, status) {
        (true, _) => (PAGE_DOT.0, color::ACCENT),
        (false, Status::Waiting) => (PAGE_DOT.1, state::WAITING),
        (false, Status::Error) => (PAGE_DOT.1, state::ERROR),
        (false, _) => (PAGE_DOT.1, color::FG_FAINT),
    };
    view()
        .w(px(w))
        .h(px(PAGE_DOT.2))
        .rounded(px(PAGE_DOT.2 / 2.0))
        .bg(hue)
}

/// Extra keys. `latched` lights the modifier keys; `on_key` hears a press
/// with the key.
pub fn extra_keys(
    keys: Vec<ExtraKey>,
    latched: impl IntoReactive<bool>,
    on_key: impl Fn(&ExtraKey) + 'static,
) -> View {
    let latched = latched.into_reactive();
    let on_key: Rc<dyn Fn(&ExtraKey)> = Rc::new(on_key);
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
            let latched = latched.get();
            keys.iter()
                .map(|k| extra_key(k, k.modifier && latched, on_key.clone()))
                .collect::<Vec<_>>()
        }))
}

fn extra_key(key: &ExtraKey, latched: bool, on_key: Rc<dyn Fn(&ExtraKey)>) -> View {
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
    let pressed = key.clone();
    view()
        .flex_1()
        .items_center()
        .justify_center()
        .bg(bg)
        .role(AccessibilityRole::KeyboardKey)
        .label(name)
        .on_press(move |_: &PressEvent| on_key(&pressed))
        .child(typed(text(key.label.clone()), typo::MONO_TERM).text_color(fg))
}

/// The command field and `↵`. Enter or `↵` hears `on_submit` with the draft.
pub fn input_bar(draft: Signal<String>, on_submit: impl Fn(String) + 'static) -> View {
    let on_submit: Rc<dyn Fn(String)> = Rc::new(on_submit);
    let send = on_submit.clone();
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
            draft,
            Density::Regular,
            move |cmd| on_submit(cmd),
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
                .on_press(move |_: &PressEvent| send(draft.get_untracked())),
        )
}
