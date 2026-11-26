//! `add_deck` against a scripted deck over in-memory pipes.

use super::*;
use bytes::Bytes;
use lattice_key::{NodeKey, SecretKeyBytes};
use lattice_sync_proto::PairSecret;
use lattice_transport::{CloseReason, PathInfo};
use std::sync::Mutex;
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

/// Dials one scripted deck: worker → deck and deck → worker are separate pipes.
struct OneDial(Mutex<Option<(DuplexStream, DuplexStream)>>);
struct Dialed(Mutex<Option<(DuplexStream, DuplexStream)>>);
struct PipeSend(Option<DuplexStream>);
struct Recv(DuplexStream);

impl Transport for OneDial {
    type Conn = Dialed;

    async fn connect(&self, peer: NodeId) -> Result<Dialed, TransportError> {
        assert_eq!(peer, id(2), "dials the invite's deck");
        let pipes = self.0.lock().unwrap().take();
        Ok(Dialed(Mutex::new(pipes)))
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

    fn close(&self, _reason: CloseReason) {
        // Not observed by these tests.
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

/// A deck that reads the request, answers `reply`, and reports what it saw and when the
/// worker finished its side.
async fn deck(
    mut from_worker: DuplexStream,
    mut to_worker: DuplexStream,
    reply: Message,
) -> (Message, bool) {
    let mut dec = FrameDecoder::new();
    let mut buf = vec![0u8; 1024];
    let request = loop {
        if let Some(Decoded::Message(m)) = dec.next().unwrap() {
            break m;
        }
        let n = from_worker.read(&mut buf).await.unwrap();
        dec.push(&buf[..n]);
    };
    let mut wire = Vec::new();
    encode(&reply, &mut wire).unwrap();
    to_worker.write_all(&wire).await.unwrap();
    let finished = from_worker.read(&mut buf).await.unwrap() == 0;
    drop(to_worker);
    (request, finished)
}

fn scratch(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("lattice-pair-{name}-{}", std::process::id()));
    drop(std::fs::remove_file(&path));
    path
}

async fn run(
    reply: Message,
    path: &Path,
) -> (Result<PairedDeck, PairError>, Option<(Message, bool)>) {
    let (worker_w, deck_r) = tokio::io::duplex(4096);
    let (deck_w, worker_r) = tokio::io::duplex(4096);
    let transport = OneDial(Mutex::new(Some((worker_w, worker_r))));
    let deck = tokio::spawn(deck(deck_r, deck_w, reply));
    let res = add_deck(&transport, &invite(), &card(), path, NOW).await;
    (res, deck.await.ok())
}

#[tokio::test]
async fn pair_ok_adds_the_deck_to_the_allowlist_before_finishing() {
    let path = scratch("ok");
    let ok = Message::PairOk {
        name: "minh-mbp".to_owned(),
    };
    let (res, deck) = run(ok, &path).await;
    let paired = res.unwrap();
    assert_eq!(
        paired,
        PairedDeck {
            id: id(2),
            name: "minh-mbp".to_owned()
        }
    );
    let (request, finished) = deck.unwrap();
    assert!(finished, "the worker finished its stream");
    let Message::PairRequest { secret, name, addr } = request else {
        panic!("expected PairRequest, got {request:?}");
    };
    assert!(secret.matches(invite().secret()));
    assert_eq!((name.as_str(), addr), ("build-01", card().addr));
    let list = Allowlist::parse(&std::fs::read_to_string(&path).unwrap());
    assert!(list.contains(&id(2)));
    std::fs::remove_file(path).unwrap();
}

#[tokio::test]
async fn a_denied_pairing_writes_nothing() {
    let path = scratch("denied");
    let denied = Message::PairDenied {
        code: "pair.bad_secret".to_owned(),
    };
    let (res, _) = run(denied, &path).await;
    assert_eq!(res, Err(PairError::Denied("pair.bad_secret".to_owned())));
    assert!(!path.exists());
}

#[tokio::test]
async fn expired_pair_secret_is_denied_before_dialing() {
    let path = scratch("expired");
    let transport = OneDial(Mutex::new(None));
    let late = invite().expires();
    let res = add_deck(&transport, &invite(), &card(), &path, late).await;
    assert_eq!(res, Err(PairError::Expired));
    assert!(!path.exists());
}

#[test]
fn error_codes_are_stable_and_namespaced() {
    let cases = [
        (PairError::Expired, "pair.expired"),
        (PairError::Denied(String::new()), "pair.denied"),
        (PairError::Timeout, "pair.timeout"),
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
