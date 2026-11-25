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
deck core (Rust)
 ├─ transport (own UDP + quinn)    L1
 ├─ sync engine + local store      L2
 ├─ layout engine                  layout data + live state → screen tree
 ├─ app host (wasmtime)            runs app view parts (WASM) → view trees
 └─ view-tree → NSH elements       our interpreter
UI (NSH, pure Rust)
 ├─ shell chrome only: title bar, status bar, pairing, settings
 ├─ Terminal element fed by `screen`/`raw` streams
 └─ theme = docs/design-system/tokens.json
```

## NSH usage

1. **Pure-Rust app template only.** No TSX, no QuickJS.
2. **Terminal**: NSH `Terminal` element. Feed bytes with `write`; send `onData` as `input` actions. Implement NSH's `TerminalEmulator` trait over our emulator if needed for parity with the worker (`alacritty_terminal`).
3. **Theme**: map `tokens.json` into NSH theme tokens (colors, Inter / JetBrains Mono, spacing, radii). Dark only in v1.
4. **All feature UI comes from WASM apps** (apps.md). NSH plugins are TSX-only. We do not use them. App view parts emit our view tree; our interpreter maps it to NSH elements.
5. Native look differs per platform. Tokens apply; pixels may not match the canvas exactly.

## View-tree protocol

- View parts are pure: `render(state, ui_state) -> ViewTree`, `on_event(ui_event) -> [Action]`.
- Widget vocabulary v1 (draft): terminal, text, list, table, markdown, diff, button, input, toggle, chart, image.
- The vocabulary is a versioned API from day one.

## Local store

- Stream cursors and cached snapshots for fast reopen.
- Doc replicas (Loro) for user meta.
- Blob cache (content-addressed).
- Encrypted at rest with the user content key; key wrapped by macOS Keychain.

## Risks

- NSH is young (v0.7.0, pinned `=0.7.0`). API churn expected. NSH stays inside `lattice-deck` (D20).
- Verified 2026-11-25 with NSH 0.7.0: plain `cargo build` and `cargo run` work on macOS; the iOS simulator runs through the `nsh` CLI (09-platform-l1.md).
- The NSH `Terminal` element uses its own ANSI palette, not our tokens. The demo draws styled text instead.
- TBD: verify sideload (development / ad-hoc) `.ipa` packaging for iOS.
- TBD: Terminal element VT coverage and throughput under heavy output.
