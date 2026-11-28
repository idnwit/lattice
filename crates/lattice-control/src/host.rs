//! Canonical relay hosts (specs/01-pairing.md, Contact) and `host:port` pairs.

use std::fmt;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::str::FromStr;

/// Longest DNS name, in bytes (specs/01-pairing.md, Contact).
const MAX_NAME: usize = 253;
/// Longest DNS label, in bytes.
const MAX_LABEL: usize = 63;

/// A host in canonical form: a DNS name or an IP literal (specs/01-pairing.md, Contact).
///
/// - DNS name: LDH labels of 1–63 bytes, no leading or trailing hyphen, lower case, no
///   trailing dot, at most 253 bytes. A last label of only digits (`127.1`) is refused.
/// - IP literal: IPv4 dotted decimal without leading zeros, or IPv6 in RFC 5952 form.
///
/// Two hosts are equal when their canonical forms are equal.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Host(HostKind);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum HostKind {
    Name(String),
    Ip(IpAddr),
}

/// A host is not in canonical form (`config.invalid_value` at the config layer).
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("host is not canonical")]
pub struct HostError;

impl Host {
    /// The IP literal, when the host is one.
    fn ip(&self) -> Option<IpAddr> {
        match &self.0 {
            HostKind::Ip(ip) => Some(*ip),
            HostKind::Name(_) => None,
        }
    }
}

impl FromStr for Host {
    type Err = HostError;

    /// Accepts only the canonical form; never rewrites one (`Example.com` is refused).
    fn from_str(s: &str) -> Result<Self, HostError> {
        if s.contains(':') {
            let ip: Ipv6Addr = s.parse().map_err(|_| HostError)?;
            // RFC 5952: std prints the canonical text form.
            return (ip.to_string() == s)
                .then_some(Self(HostKind::Ip(IpAddr::V6(ip))))
                .ok_or(HostError);
        }
        if let Ok(ip) = s.parse::<Ipv4Addr>() {
            return (ip.to_string() == s)
                .then_some(Self(HostKind::Ip(IpAddr::V4(ip))))
                .ok_or(HostError);
        }
        dns_name(s)
            .then(|| Self(HostKind::Name(s.to_owned())))
            .ok_or(HostError)
    }
}

/// LDH, lower case, no trailing dot, last label not all digits.
fn dns_name(s: &str) -> bool {
    if s.is_empty() || s.len() > MAX_NAME {
        return false;
    }
    let label_ok = |l: &str| {
        !l.is_empty()
            && l.len() <= MAX_LABEL
            && !l.starts_with('-')
            && !l.ends_with('-')
            && l.bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    };
    let last_numeric = s
        .rsplit('.')
        .next()
        .is_some_and(|l| l.bytes().all(|b| b.is_ascii_digit()));
    s.split('.').all(label_ok) && !last_numeric
}

impl fmt::Display for Host {
    /// The canonical form, IPv6 without brackets.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            HostKind::Name(n) => f.write_str(n),
            HostKind::Ip(ip) => write!(f, "{ip}"),
        }
    }
}

/// A canonical host and a non-zero port: what pins and relay URLs compare by
/// (specs/10-settings.md, rules 29 and 37).
///
/// Text form `host:port`; an IPv6 host is bracketed (`[2001:db8::1]:7400`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HostPort {
    host: Host,
    port: u16,
}

impl HostPort {
    /// The host.
    pub fn host(&self) -> &Host {
        &self.host
    }

    /// The UDP port, never 0.
    pub fn port(&self) -> u16 {
        self.port
    }
}

impl FromStr for HostPort {
    type Err = HostError;

    fn from_str(s: &str) -> Result<Self, HostError> {
        let (host, port) = match s.strip_prefix('[') {
            Some(rest) => {
                let (v6, port) = rest.split_once("]:").ok_or(HostError)?;
                // A bracket holds an IPv6 literal only.
                v6.contains(':').then_some(()).ok_or(HostError)?;
                (v6, port)
            }
            None => {
                let (host, port) = s.rsplit_once(':').ok_or(HostError)?;
                // An IPv6 host needs brackets.
                (!host.contains(':')).then_some(()).ok_or(HostError)?;
                (host, port)
            }
        };
        // Canonical port: decimal, no sign, no leading zero. Port 0 cannot be dialed.
        let canonical = !port.starts_with('0') && port.bytes().all(|b| b.is_ascii_digit());
        let port: u16 = port.parse().ok().filter(|_| canonical).ok_or(HostError)?;
        Ok(Self {
            host: host.parse()?,
            port,
        })
    }
}

impl fmt::Display for HostPort {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.host.ip() {
            Some(IpAddr::V6(ip)) => write!(f, "[{ip}]:{}", self.port),
            _ => write!(f, "{}:{}", self.host, self.port),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_hosts_are_accepted() {
        for case in [
            "relay.example.com",
            "localhost",
            "a-b.c0",
            "10.0.0.1",
            "2001:db8::1",
            "::1",
        ] {
            let host: Host = case.parse().unwrap_or_else(|_| panic!("case={case}"));
            assert_eq!(host.to_string(), case, "case={case}");
        }
    }

    #[test]
    fn contact_rejects_a_non_canonical_host() {
        let too_long = format!("{}.com", "a.".repeat(126));
        for case in [
            "",
            "Relay.example.com",
            "relay.example.com.",
            "-a.com",
            "a-.com",
            "a..com",
            "a_b.com",
            "127.1",
            "01.2.3.4",
            "1.2.3.04",
            "2001:DB8::1",
            "2001:db8:0:0:0:0:0:1",
            &too_long,
            "[::1]",
        ] {
            assert_eq!(case.parse::<Host>(), Err(HostError), "case={case}");
        }
    }

    #[test]
    fn host_port_text_roundtrips_and_brackets_ipv6() {
        for case in ["relay.example.com:7400", "10.0.0.1:1", "[2001:db8::1]:7400"] {
            let hp: HostPort = case.parse().unwrap_or_else(|_| panic!("case={case}"));
            assert_eq!(hp.to_string(), case, "case={case}");
        }
        for case in [
            "relay.example.com",
            "relay.example.com:0",
            "relay.example.com:07400",
            "relay.example.com:+7400",
            "relay.example.com:65536",
            "2001:db8::1:7400",
            "[relay.example.com]:7400",
        ] {
            assert_eq!(case.parse::<HostPort>(), Err(HostError), "case={case}");
        }
    }
}
