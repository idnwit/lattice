//! Fuzz-style properties (.rules/verification.md): no panic on any bytes, any split.

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test harness helpers: a failure should abort the test"
)]

use lattice_proc_ipc::{FrameDecoder, FrameError, decode_reply, decode_request};
use proptest::prelude::*;
use proptest::test_runner::{Config, RngSeed};

const FIXTURES: &[&str] = &[
    "hello_request",
    "open_request",
    "spawn_request",
    "subscribe_request",
    "signal_request",
    "journals_reply",
    "entry_out_reply",
    "entry_exit_reply",
    "gap_reply",
    "error_reply",
];

fn config() -> Config {
    Config {
        cases: 2048,
        rng_seed: RngSeed::Fixed(0x1a77_1ce0),
        ..Config::default()
    }
}

fn fixture(i: usize) -> Vec<u8> {
    let name = FIXTURES[i % FIXTURES.len()];
    std::fs::read(format!(
        "{}/tests/fixtures/{name}.bin",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

/// Feeds `bytes` in `chunk`-sized pieces; decodes every frame both ways.
fn drive(bytes: &[u8], chunk: usize) -> Result<usize, FrameError> {
    let mut dec = FrameDecoder::new();
    let mut frames = 0;
    for piece in bytes.chunks(chunk.max(1)) {
        dec.push(piece);
        while let Some(frame) = dec.next_frame()? {
            let _req = decode_request(&frame);
            let _reply = decode_reply(&frame);
            frames += 1;
        }
    }
    Ok(frames)
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn random_bytes_never_panic(bytes in proptest::collection::vec(any::<u8>(), 0..512), chunk in 1usize..64) {
        // Any outcome is fine; the property is that it returns.
        drive(&bytes, chunk).ok();
    }

    #[test]
    fn mutated_golden_frames_never_panic(
        i in 0usize..FIXTURES.len(),
        flips in proptest::collection::vec((any::<usize>(), any::<u8>()), 1..6),
    ) {
        let mut bytes = fixture(i);
        let len = bytes.len();
        for (at, val) in flips {
            bytes[at % len] = val;
        }
        // Any outcome is fine; the property is that it returns.
        drive(&bytes, 7).ok();
    }

    #[test]
    fn frames_survive_any_split(i in 0usize..FIXTURES.len(), j in 0usize..FIXTURES.len(), chunk in 1usize..40) {
        let mut bytes = fixture(i);
        bytes.extend(fixture(j));
        prop_assert_eq!(drive(&bytes, chunk), Ok(2));
    }
}
