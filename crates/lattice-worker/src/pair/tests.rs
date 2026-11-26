//! `add_deck` against a scripted deck over in-memory pipes.

use super::*;
use crate::allowlist::Allowlist;
use crate::policy::PeerPolicy;
use bytes::Bytes;
use lattice_key::{NodeKey, SecretKeyBytes};
use lattice_sync_proto::PairSecret;
use lattice_transport::{CloseReason, PathInfo};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream};

fn id(seed: u8) -> NodeId {
    NodeKey::from_secret(SecretKeyBytes::new([seed; 32])).node_id()
}

const NOW: u64 = 1_800_000_000;

fn invite() -> Invite {
    let addr = "127.0.0.1:9".parse().unwrap();
    Invite::new(id(2), addr, PairSecret::new([7; 32]), NOW + 300)
}

fn card() -> WorkerCard {
    WorkerCard {
        name: "build-01".to_owned(),
        addr: "192.168.1.5:7441".parse().unwrap(),
    }
}

/// How the one connection was closed, if it was.
type Closed = Arc<Mutex<Option<CloseReason>>>;

/// Dials one scripted deck: worker → deck and deck → worker are separate pipes.
struct OneDial(Mutex<Option<(DuplexStream, DuplexStream)>>, Closed);
struct Dialed(Mutex<Option<(DuplexStream, DuplexStream)>>, Closed);
struct PipeSend(Option<DuplexStream>);
struct Recv(DuplexStream);

impl Transport for OneDial {
    type Conn = Dialed;

    async fn connect(&self, peer: NodeId) -> Result<Dialed, TransportError> {
        assert_eq!(peer, id(2), "dials the invite's deck");
        let pipes = self.0.lock().unwrap().take();
        Ok(Dialed(Mutex::new(pipes), self.1.clone()))
    }

    async fn accept(&self) -> Result<(NodeId, Dialed), TransportError> {
        std::future::pending().await
    }

    fn local_id(&self) -> NodeId {
        id(1)
    }
}

impl Conn for Dialed {
    type Send = PipeSend;
    type Recv = Recv;

    async fn open_bi(&self) -> Result<(PipeSend, Recv), TransportError> {
        let (w, r) = self
            .0
            .lock()
            .unwrap()
            .take()
            .ok_or(TransportError::Closed)?;
        Ok((PipeSend(Some(w)), Recv(r)))
    }

    async fn accept_bi(&self) -> Result<(PipeSend, Recv), TransportError> {
        Err(TransportError::Closed)
    }

    fn send_datagram(&self, _bytes: Bytes) -> Result<(), TransportError> {
        Err(TransportError::Closed)
    }

    async fn recv_datagram(&self) -> Result<Bytes, TransportError> {
        Err(TransportError::Closed)
    }

    fn path(&self) -> PathInfo {
        PathInfo::new(
            lattice_transport::PathKind::Direct,
            Duration::from_millis(1),
        )
    }

    fn remote_id(&self) -> NodeId {
        id(2)
    }

    fn close(&self, reason: CloseReason) {
        *self.1.lock().unwrap() = Some(reason);
    }
}

impl SendStream for PipeSend {
    async fn write_all(&mut self, bytes: &[u8]) -> Result<(), TransportError> {
        let w = self.0.as_mut().ok_or(TransportError::Closed)?;
        w.write_all(bytes)
            .await
            .map_err(|e| TransportError::Io(e.to_string()))
    }

    fn finish(&mut self) -> Result<(), TransportError> {
        // Dropping the writer ends the pipe: the deck reads EOF.
        self.0.take().map(drop).ok_or(TransportError::Closed)
    }
}

impl RecvStream for Recv {
    async fn read(&mut self, buf: &mut [u8]) -> Result<Option<usize>, TransportError> {
        match self.0.read(buf).await {
            Ok(0) => Ok(None),
            Ok(n) => Ok(Some(n)),
            Err(e) => Err(TransportError::Io(e.to_string())),
        }
    }
}

/// What the scripted deck does after its reply.
#[derive(Clone, Copy, PartialEq)]
enum After {
    /// Reads `PairDone`, finishes, then waits for the worker's finish (rule 9).
    Confirm,
    /// Reads `PairDone`, then never answers.
    Stall,
    /// Reads `PairDone`, then sends a stray byte instead of finishing.
    Garble,
    /// Nothing: the reply was a denial.
    Hang,
}

/// What the deck saw: the request, whether `PairDone` came, whether the worker finished
/// after the deck's own finish.
#[derive(Debug)]
struct Seen {
    request: Message,
    done: bool,
    worker_finished: bool,
}

async fn read_msg(dec: &mut FrameDecoder, pipe: &mut DuplexStream) -> Option<Message> {
    let mut buf = vec![0u8; 1024];
    loop {
        if let Some(Decoded::Message(m)) = dec.next().unwrap() {
            return Some(m);
        }
        let n = pipe.read(&mut buf).await.unwrap();
        if n == 0 {
            return None;
        }
        dec.push(&buf[..n]);
    }
}

async fn deck(
    mut from_worker: DuplexStream,
    mut to_worker: DuplexStream,
    reply: Message,
    after: After,
) -> Seen {
    let mut dec = FrameDecoder::new();
    let request = read_msg(&mut dec, &mut from_worker).await.unwrap();
    let mut wire = Vec::new();
    encode(&reply, &mut wire).unwrap();
    to_worker.write_all(&wire).await.unwrap();
    let done = read_msg(&mut dec, &mut from_worker).await == Some(Message::PairDone);
    let mut worker_finished = false;
    match after {
        After::Confirm => {
            drop(to_worker);
            let mut buf = [0u8; 16];
            worker_finished = from_worker.read(&mut buf).await.unwrap() == 0;
        }
        After::Stall => std::future::pending::<()>().await,
        After::Garble => to_worker.write_all(b"x").await.unwrap(),
        After::Hang => {}
    }
    Seen {
        request,
        done,
        worker_finished,
    }
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lattice-pair-{name}-{}", std::process::id()));
    drop(std::fs::remove_dir_all(&dir));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Runs `add_deck` against a deck that answers `reply`, then acts as `after`.
async fn run(
    reply: Message,
    after: After,
    path: &Path,
) -> (Result<PairedDeck, PairError>, Option<Seen>, Closed) {
    let (worker_w, deck_r) = tokio::io::duplex(4096);
    let (deck_w, worker_r) = tokio::io::duplex(4096);
    let closed = Closed::default();
    let transport = OneDial(Mutex::new(Some((worker_w, worker_r))), closed.clone());
    let deck = tokio::spawn(deck(deck_r, deck_w, reply, after));
    let res = add_deck(&transport, &invite(), &card(), path, NOW).await;
    let seen = if after == After::Stall {
        deck.abort();
        None
    } else {
        deck.await.ok()
    };
    (res, seen, closed)
}

fn ok() -> Message {
    Message::PairOk {
        name: "minh-mbp".to_owned(),
    }
}

fn listed(path: &Path) -> bool {
    std::fs::read_to_string(path).is_ok_and(|t| Allowlist::parse(&t).contains(&id(2)))
}

#[tokio::test]
async fn pair_ok_writes_the_allowlist_then_confirms_with_pair_done() {
    let dir = scratch("ok");
    let path = dir.join("decks");
    let (res, seen, closed) = run(ok(), After::Confirm, &path).await;
    let want = PairedDeck {
        id: id(2),
        name: "minh-mbp".to_owned(),
    };
    assert_eq!(res, Ok(want));
    let seen = seen.unwrap();
    assert!(seen.done, "the deck got PairDone");
    assert!(seen.worker_finished, "the worker finished after the deck");
    let Message::PairRequest { secret, name, addr } = seen.request else {
        panic!("expected PairRequest, got {:?}", seen.request);
    };
    assert!(secret.matches(invite().secret()));
    assert_eq!((name.as_str(), addr), ("build-01", card().addr));
    assert!(listed(&path));
    assert_eq!(
        *closed.lock().unwrap(),
        None,
        "a paired exchange is not refused"
    );
    std::fs::remove_dir_all(dir).unwrap();
}

/// specs/01-pairing.md, rule 14: a fresh `latticed` reads the same allowlist file.
#[tokio::test]
async fn allowlist_survives_latticed_restart() {
    let dir = scratch("restart");
    let path = dir.join("decks");
    let (res, _, _) = run(ok(), After::Confirm, &path).await;
    res.unwrap();
    for start in ["first start", "after a restart"] {
        let policy = PeerPolicy::allow_file(path.clone());
        assert!(policy.permits(&id(2)).await, "case={start}");
        assert!(!policy.permits(&id(3)).await, "case={start}");
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[tokio::test]
async fn a_denied_pairing_shows_the_deck_code_and_writes_nothing() {
    let dir = scratch("denied");
    let path = dir.join("decks");
    let denied = Message::PairDenied {
        code: "pair.bad_secret".to_owned(),
    };
    let (res, seen, closed) = run(denied, After::Hang, &path).await;
    let err = res.unwrap_err();
    assert_eq!(err.code(), "pair.bad_secret");
    assert!(!seen.unwrap().done);
    assert!(!path.exists());
    assert_eq!(*closed.lock().unwrap(), Some(CloseReason::Refused));
    std::fs::remove_dir_all(dir).unwrap();
}

#[tokio::test]
async fn a_failed_allowlist_write_sends_no_pair_done() {
    let dir = scratch("unwritable");
    let path = dir.join("missing").join("decks");
    let (res, seen, closed) = run(ok(), After::Hang, &path).await;
    assert_eq!(res.unwrap_err().code(), "pair.allowlist");
    assert!(!seen.unwrap().done, "the deck must not save this worker");
    assert_eq!(*closed.lock().unwrap(), Some(CloseReason::Refused));
    std::fs::remove_dir_all(dir).unwrap();
}

/// Rule 10: the deck stalls after `PairDone`. The worker times out and keeps its line.
#[tokio::test(start_paused = true)]
async fn a_stalled_deck_times_out_and_the_allowlist_line_stays() {
    let dir = scratch("stall");
    let path = dir.join("decks");
    let (res, _, closed) = run(ok(), After::Stall, &path).await;
    assert_eq!(res, Err(PairError::Timeout));
    assert!(listed(&path), "rule 10: no rollback");
    assert_eq!(*closed.lock().unwrap(), Some(CloseReason::Refused));
    std::fs::remove_dir_all(dir).unwrap();
}

#[tokio::test]
async fn a_deck_that_does_not_finish_cleanly_is_unconfirmed() {
    let dir = scratch("garble");
    let path = dir.join("decks");
    let (res, seen, closed) = run(ok(), After::Garble, &path).await;
    assert_eq!(res, Err(PairError::Unconfirmed));
    assert!(seen.unwrap().done);
    assert_eq!(*closed.lock().unwrap(), Some(CloseReason::Refused));
    std::fs::remove_dir_all(dir).unwrap();
}

#[tokio::test]
async fn expired_pair_secret_is_denied_before_dialing() {
    let dir = scratch("expired");
    let path = dir.join("decks");
    let transport = OneDial(Mutex::new(None), Closed::default());
    let late = invite().expires();
    let res = add_deck(&transport, &invite(), &card(), &path, late).await;
    assert_eq!(res, Err(PairError::Expired));
    assert!(!path.exists());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn deck_codes_are_reduced_to_known_tokens() {
    let cases = [
        ("pair.bad_secret", "pair.bad_secret", "pair.bad_secret"),
        ("pair.expired", "pair.expired", "pair.expired"),
        ("pair.used", "pair.used", "pair.used"),
        ("pair.other", "pair.other", "pair.denied"),
        ("PAIR.used\n\u{1b}[2J", ".used", "pair.denied"),
    ];
    for (raw, kept, code) in cases {
        let e = PairError::Denied(sanitize_code(raw));
        assert_eq!(e, PairError::Denied(kept.to_owned()), "case={raw:?}");
        assert_eq!(e.code(), code, "case={raw:?}");
    }
    assert_eq!(sanitize_code(&"a".repeat(100)).len(), DENY_CODE_MAX);
}

#[test]
fn error_codes_are_stable_and_namespaced() {
    let cases = [
        (PairError::Expired, "pair.expired"),
        (PairError::Denied(String::new()), "pair.denied"),
        (PairError::Timeout, "pair.timeout"),
        (PairError::Unconfirmed, "pair.unconfirmed"),
        (PairError::Allowlist(String::new()), "pair.allowlist"),
        (
            PairError::Transport(TransportError::IdentityMismatch),
            "transport.identity_mismatch",
        ),
    ];
    for (e, code) in cases {
        assert_eq!(e.code(), code, "case={code}");
    }
}
