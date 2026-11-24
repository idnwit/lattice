# 03 — Identity and security

Status: draft
Owns: node keys, device list, content keys, access control, pairing crypto.

## Summary

Every node (worker or deck) is an ed25519 key. The key is also its transport address (NodeId).
A user is a **signed device list**. Membership in that list is the only authority.

## Keys

| Key | Held by | Purpose |
|---|---|---|
| Node key (ed25519) | each worker and deck | Transport identity, signing changes |
| Node agreement key (X25519) | each node | Receive sealed content keys (HPKE) |
| User content key (XChaCha20-Poly1305), per epoch | every member node | Encrypt stored data (snapshots, Doc logs, blobs) |
| Recovery key (optional) | user, offline | Re-root the device list if all devices are lost |

## Device list

- A hash chain of entries: `AddNode{key, kind: worker|deck, name, caps}`, `RemoveNode{key}`, `Rotate{epoch}`.
- Each entry is signed by a node already in the list (or the recovery key).
- Every node holds the chain. `Hello` carries the chain head so peers detect divergence.
- Pattern: Keybase sigchain / Matrix cross-signing. Not MLS in v1.

## Content key epochs

1. On create, the first device generates the content key and seals it to itself.
2. `AddNode` seals the current key to the new node's X25519 key (HPKE).
3. `RemoveNode` triggers `Rotate`: new key, sealed to remaining nodes. Old ciphertext stays readable by the removed node; accepted for v1.

## Access control

1. A worker accepts a connection only if the remote NodeId is in the device list.
2. Per-node caps (TBD: which?) can restrict a deck, e.g. view-only.
3. SSH access (feature spec 08) is a separate cap granted per deck node.
4. Apps get permissions from their manifest, enforced by the host (apps.md). Node trust does not imply app trust.

## Pairing

1. A trusted device shows a QR or 6-char code. Or the new worker runs `lattice pair` and shows it.
2. The code encodes: inviter NodeId, relay URL, one-time secret.
3. The new node dials the inviter by NodeId. Both run a PAKE (TBD: SPAKE2 or CPace) on the secret.
4. On success the inviter appends `AddNode` and seals the content key to the new node.
5. Codes are single-use and expire in 5 minutes.

No account server is needed for any of this. TBD: do we also want an optional account (email/OAuth) for discovery and billing? See decisions.md.

## Threat notes

- Relay or storage compromise: sees ciphertext, sizes, timing, NodeIds. Cannot read or forge.
- Stolen device: has the content key. Remove it from another device → rotate.
- Malicious app: limited by WASM sandbox and permission manifest.
- Secrets never go into logs (`.rules/engineering.md`).
