use std::path::PathBuf;

use lattice_control::{
    CONTROL_VERSION, MAX_CONTROL_FRAME, ReplyReader, SOCKET_FILE, encode_request,
};

use super::*;

/// A scratch dir with a short name: socket paths have a length limit.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ltc-{name}-{}", std::process::id()));
    drop(std::fs::remove_dir_all(&dir));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn uid() -> u32 {
    crate::runtime::uid()
}

/// Serves a handler that answers every request with `Failed{test.ok}`.
fn spawn_server(dir: &Path, uid: u32) -> PathBuf {
    let path = dir.join(SOCKET_FILE);
    let listener = bind(&path).unwrap();
    let handler = Arc::new(|_req: Request| Reply::failed("test.ok"));
    tokio::spawn(serve(listener, uid, handler));
    path
}

/// Sends `bytes`, then reads to EOF: the reply, or `None` when it closed without one.
async fn exchange(path: &Path, bytes: &[u8]) -> Option<Reply> {
    let mut s = UnixStream::connect(path).await.unwrap();
    // A peer that closes early may refuse the write; the read below tells.
    drop(s.write_all(bytes).await);
    let mut got = Vec::new();
    drop(s.read_to_end(&mut got).await);
    let mut r = ReplyReader::new();
    r.push(&got);
    r.next().unwrap()
}

fn request(req: &Request) -> Vec<u8> {
    let mut out = Vec::new();
    encode_request(req, &mut out).unwrap();
    out
}

#[tokio::test]
async fn one_request_gets_one_reply() {
    let dir = scratch("one");
    let path = spawn_server(&dir, uid());
    let got = exchange(&path, &request(&Request::ConfigList)).await;
    assert_eq!(got, Some(Reply::failed("test.ok")));
    std::fs::remove_dir_all(dir).unwrap();
}

/// specs/11-control-socket.md, Invariant 2.
#[tokio::test]
async fn an_unknown_control_request_is_unsupported() {
    let dir = scratch("unsup");
    let path = spawn_server(&dir, uid());
    let unknown = [CONTROL_VERSION, 0, 0, 0, 1, 42];
    assert_eq!(
        exchange(&path, &unknown).await,
        Some(Reply::failed("control.unsupported"))
    );
    // Malformed bytes close only their connection.
    let malformed = [CONTROL_VERSION, 0, 0, 0, 1, 0];
    assert_eq!(exchange(&path, &malformed).await, None);
    let got = exchange(&path, &request(&Request::ConfigList)).await;
    assert_eq!(
        got,
        Some(Reply::failed("test.ok")),
        "the daemon still serves"
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[tokio::test]
async fn another_version_gets_control_version() {
    let dir = scratch("ver");
    let path = spawn_server(&dir, uid());
    assert_eq!(
        exchange(&path, &[2, 0, 0, 0, 0]).await,
        Some(Reply::failed("control.version"))
    );
    std::fs::remove_dir_all(dir).unwrap();
}

/// specs/11-control-socket.md, Invariant 3.
#[tokio::test]
async fn an_oversized_control_frame_closes_the_connection() {
    let dir = scratch("big");
    let path = spawn_server(&dir, uid());
    let len = u32::try_from(MAX_CONTROL_FRAME + 1).unwrap();
    let mut wire = vec![CONTROL_VERSION];
    wire.extend_from_slice(&len.to_be_bytes());
    // Only the prefix: the server closes before any body byte arrives.
    assert_eq!(exchange(&path, &wire).await, None);
    std::fs::remove_dir_all(dir).unwrap();
}

/// specs/11-control-socket.md, Invariant 4.
#[tokio::test]
async fn a_control_peer_with_another_uid_is_refused() {
    let dir = scratch("uid");
    // The server serves another uid only, so this process is "another uid".
    let path = spawn_server(&dir, uid().wrapping_add(1));
    assert_eq!(exchange(&path, &request(&Request::ConfigList)).await, None);
    std::fs::remove_dir_all(dir).unwrap();
}

/// specs/11-control-socket.md, Invariant 5 (stale socket half).
#[tokio::test]
async fn a_stale_control_socket_is_replaced() {
    let dir = scratch("stale");
    let path = dir.join(SOCKET_FILE);
    // A dead daemon leaves its socket file behind.
    drop(std::os::unix::net::UnixListener::bind(&path).unwrap());
    assert!(path.exists());
    let path = spawn_server(&dir, uid());
    let got = exchange(&path, &request(&Request::ConfigList)).await;
    assert_eq!(got, Some(Reply::failed("test.ok")));
    let mode =
        std::os::unix::fs::PermissionsExt::mode(&std::fs::metadata(&path).unwrap().permissions());
    assert_eq!(mode & 0o777, 0o600);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_socket_path_over_the_limit_fails() {
    let long = PathBuf::from("/").join("d".repeat(SUN_PATH_MAX));
    let err = bind(&long).unwrap_err();
    assert!(
        format!("{err}").starts_with("control.path_too_long"),
        "err={err}"
    );
}

/// Rule 11: a fifth connection closes at once, with no reply.
#[tokio::test]
async fn extra_connections_close_without_a_reply() {
    let dir = scratch("cap");
    let path = spawn_server(&dir, uid());
    let mut idle = Vec::new();
    for _ in 0..MAX_CONNECTIONS {
        idle.push(UnixStream::connect(&path).await.unwrap());
    }
    // Let the server take the four slots.
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(exchange(&path, &request(&Request::ConfigList)).await, None);
    drop(idle);
    tokio::time::sleep(Duration::from_millis(100)).await;
    let got = exchange(&path, &request(&Request::ConfigList)).await;
    assert_eq!(got, Some(Reply::failed("test.ok")), "slots come back");
    std::fs::remove_dir_all(dir).unwrap();
}

/// Rule 10: a connection with no whole request closes after the timeout.
#[tokio::test(start_paused = true)]
async fn a_slow_request_times_out() {
    let dir = scratch("slow");
    let path = spawn_server(&dir, uid());
    let mut s = UnixStream::connect(&path).await.unwrap();
    s.write_all(&[CONTROL_VERSION, 0]).await.unwrap();
    let mut got = Vec::new();
    let read = tokio::time::timeout(REQUEST_TIMEOUT * 2, s.read_to_end(&mut got)).await;
    assert!(matches!(read, Ok(Ok(0))), "closed by the server: {read:?}");
    std::fs::remove_dir_all(dir).unwrap();
}
