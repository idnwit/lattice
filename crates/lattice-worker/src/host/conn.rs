//! Accept loop and per-connection I/O tasks. No state: everything goes to the core.

use lattice_key::NodeId;
use lattice_sync_proto::{Decoded, FrameDecoder, Message, encode};
use lattice_transport::{Conn, RecvStream, SendStream, Transport, TransportError};
use tokio::sync::mpsc;

use crate::error::WorkerError;
use crate::policy::PeerPolicy;

/// Messages queued per deck before it counts as too slow and is disconnected.
pub(super) const OUTBOX: usize = 1024;

/// Bytes read from a control stream per call.
const READ_CHUNK: usize = 16 * 1024;

/// Connection id, local to this process.
pub(super) type ConnId = u64;

/// What a connection task tells the core.
#[derive(Debug)]
pub(super) enum Event {
    /// A deck opened its control stream; the core writes to it through `outbox`.
    Open {
        conn: ConnId,
        peer: NodeId,
        outbox: mpsc::Sender<Message>,
    },
    /// A sync message from the deck.
    Message { conn: ConnId, msg: Message },
    /// A message type this build does not know (02-sync.md, Wire messages rule 5).
    Unsupported { conn: ConnId },
    /// The connection ended.
    Closed { conn: ConnId },
}

/// Accepts decks until the transport closes; one task per permitted connection.
pub(super) async fn accept_loop<T: Transport + 'static>(
    transport: T,
    policy: PeerPolicy,
    events: mpsc::Sender<Event>,
) -> Result<(), TransportError> {
    let mut next: ConnId = 0;
    loop {
        let (peer, conn) = match transport.accept().await {
            Ok(accepted) => accepted,
            Err(TransportError::Closed) => return Ok(()),
            Err(e) => {
                tracing::warn!(code = e.code(), "accept failed");
                continue;
            }
        };
        if !policy.permits(&peer).await {
            tracing::warn!(?peer, "connection refused by policy");
            continue; // dropping `conn` closes it
        }
        next = next.wrapping_add(1);
        tracing::info!(?peer, conn = next, "deck connected");
        tokio::spawn(serve(conn, next, peer, events.clone()));
    }
}

/// Maps the end of the accept task to the end of [`super::WorkerHost::run`].
pub(super) fn accept_ended(
    res: Result<Result<(), TransportError>, tokio::task::JoinError>,
) -> Result<(), WorkerError> {
    match res {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(WorkerError::Transport(e)),
        Err(_) => Err(WorkerError::Transport(TransportError::Closed)),
    }
}

/// One deck connection: the first bi stream it opens is the control stream.
async fn serve<C: Conn>(conn: C, id: ConnId, peer: NodeId, events: mpsc::Sender<Event>) {
    let (send, recv) = match conn.accept_bi().await {
        Ok(streams) => streams,
        Err(e) => {
            tracing::warn!(conn = id, code = e.code(), "no control stream");
            return;
        }
    };
    let (outbox, rx) = mpsc::channel(OUTBOX);
    let open = Event::Open {
        conn: id,
        peer,
        outbox,
    };
    if events.send(open).await.is_err() {
        return; // the core is gone: the worker is shutting down
    }
    tokio::select! {
        () = read_loop(recv, id, &events) => {},
        () = write_loop(send, rx, id) => {},
    }
    if events.send(Event::Closed { conn: id }).await.is_err() {
        tracing::debug!(conn = id, "core gone before the close event");
    }
    tracing::info!(conn = id, "deck disconnected");
    drop(conn);
}

/// Decodes frames into events until EOF or a framing error.
async fn read_loop<R: RecvStream>(mut recv: R, id: ConnId, events: &mpsc::Sender<Event>) {
    let mut dec = FrameDecoder::new();
    let mut buf = vec![0u8; READ_CHUNK];
    loop {
        let ev = match dec.next() {
            Ok(Some(Decoded::Message(msg))) => Event::Message { conn: id, msg },
            Ok(Some(Decoded::Unsupported)) => Event::Unsupported { conn: id },
            Ok(None) => match recv.read(&mut buf).await {
                Ok(Some(n)) => {
                    dec.push(buf.get(..n).unwrap_or_default());
                    continue;
                }
                Ok(None) => return,
                Err(e) => {
                    tracing::debug!(conn = id, code = e.code(), "control stream read failed");
                    return;
                }
            },
            Err(e) => {
                tracing::warn!(conn = id, code = e.code(), "bad frame from deck");
                return;
            }
        };
        if events.send(ev).await.is_err() {
            return;
        }
    }
}

/// Writes the core's messages until the core drops this deck's outbox.
async fn write_loop<S: SendStream>(mut send: S, mut rx: mpsc::Receiver<Message>, id: ConnId) {
    let mut wire = Vec::new();
    while let Some(msg) = rx.recv().await {
        wire.clear();
        if let Err(e) = encode(&msg, &mut wire) {
            tracing::error!(
                conn = id,
                code = e.code(),
                "message does not encode; dropped"
            );
            continue;
        }
        if let Err(e) = send.write_all(&wire).await {
            tracing::debug!(conn = id, code = e.code(), "control stream write failed");
            return;
        }
    }
    if let Err(e) = send.finish() {
        // The stream may already be reset; the connection closes next anyway.
        tracing::debug!(conn = id, code = e.code(), "control stream finish failed");
    }
}
