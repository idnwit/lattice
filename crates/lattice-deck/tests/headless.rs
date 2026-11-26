//! Headless renders of the deck shell, each in its own data dir (specs/01-pairing.md, UI).
//!
//! `DeckApp` reads `LATTICE_DATA_DIR`. Setting it in-process is `unsafe` (edition 2024), so
//! each test reruns this binary for itself with the variable set ([`in_data_dir`]).

use std::error::Error;
use std::path::PathBuf;
use std::time::Duration;

use lattice_deck::DeckApp;
use nsh_core::prelude::px;
use nsh_engine::{App, EngineConfig, Root};
use nsh_testing::Headless;

/// Set in the rerun child: the test body runs there.
const CHILD_ENV: &str = "LATTICE_DECK_TEST_CHILD";
/// A worker list line: `<node id> <ip:port> <name>` (any valid key).
const WORKER_LINE: &str =
    "8a88e3dd7409f195fd52db2d3cba5d72ca6709bf1d94121bf3748801b40f6f5c 10.0.0.2:7441 build-01\n";

/// Runs `test` in a child process with a fresh data dir; `workers` seeds `deck/workers`.
fn in_data_dir(
    name: &str,
    workers: Option<&str>,
    test: fn() -> Result<(), Box<dyn Error>>,
) -> Result<(), Box<dyn Error>> {
    if std::env::var_os(CHILD_ENV).is_some() {
        return test();
    }
    let dir: PathBuf =
        std::env::temp_dir().join(format!("lattice-deck-{name}-{}", std::process::id()));
    drop(std::fs::remove_dir_all(&dir));
    std::fs::create_dir_all(dir.join("deck"))?;
    if let Some(text) = workers {
        std::fs::write(dir.join("deck/workers"), text)?;
    }
    let out = std::process::Command::new(std::env::current_exe()?)
        .args([name, "--exact", "--nocapture"])
        .env(CHILD_ENV, "1")
        .env("LATTICE_DATA_DIR", &dir)
        .output()?;
    drop(std::fs::remove_dir_all(&dir));
    let log = String::from_utf8_lossy(&out.stdout);
    if !out.status.success() || !log.contains("1 passed") {
        return Err(format!("{name} failed in its child:\n{log}").into());
    }
    Ok(())
}

fn start() -> Result<Headless, Box<dyn Error>> {
    let mut config = EngineConfig::new(App::Rust(Root::of::<DeckApp>()));
    config.viewport = nsh_core::Size::new(px(1440.0), px(900.0));
    let mut app = Headless::start(config)?;
    app.first_render()?;
    Ok(app)
}

#[test]
fn without_a_paired_worker_the_workspace_offers_add_worker() -> Result<(), Box<dyn Error>> {
    in_data_dir(
        "without_a_paired_worker_the_workspace_offers_add_worker",
        None,
        || {
            let mut app = start()?;
            app.wait_for("text:No worker yet.")?;
            app.wait_for("text:no worker")?;
            app.press("role:button[Add worker] >> nth=1")?;
            app.wait_for("role:dialog[Add worker]")?;
            app.wait_for("text:Expires in 5 min. One use.")?;
            app.press_key("escape")?;
            app.wait_for_gone("role:dialog[Add worker]", Duration::from_secs(2))?;
            Ok(())
        },
    )
}

#[test]
fn a_paired_worker_shows_on_the_rail_and_the_app_picker_opens() -> Result<(), Box<dyn Error>> {
    in_data_dir(
        "a_paired_worker_shows_on_the_rail_and_the_app_picker_opens",
        Some(WORKER_LINE),
        || {
            let mut app = start()?;
            app.wait_for("role:tab[B1]")?;
            app.wait_for("label:App picker")?;
            app.wait_for("role:button[Terminal]")?;
            Ok(())
        },
    )
}
