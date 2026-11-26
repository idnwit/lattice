//! Static demo data: the values the design mocks show. No networking.
//!
//! Only the root component reads these. The deck feeds real state instead.

mod desktop;
mod mobile;
mod panes;
mod term;

pub(crate) use desktop::workspace;
pub(crate) use mobile::mobile_terminal;

use crate::app::model::{Approval, Pane, Split};
use lattice_deck_ui::workspace::Axis;

/// A split leaf holding `pane`.
fn leaf(pane: Pane) -> Split {
    Split::Leaf(pane)
}

/// Children side by side, with flex weights.
fn row(children: Vec<(f32, Split)>) -> Split {
    Split::Split {
        axis: Axis::Row,
        children,
    }
}

/// Children stacked, with flex weights.
fn column(children: Vec<(f32, Split)>) -> Split {
    Split::Split {
        axis: Axis::Column,
        children,
    }
}

/// The pending approval both mocks show.
fn approval() -> Approval {
    Approval {
        agent: "claude-code".to_owned(),
        origin: "pane 2 · wants to run".to_owned(),
        command: "git push origin feat/relay-fallback".to_owned(),
    }
}
