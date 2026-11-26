//! Headless runs of the gallery: both layouts show the canvas mocks' key
//! texts and their interactions work through the library's components.

#![allow(
    dead_code,
    reason = "the gallery's sizes are read by the example binary only"
)]

#[path = "gallery/app/mod.rs"]
mod app;

use std::error::Error;

use app::Gallery as DeckApp;
use nsh_core::prelude::px;
use nsh_engine::{App, EngineConfig, Root};
use nsh_testing::Headless;

fn start(width: f32, height: f32) -> Result<Headless, Box<dyn Error>> {
    let mut config = EngineConfig::new(App::Rust(Root::of::<DeckApp>()));
    config.viewport = nsh_core::Size::new(px(width), px(height));
    let mut app = Headless::start(config)?;
    app.first_render()?;
    Ok(app)
}

fn assert_shows(app: &mut Headless, texts: &[&str], case: &str) -> Result<(), Box<dyn Error>> {
    for want in texts {
        app.wait_for(&format!("text:{want}"))
            .map_err(|e| format!("case={case} text={want}: {e}"))?;
    }
    Ok(())
}

#[test]
fn desktop_width_shows_the_workspace() -> Result<(), Box<dyn Error>> {
    let mut app = start(1440.0, 900.0)?;
    assert_shows(
        &mut app,
        &[
            "api-server",
            "build-01",
            "direct · 18ms",
            "SESSIONS",
            "refactor-auth",
            "Push",
            "a3f9c21",
        ],
        "desktop",
    )?;
    assert!(
        !app.screen().shows("swipe ⇆"),
        "desktop must not show the mobile screen"
    );
    Ok(())
}

#[test]
fn mobile_width_shows_the_terminal_screen() -> Result<(), Box<dyn Error>> {
    let mut app = start(390.0, 844.0)?;
    assert_shows(
        &mut app,
        &[
            "api-server",
            "build-01 · direct 18ms",
            "Allow",
            "Always",
            "Deny",
            "pane 1/3",
            "esc",
        ],
        "mobile",
    )?;
    assert!(
        !app.screen().shows("refactor-auth"),
        "mobile must not show the desktop workspace"
    );
    Ok(())
}

#[test]
fn mobile_tap_targets_are_at_least_touch_height() -> Result<(), Box<dyn Error>> {
    let app = start(390.0, 844.0)?;
    for name in ["Allow", "Always", "Deny"] {
        let layout = app
            .screen()
            .layout(&format!("role:button[{name}]"))?
            .ok_or_else(|| format!("no layout for {name}"))?;
        let height = layout.bounds.size.height.0;
        assert!(
            height >= lattice_deck_ui::MIN_TAP_TARGET,
            "case={name} height={height}"
        );
    }
    Ok(())
}

#[test]
fn desktop_tab_press_switches_the_panes() -> Result<(), Box<dyn Error>> {
    let mut app = start(1440.0, 900.0)?;
    app.wait_for("text:running 6 tests")?;
    app.press("role:tab[db-shell]")?;
    app.wait_for("text:/select count/")?;
    assert!(
        !app.screen().shows("running 6 tests"),
        "old tab's panes are gone"
    );
    app.wait_for("text:db-shell › tab 3 › pane 3.1")?;
    Ok(())
}

#[test]
fn desktop_tool_tabs_switch_the_panel_and_ports_toggle() -> Result<(), Box<dyn Error>> {
    let mut app = start(1440.0, 900.0)?;
    app.press("role:tab[Files]")?;
    app.wait_for("text:handshake.rs")?;
    app.press("role:tab[Ports]")?;
    app.wait_for("text:2/5 on")?;
    app.press("label:Forward port 9229")?;
    app.wait_for("text:3/5 on")?;
    app.wait_for("text:⇄ 3 ports")?;
    // A conflicting port stays off.
    app.press("label:Forward port 8080")?;
    app.wait_for("text:3/5 on")?;
    Ok(())
}

#[test]
fn desktop_commit_moves_staged_files_into_history() -> Result<(), Box<dyn Error>> {
    let mut app = start(1440.0, 900.0)?;
    app.type_text("label:Commit message", " today")?;
    app.press("role:button[Commit 2 files]")?;
    app.wait_for("text:peer: survive NAT rebinding on relay today")?;
    app.wait_for("text:↑3 ↓0")?;
    app.wait_for("role:button[Commit 0 files]")?;
    app.press("role:button[Push]")?;
    app.wait_for("text:↑0 ↓0")?;
    Ok(())
}

#[test]
fn desktop_approval_buttons_and_keys_resolve_it() -> Result<(), Box<dyn Error>> {
    let mut app = start(1440.0, 900.0)?;
    app.wait_for("text:1 agent waiting")?;
    app.press("role:button[Allow]")?;
    app.wait_for("text:/^✓ allowed: git push/")?;
    app.wait_for_gone("text:1 agent waiting", std::time::Duration::from_secs(2))?;
    let mut app = start(1440.0, 900.0)?;
    app.press_key("n")?;
    app.wait_for("text:/^✗ denied: git push/")?;
    Ok(())
}

#[test]
fn desktop_terminal_runs_typed_commands() -> Result<(), Box<dyn Error>> {
    let mut app = start(1440.0, 900.0)?;
    app.type_text("label:Terminal input", "ls -la")?;
    app.submit("label:Terminal input")?;
    app.wait_for("text:/demo: ls -la/")?;
    app.type_text("label:Terminal input", "clear")?;
    app.submit("label:Terminal input")?;
    app.wait_for_gone("text:/demo: ls -la/", std::time::Duration::from_secs(2))?;
    // Letters typed into the field never answer the approval (`a` in `clear`).
    assert!(
        app.screen().shows("1 agent waiting"),
        "approval still waits"
    );
    Ok(())
}

#[test]
fn desktop_session_press_selects_it() -> Result<(), Box<dyn Error>> {
    let mut app = start(1440.0, 900.0)?;
    assert!(
        !app.screen().selected("role:button[web-e2e]")?,
        "case=before"
    );
    app.press("role:button[web-e2e]")?;
    app.settle();
    assert!(app.screen().selected("role:button[web-e2e]")?, "case=after");
    assert!(
        !app.screen().selected("role:button[api-server]")?,
        "case=old row"
    );
    Ok(())
}

#[test]
fn mobile_input_runs_commands_and_tabs_switch() -> Result<(), Box<dyn Error>> {
    let mut app = start(390.0, 844.0)?;
    app.submit("label:Command")?;
    app.wait_for("text:/demo: git st/")?;
    app.press("role:tab[db-shell]")?;
    app.wait_for("text:/select count/")?;
    app.press("label:Next pane")?;
    app.wait_for("text:pane 2/2")?;
    Ok(())
}

#[test]
fn mobile_ctrl_latches_and_keys_type_markers() -> Result<(), Box<dyn Error>> {
    let mut app = start(390.0, 844.0)?;
    app.press("label:Control")?;
    app.wait_for("label:Control, latched")?;
    app.press("label:Escape")?;
    app.wait_for("label:Control")?;
    assert_eq!(
        app.screen().input_text("label:Command")?,
        Some("git st^esc")
    );
    Ok(())
}

#[test]
fn mobile_approval_answer_replaces_the_card() -> Result<(), Box<dyn Error>> {
    let mut app = start(390.0, 844.0)?;
    app.press("role:button[Always]")?;
    app.wait_for("text:always allowed: git push origin feat/relay-fallback")?;
    assert!(!app.screen().shows("Deny"), "card is gone");
    Ok(())
}

#[test]
fn mobile_switcher_shows_a_region_and_hides_itself() -> Result<(), Box<dyn Error>> {
    let mut app = start(390.0, 844.0)?;
    app.press("role:button[Open sessions]")?;
    app.wait_for("label:Region switcher")?;
    app.press("role:button[Sessions]")?;
    app.wait_for("text:refactor-auth")?;
    app.wait_for_gone("label:Region switcher", std::time::Duration::from_secs(2))?;
    assert!(!app.screen().shows("swipe ⇆"), "one region at a time");
    app.press("role:button[Back to workspace]")?;
    app.wait_for("text:swipe ⇆")?;
    Ok(())
}

#[test]
fn desktop_panel_keys_toggle_the_panels() -> Result<(), Box<dyn Error>> {
    let mut app = start(1440.0, 900.0)?;
    app.wait_for("text:refactor-auth")?;
    app.press_key("mod+b")?;
    app.wait_for_gone("text:refactor-auth", std::time::Duration::from_secs(2))?;
    app.press_key("alt+mod+b")?;
    app.wait_for_gone("role:tab[Files]", std::time::Duration::from_secs(2))?;
    app.press_key("mod+b")?;
    app.wait_for("text:refactor-auth")?;
    Ok(())
}

#[test]
fn desktop_plus_opens_the_new_tab_menu() -> Result<(), Box<dyn Error>> {
    let mut app = start(1440.0, 900.0)?;
    app.press("role:button[New tab]")?;
    app.wait_for("role:menu[New tab]")?;
    app.press_key("escape")?;
    app.wait_for_gone("role:menu[New tab]", std::time::Duration::from_secs(2))?;
    Ok(())
}
