# 09 — Platform layer 1

Status: draft
Owns: the layer-1 crate map, skeleton status, contract tests, and open TBDs that block implementation.

## Summary

Layer 1 is the platform foundation: transport, relay, sync wire protocol, worker, and decks (macOS, iOS).
The crate graph lives in `brickie.toml` (D20). Rules: [.rules/architecture.md](../../.rules/architecture.md).
Every crate exists as a skeleton: types, signatures, docs, and commented bodies.
Each unimplemented body carries `#[allow(clippy::todo, reason = "L1 skeleton: …")]`.
Contract tests are `#[ignore = "contract: …"]`. Implementing a body un-ignores its tests (verification.md rule 4).

## Crates

| Crate | Row | State | Owner spec |
|---|---|---|---|
| `lattice-key` | base | implemented, tested | 03-identity-security |
| `lattice-proc-ipc` | frozen | skeleton | 04-worker (Local IPC) |
| `lattice-transport` | contract | skeleton; `PathInfo` implemented | 01-transport |
| `lattice-relay-proto` | contract | skeleton | 01-transport (Relay wire format) |
| `lattice-sync-proto` | contract | skeleton | 02-sync (Wire messages) |
| `lattice-quic` | adapter | skeleton | 01-transport (Paths) |
| `lattice-worker` | host | skeleton | 04-worker, 02-sync |
| `lattice-deck-core` | host | skeleton | 05-deck, specs/03-sessions |
| `latticed-proc` | bin (`proc`) | skeleton | 04-worker |
| `latticed` | bin | skeleton | 04-worker |
| `lattice-relay` | bin (`relay`) | skeleton | 08-infrastructure |
| `lattice-deck` | bin | UI demo on fixtures | 05-deck, design-system |
| `lattice` | bin | placeholder | — |

## Run the deck demo

| Target | Command | Notes |
|---|---|---|
| macOS, desktop layout | `cargo run -p lattice-deck` | 1440×900 window. The app activates itself on open |
| macOS, mobile layout | `LATTICE_DECK_LAYOUT=mobile cargo run -p lattice-deck` | 390×844 window |
| iOS simulator | `cd crates/lattice-deck && nsh run --target ios --device "iPhone 16 Pro"` | Needs the `nsh` CLI 0.7.0 (`brew install longbridge/tap/nsh`), Xcode, an iOS runtime |

Interactive flows run on the real platforms with `nsh test nsh.json --target macos|ios`, steps on stdin (`await app.press("role:tab[db-shell]")`, `await app.screenshot("x.png")`).
The iOS test host builds QuickJS (nsh tooling, not the product). It needs `BINDGEN_EXTRA_CLANG_ARGS_aarch64_apple_ios_sim="--sysroot=$(xcrun --sdk iphonesimulator --show-sdk-path)"`.

NSH 0.7 uses window model v2: the host window never shows, so `DeckApp` declares its own `window()`.
The `nsh` CLI finds the root through `nsh.json` (`"root": "DeckApp"`) and the crate's `#[native_module]`.

## Implementation order

Bottom-up, one PR per crate or per sans-IO core (architecture.md, Brickie):

1. `lattice-proc-ipc` codec, then `latticed-proc` ring and fan-out, then its poll loop.
2. `lattice-relay-proto` codec, token table, engine, client; then `lattice-relay` loop.
3. `lattice-transport` `PathSelector`.
4. `lattice-sync-proto` framing.
5. `lattice-quic` magic socket and `QuicTransport`.
6. `lattice-worker` session and host; `latticed` wiring.
7. `lattice-deck-core` links and model; `lattice-deck` swaps fixtures for `DeckState`.

## Contract tests

| Crate | Tests |
|---|---|
| `lattice-proc-ipc` | `decoder_yields_none_until_a_frame_is_complete`, `tlv_iter_reports_truncated_field_once`, `every_request_roundtrips`, `unknown_type_decodes_to_none`, `spawn_spec_rejects_empty_argv`, `gap_and_exit_replies_roundtrip` |
| `latticed-proc` | `reads_back_what_was_pushed_across_the_wrap`, `gap_exactly_when_evicted` |
| `lattice-transport` | `starts_on_relay_and_goes_direct_after_probe`, `relay_fallback_after_two_seconds_of_direct_silence`, `no_fallback_while_relay_is_dead` |
| `lattice-relay-proto` | `relay_and_quic_split_on_first_byte`, `keepalive_roundtrips`, `unknown_relay_type_is_rejected_not_panicking`, `send_accepted_only_from_the_tokens_bound_address`, `expired_token_is_refused`, `send_is_forwarded_as_recv_to_the_registered_peer`, `quic_packets_sent_to_the_relay_are_dropped`, `keepalive_every_25_seconds_after_registration`, `send_needs_registration` |
| `lattice-sync-proto` | `stream_id_accepts_paths_and_rejects_empty_segments`, `messages_roundtrip_across_split_reads`, `unknown_message_type_is_unsupported_not_fatal` |
| `lattice-quic` | `connect_via_relay_then_echo_on_a_bi_stream` (localhost, in-process relay) |
| `lattice-worker` | `proc_stream_id_roundtrips`, `messages_before_hello_get_an_error`, `subscribe_raw_asks_proc_for_output`, `input_action_writes_to_the_pty` |
| `lattice-deck-core` | `busiest_child_follows_the_token_priority`, `pane_id_parses_tab_dot_pane`, `worker_status_follows_its_link` |

Planned with the first implementation (verification.md scope): proptest for the ring and fan-out, fuzz targets for both codecs, golden v1 IPC fixtures, deterministic simulation of `PathSelector`.

## Open questions

Each blocks the body that names it. Do not code past it without an answer (spec-first.md).

- TBD: IPC v1 numeric type codes, TLV tag table, byte order, and max frame length?
- TBD: width of `ProcId` on the wire (u32 or u64)?
- TBD: ring capacity per process (04 says 4–16 MiB)?
- TBD: relay `ts` units, `ttl` units and default, `REGISTER` freshness window?
- TBD: hole-punch candidate list encoding, and the source of the punch nonce?
- TBD: duplicate window after a path switch: 1 s or 2 s (01 says 1–2 s)?
- TBD: virtual address scheme for a peer NodeId inside the magic socket?
- TBD: sync frame byte order and max frame length; stream id charset and length?
- TBD: signature batches for `Change` (02 says signatures cover batches)?
- TBD: raw `Log` seq ↔ proc byte offset mapping?
- TBD: canonical text form of a `NodeId` (CLI, config, pairing)?
- TBD: where `latticed` gets the relay `NodeId` and the allowed decks before `lattice-identity` lands?
- TBD: proc socket path on macOS (no `XDG_RUNTIME_DIR`)?
