//! Lattice deck: the native control center, built with NSH (pure Rust).
//!
//! The M1 PoC shell (D40) over `lattice-deck-ui`: tabs, splits, the `+` menu and the app
//! picker of the built-in apps, and one WASM viewer per live pane (D42). The deck holds the
//! app list (`lattice-builtin-apps`) and serves packages to the worker (D44).
//!
//! - `workspace`: tabs, split trees, panes, focus, as data.
//! - `link`: the local worker link (experiment, `LATTICE_DECK_LIVE=1`) and the viewers.
//! - `interp`: the view-tree interpreter.
//! - [`DeckApp`]: the root component.

mod interp;
mod link;
mod root;
mod workspace;

pub use root::DeckApp;

use nsh_core::prelude::*;

/// The deck's NSH component set. The `nsh` CLI's runners (iOS, packaging)
/// look it up to mount [`DeckApp`] as the app root (`nsh.json` `"root"`).
#[native_module("lattice-deck/native")]
fn native(registry: &mut Registry) {
    registry.register::<DeckApp>();
}
