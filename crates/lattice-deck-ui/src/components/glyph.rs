//! State glyph (tokens.md, State): shape + hue, never hue alone.

use nsh_core::prelude::*;

use crate::style::RING_STROKE;
use crate::tokens::state;

/// Dot, ring and square glyph size (tokens.md: 7–8px).
const GLYPH: f32 = 7.0;
/// Diamond glyph size before rotation (tokens.md: 6–7px).
const DIAMOND: f32 = 6.0;
/// Waiting bar: 3×10 (tokens.md).
const BAR_W: f32 = 3.0;
/// Waiting bar height.
const BAR_H: f32 = 10.0;

/// State of a worker, pane, tab, session, or agent, as the UI draws it.
///
/// TBD: `lattice_deck_core::Status` holds the same values plus the
/// busiest-child order; the deck maps one to the other until one crate owns both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Worker reachable, direct P2P: dot.
    Online,
    /// Reachable via relay: ring.
    Relay,
    /// Agent running: diamond.
    Agent,
    /// Needs input or approval: bar.
    Waiting,
    /// Exit ≠ 0, link lost: square.
    Error,
    /// Worker asleep: dim ring.
    Offline,
    /// Agent idle: ring.
    Idle,
}

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
