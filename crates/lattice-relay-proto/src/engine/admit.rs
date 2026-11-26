//! Who may spend relay state on a `REGISTER`: a cookie proves the source address,
//! then a rate limit per source (01-transport.md, Relay rules 18–21).

use std::collections::{BTreeMap, BTreeSet};
use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

/// Cookie time bucket (rule 19). A cookie is valid in its bucket and the next one.
const COOKIE_BUCKET_SECS: u64 = 30;
/// `REGISTER`s per second per source, after a valid cookie (rule 20; proposed).
const REGISTER_RATE_PER_SEC: u64 = 10;
/// Burst per source (rule 20; proposed).
const REGISTER_BURST: u64 = 20;
/// `REGISTER`s per second per IPv6 /48, the second tier (rule 20; proposed).
const SLASH48_RATE_PER_SEC: u64 = 100;
/// Burst per IPv6 /48 (rule 20; proposed).
const SLASH48_BURST: u64 = 200;
/// Most buckets one tier tracks; when full, the least recently used is evicted (rule 20; proposed).
const MAX_SOURCES: usize = 65_536;
/// Token bucket fixed point: one `REGISTER` costs this many units.
const UNIT: u64 = 1_000;

/// Stateless return-routability cookies keyed by a per-process secret (rule 19).
#[derive(Clone)]
pub(crate) struct Cookies {
    secret: [u8; 32],
}

impl std::fmt::Debug for Cookies {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Cookies(..)")
    }
}

impl Cookies {
    pub(crate) fn new(secret: [u8; 32]) -> Self {
        Self { secret }
    }

    /// The cookie for `src` now.
    pub(crate) fn issue(&self, src: SocketAddr, unix_secs: u64) -> [u8; 16] {
        self.mac(src, unix_secs / COOKIE_BUCKET_SECS)
    }

    /// True when `cookie` was issued to `src` in this bucket or the one before.
    pub(crate) fn valid(&self, src: SocketAddr, unix_secs: u64, cookie: [u8; 16]) -> bool {
        if cookie == [0; 16] {
            return false;
        }
        let bucket = unix_secs / COOKIE_BUCKET_SECS;
        let now = ct_eq(&cookie, &self.mac(src, bucket));
        let before = bucket
            .checked_sub(1)
            .is_some_and(|b| ct_eq(&cookie, &self.mac(src, b)));
        now | before
    }

    /// `blake3::keyed_hash(secret, bucket || ip 16 || port 2)`, first 16 bytes.
    fn mac(&self, src: SocketAddr, bucket: u64) -> [u8; 16] {
        let mut data = [0u8; 26];
        let (b, rest) = data.split_at_mut(8);
        let (ip, port) = rest.split_at_mut(16);
        b.copy_from_slice(&bucket.to_be_bytes());
        ip.copy_from_slice(&v6_octets(src.ip()));
        port.copy_from_slice(&src.port().to_be_bytes());
        let hash = blake3::keyed_hash(&self.secret, &data);
        let mut out = [0u8; 16];
        if let Some(head) = hash.as_bytes().first_chunk::<16>() {
            out = *head;
        }
        out
    }
}

/// Compares without an early exit, so timing leaks nothing about a valid cookie.
fn ct_eq(a: &[u8; 16], b: &[u8; 16]) -> bool {
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn v6_octets(ip: IpAddr) -> [u8; 16] {
    match ip.to_canonical() {
        IpAddr::V4(v4) => v4.to_ipv6_mapped().octets(),
        IpAddr::V6(v6) => v6.octets(),
    }
}

/// Rate-limit key, first tier: an IPv4 address, or the /64 of an IPv6 address (rule 20).
/// A v4-mapped IPv6 address is its IPv4 form.
fn source_key(ip: IpAddr) -> [u8; 16] {
    match ip.to_canonical() {
        IpAddr::V4(_) => v6_octets(ip),
        IpAddr::V6(v6) => prefix(v6.octets(), 8),
    }
}

/// Rate-limit key, second tier: the /48 of an IPv6 address. IPv4 has none (rule 20).
fn slash48_key(ip: IpAddr) -> Option<[u8; 16]> {
    match ip.to_canonical() {
        IpAddr::V4(_) => None,
        IpAddr::V6(v6) => Some(prefix(v6.octets(), 6)),
    }
}

/// `octets` with every byte from `keep` on zeroed.
fn prefix(mut octets: [u8; 16], keep: usize) -> [u8; 16] {
    for (i, b) in octets.iter_mut().enumerate() {
        if i >= keep {
            *b = 0;
        }
    }
    octets
}

#[derive(Debug, Clone, Copy)]
struct Bucket {
    units: u64,
    /// Last refill, which is also the last use: every request touches its bucket.
    at: Duration,
}

/// One tier of token buckets with least-recently-used eviction (rule 20).
#[derive(Debug)]
struct Tier {
    /// `REGISTER`s per second; also refill units per millisecond, as one costs [`UNIT`].
    rate_per_sec: u64,
    burst: u64,
    max: usize,
    buckets: BTreeMap<[u8; 16], Bucket>,
    /// `(last use, key)` of every bucket: the first entry is the one to evict.
    by_use: BTreeSet<(Duration, [u8; 16])>,
}

impl Tier {
    fn new(rate_per_sec: u64, burst: u64) -> Self {
        Self {
            rate_per_sec,
            burst,
            max: MAX_SOURCES,
            buckets: BTreeMap::new(),
            by_use: BTreeSet::new(),
        }
    }

    fn full(&self) -> u64 {
        self.burst.saturating_mul(UNIT)
    }

    /// Units `key` holds at `now`; a key not tracked holds a full bucket.
    fn units(&self, key: [u8; 16], now: Duration) -> u64 {
        let Some(b) = self.buckets.get(&key) else {
            return self.full();
        };
        let ms = u64::try_from(now.saturating_sub(b.at).as_millis()).unwrap_or(u64::MAX);
        b.units
            .saturating_add(ms.saturating_mul(self.rate_per_sec))
            .min(self.full())
    }

    /// Stores `units` for `key`, used at `now`. A new key in a full tier evicts the idle-most.
    fn set(&mut self, key: [u8; 16], units: u64, now: Duration) {
        let bucket = Bucket { units, at: now };
        if let Some(old) = self.buckets.insert(key, bucket) {
            self.by_use.remove(&(old.at, key));
        } else if self.buckets.len() > self.max
            && let Some((_, idle)) = self.by_use.pop_first()
        {
            self.buckets.remove(&idle);
        }
        self.by_use.insert((now, key));
    }

    /// Forgets buckets that are full again: they cost nothing to recreate.
    fn expire(&mut self, now: Duration) {
        let full: Vec<[u8; 16]> = self
            .buckets
            .keys()
            .copied()
            .filter(|&k| self.units(k, now) >= self.full())
            .collect();
        for key in full {
            if let Some(b) = self.buckets.remove(&key) {
                self.by_use.remove(&(b.at, key));
            }
        }
    }
}

/// Token buckets per source and, for IPv6, per /48 (rule 20).
#[derive(Debug)]
pub(crate) struct RateLimit {
    sources: Tier,
    slash48: Tier,
}

impl Default for RateLimit {
    fn default() -> Self {
        Self {
            sources: Tier::new(REGISTER_RATE_PER_SEC, REGISTER_BURST),
            slash48: Tier::new(SLASH48_RATE_PER_SEC, SLASH48_BURST),
        }
    }
}

impl RateLimit {
    /// Spends one `REGISTER` for `ip`. False when its source or its /48 has no room;
    /// then nothing is written to either tier.
    pub(crate) fn allow(&mut self, ip: IpAddr, now: Duration) -> bool {
        let src = source_key(ip);
        let src_units = self.sources.units(src, now);
        let wide = slash48_key(ip).map(|k| (k, self.slash48.units(k, now)));
        let (Some(src_left), wide_left) = (
            src_units.checked_sub(UNIT),
            wide.map(|(k, units)| (k, units.checked_sub(UNIT))),
        ) else {
            return false;
        };
        // A refusal writes nothing: `units` refills lazily, so no tier is touched or evicted.
        if wide_left.is_some_and(|(_, left)| left.is_none()) {
            return false;
        }
        self.sources.set(src, src_left, now);
        if let Some((k, Some(left))) = wide_left {
            self.slash48.set(k, left, now);
        }
        true
    }

    /// Forgets buckets that are full again, in both tiers.
    pub(crate) fn expire(&mut self, now: Duration) {
        self.sources.expire(now);
        self.slash48.expire(now);
    }

    #[cfg(test)]
    pub(crate) fn set_max_sources(&mut self, max: usize) {
        self.sources.max = max;
        self.slash48.max = max;
    }

    #[cfg(test)]
    pub(crate) fn sources(&self) -> usize {
        self.sources.buckets.len()
    }
}

#[cfg(test)]
mod tests;
