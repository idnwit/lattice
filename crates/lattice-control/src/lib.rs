//! The worker control socket codec and the worker config model.
//!
//! Owner specs: `docs/specs/11-control-socket.md` (socket, messages, limits) and
//! `docs/specs/10-settings.md` › Worker config (`config.toml`, precedence, relay pin).
//! Shared by `latticed`, which serves the socket and owns `config.toml`, and the `lattice`
//! CLI, which sends one request, or writes the file itself while no daemon runs.
//!
//! Everything here is sans-IO except [`DirLock`] and [`read_or_empty`]: sync `std` file
//! calls and `flock`, no sockets, no runtime. The socket I/O lives in the binaries.
//!
//! # Usage
//!
//! ```
//! use lattice_control::{ConfigFile, Key, Layers, RelayPlan, Source};
//!
//! let mut file = ConfigFile::parse("[relay]\naddr = \"udp://relay.example.com:7400\"\n")?;
//! file.set(Key::Name, "build-01")?;
//! let resolved = Layers::new("host".to_owned())
//!     .env(Key::RelayAddr, "none")?
//!     .resolve(&file);
//! assert_eq!(resolved.entry(Key::RelayAddr).map(|e| e.source()), Some(Source::Env));
//! assert_eq!(*resolved.relay(), RelayPlan::None);
//! # Ok::<(), lattice_control::ConfigError>(())
//! ```
//!
//! # Key items
//!
//! - [`ConfigFile`], [`Key`], [`ConfigError`]: the file, its keys, and the error codes.
//! - [`Layers`], [`Resolved`], [`Entry`], [`Source`]: flag > env > file > default.
//! - [`RelayAddr`], [`RelayPlan`], [`RelayId`], [`Pin`]: the home relay and its id pin.
//! - [`Host`], [`HostPort`]: canonical hosts (specs/01-pairing.md, Contact).
//! - [`Request`], [`Reply`], [`RequestReader`], [`ReplyReader`], [`encode_request`],
//!   [`encode_reply`]: one request, one reply.
//! - [`DirLock`]: the exclusive `flock` on `worker/` and atomic writes under it.

#![deny(clippy::indexing_slicing, clippy::arithmetic_side_effects)]
#![deny(clippy::cast_possible_truncation)]

mod config;
mod files;
mod host;
mod relay;
mod resolve;
mod wire;

pub use config::{ConfigError, ConfigFile, Key};
pub use files::{CONFIG_ENV, DirLock, LockError, SOCKET_FILE, config_path, read_or_empty};
pub use host::{Host, HostError, HostPort};
pub use relay::{Pin, RelayAddr, RelayId, RelayPlan};
pub use resolve::{DEFAULT_LISTEN, Entry, Layers, RELAY_ENV, Resolved, Source};
pub use wire::{
    CONTROL_VERSION, ControlError, Incoming, MAX_CONTROL_FRAME, Reply, ReplyReader, Request,
    RequestReader, encode_reply, encode_request, file_hash,
};
