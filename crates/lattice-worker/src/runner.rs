//! The runner contract. It lives in `lattice_app`, shared with WASM guests (D43).

pub use lattice_app::{DynRunner, Input, Mode, Step, hosted};

#[cfg(test)]
#[path = "runner_tests.rs"]
mod tests;
