//! v1 wire gate (.rules/verification.md, CI): checked-in frames decode to the same
//! messages forever. Fixtures under `tests/fixtures/` are never regenerated.

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test harness helpers: a failure should abort the test"
)]

use lattice_proc_ipc::{
    EntryKind, ErrorCode, ExitStatus, Frame, FrameDecoder, JournalId, JournalInfo, Offset,
    OpenSpec, ProcId, ProcInfo, Reply, Request, SpawnSpec, decode_reply, decode_request,
    encode_reply, encode_request,
};

fn jid() -> JournalId {
    JournalId::try_new(7u64.to_be_bytes().to_vec()).unwrap()
}

fn requests() -> Vec<(&'static str, Request)> {
    let spawn = SpawnSpec::try_new(
        jid(),
        vec![b"/bin/sh".to_vec(), b"-l".to_vec()],
        b"/home/u".to_vec(),
        80,
        24,
    )
    .unwrap()
    .with_env(b"LATTICE_PROC_ID".to_vec(), b"a=b".to_vec())
    .unwrap();
    vec![
        (
            "hello_request",
            Request::Hello {
                version: 1,
                features: 0,
            },
        ),
        (
            "open_request",
            Request::Open(OpenSpec::try_new(jid(), b"m".to_vec(), 16 << 20).unwrap()),
        ),
        ("spawn_request", Request::Spawn(spawn)),
        (
            "subscribe_request",
            Request::Subscribe {
                journal: jid(),
                from_off: Offset::from_raw(42),
            },
        ),
        (
            "signal_request",
            Request::Signal {
                id: ProcId::from_raw(3),
                signo: -2,
            },
        ),
    ]
}

fn replies() -> Vec<(&'static str, Reply)> {
    let procs = vec![
        ProcInfo::new(ProcId::from_raw(1), 100, (80, 24), None),
        ProcInfo::new(
            ProcId::from_raw(2),
            101,
            (132, 50),
            Some(ExitStatus::Signal(9)),
        ),
    ];
    let info = JournalInfo::new(
        jid(),
        b"m".to_vec(),
        (Offset::from_raw(4), Offset::from_raw(9)),
        procs,
    );
    vec![
        ("journals_reply", Reply::Journals(vec![info])),
        (
            "entry_out_reply",
            Reply::Entry {
                journal: jid(),
                off: Offset::from_raw(0),
                entry: EntryKind::Out {
                    proc: ProcId::from_raw(1),
                    bytes: b"hello\r\n".to_vec(),
                },
            },
        ),
        (
            "entry_exit_reply",
            Reply::Entry {
                journal: jid(),
                off: Offset::from_raw(7),
                entry: EntryKind::Exit {
                    proc: ProcId::from_raw(1),
                    status: ExitStatus::Code(3),
                },
            },
        ),
        (
            "gap_reply",
            Reply::Gap {
                journal: jid(),
                new_start: Offset::from_raw(4096),
            },
        ),
        (
            "error_reply",
            Reply::Error {
                code: ErrorCode::Busy,
                msg: "busy".into(),
            },
        ),
    ]
}

fn fixture(name: &str) -> Vec<u8> {
    let path = format!("{}/tests/fixtures/{name}.bin", env!("CARGO_MANIFEST_DIR"));
    std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn one_frame(bytes: &[u8]) -> Frame {
    let mut dec = FrameDecoder::new();
    dec.push(bytes);
    let frame = dec.next_frame().unwrap().unwrap();
    assert_eq!(dec.next_frame(), Ok(None));
    frame
}

#[test]
fn golden_v1_requests_decode() {
    for (name, want) in requests() {
        let got = decode_request(&one_frame(&fixture(name))).unwrap();
        assert_eq!(got, Some(want), "case={name}");
    }
}

#[test]
fn golden_v1_replies_decode() {
    for (name, want) in replies() {
        let got = decode_reply(&one_frame(&fixture(name))).unwrap();
        assert_eq!(got, Some(want), "case={name}");
    }
}

#[test]
fn encoder_still_writes_the_golden_bytes() {
    for (name, req) in requests() {
        let mut wire = Vec::new();
        encode_request(&req, &mut wire);
        assert_eq!(wire, fixture(name), "case={name}");
    }
    for (name, reply) in replies() {
        let mut wire = Vec::new();
        encode_reply(&reply, &mut wire);
        assert_eq!(wire, fixture(name), "case={name}");
    }
}

#[test]
fn unknown_tag_is_ignored() {
    let mut wire = fixture("subscribe_request");
    // Append TLV tag 0x7777, len 2; bump the u32 payload length by 8.
    wire.extend_from_slice(&[0x77, 0x77, 0, 0, 0, 2, 0xAB, 0xCD]);
    let len = u32::from_be_bytes(wire[..4].try_into().unwrap()) + 8;
    wire[..4].copy_from_slice(&len.to_be_bytes());
    let want = requests()
        .into_iter()
        .find(|(n, _)| *n == "subscribe_request")
        .unwrap()
        .1;
    assert_eq!(decode_request(&one_frame(&wire)).unwrap(), Some(want));
}

#[test]
fn unknown_reply_type_decodes_to_none() {
    let frame = one_frame(&[0, 0, 0, 0, 0x80, 0xFF, 0, 0]);
    assert_eq!(decode_reply(&frame), Ok(None));
}
