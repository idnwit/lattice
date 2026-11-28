//! The home relay of `latticed`: the transport mode at start and live relay changes
//! (specs/10-settings.md, rules 35–38; specs/02-connectivity.md, Relay).
//!
//! Only one home relay, as the magic socket holds today (ROADMAP M2.8 adds several
//! registrations and the first-contact pin).

use std::net::SocketAddr;

use anyhow::Context;
use lattice_control::{HostPort, RelayPlan, Resolved};
use lattice_key::NodeKey;
use lattice_quic::{HomeRelayHandle, QuicConfig};
use lattice_transport::HomeRelay;
use tokio::sync::watch;

/// The transport config for `resolved`: relay mode with a known relay id, else direct.
pub(crate) async fn quic_config(key: NodeKey, resolved: &Resolved) -> anyhow::Result<QuicConfig> {
    let listen = resolved.listen();
    Ok(match home_relay(resolved.relay(), listen).await? {
        Some(relay) => QuicConfig::new(key, relay, listen),
        None => QuicConfig::direct_lan(key, listen),
    })
}

/// The relay to register at, `None` for direct mode.
///
/// An unpinned relay cannot register yet: `RelayClient` needs the relay id up front, and
/// the first-contact pin from `COOKIE.relay_id` lands with ROADMAP M2.8. Until then the
/// worker logs `relay.no_id` and serves direct.
async fn home_relay(plan: &RelayPlan, listen: SocketAddr) -> anyhow::Result<Option<HomeRelay>> {
    let RelayPlan::Relay { at, id } = plan else {
        return Ok(None);
    };
    let Some(id) = id.known() else {
        // TBD: first-contact pin (ROADMAP M2.8). Until then pin with udp://<id>@host:port.
        tracing::warn!(
            code = "relay.no_id",
            relay = %at,
            "no relay id: pin it with `lattice config set relay.addr udp://<id>@{at}`; serving direct only"
        );
        return Ok(None);
    };
    let addr = lookup(at, listen).await?;
    tracing::info!(relay = %at, %addr, "home relay");
    Ok(Some(HomeRelay::new(addr, id)))
}

/// The first address of `at` in the family of the listen socket.
///
/// TBD: a name that does not resolve fails the start. Should `latticed` retry instead
/// (a boot before the network is up)?
async fn lookup(at: &HostPort, listen: SocketAddr) -> anyhow::Result<SocketAddr> {
    let host = at.host().to_string();
    let addrs = tokio::net::lookup_host((host.as_str(), at.port()))
        .await
        .with_context(|| format!("resolve relay {at}"))?;
    addrs
        .into_iter()
        .find(|a| a.is_ipv4() == listen.is_ipv4())
        .with_context(|| format!("relay {at} has no address of the listen family"))
}

/// Applies changes of the resolved config that need no restart (rule 35).
///
/// A new relay applies on the running transport. A change of `listen.addr`, or between
/// relay and direct mode, logs `config.restart_needed`: the magic socket carries no direct
/// path until ROADMAP M2.5, so the mode is fixed at bind.
pub(crate) async fn follow(
    mut rx: watch::Receiver<Resolved>,
    handle: Option<HomeRelayHandle>,
    mut running: Resolved,
) {
    while rx.changed().await.is_ok() {
        let next = rx.borrow_and_update().clone();
        if next.listen() != running.listen() {
            tracing::warn!(
                code = "config.restart_needed",
                key = "listen.addr",
                "restart latticed to apply"
            );
        }
        if next.relay() != running.relay() {
            apply_relay(handle.as_ref(), &next).await;
        }
        running = next;
    }
}

async fn apply_relay(handle: Option<&HomeRelayHandle>, next: &Resolved) {
    let relay = match home_relay(next.relay(), next.listen()).await {
        Ok(relay) => relay,
        Err(e) => {
            tracing::warn!(code = "config.restart_needed", error = %format!("{e:#}"), "new relay not applied");
            return;
        }
    };
    match (handle, relay) {
        (Some(handle), Some(relay)) => {
            // `false`: the transport is gone, the daemon is stopping.
            if !handle.set(relay) {
                tracing::debug!("relay change after the transport closed");
            }
        }
        (None, None) => {}
        _ => tracing::warn!(
            code = "config.restart_needed",
            key = "relay.addr",
            "switching between relay and direct mode needs a restart (until ROADMAP M2.5)"
        ),
    }
}
