//! `vt_diff` — the GATE G5 smoke target named by
//! .port/phase-0/plan/DEFINITION_OF_PARITY.md §G5 and driven by
//! .port/tools/gates/run_fuzz_smoke.sh.
//!
//! This is the stream harness (the broadest of the three reference harnesses:
//! every print/CSI/OSC/DCS/scroll path over a real Terminal). The
//! ref-vs-port DIFFERENTIAL comparison itself stays in the phase-0 comparator
//! (`.port/phase-0/harness`, `run_differential.sh`): the Zig reference cannot
//! be linked into a libFuzzer binary, so this target finds crashes/panics
//! (any artifact is a G5 RED) while the comparator replays the same shared
//! corpus through both oracles for byte-divergence.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    rustty_vt::fuzz_harness::fuzz_stream(data);
});
