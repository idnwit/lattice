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
7. **MTU**: quinn's UDP payload is fixed at 1200 bytes, with PMTUD off. A packet plus the 41-byte `SEND` header is 1241 bytes, so it fits the relay path. PMTUD cannot see path changes.
8. **Virtual address**: each peer gets `240.0.0.0/4` (RFC 1112 class E, never routed). IP = `0xF0000000 | (node_id[0..4] as u32 & 0x0FFFFFFF)`, port = `node_id[4..6]` (big-endian). Port 0 becomes 1. It lives only inside the socket and never goes on the wire.
9. Two peers with the same virtual address: the second one takes the next port up, at most 16 ports (proposed). One socket maps at most 4,096 peers (proposed): a `RECV` can name any NodeId. A peer with no free address is dropped, and dialing it fails with `transport.unreachable`. On an IPv6 socket quinn sees each virtual address in its v4-mapped form. TBD: replace this placeholder scheme?
10. A datagram that is neither a relay message from the relay address nor QUIC from a known direct address is dropped.

Known cost: quinn does not know the path changed. direct → relay may cause a short burst of loss or spurious PTO (hundreds of ms to ~1–2 s). Acceptable for terminal traffic.

Upgrade path: `noq` (quinn fork with QUIC multipath, per-path congestion control) behind the same `Transport` trait, if switch hiccups matter in practice.

## Relay wire format (UDP)

The first byte demultiplexes. `0x00–0x3F` = relay message. QUIC packets always set `0x40` or `0x80` (RFC 7983 trick).

```
REGISTER   [0x01][node_id 32][nonce 16][ts 8][sig 64]   sig = ed25519(relay_id || nonce || ts)
REG_OK     [0x02][token 8][observed addr 18][ttl 2][nonce 16]   nonce echoes the REGISTER
KEEPALIVE  [0x03][token 8]
PONG       [0x04][observed addr 18]                      detects NAT rebinding
SEND       [0x10][token 8][dst node_id 32][QUIC packet]
RECV       [0x11][src node_id 32][QUIC packet]
PUNCH_REQ  [0x20][token 8][dst node_id 32][candidates]
PUNCH_GO   [0x21][peer node_id 32][peer candidates][punch nonce 16]
```

Fields:

| Field | Encoding |
|---|---|
| Integers (`ts`, `ttl`, port) | Big-endian |
| `ts` | Seconds since the UNIX epoch, node wall clock |
| `ttl` | Seconds the registration lives without a keepalive |
| `observed addr` | 16-byte IPv6 address (IPv4 as v4-mapped `::ffff:a.b.c.d`), then a 2-byte port |
| `candidates` | Raw bytes. TBD: encoding, decided with hole punching (M2.5) |

Bytes after the fixed fields of a fixed-size message are ignored. Later versions may append fields there.

Rules:

1. Relay table: `token → (node_id, src addr, expiry)`.
2. `SEND` is accepted only from the source address bound to its token. Blocks spoofing and amplification.
3. Keepalive every 25 s. Many home NATs drop UDP mappings after ~30 s.
4. Rate limits attach to tokens. Not needed in v1.
5. Unknown message types are dropped silently.
6. `REGISTER` is fresh when `|relay wall clock - ts| <= 30 s`. A stale one is dropped.
7. The relay remembers each accepted `(node_id, nonce)` until its `ts` is stale on the relay wall clock. A repeat inside that time is dropped. Blocks a replayed `REGISTER` from stealing the binding. Both checks use one clock, so a clock step never reopens a replay.
8. The nonce cache holds at most 65,536 entries. When full, new `REGISTER`s are dropped until entries expire.
9. A valid `REGISTER` gets a fresh random token. It replaces the node's previous token and address.
10. Default registration `ttl` = 90 s. It survives 2 lost keepalives (3 x 25 s = 75 s).
11. A `KEEPALIVE` from the bound address extends the registration by `ttl` and gets a `PONG`. A `KEEPALIVE` with a live token from another address gets a `PONG` with that address. It neither extends nor moves the binding.
12. `SEND` to a node with no live registration is dropped.
13. The client re-registers when it heard no `REG_OK` or `PONG` for 60 s. Unregistered, it sends `REGISTER` every 5 s until `REG_OK`.
14. `PUNCH_REQ` is dropped until hole punching lands (M2.5). TBD: source of the punch nonce?
15. The client accepts a `REG_OK` only when its nonce matches the last `REGISTER` it sent. A late or spoofed `REG_OK` cannot plant a dead token.
16. A `PONG` with a new observed address means a NAT rebinding. The client sends a `REGISTER` at once, then at most every 5 s. It keeps its token until the matching `REG_OK`. A spoofed `PONG` then costs one `REGISTER`, never the registration.
17. The relay checks the nonce cache before the signature. A repeated `REGISTER` costs no signature check.

## Relay invariants

Checks per [.rules/verification.md](../../.rules/verification.md).

1. `decode` never panics on any bytes. Check: proptest `decode_never_panics`, fuzz target `relay_decode`.
2. First byte `0x00–0x3F` is a relay message, anything else non-empty is QUIC. Check: test `relay_and_quic_split_on_first_byte`.
3. `decode(encode(m)) == m` for every message. Check: proptest `every_message_roundtrips`.
4. `SEND` is forwarded only from the address bound to a live token. Check: tests `send_accepted_only_from_the_tokens_bound_address`, `expired_token_is_refused`, `send_from_another_address_is_dropped`. Kani planned once stable.
5. A replayed or stale `REGISTER` changes nothing. Check: tests `replayed_register_is_dropped`, `stale_register_is_dropped`, `a_replay_after_a_wall_clock_step_back_is_dropped`.
6. Every message has the exact byte layout above. Check: test `every_message_has_its_spec_layout`.

## Path invariants

1. The selector moves to direct only on a validated probe. Check: proptest `switches_only_on_probe_or_direct_silence`.
2. It moves back to relay only after 2 s of direct silence while the relay is alive. Check: same proptest; tests `relay_fallback_after_two_seconds_of_direct_silence`, `no_fallback_while_relay_is_dead`.
3. Each switch opens exactly one duplicate window. Check: same proptest.

## Connect sequence

1. Worker registers with its home relay on start and keeps it alive.
2. Deck knows the worker's NodeId and home relay from pairing (synced in the user Doc).
3. Deck registers with the same relay and sends QUIC Initial via `SEND`. Traffic flows on the relay at once. `connect` waits at most 5 s for the registration, then fails with `transport.unreachable`.
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
    async fn shutdown(&self);
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
3. `shutdown()` closes every open connection with the generic close code, then waits until each peer was told, at most `SHUTDOWN_WAIT` = 1 s (proposed). A connection closed before keeps its close code.
4. A process calls `shutdown()` before it exits. Without it, packets still queued are never sent: a peer misses a stream's finish or the close, and waits for its idle timeout.

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
