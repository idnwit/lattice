//! `latticed-proc`: the frozen process daemon.
//!
//! Owner spec: `docs/architecture/04-worker.md`. Spawns children on PTYs, owns
//! the masters, reaps exits, and keeps one journal per app instance: the output
//! and exits of its processes, in read order. Serves `latticed` over a unix
//! socket with the `lattice-proc-ipc` v1 protocol.
//! See this directory's `AGENTS.md` for the frozen-daemon rules.
//!
//! Socket path: [`lattice_proc_ipc::default_socket_path`] (`LATTICE_PROC_SOCK` overrides).

mod fanout;
mod journal;
mod pty;
mod requests;
mod server;

use std::process::ExitCode;

fn main() -> ExitCode {
    let path = lattice_proc_ipc::default_socket_path();
    #[cfg(target_os = "linux")]
    if let Err(e) = rustix::process::set_child_subreaper(Some(rustix::process::getpid())) {
        eprintln!("latticed-proc: PR_SET_CHILD_SUBREAPER: {e}");
        return ExitCode::FAILURE;
    }
    let server = match server::Server::bind(&path) {
        Ok(server) => server,
        Err(e) => {
            eprintln!("latticed-proc: bind {}: {e}", path.display());
            return ExitCode::FAILURE;
        }
    };
    match server.run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("latticed-proc: poll: {e}");
            ExitCode::FAILURE
        }
    }
}
