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

## Relay (M2)

Planned (M2.8, status display M2.10). M1 code is direct only.

Mechanism: [architecture/01-transport.md](../architecture/01-transport.md) › Connect sequence, and Relay wire format rules 24–28 (relay id pin). Settings: [10-settings.md](10-settings.md).

14. Each node (deck and worker) has a **home relay**, set as a URL `udp://host:port`. The scheme selects the relay protocol. Only `udp` exists. TBD: which later schemes (`wss://`, `quic://`)?
15. The value `none` means no home relay: peers reach the node directly, as in M1 (rules 7–10; 01-pairing.md › Addresses). `none` governs only the home relay: the node may still dial a peer through that peer's relay (rule 19). `none` is the default until a public relay exists. TBD: public relay host?
16. A node registers at its home relay on start and keeps the registration alive. Keepalive every 25 s (01-transport.md, Relay rule 3).
17. Once registered (`REG_OK`), the node has a **public address**: `Contact::Relay{host, port, relay_id}` of its home relay. The node id is held beside it, never inside (01-pairing.md › Contact). It is the node's only published contact point. A node with relay `none` is reached at its `ip:port` instead. Either one is the node's **contact** (glossary).
18. A node hands out its public address (invite, `PairRequest`, user Doc) only while registered. With relay `none` it hands out `Contact::Direct(ip:port)`.
19. To connect to node X, the dialer uses X's public address. It registers at X's relay unless it is registered there already. It sends QUIC through `SEND` at once (relay first), then upgrades to direct by hole punching when possible (M2.5). The relay stays as backup.
20. Peers on different home relays connect. A node may hold registrations at several relays: its home relay always, plus the relay of each peer it dials. An extra registration lives while a connection through it is open (proposed).

### Relay id pin

21. A relay URL may carry the relay id: `udp://<relay id>@host:port` (64 hex digits) pins it explicitly and always wins. Without one, the node trusts the id the relay sends on first contact and pins it, keyed by the relay `host:port` (10-settings.md, rules 29 and 37). Worker: `relay.id` + `relay.pinned_for` in `config.toml`. Deck: `lattice.connectivity.relay_id` (device scope).
22. A `COOKIE` that shows another id is treated as spoofed and ignored; the node keeps retrying. After 3 (proposed) consecutive attempts that bring only mismatched ids and no `REG_OK`, the node stops registering there and reports `relay.id_changed` (01-transport.md, Relay rule 28). The user re-pins (10-settings.md, rules 31 and 44). `latticed` logs `relay.id_changed` with the note that paired decks must pair again after a re-pin (10-settings.md, rule 40).
23. A dial through a peer's address takes the relay id from that address, never from first contact. If the dialer already holds a registration or pin for that `host:port` with another id, the dial fails at once with `relay.id_mismatch` and sends nothing. Mismatched ids count as in rule 22, and each one triggers the next `REGISTER` at once. A dial that fails after any id mismatch reports `relay.id_mismatch`; otherwise `transport.unreachable` (01-transport.md, Connect sequence step 3).
24. A relay that sends no id, to a node with no pin: the node cannot register and reports `relay.no_id`. The user pins with `udp://<id>@host:port`.

Security trade-off: [architecture/03-identity-security.md](../architecture/03-identity-security.md) › Threat notes.

### Relay status

25. The deck shows the status of its own relay link (Settings › Connectivity › Status, 10-settings.md rule 30):

| Field | Example |
|---|---|
| Home relay | `udp://relay.example.com:7400` or `none` |
| Registration | `registering`, `registered`, `failed · relay.timeout` |
| Public address | `relay.example.com:7400 · 7f3a…c91e` (relay `host:port` · first and last 4 hex digits of the node id) |
| Relay RTT | `relay rtt · 23ms` with its level glyph (table below) |
| Last keepalive | `4s ago`: age of the last `PONG` or `REG_OK` |
| Per peer | `build-01 · direct · 18ms` (dot), `gpu-02 · relay · 64ms · via relay-eu.example.com` (ring) |

26. Relay RTT is the time from a `KEEPALIVE` to the next `PONG`. A `PONG` later than 5 s (proposed) gives no sample. The status shows the latest sample. TBD: smooth samples?
27. Registration shows `failed` with a code in two cases. One: no `REG_OK` came within 15 s (proposed: 3 `REGISTER` tries). A try is one `REGISTER` sent, resends after a `COOKIE` included. Two: a relay id error (rules 22–24). The node keeps retrying (01-transport.md, Relay rule 13), except after `relay.id_changed`, `relay.id_mismatch`, or `relay.no_id`. A single mismatched `COOKIE` never shows `failed`.
28. `Test relay` sends a `KEEPALIVE` at once, or a `REGISTER` when unregistered. It shows the new RTT, or the failure code after 5 s (proposed).
29. A relay link to a peer shows `via <relay host>`: the relay that carries it.
30. Rule 6 holds for every setting: no setting hides the link type or the RTT.
31. A saved contact never updates in v1. When a worker's home relay or that relay's key changes, each of its decks must pair with it again (01-pairing.md, rule 22).

Relay RTT uses a signal-bars glyph (3 bars), so it never looks like the link dot or ring. Thresholds 50 ms and 150 ms are proposed.

| Relay RTT | Glyph | Token |
|---|---|---|
| < 50 ms | 3 of 3 bars filled, green | `rtt/good` |
| 50–150 ms | 2 of 3 bars filled, yellow | `rtt/fair` |
| > 150 ms | 1 of 3 bars filled, red | `rtt/poor` |
| down or unregistered | empty bars with a cross, red | `rtt/down` |

Codes: `relay.id_changed`, `relay.id_mismatch`, `relay.no_id`, `relay.timeout`, `relay.unresolved` (the relay host does not resolve). Proposed.
Canvas: "Settings · Connectivity", "Mobile — Settings · Connectivity".

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
