# 00 — Overview

Status: draft
Owns: product scope, glossary, system components.

## Summary

Lattice is a peer-to-peer terminal. A user pairs **workers** (their machines) and **decks** (control centers: macOS and iOS; web later) into one signed device list.
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
| `latticed` | Worker | Upgradable connectivity daemon and apps runner. Transport, sync, VT emulation, app instances, hooks, files, SSH endpoint. It runs the worker side of pairing. From M2.9 it also serves the control socket `worker/latticed.sock` (specs/11) |
| `lattice` CLI (M1, current) | Worker, desktop | `lattice add-deck` (runs `latticed add-deck`), `lattice invite` (deck without a GUI), `lattice attach`, `lattice probe`. Dropped (D53), effective M2.9 |
| `lattice` CLI (from M2.9) | Worker | `lattice add-deck` (specs/01, CLI) and `lattice config` (specs/10). Talks to the running `latticed`. No deck commands: the deck is GUI-only (D53). TBD: where `lo <file>` and the app/layout dev tools (06-apps.md) live |
| Deck | macOS and iOS (v1), web (later) | Rust core (transport, sync, layouts) + NSH UI. 100% Rust |
| Relay | Public (ours) or self-hosted | UDP forwarder: registration, first path, hole-punch coordination, fallback. Each node picks its home relay. Sees ciphertext only |
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
| App | One Rust crate that builds into a runner (worker) and a viewer (deck). Default app of a new pane: `terminal` |
| Package | Built app: manifest, runner, viewer. Identified by its blake3 hash. Immutable |
| App instance | One running use of an app on one worker, created by a deck. Id `iid`. Isolated from other instances |
| Runner | Worker-side part of an app. Takes actions, drives its processes, reduces their output into state |
| Viewer | Deck-side part of an app. Renders the instance state as a view tree and sends actions |
| Params | Typed input to create an instance (e.g. `cwd`). Validated against the manifest |
| Delta | Encoded changes of one committed reducer call. Decks apply deltas in version order |
| Boot epoch | First version of every `State` stream a worker serves in one `latticed` run: `unix_ms << 20` at its start. Versions after a restart exceed all earlier ones |
| Journal | Per-instance history in `latticed-proc`: meta plus process output and exits. The only source for restore |
| Clean point | Journal offset where older output no longer affects state. The runner trims there |
| Layout | Data tree that places app widgets on screen |
| Link | The P2P connection deck ⇄ worker. Type `direct` or `relay`, with RTT |
| Session | Named, persistent workspace on one worker (`api-server`). Holds tabs, port forwards, agents |
| Tab | Ordered child of a session. Holds a split tree of panes |
| Pane | One leaf in a tab. Shows the viewer of one app instance: shell, agent, browser, or file viewer. Id `tab.pane` (`1.2`) |
| Agent | A `coding_agent` app instance (claude-code, codex, …), opened from the deck UI. Its process runs in a PTY |
| Hook event | Structured event emitted by an agent (PreToolUse, PostToolUse, TodoWrite, Notification…) |
| Approval | An agent request that waits for the user: Allow / Always / Deny |
| View | How an agent pane renders: TTY, Visual, Zoom, Mirrored |
| Port forward | Mapping `worker:remote → deck localhost:local`, owned by a session |
| Extension | User-facing name for a `files`-kind app that opens file types or links |
| Invite | One-time pairing offer from the deck GUI: deck NodeId, deck contact, secret. The deck expires it after 5 minutes. Given to `lattice add-deck` on stdin (D47, D53) |
| Attach | A CLI client (`lattice attach`) bound to one `terminal` instance in a local terminal. Dropped (D53), effective M2.9 |
| Allowlist | The decks a worker serves (`worker/decks`). M1 stand-in for the device list |
| Home relay | The relay a node registers at and keeps alive, set as `udp://host:port`, or `none` for direct only (D49) |
| Relay id | The relay's ed25519 public key. Signed into `REGISTER`; pinned on first contact (D50) |
| Public address | A node's contact once registered at its home relay: relay `host:port` and relay id (`Contact::Relay`). The node id is held beside it, never inside. Shown as `relay.example.com:7400 · 7f3a…c91e` |
| Contact | How peers reach a node: `Relay{host, port, relay_id}` (its public address) or `Direct(ip:port)` when its home relay is `none`. A peer record is `(NodeId, Contact)` (specs/01 › Contact) |
| Settings registry | The set of setting schemas: platform keys `lattice.*` plus each app's `app.toml` `[settings]`. Only registered keys apply (D51) |
| Scope | Where a setting value lives: `device` (This deck), `user` (synced Doc), `default` (schema), or `worker` (the worker's `config.toml`) |

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
| 10 | [Settings](10-settings.md) |
| 11 | [Worker control socket](11-control-socket.md) |
