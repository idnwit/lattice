//! Built-in `echo` app: lines sent by viewers, kept in state (`docs/architecture/06-apps.md`).
//!
//! The second built-in app after `terminal` (D40). It proves the platform is not biased
//! to the terminal: no `[[spawn]]`, no process, no journal output. Every change comes
//! from a reducer.
//!
//! One crate, two parts. `model` and `api` compile into both; feature `runner` adds the
//! worker side ([`Echo`], an [`lattice_app::App`]); feature `viewer` adds the deck side
//! ([`EchoViewer`], view trees). Each builds to a WASM module of the package (D42).
//!
//! Behavior:
//! - `start` and `resumed` do nothing: there is nothing to spawn or re-issue.
//! - `send` appends a [`Line`] with the next sequence number and returns it.
//!   Blank text is refused ([`EchoError::Empty`]), text over [`MAX_TEXT_BYTES`] too
//!   ([`EchoError::TooLong`]); a refused call commits no delta.
//! - `clear` empties the lines. Sequence numbers keep counting.
//! - At most [`LINES_CAP`] lines (feature `runner`); see `runner` for the overflow rule.
//!
//! # Restart
//!
//! Actions are never stored (D26) and the app has no process whose output could be
//! replayed. So nothing survives a `latticed` restart: the instance restores from an
//! empty journal to an empty [`EchoState`], and sequence numbers start again at 1
//! (06-apps.md, Design philosophy rule 2: state not derivable from process output is lost).
//!
//! # Key items
//!
//! - [`EchoState`], [`Line`]: what decks hold and render.
//! - [`EchoApi`]: reducers `send` ([`Message`] → `u64` seq) and `clear`;
//!   [`EchoApiClient`] for viewers and the CLI, [`EchoApiRunner`] for the worker host.
//! - [`EchoViewer`]: lines, a field (`send`), and `Clear`.
//! - [`EchoParams`], [`EchoError`].

mod api;
mod model;
#[cfg(feature = "runner")]
mod runner;
#[cfg(feature = "viewer")]
mod viewer;

pub use api::{
    EchoApi, EchoApiAction, EchoApiClient, EchoApiRunner, EchoError, EchoParams, MAX_TEXT_BYTES,
    Message,
};
pub use model::{EchoState, Line};
#[cfg(feature = "runner")]
pub use runner::{Echo, LINES_CAP};
#[cfg(feature = "viewer")]
pub use viewer::EchoViewer;

// The package modules: ABI v0 exports on wasm32, nothing natively (D43).
// A wasm32 build enables one of `runner` and `viewer`.
#[cfg(feature = "runner")]
lattice_app_guest::export_runner!(EchoApiRunner<Echo>);
#[cfg(feature = "viewer")]
lattice_app_guest::export_viewer!(EchoViewer);

/// App id, as in `app.toml`.
pub const APP_ID: &str = "dev.lattice.echo";
