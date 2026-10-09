# Runner MAC provisional findings (P8-W9 T2)

## N532 - G6 RED: rustty-bench hyperlink_map::tests::hyperlink_map under Miri
- Lane: G6 (check_leaks.sh --all-done), SB, debug assertions on, at 1d22ba80d (code tree = a60e9e2a5 port).
- Test: rustty-bench `hyperlink_map::tests::hyperlink_map`.
- Miri error: "inline assembly is not supported", from the `reg_sink!` macro,
  `DoNotOptimize::sink` at crates/rustty-bench/src/lib.rs:194 (`core::arch::asm!`).
- Not fixed, gate not relaxed (R5). Controller dispatches the fix.
- Evidence: .port/phase-8/w9/G6-at-a60e9e2a5.txt

## N533 - G6 RED (provisional TIMEOUT): rustty-vt semantic_prompt_effect_not_called_when_the_terminal_fails
- Test: stream_terminal::tests::effects::semantic_prompt_effect_not_called_when_the_terminal_fails
  (crates/rustty-vt/src/stream_terminal/tests/effects.rs:586, P8-W5 T3, Rust-only).
- Under Miri SB it ran single-process at 100% CPU for over 2h40m with no completion (loop `while !has_induced_failure`, cap 100_000
  iterations of `x ESC]133;A BEL` filling pages with unlimited scrollback). Controller killed PID 93160 (SIGTERM). All other rustty-vt tests passed.
- Not a UB report; a Miri-cost problem. Not fixed, gate not relaxed.
- Evidence: .port/phase-8/w9/G6-at-a60e9e2a5.txt, logs copy of the nextest output under logs/n601-vt-output (not committed).

## N534 - G-LSAN (Mac) RED: rustty-apprt-c, LeakSanitizer, 328 bytes in 3 allocations
- Lane: bash .port/tools/gates/sanitizers.sh --per-crate-target --gate lsan; every other crate GREEN (rustty-vt 3420 tests, vt-c 604, ...).
- All 16+19+... rustty-apprt-c tests PASSED; the leak is reported at process exit of the test binary `macos_window_host`
  (the 19-test binary, see log). Direct leak 144 B (+ indirect 128 B + 56 B).
- Allocation stack is Apple-internal: calloc <- libobjc class_createInstance <- libdispatch _dispatch_mach_create <- libxpc
  xpc_connection_create_mach_service <- TCC TCCAccessPreflight <- SkyLight __SLSInitialize_block_invoke (dispatch_once)
  <- SLSGetActiveDisplayList <- CVDisplayLinkCreateWithActiveCGDisplays <- rustty_renderer::metal::cvdisplaylink::DisplayLink::create_with_active_cg_displays
  <- Renderer::sync_display_link_raw <- loop_enter_raw <- rustty_core renderer_seam Thread::threadMain (render thread).
- Not matched by lsan-darwin.supp (S1-S9). Possibly a first-time-initialisation of SkyLight on a thread (the test hosts a render thread that
  creates a display link) or host-state dependent (TCC). Not diagnosed further; not suppressed (R5). Controller decides: new suppression with citation vs. code.
- Evidence: .port/phase-8/w9/G-LSAN-mac-at-a60e9e2a5.txt, G-LSAN-mac.rustty-apprt-c.log

## N535 - D1 (check_slice_reinterpret.sh, PROGRESS in ci.sh) RED on the Mac
- 3 UNLISTED reinterpret sites (crates/rustty-vt/src/bitmap_allocator.rs:880, page_impl.rs:2297, page_impl.rs:2467) and 4 STALE allowlist rows
  (page_impl.rs:2236, :2414, bitmap_allocator.rs:829, :861). Looks like line drift plus new sites; slice-reinterpret.tsv not updated.
- Observed standalone (the full ci.sh did not finish, see ci record). In ci.sh D1 is PROGRESS (non-blocking). Not fixed.
- Evidence: .port/phase-8/w9/D1-mac.full.log

## N536 - G-LINT (HARD) RED on the Mac
- shellcheck FAIL (157 files; SC2010 and SC2164 among the findings), prettier FAIL (docs/parity-audit/src-renderer.md, docs/parity-audit/src.md,
  docs/superpowers/plans/2026-10-07-phase8-upstream-resync.md), typos/swiftlint/actionlint ok, alejandra and pinact skipped (not installed / no GITHUB_TOKEN).
- Observed standalone. Not fixed, nothing relaxed.
- Evidence: .port/phase-8/w9/G-LINT-mac.full.log

N536 correction: of the shellcheck hits, only .port/tools/cvt-parity.sh:24 (SC2010) is a repository finding. The other four (SC2164 in
.port/phase-8/w9/g6-part2.sh, logs/g6p2.sh, logs/lane.sh, logs/rec.sh) are the runner's own helper scripts, which sat in the worktree during the lint run
(g6-part2.sh is renamed to .txt in the evidence dir; logs/ is untracked). A re-run on a clean checkout would show only cvt-parity.sh plus prettier.
