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
| `lattice-state` | base | implemented, tested (ported from `realtime-workspace` `rtf-core`) | 06-apps (State) |
| `lattice-state-macros` | base | implemented, tested (`#[derive(Synced)]`) | 06-apps (State) |
| `lattice-proc-ipc` | frozen | implemented, tested (golden v1 fixtures, decode proptest) | 04-worker (Local IPC) |
| `lattice-app` | contract | implemented, tested (`Dispatch`, `Caller`, tickets; `view`: view-tree vocabulary v0, `Viewer`) | 06-apps (Interface, Runner, Instances) |
| `lattice-app-macros` | base | implemented, tested (`#[interface]`, ported from `realtime-workspace` `#[rtf::interface]`, no drafts) | 06-apps (Interface) |
| `lattice-app-guest` | contract | implemented (ABI v0 guest exports, `export_runner!`, `export_viewer!`); exercised by `lattice-wasm` tests | 06-apps (Runtimes) |
| `lattice-package` | contract | implemented, tested (manifest, package, blake3 verify) | 06-apps (Distribution, Manifest) |
| `lattice-builtin-apps` | contract | implemented, tested (`build.rs` packs `apps/*`; embedded packages verify) | 06-apps (Distribution) |
| `lattice-wasm` | adapter | implemented, tested (wasmtime host for runners and viewers; echo through `AppsRunner`, terminal runner and viewer wasm = native) | 06-apps (Runtimes) |
| `lattice-app-terminal` (`apps/terminal`) | app | implemented, tested (vt100 emulator; builds for wasm32) | 06-apps, specs/03-sessions |
| `lattice-app-echo` (`apps/echo`) | app | implemented, tested (no process; builds for wasm32) | 06-apps |
| `lattice-transport` | contract | skeleton; `PathInfo` implemented | 01-transport |
| `lattice-relay-proto` | contract | skeleton | 01-transport (Relay wire format) |
| `lattice-sync-proto` | contract | implemented, tested (framing, `StreamId`) | 02-sync (Wire messages) |
| `lattice-quic` | adapter | direct-only mode implemented, tested (loopback bind only); relay mode skeleton | 01-transport (Paths) |
| `lattice-worker` | host | implemented, tested (apps runner with a fake daemon, host end to end over in-memory fakes) | 04-worker, 02-sync |
| `lattice-deck-core` | host | implemented, tested (replica, link over a fake transport); no session or layout model yet | 05-deck, specs/03-sessions |
| `latticed-proc` | bin (`proc`) | implemented, tested (journal proptest, end-to-end daemon test) | 04-worker |
| `latticed` | bin | implemented (ships no app; WASM package loader and cache) | 04-worker, 06-apps |
| `lattice-relay` | bin (`relay`) | skeleton | 08-infrastructure |
| `lattice-deck-ui` | ui | implemented, tested (components, layouts, workspace pieces; gallery and PoC headless tests) | 05-deck, design-system, specs/03-sessions |
| `lattice-deck` | bin | M1 PoC shell (tabs, splits, `+` menu, app picker, close confirmation, mobile pager); WASM viewers and the view-tree interpreter; links no app crate | 05-deck, specs/03-sessions |
| `lattice` | bin | placeholder | — |

## App instances (D23–D35)

Skeletons follow the instance model: per-instance journals in `latticed-proc`, the apps runner in `lattice-worker`, replicas (`Replica`, in `lattice-app` so any deck-side crate can hold one) fed by `lattice-deck-core`.
The crate graph change is brickie draft v9 (bricks `state`, `state-macros`, `app`, `app-terminal`; tag `app`). It is not published: it adds rules, which need owner approval (`brickie design publish`).

- TBD: the `terminal` runner needs a VT emulator, but apps may not reach adapters (`lattice-vt`). Inject it through a trait, or move the parser into a core crate?
- TBD: journal id encoding of an `iid` (tests assume 8 big-endian bytes)?

## Run the terminal experiment (M1, loopback only)

Deck ⇄ QUIC ⇄ `latticed` (terminal runner) ⇄ `latticed-proc` ⇄ shell, on one machine. No pairing: the worker binds loopback only and accepts any local deck.

| Step | Command |
|---|---|
| 1. Process daemon | `cargo run -p latticed-proc` |
| 2. Worker | `cargo run -p latticed` (listens on `127.0.0.1:7441`, writes `worker.addr` in the runtime dir, specs/01-pairing.md › Files) |
| 3a. CLI check | `cargo run -p lattice -- probe "ls /"` (add `--exit` to end the instance, `--iid N` to attach) |
| 3b. Deck | `LATTICE_DECK_LIVE=1 cargo run -p lattice-deck`; pick Terminal or Echo, `⌘D` to split, `⌘W` to close |

Restore check: stop `latticed` and start it again. Shells keep running; instances rebuild from their journals.

TBD: a deck that disconnects leaves its instance running (persistent by design). How are stale instances listed and closed before the UI has close and kill?



| Target | Command | Notes |
|---|---|---|
| macOS, deck | `cargo run -p lattice-deck` | 1440×900 window. The app activates itself on open |
| macOS, deck, mobile layout | `LATTICE_DECK_LAYOUT=mobile cargo run -p lattice-deck` | 390×844 window |
| macOS, UI gallery (canvas screens) | `cargo run -p lattice-deck-ui --example gallery` | Add `LATTICE_DECK_LAYOUT=mobile` for the phone size |
| macOS, workspace PoC | `LATTICE_GALLERY=poc cargo run -p lattice-deck-ui --example gallery` | `+` menu, `⌘D` / `⌘⇧D` splits with the app picker, `⌘W` with a confirmation |
| iOS simulator | `cd crates/lattice-deck && nsh run --target ios --device "iPhone 16 Pro"` | Needs the `nsh` CLI 0.7.0 (`brew install longbridge/tap/nsh`), Xcode, an iOS runtime |

Interactive flows run on the real platforms with `nsh test nsh.json --target macos|ios`, steps on stdin (`await app.press("role:tab[db-shell]")`, `await app.screenshot("x.png")`).
The iOS test host builds QuickJS (nsh tooling, not the product). It needs `BINDGEN_EXTRA_CLANG_ARGS_aarch64_apple_ios_sim="--sysroot=$(xcrun --sdk iphonesimulator --show-sdk-path)"`.

The gallery runs on the platforms the same way: `cd crates/lattice-deck-ui && nsh test nsh.json --target macos`. Its `nsh.json` points `"rust"` at `examples/gallery`, a harness crate (own empty workspace, not a member) that mounts the example's `Gallery` root: the nsh CLI mounts roots from a library only.

NSH 0.7 uses window model v2: the host window never shows, so `DeckApp` and `Gallery` declare their own `window()`.
The `nsh` CLI finds the root through `nsh.json` (`"root"`) and the crate's `#[native_module]`.

## Implementation order

Bottom-up, one PR per crate or per sans-IO core (architecture.md, Brickie):

1. `lattice-proc-ipc` codec, then `latticed-proc` journal and fan-out, then its poll loop.
2. `lattice-relay-proto` codec, token table, engine, client; then `lattice-relay` loop.
3. `lattice-transport` `PathSelector`.
4. `lattice-sync-proto` framing.
5. `lattice-quic` magic socket and `QuicTransport`.
6. `lattice-worker` session and host; `latticed` wiring.
7. `lattice-deck-core` links and model; `lattice-deck` composes `lattice-deck-ui` over `DeckState`.

## Contract tests

| Crate | Tests |
|---|---|
| `lattice-proc-ipc` | `decoder_yields_none_until_a_frame_is_complete`, `tlv_iter_reports_truncated_field_once`, `every_request_roundtrips`, `unknown_type_decodes_to_none`, `spawn_spec_rejects_empty_argv`, `journal_id_rejects_empty`, `entry_gap_and_error_replies_roundtrip`, `journals_reply_roundtrips`, `decoder_rejects_a_frame_above_one_mib`, `golden_v1_requests_decode`, `golden_v1_replies_decode`, `encoder_still_writes_the_golden_bytes`, `unknown_tag_is_ignored`, `random_bytes_never_panic`, `mutated_golden_frames_never_panic`, `frames_survive_any_split` |
| `latticed-proc` | `two_procs_interleave_in_read_order`, `gap_exactly_when_over_budget`, `trim_drops_history_and_offsets_keep_growing`, `oversized_out_keeps_only_its_tail`, `journal_matches_the_history_oracle`, `slow_subscriber_never_blocks_the_others`, `shell_output_then_exit_code_then_drop`, `cat_echoes_input_and_drop_is_busy_while_it_lives`, `unknown_type_is_unsupported_and_the_connection_stays`, `malformed_frame_closes_only_that_client`, `a_reconnecting_client_finds_the_journal_and_replays_it` |
| `lattice-transport` | `starts_on_relay_and_goes_direct_after_probe`, `relay_fallback_after_two_seconds_of_direct_silence`, `no_fallback_while_relay_is_dead` |
| `lattice-relay-proto` | `relay_and_quic_split_on_first_byte`, `keepalive_roundtrips`, `unknown_relay_type_is_rejected_not_panicking`, `send_accepted_only_from_the_tokens_bound_address`, `expired_token_is_refused`, `send_is_forwarded_as_recv_to_the_registered_peer`, `quic_packets_sent_to_the_relay_are_dropped`, `keepalive_every_25_seconds_after_registration`, `send_needs_registration` |
| `lattice-sync-proto` | `stream_id_accepts_paths_and_rejects_empty_segments`, `messages_roundtrip_across_split_reads`, `unknown_message_type_is_unsupported_not_fatal` |
| `lattice-quic` | `direct_localhost_echo_on_a_bi_stream`, `wrong_node_id_is_refused`, `non_loopback_bind_is_refused`, `a_refused_peer_fails_with_refused_instead_of_hanging`; `connect_via_relay_then_echo_on_a_bi_stream` (localhost, in-process relay; ignored until relay mode) |
| `lattice-worker` | `worker_stream_ids_roundtrip`, `messages_before_hello_get_an_error`, `subscribe_without_version_resumes_from_full_state`, `instance_action_goes_to_its_runner`, `resume_inside_the_queue_sends_only_missing_deltas`, `resume_older_than_the_queue_gets_full_state`, `create_opens_a_journal_before_it_spawns`, `create_of_an_unknown_app_opens_nothing`, `close_drops_the_journal_only_after_every_exit`, `finish_closes_the_instance_without_a_request`, `replay_rebuilds_state_and_runs_no_effects`, `instances_never_see_each_other`, `replay_commits_deltas_but_drops_effects`, `a_refused_action_rolls_back`, `resumed_runs_once_when_replay_reaches_the_listed_end`, `a_new_instance_and_an_empty_journal_resume_at_once`, `an_instance_action_answers_with_its_reply_or_its_app_error`, `instance_lifecycle_with_a_fake_proc_daemon`, `close_hangs_up_live_processes_and_kill_escalates`, `a_deck_creates_an_instance_and_reads_its_state`, `unpaired_deck_cannot_open_a_stream` |
| `lattice-app` | `delta_out_of_order_asks_for_resync`, `full_state_then_deltas_equal_the_runner_state`, `bad_bytes_ask_for_a_resync_without_a_version`, `app_id_rejects_empty`, `app_id_is_reverse_dns`, `journal_meta_roundtrips`, `a_new_record_starts_with_no_process`, `record_list_roundtrips`, `a_typed_call_round_trips_through_dispatch`, `actions_encode_by_declaration_order_and_bad_bytes_are_refused`, `a_ticket_settles_each_way`, `dropping_the_resolver_settles_unknown`, `bytes_that_do_not_decode_are_a_bad_reply`, `resolving_wakes_the_polling_task`, `variant_names_follow_serde_renames`, `app_error_carries_the_variant_and_its_encoding` |
| `lattice-app-macros` | `expands_action_enum_runner_and_client_in_declaration_order`, `crate_attribute_overrides_the_path`, `method_names_become_upper_camel_variants`, `refuses_bad_reducer_shapes`, `refuses_self_types_other_items_and_a_missing_app_bound` |
| `lattice-app-terminal` | `start_spawns_the_shell_template`, `input_goes_to_the_shell_and_changes_no_state`, `zero_size_is_refused`, `clear_outside_alt_screen_is_a_clean_point`, `shell_exit_finishes_the_instance`, `resumed_nudges_the_pty_of_a_live_shell_only`, `resize_through_the_client_returns_the_size_in_effect` |
| `lattice-deck-core` | `busiest_child_follows_the_token_priority`, `pane_id_parses_tab_dot_pane`, `worker_status_follows_its_link`, `updates_route_to_full_state_and_delta`, `open_link_sends_hello_first_and_reports_the_path`, `a_typed_call_resolves_from_the_worker_reply_and_link_loss_gives_unknown`, `queued_calls_resolve_from_their_action_results`, `link_end_settles_queued_not_connected_and_in_flight_unknown` |

Planned with the first implementation (verification.md scope): proptest for the fan-out (unit tests only so far), fuzz targets for both codecs (the IPC codec has a proptest stand-in), deterministic simulation of `PathSelector`.

## Open questions

Each blocks the body that names it. Do not code past it without an answer (spec-first.md).

- TBD: relay `ts` units, `ttl` units and default, `REGISTER` freshness window?
- TBD: hole-punch candidate list encoding, and the source of the punch nonce?
- TBD: duplicate window after a path switch: 1 s or 2 s (01 says 1–2 s)?
- TBD: virtual address scheme for a peer NodeId inside the magic socket?
- TBD: sync frame byte order and max frame length; stream id charset and length?
- TBD: signature batches for `Change` (02 says signatures cover batches)?
- TBD: canonical text form of a `NodeId` (CLI, config, pairing)?
- TBD: where `latticed` gets the relay `NodeId` and the allowed decks before `lattice-identity` lands?
