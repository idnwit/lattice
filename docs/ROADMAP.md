# Roadmap

Status: draft
Owns: milestone order, milestone scope, and exit criteria. Behavior lives in [specs/](specs/); design lives in [architecture/](architecture/).

## Summary

Four milestones, each one a working product slice:

1. **M1** Platform frame, app instances, and the terminal app over a direct link.
2. **M2** Relay, hole punching, real pairing, iOS deck.
3. **M3** First coding agent: Claude Code.
4. **M4** WASM runtime for runners and viewers, with node metrics as the first example.

Each milestone ends with an end-to-end test of its exit criteria.
Work order inside a milestone follows [architecture/09-platform-l1.md](architecture/09-platform-l1.md): bottom-up, one PR per crate or per sans-IO core.

## M1 — Platform frame, app instances, terminal app, direct link

Goal: a macOS deck creates a `terminal` app instance on a worker over QUIC on plain UDP. Restarting `latticed` never kills the shell and restores the instance.
The instance model and create API ship here (D27). The `terminal` runner is native Rust behind the `Runner` trait; WASM arrives in M4.

M1 is a temporary step away from D2 (relay first). The deck dials `ip:port` from the pairing payload. M2 restores D2.

| # | Goal | Crates | Done when |
|---|---|---|---|
| M1.1 | Proc IPC v1 codec: TLV, `Hello`, journals (`Open`, `Trim`, `Drop`), spawn, write, resize, subscribe, gap, exit | `lattice-proc-ipc` | Its 6 contract tests pass. Golden v1 fixtures and a fuzz target exist |
| M1.2 | Process daemon: PTYs, per-instance journals with budget and trim, fan-out, poll loop, unix socket | `latticed-proc` | A test client opens a journal, spawns `sh`, writes, and reads back. Journal tests pass across eviction and trim. Proptest covers the journal |
| M1.3 | Sync wire: postcard frames with `u32` length, `Hello`, `Subscribe`, `Change`, `Action` | `lattice-sync-proto` | Its 3 contract tests pass, including split reads |
| M1.4 | Local worker host: proc client, journal subscriptions, sessions, sync session to decks | `lattice-worker` | Its contract tests pass against a real `latticed-proc`, with no network |
| M1.4a | App SDK: synced types and `#[derive(Synced)]` (copied from `realtime-workspace`). Done: laws and postcard roundtrips pass under proptest | `lattice-state`, `lattice-state-macros` | Type laws hold under proptest: apply after commit, rollback, one encoding |
| M1.4b | Apps runner: `worker/apps`, `create`/`close`, params validation, state stream with a 1024-delta queue, native `terminal` app | `lattice-worker`, `lattice-sync-proto` | `create{terminal}` opens a journal and a shell. `close` waits for exit, then drops it. Two instances never read each other's state. A shell `exit` closes the instance |
| M1.4c | Restore from journals: rebuild runners from `meta`, replay with effects dropped, `clear` trims, `Gap` marks `degraded` | `lattice-worker` | After a `latticed` restart every instance returns with the same `iid` and screen. Replay writes nothing to a PTY |
| M1.5 | Direct-only transport: quinn on a plain UDP socket, TLS bound to the ed25519 node key, NodeId pinning | `lattice-quic`, `lattice-transport` | Two local processes echo on a bi stream. A wrong NodeId is refused |
| M1.6 | One-shot pairing by QR or connection code | `lattice`, `latticed`, `lattice-worker`, `lattice-deck` | See [M1.6 pairing](#m16-pairing) |
| M1.7 | Headless deck: worker link, session and pane model, create and close client, state subscriptions, `lattice attach` as a minimal terminal client | `lattice-deck-core`, `lattice` | A `terminal` instance on worker B is created and used from machine A over a LAN |
| M1.8 | macOS deck: `DeckState` replaces fixtures; terminal widget, input, resize, worker status | `lattice-deck` | Tabs and split panes work. `vim` and `htop` render correctly |
| M1.9 | Model proof: `latticed` restarts mid-session; the instance restores; the deck reconnects and replays from its offset | all | An end-to-end test shows the process and its instance survive. Output is complete, except ranges reported as a gap |

### M1.6 pairing

`lattice pair` lets exactly one deck pair. The worker then adds that deck's public key to its allowlist.

1. `lattice pair` on the worker prints a QR code and a text connection code in the terminal.
2. Both carry one payload: worker NodeId, `ip:port`, a 32-byte one-time secret, and an expiry.
3. The deck reads the payload. macOS pastes the code. iOS scans the QR (M2.7).
4. The deck dials `ip:port` and pins the worker NodeId from the payload. A different worker key aborts the dial.
5. Inside the authenticated QUIC connection, the deck sends `PairRequest{secret, deck name}`.
6. The worker takes the deck public key from the TLS handshake, never from the message.
7. A valid, unexpired, unused secret makes the worker do three things: add the deck NodeId to its allowlist, mark the secret used, and reply `PairOk{worker name}`.
8. Any other secret gets `PairDenied`, and the worker closes the connection.
9. A deck that is not in the allowlist cannot open any stream.
10. The allowlist persists across `latticed` restarts.

Differences from [specs/01-pairing.md](specs/01-pairing.md):

- M1 uses a full-entropy secret, so it needs no PAKE. The 6-character code and the PAKE arrive in M2.6.
- M1 keeps a local allowlist on the worker. The signed device list and the content key replace it in M2.6.
- The code TTL follows the spec: 5 minutes.

Tests: `pair_secret_is_single_use`, `expired_pair_secret_is_denied`, `unpaired_deck_cannot_open_a_stream`, `allowlist_survives_latticed_restart`.

### Blocking questions

From [architecture/09-platform-l1.md](architecture/09-platform-l1.md), plus pairing:

- TBD: IPC v1 type codes, TLV tag table, byte order, max frame length?
- TBD: width of `ProcId` on the wire (u32 or u64)?
- TBD: ring capacity per process?
- TBD: sync frame byte order, max frame length, stream id charset?
- TBD: canonical text form of a `NodeId`?
- TBD: proc socket path on macOS?
- TBD: M1 connection code encoding: base32 or base64url, and a prefix such as `lattice1:`?
- TBD: does "show the code in the browser" mean a local web page served by `latticed`? M1 assumes terminal output only.
- TBD: does M1 need `lattice unpair` to remove a deck?
- TBD: default journal budget per instance?

### Out of scope for M1

Relay, hole punching, PAKE, device list, content keys, iOS deck, WASM runtime for runners and viewers.

## M2 — Relay and real connectivity

Goal: deck and worker behind different NATs connect at once through the relay, then upgrade to direct.

| # | Goal | Crates |
|---|---|---|
| M2.1 | Relay codec, token table, relay engine, client registration FSM (sans-IO) | `lattice-relay-proto` |
| M2.2 | Public UDP relay binary. It forwards ciphertext and never decrypts | `lattice-relay` |
| M2.3 | Magic socket: relay and direct paths under one virtual address | `lattice-quic` |
| M2.4 | `PathSelector`: relay first, direct after a probe, back to relay after 2 s of direct silence | `lattice-transport` |
| M2.5 | Hole punching: candidate exchange and punch nonce | `lattice-quic` |
| M2.6 | Pairing per [specs/01-pairing.md](specs/01-pairing.md): 6-character code, PAKE, signed device list, content key | identity crate (new) |
| M2.7 | iOS deck: QR scan, network changes, path switches | `lattice-deck` |

Done when: losing the direct path falls back to relay without dropping the session. The M1 allowlist migrates into the device list.

## M3 — First coding agent: Claude Code

Goal: a Claude Code process on a worker shows as a TTY or Visual view, with approvals from the deck. Behavior: [specs/05-agents.md](specs/05-agents.md).

1. The deck creates a `coding_agent` instance (D23, D24). TBD: hook event capture, under D26 (restore only from `latticed-proc`).
2. Our own status escape sequence (D17) reports agent state.
3. TTY view embeds the `terminal` widget. Visual view shows plan, diffs, and approvals.
4. Agents overview, run panel, and composer.

- TBD: hook event schema for Claude Code?
- TBD: does a Visual-mode message write into PTY stdin or a side channel?
- TBD: where are "Always" approvals stored, and how are they revoked? D25 rules out shared app data.
- TBD: M3 before M4 means the Claude Code runner ships native behind the `Runner` trait, then ports to WASM in M4. Swap M3 and M4?

## M4 — WASM runtime for runners and viewers

Goal: runners and viewers run as WASM, one sandbox per instance (D25). The `metrics` app proves the model. Design: [architecture/06-apps.md](architecture/06-apps.md).

1. WIT world `lattice:app@1`, `app.toml` manifest, permissions.
2. Worker WASM host behind the `Runner` trait: wasmtime, one `Store` per instance, epoch deadlines, `ResourceLimiter`.
3. Deck app host: wasmtime on macOS. iOS uses an interpreter (decisions.md Q6).
4. View tree to NSH interpreter (D14) and the v1 host widget set.
5. `Series` stream kind for metrics.
6. `metrics` app: an instance with no process. The runner samples CPU, memory, and GPU; the viewer renders charts.
7. `lattice app build`, `lattice app install`, `lattice app reload`.
8. Port the native `terminal` runner to WASM. Port Claude Code too, if M3 shipped it native.

- TBD: WIT world v1 host widget set and versioning scheme (decisions.md Q7)?
- TBD: wasmtime component model with WASI 0.3 on worker and deck (decisions.md Q8)?
