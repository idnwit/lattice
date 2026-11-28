# 08 — Infrastructure

Status: draft
Owns: hosted services. All are untrusted with content.

## Summary

There is no account server. The only hosted piece is the **relay**: a public UDP forwarder we run, which anyone can also self-host.

| Service | Role | v1 |
|---|---|---|
| Relay | Registration, first path for every connection, hole-punch coordination, fallback path | yes |
| Ciphertext store | Mailbox for Doc changes and blobs while devices are offline | no (later, optional) |
| Web app hosting | Serve the web deck from the relay node | no (web deferred) |

## Relay

1. Stateless except the registration table (`token → node_id, addr, expiry`).
2. Discovery is implicit: a node is reachable at its public address on its home relay. The address travels in the invite (specs/01-pairing.md › Contact; specs/02-connectivity.md rule 17), the worker list, and the user Doc.
3. Sees only: NodeIds, addresses, packet sizes, timing. Never content.
4. Rate limits per token on the public relay. Not needed in v1.
5. Regional placement matters for relay-path latency. TBD: first region.
6. Peers on different home relays connect: the dialer registers at the target's relay (D49; 01-transport.md › Connect sequence). Relays never talk to each other.
7. Each node picks its home relay: a self-hosted one or `none` (specs/10-settings.md). The default is `none` until a public relay exists. TBD: public relay host.

## What this replaces

Earlier specs said "cloud syncs account state". Now: decks sync the user `Doc` peer-to-peer. "Synced" in the UI means "all my devices have converged".

## Open questions

- TBD: worker wake (Wake-on-LAN) via another worker on the same LAN?
