//! Viewer modules (`viewer.wasm`) as [`DynViewer`]s (05-deck.md, View-tree protocol).

use lattice_app::ReplicaEvent;
use lattice_app::view::{DynViewer, UiEvent, ViewTree};
use wasmtime::{Engine, Module, TypedFunc};

use crate::guest::{Guest, compile};
use crate::{Limits, WasmError};

/// One compiled viewer module.
#[derive(Clone)]
pub struct WasmViewerModule {
    engine: Engine,
    module: Module,
    limits: Limits,
}

impl std::fmt::Debug for WasmViewerModule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WasmViewerModule")
            .field("limits", &self.limits)
            .finish()
    }
}

impl WasmViewerModule {
    /// Compiles `bytes` and checks the ABI: no imports, the v0 viewer exports, and
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

    /// The same module with other limits for the viewers it makes from now on.
    #[must_use]
    pub fn with_limits(self, limits: Limits) -> Self {
        Self { limits, ..self }
    }

    /// A fresh viewer for one pane. A module that fails to start gives a dead viewer.
    pub fn instantiate(&self) -> WasmViewer {
        let built = Guest::new(&self.engine, &self.module, self.limits)
            .and_then(|mut g| Exports::of(&mut g).map(|e| (g, e)));
        WasmViewer {
            inner: built.ok(),
            version: None,
        }
    }
}

/// The viewer exports of ABI v0.
struct Exports {
    load: TypedFunc<(u32, u32), u64>,
    apply: TypedFunc<(u32, u32), u64>,
    render: TypedFunc<(), u64>,
    handle: TypedFunc<(u32, u32), u64>,
}

impl Exports {
    fn of(g: &mut Guest) -> Result<Self, WasmError> {
        Ok(Self {
            load: g.func("lattice_load")?,
            apply: g.func("lattice_apply")?,
            render: g.func("lattice_render")?,
            handle: g.func("lattice_handle")?,
        })
    }
}

/// One pane's viewer module, driven as a [`DynViewer`].
///
/// A trap, running out of fuel, or a result that does not decode kills it: updates answer
/// `Resync`, `render` an empty tree, `handle` no action, and [`DynViewer::is_dead`] is true.
/// The version mirrors the guest's replica: set by a `Changed`, cleared by a `Resync`.
pub struct WasmViewer {
    inner: Option<(Guest, Exports)>,
    version: Option<u64>,
}

impl std::fmt::Debug for WasmViewer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WasmViewer")
            .field("dead", &self.is_dead())
            .field("version", &self.version)
            .finish()
    }
}

impl WasmViewer {
    fn update(&mut self, full: bool, version: u64, bytes: &[u8]) -> ReplicaEvent {
        let input = lattice_state::encode(&(version, bytes)).unwrap_or_default();
        let event = self
            .call(|g, e| {
                let f = if full { &e.load } else { &e.apply };
                g.call_with(f, &input, |ptr, len| (ptr, len))
            })
            .unwrap_or(ReplicaEvent::Resync);
        self.version = match event {
            ReplicaEvent::Changed => Some(version),
            ReplicaEvent::Resync => None,
        };
        event
    }

    /// Runs one export and decodes its result. A result that does not decode kills the guest.
    fn call<T: serde::de::DeserializeOwned>(
        &mut self,
        f: impl FnOnce(&mut Guest, &Exports) -> Option<Vec<u8>>,
    ) -> Option<T> {
        let (guest, exports) = self.inner.as_mut()?;
        let out = f(guest, exports)?;
        let value = lattice_state::decode(&out).ok();
        if value.is_none() {
            guest.kill();
        }
        value
    }
}

impl DynViewer for WasmViewer {
    fn load(&mut self, version: u64, bytes: &[u8]) -> ReplicaEvent {
        self.update(true, version, bytes)
    }

    fn apply(&mut self, version: u64, bytes: &[u8]) -> ReplicaEvent {
        self.update(false, version, bytes)
    }

    fn version(&self) -> Option<u64> {
        self.version
    }

    fn render(&mut self) -> ViewTree {
        self.call(|g, e| g.call0(&e.render)).unwrap_or_default()
    }

    fn handle(&mut self, event: &UiEvent) -> Vec<Vec<u8>> {
        let Ok(input) = lattice_state::encode(event) else {
            return Vec::new();
        };
        self.call(|g, e| g.call_with(&e.handle, &input, |ptr, len| (ptr, len)))
            .unwrap_or_default()
    }

    fn is_dead(&self) -> bool {
        self.inner.as_ref().is_none_or(|(g, _)| g.is_dead())
    }
}
