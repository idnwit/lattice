//! v1 type codes, TLV tag table, and the encoders (04-worker.md, Local IPC).
//!
//! Integers are big-endian. A field may repeat only where the table says so;
//! otherwise the last copy wins. Unknown tags are skipped.

use super::{JournalInfo, Reply, Request, SpawnSpec};
use crate::frame::{put_frame, put_tlv};
use crate::ids::{EntryKind, ErrorCode, ExitStatus, JournalId};

/// v1 TLV tags. One table for every message.
pub(crate) mod tag {
    pub(crate) const VERSION: u16 = 0x01; // `u16` protocol version.
    pub(crate) const FEATURES: u16 = 0x02; // `u64` feature bits.
    pub(crate) const JOURNAL: u16 = 0x03; // Journal id bytes, 1–64.
    pub(crate) const META: u16 = 0x04; // Opaque meta bytes.
    pub(crate) const BUDGET: u16 = 0x05; // `u64` byte budget.
    pub(crate) const ARGV: u16 = 0x06; // One argv element. Repeated, in order.
    pub(crate) const ENV: u16 = 0x07; // One env var as `NAME=VALUE`. Repeated, in order.
    pub(crate) const CWD: u16 = 0x08; // Working directory bytes; empty means inherit.
    pub(crate) const COLS: u16 = 0x09; // `u16` columns.
    pub(crate) const ROWS: u16 = 0x0A; // `u16` rows.
    pub(crate) const OFFSET: u16 = 0x0B; // `u64`: from_off, upto, off, new_start, start_off.
    pub(crate) const END_OFF: u16 = 0x0C; // `u64` end offset (exclusive).
    pub(crate) const PROC: u16 = 0x0D; // `u64` process id.
    pub(crate) const PID: u16 = 0x0E; // `u32` OS pid.
    pub(crate) const BYTES: u16 = 0x0F; // Raw bytes: `Input` and `Out`.
    pub(crate) const SIGNO: u16 = 0x10; // `i32` signal number.
    pub(crate) const KIND: u16 = 0x11; // `u8` entry kind: 1 `Out`, 2 `Exit`.
    pub(crate) const EXIT: u16 = 0x12; // 5 bytes: `u8` (0 code, 1 signal), `i32`.
    pub(crate) const CODE: u16 = 0x13; // `u16` error code.
    pub(crate) const MSG: u16 = 0x14; // UTF-8 error text.
    pub(crate) const JOURNAL_INFO: u16 = 0x15; // One journal record, nested TLV. Repeated.
    pub(crate) const PROC_INFO: u16 = 0x16; // Process record, nested in `JOURNAL_INFO`. Repeated.
}

/// v1 request and reply type codes.
pub(in crate::message) mod ty {
    pub(in crate::message) const HELLO: u16 = 1;
    pub(in crate::message) const OPEN: u16 = 2;
    pub(in crate::message) const SPAWN: u16 = 3;
    pub(in crate::message) const LIST: u16 = 4;
    pub(in crate::message) const SUBSCRIBE: u16 = 5;
    pub(in crate::message) const UNSUBSCRIBE: u16 = 6;
    pub(in crate::message) const TRIM: u16 = 7;
    pub(in crate::message) const INPUT: u16 = 8;
    pub(in crate::message) const RESIZE: u16 = 9;
    pub(in crate::message) const SIGNAL: u16 = 10;
    pub(in crate::message) const DROP: u16 = 11;
    pub(in crate::message) const R_HELLO: u16 = 0x8001;
    pub(in crate::message) const R_OPENED: u16 = 0x8002;
    pub(in crate::message) const R_SPAWNED: u16 = 0x8003;
    pub(in crate::message) const R_JOURNALS: u16 = 0x8004;
    pub(in crate::message) const R_ENTRY: u16 = 0x8005;
    pub(in crate::message) const R_GAP: u16 = 0x8006;
    pub(in crate::message) const R_TRIMMED: u16 = 0x8007;
    pub(in crate::message) const R_DROPPED: u16 = 0x8008;
    pub(in crate::message) const R_ERROR: u16 = 0x8009;
}

pub(in crate::message) const ERROR_CODES: [ErrorCode; 7] = [
    ErrorCode::Unsupported,
    ErrorCode::Malformed,
    ErrorCode::NoSuchProc,
    ErrorCode::NoSuchJournal,
    ErrorCode::JournalExists,
    ErrorCode::Busy,
    ErrorCode::SpawnFailed,
];

/// Wire value of an error code: its position in [`ERROR_CODES`], from 1.
fn error_code_raw(code: ErrorCode) -> u16 {
    let pos = ERROR_CODES.iter().position(|c| *c == code).unwrap_or(0);
    u16::try_from(pos).unwrap_or(0).saturating_add(1)
}

/// Payload builder.
#[derive(Default)]
struct P(Vec<u8>);

impl P {
    fn bytes(mut self, tag: u16, v: &[u8]) -> Self {
        put_tlv(&mut self.0, tag, v);
        self
    }
    fn u16(self, tag: u16, v: u16) -> Self {
        self.bytes(tag, &v.to_be_bytes())
    }
    fn u64(self, tag: u16, v: u64) -> Self {
        self.bytes(tag, &v.to_be_bytes())
    }
    fn journal(self, j: &JournalId) -> Self {
        self.bytes(tag::JOURNAL, j.as_bytes())
    }
    fn exit(self, s: ExitStatus) -> Self {
        let (k, v) = match s {
            ExitStatus::Code(c) => (0u8, c),
            ExitStatus::Signal(s) => (1u8, s),
        };
        let b = v.to_be_bytes();
        self.bytes(tag::EXIT, &[k, b[0], b[1], b[2], b[3]])
    }
    fn send(self, ty: u16, out: &mut Vec<u8>) {
        put_frame(out, ty, &self.0);
    }
}

/// Appends one framed request to `out`.
pub fn encode_request(req: &Request, out: &mut Vec<u8>) {
    let p = P::default();
    match req {
        Request::Hello { version, features } => p
            .u16(tag::VERSION, *version)
            .u64(tag::FEATURES, *features)
            .send(ty::HELLO, out),
        Request::Open(o) => p
            .journal(&o.journal)
            .bytes(tag::META, &o.meta)
            .u64(tag::BUDGET, o.budget)
            .send(ty::OPEN, out),
        Request::Spawn(s) => encode_spawn(s, p).send(ty::SPAWN, out),
        Request::List => p.send(ty::LIST, out),
        Request::Subscribe { journal, from_off } => p
            .journal(journal)
            .u64(tag::OFFSET, from_off.raw())
            .send(ty::SUBSCRIBE, out),
        Request::Unsubscribe { journal } => p.journal(journal).send(ty::UNSUBSCRIBE, out),
        Request::Trim { journal, upto } => p
            .journal(journal)
            .u64(tag::OFFSET, upto.raw())
            .send(ty::TRIM, out),
        Request::Input { id, bytes } => p
            .u64(tag::PROC, id.raw())
            .bytes(tag::BYTES, bytes)
            .send(ty::INPUT, out),
        Request::Resize { id, cols, rows } => p
            .u64(tag::PROC, id.raw())
            .u16(tag::COLS, *cols)
            .u16(tag::ROWS, *rows)
            .send(ty::RESIZE, out),
        Request::Signal { id, signo } => p
            .u64(tag::PROC, id.raw())
            .bytes(tag::SIGNO, &signo.to_be_bytes())
            .send(ty::SIGNAL, out),
        Request::Drop { journal } => p.journal(journal).send(ty::DROP, out),
    }
}

fn encode_spawn(s: &SpawnSpec, mut p: P) -> P {
    p = p.journal(&s.journal);
    for arg in &s.argv {
        p = p.bytes(tag::ARGV, arg);
    }
    for (name, value) in &s.env {
        let mut kv = Vec::with_capacity(name.len().saturating_add(value.len()).saturating_add(1));
        kv.extend_from_slice(name);
        kv.push(b'=');
        kv.extend_from_slice(value);
        p = p.bytes(tag::ENV, &kv);
    }
    p.bytes(tag::CWD, &s.cwd)
        .u16(tag::COLS, s.cols)
        .u16(tag::ROWS, s.rows)
}

/// Appends one framed reply to `out`.
pub fn encode_reply(reply: &Reply, out: &mut Vec<u8>) {
    let p = P::default();
    match reply {
        Reply::Hello { version, features } => p
            .u16(tag::VERSION, *version)
            .u64(tag::FEATURES, *features)
            .send(ty::R_HELLO, out),
        Reply::Opened { journal } => p.journal(journal).send(ty::R_OPENED, out),
        Reply::Spawned { id, pid } => p
            .u64(tag::PROC, id.raw())
            .bytes(tag::PID, &pid.to_be_bytes())
            .send(ty::R_SPAWNED, out),
        Reply::Journals(list) => list
            .iter()
            .fold(p, |p, j| p.bytes(tag::JOURNAL_INFO, &encode_info(j)))
            .send(ty::R_JOURNALS, out),
        Reply::Entry {
            journal,
            off,
            entry,
        } => {
            let p = p.journal(journal).u64(tag::OFFSET, off.raw());
            let p = match entry {
                EntryKind::Out { proc, bytes } => p
                    .bytes(tag::KIND, &[1])
                    .u64(tag::PROC, proc.raw())
                    .bytes(tag::BYTES, bytes),
                EntryKind::Exit { proc, status } => p
                    .bytes(tag::KIND, &[2])
                    .u64(tag::PROC, proc.raw())
                    .exit(*status),
            };
            p.send(ty::R_ENTRY, out);
        }
        Reply::Gap { journal, new_start } => p
            .journal(journal)
            .u64(tag::OFFSET, new_start.raw())
            .send(ty::R_GAP, out),
        Reply::Trimmed { journal, start_off } => p
            .journal(journal)
            .u64(tag::OFFSET, start_off.raw())
            .send(ty::R_TRIMMED, out),
        Reply::Dropped { journal } => p.journal(journal).send(ty::R_DROPPED, out),
        Reply::Error { code, msg } => p
            .u16(tag::CODE, error_code_raw(*code))
            .bytes(tag::MSG, msg.as_bytes())
            .send(ty::R_ERROR, out),
    }
}

fn encode_info(j: &JournalInfo) -> Vec<u8> {
    let p = P::default()
        .journal(&j.journal)
        .bytes(tag::META, &j.meta)
        .u64(tag::OFFSET, j.start_off.raw())
        .u64(tag::END_OFF, j.end_off.raw());
    let p = j.procs.iter().fold(p, |p, pr| {
        let mut q = P::default()
            .u64(tag::PROC, pr.id.raw())
            .bytes(tag::PID, &pr.pid.to_be_bytes())
            .u16(tag::COLS, pr.cols)
            .u16(tag::ROWS, pr.rows);
        if let Some(s) = pr.exit {
            q = q.exit(s);
        }
        p.bytes(tag::PROC_INFO, &q.0)
    });
    p.0
}
