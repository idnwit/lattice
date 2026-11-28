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
//! Files (specs/01-pairing.md, Files): `<data dir>/worker/{node.key, decks, packages/,
//! config.toml, latticed.sock}`. One `latticed` per data dir: it holds the `flock` on
//! `worker/` for its whole life and serves the control socket (specs/11-control-socket.md).
//! It is the only writer of `config.toml` while it runs (specs/10-settings.md, rule 34).

// Binaries may print (.rules/engineering.md, Lints).
#![allow(clippy::print_stdout)]

mod config;
mod control;
mod packages;
mod relay;
mod runtime;

use runtime::runtime_dir;

use std::io::Read;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Context;
use clap::{Parser, Subcommand};
use lattice_control::{ConfigFile, DirLock, Key, Layers, LockError, RELAY_ENV};
use lattice_key::{NodeKey, SecretKeyBytes};
use lattice_quic::{QuicConfig, QuicTransport};
use lattice_worker::{BootEpoch, PeerPolicy, ProcClient, WorkerCard, WorkerHost};

/// Overrides the data dir (specs/01-pairing.md, Files).
const DATA_DIR_ENV: &str = "LATTICE_DATA_DIR";

/// Command-line arguments.
#[derive(Debug, Parser)]
#[command(version, about)]
struct Args {
    /// UDP address to listen on (`listen.addr`; default `0.0.0.0:7441`).
    #[arg(long)]
    listen: Option<String>,
    /// Home relay URL `udp://[<id>@]host:port` or `none` (`relay.addr`; beats
    /// `$LATTICE_RELAY` and the config file).
    #[arg(long)]
    relay: Option<String>,
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
    let worker = data_dir()?.join("worker");
    match args.cmd {
        Some(Cmd::AddDeck { invite, name }) => {
            let files = private_dir(&worker)?;
            let invite = match invite {
                Some(code) => code,
                None => read_invite().await?,
            };
            add_deck(&files, &invite, name).await
        }
        None => {
            let layers = layers(args.relay.as_deref(), args.listen.as_deref())?;
            let proc_sock = args
                .proc_sock
                .unwrap_or_else(|| ProcClient::default_path(runtime::uid()));
            serve(&worker, &runtime_dir()?, layers, &proc_sock).await
        }
    }
}

/// The flag and environment layers of the worker config (specs/10-settings.md, rule 33).
fn layers(relay: Option<&str>, listen: Option<&str>) -> anyhow::Result<Layers> {
    let mut layers = Layers::new(host_name());
    if let Some(value) = std::env::var_os(RELAY_ENV) {
        let value = value
            .to_str()
            .with_context(|| format!("config.invalid_value: ${RELAY_ENV} is not UTF-8"))?;
        layers = layers
            .env(Key::RelayAddr, value)
            .map_err(|e| anyhow::anyhow!("{e} (${RELAY_ENV})"))?;
    }
    for (key, flag, value) in [
        (Key::RelayAddr, "--relay", relay),
        (Key::ListenAddr, "--listen", listen),
    ] {
        if let Some(value) = value {
            layers = layers
                .flag(key, value)
                .map_err(|e| anyhow::anyhow!("{e} ({flag})"))?;
        }
    }
    Ok(layers)
}

/// Runs the worker daemon until the transport closes. `worker` is `<data dir>/worker`,
/// `run` the runtime dir.
async fn serve(worker: &Path, run: &Path, layers: Layers, proc_sock: &Path) -> anyhow::Result<()> {
    // Versions of this boot start here, above every version of an earlier boot
    // (06-apps.md, Sync to viewers rules 10–12).
    let epoch = BootEpoch::from_unix_millis(unix_millis());
    // Start order (04-worker.md, `latticed`; specs/11-control-socket.md, rule 1).
    // 1. Lock: one latticed per data dir, held until the process ends.
    let lock = DirLock::acquire(worker, runtime::uid()).map_err(lock_error)?;
    // An invalid config file fails the start (specs/10-settings.md, rule 36).
    let store = Arc::new(config::ConfigStore::load(
        config::config_path(worker),
        layers,
        lock,
    )?);
    // 2. Key.
    let key = load_or_create_key(&worker.join("node.key"))?;
    // 3. Control socket: served from now on; config requests need nothing below.
    let listener = control::bind(&worker.join(lattice_control::SOCKET_FILE))?;
    let handler = {
        let store = store.clone();
        Arc::new(move |req| store.handle(req))
    };
    tokio::spawn(control::serve(listener, runtime::uid(), handler));
    tokio::spawn(watch_config(store.clone()));
    // 4. latticed-proc; worker.addr only once serving.
    let proc = ProcClient::connect(proc_sock).await.with_context(|| {
        format!(
            "no latticed-proc at {}: start it first",
            proc_sock.display()
        )
    })?;
    // 5. QUIC, at the configured home relay or direct.
    let id = key.node_id();
    let resolved = store.current();
    let transport = QuicTransport::bind(relay::quic_config(key, &resolved).await?).await?;
    tokio::spawn(relay::follow(
        store.subscribe(),
        transport.home_relay_handle(),
        resolved,
    ));
    let card = format!("{id}@{}", transport.local_addr()?);
    let engine = lattice_wasm::engine().map_err(|e| anyhow::anyhow!(e.code()))?;
    // 6. Install the cached packages.
    let cache = private_dir(&worker.join("packages"))?;
    let loader = packages::package_loader(engine, cache.clone());
    let apps = packages::cached_packages(&cache, &loader, epoch);
    let policy = PeerPolicy::allow_file(worker.join("decks"));
    let host = WorkerHost::new(transport, proc, policy, apps, loader);
    // 7. Serve.
    runtime::publish_card(run, &card)?;
    tracing::info!(%card, "worker listening; allowlisted decks only");
    host.run().await?;
    Ok(())
}

/// `worker.already_running` when another process holds the `worker/` lock (rule 1).
fn lock_error(e: LockError) -> anyhow::Error {
    match e {
        LockError::Locked => {
            anyhow::anyhow!("worker.already_running: another latticed holds this data dir")
        }
        other => anyhow::Error::new(other),
    }
}

/// Polls the config file and applies valid changes (specs/10-settings.md, rule 35).
async fn watch_config(store: Arc<config::ConfigStore>) {
    loop {
        tokio::time::sleep(config::CONFIG_POLL).await;
        let store = store.clone();
        if let Err(e) = tokio::task::spawn_blocking(move || store.reload()).await {
            tracing::error!(error = %e, "config reload failed");
        }
    }
}

/// `name` from the environment, the file, or the host name (specs/10-settings.md, Worker
/// config table). Read only: `add-deck` runs beside the daemon.
fn configured_name(files: &Path) -> anyhow::Result<String> {
    let path = config::config_path(files);
    let bytes = lattice_control::read_or_empty(&path)
        .with_context(|| format!("read {}", path.display()))?;
    let text = std::str::from_utf8(&bytes).context("config.parse: not UTF-8")?;
    let file = ConfigFile::parse(text).map_err(|e| anyhow::anyhow!("{e}: {}", path.display()))?;
    Ok(layers(None, None)?.resolve(&file).name().to_owned())
}

/// `latticed add-deck <invite>`: dials the deck with the worker key, proves the invite, and
/// adds the deck to the allowlist (specs/01-pairing.md, rules 3–9).
async fn add_deck(files: &Path, code: &str, name: Option<String>) -> anyhow::Result<()> {
    let invite = lattice_worker::Invite::decode(code)
        .map_err(|e| anyhow::anyhow!("bad invite: {}", e.code()))?;
    let port = runtime::listen_port(&runtime_dir()?)?;
    let ip =
        lattice_quic::advertise_ip(Some(invite.addr().ip())).context("no route to the deck")?;
    let name = match name {
        Some(name) => name,
        None => configured_name(files)?,
    };
    let me = WorkerCard {
        name,
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

/// Reads the node key, or creates it with mode 0600 when the file is missing. A file that
/// is not exactly 32 bytes fails with `worker.bad_key` and is never overwritten
/// (specs/11-control-socket.md, rule 1 step 2). Never logs the bytes.
fn load_or_create_key(path: &Path) -> anyhow::Result<NodeKey> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut bytes = [0u8; 32];
    match std::fs::read(path) {
        Ok(found) => {
            bytes = <[u8; 32]>::try_from(found.as_slice()).map_err(|_| {
                anyhow::anyhow!(
                    "worker.bad_key: {} is not 32 bytes; restore it, or delete it to make a new identity (then pair again)",
                    path.display()
                )
            })?;
        }
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
            return Err(e).with_context(|| format!("read {}", path.display()));
        }
        Err(_) => {
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
        let layers = Layers::new("h".into())
            .flag(Key::ListenAddr, "127.0.0.1:0")
            .unwrap();
        let err = serve(&files, &run, layers, &sock).await.unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains(&sock.display().to_string()), "msg={msg}");
        assert!(!run.join("worker.addr").exists(), "no stale worker.addr");
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// specs/11-control-socket.md, Invariant 5.
    #[tokio::test]
    async fn a_second_latticed_exits_already_running() {
        let dir = std::env::temp_dir().join(format!("latticed-twice-{}", std::process::id()));
        drop(std::fs::remove_dir_all(&dir));
        let files = private_dir(&dir.join("worker")).unwrap();
        let _first = DirLock::acquire(&files, runtime::uid()).unwrap();
        let layers = Layers::new("h".into());
        let err = serve(&files, &dir, layers, &dir.join("p.sock"))
            .await
            .unwrap_err();
        assert!(
            format!("{err}").starts_with("worker.already_running"),
            "err={err}"
        );
        assert!(!files.join("node.key").exists(), "nothing past step 1");
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// specs/11-control-socket.md, Invariant 5: the lock, not timing, picks one daemon.
    #[test]
    fn two_concurrent_latticed_starts_leave_one_running() {
        let dir = std::env::temp_dir().join(format!("latticed-race-{}", std::process::id()));
        drop(std::fs::remove_dir_all(&dir));
        let files = dir.join("worker");
        let results: Vec<Result<DirLock, LockError>> = std::thread::scope(|s| {
            let starts: Vec<_> = (0..2)
                .map(|_| s.spawn(|| DirLock::acquire(&files, runtime::uid())))
                .collect();
            starts.into_iter().map(|t| t.join().unwrap()).collect()
        });
        let running = results.iter().filter(|r| r.is_ok()).count();
        let refused = results
            .into_iter()
            .filter_map(Result::err)
            .map(|e| format!("{}", lock_error(e)))
            .collect::<Vec<_>>();
        assert_eq!(running, 1);
        assert!(
            refused[0].starts_with("worker.already_running"),
            "{refused:?}"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_node_key_of_another_size_is_bad_and_kept() {
        let dir = std::env::temp_dir().join(format!("latticed-badkey-{}", std::process::id()));
        drop(std::fs::remove_dir_all(&dir));
        let files = private_dir(&dir).unwrap();
        let path = files.join("node.key");
        for len in [0usize, 31, 33] {
            std::fs::write(&path, vec![1u8; len]).unwrap();
            let err = load_or_create_key(&path).unwrap_err();
            assert!(format!("{err}").starts_with("worker.bad_key"), "len={len}");
            assert_eq!(
                std::fs::read(&path).unwrap().len(),
                len,
                "never overwritten"
            );
        }
        std::fs::remove_file(&path).unwrap();
        let made = load_or_create_key(&path).unwrap();
        let again = load_or_create_key(&path).unwrap();
        assert_eq!(
            made.node_id(),
            again.node_id(),
            "a missing key is made once"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}
