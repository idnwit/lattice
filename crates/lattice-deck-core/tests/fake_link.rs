//! Contract: a deck link over an in-memory fake transport.

#![allow(
    clippy::unwrap_used,
    reason = "test harness helpers: a failure should abort the test"
)]

use bytes::Bytes;
use lattice_app::{App, CallError, Ctx, ExitStatus, InstanceId, ProcRef};
use lattice_deck_core::DeckHost;
use lattice_key::{NodeId, NodeKey, SecretKeyBytes};
use lattice_state::Value;
use lattice_sync_proto::{ActionId, Decoded, FrameDecoder, Message, encode};
use lattice_transport::{
    CloseReason, Conn, PathInfo, PathKind, RecvStream, SendStream, Transport, TransportError,
};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::time::Duration;

fn id(seed: u8) -> NodeId {
    NodeKey::from_secret(SecretKeyBytes::new([seed; 32])).node_id()
}

/// Bytes the deck wrote on the control stream.
type Sent = Arc<Mutex<Vec<u8>>>;

struct FakeTransport {
    local: NodeId,
    worker: NodeId,
    sent: Sent,
    /// What the worker sends after `Hello`, then the stream ends.
    script: Vec<Message>,
}

struct FakeConn {
    remote: NodeId,
    sent: Sent,
    /// Worker's scripted answer on the control stream.
    reply: Vec<u8>,
}

struct FakeSend(Sent);
struct FakeRecv(Vec<u8>);

impl Transport for FakeTransport {
    type Conn = FakeConn;

    async fn connect(&self, peer: NodeId) -> Result<FakeConn, TransportError> {
        let mut reply = Vec::new();
        let hello = Message::Hello {
            node_id: self.worker,
            device_list_head: None,
            versions: vec![1],
        };
        encode(&hello, &mut reply).unwrap();
        for msg in &self.script {
            encode(msg, &mut reply).unwrap();
        }
        Ok(FakeConn {
            remote: peer,
            sent: self.sent.clone(),
            reply,
        })
    }

    async fn accept(&self) -> Result<(NodeId, FakeConn), TransportError> {
        Err(TransportError::Closed)
    }

    fn local_id(&self) -> NodeId {
        self.local
    }
}

impl Conn for FakeConn {
    type Send = FakeSend;
    type Recv = FakeRecv;

    async fn open_bi(&self) -> Result<(FakeSend, FakeRecv), TransportError> {
        Ok((FakeSend(self.sent.clone()), FakeRecv(self.reply.clone())))
    }

    async fn accept_bi(&self) -> Result<(FakeSend, FakeRecv), TransportError> {
        Err(TransportError::Closed)
    }

    fn send_datagram(&self, _bytes: Bytes) -> Result<(), TransportError> {
        Err(TransportError::Closed)
    }

    async fn recv_datagram(&self) -> Result<Bytes, TransportError> {
        Err(TransportError::Closed)
    }

    fn path(&self) -> PathInfo {
        PathInfo::new(PathKind::Relay, Duration::from_millis(64))
    }

    fn remote_id(&self) -> NodeId {
        self.remote
    }

    fn close(&self, _reason: CloseReason) {
        // The deck never closes a link on purpose in these tests.
    }
}

impl SendStream for FakeSend {
    async fn write_all(&mut self, bytes: &[u8]) -> Result<(), TransportError> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(())
    }

    fn finish(&mut self) -> Result<(), TransportError> {
        Ok(())
    }
}

impl RecvStream for FakeRecv {
    async fn read(&mut self, buf: &mut [u8]) -> Result<Option<usize>, TransportError> {
        if self.0.is_empty() {
            return Ok(None);
        }
        let n = buf.len().min(self.0.len());
        buf[..n].copy_from_slice(&self.0[..n]);
        self.0.drain(..n);
        Ok(Some(n))
    }
}

#[tokio::test]
async fn open_link_sends_hello_first_and_reports_the_path() {
    let sent = Sent::default();
    let host = DeckHost::new(FakeTransport {
        local: id(2),
        worker: id(1),
        sent: sent.clone(),
        script: Vec::new(),
    });
    let mut link = host.open_link(id(1)).await.unwrap();
    assert_eq!(link.path().to_string(), "relay · 64ms");

    let mut dec = FrameDecoder::new();
    let hello_len = sent.lock().unwrap().len();
    dec.push(&sent.lock().unwrap());
    let first = dec.next().unwrap();
    assert!(
        matches!(first, Some(Decoded::Message(Message::Hello { node_id, .. })) if node_id == id(2))
    );

    link.instance_action(lattice_app::InstanceId::from_raw(1), b"ls\r".to_vec())
        .await
        .unwrap();
    link.subscribe("inst/1/state".parse().unwrap())
        .await
        .unwrap();
    dec.push(&sent.lock().unwrap()[hello_len..]);
    assert!(
        matches!(dec.next().unwrap(), Some(Decoded::Message(Message::Action { target, payload, .. })) if target.as_str() == "inst/1/state" && payload == b"ls\r")
    );
    assert!(
        matches!(dec.next().unwrap(), Some(Decoded::Message(Message::Subscribe { stream, .. })) if stream.as_str() == "inst/1/state")
    );
    assert!(
        link.next_message().await.is_err(),
        "fake stream ends after Hello"
    );
}

/// A pane-size app. The fake worker answers through its generated runner adapter.
struct Pane;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct Size {
    cols: u16,
    rows: u16,
}

#[derive(Debug, PartialEq, Serialize, Deserialize, thiserror::Error)]
enum PaneError {
    #[error("bad size")]
    BadSize,
}

impl App for Pane {
    const ID: &'static str = "test.pane";
    type State = Value<Size>;
    type Params = ();
    type Error = PaneError;
    fn new(_: &()) -> Self {
        Self
    }
    fn start(&mut self, _: &mut Ctx, _: &mut Value<Size>) -> Result<(), PaneError> {
        Ok(())
    }
    fn on_output(&mut self, _: &mut Ctx, _: &mut Value<Size>, _: ProcRef, _: &[u8]) {}
    fn on_exit(&mut self, _: &mut Ctx, _: &mut Value<Size>, _: ProcRef, _: ExitStatus) {}
    fn on_gap(&mut self, _: &mut Ctx, _: &mut Value<Size>) {}
}

impl Default for Size {
    fn default() -> Self {
        Self { cols: 80, rows: 24 }
    }
}

#[lattice_app::interface]
trait PaneApi: App<State = Value<Size>, Params = (), Error = PaneError> {
    fn resize(
        &mut self,
        ctx: &mut Ctx,
        state: &mut Value<Size>,
        req: Size,
    ) -> Result<Size, PaneError>;
}

impl PaneApi for Pane {
    fn resize(
        &mut self,
        _: &mut Ctx,
        state: &mut Value<Size>,
        req: Size,
    ) -> Result<Size, PaneError> {
        if req.cols == 0 || req.rows == 0 {
            return Err(PaneError::BadSize);
        }
        state.set(req);
        Ok(req)
    }
}

/// What a worker running `Pane` answers to `action`.
fn worker_reply(action: &[u8]) -> Vec<u8> {
    use lattice_app::Dispatch;
    let mut state = Value::new(Size::default());
    PaneApiRunner::<Pane>::dispatch(&mut Pane, &mut Ctx::new(), &mut state, action).unwrap()
}

#[tokio::test]
async fn a_typed_call_resolves_from_the_worker_reply_and_link_loss_gives_unknown() {
    let size = Size {
        cols: 100,
        rows: 30,
    };
    let sent = Sent::default();
    let host = DeckHost::new(FakeTransport {
        local: id(2),
        worker: id(1),
        sent: sent.clone(),
        // The first action of a link has id 1.
        script: vec![Message::ActionResult {
            id: ActionId::new(1),
            error: None,
            result: worker_reply(&lattice_state::encode(&PaneApiAction::Resize(size)).unwrap()),
            detail: Vec::new(),
        }],
    });
    let mut link = host.open_link(id(1)).await.unwrap();
    let client = PaneApiClient::new(link.caller(InstanceId::from_raw(7)));
    let mut answered = client.resize(size);
    link.flush_calls().await.unwrap();
    assert!(matches!(
        link.next_message().await.unwrap(),
        Message::ActionResult { .. }
    ));
    assert_eq!(answered.try_take(), Some(Ok(size)));

    let lost = client.resize(size);
    link.flush_calls().await.unwrap();
    assert!(link.next_message().await.is_err(), "the fake stream ends");
    assert_eq!(lost.await, Err(CallError::Unknown));
    let after = client.resize(size).await;
    assert_eq!(after, Err(CallError::NotConnected));

    // The action went to the instance's state stream, encoded as `PaneApiAction::Resize`.
    let mut dec = FrameDecoder::new();
    dec.push(&sent.lock().unwrap());
    let _hello = dec.next().unwrap();
    let want = lattice_state::encode(&PaneApiAction::Resize(size)).unwrap();
    assert!(matches!(
        dec.next().unwrap(),
        Some(Decoded::Message(Message::Action { target, payload, .. }))
            if target.as_str() == "inst/7/state" && payload == want
    ));
}
