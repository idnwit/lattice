//! Contract: two nodes connect through an in-process relay on localhost.

#![allow(
    clippy::unwrap_used,
    reason = "test harness helpers: a failure should abort the test"
)]

use lattice_key::{NodeKey, SecretKeyBytes};
use lattice_quic::{QuicConfig, QuicTransport};
use lattice_relay_proto::{EngineOutput, RelayEngine, Token};
use lattice_transport::{Conn, HomeRelay, PathKind, RecvStream, SendStream, Transport};
use std::net::SocketAddr;
use std::time::{Duration, Instant};
use tokio::net::UdpSocket;

fn key(seed: u8) -> NodeKey {
    NodeKey::from_secret(SecretKeyBytes::new([seed; 32]))
}

/// Minimal relay loop for tests. Mirrors `lattice-relay`'s driver.
async fn spawn_relay() -> HomeRelay {
    let relay_key = key(100);
    let sock = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let addr: SocketAddr = sock.local_addr().unwrap();
    let mut engine = RelayEngine::new(relay_key.node_id(), Duration::from_secs(60));
    tokio::spawn(async move {
        let start = Instant::now();
        let mut buf = vec![0u8; 2048];
        let mut n_tok = 0u8;
        loop {
            let (n, src) = sock.recv_from(&mut buf).await.unwrap();
            n_tok = n_tok.wrapping_add(1);
            let out = engine.handle(
                start.elapsed(),
                src,
                &buf[..n],
                Token::from_bytes([n_tok; 8]),
            );
            match out {
                EngineOutput::Drop => {}
                EngineOutput::Reply(t) | EngineOutput::Forward(t) => {
                    sock.send_to(t.bytes(), t.dst()).await.unwrap();
                }
                EngineOutput::Punch(ts) => {
                    for t in ts {
                        sock.send_to(t.bytes(), t.dst()).await.unwrap();
                    }
                }
            }
        }
    });
    HomeRelay::new(addr, relay_key.node_id())
}

#[tokio::test]
#[ignore = "contract: lands with QuicTransport and MagicSocket"]
async fn connect_via_relay_then_echo_on_a_bi_stream() {
    let relay = spawn_relay().await;
    let bind: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let worker = QuicTransport::bind(QuicConfig::new(key(1), relay, bind))
        .await
        .unwrap();
    let deck = QuicTransport::bind(QuicConfig::new(key(2), relay, bind))
        .await
        .unwrap();
    let worker_id = worker.local_id();

    tokio::spawn(async move {
        let (_peer, conn) = worker.accept().await.unwrap();
        let (mut tx, mut rx) = conn.accept_bi().await.unwrap();
        let mut buf = [0u8; 4];
        let n = rx.read(&mut buf).await.unwrap().unwrap();
        tx.write_all(&buf[..n]).await.unwrap();
        tx.finish().unwrap();
    });

    let conn = deck.connect(worker_id).await.unwrap();
    assert_eq!(conn.remote_id(), worker_id);
    assert_eq!(
        conn.path().kind(),
        PathKind::Relay,
        "connect is relay-first"
    );
    let (mut tx, mut rx) = conn.open_bi().await.unwrap();
    tx.write_all(b"ping").await.unwrap();
    let mut buf = [0u8; 4];
    assert_eq!(rx.read(&mut buf).await.unwrap(), Some(4));
    assert_eq!(&buf, b"ping");
}
