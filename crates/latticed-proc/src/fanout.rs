//! Subscriber cursors per process. A slow subscriber never blocks the PTY reader.

#![allow(dead_code, reason = "L1 skeleton: used by the server once implemented")]

use crate::ring::{Read, Ring};

/// Index of a connected client in the server's table.
pub(crate) type ClientIdx = usize;

/// Who reads a process and from where.
#[derive(Debug, Default)]
pub(crate) struct Fanout {
    cursors: Vec<(ClientIdx, u64)>,
}

impl Fanout {
    /// Starts streaming to `client` from `from_off`.
    #[allow(
        clippy::todo,
        unused_variables,
        reason = "L1 skeleton: body lands with its contract test"
    )]
    pub(crate) fn subscribe(&mut self, client: ClientIdx, from_off: u64) {
        // replace an existing cursor for client, else push
        todo!()
    }

    /// Stops streaming to `client`.
    #[allow(
        clippy::todo,
        unused_variables,
        reason = "L1 skeleton: body lands with its contract test"
    )]
    pub(crate) fn unsubscribe(&mut self, client: ClientIdx) {
        // remove its cursor
        todo!()
    }

    /// For each subscriber whose socket is writable, what to send next.
    ///
    /// Calls `emit(client, read)`; the caller advances with [`Fanout::advance`]
    /// after a successful non-blocking write. Lagging clients get `Gap`.
    #[allow(
        clippy::todo,
        unused_variables,
        reason = "L1 skeleton: body lands with its contract test"
    )]
    pub(crate) fn pending<'r>(&self, ring: &'r Ring, emit: impl FnMut(ClientIdx, Read<'r>)) {
        // for each cursor: emit(client, ring.read_from(off)) unless Empty
        todo!()
    }

    /// Moves `client`'s cursor to `off` after a write or a gap.
    #[allow(
        clippy::todo,
        unused_variables,
        reason = "L1 skeleton: body lands with its contract test"
    )]
    pub(crate) fn advance(&mut self, client: ClientIdx, off: u64) {
        // set the cursor; never move backwards
        todo!()
    }
}
