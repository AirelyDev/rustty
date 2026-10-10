> Ids here are provisional, kept verbatim: N565 → N547, N566 → N549, N567 → N548 (see `.port/phase-8/README.md`, "Provisional id renumbering (P8-W10 FINAL)").

# P8-W10 FINAL findings (provisional ids; controller renumbers)

## N565 (provisional): G-TSAN RED on Mac, rustty-vt-c sys_log_no_alloc
- Gate: G-TSAN (inside Mac ci.sh), host Mac, head f5f2bdf28, 3306s. Only rustty-vt-c failed (368s); core/termio/xev/renderer/vt/highway-sys passed (3900 tests ran).
- Test: crates/rustty-vt-c/tests/sys_log_no_alloc.rs:124 log_fn_dispatch_is_allocation_free: short_allocs left=1 right=0 ("sys::log_fn must format into the stack buffer, not the heap").
- Log: target/san-logs/tsan/rustty-vt-c.log (copied to .port/phase-8/final/G-TSAN-rustty-vt-c.log).
- Baseline: the same crate passed 604/604 under TSan at a60e9e2a5 (g-tsan-at-a60e9e2a5.txt line 46). Test file last changed in 4348771dc (old). Cause not yet diagnosed: either a real heap alloc on the log path introduced since a60e9e2a5, or TSan-runtime/ concurrent-thread allocation counted by the global ALLOCS counter (flake).
- Status: not rerun; runner does not fix.

## Disk-floor failures (superseded by reruns)
- G-ASAN (1307s) and G-LSAN (1349s) in ci-full-mac.log stopped at SAN_MIN_FREE_GB=15 floor: "disk floor, superseded". Reruns: G-ASAN-rerun-mac.log, G-LSAN-rerun-mac.log.
- G-ASAN rerun 1 (G-ASAN-rerun-mac.log): stopped by controller order at the disk floor, superseded; rerun after Mac ci.sh ends.

## N566 (provisional): G-REL RED on Mac, rustty-termio readthread latency test (likely load-sensitive)
- Gate: G-REL (Mac ci.sh, release lane). Only failure: rustty-termio exec::tests::read_thread::readthread_pipeline_keeps_interactive_latency (read_thread.rs:1108): median 10.23ms, worst 12.26ms vs the 300us budget ("healthy 22-46us on this host"). The same test passed in the default test lane earlier in the same run (ci-full-mac.log line 2892).
- Context: the Mac was running ASan/Miri/TSan builds at once and near the disk floor when this ran (~10ms is 200x the healthy value; a bridging-always pipeline would be ~1.18ms, so this is not the batching bug). Not rerun by the runner; the scoped rerun is the controller's call.

## N567 (provisional): G-LSAN rerun RED on Mac, rustty-vt search::thread::tests::search_thread leaks a Terminal
- Gate: G-LSAN full rerun (G-LSAN-rerun-mac.log; supersedes the disk-floor stop of the ci.sh run), Mac, head f5f2bdf28. Only rustty-vt failed (429s); 6643 tests ran, Miri-exception cross-check 0 problems.
- LeakSanitizer: 1660 bytes in 7 allocations (1 direct 848B + 6 indirect), all rooted at ScreenSet::init -> create_screen (screen_set.rs:44) <- Terminal::init (terminal.rs:798) <- search::thread::tests::search_thread (thread.rs:1757). A Terminal built in that test is never dropped/freed.
- Baseline: rustty-vt (and the wuffs,oniguruma lane) were GREEN under G-LSAN at a60e9e2a5 (g-lsan-at-a60e9e2a5.txt). Not diagnosed: real test/ownership leak vs a thread still alive at process exit (search thread outliving the test) -- check git log of search/thread.rs since a60e9e2a5.
- Log: G-LSAN-rustty-vt.log.gz.
