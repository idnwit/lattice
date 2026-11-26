//! The worker host's accept loop.
//!
//! Tasks (engineering.md, Async and I/O):
//!
//! - **core** (the `run` future): owns the [`AppsRunner`], the [`ProcClient`], and every
//!   [`crate::PeerSession`]. It selects over deck events and proc replies, one at a time.
//! - **accept**: `Transport::accept`, policy check, one connection task per deck.
//! - **connection**: accepts the control bi stream, decodes sync frames into events for the
//!   core (bounded, `send().await`), and writes the core's messages back (bounded per deck;
//!   a deck that cannot keep up is disconnected, never waited for).
//!
//! M1: one bi stream per connection carries every message, subscriptions included.

mod conn;
mod core;

use crate::apps::{AppsRunner, PackageLoader};
use crate::error::WorkerError;
use crate::policy::PeerPolicy;
use crate::proc_client::ProcClient;
use lattice_proc_ipc::{Reply, Request};
use lattice_transport::Transport;
use tokio::sync::mpsc;

/// Deck events queued for the core.
const EVENT_QUEUE: usize = 256;

/// Serves deck connections for one worker.
pub struct WorkerHost<T> {
    transport: T,
    proc: ProcClient,
    policy: PeerPolicy,
    apps: AppsRunner,
    loader: PackageLoader,
}

impl<T> std::fmt::Debug for WorkerHost<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorkerHost")
            .field("apps", &self.apps)
            .finish_non_exhaustive()
    }
}

impl<T: Transport + 'static> WorkerHost<T> {
    /// A host over `transport`, talking to `latticed-proc` through `proc`, running `apps`
    /// (the packages installed so far). `loader` installs a package a deck sends.
    pub fn new(
        transport: T,
        proc: ProcClient,
        policy: PeerPolicy,
        apps: AppsRunner,
        loader: PackageLoader,
    ) -> Self {
        Self {
            transport,
            proc,
            policy,
            apps,
            loader,
        }
    }

    /// Runs until the transport closes.
    ///
    /// Restores instances from `List` first (06-apps.md, Restore), then serves decks.
    ///
    /// # Errors
    ///
    /// [`WorkerError`] when the proc daemon or transport fails for good.
    pub async fn run(self) -> Result<(), WorkerError> {
        let Self {
            transport,
            mut proc,
            policy,
            mut apps,
            loader,
        } = self;
        let worker = transport.local_id();
        proc.send(&Request::List).await?;
        let journals = loop {
            match proc.next_reply().await? {
                Reply::Journals(j) => break j,
                other => tracing::debug!(?other, "reply before the journal list ignored"),
            }
        };
        tracing::info!(journals = journals.len(), "restoring instances");
        let effects = apps.restore(journals);
        let mut core = core::Core::new(worker, apps, proc, loader);
        core.run_effects(effects).await?;

        let (events, mut inbox) = mpsc::channel(EVENT_QUEUE);
        let mut accept = tokio::spawn(conn::accept_loop(transport, policy, events));
        loop {
            tokio::select! {
                ev = inbox.recv() => core.on_event(ev).await?,
                reply = core.next_reply() => core.on_reply(reply?).await?,
                res = &mut accept => return conn::accept_ended(res),
            }
        }
    }
}

#[cfg(test)]
mod tests;
