//! The pairing task behind the Add worker dialog (specs/01-pairing.md, Invite flow; D47).
//!
//! One task per invite on the NSH executor: bind a QUIC endpoint on `0.0.0.0:0`, make the
//! invite, then serve the worker's callback ([`accept_pair`]) until it pairs or the NSH
//! timer reaches the invite's expiry. A new invite cancels the old task. On success the
//! worker list is saved, reloaded into the rail, and the link switches to the new worker.
//!
//! The command holds the pairing secret: it goes to the dialog and the clipboard, never to
//! a log.

use std::net::{Ipv4Addr, SocketAddr};
use std::time::Duration;

use lattice_deck_core::{
    DeckError, DeckFiles, INVITE_TTL_SECS, PairedWorker, PendingInvite, WorkerList, accept_pair,
    random_secret,
};
use lattice_quic::{QuicConfig, QuicTransport};
use nsh_core::prelude::*;
use tokio::sync::mpsc::UnboundedSender;

use crate::data::{host_name, unix_now};
use crate::invite::{InviteCommand, InviteDialog};
use crate::link::Cmd;

/// What the pairing task reads and writes. `Copy`.
#[derive(Clone, Copy)]
pub(crate) struct Pairing {
    pub(crate) dialog: Signal<InviteDialog>,
    pub(crate) workers: Signal<WorkerList>,
    /// `<data dir>/deck`; `None` when it cannot be made.
    pub(crate) files: Stored<Option<DeckFiles>>,
    /// The link's command queue, told about the new worker.
    pub(crate) cmds: Stored<UnboundedSender<Cmd>>,
    /// The task serving the open invite.
    pub(crate) task: Stored<Option<Spawned<()>>>,
    /// The deck root's owner: the task outlives the view that started it.
    pub(crate) owner: Owner,
}

impl Pairing {
    /// `+`: shows the dialog, making an invite unless one still waits.
    pub(crate) fn open(self) {
        let mut make = None;
        self.dialog.update(|d| make = d.open());
        if let Some(generation) = make {
            self.begin(generation);
        }
    }

    /// Close or `Escape`.
    pub(crate) fn close(self) {
        self.dialog.update(InviteDialog::close);
    }

    /// `New invite`.
    pub(crate) fn renew(self) {
        let mut make = None;
        self.dialog.update(|d| make = d.renew());
        if let Some(generation) = make {
            self.begin(generation);
        }
    }

    /// `Copy`: the command to the system clipboard.
    pub(crate) fn copy(self) {
        let command = self.dialog.with(|d| d.command().map(|c| c.0.clone()));
        if let Some(command) = command {
            // The write happens either way; the answer only says whether the system took it.
            drop(set_clipboard(command));
        }
    }

    /// Starts the task for invite `generation`, cancelling the one before it.
    fn begin(self, generation: u64) {
        let task = self.owner.run(|| {
            spawn(async move {
                let outcome = serve(self, generation).await;
                self.finish(generation, outcome);
            })
        });
        if let Some(old) = self.task.replace(Some(task)) {
            old.cancel();
        }
    }

    fn finish(self, generation: u64, outcome: Result<PairedWorker, String>) {
        match outcome {
            Ok(worker) => {
                tracing::info!(name = %worker.name, "worker paired from the deck");
                let list = self.files.with(|f| f.as_ref().map(DeckFiles::workers));
                if let Some(Ok(list)) = list {
                    self.workers.set(list);
                }
                let name = worker.name.clone();
                let sent = self.cmds.with(|tx| tx.send(Cmd::Worker(worker)).is_ok());
                if !sent {
                    tracing::warn!("link task gone; the new worker links on the next start");
                }
                self.dialog.update(|d| d.paired(generation, name));
            }
            Err(code) if code == "pair.expired" => {
                self.dialog.update(|d| d.expired(generation));
            }
            Err(code) => {
                tracing::info!(code, "invite failed");
                self.dialog.update(|d| d.failed(generation, code));
            }
        }
    }
}

/// Makes invite `generation` and serves it until it pairs, fails, or expires.
async fn serve(pairing: Pairing, generation: u64) -> Result<PairedWorker, String> {
    let files = pairing
        .files
        .get()
        .ok_or_else(|| "deck.data_dir".to_owned())?;
    let key = files.node_key().map_err(|_| "deck.key".to_owned())?;
    let deck = key.node_id();
    let bind = SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), 0);
    let transport = QuicTransport::bind(QuicConfig::direct_lan(key, bind))
        .await
        .map_err(|e| e.code().to_owned())?;
    let ip = lattice_quic::advertise_ip(None).map_err(|_| "pair.no_route".to_owned())?;
    let port = transport
        .local_addr()
        .map_err(|e| e.code().to_owned())?
        .port();
    let secret = random_secret().map_err(|_| "pair.rng".to_owned())?;
    let mut pending = PendingInvite::new(deck, SocketAddr::new(ip, port), secret, unix_now());
    let command = InviteCommand(pending.command());
    pairing.dialog.update(|d| d.made(generation, command));
    let name = host_name();
    let save = |w: &PairedWorker| save_worker(&files, w);
    let on_step = |step| pairing.dialog.update(|d| d.step(generation, step));
    let accept = accept_pair(&transport, &mut pending, &name, unix_now, save, on_step);
    // Rule 5 of the dialog: stop serving at the invite's expiry.
    let ttl = Duration::from_secs(INVITE_TTL_SECS);
    tokio::select! {
        paired = accept => paired.map_err(|e| pair_code(&e)),
        () = timer(ttl) => Err("pair.expired".to_owned()),
    }
}

/// Adds `worker` to the saved worker list (rule 7: before `PairOk`).
fn save_worker(files: &DeckFiles, worker: &PairedWorker) -> Result<(), String> {
    let mut list = files
        .workers()
        .map_err(|_| "deck.workers_read".to_owned())?;
    list.add(worker.clone());
    files
        .save_workers(&list)
        .map_err(|_| "deck.workers_write".to_owned())
}

/// The code the status line shows: a denial's own code, else the error's.
fn pair_code(e: &DeckError) -> String {
    match e {
        DeckError::Remote(code) => code.clone(),
        other => other.code().to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_status_line_shows_the_denial_code_not_deck_remote() {
        let denied = DeckError::Remote("pair.used".to_owned());
        assert_eq!(pair_code(&denied), "pair.used");
        let closed = DeckError::Transport(lattice_transport::TransportError::Closed);
        assert_eq!(pair_code(&closed), closed.code());
    }
}
