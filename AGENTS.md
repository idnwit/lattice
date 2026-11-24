# AGENTS.md

Entry point for AI agents and humans working in this repo. Read this first, then the rule files it links.

## Project

**Lattice** — a peer-to-peer terminal for engineers who live in the shell.
Two node types: **workers** (run processes: `latticed-proc` + `latticed`) and **decks** (control centers: macOS first, then iOS; web later). 100% Rust, UI on NSH.
Nodes talk E2E-encrypted over QUIC, relay-first then direct. No trusted server.
Everything syncs as streams (Log, State, Series, Doc). Features are WASM **apps** on a generic platform; **layouts** are data.
Coding agents (claude-code, codex, …) run on workers and show as **TTY** or **Visual** views.

Repo: https://github.com/idnwit/lattice · crate `lattice` (Rust, edition 2024). Early stage: specs lead, code follows.
Workflow: PR-only to `main`, squash merge, Conventional Commits, release-plz (see `.rules/engineering.md`).

## Map

| Path | What |
|---|---|
| `docs/README.md` | Docs index |
| `docs/architecture/` | Platform design: transport, sync, identity, worker, deck, apps, layouts. Decision log |
| `docs/specs/` | Feature specs. Source of truth for user-visible behavior |
| `docs/design-system/` | Lattice design system: tokens, components, patterns |
| `docs/design-system/tokens.json` | Machine-readable tokens. Code imports from here |
| `.rules/` | Working rules. All apply, always |
| `src/` | Rust source |

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

## Hard rules

- English only in repo files.
- 100% Rust. No JS/TS in the product (NSH pure-Rust mode, no TSX).
- Never invent product facts. Unknown → write `TBD:` with the question.
- Never hard-code a color, size, or timing that exists in `tokens.json`.
- State is shape + hue, never hue alone.
- Do not commit, push, or open PRs unless asked.
