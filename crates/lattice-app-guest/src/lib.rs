//! WASM guest glue for app runners and viewers: ABI v0, core wasm + postcard (D43;
//! 06-apps.md, Runtimes).
//!
//! An app crate built for `wasm32-unknown-unknown` with feature `runner` calls
//! [`export_runner!`] once with its runner adapter. The guest then runs the whole runner core
//! ([`lattice_app::hosted`]): draft, commit, rollback, and effects are decided inside the
//! module. Built with feature `viewer`, it calls [`export_viewer!`] with its
//! [`lattice_app::view::Viewer`]: the guest holds the replica and renders view trees. The
//! host (`lattice-wasm`) only moves bytes. One module exports one of the two: both define
//! `lattice_abi` and `lattice_alloc`.
//!
//! # ABI v0
//!
//! All pointers and lengths are `u32` offsets into the guest's exported `memory`.
//! A result is one `u64`: `ptr << 32 | len` of a buffer the guest owns until its next call;
//! the host copies it out at once.
//!
//! | Export | Signature | Does |
//! |---|---|---|
//! | `lattice_abi` | `() -> u32` | ABI version, [`ABI_VERSION`] |
//! | `lattice_alloc` | `(len) -> ptr` | A buffer the host fills; the next call takes it |
//! | `lattice_init` | `(ptr, len, mode) -> u64` | Params in; `Option<Step>` out (`None`: bad params) |
//! | `lattice_step` | `(ptr, len, mode) -> u64` | An encoded `Input` in; a `Step` out |
//! | `lattice_snapshot` | `() -> u64` | The encoded state |
//!
//! `mode` is `0` for replay and `1` for live. Encodings are postcard (D19). An input that
//! does not decode gives a `Step` with error `guest.bad_input`. A panic aborts: the host sees
//! a trap and marks the instance `failed`.
//!
//! A viewer module exports `lattice_abi`, `lattice_alloc`, and:
//!
//! | Export | Signature | Does |
//! |---|---|---|
//! | `lattice_load` | `(ptr, len) -> u64` | `(version, full state)` in; `ReplicaEvent` out |
//! | `lattice_apply` | `(ptr, len) -> u64` | `(version, delta)` in; `ReplicaEvent` out |
//! | `lattice_render` | `() -> u64` | The `ViewTree` |
//! | `lattice_handle` | `(ptr, len) -> u64` | A `UiEvent` in; the actions (`Vec<Vec<u8>>`) out |
//!
//! An update that does not decode is a `Resync`; an event that does not decode sends nothing.
//!
//! TBD: `wake_at` export (the host has no timers yet), imports (none in v0).
//!
//! # Usage
//!
//! ```ignore
//! #[cfg(feature = "runner")]
//! lattice_app_guest::export_runner!(EchoApiRunner<Echo>);
//! ```
//!
//! # Safety
//!
//! The only `unsafe` is `take`: it rebuilds a buffer [`alloc`] leaked
//! (`.rules/engineering.md`, Lints).

use std::cell::RefCell;

use lattice_app::view::{DynViewer, UiEvent, Viewer, ViewerCore};
use lattice_app::{App, Dispatch, DynRunner, Input, Mode, ReplicaEvent, Step, hosted};

/// Version of this ABI. The host refuses a module with another one.
pub const ABI_VERSION: u32 = 0;

/// Step error code: the input did not decode.
const BAD_INPUT: &str = "guest.bad_input";

/// Step error code: `lattice_step` before a successful `lattice_init`.
pub const NO_RUNNER: &str = "guest.no_runner";

thread_local! {
    /// The one runner of this module instance (one instance per app instance).
    static RUNNER: RefCell<Option<Box<dyn DynRunner>>> = RefCell::new(None);
    /// The one viewer of this module instance, built on its first call.
    static VIEWER: RefCell<Option<Box<dyn DynViewer>>> = RefCell::new(None);
    /// The last result, kept until the next call.
    static OUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

/// Exports the ABI v0 functions for runner adapter `$runner` (`XApiRunner<App>`).
///
/// The functions exist on every target so the app crate type-checks them natively; only on
/// `wasm32` are they exported (`no_mangle`).
#[macro_export]
macro_rules! export_runner {
    ($runner:ty) => {
        // Natively nothing calls them: they are the module's exports on wasm32 only.
        #[allow(dead_code)]
        const _: () = {
            #[cfg_attr(target_arch = "wasm32", unsafe(no_mangle))]
            extern "C" fn lattice_abi() -> u32 {
                $crate::ABI_VERSION
            }
            #[cfg_attr(target_arch = "wasm32", unsafe(no_mangle))]
            extern "C" fn lattice_alloc(len: u32) -> u32 {
                $crate::alloc(len)
            }
            #[cfg_attr(target_arch = "wasm32", unsafe(no_mangle))]
            extern "C" fn lattice_init(ptr: u32, len: u32, mode: u32) -> u64 {
                $crate::init::<$runner>(ptr, len, mode)
            }
            #[cfg_attr(target_arch = "wasm32", unsafe(no_mangle))]
            extern "C" fn lattice_step(ptr: u32, len: u32, mode: u32) -> u64 {
                $crate::step(ptr, len, mode)
            }
            #[cfg_attr(target_arch = "wasm32", unsafe(no_mangle))]
            extern "C" fn lattice_snapshot() -> u64 {
                $crate::snapshot()
            }
        };
    };
}

/// Exports the viewer functions of ABI v0 for `$viewer` (a [`lattice_app::view::Viewer`]).
///
/// Like [`export_runner!`], the functions exist on every target and are exported on `wasm32`.
#[macro_export]
macro_rules! export_viewer {
    ($viewer:ty) => {
        // Natively nothing calls them: they are the module's exports on wasm32 only.
        #[allow(dead_code)]
        const _: () = {
            #[cfg_attr(target_arch = "wasm32", unsafe(no_mangle))]
            extern "C" fn lattice_abi() -> u32 {
                $crate::ABI_VERSION
            }
            #[cfg_attr(target_arch = "wasm32", unsafe(no_mangle))]
            extern "C" fn lattice_alloc(len: u32) -> u32 {
                $crate::alloc(len)
            }
            #[cfg_attr(target_arch = "wasm32", unsafe(no_mangle))]
            extern "C" fn lattice_load(ptr: u32, len: u32) -> u64 {
                $crate::update::<$viewer>(ptr, len, true)
            }
            #[cfg_attr(target_arch = "wasm32", unsafe(no_mangle))]
            extern "C" fn lattice_apply(ptr: u32, len: u32) -> u64 {
                $crate::update::<$viewer>(ptr, len, false)
            }
            #[cfg_attr(target_arch = "wasm32", unsafe(no_mangle))]
            extern "C" fn lattice_render() -> u64 {
                $crate::render::<$viewer>()
            }
            #[cfg_attr(target_arch = "wasm32", unsafe(no_mangle))]
            extern "C" fn lattice_handle(ptr: u32, len: u32) -> u64 {
                $crate::handle::<$viewer>(ptr, len)
            }
        };
    };
}

/// Runs `f` on this module's viewer, built from `V::default()` on first use.
fn with_viewer<V: Viewer + 'static, R>(f: impl FnOnce(&mut dyn DynViewer) -> R) -> R {
    VIEWER.with(|slot| {
        let mut slot = slot.borrow_mut();
        let viewer = slot.get_or_insert_with(|| Box::new(ViewerCore::<V>::default()));
        f(viewer.as_mut())
    })
}

// brickie: allow(interface/unused) -- called only from the export_runner!/export_viewer! expansions in app crates
/// `lattice_load` (`full`) and `lattice_apply`: one `(version, bytes)` update of the state.
pub fn update<V: Viewer + 'static>(ptr: u32, len: u32, full: bool) -> u64 {
    let bytes = take(ptr, len);
    let event = match lattice_state::decode_borrowed::<(u64, &[u8])>(&bytes) {
        Err(_) => ReplicaEvent::Resync,
        Ok((version, state)) => with_viewer::<V, _>(|v| {
            if full {
                v.load(version, state)
            } else {
                v.apply(version, state)
            }
        }),
    };
    emit(encode(&event))
}

// brickie: allow(interface/unused) -- called only from the export_runner!/export_viewer! expansions in app crates
/// `lattice_render`: the view tree of the state held.
pub fn render<V: Viewer + 'static>() -> u64 {
    let tree = with_viewer::<V, _>(|v| v.render());
    emit(encode(&tree))
}

// brickie: allow(interface/unused) -- called only from the export_runner!/export_viewer! expansions in app crates
/// `lattice_handle`: one encoded [`UiEvent`] in; the encoded actions out.
pub fn handle<V: Viewer + 'static>(ptr: u32, len: u32) -> u64 {
    let bytes = take(ptr, len);
    let actions = match lattice_state::decode::<UiEvent>(&bytes) {
        Err(_) => Vec::new(),
        Ok(event) => with_viewer::<V, _>(|v| v.handle(&event)),
    };
    emit(encode(&actions))
}

// brickie: allow(interface/unused) -- called only from the export_runner!/export_viewer! expansions in app crates
/// `lattice_alloc`: a zeroed buffer of `len` bytes, leaked until the next call takes it back.
pub fn alloc(len: u32) -> u32 {
    let buf: Box<[u8]> = vec![0; len as usize].into_boxed_slice();
    let ptr: *mut [u8] = Box::into_raw(buf);
    ptr.cast::<u8>() as usize as u32
}

/// Takes back a buffer from [`alloc`]. The host passes the `ptr`/`len` it got and filled.
#[allow(unsafe_code)]
fn take(ptr: u32, len: u32) -> Vec<u8> {
    if len == 0 {
        // `alloc(0)` hands out a dangling pointer to a zero-sized box; nothing to free.
        return Vec::new();
    }
    let slice = std::ptr::slice_from_raw_parts_mut(ptr as usize as *mut u8, len as usize);
    // SAFETY: the ABI contract is that `ptr`/`len` came from `alloc(len)` in this module and
    // are passed back exactly once, so this rebuilds the box `alloc` leaked, with its layout.
    let buf: Box<[u8]> = unsafe { Box::from_raw(slice) };
    buf.into_vec()
}

/// Stores `bytes` as the last result and returns `ptr << 32 | len`.
fn emit(bytes: Vec<u8>) -> u64 {
    OUT.with(|out| {
        let mut out = out.borrow_mut();
        *out = bytes;
        ((out.as_ptr() as usize as u64) << 32) | out.len() as u64
    })
}

fn mode(raw: u32) -> Mode {
    if raw == 0 { Mode::Replay } else { Mode::Live }
}

/// Encodes a value, or an empty buffer when it does not encode (the host reads that as a bad reply).
fn encode<T: serde::Serialize>(value: &T) -> Vec<u8> {
    lattice_state::encode(value).unwrap_or_default()
}

// brickie: allow(interface/unused) -- called only from the export_runner!/export_viewer! expansions in app crates
/// `lattice_init`: builds the runner of `D` from params and calls `start`.
pub fn init<D>(ptr: u32, len: u32, raw_mode: u32) -> u64
where
    D: Dispatch + 'static,
    D::App: Send,
    <D::App as App>::State: Send,
{
    let params = take(ptr, len);
    let built = hosted::<D>(&params, mode(raw_mode));
    let step = built.map(|(runner, step)| {
        RUNNER.with(|r| *r.borrow_mut() = Some(runner));
        step
    });
    emit(encode(&step))
}

// brickie: allow(interface/unused) -- called only from the export_runner!/export_viewer! expansions in app crates
/// `lattice_step`: feeds one encoded [`Input`] to the runner.
pub fn step(ptr: u32, len: u32, raw_mode: u32) -> u64 {
    let bytes = take(ptr, len);
    let step = match lattice_state::decode_borrowed::<Input<'_>>(&bytes) {
        Err(_) => refused(BAD_INPUT),
        Ok(input) => RUNNER.with(|r| match r.borrow_mut().as_mut() {
            Some(runner) => runner.step(input, mode(raw_mode)),
            None => refused(NO_RUNNER),
        }),
    };
    emit(encode(&step))
}

// brickie: allow(interface/unused) -- called only from the export_runner!/export_viewer! expansions in app crates
/// `lattice_snapshot`: the encoded state; empty before `init`.
pub fn snapshot() -> u64 {
    let bytes = RUNNER.with(|r| r.borrow().as_ref().map(|r| r.snapshot()));
    emit(bytes.unwrap_or_default())
}

fn refused(code: &str) -> Step {
    Step {
        error: Some(code.to_owned()),
        ..Step::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_zero_is_replay_and_anything_else_is_live() {
        assert_eq!(mode(0), Mode::Replay);
        assert_eq!(mode(1), Mode::Live);
    }

    #[test]
    fn a_step_before_init_or_with_a_bad_input_is_refused() {
        assert_eq!(refused(NO_RUNNER).error.as_deref(), Some(NO_RUNNER));
        assert!(take(0, 0).is_empty());
    }
}
