//! End to end: the real daemon binary on a temp socket, driven over `lattice-proc-ipc`.

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "test harness helpers: a failure should abort the test"
)]

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::{Child, Command};
use std::time::{Duration, Instant};

use lattice_proc_ipc::{
    EntryKind, ErrorCode, ExitStatus, FrameDecoder, JournalId, Offset, OpenSpec, ProcId, Reply,
    Request, SOCKET_ENV, SpawnSpec, decode_reply, encode_request,
};

/// The daemon, killed when the test ends.
struct Daemon {
    child: Child,
    dir: PathBuf,
}

impl Drop for Daemon {
    fn drop(&mut self) {
        // Best effort: the test is over either way.
        self.child.kill().ok();
        self.child.wait().ok();
        std::fs::remove_dir_all(&self.dir).ok();
    }
}

struct Conn {
    sock: UnixStream,
    dec: FrameDecoder,
}

impl Conn {
    fn send(&mut self, req: &Request) {
        let mut wire = Vec::new();
        encode_request(req, &mut wire);
        self.sock.write_all(&wire).unwrap();
    }

    fn send_raw(&mut self, bytes: &[u8]) {
        self.sock.write_all(bytes).unwrap();
    }

    fn recv(&mut self) -> Reply {
        let mut buf = [0u8; 4096];
        loop {
            if let Some(frame) = self.dec.next_frame().unwrap() {
                return decode_reply(&frame).unwrap().unwrap();
            }
            let n = self.sock.read(&mut buf).unwrap();
            assert!(n > 0, "daemon closed the connection");
            self.dec.push(&buf[..n]);
        }
    }

    fn call(&mut self, req: &Request) -> Reply {
        self.send(req);
        self.recv()
    }

    /// Reads entries of `journal` until an `Exit`; returns all `Out` bytes and the status.
    fn until_exit(&mut self) -> (Vec<u8>, ExitStatus) {
        let mut out = Vec::new();
        loop {
            match self.recv() {
                Reply::Entry {
                    entry: EntryKind::Out { bytes, .. },
                    ..
                } => out.extend(bytes),
                Reply::Entry {
                    entry: EntryKind::Exit { status, .. },
                    ..
                } => return (out, status),
                other => panic!("unexpected {other:?}"),
            }
        }
    }

    /// Reads `Out` entries until `needle` shows up.
    fn until_output(&mut self, needle: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        while !out.windows(needle.len()).any(|w| w == needle) {
            match self.recv() {
                Reply::Entry {
                    entry: EntryKind::Out { bytes, .. },
                    ..
                } => out.extend(bytes),
                other => panic!("unexpected {other:?}"),
            }
        }
        out
    }
}

fn start(name: &str) -> (Daemon, Conn) {
    let dir = std::env::temp_dir().join(format!("lp-{}-{name}", std::process::id()));
    let sock = dir.join("proc.sock");
    let child = Command::new(env!("CARGO_BIN_EXE_latticed-proc"))
        .env(SOCKET_ENV, &sock)
        .spawn()
        .unwrap();
    let daemon = Daemon { child, dir };
    let deadline = Instant::now() + Duration::from_secs(10);
    let sock = loop {
        match UnixStream::connect(&sock) {
            Ok(s) => break s,
            Err(_) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            Err(e) => panic!("daemon did not listen: {e}"),
        }
    };
    sock.set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let mut conn = Conn {
        sock,
        dec: FrameDecoder::new(),
    };
    let hello = conn.call(&Request::Hello {
        version: 1,
        features: 0,
    });
    assert_eq!(
        hello,
        Reply::Hello {
            version: 1,
            features: 0
        }
    );
    (daemon, conn)
}

fn jid(n: u64) -> JournalId {
    JournalId::try_new(n.to_be_bytes().to_vec()).unwrap()
}

fn open_and_spawn(conn: &mut Conn, j: &JournalId, argv: &[&str]) -> ProcId {
    let open = OpenSpec::try_new(j.clone(), b"meta".to_vec(), 16 << 20).unwrap();
    assert_eq!(
        conn.call(&Request::Open(open)),
        Reply::Opened { journal: j.clone() }
    );
    let argv = argv.iter().map(|a| a.as_bytes().to_vec()).collect();
    let spec = SpawnSpec::try_new(j.clone(), argv, Vec::new(), 80, 24)
        .unwrap()
        .with_env(b"LATTICE_TEST".to_vec(), b"1".to_vec())
        .unwrap();
    let Reply::Spawned { id, pid } = conn.call(&Request::Spawn(spec)) else {
        panic!("spawn failed");
    };
    assert!(pid > 0);
    id
}

#[test]
fn shell_output_then_exit_code_then_drop() {
    let (_daemon, mut conn) = start("sh");
    let j = jid(1);
    open_and_spawn(&mut conn, &j, &["/bin/sh", "-c", "echo hello; exit 3"]);
    conn.send(&Request::Subscribe {
        journal: j.clone(),
        from_off: Offset::from_raw(0),
    });
    let (out, status) = conn.until_exit();
    assert!(
        String::from_utf8_lossy(&out).contains("hello"),
        "out={out:?}"
    );
    assert_eq!(status, ExitStatus::Code(3));

    let Reply::Journals(list) = conn.call(&Request::List) else {
        panic!("list")
    };
    let info = list.iter().find(|i| *i.journal() == j).unwrap();
    assert_eq!(info.meta(), b"meta");
    assert_eq!(
        info.procs().first().unwrap().exit(),
        Some(ExitStatus::Code(3))
    );

    assert_eq!(
        conn.call(&Request::Drop { journal: j.clone() }),
        Reply::Dropped { journal: j }
    );
}

#[test]
fn cat_echoes_input_and_drop_is_busy_while_it_lives() {
    let (_daemon, mut conn) = start("cat");
    let j = jid(2);
    let id = open_and_spawn(&mut conn, &j, &["/bin/cat"]);
    conn.send(&Request::Subscribe {
        journal: j.clone(),
        from_off: Offset::from_raw(0),
    });
    conn.send(&Request::Input {
        id,
        bytes: b"abc\n".to_vec(),
    });
    conn.until_output(b"abc");

    let busy = conn.call(&Request::Drop { journal: j.clone() });
    assert!(
        matches!(
            busy,
            Reply::Error {
                code: ErrorCode::Busy,
                ..
            }
        ),
        "{busy:?}"
    );

    conn.send(&Request::Signal { id, signo: 9 });
    let (_, status) = conn.until_exit();
    assert_eq!(status, ExitStatus::Signal(9));
    assert_eq!(
        conn.call(&Request::Drop { journal: j.clone() }),
        Reply::Dropped { journal: j }
    );
}

#[test]
fn unknown_type_is_unsupported_and_the_connection_stays() {
    let (_daemon, mut conn) = start("unknown");
    conn.send_raw(&[0, 0, 0, 0, 0x7F, 0x00, 0, 0]);
    let reply = conn.recv();
    assert!(
        matches!(
            reply,
            Reply::Error {
                code: ErrorCode::Unsupported,
                ..
            }
        ),
        "{reply:?}"
    );
    assert_eq!(conn.call(&Request::List), Reply::Journals(Vec::new()));
}

#[test]
fn open_twice_is_journal_exists() {
    let (_daemon, mut conn) = start("exists");
    let open = OpenSpec::try_new(jid(3), Vec::new(), 64).unwrap();
    assert!(matches!(
        conn.call(&Request::Open(open.clone())),
        Reply::Opened { .. }
    ));
    let again = conn.call(&Request::Open(open));
    assert!(
        matches!(
            again,
            Reply::Error {
                code: ErrorCode::JournalExists,
                ..
            }
        ),
        "{again:?}"
    );
}

#[test]
fn a_reconnecting_client_finds_the_journal_and_replays_it() {
    let (daemon, mut first) = start("reconnect");
    let j = jid(4);
    let id = open_and_spawn(&mut first, &j, &["/bin/cat"]);
    first.send(&Request::Input {
        id,
        bytes: b"kept\n".to_vec(),
    });
    first.send(&Request::Subscribe {
        journal: j.clone(),
        from_off: Offset::from_raw(0),
    });
    first.until_output(b"kept");
    drop(first);

    let path = daemon.dir.join("proc.sock");
    let sock = UnixStream::connect(path).unwrap();
    sock.set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let mut second = Conn {
        sock,
        dec: FrameDecoder::new(),
    };
    let Reply::Journals(list) = second.call(&Request::List) else {
        panic!("list")
    };
    let info = list.iter().find(|i| *i.journal() == j).unwrap();
    assert_eq!(info.procs().first().map(|p| p.exit()), Some(None));
    second.send(&Request::Subscribe {
        journal: j,
        from_off: info.range().0,
    });
    second.until_output(b"kept");
}

#[test]
fn malformed_frame_closes_only_that_client() {
    let (daemon, mut bad) = start("malformed");
    // Open (type 2) with an empty payload: required tags missing.
    bad.send_raw(&[0, 0, 0, 0, 0, 2, 0, 0]);
    let reply = bad.recv();
    assert!(
        matches!(
            reply,
            Reply::Error {
                code: ErrorCode::Malformed,
                ..
            }
        ),
        "{reply:?}"
    );
    let mut buf = [0u8; 16];
    assert_eq!(
        bad.sock.read(&mut buf).unwrap(),
        0,
        "connection must be closed"
    );

    let sock = UnixStream::connect(daemon.dir.join("proc.sock")).unwrap();
    let mut good = Conn {
        sock,
        dec: FrameDecoder::new(),
    };
    assert_eq!(good.call(&Request::List), Reply::Journals(Vec::new()));
}
