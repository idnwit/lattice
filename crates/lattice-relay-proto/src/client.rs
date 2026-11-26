//! Client side of the relay: registration, keepalive, `SEND` / `RECV`.

use crate::codec::{ObservedAddr, RECV_HEADER_LEN, RelayMsg, Token, decode, encode};
use crate::engine::register_signed;
use lattice_key::{NodeId, NodeKey};
use std::ops::Range;
use std::time::Duration;

/// Keepalive period. Many home NATs drop UDP mappings after ~30 s (01, Relay rule 3).
pub const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(25);

/// What a relay datagram meant for this node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientEvent {
    /// `REG_OK`: registered; the relay sees us at `observed`.
    Registered {
        /// Our public address as the relay saw it.
        observed: ObservedAddr,
    },
    /// `PONG` showed a new observed address (NAT rebinding). A `REGISTER` is due
    /// now; the token stays until its `REG_OK` (rule 16).
    AddrChanged {
        /// New public address.
        observed: ObservedAddr,
    },
    /// `RECV`: a QUIC packet from `src`, at `packet` within the datagram.
    Packet {
        /// Sending node.
        src: NodeId,
        /// Byte range of the QUIC packet in the input datagram.
        packet: Range<usize>,
    },
    /// `PUNCH_GO`: start probing `peer`.
    PunchGo {
        /// Peer to probe.
        peer: NodeId,
        /// Shared probe nonce.
        punch_nonce: [u8; 16],
    },
}

/// Re-register when no `REG_OK` or `PONG` came for this long (01, Relay rule 13).
pub const SILENCE_LIMIT: Duration = Duration::from_secs(60);
/// Unregistered: send `REGISTER` this often until `REG_OK` (rule 13).
pub const REGISTER_RETRY: Duration = Duration::from_secs(5);

/// Registration FSM for one home relay. Never stops registering (01, Paths rule 4).
///
/// Driver loop: on each deadline call [`RelayClient::poll_register`] (then
/// [`RelayClient::register`] when it says so) and [`RelayClient::poll_keepalive`].
#[derive(Debug)]
pub struct RelayClient {
    relay_id: NodeId,
    state: State,
    /// Nonce of the last `REGISTER` built; only its `REG_OK` is accepted (rule 15).
    pending: Option<[u8; 16]>,
}

#[derive(Debug, Clone, Copy)]
enum State {
    /// No token. `retry_at`: when the next `REGISTER` is due (`None`: now).
    Unregistered { retry_at: Option<Duration> },
    /// `REG_OK` arrived.
    Registered {
        token: Token,
        observed: ObservedAddr,
        next_keepalive: Duration,
        last_heard: Duration,
        /// A `PONG` showed a new address: the next `REGISTER` is due then (rule 16).
        /// The token stays until the matching `REG_OK`, so a spoofed `PONG` costs
        /// one extra `REGISTER` per [`REGISTER_RETRY`], never the registration.
        rebind_at: Option<Duration>,
    },
}

impl RelayClient {
    /// A client for the relay whose key is `relay_id`. Starts unregistered.
    pub fn new(relay_id: NodeId) -> Self {
        Self {
            relay_id,
            state: State::Unregistered { retry_at: None },
            pending: None,
        }
    }

    /// True once `REG_OK` arrived, until [`SILENCE_LIMIT`] passes without a reply.
    pub fn is_registered(&self) -> bool {
        matches!(self.state, State::Registered { .. })
    }

    /// Builds a signed `REGISTER`. `nonce` and `ts` (UNIX seconds) come from the driver.
    /// Only the `REG_OK` echoing this `nonce` is accepted afterwards (rule 15).
    pub fn register(&mut self, key: &NodeKey, nonce: [u8; 16], ts: u64, out: &mut Vec<u8>) {
        self.pending = Some(nonce);
        let sig = key.sign(&register_signed(self.relay_id, nonce, ts));
        encode(
            &RelayMsg::Register {
                node_id: key.node_id(),
                nonce,
                ts,
                sig,
            },
            out,
        );
    }

    /// True when a `REGISTER` is due now; schedules the next retry.
    pub fn poll_register(&mut self, now: Duration) -> bool {
        self.check_silence(now);
        let next = Some(now.saturating_add(REGISTER_RETRY));
        match &mut self.state {
            State::Unregistered { retry_at } if retry_at.is_none_or(|at| now >= at) => {
                *retry_at = next;
                true
            }
            State::Registered {
                rebind_at: rebind @ Some(_),
                ..
            } if rebind.is_some_and(|at| now >= at) => {
                *rebind = next;
                true
            }
            State::Unregistered { .. } | State::Registered { .. } => false,
        }
    }

    /// Handles a relay datagram from the relay address. Bad datagrams are dropped silently.
    pub fn on_datagram(&mut self, now: Duration, bytes: &[u8]) -> Option<ClientEvent> {
        match decode(bytes).ok()? {
            RelayMsg::RegOk {
                token,
                observed,
                nonce,
                ..
            } => {
                if self.pending != Some(nonce) {
                    return None;
                }
                self.pending = None;
                self.state = State::Registered {
                    token,
                    observed,
                    next_keepalive: now.saturating_add(KEEPALIVE_INTERVAL),
                    last_heard: now,
                    rebind_at: None,
                };
                Some(ClientEvent::Registered { observed })
            }
            RelayMsg::Pong { observed: seen } => self.on_pong(now, seen),
            RelayMsg::Recv { src, .. } => Some(ClientEvent::Packet {
                src,
                packet: RECV_HEADER_LEN..bytes.len(),
            }),
            RelayMsg::PunchGo {
                peer, punch_nonce, ..
            } => Some(ClientEvent::PunchGo { peer, punch_nonce }),
            RelayMsg::Register { .. }
            | RelayMsg::Keepalive { .. }
            | RelayMsg::Send { .. }
            | RelayMsg::PunchReq { .. } => None,
        }
    }

    /// Writes a `KEEPALIVE` into `out` when due. Returns true when written.
    pub fn poll_keepalive(&mut self, now: Duration, out: &mut Vec<u8>) -> bool {
        self.check_silence(now);
        let State::Registered {
            token,
            next_keepalive,
            ..
        } = &mut self.state
        else {
            return false;
        };
        if now < *next_keepalive {
            return false;
        }
        *next_keepalive = now.saturating_add(KEEPALIVE_INTERVAL);
        encode(&RelayMsg::Keepalive { token: *token }, out);
        true
    }

    /// Wraps a QUIC packet for `dst` in a `SEND`. `None` until registered.
    pub fn wrap_send(&self, dst: NodeId, packet: &[u8], out: &mut Vec<u8>) -> Option<()> {
        let State::Registered { token, .. } = self.state else {
            return None;
        };
        encode(&RelayMsg::Send { token, dst, packet }, out);
        Some(())
    }

    /// Next time [`RelayClient::poll_register`] or [`RelayClient::poll_keepalive`] must run.
    pub fn next_deadline(&self) -> Option<Duration> {
        Some(match self.state {
            State::Unregistered { retry_at } => retry_at.unwrap_or(Duration::ZERO),
            State::Registered {
                next_keepalive,
                last_heard,
                rebind_at,
                ..
            } => {
                let due = next_keepalive.min(last_heard.saturating_add(SILENCE_LIMIT));
                rebind_at.map_or(due, |at| due.min(at))
            }
        })
    }

    fn on_pong(&mut self, now: Duration, seen: ObservedAddr) -> Option<ClientEvent> {
        let State::Registered {
            observed,
            last_heard,
            rebind_at,
            ..
        } = &mut self.state
        else {
            return None;
        };
        *last_heard = now;
        if *observed == seen {
            return None;
        }
        // The relay never moves a binding on KEEPALIVE: re-register from the new address.
        if rebind_at.is_none() {
            *rebind_at = Some(now);
        }
        Some(ClientEvent::AddrChanged { observed: seen })
    }

    /// Drops the token after [`SILENCE_LIMIT`] without `REG_OK` or `PONG` (rule 13).
    fn check_silence(&mut self, now: Duration) {
        if let State::Registered { last_heard, .. } = self.state
            && now >= last_heard.saturating_add(SILENCE_LIMIT)
        {
            self.state = State::Unregistered { retry_at: None };
        }
    }
}

#[cfg(test)]
mod tests;
