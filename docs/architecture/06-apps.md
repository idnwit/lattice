# 06 — Apps

Status: draft — **in v1**
Owns: app crate model, typed state and deltas, runner and viewer, app instances, create and close, app distribution, restore and clean points, manifest, permissions, runtimes.

## Summary

An **app** is one Rust crate. It builds into two parts:

- **Runner** (worker): runs one app instance. It handles actions from viewers, drives its processes in `latticed-proc`, and reduces their output into state.
- **Viewer** (deck): renders the instance state as a view tree and sends actions.

The unit of sync is the whole state of one instance. A deck holds the full state and keeps it current with deltas.

Control flows one way: viewer → runner → process. Data flows back: process output → runner → state changes → viewer.
The state model follows the typed-app design of `realtime-workspace` (RFC-0005 §6, RFC-0011): typed state, reducers, changes. The runner plays the server role.

## Design philosophy

1. A runner restores only from what `latticed-proc` keeps: the instance meta and its process history (04-worker.md › Journal).
2. State that the runner can derive from process output survives a runner restart. Anything else is lost on restart.
3. Each app decides what it keeps. An app that needs durable data gets it from its process, as a pure process would (e.g. a coding agent's own session files).
4. The platform never stores app data outside `latticed-proc`. `latticed` keeps no app state on disk.

## v1 scope

1. Built-in features are apps on the same API as third-party apps: `terminal`, `coding_agent` (claude-code, codex), `files`, `ports`, `git`, `metrics`.
2. The platform stays generic: apps, instances, journals, states, actions, layouts, host widgets. It never special-cases an app.
3. Host widgets are native NSH elements (e.g. `terminal`, `diff`, `table`). Viewers compose them; they never draw pixels.
4. Every user command is an action of the app interface. No side channels.
5. Runners and viewers are WASM from the M1 PoC (D42), with ABI v0 = core wasm + postcard (D43). Built-in apps live in `apps/` (one folder per app, outside `crates/`) and ship inside the deck; the worker ships none (D44).

## App crate

```
app-terminal/
  app.toml          manifest
  src/model.rs      State: struct of synced types
  src/api.rs        interface trait: actions, params, error enum
  src/runner.rs     #[cfg(feature = "runner")]  App impl: start, reducers, clean points
  src/viewer.rs     #[cfg(feature = "viewer")]  render(state) -> ViewTree
```

1. `model.rs` and `api.rs` compile into both parts, so both parts agree on state and actions.
2. `lattice app build` builds the crate twice: `--features runner` and `--features viewer`.
3. The output is one **package**: `app.toml`, `runner.wasm`, `viewer.wasm`. Its id is the blake3 hash of the three files.
4. A package is immutable. A new build is a new hash, even at the same `version`.

## State

A state is a struct of **synced types**. Each synced type records its own changes. One reducer call that commits yields one **delta**: the encoded changes of every field it touched.

```rust
#[derive(Synced, Default)]
pub struct TermState {
    pub screen: Screen,                // derive(Synced): rows, cursor, size, title
    pub scrollback: List<Value<Row>>,  // synced in full for now
}
```

TODO: a dedicated synced type for append-heavy, bounded data such as scrollback (lazy, ranged, or capped). Until then scrollback syncs in full.

| Synced type | Methods | Change |
|---|---|---|
| `Value<T>` | `get`, `set` | new value |
| `Map<K, T: Synced>` | `get`, `get_mut`, `insert`, `remove`, `iter` | per key: set, remove, or `T::Change` |
| `List<T: Synced>` | `get`, `get_mut`, `push`, `truncate`, `pop`, `clear`, `iter` | truncate, push, edits by index |
| `#[derive(Synced)]` struct | one field each | per field: `Option<F::Change>` |

```rust
pub trait Synced: Serialize + DeserializeOwned {
    type Change: Serialize + DeserializeOwned;
    fn has_draft(&self) -> bool;                 // O(touched)
    fn write_change<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error>;
    fn commit(&mut self);                        // O(touched)
    fn rollback(&mut self);                      // O(touched)
    fn apply(&mut self, change: Self::Change) -> Result<(), ApplyError>;  // on a deck
    fn rev(&self) -> u64;                        // local; the viewer re-renders on change
}
```

1. Data changes only through type methods. A setter moves the old value into the draft; it never clones.
2. One reducer call ends in commit (on `Ok`) or rollback (on `Err`). Commit yields one delta, or none when nothing changed.
3. Laws, property-tested per type: `apply(change)` on a replica gives the same value; `rollback` gives the last committed value; one canonical encoding.
4. A runner may hold private memory outside the state (e.g. the VT emulator). Replay rebuilds it. Decks never see it.
5. Large binary data goes through blobs, not state.

## Interface

A runner has two kinds of functions, as in `realtime-workspace` (RFC-0005 §3–§7):

| Kind | Called by | Declared in | Replayed on restore |
|---|---|---|---|
| **Reducer** (action) | A viewer, through the generated client | The app's own **interface trait**: params, result, and error are the app's | No: actions are never stored (D26) |
| **Hook** (system event) | The host | The fixed [`App`](#runner) trait | Only `on_output`, `on_exit`, `on_gap` |

```rust
// api.rs: the contract between runner and viewer
#[lattice_app::interface]
pub trait TerminalApi: App<State = TermState, Params = TermParams, Error = TermError> {
    fn input(&mut self, ctx: &mut Ctx, state: &mut TermState, req: Input) -> Result<(), TermError>;
    fn resize(&mut self, ctx: &mut Ctx, state: &mut TermState, req: Resize) -> Result<Size, TermError>;
}
```

1. Each method is a reducer: `Ok` commits one delta, `Err` rolls back. `&mut self` is the runner's private memory.
2. `#[lattice_app::interface]` (crate `lattice-app-macros`, re-exported by `lattice-app`) generates:
   - `TerminalApiAction`: one variant per method, postcard-encoded. New methods are appended; old clients keep working.
   - `TerminalApiRunner<T>`: the adapter the host calls: decode the action, call the method, encode `Res` or the error. It implements `lattice_app::Dispatch`; the worker registers `hosted::<TerminalApiRunner<Terminal>>`.
   - `TerminalApiClient<C>`: typed methods for viewers, the CLI, and tests. `client.resize(req)` returns `Ticket<Size, TermError>`.
3. A ticket settles `Ok(Res)`, `Err(CallError::App(E))`, `Err(CallError::Platform(code))`, `Err(CallError::NotConnected)`, or `Err(CallError::Unknown)` when the link ends while waiting. It never resends.
4. `Res` is a reply to the caller only. It is not stored, not replayed, and never needed to render: views read state.
5. The viewer never predicts. It shows state as the runner commits it.
6. One error enum per app: `thiserror` + serde, snake-case variant names as wire codes. On the wire an app error is `{code, detail}`: the variant name and its postcard encoding.
   The code is the variant name serde reports (`#[serde(rename_all = "snake_case")]` applies); no extra derive. A platform error (`app.bad_action`, `worker.no_instance`) has an empty `detail`: that is how a client tells the two apart.
7. The platform knows no app method. Hosts move opaque action bytes; only the generated code knows the types.

## Runner

```rust
pub trait App: Sized {
    const ID: &'static str;
    type State: Synced + Default; type Params; type Error;
    fn new(params: &Self::Params) -> Self;                          // private memory, e.g. VT emulator
    fn start(&mut self, ctx: &mut Ctx, state: &mut Self::State) -> Result<(), Self::Error>;
    fn on_output(&mut self, ctx: &mut Ctx, state: &mut Self::State, proc: ProcRef, bytes: &[u8]);
    fn on_exit(&mut self, ctx: &mut Ctx, state: &mut Self::State, proc: ProcRef, status: ExitStatus);
    fn on_gap(&mut self, ctx: &mut Ctx, state: &mut Self::State);
    fn resumed(&mut self, ctx: &mut Ctx, state: &mut Self::State);  // replay done; effects run again
    fn wake_at(&self, state: &Self::State) -> Option<Millis>;      // timers, RFC-0011
    fn wake(&mut self, ctx: &mut Ctx, state: &mut Self::State, now: Millis);
}
```

Code: `crates/lattice-app`. Reducers live in the interface trait, not here.

1. `Ctx` records effects: `spawn(template)`, `write`, `resize`, `signal`, `trim`, `finish`. The host runs them after commit.
   Example: terminal `input` only records `write(bytes)`. The keystrokes go straight to the shell and are never stored; the echo comes back as output.
2. `new`, `start`, `on_output`, `on_exit`, and `on_gap` must be deterministic in their inputs. They run again on replay.
3. During replay the host drops every effect, because the processes already exist and already got their input.
4. After replay the host calls `resumed`. The app re-issues what it needs live: timers, ephemeral requests, a PTY size nudge. A new instance gets `resumed` right after `start`.
5. Timers are not stored. After `resumed` the host reads `wake_at`.
   TBD: should `terminal` use `wake_at` to batch output into one delta per frame instead of one per read?
6. A runner spawns only from `[[spawn]]` templates. Params fill typed holes; they never form free argv.
7. Each instance runs in its own sandbox. Instances share no memory, state, journal, or processes.
8. TBD (with the first app that needs it): capability calls (`ctx.call("fs.read@1", req)`) and their `on_reply` hook; `[[spawn]]` `io = pty | pipes | none` and an events fd, with a `stream` tag on journal `Out` entries.

## Instances

| Action on `worker/apps` | Meaning |
|---|---|
| `create{app, package, params, session}` | Start an instance. Returns `iid` |
| `close{iid}` | Stop the instance, then delete it and its journal |
| `kill{iid}` | Send `SIGKILL` to every process of the instance. The close then completes |

1. A deck creates an instance from the `+` new-tab menu or a split (specs/03-sessions.md).
2. The worker validates params, assigns `iid`, and sends `Open{iid, meta, budget}` to `latticed-proc`. `meta` holds app id, package hash, and params.
   App id (proposed): reverse-DNS, ≤ 128 bytes, two or more labels of `[a-z0-9-]` joined by `.`. Journal id = `iid` as 8 big-endian bytes. Budget (proposed): 16 MiB.
3. Then the runner gets `start`. Invalid params get `ActionResult{err}`, and no journal opens.
   Empty params mean the app's default params (`App::Params: Default`, D46): what the deck sends from the `+` menu and the app picker, which have no params form.
4. `worker/apps` is a `State` stream: `{iid, app, package, session, state, procs}` per instance.
   M1 sends it as full states only: every change is one `Snapshot` of the whole record list at a new version.
5. `close` sends `SIGHUP` to every process of the instance and waits until all have exited. Then it sends `Drop{iid}`.
   A process whose `Spawned` is still due counts as live; it gets the signal when `Spawned` arrives.
6. The instance shows `closing` until `Drop` succeeds. It never escalates by itself; the user can send `kill` (specs/03-sessions.md).
7. A runner ends its own instance with `ctx.finish()`. The host then closes it as in rule 5, with no confirmation.
8. `terminal` calls `finish` when its shell exits (`exit`, `Ctrl-D`, crash). The pane closes.
9. A process started by hand inside a shell belongs to the shell's `terminal` instance. No app attaches to it.

| State | Meaning |
|---|---|
| `starting` | Package loading, `start` running |
| `running` | Runner live |
| `degraded` | Replay hit a `Gap`; state is partial |
| `failed` | Runner trapped, exceeded limits, or could not restore |
| `closing` | `close`, `kill`, or `finish`; waiting for processes to exit |

Each state maps to a design-system glyph (shape + hue). TBD: glyph mapping in `docs/design-system/`.

## Distribution

The deck holds installed packages. A worker caches packages by hash.

1. `create` names the package hash.
2. A worker without that hash sends `BlobRequest{hash}` to the deck on the same connection, verifies blake3, and caches it.
3. Then it starts the instance. The deck sees one `create` with a longer `starting`.
4. A deck that lacks the viewer of an instance fetches the package from the worker by hash.
5. Instances of different versions of one app run side by side.
6. Built-in apps are packages like any other: built from `apps/`, embedded in the deck, fetched by workers on first use (D44).
7. M1 implementation:
   - Package = postcard of `{manifest, runner}` (`lattice-package`); id = blake3 of those bytes.
   - `lattice-builtin-apps`' `build.rs` builds every `apps/*/` runner to wasm32 and embeds the packages. The deck's `+` menu lists them.
   - The deck answers `BlobRequest` while it waits for its `create` result. Creates for one hash wait together. A deck that leaves hands the request to the next waiting deck.
   - The worker checks the hash, compiles the runner (`lattice-wasm`), and caches the bytes in `$TMPDIR/lattice/packages/<hash>.lpkg`. At start it installs every cached package before restore.
   - A wrong hash or a module that does not load answers `worker.package_unavailable`. A blob nobody asked for is dropped.

TBD: cache eviction policy for unused packages on a worker? A data dir that survives reboots (the cache is in `$TMPDIR`)? Chunking for packages over one 1 MiB frame? Compiling off the core task?

## Sync to viewers

1. Each instance has one state stream, `inst/<iid>/state`. Its version counts committed deltas from the boot epoch (rule 10).
2. The runner keeps the last 1024 deltas in a queue. Older ones drop.
3. A deck subscribes with the last version it holds, or none.
4. Held version inside the queue: the worker sends the missing deltas, then live deltas.
5. No version, or one older than the queue: the worker sends the full state, then live deltas.
6. A deck applies a delta only at `version == held + 1`; otherwise it resubscribes.
7. History never crosses the wire. Decks get state and deltas only.
8. A delta that fails to apply is a bug: log it, then resubscribe without a version.
9. Wire mapping (02-sync.md): the worker is the only writer and the version is its `seq`. Full state = `Snapshot{covers: {worker: version}}`; delta = `Change{writer: worker, seq: version}`. `prev_hash`, `ts`, and `sig` are unused for `State` streams in M1 (TBD: signing).
10. Versions start at a per-boot **epoch**, not at 0: `unix_ms << 20` of the `latticed` start, saturating. `worker/apps` and every `inst/<iid>/state` start there.
    Reason: the queue and the version restart with `latticed`. A pre-restart version must never match a post-restart one.
11. Every version of a boot is greater than every version of an earlier boot. A deck that resumes with a pre-restart version gets the full state (rule 5), never deltas.
    Assumption 1: a boot commits fewer than 2^20 deltas per stream per millisecond it runs.
    Assumption 2: the wall clock never steps back between two boots.
    Checks: unit test `a_resume_from_before_a_restart_gets_the_full_state`; end-to-end test `an_instance_survives_a_latticed_restart` (ROADMAP M1.9).
12. The apps runner gets the epoch from `latticed` at boot. It never reads a clock (sans-IO).

TBD: guard against the wall clock stepping back between two boots (persist the last epoch in the data dir?).

## Restore

1. On start, `latticed` sends `List` to `latticed-proc` and creates one runner per journal from its `meta`.
2. The runner is built with `new(params)` and gets `start` with effects dropped, then every entry from `start_off` to `end_off`.
3. Then it goes live: effects run, and `wake_at` is read.
4. A `Gap` during replay calls `on_gap` and marks the instance `degraded`.
5. An app upgrade restores the same way. The new runner replays raw output, so state needs no migration.
6. A restored instance keeps its `iid`. Its state stream restarts at the new boot epoch (Sync to viewers rule 10), so decks resync from the full state.

## Clean points

A **clean point** is a journal offset where older process output no longer affects state.

1. At a clean point the runner calls `ctx.trim()`. The host sends `Trim{iid, upto: offset of the current entry}`.
2. Replay after a trim starts from an empty state plus `start`, then the remaining entries.
3. `terminal`: the screen and scrollback were cleared (`ESC[2J` and `ESC[3J`, as sent by `clear`) outside the alternate screen.
   Precisely: `ESC[3J` processed outside the alternate screen and outside a synchronized update (`ESC[?2026h`) clears the synced scrollback. It is a clean point only if the screen is blank right after it. `ESC[3J` inside a synchronized update clears nothing (TBD).
4. A runner that never trims relies on the journal budget. Old output is dropped and restore ends `degraded`.

TBD: `terminal` scrollback holds at most 10,000 rows; past that it keeps the newest 5,000 (one full delta), since `List` cuts only from the end.
TBD: does `terminal` lose emulator modes (cursor keys, bracketed paste, title) across a trim? If yes, is a resize nudge after restore enough?

## Manifest (`app.toml`)

Draft. Field names and the params type set are TBD.

```toml
id = "dev.lattice.terminal"
name = "Terminal"
version = "0.1.0"
api = "lattice:app@1"            # WIT world version
kinds = ["terminal"]

[params]                         # validated by the host before create
cwd = { type = "path" }

[[spawn]]                        # the only commands this app can start
name = "shell"
argv = ["$SHELL", "-l"]          # fixed; `$SHELL` is the only expansion (D45)

[permissions]
pty_write = true

[limits]
memory_mb = 64
cpu_ms_per_call = 50
journal_mb = 16

[widgets]
names = ["terminal"]             # named views for layouts
```

## Kinds

| Kind | Examples |
|---|---|
| `terminal` | Shell in a PTY |
| `coding_agent` | claude-code, codex: TTY and Visual views, approvals, plan, diffs |
| `runner` | tests, builds, logs, dev servers |
| `files` | file tree, editor, viewers (image, audio, video, markdown) |
| `metrics` | worker health, GPU, custom series. No process, so nothing survives a restart |
| `integration` | git, CI, issue tracker |
| `communication` | chat, notifications between nodes |
| `entertainment` | anything else; may use `canvas` |

An app may declare several kinds. TBD: confirm list.
The kind `runner` names a product category. It is unrelated to the runner part of an app.

## Permissions

| Permission | Scope |
|---|---|
| `process_spawn` | implied by `[[spawn]]` templates; confirmed at install |
| `pty_write` | processes of the same instance |
| `fs_read` / `fs_write` | paths on the worker; may use params (`{cwd}`) |
| `net_connect` | host list |
| `clipboard_read` / `clipboard_write` | deck |
| `notify` | deck notifications |
| `secrets` | named secrets |

Granted on the deck at install; upgrade shows a permission diff. A worker runs a package only if a paired deck sent it.
No permission lets an instance read another instance.

## Runtimes and limits

- Worker and desktop: wasmtime (`lattice-wasm`), one `Store` and one module instance per app instance. Fuel per call (proposed 1e9) and a memory cap (proposed 64 MiB) via `ResourceLimiter`. TBD: epoch deadlines when timers land.
- Web (later): browser WASM.
- iOS (sideloaded): viewers use an interpreter (wasmtime Pulley or wasmi), because iOS forbids JIT.
- Interface v0 (D43): core wasm + postcard, no imports. The guest (`lattice-app-guest`, `export_runner!`) runs the whole runner core: draft, commit, rollback, effects. The host only copies bytes.

| Runner export | Signature | Does |
|---|---|---|
| `lattice_abi` | `() -> u32` | ABI version (0) |
| `lattice_alloc` | `(len) -> ptr` | A buffer the host fills; the next call takes it |
| `lattice_init` | `(ptr, len, mode) -> u64` | Params in; `Option<Step>` out (`None`: bad params) |
| `lattice_step` | `(ptr, len, mode) -> u64` | An `Input` in; a `Step` out |
| `lattice_snapshot` | `() -> u64` | The encoded state |

  A `u64` result is `ptr << 32 | len` of a guest buffer, valid until the next call. `mode`: 0 replay, 1 live. A trap, running out of fuel, or a malformed result kills the runner; the instance becomes `failed`. TBD: `wake_at` export.

  A viewer module (`export_viewer!`) exports `lattice_abi`, `lattice_alloc`, and the calls below. The guest holds the instance state (a `Replica`) and enforces the version order itself (Sync to viewers rule 6). The deck runs it with `lattice-wasm` (`WasmViewer`); a dead viewer draws nothing.

| Viewer export | Signature | Does |
|---|---|---|
| `lattice_load` | `(ptr, len) -> u64` | `(version, full state)` in; `ReplicaEvent` out |
| `lattice_apply` | `(ptr, len) -> u64` | `(version, delta)` in; `ReplicaEvent` out (`Resync`: resubscribe) |
| `lattice_render` | `() -> u64` | The `ViewTree` (05-deck.md, View-tree protocol) |
| `lattice_handle` | `(ptr, len) -> u64` | A `UiEvent` in; the encoded actions out, sent fire and forget |
- Later: WASI 0.3 component model. Keep the WIT small and data-oriented.

## Data flow

```
viewer ── action ──▶ runner ── write / resize / signal / spawn / trim ──▶ latticed-proc ──▶ process
  ▲                    │                                                     │
  │                    ◀──────────── Out / Exit (live, or replay) ◀──────────┘
  └── deltas ◀──────── commit
```

## Coding agents (later)

Hook events, approvals, and the Visual view are out of the M1 PoC. They follow the same philosophy: an agent's durable data comes from the agent process itself. TBD: event capture design, after M1.

## Agent-authored apps

- Ship an `AGENTS.md` + app SDK docs + manifest schema so a local coding agent can build apps and layouts.
- `lattice app build` and `lattice app install` let agents build and install apps on a deck.
- The host enforces the manifest. Agent-written apps start with least privilege.

## Open questions

- TBD: is the delta queue (1024) a constant or set per app in the manifest?
- TBD: distribution beyond local install — a registry with signatures?
- TBD: params type set, and does the `+` menu ever show a params form?
- 