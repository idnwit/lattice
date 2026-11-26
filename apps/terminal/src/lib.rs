//! Built-in `terminal` app: a shell on a PTY (`docs/architecture/06-apps.md`).
//!
//! One crate, two parts. `model` and `api` compile into both; feature `runner`
//! adds the worker side ([`Terminal`], an [`lattice_app::App`]); feature
//! `viewer` adds the deck side ([`TermViewer`], view trees). Each builds to a WASM
//! module of the package (D42).
//!
//! Behavior:
//! - `start` spawns the `shell` template from `app.toml`.
//! - `input` writes keystrokes straight to the shell. They are never stored.
//! - Output feeds a VT emulator (private runner memory) and updates [`TermState`].
//! - `clear` (`ESC[2J` + `ESC[3J`) outside the alternate screen is a clean point: trim.
//! - The shell exiting ends the instance (`finish`); the pane closes.
//!
//! # Key items
//!
//! - [`TermState`]: screen, scrollback, size, title. Decks hold a replica.
//! - [`Row`], [`Run`], [`Color`], [`Attrs`]: styled rows; decks map colors to tokens.
//! - [`TerminalApi`]: reducers `input` ([`Input`]) and `resize` ([`Resize`] → [`Size`]);
//!   [`TerminalApiClient`] for viewers, [`TerminalApiRunner`] for the worker host.
//! - [`TermViewer`]: the screen as one terminal widget; [`key_bytes`]: keys as xterm bytes.
//! - [`TermParams`], [`TermError`].

mod api;
mod model;
#[cfg(feature = "runner")]
mod runner;
#[cfg(feature = "viewer")]
mod viewer;

pub use api::{
    Input, Resize, TermError, TermParams, TerminalApi, TerminalApiAction, TerminalApiClient,
    TerminalApiRunner,
};
pub use model::{Attrs, Color, Cursor, Row, Run, Size, TermState};
#[cfg(feature = "runner")]
pub use runner::{SCROLLBACK_CAP, Terminal};
#[cfg(feature = "viewer")]
pub use viewer::{TermViewer, key_bytes};

// The package modules: ABI v0 exports on wasm32, nothing natively (D43).
// A wasm32 build enables one of `runner` and `viewer`.
#[cfg(feature = "runner")]
lattice_app_guest::export_runner!(TerminalApiRunner<Terminal>);
#[cfg(feature = "viewer")]
lattice_app_guest::export_viewer!(TermViewer);

/// App id, as in `app.toml`.
pub const APP_ID: &str = "dev.lattice.terminal";
