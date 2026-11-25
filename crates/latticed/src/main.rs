//! `latticed`: the upgradable worker daemon. Composition root only.
//!
//! Owner spec: `docs/architecture/04-worker.md`. Loads the node key, binds the
//! magic socket, connects to `latticed-proc`, and runs the worker host.
//! A restart never kills processes: they live in `latticed-proc`.

use clap::Parser;
use lattice_key::NodeKey;
use lattice_quic::{QuicConfig, QuicTransport};
use lattice_transport::HomeRelay;
use lattice_worker::{PeerPolicy, ProcClient, WorkerHost};
use std::net::SocketAddr;
use std::path::PathBuf;

/// Command-line arguments.
///
/// TBD: config file format and location (relay, allowed decks) instead of flags?
#[derive(Debug, Parser)]
#[command(version, about)]
struct Args {
    /// Node key file. Created on first run.
    #[arg(long)]
    key: PathBuf,
    /// Home relay UDP address.
    #[arg(long)]
    relay: SocketAddr,
    /// `latticed-proc` socket. Default: `$XDG_RUNTIME_DIR/lattice/proc.sock`.
    #[arg(long)]
    proc_sock: Option<PathBuf>,
    /// Local UDP bind address.
    #[arg(long, default_value = "0.0.0.0:0")]
    bind: SocketAddr,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let args = Args::parse();
    let key = load_or_create_key(&args.key)?;
    let transport =
        QuicTransport::bind(QuicConfig::new(key, home_relay(&args)?, args.bind)).await?;
    let proc = ProcClient::connect(&proc_socket(&args)?).await?;
    WorkerHost::new(transport, proc, allowed_decks()?)
        .run()
        .await?;
    Ok(())
}

/// Reads the node key, or creates it with 0600 permissions on first run.
#[allow(
    clippy::todo,
    unused_variables,
    reason = "L1 skeleton: body lands with its contract test"
)]
fn load_or_create_key(path: &std::path::Path) -> anyhow::Result<NodeKey> {
    // exists: read 32 bytes → SecretKeyBytes → NodeKey
    // missing: 32 bytes from the OS RNG; write with mode 0600; never log the bytes
    todo!()
}

/// The relay address plus its key.
#[allow(
    clippy::todo,
    unused_variables,
    reason = "L1 skeleton: body lands with its contract test"
)]
fn home_relay(args: &Args) -> anyhow::Result<HomeRelay> {
    // TBD: where the relay NodeId comes from (pairing code carries the relay URL; 03, Pairing)
    todo!()
}

/// The proc daemon socket path.
#[allow(
    clippy::todo,
    unused_variables,
    reason = "L1 skeleton: body lands with its contract test"
)]
fn proc_socket(args: &Args) -> anyhow::Result<PathBuf> {
    // args.proc_sock, else $XDG_RUNTIME_DIR/lattice/proc.sock (TBD on macOS)
    todo!()
}

/// Decks allowed to connect. L1 stand-in for the device list.
#[allow(
    clippy::todo,
    reason = "L1 skeleton: body lands with its contract test"
)]
fn allowed_decks() -> anyhow::Result<PeerPolicy> {
    // TBD: read the device list once lattice-identity lands; until then a local allow-list file
    todo!()
}
