//! Lattice deck: the native control center UI, built with NSH (pure Rust).
//!
//! A demo of two design mocks with static fixture data and no networking:
//! the desktop workspace and the mobile terminal screen. [`DeckApp`] picks
//! one by viewport width.
//!
//! - `model`: view data the screens take as props.
//! - `fixtures`: the mocks' values. Deck-core state replaces them later.
//! - [`ui`]: NSH components; `ui::tokens` is generated from
//!   `docs/design-system/tokens.json`.

mod fixtures;
mod model;
mod shell;
pub mod ui;

pub use ui::DeckApp;

use nsh_core::prelude::*;

/// The deck's NSH component set. The `nsh` CLI's runners (iOS, packaging)
/// look it up to mount [`DeckApp`] as the app root (`nsh.json` `"root"`).
#[native_module("lattice-deck/native")]
fn native(registry: &mut Registry) {
    registry.register::<DeckApp>();
}
