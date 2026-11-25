//! Shared components, named after `docs/design-system/components.md`.

use nsh_core::prelude::*;

use super::style::{
    FOCUS_BAR, HAIRLINE, RING_STROKE, badge, body, hrow, meta, mono, selected_state, spacer, typed,
};
use super::tokens::{color, radius, row, space, state, tint, typo};
use crate::model::{AgentRow, Change, Mark, Pane, Session, Tab, Verdict, Worker};
use lattice_deck_core::Status;

/// Dot, ring and square glyph size (tokens.md: 7–8px).
const GLYPH: f32 = 7.0;
/// Diamond glyph size before rotation (tokens.md: 6–7px).
const DIAMOND: f32 = 6.0;
/// Waiting bar: 3×10 (tokens.md).
const BAR_W: f32 = 3.0;
/// Waiting bar height.
const BAR_H: f32 = 10.0;
/// Meter track height (components.md: 3px).
const METER: f32 = 3.0;
/// Load at which a meter turns `waiting` (components.md: ≥ 85%).
const METER_HOT: u8 = 85;

/// State glyph: shape + hue, never hue alone.
pub(crate) fn state_glyph(status: Status) -> View {
    let base = view().flex_none();
    match status {
        Status::Online => base.size(px(GLYPH)).rounded_full().bg(state::ONLINE),
        Status::Relay => ring(base, state::RELAY),
        Status::Agent => base.size(px(DIAMOND)).bg(state::AGENT).rotate(deg(45.0)),
        Status::Waiting => base.w(px(BAR_W)).h(px(BAR_H)).bg(state::WAITING),
        Status::Error => base.size(px(GLYPH)).bg(state::ERROR),
        Status::Offline => ring(base, state::OFFLINE),
        Status::Idle => ring(base, state::IDLE),
    }
}

fn ring(base: View, hue: Rgba) -> View {
    base.size(px(GLYPH))
        .rounded_full()
        .border(px(RING_STROKE))
        .border_color(hue)
}

/// Button variants (components.md › Button).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ButtonKind {
    /// Accent fill. One per surface.
    Primary,
    /// `bg/2` fill, `line/strong` border.
    Secondary,
    /// Transparent, no border.
    Ghost,
}

/// A button with a verb label and an optional chord hint.
pub(crate) fn button(kind: ButtonKind, text: &str, chord: Option<&str>, height: f32) -> View {
    let (fill, fg, weight) = match kind {
        ButtonKind::Primary => (color::ACCENT, color::ACCENT_INK, 600),
        ButtonKind::Secondary => (color::BG_2, color::FG, 500),
        ButtonKind::Ghost => (transparent_black().into(), color::FG, 500),
    };
    hrow(space::SP_2)
        .justify_center()
        .h(px(height))
        .px(px(space::SP_3))
        .rounded(px(radius::R_3))
        .bg(fill)
        .when(kind == ButtonKind::Secondary, |b| {
            b.border(px(HAIRLINE)).border_color(color::LINE_STRONG)
        })
        .role(AccessibilityRole::Button)
        .label(text.to_owned())
        .child(body(text.to_owned(), fg).font_weight(FontWeight(weight)))
        .when_some(chord, |b, c| {
            b.child(badge(
                c.to_owned(),
                if kind == ButtonKind::Primary {
                    fg
                } else {
                    color::FG_DIM
                },
            ))
        })
}

/// Port chip: `⇄ n` in accent, thin accent border.
pub(crate) fn port_chip(count: u32) -> View {
    view()
        .px(px(space::SP_1))
        .rounded(px(radius::R_3))
        .border(px(HAIRLINE))
        .border_color(color::ACCENT_BORDER)
        .child(badge(format!("⇄ {count}"), color::ACCENT))
}

/// Terminal tab: glyph, name, pane count. Active: `bg/3` + accent top bar.
pub(crate) fn terminal_tab(tab: &Tab, active: bool) -> View {
    let fg = if active { color::FG } else { color::FG_MUTED };
    hrow(space::SP_2)
        .h_full()
        .px(px(space::SP_3))
        .border_r(px(HAIRLINE))
        .border_color(color::LINE)
        .role(AccessibilityRole::Tab)
        .label(tab.name.clone())
        .accessibility_state(selected_state(active))
        .when(active, |t| {
            t.bg(color::BG_3)
                .border_t(px(FOCUS_BAR))
                .border_t_color(color::ACCENT)
        })
        .child(state_glyph(tab.status))
        .child(mono(tab.name.clone(), fg))
        .child(badge(format!("▦{}", tab.panes()), color::FG_DIM))
}

/// Pane header: glyph, worker, context or agent, note or id.
///
/// `status` and `note` are live: an answered approval changes them.
pub(crate) fn pane_header(pane: &Pane, status: Status, note: Option<&str>) -> View {
    let fg = if pane.focused {
        color::FG
    } else {
        color::FG_MUTED
    };
    let right = match note {
        Some(note) => meta(note.to_owned()).text_color(state::WAITING),
        None => meta(pane.id.clone()),
    };
    hrow(space::SP_2)
        .flex_none()
        .h(px(row::ROW_SM))
        .px(px(space::SP_3))
        .bg(color::BG_1)
        .child(state_glyph(status))
        .child(meta(pane.worker.clone()).text_color(fg))
        .when(!pane.context.is_empty(), |h| {
            h.child(meta(pane.context.clone()))
        })
        .when_some(pane.agent.as_ref(), |h, a| {
            h.child(meta(format!("agent: {a}")).text_color(state::AGENT))
        })
        .child(spacer())
        .child(right)
        .when(pane.agent.is_some(), |h| {
            h.child(segmented(&["TTY", "Visual"], 0))
        })
}

/// Segmented control; `selected` is the chosen index.
pub(crate) fn segmented(items: &[&str], selected: usize) -> View {
    hrow(HAIRLINE)
        .p(px(HAIRLINE))
        .border(px(HAIRLINE))
        .border_color(color::LINE)
        .rounded(px(radius::R_3))
        .children(items.iter().enumerate().map(|(i, item)| {
            let on = i == selected;
            view()
                .px(px(space::SP_1))
                .rounded(px(radius::R_3))
                .when(on, |s| s.bg(color::BG_3))
                .child(badge(
                    (*item).to_owned(),
                    if on { color::FG } else { color::FG_MUTED },
                ))
        }))
}

/// Session row: glyph · name · port chip · tab count; agents nest below.
///
/// `answered` is the approval answer, which settles waiting agents.
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

/// A waiting state once the approval is answered: running again.
pub(crate) fn settle(status: Status, answered: Option<Verdict>) -> Status {
    match (status, answered) {
        (Status::Waiting, Some(Verdict::Deny)) => Status::Idle,
        (Status::Waiting, Some(_)) => Status::Agent,
        (s, _) => s,
    }
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

/// Worker card: name, OS, link; cpu and mem meters.
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

/// Progress meter: label over a 3px bar.
pub(crate) fn meter(caption: &str, pct: u8) -> View {
    let fill = if pct >= METER_HOT {
        state::WAITING
    } else {
        color::FG_MUTED
    };
    view()
        .flex_1()
        .gap(px(space::SP_1))
        .child(badge(caption.to_owned(), color::FG_MUTED))
        .child(
            view()
                .h(px(METER))
                .bg(color::LINE)
                .child(view().h_full().w(relative(f32::from(pct) / 100.0)).bg(fill)),
        )
}

/// Git change row: path · delta · mark.
pub(crate) fn change_row(change: &Change) -> View {
    let (mark, hue) = match change.mark {
        Mark::Modified => ("M", tint::DIFF_MODIFIED),
        Mark::Added => ("A", tint::DIFF_ADDED),
        Mark::Deleted => ("D", tint::DIFF_DELETED),
    };
    let deleted = change.mark == Mark::Deleted;
    let path = mono(
        change.path.clone(),
        if deleted { color::FG_MUTED } else { color::FG },
    );
    hrow(space::SP_2)
        .h(px(row::ROW_XS + space::SP_1 / 2.0))
        .px(px(space::SP_3))
        .child(path.when(deleted, Styled::line_through).flex_1())
        .child(meta(change.delta.clone()))
        .child(mono(mark, hue).w(px(space::SP_4)).text_center())
}

/// Switch: 30×16 track, 12px knob. On: accent track, knob right.
pub(crate) fn toggle(on: bool, enabled: bool, name: String) -> View {
    let track = if on {
        color::ACCENT
    } else {
        color::LINE_STRONG
    };
    view()
        .flex_none()
        .flex_row()
        .w(px(TOGGLE.0))
        .h(px(TOGGLE.1))
        .p(px(TOGGLE_PAD))
        .rounded_full()
        .bg(track)
        .when(on, |t| t.justify_end())
        .when(!enabled, |t| t.opacity(DISABLED_OPACITY))
        .role(AccessibilityRole::Switch)
        .label(name)
        .accessibility_state(selected_state(on))
        .child(
            view()
                .size(px(TOGGLE.1 - 2.0 * TOGGLE_PAD))
                .rounded_full()
                .bg(color::FG),
        )
}

/// Toggle track size (components.md: 30×16).
const TOGGLE: (f32, f32) = (30.0, 16.0);
/// Gap between track and knob.
const TOGGLE_PAD: f32 = 2.0;
/// Opacity of a control that cannot change.
const DISABLED_OPACITY: f32 = 0.4;
