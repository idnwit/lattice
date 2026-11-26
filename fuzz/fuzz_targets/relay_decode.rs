//! Relay decoder: no panic on any bytes (01-transport.md, Relay invariant 1).
//!
//! ```sh
//! cd fuzz
//! mkdir -p corpus/relay_decode
//! cargo +nightly fuzz run relay_decode corpus/relay_decode -- -max_total_time=300
//! ```

#![no_main]

use lattice_relay_proto::{classify, decode};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    drop(classify(data));
    drop(decode(data));
});
