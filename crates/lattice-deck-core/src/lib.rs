//! The deck host: everything a deck does except drawing.
//!
//! Owner specs: `docs/architecture/05-deck.md`, `docs/specs/03-sessions.md`,
//! `docs/specs/02-connectivity.md`.
//! macOS and iOS share this crate unchanged. Only the views in `lattice-deck`
//! differ. It dials workers through `lattice_transport::Transport` and never
//! names `quinn` or NSH.
//!
//! Not here yet (later layers): local store, layout engine, WASM app host.
//!
//! # Usage
//!
//! ```no_run
//! # async fn run<T: lattice_transport::Transport>(t: T, worker: lattice_key::NodeId) -> Result<(), lattice_deck_core::DeckError> {
//! use lattice_deck_core::DeckHost;
//!
//! let host = DeckHost::new(t);
//! let mut link = host.open_link(worker).await?;
//! link.subscribe("proc/1/raw".parse().unwrap()).await?;
//! # Ok(()) }
//! ```
//!
//! # Key items
//!
//! - [`DeckHost`], [`WorkerLink`]: dial a worker, subscribe, send input.
//! - [`DeckState`]: workers and their links; what views render.
//! - [`PaneId`]: `<tab>.<pane>` ids (`1.2`).
//! - [`Status`]: state glyph values and busiest-child priority.

#![deny(clippy::indexing_slicing, clippy::arithmetic_side_effects)]

mod link;
mod model;
mod status;

pub use link::{DeckError, DeckHost, WorkerLink};
pub use model::{DeckState, PaneId, PaneIdError, WorkerEntry};
pub use status::Status;
