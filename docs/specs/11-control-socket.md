# 11 — Worker control socket

Status: draft
Owns: the worker control socket, its messages and limits, and the single-instance rule for `latticed` and the deck.
Pairing over it: [01-pairing.md](01-pairing.md). Config over it: [10-settings.md](10-settings.md). Decision: D53.

## Summary

Only the process that holds a node key uses it (D53). On a worker that is `latticed`.
The `lattice` CLI (`add-deck`, `config`) sends one request over a Unix socket in the data dir and gets one reply.
One `latticed` and one deck GUI run per data dir.

Planned (M2.9). M1 code has no control socket; `latticed add-deck` pairs by itself.

## Behavior

### Single instance

1. One `latticed` per data dir. Its start follows the sequence in architecture/04-worker.md › `latticed` start order. Steps 1–3 are:
   1. Lock: create `worker/` with mode 0700 if it is missing (an existing dir is fine). Refuse a symlink or a dir owned by another user. Open it as a directory and take an exclusive `flock` on it, held for the whole life. A taken lock exits with `worker.already_running`.
   2. Key: load `worker/node.key`. A file that is not exactly 32 bytes exits with `worker.bad_key`; `latticed` never overwrites it. Only a missing file makes it create a new key.
   3. Control socket: unlink any stale `worker/latticed.sock` and bind a new one (rule 3).
   - A live daemon is one whose socket accepts `connect()`.
   - The CLI takes the same lock the same way before it writes a file itself (10-settings.md, rule 34).
2. One deck GUI per data dir. It prepares and locks `deck/` as rule 1 step 1 does, for its whole life. A second instance that cannot take the lock focuses the first one and exits with `deck.already_running`.

### Socket

3. `latticed` serves `<data dir>/worker/latticed.sock`, mode 0600. The socket sits next to the node key. So a CLI on that data dir always reaches the process that holds that key, and requests carry no NodeId.
4. `latticed` checks the peer credentials (`SO_PEERCRED` on Linux, `getpeereid` on macOS). It closes any connection from another uid.
5. A socket path holds at most 104 bytes on macOS and 108 on Linux. A data dir too long for it makes `latticed` exit with `control.path_too_long`.
6. The CLI sends one version byte, `1`, then frames: postcard in `u32` big-endian length-prefixed frames, as the sync wire (D19). Another version gets `Failed{control.version}`.
7. A frame body over 64 KiB (proposed) closes the connection. One request, one reply, then `latticed` closes.
8. An unknown request type gets `Failed{control.unsupported}`. Malformed bytes close the connection; they never stop the daemon.
9. Messages: Data › Messages.
10. `latticed` closes a connection that sends no complete request within 5 s (proposed).
11. `latticed` serves at most 4 connections at once (proposed). It closes extra ones at once, without a reply.
12. The CLI waits for a reply at most `PAIR_TIMEOUT` + 5 s = 20 s (proposed), then fails with `control.timeout`.
13. CLI outcomes: `connect()` fails gives `worker.not_running`. `connect()` succeeds, then the socket closes with no reply: `worker.busy`.

## Data

### Messages

| Request (CLI → `latticed`) | Reply | Use |
|---|---|---|
| `AddDeck{code: String (≤ 1024 bytes), name: Option<String>}` | `Paired{deck: NodeId, name: String, contact: Contact}` or `Failed{code}` | `lattice add-deck` (01-pairing.md, CLI) |
| `ConfigList` | `Config{entries: [{key, value, source}]}` | `lattice config list` (10-settings.md, rule 41) |
| `ConfigGet{key: String}` | `Config{entries}` with one entry, or `Failed{config.unknown_key}` | `lattice config get` |
| `ConfigSet{key: String, value: String}` | `Config{entries}` with the new entry, or `Failed{config.unknown_key \| config.invalid_value}` | `lattice config set` (10-settings.md, rule 43) |
| `ConfigReset{key: String}` | `Config{entries}` with the resolved entry, or `Failed{config.unknown_key}` | `lattice config reset` |
| `ConfigReplace{toml: String, base_hash: [u8; 32]}` | `Config{entries}` (all), or `Failed{config.conflict \| config.parse \| config.invalid_value \| config.unknown_key}` | `lattice config edit` (10-settings.md, rule 45) |

- `source` is one of `flag`, `env`, `file`, `default`.
- `base_hash` is the blake3 hash of the file the CLI read.
- The `code` field of `AddDeck` carries the invite secret: `latticed` never logs it.

### Codes

| Code | Where | Meaning, hint |
|---|---|---|
| `worker.not_running` | CLI | `connect()` failed: `latticed` is not running. Start it, then run the command again |
| `worker.busy` | CLI | The socket closed with no reply (rule 11), or the lock is held but the socket does not answer (10-settings.md, rule 34). Retry |
| `worker.already_running` | `latticed` start | Another `latticed` holds the data dir (rule 1) |
| `deck.already_running` | deck start | Another deck holds the data dir (rule 2) |
| `control.version` | CLI | Version byte mismatch (rule 6) |
| `control.unsupported` | CLI | Unknown request type (rule 8) |
| `control.timeout` | CLI | No reply in time (rule 12) |
| `control.path_too_long` | `latticed` start | Socket path over the OS limit (rule 5) |
| `worker.bad_key` | `latticed` start | `worker/node.key` is not 32 bytes (rule 1). Restore it or delete it to make a new identity (then pair again) |

All codes are proposed.

## Invariants

Checks per [.rules/verification.md](../../.rules/verification.md). All planned.

1. Control frame decoding never panics on any bytes. Check: fuzz target `control_decode`, proptest `control_decode_never_panics`.
2. An unknown request type gets `Failed{control.unsupported}` and never stops the daemon. Check: test `an_unknown_control_request_is_unsupported`.
3. A frame body over 64 KiB closes the connection before it is read. Check: test `an_oversized_control_frame_closes_the_connection`.
4. A connection from another uid gets no reply. Check: test `a_control_peer_with_another_uid_is_refused`.
5. A second `latticed` on one data dir exits with `worker.already_running`; a stale socket is replaced. Check: tests `a_second_latticed_exits_already_running`, `two_concurrent_latticed_starts_leave_one_running`, `a_stale_control_socket_is_replaced`.
6. A second deck on one data dir exits with `deck.already_running`. Check: test `a_second_deck_exits_already_running`.

## Open questions

- TBD: does the deck need its own control socket later (for example to focus the first instance, rule 2)?
