//! The worker host: what `latticed` runs between the network and the PTYs.
//!
//! Owner specs: `docs/architecture/04-worker.md` and `02-sync.md`.
//! It talks to the frozen `latticed-proc` over the local IPC socket
//! ([`ProcClient`]), publishes each process as sync streams, and serves deck
//! connections that arrive through `lattice_transport::Transport`.
//! It never names `quinn`: `latticed` passes the concrete transport in.
//!
//! Not here yet (later layers): VT emulation and the `screen` State stream
//! (`lattice-vt`), WASM apps (`lattice-wasm`), hook ingestion, files.
//!
//! # Usage
//!
//! ```no_run
//! # async fn run<T: lattice_transport::Transport + 'static>(transport: T) -> Result<(), lattice_worker::WorkerError> {
//! use lattice_worker::{PeerPolicy, ProcClient, WorkerHost};
//!
//! let proc = ProcClient::connect("/run/user/1000/lattice/proc.sock".as_ref()).await?;
//! WorkerHost::new(transport, proc, PeerPolicy::allow_list(Vec::new())).run().await
//! # }
//! ```
//!
//! # Key items
//!
//! - [`WorkerHost`]: accept loop; one task per deck connection.
//! - [`PeerSession`]: sans-IO handler of one deck's sync messages.
//! - [`ProcClient`]: async IPC client for `latticed-proc`.
//! - [`ProcStream`]: stream ids per process (`proc/<id>/raw`).
//! - [`PeerPolicy`]: which NodeIds may connect.

mod error;
mod host;
mod policy;
mod proc_client;
mod session;
mod streams;

pub use error::WorkerError;
pub use host::WorkerHost;
pub use policy::PeerPolicy;
pub use proc_client::ProcClient;
pub use session::{PeerSession, SessionEffect};
pub use streams::{ProcChannel, ProcStream};
