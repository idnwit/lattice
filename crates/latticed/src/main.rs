//! `latticed`: the upgradable worker daemon. Composition root only.
//!
//! Owner spec: `docs/architecture/04-worker.md`. Loads the node key, binds QUIC,
//! connects to `latticed-proc`, installs the cached app packages, and runs the worker
//! host. It ships no app (D44): a deck sends the package of an app the worker lacks, and
//! the loader here verifies it, compiles its runner (`lattice-wasm`), and caches it in
//! `<data dir>/worker/packages/<hash>.lpkg` so restore finds it after a restart. A restart
//! never kills processes: they live in `latticed-proc`, and runners rebuild from its journals.
//!
//! M1 (direct only): QUIC on `0.0.0.0:7441`; only decks in the allowlist `worker/decks`
//! may connect (specs/01-pairing.md, rules 12–14). The worker writes `<node id>@<addr>` to
//! `worker.addr` in the per-user runtime dir, so `latticed add-deck` finds the listen port.
//!
//! `latticed add-deck [<invite>]` (what `lattice add-deck` runs): the worker side of pairing.
//! It dials the deck with the worker key and adds the deck to the allowlist. Without the
//! argument it reads the invite from stdin, which `ps` does not show (specs/01-pairing.md,
//! CLI rules 2–3).
//!
//! Files (specs/01-pairing.md, Files): `<data dir>/worker/{node.key, decks, packages/}`.

// Binaries may print (.rules/engineering.md, Lints).
#![allow(clippy::print_stdout)]

mod packages;
mod runtime;

use runtime::runtime_dir;

use std::io::Read;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Context;
use clap::{Parser, Subcommand};
use lattice_key::{NodeKey, SecretKeyBytes};
use lattice_quic::{QuicConfig, QuicTransport};
use lattice_worker::{BootEpoch, PeerPolicy, ProcClient, WorkerCard, WorkerHost};

/// Overrides the data dir (specs/01-pairing.md, Files).
const DATA_DIR_ENV: &str = "LATTICE_DATA_DIR";

/// Command-line arguments.
#[derive(Debug, Parser)]
#[command(version, about)]
struct Args {
    /// UDP address to listen on.
    #[arg(long, default_value = "0.0.0.0:7441")]
    listen: SocketAddr,
    /// `latticed-proc` socket. Default: `$LATTICE_PROC_SOCK`, else the runtime dir.
    #[arg(long)]
    proc_sock: Option<PathBuf>,
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Debug, Subcommand)]
enum Cmd {
    /// Pair with the deck that made this invite (`lattice add-deck`; specs/01-pairing.md).
    AddDeck {
        /// The invite code (`lattice1:…`) the deck shows. Omit it to read the code from the
        /// first line of stdin: other local users see arguments in `ps`.
        invite: Option<String>,
        /// This worker's name, shown on the deck. Default: the host name.
        #[arg(long)]
        name: Option<String>,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let args = Args::parse();
    let files = private_dir(&data_dir()?.join("worker"))?;
    match args.cmd {
        Some(Cmd::AddDeck { invite, name }) => {
            let invite = match invite {
                Some(code) => code,
                None => read_invite().await?,
            };
            add_deck(&files, &invite, name).await
        }
        None => {
            let proc_sock = args
                .proc_sock
                .unwrap_or_else(|| ProcClient::default_path(runtime::uid()));
            serve(&files, &runtime_dir()?, args.listen, &proc_sock).await
        }
    }
}

/// Runs the worker daemon until the transport closes. `run` is the runtime dir.
async fn serve(
    files: &Path,
    run: &Path,
    listen: SocketAddr,
    proc_sock: &Path,
) -> anyhow::Result<()> {
    // Versions of this boot start here, above every version of an earlier boot
    // (06-apps.md, Sync to viewers rules 10–12).
    let epoch = BootEpoch::from_unix_millis(unix_millis());
    // Start order (04-worker.md, `latticed`): proc first; worker.addr only once serving.
    let proc = ProcClient::connect(proc_sock).await.with_context(|| {
        format!(
            "no latticed-proc at {}: start it first",
            proc_sock.display()
        )
    })?;
    let key = load_or_create_key(&files.join("node.key"))?;
    let id = key.node_id();
    let transport = QuicTransport::bind(QuicConfig::direct_lan(key, listen)).await?;
    let card = format!("{id}@{}", transport.local_addr()?);
    let engine = lattice_wasm::engine().map_err(|e| anyhow::anyhow!(e.code()))?;
    let cache = private_dir(&files.join("packages"))?;
    let loader = packages::package_loader(engine, cache.clone());
    let apps = packages::cached_packages(&cache, &loader, epoch);
    let policy = PeerPolicy::allow_file(files.join("decks"));
    let host = WorkerHost::new(transport, proc, policy, apps, loader);
    runtime::publish_card(run, &card)?;
    tracing::info!(%card, "worker listening; allowlisted decks only");
    host.run().await?;
    Ok(())
}

/// `latticed add-deck <invite>`: dials the deck with the worker key, proves the invite, and
/// adds the deck to the allowlist (specs/01-pairing.md, rules 3–9).
async fn add_deck(files: &Path, code: &str, name: Option<String>) -> anyhow::Result<()> {
    let invite = lattice_worker::Invite::decode(code)
        .map_err(|e| anyhow::anyhow!("bad invite: {}", e.code()))?;
    let port = runtime::listen_port(&runtime_dir()?)?;
    let ip =
        lattice_quic::advertise_ip(Some(invite.addr().ip())).context("no route to the deck")?;
    let me = WorkerCard {
        name: name.unwrap_or_else(host_name),
        addr: SocketAddr::new(ip, port),
    };
    let key = load_or_create_key(&files.join("node.key"))?;
    let bind = SocketAddr::new(unspecified(invite.addr()), 0);
    let transport = QuicTransport::bind(QuicConfig::direct_lan(key, bind)).await?;
    transport.add_peer(invite.deck(), invite.addr());
    println!(
        "pairing with deck {} at {}…",
        short(invite.deck()),
        invite.addr()
    );
    let deck = lattice_worker::add_deck(&transport, &invite, &me, &files.join("decks"), unix_now())
        .await
        .map_err(|e| anyhow::anyhow!("pairing failed: {}", e.code()))?;
    println!(
        "paired with deck \"{}\" ({}); it reaches this worker at {}",
        deck.name,
        short(deck.id),
        me.addr
    );
    Ok(())
}

/// Longest stdin line read as an invite: a code is about 140 characters.
const INVITE_LINE_MAX: u64 = 1024;

/// The invite from the first line of stdin (specs/01-pairing.md, CLI rule 3).
async fn read_invite() -> anyhow::Result<String> {
    use std::io::BufRead;
    let line = tokio::task::spawn_blocking(|| {
        let mut line = String::new();
        std::io::stdin()
            .lock()
            .take(INVITE_LINE_MAX)
            .read_line(&mut line)
            .map(|_| line)
    })
    .await
    .context("read stdin")?
    .context("read the invite from stdin")?;
    let code = line.trim();
    anyhow::ensure!(
        !code.is_empty(),
        "no invite: pass it as an argument or on stdin"
    );
    Ok(code.to_owned())
}

/// `0.0.0.0` or `[::]`, the family of `peer`.
fn unspecified(peer: SocketAddr) -> std::net::IpAddr {
    match peer {
        SocketAddr::V4(_) => std::net::Ipv4Addr::UNSPECIFIED.into(),
        SocketAddr::V6(_) => std::net::Ipv6Addr::UNSPECIFIED.into(),
    }
}

/// The first 8 hex digits of a NodeId, for messages.
fn short(id: lattice_key::NodeId) -> String {
    id.to_string().chars().take(8).collect()
}

/// The host name (`hostname -s`), else `worker`.
fn host_name() -> String {
    std::process::Command::new("hostname")
        .arg("-s")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "worker".to_owned())
}

/// Seconds since the Unix epoch.
fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default()
}

/// Milliseconds since the Unix epoch: the boot epoch of this run.
fn unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or_default()
}

/// The data dir (specs/01-pairing.md, Files): `$LATTICE_DATA_DIR`, else the OS data dir.
/// `lattice-deck-core` callers resolve the same dir.
fn data_dir() -> anyhow::Result<PathBuf> {
    if let Some(dir) = std::env::var_os(DATA_DIR_ENV) {
        return Ok(PathBuf::from(dir));
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .context("no HOME")?;
    if cfg!(target_os = "macos") {
        return Ok(home.join("Library/Application Support/lattice"));
    }
    Ok(match std::env::var_os("XDG_DATA_HOME") {
        Some(xdg) => PathBuf::from(xdg).join("lattice"),
        None => home.join(".local/share/lattice"),
    })
}

/// Creates `dir` (mode 0700) if missing.
pub(crate) fn private_dir(dir: &Path) -> anyhow::Result<PathBuf> {
    use std::os::unix::fs::DirBuilderExt;
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)
        .with_context(|| format!("create {}", dir.display()))?;
    Ok(dir.to_owned())
}

/// Reads the node key, or creates it with mode 0600 on first run. Never logs the bytes.
fn load_or_create_key(path: &Path) -> anyhow::Result<NodeKey> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut bytes = [0u8; 32];
    if let Ok(mut f) = std::fs::File::open(path) {
        f.read_exact(&mut bytes).context("read node key")?;
    } else {
        std::fs::File::open("/dev/urandom")
            .and_then(|mut r| r.read_exact(&mut bytes))
            .context("read /dev/urandom")?;
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
            .context("create node key")?;
        std::io::Write::write_all(&mut f, &bytes).context("write node key")?;
    }
    Ok(NodeKey::from_secret(SecretKeyBytes::new(bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 04-worker.md, `latticed` start order.
    #[tokio::test]
    async fn a_missing_proc_daemon_is_named_and_publishes_no_worker_addr() {
        let dir = std::env::temp_dir().join(format!("latticed-noproc-{}", std::process::id()));
        drop(std::fs::remove_dir_all(&dir));
        let files = private_dir(&dir.join("worker")).unwrap();
        let run = private_dir(&dir.join("run")).unwrap();
        let sock = dir.join("missing.sock");
        let listen = "127.0.0.1:0".parse().unwrap();
        let err = serve(&files, &run, listen, &sock).await.unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains(&sock.display().to_string()), "msg={msg}");
        assert!(!run.join("worker.addr").exists(), "no stale worker.addr");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
