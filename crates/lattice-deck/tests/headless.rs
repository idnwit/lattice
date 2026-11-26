//! Headless render of the deck shell: without a worker it shows the app picker.

use std::error::Error;

use lattice_deck::DeckApp;
use nsh_core::prelude::px;
use nsh_engine::{App, EngineConfig, Root};
use nsh_testing::Headless;

#[test]
fn without_a_worker_the_workspace_shows_the_app_picker() -> Result<(), Box<dyn Error>> {
    let mut config = EngineConfig::new(App::Rust(Root::of::<DeckApp>()));
    config.viewport = nsh_core::Size::new(px(1440.0), px(900.0));
    let mut app = Headless::start(config)?;
    app.first_render()?;
    app.wait_for("label:App picker")?;
    app.wait_for("role:button[Terminal]")?;
    Ok(())
}
