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
Session 1─* AppInstance 0─* Process
Pane    *─1 AppInstance
```

## Behavior

1. `+ Session` (`⌘T`) creates a session on the selected worker.
2. A tab holds a split tree. Split right `⌘D`, split down `⌘⇧D`. Close pane `⌘W`.
3. A pane may target a different worker than its session ("Split onto worker gpu-02…"). TBD: confirm ownership rules for such panes.
4. A pane shows the viewer of one app instance. Common: `terminal` (shell), `coding_agent`, browser, file viewer. See [architecture/06-apps.md](../architecture/06-apps.md).
5. Pane id is `<tab>.<pane>` (`1.2`), shown in its header.
6. PTYs live on the worker. Closing a deck never kills them.
7. A shell that exits (`exit`, `Ctrl-D`, crash) closes its pane at once, with no confirmation. TBD: the canvas shows `exited 1 · 12m ago` in the session row; drop it, or keep a short notice?
8. `Kill session` is destructive and needs confirmation.
9. Tab and session glyph = state of the busiest pane (see design-system tokens.md › State).
10. CLI: `lattice session <name>` attaches from the worker shell. TBD: full CLI.
11. The `+` new-tab menu lists the installed apps. Picking one sends `create` and opens a new tab with that instance. No params form in v1. M1 PoC apps: `Echo`, `Terminal`.
12. Split right and split down open an empty pane with an app picker (the same list as `+`). Nothing is created until the user picks an app. Closing an empty pane needs no confirmation.
13. Closing a pane by the user asks for confirmation: the instance stops and its history is deleted.
14. After confirmation the deck sends `close{iid}`. The pane shows `closing` until every process has exited and the instance is deleted. Then the pane goes away.
15. Right-click on a tab opens a context menu with `Force kill`. It sends `kill{iid}` for the instances of that tab. TBD: design canvas and simulator update (design-change.md), and its key chord?

## Mobile

- The deck shows one region at a time: left panel, workspace, or right panel. A switcher picks the region and hides itself after use. On desktop the panels toggle beside the workspace instead.
- Splits become swipeable pages (`pane 1/3`).

## M1 PoC deck

The first deck build shows only the workspace: tabs, splits, the `+` menu, and a status line for the local worker link. No sessions list, no left or right panel content. Layout and components come from `lattice-deck-ui` (05-deck.md).
TBD: design canvas and simulator update for the app picker and the mobile region switcher (design-change.md).
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
