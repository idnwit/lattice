//! `latticed-proc`: the frozen process daemon.
//!
//! Owner spec: `docs/architecture/04-worker.md`. Spawns children on PTYs, owns
//! the masters, reaps exits, and keeps a raw output ring per process. Serves
//! `latticed` over a unix socket with the `lattice-proc-ipc` v1 protocol.
//! See this directory's `AGENTS.md` for the frozen-daemon rules.

mod fanout;
mod pty;
mod ring;
mod server;

use std::process::ExitCode;

fn main() -> ExitCode {
    // socket path: $XDG_RUNTIME_DIR/lattice/proc.sock (mode 0700); TBD on macOS (no XDG_RUNTIME_DIR)?
    // Linux: PR_SET_CHILD_SUBREAPER before any spawn
    match server::Server::bind_default().and_then(server::Server::run) {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::FAILURE,
    }
}
