//! The deck's root: the M1 PoC shell (D40, D41; specs/03-sessions.md, M1 PoC deck).
//!
//! Workspace only: tabs, splits, the `+` menu of built-in apps, the empty-pane app picker,
//! the close confirmation, and a status line for the local worker link. Each live pane draws
//! its instance's view tree ([`crate::interp`]). On a phone-width window the workspace shows
//! the focused pane only, with a pager over it (`pane n/m`).

use std::collections::BTreeMap;
use std::rc::Rc;

use lattice_builtin_apps::Builtin;
use lattice_deck_core::PaneId;
use lattice_deck_ui::components::{Status, link_indicator, pane_switcher, status_bar};
use lattice_deck_ui::layout::{Layout, Panels, Region, Regions, Shell, responsive_layout};
use lattice_deck_ui::style::{body, meta, spacer};
use lattice_deck_ui::tokens::{color, row, space};
use lattice_deck_ui::workspace::{
    AppChoice, Axis, Confirm, PaneHeader, SplitNode, TabBar, TabItem, app_picker, bind_pane_keys,
    confirm_card, pane_frame, pane_header, split_view,
};
use nsh_core::element::desktop::{current_window, window as app_window};
use nsh_core::prelude::*;
use tokio::sync::mpsc::UnboundedSender;

use crate::instances::InstanceCmd;
use crate::invite::InviteDialog;
use crate::link::{Cmd, Shared};
use crate::pairing::Pairing;
use crate::status::LinkState;
use crate::workspace::{App, CloseAsk, Content, Pane, Workspace};

/// Desktop canvas size (artboard "Desktop / Web — Advanced workspace").
const DESKTOP_SIZE: (f32, f32) = (1440.0, 900.0);
/// Mobile canvas size (artboards "Mobile — …").
const MOBILE_SIZE: (f32, f32) = (390.0, 844.0);
/// Opens the `+` menu (specs/09-keyboard.md: `⌘T`).
const NEW_TAB_KEY: &str = "mod+t";
/// Code shown on the pane of a `failed` instance (specs/03-sessions.md, Behavior 17).
const INSTANCE_FAILED: &str = "instance.failed";

/// Size of the window when it opens on a desktop.
///
/// `LATTICE_DECK_LAYOUT=mobile` opens a phone-sized window, so the mobile
/// layout can be shown on macOS. On iOS the window fills the screen.
fn opening_size() -> Size<Points> {
    let mobile = std::env::var("LATTICE_DECK_LAYOUT").is_ok_and(|v| v == "mobile");
    let (w, h) = if mobile { MOBILE_SIZE } else { DESKTOP_SIZE };
    size(px(w), px(h))
}

/// The apps this deck ships: every built-in package (D44), in app id order.
fn apps() -> Vec<AppChoice> {
    lattice_builtin_apps::PACKAGES
        .iter()
        .map(|p| AppChoice::new(p.id, p.name))
        .collect()
}

/// Handles the views share. `Copy`.
#[derive(Clone, Copy)]
pub(crate) struct Deck {
    pub(crate) shared: Shared,
    /// The link task's command queue.
    cmds: Stored<UnboundedSender<Cmd>>,
    /// The Add worker dialog and its pairing task.
    pub(crate) pairing: Pairing,
}

impl Deck {
    fn ws(self, f: impl FnOnce(&mut Workspace)) {
        self.shared.ws.update(f);
    }

    /// Sends `cmd` to the link task. False once the task ended.
    fn send(self, cmd: InstanceCmd) -> bool {
        self.cmds.with(|tx| tx.send(Cmd::Instance(cmd)).is_ok())
    }

    fn create(self, pane: PaneId, app: &'static Builtin) {
        if !self.send(InstanceCmd::Create { pane, app }) {
            self.ws(|w| w.failed(pane, "deck.no_worker".to_owned()));
        }
    }

    /// `+` menu: a new tab creating `app_id`.
    fn open(self, app_id: &str) {
        let Some(app) = lattice_builtin_apps::by_id(app_id) else {
            return;
        };
        let mut pane = None;
        self.ws(|w| pane = w.new_tab(app_of(app)));
        if let Some(pane) = pane {
            self.create(pane, app);
        }
    }

    /// App picker: pane `pane` creates `app_id`.
    fn pick(self, pane: PaneId, app_id: &str) {
        let Some(app) = lattice_builtin_apps::by_id(app_id) else {
            return;
        };
        let mut picked = false;
        self.ws(|w| picked = w.pick(pane, app_of(app)));
        if picked {
            self.create(pane, app);
        }
    }

    /// `⌘D` / `⌘⇧D`: a picker pane after the focused one.
    fn split(self, axis: Axis) {
        let mut added = None;
        self.ws(|w| added = w.split(axis));
        if added.is_none() {
            tracing::debug!("nothing to split");
        }
    }

    /// `⌘W`: close the focused pane, asking first when it holds an instance.
    fn close(self) {
        let mut ask = CloseAsk::Nothing;
        self.ws(|w| ask = w.ask_close());
        if ask == CloseAsk::Nothing {
            tracing::debug!("nothing to close");
        }
    }

    fn confirm(self) {
        let mut iid = None;
        self.ws(|w| iid = w.confirm_close());
        if let Some(iid) = iid
            && !self.send(InstanceCmd::Close { iid })
        {
            // No link: nothing will ever answer; the pane goes now.
            self.ws(|w| w.instances(&[]));
        }
    }
}

fn app_of(b: &Builtin) -> App {
    App {
        id: b.id.to_owned(),
        name: b.name.to_owned(),
    }
}

/// Root component: one window; the layout follows its width.
///
/// NSH window model v2: the app host never shows; the app declares its own
/// window. Off the desktops (iOS) the window's content fills the app surface.
#[component]
pub fn DeckApp() -> impl IntoElement {
    lattice_deck_ui::fonts::register();
    let files = crate::data::deck_files();
    let list = crate::data::paired_workers(files.as_ref());
    let shared = Shared {
        ws: signal(Workspace::default()),
        trees: signal(BTreeMap::new()),
        link: signal(LinkState::NoWorker),
    };
    let active = list.latest().cloned();
    let cmds = stored(crate::link::start(shared, files.clone(), active));
    let deck = Deck {
        shared,
        cmds,
        pairing: Pairing {
            dialog: signal(InviteDialog::default()),
            workers: signal(list),
            files: stored(files),
            cmds,
            task: stored(None),
            owner: Owner::current(),
        },
    };
    app_window()
        .id("main")
        .title("Lattice")
        .default_size(opening_size())
        .content(move || screen(deck))
}

fn screen(deck: Deck) -> View {
    // A binary started from a terminal is not activated by macOS: bring the window front.
    let win = current_window();
    effect(move || win.focus());
    bind_pane_keys(move |axis| deck.split(axis), move || deck.close());
    let link = deck.shared.link;
    let shell = Shell::new(move || workspace(deck))
        .rail(move || crate::rail::rail(deck.pairing.workers, link, deck.pairing))
        .status(move || {
            status_bar()
                .child(dynamic(move || {
                    link.with(|l| link_indicator(l.status(), &l.text()))
                }))
                .child(spacer())
                .child(meta("⌘T new tab · ⌘D split · ⌘W close"))
        });
    responsive_layout(
        shell,
        Panels::new(false, false),
        Regions::new(Region::Workspace),
    )
}

fn workspace(deck: Deck) -> View {
    let ws = deck.shared.ws;
    let bar = TabBar::new(move || ws.with(tab_items), move || ws.with(|w| w.active))
        .apps(apps())
        .menu_key(NEW_TAB_KEY)
        .on_select(move |i| deck.ws(|w| w.select(i)))
        .on_pick(move |id| deck.open(&id))
        .on_split(move |axis| deck.split(axis));
    let mobile = computed(|| Layout::for_width(window().width.0) == Layout::Mobile);
    let unpaired = deck.pairing.workers;
    view()
        .flex_1()
        .min_w(px(0.0))
        .min_h(px(0.0))
        .relative()
        .child(bar.render())
        .child(dynamic(move || {
            let mobile = mobile.get();
            ws.with(|w| match w.tab() {
                None if unpaired.with(|l| l.workers().is_empty()) => {
                    crate::rail::no_worker(deck.pairing)
                }
                None => {
                    let height = if mobile { row::ROW_TOUCH } else { row::ROW_MD };
                    app_picker(&apps(), height, move |id| deck.open(&id))
                }
                Some(tab) if mobile => focused_page(deck, w, &tab.root, tab.focused),
                Some(tab) => {
                    let focused = tab.focused;
                    let confirming = w.confirming;
                    split_view(&tab.root, &|p: &Pane| {
                        pane_view(deck, p, p.id == focused, confirming == Some(p.id), false)
                    })
                }
            })
        }))
        .child(crate::rail::dialog(deck.pairing))
}

fn tab_items(w: &Workspace) -> Vec<TabItem> {
    w.tabs
        .iter()
        .map(|t| {
            let leaves = t.root.leaves();
            let status = if leaves
                .iter()
                .any(|p| pane_status(&p.content) == Status::Error)
            {
                Status::Error
            } else {
                Status::Online
            };
            TabItem {
                name: t.name.to_lowercase(),
                status,
                panes: leaves.len(),
            }
        })
        .collect()
}

/// Mobile: the focused pane only; a pager over it when the tab has more than one.
fn focused_page(deck: Deck, w: &Workspace, root: &SplitNode<Pane>, focused: PaneId) -> View {
    let leaves = root.leaves();
    let at = leaves.iter().position(|p| p.id == focused).unwrap_or(0);
    let Some(pane) = leaves.get(at) else {
        return view();
    };
    let pages: Vec<Status> = leaves.iter().map(|p| pane_status(&p.content)).collect();
    view()
        .flex_1()
        .min_h(px(0.0))
        .when(pages.len() > 1, |v| {
            v.child(pane_switcher(
                &pages,
                at,
                &title(&pane.content),
                move || {
                    deck.ws(Workspace::next_pane);
                },
            ))
        })
        .child(pane_view(
            deck,
            pane,
            true,
            w.confirming == Some(pane.id),
            true,
        ))
}

fn pane_status(c: &Content) -> Status {
    match c {
        Content::Failed { .. } | Content::Live { failed: true, .. } => Status::Error,
        Content::Picker | Content::Starting(_) => Status::Idle,
        Content::Live { .. } | Content::Closing { .. } => Status::Online,
    }
}

fn title(c: &Content) -> String {
    match c {
        Content::Picker => "empty".to_owned(),
        Content::Starting(app)
        | Content::Live { app, .. }
        | Content::Closing { app, .. }
        | Content::Failed { app, .. } => app.name.to_lowercase(),
    }
}

/// One pane: frame, header, the close confirmation when asked, and its content.
fn pane_view(deck: Deck, pane: &Pane, focused: bool, confirming: bool, mobile: bool) -> View {
    let id = pane.id;
    let header = header(pane, focused);
    let height = if mobile { row::ROW_TOUCH } else { row::ROW_SM };
    pane_frame(&id.to_string(), focused)
        .on_press(move |_: &PressEvent| deck.ws(|w| w.focus(id)))
        .child(pane_header(&header))
        .when(confirming, |v| {
            v.child(confirm_card(Confirm {
                title: format!("Close pane {id}?"),
                detail: "The instance stops and its history is deleted.".to_owned(),
                action: "Close".to_owned(),
                height,
                on_confirm: Rc::new(move || deck.confirm()),
                on_cancel: Rc::new(move || deck.ws(Workspace::cancel_close)),
            }))
        })
        .child(content(deck, pane, focused, mobile))
}

/// The header of `pane`: state glyph, title, and a note for a transient or failed state.
fn header(pane: &Pane, focused: bool) -> PaneHeader {
    let mut header = PaneHeader::new(
        pane.id.to_string(),
        pane_status(&pane.content),
        title(&pane.content),
    );
    header.focused = focused;
    header.context = "local".to_owned();
    header.note = match &pane.content {
        Content::Starting(_) => Some("starting".to_owned()),
        Content::Closing { .. } => Some("closing".to_owned()),
        Content::Failed { code, .. } => Some(code.clone()),
        Content::Live { failed: true, .. } => Some(INSTANCE_FAILED.to_owned()),
        _ => None,
    };
    header
}

/// The body of a pane that shows a message instead of a view tree, if it is one.
fn notice(c: &Content) -> Option<View> {
    match c {
        Content::Starting(app) => Some(note(&format!("Starting {}…", app.name), None)),
        Content::Failed { app, code } => {
            Some(note(&format!("{} did not start", app.name), Some(code)))
        }
        // Behavior 17: the instance stays until the user closes the pane.
        Content::Live {
            app, failed: true, ..
        } => Some(note(&format!("{} failed", app.name), Some(INSTANCE_FAILED))),
        _ => None,
    }
}

fn content(deck: Deck, pane: &Pane, focused: bool, mobile: bool) -> View {
    let id = pane.id;
    if let Some(v) = notice(&pane.content) {
        return v;
    }
    match &pane.content {
        Content::Picker => {
            let height = if mobile { row::ROW_TOUCH } else { row::ROW_MD };
            app_picker(&apps(), height, move |app| deck.pick(id, &app))
        }
        Content::Starting(_) | Content::Failed { .. } => view(),
        Content::Live { iid, .. } | Content::Closing { iid, .. } => {
            let (iid, trees) = (*iid, deck.shared.trees);
            let tree = move || trees.with(|m| m.get(&iid).cloned().unwrap_or_default());
            let on_event = Rc::new(move |event| {
                deck.send(InstanceCmd::Event { iid, event });
            });
            crate::interp::tree_view(tree, focused, on_event)
        }
    }
}

/// A centered message, with a code under it.
fn note(text: &str, code: Option<&str>) -> View {
    view()
        .flex_1()
        .items_center()
        .justify_center()
        .gap(px(space::SP_2))
        .bg(color::BG_0)
        .child(body(text.to_owned(), color::FG_MUTED))
        .when_some(code, |v, c| v.child(meta(c.to_owned())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use nsh_engine::{App as EngineApp, EngineConfig, Root};
    use nsh_testing::Headless;

    fn terminal() -> App {
        App {
            id: "dev.lattice.terminal".to_owned(),
            name: "Terminal".to_owned(),
        }
    }

    /// A workspace with one terminal pane whose instance 4 is `failed` in `worker/apps`.
    fn failed_ws() -> Workspace {
        let mut w = Workspace::default();
        let pane = w.new_tab(terminal()).unwrap();
        w.created(pane, 4);
        w.instances(&[(4, lattice_app::InstancePhase::Failed)]);
        w
    }

    /// Behavior 17: error glyph and `instance.failed` on the pane and its tab.
    #[test]
    fn a_failed_instance_shows_the_error_glyph_and_code() {
        let w = failed_ws();
        let pane = w.tab().unwrap().root.leaves()[0].clone();
        let h = header(&pane, true);
        assert_eq!(h.status, Status::Error);
        assert_eq!(h.note.as_deref(), Some(INSTANCE_FAILED));
        assert_eq!(tab_items(&w)[0].status, Status::Error);
    }

    /// The failed pane as the deck draws it: header plus body.
    #[component]
    fn FailedPane() -> impl IntoElement {
        app_window().id("main").content(|| {
            let w = failed_ws();
            let pane = w.tab().unwrap().root.leaves()[0].clone();
            pane_frame("1.1", true)
                .child(pane_header(&header(&pane, true)))
                .child(notice(&pane.content).unwrap_or_else(view))
        })
    }

    /// Behavior 16–17, headless: the pane stays and says that its instance failed.
    #[test]
    fn a_failed_instance_pane_renders_its_failed_state() {
        let mut config = EngineConfig::new(EngineApp::Rust(Root::of::<FailedPane>()));
        config.viewport = nsh_core::Size::new(px(1440.0), px(900.0));
        let mut app = Headless::start(config).unwrap();
        app.first_render().unwrap();
        app.wait_for("text:Terminal failed").unwrap();
        app.wait_for("text:instance.failed").unwrap();
    }

    #[test]
    fn tabs_show_their_pane_count_and_an_error_when_a_pane_failed() {
        let mut w = Workspace::default();
        let echo = App {
            id: "dev.lattice.echo".to_owned(),
            name: "Echo".to_owned(),
        };
        let pane = w.new_tab(echo).unwrap();
        w.split(Axis::Row);
        w.failed(pane, "deck.no_worker".to_owned());
        let items = tab_items(&w);
        assert_eq!(items.len(), 1);
        assert_eq!((items[0].name.as_str(), items[0].panes), ("echo", 2));
        assert_eq!(items[0].status, Status::Error);
    }
}
