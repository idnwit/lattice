//! Runner modules (`runner.wasm`) as [`DynRunner`]s.

use std::sync::{Mutex, PoisonError};

use lattice_app::{AppError, DynRunner, Input, Mode, Step};
use wasmtime::{Engine, Module, TypedFunc};

use crate::guest::{Guest, compile};
use crate::{Limits, TRAP, WasmError};

/// One compiled runner module.
#[derive(Clone)]
pub struct WasmApp {
    engine: Engine,
    module: Module,
    limits: Limits,
}

impl std::fmt::Debug for WasmApp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WasmApp")
            .field("limits", &self.limits)
            .finish()
    }
}

impl WasmApp {
    /// Compiles `bytes` and checks the ABI: no imports, the v0 runner exports, and
    /// `lattice_abi() == 0`.
    ///
    /// # Errors
    ///
    /// [`WasmError`] when the module does not compile or does not speak ABI v0.
    pub fn load(engine: &Engine, bytes: &[u8], limits: Limits) -> Result<Self, WasmError> {
        let module = compile(engine, bytes, limits, |g| Exports::of(g).map(drop))?;
        Ok(Self {
            engine: engine.clone(),
            module,
            limits,
        })
    }

    /// The same package with other limits for the instances it makes from now on.
    #[must_use]
    pub fn with_limits(self, limits: Limits) -> Self {
        Self { limits, ..self }
    }

    /// A fresh module instance for one app instance: decodes params and runs `start`.
    ///
    /// Same contract as [`lattice_app::hosted`]: `None` when the params do not decode; a
    /// failed `start` (or a trap) shows as the [`Step`]'s error.
    pub fn instantiate(&self, params: &[u8], mode: Mode) -> Option<(Box<dyn DynRunner>, Step)> {
        let built = Guest::new(&self.engine, &self.module, self.limits)
            .and_then(|mut g| Exports::of(&mut g).map(|e| (g, e)));
        let Ok((guest, exports)) = built else {
            return Some((Box::new(WasmRunner::dead()), trapped()));
        };
        let mut inner = Inner { guest, exports };
        let step = match inner.call(Call::Init, params, mode) {
            Some(out) => match lattice_state::decode::<Option<Step>>(&out) {
                Ok(None) => return None,
                Ok(Some(step)) => step,
                Err(_) => inner.kill(),
            },
            None => trapped(),
        };
        let runner = WasmRunner {
            inner: Mutex::new(Some(inner)),
        };
        Some((Box::new(runner), step))
    }
}

/// The runner exports of ABI v0.
struct Exports {
    init: TypedFunc<(u32, u32, u32), u64>,
    step: TypedFunc<(u32, u32, u32), u64>,
    snapshot: TypedFunc<(), u64>,
}

impl Exports {
    fn of(g: &mut Guest) -> Result<Self, WasmError> {
        Ok(Self {
            init: g.func("lattice_init")?,
            step: g.func("lattice_step")?,
            snapshot: g.func("lattice_snapshot")?,
        })
    }
}

/// Which input-taking export to call.
#[derive(Clone, Copy)]
enum Call {
    Init,
    Step,
}

struct Inner {
    guest: Guest,
    exports: Exports,
}

impl Inner {
    /// Calls `lattice_init` or `lattice_step` with `bytes`. Any failure kills the runner.
    fn call(&mut self, call: Call, bytes: &[u8], mode: Mode) -> Option<Vec<u8>> {
        let f = match call {
            Call::Init => &self.exports.init,
            Call::Step => &self.exports.step,
        };
        let raw_mode = match mode {
            Mode::Replay => 0,
            Mode::Live => 1,
        };
        self.guest
            .call_with(f, bytes, |ptr, len| (ptr, len, raw_mode))
    }

    fn snapshot(&mut self) -> Option<Vec<u8>> {
        self.guest.call0(&self.exports.snapshot)
    }

    /// Marks the runner dead (a result that does not decode breaks the ABI).
    fn kill(&mut self) -> Step {
        self.guest.kill();
        trapped()
    }
}

/// One instance's module, driven as a [`DynRunner`].
///
/// The store sits behind a mutex because [`DynRunner::snapshot`] takes `&self` and calling
/// into wasm needs the store mutably. Only the apps runner calls it, one call at a time.
pub struct WasmRunner {
    inner: Mutex<Option<Inner>>,
}

impl std::fmt::Debug for WasmRunner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WasmRunner")
            .field("dead", &self.is_dead())
            .finish()
    }
}

impl WasmRunner {
    fn dead() -> Self {
        Self {
            inner: Mutex::new(None),
        }
    }

    fn with<R>(&self, f: impl FnOnce(&mut Inner) -> R) -> Option<R> {
        let mut guard = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        guard.as_mut().map(f)
    }
}

impl DynRunner for WasmRunner {
    fn step(&mut self, input: Input<'_>, mode: Mode) -> Step {
        let action = matches!(input, Input::Action(_));
        let step = lattice_state::encode(&input).ok().and_then(|bytes| {
            self.with(|inner| match inner.call(Call::Step, &bytes, mode) {
                Some(out) => lattice_state::decode::<Step>(&out).unwrap_or_else(|_| inner.kill()),
                None => trapped(),
            })
        });
        let mut step = step.unwrap_or_else(trapped);
        if action && step.error.as_deref() == Some(TRAP) {
            step.reply = Some(Err(AppError::platform(TRAP)));
        }
        step
    }

    fn snapshot(&self) -> Vec<u8> {
        self.with(Inner::snapshot).flatten().unwrap_or_default()
    }

    fn is_dead(&self) -> bool {
        self.with(|i| i.guest.is_dead()).unwrap_or(true)
    }
}

fn trapped() -> Step {
    Step {
        error: Some(TRAP.to_owned()),
        ..Step::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dead_runner_answers_every_action_with_a_trap() {
        let mut runner = WasmRunner::dead();
        assert!(runner.is_dead());
        let step = runner.step(Input::Action(&[0]), Mode::Live);
        assert_eq!(step.reply, Some(Err(AppError::platform(TRAP))));
        assert!(runner.snapshot().is_empty());
    }
}
