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
2. The deck shows the command `lattice add-deck <invite code>` with `Copy`, and the status `waiting for worker…`. One invite is open per deck data dir at a time. In one process, a new invite replaces the old one. A second process on the same data dir fails with `pair.invite_busy` (CLI rule 6).
3. The user runs the command on the worker. The worker checks the expiry first. An expired invite fails with `pair.expired` and dials nothing.
4. The worker dials the deck `ip:port` and pins the deck NodeId from the invite. Any other deck key aborts the dial (`transport.identity_mismatch`).
5. Inside that connection the worker sends `PairRequest{secret, worker name, worker ip:port}`.
6. The deck takes the worker NodeId from the TLS handshake, never from a message.
7. A valid, unexpired, unused secret makes the deck mark the invite used and reply `PairOk{deck name}`. The deck saves nothing yet.
8. Any other secret gets `PairDenied{code}` (`pair.bad_secret`, `pair.expired`, `pair.used`). The deck waits for the worker to end its stream, then closes the connection as refused. The invite stays open after `pair.bad_secret`.
9. On `PairOk` the worker adds the deck NodeId and name to its allowlist and syncs the file to disk. Then it sends `PairDone` and waits.
   - On `PairDone` the deck saves the worker (NodeId, `ip:port`, name) to its worker list, then finishes its stream.
   - The worker reports paired only when it reads that finish. Then it finishes its own stream and shuts its transport down: a clean close, sent before the process exits.
   - The deck waits up to `PAIR_TIMEOUT` for that finish or that close, so its own finish reaches the worker. It then reports paired and dials the worker at the saved `ip:port`.
   - A worker that exits without the shutdown leaves its finish unsent. The deck then waits the whole `PAIR_TIMEOUT`, though it paired.
   - The deck saves the worker only after `PairDone`. Any other end of the stream, a stream reset, or a closed connection saves nothing.
10. Each side bounds the exchange of one connection by `PAIR_TIMEOUT` = 10 s (proposed).
   - The deck closes a connection that does not finish in time as refused, then serves the next one. A silent peer therefore cannot hold the invite.
   - An exchange that fails after `PairOk` used the invite up. The deck reports the failure code: `pair.timeout`, `pair.unconfirmed` (no `PairDone`), or the worker list write error.
   - The worker fails with `pair.timeout` when the deck does not finish in time. It fails with `pair.unconfirmed` when the deck closes or resets instead.
   - Neither side ends a failed exchange with a clean finish: it closes the connection as refused. The worker then shuts its transport down, so the refusal is sent too.
   - A worker that fails after its allowlist write keeps the line. It does not roll back. The line is harmless: the deck does not list that worker, so it never dials it. The line admits only the deck the user chose. Pair again with a new invite, or delete the line (rule 15).
11. The deck status line follows the invite: `waiting for worker…`, `pairing…`, `paired · <worker name>`, `expired`, or the error code.

Why it is safe without a PAKE: the deck key reaches the worker through the user's own copy and paste. The worker key reaches the deck inside a TLS session pinned to that deck key, next to a secret only the invite holds. A network attacker sees neither the secret nor a key it can swap.

### Access

12. `latticed` accepts a connection only from a NodeId in its allowlist (03-identity-security.md, Access control rule 1). It closes any other connection at once as refused. The deck fails with `transport.refused` and does not wait for a timeout.
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
| worker.name, deck name from a peer | Control characters become spaces. Unicode format characters (category Cf, such as U+202E) are dropped. Trimmed, then cut to 64 characters (proposed) |
| worker.monogram | `B1`: first and last character of the name, uppercased (proposed) |
| worker.os | `linux · x86_64` |
| worker.state | online · relay · offline |

### Invite code

`lattice1:` followed by RFC 4648 base32 of the postcard-encoded invite, lower case, no padding. Decoding ignores case. It accepts only canonical base32: a length that ends on a whole byte and zero spare bits.

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
| `worker/decks.lock` | `latticed add-deck` | empty; locked while the allowlist is rewritten |
| `worker/packages/` | `latticed` | cached app packages |
| `deck/node.key` | deck, `lattice` | 32 secret bytes, mode 0600 |
| `deck/workers` | deck, `lattice` | worker list: one worker per line, `<node id> <ip:port> <name>` |
| `deck/invite.lock` | deck, `lattice invite` | empty; locked while an invite is open (rule 2) |
| `worker.addr` (runtime dir) | `latticed` | `<node id>@<ip:port>`, mode 0600. Written only once `latticed` serves (04-worker.md, `latticed`) |

Lines starting with `#` and blank lines are ignored. The name is the rest of the line.
Writers write a temporary file in the same dir, sync it to disk, then rename it over the old file.
Writers of `worker/decks` hold a lock on `worker/decks.lock` from read to rename. Writers of `deck/workers` hold `deck/invite.lock`.
Runtime dir, one per user: `$XDG_RUNTIME_DIR/lattice` when `XDG_RUNTIME_DIR` is set, else `$TMPDIR/lattice-<uid>`, else `/tmp/lattice-<uid>`.

- It holds `proc.sock` and `worker.addr`. `LATTICE_PROC_SOCK` overrides the socket path only.
- Both daemons and the CLI resolve it the same way. Start them with the same `XDG_RUNTIME_DIR` and `TMPDIR`.
- The first process creates it with mode 0700. `latticed` refuses a dir that is a symlink, has another owner, or that others may open.
- `<uid>` keeps users apart where `$TMPDIR` is shared, as `/tmp` is on Linux.

### Addresses (M1, direct only)

- `latticed` listens on `0.0.0.0:7441` by default.
- A deck listens on `0.0.0.0` with a port the OS picks.
- An advertised `ip` is the address of the interface that routes to the peer: the default route for the deck, the route to the deck for the worker. `LATTICE_ADVERTISE_IP` overrides it.

## CLI

The CLI pairs without the deck GUI. It uses the same data dir and files.

1. `lattice invite [--name <deck name>]` makes an invite for a deck without a GUI. It uses `<data dir>/deck`. It prints the command, serves rules 3–10, prints `paired · <worker name> at <ip:port> (<node id prefix>)`, shuts its transport down, and exits. The deck name defaults to the host name.
2. `lattice add-deck <invite code> [--name <worker name>]` runs on the worker. It runs `latticed add-deck` (next to `lattice`, else on `PATH`). It passes the invite code on stdin, never as an argument.
3. `latticed add-deck [<invite code>] [--name <worker name>]` is the worker side of rules 3–10. Without the argument it reads the code from the first line of stdin. Any local user sees an argument in `ps`, so prefer stdin.
4. `latticed add-deck` needs a running `latticed` on the same machine: it reads the listen port from `worker.addr`. The worker name defaults to the host name.
5. On failure each command prints the error code and exits non-zero. Codes: `pair.bad_secret`, `pair.expired`, `pair.used`, `pair.timeout`, `pair.unconfirmed`, `pair.invite_busy`, `pair.invite_malformed`.
6. `lattice invite` and the deck GUI hold `deck/invite.lock` while their invite is open. The second one on the same data dir fails with `pair.invite_busy`.

## UI

Design system: patterns.md › Add worker; components.md › Invite command.
M1 deck: one active worker, the most recently paired one. TBD: switch the active worker from the rail.

Add worker dialog (desktop deck):

1. The rail shows one entry per paired worker: its monogram and its link state glyph (02-connectivity.md, Link states).
2. `+` opens the dialog. It makes an invite unless one is still waiting; then it shows that one again.
3. Close and `Escape` hide the dialog. The invite stays open until it pairs or expires.
4. The status is `pairing…` while a worker's callback is served, and back to `waiting for worker…` if that callback does not pair.
5. The deck stops serving the invite at its expiry (`INVITE_TTL_SECS`) and shows `expired`.
6. `New invite` shows once the invite expired or failed. It replaces the invite.
7. On `paired · <name>` the deck links to that worker: it is the most recently paired one.
8. With no paired worker, the workspace shows "No worker yet." and `Add worker`, which opens the dialog.

Canvas: "Desktop — Add worker · invite flow · no worker · link states". The simulator runs the same flow.
TBD: iOS adds a worker the same way. The canvas shows a proposed sheet ("Mobile — Add worker"); confirm it (patterns.md › Mobile)?

## Open questions

- TBD: worker rename flow.
- TBD: rate limit on `PairRequest` attempts per invite.
- TBD: recovery when every deck is lost.
