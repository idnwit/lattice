//! Tab bar (components.md › Tabs): tabs, the `+` new-tab menu, split buttons.

use std::rc::Rc;

use nsh_core::prelude::*;

use super::apps::{AppChoice, app_menu};
use super::split::Axis;
use crate::components::{Status, state_glyph};
use crate::style::{FOCUS_BAR, HAIRLINE, badge, body, hrow, mono, selected_state, spacer};
use crate::tokens::{color, row, space};

/// A tab as the bar shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct TabItem {
    /// Tab name.
    pub name: String,
    /// Busiest pane state.
    pub status: Status,
    /// Pane count (`▦n`).
    pub panes: usize,
}

/// One tab: glyph, name, pane count. Active: `bg/3` + accent top bar.
pub(crate) fn tab_button(tab: &TabItem, active: bool) -> View {
    let fg = if active { color::FG } else { color::FG_MUTED };
    hrow(space::SP_2)
        .h_full()
        .px(px(space::SP_3))
        .border_r(px(HAIRLINE))
        .border_color(color::LINE)
        .role(AccessibilityRole::Tab)
        .label(tab.name.clone())
        .accessibility_state(selected_state(active))
        .when(active, |t| {
            t.bg(color::BG_3)
                .border_t(px(FOCUS_BAR))
                .border_t_color(color::ACCENT)
        })
        .child(state_glyph(tab.status))
        .child(mono(tab.name.clone(), fg))
        .child(badge(format!("▦{}", tab.panes), color::FG_DIM))
}

/// The tab bar. Build with [`TabBar::new`], add handlers, then [`TabBar::render`].
pub struct TabBar {
    tabs: Reactive<Vec<TabItem>>,
    active: Reactive<usize>,
    apps: Vec<AppChoice>,
    menu_key: Option<String>,
    on_select: Rc<dyn Fn(usize)>,
    on_pick: Rc<dyn Fn(String)>,
    on_split: Option<Rc<dyn Fn(Axis)>>,
}

impl TabBar {
    /// A bar of `tabs` with `active` selected; no apps, handlers do nothing.
    pub fn new(tabs: impl IntoReactive<Vec<TabItem>>, active: impl IntoReactive<usize>) -> Self {
        Self {
            tabs: tabs.into_reactive(),
            active: active.into_reactive(),
            apps: Vec::new(),
            menu_key: None,
            on_select: Rc::new(|_| {}),
            on_pick: Rc::new(|_| {}),
            on_split: None,
        }
    }

    /// The apps the `+` menu lists. Each app's `key` picks it from anywhere in the window.
    pub fn apps(mut self, apps: Vec<AppChoice>) -> Self {
        self.apps = apps;
        self
    }

    /// A keystroke that opens the `+` menu (`mod+t`).
    pub fn menu_key(mut self, key: impl Into<String>) -> Self {
        self.menu_key = Some(key.into());
        self
    }

    /// Hears a tab press, with its index.
    pub fn on_select(mut self, f: impl Fn(usize) + 'static) -> Self {
        self.on_select = Rc::new(f);
        self
    }

    /// Hears an app pick from the `+` menu (or its key), with the app id.
    pub fn on_pick(mut self, f: impl Fn(String) + 'static) -> Self {
        self.on_pick = Rc::new(f);
        self
    }

    /// Shows the split buttons; `f` hears one, with its axis.
    /// Bind their chords with [`crate::workspace::bind_pane_keys`].
    pub fn on_split(mut self, f: impl Fn(Axis) + 'static) -> Self {
        self.on_split = Some(Rc::new(f));
        self
    }

    /// The bar.
    pub fn render(self) -> View {
        let Self {
            tabs,
            active,
            apps,
            menu_key,
            on_select,
            on_pick,
            on_split,
        } = self;
        for app in &apps {
            if let Some(key) = &app.key {
                let (id, on_pick) = (app.id.clone(), on_pick.clone());
                on_key(key, move |_| on_pick(id.clone()));
            }
        }
        view()
            .flex_none()
            .flex_row()
            .h(px(row::ROW_MD))
            .bg(color::BG_1)
            .border_b(px(HAIRLINE))
            .border_color(color::LINE)
            .z_index(super::apps::MENU_Z)
            .role(AccessibilityRole::Tablist)
            .child(dynamic(move || {
                let active = active.get();
                tabs.get()
                    .iter()
                    .enumerate()
                    .map(|(i, t)| {
                        let on_select = on_select.clone();
                        tab_button(t, i == active).on_press(move |_: &PressEvent| on_select(i))
                    })
                    .collect::<Vec<_>>()
            }))
            .child(new_tab(apps, menu_key, on_pick))
            .child(spacer())
            .when_some(on_split, |b, f| b.child(split_buttons(f)))
    }
}

/// `+` and its menu. A pick, a second press, or Escape closes the menu.
fn new_tab(apps: Vec<AppChoice>, menu_key: Option<String>, on_pick: Rc<dyn Fn(String)>) -> View {
    let open = signal(false);
    if let Some(key) = &menu_key {
        on_key(key, move |_| open.update(|o| *o = !*o));
    }
    view()
        .relative()
        .h_full()
        .child(
            hrow(0.0)
                .h_full()
                .w(px(row::ROW_MD))
                .justify_center()
                .role(AccessibilityRole::Button)
                .label("New tab")
                .on_press(move |_: &PressEvent| open.update(|o| *o = !*o))
                .child(body("+", color::FG_MUTED)),
        )
        .child(dynamic(move || {
            open.get().then(|| {
                on_key("escape", move |_| open.set(false));
                let on_pick = on_pick.clone();
                app_menu(&apps, move |id| {
                    open.set(false);
                    on_pick(id);
                })
                .top(px(row::ROW_MD))
                .left(px(0.0))
            })
        }))
}

fn split_buttons(on_split: Rc<dyn Fn(Axis)>) -> View {
    let down = on_split.clone();
    hrow(space::SP_2)
        .pr(px(space::SP_3))
        .child(
            body("◫", color::FG_MUTED)
                .label("Split right")
                .on_press(move |_: &PressEvent| on_split(Axis::Row)),
        )
        .child(
            body("⊟", color::FG_MUTED)
                .label("Split down")
                .on_press(move |_: &PressEvent| down(Axis::Column)),
        )
}
