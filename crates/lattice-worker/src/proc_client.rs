//! Async client of `latticed-proc` (04-worker.md, Local IPC).

use crate::error::WorkerError;
use lattice_proc_ipc::{FrameDecoder, Reply, Request};
use std::path::Path;
use tokio::net::UnixStream;

/// One IPC connection to the process daemon.
///
/// Requests are written in order; replies and output stream back on the same
/// socket and are read with [`ProcClient::next_reply`].
#[derive(Debug)]
#[allow(dead_code, reason = "L1 skeleton: read once the client is implemented")]
pub struct ProcClient {
    sock: UnixStream,
    decoder: FrameDecoder,
}

impl ProcClient {
    /// Connects and exchanges `Hello`.
    ///
    /// # Errors
    ///
    /// [`WorkerError::Proc`] when the socket is missing or the version is unsupported.
    #[allow(
        clippy::todo,
        unused_variables,
        reason = "L1 skeleton: body lands with its contract test"
    )]
    pub async fn connect(path: &Path) -> Result<Self, WorkerError> {
        // UnixStream::connect(path), map io errors to Proc; then handshake
        todo!()
    }

    /// Exchanges `Hello` on an open socket.
    ///
    /// # Errors
    ///
    /// [`WorkerError::Proc`] when the daemon closes or speaks another version.
    #[allow(
        clippy::todo,
        unused_variables,
        reason = "L1 skeleton: body lands with its contract test"
    )]
    pub async fn handshake(sock: UnixStream) -> Result<Self, WorkerError> {
        // send Hello{PROTOCOL_VERSION, features}; expect Reply::Hello with version 1
        todo!()
    }

    /// Sends one request.
    ///
    /// # Errors
    ///
    /// [`WorkerError::Proc`] when the socket write fails.
    #[allow(
        clippy::todo,
        unused_variables,
        reason = "L1 skeleton: body lands with its contract test"
    )]
    pub async fn send(&mut self, req: &Request) -> Result<(), WorkerError> {
        // encode_request into a buffer; write_all
        todo!()
    }

    /// Waits for the next reply or output frame. Unknown types are skipped.
    ///
    /// # Errors
    ///
    /// [`WorkerError::Ipc`] on a malformed frame; [`WorkerError::Proc`] on EOF.
    #[allow(
        clippy::todo,
        reason = "L1 skeleton: body lands with its contract test"
    )]
    pub async fn next_reply(&mut self) -> Result<Reply, WorkerError> {
        // loop: decoder.next_frame(); Some(frame) → decode_reply; None (unknown) → continue
        //       need more bytes → read from sock; EOF → Proc("closed")
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lattice_proc_ipc::{FrameDecoder, PROTOCOL_VERSION, decode_request, encode_reply};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// Fake daemon side: reads one request, answers with `reply`.
    async fn answer(sock: &mut UnixStream, reply: &Reply) -> Request {
        let mut dec = FrameDecoder::new();
        let mut buf = [0u8; 512];
        let req = loop {
            let n = sock.read(&mut buf).await.unwrap();
            dec.push(&buf[..n]);
            if let Some(frame) = dec.next_frame().unwrap() {
                break decode_request(&frame).unwrap().unwrap();
            }
        };
        let mut wire = Vec::new();
        encode_reply(reply, &mut wire);
        sock.write_all(&wire).await.unwrap();
        req
    }

    #[tokio::test]
    #[ignore = "contract: lands with ProcClient"]
    async fn hello_first_then_requests_and_replies_in_order() {
        let (ours, mut daemon) = UnixStream::pair().unwrap();
        let hello = Reply::Hello {
            version: PROTOCOL_VERSION,
            features: 0,
        };
        let (client, got) = tokio::join!(ProcClient::handshake(ours), answer(&mut daemon, &hello));
        assert!(matches!(
            got,
            Request::Hello {
                version: PROTOCOL_VERSION,
                ..
            }
        ));
        let mut client = client.unwrap();
        client.send(&Request::List).await.unwrap();
        let procs = Reply::Procs(Vec::new());
        let (reply, got) = tokio::join!(client.next_reply(), answer(&mut daemon, &procs));
        assert_eq!(got, Request::List);
        assert_eq!(reply.unwrap(), procs);
    }
}
