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

## Attach (CLI)

`lattice attach` is a minimal deck in a local terminal: it shows one `terminal` instance on a paired worker and sends it keys (ROADMAP M1.7).

1. `lattice attach` creates a `terminal` instance on the paired worker and attaches to it. `--worker` picks the worker, as for every deck command.
2. `lattice attach --iid <iid>` attaches to an existing instance instead. It exits with `no instance <iid>` and code 1 when the first `worker/apps` state lacks it.
3. Stdin and stdout must both be a TTY. Otherwise the command exits with code 1 before it connects.
4. A new instance starts at the local terminal size, in the worker's default cwd. Its session name is `attach`.
5. On attach, the client sends `resize` with the local size. Each `SIGWINCH` sends `resize` with the new size. A TTY that reports a zero side counts as 80 by 24.
6. While attached, the local TTY is in raw mode, on the alternate screen, with line wrap off.
7. Every stdin byte goes to the instance as `input`, unchanged, except the detach key.
8. The detach key is `Ctrl-\` (byte `0x1C`). Bytes typed before it in the same read are sent first. The instance keeps running.
9. On detach, the client prints `detached · instance <iid>` and exits with code 0. Stdin closing also detaches.
10. The instance leaving `worker/apps` (shell exit) ends the attach. The client prints `instance <iid> exited` and exits with code 0.
11. A link loss or any other error prints the error and exits with code 1.
12. The client restores the TTY on every exit path, panic included: cooked mode, main screen, line wrap on, cursor shown.
13. The client draws the instance screen, top-left aligned. Each frame redraws only the rows that changed. A local resize redraws all rows.
14. Cell colors pass through as SGR codes: indexed colors as indexed, true colors as true colors. The local terminal theme sets the hues.
15. Control characters in cell text are drawn as spaces. Instance state is untrusted network input.
16. Rows past the local height are not drawn.

- TBD: is `Ctrl-\` the final detach key? Should a flag change it, and how does the user send a literal `Ctrl-\`?
- TBD: `TermState` carries no terminal modes (application cursor keys, bracketed paste, mouse). Should it, so attach and the deck can mirror them?
- TBD: should attach show scrollback? It shows the screen only.

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
