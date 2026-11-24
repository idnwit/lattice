# 06 — Files and extensions

Status: draft
Owns: file tree, opening content, extensions, browser panes.

## Summary

Files live on the worker. Bytes stream from the worker over P2P.
An **extension** is a `files`-kind app ([architecture/06-apps.md](../architecture/06-apps.md)). It runs in the WASM sandbox with the permissions in its manifest.

## Opening content

1. Double-click a file → open as a tab with the default extension.
2. Drag a file → drop zone: `Split right`, `Below`, or `tab`.
3. Right-click → Open, Open to the Right, Open Below, OPEN WITH <extension>, Manage extensions…, Open in IDE ▸, Copy path, Reveal in Files.
4. `lo <file>` in a worker shell opens the file in a new tab on the attached deck.
5. Terminal URL hover → Open in browser tab `↵`, Open browser to the Right, Open browser Below, Copy link.
6. `+` new-tab menu: Terminal `⌥T`, Browser, Agent · <name>, Open file….
7. Mobile: long-press a file → Open as tab, Open as pane in this tab, OPEN WITH, OPEN IN IDE ON DESKTOP.
8. Image hover (desktop) → preview card: name, dimensions, size. Click opens; Space = fullscreen.

## Extensions

1. Each extension declares the file types it opens. One default per type opens on double-click; others under Open With.
2. Extension settings sync across devices.
3. Built-in set:

| Extension | Types |
|---|---|
| Code editor | `.rs .py .swift .toml .ts .json .md .csv …` |
| Markdown viewer | `.md` (default) |
| Image viewer | `.png .jpg .gif .webp .svg` (default) |
| Audio player | `.wav .mp3 .flac .ogg` (default). Waveform + 64-band mel spectrogram 0–8 kHz, click to seek |
| Video player | `.mp4 .webm .mov` (default). Filmstrip, frame step, speed |
| Browser | links, forwarded ports |

4. Community examples: CSV table (`.csv .tsv`), Hex viewer (any file). Packaging and sandbox: app manifest. v1 install is local via the `lattice` CLI (architecture/06-apps.md › v1 scope).

## Code editor save

1. `⌘S` saves to the worker. Unsaved tab shows `●`.
2. File changed on the worker since open → conflict bar `Reload` / `Keep mine`.

## Browser pane

Address bar shows the route (`⇄ build-01 localhost:3000/peers`). Requests go through the session's forward.

## Open questions

- TBD: large-file limits for editor and viewers.
- TBD: binary detection rule for the default extension.
