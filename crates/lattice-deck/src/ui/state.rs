//! Local UI state: signals the views read and their listeners change.
//!
//! All state is local to this device and in memory. No networking.

use nsh_core::prelude::*;

use crate::model::{Change, Commit, MobileTerminal, PortRow, TermLine, Verdict, Workspace};

/// The right panel's tools.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tool {
    /// File tree (`⌘⇧E`).
    Files,
    /// Git (`⌘⇧G`).
    Git,
    /// Port forwards (`⌘⇧P`).
    Ports,
}

impl Tool {
    /// Every tool, in tab order.
    pub(crate) const ALL: [Tool; 3] = [Tool::Files, Tool::Git, Tool::Ports];

    /// Tab label.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Tool::Files => "Files",
            Tool::Git => "Git",
            Tool::Ports => "Ports",
        }
    }

    /// Keyboard chord that opens it.
    pub(crate) fn chord(self) -> &'static str {
        match self {
            Tool::Files => "⌘⇧E",
            Tool::Git => "⌘⇧G",
            Tool::Ports => "⌘⇧P",
        }
    }
}

/// Desktop workspace state.
#[derive(Clone, Copy)]
pub(crate) struct Desk {
    /// Active tab index.
    pub(crate) tab: Signal<usize>,
    /// Active right-panel tool.
    pub(crate) tool: Signal<Tool>,
    /// Selected session row.
    pub(crate) session: Signal<usize>,
    /// The focused pane's lines, per tab.
    pub(crate) shells: Signal<Vec<Vec<TermLine>>>,
    /// Port forwards.
    pub(crate) ports: Signal<Vec<PortRow>>,
    /// Staged changes.
    pub(crate) staged: Signal<Vec<Change>>,
    /// Commit history, newest first.
    pub(crate) history: Signal<Vec<Commit>>,
    /// Commit message draft.
    pub(crate) message: Signal<String>,
    /// Commits ahead of the remote.
    pub(crate) ahead: Signal<u32>,
    /// The approval answer; `None` while it waits.
    pub(crate) verdict: Signal<Option<Verdict>>,
}

impl Desk {
    /// Initial state from the screen's data.
    pub(crate) fn new(ws: &Workspace) -> Self {
        let shells = ws.tabs.iter().map(|t| focused_lines(&t.split)).collect();
        Self {
            tab: signal(0),
            tool: signal(Tool::Git),
            session: signal(0),
            shells: signal(shells),
            ports: signal(ws.ports.clone()),
            staged: signal(ws.git.staged.clone()),
            history: signal(ws.git.history.clone()),
            message: signal(ws.git.message.clone()),
            ahead: signal(ws.git.ahead),
            verdict: signal(None),
        }
    }

    /// Enabled, non-conflicting port forwards.
    pub(crate) fn live_ports(self) -> usize {
        self.ports
            .get()
            .iter()
            .filter(|p| p.on && !p.conflict)
            .count()
    }

    /// Commits the staged changes with the draft message. No-op without
    /// staged changes or a message.
    pub(crate) fn commit(self) {
        let message = self.message.get().trim().to_owned();
        if message.is_empty() || self.staged.get().is_empty() {
            return;
        }
        let n = self.history.get().len();
        let sha = format!("{:07x}", DEMO_SHA_BASE.wrapping_add(n));
        self.history
            .update(|h| h.insert(0, Commit { message, sha }));
        self.staged.set(Vec::new());
        self.message.set(String::new());
        self.ahead.update(|a| *a = a.saturating_add(1));
    }

    /// Flips port `i`; a conflicting port stays off.
    pub(crate) fn toggle_port(self, i: usize) {
        self.ports.update(|ports| {
            if let Some(p) = ports.get_mut(i).filter(|p| !p.conflict) {
                p.on = !p.on;
            }
        });
    }

    /// Turns every forward on or off; conflicts stay off.
    pub(crate) fn set_all_ports(self, on: bool) {
        self.ports
            .update(|ports| ports.iter_mut().for_each(|p| p.on = on && !p.conflict));
    }

    /// Answers the approval once; later answers are ignored.
    pub(crate) fn answer(self, verdict: Verdict) {
        if self.verdict.get_untracked().is_none() {
            self.verdict.set(Some(verdict));
        }
    }
}

/// Seed for demo commit hashes (no real repository).
const DEMO_SHA_BASE: usize = 0x00b1_d5e0;

fn focused_lines(split: &crate::model::Split) -> Vec<TermLine> {
    split
        .panes()
        .into_iter()
        .find(|p| p.focused)
        .map(|p| p.lines.clone())
        .unwrap_or_default()
}

/// Mobile terminal state.
#[derive(Clone, Copy)]
pub(crate) struct Phone {
    /// Active tab index.
    pub(crate) tab: Signal<usize>,
    /// Shown pane page in the active tab.
    pub(crate) page: Signal<usize>,
    /// Lines per tab, per page.
    pub(crate) lines: Signal<Vec<Vec<Vec<TermLine>>>>,
    /// Command input draft.
    pub(crate) draft: Signal<String>,
    /// `ctrl` is latched.
    pub(crate) ctrl: Signal<bool>,
    /// The approval answer; `None` while it waits.
    pub(crate) verdict: Signal<Option<Verdict>>,
}

impl Phone {
    /// Initial state from the screen's data.
    pub(crate) fn new(m: &MobileTerminal) -> Self {
        let lines = m
            .tabs
            .iter()
            .map(|t| t.pages.iter().map(|p| p.lines.clone()).collect())
            .collect();
        Self {
            tab: signal(0),
            page: signal(0),
            lines: signal(lines),
            draft: signal(m.input.clone()),
            ctrl: signal(false),
            verdict: signal(None),
        }
    }

    /// Runs `cmd` in the shown page and clears the draft.
    pub(crate) fn run(self, cmd: &str) {
        let (tab, page) = (self.tab.get_untracked(), self.page.get_untracked());
        self.lines.update(|tabs| {
            if let Some(lines) = tabs.get_mut(tab).and_then(|t| t.get_mut(page)) {
                crate::shell::run(lines, cmd);
            }
        });
        self.draft.set(String::new());
    }

    /// Lines of the shown page.
    pub(crate) fn shown(self) -> Vec<TermLine> {
        let (tab, page) = (self.tab.get(), self.page.get());
        self.lines
            .get()
            .get(tab)
            .and_then(|t| t.get(page))
            .cloned()
            .unwrap_or_default()
    }

    /// Picks tab `i` and its first page.
    pub(crate) fn pick_tab(self, i: usize) {
        self.tab.set(i);
        self.page.set(0);
    }

    /// Shows the next page of `count`, wrapping.
    pub(crate) fn next_page(self, count: usize) {
        self.page
            .update(|p| *p = if count == 0 { 0 } else { (*p + 1) % count });
    }

    /// An extra key: `ctrl` latches; others type a marker into the draft.
    pub(crate) fn key(self, label: &str, modifier: bool) {
        if modifier {
            self.ctrl.update(|c| *c = !*c);
            return;
        }
        let marker = if self.ctrl.get_untracked() {
            format!("^{label}")
        } else {
            format!("‹{label}›")
        };
        self.ctrl.set(false);
        self.draft.update(|d| d.push_str(&marker));
    }
}
