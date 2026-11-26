//! The link to the active worker (specs/01-pairing.md, UI; specs/02-connectivity.md, Link).
//!
//! The active worker is the most recently paired one in `<data dir>/deck/workers` (M1). One
//! task on the NSH engine thread owns the QUIC link and the instances ([`Instances`]). It
//! dials with the deck key from `deck/node.key`, from a socket on `0.0.0.0:0`, and shows its
//! state in the `link` signal ([`LinkState`]). `Cmd::Worker` switches it to a newly paired
//! worker.
//!
//! A link that ends is dialed again (specs/02-connectivity.md, Reconnect): waits from
//! [`Backoff`], each dial bounded by [`CONNECT_TIMEOUT`] on the NSH timer. The viewers stay;
//! a new link resubscribes each at the version it holds ([`Instances::linked`]).

use std::collections::BTreeMap;
use std::net::{Ipv4Addr, SocketAddr};

use lattice_app::view::ViewTree;
use lattice_deck_core::{Backoff, CONNECT_TIMEOUT, DeckFiles, DeckHost, PairedWorker, WorkerLink};
use lattice_quic::{QuicConfig, QuicConn, QuicTransport};
use nsh_core::prelude::*;
use tokio::sync::mpsc;

use crate::instances::{InstanceCmd, Instances};
use crate::status::LinkState;
use crate::workspace::Workspace;

/// What the views ask the link task to do.
#[derive(Debug)]
pub(crate) enum Cmd {
    /// A pane's instance command.
    Instance(InstanceCmd),
    /// Link to this newly paired worker instead.
    Worker(PairedWorker),
}

/// Signals the link writes and the views read. `Copy`.
#[derive(Clone, Copy)]
pub(crate) struct Shared {
    pub(crate) ws: Signal<Workspace>,
    pub(crate) trees: Signal<BTreeMap<u64, ViewTree>>,
    /// The active worker's link.
    pub(crate) link: Signal<LinkState>,
}

/// Starts the link task toward `worker` (none: wait for a pairing). Commands sent after
/// the task ended are dropped.
pub(crate) fn start(
    shared: Shared,
    files: Option<DeckFiles>,
    worker: Option<PairedWorker>,
) -> mpsc::UnboundedSender<Cmd> {
    let (tx, rx) = mpsc::unbounded_channel();
    spawn(async move {
        match lattice_wasm::engine() {
            Ok(engine) => run(Instances::new(shared, engine), files, worker, rx).await,
            Err(e) => {
                tracing::warn!(
                    code = e.code(),
                    "no wasm engine: the deck runs without a link"
                );
            }
        }
    });
    tx
}

/// How one phase of the link task ended.
enum Next {
    /// The link to the worker opened.
    Linked(WorkerLink<QuicConn>),
    /// The link ended or did not open; the code says why.
    Lost(String),
    /// Link to this worker instead.
    Switch(PairedWorker),
    /// The views are gone.
    Quit,
}

async fn run(
    mut inst: Instances,
    files: Option<DeckFiles>,
    mut worker: Option<PairedWorker>,
    mut cmds: mpsc::UnboundedReceiver<Cmd>,
) {
    let mut backoff = Backoff::default();
    loop {
        let Some(target) = worker.clone() else {
            inst.set_link(LinkState::NoWorker);
            match wait(&mut inst, &mut cmds).await {
                Next::Switch(w) => worker = Some(w),
                _ => return,
            }
            continue;
        };
        let name = target.name.clone();
        // While retrying, the rail keeps showing the loss (Link states: lost).
        if backoff == Backoff::default() {
            inst.set_link(LinkState::Connecting {
                worker: name.clone(),
            });
        }
        let mut next = dial(&mut inst, files.as_ref(), &target, &mut cmds).await;
        if let Next::Linked(link) = next {
            next = session(&mut inst, &mut cmds, link, &name, &mut backoff).await;
        }
        if let Next::Lost(code) = next {
            tracing::info!(code, "worker link lost; dialing again");
            inst.unlinked();
            inst.set_link(LinkState::Lost { worker: name, code });
            next = pause(&mut inst, &mut cmds, backoff.next_delay()).await;
        }
        match next {
            Next::Switch(w) => {
                inst.forget();
                backoff.reset();
                worker = Some(w);
            }
            Next::Lost(_) => {}
            Next::Quit | Next::Linked(_) => return,
        }
    }
}

/// A link opened: resubscribe, then serve it until it ends.
async fn session(
    inst: &mut Instances,
    cmds: &mut mpsc::UnboundedReceiver<Cmd>,
    link: WorkerLink<QuicConn>,
    name: &str,
    backoff: &mut Backoff,
) -> Next {
    let path = link.path();
    if let Err(code) = inst.linked(link).await {
        return Next::Lost(code);
    }
    backoff.reset();
    inst.set_link(LinkState::Up {
        worker: name.to_owned(),
        path,
    });
    serve(inst, cmds).await
}

/// Waits `delay` before the next dial while serving commands offline. `Lost` when it is time.
async fn pause(
    inst: &mut Instances,
    cmds: &mut mpsc::UnboundedReceiver<Cmd>,
    delay: std::time::Duration,
) -> Next {
    tokio::select! {
        next = wait(inst, cmds) => next,
        () = timer(delay) => Next::Lost(String::new()),
    }
}

/// Without a link: serve commands offline until a new worker or the end.
async fn wait(inst: &mut Instances, cmds: &mut mpsc::UnboundedReceiver<Cmd>) -> Next {
    loop {
        match cmds.recv().await {
            None => return Next::Quit,
            Some(Cmd::Worker(w)) => return Next::Switch(w),
            // No link: `on_cmd` takes the offline path, which cannot fail.
            Some(Cmd::Instance(cmd)) => drop(inst.on_cmd(cmd).await),
        }
    }
}

/// Dials `worker` while serving commands offline.
async fn dial(
    inst: &mut Instances,
    files: Option<&DeckFiles>,
    worker: &PairedWorker,
    cmds: &mut mpsc::UnboundedReceiver<Cmd>,
) -> Next {
    let open = connect(files, worker);
    tokio::pin!(open);
    let deadline = timer(CONNECT_TIMEOUT);
    tokio::pin!(deadline);
    loop {
        tokio::select! {
            opened = &mut open => return opened.map_or_else(Next::Lost, Next::Linked),
            () = &mut deadline => return Next::Lost("deck.connect_timeout".to_owned()),
            cmd = cmds.recv() => match cmd {
                None => return Next::Quit,
                Some(Cmd::Worker(w)) => return Next::Switch(w),
                // Not linked yet: the offline path, which cannot fail.
                Some(Cmd::Instance(cmd)) => drop(inst.on_cmd(cmd).await),
            },
        }
    }
}

/// While linked: messages from the worker, commands from the views.
async fn serve(inst: &mut Instances, cmds: &mut mpsc::UnboundedReceiver<Cmd>) -> Next {
    loop {
        let done = tokio::select! {
            msg = inst.next_message() => match msg {
                Ok(msg) => inst.on_message(msg).await,
                Err(code) => Err(code),
            },
            cmd = cmds.recv() => match cmd {
                None => return Next::Quit,
                Some(Cmd::Worker(w)) => return Next::Switch(w),
                Some(Cmd::Instance(cmd)) => inst.on_cmd(cmd).await,
            },
        };
        if let Err(code) = done {
            return Next::Lost(code);
        }
    }
}

/// Opens a link to `worker` with the deck key, from a fresh socket on `0.0.0.0:0`.
async fn connect(
    files: Option<&DeckFiles>,
    worker: &PairedWorker,
) -> Result<WorkerLink<QuicConn>, String> {
    let files = files.ok_or("deck.data_dir")?;
    let key = files.node_key().map_err(|_| "deck.key")?;
    let bind = SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), 0);
    let transport = QuicTransport::bind(QuicConfig::direct_lan(key, bind))
        .await
        .map_err(|e| e.code().to_owned())?;
    transport.add_peer(worker.id, worker.addr);
    DeckHost::new(transport)
        .open_link(worker.id)
        .await
        .map_err(|e| e.code().to_owned())
}
