//! v1 decoders (04-worker.md, Local IPC). Unknown tags are skipped; the last copy of a
//! non-repeated tag wins.

use super::codec::{ERROR_CODES, tag, ty};
use super::{JournalInfo, OpenSpec, ProcInfo, Reply, Request, SpawnSpec};
use crate::frame::{Frame, FrameError, TlvIter};
use crate::ids::{EntryKind, ErrorCode, ExitStatus, JournalId, Offset, ProcId};

/// Fields of one payload, validated as a whole TLV sequence.
struct Fields<'a>(Vec<(u16, &'a [u8])>);

impl<'a> Fields<'a> {
    fn parse(bytes: &'a [u8]) -> Result<Self, FrameError> {
        TlvIter::new(bytes)
            .map(|t| t.map(|t| (t.tag(), t.value())))
            .collect::<Result<_, _>>()
            .map(Self)
    }
    fn of(frame: &'a Frame) -> Result<Self, FrameError> {
        frame
            .tlvs()
            .map(|t| t.map(|t| (t.tag(), t.value())))
            .collect::<Result<_, _>>()
            .map(Self)
    }
    fn get(&self, tag: u16) -> Option<&'a [u8]> {
        self.0
            .iter()
            .rev()
            .find(|(t, _)| *t == tag)
            .map(|(_, v)| *v)
    }
    fn all(&self, tag: u16) -> impl Iterator<Item = &'a [u8]> + '_ {
        self.0
            .iter()
            .filter(move |(t, _)| *t == tag)
            .map(|(_, v)| *v)
    }
    fn req(&self, tag: u16) -> Result<&'a [u8], FrameError> {
        self.get(tag).ok_or(FrameError::MissingTag(tag))
    }
    fn fixed<const N: usize>(&self, tag: u16) -> Result<[u8; N], FrameError> {
        <[u8; N]>::try_from(self.req(tag)?).map_err(|_| FrameError::BadValue(tag))
    }
    fn u16(&self, tag: u16) -> Result<u16, FrameError> {
        self.fixed(tag).map(u16::from_be_bytes)
    }
    fn u64(&self, tag: u16) -> Result<u64, FrameError> {
        self.fixed(tag).map(u64::from_be_bytes)
    }
    fn off(&self) -> Result<Offset, FrameError> {
        self.u64(tag::OFFSET).map(Offset::from_raw)
    }
    fn proc(&self) -> Result<ProcId, FrameError> {
        self.u64(tag::PROC).map(ProcId::from_raw)
    }
    fn journal(&self) -> Result<JournalId, FrameError> {
        JournalId::try_new(self.req(tag::JOURNAL)?.to_vec())
    }
    fn exit(&self) -> Result<Option<ExitStatus>, FrameError> {
        let Some(v) = self.get(tag::EXIT) else {
            return Ok(None);
        };
        let Some((kind, rest)) = v.split_first() else {
            return Err(FrameError::BadValue(tag::EXIT));
        };
        let n = <[u8; 4]>::try_from(rest).map_err(|_| FrameError::BadValue(tag::EXIT))?;
        let n = i32::from_be_bytes(n);
        match kind {
            0 => Ok(Some(ExitStatus::Code(n))),
            1 => Ok(Some(ExitStatus::Signal(n))),
            _ => Err(FrameError::BadValue(tag::EXIT)),
        }
    }
}

/// Parses a request frame. Unknown tags are ignored.
///
/// # Errors
///
/// [`FrameError`] on a missing or malformed field. An unknown type returns
/// `Ok(None)`: the server answers `Error{Unsupported}` and keeps the connection.
pub fn decode_request(frame: &Frame) -> Result<Option<Request>, FrameError> {
    let f = Fields::of(frame)?;
    let req = match frame.msg_type().raw() {
        ty::HELLO => Request::Hello {
            version: f.u16(tag::VERSION)?,
            features: f.u64(tag::FEATURES)?,
        },
        ty::OPEN => Request::Open(OpenSpec::try_new(
            f.journal()?,
            f.req(tag::META)?.to_vec(),
            f.u64(tag::BUDGET)?,
        )?),
        ty::SPAWN => Request::Spawn(decode_spawn(&f)?),
        ty::LIST => Request::List,
        ty::SUBSCRIBE => Request::Subscribe {
            journal: f.journal()?,
            from_off: f.off()?,
        },
        ty::UNSUBSCRIBE => Request::Unsubscribe {
            journal: f.journal()?,
        },
        ty::TRIM => Request::Trim {
            journal: f.journal()?,
            upto: f.off()?,
        },
        ty::INPUT => Request::Input {
            id: f.proc()?,
            bytes: f.req(tag::BYTES)?.to_vec(),
        },
        ty::RESIZE => Request::Resize {
            id: f.proc()?,
            cols: f.u16(tag::COLS)?,
            rows: f.u16(tag::ROWS)?,
        },
        ty::SIGNAL => Request::Signal {
            id: f.proc()?,
            signo: f.fixed(tag::SIGNO).map(i32::from_be_bytes)?,
        },
        ty::DROP => Request::Drop {
            journal: f.journal()?,
        },
        _ => return Ok(None),
    };
    Ok(Some(req))
}

fn decode_spawn(f: &Fields<'_>) -> Result<SpawnSpec, FrameError> {
    let argv = f.all(tag::ARGV).map(<[u8]>::to_vec).collect();
    let mut spec = SpawnSpec::try_new(
        f.journal()?,
        argv,
        f.req(tag::CWD)?.to_vec(),
        f.u16(tag::COLS)?,
        f.u16(tag::ROWS)?,
    )?;
    for kv in f.all(tag::ENV) {
        let eq = kv.iter().position(|b| *b == b'=');
        let (name, value) = kv
            .split_at_checked(eq.ok_or(FrameError::BadValue(tag::ENV))?)
            .ok_or(FrameError::BadValue(tag::ENV))?;
        let value = value.get(1..).ok_or(FrameError::BadValue(tag::ENV))?;
        spec = spec.with_env(name.to_vec(), value.to_vec())?;
    }
    Ok(spec)
}

/// Parses a reply frame. Same rules as [`decode_request`].
///
/// # Errors
///
/// [`FrameError`] on a missing or malformed field.
pub fn decode_reply(frame: &Frame) -> Result<Option<Reply>, FrameError> {
    let f = Fields::of(frame)?;
    let reply = match frame.msg_type().raw() {
        ty::R_HELLO => Reply::Hello {
            version: f.u16(tag::VERSION)?,
            features: f.u64(tag::FEATURES)?,
        },
        ty::R_OPENED => Reply::Opened {
            journal: f.journal()?,
        },
        ty::R_SPAWNED => Reply::Spawned {
            id: f.proc()?,
            pid: f.fixed(tag::PID).map(u32::from_be_bytes)?,
        },
        ty::R_JOURNALS => Reply::Journals(
            f.all(tag::JOURNAL_INFO)
                .map(decode_info)
                .collect::<Result<_, _>>()?,
        ),
        ty::R_ENTRY => Reply::Entry {
            journal: f.journal()?,
            off: f.off()?,
            entry: decode_entry(&f)?,
        },
        ty::R_GAP => Reply::Gap {
            journal: f.journal()?,
            new_start: f.off()?,
        },
        ty::R_TRIMMED => Reply::Trimmed {
            journal: f.journal()?,
            start_off: f.off()?,
        },
        ty::R_DROPPED => Reply::Dropped {
            journal: f.journal()?,
        },
        ty::R_ERROR => Reply::Error {
            code: decode_error_code(f.u16(tag::CODE)?)?,
            msg: String::from_utf8_lossy(f.get(tag::MSG).unwrap_or_default()).into_owned(),
        },
        _ => return Ok(None),
    };
    Ok(Some(reply))
}

fn decode_error_code(raw: u16) -> Result<ErrorCode, FrameError> {
    let pos = usize::from(raw.checked_sub(1).ok_or(FrameError::BadValue(tag::CODE))?);
    ERROR_CODES
        .get(pos)
        .copied()
        .ok_or(FrameError::BadValue(tag::CODE))
}

fn decode_entry(f: &Fields<'_>) -> Result<EntryKind, FrameError> {
    let proc = f.proc()?;
    match f.req(tag::KIND)? {
        [1] => Ok(EntryKind::Out {
            proc,
            bytes: f.req(tag::BYTES)?.to_vec(),
        }),
        [2] => Ok(EntryKind::Exit {
            proc,
            status: f.exit()?.ok_or(FrameError::MissingTag(tag::EXIT))?,
        }),
        _ => Err(FrameError::BadValue(tag::KIND)),
    }
}

fn decode_info(bytes: &[u8]) -> Result<JournalInfo, FrameError> {
    let f = Fields::parse(bytes)?;
    let procs = f
        .all(tag::PROC_INFO)
        .map(|b| {
            let p = Fields::parse(b)?;
            Ok(ProcInfo::new(
                p.proc()?,
                p.fixed(tag::PID).map(u32::from_be_bytes)?,
                (p.u16(tag::COLS)?, p.u16(tag::ROWS)?),
                p.exit()?,
            ))
        })
        .collect::<Result<_, FrameError>>()?;
    Ok(JournalInfo::new(
        f.journal()?,
        f.req(tag::META)?.to_vec(),
        (f.off()?, Offset::from_raw(f.u64(tag::END_OFF)?)),
        procs,
    ))
}
