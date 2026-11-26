//! `lattice attach`: a `terminal` instance in the local terminal (specs/03-sessions.md, Attach).
//!
//! I/O loop only. The decisions live in sans-IO modules: [`keys`] (stdin bytes, detach,
//! `worker/apps` presence) and [`paint`] (frames to escape sequences).

mod keys;
mod paint;
mod tty;

use std::io::Read;
use std::time::Duration;

use anyhow::Context;
use lattice_app::{AppId, InstanceId, InstanceRecord, ReplicaEvent, Ticket};
use lattice_app_terminal::{
    Input, Resize, Run, TermError, TermParams, TermState, TerminalApiClient,
};
use lattice_deck_core::{Replica, StateUpdate, WorkerLink};
use lattice_quic::QuicConn;
use tokio::io::AsyncWriteExt;
use tokio::signal::unix::{SignalKind, signal};
use tokio::sync::mpsc;

use keys::{Gone, Keys, Origin, Presence};
use paint::Painter;
use tty::Tty;

/// Session name of a created instance (rule 4). TBD: session id type (InstanceRecord).
const SESSION: &str = "attach";
/// Stdin reads queued ahead of the link.
const STDIN_QUEUE: usize = 64;
/// Bytes per stdin read.
const STDIN_CHUNK: usize = 4096;
/// How long a detach waits for the bytes typed before the key to land (rule 8).
const DETACH_FLUSH: Duration = Duration::from_secs(2);

/// How an attach ends without an error.
enum End {
    /// Detach key or stdin closed (rules 8, 9).
    Detached,
    /// The instance left `worker/apps` (rule 10).
    Exited,
    /// `--iid` names no instance (rule 2).
    Missing,
}

/// Runs `lattice attach` until detach, instance exit, or error.
pub(crate) async fn attach(worker: Option<&str>, iid: Option<u64>) -> anyhow::Result<()> {
    let tty = Tty::check()?;
    let size = tty.size();
    let mut link = super::open_link(worker).await?;
    let (iid, origin) = match iid {
        Some(raw) => (InstanceId::from_raw(raw), Origin::Existing),
        None => (create(&mut link, size).await?, Origin::Created),
    };
    link.subscribe("worker/apps".parse()?).await?;
    link.subscribe(format!("inst/{iid}/state").parse()?).await?;
    let end = {
        let _raw = tty.raw()?;
        Attached::new(link, iid, origin, tty).run().await
    };
    match end? {
        End::Detached => println!("detached · instance {iid}"),
        End::Exited => println!("instance {iid} exited"),
        End::Missing => anyhow::bail!("no instance {iid}"),
    }
    Ok(())
}

/// Creates a `terminal` instance at `(cols, rows)` in the worker's default cwd (rule 4).
async fn create(
    link: &mut WorkerLink<QuicConn>,
    (cols, rows): (u16, u16),
) -> anyhow::Result<InstanceId> {
    let app = AppId::try_new(lattice_app_terminal::APP_ID)?;
    let package = super::builtin(&app)?;
    let params = lattice_state::encode(&TermParams::default().with_size(cols, rows))?;
    Ok(link
        .create(app, package, params, SESSION.to_owned())
        .await?)
}

/// One attached instance: link, replica, and local screen.
struct Attached {
    link: WorkerLink<QuicConn>,
    iid: InstanceId,
    client: TerminalApiClient<lattice_deck_core::LinkCaller>,
    replica: Replica<TermState>,
    presence: Presence,
    painter: Painter,
    tty: Tty,
    size: (u16, u16),
    out: tokio::io::Stdout,
}

impl Attached {
    fn new(link: WorkerLink<QuicConn>, iid: InstanceId, origin: Origin, tty: Tty) -> Self {
        let size = tty.size();
        Self {
            client: TerminalApiClient::new(link.caller(iid)),
            link,
            iid,
            replica: Replica::default(),
            presence: Presence::new(origin),
            painter: Painter::new(size.1),
            tty,
            size,
            out: tokio::io::stdout(),
        }
    }

    async fn run(mut self) -> anyhow::Result<End> {
        let mut stdin = spawn_stdin();
        let mut winch = signal(SignalKind::window_change()).context("watch SIGWINCH")?;
        let worker = self.link.worker();
        self.resize().await?;
        loop {
            let end = tokio::select! {
                msg = self.link.next_message() => self.on_update(StateUpdate::from_message(msg?, &worker)).await?,
                read = stdin.recv() => self.on_stdin(read).await?,
                _ = winch.recv() => self.on_winch().await?,
            };
            if let Some(end) = end {
                return Ok(end);
            }
        }
    }

    /// One `State` stream update from the worker; other messages arrive as `None`.
    async fn on_update<S: Into<String>>(
        &mut self,
        found: Option<(S, StateUpdate)>,
    ) -> anyhow::Result<Option<End>> {
        let Some((stream, update)) = found else {
            return Ok(None);
        };
        let stream: String = stream.into();
        if stream == "worker/apps" {
            return Ok(self.on_apps(&update));
        }
        if stream != format!("inst/{}/state", self.iid) {
            return Ok(None);
        }
        match update.apply_to(&mut self.replica) {
            ReplicaEvent::Changed => self.draw().await?,
            ReplicaEvent::Resync => self.link.subscribe(stream.parse()?).await?,
        }
        Ok(None)
    }

    /// `worker/apps` syncs full lists only (06-apps.md, Instances rule 4).
    fn on_apps(&mut self, update: &StateUpdate) -> Option<End> {
        let StateUpdate::Full { bytes, .. } = update else {
            return None;
        };
        let list = InstanceRecord::decode_list(bytes)?;
        let listed = list.iter().any(|r| r.iid() == self.iid);
        self.presence.on_list(listed).map(|gone| match gone {
            Gone::Exited => End::Exited,
            Gone::Missing => End::Missing,
        })
    }

    async fn on_stdin(&mut self, read: Option<Vec<u8>>) -> anyhow::Result<Option<End>> {
        let Some(read) = read else {
            return Ok(Some(End::Detached));
        };
        match keys::keys(&read) {
            Keys::Input(bytes) => {
                // Not awaited: the shell's echo arrives as state.
                drop(self.input(bytes).await?);
                Ok(None)
            }
            Keys::Detach { before: [] } => Ok(Some(End::Detached)),
            Keys::Detach { before } => {
                let ticket = self.input(before).await?;
                self.settle(ticket).await;
                Ok(Some(End::Detached))
            }
        }
    }

    async fn on_winch(&mut self) -> anyhow::Result<Option<End>> {
        let size = self.tty.size();
        if size != self.size {
            self.size = size;
            self.painter.resize(size.1);
            self.resize().await?;
            self.draw().await?;
        }
        Ok(None)
    }

    /// Sends `input`; the ticket settles in [`WorkerLink::next_message`].
    async fn input(&mut self, bytes: &[u8]) -> anyhow::Result<Ticket<(), TermError>> {
        let ticket = self.client.input(Input::new(bytes.to_vec()));
        self.link.flush_calls().await?;
        Ok(ticket)
    }

    /// Sends `resize` with the local size (rule 5). Not awaited: the size shows in state.
    async fn resize(&mut self) -> anyhow::Result<()> {
        let (cols, rows) = self.size;
        drop(self.client.resize(Resize::new(cols, rows)));
        Ok(self.link.flush_calls().await?)
    }

    /// Waits up to [`DETACH_FLUSH`] for `ticket`, so the bytes reach the worker before exit.
    async fn settle(&mut self, mut ticket: Ticket<(), TermError>) {
        let wait = async {
            while ticket.try_take().is_none() {
                if self.link.next_message().await.is_err() {
                    return;
                }
            }
        };
        // On timeout the bytes may still land; detach goes on either way.
        drop(tokio::time::timeout(DETACH_FLUSH, wait).await);
    }

    async fn draw(&mut self) -> anyhow::Result<()> {
        let state = self.replica.state();
        let rows: Vec<&[Run]> = state.screen().iter().map(|r| r.get().runs()).collect();
        let frame = self.painter.paint(&rows, state.cursor());
        if !frame.is_empty() {
            self.out.write_all(&frame).await?;
            self.out.flush().await?;
        }
        Ok(())
    }
}

/// Reads stdin on its own thread: a blocking read must not hold a runtime worker. The thread
/// ends with the process.
fn spawn_stdin() -> mpsc::Receiver<Vec<u8>> {
    let (tx, rx) = mpsc::channel(STDIN_QUEUE);
    std::thread::spawn(move || {
        let mut stdin = std::io::stdin().lock();
        let mut buf = [0u8; STDIN_CHUNK];
        while let Ok(n) = stdin.read(&mut buf) {
            let Some(read) = buf.get(..n).filter(|r| !r.is_empty()) else {
                break;
            };
            if tx.blocking_send(read.to_vec()).is_err() {
                break;
            }
        }
    });
    rx
}
