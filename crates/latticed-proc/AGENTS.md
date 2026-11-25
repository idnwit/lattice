# AGENTS.md — latticed-proc

Adds to the root `AGENTS.md`. These rules override it in this directory.

- Frozen daemon. It never restarts for upgrades. Changes here are rare and reviewed.
- Dependencies: `lattice-proc-ipc`, `rustix`, std. Nothing else.
- Sync, single-threaded, `poll`-based. No async runtime, no networking, no VT parsing.
- It knows nothing about apps. `Spawn` takes opaque argv, env, cwd, and tag.
- Budget: this crate + `lattice-proc-ipc` ≤ 2,500 lines.
- `unsafe`: exactly one `#[allow(unsafe_code)]` (PTY spawn in `pre_exec`) with a `// SAFETY:` comment.
- `ring` is a sans-IO core in `.rules/verification.md` scope: proptest state machine.
