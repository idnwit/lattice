//! WASM host for app runners and viewers: ABI v0, core wasm + postcard (D42, D43;
//! 06-apps.md, Runtimes).
//!
//! A package holds two core wasm modules built from the app crate:
//! `runner.wasm` (`lattice_app_guest::export_runner!`) for workers and `viewer.wasm`
//! (`export_viewer!`) for decks. The guest runs the runner core or holds the viewer's replica
//! itself; this crate only moves encoded bytes across the module's memory, so a
//! [`WasmRunner`] is a [`DynRunner`] and a [`WasmViewer`] a [`DynViewer`], like native ones.
//!
//! Sandbox (v0): no imports at all (no WASI, no clock, no host calls), linear memory capped
//! at [`Limits::memory_bytes`], and [`Limits::fuel_per_call`] fuel per call. A trap, running
//! out of fuel, or a malformed result kills the module: a runner answers [`TRAP`] from then
//! on and [`DynRunner::is_dead`] is true, so the host marks the instance `failed`; a viewer
//! draws nothing and [`DynViewer::is_dead`] is true.
//!
//! # Usage
//!
//! ```no_run
//! # fn run(bytes: &[u8]) -> Result<(), lattice_wasm::WasmError> {
//! use lattice_app::Mode;
//! use lattice_wasm::{Limits, WasmApp, engine};
//!
//! let app = WasmApp::load(&engine()?, bytes, Limits::default())?;
//! let built = app.instantiate(&[], Mode::Live); // like `lattice_app::hosted`
//! # Ok(()) }
//! ```
//!
//! # Key items
//!
//! - [`engine`]: the shared wasmtime engine (fuel on).
//! - [`WasmApp`]: one compiled runner module; [`WasmApp::instantiate`] per instance.
//! - [`WasmRunner`]: one instance's module, as a [`DynRunner`].
//! - [`WasmViewerModule`]: one compiled viewer module; [`WasmViewer`] per pane, a [`DynViewer`].
//! - [`Limits`], [`WasmError`].

mod guest;
mod runner;
mod viewer;

pub use runner::{WasmApp, WasmRunner};
pub use viewer::{WasmViewer, WasmViewerModule};

#[cfg(doc)]
use lattice_app::{DynRunner, view::DynViewer};
use lattice_app_guest::ABI_VERSION;
use wasmtime::Config;
/// The wasmtime engine type [`engine`] returns, so hosts need not name wasmtime.
pub use wasmtime::Engine;

/// Step error code: the module trapped, ran out of fuel, or broke the ABI. The runner is dead.
pub const TRAP: &str = "wasm.trap";

/// Resource caps of one module instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Max linear memory, in bytes.
    pub memory_bytes: usize,
    /// Fuel (about one per wasm instruction) for one call.
    pub fuel_per_call: u64,
}

impl Default for Limits {
    /// 64 MiB and 1e9 fuel per call. Proposed; TBD in the manifest (06-apps.md, Manifest).
    fn default() -> Self {
        Self {
            memory_bytes: 64 << 20,
            fuel_per_call: 1_000_000_000,
        }
    }
}

/// Why a package cannot load. Codes are stable.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WasmError {
    /// The engine could not be built.
    #[error("wasm engine: {0}")]
    Engine(String),
    /// The bytes are not a valid core wasm module.
    #[error("wasm compile: {0}")]
    Compile(String),
    /// The module lacks an export of ABI v0, or has the wrong signature.
    #[error("wasm export missing: {0}")]
    Export(&'static str),
    /// The module imports something; ABI v0 allows no import.
    #[error("wasm import not allowed: {0}")]
    Import(String),
    /// The module speaks another ABI version.
    #[error("wasm abi {0}, want {ABI_VERSION}")]
    Abi(u32),
}

impl WasmError {
    /// Stable code.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Engine(_) => "wasm.engine",
            Self::Compile(_) => "wasm.compile",
            Self::Export(_) => "wasm.export",
            Self::Import(_) => "wasm.import",
            Self::Abi(_) => "wasm.abi",
        }
    }
}

/// The engine every module compiles with: cranelift, fuel metering on.
///
/// # Errors
///
/// [`WasmError::Engine`] when wasmtime refuses the config on this host.
pub fn engine() -> Result<Engine, WasmError> {
    let mut config = Config::new();
    config.consume_fuel(true);
    Engine::new(&config).map_err(|e| WasmError::Engine(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_that_are_not_wasm_do_not_load() {
        let engine = engine().unwrap();
        let err = WasmApp::load(&engine, b"not wasm", Limits::default()).unwrap_err();
        assert_eq!(err.code(), "wasm.compile");
    }

    #[test]
    fn error_codes_are_stable() {
        assert_eq!(WasmError::Export("memory").code(), "wasm.export");
        assert_eq!(WasmError::Abi(1).code(), "wasm.abi");
        assert_eq!(WasmError::Import("env::f".into()).code(), "wasm.import");
        assert_eq!(WasmError::Engine(String::new()).code(), "wasm.engine");
    }
}
