//! `latticed`: the upgradable worker daemon. Composition root only.
//!
//! Owner spec: `docs/architecture/04-worker.md`. Loads the node key, binds QUIC,
//! connects to `latticed-proc`, installs the cached app packages, and runs the worker
//! host. It ships no app (D44): a deck sends the package of an app the worker lacks, and
//! the loader here verifies it, compiles its runner (`lattice-wasm`), and caches it in
//! `$TMPDIR/lattice/packages/<hash>.lpkg` so restore finds it after a restart. A restart never kills processes: they live in `latticed-proc`, and
//! runners rebuild from its journals.
//!
//! Experiment mode (M1, before pairing): QUIC direct on a loopback address only.
//! Any deck on this machine may connect. The worker writes `<node id hex>@<addr>`
//! to `$TMPDIR/lattice/worker.addr` so a local deck can find it.

use std::io::Read;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Context;
use clap::Parser;
use lattice_app::PackageHash;
use lattice_key::{NodeKey, SecretKeyBytes};
use lattice_package::Package;
use lattice_quic::{QuicConfig, QuicTransport};
use lattice_wasm::{Limits, WasmApp};
use lattice_worker::{AppsRunner, Installed, PackageLoader, PeerPolicy, ProcClient, WorkerHost};

/// Extension of a cached package file.
const PACKAGE_EXT: &str = "lpkg";

/// Command-line arguments.
#[derive(Debug, Parser)]
#[command(version, about)]
struct Args {
    /// Node key file. Created on first run. Default: `$TMPDIR/lattice/worker.key`.
    #[arg(long)]
    key: Option<PathBuf>,
    /// Loopback UDP address to listen on (experiment mode refuses any other).
    #[arg(long, default_value = "127.0.0.1:7441")]
    listen: SocketAddr,
    /// `latticed-proc` socket. Default: `$LATTICE_PROC_SOCK`, else the runtime dir.
    #[arg(long)]
    proc_sock: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let args = Args::parse();
    let dir = runtime_dir()?;
    let key = load_or_create_key(&args.key.unwrap_or_else(|| dir.join("worker.key")))?;
    let id = key.node_id();
    let cfg = QuicConfig::direct(key, args.listen).map_err(|e| anyhow::anyhow!(e.code()))?;
    let transport = QuicTransport::bind(cfg).await?;
    let addr = transport.local_addr()?;
    let card = format!("{}@{addr}", hex(id.as_bytes()));
    std::fs::write(dir.join("worker.addr"), &card).context("write worker.addr")?;
    tracing::info!(%card, "worker listening (experiment: loopback only)");
    let proc_path = args.proc_sock.unwrap_or_else(ProcClient::default_path);
    let proc = ProcClient::connect(&proc_path).await?;
    let engine = lattice_wasm::engine().map_err(|e| anyhow::anyhow!(e.code()))?;
    let cache = private_dir(&dir.join("packages"))?;
    let loader = package_loader(engine, cache.clone());
    let apps = cached_packages(&cache, &loader);
    WorkerHost::new(
        transport,
        proc,
        PeerPolicy::allow_any_loopback(),
        apps,
        loader,
    )
    .run()
    .await?;
    Ok(())
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

/// Every package in the cache, installed. A file that does not load is skipped and logged.
fn cached_packages(cache: &Path, loader: &PackageLoader) -> AppsRunner {
    let mut apps = AppsRunner::new();
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

/// `$TMPDIR/lattice` (created, mode 0700), shared with `latticed-proc` and the deck.
fn runtime_dir() -> anyhow::Result<PathBuf> {
    private_dir(&std::env::temp_dir().join("lattice"))
}

/// Creates `dir` (mode 0700) if missing. TBD: a data dir that survives reboots for packages.
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

/// Lowercase hex. TBD: canonical text form of a NodeId (09-platform-l1.md).
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
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

        let apps = cached_packages(&cache, &loader);
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
