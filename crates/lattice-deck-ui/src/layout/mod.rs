//! Layouts: desktop (panels beside the workspace), mobile (one region at a
//! time, D41), and a responsive one that picks by width.
//!
//! A [`Shell`] names the parts; each part is a render closure, called
//! again when the layout changes (state that must survive a switch lives
//! with the caller, in signals).

mod desktop;
mod mobile;

use std::rc::Rc;

use nsh_core::prelude::*;

pub(crate) use desktop::desktop_layout;
pub(crate) use mobile::mobile_layout;

use crate::tokens::{color, row};

/// Minimum tap target height on mobile, in points (design invariant: ≥ 44px).
pub const MIN_TAP_TARGET: f32 = row::ROW_TOUCH;

/// Width below which the deck shows the mobile layout, in points.
pub(crate) const MOBILE_MAX_WIDTH: f32 = 600.0;

/// A part of the shell: called to build its view.
pub(crate) type Render = Rc<dyn Fn() -> View>;

/// Which layout the deck shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    /// Panels beside the workspace.
    Desktop,
    /// One region at a time.
    Mobile,
}

impl Layout {
    /// The layout for a viewport `width` in points.
    pub fn for_width(width: f32) -> Self {
        if width < MOBILE_MAX_WIDTH {
            Self::Mobile
        } else {
            Self::Desktop
        }
    }
}

/// A side panel: a region on mobile, a column beside the workspace on desktop.
#[derive(Clone)]
pub struct Panel {
    /// Region name on mobile (`Sessions`, `Tools`).
    pub label: String,
    /// Column width on desktop, in points.
    pub width: f32,
    /// Keystroke that toggles it on desktop (`mod+b`).
    pub key: String,
    /// Its content.
    pub render: Render,
}

impl Panel {
    /// A panel named `label`, `width` wide on desktop, toggled by `key`.
    pub fn new(
        label: impl Into<String>,
        width: f32,
        key: impl Into<String>,
        render: impl Fn() -> View + 'static,
    ) -> Self {
        Self {
            label: label.into(),
            width,
            key: key.into(),
            render: Rc::new(render),
        }
    }
}

/// The parts of a deck screen. Only the workspace is required.
#[derive(Clone)]
pub struct Shell {
    pub(crate) header: Option<Render>,
    pub(crate) left: Option<Panel>,
    pub(crate) workspace: Render,
    pub(crate) right: Option<Panel>,
    pub(crate) status: Option<Render>,
}

impl Shell {
    /// A shell with only a workspace.
    pub fn new(workspace: impl Fn() -> View + 'static) -> Self {
        Self {
            header: None,
            left: None,
            workspace: Rc::new(workspace),
            right: None,
            status: None,
        }
    }

    /// A bar above everything on desktop (the title bar). Not shown on mobile.
    pub fn header(mut self, render: impl Fn() -> View + 'static) -> Self {
        self.header = Some(Rc::new(render));
        self
    }

    /// The left panel.
    pub fn left(mut self, panel: Panel) -> Self {
        self.left = Some(panel);
        self
    }

    /// The right panel.
    pub fn right(mut self, panel: Panel) -> Self {
        self.right = Some(panel);
        self
    }

    /// The status bar under everything on desktop. Not shown on mobile.
    pub fn status(mut self, render: impl Fn() -> View + 'static) -> Self {
        self.status = Some(Rc::new(render));
        self
    }
}

/// Which desktop panels show. `Copy`: signals of the calling component.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Panels {
    /// The left panel shows.
    pub left: Signal<bool>,
    /// The right panel shows.
    pub right: Signal<bool>,
}

impl Panels {
    /// Panel state, owned by the calling component.
    pub fn new(left: bool, right: bool) -> Self {
        Self {
            left: signal(left),
            right: signal(right),
        }
    }
}

/// A mobile region.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Region {
    /// The left panel.
    Left,
    /// The workspace.
    Workspace,
    /// The right panel.
    Right,
}

/// Which mobile region shows, and whether the switcher is open. `Copy`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Regions {
    /// The region shown.
    pub region: Signal<Region>,
    /// The switcher shows.
    pub switcher: Signal<bool>,
}

impl Regions {
    /// Region state, owned by the calling component; the switcher starts hidden.
    pub fn new(region: Region) -> Self {
        Self {
            region: signal(region),
            switcher: signal(false),
        }
    }

    /// Shows `region` and hides the switcher.
    pub fn show(self, region: Region) {
        self.region.set(region);
        self.switcher.set(false);
    }

    /// Opens the switcher.
    pub fn open_switcher(self) {
        self.switcher.set(true);
    }
}

/// Desktop or mobile by the window width ([`Layout::for_width`]). Resizing
/// within one layout keeps it and its state.
pub fn responsive_layout(shell: Shell, panels: Panels, regions: Regions) -> View {
    let layout = computed(move || Layout::for_width(window().width.0));
    view()
        .flex_1()
        .bg(color::BG_0)
        .child(dynamic(move || match layout.get() {
            Layout::Desktop => desktop_layout(&shell, panels),
            Layout::Mobile => mobile_layout(&shell, regions),
        }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_switches_at_the_mobile_width() {
        let cases = [
            (390.0, Layout::Mobile),
            (599.0, Layout::Mobile),
            (600.0, Layout::Desktop),
            (1440.0, Layout::Desktop),
        ];
        for (width, want) in cases {
            assert_eq!(Layout::for_width(width), want, "case={width}");
        }
    }
}
