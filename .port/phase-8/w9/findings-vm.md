# P8-W9 T2 provisional findings, Runner VM (ids renumbered at P8-W9 T3; mapping in .port/phase-8/README.md)

All found on rvm at head 1d22ba80d (pin a60e9e2a5). Provisional: no root-cause fix attempted, no gate relaxed.

## N538 G-MSAN RED: `app_init_creates_and_deinit_frees_the_render_device` (rustty-core) drives uninstrumented libEGL
- Gate: `sanitizers.sh --gate msan`, rc 1, rustty-core only (every other crate GREEN, rustty-vt 3,414 tests).
- Finding: `MemorySanitizer: use-of-uninitialized-value` in `/lib64/libEGL.so.1` via
  `eglGetPlatformDisplay` <- `rustty_renderer::opengl::gl::sys::init_platform` (sys.rs:90) <- `Device::init`
  (device.rs:110) <- `DeviceBox::init` (app.rs:257) <- `App::init` (app.rs:433) <- the test (app.rs:1836).
  Origin: a `malloc` inside libEGL. Same class as SANITIZER_SUPPRESSIONS.md M3 (the twelve `MSAN_SKIPS` for tests that
  create an EGL context). The P8-W6 T2 test (new at this pin) is not in `MSAN_SKIPS`, and
  `device_tests::two_surfaces_...` / `app_create_and_destroy_empty` also call `App::init`.
- Evidence: `g-msan-at-a60e9e2a5.txt`, `g-msan-rustty-core-findinglog-at-a60e9e2a5.txt`.
- Likely fix class: gate scope (a new MSAN_SKIPS entry with reason, M3 style), not a Rustty memory bug. Controller to decide.

## N539 G-HWASAN RED: zlib-sys C-archive instrumentation check finds a host-built, uninstrumented `libz.a`
- Gate: `sanitizers.sh --gate hwasan`, rc 1. All 10,182 tests passed; only the archive check failed:
  `zlib-sys: 1 of 2 C archive(s) have no __hwasan_ reference (the cc wrapper was bypassed): debug/build/zlib-sys-76a181c741038e4b/out/libz.a`.
- That path has no target-triple component, so it is the HOST (build-dependency) copy: `rustty-cli` has `zlib-sys` as a
  build-dependency. The shared target dir of the full pass holds it from an earlier crate. The same gate with
  `SAN_ONLY=zlib-sys` (own run, `g-hwasan-zlib-isolated-...txt`) reports "1 C archive(s), all instrumented" (that run is BLOCKED by design, SAN_ONLY).
- No gate or crate change since Phase 7 touches this path (git log 401b95b0e..HEAD is empty for sanitizers.sh, zlib-sys, rustty-cli/Cargo.toml), so this is probably an order dependency in the check
  that earlier passes did not hit; not established.
- Evidence: `g-hwasan-at-a60e9e2a5.txt`.

## N540 G-TSAN RED: rustty-core, Review Focus 4 test runs but the pass is RED (races in uninstrumented Mesa and in onig init)
- Gate: `sanitizers.sh --gate tsan`, rc 1. rustty-termio, xev, renderer, vt (3,444), vt-c, highway-sys GREEN; rustty-core FINDING.
- Review Focus 4 confirmed to RUN under TSan: `app::device_tests::two_surfaces_share_the_device_destroy_one_draw_the_other_then_deinit`
  appears in the race stacks (device_tests.rs:250/251), so it is not skipped, but it is part of the RED:
  1. `onig_reg_init` regcomp.c:7667 (read of `onig_inited`) vs `onig_initialize` regcomp.c:7769 (write) from two threads, both via
     `Link::oni_regex` -> `DerivedConfig::init` (one thread in that test's `new_surface`, one in
     `clipboard_codepoint_map_reaches_copy_selection_to_clipboards`). Memory `parallel-cargo-test-exposes-c-init-races` describes this class (OnceLock the init).
  2. `memcmp` (read) vs `calloc` (write) inside `libEGL_mesa.so.0` from `init_platform`, called concurrently by `app_init_creates_and_deinit_frees_the_render_device` and `app_create_and_destroy_empty` (Mesa is not TSan-instrumented).
  3. `pthread_mutex_destroy` / `pthread_cond_destroy` in `libgallium-26.1.4.so` called from `gl::draw::flush` (draw.rs:421) on the render thread of the destroyed surface (`DeviceApp`),
     racing with `pthread_mutex_lock` / cond use by another render thread (T88/T95) on the same Mesa-heap objects. This is the RF4 shape (one surface destroyed while another draws on the shared Device);
     whether it is Mesa-internal (an uninstrumented-library false positive) or a Rustty ordering gap is NOT established.
- Evidence: `g-tsan-at-a60e9e2a5.txt`, `g-tsan-rustty-core-findinglog-at-a60e9e2a5.txt` (1,421 lines).

## Not findings (environment)
- First full-Linux attempt used a clone without `.port/` and `docs/superpowers` (`.gitattributes` has `export-ignore` on both): 3 test failures plus three gates rc 127 were
  artifacts of that. Fixed by copying both dirs from the worktree and re-running; the recorded runs are the re-runs.
- First G-ESCTEST attempt was BLOCKED (rc 3): the oracle's Zig build could not fetch packages (`TlsInitializationFailed`), right after the clock fix; the re-run ~4 min later was GREEN. No change made.

## N535 D1 RED (slice-reinterpret audit): 4 unlisted `from_raw_parts` sites and 4 stale allowlist rows, claimed-done regression (R5)
- `check_slice_reinterpret.sh`, clean clone, rc 1 (also RED in ci.sh). Code files come from `git archive`, so this is the repo, not a clone artifact.
- UNLISTED: `crates/rustty-vt/src/bitmap_allocator.rs:848, :880`; `crates/rustty-vt/src/page_impl.rs:2297, :2467`.
  STALE rows (line drift): `page_impl.rs:2236, :2414`; `bitmap_allocator.rs:829, :861`. The sites moved by ~19-53 lines; the
  allowlist `.port/audits/slice-reinterpret.tsv` was not re-audited after Phase 8 edits to these two files. Evidence: `d1-clean-clone-at-a60e9e2a5.txt`.
- This gate is PROGRESS class but a claimed-done crate regression fails CI (R5). If the T1 17-gate set did not include D1, that explains why it was missed. Mac result unknown to this runner.

## N536 G-LINT RED (HARD) on rvm: shellcheck SC2010 and prettier on 3 docs
- `check_lints.sh`, clean clone, rc 1: `shellcheck` FAIL (`.port/tools/cvt-parity.sh:24` SC2010 `ls | grep`, a warning-level finding of rvm's shellcheck version);
  `prettier` FAIL on `docs/parity-audit/src-renderer.md`, `docs/parity-audit/src.md`, `docs/superpowers/plans/2026-10-07-phase8-upstream-resync.md`.
  typos, alejandra, actionlint ok; swiftlint and pinact skipped by design.
- `docs/superpowers` is export-ignored from `git archive`; I copied it from the worktree, so these 3 files are as committed. Whether the Mac lint passes (different shellcheck/prettier versions) is not known to this runner.
- (The first ci.sh run's prettier list of 55 files and D6's 10 findings were `._*` AppleDouble files from my own tar and are discarded; D6 is GREEN in the clean clone: `d6-clean-clone-at-a60e9e2a5.txt`.)

## G4 on rvm: BLOCKED, not run
- ci.sh's G4 BLOCKED "reference driver not built". `.port/phase-0/harness/build_ref_driver.sh` is macOS-only (looks for a Mach-O `libghostty-vt.a` under /Users/gael/projects/ghostty/zig-out), so no G4 on rvm. The Mac runner/T1 owns G4. Evidence: `g4-not-runnable-on-rvm-at-a60e9e2a5.txt`.

## ci.sh on rvm: other BLOCKED (environment)
G-NIX (nixpkgs wasmtime fails to build on aarch64-linux), G-XCF/G-XCF-SELF/G-FONTBACKEND (macOS-only), G6/G6c/G6t (cargo-nextest not installed on rvm; Mac lanes), G-VALGRIND (deferred to its sharded lane by RUSTTY_VALGRIND_OWN_JOB).

## N541 V26 (rwin) RED: flaky `rustty-vt-c` test `osc::tests::command_data_invalid_value_warns_through_the_scoped_log_sink`
- In `cargo test --workspace` on x86_64-pc-windows-msvc, one failure: the scoped log sink captured an extra `(Warning, "osc", "osc_set invalid option value=1")` before the expected `commandData invalid data value=999`
  (osc.rs:1132, message "the S174-F2 warning was not routed through sys::log_fn"). It passed alone and in 7 later runs of the lib (2+5) on rwin, and in every Linux run (rvm, 604 tests under TSan/ASan/MSan/valgrind).
- Reading: a parallel sibling test that sets an invalid OSC option logs while this test's sink is installed (the sink is process-global rather than per-thread). Order-dependent flake, not reproduced. Evidence: `v26-windows-full-workspace-at-a60e9e2a5.txt`.
- The two `xtask dist::` failures in the same run were my clone having no `.git`; both pass with a git repo (not a finding).
- DLL step GREEN (`v26-dll-step-and-reruns-at-a60e9e2a5.txt`).
