//! Relay server engine: one datagram in, what to send out.

use crate::codec::{Datagram, ObservedAddr, RelayMsg, Token, classify, decode, encode};
use crate::table::TokenTable;
use lattice_key::{NodeId, Signature};
use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::time::Duration;

/// One datagram to send.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transmit {
    dst: SocketAddr,
    bytes: Vec<u8>,
}

impl Transmit {
    /// Where to send.
    pub fn dst(&self) -> SocketAddr {
        self.dst
    }

    /// Datagram bytes.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// Default registration lifetime without a keepalive (01, Relay rule 10).
pub const DEFAULT_TTL: Duration = Duration::from_secs(90);
/// A `REGISTER` is fresh when its `ts` is within this many seconds of the relay clock (rule 6).
const REGISTER_WINDOW_SECS: u64 = 30;
/// Most remembered nonces; new `REGISTER`s are dropped while full (rule 8).
const MAX_SEEN_NONCES: usize = 65_536;

/// What the UDP loop does with one input datagram.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineOutput {
    /// Drop silently: unknown, unauthorized, or QUIC sent to the relay.
    Drop,
    /// Answer the sender (`REG_OK`, `PONG`).
    Reply(Transmit),
    /// `SEND` rewritten as `RECV` to the destination.
    Forward(Transmit),
    /// `PUNCH_GO` to both sides.
    Punch([Transmit; 2]),
}

/// Relay clocks, injected by the driver (verification.md rule 2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Now {
    mono: Duration,
    unix_secs: u64,
}

impl Now {
    /// `mono`: monotonic time since the relay started. `unix_secs`: wall clock, UNIX seconds.
    pub fn new(mono: Duration, unix_secs: u64) -> Self {
        Self { mono, unix_secs }
    }
}

/// Sans-IO relay. Owns the token table. Never reads packet contents.
#[derive(Debug)]
pub struct RelayEngine {
    relay_id: NodeId,
    ttl: Duration,
    table: TokenTable,
    /// `(node_id, nonce) → ts`: forgotten once `ts` is stale on the wall clock (rule 7).
    seen: BTreeMap<(NodeId, [u8; 16]), u64>,
    /// [`MAX_SEEN_NONCES`]; a field so tests can fill it with a few entries.
    max_seen: usize,
}

impl RelayEngine {
    /// A relay with key `relay_id` granting registrations for `ttl` ([`DEFAULT_TTL`] in production).
    ///
    /// `ttl` is cut to whole seconds, at most `u16::MAX`: the relay enforces what `REG_OK` says.
    pub fn new(relay_id: NodeId, ttl: Duration) -> Self {
        let secs = ttl.as_secs().min(u64::from(u16::MAX));
        Self {
            relay_id,
            ttl: Duration::from_secs(secs),
            table: TokenTable::new(),
            seen: BTreeMap::new(),
            max_seen: MAX_SEEN_NONCES,
        }
    }

    /// Handles one datagram from `src` at `now`.
    ///
    /// `fresh_token` is a random token from the driver's RNG; used only when
    /// this datagram creates a registration (keeps the engine deterministic).
    pub fn handle(
        &mut self,
        now: Now,
        src: SocketAddr,
        bytes: &[u8],
        fresh_token: Token,
    ) -> EngineOutput {
        if classify(bytes) != Datagram::Relay {
            return EngineOutput::Drop;
        }
        let Ok(msg) = decode(bytes) else {
            return EngineOutput::Drop;
        };
        match msg {
            RelayMsg::Register {
                node_id,
                nonce,
                ts,
                sig,
            } => self.on_register(now, src, (node_id, nonce, ts, sig), fresh_token),
            RelayMsg::Keepalive { token } => self.on_keepalive(now.mono, src, token),
            RelayMsg::Send { token, dst, packet } => {
                self.on_send(now.mono, src, token, dst, packet)
            }
            // PUNCH_REQ waits for M2.5 (rule 14). The rest only flows relay → node.
            RelayMsg::PunchReq { .. }
            | RelayMsg::RegOk { .. }
            | RelayMsg::Pong { .. }
            | RelayMsg::Recv { .. }
            | RelayMsg::PunchGo { .. } => EngineOutput::Drop,
        }
    }

    /// Drops expired registrations and nonces. Call on a timer.
    pub fn on_tick(&mut self, now: Now) {
        self.table.expire(now.mono);
        self.seen.retain(|_, ts| is_fresh(now.unix_secs, *ts));
    }

    fn on_register(
        &mut self,
        now: Now,
        src: SocketAddr,
        (node_id, nonce, ts, sig): (NodeId, [u8; 16], u64, Signature),
        token: Token,
    ) -> EngineOutput {
        if !is_fresh(now.unix_secs, ts) {
            return EngineOutput::Drop;
        }
        // Nonce cache before the signature: a repeat costs no curve work (rule 17).
        let key = (node_id, nonce);
        if self.seen.contains_key(&key) || self.seen.len() >= self.max_seen {
            return EngineOutput::Drop;
        }
        if node_id
            .verify(&register_signed(self.relay_id, nonce, ts), &sig)
            .is_err()
        {
            return EngineOutput::Drop;
        }
        if self.table.contains(token) {
            // A 64-bit collision: the client retries with a new REGISTER (rule 13).
            return EngineOutput::Drop;
        }
        self.seen.insert(key, ts);
        self.table.register(node_id, src, token, now.mono, self.ttl);
        let ttl = u16::try_from(self.ttl.as_secs()).unwrap_or(u16::MAX);
        let observed = ObservedAddr::new(src);
        reply(
            src,
            &RelayMsg::RegOk {
                token,
                observed,
                ttl,
                nonce,
            },
        )
    }

    /// Rule 11. From another address: a `PONG` so the client sees its rebinding, no refresh.
    fn on_keepalive(&mut self, now: Duration, src: SocketAddr, token: Token) -> EngineOutput {
        if self.table.authorize(token, src, now).is_some() {
            self.table.refresh(token, now, self.ttl);
        } else if self.table.live(token, now).is_none() {
            return EngineOutput::Drop;
        }
        reply(
            src,
            &RelayMsg::Pong {
                observed: ObservedAddr::new(src),
            },
        )
    }

    fn on_send(
        &self,
        now: Duration,
        src: SocketAddr,
        token: Token,
        dst: NodeId,
        packet: &[u8],
    ) -> EngineOutput {
        let Some(sender) = self.table.authorize(token, src, now) else {
            return EngineOutput::Drop;
        };
        let Some(peer) = self.table.lookup_node(dst, now) else {
            return EngineOutput::Drop;
        };
        let mut bytes = Vec::new();
        encode(
            &RelayMsg::Recv {
                src: sender.node_id(),
                packet,
            },
            &mut bytes,
        );
        EngineOutput::Forward(Transmit {
            dst: peer.addr(),
            bytes,
        })
    }
}

/// Rule 6: `ts` within [`REGISTER_WINDOW_SECS`] of the relay wall clock.
fn is_fresh(unix_secs: u64, ts: u64) -> bool {
    unix_secs.abs_diff(ts) <= REGISTER_WINDOW_SECS
}

/// Length of what a `REGISTER` signs: relay_id 32 + nonce 16 + ts 8.
const REGISTER_SIGNED_LEN: usize = 56;

/// The bytes a `REGISTER` signs: `relay_id || nonce || ts` (big-endian).
pub(crate) fn register_signed(relay_id: NodeId, nonce: [u8; 16], ts: u64) -> Vec<u8> {
    let mut signed = Vec::with_capacity(REGISTER_SIGNED_LEN);
    signed.extend_from_slice(relay_id.as_bytes());
    signed.extend_from_slice(&nonce);
    signed.extend_from_slice(&ts.to_be_bytes());
    signed
}

fn reply(dst: SocketAddr, msg: &RelayMsg<'_>) -> EngineOutput {
    let mut bytes = Vec::new();
    encode(msg, &mut bytes);
    EngineOutput::Reply(Transmit { dst, bytes })
}

#[cfg(test)]
mod tests;
