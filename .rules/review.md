# Review

Fresh-context review before decisions and merges. A light version of luvox `EGM.md` (Elephant-Goldfish Model).

- **Main agent** holds the context, drafts, and coordinates.
- **Reviewer** is stateless and read-only, with a fresh context. It never sees chat history or the author's conclusion.

## Gates

| Gate | Runs on | Reviewer input |
|---|---|---|
| Design | New or changed `D#` in `decisions.md`, `stable` spec changes, crate topology changes | Owning spec or decision, the proposed change |
| Code | PRs in `verification.md` scope, or over ~200 changed lines, before merge | Owning spec, PR description, diff |

Code gate also checks `verification.md`: RED evidence, a named enforcer for every `assume`, independent oracles.

## Findings

- Severity: `Blocker`, `Major`, `Nit`. Each with `file:line` and the rule it breaks.
- Fix every `Blocker` before moving on. `Nit` informs, never blocks.
- Escalate to the user when a `Blocker` is disputed, reviewers disagree, or fixes pass 2 rounds.
- No subagents available? Say so. A same-context review is not a fresh-context review.

## Structure lint (advisory)

- Run [`deslop-please`](https://github.com/deterministic-machines/deslop-please) on changed crates: deep modules, narrow interfaces.
- Fix findings from first principles (which layer owns this? can it hide behind a narrow interface?), never by mechanical moves.
- Fixes go in a separate `refactor:` PR. Findings never block merge.
