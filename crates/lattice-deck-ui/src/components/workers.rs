//! Worker components: worker card, session row, worker rail, worker panel.

use std::rc::Rc;

use nsh_core::prelude::*;

use super::approval::{Verdict, settle};
use super::controls::{ButtonKind, button, meter, port_chip};
use super::glyph::{Status, state_glyph};
use crate::style::{
    FOCUS_BAR, HAIRLINE, MONO_BADGE, badge, body, hrow, label, meta, mono, selected_state, spacer,
    typed,
};
use crate::tokens::{color, radius, row, space, state, tint, typo};

/// Rail width (canvas: 52).
pub(crate) const RAIL_W: f32 = row::ROW_TOUCH + space::SP_2;
/// Rail badge size (canvas: 14).
const RAIL_BADGE: f32 = 14.0;

/// An agent nested under a session row.
#[derive(Debug, Clone, PartialEq)]
pub struct AgentRow {
    /// Agent name (`claude-code`).
    pub name: String,
    /// Meta text (`approve`, `15/40`, `done 2m`).
    pub meta: String,
    /// Agent state.
    pub status: Status,
}

/// A session row in the worker panel.
#[derive(Debug, Clone, PartialEq)]
pub struct Session {
    /// Session name.
    pub name: String,
    /// Enabled port forwards, if any.
    pub ports: Option<u32>,
    /// Tab count.
    pub tabs: u32,
    /// Busiest child state.
    pub status: Status,
    /// Agents running in it.
    pub agents: Vec<AgentRow>,
}

/// A worker as the worker card shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct Worker {
    /// Worker name (`build-01`).
    pub name: String,
    /// OS and arch line.
    pub os: String,
    /// Link and latency (`direct · 18ms`).
    pub link: String,
    /// Link state.
    pub status: Status,
    /// CPU load, percent.
    pub cpu_pct: u8,
    /// Memory label (`11/32G`).
    pub mem: String,
    /// Memory use, percent.
    pub mem_pct: u8,
    /// Its sessions.
    pub sessions: Vec<Session>,
}

/// An entry in the worker rail.
#[derive(Debug, Clone, PartialEq)]
pub struct RailItem {
    /// Two-letter monogram, or `∗` for all workers.
    pub mono: String,
    /// Link state; `None` for the all-workers entry.
    pub status: Option<Status>,
    /// The entry is selected.
    pub selected: bool,
}

/// Session row: glyph · name · port chip · tab count; agents nest below.
///
/// `answered` is the approval answer, which settles waiting agents. The row
/// head is a button named after the session: add `.on_press` to the result.
pub(crate) fn session_row(session: &Session, selected: bool, answered: Option<Verdict>) -> View {
    let status = settle(session.status, answered);
    let fg = match (status, selected) {
        (Status::Offline, _) => color::FG_DIM,
        (_, true) => color::FG,
        _ => color::FG_TERM,
    };
    let head = hrow(space::SP_2)
        .h(px(row::ROW_SM))
        .pl(px(space::SP_3))
        .pr(px(space::SP_3))
        .role(AccessibilityRole::Button)
        .label(session.name.clone())
        .accessibility_state(selected_state(selected))
        .when(selected, |r| {
            r.bg(color::BG_3)
                .border_l(px(FOCUS_BAR))
                .border_l_color(color::ACCENT)
        })
        .child(state_glyph(status))
        .child(mono(session.name.clone(), fg).flex_1().truncate())
        .when_some(session.ports, |r, n| r.child(port_chip(n)))
        .child(meta(session.tabs.to_string()));
    view()
        .child(head)
        .children(session.agents.iter().map(|a| agent_row(a, answered)))
}

fn agent_row(agent: &AgentRow, answered: Option<Verdict>) -> View {
    let status = settle(agent.status, answered);
    let meta_text = match (agent.status, answered) {
        (Status::Waiting, Some(v)) => v.done().to_owned(),
        _ => agent.meta.clone(),
    };
    let meta_color = match status {
        Status::Waiting => state::WAITING,
        Status::Error => state::ERROR,
        _ => color::FG_DIM,
    };
    hrow(space::SP_2)
        .h(px(row::ROW_XS))
        .pl(px(space::SP_8))
        .pr(px(space::SP_3))
        .child(state_glyph(status))
        .child(
            meta(agent.name.clone())
                .text_color(tint::AGENT_FG)
                .flex_1()
                .truncate(),
        )
        .child(badge(meta_text, meta_color))
}

/// Worker card: name, OS, link; cpu and mem meters; `+ Session`, `+ Agent`.
pub(crate) fn worker_card(worker: &Worker) -> View {
    let name = typed(text(worker.name.clone()), typo::MONO_UI)
        .text_size(px(typo::HEADING.size))
        .font_weight(FontWeight(typo::HEADING.weight));
    view()
        .gap(px(space::SP_2))
        .p(px(space::SP_3))
        .border_b(px(HAIRLINE))
        .border_color(color::LINE)
        .child(
            hrow(space::SP_2)
                .child(state_glyph(worker.status))
                .child(name.text_color(color::FG))
                .child(spacer())
                .child(meta(worker.link.clone()).text_color(color::FG_MUTED)),
        )
        .child(meta(worker.os.clone()))
        .child(
            hrow(space::SP_3)
                .child(meter(&format!("cpu {}%", worker.cpu_pct), worker.cpu_pct))
                .child(meter(&format!("mem {}", worker.mem), worker.mem_pct)),
        )
        .child(
            hrow(space::SP_2)
                .child(button(ButtonKind::Secondary, "+ Session", Some("⌘T"), row::ROW_SM).flex_1())
                .child(button(ButtonKind::Secondary, "+ Agent", Some("⌘⇧A"), row::ROW_SM).flex_1()),
        )
}

/// Worker rail: monograms with link glyphs, `+`, and the `⌘1–9` hint.
///
/// `waiting` shows the waiting badge on the selected and the all-workers entries.
/// `on_add` hears `+` (Add worker, patterns.md).
pub fn worker_rail(
    items: &[RailItem],
    waiting: impl IntoReactive<bool>,
    on_add: impl Fn() + 'static,
) -> View {
    let waiting = waiting.into_reactive();
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
        .children(items.iter().map(|item| rail_item(item, waiting.clone())))
        .child(view().w(px(space::SP_6)).h(px(HAIRLINE)).bg(color::LINE))
        .child(
            rail_button(body("+", color::FG_MUTED))
                .border_dashed()
                .border_color(color::LINE_STRONG)
                .role(AccessibilityRole::Button)
                .label("Add worker")
                .on_press(move |_: &PressEvent| on_add()),
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

fn rail_item(item: &RailItem, waiting: Reactive<bool>) -> View {
    // The waiting agent runs on the picked worker; `∗` counts all workers.
    let has_waiter = item.selected || item.status.is_none();
    let fg = if item.status == Some(Status::Offline) {
        color::FG_DIM
    } else {
        color::FG
    };
    let mono = typed(text(item.mono.clone()), typo::MONO_UI)
        .font_weight(FontWeight(typo::HEADING.weight))
        .text_color(fg);
    let (bg, border) = if item.selected {
        (color::BG_3, color::LINE_STRONG)
    } else {
        (color::BG_1, color::LINE)
    };
    rail_button(mono)
        .relative()
        .role(AccessibilityRole::Tab)
        .label(item.mono.clone())
        .accessibility_state(selected_state(item.selected))
        .bg(bg)
        .border_color(border)
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
            b.child(dynamic(move || waiting.get().then(|| wait_badge(1))))
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

/// Worker panel: worker card, filter, the sessions list, chord hints.
///
/// `selected` is the selected session index, `answered` the approval
/// answer; `on_select` hears a session press.
pub fn worker_panel(
    worker: &Worker,
    selected: impl IntoReactive<usize>,
    answered: impl IntoReactive<Option<Verdict>>,
    on_select: impl Fn(usize) + 'static,
) -> View {
    let agents: usize = worker.sessions.iter().map(|s| s.agents.len()).sum();
    let (selected, answered) = (selected.into_reactive(), answered.into_reactive());
    let (list, on_select) = (worker.sessions.clone(), Rc::new(on_select));
    let sessions = dynamic(move || {
        let (picked, answered) = (selected.get(), answered.get());
        list.iter()
            .enumerate()
            .map(|(i, s)| {
                let on_select = on_select.clone();
                session_row(s, i == picked, answered).on_press(move |_: &PressEvent| on_select(i))
            })
            .collect::<Vec<_>>()
    });
    view()
        .flex_1()
        .min_w(px(0.0))
        .bg(color::BG_1)
        .child(worker_card(worker))
        .child(filter(&worker.name))
        .child(
            hrow(space::SP_2)
                .h(px(row::ROW_SM - 2.0))
                .px(px(space::SP_3))
                .child(label("Sessions"))
                .child(meta(format!("{} · {agents} agents", worker.sessions.len()))),
        )
        .child(
            view()
                .flex_1()
                .min_h(px(0.0))
                .overflow_hidden()
                .child(sessions),
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
