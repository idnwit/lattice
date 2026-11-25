//! Left side: worker rail and worker-scoped panel.

use nsh_core::prelude::*;

use super::super::components::{session_row, state_glyph, worker_card};
use super::super::state::Desk;
use super::super::style::{HAIRLINE, MONO_BADGE, badge, body, hrow, label, meta, typed};
use super::super::tokens::{color, radius, row, space, state, typo};
use crate::model::{RailItem, Session, Workspace};

/// Left column width (canvas: 288).
const SIDEBAR_W: f32 = 288.0;
/// Rail width (canvas: 52).
const RAIL_W: f32 = row::ROW_TOUCH + space::SP_2;
/// Rail badge size (canvas: 14).
const RAIL_BADGE: f32 = 14.0;

/// Rail + panel.
pub(super) fn sidebar(ws: &Workspace, desk: Desk) -> View {
    view()
        .flex_none()
        .flex_row()
        .w(px(SIDEBAR_W))
        .border_r(px(HAIRLINE))
        .border_color(color::LINE)
        .child(rail(&ws.rail, desk))
        .child(panel(ws, desk))
}

fn rail(items: &[RailItem], desk: Desk) -> View {
    view()
        .flex_none()
        .w(px(RAIL_W))
        .items_center()
        .gap(px(space::SP_2))
        .py(px(space::SP_2))
        .bg(color::BG_0)
        .border_r(px(HAIRLINE))
        .border_color(color::LINE)
        .role(AccessibilityRole::Tablist)
        .children(items.iter().map(|item| rail_item(item, desk)))
        .child(view().w(px(space::SP_6)).h(px(HAIRLINE)).bg(color::LINE))
        .child(
            rail_button(body("+", color::FG_MUTED))
                .border_dashed()
                .border_color(color::LINE_STRONG),
        )
        .child(view().flex_1())
        .child(badge("⌘1–9", color::FG_DIM))
}

fn rail_button(content: Text) -> View {
    view()
        .size(px(row::ROW_KEY))
        .items_center()
        .justify_center()
        .rounded(px(radius::R_6))
        .border(px(HAIRLINE))
        .child(content)
}

fn rail_item(item: &RailItem, desk: Desk) -> View {
    // The waiting agent runs on the picked worker; `∗` counts all workers.
    let has_waiter = item.selected || item.status.is_none();
    let fg = if item.status == Some(lattice_deck_core::Status::Offline) {
        color::FG_DIM
    } else {
        color::FG
    };
    let mono = typed(text(item.mono.clone()), typo::MONO_UI)
        .font_weight(FontWeight(typo::HEADING.weight))
        .text_color(fg);
    rail_button(mono)
        .relative()
        .role(AccessibilityRole::Tab)
        .bg(if item.selected {
            color::BG_3
        } else {
            color::BG_1
        })
        .border_color(if item.selected {
            color::LINE_STRONG
        } else {
            color::LINE
        })
        .when_some(item.status, |b, s| {
            b.child(
                view()
                    .absolute()
                    .right(px(3.0))
                    .bottom(px(3.0))
                    .child(state_glyph(s)),
            )
        })
        .when(has_waiter, |b| {
            b.child(dynamic(move || {
                desk.verdict.get().is_none().then(|| wait_badge(1))
            }))
        })
}

fn wait_badge(n: u32) -> View {
    view()
        .absolute()
        .top(px(-space::SP_1))
        .right(px(-space::SP_1))
        .min_w(px(RAIL_BADGE))
        .h(px(RAIL_BADGE))
        .px(px(3.0))
        .rounded_full()
        .items_center()
        .justify_center()
        .bg(state::WAITING)
        .child(
            badge(n.to_string(), color::BG_0)
                .text_size(px(MONO_BADGE - 1.0))
                .font_weight(FontWeight(700)),
        )
}

fn panel(ws: &Workspace, desk: Desk) -> View {
    let w = &ws.worker;
    let agents: usize = w.sessions.iter().map(|s| s.agents.len()).sum();
    view()
        .flex_1()
        .min_w(px(0.0))
        .bg(color::BG_1)
        .child(worker_card(w))
        .child(filter(&w.name))
        .child(
            hrow(space::SP_2)
                .h(px(row::ROW_SM - 2.0))
                .px(px(space::SP_3))
                .child(label("Sessions"))
                .child(meta(format!("{} · {agents} agents", w.sessions.len()))),
        )
        .child(
            view()
                .flex_1()
                .min_h(px(0.0))
                .overflow_hidden()
                .child(sessions(w.sessions.clone(), desk)),
        )
        .child(
            hrow(space::SP_2)
                .flex_none()
                .h(px(row::ROW_SM))
                .px(px(space::SP_3))
                .border_t(px(HAIRLINE))
                .border_color(color::LINE)
                .children(
                    ["⌘1–9 worker", "⌘J session", "⌘B hide"].map(|s| badge(s, color::FG_DIM)),
                ),
        )
}

fn filter(worker: &str) -> View {
    view()
        .px(px(space::SP_2))
        .pt(px(space::SP_2))
        .pb(px(space::SP_1))
        .child(
            hrow(space::SP_2)
                .h(px(row::ROW_SM - 2.0))
                .px(px(space::SP_2))
                .rounded(px(radius::R_3))
                .border(px(HAIRLINE))
                .border_color(color::LINE)
                .bg(color::BG_0)
                .role(AccessibilityRole::Search)
                .child(body(format!("Filter {worker}…"), color::FG_DIM)),
        )
}

/// Session rows; a press selects one.
fn sessions(list: Vec<Session>, desk: Desk) -> AnyElement {
    dynamic(move || {
        let (picked, answered) = (desk.session.get(), desk.verdict.get());
        list.iter()
            .enumerate()
            .map(|(i, s)| {
                session_row(s, i == picked, answered)
                    .on_press(move |_: &PressEvent| desk.session.set(i))
            })
            .collect::<Vec<_>>()
    })
}
