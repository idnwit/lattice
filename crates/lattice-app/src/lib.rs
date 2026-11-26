//! App contract between the platform and one app (`docs/architecture/06-apps.md`).
//!
//! An app is one Rust crate built into a runner (worker) and a viewer (deck).
//! This crate holds what both sides and the hosts agree on: the [`App`] trait a
//! runner implements, the [`Ctx`] that records its effects, instance ids, the
//! `worker/apps` records, and the [`AppsAction`]s a deck sends. It is sans-IO:
//! no processes, no sockets, no clock. The worker host runs effects; the deck
//! host applies deltas.
//!
//! Design philosophy (06-apps.md): a runner restores only from its journal in
//! `latticed-proc`. Reducers that read process output must be deterministic;
//! the host drops effects during replay.
//!
//! # Usage
//!
//! ```ignore
//! impl App for Terminal {
//!     const ID: &'static str = "dev.lattice.terminal";
//!     type State = TermState;
//!     // ... hooks: start, on_output, on_exit, on_gap, resumed
//! }
//!
//! #[lattice_app::interface]
//! pub trait TerminalApi: App<State = TermState, Params = TermParams, Error = TermError> {
//!     fn resize(&mut self, ctx: &mut Ctx, state: &mut TermState, req: Resize) -> Result<Size, TermError>;
//! }
//!
//! // Host: hosted::<TerminalApiRunner<Terminal>>. Viewer:
//! let size = TerminalApiClient::new(caller).resize(Resize::new(120, 40)).await?;
//! ```
//!
//! # Key items
//!
//! - [`App`]: runner hooks (`start`, `on_output`, `on_exit`, `on_gap`, `resumed`, `wake`).
//! - [`macro@interface`]: an app's reducers as a typed trait; generates the action enum, the
//!   runner adapter ([`Dispatch`]) and a typed client over a [`Caller`] (D36).
//! - [`Dispatch`], [`AppError`]: the runner side of reducers; app errors cross as `{code, detail}`.
//! - [`Caller`], [`Ticket`], [`RawTicket`], [`Resolver`], [`CallError`]: the caller side.
//! - [`Ctx`], [`Effect`]: effects a reducer records; run after commit, dropped on replay.
//! - [`InstanceId`], [`AppId`], [`PackageHash`], [`ProcRef`]: ids.
//! - [`InstanceRecord`], [`InstancePhase`]: one row of the `worker/apps` state.
//! - [`AppsAction`]: `create`, `close`, `kill` on `worker/apps`.
//! - [`Replica`], [`ReplicaEvent`]: a viewer's copy of one instance state, kept current with deltas.
//! - [`JournalMeta`]: what `latticed` stores at `Open` to rebuild a runner.
//! - [`hosted`], [`DynRunner`], [`Input`], [`Step`], [`Mode`]: the runner core: one input in,
//!   one committed delta and its effects out. Natively in tests, inside the guest in WASM.
//! - [`view`]: the viewer side: [`view::ViewTree`] vocabulary v0, [`view::Viewer`],
//!   [`view::DynViewer`], [`view::ViewerCore`].

mod call;
mod ctx;
mod dispatch;
mod ids;
mod instance;
mod replica;
mod runner;
pub mod view;

pub use call::{CallError, Caller, RawCallError, RawTicket, Resolver, Ticket};
pub use ctx::{Ctx, Effect, Signal};
#[doc(hidden)]
pub use dispatch::__private;
pub use dispatch::{AppError, BAD_ACTION, BAD_REPLY, Dispatch, variant_name};
pub use ids::{AppId, IdError, InstanceId, MAX_APP_ID_LEN, PackageHash, ProcRef};
pub use instance::{AppsAction, InstancePhase, InstanceRecord, JournalMeta};
pub use lattice_app_macros::interface;
pub use replica::{Replica, ReplicaEvent};
pub use runner::{DynRunner, Input, Mode, Step, hosted};

use lattice_state::Synced;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

/// How a process of the instance ended, as the runner sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExitStatus {
    /// Exited with a code.
    Code(i32),
    /// Killed by a signal.
    Signal(i32),
}

/// Milliseconds on the worker clock, given by the host. Runners never read a clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Millis(pub u64);

/// The runner side of an app: hooks the host calls on one instance state (D37).
///
/// `self` is the runner's private memory (e.g. a VT emulator). Decks never see
/// it; replay rebuilds it. Every call runs on a draft of the state. `Ok` (or
/// a hook without a result) commits one delta; `Err` rolls back. Effects
/// recorded in [`Ctx`] run after commit. Only `on_output`, `on_exit`, and
/// `on_gap` replay (06-apps.md, Restore).
///
/// Reducers (viewer actions) are not here: they are the methods of the app's own
/// interface trait, under [`macro@interface`].
pub trait App: Sized {
    /// Stable app id, as in `app.toml`.
    const ID: &'static str;
    /// The instance state. Decks hold a replica.
    type State: Synced + Default;
    /// Typed params of `create`, validated by the host. Empty params bytes mean
    /// `Params::default()`: what a deck sends when it has no params form (03-sessions.md,
    /// Behavior 11).
    type Params: DeserializeOwned + Default;
    /// The app's error enum (`thiserror` + serde); serde variant names are wire codes.
    type Error: std::error::Error + Serialize;

    /// Private runner memory for a new or restored instance.
    fn new(params: &Self::Params) -> Self;

    /// First call of an instance, also on restore (effects dropped then).
    ///
    /// # Errors
    ///
    /// The app error; the instance becomes `failed`.
    fn start(&mut self, ctx: &mut Ctx, state: &mut Self::State) -> Result<(), Self::Error>;

    /// Output of a process of the instance. Deterministic: replayed on restore.
    fn on_output(&mut self, ctx: &mut Ctx, state: &mut Self::State, proc: ProcRef, bytes: &[u8]);

    /// A process of the instance exited. Deterministic: replayed on restore.
    fn on_exit(
        &mut self,
        ctx: &mut Ctx,
        state: &mut Self::State,
        proc: ProcRef,
        status: ExitStatus,
    );

    /// History was dropped before this point; the state is partial.
    fn on_gap(&mut self, ctx: &mut Ctx, state: &mut Self::State);

    /// Replay is done and effects run again. Re-issue what is needed live: timers,
    /// ephemeral requests, a PTY size nudge. A new instance gets it right after `start`.
    fn resumed(&mut self, ctx: &mut Ctx, state: &mut Self::State) {
        let _ = (ctx, state);
    }

    /// When the runner next wants [`App::wake`]. Read after every commit and after restore.
    fn wake_at(&self, state: &Self::State) -> Option<Millis> {
        let _ = state;
        None
    }

    /// Called at or after [`App::wake_at`].
    fn wake(&mut self, ctx: &mut Ctx, state: &mut Self::State, now: Millis) {
        let _ = (ctx, state, now);
    }
}

#[cfg(test)]
mod tests;
