# Runner MAC provisional findings (P8-W9 T2)

## N600 - G6 RED: rustty-bench hyperlink_map::tests::hyperlink_map under Miri
- Lane: G6 (check_leaks.sh --all-done), SB, debug assertions on, at 1d22ba80d (code tree = a60e9e2a5 port).
- Test: rustty-bench `hyperlink_map::tests::hyperlink_map`.
- Miri error: "inline assembly is not supported", from the `reg_sink!` macro,
  `DoNotOptimize::sink` at crates/rustty-bench/src/lib.rs:194 (`core::arch::asm!`).
- Not fixed, gate not relaxed (R5). Controller dispatches the fix.
- Evidence: .port/phase-8/w9/G6-at-a60e9e2a5.txt

## N601 - G6 RED (provisional TIMEOUT): rustty-vt semantic_prompt_effect_not_called_when_the_terminal_fails
- Test: stream_terminal::tests::effects::semantic_prompt_effect_not_called_when_the_terminal_fails
  (crates/rustty-vt/src/stream_terminal/tests/effects.rs:586, P8-W5 T3, Rust-only).
- Under Miri SB it ran single-process at 100% CPU for over 2h40m with no completion (loop `while !has_induced_failure`, cap 100_000
  iterations of `x ESC]133;A BEL` filling pages with unlimited scrollback). Controller killed PID 93160 (SIGTERM). All other rustty-vt tests passed.
- Not a UB report; a Miri-cost problem. Not fixed, gate not relaxed.
- Evidence: .port/phase-8/w9/G6-at-a60e9e2a5.txt, logs copy of the nextest output under logs/n601-vt-output (not committed).
