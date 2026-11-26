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
| Pair worker | TBD (D47): copy `lattice add-deck <invite>` · status. The artboard still shows Scan QR / Enter code |
| Agent session | Visual or Terminal · sticky approval in both · composer |
| Files as tabs | Long-press sheet: Open as tab, Open as pane, OPEN WITH, OPEN IN IDE ON DESKTOP |
| Attach / voice | Attach sheet: Paste image, Photo library, Take photo, File from worker… |

Splits become swipeable pages on mobile. Live ports open in the in-app browser; long-press copies the URL.

## Add worker (desktop, D47)

The `+` at the end of the worker rail opens a dialog over the workspace: `bg/1` + `line/strong`, 480px wide, `r/6`.

| Part | Shape |
|---|---|
| Header | `Add worker` · close (`Escape`) |
| Lead | "Run this on the worker:" |
| Command | Invite command component · `Copy` (primary) |
| Status | State glyph + mono meta: `waiting for worker…` (waiting), `pairing…` (idle), `paired · build-01` (online), `expired` (offline), error code (error) |
| Helper | "Expires in 5 min. One use." |
| Footer | `New invite` (secondary) once the invite expired or failed |

TBD: canvas artboard and simulator flow for this dialog (design-change.md).
