//! Desktop layout: header · [left panel] workspace [right panel] · status bar.

use nsh_core::prelude::*;

use super::{Panel, Panels, Shell};
use crate::style::HAIRLINE;
use crate::tokens::color;

/// Panels toggle beside the workspace, each with its key.
pub(crate) fn desktop_layout(shell: &Shell, panels: Panels) -> View {
    let workspace = shell.workspace.clone();
    view()
        .flex_1()
        .bg(color::BG_0)
        .when_some(shell.header.clone(), |v, h| v.child(h()))
        .child(
            view()
                .flex_1()
                .flex_row()
                .min_h(px(0.0))
                .when_some(shell.rail.clone(), |v, r| v.child(r()))
                .when_some(shell.left.clone(), |v, p| {
                    v.child(side(p, panels.left, Edge::Left))
                })
                .child(
                    view()
                        .flex_1()
                        .min_w(px(0.0))
                        .label("Workspace")
                        .child(workspace()),
                )
                .when_some(shell.right.clone(), |v, p| {
                    v.child(side(p, panels.right, Edge::Right))
                }),
        )
        .when_some(shell.status.clone(), |v, s| v.child(s()))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Edge {
    Left,
    Right,
}

/// A panel column, shown while `open`; its key flips `open`.
fn side(panel: Panel, open: Signal<bool>, edge: Edge) -> AnyElement {
    on_key(&panel.key, move |_| open.update(|o| *o = !*o));
    let Panel {
        label,
        width,
        render,
        ..
    } = panel;
    dynamic(move || {
        open.get().then(|| {
            let col = view()
                .flex_none()
                .w(px(width))
                .bg(color::BG_1)
                .border_color(color::LINE)
                .overflow_hidden()
                .label(label.clone());
            match edge {
                Edge::Left => col.border_r(px(HAIRLINE)),
                Edge::Right => col.border_l(px(HAIRLINE)),
            }
            .child(render())
        })
    })
}
