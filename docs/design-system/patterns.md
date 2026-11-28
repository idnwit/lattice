# Patterns

Screen-level layouts. Each section names its canvas artboard. Behavior lives in `docs/specs/`.

## Desktop workspace — artboard "Desktop / Web — Advanced workspace"

```
┌ title bar 36: lattice / account · ⌘K search · sync · Add worker ─────────────┐
│ rail │ worker panel │ tabs 32                                   │ tools tabs │
│ B1   │ worker card  │ ┌ pane 1.1 ────────┬ pane 1.2 ──────────┐ │ Files Git  │
│ G2   │ + Session ⌘T │ │ shell            │ agent (TTY/Visual) │ │ Ports      │
│ MM   │ + Agent ⌘⇧A  │ │                  ├ pane 1.3 ──────────┤ │ Agents     │
│ ∗    │ SESSIONS     │ │                  │ logs (other worker)│ │            │
│      │ rows+agents  │ └──────────────────┴────────────────────┘ │            │
└ status bar 22: worker · link · path › tab › pane · ⇄ ports · ▍ waiting · utf-8 · cols×rows · shell ┘
```

- Left: worker rail + worker-scoped panel (`⌘B` hides). Sleeping worker shows "Asleep. Sessions restore when it wakes." + `Wake`.
- Center: tabs; each tab holds a split tree of panes. Panes may target different workers.
- Right: tools (`Files ⌘⇧E`, `Git ⌘⇧G`, `Ports ⌘⇧P`, `Agents`). Fluid: wraps below the center on narrow widths.
- Status bar always shows the focused pane's worker, link type, latency, and pending approvals.
- Failed instance pane (specs/03-sessions.md, Behavior 16–18): header error glyph + `instance.failed` meta, body `<App> failed` centered over `bg/0`. The same note layout as a failed create.
  TBD: canvas artboard for a failed instance pane. Does it keep the last screen of the instance, or show only the note?

## Agent views — artboards "Agent session · Visual ⇄ Terminal", "Agent pane in a split"

One agent, one PTY, several views. Views share one event cursor. The PTY never stops.

| View | Trigger | Shape |
|---|---|---|
| TTY | default | Raw terminal of the agent CLI |
| Visual in place | `⌘.` toggles | Pane renders hook events as cards |
| Zoom | `⌘⇧↵` | Pane fills the center; other panes become live chips in a breadcrumb; `esc` restores |
| Mirrored to side | dock button | Pane stays TTY; right panel `Agent` tab follows focus (or pinned) in Visual |

- Visual display options: Thinking, Tool output, Inline diffs, Timestamps; density comfortable / compact.
- Agent session (full) adds a right **Run panel**: status, elapsed, tool-call count, context %, files changed, checkpoints (`Rewind`), event stream.
- Approvals answer from any view with `Y` / `A` / `N`.

## Agents overview — artboard "Right panel · Agents overview"

Right-panel `Agents` tab. Scope segmented: Session / Worker / All. Sections: AGENTS (with inline approval), TODOS (grouped per agent), COMMANDS (approve / ok / time), FILES TOUCHED (`A` `M` `read`).

## Opening content — artboard "Open as tab · drag to split · Open With …"

- File tree: double-click → tab; drag → split (drop zones show `Split right` / `Below` / `tab`); right-click → context menu.
- Context menu: Open, Open to the Right, Open Below · OPEN WITH (extensions) · Open in IDE ▸ (VS Code, Cursor, Zed, JetBrains Gateway) · Copy path · Reveal in Files.
- `+` menu: Terminal `⌥T`, Browser, Agent · <name>, Open file….
- Terminal link hover: Open in browser tab `↵`, Open browser to the Right, Open browser Below, Copy link. Opening forwards the port if needed.
- Terminal command `lo <file>` opens a file in a new tab.

## Extensions — artboard "Extensions"

Viewer header (28px): name · `worker:path` · `Save ⌘S` (editors only) · `<Extension> ▾` switcher.
Built-in: Code editor, Markdown viewer, Image viewer, Audio player (waveform + mel spectrogram), Video player (filmstrip, frame step), Browser. Manager lists INSTALLED (toggle) and AVAILABLE (`Install`).

## Media input — artboards "Image hover · paste · voice", "Mobile — Attach image · hold to talk"

- Image file hover: preview card with name, dimensions, size; click opens; Space = fullscreen.
- Paste into agent TTY: chip `[Image #n] name · size` with P2P upload progress; CLI receives `[Image #n]`.
- Paste into Visual: attachment thumbnail in composer with remove `×`.
- Voice: mic in composer, shown only when the agent supports voice input. Desktop `⌥Space`; mobile hold to talk, slide up to lock.

## IDE access — artboard "Open in IDE · SSH access through the tunnel"

Worker panel shows `SSH access` with `ssh <worker>.lattice` and Copy. First use asks: "Allow SSH access to build-01?" with `Cancel` / `Enable & open in VS Code`. Lists live IDE sessions with origin device.

## Mobile — artboards "Mobile — …" (390×844)

Same model, thumb-first. All rows ≥ 44px. Radius `r/12` on sheets.

| Screen | Shape |
|---|---|
| Terminal | App bar (session, worker, link, `⇄n`) · tabs · pane switcher ("pane 1/3", swipe ⇆) · terminal · global approval card · extra keys · input |
| Sessions drawer | Sessions grouped by worker · `+ New` · WORKERS (n/m online) · `Add worker` |
| Tools sheet | Bottom sheet over dimmed terminal: Ports / Git / Files / Agents |
| Add worker (proposed) | Bottom sheet, `r/12`, 16px gutter: header + close · "Run this on the worker:" · invite command (wraps) · `Copy` (primary, full width) · status · helper · `New invite` next to a `Waiting` invite and once it ended (as desktop, specs/01 UI rule 6). Opens from the drawer `Add worker` |
| Agent session | Visual or Terminal · sticky approval in both · composer |
| Files as tabs | Long-press sheet: Open as tab, Open as pane, OPEN WITH, OPEN IN IDE ON DESKTOP |
| Attach / voice | Attach sheet: Paste image, Photo library, Take photo, File from worker… |

Splits become swipeable pages on mobile. Live ports open in the in-app browser; long-press copies the URL.

TBD: Mobile Add worker is a proposal (D47 names no iOS UI). Sheet or full screen, and what confirms `Copy`?

## Settings — artboards "Settings registry", "Settings tab", "Settings · Connectivity", "Settings · worker", "Raw settings.toml", "Mobile — Settings", "Mobile — Settings · Connectivity"

Behavior: [specs/10-settings.md](../specs/10-settings.md). Opens as a tab (`sliders` icon, `Settings`) in the tab bar.

```
┌ tabs: api-server · Settings ─────────────────────────────────────────────────┐
│ [search ⌘F] [User | This deck] scope note                    {} TOML ⌘⇧,     │
├ tree 232 ────────┬ content (max 820) ────────────────────────────────────────┤
│ Appearance       │ Connectivity   lattice.connectivity · n settings          │
│ Keyboard         │ STATUS · read-only · live              Test relay ⌘⇧T     │
│ Connectivity     │  relay rtt · registration · address · keepalive · peers   │
│ Workers ▾        │ RELAY  rows…                                              │
│   build-01       │ PATH   rows…                                              │
│ Apps ▾           │                                                           │
│   Terminal       │                                                           │
└──────────────────┴───────────────────────────────────────────────────────────┘
```

- Header bar `bg/1`: search Input (with `⌘F` kbd), scope Segmented control, scope note (`Synced to all your decks` / `Overrides User on this deck only`), `{}` secondary button.
- Tree `bg/1`, rows `row/sm`. Selected node: `bg/3` + 2px `fg/muted` left; the bar turns `accent` only while the tree has keyboard focus (specs/10 rule 22). Worker nodes carry their link glyph; app nodes carry an `error` count when a value fails.
- Content: section `label` heads over Settings rows. Worker pages start with the Read-only notice.
- Status block: Relay RTT level, registration, public address, last keepalive, then one Link chip per peer.
- Mobile: list of grouped sections (Deck, Workers, Apps; `r/12` cards, rows at least `row/touch`) → drill-down page per section. Status first on Connectivity. No TOML view.
- TOML view: read-only in v1 (specs/10 rule 26, D51). No `Save`.

## Add worker (desktop, D47) — artboard "Desktop — Add worker · invite flow · no worker · link states"

The `+` at the end of the worker rail opens a dialog over the workspace: `bg/1` + `line/strong`, 480px wide, `r/6`.

| Part | Shape |
|---|---|
| Header | `Add worker` · close (`Escape`) |
| Lead | "Run this on the worker:" |
| Command | Invite command component · `Copy` (primary) |
| Status | State glyph + mono meta: `registering…` (idle), `waiting for worker…` (waiting), `paired · build-01` (online), `expired` (offline), error code (error) |
| Helper | "Expires in 5 min. One use." |
| Footer | `New invite` (secondary): replaces the invite (specs/01 UI rule 6) |
