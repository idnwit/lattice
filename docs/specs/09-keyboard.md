# 09 — Keyboard

Status: draft
Owns: the default key map.

Every action has a chord. Shown with macOS glyphs; on Linux/Windows `⌘` maps to `Ctrl`. TBD: confirm mapping and conflicts with terminal control keys.

## Global

| Chord | Action |
|---|---|
| `⌘K` | Command palette |
| `⌘1–9` | Select worker |
| `⌘J` | Jump to session |
| `⌘B` | Toggle sessions panel |
| `⌘T` | New session / tab |
| `⌥T` | New terminal (`+` menu) |
| `⌘⇧A` | New agent |
| `⌘,` | Settings (opens or focuses the Settings tab, specs/10) |

## Panes

| Chord | Action |
|---|---|
| `⌘D` | Split right |
| `⌘⇧D` | Split down |
| `⌥←` `⌥→` | Move focus |
| `⌘W` | Close pane |
| `⌘⇧↵` | Zoom pane |
| `esc` | Restore split (in zoom) |

## Tools

| Chord | Action |
|---|---|
| `⌘⇧E` | Files |
| `⌘⇧G` | Git |
| `⌘⇧P` | Ports / forward a port |
| `⌘↵` | Commit (Git) · Send (agent composer) |
| `⌘S` | Save file |

## Agents

| Chord | Action |
|---|---|
| `⌘.` | Toggle TTY ⇄ Visual |
| `Y` / `A` / `N` | Allow / Always / Deny approval |
| `esc` | Interrupt agent |
| `⌥Space` | Voice input |

## Settings

Active only while the Settings tab has focus (10-settings.md › Editor).

| Chord | Action |
|---|---|
| `⌘F` | Focus search |
| `⌘⇧S` | Toggle scope: User ⇄ This deck |
| `⌘⇧,` | Open the TOML view of the selected scope |
| `⌘⇧T` | Test relay (Connectivity page) |
| `⌘⇧C` | Copy the `lattice config set` command (worker page) |
| `⌘⌫` | Reset the focused row to the next scope down (proposed). Only when a row has focus and no text field is being edited |
| `↑` `↓` | Move in the tree (tree has focus) |
| `←` `→` | Collapse / expand a tree node (tree has focus) |

## Mobile extra keys

`esc tab ctrl alt ← ↑ ↓ →`. `ctrl` latches for one keystroke.

## Open questions

- TBD: chord that toggles the right panel (desktop). The `lattice-deck-ui` gallery uses `⌥⌘B` until decided.
- TBD: `⌘T` is shown as both "new session" (worker panel) and "new tab" (chords). Pick one.
- TBD: `Y/A/N` focus rules — only when an approval is focused?
- TBD: chords for `Re-pin` (relay id, Settings › Connectivity), `Copy` and `New invite` (Add worker dialog), and path browse (settings `path` rows)?
