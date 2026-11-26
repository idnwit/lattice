//! Lattice deck: the native control center, built with NSH (pure Rust).
//!
//! The M1 PoC shell (D40) over `lattice-deck-ui`: tabs, splits, the `+` menu and the app
//! picker of the built-in apps, and one WASM viewer per live pane (D42). The deck holds the
//! app list (`lattice-builtin-apps`) and serves packages to the worker (D44).
//!
//! - `workspace`: tabs, split trees, panes, focus, as data.
//! - `link`: the link to the active worker; `instances`: the viewers over it.
//! - `pairing`, `invite`: the Add worker dialog and its pairing task (D47).
//! - `rail`, `status`: the worker rail and the link state.
//! - `data`: the data dir (`LATTICE_DATA_DIR`, else the OS data dir).
//! - `interp`: the view-tree interpreter.
//! - [`DeckApp`]: the root component.

mod data;
mod instances;
mod interp;
mod invite;
mod link;
mod pairing;
mod rail;
mod root;
mod status;
mod workspace;

pub use root::DeckApp;

use nsh_core::prelude::*;

/// The deck's NSH component set. The `nsh` CLI's runners (iOS, packaging)
/// look it up to mount [`DeckApp`] as the app root (`nsh.json` `"root"`).
#[native_module("lattice-deck/native")]
fn native(registry: &mut Registry) {
    registry.register::<DeckApp>();
}
