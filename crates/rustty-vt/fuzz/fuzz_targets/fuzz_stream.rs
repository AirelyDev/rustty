//! libFuzzer shim for the full terminal stream harness — the `zig_fuzz_test`
//! export of the reference `test/fuzz-libghostty/src/fuzz_stream.zig`. The
//! harness body (80x24 Terminal, 100-byte scrollback; byte 0 even →
//! `next_slice`, odd → scalar `next`) lives in
//! `rustty_vt::fuzz_harness::fuzz_stream`.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    rustty_vt::fuzz_harness::fuzz_stream(data);
});
