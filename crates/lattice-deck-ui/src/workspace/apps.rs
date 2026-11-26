//! App choices: the `+` new-tab menu and the empty-pane app picker
//! (specs/03-sessions.md, Behavior 11–12; D40).

use std::rc::Rc;

use nsh_core::prelude::*;

use crate::style::{HAIRLINE, badge, body, hrow, label, spacer};
use crate::tokens::{color, radius, row, space};

/// Width of the `+` menu (components.md › Context menu).
const MENU_W: f32 = 220.0;
/// Width of the empty-pane picker list.
const PICKER_W: f32 = 260.0;

/// An app the user can open: a menu and picker entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppChoice {
    /// App id (`dev.lattice.terminal`); what `on_pick` hears.
    pub id: String,
    /// Label (`Terminal`).
    pub label: String,
    /// Keystroke that picks it from the tab bar (`alt+t`, as `nsh/keymap` names keys).
    pub key: Option<String>,
}

impl AppChoice {
    /// An app without a keystroke.
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            key: None,
        }
    }

    /// The same app picked by `key` (`alt+t`).
    pub fn with_key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }
}

/// One entry row: label, then the chord as this platform shows it.
fn choice_row(app: &AppChoice, height: f32, on_pick: Rc<dyn Fn(String)>) -> View {
    let id = app.id.clone();
    hrow(space::SP_2)
        .h(px(height))
        .px(px(space::SP_3))
        .rounded(px(radius::R_3))
        .role(AccessibilityRole::Button)
        .label(app.label.clone())
        .focusable(true)
        .hover(|s| s.bg(color::BG_3))
        .on_press(move |_: &PressEvent| on_pick(id.clone()))
        .child(body(app.label.clone(), color::FG))
        .child(spacer())
        .when_some(app.key.as_deref(), |r, k| {
            r.child(badge(key_label(k), color::FG_DIM))
        })
}

/// The `+` menu: an overlay list of apps (elevation: `bg/1` + `line/strong`).
///
/// Position it yourself (it is `absolute`); [`crate::workspace::TabBar`] does.
pub(crate) fn app_menu(apps: &[AppChoice], on_pick: impl Fn(String) + 'static) -> View {
    let on_pick: Rc<dyn Fn(String)> = Rc::new(on_pick);
    view()
        .absolute()
        .w(px(MENU_W))
        .p(px(space::SP_1))
        .rounded(px(radius::R_6))
        .border(px(HAIRLINE))
        .border_color(color::LINE_STRONG)
        .bg(color::BG_1)
        .z_index(MENU_Z)
        .role(AccessibilityRole::Menu)
        .label("New tab")
        .child(
            hrow(0.0)
                .h(px(row::ROW_XS))
                .px(px(space::SP_3))
                .child(label("New tab")),
        )
        .children(
            apps.iter()
                .map(|a| choice_row(a, row::ROW_SM, on_pick.clone())),
        )
}

/// Stacking order of menus over the workspace.
pub(crate) const MENU_Z: i32 = 10;

/// The empty-pane app picker: what a split opens (D40). Nothing is created
/// until a pick. Rows are `height` tall (`row/touch` on mobile).
pub fn app_picker(apps: &[AppChoice], height: f32, on_pick: impl Fn(String) + 'static) -> View {
    let on_pick: Rc<dyn Fn(String)> = Rc::new(on_pick);
    view()
        .flex_1()
        .items_center()
        .justify_center()
        .bg(color::BG_0)
        .child(
            view()
                .w(px(PICKER_W))
                .gap(px(space::SP_1))
                .role(AccessibilityRole::List)
                .label("App picker")
                .child(
                    hrow(0.0)
                        .h(px(row::ROW_XS))
                        .px(px(space::SP_3))
                        .child(label("Open in this pane")),
                )
                .children(apps.iter().map(|a| {
                    choice_row(a, height, on_pick.clone())
                        .bg(color::BG_1)
                        .border(px(HAIRLINE))
                        .border_color(color::LINE)
                })),
        )
}
