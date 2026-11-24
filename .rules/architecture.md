# Architecture rules

Crate layering, forbidden edges, and size budgets. The conceptual layers (L1–L4) live in `docs/architecture/README.md`. This file owns the crate graph until `brickie.toml` exists; then `brickie.toml` owns it and this file keeps the rules.

Adapted from luvox `BRICKIE.md` and `AGENTS.md`.

## Rules

1. Dependencies point down only. No cycles. Crates in the same row never depend on each other.
2. Trust boundaries are missing edges. If a crate must not see something, it has no path to it.
3. Concrete I/O (quinn, UDP sockets, wasmtime, NSH, alacritty) lives only in adapter crates (row 2). Core and contract crates (rows 3–5) do not depend on `tokio`, `quinn`, or sockets.
4. Binaries are thin composition roots: wiring and lifecycle only.
5. Add a trait only at a real I/O or protocol boundary. No god-traits, no pass-through wrappers.
6. Name adapters after what they wrap: `lattice-quic`, not `lattice-transport-impl`.
7. Features are additive. `--all-features` must build. `cfg(target_os)` only in adapters and binaries.
8. A shared crate needs more than one real user. Never a dumping ground.
9. Create crates lazily: a crate appears when its first code lands. The first split from the single `lattice` crate is one `refactor(build)` PR.

## Layering

Row 0 is the top. A crate depends only on crates in lower rows.

| Row | Kind | Crates |
|---|---|---|
| 0 | bin | `latticed-proc`, `latticed`, `lattice-deck` (macOS), `lattice-relay`, `lattice` (CLI + `lattice-hook`) |
| 1 | host | `lattice-worker` (process host, app host wiring), `lattice-deck-core` (layout engine, view host), `lattice-ui-nsh` |
| 2 | adapter | `lattice-quic` (magic socket + quinn), `lattice-wasm` (wasmtime host), `lattice-vt` (`Emulator` + alacritty) |
| 3 | core | `lattice-sync` (streams, compaction, Loro allowed), `lattice-layout` |
| 4 | contract | `lattice-transport` (traits, `NodeId`, path-selection FSM), `lattice-identity`, `lattice-app-wit` (WIT, view-tree types), `lattice-relay-proto` (codec, token table) |
| 5 | frozen | `lattice-proc-ipc` (TLV codec, v1 fixtures) |

TBD: confirm crate names and rows when the first crates land.

## Forbidden edges

| From | Must not reach | Why |
|---|---|---|
| `latticed-proc` | anything but `lattice-proc-ipc`, `rustix`, std | Frozen daemon. Never restarts for upgrades |
| any library | `lattice-ui-nsh` | Only `lattice-deck` renders |
| `lattice-sync` | `lattice-quic`, `quinn` | Sync is transport-agnostic |
| `lattice-relay` | `lattice-sync`, `lattice-identity` | Relay sees only ciphertext |
| `lattice-app-wit` | wasmtime | Contract, not host |

## Budgets

| Budget | Limit | Enforced by |
|---|---|---|
| `latticed-proc` + `lattice-proc-ipc` | ≤ 2,500 lines | CI check, added with the crates |
| Function | ≤ 60 lines | clippy `too_many_lines` (deny) |
| File | ≤ 400 lines | `brickie --measure` (warn) |
| Cognitive complexity | ≤ 15 | `brickie --measure` (warn) |

## `latticed-proc` is a sealed shell

- Pure I/O, sync, single-threaded. Budgeted by size and edges, not by purity.
- Its ring buffer and fan-out state sit in a sans-IO module inside it (`verification.md`).
- It is the only crate with `unsafe` (`engineering.md` › Lints).
- Its nested `AGENTS.md`, created with the crate, repeats these rules. So does `lattice-proc-ipc`'s: v1 frozen, optional tags only, fixtures never regenerated.

## Brickie

Applies once `brickie.toml` exists. Run `brickie llms.txt` for exact syntax.

- Never hand-edit `brickie.toml`. Change it only through `brickie design`.
- Run `brickie design show` before every design edit. Never reuse a remembered base.
- A topology change needs a design draft and user approval before code.
- Every `[[link]]` carries a `--reason`.
- Implement bottom-up: contracts, core, adapters, binaries.
- Run `brickie interface <brick>` before using a brick from outside.
- After each change: `brickie measure && brickie check --brick <path>`.
- Gate: `brickie check --measure --format short` exits 0. A stale cache is a failure. Added to `ci-ok` with a pinned version.

TBD: brickie install source and version to pin in CI.
