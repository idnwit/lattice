//! Params, reducers, and errors of the terminal app.

use lattice_app::{App, Ctx};
use serde::{Deserialize, Serialize};

use crate::model::{Size, TermState};

/// Columns until the first resize, when params name none.
pub(crate) const DEFAULT_COLS: u16 = 80;
/// Rows until the first resize, when params name none.
pub(crate) const DEFAULT_ROWS: u16 = 24;

/// Params of `create`. The default (empty params): the worker's default cwd (`$HOME`) at
/// 80 by 24. TBD: default `shell` (specs/03-sessions.md rule 12)?
///
/// `cols` and `rows` size the emulator until the first `Resize`; zero or absent means
/// `DEFAULT_COLS` by `DEFAULT_ROWS` (80 by 24). TBD: must match the PTY size `latticed-proc`
/// spawns with; not yet in `app.toml` `[params]` (params type set is TBD).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TermParams {
    cwd: String,
    #[serde(default = "default_cols")]
    cols: u16,
    #[serde(default = "default_rows")]
    rows: u16,
}

fn default_cols() -> u16 {
    DEFAULT_COLS
}

fn default_rows() -> u16 {
    DEFAULT_ROWS
}

impl TermParams {
    /// Params for a shell started in `cwd`, at the default size.
    pub fn new(cwd: String) -> Self {
        Self {
            cwd,
            cols: DEFAULT_COLS,
            rows: DEFAULT_ROWS,
        }
    }

    /// The same params with an initial size.
    #[must_use]
    pub fn with_size(self, cols: u16, rows: u16) -> Self {
        Self { cols, rows, ..self }
    }

    /// Working directory of the shell; empty means the host's default.
    #[cfg_attr(
        not(feature = "runner"),
        allow(dead_code, reason = "read by the runner only")
    )]
    pub(crate) fn cwd(&self) -> &str {
        &self.cwd
    }

    /// Initial `(cols, rows)`; a zero side falls back to the default.
    #[cfg_attr(
        not(feature = "runner"),
        allow(dead_code, reason = "read by the runner only")
    )]
    pub(crate) fn size(&self) -> (u16, u16) {
        let or = |v: u16, d: u16| if v == 0 { d } else { v };
        (or(self.cols, DEFAULT_COLS), or(self.rows, DEFAULT_ROWS))
    }
}

/// Request of `input`: keystrokes or paste.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Input {
    bytes: Vec<u8>,
}

impl Input {
    /// Raw bytes for the shell.
    pub fn new(bytes: Vec<u8>) -> Self {
        Self { bytes }
    }

    #[cfg(feature = "runner")]
    pub(crate) fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

/// Request of `resize`: the pane's new size in cells. Zero is refused with [`TermError::BadSize`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resize {
    cols: u16,
    rows: u16,
}

impl Resize {
    /// `cols` by `rows` cells.
    pub fn new(cols: u16, rows: u16) -> Self {
        Self { cols, rows }
    }

    /// Columns.
    #[cfg(feature = "runner")]
    pub(crate) fn cols(&self) -> u16 {
        self.cols
    }

    /// Rows.
    #[cfg(feature = "runner")]
    pub(crate) fn rows(&self) -> u16 {
        self.rows
    }
}

/// Reducers of the terminal app: the contract between runner and viewer (06-apps.md, Interface).
///
/// Append new methods at the end: the action tag is the declaration order.
#[lattice_app::interface]
pub trait TerminalApi: App<State = TermState, Params = TermParams, Error = TermError> {
    /// Writes keystrokes to the shell. Never stored; the echo comes back as output.
    fn input(&mut self, ctx: &mut Ctx, state: &mut TermState, req: Input) -> Result<(), TermError>;

    /// Resizes the emulator and the PTY. Returns the size now in effect.
    fn resize(
        &mut self,
        ctx: &mut Ctx,
        state: &mut TermState,
        req: Resize,
    ) -> Result<Size, TermError>;
}

/// Errors of the terminal app. Variant names are wire codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[serde(rename_all = "snake_case")]
pub enum TermError {
    /// The shell is gone; input has nowhere to go.
    #[error("the shell has exited")]
    NoShell,
    /// A size of zero.
    #[error("invalid size")]
    BadSize,
}
