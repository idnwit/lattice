//! Mobile terminal (artboard "Mobile — Terminal", 390×844).
//!
//! App bar · tabs · pane switcher · terminal · approval card · extra keys ·
//! input. Every tap target is at least `row/touch` (44).

mod input;

use nsh_core::prelude::*;

use super::approval::{approval_card, verdict_note};
use super::components::state_glyph;
use super::state::Phone;
use super::style::{HAIRLINE, badge, body, hrow, meta, mono, selected_state, spacer, typed};
use super::terminal::term_view;
use super::tokens::{color, radius, row, space, state, typo};
use crate::model::{Density, MobileTab, MobileTerminal};
use lattice_deck_core::Status;

/// App bar height (canvas: 52).
const APP_BAR: f32 = row::ROW_TOUCH + space::SP_2;
/// Page dot: active 18×4, others 6×4.
const PAGE_DOT: (f32, f32, f32) = (18.0, 6.0, 4.0);

/// The whole mobile terminal screen, with its local state.
pub(crate) fn terminal(m: &MobileTerminal) -> View {
    let phone = Phone::new(m);
    let (tabs, approval) = (m.tabs.clone(), m.approval.clone());
    let pages_tabs = tabs.clone();
    view()
        .flex_1()
        .bg(color::BG_0)
        .p_safe_area()
        .child(app_bar(m, phone))
        .child(tab_strip(tabs, phone))
        .child(dynamic(move || {
            pages_tabs
                .get(phone.tab.get())
                .map(|t| pane_switcher(t, phone))
        }))
        .child(dynamic(move || term_view(&phone.shown(), Density::Compact)))
        .child(
            view()
                .flex_none()
                .mx(px(space::SP_2))
                .mb(px(space::SP_2))
                .child(dynamic(move || match phone.verdict.get() {
                    None => approval_card(&approval, row::ROW_TOUCH, move |v| {
                        phone.verdict.set(Some(v))
                    }),
                    Some(v) => verdict_note(&approval, v),
                })),
        )
        .child(input::extra_keys(m.keys.clone(), phone))
        .child(input::input_bar(phone))
}

/// A square tap target with a glyph.
fn icon_button(glyph: &str, name: &str) -> View {
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

fn app_bar(m: &MobileTerminal, phone: Phone) -> View {
    let names: Vec<String> = m.tabs.iter().map(|t| t.name.clone()).collect();
    let title = typed(
        text(move || names.get(phone.tab.get()).cloned().unwrap_or_default()),
        typo::MONO_UI,
    )
    .text_size(px(typo::HEADING.size))
    .font_weight(FontWeight(typo::HEADING.weight));
    let ports = hrow(space::SP_1)
        .h(px(row::ROW_TOUCH))
        .px(px(space::SP_3))
        .role(AccessibilityRole::Button)
        .label(format!("Ports, {} live", m.ports))
        .child(
            hrow(0.0)
                .h(px(row::ROW_MD))
                .px(px(space::SP_3))
                .rounded(px(row::ROW_MD / 2.0))
                .border(px(HAIRLINE))
                .border_color(color::ACCENT_BORDER)
                .child(mono(format!("⇄ {}", m.ports), color::ACCENT)),
        );
    let link = hrow(space::SP_2)
        .child(state_glyph(m.status))
        .child(meta(format!("{} · {}", m.worker, m.link)).text_color(color::FG_MUTED));
    hrow(space::SP_1)
        .flex_none()
        .h(px(APP_BAR))
        .px(px(space::SP_1))
        .bg(color::BG_1)
        .border_b(px(HAIRLINE))
        .border_color(color::LINE)
        .child(icon_button("☰", "Open sessions"))
        .child(
            view()
                .flex_1()
                .min_w(px(0.0))
                .child(title.text_color(color::FG))
                .child(link),
        )
        .child(ports)
        .child(icon_button("⎇", "Tools: files, git, ports"))
}

fn tab_strip(tabs: Vec<MobileTab>, phone: Phone) -> View {
    hrow(space::SP_2)
        .flex_none()
        .h(px(row::ROW_TOUCH))
        .px(px(space::SP_2))
        .border_b(px(HAIRLINE))
        .border_color(color::LINE)
        .overflow_hidden()
        .role(AccessibilityRole::Tablist)
        .child(dynamic(move || {
            let active = phone.tab.get();
            tabs.iter()
                .enumerate()
                .map(|(i, t)| {
                    tab_pill(t, i == active).on_press(move |_: &PressEvent| phone.pick_tab(i))
                })
                .collect::<Vec<_>>()
        }))
        .child(spacer())
        .child(icon_button("+", "New tab"))
}

/// Tab pill. The pill is 32 tall inside a 44pt tap target.
fn tab_pill(tab: &MobileTab, active: bool) -> View {
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
            t.child(badge(format!("▦{}", tab.pages.len()), color::FG_DIM))
        });
    hrow(0.0)
        .flex_none()
        .h(px(row::ROW_TOUCH))
        .role(AccessibilityRole::Tab)
        .label(tab.name.clone())
        .accessibility_state(selected_state(active))
        .child(pill)
}

/// `pane n/m · process`, page dots, `swipe ⇆`. A tap shows the next pane.
fn pane_switcher(tab: &MobileTab, phone: Phone) -> View {
    let count = tab.pages.len();
    let current = phone.page.get().min(count.saturating_sub(1));
    let process = tab
        .pages
        .get(current)
        .map(|p| p.process.clone())
        .unwrap_or_default();
    hrow(space::SP_2)
        .flex_none()
        .h(px(row::ROW_TOUCH))
        .px(px(space::SP_3))
        .bg(color::BG_1)
        .border_b(px(HAIRLINE))
        .border_color(color::LINE)
        .role(AccessibilityRole::Button)
        .label("Next pane")
        .on_press(move |_: &PressEvent| phone.next_page(count))
        .child(meta(format!("pane {}/{count}", current + 1)).text_color(color::FG))
        .child(meta(format!("· {process}")).text_color(color::FG_MUTED))
        .child(spacer())
        .child(
            hrow(space::SP_1).children(
                tab.pages
                    .iter()
                    .enumerate()
                    .map(|(i, p)| page_dot(p.status, i == current)),
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
