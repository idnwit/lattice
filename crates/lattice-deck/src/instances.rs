//! Instances and their viewers over the worker link (06-apps.md, Sync to viewers; D42–D44).
//!
//! - `Create`: sends `create` with the built-in package (empty params: the app's defaults)
//!   and answers the worker's `BlobRequest` with the package bytes (D44). The `ActionResult`
//!   gives the instance id: the pane goes live, its viewer starts from the package's
//!   `viewer.wasm`, and the deck subscribes to `inst/<iid>/state`.
//! - State updates feed the viewer; each change renders a new view tree into the `trees`
//!   signal. A `Resync` resubscribes without a version.
//! - `Event`: the viewer turns a UI event into actions, sent to the instance (fire and forget).
//! - `Close`: sends `close{iid}`. `worker/apps` lists the live instances; a pane whose
//!   instance left it goes away ([`Workspace::instances`]).
//!
//! Viewers live here, not in the link: they outlast a link that ends. Without a link,
//! `create` fails with `deck.offline`, input is dropped, and closes wait for the next link.
//!
//! The deck never sees PTY bytes: only state and deltas.

use std::collections::BTreeMap;

use lattice_app::view::{DynViewer, UiEvent};
use lattice_app::{AppId, AppsAction, InstanceId, InstanceRecord, ReplicaEvent};
use lattice_builtin_apps::Builtin;
use lattice_deck_core::{PaneId, StateUpdate, WorkerLink};
use lattice_quic::QuicConn;
use lattice_sync_proto::{ActionId, Message, StreamId};
use lattice_wasm::{Engine, Limits, WasmViewer, WasmViewerModule};

use crate::link::Shared;
use crate::workspace::Workspace;

/// Pane code of a `create` sent while no link is up.
const OFFLINE: &str = "deck.offline";
/// Pane code of a `create` whose link ended before its result.
const LINK_LOST: &str = "deck.link_lost";

/// What the views ask of the instances.
#[derive(Debug)]
pub(crate) enum InstanceCmd {
    /// Create an instance of `app` for `pane`.
    Create { pane: PaneId, app: &'static Builtin },
    /// A UI event for the viewer of `iid`.
    Event { iid: u64, event: UiEvent },
    /// Close instance `iid`.
    Close { iid: u64 },
}

/// The viewers, and the link they talk over while it is up.
pub(crate) struct Instances {
    shared: Shared,
    engine: Engine,
    link: Option<WorkerLink<QuicConn>>,
    /// Compiled viewer modules, by package.
    modules: BTreeMap<[u8; 32], WasmViewerModule>,
    /// One viewer per live instance.
    viewers: BTreeMap<u64, WasmViewer>,
    /// Creates in flight: action id → (pane, app).
    creating: BTreeMap<ActionId, (PaneId, &'static Builtin)>,
    /// Closes asked while no link was up, sent on the next link.
    closes: Vec<u64>,
}

impl Instances {
    pub(crate) fn new(shared: Shared, engine: Engine) -> Self {
        Self {
            shared,
            engine,
            link: None,
            modules: BTreeMap::new(),
            viewers: BTreeMap::new(),
            creating: BTreeMap::new(),
            closes: Vec::new(),
        }
    }

    /// Shows the link's state.
    pub(crate) fn set_link(&self, state: crate::status::LinkState) {
        self.shared.link.set(state);
    }

    /// A link opened: subscribe to `worker/apps` and send the closes that waited.
    pub(crate) async fn linked(&mut self, link: WorkerLink<QuicConn>) -> Result<(), String> {
        self.link = Some(link);
        let apps = stream("worker/apps")?;
        self.link()?.subscribe(apps).await.map_err(code)?;
        for iid in std::mem::take(&mut self.closes) {
            let close = AppsAction::Close {
                iid: InstanceId::from_raw(iid),
            };
            self.link()?.apps_action(close).await.map_err(code)?;
        }
        Ok(())
    }

    /// The link ended: creates in flight fail; viewers stay for the next link.
    pub(crate) fn unlinked(&mut self) {
        self.link = None;
        for (_, (pane, _)) in std::mem::take(&mut self.creating) {
            self.shared
                .ws
                .update(|w| w.failed(pane, LINK_LOST.to_owned()));
        }
    }

    /// Another worker becomes the active one: the old worker's instances leave the panes.
    pub(crate) fn forget(&mut self) {
        self.unlinked();
        self.viewers.clear();
        self.closes.clear();
        self.shared.trees.update(BTreeMap::clear);
        self.shared.ws.update(|w| w.instances(&[]));
    }

    /// Next message on the link; never resolves without one.
    pub(crate) async fn next_message(&mut self) -> Result<Message, String> {
        match self.link.as_mut() {
            Some(link) => link.next_message().await.map_err(code),
            None => std::future::pending().await,
        }
    }

    fn link(&mut self) -> Result<&mut WorkerLink<QuicConn>, String> {
        self.link.as_mut().ok_or_else(|| OFFLINE.to_owned())
    }

    /// A view's command. `Err` only when the link broke while sending.
    pub(crate) async fn on_cmd(&mut self, cmd: InstanceCmd) -> Result<(), String> {
        if self.link.is_none() {
            self.offline(cmd);
            return Ok(());
        }
        match cmd {
            InstanceCmd::Create { pane, app } => {
                let action = AppsAction::Create {
                    app: AppId::try_new(app.id).map_err(|e| e.code())?,
                    package: Some(app.package()),
                    params: Vec::new(),
                    session: "deck".to_owned(),
                };
                let id = self.link()?.apps_action(action).await.map_err(code)?;
                self.creating.insert(id, (pane, app));
            }
            InstanceCmd::Event { iid, event } => {
                let actions = match self.viewers.get_mut(&iid) {
                    Some(v) => v.handle(&event),
                    None => Vec::new(),
                };
                for payload in actions {
                    let iid = InstanceId::from_raw(iid);
                    let link = self.link()?;
                    link.instance_action(iid, payload).await.map_err(code)?;
                }
            }
            InstanceCmd::Close { iid } => {
                let iid = InstanceId::from_raw(iid);
                let close = AppsAction::Close { iid };
                self.link()?.apps_action(close).await.map_err(code)?;
            }
        }
        Ok(())
    }

    fn offline(&mut self, cmd: InstanceCmd) {
        match cmd {
            InstanceCmd::Create { pane, .. } => {
                self.shared
                    .ws
                    .update(|w| w.failed(pane, OFFLINE.to_owned()));
            }
            InstanceCmd::Event { iid, .. } => {
                tracing::debug!(iid, "input dropped: no link");
            }
            InstanceCmd::Close { iid } => self.closes.push(iid),
        }
    }

    /// A message from the worker. `Err` only when the link broke while answering.
    pub(crate) async fn on_message(&mut self, msg: Message) -> Result<(), String> {
        match msg {
            Message::BlobRequest { hash } => {
                let pkg = lattice_app::PackageHash::from_bytes(hash);
                if let Some(app) = lattice_builtin_apps::by_hash(&pkg) {
                    self.link()?.send_blob(pkg, app.bytes).await.map_err(code)?;
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
                let worker = self.link()?.worker();
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
                self.link()?.apps_action(close).await.map_err(code)?;
                return Ok(());
            }
        };
        if !self.update_ws(|w| w.created(pane, iid.raw())) {
            // The pane went away while creating: the instance has no one to show it.
            let close = AppsAction::Close { iid };
            self.link()?.apps_action(close).await.map_err(code)?;
            return Ok(());
        }
        self.viewers.insert(iid.raw(), viewer);
        let s = stream(&format!("inst/{iid}/state"))?;
        self.link()?.subscribe(s).await.map_err(code)?;
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
                    self.link()?
                        .subscribe(stream_id.clone())
                        .await
                        .map_err(code)?;
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
