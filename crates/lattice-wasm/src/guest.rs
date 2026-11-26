//! One module instance and the ABI v0 byte moves every call shares (D43).
//!
//! A call: set the fuel, `lattice_alloc` a guest buffer, write the input into it, call the
//! export, and copy the `ptr << 32 | len` result out. Any failure (trap, no fuel, a result
//! outside memory) kills the guest: every later call fails at once.

use lattice_app_guest::ABI_VERSION;
use wasmtime::{
    Engine, Instance, Memory, Module, Store, StoreLimits, StoreLimitsBuilder, TypedFunc,
    WasmParams, WasmResults,
};

use crate::{Limits, WasmError};

/// A store sized by `limits`, with one call's fuel (instantiation burns fuel too).
fn store(engine: &Engine, limits: Limits) -> Store<StoreLimits> {
    let caps = StoreLimitsBuilder::new()
        .memory_size(limits.memory_bytes)
        .instances(1)
        .build();
    let mut store = Store::new(engine, caps);
    store.limiter(|l| l);
    if store.set_fuel(limits.fuel_per_call).is_err() {
        // Fuel is on in `engine()`; an engine without it simply runs unmetered.
    }
    store
}

/// Compiles `bytes` and checks what every v0 module must have: no imports, `memory`,
/// `lattice_alloc`, and `lattice_abi() == 0`. Then `check` looks up the kind's exports.
pub(crate) fn compile(
    engine: &Engine,
    bytes: &[u8],
    limits: Limits,
    check: impl FnOnce(&mut Guest) -> Result<(), WasmError>,
) -> Result<Module, WasmError> {
    let module = Module::new(engine, bytes).map_err(|e| WasmError::Compile(e.to_string()))?;
    if let Some(import) = module.imports().next() {
        let name = format!("{}::{}", import.module(), import.name());
        return Err(WasmError::Import(name));
    }
    let mut guest = Guest::new(engine, &module, limits)?;
    let abi: TypedFunc<(), u32> = guest.func("lattice_abi")?;
    match abi.call(&mut guest.store, ()) {
        Ok(ABI_VERSION) => {}
        Ok(other) => return Err(WasmError::Abi(other)),
        Err(_) => return Err(WasmError::Export("lattice_abi")),
    }
    check(&mut guest)?;
    Ok(module)
}

/// One live module instance.
pub(crate) struct Guest {
    store: Store<StoreLimits>,
    instance: Instance,
    memory: Memory,
    alloc: TypedFunc<u32, u32>,
    fuel: u64,
    dead: bool,
}

impl Guest {
    /// Instantiates `module` and finds `memory` and `lattice_alloc`.
    pub(crate) fn new(engine: &Engine, module: &Module, limits: Limits) -> Result<Self, WasmError> {
        let mut store = store(engine, limits);
        let instance = Instance::new(&mut store, module, &[])
            .map_err(|e| WasmError::Compile(e.to_string()))?;
        let memory = instance
            .get_memory(&mut store, "memory")
            .ok_or(WasmError::Export("memory"))?;
        let alloc = instance
            .get_typed_func(&mut store, "lattice_alloc")
            .map_err(|_| WasmError::Export("lattice_alloc"))?;
        Ok(Self {
            store,
            instance,
            memory,
            alloc,
            fuel: limits.fuel_per_call,
            dead: false,
        })
    }

    /// The typed export `name`.
    pub(crate) fn func<P: WasmParams, R: WasmResults>(
        &mut self,
        name: &'static str,
    ) -> Result<TypedFunc<P, R>, WasmError> {
        self.instance
            .get_typed_func(&mut self.store, name)
            .map_err(|_| WasmError::Export(name))
    }

    /// True once a call failed.
    pub(crate) fn is_dead(&self) -> bool {
        self.dead
    }

    /// Marks the guest dead (a result that does not decode breaks the ABI).
    pub(crate) fn kill(&mut self) {
        self.dead = true;
    }

    /// Calls `f(ptr, len)` with `bytes` in a guest buffer and returns the result bytes.
    pub(crate) fn call_with<P: WasmParams>(
        &mut self,
        f: &TypedFunc<P, u64>,
        bytes: &[u8],
        args: impl FnOnce(u32, u32) -> P,
    ) -> Option<Vec<u8>> {
        self.guard(|g| {
            g.store.set_fuel(g.fuel).ok()?;
            let len = u32::try_from(bytes.len()).ok()?;
            let ptr = g.alloc.call(&mut g.store, len).ok()?;
            let at = usize::try_from(ptr).ok()?;
            g.memory.write(&mut g.store, at, bytes).ok()?;
            let packed = f.call(&mut g.store, args(ptr, len)).ok()?;
            g.read(packed)
        })
    }

    /// Calls a no-argument export and returns the result bytes.
    pub(crate) fn call0(&mut self, f: &TypedFunc<(), u64>) -> Option<Vec<u8>> {
        self.guard(|g| {
            g.store.set_fuel(g.fuel).ok()?;
            let packed = f.call(&mut g.store, ()).ok()?;
            g.read(packed)
        })
    }

    fn guard(&mut self, f: impl FnOnce(&mut Self) -> Option<Vec<u8>>) -> Option<Vec<u8>> {
        if self.dead {
            return None;
        }
        let out = f(self);
        if out.is_none() {
            self.dead = true;
        }
        out
    }

    /// The guest buffer `ptr << 32 | len`, copied out; `None` when it is outside memory.
    fn read(&self, packed: u64) -> Option<Vec<u8>> {
        let ptr = usize::try_from(packed >> 32).ok()?;
        let len = usize::try_from(packed & u64::from(u32::MAX)).ok()?;
        let data = self.memory.data(&self.store);
        data.get(ptr..ptr.checked_add(len)?).map(<[u8]>::to_vec)
    }
}
