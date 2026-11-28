//! The control socket `worker/latticed.sock` (specs/11-control-socket.md, rules 3–11).
//!
//! One request, one reply, then close. Peers of another uid, a wrong version byte, an
//! oversized or malformed frame, or a slow sender never stop the daemon.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use lattice_control::{ControlError, Incoming, Reply, Request, RequestReader, encode_reply};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::Semaphore;

/// Connections served at once (rule 11; proposed). Extra ones close without a reply.
const MAX_CONNECTIONS: usize = 4;

/// A connection must send a whole request within this time (rule 10; proposed).
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

/// Bytes read from a connection per call.
const READ_CHUNK: usize = 4096;

/// `sun_path` size: a socket path holds at most this many bytes, NUL included (rule 5).
#[cfg(target_os = "macos")]
const SUN_PATH_MAX: usize = 104;
#[cfg(not(target_os = "macos"))]
const SUN_PATH_MAX: usize = 108;

/// Unlinks a stale socket at `path` and binds a new one, mode 0600 (rule 1 step 3).
///
/// The caller holds the `worker/` lock, so a socket file there is stale.
pub(crate) fn bind(path: &Path) -> anyhow::Result<UnixListener> {
    use std::os::unix::fs::PermissionsExt;
    let len = path.as_os_str().len();
    anyhow::ensure!(
        len < SUN_PATH_MAX,
        "control.path_too_long: {} has {len} bytes; the limit is {}",
        path.display(),
        SUN_PATH_MAX.saturating_sub(1)
    );
    match std::fs::remove_file(path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
            return Err(e).with_context(|| format!("remove stale {}", path.display()));
        }
        _ => {}
    }
    let listener = UnixListener::bind(path).with_context(|| format!("bind {}", path.display()))?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        .with_context(|| format!("chmod {}", path.display()))?;
    Ok(listener)
}

/// Serves `listener` until it fails. `uid` is the only peer uid served (rule 4).
/// `handler` answers one request; it may block, so it runs off the async workers.
pub(crate) async fn serve<H>(listener: UnixListener, uid: u32, handler: Arc<H>)
where
    H: Fn(Request) -> Reply + Send + Sync + 'static,
{
    let slots = Arc::new(Semaphore::new(MAX_CONNECTIONS));
    loop {
        let stream = match listener.accept().await {
            Ok((stream, _)) => stream,
            Err(e) => {
                // EMFILE and the like pass; the next accept retries.
                tracing::warn!(error = %e, "control accept failed");
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            }
        };
        let Ok(slot) = slots.clone().try_acquire_owned() else {
            tracing::debug!("control: connection cap reached, closing");
            continue;
        };
        let handler = handler.clone();
        tokio::spawn(async move {
            connection(stream, uid, handler).await;
            drop(slot);
        });
    }
}

/// One connection: peer check, one request within the timeout, one reply.
async fn connection<H>(mut stream: UnixStream, uid: u32, handler: Arc<H>)
where
    H: Fn(Request) -> Reply + Send + Sync + 'static,
{
    match stream.peer_cred() {
        Ok(cred) if cred.uid() == uid => {}
        Ok(cred) => {
            tracing::warn!(
                peer_uid = cred.uid(),
                "control: peer of another uid refused"
            );
            return;
        }
        Err(e) => {
            tracing::warn!(error = %e, "control: no peer credentials, refused");
            return;
        }
    }
    let incoming = match tokio::time::timeout(REQUEST_TIMEOUT, read_request(&mut stream)).await {
        Ok(Ok(incoming)) => incoming,
        Ok(Err(code)) => {
            tracing::info!(code, "control: connection closed");
            return;
        }
        Err(_) => {
            tracing::info!("control: no request within the timeout");
            return;
        }
    };
    let reply = match incoming {
        Incoming::Request(req) => {
            tracing::debug!(request = ?req, "control request");
            let handler = handler.clone();
            match tokio::task::spawn_blocking(move || handler(req)).await {
                Ok(reply) => reply,
                Err(e) => {
                    tracing::error!(error = %e, "control handler failed");
                    return;
                }
            }
        }
        Incoming::Unsupported => Reply::failed("control.unsupported"),
        Incoming::BadVersion => Reply::failed("control.version"),
    };
    let mut out = Vec::new();
    if let Err(e) = encode_reply(&reply, &mut out) {
        tracing::error!(code = e.code(), "control reply not encoded");
        return;
    }
    if let Err(e) = stream.write_all(&out).await {
        tracing::debug!(error = %e, "control reply not sent");
    }
    // The peer may be gone already; the connection ends either way.
    drop(stream.shutdown().await);
}

/// Reads until one request is in. Errors are log codes; the connection then closes.
async fn read_request(stream: &mut UnixStream) -> Result<Incoming, &'static str> {
    let mut reader = RequestReader::new();
    let mut buf = [0u8; READ_CHUNK];
    loop {
        if let Some(incoming) = reader.next().map_err(|e: ControlError| e.code())? {
            return Ok(incoming);
        }
        let n = stream.read(&mut buf).await.map_err(|_| "control.io")?;
        let Some(read) = buf.get(..n).filter(|r| !r.is_empty()) else {
            return Err("control.eof");
        };
        reader.push(read);
    }
}

#[cfg(test)]
mod tests;
