//! `lattice` CLI.
//!
//! Pairing (specs/01-pairing.md, D47):
//!
//! - `lattice invite`: this machine acts as a deck. Prints `lattice add-deck <invite>`,
//!   waits for the worker to call back, and saves it to `<data dir>/deck/workers`.
//!   It holds `deck/invite.lock` meanwhile (one open invite per data dir).
//! - `lattice add-deck <invite>`: on a worker. Runs `latticed add-deck`, which holds the
//!   worker key and the allowlist, and hands it the invite on stdin: `ps` shows argv.
//!
//! The deck commands below use the deck key and the most recently paired worker
//! (`--worker` picks another, by name or NodeId prefix).
//!
//! `lattice probe`: experiment client (M1). Connects to a paired worker over
//! QUIC, creates a `terminal` instance, resizes it once through the typed client
//! (printing the `Size` it returns), types a command, and prints the screen
//! state it receives. Proves deck ⇄ runner ⇄ proc end to end without a GUI.
//!
//! `lattice attach`: a `terminal` instance in the local terminal (specs/03-sessions.md,
//! Attach). Creates one, or `--iid` attaches to an existing one. `Ctrl-\` detaches.
//!
//! `lattice probe-echo`: creates an `echo` instance, sends one line through the
//! typed client, and prints the returned seq and the lines it receives as state.
//! Proves an app with no process end to end (D40).

// Binaries may print (.rules/engineering.md, Lints).
#![allow(clippy::print_stdout)]

mod attach;
mod probe;

use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Context;
use clap::{Parser, Subcommand};
use lattice_app::AppId;
use lattice_deck_core::{DeckFiles, DeckHost, PairedWorker, PendingInvite, WorkerLink};
use lattice_quic::{QuicConfig, QuicConn, QuicTransport};

/// Longest invite code `lattice add-deck` passes on: far below a pipe buffer.
const INVITE_ARG_MAX: usize = 1024;

/// Overrides the data dir (specs/01-pairing.md, Files).
const DATA_DIR_ENV: &str = "LATTICE_DATA_DIR";

/// Command-line arguments.
#[derive(Debug, Parser)]
#[command(version, about)]
struct Cli {
    /// Paired worker to use: name or NodeId prefix. Default: the most recently paired.
    #[arg(long, global = true)]
    worker: Option<String>,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Debug, Subcommand)]
enum Cmd {
    /// Deck: make an invite, print the command to run on the worker, wait for it to pair.
    Invite {
        /// This deck's name, shown on the worker. Default: the host name.
        #[arg(long)]
        name: Option<String>,
    },
    /// Worker: pair with the deck that made this invite (runs `latticed add-deck`).
    AddDeck {
        /// The invite code (`lattice1:…`).
        invite: String,
        /// This worker's name, shown on the deck. Default: the host name.
        #[arg(long)]
        name: Option<String>,
    },
    /// Attach the local terminal to a terminal instance on the worker. `Ctrl-\` detaches.
    Attach {
        /// Attach to this existing instance instead of creating one.
        #[arg(long)]
        iid: Option<u64>,
    },
    /// Open a terminal on the worker, run `command`, print the screen.
    Probe {
        /// Command typed into the shell.
        #[arg(default_value = "echo hello from lattice")]
        command: String,
        /// Type `exit` afterwards and wait for the instance to finish.
        #[arg(long)]
        exit: bool,
        /// Attach to this existing instance instead of creating one.
        #[arg(long)]
        iid: Option<u64>,
    },
    /// Open an echo instance on the worker, send `text`, print the lines.
    ProbeEcho {
        /// Text of the line to send.
        #[arg(default_value = "hello from lattice")]
        text: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let cli = Cli::parse();
    let worker = cli.worker.as_deref();
    match cli.cmd {
        Cmd::Invite { name } => invite(name).await,
        Cmd::AddDeck { invite, name } => add_deck(&invite, name),
        Cmd::Attach { iid } => attach::attach(worker, iid).await,
        Cmd::Probe { command, exit, iid } => probe::probe(worker, &command, exit, iid).await,
        Cmd::ProbeEcho { text } => probe::probe_echo(worker, &text).await,
    }
}

/// `lattice invite`: the deck side of pairing (specs/01-pairing.md, rules 1–11, CLI rule 1).
async fn invite(name: Option<String>) -> anyhow::Result<()> {
    let files = deck_files()?;
    // Rule 2: one open invite per data dir, shared with the deck GUI.
    let _lock = files
        .lock_invite()
        .map_err(|e| anyhow::anyhow!("{}", e.code()))?;
    let key = files.node_key().context("deck key")?;
    let deck = key.node_id();
    let transport = QuicTransport::bind(QuicConfig::direct_lan(key, "0.0.0.0:0".parse()?)).await?;
    let ip = lattice_quic::advertise_ip(None).context("no network route")?;
    let addr = std::net::SocketAddr::new(ip, transport.local_addr()?.port());
    let secret = lattice_deck_core::random_secret()?;
    let mut pending = PendingInvite::new(deck, addr, secret, unix_now());
    println!(
        "Run this on the worker (expires in {} min, one use):\n\n  {}\n",
        lattice_deck_core::INVITE_TTL_SECS / 60,
        pending.command()
    );
    println!("waiting for worker…");
    let ttl = Duration::from_secs(lattice_deck_core::INVITE_TTL_SECS);
    let name = name.unwrap_or_else(host_name);
    let save = |w: &PairedWorker| {
        let mut list = files.workers().map_err(|e| e.kind().to_string())?;
        list.add(w.clone());
        files.save_workers(&list).map_err(|e| e.kind().to_string())
    };
    // Rule 10: each worker's exchange gets `PAIR_TIMEOUT`.
    let sleep = || tokio::time::sleep(lattice_deck_core::PAIR_TIMEOUT);
    let accept = lattice_deck_core::accept_pair(
        &transport,
        &mut pending,
        &name,
        unix_now,
        save,
        |_| {},
        sleep,
    );
    let paired = tokio::time::timeout(ttl, accept)
        .await
        .map_err(|_| anyhow::anyhow!("pair.expired"))?
        .map_err(|e| anyhow::anyhow!("pairing failed: {}", deck_code(&e)))?;
    println!(
        "paired · {} at {} ({})",
        paired.name,
        paired.addr,
        short(&paired)
    );
    Ok(())
}

/// The code the CLI prints: a denial's own code, else the error's.
fn deck_code(e: &lattice_deck_core::DeckError) -> String {
    match e {
        lattice_deck_core::DeckError::Remote(code) => code.clone(),
        other => other.code().to_owned(),
    }
}

/// `lattice add-deck`: runs `latticed add-deck` (next to this binary, else on `PATH`),
/// which holds the worker key and the allowlist (specs/01-pairing.md, CLI rule 2).
///
/// The invite holds the secret, so it goes to `latticed` on stdin, never in argv: every
/// local user sees argv in `ps`. The code is below the pipe buffer, so the write never
/// blocks before `exec`.
fn add_deck(code: &str, name: Option<String>) -> anyhow::Result<()> {
    use std::io::Write;
    use std::os::unix::process::CommandExt;
    let sibling = std::env::current_exe()
        .ok()
        .map(|exe| exe.with_file_name("latticed"))
        .filter(|p| p.is_file());
    let mut cmd = std::process::Command::new(sibling.unwrap_or_else(|| "latticed".into()));
    cmd.arg("add-deck");
    if let Some(name) = name {
        cmd.arg("--name").arg(name);
    }
    let code = code.trim();
    // A real code is about 140 characters; a longer one could fill the pipe and block.
    anyhow::ensure!(code.len() <= INVITE_ARG_MAX, "pair.invite_malformed");
    let (reader, mut writer) = std::io::pipe().context("pipe")?;
    writeln!(writer, "{code}").context("write the invite to the pipe")?;
    drop(writer);
    cmd.stdin(reader);
    Err(cmd.exec()).context("run latticed add-deck")
}

/// Opens a link to the chosen paired worker and prints its path.
async fn open_link(worker: Option<&str>) -> anyhow::Result<WorkerLink<QuicConn>> {
    let files = deck_files()?;
    let list = files.workers().context("worker list")?;
    let chosen = match worker {
        Some(key) => list
            .find(key)
            .with_context(|| format!("no paired worker {key:?}"))?,
        None => list
            .latest()
            .context("no paired worker: run `lattice invite` first")?,
    };
    let key = files.node_key().context("deck key")?;
    let bind = std::net::SocketAddr::new(unspecified(chosen.addr), 0);
    let transport = QuicTransport::bind(QuicConfig::direct_lan(key, bind)).await?;
    transport.add_peer(chosen.id, chosen.addr);
    let link = DeckHost::new(transport).open_link(chosen.id).await?;
    println!("link: {} · {}", chosen.name, link.path());
    Ok(link)
}

/// The built-in package of `app`: its hash and bytes, sent if the worker lacks it (D44).
fn builtin(app: &AppId) -> anyhow::Result<(lattice_app::PackageHash, &'static [u8])> {
    let p = lattice_builtin_apps::by_id(app.as_str()).context("no built-in package")?;
    Ok((p.package(), p.bytes))
}

/// `<data dir>/deck`.
fn deck_files() -> anyhow::Result<DeckFiles> {
    DeckFiles::at(data_dir()?.join("deck")).context("deck data dir")
}

/// The data dir (specs/01-pairing.md, Files): `$LATTICE_DATA_DIR`, else the OS data dir.
/// `latticed` and the deck resolve the same dir.
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

/// `0.0.0.0` or `[::]`, the family of `peer`.
fn unspecified(peer: std::net::SocketAddr) -> std::net::IpAddr {
    match peer {
        std::net::SocketAddr::V4(_) => std::net::Ipv4Addr::UNSPECIFIED.into(),
        std::net::SocketAddr::V6(_) => std::net::Ipv6Addr::UNSPECIFIED.into(),
    }
}

/// The first 8 hex digits of a worker's NodeId.
fn short(w: &PairedWorker) -> String {
    w.id.to_string().chars().take(8).collect()
}

/// The host name (`hostname -s`), else `deck`.
fn host_name() -> String {
    std::process::Command::new("hostname")
        .arg("-s")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "deck".to_owned())
}

/// Seconds since the Unix epoch.
fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default()
}
