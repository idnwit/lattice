//! Headless runs of the workspace PoC: `+` menu, splits with the app
//! picker, and closing with a confirmation (specs/03-sessions.md, Behavior 11–13).

#![allow(
    dead_code,
    reason = "the gallery's canvas root is exercised in tests/gallery.rs"
)]

#[path = "gallery/app/mod.rs"]
mod app;

use std::error::Error;
use std::time::Duration;

use app::PocGallery;
use nsh_core::prelude::px;
use nsh_engine::{App, EngineConfig, Root};
use nsh_testing::Headless;

fn start() -> Result<Headless, Box<dyn Error>> {
    let mut config = EngineConfig::new(App::Rust(Root::of::<PocGallery>()));
    config.viewport = nsh_core::Size::new(px(1440.0), px(900.0));
    let mut app = Headless::start(config)?;
    app.first_render()?;
    Ok(app)
}

#[test]
fn plus_menu_pick_opens_a_tab_with_that_app() -> Result<(), Box<dyn Error>> {
    let mut app = start()?;
    app.wait_for("label:App picker")?;
    app.press("role:button[New tab]")?;
    app.wait_for("role:menu[New tab]")?;
    app.press("role:menu[New tab] >> role:button[Counter]")?;
    app.wait_for("role:tab[counter]")?;
    app.wait_for("text:count 0")?;
    app.press("role:button[Add]")?;
    app.wait_for("text:count 1")?;
    Ok(())
}

#[test]
fn split_opens_an_empty_pane_with_the_app_picker() -> Result<(), Box<dyn Error>> {
    let mut app = start()?;
    app.press_key("alt+c")?;
    app.wait_for("text:count 0")?;
    app.press_key("mod+d")?;
    app.wait_for("label:App picker")?;
    app.wait_for("text:▦2")?;
    app.press("role:button[Note]")?;
    app.wait_for("text:Note pane")?;
    Ok(())
}

#[test]
fn closing_an_app_pane_asks_and_an_empty_one_does_not() -> Result<(), Box<dyn Error>> {
    let mut app = start()?;
    app.press_key("alt+c")?;
    app.press_key("shift+mod+d")?;
    app.wait_for("label:App picker")?;
    // The focused empty pane closes at once.
    app.press_key("mod+w")?;
    app.wait_for("text:▦1")?;
    app.press_key("mod+w")?;
    app.wait_for("text:Close pane 1.1?")?;
    app.press("role:button[Cancel]")?;
    app.wait_for_gone("text:Close pane 1.1?", Duration::from_secs(2))?;
    app.press_key("mod+w")?;
    app.press("role:button[Close]")?;
    app.wait_for_gone("role:tab[counter]", Duration::from_secs(2))?;
    Ok(())
}
