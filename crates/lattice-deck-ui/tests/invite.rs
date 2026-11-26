//! Headless runs of the Add worker dialog (patterns.md › Add worker; specs/01-pairing.md, UI).

use std::error::Error;
use std::rc::Rc;
use std::time::Duration;

use lattice_deck_ui::components::{
    AddWorker, AddWorkerActions, RailItem, Status, add_worker_dialog, worker_rail,
};
use nsh_core::element::desktop::window as app_window;
use nsh_core::prelude::*;
use nsh_engine::{App, EngineConfig, Root};
use nsh_testing::Headless;

/// A rail and the dialog it opens; `Copy` counts, `New invite` shows when expired.
#[component]
fn DialogHost() -> impl IntoElement {
    app_window().id("main").title("Invite").content(host)
}

fn host() -> View {
    let open = signal(false);
    let copies = signal(0u32);
    let expired = signal(false);
    on_key("mod+e", move |_| expired.set(true));
    let rail = vec![RailItem {
        mono: "B1".to_owned(),
        status: Some(Status::Online),
        selected: true,
    }];
    let actions = AddWorkerActions {
        on_copy: Rc::new(move || copies.update(|n| *n += 1)),
        on_close: Rc::new(move || open.set(false)),
        on_renew: Rc::new(move || expired.set(false)),
    };
    view()
        .flex_1()
        .flex_row()
        .relative()
        .child(worker_rail(&rail, false, move || open.set(true)))
        .child(dynamic(move || text(format!("copied {}", copies.get()))))
        .child(dynamic(move || {
            open.get().then(|| {
                let expired = expired.get();
                let dialog = AddWorker {
                    command: Some("lattice add-deck lattice1:abc".to_owned()),
                    status: if expired {
                        Status::Offline
                    } else {
                        Status::Waiting
                    },
                    status_text: if expired {
                        "expired"
                    } else {
                        "waiting for worker…"
                    }
                    .to_owned(),
                    renew: expired,
                };
                add_worker_dialog(&dialog, &actions)
            })
        }))
        .child(text("mod+e expires"))
}

fn start() -> Result<Headless, Box<dyn Error>> {
    let mut config = EngineConfig::new(App::Rust(Root::of::<DialogHost>()));
    config.viewport = nsh_core::Size::new(px(1440.0), px(900.0));
    let mut app = Headless::start(config)?;
    app.first_render()?;
    Ok(app)
}

#[test]
fn plus_on_the_rail_opens_add_worker_and_escape_closes_it() -> Result<(), Box<dyn Error>> {
    let mut app = start()?;
    app.wait_for("role:tab[B1]")?;
    app.press("role:button[Add worker]")?;
    app.wait_for("role:dialog[Add worker]")?;
    app.wait_for("text:Run this on the worker:")?;
    app.wait_for("text:waiting for worker…")?;
    app.wait_for("text:Expires in 5 min. One use.")?;
    app.press("role:button[Copy]")?;
    app.wait_for("text:copied 1")?;
    app.press_key("escape")?;
    app.wait_for_gone("role:dialog[Add worker]", Duration::from_secs(2))?;
    Ok(())
}

#[test]
fn new_invite_shows_only_once_the_invite_expired() -> Result<(), Box<dyn Error>> {
    let mut app = start()?;
    app.press("role:button[Add worker]")?;
    app.wait_for("role:dialog[Add worker]")?;
    assert!(
        app.wait_for_gone("role:button[New invite]", Duration::from_millis(200))
            .is_ok()
    );
    app.press_key("mod+e")?;
    app.wait_for("text:expired")?;
    app.press("role:button[New invite]")?;
    app.wait_for("text:waiting for worker…")?;
    app.press("role:button[Close]")?;
    app.wait_for_gone("role:dialog[Add worker]", Duration::from_secs(2))?;
    Ok(())
}
