//! Lattice deck UI library on NSH (pure Rust): tokens, components, and
//! layouts (D39).
//!
//! Components take props (plain structs and values) and report events
//! through closures, like a React component library: no fixtures, no
//! networking, no global state, no app-specific names. The deck
//! (`lattice-deck`) composes them; later, the view-tree interpreter maps
//! WASM viewer output (nodes: key, parent, widget, props) onto them.
//! `examples/gallery` shows the wiring with the design canvas' fixture data.
//!
//! # Modules
//!
//! - [`tokens`]: generated from `docs/design-system/tokens.json` (the only source).
//! - [`fonts`]: the bundled fonts of the `mono` stack; [`fonts::register`] at start.
//! - [`style`]: type styles and small layout helpers over tokens.
//! - [`components`]: state glyph, buttons, styled lines, approval card, worker,
//!   git, ports, files, tool tabs, chrome, mobile pieces.
//! - [`workspace`]: tab bar with the `+` menu, split tree, pane frame and header,
//!   app picker, close confirmation, pane chords.
//! - [`layout`]: desktop, mobile (one region at a time), responsive.
//!
//! # Usage
//!
//! ```ignore
//! let shell = Shell::new(move || workspace_view())
//!     .left(Panel::new("Sessions", 288.0, "mod+b", sessions_view))
//!     .status(status_view);
//! responsive_layout(shell, Panels::new(false, false), Regions::new(Region::Workspace))
//! ```

pub mod components;
pub mod fonts;
pub mod layout;
pub mod style;
pub mod tokens;
pub mod workspace;

pub use layout::{
    Layout, MIN_TAP_TARGET, Panel, Panels, Region, Regions, Shell, responsive_layout,
};
