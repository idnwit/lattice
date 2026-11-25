# Architecture — Lattice platform

Status: draft (proposed, under discussion)
Owns: system layering, component boundaries, cross-cutting principles.

## Summary

Lattice has two kinds of nodes: **workers** (run processes) and **decks** (the user's control centers — show and steer workers). v1 decks: macOS and iOS. Web later.
There is no trusted server. Nodes talk end-to-end encrypted over a relay-first P2P transport.
Everything above the transport is a **sync** of signed, append-only streams plus snapshots.
Product features are **apps** on top of a generic platform.

## Principles

1. **E2E always.** Relays and storage peers see only ciphertext and signatures.
2. **Fast first.** UDP/QUIC paths. Relay first so connect is instant; upgrade to direct in the background.
3. **Generic platform, specific apps.** The platform knows processes, streams, and views. It does not know what "claude-code" is.
4. **Never kill user processes.** The process daemon is tiny and frozen. Everything else can restart.
5. **Data over code at the edges.** Layouts and UI trees cross the wire as data. Code runs sandboxed (WASM) where policy allows.
6. **Modular layers.** Each layer exposes the smallest useful API. Higher layers never reach past the layer below.

## Layers

```
┌──────────────────────────────────────────────────────────────────┐
│ L4 Apps        app = worker part (wasm) + view part (wasm)        │
│                layouts (data) · built-in apps · user/agent apps   │
├──────────────────────────────────────────────────────────────────┤
│ L3 Platform    worker: process host · app host                    │
│                deck: view host · layout engine · app host       │
├──────────────────────────────────────────────────────────────────┤
│ L2 Sync        streams: Log · State · Series · Doc  + blobs       │
│                change / snapshot / action · E2E content keys      │
├──────────────────────────────────────────────────────────────────┤
│ L1 Transport   p2p: UDP + relay, relay-first → hole punch          │
│                stream: QUIC (streams + datagrams), id = ed25519    │
└──────────────────────────────────────────────────────────────────┘
```

## Nodes and processes

```
 worker                                         deck (macOS | iOS)          
┌───────────────────────────┐                 ┌──────────────────────────────┐
│ latticed-proc  (frozen)   │                 │ deck core (Rust)           │
│  PTYs, children, ring buf │                 │  transport · sync · app host │
│        ▲ unix socket      │                 │  layout engine               │
│ latticed  (upgradable)    │◀══ QUIC/E2E ══▶ │ view host (native UI)        │
│  VT emu · sync · apps     │   relay|direct  │  renders view trees          │
│  transport · hooks        │                 └──────────────────────────────┘
└───────────────────────────┘          ▲
              ▲                        │  deck ⇄ deck: Doc sync (config, layouts)
              └──── relay + discovery (untrusted) ────┘
```

## Documents

| # | Doc | Owns |
|---|---|---|
| 01 | [transport.md](01-transport.md) | L1: own UDP relay + hole punch, QUIC (quinn, magic socket), channels |
| 02 | [sync.md](02-sync.md) | L2: stream model, messages, compaction |
| 03 | [identity-security.md](03-identity-security.md) | Keys, device list, content keys, ACL, pairing crypto |
| 04 | [worker.md](04-worker.md) | Process daemon, connectivity daemon, IPC, hooks |
| 05 | [deck.md](05-deck.md) | Deck core, NSH UI, platforms |
| 06 | [apps.md](06-apps.md) | App model, manifest, kinds, permissions, runtimes |
| 07 | [layouts.md](07-layouts.md) | Layout data model and generation |
| 08 | [infrastructure.md](08-infrastructure.md) | Relay, discovery, optional ciphertext store |
| 09 | [platform-l1.md](09-platform-l1.md) | Layer 1 crates, skeleton status, contract tests, open TBDs |
| — | [decisions.md](decisions.md) | Decision log and open questions |

## Naming

The user-facing word "terminal" already means a PTY view. The user's control-center devices are called **decks** in all docs (command deck). Alternatives considered: helm, cockpit, station.
