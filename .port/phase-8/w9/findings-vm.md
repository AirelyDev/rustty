# P8-W9 T2 provisional findings, Runner VM (ids N620+; controller renumbers)

All found on rvm at head 1d22ba80d (pin a60e9e2a5). Provisional: no root-cause fix attempted, no gate relaxed.

## N620 G-MSAN RED: `app_init_creates_and_deinit_frees_the_render_device` (rustty-core) drives uninstrumented libEGL
- Gate: `sanitizers.sh --gate msan`, rc 1, rustty-core only (every other crate GREEN, rustty-vt 3,414 tests).
- Finding: `MemorySanitizer: use-of-uninitialized-value` in `/lib64/libEGL.so.1` via
  `eglGetPlatformDisplay` <- `rustty_renderer::opengl::gl::sys::init_platform` (sys.rs:90) <- `Device::init`
  (device.rs:110) <- `DeviceBox::init` (app.rs:257) <- `App::init` (app.rs:433) <- the test (app.rs:1836).
  Origin: a `malloc` inside libEGL. Same class as SANITIZER_SUPPRESSIONS.md M3 (the twelve `MSAN_SKIPS` for tests that
  create an EGL context). The P8-W6 T2 test (new at this pin) is not in `MSAN_SKIPS`, and
  `device_tests::two_surfaces_...` / `app_create_and_destroy_empty` also call `App::init`.
- Evidence: `g-msan-at-a60e9e2a5.txt`, `g-msan-rustty-core-findinglog-at-a60e9e2a5.txt`.
- Likely fix class: gate scope (a new MSAN_SKIPS entry with reason, M3 style), not a Rustty memory bug. Controller to decide.

## N621 G-HWASAN RED: zlib-sys C-archive instrumentation check finds a host-built, uninstrumented `libz.a`
- Gate: `sanitizers.sh --gate hwasan`, rc 1. All 10,182 tests passed; only the archive check failed:
  `zlib-sys: 1 of 2 C archive(s) have no __hwasan_ reference (the cc wrapper was bypassed): debug/build/zlib-sys-76a181c741038e4b/out/libz.a`.
- That path has no target-triple component, so it is the HOST (build-dependency) copy: `rustty-cli` has `zlib-sys` as a
  build-dependency. The shared target dir of the full pass holds it from an earlier crate. The same gate with
  `SAN_ONLY=zlib-sys` (own run, `g-hwasan-zlib-isolated-...txt`) reports "1 C archive(s), all instrumented" (that run is BLOCKED by design, SAN_ONLY).
- No gate or crate change since Phase 7 touches this path (git log 401b95b0e..HEAD is empty for sanitizers.sh, zlib-sys, rustty-cli/Cargo.toml), so this is probably an order dependency in the check
  that earlier passes did not hit; not established.
- Evidence: `g-hwasan-at-a60e9e2a5.txt`.

## N622 G-TSAN RED: rustty-core, Review Focus 4 test runs but the pass is RED (races in uninstrumented Mesa and in onig init)
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
