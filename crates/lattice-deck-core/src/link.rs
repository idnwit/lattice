//! Links from the deck to workers.
//!
//! M1: one bi stream per link (the control stream, opened by the deck) carries every
//! message, subscriptions and actions included. Per-stream channels (01-transport.md,
//! Channel layout) come later.
//!
//! Typed app calls: [`WorkerLink::caller`] gives a [`LinkCaller`] for a generated client;
//! [`WorkerLink::flush_calls`] sends what it queued; [`WorkerLink::next_message`] settles
//! tickets from `ActionResult`s. When the link ends, tickets in flight settle `Unknown`.

use lattice_app::{AppId, AppsAction, InstanceId, PackageHash};
use lattice_key::NodeId;
use lattice_sync_proto::{
    ActionId, Decoded, FrameDecoder, Have, Message, PROTOCOL_VERSION, Seq, StreamId, SubscribeMode,
    encode,
};
use lattice_transport::{Conn, PathInfo, RecvStream, SendStream, Transport, TransportError};

use crate::calls::{CallQueue, LinkCaller, Pending};

/// Bytes read from the control stream per call.
const READ_CHUNK: usize = 16 * 1024;

/// Error from a worker link. Never holds peer bytes.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DeckError {
    /// Transport failure.
    #[error("transport: {}", .0.code())]
    Transport(TransportError),
    /// The worker answered with a protocol error code.
    #[error("worker error: {0}")]
    Remote(String),
    /// The link broke the sync protocol; the code says how.
    #[error("protocol: {0}")]
    Protocol(&'static str),
}

impl DeckError {
    /// Stable code for logs and the UI.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Transport(e) => e.code(),
            Self::Remote(_) => "deck.remote",
            Self::Protocol(code) => code,
        }
    }
}

impl From<TransportError> for DeckError {
    fn from(e: TransportError) -> Self {
        Self::Transport(e)
    }
}

/// Dials workers for this deck.
#[derive(Debug)]
pub struct DeckHost<T> {
    transport: T,
}

impl<T: Transport> DeckHost<T> {
    /// A host over `transport`.
    pub fn new(transport: T) -> Self {
        Self { transport }
    }

    /// Connects to `worker`, opens the control stream, exchanges `Hello`.
    ///
    /// # Errors
    ///
    /// [`DeckError`] when the worker is unreachable or rejects us.
    pub async fn open_link(&self, worker: NodeId) -> Result<WorkerLink<T::Conn>, DeckError> {
        let conn = self.transport.connect(worker).await?;
        let (send, recv) = conn.open_bi().await?;
        let mut link = WorkerLink {
            conn,
            control: (send, recv),
            decoder: FrameDecoder::new(),
            buf: vec![0; READ_CHUNK],
            next_action: 1,
            calls: CallQueue::new(),
            pending: Pending::default(),
        };
        link.send(&Message::Hello {
            node_id: self.transport.local_id(),
            device_list_head: None,
            versions: vec![PROTOCOL_VERSION],
        })
        .await?;
        match link.next_message().await? {
            Message::Hello {
                node_id, versions, ..
            } => {
                if node_id != worker {
                    return Err(DeckError::Transport(TransportError::IdentityMismatch));
                }
                if !versions.contains(&PROTOCOL_VERSION) {
                    return Err(DeckError::Protocol("deck.version"));
                }
                Ok(link)
            }
            Message::Error { code } => Err(DeckError::Remote(code)),
            _ => Err(DeckError::Protocol("deck.hello_expected")),
        }
    }
}

/// One live connection to a worker (link = connection, glossary).
pub struct WorkerLink<C: Conn> {
    conn: C,
    control: (C::Send, C::Recv),
    decoder: FrameDecoder,
    buf: Vec<u8>,
    next_action: u64,
    /// Calls queued by [`LinkCaller`]s, not sent yet.
    calls: CallQueue,
    /// Calls sent, waiting for `ActionResult`.
    pending: Pending,
}

impl<C: Conn> Drop for WorkerLink<C> {
    fn drop(&mut self) {
        // The link ends: queued calls settle `NotConnected`, calls in flight `Unknown`.
        self.calls.close();
    }
}

impl<C: Conn> std::fmt::Debug for WorkerLink<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorkerLink")
            .field("worker", &self.conn.remote_id())
            .field("path", &self.conn.path())
            .finish_non_exhaustive()
    }
}

impl<C: Conn> WorkerLink<C> {
    /// Current path for the status bar (`direct · 18ms`).
    pub fn path(&self) -> PathInfo {
        self.conn.path()
    }

    /// The worker at the other end; the only writer of its `State` streams.
    pub fn worker(&self) -> NodeId {
        self.conn.remote_id()
    }

    /// Subscribes to a stream from scratch: the worker sends the full state, then deltas.
    ///
    /// # Errors
    ///
    /// [`DeckError`] when the link is down.
    pub async fn subscribe(&mut self, stream: StreamId) -> Result<(), DeckError> {
        self.subscribe_at(stream, None).await
    }

    /// Subscribes holding `version` (e.g. [`crate::Replica::version`]); the worker sends only
    /// what is missing when it can (06-apps.md, Sync to viewers rules 3–5).
    ///
    /// # Errors
    ///
    /// [`DeckError`] when the link is down.
    pub async fn subscribe_at(
        &mut self,
        stream: StreamId,
        version: Option<u64>,
    ) -> Result<(), DeckError> {
        let mut have = Have::empty();
        if let Some(v) = version {
            have.advance(self.worker(), Seq::new(v));
        }
        self.send(&Message::Subscribe {
            stream,
            have,
            mode: SubscribeMode::Live,
        })
        .await
    }

    /// Sends `create`, `close`, or `kill` on `worker/apps`. The result arrives as `ActionResult`.
    ///
    /// # Errors
    ///
    /// [`DeckError`] when the link is down.
    pub async fn apps_action(&mut self, action: AppsAction) -> Result<ActionId, DeckError> {
        let payload =
            lattice_state::encode(&action).map_err(|_| DeckError::Protocol("deck.encode"))?;
        let target = parse_stream("worker/apps")?;
        self.action(target, "apps", payload).await
    }

    /// Sends an app action (e.g. terminal `input`) to one instance.
    ///
    /// # Errors
    ///
    /// [`DeckError`] when the link is down.
    pub async fn instance_action(
        &mut self,
        iid: InstanceId,
        payload: Vec<u8>,
    ) -> Result<ActionId, DeckError> {
        // keystrokes always go on a reliable stream (01, Channel rule 2)
        let target = parse_stream(&format!("inst/{iid}/state"))?;
        self.action(target, "action", payload).await
    }

    /// A [`lattice_app::Caller`] for one instance, for a generated client
    /// (`TerminalApiClient::new(link.caller(iid))`). Its calls wait in a queue until
    /// [`WorkerLink::flush_calls`]; tickets settle in [`WorkerLink::next_message`].
    pub fn caller(&self, iid: InstanceId) -> LinkCaller {
        LinkCaller::new(self.calls.clone(), iid)
    }

    /// Sends every call queued by this link's callers, oldest first.
    ///
    /// # Errors
    ///
    /// [`DeckError`] when the link is down; the calls not sent settle `Unknown`.
    pub async fn flush_calls(&mut self) -> Result<(), DeckError> {
        for call in self.calls.drain() {
            let id = self.instance_action(call.iid, call.payload).await?;
            self.pending.insert(id, call.resolver);
        }
        Ok(())
    }

    /// Creates an instance of the package `package` (its bytes) and waits for its id.
    ///
    /// A worker that lacks the package asks for it (`BlobRequest`); this answers with the
    /// bytes (D44). Other messages that arrive first are dropped.
    ///
    /// # Errors
    ///
    /// [`DeckError::Remote`] with the worker's code when it refuses (`worker.unknown_package`,
    /// `worker.package_unavailable`, ...).
    pub async fn create(
        &mut self,
        app: AppId,
        package: (PackageHash, &[u8]),
        params: Vec<u8>,
        session: String,
    ) -> Result<InstanceId, DeckError> {
        let (hash, bytes) = package;
        let action = AppsAction::Create {
            app,
            package: Some(hash),
            params,
            session,
        };
        let want = self.apps_action(action).await?;
        loop {
            match self.next_message().await? {
                Message::BlobRequest { hash: asked } if asked == *hash.as_bytes() => {
                    self.send_blob(hash, bytes).await?;
                }
                Message::ActionResult {
                    id, error, result, ..
                } if id == want => {
                    if let Some(code) = error {
                        return Err(DeckError::Remote(code));
                    }
                    return lattice_state::decode(&result)
                        .map_err(|_| DeckError::Protocol("deck.iid"));
                }
                _ => {}
            }
        }
    }

    /// Answers the worker's `BlobRequest` for package `hash` with its bytes (D44).
    ///
    /// # Errors
    ///
    /// [`DeckError`] when the link is down.
    pub async fn send_blob(&mut self, hash: PackageHash, bytes: &[u8]) -> Result<(), DeckError> {
        let hash = *hash.as_bytes();
        let bytes = bytes.to_vec();
        self.send(&Message::Blob { hash, bytes }).await
    }

    /// Next message from the control stream. Unknown message types are skipped. An
    /// `ActionResult` settles the ticket of its call, then is returned too.
    ///
    /// Cancel-safe: bytes already read stay in the decoder.
    ///
    /// # Errors
    ///
    /// [`DeckError`] when the link closes or a frame is malformed. Every ticket still
    /// waiting then settles `Unknown`, and later calls `NotConnected`.
    pub async fn next_message(&mut self) -> Result<Message, DeckError> {
        let next = self.read_message().await;
        match &next {
            Ok(msg) => self.pending.on_message(msg),
            Err(_) => {
                self.calls.close();
                self.pending.clear();
            }
        }
        next
    }

    async fn read_message(&mut self) -> Result<Message, DeckError> {
        loop {
            match self.decoder.next() {
                Ok(Some(Decoded::Message(msg))) => return Ok(msg),
                Ok(Some(Decoded::Unsupported)) => {
                    tracing::debug!("unknown message type from worker skipped");
                }
                Ok(None) => {
                    let n = self.control.1.read(&mut self.buf).await?;
                    let read = n.and_then(|n| self.buf.get(..n));
                    let Some(read) = read else {
                        return Err(DeckError::Transport(TransportError::Closed));
                    };
                    self.decoder.push(read);
                }
                Err(e) => return Err(DeckError::Protocol(e.code())),
            }
        }
    }

    async fn action(
        &mut self,
        target: StreamId,
        name: &str,
        payload: Vec<u8>,
    ) -> Result<ActionId, DeckError> {
        let id = ActionId::new(self.next_action);
        self.next_action = self.next_action.saturating_add(1);
        self.send(&Message::Action {
            target,
            id,
            name: name.to_owned(),
            payload,
        })
        .await?;
        Ok(id)
    }

    async fn send(&mut self, msg: &Message) -> Result<(), DeckError> {
        let mut wire = Vec::new();
        encode(msg, &mut wire).map_err(|e| DeckError::Protocol(e.code()))?;
        self.control.0.write_all(&wire).await?;
        Ok(())
    }
}

fn parse_stream(text: &str) -> Result<StreamId, DeckError> {
    text.parse()
        .map_err(|_| DeckError::Protocol("deck.stream_id"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_codes_are_stable_and_namespaced() {
        assert_eq!(DeckError::Remote(String::new()).code(), "deck.remote");
        assert_eq!(
            DeckError::Transport(TransportError::Unreachable).code(),
            "transport.unreachable"
        );
        assert_eq!(
            DeckError::Transport(TransportError::Refused).code(),
            "transport.refused"
        );
        assert_eq!(
            DeckError::Protocol("sync.malformed").code(),
            "sync.malformed"
        );
    }
}
