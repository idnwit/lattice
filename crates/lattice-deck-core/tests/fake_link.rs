//! Contract: a deck link over an in-memory fake transport.

#![allow(
    clippy::unwrap_used,
    reason = "test harness helpers: a failure should abort the test"
)]

use bytes::Bytes;
use lattice_deck_core::DeckHost;
use lattice_key::{NodeId, NodeKey, SecretKeyBytes};
use lattice_sync_proto::{Decoded, FrameDecoder, Message, encode};
use lattice_transport::{
    Conn, PathInfo, PathKind, RecvStream, SendStream, Transport, TransportError,
};
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
        encode(&hello, &mut reply);
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
#[ignore = "contract: lands with DeckHost::open_link"]
async fn open_link_sends_hello_first_and_reports_the_path() {
    let sent = Sent::default();
    let host = DeckHost::new(FakeTransport {
        local: id(2),
        worker: id(1),
        sent: sent.clone(),
    });
    let mut link = host.open_link(id(1)).await.unwrap();
    assert_eq!(link.path().to_string(), "relay · 64ms");

    let mut dec = FrameDecoder::new();
    dec.push(&sent.lock().unwrap());
    let first = dec.next().unwrap();
    assert!(
        matches!(first, Some(Decoded::Message(Message::Hello { node_id, .. })) if node_id == id(2))
    );

    link.send_input("proc/1/raw".parse().unwrap(), b"ls\r".to_vec())
        .await
        .unwrap();
    link.subscribe("proc/1/raw".parse().unwrap()).await.unwrap();
    assert!(
        link.next_message().await.is_err(),
        "fake stream ends after Hello"
    );
}
