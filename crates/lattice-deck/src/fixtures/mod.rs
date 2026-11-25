//! Static demo data: the values the design mocks show. No networking.
//!
//! Only the root component reads these. Deck-core state replaces them later.

mod desktop;
mod mobile;
mod panes;
mod term;

pub(crate) use desktop::workspace;
pub(crate) use mobile::mobile_terminal;

use crate::model::{Approval, Pane, Split};

/// A split leaf holding `pane`.
fn leaf(pane: Pane) -> Split {
    Split::Leaf(Box::new(pane))
}

/// The pending approval both mocks show.
fn approval() -> Approval {
    Approval {
        agent: "claude-code".to_owned(),
        origin: "pane 2 · wants to run".to_owned(),
        command: "git push origin feat/relay-fallback".to_owned(),
    }
}
