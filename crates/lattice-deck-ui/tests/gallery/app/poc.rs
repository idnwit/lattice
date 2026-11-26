//! Workspace PoC (D40): the wiring the M1 deck needs, with local demo
//! content instead of worker instances.
//!
//! Tabs from the `+` menu, splits that open the app picker, `⌘W` with a
//! confirmation for panes that hold an app. The deck replaces [`content`]
//! with the instance's view and the close with `close{iid}`.

use std::collections::BTreeMap;
use std::rc::Rc;

use lattice_deck_ui::components::{ButtonKind, Status, button, link_indicator, status_bar};
use lattice_deck_ui::layout::{Panels, Region, Regions, Shell, responsive_layout};
use lattice_deck_ui::style::{body, meta, spacer, typed};
use lattice_deck_ui::tokens::{color, row, space, typo};
use lattice_deck_ui::workspace::{
    AppChoice, Axis, Confirm, PaneHeader, SplitNode, TabBar, TabItem, app_picker, bind_pane_keys,
    confirm_card, pane_frame, pane_header, split_view,
};
use nsh_core::element::desktop::window as app_window;
use nsh_core::prelude::*;

/// The demo apps the `+` menu and the picker list.
fn apps() -> Vec<AppChoice> {
    vec![
        AppChoice::new("demo.counter", "Counter").with_key("alt+c"),
        AppChoice::new("demo.note", "Note"),
    ]
}

/// One tab of the PoC.
#[derive(Clone, PartialEq)]
struct PocTab {
    name: String,
    tree: SplitNode<u32>,
}

/// The PoC workspace state. `Copy`: signals of the root.
#[derive(Clone, Copy)]
struct Poc {
    tabs: Signal<Vec<PocTab>>,
    active: Signal<usize>,
    focused: Signal<u32>,
    next: Signal<u32>,
    /// The pane a close waits on.
    confirm: Signal<Option<u32>>,
    /// App per pane; a pane without one shows the picker.
    slots: Signal<BTreeMap<u32, AppChoice>>,
}

impl Poc {
    fn new() -> Self {
        Self {
            tabs: signal(Vec::new()),
            active: signal(0),
            focused: signal(0),
            next: signal(1),
            confirm: signal(None),
            slots: signal(BTreeMap::new()),
        }
    }

    fn new_pane(self) -> u32 {
        let id = self.next.get_untracked();
        self.next.set(id.saturating_add(1));
        id
    }

    /// Puts app `app_id` in pane `id`.
    fn set_slot(self, id: u32, app_id: &str) {
        if let Some(app) = apps().into_iter().find(|a| a.id == app_id) {
            self.slots.update(|s| {
                s.insert(id, app);
            });
        }
    }

    fn open_tab(self, app_id: &str) {
        let id = self.new_pane();
        self.set_slot(id, app_id);
        let name = self
            .slots
            .with_untracked(|s| s.get(&id).map(|a| a.label.to_lowercase()))
            .unwrap_or_default();
        self.tabs.update(|t| {
            t.push(PocTab {
                name,
                tree: SplitNode::Leaf(id),
            });
        });
        self.active
            .set(self.tabs.with_untracked(Vec::len).saturating_sub(1));
        self.focused.set(id);
    }

    fn split(self, axis: Axis) {
        let (at, tab) = (self.focused.get_untracked(), self.active.get_untracked());
        if self.tabs.with_untracked(|t| t.get(tab).is_none()) {
            return;
        }
        let id = self.new_pane();
        self.tabs.update(|t| {
            if let Some(t) = t.get_mut(tab) {
                t.tree.split(&|l| *l == at, axis, id);
            }
        });
        self.focused.set(id);
    }

    /// `⌘W`: an empty pane closes at once; a pane with an app asks first.
    fn ask_close(self) {
        let id = self.focused.get_untracked();
        if self.slots.with_untracked(|s| s.contains_key(&id)) {
            self.confirm.set(Some(id));
        } else {
            self.close(id);
        }
    }

    /// Removes pane `id`; a tab's last pane closes the tab.
    fn close(self, id: u32) {
        let tab = self.active.get_untracked();
        self.confirm.set(None);
        self.slots.update(|s| {
            s.remove(&id);
        });
        self.tabs.update(|tabs| {
            let last = tabs.get(tab).is_some_and(|t| t.tree.leaves().len() == 1);
            if last {
                tabs.remove(tab);
            } else if let Some(t) = tabs.get_mut(tab) {
                t.tree.remove(&|l| *l == id);
            }
        });
        let tab = tab.min(self.tabs.with_untracked(Vec::len).saturating_sub(1));
        self.active.set(tab);
        let first = self.tabs.with_untracked(|t| {
            t.get(tab)
                .and_then(|t| t.tree.leaves().first().map(|l| **l))
        });
        self.focused.set(first.unwrap_or(0));
    }
}

/// Root: the workspace PoC in one window.
#[component]
pub fn PocGallery() -> impl IntoElement {
    lattice_deck_ui::fonts::register();
    app_window()
        .id("main")
        .title("Lattice PoC")
        .default_size(super::opening_size())
        .content(poc_screen)
}

fn poc_screen() -> View {
    super::bring_to_front();
    let poc = Poc::new();
    bind_pane_keys(move |axis| poc.split(axis), move || poc.ask_close());
    let shell = Shell::new(move || workspace(poc)).status(|| {
        status_bar()
            .child(link_indicator(Status::Online, "local · demo"))
            .child(spacer())
            .child(meta(
                "⌘T new tab · ⌘D split right · ⌘⇧D split down · ⌘W close",
            ))
    });
    responsive_layout(
        shell,
        Panels::new(false, false),
        Regions::new(Region::Workspace),
    )
}

fn workspace(poc: Poc) -> View {
    let items = move || {
        poc.tabs
            .get()
            .iter()
            .map(|t| TabItem {
                name: t.name.clone(),
                status: Status::Online,
                panes: t.tree.leaves().len(),
            })
            .collect::<Vec<_>>()
    };
    view()
        .flex_1()
        .min_w(px(0.0))
        .child(
            TabBar::new(items, poc.active)
                .apps(apps())
                .menu_key("mod+t")
                .on_select(move |i| poc.active.set(i))
                .on_pick(move |id| poc.open_tab(&id))
                .on_split(move |axis| poc.split(axis))
                .render(),
        )
        .child(
            view()
                .flex_1()
                .min_h(px(0.0))
                .bg(color::LINE)
                .child(dynamic(move || {
                    match poc.tabs.get().get(poc.active.get()).cloned() {
                        Some(t) => split_view(&t.tree, &|id: &u32| pane(poc, *id)),
                        None => app_picker(&apps(), row::ROW_MD, move |id| poc.open_tab(&id)),
                    }
                })),
        )
}

fn pane(poc: Poc, id: u32) -> View {
    pane_frame(&id.to_string(), move || poc.focused.get() == id)
        .on_press(move |_: &PressEvent| poc.focused.set(id))
        .child(dynamic(move || {
            let app = poc.slots.with(|s| s.get(&id).cloned());
            let mut header = PaneHeader::new(
                format!("1.{id}"),
                Status::Online,
                app.as_ref().map_or("empty", |a| a.label.as_str()),
            );
            header.focused = poc.focused.get() == id;
            let body = match app {
                Some(app) => content(&app),
                None => app_picker(&apps(), row::ROW_MD, move |app| poc.set_slot(id, &app)),
            };
            view()
                .flex_1()
                .child(pane_header(&header))
                .child(dynamic(move || {
                    (poc.confirm.get() == Some(id)).then(|| close_confirm(poc, id))
                }))
                .child(body)
        }))
}

fn close_confirm(poc: Poc, id: u32) -> View {
    confirm_card(Confirm {
        title: format!("Close pane 1.{id}?"),
        detail: "The instance stops and its history is deleted.".to_owned(),
        action: "Close".to_owned(),
        height: row::ROW_SM,
        on_confirm: Rc::new(move || poc.close(id)),
        on_cancel: Rc::new(move || poc.confirm.set(None)),
    })
}

/// Demo content of an app pane. The deck shows the instance's view here.
fn content(app: &AppChoice) -> View {
    let count = signal(0u32);
    let base = view()
        .flex_1()
        .items_center()
        .justify_center()
        .gap(px(space::SP_3))
        .bg(color::BG_0);
    if app.id == "demo.counter" {
        base.child(
            typed(text(move || format!("count {}", count.get())), typo::BODY).text_color(color::FG),
        )
        .child(
            button(ButtonKind::Secondary, "Add", None, row::ROW_SM)
                .on_press(move |_: &PressEvent| count.update(|c| *c = c.saturating_add(1))),
        )
    } else {
        base.child(body(format!("{} pane", app.label), color::FG_MUTED))
    }
}
