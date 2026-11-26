# 01 — Pairing and identity

Status: draft
Owns: first-device setup, pairing workers and decks, removing nodes (user-facing flow), the invite code, the allowlist and worker list files.
Crypto and data model: [architecture/03-identity-security.md](../architecture/03-identity-security.md).

## Summary

There is no server account. The user's identity is a signed device list held by every node (M2.6).
A deck adds a worker with an **invite**: the deck shows a command, the user runs it on the worker, and the worker calls the deck back.
Both sides end up holding the other's public key, each learned over a channel the other cannot fake (D47).

## Behavior

### Invite flow (D47)

1. `Add worker` on a deck (the `+` at the end of the worker rail) creates an invite. The invite holds the deck NodeId, the deck `ip:port`, a 32-byte one-time secret from the OS RNG, and an expiry 5 minutes ahead.
2. The deck shows the command `lattice add-deck <invite code>` with `Copy`, and the status `waiting for worker…`. One invite is open per deck at a time; a new invite replaces the old one.
3. The user runs the command on the worker. The worker checks the expiry first. An expired invite fails with `pair.expired` and dials nothing.
4. The worker dials the deck `ip:port` and pins the deck NodeId from the invite. Any other deck key aborts the dial (`transport.identity_mismatch`).
5. Inside that connection the worker sends `PairRequest{secret, worker name, worker ip:port}`.
6. The deck takes the worker NodeId from the TLS handshake, never from a message.
7. A valid, unexpired, unused secret makes the deck do three things: save the worker (NodeId, `ip:port`, name) to its worker list, mark the invite used, and reply `PairOk{deck name}`.
8. Any other secret gets `PairDenied{code}` (`pair.bad_secret`, `pair.expired`, `pair.used`), and the deck closes the connection. The invite stays open after `pair.bad_secret`.
9. On `PairOk` the worker adds the deck NodeId and name to its allowlist, then finishes the stream. The deck waits for that finish, then dials the worker at the saved `ip:port`.
10. The whole exchange times out after `PAIR_TIMEOUT` = 10 s (proposed). Nothing is saved on either side after a timeout.
11. The deck status line follows the invite: `waiting for worker…`, `pairing…`, `paired · <worker name>`, `expired`, or the error code.

Why it is safe without a PAKE: the deck key reaches the worker through the user's own copy and paste. The worker key reaches the deck inside a TLS session pinned to that deck key, next to a secret only the invite holds. A network attacker sees neither the secret nor a key it can swap.

### Access

12. `latticed` accepts a connection only from a NodeId in its allowlist (03-identity-security.md, Access control rule 1).
13. `latticed` rereads the allowlist file when an unknown NodeId connects, so a deck added by `lattice add-deck` connects without a restart.
14. The allowlist and the worker list persist across restarts and reboots (data dir below).
15. M1 has no `lattice unpair`. Remove a deck by deleting its line from the allowlist file. The device list replaces both files in M2.6.

### Later (M2.6 and beyond)

16. First launch of the first deck creates the device list and the content key. TBD: offer a recovery key at this step?
17. Worker install: `curl -fsSL [INSTALL_URL] | sh` installs `latticed` and `latticed-proc` once. TBD: install URL and supported OS/arch list.
18. On success the inviter appends `AddNode` to the device list and seals the content key to the new node. The list propagates P2P to all nodes.
19. Adding another deck: a trusted deck shows the invite, the new deck runs it. TBD: the command or screen on the new deck.
20. Each worker reports: name, OS, arch, hardware summary (`linux · x86_64 · 16 cores`, `linux · cuda 12.4 · 2×A100`).
21. Removing a node from any trusted deck appends `RemoveNode` and rotates the content key. All grants of that node end (including SSH access, see 08).

## Data

| Field | Example |
|---|---|
| node.id | ed25519 public key; text form: 64 lowercase hex digits (D48) |
| worker.name | `build-01` (default: the host name) |
| worker.monogram | `B1`: first and last character of the name, uppercased (proposed) |
| worker.os | `linux · x86_64` |
| worker.state | online · relay · offline |

### Invite code

`lattice1:` followed by RFC 4648 base32 of the postcard-encoded invite, lower case, no padding. Decoding ignores case.

| Field | Type |
|---|---|
| deck | NodeId, 32 bytes |
| addr | deck `ip:port` |
| secret | 32 bytes, one use |
| expires | Unix seconds |

The secret is a secret: it never goes into logs (`.rules/engineering.md`, Security).

### Files

Data dir: macOS `~/Library/Application Support/lattice`; Linux `$XDG_DATA_HOME/lattice`, else `~/.local/share/lattice`. `LATTICE_DATA_DIR` overrides it.

| File | Owner | Content |
|---|---|---|
| `worker/node.key` | `latticed` | 32 secret bytes, mode 0600 |
| `worker/decks` | `latticed`, `lattice add-deck` | allowlist: one deck per line, `<node id> <name>` |
| `worker/packages/` | `latticed` | cached app packages |
| `deck/node.key` | deck, `lattice` | 32 secret bytes, mode 0600 |
| `deck/workers` | deck, `lattice` | worker list: one worker per line, `<node id> <ip:port> <name>` |

Lines starting with `#` and blank lines are ignored. The name is the rest of the line.
Runtime files stay in `$TMPDIR/lattice` (`proc.sock`, `worker.addr`).

### Addresses (M1, direct only)

- `latticed` listens on `0.0.0.0:7441` by default.
- A deck listens on `0.0.0.0` with a port the OS picks.
- An advertised `ip` is the address of the interface that routes to the peer: the default route for the deck, the route to the deck for the worker. `LATTICE_ADVERTISE_IP` overrides it.

## UI

Design system: patterns.md › Add worker; components.md › Invite command.
M1 deck: one active worker, the most recently paired one. TBD: switch the active worker from the rail.
TBD: the canvas and the simulator still show the worker-shows-code flow (`lattice pair`, Scan QR / Enter code). Update both to the invite flow (D47).
TBD: iOS adds a worker the same way (copy the command, run it on the worker); the Mobile "Pair worker" artboard changes with it.

## Open questions

- TBD: worker rename flow.
- TBD: rate limit on `PairRequest` attempts per invite.
- TBD: recovery when every deck is lost.
