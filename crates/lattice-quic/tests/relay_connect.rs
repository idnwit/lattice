//! Contract: two nodes connect through an in-process relay on localhost.

#![allow(
    clippy::unwrap_used,
    reason = "test harness helpers: a failure should abort the test"
)]

use lattice_key::{NodeKey, SecretKeyBytes};
use lattice_quic::{QuicConfig, QuicTransport};
use lattice_relay_proto::{DEFAULT_TTL, EngineOutput, Now, RelayEngine, SEND_HEADER_LEN, Token};
use lattice_transport::{
    Conn, HomeRelay, PathKind, RecvStream, SendStream, Transport, TransportError,
};
use std::net::SocketAddr;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tokio::net::UdpSocket;

fn key(seed: u8) -> NodeKey {
    NodeKey::from_secret(SecretKeyBytes::new([seed; 32]))
}

/// Minimal relay loop for tests. Mirrors `lattice-relay`'s driver.
async fn spawn_relay(bind: &str) -> HomeRelay {
    spawn_relay_keyed(bind, 100).await
}

/// [`spawn_relay`] with the relay key of `seed`.
async fn spawn_relay_keyed(bind: &str, seed: u8) -> HomeRelay {
    let relay_key = key(seed);
    let sock = UdpSocket::bind(bind).await.unwrap();
    let addr: SocketAddr = sock.local_addr().unwrap();
    let mut engine = RelayEngine::new(relay_key.node_id(), DEFAULT_TTL, [7; 32]);
    tokio::spawn(async move {
        let start = Instant::now();
        let mut buf = vec![0u8; 2048];
        let mut n_tok = 0u8;
        loop {
            let (n, src) = sock.recv_from(&mut buf).await.unwrap();
            // Paths rule 7: 1200-byte QUIC payload + the 41-byte SEND header.
            assert!(
                n <= 1200 + SEND_HEADER_LEN,
                "datagram of {n} bytes to the relay"
            );
            n_tok = n_tok.wrapping_add(1);
            let unix_secs = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();
            let out = engine.handle(
                Now::new(start.elapsed(), unix_secs),
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
async fn connect_via_relay_then_echo_on_a_bi_stream() {
    echo_via_relay("127.0.0.1:0").await;
}

/// Paths rule 9: on an IPv6 socket quinn sees peers in v4-mapped form.
#[tokio::test]
async fn connect_via_relay_on_ipv6_sockets() {
    echo_via_relay("[::1]:0").await;
}

async fn echo_via_relay(at: &str) {
    let relay = spawn_relay(at).await;
    let bind: SocketAddr = at.parse().unwrap();
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
        // Keep the worker endpoint alive until the deck has read the echo.
        let _closed = conn.accept_bi().await;
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

/// Connect sequence rule 3: no registration within 5 s fails instead of hanging.
#[tokio::test]
async fn connect_without_a_reachable_relay_is_unreachable() {
    // A bound socket that never answers stands in for a dead relay.
    let silent = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let relay = HomeRelay::new(silent.local_addr().unwrap(), key(100).node_id());
    let bind: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let deck = QuicTransport::bind(QuicConfig::new(key(2), relay, bind))
        .await
        .unwrap();
    let started = Instant::now();
    let err = deck.connect(key(1).node_id()).await.unwrap_err();
    assert_eq!(err, TransportError::Unreachable);
    assert!(started.elapsed() < std::time::Duration::from_secs(7));
}

/// specs/10-settings.md, rule 35: a new home relay applies on the live transport.
#[tokio::test]
async fn a_home_relay_change_re_registers_without_rebinding() {
    // The worker starts at a relay that never answers, then moves to a live one.
    let silent = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let dead = HomeRelay::new(silent.local_addr().unwrap(), key(101).node_id());
    let live = spawn_relay_keyed("127.0.0.1:0", 102).await;
    let bind: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let worker = QuicTransport::bind(QuicConfig::new(key(1), dead, bind))
        .await
        .unwrap();
    let before = worker.local_addr().unwrap();
    let handle = worker.home_relay_handle().unwrap();
    assert!(handle.set(live));
    assert_eq!(worker.local_addr().unwrap(), before, "same socket");
    let deck = QuicTransport::bind(QuicConfig::new(key(2), live, bind))
        .await
        .unwrap();
    let worker_id = worker.local_id();
    tokio::spawn(async move {
        let (_peer, conn) = worker.accept().await.unwrap();
        let _closed = conn.accept_bi().await;
    });
    let conn = deck.connect(worker_id).await.unwrap();
    assert_eq!(conn.remote_id(), worker_id);
}

#[tokio::test]
async fn direct_mode_has_no_home_relay_handle() {
    let bind: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let t = QuicTransport::bind(QuicConfig::direct(key(1), bind).unwrap())
        .await
        .unwrap();
    assert!(t.home_relay_handle().is_none());
}
