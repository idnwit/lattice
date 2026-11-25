//! Ports tool: forwards of the session, each with a toggle.

use nsh_core::prelude::*;

use super::super::components::{ButtonKind, button, toggle};
use super::super::style::{HAIRLINE, body, hrow, meta, mono};
use super::super::tokens::{color, row, space, state};
use super::Desk;
use crate::model::PortRow;

/// Port row height (canvas: 40).
const PORT_ROW: f32 = row::ROW_KEY;

/// Header, port rows, footer. Rows come from state.
pub(super) fn ports(desk: Desk) -> View {
    view()
        .child(dynamic(move || header(desk)))
        .child(dynamic(move || {
            desk.ports
                .get()
                .iter()
                .enumerate()
                .map(|(i, p)| port_row(p).on_press(move |_: &PressEvent| desk.toggle_port(i)))
                .collect::<Vec<_>>()
        }))
        .child(
            hrow(0.0)
                .h(px(row::ROW_MD))
                .px(px(space::SP_3))
                .child(body("+ Forward port…  ⌘⇧P", color::FG_MUTED)),
        )
}

fn header(desk: Desk) -> View {
    let total = desk.ports.get().len();
    hrow(space::SP_2)
        .h(px(row::ROW_SM + space::SP_2))
        .px(px(space::SP_3))
        .border_b(px(HAIRLINE))
        .border_color(color::LINE)
        .child(mono("api-server", color::FG))
        .child(meta(format!("{}/{total} on", desk.live_ports())))
        .child(view().flex_1())
        .child(
            button(ButtonKind::Secondary, "Enable all", None, row::ROW_XS)
                .on_press(move |_: &PressEvent| desk.set_all_ports(true)),
        )
        .child(
            button(ButtonKind::Ghost, "Disable all", None, row::ROW_XS)
                .on_press(move |_: &PressEvent| desk.set_all_ports(false)),
        )
}

/// `:remote → :local` over `label · meta`, state text, toggle.
fn port_row(p: &PortRow) -> View {
    let on = p.on && !p.conflict;
    let (word, hue) = match (p.conflict, on) {
        (true, _) => ("conflict", state::ERROR),
        (false, true) => ("live", state::ONLINE),
        (false, false) => ("off", color::FG_DIM),
    };
    hrow(space::SP_3)
        .h(px(PORT_ROW))
        .px(px(space::SP_3))
        .border_b(px(HAIRLINE))
        .border_color(color::LINE_SUBTLE)
        .child(
            view()
                .flex_1()
                .min_w(px(0.0))
                .child(mono(
                    format!(":{} → :{}", p.remote, p.local),
                    if on { color::FG } else { color::FG_MUTED },
                ))
                .child(meta(format!("{} · {}", p.label, p.meta)).truncate()),
        )
        .child(meta(word).text_color(hue))
        .child(toggle(
            on,
            !p.conflict,
            format!("Forward port {}", p.remote),
        ))
}
