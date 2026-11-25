# AGENTS.md

Entry point for AI agents and humans working in this repo. Read this first, then the rule files it links.

## Project

**Lattice** — a peer-to-peer terminal for engineers who live in the shell.
Two node types: **workers** (run processes: `latticed-proc` + `latticed`) and **decks** (control centers: macOS and iOS; web later). 100% Rust, UI on NSH.
Nodes talk E2E-encrypted over QUIC, relay-first then direct. No trusted server.
Everything syncs as streams (Log, State, Series, Doc). Features are WASM **apps** on a generic platform; **layouts** are data.
Coding agents (claude-code, codex, …) run on workers and show as **TTY** or **Visual** views.

Repo: https://github.com/idnwit/lattice · crate `lattice` (Rust, edition 2024). Early stage: specs lead, code follows.
Workflow: PR-only to `main`, rebase merge, Conventional Commits on every commit, release-plz (see `.rules/engineering.md`).

## Map

| Path | What |
|---|---|
| `docs/README.md` | Docs index |
| `docs/architecture/` | Platform design: transport, sync, identity, worker, deck, apps, layouts. Decision log |
| `docs/specs/` | Feature specs. Source of truth for user-visible behavior |
| `docs/design-system/` | Lattice design system: tokens, components, patterns |
| `docs/design-system/tokens.json` | Machine-readable tokens. Code imports from here |
| `docs/images/` | README screenshots, rendered from the design canvas. Re-render when the canvas changes |
| `.rules/` | Working rules. All apply, always |
| `crates/` | Rust workspace. One crate per brick in `brickie.toml` |
| `brickie.toml` | Crate graph: bricks, rules, links. Edit only with `brickie design` |

## Sources of truth

1. **Design canvas** (visual): https://claude.ai/artifact/XgswugNGzHbdYaWjpYDDhe
2. **Simulator** (end-to-end UX flow): https://claude.ai/artifact/5rwS5TZeyK6fziD1YRM8ak
3. **This repo** (`docs/`): written reference derived from 1 and 2.

Conflict between them? Stop and ask. Do not pick one silently.

## Rules (read all)

- [.rules/response-style.md](.rules/response-style.md) — how to reply in chat: terse, payload exact.
- [.rules/docs-style.md](.rules/docs-style.md) — how to write docs and specs.
- [.rules/design-change.md](.rules/design-change.md) — process for any UI/UX change.
- [.rules/engineering.md](.rules/engineering.md) — code, Rust, testing, commits.
- [.rules/spec-first.md](.rules/spec-first.md) — specs before code; how to update them.
- [.rules/architecture.md](.rules/architecture.md) — crate layering, forbidden edges, budgets, brickie.
- [.rules/verification.md](.rules/verification.md) — sans-IO cores, invariants, RED before GREEN, proofs.
- [.rules/review.md](.rules/review.md) — fresh-context review gates, structure lint.

## Verification (before every PR)

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
```

```sh
brickie check --measure --format short
```

Later, as each lands: `cargo deny check`, proptest with `PROPTEST_RNG_SEED`.

## Agent notes

- A nested `AGENTS.md` adds to and overrides this one for its directory (planned: `latticed-proc`, `lattice-proc-ipc`).
- Scratch output goes to `.generated/<slug>/` (gitignored). Delete it when done.

## Hard rules

- English only in repo files.
- 100% Rust. No JS/TS in the product (NSH pure-Rust mode, no TSX).
- Never invent product facts. Unknown → write `TBD:` with the question.
- Never hard-code a color, size, or timing that exists in `tokens.json`.
- State is shape + hue, never hue alone.
- Do not commit, push, or open PRs unless asked.
