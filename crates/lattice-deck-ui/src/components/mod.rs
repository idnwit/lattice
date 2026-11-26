//! Components, named after `docs/design-system/components.md`.
//!
//! Each takes props (plain values, or `impl IntoReactive<T>` where a value
//! changes: a value, a signal, or a closure) and reports events through
//! callbacks. None reads global state, fixtures, or the network.

mod approval;
mod chrome;
mod controls;
mod files;
mod git;
mod glyph;
mod invite;
mod lines;
mod mobile;
mod ports;
mod tools;
mod workers;

pub use approval::{Approval, Verdict, approval_bar, approval_card, settle, verdict_note};

pub use chrome::{link_indicator, status_bar, title_bar, waiting_note};
pub use controls::{ButtonKind, button};
pub(crate) use controls::{icon_button, segmented};
pub use files::{FileRow, file_tree};

pub use git::{Change, Commit, GitTool, Mark, git_tool};

pub use glyph::Status;
pub(crate) use glyph::state_glyph;
pub use invite::{AddWorker, AddWorkerActions, add_worker_dialog};
pub use lines::{
    Density, Line, Run, Tone, cell_grid, cell_probe, prompt_input, styled_lines, tone_color,
};

pub use mobile::{
    AppBar, ExtraKey, TabPill, app_bar, extra_keys, input_bar, pane_switcher, tab_strip,
};

pub use ports::{PortRow, PortsTool, live_ports, ports_tool};

pub use tools::{ToolTab, tool_tabs};
pub use workers::{AgentRow, RailItem, Session, Worker, worker_panel, worker_rail};
