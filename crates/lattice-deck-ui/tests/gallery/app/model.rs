//! Screen data of the gallery mocks: what the fixtures build and the
//! screens take as props. Leaf types come from the library.

pub(crate) use lattice_deck_ui::components::{
    AgentRow, Approval, Change, Commit, Density, ExtraKey, FileRow, Line, Mark, PortRow, RailItem,
    Run, Session, Status, Tone, Worker,
};
use lattice_deck_ui::workspace::SplitNode;

/// A tab's split tree of panes.
pub(crate) type Split = SplitNode<Pane>;

/// A terminal tab and its split tree.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Tab {
    /// Tab name.
    pub(crate) name: String,
    /// Busiest pane state.
    pub(crate) status: Status,
    /// The tab's panes: layouts are data.
    pub(crate) split: Split,
}

/// A pane: a header and its lines.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Pane {
    /// Pane id (`1.1`).
    pub(crate) id: String,
    /// Worker it runs on.
    pub(crate) worker: String,
    /// Pane state glyph.
    pub(crate) status: Status,
    /// Context after the worker (`~/src/api · zsh`).
    pub(crate) context: String,
    /// Agent name, when an agent runs here.
    pub(crate) agent: Option<String>,
    /// The pane shows the pending approval.
    pub(crate) asks: bool,
    /// The pane has focus and takes typed commands.
    pub(crate) focused: bool,
    /// Text density.
    pub(crate) density: Density,
    /// Pane content.
    pub(crate) lines: Vec<Line>,
}

/// The Git tool's data.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Git {
    /// Branch name.
    pub(crate) branch: String,
    /// Commits ahead of the remote.
    pub(crate) ahead: u32,
    /// Draft commit message.
    pub(crate) message: String,
    /// Staged changes.
    pub(crate) staged: Vec<Change>,
    /// Unstaged changes.
    pub(crate) changes: Vec<Change>,
    /// Recent commits, newest first.
    pub(crate) history: Vec<Commit>,
}

/// The status bar's fixed parts.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct StatusBar {
    /// Focused pane's worker.
    pub(crate) worker: String,
    /// Link state.
    pub(crate) status: Status,
    /// Link and latency (`direct · 18ms`).
    pub(crate) link: String,
    /// Right side (`utf-8 · 132×41 · zsh`).
    pub(crate) info: String,
}

/// The desktop workspace screen.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Workspace {
    /// Account name in the title bar.
    pub(crate) account: String,
    /// Sync label in the title bar.
    pub(crate) sync: String,
    /// Worker rail.
    pub(crate) rail: Vec<RailItem>,
    /// The picked worker.
    pub(crate) worker: Worker,
    /// Terminal tabs.
    pub(crate) tabs: Vec<Tab>,
    /// The Files tool tree.
    pub(crate) files: Vec<FileRow>,
    /// The Git tool.
    pub(crate) git: Git,
    /// The Ports tool.
    pub(crate) ports: Vec<PortRow>,
    /// The pending agent approval.
    pub(crate) approval: Approval,
    /// The status bar.
    pub(crate) status: StatusBar,
}

/// A mobile tab: its panes as swipeable pages.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MobileTab {
    /// Tab name.
    pub(crate) name: String,
    /// Busiest pane state.
    pub(crate) status: Status,
    /// The panes, one page each.
    pub(crate) pages: Vec<Page>,
}

/// One pane page on mobile.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Page {
    /// Process in the pane (`zsh`).
    pub(crate) process: String,
    /// Pane state.
    pub(crate) status: Status,
    /// Pane content.
    pub(crate) lines: Vec<Line>,
}

/// The mobile terminal screen.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MobileTerminal {
    /// Worker name.
    pub(crate) worker: String,
    /// Link state.
    pub(crate) status: Status,
    /// Link and latency.
    pub(crate) link: String,
    /// Enabled port forwards.
    pub(crate) ports: u32,
    /// Tabs.
    pub(crate) tabs: Vec<MobileTab>,
    /// Pending approval.
    pub(crate) approval: Approval,
    /// Extra keys.
    pub(crate) keys: Vec<ExtraKey>,
    /// Command input draft.
    pub(crate) input: String,
}
