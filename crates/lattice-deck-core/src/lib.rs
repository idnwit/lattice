//! The deck host: everything a deck does except drawing.
//!
//! Owner specs: `docs/architecture/05-deck.md`, `docs/specs/03-sessions.md`,
//! `docs/specs/02-connectivity.md`.
//! macOS and iOS share this crate unchanged. Only the views in `lattice-deck`
//! differ. It dials workers through `lattice_transport::Transport` and never
//! names `quinn` or NSH.
//!
//! It holds one [`Replica`] per visible app instance: full state plus deltas.
//!
//! Not here yet (later layers): local store, layout engine, WASM viewers.
//!
//! # Usage
//!
//! ```no_run
//! # async fn run<T: lattice_transport::Transport>(t: T, worker: lattice_key::NodeId) -> Result<(), lattice_deck_core::DeckError> {
//! use lattice_deck_core::DeckHost;
//!
//! let host = DeckHost::new(t);
//! let mut link = host.open_link(worker).await?;
//! link.subscribe("inst/1/state".parse().unwrap()).await?;
//! # Ok(()) }
//! ```
//!
//! # Key items
//!
//! - [`DeckHost`], [`WorkerLink`]: dial a worker, subscribe, send `create`/`close`/`kill` and app actions.
//! - [`LinkCaller`]: a `lattice_app::Caller` over a link, for generated clients (typed calls, tickets).
//! - [`Replica`]: one instance state kept current with deltas (re-exported from `lattice_app`).
//! - [`StateUpdate`]: a `Snapshot` or `Change` of a worker's `State` stream, read from a [`lattice_sync_proto::Message`];
//!   [`StateUpdate::apply_to`] feeds it to a [`Replica`].
//! - [`DeckState`]: workers and their links; what views render.
//! - [`PaneId`]: `<tab>.<pane>` ids (`1.2`).
//! - [`Status`]: state glyph values and busiest-child priority.
//! - [`PendingInvite`], [`accept_pair`]: the deck side of pairing (specs/01-pairing.md, D47).
//! - [`Backoff`]: waits between dials of a lost worker (specs/02-connectivity.md, Reconnect).
//! - [`DeckFiles`], [`WorkerList`], [`InviteLock`]: the deck key, the paired workers, and the
//!   one-open-invite lock in `<data dir>/deck`.

#![deny(clippy::indexing_slicing, clippy::arithmetic_side_effects)]

mod calls;
mod files;
mod link;
mod model;
mod pair;
mod reconnect;
mod status;
mod wire;
mod workers;

pub use calls::LinkCaller;
pub use files::{DeckFiles, InviteLock, InviteLockError, random_secret};
pub use lattice_app::{Replica, ReplicaEvent};
pub use lattice_sync_proto::{INVITE_TTL_SECS, PAIR_TIMEOUT};
pub use link::{DeckError, DeckHost, WorkerLink};
pub use model::{DeckState, PaneId, PaneIdError, WorkerEntry};
pub use pair::{PairDeny, PairStep, PairedWorker, PendingInvite, accept_pair};
pub use reconnect::{Backoff, CONNECT_TIMEOUT, RECONNECT_FIRST, RECONNECT_MAX};
pub use status::Status;
pub use wire::StateUpdate;
pub use workers::{WorkerList, monogram};
