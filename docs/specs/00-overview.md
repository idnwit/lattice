# 00 — Overview

Status: draft
Owns: product scope, glossary, system components.

## Summary

Lattice is a peer-to-peer terminal. A user pairs **workers** (their machines) and **decks** (control centers: macOS first, then iOS; web later) into one signed device list.
Decks open **sessions** on workers over an E2E-encrypted, relay-first P2P link.
Coding **agents** run inside sessions and can be watched and steered as raw terminal or as a visual event view.
There is no trusted server. Hosted services (relay, discovery, optional ciphertext store) never see content.
Platform design: [../architecture/README.md](../architecture/README.md).

## Goals

1. Terminal-first workspace across many machines: tabs, splits, panes on different workers.
2. Same model on desktop and mobile. Mobile is thumb-first, not reduced.
3. First-class coding agents: approvals, plans, diffs, from any device.
4. Zero network setup: no open ports, no SSH keys to copy, port forwarding per session.

## Non-goals (v0.1)

- TBD: team / shared workers? Current designs show a single personal account (`/ personal`).
- TBD: hosted (cloud-run) workers?

## System components

| Component | Runs on | Role |
|---|---|---|
| `latticed-proc` | Worker | Frozen process daemon. Owns PTYs and children, raw output ring. Never restarts |
| `latticed` | Worker | Upgradable connectivity daemon. Transport, sync, VT emulation, app host, hooks, files, SSH endpoint |
| `lattice` CLI | Worker, desktop | `lattice pair`, `lattice session …`, `lo <file>`, app/layout dev tools |
| Deck | macOS (v1), iOS (v2), web (later) | Rust core (transport, sync, layouts) + NSH UI. 100% Rust |
| Relay | Public (ours) or self-hosted | UDP forwarder: registration, first path, hole-punch coordination, fallback. Sees ciphertext only |
| Ciphertext store | Later, optional | Mailbox/backup for device sync while peers are offline |

```
deck ══QUIC/E2E (relay → direct)══▶ latticed ──unix socket──▶ latticed-proc
   ▲                                     (worker)
   └══ deck ⇄ deck: user Doc sync (P2P, E2E) ══▶ other decks
```

Details: [architecture/04-worker.md](../architecture/04-worker.md), [05-deck.md](../architecture/05-deck.md), [08-infrastructure.md](../architecture/08-infrastructure.md).

## Glossary

| Term | Definition |
|---|---|
| Node | Any worker or deck. Identified by an ed25519 key (NodeId) |
| Device list | Signed hash chain of the user's nodes. The only authority ("account") |
| Deck | The user's control center: a node that shows and steers workers (macOS, iOS). Not "terminal": that word means a PTY view |
| Worker | A machine running `latticed` + `latticed-proc`, in the device list. Named (`build-01`) |
| Stream | Ordered, signed log of changes plus snapshots. Kinds: Log, State, Series, Doc |
| Action | A request from a reader to a stream owner. Effects return as changes |
| App | Worker part + view part (WASM) that gives a process type its UI. Fallback app: `terminal` |
| Layout | Data tree that places app widgets on screen |
| Link | The P2P connection deck ⇄ worker. Type `direct` or `relay`, with RTT |
| Session | Named, persistent workspace on one worker (`api-server`). Holds tabs, port forwards, agents |
| Tab | Ordered child of a session. Holds a split tree of panes |
| Pane | One leaf in a tab: shell, agent, browser, or file viewer. Id `tab.pane` (`1.2`) |
| Agent | A coding-agent process (claude-code, codex, …) running in a pane PTY |
| Hook event | Structured event emitted by an agent (PreToolUse, PostToolUse, TodoWrite, Notification…) |
| Approval | An agent request that waits for the user: Allow / Always / Deny |
| View | How an agent pane renders: TTY, Visual, Zoom, Mirrored |
| Port forward | Mapping `worker:remote → deck localhost:local`, owned by a session |
| Extension | User-facing name for a `files`-kind app that opens file types or links |
| Pairing code | 6-character one-time code from `lattice pair`, 5-minute TTL |

## Spec index

| # | Spec |
|---|---|
| 01 | [Pairing and identity](01-pairing.md) |
| 02 | [Connectivity and sync](02-connectivity.md) |
| 03 | [Sessions, tabs, panes](03-sessions.md) |
| 04 | [Port forwarding](04-ports.md) |
| 05 | [Agents](05-agents.md) |
| 06 | [Files and extensions](06-files-extensions.md) |
| 07 | [Media input](07-media-input.md) |
| 08 | [IDE access](08-ide-access.md) |
| 09 | [Keyboard](09-keyboard.md) |
