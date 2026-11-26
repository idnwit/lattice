//! How a node's transport is bound: relay mode or direct-only mode.

use lattice_key::NodeKey;
use lattice_transport::HomeRelay;
use std::net::SocketAddr;

/// A [`QuicConfig`] was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ConfigError {
    /// Direct-only mode binds loopback addresses only.
    #[error("direct-only mode binds loopback only")]
    NonLoopbackBind,
}

impl ConfigError {
    /// Stable code for logs and the UI.
    pub fn code(&self) -> &'static str {
        match self {
            Self::NonLoopbackBind => "quic.non_loopback_bind",
        }
    }
}

/// Which paths the transport uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    /// Relay first, then hole-punched direct (01-transport.md, Connect sequence).
    Relay(HomeRelay),
    /// Plain UDP to addresses from [`crate::QuicTransport::add_peer`]. Loopback only.
    Direct,
}

/// Everything needed to bind a node's transport.
#[derive(Debug)]
pub struct QuicConfig {
    pub(crate) key: NodeKey,
    pub(crate) mode: Mode,
    pub(crate) bind: SocketAddr,
}

impl QuicConfig {
    /// Node key, home relay, and local UDP bind address.
    pub fn new(key: NodeKey, relay: HomeRelay, bind: SocketAddr) -> Self {
        Self {
            key,
            mode: Mode::Relay(relay),
            bind,
        }
    }

    /// Direct-only mode (ROADMAP M1.5): no relay, no hole punching. Peers are
    /// dialed at the addresses given to [`crate::QuicTransport::add_peer`].
    ///
    /// Security: a worker behind this transport serves a shell to any key
    /// that completes the handshake, until pairing and the allowlist land
    /// (M1.6). So direct mode refuses to bind anything but loopback.
    ///
    /// # Errors
    ///
    /// [`ConfigError::NonLoopbackBind`] when `bind` is not a loopback address
    /// (`0.0.0.0` and `[::]` included).
    pub fn direct(key: NodeKey, bind: SocketAddr) -> Result<Self, ConfigError> {
        if !bind.ip().is_loopback() {
            return Err(ConfigError::NonLoopbackBind);
        }
        Ok(Self {
            key,
            mode: Mode::Direct,
            bind,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_codes_are_stable_and_namespaced() {
        assert_eq!(
            ConfigError::NonLoopbackBind.code(),
            "quic.non_loopback_bind"
        );
    }
}
