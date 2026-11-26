//! The worker host: what `latticed` runs between the network and the PTYs.
//!
//! Owner specs: `docs/architecture/04-worker.md` and `02-sync.md`.
//! It talks to the frozen `latticed-proc` over the local IPC socket
//! ([`ProcClient`]), runs one app runner per instance ([`AppsRunner`]),
//! publishes `worker/apps` and each instance's `inst/<iid>/state`, and serves
//! deck connections that arrive through `lattice_transport::Transport`.
//! It never names `quinn`: `latticed` passes the concrete transport in.
//!
//! Restore reads only journals in `latticed-proc` (06-apps.md, Design philosophy).
//! The worker ships no app (D44): a `create` naming a package it lacks makes it ask the deck
//! (`BlobRequest` → `Blob`), verify the hash, and install it through the [`PackageLoader`]
//! `latticed` passes (WASM runners via `lattice-wasm`). Not here yet: hook ingestion, files.
//!
//! # Usage
//!
//! ```no_run
//! # async fn run<T: lattice_transport::Transport + 'static>(
//! #     transport: T,
//! #     loader: lattice_worker::PackageLoader,
//! # ) -> Result<(), lattice_worker::WorkerError> {
//! use lattice_worker::{AppsRunner, BootEpoch, PeerPolicy, ProcClient, WorkerHost};
//!
//! let proc = ProcClient::connect("/run/user/1000/lattice/proc.sock".as_ref()).await?;
//! // No app yet: packages come from the cache (`install`) or from decks (`loader`).
//! let apps = AppsRunner::new(BootEpoch::from_unix_millis(1_700_000_000_000));
//! WorkerHost::new(transport, proc, PeerPolicy::allow_list(Vec::new()), apps, loader).run().await
//! # }
//! ```
//!
//! # Key items
//!
//! - [`WorkerHost`]: accept loop; one task per deck connection.
//! - [`PeerSession`]: sans-IO handler of one deck's sync messages.
//! - [`ProcClient`]: async IPC client for `latticed-proc`; [`ProcClient::runtime_dir`]: where
//!   its socket and `worker.addr` live.
//! - [`AppsRunner`]: packages, instances, journals, effects, delta queues; [`BootEpoch`]
//!   (a boot's first version); [`Installed`],
//!   [`PackageLoader`]: how a package a deck sends becomes runnable.
//! - [`WorkerStream`]: `worker/apps` and `inst/<iid>/state`.
//! - [`PeerPolicy`], [`Allowlist`]: which NodeIds may connect (the `worker/decks` file).
//! - [`add_deck`]: the worker side of pairing (`lattice add-deck`, specs/01-pairing.md).

mod allowlist;
mod apps;
mod decks_file;
mod delta_queue;
mod error;
mod host;
mod pair;
mod policy;
mod proc_client;
mod runner;
mod session;
mod spawn;
mod streams;
#[cfg(test)]
mod test_app;
mod wire;

pub use apps::{AppsEffect, AppsRunner, Installed, PackageLoader, RunnerFactory, StateSync};
pub use delta_queue::BootEpoch;

pub use allowlist::Allowlist;
pub use error::WorkerError;
pub use host::WorkerHost;
/// The pairing invite `add_deck` takes (re-exported for `latticed`).
pub use lattice_sync_proto::Invite;
pub use pair::{PairError, PairedDeck, WorkerCard, add_deck};
pub use policy::PeerPolicy;
pub use proc_client::ProcClient;
pub use runner::{DynRunner, Input, Mode, Step, hosted};
pub use session::{PeerSession, SessionEffect};
pub use streams::WorkerStream;
