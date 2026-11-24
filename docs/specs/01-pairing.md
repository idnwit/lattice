# 01 — Pairing and identity

Status: draft
Owns: first-device setup, pairing workers and decks, removing nodes (user-facing flow).
Crypto and data model: [architecture/03-identity-security.md](../architecture/03-identity-security.md).

## Summary

There is no server account. The user's identity is a signed device list held by every node.
The first deck creates it. Every new worker or deck joins by pairing with a node already in the list, using a code or QR.

## Behavior

1. First launch of the first deck creates the device list and the content key. TBD: offer a recovery key at this step?
2. Worker install: `curl -fsSL [INSTALL_URL] | sh` installs `latticed` and `latticed-proc` once. TBD: install URL and supported OS/arch list.
3. `lattice pair` on the worker prints a QR and a 6-character code (`K7Q–4XM`). The code encodes the worker NodeId, its relay URL, and a one-time secret.
4. On a deck, `Add worker` offers `Scan QR` or `Enter code`. Deck shows `waiting for worker…` while both sides run the PAKE.
5. Adding another deck works the same way in reverse: a trusted deck shows the code, the new deck scans it.
6. Code TTL is 5 minutes. Codes are single-use.
7. On success the inviter appends `AddNode` to the device list and seals the content key to the new node. The list propagates P2P to all nodes.
8. Each worker reports: name, OS, arch, hardware summary (`linux · x86_64 · 16 cores`, `linux · cuda 12.4 · 2×A100`).
9. Removing a node from any trusted deck appends `RemoveNode` and rotates the content key. All grants of that node end (including SSH access, see 08).

## Data

| Field | Example |
|---|---|
| node.id | ed25519 public key |
| worker.name | `build-01` |
| worker.monogram | `B1` (2 chars, rail) |
| worker.os | `linux · x86_64` |
| worker.state | online · relay · offline |

## UI

Design system: patterns.md › Mobile › Pair worker; components.md › Pairing code.
TBD: the canvas shows "sign in" and "/ personal". Update the design to the account-less model, or keep an optional account (decisions.md Q2).

## Open questions

- TBD: worker rename flow.
- TBD: code alphabet (exclude ambiguous chars like `0/O`, `1/I`?).
- TBD: rate limit on code attempts.
- TBD: recovery when every deck is lost.
