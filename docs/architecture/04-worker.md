# 04 — Worker

Status: draft
Owns: worker daemons, instance journals, local IPC, process lifecycle, structured event capture.
App instances, runners, and restore: [06-apps.md](06-apps.md).

## Summary

A worker has two parts: the process daemon and `latticed`, which holds connectivity and the **apps runner**.

| Daemon | Role | Updates |
|---|---|---|
| `latticed-proc` (process daemon) | Spawn children, own PTY masters, reap exits, keep one **journal** per app instance | Frozen. Never restarts in normal operation |
| `latticed` (connectivity daemon + apps runner) | Transport, sync, runners, hook ingestion, file access, SSH endpoint | Any time. Restart never kills processes; runners rebuild from journals |

Pattern: dtach/abduco minimalism for the process daemon. tmux/zellij/shpool keep VT state in the daemon that owns PTYs, so upgrading kills sessions. We avoid that.

## Principle

1. `latticed-proc` keeps as much history as its budget allows, so a runner can always rebuild.
2. `latticed` keeps no app state on disk. Restore reads only journals.
3. A runner trims its journal at clean points, so history stays small (06-apps.md › Clean points).

## Journal

A journal is the ordered input history of one app instance. `latticed-proc` stores it as opaque bytes.

| Entry | Written by | Content |
|---|---|---|
| `Out{proc, bytes}` | `latticed-proc` | Output read from a PTY of the instance |
| `Exit{proc, status}` | `latticed-proc` | A process of the instance exited |

Besides entries, a journal holds `meta`: opaque bytes set at `Open` (app id, package hash, params). `latticed` needs it to rebuild the runner.
Nothing else is stored. Actions from viewers never enter a journal (06-apps.md › Design philosophy).

1. One journal per instance holds the output of all its processes, in read order. Replay sees the same interleaving.
2. Every entry has a byte offset. Offsets only grow, also across `Trim`.
3. A journal has a byte budget. Past it, `latticed-proc` drops the oldest entries; a subscriber behind gets `Gap{new_start}`.
4. `Trim{upto}` drops every entry before `upto`. The runner calls it at a clean point (06-apps.md).
5. A journal lives until `Drop`. Its processes must have exited first.
6. Journals live in memory. A worker reboot loses them, and the processes with them.

Journal sizing (proposed, experiment):

| Rule | Value |
|---|---|
| Default budget (`DEFAULT_JOURNAL_BUDGET`) | 16 MiB per journal. `Open` still carries the budget |
| Minimum budget | 8 bytes, the size of one `Exit`. `Open` rejects less |
| Size of `Out` | `bytes.len()`. An empty `Out` is not stored |
| Size of `Exit` | 8 |
| `Out` larger than the whole budget | Only its last `budget` bytes are kept, at their stream offset. Older entries drop |
| `Trim{upto}` | Drops every entry that starts before `upto`. `upto` past `end_off` clamps to `end_off` |

TBD: spill journals to disk past the memory budget?

## `latticed-proc`

- About 1–2k lines of Rust. `rustix`/`nix`, `poll`, hand-rolled TLV. No async runtime, no networking, no VT parsing.
- Per journal: `iid` and `meta` (opaque bytes), budget, entries with `start_off`/`end_off`, subscribers.
- Per process: `id`, `pid`, master fd, argv/env/cwd, size, journal, exit status.
- Single reader of each PTY. Appends to the journal, then fans out. Never blocks on a slow subscriber; a lagging one gets `Gap`.
- `PR_SET_CHILD_SUBREAPER` on Linux.
- Children run in a new session with the PTY slave as controlling tty and fds 0/1/2. Empty `cwd` inherits. Env: see Child env below.

Loop and fan-out (proposed, experiment):

1. One `poll` loop over the listener, clients, and PTY masters. No signal handler.
2. Exits: `wait(WNOHANG)` after every wake. While a child lives, `poll` wakes at least every 50 ms.
3. On exit, the daemon drains the PTY (at most 4 MiB), closes the master, then appends `Exit`.
4. Each client has an output buffer. Fan-out stops filling it at 256 KiB and resumes as the socket drains.
5. A slow subscriber is never disconnected for lag. Its cursor stays in the journal; once those entries drop, it gets `Gap{new_start}` and continues.
6. A client is disconnected only when replies it never reads pass 4 MiB. Reads from it pause above 256 KiB.
7. PTY input is queued per process, up to 1 MiB. Excess input is dropped.
8. Clients may connect at any time; several at once is fine. A disconnect drops its subscriptions; children keep running.
- Knows nothing about apps. `meta` is opaque.

## Child env

A spawned process must see a terminal it can drive, and nothing of the daemons' own setup.

1. `latticed-proc` builds the env from its own env without any `LATTICE_*` variable. Daemon settings (`LATTICE_DATA_DIR`, `LATTICE_PROC_SOCK`, …) never reach a user's shell.
2. Then it applies the `Spawn` env vars, in order. A `Spawn` var replaces an inherited one of the same name.
3. `latticed` sends these vars with every `Spawn`:

| Var | Value | Why |
|---|---|---|
| `TERM` | `xterm-256color` (proposed) | The `terminal` emulator (`vt100`) implements the xterm control sequences, 256 colors included. Set even when the daemon has no tty |
| `COLORTERM` | `truecolor` (proposed) | The emulator keeps 24-bit SGR colors, and the viewers draw them (specs/03-sessions.md, Attach rule 14) |
| `LATTICE_INSTANCE_ID` | the `iid` | Structured events, below |

TBD: per-template env in `[[spawn]]`, once an app needs a different `TERM`?
TBD: drop other vars of the terminal that started the daemon (`TERM_PROGRAM`, `TERMINFO`, …)?

## Local IPC (`latticed` ⇄ `latticed-proc`)

- Unix socket `$XDG_RUNTIME_DIR/lattice/proc.sock`, mode 0700, peer uid check (`SO_PEERCRED` / `getpeereid`).
- macOS has no `XDG_RUNTIME_DIR`: `$TMPDIR/lattice/proc.sock`, else `/tmp/lattice/proc.sock`. `LATTICE_PROC_SOCK` overrides the path (proposed, experiment).
- The daemon creates the directory with mode 0700. It refuses to start if a daemon already answers on the socket.
- Frame: `u32 len | u16 type | u16 flags | TLV payload (u16 tag, u32 len, bytes)`.
- Compatibility: `Hello{version, features}` first. Unknown tags ignored. Unknown types → `Error{unsupported}`. v1 semantics frozen; only optional tags added.
- A frame that fails to parse gets `Error{malformed}`, then the daemon closes that client.

TBD: the peer uid check runs on Linux only; `rustix` has no `getpeereid`. On macOS the 0700 directory is the only guard. Add `getpeereid` how, without a second `unsafe`?
TBD: `latticed` cannot inject `LATTICE_PROC_ID` before `Spawned` returns the id. Should `latticed-proc` inject it?

| Request | Reply / stream |
|---|---|
| `Hello{ver, features}` | `Hello` |
| `Open{iid, meta, budget}` | `Opened{iid}` (create a journal) |
| `Spawn{iid, argv, env, cwd, cols, rows}` | `Spawned{id, pid}` |
| `List` | `Journals[{iid, meta, start_off, end_off, procs[{id, pid, cols, rows, exit?}]}]` |
| `Subscribe{iid, from_off}` | `Entry{iid, off, kind, proc?, bytes}` …, `Gap{iid, new_start}` |
| `Unsubscribe{iid}` | — |
| `Trim{iid, upto}` | `Trimmed{start_off}` |
| `Input{id, bytes}` | — |
| `Resize{id, cols, rows}` | — |
| `Signal{id, signo}` | — |
| `Drop{iid}` | `Dropped` or `Error{busy}` while a process lives |
| `GetFd{id}` (optional feature) | master fd via SCM_RIGHTS |
| — | `Error{code, msg}` |

`Trimmed` and `Dropped` also carry the journal id. `Subscribe`, `Unsubscribe`, `Input`, `Resize`, and `Signal` have no success reply; failures answer `Error`.

### Wire encoding (v1, proposed, experiment)

- Integers in the header and in TLV values are big-endian.
- `len` counts the payload only. Maximum payload: 1 MiB (`MAX_FRAME_LEN`). The decoder fails with `TooLong` above it.
- `flags` is 0. No v1 flags.
- A non-repeated tag that appears twice: the last copy wins.
- Golden v1 frames: `crates/lattice-proc-ipc/tests/fixtures/`. Never regenerated.

| Request | Type | Reply | Type |
|---|---|---|---|
| `Hello` | `0x0001` | `Hello` | `0x8001` |
| `Open` | `0x0002` | `Opened` | `0x8002` |
| `Spawn` | `0x0003` | `Spawned` | `0x8003` |
| `List` | `0x0004` | `Journals` | `0x8004` |
| `Subscribe` | `0x0005` | `Entry` | `0x8005` |
| `Unsubscribe` | `0x0006` | `Gap` | `0x8006` |
| `Trim` | `0x0007` | `Trimmed` | `0x8007` |
| `Input` | `0x0008` | `Dropped` | `0x8008` |
| `Resize` | `0x0009` | `Error` | `0x8009` |
| `Signal` | `0x000A` | | |
| `Drop` | `0x000B` | | |

One tag table serves every message:

| Tag | Name | Value | Used by |
|---|---|---|---|
| `0x01` | version | `u16` | `Hello` |
| `0x02` | features | `u64` | `Hello` |
| `0x03` | journal | 1–64 bytes. `latticed` sends the instance id as 8 bytes big-endian | all journal messages |
| `0x04` | meta | bytes, at most 64 KiB | `Open`, journal record |
| `0x05` | budget | `u64` | `Open` |
| `0x06` | argv | bytes. Repeated, in order, at least one | `Spawn` |
| `0x07` | env | `NAME=VALUE`. Split at the first `=`. `NAME` non-empty, no NUL. Repeated, in order | `Spawn` |
| `0x08` | cwd | bytes. Empty inherits | `Spawn` |
| `0x09` | cols | `u16`, non-zero | `Spawn`, `Resize`, process record |
| `0x0A` | rows | `u16`, non-zero | `Spawn`, `Resize`, process record |
| `0x0B` | offset | `u64` | `from_off`, `upto`, `off`, `new_start`, `start_off` |
| `0x0C` | end_off | `u64` | journal record |
| `0x0D` | proc | `u64` `ProcId` | `Input`, `Resize`, `Signal`, `Spawned`, `Entry`, process record |
| `0x0E` | pid | `u32` | `Spawned`, process record |
| `0x0F` | bytes | bytes | `Input`, `Entry` (`Out`) |
| `0x10` | signo | `i32` | `Signal` |
| `0x11` | kind | `u8`: 1 `Out`, 2 `Exit` | `Entry` |
| `0x12` | exit | 5 bytes: `u8` (0 code, 1 signal), then `i32` | `Entry` (`Exit`), process record (absent while alive) |
| `0x13` | code | `u16` error code | `Error` |
| `0x14` | msg | UTF-8 | `Error` |
| `0x15` | journal record | nested TLV: journal, meta, offset (`start_off`), end_off, process records | `Journals`, repeated |
| `0x16` | process record | nested TLV: proc, pid, cols, rows, exit? | inside a journal record, repeated |

| Error code | Value |
|---|---|
| `unsupported` | 1 |
| `malformed` | 2 |
| `no_such_proc` | 3 |
| `no_such_journal` | 4 |
| `journal_exists` | 5 |
| `busy` | 6 |
| `spawn_failed` | 7 |

## `latticed`

- Runs one runner per app instance (06-apps.md). Built-in runners are native in M1; WASM from M4.
- The `terminal` runner holds a VT emulator behind `trait Emulator`. Uses `vt100` (pure Rust, no I/O, builds for `wasm32-unknown-unknown`); evaluate `libghostty-vt` later. Not covered yet: concealed text (`SGR 8`), strikeout, buffered synchronized updates.
- Publishes one stream per state tree, `inst/<iid>/<tree>` (02-sync.md). Decks get the current tree, then changes.
- **Restart recovery**: `List`, then per journal: new runner from `meta`, `start` with effects dropped, `Subscribe{from_off: start_off}`, replay to `end_off`, then live.
- Effects are not run during replay. After replay, the `terminal` runner sends a resize nudge so TUIs redraw.
- Every process belongs to an instance. `Spawn` always names a journal.

## Structured events from processes

Deferred until after the M1 terminal PoC. Agent hooks and status sequences will be designed with the first coding agent (ROADMAP M3).
Rule kept: `latticed` injects `LATTICE_INSTANCE_ID` and `LATTICE_PROC_ID` into every spawn env.

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
