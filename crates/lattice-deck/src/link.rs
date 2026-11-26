//! The link to the local worker (experiment, M1): instances and their viewers.
//!
//! Started only with `LATTICE_DECK_LIVE=1`, reading `$TMPDIR/lattice/worker.addr`
//! (written by `latticed`). One task on the NSH engine thread owns the QUIC link and every
//! viewer:
//!
//! - `Create`: sends `create` with the built-in package (empty params: the app's defaults)
//!   and answers the worker's `BlobRequest` with the package bytes (D44). The `ActionResult`
//!   gives the instance id: the pane goes live, its viewer starts from the package's
//!   `viewer.wasm`, and the deck subscribes to `inst/<iid>/state`.
//! - State updates feed the viewer (06-apps.md, Sync to viewers); each change renders a new
//!   view tree into the `trees` signal. A `Resync` resubscribes without a version.
//! - `Event`: the viewer turns a UI event into actions, sent to the instance (fire and forget).
//! - `Close`: sends `close{iid}`. `worker/apps` lists the live instances; a pane whose
//!   instance left it goes away ([`Workspace::instances`]).
//!
//! The deck never sees PTY bytes: only state and deltas.

use std::collections::BTreeMap;
use std::io::Read;

use lattice_app::view::{DynViewer, UiEvent, ViewTree};
use lattice_app::{AppId, AppsAction, InstanceId, InstanceRecord, ReplicaEvent};
use lattice_builtin_apps::Builtin;
use lattice_deck_core::{DeckHost, PaneId, StateUpdate, WorkerLink};
use lattice_key::{NodeId, NodeKey, SecretKeyBytes};
use lattice_quic::{QuicConfig, QuicConn, QuicTransport};
use lattice_sync_proto::{ActionId, Message, StreamId};
use lattice_wasm::{Engine, Limits, WasmViewer, WasmViewerModule};
use nsh_core::prelude::*;
use tokio::sync::mpsc;

use crate::workspace::Workspace;

/// What the views ask the link to do.
#[derive(Debug)]
pub(crate) enum Cmd {
    /// Create an instance of `app` for `pane`.
    Create { pane: PaneId, app: &'static Builtin },
    /// A UI event for the viewer of `iid`.
    Event { iid: u64, event: UiEvent },
    /// Close instance `iid`.
    Close { iid: u64 },
}

/// Signals the link writes and the views read. `Copy`.
#[derive(Clone, Copy)]
pub(crate) struct Shared {
    pub(crate) ws: Signal<Workspace>,
    pub(crate) trees: Signal<BTreeMap<u64, ViewTree>>,
    /// `connecting`, `direct · 3ms`, `no worker`, or an error code.
    pub(crate) status: Signal<String>,
}

/// Starts the link if a local worker announced itself. `None`: no worker; picks fail.
pub(crate) fn start(shared: Shared) -> Option<mpsc::UnboundedSender<Cmd>> {
    if std::env::var("LATTICE_DECK_LIVE").ok().as_deref() != Some("1") {
        return None;
    }
    let card = std::fs::read_to_string(std::env::temp_dir().join("lattice/worker.addr")).ok()?;
    let (worker, addr) = parse_card(&card)?;
    let (tx, rx) = mpsc::unbounded_channel();
    shared.status.set("connecting".to_owned());
    spawn(async move {
        if let Err(code) = run(worker, addr, rx, shared).await {
            shared.status.set(code);
        }
    });
    Some(tx)
}

/// The link task's state.
struct Task {
    link: WorkerLink<QuicConn>,
    shared: Shared,
    engine: Engine,
    /// Compiled viewer modules, by package.
    modules: BTreeMap<[u8; 32], WasmViewerModule>,
    /// One viewer per live instance.
    viewers: BTreeMap<u64, WasmViewer>,
    /// Creates in flight: action id → (pane, app).
    creating: BTreeMap<ActionId, (PaneId, &'static Builtin)>,
}

async fn run(
    worker: NodeId,
    addr: std::net::SocketAddr,
    mut cmds: mpsc::UnboundedReceiver<Cmd>,
    shared: Shared,
) -> Result<(), String> {
    let key = random_key().ok_or("deck.key")?;
    let bind = "127.0.0.1:0".parse().map_err(|_| "deck.bind")?;
    let cfg = QuicConfig::direct(key, bind).map_err(|e| e.code().to_owned())?;
    let transport = QuicTransport::bind(cfg)
        .await
        .map_err(|e| e.code().to_owned())?;
    transport.add_peer(worker, addr);
    let link = DeckHost::new(transport)
        .open_link(worker)
        .await
        .map_err(|e| e.code().to_owned())?;
    shared.status.set(link.path().to_string());
    let engine = lattice_wasm::engine().map_err(|e| e.code().to_owned())?;
    let mut task = Task {
        link,
        shared,
        engine,
        modules: BTreeMap::new(),
        viewers: BTreeMap::new(),
        creating: BTreeMap::new(),
    };
    task.link
        .subscribe(stream("worker/apps")?)
        .await
        .map_err(code)?;
    loop {
        tokio::select! {
            msg = task.link.next_message() => task.on_message(msg.map_err(code)?).await?,
            cmd = cmds.recv() => match cmd {
                Some(cmd) => task.on_cmd(cmd).await?,
                None => return Ok(()),
            },
        }
    }
}

impl Task {
    async fn on_cmd(&mut self, cmd: Cmd) -> Result<(), String> {
        match cmd {
            Cmd::Create { pane, app } => {
                let action = AppsAction::Create {
                    app: AppId::try_new(app.id).map_err(|e| e.code())?,
                    package: Some(app.package()),
                    params: Vec::new(),
                    session: "deck".to_owned(),
                };
                let id = self.link.apps_action(action).await.map_err(code)?;
                self.creating.insert(id, (pane, app));
            }
            Cmd::Event { iid, event } => {
                let actions = match self.viewers.get_mut(&iid) {
                    Some(v) => v.handle(&event),
                    None => Vec::new(),
                };
                for payload in actions {
                    let iid = InstanceId::from_raw(iid);
                    self.link
                        .instance_action(iid, payload)
                        .await
                        .map_err(code)?;
                }
            }
            Cmd::Close { iid } => {
                let iid = InstanceId::from_raw(iid);
                self.link
                    .apps_action(AppsAction::Close { iid })
                    .await
                    .map_err(code)?;
            }
        }
        Ok(())
    }

    async fn on_message(&mut self, msg: Message) -> Result<(), String> {
        match msg {
            Message::BlobRequest { hash } => {
                let pkg = lattice_app::PackageHash::from_bytes(hash);
                if let Some(app) = lattice_builtin_apps::by_hash(&pkg) {
                    self.link.send_blob(pkg, app.bytes).await.map_err(code)?;
                }
            }
            Message::ActionResult {
                id, error, result, ..
            } => {
                if let Some((pane, app)) = self.creating.remove(&id) {
                    self.on_created(pane, app, error, &result).await?;
                }
            }
            other => {
                let worker = self.link.worker();
                if let Some((stream, update)) = StateUpdate::from_message(other, &worker) {
                    self.on_update(&stream, &update).await?;
                }
            }
        }
        Ok(())
    }

    async fn on_created(
        &mut self,
        pane: PaneId,
        app: &'static Builtin,
        error: Option<String>,
        result: &[u8],
    ) -> Result<(), String> {
        let iid = match (error, lattice_state::decode::<InstanceId>(result)) {
            (None, Ok(iid)) => iid,
            (Some(e), _) => {
                self.shared.ws.update(|w| w.failed(pane, e));
                return Ok(());
            }
            (None, Err(_)) => {
                self.shared
                    .ws
                    .update(|w| w.failed(pane, "deck.iid".to_owned()));
                return Ok(());
            }
        };
        let viewer = match self.viewer(app) {
            Ok(viewer) => viewer,
            Err(e) => {
                // No viewer, no pane: the instance would run unseen.
                self.shared.ws.update(|w| w.failed(pane, e));
                let close = AppsAction::Close { iid };
                self.link.apps_action(close).await.map_err(code)?;
                return Ok(());
            }
        };
        if !self.update_ws(|w| w.created(pane, iid.raw())) {
            // The pane went away while creating: the instance has no one to show it.
            let close = AppsAction::Close { iid };
            self.link.apps_action(close).await.map_err(code)?;
            return Ok(());
        }
        self.viewers.insert(iid.raw(), viewer);
        let s = stream(&format!("inst/{iid}/state"))?;
        self.link.subscribe(s).await.map_err(code)?;
        Ok(())
    }

    fn update_ws(&self, f: impl FnOnce(&mut Workspace) -> bool) -> bool {
        let mut out = false;
        self.shared.ws.update(|w| out = f(w));
        out
    }

    /// A fresh viewer from the package's `viewer.wasm` (compiled once per package).
    fn viewer(&mut self, app: &'static Builtin) -> Result<WasmViewer, String> {
        if !self.modules.contains_key(&app.hash) {
            let package = lattice_package::Package::verify(&app.package(), app.bytes)
                .map_err(|e| e.code().to_owned())?;
            let module = WasmViewerModule::load(&self.engine, &package.viewer, Limits::default())
                .map_err(|e| e.code().to_owned())?;
            self.modules.insert(app.hash, module);
        }
        let module = self.modules.get(&app.hash).ok_or("deck.no_viewer")?;
        Ok(module.instantiate())
    }

    async fn on_update(
        &mut self,
        stream_id: &StreamId,
        update: &StateUpdate,
    ) -> Result<(), String> {
        if stream_id.as_str() == "worker/apps" {
            if let StateUpdate::Full { bytes, .. } = update {
                let alive: Vec<u64> = InstanceRecord::decode_list(bytes)
                    .unwrap_or_default()
                    .iter()
                    .map(|r| r.iid().raw())
                    .collect();
                self.viewers.retain(|iid, _| alive.contains(iid));
                self.shared.ws.update(|w| w.instances(&alive));
            }
            return Ok(());
        }
        let Some(iid) = instance_of(stream_id) else {
            return Ok(());
        };
        let Some(viewer) = self.viewers.get_mut(&iid) else {
            return Ok(());
        };
        let event = match update {
            StateUpdate::Full { version, bytes } => viewer.load(*version, bytes),
            StateUpdate::Delta { version, bytes } => viewer.apply(*version, bytes),
        };
        match event {
            ReplicaEvent::Changed => {
                let tree = viewer.render();
                self.shared.trees.update(|t| {
                    t.insert(iid, tree);
                });
            }
            ReplicaEvent::Resync => {
                tracing::warn!(iid, "viewer asks for a resync");
                if !viewer.is_dead() {
                    self.link.subscribe(stream_id.clone()).await.map_err(code)?;
                }
            }
        }
        Ok(())
    }
}

/// `inst/<iid>/state` → iid.
fn instance_of(stream: &StreamId) -> Option<u64> {
    stream
        .as_str()
        .strip_prefix("inst/")?
        .strip_suffix("/state")?
        .parse()
        .ok()
}

fn stream(text: &str) -> Result<StreamId, String> {
    text.parse().map_err(|_| "deck.stream_id".to_owned())
}

fn code(e: lattice_deck_core::DeckError) -> String {
    e.code().to_owned()
}

/// `<node id hex>@<addr>`. TBD: canonical text form of a NodeId.
fn parse_card(card: &str) -> Option<(NodeId, std::net::SocketAddr)> {
    let (id, addr) = card.trim().split_once('@')?;
    if id.len() != 64 {
        return None;
    }
    let mut bytes = [0u8; 32];
    for (i, b) in bytes.iter_mut().enumerate() {
        *b = u8::from_str_radix(id.get(i * 2..i * 2 + 2)?, 16).ok()?;
    }
    Some((NodeId::try_from_bytes(bytes).ok()?, addr.parse().ok()?))
}

/// A fresh deck key per run (experiment: the worker allows any loopback deck).
fn random_key() -> Option<NodeKey> {
    let mut bytes = [0u8; 32];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut r| r.read_exact(&mut bytes))
        .ok()?;
    Some(NodeKey::from_secret(SecretKeyBytes::new(bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instance_streams_name_their_iid() {
        let s: StreamId = "inst/42/state".parse().unwrap();
        assert_eq!(instance_of(&s), Some(42));
        let apps: StreamId = "worker/apps".parse().unwrap();
        assert_eq!(instance_of(&apps), None);
    }
}
