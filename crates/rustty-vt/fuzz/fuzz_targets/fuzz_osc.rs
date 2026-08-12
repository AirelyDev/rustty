//! libFuzzer shim for the OSC parser harness — the `zig_fuzz_test` export of
//! the reference `test/fuzz-libghostty/src/fuzz_osc.zig`. The harness body
//! (terminator-selector byte 0, byte loop through `osc::Parser::next`, then
//! `.end(terminator)`) lives in `rustty_vt::fuzz_harness::fuzz_osc`.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    rustty_vt::fuzz_harness::fuzz_osc(data);
});
