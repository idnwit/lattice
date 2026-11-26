//! Gallery of `lattice-deck-ui`: the design canvas' screens built from the
//! library, with fixture data, in a native window (macOS).
//!
//! `cargo run -p lattice-deck-ui --example gallery`. The layout follows the
//! window width; `LATTICE_DECK_LAYOUT=mobile` opens a phone-sized
//! (390×844) window. `LATTICE_GALLERY=poc` shows the workspace PoC (tabs,
//! `+` menu, splits, app picker, close confirmation) instead.
#![allow(
    clippy::print_stdout,
    reason = "example binary: may print, as binaries do (engineering.md, Lints)"
)]

// The screens live with the headless tests that drive them (`tests/gallery/app`).
#[path = "../../tests/gallery/app/mod.rs"]
mod app;

#[cfg(target_os = "macos")]
fn main() -> std::process::ExitCode {
    use nsh_platform_macos::{AppOptions, AppSource, Root, run_app};

    let mobile = std::env::var("LATTICE_DECK_LAYOUT").is_ok_and(|v| v == "mobile");
    let (w, h) = if mobile {
        app::MOBILE_SIZE
    } else {
        app::DESKTOP_SIZE
    };
    let options = AppOptions {
        title: "Lattice".to_owned(),
        app_id: "dev.lattice.deck-ui.gallery".to_owned(),
        // Canvas sizes are whole points.
        size: (w as i32, h as i32),
        keep_size: true,
        ..AppOptions::default()
    };
    let poc = std::env::var("LATTICE_GALLERY").is_ok_and(|v| v == "poc");
    let root = if poc {
        Root::of::<app::PocGallery>()
    } else {
        Root::of::<app::Gallery>()
    };
    run_app(AppSource::Rust(root), options)
}

/// Only macOS has a window backend wired in this example.
#[cfg(not(target_os = "macos"))]
fn main() {}
