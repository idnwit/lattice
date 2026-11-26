//! Ports tool (right panel): forwards of a session, each with a toggle.

use std::rc::Rc;

use nsh_core::prelude::*;

use super::controls::{ButtonKind, button, toggle};
use crate::style::{HAIRLINE, body, hrow, meta, mono};
use crate::tokens::{color, row, space, state};

/// Port row height (canvas: 40).
const PORT_ROW: f32 = row::ROW_KEY;

/// A port forward.
#[derive(Debug, Clone, PartialEq)]
pub struct PortRow {
    /// Port on the worker.
    pub remote: u16,
    /// Port on this device.
    pub local: u16,
    /// What listens (`vite dev`).
    pub label: String,
    /// Meta (`auto-detected`).
    pub meta: String,
    /// The forward is enabled.
    pub on: bool,
    /// The local port is taken: stays off.
    pub conflict: bool,
}

/// Enabled, non-conflicting forwards.
pub fn live_ports(ports: &[PortRow]) -> usize {
    ports.iter().filter(|p| p.on && !p.conflict).count()
}

/// Props of [`ports_tool`].
pub struct PortsTool {
    /// Session name in the header.
    pub session: String,
    /// The forwards.
    pub ports: Reactive<Vec<PortRow>>,
    /// Hears a row press, with the row index.
    pub on_toggle: Rc<dyn Fn(usize)>,
    /// Hears `Enable all` (`true`) and `Disable all` (`false`).
    pub on_set_all: Rc<dyn Fn(bool)>,
}

/// Header, port rows, footer.
pub fn ports_tool(props: PortsTool) -> View {
    let PortsTool {
        session,
        ports,
        on_toggle,
        on_set_all,
    } = props;
    let rows = ports.clone();
    view()
        .child(dynamic(move || {
            header(&session, &ports.get(), on_set_all.clone())
        }))
        .child(dynamic(move || {
            rows.get()
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    let on_toggle = on_toggle.clone();
                    port_row(p).on_press(move |_: &PressEvent| on_toggle(i))
                })
                .collect::<Vec<_>>()
        }))
        .child(
            hrow(0.0)
                .h(px(row::ROW_MD))
                .px(px(space::SP_3))
                .child(body("+ Forward port…  ⌘⇧P", color::FG_MUTED)),
        )
}

fn header(session: &str, ports: &[PortRow], on_set_all: Rc<dyn Fn(bool)>) -> View {
    let off = on_set_all.clone();
    hrow(space::SP_2)
        .h(px(row::ROW_SM + space::SP_2))
        .px(px(space::SP_3))
        .border_b(px(HAIRLINE))
        .border_color(color::LINE)
        .child(mono(session.to_owned(), color::FG))
        .child(meta(format!("{}/{} on", live_ports(ports), ports.len())))
        .child(view().flex_1())
        .child(
            button(ButtonKind::Secondary, "Enable all", None, row::ROW_XS)
                .on_press(move |_: &PressEvent| on_set_all(true)),
        )
        .child(
            button(ButtonKind::Ghost, "Disable all", None, row::ROW_XS)
                .on_press(move |_: &PressEvent| off(false)),
        )
}

/// Port row: `:remote → :local` over `label · meta`, state word, toggle.
pub(crate) fn port_row(p: &PortRow) -> View {
    let on = p.on && !p.conflict;
    // State = word + hue: `conflict`, `live`, `off`.
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
