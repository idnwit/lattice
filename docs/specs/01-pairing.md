# 01 — Pairing and identity

Status: draft
Owns: first-device setup, pairing workers and decks, removing nodes (user-facing flow), the invite code, `Contact`, the allowlist and worker list files, the `lattice add-deck` CLI.
Crypto and data model: [architecture/03-identity-security.md](../architecture/03-identity-security.md).

## Summary

There is no server account. The user's identity is a signed device list held by every node (M2.6).
A deck adds a worker with an **invite**. The deck shows a code, and the user gives it to `lattice add-deck` on the worker. The worker's `latticed` then calls the deck back.
Only the process that holds a node key pairs: the deck GUI and the running `latticed` (D53). One round trip: `PairRequest` → `PairOk`.
Both sides end up holding the other's public key, each learned over a channel the other cannot fake (D47).

## Behavior

### Invite flow (D47, D53)

Planned (M2.9). M1 code still runs the D47 flow with `PairDone` and `latticed add-deck` (ROADMAP › M1.6 pairing).

1. `Add worker` on a deck (the `+` at the end of the worker rail) creates an invite. It holds the deck NodeId, the deck contact, and a 32-byte one-time secret from the OS RNG. Only the deck GUI makes invites. It holds one invite, in memory.
2. The invite moves through the states in Data › Invite states. The deck shows the code only in `Waiting`. It enters `Waiting` once it has a contact: registered at its home relay, or relay `none` (02-connectivity.md, rule 18). Only a relay id error (`relay.id_changed`, `relay.no_id`) ends the invite with its code. A relay timeout keeps it in `Registering`, retrying, until `INVITE_TTL`.
3. `+` while `Waiting` shows the same invite. `New invite` replaces it; the old secret stops working at once. The deck enforces `INVITE_TTL` = 300 s; then the invite ends `expired`. The code carries no expiry.
4. While `Waiting`, the deck serves a pairing connection from any NodeId, known or not. So a paired worker can pair again, for example with a new contact. Outside `Waiting`, it serves no pairing and closes connections from unknown NodeIds at once as refused.
   - Each pairing connection must deliver `PairRequest` within `PAIR_TIMEOUT`, else the deck refuses it.
   - The deck serves at most 8 pairing connections at once (proposed). It refuses extra ones at once.
   - The secret check is atomic and compares in constant time: the first valid request wins.
5. The user runs `lattice add-deck` on the worker and gives it the code on stdin (CLI rules 1–3). The running `latticed` does the rest, as one task with one deadline, `PAIR_TIMEOUT` = 15 s (proposed). The deadline covers its own registration, the deck-relay registration, the handshake, and the exchange.
6. `latticed` sends `PairRequest` only once it has its own contact: registered at its home relay, or relay `none`. Still unregistered at the deadline, it fails with the registration's code (`relay.timeout`, `relay.id_changed`, …) and dials nothing.
7. `latticed` dials the deck at the contact from the invite and pins the deck NodeId. `Relay`: it registers at the deck's relay with the relay id from the invite, and dials through it (architecture/01-transport.md › Connect sequence). `Direct`: it dials the `ip:port`. Any other deck key aborts the dial (`transport.identity_mismatch`).
8. The worker's first and only frame is `PairRequest{secret, name, contact}`. The deck takes the worker NodeId from the TLS handshake, never from a message.
9. A valid, unused secret while `Waiting`: the deck saves (worker NodeId, contact, name) to its worker list, replacing a line with the same NodeId. Then it marks the invite `Paired`, sends `PairOk{deck name}`, and finishes its stream. It never closes a successful pairing connection: a close could drop the unsent `PairOk`. It waits `RECONNECT_FIRST` (500 ms), then dials the worker as a normal link (02-connectivity.md, rules 7–10).
10. Any other request: the deck closes the connection as refused. A wrong secret leaves the invite `Waiting`. On a refusal the worker fails with `pair.denied` and writes nothing. No `PairOk` by the deadline: `pair.timeout`.
11. On `PairOk` the worker adds the deck NodeId and name to its allowlist (rule 13), then closes the connection with the generic close code. It answers the CLI `Paired`.

Commit order and its cost: the deck saves before `PairOk`, the worker on `PairOk`. A lost `PairOk`, or a worker that dies between the two saves, leaves the deck listing a worker that refuses it (`transport.refused`). No access is granted to anyone: availability only. Fix: pair again; the deck replaces the line by NodeId.

Why it is safe without a PAKE: the deck key reaches the worker through the user's own copy and paste. The worker key reaches the deck inside a TLS session pinned to that deck key, next to a secret only the invite holds. A network attacker sees neither the secret nor a key it can swap.

### Access

12. `latticed` accepts a connection only from a NodeId in its allowlist (03-identity-security.md, Access control rule 1). It closes any other connection at once as refused. The deck fails with `transport.refused` and does not wait for a timeout.
13. Planned (M2.9): the file `worker/decks` is the source of truth; `latticed` keeps only a cache of it.
   - Before every allowlist write, `latticed` rereads the file, records its blake3 hash, and merges its change into it. It never writes from its cached copy, so a line deleted by hand is never written back.
   - Before it renames the merged file over `worker/decks`, it compares the file's hash with the recorded one. On a change it rereads and merges again, so a concurrent hand edit is not lost.
   - `latticed` watches the `worker/` directory for changes to `decks`, so editors that save by rename are seen. As with `config.toml` (10-settings.md, rule 35), on a change it reloads the file, drops the NodeIds that are gone, and closes their open connections at once as refused.
   - It also rereads the file when an unknown NodeId connects, at most once per second (proposed). So an added line applies without a restart.
14. The allowlist and the worker list persist across restarts and reboots (data dir below).
15. M1 has no `lattice unpair`. Remove a deck by deleting its line from the allowlist file. From M2.9 the deck is revoked within 2 s (proposed) of the change: new connections are refused and open ones close (rule 13). The device list replaces both files in M2.6.

### Later (M2.6 and beyond)

16. First launch of the first deck creates the device list and the content key. TBD: offer a recovery key at this step?
17. Worker install: `curl -fsSL [INSTALL_URL] | sh` installs `latticed` and `latticed-proc` once. It asks for the worker's relay and starts both daemons as user services (10-settings.md › Install). Pairing needs `latticed` running (D53). TBD: install URL and supported OS/arch list.
18. On success the inviter appends `AddNode` to the device list and seals the content key to the new node. The list propagates P2P to all nodes.
19. Adding another deck: a trusted deck shows the invite, the new deck runs it. TBD: the command or screen on the new deck.
20. Each worker reports: name, OS, arch, hardware summary (`linux · x86_64 · 16 cores`, `linux · cuda 12.4 · 2×A100`).
21. Removing a node from any trusted deck appends `RemoveNode` and rotates the content key. All grants of that node end (including SSH access, see 08).

### Address changes (v1)

22. A paired peer's contact never updates in v1. When a worker changes its home relay, or its relay's key rotates, every deck must pair with it again. A deck that changes its relay needs no re-pairing: its workers keep only its NodeId, and the deck dials them.

### Limits

Planned (M2.9). Single instance of `latticed` and the deck: [11-control-socket.md](11-control-socket.md).

25. A pairing stream frame body is at most 4 KiB (proposed). A longer declared length makes the deck refuse the connection.

## Data

| Field | Example |
|---|---|
| node.id | ed25519 public key; text form: 64 lowercase hex digits (D48) |
| worker.name | `build-01` (default: the host name) |
| worker.name, deck name from a peer | Control characters become spaces. Unicode format characters (category Cf, such as U+202E) are dropped. Trimmed, then cut to 64 characters (proposed) |
| worker.monogram | `B1`: first and last character of the name, uppercased (proposed) |
| worker.os | `linux · x86_64` |
| worker.state | online · relay · offline |

### Invite states (deck)

| State | UI | Exit |
|---|---|---|
| `Registering` | `registering…`, no code shown | `REG_OK` or relay `none` → `Waiting`; relay id error → `Ended(<code>)`; `INVITE_TTL` → `Ended(expired)` |
| `Waiting` | code, `Copy`, `waiting for worker…` | valid `PairRequest` → `Paired`; `INVITE_TTL` → `Ended(expired)` |
| `Paired` | `paired · <worker name>` | terminal |
| `Ended` | `expired` or the error code, `New invite` | terminal |

Planned (M2.9).

| Timer | Value | Owner |
|---|---|---|
| `INVITE_TTL` | 300 s | deck |
| `PAIR_TIMEOUT` | 15 s (proposed) | worker task, end to end; deck, per connection |

### Invite code

Planned (M2.9).

`lattice1:` followed by RFC 4648 base32 of the postcard-encoded `Invite`, lower case, no padding. Decoding ignores case. It accepts only canonical base32: a length that ends on a whole byte and zero spare bits. It rejects trailing bytes after the `Invite`.

| # | Field | Type |
|---|---|---|
| 1 | deck | NodeId, 32 bytes |
| 2 | contact | `Contact` (below) |
| 3 | secret | 32 bytes, one use |

The secret is a secret: it never goes into logs, `argv`, or the control socket's logs (`.rules/engineering.md`, Security).
The postcard enum tags of the removed `PairDenied` and `PairDone` messages stay reserved and are never reused.
The layout changed in place: the prefix stays `lattice1:`. An older code fails with `pair.invalid_code`; there is no backward compatibility (the project is early).

### Contact

How peers reach a node (glossary). A peer record is always `(NodeId, Contact)`; the node id is never inside the contact.

```rust
enum Contact {
    Relay { host: String, port: u16, relay_id: [u8; 32] }, // postcard tag 0: the udp relay
    Direct(SocketAddr),                                    // postcard tag 1: relay `none`
}
```

- `Relay` is the **public address** (02-connectivity.md, rule 17). The variant is the scheme; a later scheme is a new variant.
- `host` is canonical. It is either a DNS name or an IP literal.
  - DNS name: LDH labels (letters, digits, hyphen; 1–63 bytes; no leading or trailing hyphen), lower case, no trailing dot, at most 253 bytes.
  - IP literal: IPv4 dotted decimal without leading zeros, or IPv6 in RFC 5952 form.
  - A DNS name whose last label is all digits (`127.1`) is rejected. The decoder rejects any other host: `pair.invalid_code` in an invite, an invalid line in a file. Two contacts are equal when their canonical forms are equal.
- Text form, in files: `udp://<relay id>@<host>:<port>` (relay id as 64 lowercase hex digits), or `<ip>:<port>`. IPv6 literals are bracketed: `udp://<id>@[2001:db8::1]:7400`, `[2001:db8::1]:7441`.
- UI form: `relay.example.com:7400 · 7f3a…c91e` (relay `host:port` · first and last 4 hex digits of the node id), or `<ip>:<port>`.
- v1 has no address update. A worker that changes its home relay, or whose relay key rotates, gets a new contact. Its decks then cannot reach it: pair again with a new invite (rule 22).

### Files

Planned (M2.9) for the rows that differ from M1: `worker/latticed.sock`, `deck/settings.toml`, single writers, and no lock files or `worker.addr`.

Data dir: macOS `~/Library/Application Support/lattice`; Linux `$XDG_DATA_HOME/lattice`, else `~/.local/share/lattice`. `LATTICE_DATA_DIR` overrides it.

| File | Owner | Content |
|---|---|---|
| `worker/node.key` | `latticed` | 32 secret bytes, mode 0600. Any other size is an error (`worker.bad_key`); `latticed` never overwrites it |
| `worker/decks` | `latticed` writes; the user may edit or delete lines | allowlist: one deck per line, `<node id> <name>`. Source of truth (rule 13) |
| `worker/latticed.sock` | `latticed` | control socket ([11-control-socket.md](11-control-socket.md)), mode 0600 |
| `worker/packages/` | `latticed` | cached app packages |
| `worker/config.toml` | `latticed`; `lattice config` only with no daemon running (`install.sh` runs `lattice config set`) | worker config (10-settings.md › Worker config, rule 34) |
| `deck/node.key` | deck | 32 secret bytes, mode 0600 |
| `deck/workers` | deck only | worker list: one worker per line, `<node id> <contact> <name>`, contact in text form |
| `deck/settings.toml` | deck only | `[user]` and `[device]` settings scopes until the `user/meta` Doc exists (10-settings.md, rule 11; proposed) |

Lines starting with `#` and blank lines are ignored. The name is the rest of the line.
Writers write a temporary file in the same dir, sync it to disk, then rename it over the old file. `worker/decks` and `deck/workers` each have one writing process. It serializes its writes, and holds the exclusive `flock` on its directory (`worker/`, `deck/`; 11-control-socket.md, rules 1–2).
Runtime dir, one per user: `$XDG_RUNTIME_DIR/lattice` when `XDG_RUNTIME_DIR` is set, else `$TMPDIR/lattice-<uid>`, else `/tmp/lattice-<uid>`.

- It holds `proc.sock`. `LATTICE_PROC_SOCK` overrides its path.
- Both daemons resolve it the same way. Start them with the same `XDG_RUNTIME_DIR` and `TMPDIR`.
- The first process creates it with mode 0700. `latticed` refuses a dir that is a symlink, has another owner, or that others may open.
- `<uid>` keeps users apart where `$TMPDIR` is shared, as `/tmp` is on Linux.

### Addresses (M1, direct only)

- `latticed` listens on `0.0.0.0:7441` by default.
- A deck listens on `0.0.0.0` with a port the OS picks.
- An advertised `ip` is the address of the interface that routes to the peer: the default route for the deck, the route to the deck for the worker. `LATTICE_ADVERTISE_IP` overrides it.
- Settings: worker `listen.addr`; deck `lattice.connectivity.listen_port` and `lattice.connectivity.advertise_ip` (10-settings.md).

### Network requirements (relay `none`)

- Pairing needs the worker to open a new UDP flow to the deck's advertised `ip:port` (rule 7). It fails behind NAT, or behind a VPN with no return route: `pair.timeout`.
- Verified case: an OpenVPN full tunnel with masquerade. A Linux worker could not dial back a Mac deck, while the deck reached the worker.
- Workarounds: the same LAN, a VPN route without masquerade, or an overlay such as ZeroTier. A relay removes the need: both sides only reach the deck's relay over UDP.
- TBD: when the dial-back fails, should the deck fall back to dialing the worker?

## CLI

Planned (M2.9).

The `lattice` CLI runs on workers only: `lattice add-deck` and `lattice config` (10-settings.md). It never pairs or registers by itself; the running `latticed` does (D53). No deck command exists: the deck is GUI-only.

1. `lattice add-deck [--name <worker name>]` reads the invite code only from stdin, never from an argument (`argv` shows in `ps`). Run alone on a tty, it prompts `Invite code:`. From a pipe it reads the first line: `echo <code> | lattice add-deck`. `echo` is a shell builtin, so the code stays out of `ps`, but the pipe form lands in shell history. The exposure is bounded: the code works once, for at most 300 s, and only the same uid can reach `latticed`'s socket. Use the prompt to keep it out of history. The worker name defaults to `name` in `config.toml`, else the host name.
2. It sends one `AddDeck` on the control socket (11-control-socket.md), prints `pairing with deck <first and last 4 hex digits> via <contact>…`, and waits for the reply.
3. `Paired` prints `paired with deck "<name>" at <contact>` and exits 0. `Failed{code}` prints the code and its hint (table below) and exits 1.
   - `connect()` fails: `worker.not_running`.
   - `connect()` succeeds, then the socket closes with no reply: `worker.busy` (11-control-socket.md, rule 11).

| Code | Where | Meaning, hint |
|---|---|---|
| `pair.invalid_code` | worker | Not a `lattice1:` code, or malformed. Copy it again |
| `pair.denied` | worker | The deck refused: wrong secret, used, expired, replaced, or no invite open. Make a new invite |
| `pair.timeout` | worker | No `PairOk` within `PAIR_TIMEOUT` |
| `pair.expired` | deck UI | `INVITE_TTL` passed |
| `worker.not_running` | worker CLI | `latticed` is not running. Start it, then run the command again |
| `worker.busy`, `worker.already_running`, `deck.already_running`, `control.*` | worker CLI, start | Control socket and single instance: [11-control-socket.md](11-control-socket.md) › Codes |
| `relay.*`, `transport.identity_mismatch`, `transport.unreachable`, file write errors | worker | Passed through |

### Control socket

`lattice add-deck` and `lattice config` talk to `latticed` over the worker control socket: [11-control-socket.md](11-control-socket.md).

## Invariants

Checks per [.rules/verification.md](../../.rules/verification.md). All planned.

1. `Invite` and `Contact` round-trip through the code; trailing bytes, non-canonical base32, and a non-canonical or invalid host fail. Check: proptest `invite_code_roundtrips`, tests `invite_code_rejects_trailing_bytes`, `contact_rejects_a_non_canonical_host`.
2. The deck saves a worker only after a valid secret, and before `PairOk`. Check: test `deck_saves_before_pair_ok`.
3. One secret pairs at most one worker, under concurrent requests. Check: test `pair_secret_is_single_use`.
4. Pairing frame decoding never panics on any bytes; a body over 4 KiB is refused unread. Check: fuzz target `pair_request_decode`, proptest `pair_request_decode_never_panics`.
5. The deck refuses an unknown NodeId outside `Waiting` at once. Check: test `an_unknown_node_outside_waiting_is_refused`.
6. A refused pairing writes nothing on the worker. Check: test `a_refused_pairing_writes_nothing_on_the_worker`.
7. While `Waiting`, a paired worker can pair again. Check: test `a_known_worker_can_pair_again`.
8. An `AddDeck` with a maximum-length `Contact` (253-byte host) fits the 1024-byte code limit. Check: test `an_add_deck_with_a_max_length_contact_fits`.
9. At most 8 pairing connections are served at once. Check: test `a_ninth_pairing_connection_is_refused`.
10. A deleted allowlist line is refused within 2 s and never written back; open connections of that deck close. Check: tests `a_deleted_allowlist_line_is_refused_and_never_rewritten`, `a_deleted_allowlist_line_closes_open_connections`.

## UI

Design system: patterns.md › Add worker; components.md › Invite command.
M1 deck: one active worker, the most recently paired one. TBD: switch the active worker from the rail.

Add worker dialog (desktop deck):

1. The rail shows one entry per paired worker: its monogram and its link state glyph (02-connectivity.md, Link states).
2. `+` opens the dialog. It makes an invite unless one is still `Waiting`; then it shows that one again.
3. Close and `Escape` hide the dialog. The invite stays open until it pairs or ends.
4. The status follows Data › Invite states: `registering…`, `waiting for worker…`, `paired · <name>`, `expired`, or the error code. There is no `pairing…` state.
5. `Copy` copies `echo <code> | lattice add-deck` (proposed), so the code never appears in `argv`. It may land in shell history (CLI rule 1).
6. `New invite` shows once the invite ended, and next to a `Waiting` invite. It replaces the invite.
7. On `paired · <name>` the deck links to that worker: it is the most recently paired one.
8. With no paired worker, the workspace shows "No worker yet." and `Add worker`, which opens the dialog.

Canvas: "Desktop — Add worker · invite flow · no worker · link states". The simulator runs the same flow.
TBD: iOS adds a worker the same way. The canvas shows a proposed sheet ("Mobile — Add worker"); confirm it (patterns.md › Mobile)?

## Open questions

- TBD: worker rename flow.
- TBD: count `PairRequest` attempts per invite? Rule 4 bounds time per connection and concurrent connections, not the total number.
- TBD: recovery when every deck is lost.
