//! The PoC workspace as data: tabs, split trees, panes, focus (D40; specs/03-sessions.md,
//! Behavior 2, 7, 11–14). No NSH, no I/O: the root draws it and the link acts on what the
//! operations return.
//!
//! Rules:
//! - A pane holds nothing (the app picker), an instance being created, a live instance, an
//!   instance closing, or a failed create.
//! - `+` opens a tab whose one pane starts creating at once; a split opens a picker pane
//!   after the focused one and focuses it. Nothing is created until a pick.
//! - Closing a picker or a failed pane needs no confirmation; a live pane asks first, then
//!   shows `closing` until the worker drops the instance.
//! - A live pane goes away when its instance leaves `worker/apps` (shell exit, close done).
//!   Only an instance seen in the list once can leave it.
//! - A live pane whose instance is `failed` stays, marked failed, until the user closes it
//!   (specs/03-sessions.md, Behavior 16–18).
//! - A tab whose last pane goes away goes too.
//!
//! TBD: move to `lattice-deck-core` with layouts as data (07-layouts.md).

use lattice_app::InstancePhase;
use lattice_deck_core::PaneId;
use lattice_deck_ui::workspace::{Axis, SplitNode};

/// An app the deck can open: id and display name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct App {
    pub(crate) id: String,
    pub(crate) name: String,
}

/// What a pane shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Content {
    /// The app picker.
    Picker,
    /// `create` sent; waiting for the instance id.
    Starting(App),
    /// A live instance. `seen`: it was in a `worker/apps` list. `failed`: its phase there
    /// is `failed`.
    Live {
        app: App,
        iid: u64,
        seen: bool,
        failed: bool,
    },
    /// `close` sent; waiting for the instance to leave `worker/apps`.
    Closing { app: App, iid: u64 },
    /// `create` failed with this code.
    Failed { app: App, code: String },
}

/// One pane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Pane {
    pub(crate) id: PaneId,
    pub(crate) content: Content,
}

/// One tab: a split tree of panes.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Tab {
    pub(crate) name: String,
    pub(crate) root: SplitNode<Pane>,
    pub(crate) focused: PaneId,
    number: u16,
    next_pane: u16,
}

/// What a close request led to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CloseAsk {
    /// The pane went away (picker or failed).
    Closed,
    /// The pane waits for confirmation ([`Workspace::confirming`]).
    Confirm,
    /// Nothing to close now (no pane, or a create in flight).
    Nothing,
}

/// Every tab, the active one, and the pane waiting for a close confirmation.
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct Workspace {
    pub(crate) tabs: Vec<Tab>,
    pub(crate) active: usize,
    pub(crate) confirming: Option<PaneId>,
    next_tab: u16,
}

impl Workspace {
    /// The active tab.
    pub(crate) fn tab(&self) -> Option<&Tab> {
        self.tabs.get(self.active)
    }

    /// The pane `id`.
    pub(crate) fn pane(&self, id: PaneId) -> Option<&Pane> {
        self.tabs
            .iter()
            .flat_map(|t| t.root.leaves())
            .find(|p| p.id == id)
    }

    fn pane_mut(&mut self, id: PaneId) -> Option<&mut Pane> {
        self.tabs.iter_mut().find_map(|t| leaf_mut(&mut t.root, id))
    }

    /// A new tab whose one pane creates `app`. Returns the pane.
    pub(crate) fn new_tab(&mut self, app: App) -> Option<PaneId> {
        let number = self.next_tab.checked_add(1)?;
        self.next_tab = number;
        let id = PaneId::try_new(number, 1).ok()?;
        self.tabs.push(Tab {
            name: app.name.clone(),
            root: SplitNode::Leaf(Pane {
                id,
                content: Content::Starting(app),
            }),
            focused: id,
            number,
            next_pane: 1,
        });
        self.active = self.tabs.len().saturating_sub(1);
        Some(id)
    }

    /// Splits the focused pane of the active tab along `axis`: a picker pane after it, focused.
    pub(crate) fn split(&mut self, axis: Axis) -> Option<PaneId> {
        let tab = self.tabs.get_mut(self.active)?;
        let n = tab.next_pane.checked_add(1)?;
        let id = PaneId::try_new(tab.number, n).ok()?;
        let at = tab.focused;
        let new = Pane {
            id,
            content: Content::Picker,
        };
        if !tab.root.split(&|p: &Pane| p.id == at, axis, new) {
            return None;
        }
        tab.next_pane = n;
        tab.focused = id;
        Some(id)
    }

    /// The picker pane `id` creates `app`. False when it is not a picker.
    pub(crate) fn pick(&mut self, id: PaneId, app: App) -> bool {
        match self.pane_mut(id) {
            Some(p) if p.content == Content::Picker => {
                p.content = Content::Starting(app);
                true
            }
            _ => false,
        }
    }

    /// The create of pane `id` gave `iid`. False when the pane is gone: close the instance.
    pub(crate) fn created(&mut self, id: PaneId, iid: u64) -> bool {
        let Some(p) = self.pane_mut(id) else {
            return false;
        };
        let Content::Starting(app) = &p.content else {
            return false;
        };
        let app = app.clone();
        p.content = Content::Live {
            app,
            iid,
            seen: false,
            failed: false,
        };
        true
    }

    /// The create of pane `id` failed with `code`.
    pub(crate) fn failed(&mut self, id: PaneId, code: String) {
        if let Some(p) = self.pane_mut(id)
            && let Content::Starting(app) = &p.content
        {
            let app = app.clone();
            p.content = Content::Failed { app, code };
        }
    }

    /// Focuses pane `id` (and its tab).
    pub(crate) fn focus(&mut self, id: PaneId) {
        if let Some(i) = self.tabs.iter().position(|t| has(&t.root, id))
            && let Some(tab) = self.tabs.get_mut(i)
        {
            tab.focused = id;
            self.active = i;
        }
    }

    /// Shows tab `i`.
    pub(crate) fn select(&mut self, i: usize) {
        if i < self.tabs.len() {
            self.active = i;
        }
    }

    /// Focuses the pane after the focused one in the active tab, wrapping (mobile pager).
    pub(crate) fn next_pane(&mut self) {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        let ids: Vec<PaneId> = tab.root.leaves().iter().map(|p| p.id).collect();
        let at = ids.iter().position(|id| *id == tab.focused).unwrap_or(0);
        if let Some(next) = ids.get(at.saturating_add(1)).or(ids.first()) {
            tab.focused = *next;
        }
    }

    /// `⌘W` on the focused pane of the active tab.
    pub(crate) fn ask_close(&mut self) -> CloseAsk {
        let Some(id) = self.tab().map(|t| t.focused) else {
            return CloseAsk::Nothing;
        };
        match self.pane(id).map(|p| &p.content) {
            Some(Content::Picker | Content::Failed { .. }) => {
                self.remove(id);
                CloseAsk::Closed
            }
            Some(Content::Live { .. }) => {
                self.confirming = Some(id);
                CloseAsk::Confirm
            }
            _ => CloseAsk::Nothing,
        }
    }

    /// Confirms the pending close: the pane shows `closing`. Returns the instance to close.
    pub(crate) fn confirm_close(&mut self) -> Option<u64> {
        let id = self.confirming.take()?;
        let p = self.pane_mut(id)?;
        let Content::Live { app, iid, .. } = &p.content else {
            return None;
        };
        let (app, iid) = (app.clone(), *iid);
        p.content = Content::Closing { app, iid };
        Some(iid)
    }

    /// Drops the pending close.
    pub(crate) fn cancel_close(&mut self) {
        self.confirming = None;
    }

    /// The instances now in `worker/apps`, with their phase: live panes see theirs and its
    /// phase; a pane whose instance was seen (or is closing) and left the list goes away.
    /// A `failed` instance stays listed, so its pane stays (Behavior 16).
    pub(crate) fn instances(&mut self, listed: &[(u64, InstancePhase)]) {
        let phase_of = |iid: u64| listed.iter().find(|(i, _)| *i == iid).map(|(_, p)| *p);
        let mut gone = Vec::new();
        for tab in &mut self.tabs {
            for_each_leaf(&mut tab.root, &mut |p| match &mut p.content {
                Content::Live {
                    iid, seen, failed, ..
                } if phase_of(*iid).is_some() => {
                    *seen = true;
                    *failed = phase_of(*iid) == Some(InstancePhase::Failed);
                }
                Content::Live { seen: true, .. } => gone.push(p.id),
                Content::Closing { iid, .. } if phase_of(*iid).is_none() => gone.push(p.id),
                _ => {}
            });
        }
        for id in gone {
            self.remove(id);
        }
    }

    /// Removes pane `id`; its tab goes with its last pane.
    pub(crate) fn remove(&mut self, id: PaneId) {
        if self.confirming == Some(id) {
            self.confirming = None;
        }
        let Some(i) = self.tabs.iter().position(|t| has(&t.root, id)) else {
            return;
        };
        let Some(tab) = self.tabs.get_mut(i) else {
            return;
        };
        if tab.root.remove(&|p: &Pane| p.id == id).is_some() {
            if tab.focused == id
                && let Some(first) = tab.root.leaves().first()
            {
                tab.focused = first.id;
            }
            return;
        }
        // The only pane: the tab goes.
        self.tabs.remove(i);
        if self.active >= self.tabs.len() {
            self.active = self.tabs.len().saturating_sub(1);
        }
    }
}

fn has(node: &SplitNode<Pane>, id: PaneId) -> bool {
    node.leaves().iter().any(|p| p.id == id)
}

fn leaf_mut(node: &mut SplitNode<Pane>, id: PaneId) -> Option<&mut Pane> {
    match node {
        SplitNode::Leaf(p) => (p.id == id).then_some(p),
        SplitNode::Split { children, .. } => children.iter_mut().find_map(|(_, c)| leaf_mut(c, id)),
    }
}

fn for_each_leaf(node: &mut SplitNode<Pane>, f: &mut dyn FnMut(&mut Pane)) {
    match node {
        SplitNode::Leaf(p) => f(p),
        SplitNode::Split { children, .. } => {
            for (_, c) in children {
                for_each_leaf(c, f);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(name: &str) -> App {
        App {
            id: format!("dev.lattice.{}", name.to_lowercase()),
            name: name.to_owned(),
        }
    }

    fn id(s: &str) -> PaneId {
        s.parse().unwrap()
    }

    fn content(ws: &Workspace, pane: &str) -> Option<Content> {
        ws.pane(id(pane)).map(|p| p.content.clone())
    }

    #[test]
    fn plus_opens_a_tab_that_creates_and_a_split_opens_a_focused_picker() {
        let mut ws = Workspace::default();
        assert_eq!(ws.new_tab(app("Terminal")), Some(id("1.1")));
        assert_eq!(
            content(&ws, "1.1"),
            Some(Content::Starting(app("Terminal")))
        );
        assert_eq!(ws.split(Axis::Row), Some(id("1.2")));
        assert_eq!(content(&ws, "1.2"), Some(Content::Picker));
        assert_eq!(ws.tab().map(|t| t.focused), Some(id("1.2")));

        assert!(ws.pick(id("1.2"), app("Echo")));
        assert!(!ws.pick(id("1.2"), app("Echo")), "only a picker picks");
        assert!(ws.created(id("1.2"), 7));
        let live = Content::Live {
            app: app("Echo"),
            iid: 7,
            seen: false,
            failed: false,
        };
        assert_eq!(content(&ws, "1.2"), Some(live));

        assert_eq!(ws.new_tab(app("Echo")), Some(id("2.1")));
        assert_eq!(ws.active, 1);
        ws.focus(id("1.1"));
        assert_eq!(ws.active, 0);
    }

    #[test]
    fn closing_asks_only_for_a_live_pane_and_waits_for_the_worker() {
        let mut ws = Workspace::default();
        ws.new_tab(app("Terminal"));
        ws.created(id("1.1"), 3);
        ws.split(Axis::Column);
        assert_eq!(
            ws.ask_close(),
            CloseAsk::Closed,
            "an empty pane closes at once"
        );
        assert_eq!(ws.tab().map(|t| t.root.leaves().len()), Some(1));

        assert_eq!(ws.ask_close(), CloseAsk::Confirm);
        assert_eq!(ws.confirming, Some(id("1.1")));
        ws.cancel_close();
        assert_eq!(ws.ask_close(), CloseAsk::Confirm);
        assert_eq!(ws.confirm_close(), Some(3));
        assert!(matches!(
            content(&ws, "1.1"),
            Some(Content::Closing { iid: 3, .. })
        ));

        ws.instances(&[(3, InstancePhase::Running)]);
        assert_eq!(ws.tabs.len(), 1, "still closing");
        ws.instances(&[]);
        assert!(ws.tabs.is_empty(), "the last pane took its tab");
    }

    #[test]
    fn a_live_pane_goes_when_its_instance_leaves_the_list_after_being_seen() {
        let mut ws = Workspace::default();
        ws.new_tab(app("Terminal"));
        ws.created(id("1.1"), 5);
        ws.instances(&[]);
        assert_eq!(
            ws.tabs.len(),
            1,
            "a list from before the create does not close it"
        );
        ws.instances(&[(5, InstancePhase::Running)]);
        ws.instances(&[]);
        assert!(ws.tabs.is_empty(), "the shell exited");
    }

    /// Behavior 16–18: a failed instance keeps its pane; only the user closes it.
    #[test]
    fn a_failed_instance_keeps_its_pane_until_the_user_closes_it() {
        let mut ws = Workspace::default();
        ws.new_tab(app("Terminal"));
        ws.created(id("1.1"), 4);
        ws.instances(&[(4, InstancePhase::Running)]);
        for _ in 0..2 {
            ws.instances(&[(4, InstancePhase::Failed)]);
            assert!(
                matches!(
                    content(&ws, "1.1"),
                    Some(Content::Live { failed: true, .. })
                ),
                "still shown, as failed"
            );
        }
        assert_eq!(ws.ask_close(), CloseAsk::Confirm, "the user closes it");
        assert_eq!(ws.confirm_close(), Some(4));
        ws.instances(&[(4, InstancePhase::Closing)]);
        assert_eq!(ws.tabs.len(), 1, "still closing");
        ws.instances(&[]);
        assert!(ws.tabs.is_empty());
    }

    #[test]
    fn a_create_for_a_pane_that_went_away_is_refused() {
        let mut ws = Workspace::default();
        ws.new_tab(app("Echo"));
        ws.split(Axis::Row);
        ws.pick(id("1.2"), app("Echo"));
        ws.remove(id("1.2"));
        assert!(!ws.created(id("1.2"), 9), "the link closes the orphan");
        ws.failed(id("1.1"), "worker.package_unavailable".to_owned());
        assert!(matches!(content(&ws, "1.1"), Some(Content::Failed { .. })));
        assert_eq!(ws.ask_close(), CloseAsk::Closed);
        assert!(ws.tabs.is_empty());
    }

    #[test]
    fn next_pane_wraps_and_removing_the_focused_pane_refocuses() {
        let mut ws = Workspace::default();
        ws.new_tab(app("Echo"));
        ws.split(Axis::Row);
        ws.split(Axis::Row);
        let focused = |ws: &Workspace| ws.tab().map(|t| t.focused.to_string());
        assert_eq!(focused(&ws).as_deref(), Some("1.3"));
        ws.next_pane();
        assert_eq!(focused(&ws).as_deref(), Some("1.1"));
        ws.next_pane();
        assert_eq!(focused(&ws).as_deref(), Some("1.2"));
        ws.remove(id("1.3"));
        assert_eq!(focused(&ws).as_deref(), Some("1.2"), "another pane went");
        ws.remove(id("1.2"));
        assert_eq!(focused(&ws).as_deref(), Some("1.1"));
    }
}
