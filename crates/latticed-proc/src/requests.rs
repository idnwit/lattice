//! Request dispatch (04-worker.md, Local IPC table).

use lattice_proc_ipc::{
    ErrorCode, Frame, JournalId, JournalInfo, Offset, OpenSpec, PROTOCOL_VERSION, ProcId, ProcInfo,
    Reply, Request, SpawnSpec, decode_request,
};
use rustix::process::{Pid, Signal};

use crate::fanout::{ClientIdx, Fanout};
use crate::journal::Journal;
use crate::pty;
use crate::server::{INPUT_MAX, Proc, Server, Slot};

/// What a request answers: a reply, or nothing (`Subscribe`, `Input`, ...).
type Answer = Option<Reply>;

fn error(code: ErrorCode, msg: &str) -> Answer {
    Some(Reply::Error {
        code,
        msg: msg.to_owned(),
    })
}

impl Server {
    /// Handles one frame from client `i`. Returns `false` when the client must be closed.
    pub(crate) fn handle(&mut self, i: ClientIdx, frame: &Frame) -> bool {
        let answer = match decode_request(frame) {
            Ok(Some(req)) => self.dispatch(i, req),
            Ok(None) => error(ErrorCode::Unsupported, "unknown message type"),
            Err(e) => {
                self.reply(i, error(ErrorCode::Malformed, e.code()));
                self.flush(i);
                return false;
            }
        };
        self.reply(i, answer);
        true
    }

    fn reply(&mut self, i: ClientIdx, answer: Answer) {
        if let (Some(reply), Some(Some(c))) = (answer, self.clients.get_mut(i)) {
            c.send(&reply);
        }
    }

    fn dispatch(&mut self, i: ClientIdx, req: Request) -> Answer {
        match req {
            Request::Hello { .. } => Some(Reply::Hello {
                version: PROTOCOL_VERSION,
                features: 0,
            }),
            Request::Open(spec) => self.open(spec),
            Request::Spawn(spec) => self.spawn(&spec),
            Request::List => Some(Reply::Journals(self.list())),
            Request::Subscribe { journal, from_off } => {
                let Some(s) = self.journals.get_mut(&journal) else {
                    return error(ErrorCode::NoSuchJournal, "subscribe");
                };
                s.fanout.subscribe(i, from_off.raw());
                None
            }
            Request::Unsubscribe { journal } => {
                if let Some(s) = self.journals.get_mut(&journal) {
                    s.fanout.unsubscribe(i);
                }
                None
            }
            Request::Trim { journal, upto } => match self.journals.get_mut(&journal) {
                Some(s) => Some(Reply::Trimmed {
                    start_off: Offset::from_raw(s.journal.trim(upto.raw())),
                    journal,
                }),
                None => error(ErrorCode::NoSuchJournal, "trim"),
            },
            Request::Input { id, bytes } => self.input(id, &bytes),
            Request::Resize { id, cols, rows } => self.resize(id, cols, rows),
            Request::Signal { id, signo } => self.signal(id, signo),
            Request::Drop { journal } => self.drop_journal(journal),
        }
    }

    fn open(&mut self, spec: OpenSpec) -> Answer {
        let id = spec.journal().clone();
        if self.journals.contains_key(&id) {
            return error(ErrorCode::JournalExists, "open");
        }
        let journal = Journal::new(id.clone(), spec.meta().to_vec(), spec.budget());
        let fanout = Fanout::default();
        self.journals.insert(id.clone(), Slot { journal, fanout });
        Some(Reply::Opened { journal: id })
    }

    fn spawn(&mut self, spec: &SpawnSpec) -> Answer {
        if !self.journals.contains_key(spec.journal()) {
            return error(ErrorCode::NoSuchJournal, "spawn");
        }
        let child = match pty::spawn(spec) {
            Ok(child) => child,
            Err(e) => return error(ErrorCode::SpawnFailed, &e.kind().to_string()),
        };
        let id = self.next_proc;
        self.next_proc = self.next_proc.saturating_add(1);
        let proc = Proc {
            pid: child.pid,
            master: Some(child.master),
            journal: spec.journal().clone(),
            size: spec.size(),
            exit: None,
            input: Vec::new(),
        };
        self.procs.insert(id, proc);
        Some(Reply::Spawned {
            id: ProcId::from_raw(id),
            pid: child.pid,
        })
    }

    fn list(&self) -> Vec<JournalInfo> {
        let info = |(id, s): (&JournalId, &Slot)| {
            let procs = self.procs.iter().filter(|(_, p)| p.journal == *id);
            let procs =
                procs.map(|(pid, p)| ProcInfo::new(ProcId::from_raw(*pid), p.pid, p.size, p.exit));
            let range = (
                Offset::from_raw(s.journal.start_off()),
                Offset::from_raw(s.journal.end_off()),
            );
            JournalInfo::new(
                id.clone(),
                s.journal.meta().to_vec(),
                range,
                procs.collect(),
            )
        };
        self.journals.iter().map(info).collect()
    }

    fn input(&mut self, id: ProcId, bytes: &[u8]) -> Answer {
        let Some(p) = self.procs.get_mut(&id.raw()).filter(|p| p.master.is_some()) else {
            return error(ErrorCode::NoSuchProc, "input");
        };
        let room = INPUT_MAX.saturating_sub(p.input.len());
        p.input
            .extend_from_slice(bytes.get(..room).unwrap_or(bytes));
        self.write_input(id.raw());
        None
    }

    fn resize(&mut self, id: ProcId, cols: u16, rows: u16) -> Answer {
        let Some(p) = self.procs.get_mut(&id.raw()) else {
            return error(ErrorCode::NoSuchProc, "resize");
        };
        let Some(master) = &p.master else {
            return error(ErrorCode::NoSuchProc, "resize: exited");
        };
        if cols == 0 || rows == 0 || pty::resize(master, cols, rows).is_err() {
            return error(ErrorCode::Malformed, "resize");
        }
        p.size = (cols, rows);
        None
    }

    fn signal(&mut self, id: ProcId, signo: i32) -> Answer {
        let Some(p) = self.procs.get(&id.raw()).filter(|p| p.exit.is_none()) else {
            return error(ErrorCode::NoSuchProc, "signal");
        };
        let pid = i32::try_from(p.pid).ok().and_then(Pid::from_raw);
        match (pid, Signal::from_named_raw(signo)) {
            (Some(pid), Some(sig)) => match rustix::process::kill_process(pid, sig) {
                Ok(()) => None,
                Err(_) => error(ErrorCode::NoSuchProc, "signal"),
            },
            _ => error(ErrorCode::Malformed, "signal number"),
        }
    }

    fn drop_journal(&mut self, journal: JournalId) -> Answer {
        if !self.journals.contains_key(&journal) {
            return error(ErrorCode::NoSuchJournal, "drop");
        }
        let mut procs = self.procs.values().filter(|p| p.journal == journal);
        if procs.any(|p| p.exit.is_none()) {
            return error(ErrorCode::Busy, "a process of the journal lives");
        }
        self.procs.retain(|_, p| p.journal != journal);
        self.journals.remove(&journal);
        Some(Reply::Dropped { journal })
    }
}
