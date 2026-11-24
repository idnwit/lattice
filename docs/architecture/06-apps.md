# 06 — Apps

Status: draft — **in v1**
Owns: app model, manifest, kinds, permissions, runtimes, fallback.

## v1 scope

Apps are WASM from day one. The platform ships an app host on both worker and deck.

1. Built-in features are WASM apps on the same API as third-party apps: `terminal`, `coding_agent` (claude-code, codex), `files`, `ports`, `git`, `metrics`.
2. The platform itself stays generic: processes, streams, actions, layouts, host widgets. It never special-cases an app.
3. Host widgets are native NSH elements (e.g. `terminal`, `diff`, `table`). Apps compose them through the view tree; they never draw pixels.
4. Every app state is a sync stream with a schema. Every user command is an `Action`. No side channels.
5. Install in v1 is local: the `lattice` CLI builds, installs, and reloads apps on a worker and deck. Coding agents use it to build and deploy apps from our docs.
6. Built-in apps are bundled with `latticed` and the deck, but load through the same host.

## Summary

An **app** adds a product capability on top of the generic platform. It has up to two parts:

- **Worker part** (optional, WASM on the worker): attaches to processes of a type, turns raw output and events into a typed state stream, handles actions.
- **View part** (WASM): turns that state into a view tree for decks.

Both parts are WASM Component Model components with a versioned WIT world. Built-in apps use the same API.

## Kinds

| Kind | Examples |
|---|---|
| `terminal` | Built-in fallback: raw PTY as `terminal` widget |
| `coding_agent` | claude-code, codex: visual view, approvals, plan, diffs |
| `runner` | tests, builds, logs, dev servers |
| `files` | file tree, editor, viewers (image, audio, video, markdown) |
| `metrics` | worker health, GPU, custom series |
| `integration` | git, CI, issue tracker |
| `communication` | chat, notifications between nodes |
| `entertainment` | anything else; may use `canvas` |

An app may declare several kinds. TBD: confirm list.

## Manifest (`app.toml`)

```toml
id = "dev.lattice.claude-code"
name = "Claude Code"
version = "0.1.0"
api = "lattice:app@1"            # WIT world version
kinds = ["coding_agent"]

[handles]                        # which processes this app attaches to
argv = ["claude", "claude-code"]
osc_agent = ["claude"]           # status escape name
events = ["claude-code/hooks"]

[parts]
worker = "worker.wasm"
view = "view.wasm"
view_placement = "auto"          # deck | worker | auto

[permissions]
process_attach = ["self"]        # read PTY + events of matched processes
pty_write = true
fs_read = ["$CWD"]
notify = true

[limits]
memory_mb = 64
cpu_ms_per_event = 50

[schema]
state = "state.wit"              # snapshot shape, migrations
actions = ["approve", "deny", "prompt", "interrupt"]
widgets = ["visual", "run-panel", "overview-row"]   # named views for layouts
```

## Permissions

| Permission | Scope |
|---|---|
| `process_attach` | matched processes or explicit ids |
| `process_spawn` | listed commands |
| `pty_write` | attached processes |
| `fs_read` / `fs_write` | paths on the worker |
| `net_connect` | host list |
| `clipboard_read` / `clipboard_write` | deck |
| `notify` | deck notifications |
| `storage_kv` | quota |
| `secrets` | named secrets |
| `peer_send` | messages to other apps/nodes |

Granted at install; upgrade shows a permission diff. Sensitive ones (`process_spawn`, `fs_write`, `clipboard_read`) also confirm at first use.

## Runtimes and limits

- Worker and desktop: wasmtime, epoch interruption for deadlines, `ResourceLimiter` for memory.
- Web (later): browser WASM.
- iOS (sideloaded, no App Store review): view parts may run on device, but iOS forbids JIT → interpreter (wasmtime Pulley or wasmi). Keep view parts light, or place them on the worker.
- Interface: WASI 0.3 component model (async, streams). Keep the WIT small and data-oriented so a core-wasm ABI can stand in where component support is weak.

## Data flow

```
process ─raw/events─▶ worker part ─State stream─▶ view part ─ViewTree─▶ view host
   ▲                                                   │
   └──────────── actions ◀── worker part ◀── Action ◀──┘
```

## Fallback

Every process without a matching app gets the built-in `terminal` app. Richer apps embed the `terminal` widget too (TTY mode of a coding agent).

## Agent-authored apps

- Ship an `AGENTS.md` + WIT + manifest schema so a local coding agent can build apps and layouts ("shape it by asking", herdr-style).
- A `lattice` CLI lets agents install, reload, and test apps locally.
- Host enforces the manifest. Agent-written apps start with least privilege.

## Open questions

- TBD: distribution — local only in v1, or a registry with signatures?
- TBD: is the coding-agent app built-in or a regular installable app?
