# 04 — Worker

Status: draft
Owns: worker daemons, local IPC, process lifecycle, structured event capture.

## Summary

Two daemons, as proposed:

| Daemon | Role | Updates |
|---|---|---|
| `latticed-proc` (process daemon) | Spawn children, own PTY masters, reap exits, raw output ring buffer | Frozen. Never restarts in normal operation |
| `latticed` (connectivity daemon) | Transport, sync, VT emulation, apps (WASM host), hook ingestion, file access, SSH endpoint | Any time. Restart never kills processes |

Pattern: dtach/abduco minimalism for the process daemon. tmux/zellij/shpool keep VT state in the daemon that owns PTYs, so upgrading kills sessions — we avoid that.

## `latticed-proc`

- About 1–2k lines of Rust. `rustix`/`nix`, `poll`, hand-rolled TLV. No async runtime, no networking, no VT parsing.
- Per process: `id`, `pid`, master fd, argv/env/cwd, size, `tag` (opaque bytes), ring buffer (4–16 MiB) with `start_off`/`end_off`, exit status.
- Single reader of each PTY. Fans out to subscribers. Never blocks on a slow subscriber; a lagging one gets `Gap`.
- `PR_SET_CHILD_SUBREAPER` on Linux.
- Knows nothing about apps. `Spawn` takes opaque argv/env/cwd/tag.

## Local IPC (`latticed` ⇄ `latticed-proc`)

- Unix socket `$XDG_RUNTIME_DIR/lattice/proc.sock`, mode 0700, peer uid check (`SO_PEERCRED` / `getpeereid`).
- Frame: `u32 len | u16 type | u16 flags | TLV payload (u16 tag, u32 len, bytes)`.
- Compatibility: `Hello{version, features}` first. Unknown tags ignored. Unknown types → `Error{unsupported}`. v1 semantics frozen; only optional tags added.

| Request | Reply / stream |
|---|---|
| `Hello{ver, features}` | `Hello` |
| `Spawn{argv, env, cwd, cols, rows, tag}` | `Spawned{id, pid}` |
| `List` | `Procs[{id, pid, tag, cols, rows, start_off, end_off, exit?}]` |
| `Subscribe{id, from_off}` | `Output{id, off, bytes}` …, `Exited{id, status}`, `Gap{id, new_start}` |
| `Unsubscribe{id}` | — |
| `Input{id, bytes}` | — |
| `Resize{id, cols, rows}` | — |
| `Signal{id, signo}` | — |
| `Close{id}` | — (forget record after exit) |
| `GetFd{id}` (optional feature) | master fd via SCM_RIGHTS |
| — | `Error{code, msg}` |

## `latticed`

- One VT emulator per process, behind `trait Emulator`. Start with `alacritty_terminal`; evaluate `libghostty-vt` later.
- Publishes per process: `raw` Log, `screen` State, plus app streams (sync.md).
- Persists emulator snapshots to disk tagged with byte offset.
- **Restart recovery**: `List` → load last snapshot → `Subscribe{from_off}`. If evicted: replay the ring + send a resize nudge so TUIs redraw.
- Hosts worker parts of apps in wasmtime (apps.md).

## Structured events from processes

1. `latticed` injects `LATTICE_PROC_ID` and `LATTICE_EVENTS_SOCK` into every spawn env.
2. Agent hooks (Claude Code hooks, Codex `notify`, …) call `lattice-hook`, which forwards stdin JSON to `$XDG_RUNTIME_DIR/lattice/events.sock`.
3. Any program can also emit status inline with an OSC sequence (herdr's OSC 7501 or our own). `latticed` extracts it from the VT stream. TBD: adopt OSC 7501 for compatibility?
4. Hooks fail open: exit 0 fast if `latticed` is down. Optional spool file for later ingestion.
5. `latticed-proc` never carries events. It stays app-agnostic.

## OS integration

| OS | `latticed-proc` | `latticed` |
|---|---|---|
| Linux | systemd user unit, `KillMode=process`, socket activation, optional `FileDescriptorStoreMax`; `loginctl enable-linger` | separate user unit, `After=` proc socket |
| macOS | LaunchAgent, `KeepAlive`, `AbandonProcessGroup=true` | separate LaunchAgent |
| Windows | per-user process (Task Scheduler at logon), ConPTY, named pipe, no fd passing | same |

The two daemons must be separate units. If `latticed` spawned `latticed-proc`, a restart would kill it with the cgroup.

## Open questions

- TBD: Windows in v1?
- TBD: headless agents (`claude -p --output-format stream-json`) — spawn without PTY, or always via PTY for uniformity?
- TBD: how `latticed` itself is updated (self-update, package manager, both)?
