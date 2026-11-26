# 02 — Connectivity and sync

Status: draft
Owns: user-visible link behavior, worker states, device sync states.
Mechanism: [architecture/01-transport.md](../architecture/01-transport.md), [02-sync.md](../architecture/02-sync.md).

## Summary

Every connection starts on a relay, so it is usable at once. It upgrades to a direct path by hole punching in the background.
All traffic is QUIC, end-to-end encrypted between nodes. Relays see only ciphertext.
The user's devices sync settings and layouts with each other directly. No server holds the plaintext.

## Link

1. Connect = dial the worker's NodeId. Traffic flows over the relay immediately.
2. In parallel, both sides probe for a direct path. When it works, traffic moves to it without interruption.
3. Symmetric NAT or failed punch → stay on relay (`staying on relay (rtt 64ms)`).
4. Path changes (NAT rebinding, wifi → cellular) migrate inside the same QUIC connection. Sessions do not restart.
5. Lost connection (e.g. mobile app backgrounded) → reconnect and resume every stream from its cursor. Workers keep all state.
6. Link type and RTT are always visible: worker card, pane header, status bar (`direct · 18ms`, `relay · 64ms`).

### Reconnect (M1, direct only)

7. When a link ends, the deck dials the worker again. Panes and their viewers stay.
8. Waits between dials: `RECONNECT_FIRST` = 500 ms, doubled after each failed dial, capped at `RECONNECT_MAX` = 8 s (proposed).
9. A dial that does not open a link within `CONNECT_TIMEOUT` = 5 s (proposed) counts as failed.
10. A link that opens resets the wait to `RECONNECT_FIRST`.
11. After a link opens, the deck subscribes to `worker/apps` and to each viewer's stream at the version it holds. The worker sends only what is missing (architecture/06-apps.md, Sync to viewers).
12. A pane action sent while the link is down fails: `create` with `deck.offline`; input is dropped.
13. The rail and the status bar show the active worker's link state (Link states below).

TBD: detecting a dead link takes the QUIC idle timeout (30 s, lattice-quic). Add an application-level heartbeat?
TBD: QUIC keep-alive must stay below NAT UDP mapping lifetimes. About 30 s was observed for one-off flows behind a VPN NAT. `lattice-quic` sends a keep-alive every 5 s now (`KEEP_ALIVE`). Is 5 s the spec value, and is it enough for every NAT we support?

### Link states (deck)

| State | Glyph | Status bar text |
|---|---|---|
| connecting | ring (idle) | `build-01 · connecting` |
| up | dot (direct) or ring (relay) | `build-01 · direct · 3ms` |
| lost | square (error) | `build-01 · reconnecting · <error code>` |
| no worker | dim ring (offline) | `no worker` |

A paired worker that is not the active one shows offline (M1 links one worker).
Canvas: "Desktop — Add worker · invite flow · no worker · link states".

## Worker states

| State | Glyph | Meaning |
|---|---|---|
| online | dot | Direct path up |
| relay | ring | Reachable via relay only |
| offline | dim ring | Asleep or unreachable. Shows `asleep · 3h` |
| error | square | Link lost |

- Offline worker: panel shows "Asleep. Sessions restore when it wakes." and `Wake`. TBD: wake mechanism (Wake-on-LAN via another worker on the LAN?).

## Device sync

Synced between the user's decks (user `Doc`, E2E): device list, worker list, session list and layouts, port-forward config, app and extension settings, app/layout definitions.
Read live from workers, not copied between decks: terminal content, file content, agent events.

| Sync state | Text |
|---|---|
| synced | `sync · up to date` + `3 workers · 7 sessions · 2 agents` |
| syncing | `sync · syncing` + what (`pushing session layout`) |
| offline | `sync · offline` — workers still reachable, other devices may be stale |

TBD: the canvas labels these `cloud · …`. Rename in the design (design-change process).

## Open questions

- TBD: relay regions and selection.
- TBD: is the optional ciphertext store on by default?
