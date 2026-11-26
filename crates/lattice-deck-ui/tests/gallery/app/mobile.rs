//! Mobile workspace of the canvas mock (artboard "Mobile — Terminal",
//! 390×844): app bar · tabs · pane switcher · lines · approval card ·
//! extra keys · input.

use std::rc::Rc;

use lattice_deck_ui::components::{
    AppBar, Density, TabPill, app_bar, approval_card, extra_keys, input_bar, pane_switcher,
    styled_lines, tab_strip, verdict_note,
};
use lattice_deck_ui::tokens::{color, row, space};
use lattice_deck_ui::{Region, Regions};
use nsh_core::prelude::*;

use super::model::MobileTerminal;
use super::state::Phone;

/// The whole mobile terminal screen. `☰` opens the region switcher, `⎇` the tools.
pub(crate) fn terminal(m: &MobileTerminal, phone: Phone, regions: Regions) -> View {
    let (tabs, approval) = (m.tabs.clone(), m.approval.clone());
    let names: Vec<String> = tabs.iter().map(|t| t.name.clone()).collect();
    let pills = tabs
        .iter()
        .map(|t| TabPill {
            name: t.name.clone(),
            status: t.status,
            pages: t.pages.len(),
        })
        .collect();
    view()
        .flex_1()
        .bg(color::BG_0)
        .child(app_bar(AppBar {
            title: (move || names.get(phone.tab.get()).cloned().unwrap_or_default())
                .into_reactive(),
            status: m.status,
            link: format!("{} · {}", m.worker, m.link),
            ports: m.ports,
            on_menu: Rc::new(move || regions.open_switcher()),
            on_tools: Rc::new(move || regions.show(Region::Right)),
        }))
        .child(tab_strip(pills, phone.tab, move |i| phone.pick_tab(i)))
        .child(dynamic(move || {
            tabs.get(phone.tab.get()).map(|t| {
                let count = t.pages.len();
                let current = phone.page.get().min(count.saturating_sub(1));
                let states: Vec<_> = t.pages.iter().map(|p| p.status).collect();
                let process = t
                    .pages
                    .get(current)
                    .map(|p| p.process.clone())
                    .unwrap_or_default();
                pane_switcher(&states, current, &process, move || phone.next_page(count))
            })
        }))
        .child(dynamic(move || {
            styled_lines(&phone.shown(), Density::Compact)
        }))
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
        .child(extra_keys(m.keys.clone(), phone.ctrl, move |k| {
            phone.key(&k.label, k.modifier)
        }))
        .child(input_bar(phone.draft, move |cmd| phone.run(&cmd)))
}
