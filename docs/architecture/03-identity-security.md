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

Flow and files: [specs/01-pairing.md](../specs/01-pairing.md) (D47).

1. The deck GUI makes an invite: its NodeId, its contact, and a 32-byte one-time secret. The code holds no expiry; the deck stops accepting it after 5 minutes.
2. The user pipes the code into `lattice add-deck` on the new worker; the running `latticed` pairs (D53). The worker dials the deck through the deck's relay, or at `ip:port` with relay `none`, and pins the deck NodeId.
3. In that TLS session the worker sends the secret. The deck reads the worker NodeId from the handshake.
4. Both sides store the other's key: the deck in its worker list before it sends `PairOk`, the worker in its allowlist (M1) or the device list (M2.6) on `PairOk`. A lost `PairOk` leaves a deck entry the worker refuses: availability only, no access (specs/01-pairing.md, Invite flow).
5. Invites are single-use. The deck alone enforces the 5-minute expiry, so worker clock skew never matters.

With a relay, the invite carries the deck's public address instead of `ip:port` (specs/01-pairing.md › Contact; specs/02-connectivity.md rule 17). The relay sees the pairing as ciphertext only.

TBD (M2.6): does the device list still need a PAKE when the secret travels only inside a pinned session?

No account server is needed for any of this. TBD: do we also want an optional account (email/OAuth) for discovery and billing? See decisions.md.

## Threat notes

- Relay or storage compromise: sees ciphertext, sizes, timing, NodeIds. Cannot read or forge.
- Stolen device: has the content key. Remove it from another device → rotate.
- Malicious app: limited by WASM sandbox and permission manifest.
- Secrets never go into logs (`.rules/engineering.md`).
- Relay id pin (D50) is trust on first use. The id in `COOKIE` is not signed by the relay key, and no relay message proves the relay holds that key. An on-path attacker at first contact can make a node pin a wrong id. The cost is denial of service and relay metadata (the attacker sees what a relay sees), never content: the real relay drops every `REGISTER` signed over the wrong id. After the pin, a `COOKIE` with another id is treated as spoofed and ignored; only 3 (proposed) consecutive mismatched attempts with no `REG_OK` report `relay.id_changed` (01-transport.md, Relay rule 28). So one injected `COOKIE` cannot stop a registration. Node keys pin peers end to end, so the relay id never guards content. It binds each `REGISTER` to one relay and makes a relay swap visible (`relay.id_changed`). An explicit `udp://<id>@host:port` avoids first contact. TBD: should `REG_OK` carry a relay signature so a node can verify the id?
- A relay key rotation makes every node that pinned the old id fail with `relay.id_changed` until it re-pins.
