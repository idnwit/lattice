# Lattice design system v0.1

A peer-to-peer terminal for engineers who live in the shell.
Dark by default, monospace where it matters, one accent, every state visible at a glance.

Visual source: [design canvas](https://claude.ai/artifact/XgswugNGzHbdYaWjpYDDhe). Change process: [`.rules/design-change.md`](../../.rules/design-change.md).

## Principles

| # | Principle | Rule |
|---|---|---|
| 01 | Keyboard first | Every action has a chord. The mouse is a fallback. |
| 02 | Density, not decoration | Pixels go to output. Chrome stays thin and quiet. |
| 03 | State always visible | Worker, link, agent, sync — never hidden in a menu. |
| 04 | One accent | Accent marks focus and the cursor. Nothing else. |

## Files

| File | Contents |
|---|---|
| [tokens.json](tokens.json) | Machine-readable tokens. Single source for code |
| [tokens.md](tokens.md) | Color, state glyphs, ANSI, type, spacing, density, radius, elevation, motion, icons |
| [components.md](components.md) | Buttons, inputs, tabs, pane header, rows, cards, palette, toast, toggles, chips |
| [patterns.md](patterns.md) | Screen layouts: desktop workspace, agent views, opening content, settings, mobile |

## Platforms

- **Desktop / Web** — advanced mode. Desktop density (rows 22–32px).
- **Mobile** — same model, thumb-first. Every row grows to 44px.
