//! Which local IP a peer reaches us at (specs/01-pairing.md, Addresses).

use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};

/// Overrides the detected IP (specs/01-pairing.md, Addresses).
pub const ADVERTISE_ENV: &str = "LATTICE_ADVERTISE_IP";

/// Destination for the default-route lookup: TEST-NET-1 (RFC 5737), port discard.
/// `connect` on a UDP socket only picks a route; no packet is sent.
const DEFAULT_ROUTE_PROBE: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1)), 9);

/// The IP of the interface that routes to `toward` (the default route for `None`),
/// unless [`ADVERTISE_ENV`] names one.
///
/// # Errors
///
/// The OS error when there is no route, or the override does not parse.
pub fn advertise_ip(toward: Option<IpAddr>) -> std::io::Result<IpAddr> {
    if let Ok(text) = std::env::var(ADVERTISE_ENV) {
        return text.trim().parse().map_err(|_| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "bad LATTICE_ADVERTISE_IP")
        });
    }
    let dest = toward.map_or(DEFAULT_ROUTE_PROBE, |ip| SocketAddr::new(ip, 9));
    let bind: SocketAddr = match dest {
        SocketAddr::V4(_) => (Ipv4Addr::UNSPECIFIED, 0).into(),
        SocketAddr::V6(_) => (std::net::Ipv6Addr::UNSPECIFIED, 0).into(),
    };
    let socket = UdpSocket::bind(bind)?;
    socket.connect(dest)?;
    Ok(socket.local_addr()?.ip())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_route_to_loopback_is_loopback() {
        // Hermetic: only the loopback route is used, and the override is unset in tests.
        if std::env::var(ADVERTISE_ENV).is_ok() {
            return;
        }
        let ip = advertise_ip(Some(IpAddr::V4(Ipv4Addr::LOCALHOST))).unwrap();
        assert!(ip.is_loopback(), "ip={ip}");
    }
}
