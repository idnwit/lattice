//! `lattice-relay`: the public UDP relay. Composition root plus its UDP loop.
//!
//! Owner specs: `docs/architecture/01-transport.md` (Relay wire format) and
//! `08-infrastructure.md`. Forwards opaque QUIC packets between registered
//! nodes. Sees NodeIds, addresses, sizes, and timing; never content.
//! All decisions live in the sans-IO `RelayEngine`; this file only moves bytes
//! (`.rules/architecture.md` rule 3: I/O in adapters and binaries).

use clap::Parser;
use lattice_key::NodeKey;
use lattice_relay_proto::{EngineOutput, RelayEngine};
use std::net::SocketAddr;
use std::path::PathBuf;
use tokio::net::UdpSocket;

/// Command-line arguments.
#[derive(Debug, Parser)]
#[command(version, about)]
struct Args {
    /// UDP listen address.
    #[arg(long, default_value = "0.0.0.0:7400")]
    listen: SocketAddr,
    /// Relay key file. Created on first run.
    #[arg(long)]
    key: PathBuf,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let args = Args::parse();
    let key = load_or_create_key(&args.key)?;
    let sock = UdpSocket::bind(args.listen).await?;
    serve(sock, RelayEngine::new(key.node_id(), registration_ttl())).await
}

/// Receives datagrams, runs the engine, sends its output. Expires on a timer.
#[allow(
    clippy::todo,
    unused_variables,
    reason = "L1 skeleton: body lands with its contract test"
)]
async fn serve(sock: UdpSocket, engine: RelayEngine) -> anyhow::Result<()> {
    // loop: select! { recv_from → engine.handle(now, src, bytes, fresh_token), tick → engine.on_tick(now) }
    // now = monotonic Duration since start; fresh_token from the OS RNG
    // each handle() output goes through send_output; send errors are logged and dropped (UDP is best effort)
    send_output(&sock, EngineOutput::Drop).await;
    todo!()
}

/// Sends what the engine decided.
#[allow(
    clippy::todo,
    unused_variables,
    reason = "L1 skeleton: body lands with its contract test"
)]
async fn send_output(sock: &UdpSocket, out: EngineOutput) {
    // Drop: nothing; Reply / Forward: send_to(t.dst()); Punch: both
    todo!()
}

/// Reads the relay key, or creates it on first run.
#[allow(
    clippy::todo,
    unused_variables,
    reason = "L1 skeleton: body lands with its contract test"
)]
fn load_or_create_key(path: &std::path::Path) -> anyhow::Result<NodeKey> {
    // same scheme as latticed (TBD: share via lattice-key helper if a third user appears)
    todo!()
}

/// How long a registration lives without keepalive.
#[allow(
    clippy::todo,
    reason = "L1 skeleton: body lands with its contract test"
)]
fn registration_ttl() -> std::time::Duration {
    // TBD: default ttl (08-infrastructure.md); must exceed KEEPALIVE_INTERVAL (25 s)
    todo!()
}
