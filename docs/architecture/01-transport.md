# 01 — Transport (L1)

Status: draft
Owns: how two nodes connect, the relay wire format, path selection, channel layout.

## Summary

Two parts, both our own code on UDP:

- **P2P transport**: one UDP socket per node. Connect via the relay first, then hole-punch to a direct path.
- **Stream transport**: upstream `quinn` over the P2P transport. Streams, datagrams, TLS 1.3 with ed25519 raw public keys (RFC 7250).

The relay forwards opaque QUIC packets. It never decrypts and never decides who may connect.

Why not iroh: its relay path is TCP (HTTPS/WebSocket). We want UDP end to end and full control of each layer.

## Stack

| Concern | Choice |
|---|---|
| Runtime | `tokio` |
| QUIC | `quinn` 0.11+, with our own `AsyncUdpSocket` (`Endpoint::new_with_abstract_socket`) |
| TLS | `rustls`, raw public keys |
| Keys | `ed25519-dalek` |
| Port mapping (optional) | `portmapper` (UPnP / NAT-PMP / PCP) |

## Paths ("magic socket")

QUIC sees one stable virtual peer address per NodeId. Our socket picks the real path under it.

1. One UDP socket carries relay traffic, direct traffic, and punch probes (demuxed by first byte).
2. Outgoing QUIC packet → direct path if active, else wrapped in `SEND` to the relay.
3. Incoming `RECV` or direct packets are handed to quinn as coming from the virtual address.
4. Path rules:
   - Go direct when a probe round-trip succeeds both ways.
   - Fall back to relay when the direct path is silent for 2 s while the relay is alive.
   - Relay registration and keepalive never stop.
5. **Duplicate during switch**: for 1–2 s after a switch, send each packet on both paths. QUIC drops duplicates by packet number.
6. **Congestion reset**: a wrapper `ControllerFactory` resets cwnd when the socket signals a path switch. RTT re-converges from new samples.
7. **MTU**: cap quinn's max UDP payload so a packet plus the 41-byte `SEND` header fits the relay path. PMTUD cannot see path changes.

Known cost: quinn does not know the path changed. direct → relay may cause a short burst of loss or spurious PTO (hundreds of ms to ~1–2 s). Acceptable for terminal traffic.

Upgrade path: `noq` (quinn fork with QUIC multipath, per-path congestion control) behind the same `Transport` trait, if switch hiccups matter in practice.

## Relay wire format (UDP)

The first byte demultiplexes. `0x00–0x3F` = relay message. QUIC packets always set `0x40` or `0x80` (RFC 7983 trick).

```
REGISTER   [0x01][node_id 32][nonce 16][ts 8][sig 64]   sig = ed25519(relay_id || nonce || ts)
REG_OK     [0x02][token 8][observed addr 18][ttl 2]
KEEPALIVE  [0x03][token 8]
PONG       [0x04][observed addr 18]                      detects NAT rebinding
SEND       [0x10][token 8][dst node_id 32][QUIC packet]
RECV       [0x11][src node_id 32][QUIC packet]
PUNCH_REQ  [0x20][token 8][dst node_id 32][candidates]
PUNCH_GO   [0x21][peer node_id 32][peer candidates][punch nonce 16]
```

Rules:

1. Relay table: `token → (node_id, src addr, expiry)`.
2. `SEND` is accepted only from the source address bound to its token. Blocks spoofing and amplification.
3. Keepalive every 25 s. Many home NATs drop UDP mappings after ~30 s.
4. Rate limits attach to tokens. Not needed in v1.
5. Unknown message types are dropped silently.

## Connect sequence

1. Worker registers with its home relay on start and keeps it alive.
2. Deck knows the worker's NodeId and home relay from pairing (synced in the user Doc).
3. Deck registers with the same relay and sends QUIC Initial via `SEND`. Traffic flows on the relay at once.
4. In parallel: `PUNCH_REQ` → both sides get `PUNCH_GO` → simultaneous probes every 50–100 ms for ~5 s.
5. A probe round-trip validates the direct path. The magic socket switches to it; the relay stays registered as backup.

## Hole punching

Candidates: relay-observed address, LAN addresses, global IPv6, optional port-mapped address.

| Case | Result |
|---|---|
| IPv6 both sides | Usually direct (firewall only). Prefer it |
| Easy NAT ↔ easy NAT / no NAT | Direct |
| Hard (symmetric) ↔ easy | Birthday probing: hard side opens ~256 ports, easy side sprays ~2048 probes |
| Hard ↔ hard | Stay on relay |

## Channel layout on one connection

One QUIC connection per peer pair.

| Channel | QUIC primitive | Content |
|---|---|---|
| control | bi stream #0 | `Hello`, subscriptions, actions (sync.md) |
| stream data | one bi stream per subscribed stream | `Snapshot`, `Change`, `Ack` |
| input | one bi stream per interactive process | keystrokes, resize, signals |
| blobs | bi stream per request | `BlobRequest` → chunks |
| fast state (v2) | datagrams | mosh-style screen diffs |

1. One stream per process. Loss in one session never stalls another.
2. Keystrokes always go on a reliable stream.
3. Terminal output v1 = reliable stream. v2 may add datagram screen-state mode.
4. `path()` exposes `direct · 18ms` / `relay · 64ms` to the UI.

## Transport trait (our boundary)

```rust
trait Transport {
    async fn connect(&self, peer: NodeId) -> Result<Conn>;
    async fn accept(&self) -> Result<(NodeId, Conn)>;
    fn local_id(&self) -> NodeId;
}
trait Conn {
    async fn open_bi(&self) -> Result<(Send, Recv)>;
    async fn accept_bi(&self) -> Result<(Send, Recv)>;
    fn send_datagram(&self, b: Bytes) -> Result<()>;
    async fn recv_datagram(&self) -> Result<Bytes>;
    fn path(&self) -> PathInfo;
    fn close(&self, reason: CloseReason);
}
```

Nothing above L1 imports `quinn` types.

1. Dropping a `Conn` does not close it: the transport keeps the connection for reuse (D4). A host ends one on purpose with `close(reason)`.
2. `close(CloseReason::Refused)` closes the connection at once with an application close code. Every stream the peer has on it fails with `transport.refused`.

## Mobile (iOS)

iOS kills background sockets. Decks reconnect and resume every stream by cursor (sync.md). Workers keep all state.

## Web (deferred)

Not in v1. Design kept open:

- The relay can add a WebTransport listener. The browser runs `quinn-proto` in WASM and tunnels QUIC packets over one length-framed WebTransport stream. The relay re-emits them as UDP. E2E holds.
- Not datagrams: WebTransport datagrams are < 1200 bytes; QUIC Initial needs ≥ 1200.
- Constraints to revisit: Safari ≥ 26.4 only; whoever serves the JS can steal keys.

## Risks

- **UDP blocked** (some corporate networks): no fallback in v1. TBD: add a TCP/WebSocket relay path later?
- Path switches are invisible to quinn: brief hiccup on direct → relay (mitigated by duplicate sending and cwnd reset).
- Relay abuse (amplification, DoS): address-bound tokens now, rate limits later.
