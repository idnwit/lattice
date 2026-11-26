//! Contract: direct-only mode on localhost (ROADMAP M1.5).
//!
//! Real UDP sockets on 127.0.0.1 only. Keys come from fixed seeds.

#![allow(
    clippy::unwrap_used,
    reason = "test harness helpers: a failure should abort the test"
)]

use bytes::Bytes;
use lattice_key::{NodeKey, SecretKeyBytes};
use lattice_quic::{ConfigError, QuicConfig, QuicTransport};
use lattice_transport::{
    CloseReason, Conn, PathKind, RecvStream, SendStream, Transport, TransportError,
};
use std::net::SocketAddr;
use std::time::Duration;

/// Upper bound for any step on localhost; a hang fails instead of blocking CI.
const STEP: Duration = Duration::from_secs(10);

fn key(seed: u8) -> NodeKey {
    NodeKey::from_secret(SecretKeyBytes::new([seed; 32]))
}

async fn node(seed: u8) -> QuicTransport {
    let bind: SocketAddr = "127.0.0.1:0".parse().unwrap();
    QuicTransport::bind(QuicConfig::direct(key(seed), bind).unwrap())
        .await
        .unwrap()
}

async fn read_to_end(rx: &mut impl RecvStream) -> Vec<u8> {
    let mut out = Vec::new();
    let mut buf = [0u8; 64];
    while let Some(n) = rx.read(&mut buf).await.unwrap() {
        out.extend_from_slice(&buf[..n]);
    }
    out
}

#[tokio::test]
async fn direct_localhost_echo_on_a_bi_stream() {
    let worker = node(1).await;
    let deck = node(2).await;
    let (worker_id, deck_id) = (worker.local_id(), deck.local_id());
    deck.add_peer(worker_id, worker.local_addr().unwrap());

    let server = tokio::spawn(async move {
        let (peer, conn) = worker.accept().await.unwrap();
        assert_eq!(conn.remote_id(), peer);
        let (mut tx, mut rx) = conn.accept_bi().await.unwrap();
        let got = read_to_end(&mut rx).await;
        tx.write_all(&got).await.unwrap();
        tx.finish().unwrap();
        // Keep the connection open until the deck has read the echo.
        let _closed = conn.accept_bi().await;
        peer
    });

    let conn = tokio::time::timeout(STEP, deck.connect(worker_id))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(conn.remote_id(), worker_id);
    assert_eq!(conn.path().kind(), PathKind::Direct);
    let (mut tx, mut rx) = conn.open_bi().await.unwrap();
    tx.write_all(b"ping").await.unwrap();
    tx.finish().unwrap();
    let echo = tokio::time::timeout(STEP, read_to_end(&mut rx))
        .await
        .unwrap();
    assert_eq!(echo, b"ping");
    // Dropping every handle closes the connection, which ends the worker task.
    drop((tx, rx, conn, deck));
    let seen_by_worker = tokio::time::timeout(STEP, server).await.unwrap().unwrap();
    assert_eq!(seen_by_worker, deck_id, "worker sees the deck's NodeId");
}

#[tokio::test]
async fn wrong_node_id_is_refused() {
    let worker = node(1).await;
    let deck = node(2).await;
    let impostor_id = key(3).node_id();
    // The deck expects key 3 at the worker's address; the worker holds key 1.
    deck.add_peer(impostor_id, worker.local_addr().unwrap());
    let server = tokio::spawn(async move { worker.accept().await });

    let got = tokio::time::timeout(STEP, deck.connect(impostor_id))
        .await
        .unwrap();
    assert_eq!(got.err(), Some(TransportError::IdentityMismatch));
    assert_eq!(
        TransportError::IdentityMismatch.code(),
        "transport.identity_mismatch"
    );
    // The failed handshake never surfaces as an accepted connection.
    assert!(
        tokio::time::timeout(Duration::from_millis(300), server)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn non_loopback_bind_is_refused() {
    for bad in [
        "0.0.0.0:0",
        "[::]:0",
        "192.168.1.10:4433",
        "[2001:db8::1]:0",
    ] {
        let addr: SocketAddr = bad.parse().unwrap();
        assert_eq!(
            QuicConfig::direct(key(1), addr).err(),
            Some(ConfigError::NonLoopbackBind),
            "case={bad}"
        );
    }
    for ok in ["127.0.0.1:0", "[::1]:0"] {
        let addr: SocketAddr = ok.parse().unwrap();
        assert!(QuicConfig::direct(key(1), addr).is_ok(), "case={ok}");
    }
}

#[tokio::test]
async fn unknown_peer_is_unreachable() {
    let deck = node(2).await;
    let got = deck.connect(key(1).node_id()).await;
    assert_eq!(got.err(), Some(TransportError::Unreachable));
}

#[tokio::test]
async fn datagrams_roundtrip_and_connect_reuses_the_connection() {
    let worker = node(1).await;
    let deck = node(2).await;
    let worker_id = worker.local_id();
    deck.add_peer(worker_id, worker.local_addr().unwrap());

    let server = tokio::spawn(async move {
        let (_peer, conn) = worker.accept().await.unwrap();
        let got = conn.recv_datagram().await.unwrap();
        conn.send_datagram(got).unwrap();
        let _closed = conn.accept_bi().await;
    });

    let conn = tokio::time::timeout(STEP, deck.connect(worker_id))
        .await
        .unwrap()
        .unwrap();
    conn.send_datagram(Bytes::from_static(b"tick")).unwrap();
    let echo = tokio::time::timeout(STEP, conn.recv_datagram())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(echo, Bytes::from_static(b"tick"));
    assert_eq!(
        conn.send_datagram(Bytes::from(vec![0; 64 * 1024])).err(),
        Some(TransportError::DatagramTooLarge)
    );

    // D4: one connection per peer pair. A second connect reuses it, so the
    // worker's single accept above is enough.
    let again = tokio::time::timeout(STEP, deck.connect(worker_id))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(again.remote_id(), worker_id);
    drop((conn, again, deck));
    tokio::time::timeout(STEP, server).await.unwrap().unwrap();
}

#[tokio::test]
async fn direct_lan_binds_any_address_and_reports_both_node_ids() {
    let server = QuicTransport::bind(QuicConfig::direct_lan(key(1), "0.0.0.0:0".parse().unwrap()))
        .await
        .unwrap();
    let client = QuicTransport::bind(QuicConfig::direct_lan(key(2), "0.0.0.0:0".parse().unwrap()))
        .await
        .unwrap();
    let port = server.local_addr().unwrap().port();
    let addr: SocketAddr = ([127, 0, 0, 1], port).into();
    client.add_peer(server.local_id(), addr);
    let accept = tokio::spawn(async move { server.accept().await.map(|(peer, _)| peer) });
    let conn = client.connect(key(1).node_id()).await.unwrap();
    assert_eq!(conn.remote_id(), key(1).node_id());
    assert_eq!(accept.await.unwrap().unwrap(), key(2).node_id());
}

/// The deck's first steps on a link: open the control stream, say hello, wait for an answer.
async fn first_exchange(conn: &impl Conn) -> Result<Option<usize>, TransportError> {
    let (mut tx, mut rx) = conn.open_bi().await?;
    tx.write_all(b"hello").await?;
    let mut buf = [0u8; 64];
    rx.read(&mut buf).await
}

#[tokio::test]
async fn a_refused_peer_fails_with_refused_instead_of_hanging() {
    let worker = node(1).await;
    let deck = node(2).await;
    let worker_id = worker.local_id();
    deck.add_peer(worker_id, worker.local_addr().unwrap());

    let server = tokio::spawn(async move {
        let (_peer, conn) = worker.accept().await.unwrap();
        conn.close(CloseReason::Refused);
        drop(conn);
        // The transport stays up, as in `latticed`: it must not keep the refused peer open.
        worker
    });

    let conn = tokio::time::timeout(STEP, deck.connect(worker_id))
        .await
        .unwrap()
        .unwrap();
    let _worker = tokio::time::timeout(STEP, server).await.unwrap().unwrap();
    let got = tokio::time::timeout(Duration::from_secs(2), first_exchange(&conn))
        .await
        .expect("the refusal arrives within 2 s");
    assert_eq!(got, Err(TransportError::Refused));
    assert_eq!(TransportError::Refused.code(), "transport.refused");
}

/// Runs `work` on its own single-thread runtime and drops the runtime as soon as `work`
/// returns, as a process exit does: tasks still due never run again.
fn run_and_exit<F: std::future::Future<Output = ()>>(work: impl FnOnce() -> F + Send + 'static) {
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(work());
    });
}

#[tokio::test]
async fn a_shutdown_before_exit_tells_the_peer_at_once() {
    // 01-transport.md, Transport trait rules 3–4; specs/01-pairing.md rule 9.
    let deck = node(2).await;
    let deck_id = deck.local_id();
    let deck_addr = deck.local_addr().unwrap();
    run_and_exit(move || async move {
        let worker = node(1).await;
        worker.add_peer(deck_id, deck_addr);
        let conn = worker.connect(deck_id).await.unwrap();
        let (mut tx, mut rx) = conn.open_bi().await.unwrap();
        tx.write_all(b"hello").await.unwrap();
        // The deck's reply: the stream is up both ways. Only the finish is left to send.
        let mut ack = [0u8; 3];
        let mut got = 0;
        while got < ack.len() {
            got += rx.read(&mut ack[got..]).await.unwrap().unwrap();
        }
        tx.finish().unwrap();
        worker.shutdown().await;
    });

    let (_, conn) = tokio::time::timeout(STEP, deck.accept())
        .await
        .unwrap()
        .unwrap();
    let (mut tx, mut rx) = tokio::time::timeout(STEP, conn.accept_bi())
        .await
        .unwrap()
        .unwrap();
    let mut buf = [0u8; 64];
    let mut got = 0;
    while got < 5 {
        got += rx.read(&mut buf).await.unwrap().unwrap();
    }
    tx.write_all(b"ack").await.unwrap();
    let started = tokio::time::Instant::now();
    let end = async {
        // The finish, or the close: either one ends the stream for the deck.
        while let Ok(Some(_)) = rx.read(&mut buf).await {}
    };
    let ended = tokio::time::timeout(Duration::from_secs(2), end).await;
    assert!(
        ended.is_ok(),
        "the deck still waits after {:?}: the end never left the worker",
        started.elapsed()
    );
}
