# 10 — Settings

Status: draft
Owns: the settings registry, scopes and precedence, the settings editor, the worker config file and its `worker/config` stream, the `lattice config` CLI, the relay prompt of `install.sh`.
Implemented (M2.11, partial): Worker config, CLI, and Install, in `lattice-control`, `latticed`, `lattice`, and `install.sh`. Planned: the registry, the editor, the `worker/config` stream (rule 15), and the first-contact pin (rules 39–40, M2.8).
Link behavior and relay status: [02-connectivity.md](02-connectivity.md). Relay mechanism: [architecture/01-transport.md](../architecture/01-transport.md). Decision: D51.

## Summary

Every setting comes from a registered schema: the platform's (`lattice.*`) or an app's (`app.toml` `[settings]`).
A value lives in one scope. Precedence: device (This deck) > user (synced) > default. Worker settings live on the worker, in its config file.
The deck edits settings in a Settings tab. The worker edits its file with `lattice config`.

## Behavior

### Registry

1. Every setting has a schema in the registry. A key without a schema is never applied.
2. Platform keys use the namespace `lattice.` (`lattice.connectivity.relay`). Each app declares its keys in `app.toml` `[settings.<name>]` (architecture/06-apps.md, Manifest). Its namespace is its app id (`dev.lattice.terminal.font_size`).
3. A key name segment matches `[a-z][a-z0-9_]*` (proposed).
4. Schema fields: see Data › Schema fields. `type`, `default`, `scope`, `title`, and `description` are required.
5. The host validates every write against the schema before it stores it. An invalid value is not stored. The editor row shows the error inline.
6. A write equal to the value of the next scope down removes the key from the scope. So a scope holds only keys that differ from the next scope down.
7. A stored value that fails its schema (written by a deck with another schema, or a removed key) is not applied. Resolution falls to the next scope down. The editor flags the row with `settings.invalid` or `settings.unknown_key`, and keeps the value.
8. An app reads only its own namespace, as resolved values, through a host call. TBD: name and signature of the host call (ABI v0 has no imports, architecture/06-apps.md › Runtimes)?
9. An app gets a notification when a resolved value in its namespace changes.

TBD: runners run on workers, which do not hold the user `Doc`. Does a runner read settings (for example `scrollback_lines`), and how do they reach the worker?

### Scopes

10. Scopes, highest first: `device` (This deck, local, not synced), `user` (synced to all the user's decks through the `user/meta` Doc, E2E; architecture/02-sync.md), `default` (the schema).
11. Until the `user/meta` Doc exists (ROADMAP M2.6), the user scope is stored on the deck and does not sync. Both scopes live in `<data dir>/deck/settings.toml`, in tables `[user]` and `[device]` (proposed). Writers follow the file rules of 01-pairing.md › Files. The scope note reads `Not synced yet` (proposed).
12. Resolution: the first scope that holds a valid value wins.
13. A schema `scope` of `user` allows a value in `user` and an override in `device`. A schema `scope` of `device` allows `device` only.
14. A schema `scope` of `worker` is platform-only. Its value lives on that worker, in its config file (Worker config). The v1 deck shows it read-only with the hint "Edit on the worker: `lattice config set <key> <value>`".
15. The worker publishes its resolved config as the worker-owned `State` stream `worker/config` (architecture/02-sync.md). It holds: each key with its value and source (`flag`, `env`, `file`, `default`), plus `read_at`, the time `latticed` last read the file. Decks subscribe to it. The source line `from <path> · read 12s ago` comes from `read_at`.

### Editor (deck)

16. Settings opens as a tab, as a file tab does. A second open focuses the existing Settings tab (proposed).
17. Entry points: `⌘,`, `⌘K` → "Settings", and the gear button at the bottom of the worker rail.
18. The gear button sits below a divider, pinned to the rail bottom. While the Settings tab is active it shows one step lighter surface (`bg/2`) and a 2px left bar in `fg/muted` (≥ 3:1 on `bg/0`). It never uses `accent`.
19. Header: search field, scope picker (`User` | `This deck`), and `{}` (TOML view).
20. Search matches key id, title, and description, ignoring case. The token `@modified` keeps only modified rows. Search spans the whole tree.
21. The scope picker selects the scope that edits write to. `This deck` rows show the device value over the user value. Worker nodes show `worker · <name>`, read-only.
22. Tree: Appearance, Keyboard, Connectivity, Workers › one node per paired worker, Apps › one node per app that declares settings. The selected node shows a 2px `fg/muted` left bar. The bar turns `accent` only while the tree has keyboard focus.
23. Row: title, key id (mono), description, control. The control follows the type (Data › Types).
24. Modified row (the selected scope holds the key, rule 6): a 2px left bar in `fg/muted` and the text label `modified · default <value>`, plus reset `↺`. The text carries the meaning; no state hue. Reset removes the key from the selected scope.
25. Invalid input: the value is not stored (rule 5). The row shows the error code and a message under the control. The previous value stays in effect.
26. `{}` opens a TOML view of the selected scope as a new tab. It is read-only in v1 (owner's choice, D51). Until M2.6 the user-scope view reads `deck/settings.toml` (rule 11). It renders the stored scope; it is not a file on disk.
27. Every editor action has a chord: [09-keyboard.md](09-keyboard.md) › Settings.
28. Mobile: a list of sections, then drill-down pages. Rows are at least `row/touch` (44px). No TOML view.

TBD: the mobile entry point to Settings (sessions drawer?).

### Connectivity (deck)

| Key | Type | Default | Scope | Meaning |
|---|---|---|---|---|
| `lattice.connectivity.relay` | string, pattern `udp://[<64 hex>@]<host>:<port>` or `none`; host canonical (01-pairing.md › Contact) | `none` | user, device override | Home relay of this deck (02-connectivity.md, Relay) |
| `lattice.connectivity.relay_id` | `map<string,string>`, relay `host:port` → 64 hex digits | empty | device | Relay ids pinned on first contact. Read-only, with `Re-pin` |
| `lattice.connectivity.direct` | bool | `true` | user, device override | Upgrade to a direct path when the home relay is set. Off: no direct path is offered; the relay you dial through still sees your IP. With relay `none`, peers dial `ip:port` and this key has no effect |
| `lattice.connectivity.advertise_ip` | optional string, IP address | empty = auto | device | IP offered for the direct path. `LATTICE_ADVERTISE_IP` overrides it |
| `lattice.connectivity.listen_port` | optional int 1024–65535 | empty = the OS picks | device | Local UDP port |

29. Every relay URL, from any source (worker file, `--relay`, `LATTICE_RELAY`, deck setting), uses the canonical host of 01-pairing.md › Contact. Pins and URLs compare by canonical `host:port`. A non-canonical host fails with `config.invalid_value` (worker) or `settings.pattern_mismatch` (deck). The deck pin is keyed by the relay it pins. The deck uses the entry of `lattice.connectivity.relay_id` whose `host:port` equals the resolved relay URL. Entries for other relays are ignored. An id inside the relay URL always wins over the map.
30. The Connectivity page starts with the read-only Status block (02-connectivity.md, Relay status) and a secondary `Test relay` button.
31. Only the relay id pin writes `relay_id` (02-connectivity.md, rule 21). The editor shows only the entry of the current relay. `Re-pin` removes the entry of the current relay; the next contact pins again (01-transport.md, Relay rule 25).

The relay default is `none` until a public relay exists. TBD: public relay host, and does the default then change?
TBD: `relay` = `none` together with `direct` = `false` leaves no path to this deck. Reject the write, or warn?

### Worker config

32. File: `<data dir>/worker/config.toml` (data dir: [01-pairing.md](01-pairing.md) › Files). `LATTICE_CONFIG` overrides the path.
33. Precedence per key: flag > environment > file > default. A key without a flag or an environment variable skips that step.
34. A running `latticed` is the only writer of `config.toml`. The CLI changes it through the control socket (`ConfigSet`, `ConfigReset`, `ConfigReplace`; 11-control-socket.md › Messages).
   - The CLI writes the file itself only while it holds the exclusive `flock` on the `worker/` directory. It creates `worker/` (mode 0700) if missing, refuses a symlink or a dir owned by another user, then opens it as a directory and locks it (11-control-socket.md, rule 1 step 1). Holding the lock proves no daemon runs.
   - `connect()` fails and the lock is taken (a daemon is starting or stuck): `worker.busy`. The CLI never falls back to writing.
   - `connect()` succeeds but the socket closes with no reply: `worker.busy`.
   - `install.sh` never locks or writes `config.toml` itself (rule 51).
   - Every writer writes a temporary file in the same dir, syncs it, then renames it over the old file.
35. `latticed` watches the file. A changed `relay.addr` applies live: `latticed` re-registers at the new relay, no restart. A changed `listen.addr` needs a restart (proposed); `latticed` logs `config.restart_needed`.
36. A file that fails to parse or validate at start makes `latticed` exit with the error code. On a live reload, `latticed` keeps the last valid values and logs the code (proposed).
37. The worker pin is keyed by the relay it pins: `relay.id` plus `relay.pinned_for = "host:port"`. `latticed` uses the pin only when `relay.pinned_for` equals the `host:port` of the resolved relay URL, from any source (flag, environment, file, live reload). Otherwise it ignores the pin and pins again on first contact.
38. An id inside the relay URL (`udp://<id>@host:port`) is an explicit pin and always wins. `latticed` then never writes `relay.id`.
39. `latticed` writes `relay.id` and `relay.pinned_for` when it pins a relay id (02-connectivity.md, rule 21).
40. On `relay.id_changed`, `latticed` logs the code and the note: `paired decks must pair again after a re-pin` (01-pairing.md, rule 22).

| Key | Flag | Environment | Default | Meaning |
|---|---|---|---|---|
| `relay.addr` | `--relay` (proposed) | `LATTICE_RELAY` | `none` | Home relay URL or `none` |
| `relay.id` | — | — | empty | Pinned relay id, 64 hex digits |
| `relay.pinned_for` | — | — | empty | Relay `host:port` that `relay.id` belongs to |
| `listen.addr` | `--listen` | — | `0.0.0.0:7441` | UDP listen address |
| `name` | — | — | the host name | Worker name. `lattice add-deck --name` overrides it for one pairing |

```toml
name = "build-01"

[relay]
addr = "udp://relay.example.com:7400"
id = "b41d…09ae"                       # 64 hex digits, written by the pin
pinned_for = "relay.example.com:7400"

[listen]
addr = "0.0.0.0:7441"
```

TBD: the magic socket carries no direct path until ROADMAP M2.5. So a worker with a relay is reachable only through it, and a change between `none` and a relay (rule 35) logs `config.restart_needed`. Apply that change live once direct paths land?
TBD: the relay client needs the relay id up front until M2.8. An unpinned relay URL logs `relay.no_id`, and `latticed` serves direct only. Keep that fallback, or fail the start?
TBD: a relay host that does not resolve at start fails the start. Retry instead, for a boot before the network is up?
TBD: `latticed` polls the file every 1 s for rule 35. Which reload latency does the spec want?
TBD: a failed read or write of `config.toml` reports `config.io`. Which code?
TBD: the rule for a `name` value? The code refuses an empty name and control characters.

On a deck the worker keys show as `lattice.worker.<key>` (`lattice.worker.relay.addr`), read-only (rule 14).

### CLI

41. `lattice config list` prints every key with its resolved value and source. When `<data dir>/worker/latticed.sock` answers, the CLI sends `ConfigList` (11-control-socket.md › Messages), and the values and sources (`flag`, `env`, `file`, `default`) come from the running daemon. Otherwise the CLI shows only `env`, `file`, and `default` as it sees them, and prints `latticed not running: flags not shown`.
42. `lattice config get <key>` prints the resolved value. It sends `ConfigGet{key}` when the socket answers, else follows the fallback of rule 41.
43. `lattice config set <key> <value>` sends `ConfigSet{key, value}` when the daemon answers, else validates and writes the file itself (rule 34). `set relay.addr` to another relay prints: `relay changed: paired decks must pair again (lattice add-deck)`. v1 has no address update (01-pairing.md, rule 22).
44. `lattice config reset <key>` removes the key from the file (`ConfigReset{key}` when the daemon answers); the default applies (proposed). `reset relay.id` also removes `relay.pinned_for`; the next contact pins again.
45. `lattice config edit` opens a copy in `$EDITOR` and records the file's blake3 hash. With the daemon running it sends `ConfigReplace{toml, base_hash}`; `latticed` replaces the file only when the copy validates and the file still has `base_hash`. Without a daemon the CLI does the same check on disk. A changed file fails with `config.conflict`; an invalid copy prints its error. Either way the old file stays.
46. An unknown key fails with `config.unknown_key`. An invalid value fails with `config.invalid_value`. A file that is not TOML fails with `config.parse`. The file stays unchanged, and the command exits non-zero.

### Install

47. `install.sh worker` (and `all`) asks for the relay through `/dev/tty`, because stdin is the `curl` pipe:

```
Relay for this worker:
  [1] none (direct only, default)
  [2] self-hosted: udp://host:port
```

48. Enter accepts the default.
49. `--relay <url|none>` or `LATTICE_RELAY` answers without asking. With no tty and no answer, the default applies.
50. An invalid URL asks again on a tty, and fails with `config.invalid_value` otherwise.
51. `install.sh` creates the data dir, resolving it as `latticed` does. Then it records the answer by running `lattice config set relay.addr <answer>`, `none` included. That command takes the lock or uses the daemon (rule 34).
52. `install.sh` never changes an existing `config.toml`: with the file present it skips rule 51's `set`. It prints the current `relay.addr` instead. With `--relay` or `LATTICE_RELAY` it also prints a warning that the file was left unchanged.
53. `--dry-run` prints the relay it would write and asks nothing (proposed).

#### User services

Planned (M2.11). Pairing and serving need a running `latticed` (D53), so the installer starts both daemons.

54. `install.sh worker` (and `all`) installs `latticed-proc` and `latticed` as user services and starts them: `systemctl --user` units on Linux, LaunchAgents on macOS.
55. `latticed-proc` starts before `latticed` (04-worker.md, `latticed` start order). Each restarts on failure with backoff, and stops after 5 failures in 60 s (proposed).
56. `--no-service` skips rules 54–55. The installer then prints how to start both daemons by hand.
57. Re-install replaces the binaries and restarts `latticed` only (proposed). `latticed-proc` keeps running, so no process dies (D11). TBD: how does a re-install pick up a new `latticed-proc`?
58. A missing service manager (no user systemd, for example in a container) prints a warning and continues as `--no-service`.

TBD: unit and agent names, file locations, log locations, and whether user services start at boot without a login (`loginctl enable-linger`)?

TBD: offer the public relay as a choice once its host exists?
TBD: should `install.sh all`, which also installs `lattice-relay`, offer this machine's relay as a choice?

## Data

### Schema fields

| Field | Required | Meaning |
|---|---|---|
| `type` | yes | One of Types below |
| `default` | yes | Value of the `default` scope. Must validate |
| `scope` | yes | `user`, `device`, or `worker` (platform only) |
| `title` | yes | Row title |
| `description` | yes | Row description, one or two sentences |
| `min`, `max` | for `int` | Inclusive bounds |
| `values` | for `enum` | Allowed strings |
| `pattern` | for `string`, optional | Regular expression the whole value must match. TBD: regex dialect? |
| `optional` | no, default `false` | `true`: the value may be empty, meaning auto (Types) |

### Types

| Type | Control (desktop) | Control (mobile) | Error |
|---|---|---|---|
| `bool` | Toggle | Toggle | — |
| `int{min,max}` | Number stepper | Number field | `settings.out_of_range` |
| `enum` | Dropdown | Picker row | `settings.not_allowed` |
| `string{pattern}` | Text field | Text field | `settings.pattern_mismatch` |
| `path` | Mono field + browse on the worker | Mono field | `settings.invalid_path` |
| `list<string>` | Editable chips | Editable list | `settings.invalid` |
| `map<string,string>` | Read-only key/value list (v1) | Read-only list | `settings.invalid` |

Modifier `optional`: any type may be optional. Empty means auto: the platform picks the value at run time (`advertise_ip`, `listen_port`). An optional field shows a placeholder (`auto`).

Error codes are proposed. Other codes: `settings.unknown_key`, `settings.invalid` (wrong type).

## Invariants

Checks per [.rules/verification.md](../../.rules/verification.md). Implemented in `lattice-control` (M2.11).

1. A pin for another relay `host:port` is ignored (rules 29 and 37). Check: test `a_pin_for_another_host_is_ignored`.
2. An explicit id in the relay URL is never overwritten by a first-contact pin (rule 38). Check: test `an_explicit_id_is_never_overwritten`.
3. Writes to `config.toml` are serialized: only the `flock` holder writes (rule 34). Check: test `config_writes_are_serialized`.

## UI

Design system: patterns.md › Settings; components.md › Settings row, Relay RTT level, Read-only notice.
Canvas: "Settings registry", "Settings tab", "Settings · Connectivity", "Settings · worker", "Raw settings.toml", "Mobile — Settings", "Mobile — Settings · Connectivity".

## Open questions

- TBD: worker settings writable from the deck (after v1)?
