//! `latticed`: the upgradable worker daemon. Composition root only.
//!
//! Owner spec: `docs/architecture/04-worker.md`. Loads the node key, binds QUIC,
//! connects to `latticed-proc`, installs the cached app packages, and runs the worker
//! host. It ships no app (D44): a deck sends the package of an app the worker lacks, and
//! the loader here verifies it, compiles its runner (`lattice-wasm`), and caches it in
//! `$TMPDIR/lattice/packages/<hash>.lpkg` so restore finds it after a restart. A restart never kills processes: they live in `latticed-proc`, and
//! runners rebuild from its journals.
//!
//! M1 (direct only): QUIC on `0.0.0.0:7441`; only decks in the allowlist `worker/decks`
//! may connect (specs/01-pairing.md, rules 12–14). The worker writes `<node id>@<addr>` to
//! `$TMPDIR/lattice/worker.addr`, so `latticed add-deck` finds the listen port.
//!
//! `latticed add-deck <invite>` (what `lattice add-deck` runs): the worker side of pairing.
//! It dials the deck with the worker key and adds the deck to the allowlist.
//!
//! Files (specs/01-pairing.md, Files): `<data dir>/worker/{node.key, decks, packages/}`.

// Binaries may print (.rules/engineering.md, Lints).
#![allow(clippy::print_stdout)]

use std::io::Read;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Context;
use clap::{Parser, Subcommand};
use lattice_app::PackageHash;
use lattice_key::{NodeKey, SecretKeyBytes};
use lattice_package::Package;
use lattice_quic::{QuicConfig, QuicTransport};
use lattice_wasm::{Limits, WasmApp};
use lattice_worker::{
    AppsRunner, BootEpoch, Installed, PackageLoader, PeerPolicy, ProcClient, WorkerCard, WorkerHost,
};

/// Extension of a cached package file.
const PACKAGE_EXT: &str = "lpkg";

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
        /// The invite code (`lattice1:…`) the deck shows.
        invite: String,
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
        Some(Cmd::AddDeck { invite, name }) => add_deck(&files, &invite, name).await,
        None => serve(&files, args.listen, args.proc_sock).await,
    }
}

/// Runs the worker daemon until the transport closes.
async fn serve(files: &Path, listen: SocketAddr, proc_sock: Option<PathBuf>) -> anyhow::Result<()> {
    // Versions of this boot start here, above every version of an earlier boot
    // (06-apps.md, Sync to viewers rules 10–12).
    let epoch = BootEpoch::from_unix_millis(unix_millis());
    let run = runtime_dir()?;
    let key = load_or_create_key(&files.join("node.key"))?;
    let id = key.node_id();
    let transport = QuicTransport::bind(QuicConfig::direct_lan(key, listen)).await?;
    let addr = transport.local_addr()?;
    let card = format!("{id}@{addr}");
    std::fs::write(run.join("worker.addr"), &card).context("write worker.addr")?;
    tracing::info!(%card, "worker listening; allowlisted decks only");
    let proc_path = proc_sock.unwrap_or_else(ProcClient::default_path);
    let proc = ProcClient::connect(&proc_path).await?;
    let engine = lattice_wasm::engine().map_err(|e| anyhow::anyhow!(e.code()))?;
    let cache = private_dir(&files.join("packages"))?;
    let loader = package_loader(engine, cache.clone());
    let apps = cached_packages(&cache, &loader, epoch);
    let policy = PeerPolicy::allow_file(files.join("decks"));
    WorkerHost::new(transport, proc, policy, apps, loader)
        .run()
        .await?;
    Ok(())
}

/// `latticed add-deck <invite>`: dials the deck with the worker key, proves the invite, and
/// adds the deck to the allowlist (specs/01-pairing.md, rules 3–9).
async fn add_deck(files: &Path, code: &str, name: Option<String>) -> anyhow::Result<()> {
    let invite = lattice_worker::Invite::decode(code)
        .map_err(|e| anyhow::anyhow!("bad invite: {}", e.code()))?;
    let port = listen_port()?;
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

/// The port the running `latticed` listens on, from `worker.addr`.
fn listen_port() -> anyhow::Result<u16> {
    let path = runtime_dir()?.join("worker.addr");
    let card = std::fs::read_to_string(&path)
        .with_context(|| format!("latticed is not running here (no {})", path.display()))?;
    let addr = card
        .trim()
        .rsplit_once('@')
        .map(|(_, a)| a)
        .unwrap_or_default();
    let addr: SocketAddr = addr.parse().context("bad worker.addr")?;
    Ok(addr.port())
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

/// Verifies a package, compiles its runner, and caches the bytes (best effort).
///
/// TBD: compile off the core task (it blocks the event loop while cranelift runs).
fn package_loader(engine: lattice_wasm::Engine, cache: PathBuf) -> PackageLoader {
    Arc::new(move |hash, bytes| {
        let installed = load_package(&engine, hash, bytes)?;
        let file = cache.join(format!("{hash}.{PACKAGE_EXT}"));
        if let Err(e) = write_atomic(&file, bytes) {
            tracing::warn!(%hash, error = %e, "package not cached; restore after a restart will fail");
        }
        Ok(installed)
    })
}

/// Package bytes → an installed app with a WASM runner.
fn load_package(
    engine: &lattice_wasm::Engine,
    hash: &PackageHash,
    bytes: &[u8],
) -> Result<Installed, String> {
    let package = Package::verify(hash, bytes).map_err(|e| e.code().to_owned())?;
    let app = WasmApp::load(engine, &package.runner, Limits::default())
        .map_err(|e| e.code().to_owned())?;
    let runner = Arc::new(move |params: &[u8], mode| app.instantiate(params, mode));
    Ok(Installed::new(package.manifest, runner))
}

/// A runner at `epoch` with every package in the cache installed. A file that does not load
/// is skipped and logged.
fn cached_packages(cache: &Path, loader: &PackageLoader, epoch: BootEpoch) -> AppsRunner {
    let mut apps = AppsRunner::new(epoch);
    let Ok(entries) = std::fs::read_dir(cache) else {
        return apps;
    };
    for path in entries.filter_map(|e| e.ok().map(|e| e.path())) {
        let hash = path
            .file_stem()
            .and_then(|s| s.to_str())
            .filter(|_| path.extension().is_some_and(|e| e == PACKAGE_EXT))
            .and_then(parse_hash);
        let Some(hash) = hash else { continue };
        match std::fs::read(&path)
            .map_err(|e| e.to_string())
            .and_then(|b| loader(&hash, &b))
        {
            Ok(installed) => apps.install(hash, installed),
            Err(code) => tracing::warn!(path = %path.display(), code, "cached package skipped"),
        }
    }
    apps
}

/// Writes `bytes` to `path` through a temporary file and a rename.
fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(tmp, path)
}

/// 64 lowercase hex digits → a package hash.
fn parse_hash(text: &str) -> Option<PackageHash> {
    if text.len() != 64 {
        return None;
    }
    let mut raw = [0u8; 32];
    for (i, b) in raw.iter_mut().enumerate() {
        *b = u8::from_str_radix(text.get(i * 2..i * 2 + 2)?, 16).ok()?;
    }
    Some(PackageHash::from_bytes(raw))
}

/// `$TMPDIR/lattice` (created, mode 0700), shared with `latticed-proc`.
fn runtime_dir() -> anyhow::Result<PathBuf> {
    private_dir(&std::env::temp_dir().join("lattice"))
}

/// Creates `dir` (mode 0700) if missing.
fn private_dir(dir: &Path) -> anyhow::Result<PathBuf> {
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

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("latticed-test-{name}-{}", std::process::id()));
        drop(std::fs::remove_dir_all(&dir));
        private_dir(&dir).unwrap()
    }

    #[test]
    fn a_built_in_package_loads_caches_and_reloads_from_the_cache() {
        let engine = lattice_wasm::engine().unwrap();
        let cache = scratch("cache");
        let loader = package_loader(engine, cache.clone());
        let echo = lattice_builtin_apps::by_id("dev.lattice.echo").unwrap();
        let installed = loader(&echo.package(), echo.bytes).unwrap();
        assert_eq!(installed.manifest().id.as_str(), echo.id);
        let file = cache.join(format!("{}.{PACKAGE_EXT}", echo.package()));
        assert!(file.is_file(), "the package is cached by hash");

        let apps = cached_packages(&cache, &loader, BootEpoch::from_unix_millis(0));
        assert!(apps.has_package(&echo.package()));
        std::fs::remove_dir_all(cache).unwrap();
    }

    #[test]
    fn a_package_under_the_wrong_hash_does_not_load() {
        let engine = lattice_wasm::engine().unwrap();
        let echo = lattice_builtin_apps::by_id("dev.lattice.echo").unwrap();
        let other = lattice_builtin_apps::by_id("dev.lattice.terminal").unwrap();
        let err = load_package(&engine, &other.package(), echo.bytes).unwrap_err();
        assert_eq!(err, "package.hash_mismatch");
        assert!(load_package(&engine, &echo.package(), echo.bytes).is_ok());
    }

    #[test]
    fn hashes_parse_from_their_file_names_only() {
        let hash = PackageHash::from_bytes([0xab; 32]);
        assert_eq!(parse_hash(&hash.to_string()), Some(hash));
        assert_eq!(parse_hash("abc"), None);
        assert_eq!(parse_hash(&"zz".repeat(32)), None);
    }
}
