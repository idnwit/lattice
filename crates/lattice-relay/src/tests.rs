//! Relay loop over real localhost sockets.

use super::*;
use lattice_relay_proto::{ClientEvent, RelayClient};

fn key(seed: u8) -> NodeKey {
    NodeKey::from_secret(SecretKeyBytes::new([seed; 32]))
}

fn unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

async fn spawn_relay(relay: &NodeKey) -> SocketAddr {
    let sock = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let addr = sock.local_addr().unwrap();
    let engine = RelayEngine::new(relay.node_id(), DEFAULT_TTL, [7; 32]);
    let tokens = TokenSource::open().unwrap();
    tokio::spawn(async move { serve(sock, engine, tokens).await });
    addr
}

/// A node socket registered at `relay`: REGISTER, COOKIE, REGISTER with the cookie, REG_OK.
async fn registered(
    relay: SocketAddr,
    relay_key: &NodeKey,
    node: &NodeKey,
) -> (UdpSocket, RelayClient) {
    let sock = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let mut client = RelayClient::new(relay_key.node_id());
    let mut buf = [0u8; 2048];
    let seed = node.node_id().as_bytes()[0];

    // Rule 18: the first REGISTER carries no cookie and gets a COOKIE.
    let mut out = Vec::new();
    client.register(node, [seed; 16], unix_secs(), &mut out);
    sock.send_to(&out, relay).await.unwrap();
    let n = recv(&sock, &mut buf).await;
    let event = client.on_datagram(Duration::ZERO, &buf[..n]);
    assert_eq!(event, Some(ClientEvent::RegisterDue), "expected COOKIE");

    // Rule 22: register again at once, with the cookie and a new nonce.
    out.clear();
    client.register(node, [seed.wrapping_add(1); 16], unix_secs(), &mut out);
    sock.send_to(&out, relay).await.unwrap();
    let n = recv(&sock, &mut buf).await;
    let event = client.on_datagram(Duration::ZERO, &buf[..n]);
    let Some(ClientEvent::Registered { observed }) = event else {
        panic!("expected REG_OK, got {event:?}");
    };
    assert_eq!(observed.addr(), sock.local_addr().unwrap());
    (sock, client)
}

async fn recv(sock: &UdpSocket, buf: &mut [u8]) -> usize {
    let got = tokio::time::timeout(Duration::from_secs(2), sock.recv(buf)).await;
    got.expect("relay answered within 2 s").unwrap()
}

#[tokio::test]
async fn two_nodes_exchange_a_packet_through_the_relay() {
    let relay_key = key(100);
    let relay = spawn_relay(&relay_key).await;
    let (a, b) = (key(1), key(2));
    let (a_sock, a_client) = registered(relay, &relay_key, &a).await;
    let (b_sock, mut b_client) = registered(relay, &relay_key, &b).await;

    let mut out = Vec::new();
    a_client
        .wrap_send(b.node_id(), &[0x40, 1, 2, 3], &mut out)
        .unwrap();
    a_sock.send_to(&out, relay).await.unwrap();

    let mut buf = [0u8; 2048];
    let n = recv(&b_sock, &mut buf).await;
    let Some(ClientEvent::Packet { src, packet }) = b_client.on_datagram(Duration::ZERO, &buf[..n])
    else {
        panic!("expected RECV");
    };
    assert_eq!((src, &buf[packet]), (a.node_id(), &[0x40, 1, 2, 3][..]));
}

#[test]
fn relay_key_survives_a_restart() {
    let dir = std::env::temp_dir().join(format!("lattice-relay-key-{}", std::process::id()));
    drop(std::fs::remove_dir_all(&dir));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("relay.key");
    let first = load_or_create_key(&path).unwrap().node_id();
    let again = load_or_create_key(&path).unwrap().node_id();
    assert_eq!(first, again);
    std::fs::remove_dir_all(dir).unwrap();
}
