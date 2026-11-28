# Components

Desktop density shown. On touch, every interactive row grows to `row/touch` (44px).
Token names refer to [tokens.md](tokens.md).

## Button

| Variant | Fill | Border | Text | Use |
|---|---|---|---|---|
| primary | `accent` | none | `accent/ink`, 600 | One per surface. `Commit`, `Allow`, `Copy` |
| secondary | `bg/2` | `line/strong` | `fg`, 500 | `New session`, `Always` |
| ghost | transparent | none | `fg/muted` | `Cancel`, `Deny`, `Disable all` |
| danger | `danger/bg` | `danger/border` | `error` | `Kill session` |
| icon | `bg/2` | none | `fg` | Square, needs `aria-label` |

- Height `row/sm` (28) default, 22–24 inside rows, `row/md` (32) in forms.
- Radius `r/3`. Labels are verbs. A shortcut hint may follow the label: `Commit 2 files ⌘↵`.

## Key / chord (`kbd`)

JetBrains Mono 11px, `bg/2`, 1px `line/strong` border with 2px bottom, `r/3`, padding 1×6px.

## Input

- Height `row/md`, `bg/1`, 1px `line/strong`, `r/3`. Mono font for machine values (worker name, paths).
- Focus: border becomes `accent`. No glow, no outline.
- Always wrapped in a `<label>`.

## Toggle (switch)

30×16 track, 12px knob `fg`. On: track `accent`, knob right. Off: track `line/strong`.
Role `switch` with `aria-checked` and an `aria-label` naming the target.

## Segmented control

Container `bg/0` + `line`, `r/4`. Segment 24px high; selected `bg/3` + `fg`, others `fg/muted`.
Used for: TTY / Visual, Session / Worker / All.

## Tabs (terminal)

- Bar height 32, `bg/1`. Tab: mono 12px, padding 0 10–12px, right divider `line`.
- Active: `bg/3` + `inset 0 2px 0 accent`, text `fg`. Inactive: `fg/muted`.
- Content: state glyph (busiest pane) + name + `▦n` (pane count, `fg/dim` 10px). File tabs show a type icon (`{}` `♪` `▶` `◎` `¶` `▣`) and `●` when unsaved.
- Trailing `+` opens the New-tab menu.

## Tool tabs (right panel)

Text tabs `Files · Git · Ports · Agents`. Active = `fg` + `inset 0 -2px 0 accent`. Optional count badge (mono 10px). The Agents badge is `waiting` color when approvals pend.

## Pane header

Height 26–28, mono 11px, `bg/1`. Content: state glyph, pane id (`1.2`), worker, cwd, process (`· cargo watch`), optional agent name in `agent` color.
Focused pane: 1px `accent` ring. Others recede to `fg/muted`. No close buttons — `⌘W`.

## Session row

Grid `14px 1fr auto auto`, height 28. Glyph · name (mono 12) · port chip `⇄ n` · tab count.
Selected: `bg/3` + `inset 2px 0 0 accent`. Agents nest below their session, indented, with their own glyph and meta (`approve`, `15/40`, `done 2m`).

## Port chip

Mono 10px, text `accent`, 1px `accent/border`, `r/2`, padding 0 4px. Content `⇄ n` = enabled forwards in the session. Click opens the Ports tool.

## Worker card

Padding 12, `bg/1`, `line`, `r/4`.
Row 1: state glyph, name (mono 600), OS/arch (`mono/meta`), link + latency right (`direct · 18ms`).
Row 2: cpu and mem meters (3px bar on `line`; fill `fg/muted`, `waiting` color at ≥ 85%).
Row 3: agent chips — running: `agent/bg` fill, `agent/fg` text, diamond; idle: `bg/2`, `fg/muted`.

## Worker rail

Vertical list of 2-letter monograms (`B1`, `G2`, `MM`) plus `∗` (all workers). Each shows a state glyph and a `waiting` count badge. `⌘1–9` selects.
The Settings button (`gear` icon, same size as a rail entry, `r/6`, `aria-label="Settings"`) is pinned to the bottom, below a 1px `line` divider `sp/6` wide. Active (Settings tab focused): `bg/2` + 1px `line/strong` border + 2px left bar in `fg/muted` (≥ 3:1 non-text contrast on `bg/0`). Never `accent`: the accent bar marks the active worker.

## Port row

Grid `1fr auto 34px`, height 36.
`:remote → localhost:local` (mono 12) over `label · meta` (11px dim). State text: `live` (online), `off` (dim), `conflict` (error). Toggle at right.
Auto-detected listeners appear dimmed until enabled. A conflict stays off and cannot be toggled.
Header: session name, `n/m on`, `Enable all`, `Disable all`. Footer: `+ Forward port… ⌘⇧P`.

## Git change row · file row

Mono 12, height 24 (file tree 22). Path · `+a −d` (dim) · mark letter in diff color.
File tree: `▾`/`▸` carets, 12–14px indent per level, selected row `bg/3`, dirty dir/file `●` in `diff/modified`.

## Command palette (`⌘K`)

Overlay `r/6`, `bg/1`, `line/strong`, shadow overlay. Input row 40px with `❯` and block cursor in `accent`.
Result rows 30px: label · group (`pane`, `worker`, `agent`) · chord. Selected row `bg/3`.

## Context menu

Overlay style as palette. Items 28px with optional icon and chord. Section heads use `label`. Separators `line`.

## Toast

`bg/2`, `line/strong`, `r/6`, padding 10×12. Glyph · message (bold object name) · one secondary action (`Review`).

## Approval card

Appears on any surface where an agent waits.
`waiting` bar glyph + title (`Approval needed · shell command`) + command in mono + buttons `Allow Y` (primary), `Always A` (secondary), `Deny N` (ghost).
Row variant in lists: `waiting/bg` fill.

## Agent run summary (Visual)

- **Prompt bubble**: `you` + timestamp + origin device.
- **Thinking**: collapsed by default, `Thinking · 6s —` + text.
- **Tool group**: `▸ Explored 3 files · Read ×3 · 0.4s`, expands to rows.
- **Plan**: `Plan 3/5` checklist. Done = accent-filled box + strike; doing = accent outline; todo = `fg/faint` outline.
- **Edit card**: `M path +a −d` + `Open` / `Revert` + inline diff (optional).
- **Run card**: `❯ cmd` + `5 passed` + `exit 0 · 7.9s` + tail of output.

## Sync and link indicators

| Glyph | Text | Meaning |
|---|---|---|
| online dot | `cloud · synced` + counts | All state pushed |
| waiting bar | `cloud · syncing` + what | Push in progress |
| error square | `cloud · offline` | P2P still works, list may be stale |

TBD: architecture removes the cloud as a sync server. Labels should become `sync · …` (specs/02-connectivity.md). Pending canvas update.

## Relay RTT level

`signal-bars` glyph + `mono/meta`: `relay rtt · 23ms`. Four levels, filled-bar count + hue (tokens.md › Relay RTT level): `rtt/good` 3 bars, `rtt/fair` 2, `rtt/poor` 1, `rtt/down` empty bars with a cross. Never a dot or ring: those mean link type. The down level reads `relay · down` or the registration state (`registering`, `failed · <code>`). `aria-label` names the level and the value.

## Link chip

Height `row/xs`, `bg/1`, 1px `line`, `r/3`, `mono/meta` in `fg/muted`. Content: link glyph (online dot for direct, relay ring for relay) + `direct · 18ms` or `relay · 64ms`; a relay link appends `· via <relay host>`. Used in Settings › Connectivity › Status and the worker settings header.

## Settings row

Grid `minmax(0, 1fr) 320px`, gap `sp/6`, padding `sp/3` (`sp/4` left), bottom `line/subtle`. Mobile: one column, min height `row/touch`.

| Part | Shape |
|---|---|
| Title | `body`, `fg`, 600, after a dim breadcrumb (`Terminal › `) |
| Key id | `mono/meta` (`dev.lattice.terminal.font_size`). Selectable |
| Description | `body`, `fg/muted` |
| Control | Per type (specs/10-settings.md › Types): Toggle, stepper, dropdown, Input, chips. Height `row/md` |
| Scope tag | Optional `label`-style chip (`User`, `This deck`) on rows that allow a device override |

States:

| State | Shape |
|---|---|
| default | as above |
| modified | 2px left bar in `fg/muted` + `mono/meta` text `modified · default <value>` in `fg/muted`. No dot, no state hue: the text carries the meaning. Reset `↺` (icon button, `aria-label="Reset"`) beside the control |
| error | Control border `error`. Under it: `error` square glyph + code (`mono/meta`, `error`) + message (`fg`). The stored value stays in effect |
| read-only | Value in a box with a 1px dashed `line/strong` border on `bg/0`, `lock` icon, mono `fg/term`. No focus ring, `aria-readonly="true"` |

## Read-only notice

`bg/1`, 1px `line/strong`, `r/6`, padding `sp/3`. `lock` icon · bold lead ("Read-only on the deck.") · hint · the command (Invite command style, `❯ lattice config set <key> <value>`) with `Copy ⌘⇧C` · source meta (`from <path> · read 12s ago`, `mono/meta`).

## Invite command

Mono 12px, `bg/0` + `line`, `r/4`, padding `sp/3`, wraps anywhere. Text `echo lattice1:… | lattice add-deck` (proposed; specs/01 UI rule 5) with the accent `❯` before it. `Copy` beside it. Replaces the 6-character pairing code (D47).

## Progress meter

3px track `line`, fill `accent` (plans) or `fg/muted` (resources). Paired with mono 10px value (`3/4`).

## Mobile extra keys

Row height `row/key` (40). Keys: `esc tab ctrl alt ← ↑ ↓ →`. Latched modifier (`ctrl`) shows `bg/3` + `accent` text.
