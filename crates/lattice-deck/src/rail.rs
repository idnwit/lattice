//! Worker views: the rail, the Add worker dialog, and the unpaired workspace
//! (specs/01-pairing.md, UI; patterns.md › Add worker).

use std::rc::Rc;

use lattice_deck_core::WorkerList;
use lattice_deck_ui::components::{
    AddWorkerActions, ButtonKind, add_worker_dialog, button, worker_rail,
};
use lattice_deck_ui::style::body;
use lattice_deck_ui::tokens::{color, row, space};
use nsh_core::prelude::*;

use crate::pairing::Pairing;
use crate::status::{LinkState, rail_items};

/// The worker rail: one entry per paired worker, `+` opens Add worker.
pub(crate) fn rail(workers: Signal<WorkerList>, link: Signal<LinkState>, pairing: Pairing) -> View {
    view().flex_none().child(dynamic(move || {
        let items = workers.with(|w| link.with(|l| rail_items(w, l)));
        // M1 has no agents: no waiting badge.
        worker_rail(&items, false, move || pairing.open()).flex_1()
    }))
}

/// The Add worker dialog while it shows.
pub(crate) fn dialog(pairing: Pairing) -> AnyElement {
    let actions = AddWorkerActions {
        on_copy: Rc::new(move || pairing.copy()),
        on_close: Rc::new(move || pairing.close()),
        on_renew: Rc::new(move || pairing.renew()),
    };
    dynamic(move || {
        pairing
            .dialog
            .with(|d| d.shown().then(|| add_worker_dialog(&d.view(), &actions)))
    })
}

/// The workspace with no paired worker (specs/01-pairing.md, UI rule 8).
pub(crate) fn no_worker(pairing: Pairing) -> View {
    view()
        .flex_1()
        .items_center()
        .justify_center()
        .gap(px(space::SP_3))
        .bg(color::BG_0)
        .child(body("No worker yet.", color::FG_MUTED))
        .child(
            button(ButtonKind::Primary, "Add worker", None, row::ROW_MD)
                .on_press(move |_: &PressEvent| pairing.open()),
        )
}
