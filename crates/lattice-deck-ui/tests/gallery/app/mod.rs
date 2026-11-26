//! The gallery: the design canvas' screens rebuilt from `lattice-deck-ui`
//! components and layouts, with fixture data, plus a workspace PoC that
//! wires a viewer through the viewer contract.
//!
//! Shared by `examples/gallery/main.rs` (`cargo run --example gallery`),
//! the `nsh test` harness (`examples/gallery/lib.rs`), and `tests/`.

mod center;
mod desktop;
mod fixtures;
mod mobile;
mod model;
mod poc;
mod shell;
mod state;

use lattice_deck_ui::layout::{Layout, Panel, Panels, Region, Regions, Shell, responsive_layout};
use lattice_deck_ui::tokens::color;
use nsh_core::element::desktop::{current_window, window as app_window};
use nsh_core::prelude::*;

#[allow(
    unused_imports,
    reason = "the example and tests/workspace.rs use it; tests/gallery.rs does not"
)]
pub(crate) use poc::PocGallery;
use state::{Desk, Phone};

/// Desktop canvas size (artboard "Desktop / Web — Advanced workspace").
pub(crate) const DESKTOP_SIZE: (f32, f32) = (1440.0, 900.0);
/// Mobile canvas size (artboards "Mobile — …").
pub(crate) const MOBILE_SIZE: (f32, f32) = (390.0, 844.0);
/// Left panel width (canvas: 288).
const SESSIONS_W: f32 = 288.0;
/// Right panel width (canvas: 300).
const TOOLS_W: f32 = 300.0;
/// Toggles the sessions panel (specs/09-keyboard.md: `⌘B`).
const SESSIONS_KEY: &str = "mod+b";
/// Toggles the tools panel. TBD: chord in specs/09-keyboard.md.
const TOOLS_KEY: &str = "alt+mod+b";

/// Size of the window when it opens on a desktop.
///
/// `LATTICE_DECK_LAYOUT=mobile` opens a phone-sized window, so the mobile
/// layout can be shown on macOS. On iOS the window fills the screen.
pub(crate) fn opening_size() -> Size<Points> {
    let mobile = std::env::var("LATTICE_DECK_LAYOUT").is_ok_and(|v| v == "mobile");
    let (w, h) = if mobile { MOBILE_SIZE } else { DESKTOP_SIZE };
    size(px(w), px(h))
}

/// Brings the window to the front once, when it opens.
///
/// A binary started from a terminal (`cargo run`) is not activated by macOS,
/// so its window would open behind the terminal or on another Space.
pub(crate) fn bring_to_front() {
    let win = current_window();
    effect(move || win.focus());
}

/// Root: the canvas mocks in one window, desktop or mobile by width.
#[component]
pub fn Gallery() -> impl IntoElement {
    lattice_deck_ui::fonts::register();
    app_window()
        .id("main")
        .title("Lattice")
        .default_size(opening_size())
        .content(screen)
}

fn screen() -> View {
    bring_to_front();
    let ws = fixtures::workspace();
    let phone_data = fixtures::mobile_terminal();
    let (desk, phone) = (Desk::new(&ws), Phone::new(&phone_data));
    let regions = Regions::new(Region::Workspace);
    let (left, right, center, status) = (ws.clone(), ws.clone(), ws.clone(), ws.clone());
    let shell = Shell::new(move || {
        // The canvas draws a different workspace per layout.
        match Layout::for_width(untracked(|| window().width.0)) {
            Layout::Desktop => center::center(&center, desk),
            Layout::Mobile => mobile::terminal(&phone_data, phone, regions),
        }
    })
    .header(move || lattice_deck_ui::components::title_bar(&ws.account, &ws.sync))
    .left(Panel::new(
        "Sessions",
        SESSIONS_W,
        SESSIONS_KEY,
        move || desktop::sessions(&left, desk),
    ))
    .right(Panel::new("Tools", TOOLS_W, TOOLS_KEY, move || {
        desktop::tools(&right, desk)
    }))
    .status(move || desktop::status(&status, desk));
    view().flex_1().bg(color::BG_0).child(responsive_layout(
        shell,
        Panels::new(true, true),
        regions,
    ))
}
