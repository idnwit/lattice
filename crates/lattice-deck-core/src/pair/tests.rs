//! The invite check, and `accept_pair` over an in-memory transport.

use super::*;
use bytes::Bytes;
use lattice_key::{NodeKey, SecretKeyBytes};
use lattice_transport::{CloseReason, PathInfo, PathKind};
use std::sync::Mutex;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream};

const NOW: u64 = 1_800_000_000;

fn id(seed: u8) -> NodeId {
    NodeKey::from_secret(SecretKeyBytes::new([seed; 32])).node_id()
}

fn pending() -> PendingInvite {
    let addr = "192.168.1.9:5000".parse().unwrap();
    PendingInvite::new(id(1), addr, PairSecret::new([7; 32]), NOW)
}

fn wrong() -> PairSecret {
    PairSecret::new([8; 32])
}

#[test]
fn pair_secret_is_single_use() {
    let mut p = pending();
    let secret = p.invite().secret().clone();
    assert_eq!(p.check(&secret, NOW), Ok(()));
    assert_eq!(p.check(&secret, NOW), Err(PairDeny::Used));
}

#[test]
fn expired_pair_secret_is_denied() {
    let mut p = pending();
    let secret = p.invite().secret().clone();
    assert_eq!(p.invite().expires(), NOW + INVITE_TTL_SECS);
    assert_eq!(
        p.check(&secret, NOW + INVITE_TTL_SECS),
        Err(PairDeny::Expired)
    );
    assert_eq!(p.check(&secret, NOW + INVITE_TTL_SECS - 1), Ok(()));
}

#[test]
fn bad_pair_secret_is_denied_and_the_invite_stays_open() {
    let mut p = pending();
    let secret = p.invite().secret().clone();
    assert_eq!(p.check(&wrong(), NOW), Err(PairDeny::BadSecret));
    assert_eq!(p.check(&secret, NOW), Ok(()));
}

#[test]
fn the_command_carries_the_invite_code() {
    let p = pending();
    let cmd = p.command();
    let code = cmd.strip_prefix("lattice add-deck ").unwrap();
    assert_eq!(Invite::decode(code).as_ref(), Ok(p.invite()));
}

#[test]
fn deny_codes_are_stable_and_namespaced() {
    assert_eq!(PairDeny::BadSecret.code(), "pair.bad_secret");
    assert_eq!(PairDeny::Expired.code(), "pair.expired");
    assert_eq!(PairDeny::Used.code(), "pair.used");
}

/// One scripted worker connection: its NodeId, deck ← worker, deck → worker.
type Scripted = (NodeId, DuplexStream, DuplexStream);

/// Hands out one scripted worker connection per entry (last first), then closes.
struct Workers(Mutex<Vec<Scripted>>);
struct Accepted(NodeId, Mutex<Option<(DuplexStream, DuplexStream)>>);
struct PipeSend(Option<DuplexStream>);
struct PipeRecv(DuplexStream);

impl Transport for Workers {
    type Conn = Accepted;

    async fn connect(&self, _peer: NodeId) -> Result<Accepted, TransportError> {
        Err(TransportError::Unreachable)
    }

    async fn accept(&self) -> Result<(NodeId, Accepted), TransportError> {
        let next = self.0.lock().unwrap().pop();
        match next {
            Some((peer, r, w)) => Ok((peer, Accepted(peer, Mutex::new(Some((r, w)))))),
            None => Err(TransportError::Closed),
        }
    }

    fn local_id(&self) -> NodeId {
        id(1)
    }
}

impl Conn for Accepted {
    type Send = PipeSend;
    type Recv = PipeRecv;

    async fn open_bi(&self) -> Result<(PipeSend, PipeRecv), TransportError> {
        Err(TransportError::Closed)
    }

    async fn accept_bi(&self) -> Result<(PipeSend, PipeRecv), TransportError> {
        let (r, w) = self
            .1
            .lock()
            .unwrap()
            .take()
            .ok_or(TransportError::Closed)?;
        Ok((PipeSend(Some(w)), PipeRecv(r)))
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
        self.0
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
        self.0.take().map(drop).ok_or(TransportError::Closed)
    }
}

impl RecvStream for PipeRecv {
    async fn read(&mut self, buf: &mut [u8]) -> Result<Option<usize>, TransportError> {
        match self.0.read(buf).await {
            Ok(0) => Ok(None),
            Ok(n) => Ok(Some(n)),
            Err(e) => Err(TransportError::Io(e.to_string())),
        }
    }
}

/// A worker that sends `PairRequest{secret}`, reads the answer, then finishes.
async fn worker(
    mut to_deck: DuplexStream,
    mut from_deck: DuplexStream,
    secret: PairSecret,
) -> Message {
    let req = Message::PairRequest {
        secret,
        name: "build-01".to_owned(),
        addr: "192.168.1.5:7441".parse().unwrap(),
    };
    let mut wire = Vec::new();
    encode(&req, &mut wire).unwrap();
    to_deck.write_all(&wire).await.unwrap();
    let mut dec = FrameDecoder::new();
    let mut buf = vec![0u8; 1024];
    let reply = loop {
        if let Some(Decoded::Message(m)) = dec.next().unwrap() {
            break m;
        }
        let n = from_deck.read(&mut buf).await.unwrap();
        dec.push(&buf[..n]);
    };
    drop(to_deck);
    reply
}

/// One scripted worker: the transport entry and the worker task.
fn dial(peer: NodeId, secret: PairSecret) -> (Scripted, tokio::task::JoinHandle<Message>) {
    let (w_to_d, d_from_w) = tokio::io::duplex(4096);
    let (d_to_w, w_from_d) = tokio::io::duplex(4096);
    let task = tokio::spawn(worker(w_to_d, w_from_d, secret));
    ((peer, d_from_w, d_to_w), task)
}

#[tokio::test]
async fn a_good_secret_pairs_with_the_tls_node_id_after_a_bad_one() {
    let mut p = pending();
    let good = p.invite().secret().clone();
    let (bad_conn, bad_task) = dial(id(3), wrong());
    let (good_conn, good_task) = dial(id(2), good);
    // `accept` pops from the end: the bad worker comes first.
    let transport = Workers(Mutex::new(vec![good_conn, bad_conn]));
    let mut saved = Vec::new();
    let save = |w: &PairedWorker| {
        saved.push(w.clone());
        Ok(())
    };
    let mut steps = Vec::new();
    let worker = accept_pair(
        &transport,
        &mut p,
        "minh-mbp",
        || NOW,
        save,
        |s| {
            steps.push(s);
        },
    )
    .await
    .unwrap();
    let want = [PairStep::Serving, PairStep::Waiting, PairStep::Serving];
    assert_eq!(steps, want, "the bad callback goes back to waiting");
    assert_eq!(worker.id, id(2), "the id comes from the connection");
    assert_eq!(
        (worker.name.as_str(), worker.addr.port()),
        ("build-01", 7441)
    );
    assert_eq!(saved, [worker]);
    assert!(p.is_used());
    let denied = Message::PairDenied {
        code: "pair.bad_secret".to_owned(),
    };
    assert_eq!(bad_task.await.unwrap(), denied);
    let ok = Message::PairOk {
        name: "minh-mbp".to_owned(),
    };
    assert_eq!(good_task.await.unwrap(), ok);
}

#[tokio::test]
async fn an_expired_invite_denies_and_stops() {
    let mut p = pending();
    let secret = p.invite().secret().clone();
    let (conn, task) = dial(id(2), secret);
    let transport = Workers(Mutex::new(vec![conn]));
    let late = NOW + INVITE_TTL_SECS;
    let res = accept_pair(&transport, &mut p, "deck", || late, |_| Ok(()), |_| {}).await;
    assert_eq!(res, Err(DeckError::Remote("pair.expired".to_owned())));
    let denied = Message::PairDenied {
        code: "pair.expired".to_owned(),
    };
    assert_eq!(task.await.unwrap(), denied);
}

#[tokio::test]
async fn a_failed_save_denies_the_worker() {
    let mut p = pending();
    let secret = p.invite().secret().clone();
    let (conn, task) = dial(id(2), secret);
    let transport = Workers(Mutex::new(vec![conn]));
    let fail = |_: &PairedWorker| Err("io".to_owned());
    let res = accept_pair(&transport, &mut p, "deck", || NOW, fail, |_| {}).await;
    assert_eq!(res, Err(DeckError::Remote("io".to_owned())));
    let denied = Message::PairDenied {
        code: "pair.save_failed".to_owned(),
    };
    assert_eq!(task.await.unwrap(), denied);
}
