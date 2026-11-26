//! Contract: every v1 message survives encode then decode.

#![allow(
    clippy::unwrap_used,
    reason = "test harness helpers: a failure should abort the test"
)]

use lattice_proc_ipc::*;

fn jid(raw: &[u8]) -> JournalId {
    JournalId::try_new(raw.to_vec()).unwrap()
}

fn roundtrip(req: &Request) -> Request {
    let mut wire = Vec::new();
    encode_request(req, &mut wire);
    let mut dec = FrameDecoder::new();
    dec.push(&wire);
    let frame = dec.next_frame().unwrap().unwrap();
    decode_request(&frame).unwrap().unwrap()
}

#[test]

fn every_request_roundtrips() {
    let open = OpenSpec::try_new(jid(b"i1"), b"meta".to_vec(), 16 << 20).unwrap();
    let spawn = SpawnSpec::try_new(jid(b"i1"), vec![b"zsh".to_vec()], b"/".to_vec(), 80, 24);
    let cases = [
        Request::Hello {
            version: PROTOCOL_VERSION,
            features: 0,
        },
        Request::Open(open),
        Request::Spawn(spawn.unwrap()),
        Request::List,
        Request::Subscribe {
            journal: jid(b"i1"),
            from_off: Offset::from_raw(7),
        },
        Request::Trim {
            journal: jid(b"i1"),
            upto: Offset::from_raw(4096),
        },
        Request::Input {
            id: ProcId::from_raw(1),
            bytes: b"ls\r".to_vec(),
        },
        Request::Resize {
            id: ProcId::from_raw(1),
            cols: 120,
            rows: 40,
        },
        Request::Drop {
            journal: jid(b"i1"),
        },
    ];
    for case in cases {
        assert_eq!(roundtrip(&case), case, "case={case:?}");
    }
}

#[test]

fn unknown_type_decodes_to_none() {
    let mut dec = FrameDecoder::new();
    // len 0, type 0xFFFF, flags 0 (big-endian).
    dec.push(&[0, 0, 0, 0, 0xFF, 0xFF, 0, 0]);
    let frame = dec.next_frame().unwrap().unwrap();
    assert_eq!(decode_request(&frame), Ok(None));
}

#[test]

fn spawn_spec_rejects_empty_argv() {
    assert!(SpawnSpec::try_new(jid(b"i1"), Vec::new(), b"/".to_vec(), 80, 24).is_err());
}

#[test]

fn journal_id_rejects_empty() {
    assert!(JournalId::try_new(Vec::new()).is_err());
}

#[test]

fn entry_gap_and_error_replies_roundtrip() {
    let cases = [
        Reply::Entry {
            journal: jid(b"i1"),
            off: Offset::from_raw(0),
            entry: EntryKind::Out {
                proc: ProcId::from_raw(2),
                bytes: b"$ ".to_vec(),
            },
        },
        Reply::Entry {
            journal: jid(b"i1"),
            off: Offset::from_raw(2),
            entry: EntryKind::Exit {
                proc: ProcId::from_raw(2),
                status: ExitStatus::Code(0),
            },
        },
        Reply::Gap {
            journal: jid(b"i1"),
            new_start: Offset::from_raw(4096),
        },
        Reply::Trimmed {
            journal: jid(b"i1"),
            start_off: Offset::from_raw(4096),
        },
        Reply::Error {
            code: ErrorCode::Busy,
            msg: String::new(),
        },
    ];
    for case in cases {
        let mut wire = Vec::new();
        encode_reply(&case, &mut wire);
        let mut dec = FrameDecoder::new();
        dec.push(&wire);
        let frame = dec.next_frame().unwrap().unwrap();
        assert_eq!(
            decode_reply(&frame).unwrap().unwrap(),
            case,
            "case={case:?}"
        );
    }
}

#[test]

fn journals_reply_roundtrips() {
    let proc = ProcInfo::new(ProcId::from_raw(2), 4242, (80, 24), None);
    let range = (Offset::from_raw(3), Offset::from_raw(9));
    assert_eq!((proc.pid(), proc.size()), (4242, (80, 24)));
    let info = JournalInfo::new(jid(b"i1"), b"meta".to_vec(), range, vec![proc]);
    assert_eq!(info.journal().as_bytes(), b"i1");
    assert_eq!(
        (info.meta(), info.range(), info.procs().len()),
        (&b"meta"[..], range, 1)
    );
    let reply = Reply::Journals(vec![info]);
    let mut wire = Vec::new();
    encode_reply(&reply, &mut wire);
    let mut dec = FrameDecoder::new();
    dec.push(&wire);
    let frame = dec.next_frame().unwrap().unwrap();
    assert_eq!(decode_reply(&frame).unwrap().unwrap(), reply);
}

#[test]
fn limits_are_enforced_at_construction() {
    use lattice_proc_ipc::{MAX_META_LEN, MIN_JOURNAL_BUDGET};
    assert!(JournalId::try_new(vec![1; JournalId::MAX_LEN]).is_ok());
    assert!(JournalId::try_new(vec![1; JournalId::MAX_LEN + 1]).is_err());
    assert!(OpenSpec::try_new(jid(b"i"), Vec::new(), MIN_JOURNAL_BUDGET - 1).is_err());
    assert!(OpenSpec::try_new(jid(b"i"), vec![0; MAX_META_LEN + 1], MIN_JOURNAL_BUDGET).is_err());
    assert!(OpenSpec::try_new(jid(b"i"), vec![0; MAX_META_LEN], MIN_JOURNAL_BUDGET).is_ok());
}

#[test]
fn frames_above_the_max_are_refused() {
    use lattice_proc_ipc::{FrameDecoder, FrameError, MAX_FRAME_LEN};
    let mut dec = FrameDecoder::new();
    let len = u32::try_from(MAX_FRAME_LEN + 1).unwrap();
    let mut header = len.to_be_bytes().to_vec();
    header.extend_from_slice(&[0, 1, 0, 0]);
    dec.push(&header);
    assert_eq!(dec.next_frame(), Err(FrameError::TooLong));
}
