//! NSH views. Views take model values as props; they never read fixtures.

mod approval;
mod components;
pub(crate) mod desktop;
pub(crate) mod mobile;
mod state;
mod style;
mod terminal;
pub(crate) mod tokens;

use nsh_core::prelude::*;

use nsh_core::element::desktop::{current_window, window as app_window};

use crate::fixtures;
use crate::model::{MobileTerminal, Workspace};

/// Minimum tap target height on mobile, in points (design invariant: ≥ 44px).
pub const MIN_TAP_TARGET: f32 = tokens::row::ROW_TOUCH;

/// Width below which the deck shows the mobile layout, in points.
pub(crate) const MOBILE_MAX_WIDTH: f32 = 600.0;

/// Which screen layout the deck shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Layout {
    /// The desktop workspace.
    Desktop,
    /// The mobile terminal screen.
    Mobile,
}

impl Layout {
    /// The layout for a viewport `width` in points.
    pub(crate) fn for_width(width: f32) -> Self {
        if width < MOBILE_MAX_WIDTH {
            Self::Mobile
        } else {
            Self::Desktop
        }
    }
}

/// Desktop canvas size (artboard "Desktop / Web — Advanced workspace").
const DESKTOP_SIZE: (f32, f32) = (1440.0, 900.0);
/// Mobile canvas size (artboards "Mobile — …").
const MOBILE_SIZE: (f32, f32) = (390.0, 844.0);

/// Size of the deck window when it opens on a desktop.
///
/// `LATTICE_DECK_LAYOUT=mobile` opens a phone-sized window, so the mobile
/// layout can be shown on macOS. On iOS the window fills the screen.
fn opening_size() -> Size<Points> {
    let mobile = std::env::var("LATTICE_DECK_LAYOUT").is_ok_and(|v| v == "mobile");
    let (w, h) = if mobile { MOBILE_SIZE } else { DESKTOP_SIZE };
    size(px(w), px(h))
}

/// Root component: one window whose content picks desktop or mobile by width.
///
/// NSH window model v2: the app host never shows; the app declares its own
/// window. Off the desktops (iOS) the window's content fills the app surface.
#[component]
pub fn DeckApp() -> impl IntoElement {
    let workspace = fixtures::workspace();
    let phone = fixtures::mobile_terminal();
    app_window()
        .id("main")
        .title("Lattice")
        .default_size(opening_size())
        .content(move || screen(workspace.clone(), phone.clone()))
}

/// Brings the deck window to the front once, when it opens.
///
/// A binary started from a terminal (`cargo run`) is not activated by macOS,
/// so its window would open behind the terminal or on another Space.
/// `focus()` activates the app and makes the window key. No-op off the desktops.
fn bring_to_front() {
    let win = current_window();
    effect(move || win.focus());
}

/// The screen for the current viewport width.
fn screen(workspace: Workspace, phone: MobileTerminal) -> impl IntoElement {
    bring_to_front();
    let layout = computed(move || Layout::for_width(window().width.0));
    view()
        .flex_1()
        .bg(tokens::color::BG_0)
        // A memo: resizing within one layout keeps the screen and its state.
        .child(dynamic(move || match layout.get() {
            Layout::Desktop => desktop::workspace(&workspace),
            Layout::Mobile => mobile::terminal(&phone),
        }))
}
