# Spec first

Specs in `docs/specs/` own behavior. Code implements them. Tests prove them.

## Flow

1. Find the owning spec (`Owns:` line at the top of each file).
2. Behavior missing or wrong? Edit the spec first, in the same change as the code.
3. New concept? Add it to the glossary in `00-overview.md`.
4. UI-visible change? Follow `design-change.md` too.
5. Crate topology change? Follow `architecture.md` too.

## Rules

- A spec rule is numbered and testable. "Fast" is not a rule. "Reconnect within 2 s" is.
- Mark open points as `TBD: <question>`. Do not code past a `TBD` without asking.
- Status `draft` may change freely. Status `stable` changes need a note in the PR description.
- Specs for components in `verification.md` scope get an `## Invariants` section: numbered rules, each naming its check (type, test, proptest, fuzz, Kani, simulation). Tests and proofs link to the invariant; they never restate it.
- Keep specs and design system in sync: specs say *what happens*, design system says *how it looks*.
