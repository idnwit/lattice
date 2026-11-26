//! Async client of `latticed-proc` (04-worker.md, Local IPC).

use crate::error::WorkerError;
use lattice_proc_ipc::{
    FrameDecoder, PROTOCOL_VERSION, Reply, Request, decode_reply, encode_request,
};
use std::path::Path;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

/// Bytes read from the socket per call.
const READ_CHUNK: usize = 64 * 1024;

/// One IPC connection to the process daemon.
///
/// Requests are written in order; replies and output stream back on the same
/// socket and are read with [`ProcClient::next_reply`].
#[derive(Debug)]
pub struct ProcClient {
    sock: UnixStream,
    decoder: FrameDecoder,
    buf: Vec<u8>,
}

impl ProcClient {
    /// Default socket of `latticed-proc` for user `uid` (04-worker.md, Local IPC):
    /// `$LATTICE_PROC_SOCK`, else `proc.sock` in [`ProcClient::runtime_dir`].
    pub fn default_path(uid: u32) -> std::path::PathBuf {
        lattice_proc_ipc::default_socket_path(uid)
    }

    /// The per-user runtime dir of user `uid`, shared by both daemons (specs/01-pairing.md,
    /// Files): `proc.sock` and `worker.addr` live there.
    pub fn runtime_dir(uid: u32) -> std::path::PathBuf {
        lattice_proc_ipc::runtime_dir(uid)
    }

    /// Connects and exchanges `Hello`.
    ///
    /// # Errors
    ///
    /// [`WorkerError::Proc`] when the socket is missing or the version is unsupported.
    pub async fn connect(path: &Path) -> Result<Self, WorkerError> {
        let sock = UnixStream::connect(path).await.map_err(io_error)?;
        Self::handshake(sock).await
    }

    /// Exchanges `Hello` on an open socket.
    ///
    /// # Errors
    ///
    /// [`WorkerError::Proc`] when the daemon closes or speaks another version.
    pub async fn handshake(sock: UnixStream) -> Result<Self, WorkerError> {
        let mut client = Self {
            sock,
            decoder: FrameDecoder::new(),
            buf: vec![0; READ_CHUNK],
        };
        client
            .send(&Request::Hello {
                version: PROTOCOL_VERSION,
                features: 0,
            })
            .await?;
        match client.next_reply().await? {
            Reply::Hello { version, .. } if version == PROTOCOL_VERSION => Ok(client),
            Reply::Hello { .. } => Err(WorkerError::Proc("unsupported version".to_owned())),
            _ => Err(WorkerError::Proc("expected hello".to_owned())),
        }
    }

    /// Sends one request.
    ///
    /// # Errors
    ///
    /// [`WorkerError::Proc`] when the socket write fails.
    pub async fn send(&mut self, req: &Request) -> Result<(), WorkerError> {
        let mut wire = Vec::new();
        encode_request(req, &mut wire);
        self.sock.write_all(&wire).await.map_err(io_error)
    }

    /// Waits for the next reply or output frame. Unknown types are skipped.
    ///
    /// Cancel-safe: bytes already read stay in the decoder, so it may sit in a `select!`.
    ///
    /// # Errors
    ///
    /// [`WorkerError::Ipc`] on a malformed frame; [`WorkerError::Proc`] on EOF.
    pub async fn next_reply(&mut self) -> Result<Reply, WorkerError> {
        loop {
            if let Some(frame) = self.decoder.next_frame().map_err(WorkerError::Ipc)? {
                match decode_reply(&frame).map_err(WorkerError::Ipc)? {
                    Some(reply) => return Ok(reply),
                    None => {
                        tracing::debug!("unknown reply type from proc daemon skipped");
                        continue;
                    }
                }
            }
            let n = self.sock.read(&mut self.buf).await.map_err(io_error)?;
            let Some(read) = self.buf.get(..n).filter(|r| !r.is_empty()) else {
                return Err(WorkerError::Proc("closed".to_owned()));
            };
            self.decoder.push(read);
        }
    }
}

/// An I/O error as a log-safe text (kind only, never paths or bytes).
fn io_error(e: std::io::Error) -> WorkerError {
    WorkerError::Proc(e.kind().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lattice_proc_ipc::decode_request;
    use lattice_proc_ipc::encode_reply;

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
        let procs = Reply::Journals(Vec::new());
        let (reply, got) = tokio::join!(client.next_reply(), answer(&mut daemon, &procs));
        assert_eq!(got, Request::List);
        assert_eq!(reply.unwrap(), procs);
    }

    #[tokio::test]
    async fn eof_ends_the_client_with_a_proc_error() {
        let (ours, mut daemon) = UnixStream::pair().unwrap();
        let hello = Reply::Hello {
            version: PROTOCOL_VERSION,
            features: 0,
        };
        let (client, _) = tokio::join!(ProcClient::handshake(ours), answer(&mut daemon, &hello));
        let mut client = client.unwrap();
        drop(daemon);
        assert_eq!(client.next_reply().await.unwrap_err().code(), "worker.proc");
    }

    #[tokio::test]
    async fn another_version_is_refused() {
        let (ours, mut daemon) = UnixStream::pair().unwrap();
        let hello = Reply::Hello {
            version: 2,
            features: 0,
        };
        let (client, _) = tokio::join!(ProcClient::handshake(ours), answer(&mut daemon, &hello));
        assert!(client.is_err());
    }
}
