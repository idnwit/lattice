//! The single-threaded `poll` loop: listener, clients, PTY masters, child exits.
//!
//! Exits are found by `wait(WNOHANG)` after every wake; while a child lives
//! the loop wakes at least every [`REAP_INTERVAL_MS`]. No signal handler.

use std::collections::BTreeMap;
use std::io::{ErrorKind, Read as _, Write as _};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;

use lattice_proc_ipc::{
    EntryKind, ExitStatus, FrameDecoder, JournalId, Offset, ProcId, Reply, encode_reply,
};
use rustix::event::{PollFd, PollFlags, Timespec};
use rustix::fd::OwnedFd;
use rustix::process::{Pid, WaitOptions};

use crate::fanout::{ClientIdx, Fanout};
use crate::journal::{Journal, Read};

/// Poll timeout while a child lives, to reap exits without a SIGCHLD handler.
const REAP_INTERVAL_MS: i64 = 50;
/// Bytes read per `read` from a client or a PTY.
const READ_CHUNK: usize = 64 << 10;
/// Fan-out stops filling a client's output buffer at this size; reads pause too.
pub(crate) const OUT_HIGH: usize = 256 << 10;
/// A client whose output buffer passes this (replies it never reads) is disconnected.
pub(crate) const OUT_MAX: usize = 4 << 20;
/// Most chunks drained from a PTY when its child is reaped (4 MiB).
const DRAIN_ROUNDS: usize = 64;
/// Pending PTY input per process; excess input is dropped.
pub(crate) const INPUT_MAX: usize = 1 << 20;

/// One connected `latticed`.
#[derive(Debug)]
pub(crate) struct Client {
    sock: UnixStream,
    dec: FrameDecoder,
    pub(crate) out: Vec<u8>,
}

impl Client {
    /// Queues one reply.
    pub(crate) fn send(&mut self, reply: &Reply) {
        encode_reply(reply, &mut self.out);
    }
}

/// A journal and who reads it.
#[derive(Debug)]
pub(crate) struct Slot {
    pub(crate) journal: Journal,
    pub(crate) fanout: Fanout,
}

/// One spawned process. Kept after exit until its journal is dropped.
#[derive(Debug)]
pub(crate) struct Proc {
    pub(crate) pid: u32,
    pub(crate) master: Option<OwnedFd>,
    pub(crate) journal: JournalId,
    pub(crate) size: (u16, u16),
    pub(crate) exit: Option<ExitStatus>,
    pub(crate) input: Vec<u8>,
}

/// What a poll slot belongs to.
#[derive(Debug, Clone, Copy)]
enum Src {
    Listener,
    Client(ClientIdx),
    Master(u64),
}

/// Daemon state: listening socket, clients, journals, processes.
#[derive(Debug)]
pub(crate) struct Server {
    listener: UnixListener,
    pub(crate) clients: Vec<Option<Client>>,
    pub(crate) journals: BTreeMap<JournalId, Slot>,
    pub(crate) procs: BTreeMap<u64, Proc>,
    pub(crate) next_proc: u64,
}

impl Server {
    /// Binds `path`. Creates its directory with mode 0700 and removes a stale socket.
    ///
    /// # Errors
    ///
    /// `AddrInUse` when a daemon already answers on `path`; any bind error.
    pub(crate) fn bind(path: &Path) -> std::io::Result<Self> {
        if let Some(dir) = path.parent() {
            use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
            std::fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(dir)?;
            std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
        }
        if UnixStream::connect(path).is_ok() {
            return Err(ErrorKind::AddrInUse.into());
        }
        match std::fs::remove_file(path) {
            Err(e) if e.kind() != ErrorKind::NotFound => return Err(e),
            _ => {}
        }
        let listener = UnixListener::bind(path)?;
        listener.set_nonblocking(true)?;
        Ok(Self {
            listener,
            clients: Vec::new(),
            journals: BTreeMap::new(),
            procs: BTreeMap::new(),
            next_proc: 1,
        })
    }

    /// Runs forever.
    ///
    /// # Errors
    ///
    /// Only when `poll` itself fails.
    pub(crate) fn run(mut self) -> std::io::Result<()> {
        loop {
            self.pump();
            for (src, ev) in self.wait()? {
                match src {
                    Src::Listener => self.accept(),
                    Src::Client(i) => self.on_client(i, ev),
                    Src::Master(id) => self.on_master(id, ev),
                }
            }
            self.reap();
        }
    }

    /// Polls every fd once; returns the ready ones.
    fn wait(&self) -> std::io::Result<Vec<(Src, PollFlags)>> {
        let mut srcs = vec![Src::Listener];
        let mut fds = vec![PollFd::new(&self.listener, PollFlags::IN)];
        for (i, c) in self.clients.iter().enumerate() {
            let Some(c) = c else { continue };
            let mut ev = PollFlags::empty();
            ev.set(PollFlags::IN, c.out.len() < OUT_HIGH);
            ev.set(PollFlags::OUT, !c.out.is_empty());
            srcs.push(Src::Client(i));
            fds.push(PollFd::new(&c.sock, ev));
        }
        for (id, p) in &self.procs {
            let Some(master) = &p.master else { continue };
            let ev = if p.input.is_empty() {
                PollFlags::IN
            } else {
                PollFlags::IN | PollFlags::OUT
            };
            srcs.push(Src::Master(*id));
            fds.push(PollFd::new(master, ev));
        }
        let live = self.procs.values().any(|p| p.exit.is_none());
        let tick = Timespec {
            tv_sec: 0,
            tv_nsec: REAP_INTERVAL_MS * 1_000_000,
        };
        match rustix::event::poll(&mut fds, live.then_some(&tick)) {
            Ok(_) => {}
            Err(rustix::io::Errno::INTR) => return Ok(Vec::new()),
            Err(e) => return Err(e.into()),
        }
        let ready = fds.iter().map(PollFd::revents);
        Ok(srcs
            .into_iter()
            .zip(ready)
            .filter(|(_, ev)| !ev.is_empty())
            .collect())
    }

    /// Accepts every pending connection from our own uid.
    fn accept(&mut self) {
        while let Ok((sock, _)) = self.listener.accept() {
            if !same_uid(&sock) || sock.set_nonblocking(true).is_err() {
                continue;
            }
            let client = Client {
                sock,
                dec: FrameDecoder::new(),
                out: Vec::new(),
            };
            match self.clients.iter().position(Option::is_none) {
                Some(i) => self.clients[i] = Some(client),
                None => self.clients.push(Some(client)),
            }
        }
    }

    fn on_client(&mut self, i: ClientIdx, ev: PollFlags) {
        if ev.contains(PollFlags::OUT) {
            self.flush(i);
        }
        if ev.intersects(PollFlags::IN | PollFlags::HUP | PollFlags::ERR) {
            self.read_client(i);
        }
    }

    fn read_client(&mut self, i: ClientIdx) {
        let Some(Some(c)) = self.clients.get_mut(i) else {
            return;
        };
        let mut buf = vec![0; READ_CHUNK];
        match c.sock.read(&mut buf) {
            Ok(0) => return self.disconnect(i),
            Ok(n) => c.dec.push(&buf[..n]),
            Err(e) if e.kind() == ErrorKind::WouldBlock || e.kind() == ErrorKind::Interrupted => {
                return;
            }
            Err(_) => return self.disconnect(i),
        }
        loop {
            let Some(Some(c)) = self.clients.get_mut(i) else {
                return;
            };
            match c.dec.next_frame() {
                Ok(None) => break,
                Ok(Some(frame)) => {
                    if !self.handle(i, &frame) {
                        return self.disconnect(i);
                    }
                }
                Err(_) => return self.disconnect(i),
            }
        }
        self.flush(i);
    }

    /// Writes as much queued output as the socket takes, without blocking.
    pub(crate) fn flush(&mut self, i: ClientIdx) {
        let Some(Some(c)) = self.clients.get_mut(i) else {
            return;
        };
        while !c.out.is_empty() {
            match c.sock.write(&c.out) {
                Ok(0) => return self.disconnect(i),
                Ok(n) => drop(c.out.drain(..n)),
                Err(e)
                    if e.kind() == ErrorKind::WouldBlock || e.kind() == ErrorKind::Interrupted =>
                {
                    break;
                }
                Err(_) => return self.disconnect(i),
            }
        }
        if c.out.len() > OUT_MAX {
            self.disconnect(i);
        }
    }

    pub(crate) fn disconnect(&mut self, i: ClientIdx) {
        if let Some(slot) = self.clients.get_mut(i) {
            *slot = None;
        }
        for s in self.journals.values_mut() {
            s.fanout.unsubscribe(i);
        }
    }

    /// Moves journal entries into subscriber buffers, up to [`OUT_HIGH`] each.
    fn pump(&mut self) {
        let clients = &mut self.clients;
        for slot in self.journals.values_mut() {
            slot.fanout.pump(&slot.journal, |ci, read| {
                let Some(Some(c)) = clients.get_mut(ci) else {
                    return false;
                };
                if c.out.len() >= OUT_HIGH {
                    return false;
                }
                let journal = slot.journal.id().clone();
                match read {
                    Read::Entry { off, entry, .. } => {
                        let (off, entry) = (Offset::from_raw(*off), (*entry).clone());
                        c.send(&Reply::Entry {
                            journal,
                            off,
                            entry,
                        });
                    }
                    Read::Gap { new_start } => {
                        c.send(&Reply::Gap {
                            journal,
                            new_start: Offset::from_raw(*new_start),
                        });
                    }
                    Read::Empty => return false,
                }
                true
            });
        }
        for i in 0..self.clients.len() {
            self.flush(i);
        }
    }

    fn on_master(&mut self, id: u64, ev: PollFlags) {
        if ev.contains(PollFlags::OUT) {
            self.write_input(id);
        }
        if ev.intersects(PollFlags::IN | PollFlags::HUP | PollFlags::ERR) {
            self.read_master(id, 1);
        }
    }

    /// Reads up to `rounds` chunks from a PTY into its journal. Closes the master on EOF.
    fn read_master(&mut self, id: u64, rounds: usize) {
        let Some(p) = self.procs.get_mut(&id) else {
            return;
        };
        let mut buf = vec![0; READ_CHUNK];
        for _ in 0..rounds {
            let Some(master) = &p.master else { return };
            match rustix::io::read(master, &mut buf) {
                Ok(0) => p.master = None,
                Ok(n) => {
                    if let Some(s) = self.journals.get_mut(&p.journal) {
                        let bytes = buf[..n].to_vec();
                        s.journal.push(EntryKind::Out {
                            proc: ProcId::from_raw(id),
                            bytes,
                        });
                    }
                }
                Err(rustix::io::Errno::AGAIN | rustix::io::Errno::INTR) => return,
                // EIO: every slave fd closed (Linux); the child is gone or going.
                Err(_) => p.master = None,
            }
        }
    }

    /// Writes pending input to a PTY without blocking.
    pub(crate) fn write_input(&mut self, id: u64) {
        let Some(p) = self.procs.get_mut(&id) else {
            return;
        };
        let Some(master) = &p.master else {
            return p.input.clear();
        };
        while !p.input.is_empty() {
            match rustix::io::write(master, &p.input) {
                Ok(n) => drop(p.input.drain(..n)),
                Err(rustix::io::Errno::AGAIN | rustix::io::Errno::INTR) => break,
                Err(_) => return p.input.clear(),
            }
        }
    }

    /// Reaps every exited child: drains its PTY, then appends `Exit` to its journal.
    ///
    /// `wait` (pid -1), not `waitpid(None)` (pid 0): children run in their own sessions.
    fn reap(&mut self) {
        while let Ok(Some((pid, status))) = rustix::process::wait(WaitOptions::NOHANG) {
            let status = match (status.exit_status(), status.terminating_signal()) {
                (Some(code), _) => ExitStatus::Code(code),
                (None, Some(sig)) => ExitStatus::Signal(sig),
                (None, None) => continue,
            };
            let pid = pid_u32(pid);
            let found = self
                .procs
                .iter()
                .find(|(_, p)| p.pid == pid && p.exit.is_none());
            let Some(id) = found.map(|(id, _)| *id) else {
                continue;
            };
            self.read_master(id, DRAIN_ROUNDS);
            let Some(p) = self.procs.get_mut(&id) else {
                continue;
            };
            p.master = None;
            p.input.clear();
            p.exit = Some(status);
            if let Some(s) = self.journals.get_mut(&p.journal) {
                s.journal.push(EntryKind::Exit {
                    proc: ProcId::from_raw(id),
                    status,
                });
            }
        }
    }
}

fn pid_u32(pid: Pid) -> u32 {
    u32::try_from(pid.as_raw_nonzero().get()).unwrap_or(0)
}

/// Peer uid check. Linux: `SO_PEERCRED`. macOS: rustix has no `getpeereid`; the 0700 dir guards.
fn same_uid(sock: &UnixStream) -> bool {
    #[cfg(target_os = "linux")]
    return rustix::net::sockopt::socket_peercred(sock)
        .is_ok_and(|c| c.uid == rustix::process::getuid());
    #[cfg(not(target_os = "linux"))]
    return sock.peer_addr().is_ok();
}
