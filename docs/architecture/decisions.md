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
| D13 | Apps = WASM (worker part + pure view part emitting our view tree) **from v1**. Built-in features (terminal, coding_agent, files, ports, git, metrics) are WASM apps on the same API | accepted | One extension path; dogfood the API; agents can build apps | 06 |
| D14 | Our own view-tree → NSH interpreter. Do not use NSH TSX plugins | proposed | NSH plugins are TSX/QuickJS only; conflicts with 100% Rust | 05, 06 |
| D15 | Layouts are data; generators may emit them | proposed | Safe for agent-authored layouts | 07 |
| D16 | Agent-built apps: coding agents follow our docs + `lattice` CLI to build and deploy apps | accepted | Owner intent ("shape it by asking") | 06 |
| D17 | Own status escape sequence (not herdr OSC 7501) | accepted | herdr is reference only | 04 |
| D18 | Hosted = public UDP relay only; rate limits later. Ciphertext store optional, later | accepted | No account server | 08 |
| D19 | Sync payload encoding = postcard, `u32` length-prefixed frames | accepted | 100% Rust; compact; serde. Cross-language not needed | 02 |
| D20 | Layer 1 crate graph per `brickie.toml`: `lattice-key` (base) holds `NodeId`; sync wire messages in `lattice-sync-proto`; NSH lives in `lattice-deck` only | accepted | Same-row crates cannot share `NodeId`; NSH is I/O with one user | 09 |
| D21 | Toolchain pinned in `rust-toolchain.toml` (1.100.0); MSRV 1.97 | accepted | NSH 0.7 needs rustc ≥ 1.97 | 05 |

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
3. Relay federation: deck and worker on different home relays?
4. Recovery when every deck is lost: offer a recovery key?
5. NSH: iOS sideload packaging on a device; Terminal element throughput and token colors. (Plain `cargo` build and the iOS simulator are verified.)
6. iOS view parts: interpreter (Pulley / wasmi) on device, or run on worker?
7. WIT world v1: exact host widget set and versioning scheme.
8. App runtime on worker and deck: wasmtime component model (WASI 0.3) confirmed?

## Sources

- Own transport: Tailscale NAT traversal https://tailscale.com/blog/how-nat-traversal-works/ · iroh on multipath https://iroh.computer/blog/iroh-on-QUIC-multipath · noq https://iroh.computer/blog/noq-announcement , https://docs.rs/crate/noq/1.2.0 · RFC 7983 demux
- WebTransport (for later web): https://caniuse.com/webtransport · https://developer.mozilla.org/en-US/docs/Web/API/WebTransportDatagramDuplexStream/maxDatagramSize
- NSH: https://nsh.dev/ · https://nsh.dev/docs/platforms.md · https://nsh.dev/elements/terminal.md · https://nsh.dev/docs/sandbox-and-capabilities.md
- Sync: mosh SSP https://mosh.org/mosh-paper.pdf · Loro https://loro.dev/blog/v1.0 · iroh-docs https://github.com/n0-computer/iroh-docs · Keyhive https://www.inkandswitch.com/keyhive/notebook/
- Worker: shpool https://github.com/shell-pool/shpool · systemd fd store https://systemd.io/FILE_DESCRIPTOR_STORE · Claude Code hooks https://code.claude.com/docs/en/hooks
- Apps: herdr https://herdr.dev/ · Zellij permissions https://zellij.dev/documentation/plugin-api-permissions · WASI 0.3 https://bytecodealliance.org/articles/WASI-0.3 · Pulley https://docs.wasmtime.dev/examples-pulley.html
