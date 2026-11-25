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

## Stream kinds

| Kind | Writer | Change | Snapshot | Compaction | Used for |
|---|---|---|---|---|---|
| `Log` | single | append entry | optional checkpoint | drop by age/size; never coalesce | agent hook events, raw PTY bytes, command history |
| `State` | single | diff (dirty rows/cells, cursor, modes) | full value | coalesce: slow readers skip stale diffs (mosh SSP) | terminal screen, app view-model |
| `Series` | single | batch of points | rollups | downsample old points to coarser buckets | cpu/mem, latency, custom metrics |
| `Doc` | multi | CRDT update bytes (Loro) | CRDT snapshot | merge, then snapshot | device config, layouts, app settings |

Raw PTY bytes are a `Log` (scrollback, exact replay). The parsed screen is a separate `State`. A deck may subscribe to either or both.

## Wire messages

Carried on the control stream and per-stream channels (transport.md). Encoding: postcard in `u32` length-prefixed frames (D19).

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
| `ActionResult{id, ok \| err}` | owner → reader | Outcome; effects arrive as changes |
| `BlobRequest{hash, ranges}` | any | Fetch content-addressed bytes |

Rules:

1. Resume is always by cursor (`have`). Reconnect never needs a full reload unless compacted.
2. A reader that falls behind a `State` stream gets the newest diff from its last ack, not every frame.
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
| Shell in a PTY | `proc/<id>/raw` Log · `proc/<id>/screen` State · actions `input`, `resize`, `signal` |
| claude-code | the above + `proc/<id>/events` Log (hooks) · app view-model State · actions `approve`, `deny`, `prompt` |
| Worker health | `worker/metrics` Series |
| User settings | `user/meta` Doc |

## Open questions

- TBD: retention defaults per kind (raw PTY ring size, event log age).
- TBD: does `Series` need server-side query (range + resolution), or only subscribe?
