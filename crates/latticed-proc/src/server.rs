//! The single-threaded `poll` loop: listener, clients, PTY masters, child exits.

#![allow(dead_code, reason = "L1 skeleton: used by main once implemented")]

use rustix::fd::OwnedFd;

/// Daemon state: listening socket, clients, processes.
#[derive(Debug)]
pub(crate) struct Server {
    listener: OwnedFd,
}

impl Server {
    /// Binds `$XDG_RUNTIME_DIR/lattice/proc.sock` with mode 0700.
    #[allow(
        clippy::todo,
        reason = "L1 skeleton: body lands with its contract test"
    )]
    pub(crate) fn bind_default() -> std::io::Result<Self> {
        // resolve the runtime dir; create lattice/ with 0700; unlink a stale socket; bind + listen
        todo!()
    }

    /// Runs forever.
    #[allow(
        clippy::todo,
        reason = "L1 skeleton: body lands with its contract test"
    )]
    pub(crate) fn run(self) -> std::io::Result<()> {
        // loop over rustix poll(listener, clients, masters):
        //   listener readable: accept; check peer uid (getpeereid / SO_PEERCRED) == ours
        //   client readable: FrameDecoder; decode_request; dispatch (unknown type → Error{Unsupported})
        //   master readable: read once into its Ring (single reader of each PTY)
        //   client writable: Fanout::pending → non-blocking write; partial writes keep the cursor
        //   SIGCHLD (self-pipe): waitpid; record exit; send Exited to subscribers
        todo!()
    }
}
