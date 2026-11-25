//! View data: what the screens show. Plain values, no behavior.
//!
//! Views take these as props. Today `fixtures` builds them; later deck-core
//! state will.

use lattice_deck_core::Status;

/// A color role for a run of terminal text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tone {
    /// Terminal body text (`fg/term`).
    Term,
    /// Primary text (`fg`).
    Bright,
    /// Secondary text (`fg/muted`).
    Muted,
    /// Meta text (`fg/dim`).
    Dim,
    /// Prompt and cursor (`accent`).
    Accent,
    /// Needs-input mark (`waiting`).
    Waiting,
    /// One of the 16 ANSI colors.
    Ansi(u8),
}

/// A styled run of terminal text.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Span {
    /// The text.
    pub(crate) text: String,
    /// Its color role.
    pub(crate) tone: Tone,
}

/// One terminal line: styled runs, left to right.
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct TermLine {
    /// The runs.
    pub(crate) spans: Vec<Span>,
    /// The line ends with the block cursor.
    pub(crate) cursor: bool,
}

/// Terminal text density.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Density {
    /// `mono/term` (13/20): the focused shell.
    Regular,
    /// 12/18: secondary panes and mobile.
    Compact,
}

/// An agent nested under a session row.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AgentRow {
    /// Agent name (`claude-code`).
    pub(crate) name: String,
    /// Meta text (`approve`, `15/40`, `done 2m`).
    pub(crate) meta: String,
    /// Agent state.
    pub(crate) status: Status,
}

/// A session row in the worker panel.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Session {
    /// Session name.
    pub(crate) name: String,
    /// Enabled port forwards, if any.
    pub(crate) ports: Option<u32>,
    /// Tab count.
    pub(crate) tabs: u32,
    /// Busiest child state.
    pub(crate) status: Status,
    /// Agents running in it.
    pub(crate) agents: Vec<AgentRow>,
}

/// A worker as the worker card shows it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Worker {
    /// Worker name (`build-01`).
    pub(crate) name: String,
    /// OS and arch line.
    pub(crate) os: String,
    /// Link and latency (`direct · 18ms`).
    pub(crate) link: String,
    /// Link state.
    pub(crate) status: Status,
    /// CPU load, percent.
    pub(crate) cpu_pct: u8,
    /// Memory label (`11/32G`).
    pub(crate) mem: String,
    /// Memory use, percent.
    pub(crate) mem_pct: u8,
    /// Its sessions.
    pub(crate) sessions: Vec<Session>,
}

/// An entry in the worker rail.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RailItem {
    /// Two-letter monogram, or `∗` for all workers.
    pub(crate) mono: String,
    /// Link state; `None` for the all-workers entry.
    pub(crate) status: Option<Status>,
    /// The entry is selected.
    pub(crate) selected: bool,
}

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

impl Tab {
    /// Number of panes in the tab.
    pub(crate) fn panes(&self) -> usize {
        self.split.panes().len()
    }
}

/// A pane: a header and a terminal.
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
    /// Terminal content.
    pub(crate) lines: Vec<TermLine>,
}

/// A split tree.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Split {
    /// One pane.
    Leaf(Box<Pane>),
    /// Children side by side, with flex weights.
    Row(Vec<(f32, Split)>),
    /// Children stacked, with flex weights.
    Column(Vec<(f32, Split)>),
}

impl Split {
    /// The panes, in reading order.
    pub(crate) fn panes(&self) -> Vec<&Pane> {
        match self {
            Self::Leaf(p) => vec![p],
            Self::Row(c) | Self::Column(c) => c.iter().flat_map(|(_, s)| s.panes()).collect(),
        }
    }
}

/// Git mark of a changed path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mark {
    /// `M`.
    Modified,
    /// `A`.
    Added,
    /// `D` (path struck through).
    Deleted,
}

/// A changed file in the Git tool.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Change {
    /// Path.
    pub(crate) path: String,
    /// Line delta (`+42 −9`).
    pub(crate) delta: String,
    /// Git mark.
    pub(crate) mark: Mark,
}

/// A commit in the Git history; the first one is HEAD.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Commit {
    /// Subject line.
    pub(crate) message: String,
    /// Short hash.
    pub(crate) sha: String,
}

/// The Git tool.
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

/// A port forward in the Ports tool.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PortRow {
    /// Port on the worker.
    pub(crate) remote: u16,
    /// Port on this device.
    pub(crate) local: u16,
    /// What listens (`vite dev`).
    pub(crate) label: String,
    /// Meta (`auto-detected`).
    pub(crate) meta: String,
    /// The forward is enabled.
    pub(crate) on: bool,
    /// The local port is taken: stays off.
    pub(crate) conflict: bool,
}

/// A row of the Files tool tree.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FileRow {
    /// File or directory name.
    pub(crate) name: String,
    /// Nesting depth.
    pub(crate) depth: u8,
    /// `Some(open)` for a directory.
    pub(crate) dir: Option<bool>,
    /// Git mark, if changed (`●` for a dirty directory).
    pub(crate) mark: Option<Mark>,
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

/// An agent approval request.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Approval {
    /// Agent name.
    pub(crate) agent: String,
    /// Where it comes from (`pane 2 · wants to run`).
    pub(crate) origin: String,
    /// The command.
    pub(crate) command: String,
}

/// How the user answered an approval.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Verdict {
    /// Allow once (`Y`).
    Allow,
    /// Always allow this command prefix (`A`).
    Always,
    /// Deny (`N`).
    Deny,
}

impl Verdict {
    /// Past-tense word shown after the answer.
    pub(crate) fn done(self) -> &'static str {
        match self {
            Self::Allow => "allowed",
            Self::Always => "always allowed",
            Self::Deny => "denied",
        }
    }
}

/// A key in the mobile extra-keys row.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ExtraKey {
    /// Key label.
    pub(crate) label: String,
    /// Accessible name.
    pub(crate) name: String,
    /// A modifier that latches (`ctrl`).
    pub(crate) modifier: bool,
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
    /// Terminal content.
    pub(crate) lines: Vec<TermLine>,
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
