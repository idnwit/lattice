# Tokens

Values live in [tokens.json](tokens.json). This file explains usage. Never hard-code these values.

## Surfaces and text

| Token | Hex | Use |
|---|---|---|
| `bg/0` | `#0A0C0D` | App ground, terminal |
| `bg/1` | `#101315` | Panels, title bar, status bar |
| `bg/2` | `#161A1D` | Hover, inputs, raised |
| `bg/3` | `#1D2226` | Selected row, active tab |
| `line` | `#22282C` | Dividers, pane gutters |
| `line/subtle` | `#1A1F22` | Row separators inside a list |
| `line/strong` | `#2E353A` | Control borders, overlays |
| `fg` | `#E4E7E9` | Primary text, output |
| `fg/term` | `#C9CED2` | Terminal body, unselected rows |
| `fg/muted` | `#9AA3A9` | Labels, secondary |
| `fg/dim` | `#838C93` | Meta, hints. Minimum for readable text (≥ 4.5:1) |
| `fg/faint` | `#4A5359` | Line numbers, disabled marks. Never body text |
| `accent` | `#5CC8E8` | Focus, cursor, primary action |
| `accent/ink` | `#04161C` | Text on an accent fill |

Accent is user-selectable: `#5CC8E8` (default), `#E8A33D`, `#9BE36B`, `#B48CFF`.

## State: shape + hue, never hue alone

| State | Hex | Glyph | Meaning |
|---|---|---|---|
| online | `#46C46E` | filled dot 7–8px | Worker reachable, direct P2P |
| relay | `#46C46E` | hollow ring, 1.5–2px stroke | Reachable via relay |
| agent | `#B48CFF` | diamond (square rotated 45°) 6–7px | Agent running |
| waiting | `#E8A33D` | vertical bar 3×10px | Needs input or approval |
| error | `#F26D6D` | filled square 7–8px | Exit ≠ 0, link lost |
| offline | `#6B747B` | hollow ring, dim | Worker asleep |
| idle | `#838C93` | hollow ring | Agent idle |

TBD: the canvas labels offline `#5A6369` but draws its ring in `#6B747B`. Which is canonical?

A tab or session shows the glyph of its busiest child. Priority: waiting > error > agent > online/relay > idle > offline. TBD: confirm priority order.

Git marks: `M` `#E8C15A`, `A` `#46C46E`, `D` `#F26D6D` (deleted path also struck through).

## Terminal ANSI 16 (tuned for `bg/0`)

| 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 |
|---|---|---|---|---|---|---|---|
| `#1D2226` | `#F26D6D` | `#46C46E` | `#E8C15A` | `#5C9DFF` | `#B48CFF` | `#5CC8E8` | `#C9CED2` |
| **8** | **9** | **10** | **11** | **12** | **13** | **14** | **15** |
| `#4A5359` | `#FF9494` | `#7BDC97` | `#F5D88A` | `#8CB9FF` | `#CFB4FF` | `#8FDCF2` | `#F4F6F7` |

## Typography

| Token | Spec | Use |
|---|---|---|
| `title` | Inter 20/28 600 | Screen titles |
| `heading` | Inter 14/20 600 | Panel headings |
| `body` | Inter 13/18 400 | Default UI text |
| `label` | Inter 11/16 600, +0.08em, uppercase | Section labels (`CHANGES`) |
| `mono/term` | JetBrains Mono 13/20 400 | Terminal |
| `mono/ui` | JetBrains Mono 12/16 500 | Paths, worker names, tabs |
| `mono/meta` | JetBrains Mono 11/14 400, `fg/dim` | Hashes, latency, durations |

- **Inter**: chrome, labels, panels. Tabular numerals on.
- **JetBrains Mono**: terminal, paths, hashes, keys — anything a user might copy.
- The `mono` stack falls back to **Symbols Nerd Font Mono** for the Private Use Area icons shell prompts draw; one icon takes one cell. Both fonts ship inside the app (`crates/lattice-deck-ui/fonts/`). TBD: design canvas update (design-change.md).

## Spacing (4px grid)

`sp/1` 4 · `sp/2` 8 · `sp/3` 12 · `sp/4` 16 · `sp/6` 24 · `sp/8` 32 · `sp/12` 48.

## Density (row heights)

| Token | px | Use |
|---|---|---|
| `row/xs` | 22 | Status bar, file tree |
| `row/sm` | 28 | Tabs, pane header, list rows |
| `row/md` | 32 | Inputs, buttons, title bar |
| `row/key` | 40 | Mobile extra-keys row |
| `row/touch` | 44 | Every tap target on mobile |

## Radius

`r/0` panes · `r/3` controls · `r/6` overlays · `r/12` mobile sheets and cards.

## Elevation and focus

- No shadows in-window. Elevation = one step lighter surface + 1px `line/strong`.
- Overlays only (palette, menus, sheets): `0 8px 24px #000a`.
- Focused pane: 1px inset `accent` ring. The only thing that glows.
- Active tab: `bg/3` + 2px `accent` top. Active tool tab: 2px `accent` bottom. Active row: 2px `accent` left.

## Motion

| Event | Duration |
|---|---|
| Hover | 80ms |
| Split / resize | 120ms |
| Sheets | 200ms |
| Terminal content | 0 — nothing animates inside a terminal |

## Icons

16px grid, 1.5px stroke, round caps and joins, `currentColor`. Decorative icons get `aria-hidden="true"`.
Set: `terminal`, `split-v`, `split-h`, `git-branch`, `folder`, `worker`, `agent`, `pair-qr`, `cloud-sync`, `p2p-link`, `search`, `new`.
Source SVGs: Foundations artboard on the canvas.
