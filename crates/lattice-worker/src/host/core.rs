//! The core task: apps runner, proc client, and deck sessions, driven one event at a time.

use std::collections::BTreeMap;

use lattice_app::{AppError, AppsAction, InstanceRecord, PackageHash};
use lattice_key::NodeId;
use lattice_proc_ipc::Reply;
use lattice_sync_proto::{ActionId, Message, StreamId, StreamInfo, StreamKind};
use tokio::sync::mpsc;
use tokio::sync::mpsc::error::TrySendError;

use super::conn::{ConnId, Event};
use crate::apps::{AppsEffect, AppsRunner, PackageLoader, StateSync};
use crate::error::WorkerError;
use crate::proc_client::ProcClient;
use crate::session::{NO_STREAM, PeerSession, SessionEffect, UNSUPPORTED};
use crate::streams::WorkerStream;
use crate::wire;

/// Schema name of `worker/apps` in `Streams` (proposed).
const APPS_SCHEMA: &str = "lattice.apps";

/// One connected deck.
struct Peer {
    session: PeerSession,
    outbox: mpsc::Sender<Message>,
}

/// A `create` waiting for its package (06-apps.md, Distribution rule 6).
struct Waiting {
    conn: ConnId,
    id: ActionId,
    action: AppsAction,
}

/// Creates waiting on one package, and the deck asked for it.
struct Fetch {
    asked: ConnId,
    waiting: Vec<Waiting>,
}

/// `ActionResult` code: the fetched package failed to verify or load.
const PACKAGE_UNAVAILABLE: &str = "worker.package_unavailable";

/// Owns all worker state; see [`super`].
pub(super) struct Core {
    worker: NodeId,
    apps: AppsRunner,
    proc: ProcClient,
    loader: PackageLoader,
    peers: BTreeMap<ConnId, Peer>,
    fetches: BTreeMap<PackageHash, Fetch>,
}

impl Core {
    pub(super) fn new(
        worker: NodeId,
        apps: AppsRunner,
        proc: ProcClient,
        loader: PackageLoader,
    ) -> Self {
        Self {
            worker,
            apps,
            proc,
            loader,
            peers: BTreeMap::new(),
            fetches: BTreeMap::new(),
        }
    }

    /// Next reply of the proc daemon. Cancel-safe.
    pub(super) async fn next_reply(&mut self) -> Result<Reply, WorkerError> {
        self.proc.next_reply().await
    }

    pub(super) async fn on_reply(&mut self, reply: Reply) -> Result<(), WorkerError> {
        let effects = self.apps.on_proc_reply(reply);
        self.run_effects(effects).await
    }

    /// One deck event. `None`: every sender is gone, which cannot happen while the accept task lives.
    pub(super) async fn on_event(&mut self, ev: Option<Event>) -> Result<(), WorkerError> {
        match ev {
            None => {}
            Some(Event::Open { conn, peer, outbox }) => {
                tracing::debug!(conn, ?peer, "session open");
                let session = PeerSession::new(self.worker);
                self.peers.insert(conn, Peer { session, outbox });
            }
            Some(Event::Closed { conn }) => {
                self.peers.remove(&conn);
                self.forget_fetches(conn);
            }
            Some(Event::Unsupported { conn }) => self.send(conn, error(UNSUPPORTED)),
            Some(Event::Message { conn, msg }) => {
                let Some(peer) = self.peers.get_mut(&conn) else {
                    return Ok(());
                };
                for effect in peer.session.on_message(msg) {
                    self.on_session_effect(conn, effect).await?;
                }
            }
        }
        Ok(())
    }

    async fn on_session_effect(
        &mut self,
        conn: ConnId,
        effect: SessionEffect,
    ) -> Result<(), WorkerError> {
        match effect {
            SessionEffect::Send(msg) => self.send(conn, msg),
            SessionEffect::Apps(id, action) => match missing_package(&self.apps, &action) {
                Some(hash) => self.fetch(conn, id, action, hash),
                None => self.apps_action(conn, id, action).await?,
            },
            SessionEffect::Blob(hash, bytes) => self.on_blob(conn, hash, &bytes).await?,
            SessionEffect::Instance(id, iid, payload) => {
                let (effects, res) = self.apps.on_instance_action(iid, &payload);
                self.run_effects(effects).await?;
                self.send(conn, action_result(id, res));
            }
            SessionEffect::Resume(stream, have) => self.resume(conn, stream, have),
            SessionEffect::List(prefix) => {
                let streams = self.streams(prefix.as_ref());
                self.send(conn, Message::Streams(streams));
            }
        }
        Ok(())
    }

    async fn apps_action(
        &mut self,
        conn: ConnId,
        id: ActionId,
        action: AppsAction,
    ) -> Result<(), WorkerError> {
        let (effects, res) = self.apps.on_apps_action(action);
        self.run_effects(effects).await?;
        let result = res
            .map(|iid| iid.and_then(|i| lattice_state::encode(&i).ok()))
            .map(Option::unwrap_or_default)
            .map_err(|code| AppError::platform(&code));
        self.send(conn, action_result(id, result));
        Ok(())
    }

    /// Parks a `create` until its package arrives; asks the deck for it once.
    fn fetch(&mut self, conn: ConnId, id: ActionId, action: AppsAction, hash: PackageHash) {
        let waiting = Waiting { conn, id, action };
        if let Some(fetch) = self.fetches.get_mut(&hash) {
            fetch.waiting.push(waiting);
            return;
        }
        tracing::info!(%hash, conn, "fetching package from the deck");
        self.fetches.insert(
            hash,
            Fetch {
                asked: conn,
                waiting: vec![waiting],
            },
        );
        self.send(
            conn,
            Message::BlobRequest {
                hash: *hash.as_bytes(),
            },
        );
    }

    /// A package arrived: verify, load, install, then run the creates that waited for it.
    /// A blob nobody asked for is dropped.
    async fn on_blob(
        &mut self,
        conn: ConnId,
        hash: PackageHash,
        bytes: &[u8],
    ) -> Result<(), WorkerError> {
        let Some(fetch) = self.fetches.remove(&hash) else {
            tracing::debug!(%hash, conn, "unrequested blob dropped");
            return Ok(());
        };
        let loaded = if lattice_package::hash(bytes) == hash {
            (self.loader)(&hash, bytes)
        } else {
            Err("package.hash_mismatch".to_owned())
        };
        match loaded {
            Ok(installed) => {
                self.apps.install(hash, installed);
                for w in fetch.waiting {
                    self.apps_action(w.conn, w.id, w.action).await?;
                }
            }
            Err(code) => {
                tracing::warn!(%hash, code, "fetched package refused");
                for w in fetch.waiting {
                    let err = AppError::platform(PACKAGE_UNAVAILABLE);
                    self.send(w.conn, action_result(w.id, Err(err)));
                }
            }
        }
        Ok(())
    }

    /// A deck left: drop its waiting creates, and ask another waiting deck for a package
    /// this one was asked for.
    fn forget_fetches(&mut self, conn: ConnId) {
        let mut ask = Vec::new();
        self.fetches.retain(|hash, fetch| {
            fetch.waiting.retain(|w| w.conn != conn);
            let Some(next) = fetch.waiting.first().map(|w| w.conn) else {
                return false;
            };
            if fetch.asked == conn {
                fetch.asked = next;
                ask.push((next, *hash));
            }
            true
        });
        for (next, hash) in ask {
            self.send(
                next,
                Message::BlobRequest {
                    hash: *hash.as_bytes(),
                },
            );
        }
    }

    /// Runs apps effects in order: proc requests, then fan-out to subscribed decks.
    pub(super) async fn run_effects(
        &mut self,
        effects: Vec<AppsEffect>,
    ) -> Result<(), WorkerError> {
        for effect in effects {
            match effect {
                AppsEffect::Proc(req) => self.proc.send(&req).await?,
                AppsEffect::Delta {
                    iid,
                    version,
                    bytes,
                } => {
                    let stream = WorkerStream::State(iid);
                    let msg = wire::delta(stream.to_stream_id(), self.worker, version, bytes);
                    match msg {
                        Some(msg) => self.fan_out(stream, &msg),
                        None => tracing::error!(%iid, "delta does not map to a change"),
                    }
                }
                AppsEffect::AppsChanged { version, bytes } => {
                    let stream = WorkerStream::Apps;
                    let msg = wire::snapshot(stream.to_stream_id(), self.worker, version, bytes);
                    self.fan_out(stream, &msg);
                }
            }
        }
        Ok(())
    }

    /// Sends a deck what it lacks of `stream` (06-apps.md, Sync to viewers rules 4–5).
    fn resume(&mut self, conn: ConnId, stream: WorkerStream, have: Option<u64>) {
        let id = stream.to_stream_id();
        match stream {
            WorkerStream::Apps => {
                let (version, bytes) = self.apps.apps_state();
                if have != Some(version) {
                    self.send(conn, wire::snapshot(id, self.worker, version, bytes));
                }
            }
            WorkerStream::State(iid) => match self.apps.state_sync(iid, have) {
                None => self.send(conn, error(NO_STREAM)),
                Some(StateSync::Full { version, bytes }) => {
                    self.send(conn, wire::snapshot(id, self.worker, version, bytes));
                }
                Some(StateSync::Deltas(deltas)) => {
                    for (version, bytes) in deltas {
                        if let Some(msg) = wire::delta(id.clone(), self.worker, version, bytes) {
                            self.send(conn, msg);
                        }
                    }
                }
            },
        }
    }

    /// `Streams` reply: `worker/apps` plus one state stream per instance, under `prefix`.
    fn streams(&self, prefix: Option<&StreamId>) -> Vec<StreamInfo> {
        let apps = (WorkerStream::Apps, APPS_SCHEMA.to_owned());
        let records = self.apps.records();
        let states = records
            .iter()
            .map(|r: &InstanceRecord| (WorkerStream::State(r.iid()), r.app().as_str().to_owned()));
        std::iter::once(apps)
            .chain(states)
            .map(|(s, schema)| (s.to_stream_id(), schema))
            .filter(|(id, _)| prefix.is_none_or(|p| under(id, p)))
            .map(|(id, schema)| StreamInfo::new(id, self.worker, StreamKind::State, schema))
            .collect()
    }

    /// Sends to every deck subscribed to `stream`.
    fn fan_out(&mut self, stream: WorkerStream, msg: &Message) {
        let readers: Vec<ConnId> = self
            .peers
            .iter()
            .filter(|(_, p)| p.session.is_subscribed(&stream))
            .map(|(c, _)| *c)
            .collect();
        for conn in readers {
            self.send(conn, msg.clone());
        }
    }

    /// Queues one message to a deck. A full outbox disconnects that deck; it resubscribes.
    fn send(&mut self, conn: ConnId, msg: Message) {
        let Some(peer) = self.peers.get(&conn) else {
            return;
        };
        match peer.outbox.try_send(msg) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => {
                tracing::warn!(conn, "deck too slow; disconnecting");
                self.peers.remove(&conn);
            }
            Err(TrySendError::Closed(_)) => {
                self.peers.remove(&conn);
            }
        }
    }
}

/// The package a `create` names when it is not installed yet.
fn missing_package(apps: &AppsRunner, action: &AppsAction) -> Option<PackageHash> {
    match action {
        AppsAction::Create {
            package: Some(hash),
            ..
        } if !apps.has_package(hash) => Some(*hash),
        _ => None,
    }
}

/// True when `id` is `prefix` or below it.
fn under(id: &StreamId, prefix: &StreamId) -> bool {
    id.as_str()
        .strip_prefix(prefix.as_str())
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
}

fn error(code: &str) -> Message {
    Message::Error {
        code: code.to_owned(),
    }
}

/// `ActionResult`: the encoded result, or the error as `{code, detail}` (D38).
fn action_result(id: ActionId, res: Result<Vec<u8>, AppError>) -> Message {
    match res {
        Ok(result) => Message::ActionResult {
            id,
            error: None,
            result,
            detail: Vec::new(),
        },
        Err(e) => {
            let (code, detail) = e.into_parts();
            Message::ActionResult {
                id,
                error: Some(code),
                result: Vec::new(),
                detail,
            }
        }
    }
}
