# 04 — Port forwarding

Status: draft
Owns: port detection, forward lifecycle, conflicts.

## Summary

Forwards map a worker port to a deck `localhost` port. They belong to the session, not the window.

## Behavior

1. `latticed` auto-detects new listeners in the session. They appear dimmed (`off`, `auto-detected`) until enabled.
2. User can pin a manual forward: `+ Forward port… ⌘⇧P`. Pinned forwards show `pinned`.
3. Local port defaults to the remote port. User may remap (`:5432 → localhost:15432`).
4. Local port in use → state `conflict`, meta `local :8080 in use`. Stays off; the toggle is disabled.
5. `Enable all` / `Disable all` act on every non-conflicting row.
6. Forwards follow the session. Reopen it on any device and enabled ports come back.
7. Opening a `localhost` link from a terminal forwards the port automatically if needed.
8. Session row shows `⇄ n` = enabled forwards. Status bar shows `⇄ n ports`.
9. Mobile: live ports open in the in-app browser. Long-press copies the URL.

## States

| State | Color | Meaning |
|---|---|---|
| live | online | Forward active |
| off | `fg/dim` | Detected or configured, not active |
| conflict | error | Local port taken |

## Open questions

- TBD: UDP support?
- TBD: forward from a pane on another worker — which session owns it? (designs show `gpu-02 · relay` row inside `api-server`)
