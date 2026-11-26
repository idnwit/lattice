//! IPC v1 decoder: no panic on any bytes, split anywhere (.rules/verification.md, scope).
//!
//! The first byte picks the read size, so one input also covers split frames.
//! Seeds: the golden v1 fixtures. libFuzzer writes new inputs into the FIRST corpus dir
//! only, so that dir must be `corpus/` (gitignored), never the frozen fixtures:
//!
//! ```sh
//! cd fuzz
//! mkdir -p corpus/proc_ipc_decode
//! cargo +nightly fuzz run proc_ipc_decode corpus/proc_ipc_decode \
//!     ../crates/lattice-proc-ipc/tests/fixtures -- -max_total_time=300
//! ```

#![no_main]

use lattice_proc_ipc::{FrameDecoder, decode_reply, decode_request};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Some((&chunk, bytes)) = data.split_first() else {
        return;
    };
    let mut dec = FrameDecoder::new();
    for piece in bytes.chunks(usize::from(chunk).max(1)) {
        dec.push(piece);
        // A framing error ends the stream; any outcome is fine as long as it returns.
        while let Ok(Some(frame)) = dec.next_frame() {
            drop(decode_request(&frame));
            drop(decode_reply(&frame));
        }
    }
});
