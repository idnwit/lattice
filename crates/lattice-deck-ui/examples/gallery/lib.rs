//! `nsh` harness of the gallery: registers the canvas root so `nsh test` and
//! `nsh run` (nsh.json `"root"`) can mount them. The sources are the
//! headless tests' (`tests/gallery/app`).

#[path = "../../tests/gallery/app/mod.rs"]
mod app;

pub use app::Gallery;

use nsh_core::prelude::*;

/// The gallery's NSH component set.
#[native_module("lattice-deck-ui-gallery/native")]
fn native(registry: &mut Registry) {
    registry.register::<Gallery>();
}
