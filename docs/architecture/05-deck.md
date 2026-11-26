# 05 — Deck

Status: draft
Owns: deck core, UI layer, platform targets.

## Summary

A **deck** is the user's control center: the node that shows and steers workers.
It is a Rust core plus a UI built with **NSH** (https://nsh.dev, pure-Rust mode, native views).
The whole project is 100% Rust.

## Targets

| Order | Platform | UI backend | Notes |
|---|---|---|---|
| v1 | macOS 14+ | NSH → AppKit | Desktop views |
| v1 | iOS / iPadOS 15+ | NSH → UIKit | Mobile views. Sideloaded (own signing), not App Store |
| later | Web | NSH → DOM + WASM core | Needs WebTransport relay (transport.md › Web) |
| no | Windows, Android | — | Not planned |

macOS and iOS share one deck core and one app crate. Only the views differ: desktop workspace vs mobile screens (design-system patterns).

## Structure

```
lattice-deck (bin)                 thin NSH shell: window, wiring, live links
 ├─ lattice-deck-core              host: worker links, replicas, sessions model. No UI
 └─ lattice-deck-ui                component + layout library (NSH, pure Rust)
     ├─ tokens                     generated from docs/design-system/tokens.json
     ├─ components                 state glyph, buttons, styled lines, approval card,
     │                             worker / git / ports / files / tool tabs, chrome, mobile pieces
     ├─ workspace                  tab bar + `+` menu, split tree, pane frame + header,
     │                             empty-pane app picker, close confirmation, pane chords
     ├─ layout                     desktop (panels beside the workspace), mobile (one region
     │                             at a time, D41), responsive (600 pt)
     └─ examples/gallery           the canvas screens from fixtures; workspace PoC
later in deck core
 ├─ sync engine + local store      L2
 ├─ layout engine                  layout data + live state → screen tree
 ├─ app host (wasmtime)            runs app viewers (WASM) → view trees
 └─ view-tree interpreter          view-tree nodes → lattice-deck-ui components
```

Components take props (plain structs) and report events through closures. They read no global state, fixtures, or network, and no name is app-specific (`styled_lines` serves terminals, logs, and diffs). The deck composes the shell from them; the view-tree interpreter (`lattice-deck`, `interp`) draws each pane from its viewer's tree. M1 PoC: the deck runs viewers itself (`link`); TBD: move the app host and layout engine to `lattice-deck-core`.

## NSH usage

1. **Pure-Rust app template only.** No TSX, no QuickJS.
2. **Terminal**: the deck receives the parsed terminal state (screen, scrollback), not PTY bytes (06-apps.md › Sync to viewers). It draws styled rows (`styled_lines`). Key input goes out as the `input` action. TBD: can the NSH `Terminal` element render from a grid, or do we keep our own row view?
3. **Theme**: map `tokens.json` into NSH theme tokens (colors, Inter / JetBrains Mono, spacing, radii). Dark only in v1.
4. **UI library**: `lattice-deck-ui` holds every component (tokens, buttons, tabs, pane header, styled lines, inputs, panels) and the desktop and mobile layouts. Its `examples/gallery` shows the wiring with fixture data. The deck and, later, the view-tree interpreter use it. App crates do not depend on it: viewers are WASM and emit view trees.
5. **All feature UI comes from apps**: viewers are WASM from the M1 PoC (D42) and emit our view tree; our interpreter maps it to NSH elements. NSH plugins are TSX-only. We do not use them.
6. Native look differs per platform. Tokens apply; pixels may not match the canvas exactly.

## View-tree protocol

- The viewer holds the instance state and applies changes (06-apps.md, Runtimes). Viewers are pure: `render(state) -> ViewTree`, `handle(state, ui_event) -> [Action]` (`lattice_app::view::Viewer`). TBD: viewer UI state beyond the viewer's own memory.
- A tree is a flat list of nodes `{key, parent, widget}`. A parent comes before its children; children draw in list order. Keys are the viewer's, unique in the tree; events name the node by key.
- Vocabulary v0 (`lattice_app::view::Widget`): `Column` and `Row` (gap as a token step, grow), `Text` (text, tone), `Lines` (monospace rows of runs, cursor), `Terminal` (rows that take the keyboard), `Button`, `Input` (one line, submits and clears).
- A run has a tone, an optional background tone, and bold, dim, italic, underline. Tones are token roles, except `Rgb`: a color the app's data names (256-color and true-color terminal output), drawn as given.
- Events v0: `Press` (button), `Submit(text)` (input), `Key` (terminal; key name and modifiers), `Resize{cols, rows}` (terminal: the cells that fit its box, on first layout, a split, or a window resize).
- Rows keep a monospace grid, as iTerm draws them: a Private Use Area glyph (prompt icons, from the bundled Symbols Nerd Font Mono) takes one cell, a wide glyph (CJK, emoji) two, any other non-ASCII glyph one; combining marks stay with their base.
- The interpreter makes elements again only when the tree's shape changes (keys, parents, widget kinds, container props); other props update in place, so focus and drafts survive state updates.
- The vocabulary is a versioned API: enums only grow at the end (postcard tags). Planned: list, table, markdown, diff, toggle, chart, image.
- Terminal size: the interpreter measures the terminal's box and an invisible sample row (`cell_probe`), then sends `Resize`; the terminal viewer sends `resize` when it differs from the state.

## Local store

- Instance states with their `version`, for fast reopen.
- App packages installed on this deck (06-apps.md › Distribution).
- Doc replicas (Loro) for user meta.
- Blob cache (content-addressed).
- Encrypted at rest with the user content key; key wrapped by macOS Keychain.

## Risks

- NSH is young (v0.7.0, pinned `=0.7.0`). API churn expected. NSH stays inside `lattice-deck` and `lattice-deck-ui` (D39).
- Verified 2026-11-25 with NSH 0.7.0: plain `cargo build` and `cargo run` work on macOS; the iOS simulator runs through the `nsh` CLI (09-platform-l1.md).
- The NSH `Terminal` element uses its own ANSI palette, not our tokens. `styled_lines` draws styled text runs instead.
- TBD: verify sideload (development / ad-hoc) `.ipa` packaging for iOS.
- TBD: Terminal element VT coverage and throughput under heavy output.
