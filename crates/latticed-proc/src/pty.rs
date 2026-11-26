//! PTY spawn and control. The only `unsafe` in the workspace lands here.

use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};

use lattice_proc_ipc::SpawnSpec;
use rustix::fd::OwnedFd;
use rustix::fs::{Mode, OFlags};
use rustix::pty::OpenptFlags;
use rustix::termios::Winsize;

/// A child running on a PTY master we own.
#[derive(Debug)]
pub(crate) struct Child {
    pub(crate) pid: u32,
    pub(crate) master: OwnedFd,
}

/// Opens a PTY pair, forks, and execs `spec` on the slave side.
///
/// The child gets a new session with the slave as controlling tty and as fds
/// 0/1/2. Its env is the daemon's env with `spec`'s vars merged on top.
pub(crate) fn spawn(spec: &SpawnSpec) -> std::io::Result<Child> {
    let master = rustix::pty::openpt(OpenptFlags::RDWR | OpenptFlags::NOCTTY)?;
    rustix::io::fcntl_setfd(&master, rustix::io::FdFlags::CLOEXEC)?;
    rustix::pty::grantpt(&master)?;
    rustix::pty::unlockpt(&master)?;
    let name = rustix::pty::ptsname(&master, Vec::new())?;
    let flags = OFlags::RDWR | OFlags::NOCTTY | OFlags::CLOEXEC;
    let slave = rustix::fs::open(name.as_c_str(), flags, Mode::empty())?;
    let (cols, rows) = spec.size();
    resize(&master, cols, rows)?;

    let (program, args) = spec
        .argv()
        .split_first()
        .ok_or(std::io::ErrorKind::InvalidInput)?;
    let mut cmd = Command::new(OsStr::from_bytes(program));
    cmd.args(args.iter().map(|a| OsStr::from_bytes(a)))
        .envs(
            spec.env()
                .iter()
                .map(|(k, v)| (OsStr::from_bytes(k), OsStr::from_bytes(v))),
        )
        .stdin(Stdio::from(slave.try_clone()?))
        .stdout(Stdio::from(slave.try_clone()?))
        .stderr(Stdio::from(slave.try_clone()?));
    if !spec.cwd().is_empty() {
        cmd.current_dir(OsStr::from_bytes(spec.cwd()));
    }
    #[allow(
        unsafe_code,
        reason = "pre_exec is the only way to make the PTY the controlling tty"
    )]
    // SAFETY: the closure runs in the forked child before exec. It only calls
    // setsid(2) and ioctl(TIOCSCTTY), both async-signal-safe raw syscalls that
    // neither allocate nor take locks. `slave` stays open until exec (CLOEXEC).
    unsafe {
        cmd.pre_exec(move || {
            rustix::process::setsid()?;
            rustix::process::ioctl_tiocsctty(&slave)?;
            Ok(())
        });
    }
    let child = cmd.spawn()?;
    rustix::io::ioctl_fionbio(&master, true)?;
    Ok(Child {
        pid: child.id(),
        master,
    })
}

/// Sets the PTY window size.
pub(crate) fn resize(master: &OwnedFd, cols: u16, rows: u16) -> std::io::Result<()> {
    let size = Winsize {
        ws_row: rows,
        ws_col: cols,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    Ok(rustix::termios::tcsetwinsize(master, size)?)
}
