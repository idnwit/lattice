# Engineering rules

## General

- Read the relevant spec before coding. Behavior not in a spec → update the spec first (`spec-first.md`).
- Smallest change that solves the task. No drive-by refactors.
- Do not add dependencies without a reason in the PR description.
- Do not delete or shorten existing comments you were not asked to change.

## Rust

- Edition 2024. `cargo fmt` and `cargo clippy -- -D warnings` must pass.
- Errors: `thiserror` in libraries, `anyhow` only at binary edges. No `unwrap()`/`expect()` outside tests and proven invariants (comment the invariant).
- Async: `tokio`. Never block inside async. Bound every channel and buffer.
- Network input is untrusted. Validate lengths and enums at the boundary.
- Every timeout, retry count, and backoff is a named constant with its spec reference.
- Public types and functions get a one-line doc comment.

## Testing

- Each spec rule that is testable gets a test. Name tests after the rule: `relay_fallback_after_two_failed_probes`.
- Network tests use in-process fakes, not real sockets to the internet.
- Tests must be deterministic. Seed any randomness.

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
