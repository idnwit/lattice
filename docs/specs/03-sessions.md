# 03 — Sessions, tabs, panes

Status: draft
Owns: session model, layout, persistence, pane types.

## Summary

A session is a named, persistent workspace on one worker. It survives deck disconnects.
Layout syncs between the user's devices (user Doc, E2E), so a session reopens identically on any device.

## Model

```
Worker 1─* Session 1─* Tab 1─1 SplitTree *─ Pane
Session 1─* PortForward
Session 1─* Agent
```

## Behavior

1. `+ Session` (`⌘T`) creates a session on the selected worker.
2. A tab holds a split tree. Split right `⌘D`, split down `⌘⇧D`. Close pane `⌘W`.
3. A pane may target a different worker than its session ("Split onto worker gpu-02…"). TBD: confirm ownership rules for such panes.
4. A pane shows one app widget. Common: `terminal` (shell, fallback for any process), `coding_agent`, browser, file viewer. See [architecture/06-apps.md](../architecture/06-apps.md).
5. Pane id is `<tab>.<pane>` (`1.2`), shown in its header.
6. PTYs live on the worker. Closing a deck never kills them.
7. Exit ≠ 0 marks the pane `error`; the session row shows `exited 1 · 12m ago`.
8. `Kill session` is destructive and needs confirmation.
9. Tab and session glyph = state of the busiest pane (see design-system tokens.md › State).
10. CLI: `lattice session <name>` attaches from the worker shell. TBD: full CLI.

## Mobile

- Splits become swipeable pages (`pane 1/3`).
- Sessions drawer groups sessions by worker.

## Data

| Session field | Example |
|---|---|
| name | `api-server` |
| worker | `build-01` |
| tabs | ordered list with split trees |
| ports | see 04 |
| subtitle | `zsh · cargo · vite`, `claude-code needs approval` |

## Open questions

- TBD: max panes per tab?
- TBD: scrollback size and whether it persists across worker restart.
- TBD: concurrent attach from two devices — shared cursor, resize policy (smallest wins?).
