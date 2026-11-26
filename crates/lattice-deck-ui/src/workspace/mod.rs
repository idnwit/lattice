//! Workspace components for the deck: tab bar with the `+` menu, split
//! tree, pane frame and header, the empty-pane app picker, and the close
//! confirmation (specs/03-sessions.md, Behavior 2 and 11–14; D40).

mod apps;
mod pane;
mod split;
mod tabs;

pub use apps::{AppChoice, app_picker};
pub use pane::{Confirm, PaneHeader, confirm_card, pane_frame, pane_header};
pub use split::{Axis, SplitNode, split_view};
pub use tabs::{TabBar, TabItem};

use nsh_core::prelude::on_key;

/// Split right (specs/09-keyboard.md: `⌘D`).
pub(crate) const SPLIT_RIGHT_KEY: &str = "mod+d";
/// Split down (`⌘⇧D`).
pub(crate) const SPLIT_DOWN_KEY: &str = "shift+mod+d";
/// Close pane (`⌘W`).
pub(crate) const CLOSE_PANE_KEY: &str = "mod+w";

/// Binds the pane chords while the calling component lives: `⌘D` and
/// `⌘⇧D` call `on_split` with [`Axis::Row`] and [`Axis::Column`], `⌘W`
/// calls `on_close`. They act on the focused pane, which the caller knows.
pub fn bind_pane_keys(on_split: impl Fn(Axis) + 'static, on_close: impl Fn() + 'static) {
    let on_split = std::rc::Rc::new(on_split);
    let down = on_split.clone();
    on_key(SPLIT_RIGHT_KEY, move |_| on_split(Axis::Row));
    on_key(SPLIT_DOWN_KEY, move |_| down(Axis::Column));
    on_key(CLOSE_PANE_KEY, move |_| on_close());
}
