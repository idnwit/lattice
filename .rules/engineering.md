# Engineering rules

## General

- Read the relevant spec before coding. Behavior not in a spec → update the spec first (`spec-first.md`).
- Smallest change that solves the task. No drive-by refactors.
- Wider cleanups go in a separate `refactor:` PR, never inside a `feat` or `fix` commit.
- Do not add dependencies without a reason in the PR description.
- Do not delete or shorten existing comments you were not asked to change.

## Rust

- Edition 2024. `cargo fmt` and `cargo clippy --workspace --all-targets --all-features -- -D warnings` must pass.
- Lints live in `Cargo.toml` `[lints]` (`[workspace.lints]` after the split). See "Lints" below. Never weaken them to pass.

### Types

- Invariant types have private fields and a validated `try_new`. No `pub` fields on wire or state types.
- Phases, modes, and outcomes are closed `enum`s, never `bool` or strings.
- Narrowest visibility first. Never widen visibility for a test.
- No speculative generics, traits, flags, or shims without a live caller.
- Secret types (pairing code, session key, content key) implement a redacting `Debug`.

### Errors

- `thiserror` in libraries, `anyhow` only in binaries (row 0 in `architecture.md`).
- No `unwrap()` outside tests. `expect("why this holds")` only for a local invariant; the message is the proof.
- Never `let _ = fallible()` without a comment saying why the failure is safe to ignore.
- Errors that cross the wire or reach logs carry a stable `code() -> &'static str`. They never hold secrets or raw peer bytes.

### Async and I/O

- Async runtime: `tokio`. Exception: `latticed-proc` is sync, single-threaded, `poll`-based (`docs/architecture/04-worker.md`).
- Never block inside async. Never `block_on`. Keep sync and async code in separate modules.
- Bound every channel and buffer. Use `send().await` for backpressure. Use `try_send` only to shed load, and handle `Full` and `Closed` separately.
- `tokio::select!` arms are one-line calls into owner methods.
- `Drop` does only sync, infallible cleanup. Async teardown is an explicit `shutdown().await`.
- Network input is untrusted. Validate lengths and enums at the boundary.
- Every timeout, retry count, and backoff is a named constant with its spec reference.

### Logging, docs, CLI

- Logs: `tracing`, not `log`. Log lifecycle, decisions, and invalid states. No per-byte hot-path logs. No payloads.
- Every crate has a `//!` doc. Public items get a one-line doc. `cargo doc --no-deps` has zero warnings.
- Binary arguments: `clap` derive.

### Lints

Base set, every crate:

| Group | Lints (deny) |
|---|---|
| rust | `unsafe_code` (`forbid`), `missing_docs`, `unreachable_pub` |
| clippy | `unwrap_used`, `expect_used` (allowed in tests via `clippy.toml`), `panic`, `todo`, `unimplemented`, `dbg_macro`, `print_stdout`, `let_underscore_must_use`, `large_futures`, `undocumented_unsafe_blocks` |

- Binaries allow `clippy::print_stdout` at the crate root. Libraries never do.
- Strict tier, parser and state crates only (rows 3–5 in `architecture.md`, plus the journal module in `latticed-proc`): `indexing_slicing`, `arithmetic_side_effects`, `cast_possible_truncation`.
- `unsafe`: only `latticed-proc` (PTY spawn in `pre_exec`) and `lattice-app-guest` (rebuilding the buffer the WASM host filled, ABI v0, D43). Each sets `unsafe_code = "deny"` in its own lint table and has exactly one `#[allow(unsafe_code)]` with a `// SAFETY:` comment. The `#[unsafe(no_mangle)]` exports come from its `export_runner!` macro, expanded in app crates.
- `allow` needs a comment with the reason. Never `allow` a lint crate-wide to silence one site.
- Planned checks, added with the first external dependency: `cargo deny check` (required: advisories, licenses, crates.io only, duplicate majors), `cargo machete` (advisory).

## Testing

- Each spec rule that is testable gets a test. Name tests after the rule: `relay_fallback_after_two_failed_probes`.
- Table-driven asserts print the case: `assert_eq!(got, want, "case={name}")`.
- Unit tests inline in `#[cfg(test)] mod tests`; move to a sibling `tests.rs` past ~200 lines. Cross-crate and simulation tests go in `tests/`.
- Network tests use in-process fakes, not real sockets to the internet.
- Tests are deterministic. Seed any randomness. Verification rules: `verification.md`.

## UI code

- Import tokens from `docs/design-system/tokens.json` (or its generated module). No raw hex in components.
- Use component names from `docs/design-system/components.md`.

## Commits and PRs

- Conventional Commits for **every commit** and every PR title: `feat(relay): address-bound tokens`. Subject ≤ 72 chars.
- Types: `feat` `fix` `perf` `refactor` `docs` `test` `build` `ci` `chore` `revert`. Breaking change: `!` after type/scope.
- Scope = area: `transport`, `relay`, `sync`, `proc`, `latticed`, `deck`, `apps`, `layouts`, `pair`, `ui`, `docs`, `ci`.
- Subject: lowercase start, imperative, no trailing period.
- `main` changes only through a PR. Never push to `main` directly.
- Branch names: `<type>/<short-slug>` (`feat/relay-register`).
- Merge method: **rebase only**. Every PR commit lands on `main` as-is, so each must be a clean, conventional, self-contained commit. No merge commits; rebase onto `main` instead.
- Local check: `git config core.hooksPath .githooks` once per clone (commit-msg hook runs `scripts/check-conventional.sh`).
- Required checks: `ci-ok` (fmt, clippy, tests), `conventional-title`, `conventional-commits`.
- Releases: release-plz opens a release PR from commits on `main`. Merging it tags `vX.Y.Z` and creates a GitHub release. Never bump versions by hand.
- Commit, push, or open PRs only when asked.

## Security

- Pairing codes, session keys, and SSH credentials never go into logs.
- Relays and any storage peer see only ciphertext. No server stores plaintext user data.
- Agent tool approvals default to ask. "Always" is scoped per agent + command prefix.
