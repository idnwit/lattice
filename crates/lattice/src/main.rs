//! `lattice` CLI.
//!
//! `lattice probe`: experiment client (M1). Connects to the local worker over
//! QUIC, creates a `terminal` instance, resizes it once through the typed client
//! (printing the `Size` it returns), types a command, and prints the screen
//! state it receives. Proves deck ⇄ runner ⇄ proc end to end without a GUI.
//!
//! `lattice probe-echo`: creates an `echo` instance, sends one line through the
//! typed client, and prints the returned seq and the lines it receives as state.
//! Proves an app with no process end to end (D40).

// Binaries may print (.rules/engineering.md, Lints).
#![allow(clippy::print_stdout)]

use std::io::Read;
use std::time::Duration;

use anyhow::{Context, bail};
use clap::{Parser, Subcommand};
use lattice_app::{AppId, Ticket};
use lattice_app_echo::{EchoApiClient, EchoParams, EchoState, Message};
use lattice_app_terminal::{
    Input, Resize, Size, TermError, TermParams, TermState, TerminalApiClient,
};
use lattice_deck_core::{DeckHost, Replica, StateUpdate, WorkerLink};
use lattice_key::{NodeId, NodeKey, SecretKeyBytes};
use lattice_quic::{QuicConfig, QuicConn, QuicTransport};

/// Columns of the probe terminal.
const PROBE_COLS: u16 = 100;
/// Rows of the probe terminal.
const PROBE_ROWS: u16 = 30;

/// Command-line arguments.
#[derive(Debug, Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Debug, Subcommand)]
enum Cmd {
    /// Open a terminal on the local worker, run `command`, print the screen.
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
    /// Open an echo instance on the local worker, send `text`, print the lines.
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
    match Cli::parse().cmd {
        Cmd::Probe { command, exit, iid } => probe(&command, exit, iid).await,
        Cmd::ProbeEcho { text } => probe_echo(&text).await,
    }
}

/// Opens a link to the local worker and prints its path.
async fn open_link() -> anyhow::Result<WorkerLink<QuicConn>> {
    let (worker, addr) = worker_card()?;
    let cfg = QuicConfig::direct(random_key()?, "127.0.0.1:0".parse()?)
        .map_err(|e| anyhow::anyhow!(e.code()))?;
    let transport = QuicTransport::bind(cfg).await?;
    transport.add_peer(worker, addr);
    let link = DeckHost::new(transport).open_link(worker).await?;
    println!("link: {}", link.path());
    Ok(link)
}

async fn probe(command: &str, exit: bool, attach: Option<u64>) -> anyhow::Result<()> {
    let mut link = open_link().await?;

    let home = std::env::var("HOME").unwrap_or_default();
    let params = lattice_state::encode(&TermParams::new(home).with_size(PROBE_COLS, PROBE_ROWS))?;
    let app = AppId::try_new(lattice_app_terminal::APP_ID)?;
    let iid = match attach {
        Some(raw) => lattice_app::InstanceId::from_raw(raw),
        None => {
            let package = builtin(&app)?;
            link.create(app, package, params, "probe".to_owned())
                .await?
        }
    };
    println!("instance: {iid}");
    link.subscribe(format!("inst/{iid}/state").parse()?).await?;

    let client = TerminalApiClient::new(link.caller(iid));
    let mut resized = Some(client.resize(Resize::new(PROBE_COLS, PROBE_ROWS)));
    link.flush_calls().await?;

    let mut replica = Replica::<TermState>::default();
    let mut typed = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(if exit { 6 } else { 3 });
    while let Ok(msg) = tokio::time::timeout_at(deadline, link.next_message()).await {
        let msg = msg?;
        report_resize(&mut resized);
        let Some((_, update)) = StateUpdate::from_message(msg, &link.worker()) else {
            continue;
        };
        update.apply_to(&mut replica);
        // The runner binds the shell on its first output; type only after the prompt shows.
        if !typed && has_prompt(replica.state()) {
            typed = true;
            // Not awaited: the shell's echo arrives as state.
            drop(client.input(Input::new(typed_line(command, exit))));
            link.flush_calls().await?;
        }
    }
    print_screen(replica.state(), replica.version());
    Ok(())
}

async fn probe_echo(text: &str) -> anyhow::Result<()> {
    let mut link = open_link().await?;
    let params = lattice_state::encode(&EchoParams::default())?;
    let app = AppId::try_new(lattice_app_echo::APP_ID)?;
    let package = builtin(&app)?;
    let iid = link
        .create(app, package, params, "probe".to_owned())
        .await?;
    println!("instance: {iid}");
    link.subscribe(format!("inst/{iid}/state").parse()?).await?;

    let client = EchoApiClient::new(link.caller(iid));
    let mut ticket = client.send(Message::new(text));
    link.flush_calls().await?;

    let mut replica = Replica::<EchoState>::default();
    let mut seq = None;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    while let Ok(msg) = tokio::time::timeout_at(deadline, link.next_message()).await {
        let msg = msg?;
        if let Some(outcome) = ticket.try_take() {
            let got = outcome.map_err(|e| anyhow::anyhow!("send failed: {e}"))?;
            println!("send: seq {got}");
            seq = Some(got);
        }
        if let Some((_, update)) = StateUpdate::from_message(msg, &link.worker()) {
            update.apply_to(&mut replica);
        }
        let shown = |s| replica.state().lines().iter().any(|l| l.get().seq() == s);
        if seq.is_some_and(shown) {
            break;
        }
    }
    println!("--- lines (version {:?}) ---", replica.version());
    for line in replica.state().lines() {
        println!("{} {}", line.get().seq(), line.get().text());
    }
    Ok(())
}

/// The built-in package of `app`: its hash and bytes, sent if the worker lacks it (D44).
fn builtin(app: &AppId) -> anyhow::Result<(lattice_app::PackageHash, &'static [u8])> {
    let p = lattice_builtin_apps::by_id(app.as_str()).context("no built-in package")?;
    Ok((p.package(), p.bytes))
}

/// Prints the outcome of the probe's `resize` once it settles.
fn report_resize(ticket: &mut Option<Ticket<Size, TermError>>) {
    if let Some(outcome) = ticket.as_mut().and_then(Ticket::try_take) {
        *ticket = None;
        match outcome {
            Ok(size) => println!("resize: {}x{}", size.cols(), size.rows()),
            Err(e) => println!("resize failed: {e}"),
        }
    }
}

/// True once any screen row shows text.
fn has_prompt(state: &TermState) -> bool {
    state
        .screen()
        .iter()
        .any(|r| !r.get().text().trim().is_empty())
}

/// `command`, Enter, and `exit` + Enter when asked.
fn typed_line(command: &str, exit: bool) -> Vec<u8> {
    let mut line = command.as_bytes().to_vec();
    line.extend_from_slice(b"\r");
    if exit {
        line.extend_from_slice(b"exit\r");
    }
    line
}

fn print_screen(state: &TermState, version: Option<u64>) {
    println!(
        "--- screen (version {version:?}, title {:?}) ---",
        state.title()
    );
    let rows: Vec<String> = state.screen().iter().map(|r| r.get().text()).collect();
    let last = rows.iter().rposition(|r| !r.trim().is_empty()).unwrap_or(0);
    for row in rows.iter().take(last.saturating_add(1)) {
        println!("{row}");
    }
    println!(
        "--- scrollback rows: {} ---",
        state.scrollback().iter().count()
    );
}

/// `<node id hex>@<addr>` written by `latticed` in `$TMPDIR/lattice/worker.addr`.
fn worker_card() -> anyhow::Result<(NodeId, std::net::SocketAddr)> {
    let path = std::env::temp_dir().join("lattice").join("worker.addr");
    let card = std::fs::read_to_string(&path).with_context(|| format!("read {path:?}"))?;
    let Some((id, addr)) = card.trim().split_once('@') else {
        bail!("bad worker card");
    };
    let mut bytes = [0u8; 32];
    if id.len() != 64 {
        bail!("bad node id length");
    }
    for (i, b) in bytes.iter_mut().enumerate() {
        let pair = id.get(i * 2..i * 2 + 2).context("bad node id")?;
        *b = u8::from_str_radix(pair, 16)?;
    }
    Ok((NodeId::try_from_bytes(bytes)?, addr.parse()?))
}

/// A fresh deck key per run (experiment: the worker allows any loopback deck).
fn random_key() -> anyhow::Result<NodeKey> {
    let mut bytes = [0u8; 32];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(NodeKey::from_secret(SecretKeyBytes::new(bytes)))
}
