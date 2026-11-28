# Decisions

Status per entry: `proposed` (needs owner OK) · `accepted` (owner agreed) · `rejected`.
Research date: 2026-11. Re-check versions before implementation.

## Log

| # | Decision | Status | Why | Ref |
|---|---|---|---|---|
| D1 | Two node types: worker and **deck** (control center). No trusted server, no account server | accepted | E2E by construction | README |
| D2 | L1 = our own UDP transport: relay first, then hole punch. Relay is a thin UDP forwarder | accepted | iroh relay path is TCP; we want UDP end to end and full control | 01 |
| D3 | L2 = upstream `quinn` over our own `AsyncUdpSocket` ("magic socket" picks relay or direct under one virtual address). `noq` multipath is the later upgrade | accepted | Mature upstream; terminal traffic tolerates switch hiccups; trait keeps the swap cheap | 01 |
| D4 | One QUIC connection per peer pair; one stream per process | proposed | No cross-session head-of-line blocking | 01 |
| D5 | Terminal output v1 = reliable stream; datagram screen-state mode later | proposed | Simple first | 01, 02 |
| D6 | Platforms: worker macOS + Linux; deck macOS and iOS in v1 (iOS sideloaded), web later; no Windows | accepted | Native first removes WebTransport, JS trust, Safari limits. iOS shares the whole deck core; only views differ | 05 |
| D7 | 100% Rust. Deck UI on NSH (pure-Rust mode, native views) | accepted | Owner choice; one language | 05 |
| D8 | One sync model: per-writer signed log + snapshot; kinds Log / State / Series / Doc | proposed | Covers terminal, events, metrics, config | 02 |
| D9 | Loro CRDT for multi-writer Doc | proposed | Rust + movable tree, stable 1.x | 02 |
| D10 | Identity = signed device list + shared content key (HPKE-sealed). No MLS in v1 | proposed | Single user, < 10 devices | 03 |
| D11 | Worker = `latticed-proc` (frozen) + `latticed` (upgradable) | accepted | Upgrade never kills processes | 04 |
| D12 | VT emulator in `latticed`, not `latticed-proc` | proposed | Emulator fixes must not restart the frozen daemon | 04 |
| D13 | Apps = WASM (runner + pure viewer emitting our view tree) **from v1**. Built-in features (terminal, coding_agent, files, ports, git, metrics) are WASM apps on the same API. M1 runs a native `terminal` runner behind the same `Runner` trait until M4 | accepted | One extension path; dogfood the API; agents can build apps | 06 |
| D14 | Our own view-tree → NSH interpreter. Do not use NSH TSX plugins | proposed | NSH plugins are TSX/QuickJS only; conflicts with 100% Rust | 05, 06 |
| D15 | Layouts are data; generators may emit them | proposed | Safe for agent-authored layouts | 07 |
| D16 | Agent-built apps: coding agents follow our docs + `lattice` CLI to build and deploy apps | accepted | Owner intent ("shape it by asking") | 06 |
| D17 | Own status escape sequence (not herdr OSC 7501) | accepted | herdr is reference only | 04 |
| D18 | Hosted = public UDP relay only; rate limits later. Ciphertext store optional, later | accepted | No account server | 08 |
| D19 | Sync payload encoding = postcard, `u32` length-prefixed frames | accepted | 100% Rust; compact; serde. Cross-language not needed | 02 |
| D20 | Layer 1 crate graph per `brickie.toml`: `lattice-key` (base) holds `NodeId`; sync wire messages in `lattice-sync-proto`; NSH lives in `lattice-deck` only (amended by D39) | accepted | Same-row crates cannot share `NodeId`; NSH is I/O with one user | 09 |
| D21 | Toolchain pinned in `rust-toolchain.toml` (1.100.0); MSRV 1.97 | accepted | NSH 0.7 needs rustc ≥ 1.97 | 05 |
| D22 | App = **runner** (worker) + **viewer** (deck). Replaces "worker part / view part" | accepted | Owner naming; the runner is not optional | 06 |
| D23 | Decks create **app instances** with `create{app, package, params}` on `worker/apps`; `close` stops, waits for exit, then deletes. Every process belongs to one instance | accepted | One entry point; apps without processes (metrics) | 06 |
| D24 | No auto-attach by argv or events. An agent typed in a shell stays part of its `terminal` instance | accepted | Agents open from the UI only | 06, specs/05 |
| D25 | Instances fully isolated: one runner sandbox each, nothing shared between instances. No shared app data | accepted | Fault and data isolation; simplest model | 06 |
| D26 | Restore only from `latticed-proc`: per-instance journal (meta + process output + exits), replay with effects dropped. Actions are never stored; each app decides what survives | accepted | Runner is a reducer over a pure process; `latticed` stays stateless | 04, 06 |
| D27 | Instance model and create API ship in M1 with the `terminal` app | accepted | Platform concept; terminal is the first instance | ROADMAP |
| D28 | `Trim{iid, upto}` lets a runner drop journal history at clean points (terminal: `clear`). No checkpoint | accepted | Keeps history small without app data in `latticed-proc` | 04, 06 |
| D29 | App = one Rust crate built twice (`runner`, `viewer` features) into a package keyed by blake3. A worker without the package pulls it from the deck on `create` | accepted | Both parts share model and API; no registry needed in v1 | 06 |
| D30 | Typed state as in `realtime-workspace` RFC-0005 §6: synced types with draft, commit, rollback, apply; no prediction | accepted | Changes cost O(touched); decks get state then deltas only | 06 |
| D31 | Unit of sync = whole instance state. 1024-delta queue per instance; a reader outside it gets the full state. Scrollback syncs in full until a dedicated type exists | accepted | Simplest model that still sends deltas | 02, 06 |
| D32 | Hook events and agent integration deferred until after the M1 terminal PoC | accepted | Simplest PoC first | 04, ROADMAP |
| D33 | Effects recorded in `Ctx`, run after commit, dropped on replay. Timers via `wake_at` | accepted | Deterministic replay; shell input goes straight to the process | 06 |
| D34 | SDK: synced types and derive copied from `rtf-core` and `rtf-macros` into `lattice-state` and `lattice-state-macros` | accepted | Proven code; encoding-agnostic via serde | 06 |
| D35 | A runner ends its instance with `finish`; `terminal` finishes when its shell exits. `close` sends `SIGHUP` and waits; `kill` sends `SIGKILL` from the tab context menu | accepted | No stuck panes; user stays in control | 06, specs/03 |
| D36 | Reducers are typed methods of an app-defined interface trait (`#[lattice_app::interface]`): the app chooses params, result, and error. The macro generates the action enum, the runner adapter, and a typed client with tickets, as `realtime-workspace` `#[rtf::interface]` | accepted | Simple, typed end to end; the platform moves opaque bytes | 06 |
| D37 | Hooks (`start`, `on_output`, `on_exit`, `on_gap`, `resumed`, `wake`) stay in the fixed `App` trait; only `on_output`, `on_exit`, `on_gap` replay | accepted | System events are the platform's; actions are the app's | 06 |
| D38 | `Res` of a reducer is a reply to the caller only: not stored, not replayed, never needed to render. App errors cross the wire as `{code, detail}` | accepted | State is the only source of views (as RFC-0005 TA-20) | 02, 06 |
| D39 | NSH lives in `lattice-deck`, the UI library `lattice-deck-ui` (components + desktop and mobile layouts, examples show the wiring), and the `viewer` feature of app crates through `lattice-deck-ui`. Interim until viewers are WASM with a view tree (M4) | accepted | Owner: viewers belong to their app; one component library | 05, 06 |
| D40 | M1 PoC deck = workspace only (tabs, splits, `+` menu with Echo and Terminal). A split opens an app picker, never a default app | accepted | Simplest PoC; platform not biased to the terminal | specs/03 |
| D41 | Mobile layout shows one region at a time (left panel, workspace, right panel) with a self-hiding switcher; desktop toggles panels beside the workspace | accepted | Owner | specs/03, 05 |
| D42 | Runners and viewers are WASM already in the M1 PoC, including `terminal`. Supersedes the native interim of D13/D27 and the native-viewer part of D39. `lattice-deck-ui` stays the component and layout library the deck's view-tree interpreter draws with | accepted | Owner: one path for every app from the first build | 05, 06 |
| D43 | WASM ABI v0 = core wasm + postcard: a few exports (`alloc`, `free`, `init`, `dispatch`, `hook`, `wake_at`, `snapshot` for runners; `load`, `apply`, `render`, `handle` for viewers), every value a postcard envelope. Component model + WIT later; the envelopes carry the same data | accepted | Simplest first; no componentize step; runs on wasmtime (and an interpreter on iOS) | 06 |
| D44 | The worker ships no app. Built-in apps live in `apps/` (one folder per app, outside `crates/`), build to packages (`app.toml`, `runner.wasm`, `viewer.wasm`, blake3), and are embedded in the deck. A worker missing a package fetches it from the creating deck over the link (`BlobRequest` → `Blob`) | accepted | Install later works the same way | 04, 06 |
| D45 | `[[spawn]]` argv is fixed by the manifest; `$SHELL` (the worker's login shell, fallback `/bin/sh`) is the only expansion. The runner's `Spawn` effect picks only the cwd (empty: home) and the initial PTY size. The worker host resolves templates; it no longer links per-app spawn code | accepted | Replaces native `SpawnFn` registration, which D44 removed with native apps | 04, 06 |
| D46 | Empty `create` params mean the app's default params (`App::Params: Default`). The deck creates from the `+` menu and the app picker with empty params; it links no app crate and never encodes an app's params | accepted | Deck holds only packages (D44); no params form in v1 (specs/03 rule 11) | 06 |
| D47 | Pairing runs from the deck: `Add worker` shows `lattice add-deck <invite>`; the user runs it on the worker, which dials the deck (pinning the deck key) and proves the invite secret. The deck learns the worker key from TLS, the worker learns the deck key from the invite. Replaces "the worker prints a code" (specs/01 before M1.6) | accepted; flow superseded by D53 | Owner: the deck user is already at the deck; copying one command to a shell is the natural step; mutual keys with no MITM window. Known limit: the worker must reach the deck's `ip:port`; a deck behind NAT or a VPN with no return route cannot pair (specs/01, Network requirements). Lifted for relay-backed decks by D49 | specs/01, ROADMAP |
| D48 | NodeId text form: 64 lowercase hex digits (CLI, files, logs). Invite codes use base32 for the whole payload | accepted | Used in `worker/decks` and `deck/workers` (earlier also `worker.addr`, removed by D53); unambiguous; no new dependency | specs/01, 03 |
| D49 | Every node (deck and worker) has its own home relay (`udp://host:port`, or `none` for direct only). Registered, it gets a public address (`Contact::Relay`: relay `host:port`, relay id), held beside the NodeId. A dialer registers at the target's relay and connects there; a node may hold several registrations | accepted | Owner. Lets peers on different relays connect (resolves relay federation), and pairing no longer needs a route to the deck's IP. Relays stay independent | 01, 08, specs/01, specs/02 |
| D50 | Relay id trust on first use: `COOKIE` appends `relay_id`; the node pins it (worker `config.toml`, deck device setting) and reports `relay.id_changed` after `ID_MISMATCH_LIMIT` mismatches. `udp://<id>@host:port` pins explicitly. A peer's address carries the id, so dialing needs no first contact | accepted | Owner. Config holds only a URL; the `REGISTER` signature needs the id. Trade-off: first-contact attacker can force a wrong pin (denial of service only; 03) | 01, 03, specs/02 |
| D51 | Settings come from a registry of schemas (`lattice.*` plus each app's `[settings]`). Scopes: device > user (synced `user/meta` Doc) > default. Worker settings live in the worker's `config.toml`, edited with `lattice config`, and published as the `worker/config` stream; v1 decks show them read-only. The v1 TOML view is read-only (owner's choice) | accepted | Owner. One model for the editor, mobile, and the TOML view; host-side validation | specs/10, 06 |
| D52 | ~~A CLI never registers at a relay with a node key while a process with that key runs. `latticed add-deck` and `lattice invite` send the request over a control socket in the runtime dir (`latticed.sock`, `deck.sock`; postcard frames); the running process dials with its existing registration. With no process running, the CLI registers itself~~ | superseded by D53 | Owner. Two `REGISTER`s of one NodeId at one relay replace each other's binding (01, Relay rule 9) | specs/01, 01 |
| D53 | Single-owner pairing: only the process that holds a node key pairs. Worker: `latticed`; `lattice add-deck` sends one `AddDeck` over `<data dir>/worker/latticed.sock` (same-uid peer check) and reads the code from stdin only. Deck: the GUI only, one in-memory invite; `New invite` replaces it. One round trip `PairRequest{secret, name, contact}` → `PairOk{name}`; a refusal is a transport close; the deck saves before `PairOk`. Invite = `{deck, Contact, secret}`, no expiry in the code. `lattice` CLI is worker-only (`add-deck`, `config`); `lattice invite`, `latticed add-deck`, `lattice attach`, `lattice probe`, `deck.sock`, the lock files and `worker.addr` are removed. Supersedes D52 | accepted | Owner. One pairing path per side; one registration per NodeId by construction; fewer messages, states, timers and codes. Cost: a lost `PairOk` leaves a deck entry the worker refuses (availability only) | specs/01, 01, 03 |

## Rejected options

| Option | Why not |
|---|---|
| iroh | Relay path is TCP (HTTPS/WebSocket); less control over layers |
| `noq` multipath in v1 | Young; kept as upgrade path behind `Transport` |
| libp2p | ~70% hole-punch success, relay v2 limits, heavy |
| Web deck in v1 (served from relay) | WebTransport + QUIC-in-WASM work; relay operator can serve malicious JS; Safari ≥ 26.4 only |
| iroh-docs for device sync | No E2E |
| Automerge + Keyhive | E2E layer is alpha |
| VT state inside the PTY-owning daemon | Upgrades kill sessions |
| NSH TSX plugins for apps | Brings JS into a Rust-only stack |
| Account server | Not needed; identity is the device list |

## Open questions

1. Name of our status escape sequence and its payload format (D17).
2. UDP-blocked networks: add a TCP/WebSocket relay path later?
3. ~~Relay federation: deck and worker on different home relays?~~ Resolved by D49.
4. Recovery when every deck is lost: offer a recovery key?
5. NSH: iOS sideload packaging on a device; Terminal element throughput and token colors. (Plain `cargo` build and the iOS simulator are verified.)
6. iOS viewers: interpreter (Pulley / wasmi) on device, or run on worker?
7. WIT world v1: exact host widget set and versioning scheme.
8. App runtime on worker and deck: wasmtime component model (WASI 0.3) confirmed?

## Sources

- Own transport: Tailscale NAT traversal https://tailscale.com/blog/how-nat-traversal-works/ · iroh on multipath https://iroh.computer/blog/iroh-on-QUIC-multipath · noq https://iroh.computer/blog/noq-announcement , https://docs.rs/crate/noq/1.2.0 · RFC 7983 demux
- WebTransport (for later web): https://caniuse.com/webtransport · https://developer.mozilla.org/en-US/docs/Web/API/WebTransportDatagramDuplexStream/maxDatagramSize
- NSH: https://nsh.dev/ · https://nsh.dev/docs/platforms.md · https://nsh.dev/elements/terminal.md · https://nsh.dev/docs/sandbox-and-capabilities.md
- Sync: mosh SSP https://mosh.org/mosh-paper.pdf · Loro https://loro.dev/blog/v1.0 · iroh-docs https://github.com/n0-computer/iroh-docs · Keyhive https://www.inkandswitch.com/keyhive/notebook/
- Worker: shpool https://github.com/shell-pool/shpool · systemd fd store https://systemd.io/FILE_DESCRIPTOR_STORE · Claude Code hooks https://code.claude.com/docs/en/hooks
- Apps: herdr https://herdr.dev/ · Zellij permissions https://zellij.dev/documentation/plugin-api-permissions · WASI 0.3 https://bytecodealliance.org/articles/WASI-0.3 · Pulley https://docs.wasmtime.dev/examples-pulley.html
