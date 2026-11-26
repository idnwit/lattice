//! Mobile layout (D41): one region at a time. A switcher picks the region
//! and hides itself after a pick. Every tap target is `row/touch` (44).

use nsh_core::prelude::*;

use super::{Region, Regions, Render, Shell};
use crate::components::icon_button;
use crate::style::{HAIRLINE, body, hrow, label, mono, selected_state, spacer};
use crate::tokens::{color, radius, row, space};

/// Opacity of the backdrop under the switcher sheet.
const BACKDROP_OPACITY: f32 = 0.6;

/// The region `regions` names; the switcher over it while open.
pub(crate) fn mobile_layout(shell: &Shell, regions: Regions) -> View {
    let names = Names {
        left: shell.left.as_ref().map(|p| p.label.clone()),
        right: shell.right.as_ref().map(|p| p.label.clone()),
    };
    let parts = Parts {
        left: shell.left.as_ref().map(|p| p.render.clone()),
        workspace: shell.workspace.clone(),
        right: shell.right.as_ref().map(|p| p.render.clone()),
    };
    let bar_names = names.clone();
    view()
        .flex_1()
        .relative()
        .bg(color::BG_0)
        .p_safe_area()
        .child(dynamic(move || {
            let region = regions.region.get();
            match (region, parts.get(region)) {
                (Region::Workspace, _) | (_, None) => (parts.workspace)(),
                (_, Some(render)) => view()
                    .flex_1()
                    .child(region_bar(&bar_names.of(region), regions))
                    .child(render()),
            }
        }))
        .child(dynamic(move || {
            regions.switcher.get().then(|| switcher(&names, regions))
        }))
}

#[derive(Clone)]
struct Names {
    left: Option<String>,
    right: Option<String>,
}

impl Names {
    fn of(&self, region: Region) -> String {
        match region {
            Region::Left => self.left.clone().unwrap_or_default(),
            Region::Workspace => "Workspace".to_owned(),
            Region::Right => self.right.clone().unwrap_or_default(),
        }
    }

    /// Regions in switcher order, with names; absent panels are left out.
    fn list(&self) -> Vec<(Region, String)> {
        [Region::Left, Region::Workspace, Region::Right]
            .into_iter()
            .filter(|r| match r {
                Region::Left => self.left.is_some(),
                Region::Workspace => true,
                Region::Right => self.right.is_some(),
            })
            .map(|r| (r, self.of(r)))
            .collect()
    }
}

struct Parts {
    left: Option<Render>,
    workspace: Render,
    right: Option<Render>,
}

impl Parts {
    fn get(&self, region: Region) -> Option<Render> {
        match region {
            Region::Left => self.left.clone(),
            Region::Workspace => Some(self.workspace.clone()),
            Region::Right => self.right.clone(),
        }
    }
}

/// Bar over a panel region: back to the workspace, its name, the switcher.
fn region_bar(name: &str, regions: Regions) -> View {
    hrow(space::SP_1)
        .flex_none()
        .h(px(row::ROW_TOUCH + space::SP_2))
        .px(px(space::SP_1))
        .bg(color::BG_1)
        .border_b(px(HAIRLINE))
        .border_color(color::LINE)
        .child(
            icon_button("‹", "Back to workspace")
                .on_press(move |_: &PressEvent| regions.show(Region::Workspace)),
        )
        .child(mono(name.to_owned(), color::FG))
        .child(spacer())
        .child(
            icon_button("☰", "Switch region")
                .on_press(move |_: &PressEvent| regions.open_switcher()),
        )
}

/// Bottom sheet over a dimmed backdrop: one row per region. A pick shows
/// it and hides the sheet; a backdrop tap hides it.
fn switcher(names: &Names, regions: Regions) -> View {
    let current = regions.region.get_untracked();
    let rows = names.list().into_iter().map(move |(region, name)| {
        let on = region == current;
        hrow(space::SP_2)
            .h(px(row::ROW_TOUCH))
            .px(px(space::SP_4))
            .rounded(px(radius::R_6))
            .when(on, |r| r.bg(color::BG_3))
            .role(AccessibilityRole::Button)
            .label(name.clone())
            .accessibility_state(selected_state(on))
            .on_press(move |_: &PressEvent| regions.show(region))
            .child(body(name, if on { color::FG } else { color::FG_MUTED }))
    });
    view()
        .absolute()
        .inset(px(0.0))
        .justify_end()
        .child(
            view()
                .absolute()
                .inset(px(0.0))
                .bg(color::BG_0)
                .opacity(BACKDROP_OPACITY)
                .label("Close switcher")
                .on_press(move |_: &PressEvent| regions.switcher.set(false)),
        )
        .child(
            view()
                .gap(px(space::SP_1))
                .p(px(space::SP_2))
                .rounded_t(px(radius::R_12))
                .border_t(px(HAIRLINE))
                .border_color(color::LINE_STRONG)
                .bg(color::BG_1)
                .role(AccessibilityRole::Menu)
                .label("Region switcher")
                .child(
                    hrow(0.0)
                        .h(px(row::ROW_XS))
                        .px(px(space::SP_4))
                        .child(label("Show")),
                )
                .children(rows),
        )
}
