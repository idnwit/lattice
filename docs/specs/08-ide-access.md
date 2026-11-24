# 08 — IDE access

Status: draft
Owns: SSH through the tunnel, Open in IDE.

## Summary

Desktop IDEs connect to a worker over SSH. The connection runs through the Lattice tunnel: no open ports, no keys to copy.

## Behavior

1. Each worker has a host alias `<worker>.lattice` (`ssh build-01.lattice`).
2. SSH access is off per device until granted. First use asks: "Allow SSH access to build-01?" → `Cancel` / `Enable & open in VS Code`.
3. The grant equals the power of opening a terminal. Removing the device from the account revokes it.
4. Open in IDE targets: VS Code, Cursor, Zed, JetBrains Gateway. Entry points: file context menu, session right-click, worker panel.
5. The worker panel lists live IDE sessions: IDE, path, origin device (`this Mac`, `from Mobile`).
6. Mobile cannot run an IDE. It can hand off to a desktop device ("OPEN IN IDE ON DESKTOP").
7. Fallbacks: `Copy URI`, `Copy ssh command`.

## Open questions

- TBD: how the deck installs the `*.lattice` SSH alias (ProxyCommand via `lattice` CLI?).
- TBD: handoff transport from mobile to desktop (cloud push?).
