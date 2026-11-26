//! The host end to end over an in-memory transport and a fake `latticed-proc`.

use super::*;
use crate::test_app::{echo_hash, echo_loader, echo_package_bytes};
use bytes::Bytes;
use lattice_app::{AppId, AppsAction, InstanceId, InstanceRecord};
use lattice_key::{NodeId, NodeKey, SecretKeyBytes};
use lattice_proc_ipc::{EntryKind, Offset, ProcId, decode_request, encode_reply};
use lattice_sync_proto::{ActionId, Decoded, FrameDecoder, Have, Message, SubscribeMode, encode};
use lattice_transport::{
    CloseReason, Conn, PathInfo, PathKind, RecvStream, SendStream, TransportError,
};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream, ReadHalf, WriteHalf};
use tokio::net::UnixStream;

fn id(seed: u8) -> NodeId {
    NodeKey::from_secret(SecretKeyBytes::new([seed; 32])).node_id()
}

/// One connection's state. The transport keeps a handle too, as `lattice-quic` does (D4),
/// so dropping a `FakeConn` does not close it; `close` does.
type Shared = Arc<Mutex<(Option<DuplexStream>, Option<CloseReason>)>>;

/// Accepts one deck, then waits forever.
struct OneDeck(Mutex<Option<Shared>>, Shared);

impl OneDeck {
    fn new(pipe: DuplexStream) -> Self {
        let shared: Shared = Arc::new(Mutex::new((Some(pipe), None)));
        Self(Mutex::new(Some(shared.clone())), shared)
    }
}

struct FakeConn(Shared);
struct FakeSend(WriteHalf<DuplexStream>);
struct FakeRecv(ReadHalf<DuplexStream>);

impl Transport for OneDeck {
    type Conn = FakeConn;

    async fn connect(&self, _peer: NodeId) -> Result<FakeConn, TransportError> {
        Err(TransportError::Unreachable)
    }

    async fn accept(&self) -> Result<(NodeId, FakeConn), TransportError> {
        let next = self.0.lock().unwrap().take();
        match next {
            Some(shared) => Ok((id(2), FakeConn(shared))),
            None => std::future::pending().await,
        }
    }

    fn local_id(&self) -> NodeId {
        id(1)
    }
}

impl Conn for FakeConn {
    type Send = FakeSend;
    type Recv = FakeRecv;

    async fn open_bi(&self) -> Result<(FakeSend, FakeRecv), TransportError> {
        Err(TransportError::Closed)
    }

    async fn accept_bi(&self) -> Result<(FakeSend, FakeRecv), TransportError> {
        let pipe = self
            .0
            .lock()
            .unwrap()
            .0
            .take()
            .ok_or(TransportError::Closed)?;
        let (r, w) = tokio::io::split(pipe);
        Ok((FakeSend(w), FakeRecv(r)))
    }

    fn send_datagram(&self, _bytes: Bytes) -> Result<(), TransportError> {
        Err(TransportError::Closed)
    }

    async fn recv_datagram(&self) -> Result<Bytes, TransportError> {
        Err(TransportError::Closed)
    }

    fn path(&self) -> PathInfo {
        PathInfo::new(PathKind::Direct, Duration::from_millis(1))
    }

    fn remote_id(&self) -> NodeId {
        id(2)
    }

    fn close(&self, reason: CloseReason) {
        *self.0.lock().unwrap() = (None, Some(reason));
    }
}

impl SendStream for FakeSend {
    async fn write_all(&mut self, bytes: &[u8]) -> Result<(), TransportError> {
        self.0
            .write_all(bytes)
            .await
            .map_err(|e| TransportError::Io(e.to_string()))
    }

    fn finish(&mut self) -> Result<(), TransportError> {
        Ok(())
    }
}

impl RecvStream for FakeRecv {
    async fn read(&mut self, buf: &mut [u8]) -> Result<Option<usize>, TransportError> {
        match self.0.read(buf).await {
            Ok(0) => Ok(None),
            Ok(n) => Ok(Some(n)),
            Err(e) => Err(TransportError::Io(e.to_string())),
        }
    }
}

/// Fake daemon: empty `List`; `Spawned` for each spawn; one `Out` entry on subscribe.
async fn fake_daemon(mut sock: UnixStream) {
    let mut dec = lattice_proc_ipc::FrameDecoder::new();
    let mut buf = vec![0u8; 4096];
    loop {
        let frame = match dec.next_frame().unwrap() {
            Some(f) => f,
            None => {
                let n = sock.read(&mut buf).await.unwrap();
                if n == 0 {
                    return;
                }
                dec.push(&buf[..n]);
                continue;
            }
        };
        let reply = match decode_request(&frame).unwrap().unwrap() {
            Request::Hello { version, .. } => Reply::Hello {
                version,
                features: 0,
            },
            Request::List => Reply::Journals(Vec::new()),
            Request::Spawn(_) => Reply::Spawned {
                id: ProcId::from_raw(1),
                pid: 42,
            },
            Request::Subscribe { journal, .. } => Reply::Entry {
                journal,
                off: Offset::from_raw(0),
                entry: EntryKind::Out {
                    proc: ProcId::from_raw(1),
                    bytes: b"hi".to_vec(),
                },
            },
            _ => continue,
        };
        let mut wire = Vec::new();
        encode_reply(&reply, &mut wire);
        sock.write_all(&wire).await.unwrap();
    }
}

/// The deck's end of the control stream.
struct Deck {
    pipe: DuplexStream,
    dec: FrameDecoder,
}

impl Deck {
    async fn send(&mut self, msg: Message) {
        let mut wire = Vec::new();
        encode(&msg, &mut wire).unwrap();
        self.pipe.write_all(&wire).await.unwrap();
    }

    async fn recv(&mut self) -> Message {
        let mut buf = [0u8; 4096];
        loop {
            if let Some(Decoded::Message(m)) = self.dec.next().unwrap() {
                return m;
            }
            let n = tokio::time::timeout(Duration::from_secs(5), self.pipe.read(&mut buf))
                .await
                .expect("worker answers within 5 s")
                .unwrap();
            assert_ne!(n, 0, "worker closed the control stream");
            self.dec.push(&buf[..n]);
        }
    }
}

type Worker = tokio::task::JoinHandle<Result<(), WorkerError>>;

/// A running host with the Echo app and a fake daemon; one deck (`id(2)`) connects.
async fn spawn_host(policy: PeerPolicy) -> (DuplexStream, Shared, Worker) {
    let (ours, theirs) = UnixStream::pair().unwrap();
    tokio::spawn(fake_daemon(theirs));
    let proc = ProcClient::handshake(ours).await.unwrap();
    // The worker ships no app: Echo arrives as a package on the first create (D44).
    let apps = AppsRunner::new(crate::BootEpoch::from_unix_millis(0));
    let (worker_end, deck_end) = tokio::io::duplex(64 * 1024);
    let transport = OneDeck::new(worker_end);
    let conn = transport.1.clone();
    let host = WorkerHost::new(transport, proc, policy, apps, echo_loader());
    (deck_end, conn, tokio::spawn(host.run()))
}

/// A running host with the Echo app, a fake daemon, and a greeted deck.
async fn start() -> (Deck, Worker) {
    let (deck_end, _conn, worker) = spawn_host(PeerPolicy::allow_list(vec![id(2)])).await;
    let mut deck = Deck {
        pipe: deck_end,
        dec: FrameDecoder::new(),
    };
    deck.send(Message::Hello {
        node_id: id(2),
        device_list_head: None,
        versions: vec![1],
    })
    .await;
    assert!(matches!(deck.recv().await, Message::Hello { node_id, .. } if node_id == id(1)));
    (deck, worker)
}

/// specs/01-pairing.md, rule 12: a refused deck sees its connection end, not a hang.
#[tokio::test]
async fn unpaired_deck_cannot_open_a_stream() {
    let (mut deck_end, conn, worker) = spawn_host(PeerPolicy::allow_list(vec![id(9)])).await;
    let mut buf = [0u8; 64];
    let n = tokio::time::timeout(Duration::from_secs(2), deck_end.read(&mut buf))
        .await
        .expect("the worker closes a refused deck within 2 s")
        .unwrap();
    assert_eq!(n, 0, "no byte reaches a refused deck");
    assert_eq!(conn.lock().unwrap().1, Some(CloseReason::Refused));
    worker.abort();
}

fn subscribe(stream: &str) -> Message {
    Message::Subscribe {
        stream: stream.parse().unwrap(),
        have: Have::empty(),
        mode: SubscribeMode::Live,
    }
}

/// Sends `create` for Echo's package.
async fn send_create(deck: &mut Deck) {
    let create = AppsAction::Create {
        app: AppId::try_new("test.echo").unwrap(),
        package: Some(echo_hash()),
        params: Vec::new(),
        session: "s".to_owned(),
    };
    deck.send(Message::Action {
        target: "worker/apps".parse().unwrap(),
        id: ActionId::new(1),
        name: "apps".to_owned(),
        payload: lattice_state::encode(&create).unwrap(),
    })
    .await;
}

/// Sends `create` for Echo, serving the package when the worker asks; returns the new
/// instance id from `ActionResult`.
async fn create(deck: &mut Deck) -> InstanceId {
    send_create(deck).await;
    loop {
        match deck.recv().await {
            Message::BlobRequest { hash } => {
                assert_eq!(hash, *echo_hash().as_bytes());
                let bytes = echo_package_bytes();
                deck.send(Message::Blob { hash, bytes }).await;
            }
            Message::ActionResult {
                id, error, result, ..
            } => {
                assert_eq!((id, error), (ActionId::new(1), None));
                return lattice_state::decode::<InstanceId>(&result).unwrap();
            }
            Message::Snapshot { .. } => {}
            other => panic!("unexpected {other:?}"),
        }
    }
}

#[tokio::test]
async fn a_package_that_does_not_match_its_hash_is_refused() {
    let (mut deck, worker) = start().await;
    send_create(&mut deck).await;
    let Message::BlobRequest { hash } = deck.recv().await else {
        panic!("the worker asks for the package it lacks");
    };
    let bytes = b"not the package".to_vec();
    deck.send(Message::Blob { hash, bytes }).await;
    loop {
        match deck.recv().await {
            Message::ActionResult { id, error, .. } => {
                assert_eq!(id, ActionId::new(1));
                assert_eq!(error.as_deref(), Some("worker.package_unavailable"));
                break;
            }
            Message::Snapshot { .. } => {}
            other => panic!("unexpected {other:?}"),
        }
    }
    worker.abort();
}

#[tokio::test]
async fn a_deck_creates_an_instance_and_reads_its_state() {
    let (mut deck, worker) = start().await;
    deck.send(subscribe("worker/apps")).await;
    let Message::Snapshot { payload, .. } = deck.recv().await else {
        panic!("expected the apps snapshot");
    };
    assert_eq!(InstanceRecord::decode_list(&payload), Some(Vec::new()));

    assert_eq!(create(&mut deck).await, InstanceId::from_raw(1));

    deck.send(subscribe("inst/1/state")).await;
    // The `Out` entry ("hi", 2 bytes) lands before or after the subscribe: as the full
    // state or as a change. Either way the deck ends up at 2.
    let two = lattice_state::encode(&2u64).unwrap();
    // A change ends with its payload (length 1, then the value) and `sig: None` (0).
    let change_tail = [&[1u8][..], two.as_slice(), &[0]].concat();
    loop {
        let reached = match deck.recv().await {
            Message::Snapshot {
                stream, payload, ..
            } => stream.as_str() == "inst/1/state" && payload == two,
            Message::Change(c) => lattice_state::encode(&c).unwrap().ends_with(&change_tail),
            _ => false,
        };
        if reached {
            break;
        }
    }
    worker.abort();
}

/// Sends an `echo` request to instance 1; returns its `ActionResult` fields.
async fn echo(deck: &mut Deck, id: u64, req: &[u8]) -> (Option<String>, Vec<u8>, Vec<u8>) {
    let action = crate::test_app::EchoApiAction::Echo(req.to_vec());
    deck.send(Message::Action {
        target: "inst/1/state".parse().unwrap(),
        id: ActionId::new(id),
        name: "action".to_owned(),
        payload: lattice_state::encode(&action).unwrap(),
    })
    .await;
    loop {
        if let Message::ActionResult {
            id: got,
            error,
            result,
            detail,
        } = deck.recv().await
        {
            assert_eq!(got, ActionId::new(id));
            return (error, result, detail);
        }
    }
}

#[tokio::test]
async fn an_instance_action_answers_with_its_reply_or_its_app_error() {
    let (mut deck, worker) = start().await;
    assert_eq!(create(&mut deck).await, InstanceId::from_raw(1));
    let max = lattice_state::encode(&u64::MAX).unwrap();
    assert_eq!(echo(&mut deck, 2, b"").await, (None, max, Vec::new()));
    let refused = lattice_state::encode(&crate::test_app::EchoError::Refused).unwrap();
    assert_eq!(
        echo(&mut deck, 3, b"x").await,
        (Some("refused".to_owned()), Vec::new(), refused)
    );
    worker.abort();
}
