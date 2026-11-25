//! `lattice-deck`: opens the deck in a native window.
//!
//! The layout follows the window width. `LATTICE_DECK_LAYOUT=mobile` opens
//! a phone-sized (390×844) window to show the mobile layout on macOS.

#[cfg(target_os = "macos")]
fn main() -> std::process::ExitCode {
    use nsh_platform_macos::{AppOptions, AppSource, Root, run_app};

    /// Desktop canvas size (artboard "Desktop / Web — Advanced workspace").
    const DESKTOP: (i32, i32) = (1440, 900);
    /// Mobile canvas size (artboards "Mobile — …").
    const MOBILE: (i32, i32) = (390, 844);

    let mobile = std::env::var("LATTICE_DECK_LAYOUT").is_ok_and(|v| v == "mobile");
    let options = AppOptions {
        title: "Lattice".to_owned(),
        app_id: "dev.lattice.deck".to_owned(),
        size: if mobile { MOBILE } else { DESKTOP },
        keep_size: true,
        ..AppOptions::default()
    };
    run_app(
        AppSource::Rust(Root::of::<lattice_deck::DeckApp>()),
        options,
    )
}

/// Only macOS has a window backend wired in this demo.
#[cfg(not(target_os = "macos"))]
fn main() {}
