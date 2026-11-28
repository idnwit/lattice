# 02 — Sync (L2)

Status: draft
Owns: the generic data model (state, changes, actions, snapshots), stream kinds, wire messages, compaction.

## Summary

Everything that moves between nodes is a **stream**: an ordered, signed, append-only log of **changes** per writer, plus **snapshots** that let late readers start fast.
Readers never write a stream. They send **actions**; the owner applies them and the effects come back as changes.
One model covers terminal screens, agent events, metrics, and multi-device config.

## Model

```
Stream   { id, owner, kind, schema, key_epoch }
Change   { stream, writer, seq, prev_hash, ts, payload, sig }   // per-writer append-only
Snapshot { stream, covers: {writer → seq}, payload | blob_hash, sig }
Action   { target: stream, id, name, payload }                   // request, not state
Blob     { blake3_hash, ciphertext }                             // verified streaming
```

- `state` = snapshot + changes after it. Readers fold changes into state with the kind's `apply`.
- Single-writer streams: `covers` is one `seq`.
- Multi-writer streams (`Doc`): one log per writer; `covers` is a version vector.
- Signatures cover batches of changes, not each change, so high-rate streams stay cheap.
- Stream `id` (proposed): at most 255 bytes; non-empty segments of `[a-z0-9._-]` separated by `/`, no leading or trailing `/`. Example: `inst/42/state`.

## Stream kinds

| Kind | Writer | Change | Snapshot | Compaction | Used for |
|---|---|---|---|---|---|
| `Log` | single | append entry | optional checkpoint | drop by age/size; never coalesce | TBD: still needed? Raw PTY bytes stay in the instance journal on the worker (04-worker.md) |
| `State` | single | delta of a typed state (06-apps.md › State), with `version` | full state | queue of the last 1024 deltas; older readers get the full state | app instance state, `worker/apps` |
| `Series` | single | batch of points | rollups | downsample old points to coarser buckets | cpu/mem, latency, custom metrics |
| `Doc` | multi | CRDT update bytes (Loro) | CRDT snapshot | merge, then snapshot | device config, layouts, app settings |

Raw PTY bytes stay on the worker, in the instance journal (04-worker.md). Decks get the parsed state only.

## Wire messages

Carried on the control stream and per-stream channels (transport.md). Encoding: postcard in `u32` big-endian length-prefixed frames (D19). Max frame body: 1 MiB (proposed); a longer declared length closes the channel. The postcard enum tag is the message type.

| Message | Direction | Meaning |
|---|---|---|
| `Hello{node_id, device_list_head, versions}` | both | Open session, negotiate versions |
| `List{filter}` → `Streams[...]` | reader → owner | Discover streams (per worker, per process) |
| `Subscribe{stream, have: {writer→seq}, mode: live\|replay}` | reader → owner | Start or resume |
| `Snapshot{stream, covers, payload}` | owner → reader | Sent when `have` is older than compaction point or empty |
| `Change{…}` | owner → reader | Live or replayed changes |
| `Ack{stream, have}` | reader → owner | Flow control; enables coalescing per reader |
| `Unsubscribe{stream}` | reader → owner | Stop |
| `Action{target, id, name, payload}` | reader → owner | Request a state change |
| `ActionResult{id, error, result, detail}` | owner → reader | Outcome; effects arrive as changes. `result`: the encoded reply on success. `error` + `detail`: a stable code; `detail` holds the app's error (postcard) and is empty for a platform error (06-apps.md, Interface) |
| `BlobRequest{hash, ranges}` | any | Fetch content-addressed bytes |

Rules:

1. Resume is always by cursor (`have`). Reconnect never needs a full reload unless compacted.
2. A `State` reader applies a change only at `version == held + 1`. Otherwise it resubscribes and gets a snapshot.
3. A reader behind the retention of a `Log` stream gets `Gap{new_start}` and a snapshot if one exists.
4. Actions are typed per stream schema. Owners validate and may reject.
5. Unknown message types return `Error{unsupported}`. They never close the connection.

## Encryption

- On a live connection, QUIC already encrypts end-to-end. Payloads may travel in clear inside it.
- Anything stored by a non-owner (relay mailbox, backup peer, another device) is encrypted with the user content key (identity-security.md). Storage sees ciphertext + signatures and can verify hash chains without the key.

## Multi-device `Doc` sync

- Each device appends Loro updates to its own log. Devices exchange logs with `Subscribe{have}` like any stream.
- The meta tree (Loro movable tree) holds config, layouts, app settings, workspace state. Large values are blobs referenced by hash.
- Devices sync directly when both are online. When not, an optional ciphertext store relays (infrastructure.md).
- Why Loro: Rust core, WASM build, movable tree without cycles, stable 1.x. Rejected: iroh-docs (no E2E), Automerge + Keyhive (alpha), Willow (immature).

## Examples

| Thing | Streams |
|---|---|
| Apps and instances on a worker | `worker/apps` State · actions `create`, `close` (06-apps.md) |
| Worker config | `worker/config` State: resolved values, sources, `read_at`. No actions in v1 (specs/10-settings.md) |
| Shell (`terminal` instance) | `inst/<iid>/state` State (screen, scrollback) · actions `input`, `resize` |
| claude-code (`coding_agent` instance) | TBD after M1 |
| Worker health (`metrics` instance) | `inst/<iid>/state` State (rollups) · TBD: keep `Series` or fold it into typed state? |
| User settings | `user/meta` Doc |

## Open questions

- TBD: retention defaults per kind (raw PTY ring size, event log age).
- TBD: does `Series` need server-side query (range + resolution), or only subscribe?
