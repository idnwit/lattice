# AGENTS.md — latticed-proc

Adds to the root `AGENTS.md`. These rules override it in this directory.

- Frozen daemon. It never restarts for upgrades. Changes here are rare and reviewed.
- Dependencies: `lattice-proc-ipc`, `rustix`, std. Nothing else. Dev-dependencies: `proptest` only.
- Sync, single-threaded, `poll`-based. No async runtime, no networking, no VT parsing.
- It knows nothing about apps. `Open` takes opaque meta; `Spawn` takes opaque argv, env, cwd and names a journal.
- Budget: this crate + `lattice-proc-ipc` ≤ 2,500 lines.
- `unsafe`: exactly one `#[allow(unsafe_code)]` (PTY spawn in `pre_exec`) with a `// SAFETY:` comment.
- `journal` is a sans-IO core in `.rules/verification.md` scope: proptest state machine.
