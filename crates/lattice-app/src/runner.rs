//! One running instance: an [`App`] runner, its state, and the commit step (06-apps.md, Runner).
//!
//! The same code runs natively (tests, a host that links an app) and inside a WASM guest
//! (`lattice-app-guest`): the guest does draft, commit, and rollback itself, and the host
//! only moves encoded [`Input`]s and [`Step`]s (D43).

use std::marker::PhantomData;

use lattice_state::{Draft, Synced};
use serde::{Deserialize, Serialize};

use crate::{App, AppError, Ctx, Dispatch, Effect, ExitStatus, Millis, ProcRef};

/// Whether effects run. Replay rebuilds state from the journal and drops effects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mode {
    /// Restoring from the journal: effects are dropped.
    Replay,
    /// Live: effects run after commit.
    Live,
}

/// A journal entry or a viewer action, as the runner sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Input<'a> {
    /// Process output.
    Output(ProcRef, #[serde(borrow)] &'a [u8]),
    /// A process exited.
    Exit(ProcRef, ExitStatus),
    /// History was dropped.
    Gap,
    /// The time the runner asked for in `wake_at` has come. Live only.
    Wake(Millis),
    /// Replay is done (or `start` ran live): the app re-issues what it needs live.
    Resumed,
    /// An encoded viewer action. Never replayed.
    Action(#[serde(borrow)] &'a [u8]),
}

/// Result of one reducer call.
#[derive(Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Step {
    /// Encoded delta, when the call committed a change.
    pub delta: Option<Vec<u8>>,
    /// Effects to run (always empty in [`Mode::Replay`]).
    pub effects: Vec<Effect>,
    /// Error code when `start` failed or the delta did not encode (the draft rolled back).
    pub error: Option<String>,
    /// Outcome of an [`Input::Action`]: the encoded `Res`, or why it was refused (the draft
    /// rolled back). `None` for every other input.
    pub reply: Option<Result<Vec<u8>, AppError>>,
}

/// Object-safe view of a runner, so the host holds runners of different apps.
pub trait DynRunner: Send {
    /// Feeds one input; commits or rolls back.
    fn step(&mut self, input: Input<'_>, mode: Mode) -> Step;
    /// The full state, encoded (for a reader outside the delta queue).
    fn snapshot(&self) -> Vec<u8>;
    /// True once the runner can take no more input (a WASM guest trapped). The host marks
    /// the instance `failed` and drops the runner.
    fn is_dead(&self) -> bool {
        false
    }
}

/// A runner of the app of `D` with its state. `D` is the generated `XApiRunner<App>`.
struct Hosted<D: Dispatch> {
    app: D::App,
    state: <D::App as App>::State,
    ctx: Ctx,
    dispatch: PhantomData<fn() -> D>,
}

/// Builds a runner from encoded params and calls `start`.
///
/// `D` is the runner adapter `#[interface]` generates: register built-in apps with
/// `hosted::<TerminalApiRunner<Terminal>>`; a WASM guest exports it with
/// `lattice_app_guest::export_runner!`.
/// Empty params are the app's default params. `None` when other params do not decode
/// (postcard). When `start` fails, the
/// returned [`Step`] carries the error code and the caller marks the instance `failed`.
pub fn hosted<D>(params: &[u8], mode: Mode) -> Option<(Box<dyn DynRunner>, Step)>
where
    D: Dispatch + 'static,
    D::App: Send,
    <D::App as App>::State: Send,
{
    let params: <D::App as App>::Params = if params.is_empty() {
        Default::default()
    } else {
        lattice_state::decode(params).ok()?
    };
    let mut runner = Hosted::<D> {
        app: D::App::new(&params),
        state: Default::default(),
        ctx: Ctx::new(),
        dispatch: PhantomData,
    };
    let res = runner.app.start(&mut runner.ctx, &mut runner.state);
    let step = runner.finish(res.map_err(|e| AppError::of(&e)), mode);
    Some((Box::new(runner), step))
}

impl<D: Dispatch> Hosted<D> {
    /// Ends one call: commit on `Ok`, rollback on `Err` (06-apps.md, State rule 2).
    ///
    /// Commit encodes the draft as the delta first. A refused call reports its
    /// [`AppError`] code. Effects run only when the call committed in [`Mode::Live`];
    /// a refused call drops them.
    fn finish(&mut self, res: Result<(), AppError>, mode: Mode) -> Step {
        let effects = self.ctx.take_effects();
        if let Err(e) = res {
            self.state.rollback();
            return Step {
                error: Some(e.code().to_owned()),
                ..Step::default()
            };
        }
        let mut step = Step::default();
        if self.state.has_draft() {
            match lattice_state::encode(&Draft(&self.state)) {
                Ok(delta) => {
                    self.state.commit();
                    step.delta = Some(delta);
                }
                Err(e) => {
                    // A state type that does not fit postcard is a bug in the app.
                    self.state.rollback();
                    step.error = Some(e.code().to_owned());
                    return step;
                }
            }
        }
        if mode == Mode::Live {
            step.effects = effects;
        }
        step
    }

    /// A viewer action: dispatch, then commit with the reply, or roll back with the error.
    fn action(&mut self, bytes: &[u8], mode: Mode) -> Step {
        match D::dispatch(&mut self.app, &mut self.ctx, &mut self.state, bytes) {
            Ok(reply) => {
                let mut step = self.finish(Ok(()), mode);
                step.reply = Some(match step.error.take() {
                    None => Ok(reply),
                    Some(code) => Err(AppError::platform(&code)),
                });
                step
            }
            Err(e) => Step {
                reply: Some(Err(e.clone())),
                error: None,
                ..self.finish(Err(e), mode)
            },
        }
    }
}

impl<D: Dispatch> DynRunner for Hosted<D>
where
    D::App: Send,
    <D::App as App>::State: Send,
{
    fn step(&mut self, input: Input<'_>, mode: Mode) -> Step {
        let (app, ctx, state) = (&mut self.app, &mut self.ctx, &mut self.state);
        match input {
            Input::Output(proc, bytes) => app.on_output(ctx, state, proc, bytes),
            Input::Exit(proc, status) => app.on_exit(ctx, state, proc, status),
            Input::Gap => app.on_gap(ctx, state),
            Input::Wake(now) => app.wake(ctx, state, now),
            Input::Resumed => app.resumed(ctx, state),
            Input::Action(bytes) => return self.action(bytes, mode),
        }
        self.finish(Ok(()), mode)
    }

    fn snapshot(&self) -> Vec<u8> {
        // A state that does not encode is the same bug as a delta that does not: the deck
        // gets an empty state.
        lattice_state::encode(&self.state).unwrap_or_default()
    }
}
