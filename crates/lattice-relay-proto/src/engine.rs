//! Relay server engine: one datagram in, what to send out.

mod admit;
mod guard;

use crate::codec::{Datagram, ObservedAddr, RelayMsg, Token, classify, decode, encode};
use crate::table::TokenTable;
use admit::{Cookies, RateLimit};
use guard::ReplayGuard;
use lattice_key::{NodeId, Signature};
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
/// Most nodes tracked, registrations and replay entries alike (rule 8; proposed).
const MAX_NODES: usize = 131_072;

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
    guard: ReplayGuard,
    cookies: Cookies,
    rate: RateLimit,
    /// [`MAX_NODES`]; a field so tests can fill it with a few entries.
    max_nodes: usize,
}

/// The fields of one `REGISTER`.
struct Register {
    node_id: NodeId,
    nonce: [u8; 16],
    ts: u64,
    sig: Signature,
    cookie: [u8; 16],
}

impl RelayEngine {
    /// A relay with key `relay_id` granting registrations for `ttl` ([`DEFAULT_TTL`] in production).
    /// `cookie_secret` comes from the driver's RNG, fresh per process (rule 19).
    ///
    /// `ttl` is cut to whole seconds, at most `u16::MAX`: the relay enforces what `REG_OK` says.
    pub fn new(relay_id: NodeId, ttl: Duration, cookie_secret: [u8; 32]) -> Self {
        let secs = ttl.as_secs().min(u64::from(u16::MAX));
        Self {
            relay_id,
            ttl: Duration::from_secs(secs),
            table: TokenTable::new(),
            guard: ReplayGuard::default(),
            cookies: Cookies::new(cookie_secret),
            rate: RateLimit::default(),
            max_nodes: MAX_NODES,
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
                cookie,
            } => {
                let reg = Register {
                    node_id,
                    nonce,
                    ts,
                    sig,
                    cookie,
                };
                self.on_register(now, src, &reg, fresh_token)
            }
            RelayMsg::Keepalive { token } => self.on_keepalive(now.mono, src, token),
            RelayMsg::Send { token, dst, packet } => {
                self.on_send(now.mono, src, token, dst, packet)
            }
            // PUNCH_REQ waits for M2.5 (rule 14). The rest only flows relay → node.
            RelayMsg::PunchReq { .. }
            | RelayMsg::RegOk { .. }
            | RelayMsg::Pong { .. }
            | RelayMsg::Cookie { .. }
            | RelayMsg::Recv { .. }
            | RelayMsg::PunchGo { .. } => EngineOutput::Drop,
        }
    }

    /// Drops expired registrations, replay entries and idle rate buckets. Call on a timer.
    pub fn on_tick(&mut self, now: Now) {
        self.table.expire(now.mono);
        self.guard.expire(|ts| is_fresh(now.unix_secs, ts));
        self.rate.expire(now.mono);
    }

    /// Rules 6–9, 17–21: cheap checks first, the signature last.
    fn on_register(
        &mut self,
        now: Now,
        src: SocketAddr,
        reg: &Register,
        token: Token,
    ) -> EngineOutput {
        if !is_fresh(now.unix_secs, reg.ts) {
            return EngineOutput::Drop;
        }
        if !self.cookies.valid(src, now.unix_secs, reg.cookie) {
            // Rule 18: no state for an unproven address, only a cookie back to it.
            let cookie = self.cookies.issue(src, now.unix_secs);
            let nonce = reg.nonce;
            return reply(src, &RelayMsg::Cookie { cookie, nonce });
        }
        if !self.rate.allow(src.ip(), now.mono)
            || !self.guard.admits(reg.node_id, reg.ts, reg.nonce)
            || !self.has_room_for(reg.node_id)
        {
            return EngineOutput::Drop;
        }
        let signed = register_signed(self.relay_id, reg.nonce, reg.ts, reg.cookie);
        if reg.node_id.verify(&signed, &reg.sig).is_err() {
            return EngineOutput::Drop;
        }
        if self.table.contains(token) {
            // A 64-bit collision: the client retries with a new REGISTER (rule 13).
            return EngineOutput::Drop;
        }
        let (node_id, nonce) = (reg.node_id, reg.nonce);
        self.guard.record(node_id, reg.ts, nonce);
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

    /// Rule 8: a known node always has room; a new one only below the cap.
    fn has_room_for(&self, node: NodeId) -> bool {
        let known = self.guard.knows(node) || self.table.knows(node);
        known || (self.guard.len() < self.max_nodes && self.table.nodes() < self.max_nodes)
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

/// Length of what a `REGISTER` signs: relay_id 32 + nonce 16 + ts 8 + cookie 16.
const REGISTER_SIGNED_LEN: usize = 72;

/// The bytes a `REGISTER` signs: `relay_id || nonce || ts || cookie` (`ts` big-endian).
/// The cookie is signed so nobody can swap in their own (rule 17).
pub(crate) fn register_signed(
    relay_id: NodeId,
    nonce: [u8; 16],
    ts: u64,
    cookie: [u8; 16],
) -> Vec<u8> {
    let mut signed = Vec::with_capacity(REGISTER_SIGNED_LEN);
    signed.extend_from_slice(relay_id.as_bytes());
    signed.extend_from_slice(&nonce);
    signed.extend_from_slice(&ts.to_be_bytes());
    signed.extend_from_slice(&cookie);
    signed
}

fn reply(dst: SocketAddr, msg: &RelayMsg<'_>) -> EngineOutput {
    let mut bytes = Vec::new();
    encode(msg, &mut bytes);
    EngineOutput::Reply(Transmit { dst, bytes })
}

#[cfg(test)]
mod tests;
