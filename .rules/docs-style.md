# Docs style

Docs are short, complete, and scannable. Write for an engineer who will implement from them.

## Rules

1. **English only.**
2. **Lead with the rule, then the reason.** One line each when possible.
3. **Plain grammar.** Full sentences, but no filler. Unlike chat, keep articles: docs persist.
4. **Max 20 words per sentence.** Active voice. Present tense. "Worker sends X", not "X will be sent".
5. **One term per concept.** Use the glossary in `docs/specs/00-overview.md`. Add new terms there first.
6. **Tables for enumerations** (tokens, states, keys, fields). Prose for behavior and reasons.
7. **Exact values.** Hex, px, ms, ports, key chords. Never "about" or "small".
8. **Mark unknowns.** `TBD: <question>` inline. Never invent a value to fill a gap.
9. **No duplication.** Link to the owning doc instead of copying. Tokens live only in `tokens.json` and `tokens.md`.
10. **Small files.** One topic per file. Split past ~200 lines.
11. **No decoration.** No emoji, no marketing tone, no "simply" or "just".

## Spec file template

```md
# NN — Title

Status: draft | stable
Owns: <what this spec is the source of truth for>

## Summary
2–4 lines.

## Behavior
Numbered rules. Testable.

## States / data
Tables.

## UI
Pointer to design-system pattern + canvas artboard name.

## Open questions
- TBD: …
```

## Pre-commit check

- Every value traceable to the canvas, simulator, or a decision note?
- Every `TBD` phrased as a question?
- Glossary terms used consistently?
- Links resolve?
