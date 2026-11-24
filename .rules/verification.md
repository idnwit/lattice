# Verification

How we prove protocol and state code correct. Applies to the components in the scope table. Elsewhere, `engineering.md` testing rules are enough.

Adapted from luvox `VERIFICATION.md` and `formal-verification.md`, scaled down.

## Rules

1. **Sans-IO core.** Protocol and state logic are sync structs driven by events. Thin tokio or `poll` wrappers do the I/O.
2. **Deterministic core.** No `Instant::now`, `SystemTime::now`, `thread_rng`, or `HashMap` iteration in a core. Inject the clock and a seeded RNG. Use `BTreeMap`.
3. **Invariants first.** Write the invariant in the owning spec's `## Invariants` section (`spec-first.md`) before code.
4. **RED before GREEN.** The test or proof must fail on a placeholder first. Show the RED output in the PR description, or land the test as `#[ignore]` and un-ignore it in the implementation commit. Never land a red commit: every commit reaches `main` (rebase merge).
5. **Never weaken a check to pass.** No loosened assert, widened bound, new `assume`, or deleted case without owner sign-off in the PR.
6. **Independent oracles.** Roundtrip, conservation, or recompute-from-scratch. Never restate the formula under test.
7. **Named assumptions.** Every `kani::assume` names who enforces it. An `UNREACHABLE` assertion is a failed proof. `#[cfg(kani)]` may only shrink bounds.
8. **Stub what we do not own.** Never model-check crypto, quinn, tokio, wasmtime, or Loro internals. Test our code around them.

## Scope

| Component | Invariants | Technique | When |
|---|---|---|---|
| Relay codec and demux ([01](../docs/architecture/01-transport.md)) | No panic on any bytes. `0x00–0x3F` relay vs QUIC split is right. Roundtrip | fuzz, proptest roundtrip | first codec |
| Relay token table (01) | `SEND` accepted only from the token's bound address. Expiry honored | Kani inductive step | planned, once `stable` |
| `latticed-proc` IPC TLV ([04](../docs/architecture/04-worker.md)) | No panic on frames. Unknown tag ignored. Unknown type gives `Error{unsupported}` | fuzz, golden v1 fixtures | first codec |
| Ring buffer and fan-out (04) | `start_off ≤ end_off`. `Gap` exactly when evicted. Slow subscriber never blocks | proptest state machine | first code |
| Sync log validation ([02](../docs/architecture/02-sync.md)) | `seq` contiguous, `prev_hash` chain holds, signature checked | fuzz decoder, proptest | first code |
| Snapshot and compaction (02) | `fold(snapshot + tail) == fold(all)`. Resume loses and duplicates nothing | proptest oracle; Stateright 2–3 nodes | Stateright planned |
| `Doc` / Loro (02) | Any order or duplication of writer logs converges | proptest permutations | first code |
| Path selection FSM (01) | Relay fallback after 2 s of direct silence. No flapping | deterministic simulation, logical clock | first code |
| `latticed` shared state | Only if custom lock-free or atomic code appears | loom | if needed |

## CI

- proptest runs with a fixed `PROPTEST_RNG_SEED`. Commit regression files.
- IPC gate: a test decodes the checked-in v1 golden frames. Fixtures are never regenerated; v1 changes may only add optional tags.
- Fuzz: `fuzz/` targets run 5 min nightly, outside `ci-ok`.
- Kani: runs only when proof code changes. Becomes required once stable.
