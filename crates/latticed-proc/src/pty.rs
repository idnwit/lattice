//! PTY spawn and control. The only `unsafe` in the workspace lands here.

#![allow(dead_code, reason = "L1 skeleton: used by the server once implemented")]

use lattice_proc_ipc::SpawnSpec;
use rustix::fd::OwnedFd;

/// A child running on a PTY master we own.
#[derive(Debug)]
pub(crate) struct Child {
    pub(crate) pid: u32,
    pub(crate) master: OwnedFd,
}

/// Opens a PTY pair, forks, and execs `spec` on the slave side.
#[allow(
    clippy::todo,
    unused_variables,
    reason = "L1 skeleton: body lands with its contract test"
)]
pub(crate) fn spawn(spec: &SpawnSpec) -> std::io::Result<Child> {
    // rustix::pty::openpt + grantpt + unlockpt; set winsize cols×rows
    // std::process::Command with pre_exec: setsid, TIOCSCTTY, dup2 slave to 0/1/2
    //   (the single #[allow(unsafe_code)] with a // SAFETY: comment)
    // master set O_NONBLOCK; return pid + master
    todo!()
}

/// Sets the PTY window size.
#[allow(
    clippy::todo,
    unused_variables,
    reason = "L1 skeleton: body lands with its contract test"
)]
pub(crate) fn resize(master: &OwnedFd, cols: u16, rows: u16) -> std::io::Result<()> {
    // rustix::termios::tcsetwinsize
    todo!()
}
