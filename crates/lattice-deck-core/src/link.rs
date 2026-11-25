//! Links from the deck to workers.

use lattice_key::NodeId;
use lattice_sync_proto::{Message, StreamId};
use lattice_transport::{Conn, PathInfo, Transport, TransportError};

/// Error from a worker link. Never holds peer bytes.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DeckError {
    /// Transport failure.
    #[error("transport: {}", .0.code())]
    Transport(TransportError),
    /// The worker answered with a protocol error code.
    #[error("worker error: {0}")]
    Remote(String),
}

impl DeckError {
    /// Stable code for logs and the UI.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Transport(e) => e.code(),
            Self::Remote(_) => "deck.remote",
        }
    }
}

/// Dials workers for this deck.
#[derive(Debug)]
#[allow(dead_code, reason = "L1 skeleton: read once open_link is implemented")]
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
    #[allow(
        clippy::todo,
        unused_variables,
        reason = "L1 skeleton: body lands with its contract test"
    )]
    pub async fn open_link(&self, worker: NodeId) -> Result<WorkerLink<T::Conn>, DeckError> {
        // transport.connect(worker); conn.open_bi() = control stream #0
        // send Hello{local_id, versions: [PROTOCOL_VERSION]}; await Hello
        todo!()
    }
}

/// One live connection to a worker (link = connection, glossary).
#[derive(Debug)]
#[allow(dead_code, reason = "L1 skeleton: read once the link is implemented")]
pub struct WorkerLink<C: Conn> {
    conn: C,
    control: (C::Send, C::Recv),
}

impl<C: Conn> WorkerLink<C> {
    /// Current path for the status bar (`direct · 18ms`).
    pub fn path(&self) -> PathInfo {
        self.conn.path()
    }

    /// Subscribes to a stream on its own bi stream (01, Channel layout).
    ///
    /// # Errors
    ///
    /// [`DeckError`] when the stream cannot open.
    #[allow(
        clippy::todo,
        unused_variables,
        reason = "L1 skeleton: body lands with its contract test"
    )]
    pub async fn subscribe(&mut self, stream: StreamId) -> Result<(), DeckError> {
        // open_bi; send Subscribe{stream, have: cached cursor or empty, mode: Live}
        // spawn-free: the caller polls next_message on the returned channel (TBD API shape)
        todo!()
    }

    /// Sends keystrokes as an `input` action on the process's input stream.
    ///
    /// # Errors
    ///
    /// [`DeckError`] when the link is down.
    #[allow(
        clippy::todo,
        unused_variables,
        reason = "L1 skeleton: body lands with its contract test"
    )]
    pub async fn send_input(&mut self, target: StreamId, bytes: Vec<u8>) -> Result<(), DeckError> {
        // keystrokes always go on a reliable stream (01, Channel rule 2)
        // Action{target, id: next id, name: "input", payload: bytes}
        todo!()
    }

    /// Next message from the control stream.
    ///
    /// # Errors
    ///
    /// [`DeckError`] when the link closes.
    #[allow(
        clippy::todo,
        reason = "L1 skeleton: body lands with its contract test"
    )]
    pub async fn next_message(&mut self) -> Result<Message, DeckError> {
        // read into a FrameDecoder; Unsupported → skip; Message → return
        todo!()
    }
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
    }
}
