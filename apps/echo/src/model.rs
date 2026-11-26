//! `EchoState`: what decks hold and render (06-apps.md, State).

use lattice_state::{List, Synced, Value};
use serde::{Deserialize, Serialize};

/// One line sent through `send`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Line {
    seq: u64,
    text: String,
}

impl Line {
    /// A line with its sequence number.
    #[cfg(feature = "runner")]
    pub(crate) fn new(seq: u64, text: String) -> Self {
        Self { seq, text }
    }

    /// Sequence number, as `send` returned it. Grows by one per accepted `send`, from 1.
    pub fn seq(&self) -> u64 {
        self.seq
    }

    /// The text.
    pub fn text(&self) -> &str {
        &self.text
    }
}

/// The whole echo state. The unit of sync (D31).
#[derive(Debug, Default, PartialEq, Serialize, Deserialize, Synced)]
pub struct EchoState {
    pub(crate) lines: List<Value<Line>>,
}

impl EchoState {
    /// Lines, oldest first.
    pub fn lines(&self) -> &List<Value<Line>> {
        &self.lines
    }
}
