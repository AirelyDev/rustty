//! libFuzzer shim for the raw VT state machine harness — the `zig_fuzz_test`
//! export of the reference `test/fuzz-libghostty/src/fuzz_parser.zig`. The
//! harness body (`Parser::next` per byte, no terminal) lives in
//! `rustty_vt::fuzz_harness::fuzz_parser`.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    rustty_vt::fuzz_harness::fuzz_parser(data);
});
