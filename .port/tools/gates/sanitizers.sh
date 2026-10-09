#!/usr/bin/env bash
# sanitizers.sh — sanitizer matrix gate. Complements Miri (R6: right tool per job).
#
# Miri catches Stacked-Borrows/provenance/uninit/leak on pure Rust but is ~100-400x
# native (a few tests are intractable). The sanitizers run at ~2x native and catch
# real memory CORRUPTION (UAF/OOB/double-free) + races across ALL tests incl. the
# Miri-intractable ones, and cover the C-FFI boundary Miri can't model.
#
# Platform support (re-verified P7-W14, 2026-10-01, and for macOS P7-W16a,
# 2026-10-02: aarch64-apple-darwin, macOS 26.5, nightly 2026-07-22 / LLVM 22.1.8;
# and the aarch64 Fedora VM `rvm`):
#   address (ASan)  ✅ macOS + Linux                         -> G-ASAN
#   leak    (LSan)  ✅ macOS + Linux, run as ASan with detect_leaks=1 -> G-LSAN
#                   The STANDALONE -Zsanitizer=leak is rejected on
#                   aarch64-apple-darwin ("leak sanitizer is not supported for
#                   this target"; rustc's target spec omits it, FIXME
#                   rust-lang/rust#88132, the standalone LSan test hung on arm64
#                   macOS), but rustc's ASan runtime (its own compiler-rt, where
#                   lsan_common.h sets CAN_SANITIZE_LEAKS for SANITIZER_APPLE +
#                   __aarch64__) runs the leak checker when detect_leaks=1 is
#                   set; compiler-rt only defaults it off on Apple
#                   (sanitizer_flags.inc: detect_leaks default !SANITIZER_APPLE).
#                   A probe's deliberate leak is RED, a clean test GREEN. It is
#                   APPLE CLANG's runtime that has no leak checker ("detect_leaks
#                   is not supported on this platform", abort), and Homebrew
#                   LLVM 21's runtimes hang on macOS 26 (ASan in its own init,
#                   standalone LSan in StopTheWorld): neither is used here.
#   memory  (MSan)  ❌ macOS: compiler-rt builds MSan for Linux|FreeBSD|NetBSD only
#                   (config-ix.cmake COMPILER_RT_HAS_MSAN); rustc and Apple clang
#                   both reject it ("memory sanitizer is not supported for this
#                   target"; "unsupported option '-fsanitize=memory'").
#                   ✅ Linux x86_64 and aarch64 (verified on rvm, 2026-10-01),
#                   needs -Zbuild-std + instrumented C -> G-MSAN (the only MSan
#                   lane; MSAN_AARCH64_NOTE turns aarch64 into BLOCKED if that regresses)
#   thread  (TSan)  ✅ macOS (verified P7-W17, 2026-10-06: a deliberate race is
#                   RED, the Mutex-ordered control GREEN) + Linux (verified
#                   on rvm, P7-W24, 2026-10-07; V32). Needs -Zbuild-std
#                   (without it: "mixing -Zsanitizer will cause an ABI
#                   mismatch"); the cc-built C is instrumented through the
#                   tsan-cc wrapper and checked (tsan_archive_check) -> G-TSAN,
#                   over the threaded crates only (TSAN_SCOPE)
#   hwaddress(HWASan)❌ macOS: compiler-rt builds it for Linux|Android|Fuchsia only
#                   (COMPILER_RT_HAS_HWASAN); rustc and Apple clang reject it.
#                   ❌ x86_64 Linux (rustc: "not supported for this target") ;
#                   ✅ aarch64 Linux (verified on rvm, P7-W16b, 2026-10-02),
#                   needs -Zbuild-std, +tagged-globals, -outline-atomics and
#                   lld (see run_crate) -> G-HWASAN
#   cfi             mitigation, not a bug-checker; needs -Clto -> optional Goal-2 hardening
#
# Why G-HWASAN next to G-ASAN (P7-W16b). HWASan tags every heap/stack/global
# object (an 8-bit tag in the pointer's top byte, one shadow byte per 16-byte
# granule) instead of
# poisoning redzones around it. So it also catches an access that lands INSIDE
# another live object (a wild index past the redzone: ASan reports nothing,
# HWASan a tag-mismatch; G-SAN-SELF's A4 wild-index arm), and a use-after-free
# long after ASan's quarantine has recycled the block. Detection is
# probabilistic (two objects share a tag with probability 1/256), and it has no
# redzones or quarantine, so it is cheaper in memory than ASan. Its C/C++ is
# instrumented (clang -fsanitize=hwaddress through the cc wrappers), which
# G-ASAN's is not (SANITIZER_SUPPRESSIONS.md "Known limits"), and that is
# checked, not assumed: every -sys crate's lib*.a must reference __hwasan_*
# (hwasan_archive_check), and G-SAN-SELF's A4 C arm overflows the heap in C.
#
# Design: docs/superpowers/specs/2026-10-01-memory-safety-lanes-design.md, section B.
#
# MODES
#   sanitizers.sh <crate> [filter] [sanitizer ...]
#       LEGACY single-crate run (check_leaks.sh's G6 ASan companion arms call it
#       this way). Sanitizers are rustc names: address thread leak memory hwaddress.
#       Prints "SANITIZERS: GREEN|PROBLEM"; exit 0 / 1.
#   sanitizers.sh --matrix --san <asan|lsan|msan|hwasan|tsan> (--crates <a,b c> | --workspace)
#                 [--filter <f>] [--exclude <crate>]...
#       The MATRIX runner: one crate at a time, all in ONE target dir per
#       sanitizer (target/san-<san>), deleted when the pass ends. A crate spec
#       may carry features: `rustty-apprt:gtk,wayland,x11`. Per-crate logs stay
#       in target/san-logs/<san>/.
#   sanitizers.sh --per-crate-target <any mode above or below>
#       Low-disk hosts: each crate in its OWN target dir
#       (target/san-<san>-<crate>), deleted after that crate, so the disk holds
#       one crate's instrumented build at a time (slower: every crate rebuilds
#       its instrumented dependencies). Same as SAN_PER_CRATE_TARGET=1.
#   sanitizers.sh --gate <asan|lsan|msan|hwasan|tsan>
#       The ci.sh gates G-ASAN / G-LSAN / G-MSAN / G-HWASAN / G-TSAN: the matrix
#       over the workspace minus that sanitizer's cited exclusions (G-MSAN and
#       G-TSAN: their allow-list scopes), plus (asan, lsan, hwasan) the
#       Miri-exception cross-check (every `cfg_attr(miri, ignore = "...")`
#       test must have RUN and PASSED here, or be expected absent).
#   sanitizers.sh --gate lsan-gtk
#       The ci.sh gate G-LSAN-GTK (P7-W24, N502's Linux coverage gap): one live
#       GTK test that creates and closes a core Surface (gtk_window_live), under
#       ASan with detect_leaks=1 inside the headless-sway wrapper, with
#       lsan.supp plus lsan-gtk.supp. Linux only (see gate_lsan_gtk).
#   sanitizers.sh --self-test
#       G-SAN-SELF: a synthetic crate with a deliberate heap overflow (RED under
#       ASan and, on aarch64 Linux, under HWASan, plus HWASan's wild-index arm),
#       leak (RED under LSan), uninitialized read (RED under MSan) and data
#       race, in Rust and in cc-built C (RED under TSan); a
#       clean control (GREEN); an empty filter (vacuous RED); and the log
#       classifier and cross-check over canned logs.
#
# EXIT CONTRACT (ci.sh): 0 GREEN ; 1 RED (a sanitizer finding, a test failure,
# zero tests run, a stale exclusion, an unverified ASan claim, a crate that
# HANGs past SAN_CRATE_TIMEOUT_S, see crate_timeout) ; 3 BLOCKED (no
# nightly, wrong host OS for the sanitizer, a crate that failed to BUILD, or the
# disk floor reached). RED dominates BLOCKED. A build failure is BLOCKED, never
# GREEN (design §B "common rules").
#
# DISK. A sanitizer pass rebuilds every dependency instrumented (one shared
# target/san-<san> of tens of GB for a whole workspace pass; 5-15 GB per heavy
# crate with --per-crate-target). Before each crate the runner checks the free space on the
# target dir's volume against SAN_MIN_FREE_GB (default 15) and stops BLOCKED
# below it, rather than filling the disk mid-build.
#
# Bash 3.2 compatible (macOS /bin/bash): no mapfile, no assoc arrays, no bare
# expansion of a possibly-empty array under `set -u`.
set -u

GATES_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$GATES_DIR/../../.." && pwd)"
OS="$(uname -s)"
ARCH="$(uname -m)"
PY="${PYTHON:-python3}"
SUPP_DIR="$ROOT/.port/phase-2"
# G-LSAN's suppressions, per host OS: Darwin's are libobjc/CoreFoundation
# per-thread data that LSan cannot see as roots there (lsan-darwin.supp's
# header); Linux's are Mesa's (lsan.supp). SANITIZER_SUPPRESSIONS.md has both.
if [ "$OS" = "Darwin" ]; then LSAN_SUPP="$SUPP_DIR/lsan-darwin.supp"; else LSAN_SUPP="$SUPP_DIR/lsan.supp"; fi
# G-TSAN's suppressions (one file for every host; each entry cited in
# SANITIZER_SUPPRESSIONS.md, ThreadSanitizer section).
TSAN_SUPP="${TSAN_SUPP:-$SUPP_DIR/tsan.supp}"
SAN_TARGET_ROOT="${SAN_TARGET_ROOT:-$ROOT/target}"
SAN_LOG_ROOT="${SAN_LOG_ROOT:-$ROOT/target/san-logs}"
SAN_MIN_FREE_GB="${SAN_MIN_FREE_GB:-15}"
# SAN_MANIFEST_PATH: run against another workspace (the self-test's synthetic
# crate). Empty = this repository.
SAN_MANIFEST_PATH="${SAN_MANIFEST_PATH:-}"

host_target() { rustc +nightly -vV 2>/dev/null | sed -n 's/host: //p'; }

# ---------------------------------------------------------------------------
# EXCLUSIONS. Format: "<crate>|<reason>". Only crates that CANNOT run under the
# sanitizer, each with a cited reason. An entry naming a crate that is not a
# workspace member is RED (a rename would otherwise leave an exclusion that
# excludes nothing while the renamed crate silently joins the run — or worse,
# hides that it never did).
# ---------------------------------------------------------------------------
ASAN_EXCLUDES=(
)
LSAN_EXCLUDES=(
)
# G-HWASAN's C-instrumentation check (hwasan_archive_check): after each `-sys`
# crate's build, every lib*.a its build script produced must reference
# __hwasan_* (every instrumented object calls __hwasan_init from its module
# constructor). An archive without, or a -sys crate that built no archive, is
# RED: the cc wrapper was bypassed and HWASan would stay silent on that C.
# Exceptions, by crate, with the reason: "<crate>|<reason>". An entry whose
# crate DID build an instrumented archive is stale (RED).
HWASAN_UNINSTRUMENTED=(
  "gettext-sys|it compiles no C on Linux: libintl is part of glibc there (crates-sys/gettext-sys/build.rs builds the vendored gettext only on macOS), so its build produces no archive to check"
)
HWASAN_EXCLUDES=(
  "xtask|its tests run nested cargo builds that inherit RUSTFLAGS=-Zsanitizer=hwaddress without -Zbuild-std, which rustc rejects ('mixing -Zsanitizer will cause an ABI mismatch in crate rustty_util'; G-HWASAN's first run, rvm, P7-W16b: 38 nested builds, 2 tests failed); the same exclusion as G-MSAN's (SANITIZER_SUPPRESSIONS.md M4, H1)"
)
# Feature lanes G-ASAN, G-LSAN and G-HWASAN add on top of the members (a plain
# `cargo test -p` never turns them on). Format: "<crate>:<features>|<reason>".
FEATURE_LANES=(
  "rustty-vt:wuffs,oniguruma|the PNG decode tests (\`image_load_png_zlib_compressed_direct\`, \`sys::tests::default_decoder_*\`) are ignored without the wuffs feature, and one of them is a Miri exception whose stated compensating control is ASan; found by G-ASAN's claim cross-check on its first run (P7-W14); and the oniguruma feature's tests, e.g. the Miri-excepted tmux::control::concurrent_first_notifications_do_not_race_oniguruma_init, which no lane compiled until the cross-check covered every Miri exception (fix round 1)"
)
# Members with NO test on a host OS, so the per-crate non-vacuity rule would
# be RED by construction. Format: "<crate>|<uname -s, or *>|<reason>". Such a crate
# still BUILDS under the sanitizer; zero tests is reported by name, and any
# test appearing there makes the entry stale (RED).
NO_TESTS_ON=(
  "rustty-wasm|*|its one native test, tests/artifact_shape.rs, is #[ignore]d: it inspects the wasm32 artifact that .port/tools/build-main-wasm.sh builds (the --webgl lane), and its exports are cfg(target_arch = \"wasm32\")"
  "gettext-sys|Linux|libintl is part of glibc on Linux, so the crate compiles no C there and every test is #[cfg(all(test, target_os = \"macos\"))] (crates-sys/gettext-sys/src/lib.rs)"
)
# Linux-only feature lanes: the same set G-VALGRIND keeps (N413).
LINUX_FEATURE_LANES=(
  "rustty-apprt:gtk,wayland,x11|GTK is the reference's Linux runtime, so its test executable there compiles the GTK apprt; cargo test --workspace does not"
  "rustty-os:flatpak|the Flatpak arm of rustty-os (links gio-2.0, Linux/FreeBSD only); cargo test --workspace does not build it"
)
# MSan phase-1 scope is an ALLOW-list, not workspace-minus-excludes: MSan
# reports any read of memory written by uninstrumented code, so a crate whose
# test binary links an uninstrumented C/C++ library cannot run under it at all.
# Every workspace member must be either in MSAN_SCOPE or in MSAN_EXCLUDES (with
# its reason); one in neither is RED.
# Phase 1, measured on rvm (aarch64 Fedora, glibc 2.43) on 2026-10-01: every
# member below passed under MSan with its own C/C++ instrumented.
MSAN_SCOPE=(
  breakpad-sys dcimgui-sys fontconfig-sys freetype-sys gettext-sys harfbuzz-sys
  highway-sys libpng-sys libxml2-sys oniguruma-sys sentry-sys simdutf-sys
  spirv-cross-sys wuffs-sys zlib-sys
  rustty rustty-apprt rustty-apprt-api rustty-apprt-c rustty-args rustty-bench
  rustty-build-config rustty-cfg-fmt rustty-cli rustty-config rustty-core
  rustty-crash rustty-datastruct rustty-extra rustty-font rustty-font-shaper
  rustty-global rustty-helpgen rustty-identity rustty-input rustty-inspector
  rustty-lib rustty-oniguruma rustty-os rustty-render-types rustty-renderer rustty-simd
  rustty-synthetic rustty-terminfo rustty-termio rustty-unicode rustty-util
  rustty-vaxis rustty-version rustty-vt rustty-vt-c rustty-wasm
  rustty-web-canvas rustty-wuffs rustty-xev rustty-z2d rustty-zf rustty-zigimg
  rustty-zigstd
)
# Tests skipped inside an in-scope crate, by libtest `--skip` filter (a
# substring of the test path). Format: "<crate>|<filter>|<reason>". Each is in
# SANITIZER_SUPPRESSIONS.md; a crate not in MSAN_SCOPE is stale (RED).
MSAN_SKIPS=(
  "rustty-renderer|generic::linux_impl::tests|the golden-renderer tests drive EGL and OpenGL in the system's uninstrumented libEGL/Mesa; MSan reports reads of what libEGL wrote itself (eglQueryString under OffscreenContext::init, origin a malloc inside libEGL.so.1) (M3)"
  "rustty-renderer|image::linux_impl::tests|the kitty-image tests build their textures through the same offscreen EGL context (gl_api -> OpenGL::init_offscreen -> eglQueryString in libEGL) (M3)"
  "rustty-renderer|opengl::tests::|opengl.rs's tests build buffers in an offscreen GL context (test_support::offscreen_gl) (M3)"
  "rustty-renderer|opengl::gl::tests::|gl.rs's tests compile shaders in an offscreen GL context (M3)"
  "rustty-renderer|opengl::gl::sys::tests::offscreen_egl_context_reaches_gl43_core|creates a real EGL context (OffscreenContext::init); the module's other six tests (version parsing, extension matching, the dmabuf query with stub function pointers) stay in (M3)"
  "rustty-renderer|opengl::gl::sys::tests::failed_prepare_context_restores_the_previous_table|creates a real EGL context (OffscreenContext::init) (M3)"
  "rustty-renderer|opengl::gl::program::|program-link failure tests in an offscreen GL context (M3)"
  "rustty-renderer|opengl::opengl_impl::|OpenGL::init_offscreen / present tests (M3)"
  "rustty-renderer|opengl::shaders::|the five-pipeline shader build, in GL and through glslang (M2) (M3)"
  "rustty-renderer|opengl::pipeline::|the framebuffer/vertex-array teardown test in an offscreen GL context (M3)"
  "rustty-renderer|opengl::target::|render-target and dmabuf export tests on the offscreen EGL context (M3)"
  "rustty-renderer|shadertoy::|custom-shader translation through glslang and spirv-cross, whose glslang preprocessor reads libstdc++-constructed stream state (M2)"
  "rustty-core|app::test_runtime::app_init_creates_and_deinit_frees_the_render_device|App::init creates the app's render device (DeviceBox::init -> Device::init -> init_platform -> eglGetPlatformDisplay in the uninstrumented libEGL.so.1; G-MSAN rvm, P8-W9 T2: use-of-uninitialized-value, origin a malloc inside libEGL.so.1; NEW-FINDINGS N538) (M6)"
  "rustty-core|app::test_runtime::app_create_and_destroy_empty|App::create runs App::init, which creates the render device through the same uninstrumented libEGL (N538) (M6)"
  "rustty-core|app::device_tests::|Review Focus 4's two live surfaces on one App: App::create's render device plus each surface renderer's offscreen EGL context and draws in the uninstrumented libEGL/Mesa (N538) (M6)"
  "rustty-core|surface_occlusion_callback_dedups_and_reports|the surface_visibility_kat integration test's TestSurface::build runs App::create, so it creates the render device through the same uninstrumented libEGL (eglGetPlatformDisplay/eglInitialize; reports in libEGL_mesa, libgallium and Mesa's driconf libexpat; N538) (M6)"
)
# G-TSAN's scope (P7-W17) is the design's allow-list of THREADED crates
# (design §B, TSan row), not the workspace: a data race needs two threads, and
# the crates below are where the port runs its own. Format: "<spec>|<why>".
# A spec naming a non-member is RED (check_members), as is a spec with zero
# tests (the per-crate non-vacuity rule).
TSAN_SCOPE=(
  "rustty-core|the render thread, the surface mailbox and the app/surface lifecycle threads (surface/lifecycle.rs, app.rs)"
  "rustty-termio|the pty read thread, the exec/Command fork paths and the termio mailbox"
  "rustty-xev|the libxev port's thread pool, async wakeups and intrusive queues"
  "rustty-renderer|the renderer Thread and its cross-thread draw/sync state (N437's area)"
  "rustty-vt:wuffs,oniguruma|the vt search thread (search/thread.rs), the tmux control/viewer threads and the oniguruma first-use init race test (tmux::control::concurrent_first_notifications_do_not_race_oniguruma_init, needs the oniguruma feature; G-ASAN's FEATURE_LANES spec, so the default-feature tests run too)"
  "rustty-vt-c|the C API's handles used from embedders' threads (search ticks, terminal effects)"
  "highway-sys|the vendored Highway runtime's dispatch state, shared by every thread that runs a SIMD routine; holds UD377's regression test (concurrent_supported_targets_calls_agree)"
)
# G-TSAN specs whose tests run one at a time (`--test-threads=1`; P7-W24,
# NEW-FINDINGS N514). Threads a test starts are still checked against each
# other; only libtest's running of DIFFERENT tests side by side is dropped,
# which the reference's Zig test runner never does. Format: "<spec>|<reason>";
# a spec not in TSAN_SCOPE is stale (RED). Applied on Linux only (spec_args):
# the reason is Linux's (io_uring, pidfd); on macOS the specs keep libtest's
# parallel runner (P7-W25).
TSAN_SERIAL=(
  "rustty-xev|TSan models each fd as a word written by the call that opens it and by close; it sees neither the io_uring arm's close ops (the kernel closes the socket) nor fds opened by raw syscalls (SYS_pidfd_open, io_uring_setup), so when one test's closed socket number is reopened by another test running beside it, the reuse is reported as a race (epoll_ctl, mmap or close against the first test's socket(); rvm, P7-W24, no suppressions: in parallel, 59 and 26 reports in 24 and 15 of two sets of 25 runs; one at a time, 0 in 25 runs). No memory is shared; Linux only (macOS's kqueue arm has no such path)"
)
# G-TSAN's C-instrumentation check (tsan_archive_check): after each crate,
# every lib*.a that a build script compiled for the instrumented target (under
# <target dir>/<triple>/debug/build/<pkg>-<hash>/out) must reference __tsan_*.
# One without is RED: the cc wrapper was bypassed, so TSan would neither see
# that C's races nor its synchronization. Exceptions: "<pkg>|<reason>"; an
# entry whose package built an instrumented archive is stale (RED).
TSAN_UNINSTRUMENTED=(
)
MSAN_EXCLUDES=(
  "glslang-sys|its test runs glslang's preprocessor, whose TPpContext holds a std::istringstream constructed and imbued by the uninstrumented libstdc++.so; MSan reports the stream's own fields as uninitialized (PpContext.cpp:98, origin: the stack allocation of 'ppContext'); needs an MSan-instrumented libstdc++/libc++ (SANITIZER_SUPPRESSIONS.md M2)"
  "xtask|its tests run nested cargo and cmake builds that inherit RUSTFLAGS=-Zsanitizer=memory without -Zbuild-std, which rustc rejects ('mixing -Zsanitizer will cause an ABI mismatch'); it is the build driver, which G-VALGRIND leaves out for the same kind of reason (M4)"
)

# ---------------------------------------------------------------------------
# Log classification. classify LOG RC -> prints one of
#   FINDING | BUILD | FAIL | VACUOUS | GREEN  and, for GREEN, the test count.
# ---------------------------------------------------------------------------
FINDING_RE='ERROR: (Address|Thread|Leak|Memory|HWAddress)Sanitizer|WARNING: (MemorySanitizer|ThreadSanitizer): |SUMMARY: [A-Za-z]*Sanitizer|runtime error:'
# EXECUTED tests: the sum of every target's `N passed; M failed` (P7-W14).
# Counting `running N tests` headers counted IGNORED tests as run, so a crate
# whose only test is #[ignore]d (rustty-wasm natively) looked like "1 test ran".
tests_ran() {
  awk '/^test result: / {
         for (i = 1; i <= NF; i++) if ($i == "passed;" || $i == "failed;") s += $(i - 1)
       } END {print s+0}' "$1"
}
# ---------------------------------------------------------------------------
# The per-crate hang bound (P7-W16a fix round 2). A sanitizer runtime can
# deadlock (as ASan's allocator locks around `fork` on macOS did, NEW-FINDINGS
# N506, closed in P7-W21: ASan builds spawn the pty child with posix_spawn), and
# a HARD gate that waits forever gives neither GREEN nor RED. Each crate's build-and-test run is bounded; past the
# bound its process tree is killed by explicit PIDs and the crate is RED with
# "HANG after Ns (no verdict)". SAN_CRATE_TIMEOUT_S overrides the default:
#   asan / lsan / leak / thread / hwaddress: 1200 s, about 4x the slowest crate
#     measured (rustty-vt, 253-294 s per-crate including its build, on a loaded
#     Mac, P7-W16a), leaving room for a shared target dir's first crate, which
#     builds every dependency;
#   msan: 2400 s (-Zbuild-std rebuilds std for the instrumented target);
#   tsan: 3600 s, 2x its slowest crate (rustty-vt with wuffs,oniguruma:
#     1781 s on the Mac, P7-W17; its 3,339 lib tests alone 1,394 s).
# ---------------------------------------------------------------------------
HANG_MARK="SANITIZERS-HANG"
crate_timeout() {  # crate_timeout SAN -> seconds
  if [ -n "${SAN_CRATE_TIMEOUT_S:-}" ]; then printf '%s' "$SAN_CRATE_TIMEOUT_S"; return; fi
  case "$1" in msan|memory) printf 2400 ;; tsan|thread) printf 3600 ;; *) printf 1200 ;; esac
}
# Every descendant of PID, deepest first, then PID itself (explicit PIDs only).
tree_pids() {
  local c
  for c in $(pgrep -P "$1" 2>/dev/null); do tree_pids "$c"; done
  printf '%s\n' "$1"
}
kill_tree() {  # kill_tree PID: TERM the whole tree, then KILL what is left
  local pids p
  pids="$(tree_pids "$1")"
  for p in $pids; do kill -TERM "$p" 2>/dev/null; done
  sleep 2
  for p in $pids; do kill -0 "$p" 2>/dev/null && kill -KILL "$p" 2>/dev/null; done
  return 0
}

classify() {
  local log="$1" rc="$2" ran
  # A hang has no verdict: RED, before anything the log may also hold.
  if grep -q "^$HANG_MARK after " "$log"; then echo HANG; return; fi
  if grep -qE "$FINDING_RE" "$log"; then echo FINDING; return; fi
  ran="$(tests_ran "$log")"
  if [ "$rc" -ne 0 ]; then
    # A compile or link error that never reached a test binary: BLOCKED.
    # Anything else (a failed test, a crashed binary) is RED.
    if grep -qE 'could not compile|linking with .* failed|^error\[E[0-9]+\]' "$log" &&
      ! grep -qE '^ *Running (unittests|tests/|benches/)' "$log"; then
      echo BUILD; return
    fi
    echo FAIL; return
  fi
  if [ "$ran" -eq 0 ]; then echo VACUOUS; return; fi
  echo "GREEN $ran"
}

# ---------------------------------------------------------------------------
# G-MSAN's C/C++ compiler wrappers. cc runs the compiler from the build script,
# whose environment carries CARGO_CFG_SANITIZE only when that build script
# builds for the instrumented target (cargo applies RUSTFLAGS to the target, not
# to build-dependencies when --target is given). So: -fsanitize=memory exactly
# when the crate being compiled is instrumented.
#
# C files also get msan-stat-shim.h (-include): rustc's MemorySanitizer runtime
# does not mark stat(2)'s output initialized for C callers on this glibc (2.43,
# aarch64), so every `stat` in vendored C is a false "use-of-uninitialized-value"
# (fontconfig's FcFileIsDir on G-MSAN's first run). Reproduced on a 2-line C
# file: clean when clang links its own MSan runtime, RED when rustc links its
# runtime (P7-W14; SANITIZER_SUPPRESSIONS.md M1). The shim unpoisons the
# struct after a successful call, which is what the missing interceptor does.
# Only for MSAN_STAT_SHIM_PKGS (cc runs the compiler with the build script's
# CARGO_PKG_NAME): the shim includes <sys/stat.h> before the source's own
# feature macros, which hid pthread_setname_np from sentry-native
# (`#define _GNU_SOURCE` comes too late) when it was applied to every crate.
# fontconfig-sys (FcFileIsDir's stat) and freetype-sys (FT_Stream_Open's
# fstat, reached from fontconfig-sys's test) are the crates the runs needed it
# for.
#
# sentry-sys gets msan-decl.h: sentry-native calls __msan_unpoison under
# __has_feature(memory_sanitizer) without including <sanitizer/msan_interface.h>
# (src/sentry_unix_pageallocator.c:65), so it does not compile under MSan
# (M5). The header declares only that function and includes nothing.
# ---------------------------------------------------------------------------
MSAN_STAT_SHIM_PKGS="fontconfig-sys freetype-sys"
MSAN_DECL_PKGS="sentry-sys"
write_san_wrappers() {
  local d="$SAN_TARGET_ROOT/san-cc-wrappers"
  mkdir -p "$d" || return 1
  cat >"$d/msan-stat-shim.h" <<'SHIM'
/* G-MSAN (sanitizers.sh): see write_san_wrappers. */
#ifndef RUSTTY_MSAN_STAT_SHIM_H
#define RUSTTY_MSAN_STAT_SHIM_H
#include <sys/stat.h>
#include <sanitizer/msan_interface.h>
static inline int rustty_msan_stat(const char *p, struct stat *b) {
  int r = stat(p, b);
  if (r == 0) __msan_unpoison(b, sizeof *b);
  return r;
}
static inline int rustty_msan_lstat(const char *p, struct stat *b) {
  int r = lstat(p, b);
  if (r == 0) __msan_unpoison(b, sizeof *b);
  return r;
}
static inline int rustty_msan_fstat(int fd, struct stat *b) {
  int r = fstat(fd, b);
  if (r == 0) __msan_unpoison(b, sizeof *b);
  return r;
}
#define stat(p, b) rustty_msan_stat(p, b)
#define lstat(p, b) rustty_msan_lstat(p, b)
#define fstat(fd, b) rustty_msan_fstat(fd, b)
#endif
SHIM
  cat >"$d/msan-decl.h" <<'DECL'
/* G-MSAN (sanitizers.sh): see write_san_wrappers (M5). */
void __msan_unpoison(const volatile void *a, unsigned long size);
DECL
  local cc
  for cc in cc c++; do
    local real=clang shim="" decl=""
    if [ "$cc" = "c++" ]; then real=clang++; else shim="-include $d/msan-stat-shim.h"; decl="-include $d/msan-decl.h"; fi
    cat >"$d/msan-$cc" <<WRAP
#!/usr/bin/env bash
shim=""
case " $MSAN_STAT_SHIM_PKGS " in *" \${CARGO_PKG_NAME:-} "*) shim="$shim" ;; esac
case " $MSAN_DECL_PKGS " in *" \${CARGO_PKG_NAME:-} "*) shim="$decl" ;; esac
case ",\${CARGO_CFG_SANITIZE:-}," in
  *,memory,*) exec $real -fsanitize=memory -fsanitize-memory-track-origins -fno-omit-frame-pointer -g \$shim "\$@" ;;
  *) exec $real "\$@" ;;
esac
WRAP
    chmod +x "$d/msan-$cc" || return 1
    # G-HWASAN (P7-W16b): -fsanitize=hwaddress exactly when the crate being
    # compiled is instrumented (as above). -mno-outline-atomics: the C must not
    # call the __aarch64_* helpers, which the Rust side does not provide under
    # -outline-atomics (see run_crate) and rustc links no libgcc.a.
    cat >"$d/hwasan-$cc" <<WRAP
#!/usr/bin/env bash
case ",\${CARGO_CFG_SANITIZE:-}," in
  *,hwaddress,*) exec $real -fsanitize=hwaddress -mno-outline-atomics -fno-omit-frame-pointer -g "\$@" ;;
  *) exec $real "\$@" ;;
esac
WRAP
    chmod +x "$d/hwasan-$cc" || return 1
    # G-TSAN (P7-W17): -fsanitize=thread exactly when the crate being compiled
    # is instrumented (as above), so the C/C++ we build ourselves reports its
    # own races and its own synchronization is visible to the runtime. The
    # wrapper adds -fsanitize=thread to EVERY invocation it sees, but cc only
    # calls it to compile (-c) and archives with ar, so in practice it
    # instruments objects; the final link is rustc's, which links its own TSan
    # runtime and resolves the __tsan_* calls (tsan_archive_check verifies
    # them).
    cat >"$d/tsan-$cc" <<WRAP
#!/usr/bin/env bash
case ",\${CARGO_CFG_SANITIZE:-}," in
  *,thread,*) exec $real -fsanitize=thread -fno-omit-frame-pointer -g "\$@" ;;
  *) exec $real "\$@" ;;
esac
WRAP
    chmod +x "$d/tsan-$cc" || return 1
  done
}

# ---------------------------------------------------------------------------
# One crate under one sanitizer. run_crate SAN SPEC FILTER TD LOG
# SAN is a matrix id (asan|lsan|msan) or a legacy rustc name.
# ---------------------------------------------------------------------------
run_crate() {
  local san="$1" spec="$2" filter="$3" td="$4" log="$5"
  local crate="${spec%%:*}" feats="" tgt rflags="" build_std="" rc san_rt_base=""
  case "$spec" in *:*) feats="${spec#*:}" ;; esac
  tgt="$(host_target)"
  local -a envs=() cargo_args=()
  # allocator_may_return_null=1: ASan's malloc otherwise ABORTS on a request
  # larger than its 1 TiB maximum, where the system malloc returns NULL. The
  # port's out-of-memory tests rely on that NULL (`Vec::try_reserve_exact`
  # reporting failure, the faithful port of Zig's error.OutOfMemory paths; e.g.
  # rustty-vt's hyperlink `try_dupe_bytes_reports_allocation_failure` asks for
  # isize::MAX bytes). It DOES turn off three reports: allocation-size-too-big,
  # calloc-overflow and out-of-memory (each becomes a NULL return instead of an
  # abort). OOB, use-after-free, double-free, leak and uninit detection are
  # unaffected. First seen as G-ASAN's first-run "allocation-size-too-big" (P7-W14).
  case "$san" in
    asan|address)
      rflags="-Zsanitizer=address"
      envs+=("ASAN_OPTIONS=detect_leaks=0:allocator_may_return_null=1") ;;  # WHY: not suppressive of anything another lane misses — G-LSAN (the same ASan build with detect_leaks=1, macOS and Linux) is the leak lane, so G-ASAN keeps its verdict to memory errors only
    lsan)
      rflags="-Zsanitizer=address"
      envs+=("ASAN_OPTIONS=detect_leaks=1:allocator_may_return_null=1")      # WHY: not suppressive — turns the leak checker ON; this IS G-LSAN (design §B: LSan run as ASan with detect_leaks=1)
      envs+=("LSAN_OPTIONS=suppressions=$LSAN_SUPP:print_suppressions=1")    # WHY: every entry in lsan.supp / lsan-darwin.supp is a third-party leak with a cited reason in .port/phase-2/SANITIZER_SUPPRESSIONS.md; print_suppressions=1 lists every match in the log so a suppression is never silent
      # macOS: the ASan runtime is a dylib that looks the user hooks
      # (__lsan_is_turned_off, __lsan_default_suppressions) up DYNAMICALLY
      # (`nm -m librustc-nightly_rt.asan.dylib`: "weak external
      # ___lsan_is_turned_off (dynamically looked up)"), so a hook a test binary
      # defines is seen only when the executable exports it, and rustc exports
      # nothing from an executable by default. Measured (P7-W16a): a probe whose
      # hook always returns 1 still reported its leak without this flag, and
      # did not with it. rustty-termio's fork-child hook (N496 item 3) needs it.
      # On Linux the runtime is linked statically and the hook resolves at link
      # time, so the flag is Darwin-only.
      # macOS: the runtime's own output goes to per-process files in a private
      # temp dir (appended to the crate log after the run, below), not to each process's
      # stderr. With lsan-darwin.supp, print_suppressions=1 prints its table on
      # the stderr of EVERY process in which a suppression matched, including
      # the CLI children that tests run and whose stderr they assert on
      # (rustty's cli_log_facade asserts `+show-config` prints nothing; P7-W16a
      # run 2: its stderr was only the S3/S4 table). A child that really leaks
      # still fails its test (exit 23), and every report and table still lands
      # in the crate log the classifier reads. Linux keeps stderr: no Linux
      # suppression matches in a child, and that lane is verified as is.
      if [ "$OS" = "Darwin" ]; then
        rflags="$rflags -Zexport-executable-symbols"
        # A private temp dir: ASAN_OPTIONS splits on ':' and whitespace, so the
        # path must hold neither (the crate log's path may: the self-test's
        # arm names do).
        local san_rt_dir
        san_rt_dir="$(mktemp -d "${TMPDIR:-/tmp}/san-rt.XXXXXX")" || return 2
        case "$san_rt_dir" in *:*|*' '*) echo "sanitizers.sh: runtime log dir '$san_rt_dir' holds ':' or a space" >&2; rmdir -- "$san_rt_dir"; return 2 ;; esac
        san_rt_base="$san_rt_dir/rt"
        envs+=("ASAN_OPTIONS=detect_leaks=1:allocator_may_return_null=1:log_path=$san_rt_base")  # WHY: not suppressive — the same options as above plus log_path (reports go to files that run_crate appends to the crate log)
      fi ;;
    msan|memory)
      rflags="-Zsanitizer=memory -Zsanitizer-memory-track-origins ${SAN_MSAN_DIAG:+-Zsanitizer-recover=memory}"
      build_std="-Zbuild-std"
      # The C/C++ we compile ourselves through `cc` is instrumented too, through
      # the msan-cc wrappers (see write_san_wrappers): plain CFLAGS would also
      # reach build-dependencies compiled for the build script, which is not
      # instrumented, and fail to link (`undefined reference to __msan_init`,
      # rustty-cli's zlib-sys build-dependency, P7-W14).
      write_san_wrappers || return 2
      envs+=("CC=$SAN_TARGET_ROOT/san-cc-wrappers/msan-cc" "CXX=$SAN_TARGET_ROOT/san-cc-wrappers/msan-c++")
      envs+=("MSAN_OPTIONS=poison_in_dtor=1:allocator_may_return_null=1${SAN_MSAN_DIAG:+:halt_on_error=0}") ;;  # SAN_MSAN_DIAG=1: a diagnosis run that reports every use instead of stopping at the first (never a gate setting; the verdict is still RED on any report)
    tsan|thread)
      # -Zbuild-std: without it rustc rejects the mix ("mixing -Zsanitizer will
      # cause an ABI mismatch"; P7-W17 probe), as for MSan and HWASan.
      rflags="-Zsanitizer=thread"; build_std="-Zbuild-std"
      # Our cc-built C/C++ is instrumented too (the same wrappers as G-MSAN).
      write_san_wrappers || return 2
      envs+=("CC=$SAN_TARGET_ROOT/san-cc-wrappers/tsan-cc" "CXX=$SAN_TARGET_ROOT/san-cc-wrappers/tsan-c++")
      # halt_on_error=0 (the default) lists every race of the run; the binary
      # still exits 66 after a report, and the classifier is RED on any.
      # second_deadlock_stack=1: a lock-order-inversion report names both
      # acquisitions. allocator_may_return_null=1: TSan's malloc aborts above
      # 1 TiB like ASan's, and the port's OOM tests rely on the NULL (P7-W17,
      # rustty-vt's first G-TSAN run). Suppressions: TSAN_SUPP, every entry cited in
      # SANITIZER_SUPPRESSIONS.md; print_suppressions=1 lists each match.
      envs+=("TSAN_OPTIONS=second_deadlock_stack=1:allocator_may_return_null=1:suppressions=$TSAN_SUPP:print_suppressions=1") ;;  # WHY: allocator_may_return_null=1 as for G-ASAN (the out-of-memory tests rely on malloc's NULL; first G-TSAN run, rustty-vt: allocation-size-too-big); the suppressions are not suppressive by themselves — each TSAN_SUPP entry is a third-party race with a cited reason in .port/phase-2/SANITIZER_SUPPRESSIONS.md (T rows), and print_suppressions=1 lists every match in the log
    leak)
      rflags="-Zsanitizer=leak" ;;
    hwasan|hwaddress)
      # Every flag here is load-bearing; each was a failure on rvm (P7-W16b):
      #   -Zbuild-std            without it rustc rejects the mix ("-Zsanitizer=
      #                          hwaddress in this crate is incompatible with
      #                          -Zsanitizer being unset in dependency core").
      #   +tagged-globals        the HWASan ABI for globals on aarch64 (clang
      #                          turns it on itself for -fsanitize=hwaddress;
      #                          rustc does not, and warns it is "unknown").
      #   -outline-atomics      compiler_builtins' outline-atomics asm takes
      #                          `adrp` of HAVE_LSE_ATOMICS, which tagged globals
      #                          move out of ADRP range: "relocation truncated to
      #                          fit: R_AARCH64_ADR_PREL_PG_HI21" (ld.bfd and lld).
      #                          Inline atomics instead (LL/SC on the baseline target).
      #   -fuse-ld=lld           GNU ld 2.46 links tagged globals but writes an
      #                          invalid instruction (`.inst 0xb2ea5fe8`, SIGILL
      #                          in std's args init before main).
      rflags="-Zsanitizer=hwaddress -Ctarget-feature=+tagged-globals,-outline-atomics -Clink-arg=-fuse-ld=lld"
      build_std="-Zbuild-std"
      # Our cc-built C/C++ is instrumented too (the same wrappers as G-MSAN).
      write_san_wrappers || return 2
      envs+=("CC=$SAN_TARGET_ROOT/san-cc-wrappers/hwasan-cc" "CXX=$SAN_TARGET_ROOT/san-cc-wrappers/hwasan-c++")
      envs+=("HWASAN_OPTIONS=detect_leaks=0:allocator_may_return_null=1") ;;  # WHY: detect_leaks=0, G-LSAN is the leak lane (HWASan carries LSan on Linux); allocator_may_return_null=1 as for G-ASAN (the out-of-memory tests rely on malloc's NULL)
    *) echo "sanitizers.sh: unknown sanitizer '$san'" >&2; return 2 ;;
  esac
  # RUST_MIN_STACK (std's default spawn stack, 2 MiB otherwise): ASan and MSan
  # put redzones and shadow bookkeeping in every frame, so instrumented debug
  # frames are 2-3x their native size. A test that checks a recursion bound
  # FITS the default stack is checking native frame sizes; under ASan it
  # overflows without any memory error. Measured (P7-W14): rustty-vt's
  # `deeply_nested_layout_is_a_syntax_error_beyond_max_depth` parses MAX_DEPTH
  # (1000) levels in 1.25-1.5 MiB natively (debug) and overflows the 2 MiB stack
  # under ASan. 8 MiB keeps that headroom; unbounded recursion still overflows,
  # and the native bound stays checked by `cargo test --workspace`. Threads
  # that set an explicit stack size are unaffected.
  if [ "$san" != leak ]; then envs+=("RUST_MIN_STACK=8388608"); fi
  # Line tables only: sanitizer reports still name every function, file and
  # line, but the instrumented debug binaries link in a fraction of the memory
  # and disk. With full debuginfo the GTK apprt lane's test links (ld.bfd, 8 at
  # once) exhausted the 10 GB Linux VM and ld died with SIGBUS (P7-W14).
  envs+=("CARGO_PROFILE_DEV_DEBUG=line-tables-only" "CARGO_PROFILE_TEST_DEBUG=line-tables-only")
  if [ -n "$SAN_MANIFEST_PATH" ]; then cargo_args+=(--manifest-path "$SAN_MANIFEST_PATH"); fi
  cargo_args+=(-p "$crate" --target "$tgt" --no-fail-fast)
  # SAN_TEST_TARGET: one integration-test target only (`--test NAME`; G-LSAN-GTK).
  if [ -n "${SAN_TEST_TARGET:-}" ]; then cargo_args+=(--test "$SAN_TEST_TARGET"); fi
  if [ -n "$feats" ]; then cargo_args+=(--features "$feats"); fi
  if [ -n "$build_std" ]; then cargo_args+=("$build_std"); fi
  mkdir -p "$(dirname "$log")"
  # The hang bound (fix round 2): the crate's whole build-and-test run gets
  # SAN_CRATE_TIMEOUT_S seconds; past it the process tree is killed by PID,
  # the log gets a HANG line, and classify reports HANG (RED), never GREEN.
  local bound
  bound="$(crate_timeout "$san")"
  # SAN_WRAP: a launcher the whole cargo run executes under (G-LSAN-GTK: the
  # headless-sway wrapper, .port/tools/gtk_live_headless_sway.sh, which runs
  # its arguments against a private compositor). It inherits this shell's
  # environment, not envs, so the compositor itself is not instrumented.
  local -a wrap=()
  if [ -n "${SAN_WRAP:-}" ]; then wrap=("$SAN_WRAP"); fi
  # shellcheck disable=SC2086  # FILTER is an intentional word list (libtest takes several)
  ${wrap[@]+"${wrap[@]}"} env "${envs[@]}" RUSTFLAGS="$rflags" RUSTDOCFLAGS="$rflags" CARGO_TARGET_DIR="$td" \
    cargo +nightly test "${cargo_args[@]}" -- $filter >"$log" 2>&1 &
  local cpid=$! waited=0
  while kill -0 "$cpid" 2>/dev/null; do
    if [ "$waited" -ge "$bound" ]; then
      kill_tree "$cpid"
      wait "$cpid" 2>/dev/null
      printf '\n%s after %ss (no verdict)\n' "$HANG_MARK" "$bound" >>"$log"
      break
    fi
    sleep 1
    waited=$((waited + 1))
  done
  wait "$cpid" 2>/dev/null
  rc=$?
  if [ -n "$san_rt_base" ]; then
    local rtf
    for rtf in "$san_rt_base".*; do
      [ -f "$rtf" ] || continue
      printf '\n===== sanitizer runtime output (%s) =====\n' "${rtf##*/}" >>"$log"
      cat "$rtf" >>"$log"
      rm -f -- "$rtf"
    done
    rmdir -- "${san_rt_base%/rt}" 2>/dev/null
  fi
  return $rc
}

# ---------------------------------------------------------------------------
# LEGACY single-crate mode (kept byte-compatible for check_leaks.sh).
# ---------------------------------------------------------------------------
legacy_main() {
  local CRATE="$1" FILTER="${2:-}"
  shift 2 2>/dev/null || shift $# 2>/dev/null
  local SANS
  if [ "$#" -gt 0 ]; then SANS="$*"
  elif [ "$OS" = "Darwin" ]; then SANS="address thread"
  else SANS="address thread leak"; fi
  local fail=0 s
  for s in $SANS; do
    case "$s" in
      leak|hwaddress) if [ "$OS" = "Darwin" ]; then echo "  ⏭  $s: skipped (rustc rejects -Zsanitizer=$s for aarch64-apple-darwin; macOS leak checking is --gate lsan, ASan with detect_leaks=1)"; continue; fi ;;
    esac
    local td="/tmp/san-gate-$CRATE-$s" log="/tmp/san-gate-$CRATE-$s.log" rc v
    echo "### sanitizer=$s crate=$CRATE ###"
    run_crate "$s" "$CRATE" "$FILTER" "$td" "$log"; rc=$?
    v="$(classify "$log" "$rc")"
    # NON-VACUITY + HONEST REPORTING (miri-reaudit, 2026-07-31): the green line
    # once echoed the LAST `test result` (the empty Doc-tests summary), so a
    # filter matching nothing looked identical to a real pass. The count is the
    # number of tests EXECUTED, summed over every target's `N passed; M failed`
    # (tests_ran; ignored tests do not count since P7-W14); zero is RED.
    case "$v" in
      HANG) echo "  ❌ $s: $(grep -m1 "^$HANG_MARK after " "$log" | sed "s/^$HANG_MARK/HANG/")"; fail=1 ;;
      FINDING) echo "  ❌ $s: SANITIZER FINDING"; grep -E 'ERROR:|SUMMARY:|runtime error:' "$log" | head -4; fail=1 ;;
      BUILD|FAIL) echo "  ⚠️  $s: build/test failed rc=$rc — $(grep -m1 -iE 'error(\[|:)' "$log" | head -c 100)"; fail=1 ;;
      VACUOUS) echo "  ❌ $s: VACUOUS — 0 tests ran (filter '${FILTER:-<none>}' matched nothing); a green over an empty set is not a pass"; fail=1 ;;
      GREEN*) echo "  ✅ $s: ${v#GREEN } test(s) ran — $(grep 'test result' "$log" | tr '\n' ';' | sed 's/;$//')" ;;
    esac
  done
  if [ $fail -eq 0 ]; then echo "SANITIZERS: GREEN ($CRATE: $SANS)"; else echo "SANITIZERS: PROBLEM ($CRATE)"; fi
  return $fail
}

# ---------------------------------------------------------------------------
# Matrix helpers.
# ---------------------------------------------------------------------------
free_gb() { df -Pk "$1" 2>/dev/null | awk 'NR==2 {printf "%d", $4/1048576}'; }

# Delete a sanitizer target dir. Guarded: only a path of the exact shape
# <SAN_TARGET_ROOT>/san-<san> or <SAN_TARGET_ROOT>/san-<san>-<crate> is ever
# removed (no variable can widen this to the repository or the main target dir).
drop_target() {
  local td="$1"
  case "$td" in
    "$SAN_TARGET_ROOT"/san-asan|"$SAN_TARGET_ROOT"/san-lsan|"$SAN_TARGET_ROOT"/san-msan|"$SAN_TARGET_ROOT"/san-hwasan|"$SAN_TARGET_ROOT"/san-tsan) ;;
    "$SAN_TARGET_ROOT"/san-asan-*|"$SAN_TARGET_ROOT"/san-lsan-*|"$SAN_TARGET_ROOT"/san-msan-*|"$SAN_TARGET_ROOT"/san-hwasan-*|"$SAN_TARGET_ROOT"/san-tsan-*) ;;
    *) echo "sanitizers.sh: refusing to delete '$td' (not a san-<san>[-<crate>] dir)" >&2; return 1 ;;
  esac
  case "$td" in *..*|*' '*) echo "sanitizers.sh: refusing to delete '$td'" >&2; return 1 ;; esac
  [ -d "$td" ] && rm -rf -- "$td"
  return 0
}

workspace_members() {
  local mp=()
  if [ -n "$SAN_MANIFEST_PATH" ]; then mp=(--manifest-path "$SAN_MANIFEST_PATH"); fi
  (cd "$ROOT" && cargo metadata --no-deps --format-version 1 ${mp[@]+"${mp[@]}"} 2>/dev/null) |
    "$PY" -c 'import json,sys; print("\n".join(sorted(p["name"] for p in json.load(sys.stdin)["packages"])))'
}

no_tests_reason() {  # no_tests_reason SPEC -> the NO_TESTS_ON reason for this OS, or ""
  local e
  for e in ${NO_TESTS_ON[@]+"${NO_TESTS_ON[@]}"}; do
    if [ "${e%%|*}" = "$1" ]; then
      local rest="${e#*|}"
      if [ "${rest%%|*}" = "$OS" ] || [ "${rest%%|*}" = "*" ]; then printf '%s' "${rest#*|}"; return; fi
    fi
  done
}

spec_skips() {  # spec_skips SAN SPEC -> " --skip F ..." for that sanitizer's named skips
  local e
  [ "$1" = msan ] || return 0
  [ -z "${SAN_MSAN_DIAG:-}" ] || return 0  # a diagnosis run lists every MSan report, so it skips nothing
  for e in ${MSAN_SKIPS[@]+"${MSAN_SKIPS[@]}"}; do
    if [ "${e%%|*}" = "$2" ]; then printf ' --skip %s' "$(printf '%s' "$e" | cut -d'|' -f2)"; fi
  done
}

spec_args() {  # spec_args SAN SPEC -> extra libtest arguments for that spec (G-TSAN: TSAN_SERIAL, Linux only)
  local e
  [ "$1" = tsan ] || return 0
  [ "$OS" = Linux ] || return 0
  for e in ${TSAN_SERIAL[@]+"${TSAN_SERIAL[@]}"}; do
    if [ "${e%%|*}" = "$2" ]; then printf ' --test-threads=1'; fi
  done
}

# skips_unmatched SPEC LISTLOG: print each MSAN_SKIPS filter of SPEC that
# matches no test in LISTLOG (libtest `--list` output: "<path>: test"); a filter
# that skips nothing is stale and RED, like any other stale entry.
skips_unmatched() {
  local e f
  for e in ${MSAN_SKIPS[@]+"${MSAN_SKIPS[@]}"}; do
    [ "${e%%|*}" = "$1" ] || continue
    f="$(printf '%s' "$e" | cut -d'|' -f2)"
    grep -E ': (test|bench)$' "$2" | grep -qF -- "$f" || printf '%s\n' "$f"
  done
}

# hwasan_archive_check CRATE TD [TRIPLE]: the C-instrumentation check (see
# HWASAN_UNINSTRUMENTED). Prints one line; returns 1 (RED) when a -sys crate's
# build produced no archive or an archive without __hwasan_* references, or
# when its exclusion is stale. Other crates: nothing to check, returns 0.
# Only archives built for the instrumented TARGET are looked at
# (<td>/<triple>/debug/build/<crate>-*/out, as in tsan_archive_check). A copy
# built for the HOST as another crate's build-dependency (<td>/debug/build/...,
# e.g. rustty-cli's zlib-sys) is compiled by the plain compiler, because the
# wrapper instruments only crates built with CARGO_CFG_SANITIZE, and it links
# into a build script, never into a test binary. In one shared target dir that
# host copy sits next to the target copy once a crate with that
# build-dependency has run (P8-W9 FX4, NEW-FINDINGS N539).
hwasan_archive_check() {
  local crate="$1" td="$2" tgt="${3:-$(host_target)}" a n=0 inst=0 bad="" ex="" e
  case "$crate" in *-sys) ;; *) return 0 ;; esac
  for e in ${HWASAN_UNINSTRUMENTED[@]+"${HWASAN_UNINSTRUMENTED[@]}"}; do
    if [ "${e%%|*}" = "$crate" ]; then ex="${e#*|}"; fi
  done
  while IFS= read -r a; do
    [ -n "$a" ] || continue
    n=$((n + 1))
    if nm "$a" 2>/dev/null | grep -q '__hwasan_'; then inst=$((inst + 1)); else bad="$bad ${a#"$td"/}"; fi
  done <<EOF
$(find "$td/$tgt/debug/build" -path "*/build/$crate-*/out/*" -name 'lib*.a' 2>/dev/null | sort)
EOF
  if [ -n "$ex" ]; then
    if [ "$inst" -gt 0 ]; then
      echo "  ❌ hwasan $crate: HWASAN_UNINSTRUMENTED lists it, but $inst of its archive(s) are instrumented — the entry is stale"
      return 1
    fi
    echo "  ⏭  hwasan $crate: C archives not checked for instrumentation — $ex"
    return 0
  fi
  if [ "$n" -eq 0 ]; then
    echo "  ❌ hwasan $crate: its build produced no lib*.a for $tgt, so no instrumented C was linked (add it to HWASAN_UNINSTRUMENTED with the reason if that is by design)"
    return 1
  fi
  if [ -n "$bad" ]; then
    echo "  ❌ hwasan $crate: $((n - inst)) of $n C archive(s) have no __hwasan_ reference (the cc wrapper was bypassed):$bad"
    return 1
  fi
  echo "  ok hwasan $crate: $n C archive(s), all instrumented (__hwasan_ references)"
  return 0
}

# tsan_archive_check TD [TRIPLE]: G-TSAN's C-instrumentation check (see
# TSAN_UNINSTRUMENTED). Every lib*.a a build script compiled for the
# instrumented target must reference __tsan_*. Prints one line per problem plus
# a summary; returns 1 (RED) on an uninstrumented archive or an exception
# whose archive is instrumented (stale). Runs after each crate (a per-crate
# target dir holds only that crate's dependencies).
# Build scripts of host build-dependencies write under <td>/debug/build, which
# is not instrumented and not looked at.
# Non-vacuity: zero archives is RED when it cannot be right, i.e. when the
# instrumented build dir is missing (the crate was not built for TRIPLE, so
# nothing was looked at) or when a build script there declared a static
# library (cargo:rustc-link-lib=static=..., what cc prints for each archive it
# builds) yet no lib*.a was found (the find looked in the wrong place). Zero
# archives with no such declaration is a crate that compiles no C: GREEN.
tsan_archive_check() {
  local td="$1" tgt="${2:-$(host_target)}" a n=0 inst=0 pkg dir e ex rc=0 decl
  if [ ! -d "$td/$tgt/debug/build" ]; then
    echo "  ❌ tsan C archives: no ${td##*/}/$tgt/debug/build — nothing was built for the instrumented target, so nothing could be checked"
    return 1
  fi
  while IFS= read -r a; do
    [ -n "$a" ] || continue
    dir="${a#"$td/$tgt/debug/build/"}"; dir="${dir%%/*}"; pkg="${dir%-*}"
    ex=""
    for e in ${TSAN_UNINSTRUMENTED[@]+"${TSAN_UNINSTRUMENTED[@]}"}; do
      if [ "${e%%|*}" = "$pkg" ]; then ex="${e#*|}"; fi
    done
    n=$((n + 1))
    if nm "$a" 2>/dev/null | grep -q '__tsan_'; then
      inst=$((inst + 1))
      if [ -n "$ex" ]; then echo "  ❌ tsan $pkg: TSAN_UNINSTRUMENTED lists it, but ${a#"$td"/} is instrumented — the entry is stale"; rc=1; fi
    elif [ -n "$ex" ]; then
      echo "  ⏭  tsan $pkg: ${a##*/} not checked for instrumentation — $ex"
    else
      echo "  ❌ tsan $pkg: ${a#"$td"/} has no __tsan_ reference (the cc wrapper was bypassed)"
      rc=1
    fi
  done <<EOF
$(find "$td/$tgt/debug/build" -path '*/out/*' -name 'lib*.a' 2>/dev/null | sort)
EOF
  if [ "$n" -eq 0 ]; then
    decl="$(grep -lE '^cargo::?rustc-link-lib=static[:=]' "$td/$tgt/debug/build"/*/output 2>/dev/null | wc -l | tr -d ' ')"
    if [ "$decl" -gt 0 ]; then
      echo "  ❌ tsan C archives: $decl build script(s) declared a static C library, but no lib*.a was found under $tgt/debug/build/*/out — the check would pass vacuously"
      return 1
    fi
  fi
  if [ "$rc" -eq 0 ]; then echo "  ok tsan C archives: $n built for the instrumented target, $inst instrumented (__tsan_ references), the rest named exceptions"; fi
  return $rc
}

# Matrix result accumulators (globals: bash 3.2 has no namerefs).
M_RED=0; M_BLOCKED=0; M_TOTAL=0; M_ROWS=""; M_WALL=0
# TARGET DIRS. Default: ONE target dir per sanitizer pass, target/san-<san>,
# shared by every crate of the pass (each instrumented dependency, the C -sys
# builds and MSan's -Zbuild-std std are built once, not once per crate), and
# deleted when the pass ends. SAN_PER_CRATE_TARGET=1 (or --per-crate-target)
# is the low-disk mode: target/san-<san>-<crate>, deleted after each crate, so
# the disk never holds more than one crate's instrumented build (P7-W14's
# first passes ran that way, on a Mac with 26 GB free).
SAN_PER_CRATE_TARGET="${SAN_PER_CRATE_TARGET:-0}"
matrix_run() {  # matrix_run SAN FILTER SPEC...
  local san="$1" filter="$2"; shift 2
  local spec crate td log rc v t0 t1 safe fg shared="" m0
  m0=$(date +%s)
  if [ "$SAN_PER_CRATE_TARGET" = 1 ]; then
    echo "=== sanitizer matrix: $san over $# crate spec(s); per-crate target dirs $SAN_TARGET_ROOT/san-$san-<crate>, each deleted after its crate ==="
  else
    shared="$SAN_TARGET_ROOT/san-$san"
    # A pass starts clean: a dir an aborted pass left behind may hold C built
    # by an older compiler wrapper, which cargo would not notice.
    drop_target "$shared"
    echo "=== sanitizer matrix: $san over $# crate spec(s); one shared target dir $shared, deleted when the pass ends ==="
  fi
  for spec in "$@"; do
    crate="${spec%%:*}"
    safe="$(printf '%s' "$spec" | tr ':,' '__')"
    td="${shared:-$SAN_TARGET_ROOT/san-$san-$safe}"
    log="$SAN_LOG_ROOT/$san/$safe.log"
    mkdir -p "$SAN_TARGET_ROOT"
    fg="$(free_gb "$SAN_TARGET_ROOT")"
    if [ -n "$fg" ] && [ "$fg" -lt "$SAN_MIN_FREE_GB" ]; then
      echo "  ⛔ BLOCKED: $fg GB free under $SAN_TARGET_ROOT, below the SAN_MIN_FREE_GB=$SAN_MIN_FREE_GB floor; stopping before $spec"
      M_BLOCKED=1
      M_ROWS="$M_ROWS$spec|BLOCKED(disk)|0|0"$'\n'
      if [ -n "$shared" ]; then drop_target "$shared"; fi
      M_WALL=$(($(date +%s) - m0))
      return
    fi
    t0=$(date +%s)
    run_crate "$san" "$spec" "$filter$(spec_skips "$san" "$spec")$(spec_args "$san" "$spec")" "$td" "$log"; rc=$?
    t1=$(date +%s)
    v="$(classify "$log" "$rc")"
    if [ -n "$(spec_skips "$san" "$spec")" ]; then
      # Every named skip must match a test: list the same build's tests (no
      # rebuild) and check each filter against the list.
      local lst="${log%.log}.list" um
      run_crate "$san" "$spec" "--list" "$td" "$lst" >/dev/null 2>&1
      um="$(skips_unmatched "$spec" "$lst")"
      if [ -n "$um" ]; then
        M_RED=1
        echo "  ❌ $san $spec: MSAN_SKIPS filter(s) matching no test (stale): $(printf '%s' "$um" | tr '\n' ' ')"
      fi
    fi
    # G-HWASAN: a -sys crate's C must really be instrumented (before the
    # target dir goes). A crate that did not build has nothing to inspect.
    if [ "$san" = hwasan ] && [ "$v" != BUILD ]; then
      hwasan_archive_check "$crate" "$td" || M_RED=1
    fi
    # G-TSAN: the same for every C archive built for the instrumented target.
    if [ "$san" = tsan ] && [ "$v" != BUILD ]; then
      tsan_archive_check "$td" || M_RED=1
    fi
    if [ -z "$shared" ]; then drop_target "$td"; fi
    local nt
    nt="$(no_tests_reason "$spec")"
    if [ -n "$nt" ]; then
      case "$v" in
        VACUOUS)
          echo "  ⏭  $san $spec: 0 tests on $OS by construction — $nt"
          M_ROWS="$M_ROWS$spec|NO-TESTS($OS)|0|$((t1 - t0))"$'\n'
          continue ;;
        GREEN*)
          M_RED=1
          echo "  ❌ $san $spec: NO_TESTS_ON says '$spec' has no test on $OS, but ${v#GREEN } ran — the entry is stale"
          M_ROWS="$M_ROWS$spec|STALE-NO-TESTS|${v#GREEN }|$((t1 - t0))"$'\n'
          continue ;;
      esac
    fi
    case "$v" in
      GREEN*)
        M_TOTAL=$((M_TOTAL + ${v#GREEN }))
        echo "  ✅ $san $spec: ${v#GREEN } test(s) ran, $((t1 - t0))s — $(grep '^test result' "$log" | awk '{p+=$4; i+=$8} END {printf "%d passed, %d ignored", p, i}')"
        M_ROWS="$M_ROWS$spec|GREEN|${v#GREEN }|$((t1 - t0))"$'\n' ;;
      FINDING)
        M_RED=1
        echo "  ❌ $san $spec: SANITIZER FINDING ($((t1 - t0))s) — log $log"
        grep -E "$FINDING_RE" "$log" | sort -u | head -6 | sed 's/^/       /'
        M_ROWS="$M_ROWS$spec|FINDING|$(tests_ran "$log")|$((t1 - t0))"$'\n' ;;
      FAIL)
        M_RED=1
        echo "  ❌ $san $spec: tests FAILED under the sanitizer, rc=$rc ($((t1 - t0))s) — log $log"
        grep -E '^test .* FAILED$|^error: test failed|signal:' "$log" | head -6 | sed 's/^/       /'
        M_ROWS="$M_ROWS$spec|FAIL|$(tests_ran "$log")|$((t1 - t0))"$'\n' ;;
      HANG)
        M_RED=1
        echo "  ❌ $san $spec: $(grep -m1 "^$HANG_MARK after " "$log" | sed "s/^$HANG_MARK/HANG/") — the process tree was killed; log $log"
        M_ROWS="$M_ROWS$spec|HANG|$(tests_ran "$log")|$((t1 - t0))"$'\n' ;;
      VACUOUS)
        M_RED=1
        echo "  ❌ $san $spec: VACUOUS — 0 tests ran (filter '${filter:-<none>}'); a green over an empty set is not a pass"
        M_ROWS="$M_ROWS$spec|VACUOUS|0|$((t1 - t0))"$'\n' ;;
      BUILD)
        M_BLOCKED=1
        echo "  ⛔ $san $spec: BLOCKED — did not build under the sanitizer, rc=$rc — log $log"
        grep -m3 -E 'error(\[E[0-9]+\])?: |linking with' "$log" | sed 's/^/       /'
        M_ROWS="$M_ROWS$spec|BLOCKED(build)|0|$((t1 - t0))"$'\n' ;;
    esac
  done
  if [ -n "$shared" ]; then drop_target "$shared"; fi
  M_WALL=$(($(date +%s) - m0))
}

matrix_summary() {  # matrix_summary SAN
  echo "--- $1 matrix summary: $M_TOTAL test(s) ran, wall ${M_WALL}s ($((M_WALL / 60)) min) ---"
  printf '%s' "$M_ROWS" | awk -F'|' 'NF {printf "  %-36s %-15s %6s tests %6ss\n", $1, $2, $3, $4}'
}

# Excluded-or-scoped completeness. check_members SAN SPEC_LIST EXCL_LIST
# Prints RED lines; returns 1 on a stale exclusion or (msan) an unlisted member.
check_members() {
  local mode="$1" members bad=0 e c
  members="$(workspace_members)"
  if [ -z "$members" ]; then echo "  ❌ cargo metadata returned no workspace members"; return 1; fi
  shift
  for e in "$@"; do
    c="${e%%|*}"; c="${c%%:*}"
    if ! printf '%s\n' "$members" | grep -qx "$c"; then
      echo "  ❌ $mode: '$c' is listed but is not a workspace member (stale entry; a rename must update it)"
      bad=1
    fi
  done
  return $bad
}

# The Miri-exception cross-check (design §B: "every Miri-excepted test actually
# ran under G-ASAN"): san_miri_crosscheck.py, over this pass's logs, with the
# per-OS expected-absent list san_expected_absent.tsv. See the script's header.
asan_claims_crosscheck() {  # SAN
  "$PY" "$GATES_DIR/san_miri_crosscheck.py" --root "$ROOT" --src "${SAN_CLAIMS_SRC:-$ROOT}" \
    --logdir "$SAN_LOG_ROOT/$1" --os "$OS" --label "$1" \
    --expected "${SAN_EXPECTED_ABSENT:-$GATES_DIR/san_expected_absent.tsv}"
}

gate_verdict() {  # gate_verdict ID EXTRA_RED
  local id="$1" extra="$2"
  if [ "$M_RED" -ne 0 ] || [ "$extra" -ne 0 ]; then echo "$id: RED"; return 1; fi
  if [ "$M_BLOCKED" -ne 0 ]; then echo "$id: BLOCKED (a crate did not build, or the disk floor was reached; see above)"; return 3; fi
  if [ "$M_TOTAL" -eq 0 ]; then echo "$id: RED — VACUOUS, 0 tests ran"; return 1; fi
  echo "$id: GREEN ($M_TOTAL test(s) under the sanitizer, 0 findings)"
  return 0
}

# ci.yml's `ci` job sets RUSTTY_SANITIZERS_OWN_JOB: the gates run in its
# `sanitizers` job instead (one job per sanitizer), as G-VALGRIND runs in the
# `valgrind` job. BLOCKED here is a WARNING naming that job, never a pass.
own_job_blocked() {  # own_job_blocked ID
  if [ -n "${RUSTTY_SANITIZERS_OWN_JOB:-}" ]; then
    echo "BLOCKED: $1 — this job defers to $RUSTTY_SANITIZERS_OWN_JOB (RUSTTY_SANITIZERS_OWN_JOB is set), which runs it"
    exit 3
  fi
}

need_nightly() {
  if ! cargo +nightly --version >/dev/null 2>&1; then
    echo "BLOCKED: the +nightly toolchain is not installed (-Zsanitizer needs it)"; exit 3
  fi
}

# Build the spec list for a workspace gate: members minus EXCL (+ EXTRA lanes).
scope_minus() {  # scope_minus "excl entries" -> stdout specs
  local excl="$1" m
  for m in $(workspace_members); do
    printf '%s\n' "$excl" | grep -q "^$m|" && continue
    echo "$m"
  done
}

# G-HWASAN's host requirements, checked first so the BLOCKED reason names the
# host. HWAddressSanitizer exists for aarch64 Linux (and Android) only: rustc
# rejects -Zsanitizer=hwaddress for aarch64-apple-darwin and
# x86_64-unknown-linux-gnu ("hwaddress sanitizer is not supported for this
# target"; P7-W16b, nightly 2026-07-22). It relies on the aarch64 Top-Byte-
# Ignore and the kernel's tagged-address ABI (PR_SET_TAGGED_ADDR_CTRL,
# CONFIG_ARM64_TAGGED_ADDR_ABI), which the runtime turns on at init.
hwasan_host_blocked() {
  if [ "$OS" != "Linux" ] || [ "$ARCH" != "aarch64" ]; then
    echo "BLOCKED: G-HWASAN needs HWAddressSanitizer, which exists on aarch64 Linux only (rustc: 'hwaddress sanitizer is not supported for this target' on $OS/$ARCH). It runs on the aarch64 Linux VM (rvm)."
    exit 3
  fi
  if ! rustup +nightly component list --installed 2>/dev/null | grep -q '^rust-src'; then
    echo "BLOCKED: G-HWASAN needs -Zbuild-std, which needs the nightly rust-src component (rustup component add rust-src --toolchain nightly)"
    exit 3
  fi
  if ! command -v clang >/dev/null 2>&1 || ! command -v ld.lld >/dev/null 2>&1; then
    echo "BLOCKED: G-HWASAN links with lld (GNU ld mislinks tagged globals) and compiles the vendored C with clang -fsanitize=hwaddress; clang or ld.lld is not on PATH"
    exit 3
  fi
}

gate_asan_like() {  # gate_asan_like asan|lsan|hwasan
  local san="$1" id excl extra=0 e
  case "$san" in asan) id="G-ASAN" ;; lsan) id="G-LSAN" ;; hwasan) id="G-HWASAN" ;; esac
  if [ "$san" = hwasan ]; then hwasan_host_blocked; fi
  own_job_blocked "$id"
  need_nightly
  # G-LSAN runs where rustc's ASan runtime carries a working leak checker:
  # Linux and macOS (verified P7-W16a; see the platform table at the top).
  # Anywhere else it is unverified, so BLOCKED, never GREEN.
  if [ "$san" = lsan ] && [ "$OS" != "Linux" ] && [ "$OS" != "Darwin" ]; then
    echo "BLOCKED: $id needs LeakSanitizer through rustc's ASan runtime (detect_leaks=1), verified on Linux and macOS only, not on $OS. It runs on the Mac, on the Linux VM (rvm) and in ci.yml's sanitizers job."
    exit 3
  fi
  if [ "$san" = lsan ] && [ ! -r "$LSAN_SUPP" ]; then
    echo "$id: RED — $LSAN_SUPP is missing (SANITIZER_SUPPRESSIONS.md's file)"; exit 1
  fi
  excl=""
  case "$san" in
    asan) for e in ${ASAN_EXCLUDES[@]+"${ASAN_EXCLUDES[@]}"}; do excl="$excl$e"$'\n'; done ;;
    lsan) for e in ${LSAN_EXCLUDES[@]+"${LSAN_EXCLUDES[@]}"}; do excl="$excl$e"$'\n'; done ;;
    hwasan) for e in ${HWASAN_EXCLUDES[@]+"${HWASAN_EXCLUDES[@]}"}; do excl="$excl$e"$'\n'; done ;;
  esac
  case "$san" in
    hwasan) echo "### $id: HWAddressSanitizer (-Zbuild-std, tagged globals, lld; C via cc with -fsanitize=hwaddress) over the workspace test suites ($OS/$ARCH) ###" ;;
    *) echo "### $id: AddressSanitizer$( [ "$san" = lsan ] && echo ' + LeakSanitizer (detect_leaks=1)') over the workspace test suites ($OS/$ARCH) ###" ;;
  esac
  if [ -n "$excl" ]; then
    echo "excluded (cannot run under the sanitizer):"
    printf '%s' "$excl" | awk -F'|' 'NF {printf "  - %s: %s\n", $1, $2}'
  fi
  local -a check=()
  while IFS= read -r e; do [ -n "$e" ] && check+=("$e"); done <<EOF
$excl
EOF
  check_members "$id exclusion" ${check[@]+"${check[@]}"} || extra=1
  if [ "$san" = hwasan ]; then
    check_members "G-HWASAN uninstrumented" ${HWASAN_UNINSTRUMENTED[@]+"${HWASAN_UNINSTRUMENTED[@]}"} || extra=1
  fi
  rm -rf -- "${SAN_LOG_ROOT:?}/$san"
  local -a specs_a=()
  while IFS= read -r e; do [ -n "$e" ] && specs_a+=("$e"); done <<EOF
$(scope_minus "$excl")
EOF
  for e in ${FEATURE_LANES[@]+"${FEATURE_LANES[@]}"}; do specs_a+=("${e%%|*}"); echo "feature lane: ${e%%|*} — ${e#*|}"; done
  if [ "$OS" = "Linux" ]; then
    for e in ${LINUX_FEATURE_LANES[@]+"${LINUX_FEATURE_LANES[@]}"}; do specs_a+=("${e%%|*}"); echo "feature lane: ${e%%|*} — ${e#*|}"; done
  fi
  check_members "$id feature lane" ${FEATURE_LANES[@]+"${FEATURE_LANES[@]}"} ${LINUX_FEATURE_LANES[@]+"${LINUX_FEATURE_LANES[@]}"} || extra=1
  check_members "NO_TESTS_ON" ${NO_TESTS_ON[@]+"${NO_TESTS_ON[@]}"} || extra=1
  if [ -n "${SAN_ONLY:-}" ]; then
    # Debug aid: SAN_ONLY="a b" narrows the run (the verdict says so: never a full green).
    local -a narrowed=()
    for e in ${specs_a[@]+"${specs_a[@]}"}; do case " $SAN_ONLY " in *" ${e%%:*} "*) narrowed+=("$e") ;; esac; done
    specs_a=(${narrowed[@]+"${narrowed[@]}"})
    echo "NOTE: SAN_ONLY narrows this run to: ${specs_a[*]:-<nothing>} — the verdict is BLOCKED, never GREEN"
  fi
  matrix_run "$san" "" ${specs_a[@]+"${specs_a[@]}"}
  matrix_summary "$san"
  if [ -n "${SAN_ONLY:-}" ]; then
    # A narrowed run cannot verify the Miri exceptions of the crates it left
    # out; it reports BLOCKED, never GREEN.
    echo "  BLOCKED: SAN_ONLY narrowed the run; the Miri-exception cross-check needs the whole pass"
    M_BLOCKED=1
  else
    asan_claims_crosscheck "$san" || extra=1
  fi
  gate_verdict "$id" "$extra"
}

# Every member in exactly one of MSAN_SCOPE / MSAN_EXCLUDES, no stale entry,
# and every MSAN_SKIPS crate in scope. Returns 1 (and prints why) otherwise.
msan_partition_check() {
  local bad=0 m listed e
  check_members "G-MSAN scope" ${MSAN_SCOPE[@]+"${MSAN_SCOPE[@]}"} || bad=1
  check_members "G-MSAN exclusion" ${MSAN_EXCLUDES[@]+"${MSAN_EXCLUDES[@]}"} || bad=1
  listed="$(printf '%s\n' ${MSAN_SCOPE[@]+"${MSAN_SCOPE[@]}"} ${MSAN_EXCLUDES[@]+"${MSAN_EXCLUDES[@]}"} | sed 's/[|:].*//')"
  for m in $(workspace_members); do
    printf '%s\n' "$listed" | grep -qx "$m" || { echo "  ❌ G-MSAN: workspace member '$m' is in neither MSAN_SCOPE nor MSAN_EXCLUDES (add it, or record why not)"; bad=1; }
  done
  for e in ${MSAN_SKIPS[@]+"${MSAN_SKIPS[@]}"}; do
    case " ${MSAN_SCOPE[*]:-} " in *" ${e%%|*} "*) ;; *) echo "  ❌ G-MSAN: MSAN_SKIPS names '${e%%|*}', which is not in MSAN_SCOPE (stale)"; bad=1 ;; esac
  done
  return $bad
}

# Every TSAN_SERIAL spec must be a TSAN_SCOPE spec (else stale, RED); each is
# printed with its reason.
tsan_serial_check() {
  local e s bad=0 found
  for e in ${TSAN_SERIAL[@]+"${TSAN_SERIAL[@]}"}; do
    found=0
    for s in ${TSAN_SCOPE[@]+"${TSAN_SCOPE[@]}"}; do [ "${s%%|*}" = "${e%%|*}" ] && found=1; done
    if [ "$found" -eq 1 ]; then printf '  tests one at a time on Linux: %s — %s\n' "${e%%|*}" "${e#*|}"
    else echo "  ❌ G-TSAN: TSAN_SERIAL names '${e%%|*}', which is not a TSAN_SCOPE spec (stale)"; bad=1; fi
  done
  return $bad
}

# G-TSAN (P7-W17): ThreadSanitizer over TSAN_SCOPE. Hosts: macOS and Linux
# (rustc's target specs for aarch64-apple-darwin, x86_64-apple-darwin and the
# Linux gnu targets carry THREAD; verified on aarch64-apple-darwin, P7-W17).
# Anywhere else it is unverified, so BLOCKED, never GREEN.
gate_tsan() {
  own_job_blocked G-TSAN
  need_nightly
  if [ "$OS" != "Darwin" ] && [ "$OS" != "Linux" ]; then
    echo "BLOCKED: G-TSAN needs ThreadSanitizer, verified on macOS and Linux only, not on $OS. It runs on the Mac and on the Linux VM (rvm)."
    exit 3
  fi
  if ! rustup +nightly component list --installed 2>/dev/null | grep -q '^rust-src'; then
    echo "BLOCKED: G-TSAN needs -Zbuild-std, which needs the nightly rust-src component (rustup component add rust-src --toolchain nightly)"
    exit 3
  fi
  if ! command -v clang >/dev/null 2>&1; then
    echo "BLOCKED: G-TSAN compiles the vendored C with clang -fsanitize=thread; clang is not on PATH"
    exit 3
  fi
  if [ ! -r "$TSAN_SUPP" ]; then
    echo "G-TSAN: RED — $TSAN_SUPP is missing (SANITIZER_SUPPRESSIONS.md's file)"; exit 1
  fi
  echo "### G-TSAN: ThreadSanitizer (-Zbuild-std; C via cc with -fsanitize=thread) over the threaded crates ($OS/$ARCH) ###"
  local e extra=0
  local -a specs_t=()
  for e in ${TSAN_SCOPE[@]+"${TSAN_SCOPE[@]}"}; do
    specs_t+=("${e%%|*}")
    printf '  scope: %s — %s\n' "${e%%|*}" "${e#*|}"
  done
  for e in ${TSAN_UNINSTRUMENTED[@]+"${TSAN_UNINSTRUMENTED[@]}"}; do printf '  C not instrumented: %s — %s\n' "${e%%|*}" "${e#*|}"; done
  check_members "G-TSAN scope" ${TSAN_SCOPE[@]+"${TSAN_SCOPE[@]}"} || extra=1
  check_members "NO_TESTS_ON" ${NO_TESTS_ON[@]+"${NO_TESTS_ON[@]}"} || extra=1
  tsan_serial_check || extra=1
  rm -rf -- "${SAN_LOG_ROOT:?}/tsan"
  if [ -n "${SAN_ONLY:-}" ]; then
    # Debug aid, as for the ASan-like gates: the verdict is BLOCKED, never GREEN.
    local -a narrowed=()
    for e in ${specs_t[@]+"${specs_t[@]}"}; do case " $SAN_ONLY " in *" ${e%%:*} "*) narrowed+=("$e") ;; esac; done
    specs_t=(${narrowed[@]+"${narrowed[@]}"})
    echo "NOTE: SAN_ONLY narrows this run to: ${specs_t[*]:-<nothing>} — the verdict is BLOCKED, never GREEN"
  fi
  matrix_run tsan "" ${specs_t[@]+"${specs_t[@]}"}
  matrix_summary tsan
  if [ -n "${SAN_ONLY:-}" ]; then echo "  BLOCKED: SAN_ONLY narrowed the run"; M_BLOCKED=1; fi
  gate_verdict "G-TSAN" "$extra"
}

# G-LSAN-GTK (P7-W24; NEW-FINDINGS N502's Linux coverage gap). No test G-LSAN
# runs on Linux creates and closes a core Surface: on Linux a surface comes
# only from the GTK runtime, and the GTK tests that open one are #[ignore]d
# live tests that need a compositor. So the per-surface teardown leaks N502
# fixed were reachable on Linux, and only macOS tests saw them. This lane runs
# one such test, gtk_window_live (a real Window, a tab, a live core Surface
# drawing frames and running /bin/sh, then the window destroyed, the Surface
# finalized and the shell reaped), under ASan with detect_leaks=1 inside
# .port/tools/gtk_live_headless_sway.sh, with lsan.supp (S1/S2, Mesa) plus
# lsan-gtk.supp (the GTK stack's own process-lifetime state, each entry cited
# in SANITIZER_SUPPRESSIONS.md with a negative control). Linux only: the GTK
# runtime is Linux/FreeBSD's, and only Linux is verified. Exactly one test must
# run and pass (non-vacuous).
GTK_LSAN_SPEC="rustty-apprt:gtk,wayland,x11"
GTK_LSAN_TEST="gtk_window_live"
GTK_LSAN_FILTER="window_hosts_a_live_terminal --exact --ignored"
LSAN_GTK_SUPP="${LSAN_GTK_SUPP:-$SUPP_DIR/lsan-gtk.supp}"
SWAY_WRAPPER="${SWAY_WRAPPER:-$ROOT/.port/tools/gtk_live_headless_sway.sh}"
# Ceilings on lsan-gtk.supp's matches (P7-W25). LSan treats a suppressed block
# as live, so whatever it points to is not reported either: a suppression can
# hide more than its own allocation. Each lsan-gtk.supp entry therefore has a
# ceiling on the allocations it may match in the lane's run ("<pattern>|<max
# count>", the pattern as the file spells it after `leak:`), read from LSan's
# "Suppressions used" table (print_suppressions=1). A match count above its
# ceiling is RED, and so is an entry with no ceiling (unpinned) or a ceiling
# whose entry is gone (stale). Ceilings: rvm P7-W25 matched 6,913 and 2,197-2,199
# over four runs, plus about 10% (see
# SANITIZER_SUPPRESSIONS.md, G-LSAN-GTK).
LSAN_GTK_CEILINGS=(
  "libpangoft2-1.0.so|7600"
  "libpangocairo-1.0.so|2420"
)
# The entry that matches in every run (G1: the Pango font map's patterns,
# 6,913 allocations each run on rvm). After a run, a missing "Suppressions
# used" table or a zero count for it means the ceilings checked nothing (the
# log moved, print_suppressions was dropped, LSan never ran): RED, never a
# vacuous GREEN.
LSAN_GTK_MUST_MATCH="libpangoft2-1.0.so"
# lsan_gtk_ceiling_check LOG [SUPP] [RAN]: RED (1) on any ceiling breach,
# unpinned entry or stale ceiling; with RAN=1 (the lane's test ran) also when
# the log has no "Suppressions used" table or LSAN_GTK_MUST_MATCH matched
# nothing. Prints one line per entry.
lsan_gtk_ceiling_check() {
  local log="$1" supp="${2:-$LSAN_GTK_SUPP}" ran="${3:-0}" bad=0 pat e max n found
  if [ "$ran" = 1 ] && { [ ! -r "$log" ] || ! grep -q '^Suppressions used:' "$log"; }; then
    echo "  ❌ G-LSAN-GTK: the test ran but its log ($log) has no LSan \"Suppressions used\" table: the ceilings checked nothing"
    return 1
  fi
  while IFS= read -r pat; do
    found=""
    for e in ${LSAN_GTK_CEILINGS[@]+"${LSAN_GTK_CEILINGS[@]}"}; do
      [ "${e%|*}" = "$pat" ] && found="${e##*|}"
    done
    if [ -z "$found" ]; then
      echo "  ❌ G-LSAN-GTK: lsan-gtk.supp entry '$pat' has no LSAN_GTK_CEILINGS row (unpinned)"; bad=1; continue
    fi
    # LSan's table rows: "<count> <bytes> <template>".
    n="$(awk -v p="$pat" '/^Suppressions used:/ {t=1; next} t && $3 == p {s += $1} t && /^-+$/ {t=0} END {print s + 0}' "$log")"
    if [ "$n" -gt "$found" ]; then
      echo "  ❌ G-LSAN-GTK: '$pat' matched $n allocations, above its ceiling $found (a leak may be growing under it)"; bad=1
    elif [ "$ran" = 1 ] && [ "$pat" = "$LSAN_GTK_MUST_MATCH" ] && [ "$n" -eq 0 ]; then
      echo "  ❌ G-LSAN-GTK: '$pat' matched nothing, though it matches in every run: the ceilings checked nothing"; bad=1
    else
      echo "  suppression '$pat': $n allocations matched (ceiling $found)"
    fi
  done < <(sed -n 's/^leak://p' "$supp")
  for e in ${LSAN_GTK_CEILINGS[@]+"${LSAN_GTK_CEILINGS[@]}"}; do
    max="${e%|*}"
    if ! sed -n 's/^leak://p' "$supp" | grep -qxF -- "$max"; then
      echo "  ❌ G-LSAN-GTK: LSAN_GTK_CEILINGS names '$max', which is not a lsan-gtk.supp entry (stale)"; bad=1
    fi
  done
  return $bad
}
gate_lsan_gtk() {
  if [ "$OS" != "Linux" ]; then
    echo "BLOCKED: G-LSAN-GTK runs a live GTK surface test, and the GTK runtime is Linux's (as in Ghostty; not verified on $OS). It runs on the Linux VM (rvm); on macOS, G-LSAN's macos_window_host tests close core Surfaces."
    exit 3
  fi
  # No hosted job runs this lane (it needs a headless sway; V33), so it does
  # not defer to RUSTTY_SANITIZERS_OWN_JOB's job: that would be a false claim.
  if [ -n "${RUSTTY_SANITIZERS_OWN_JOB:-}" ]; then
    echo "BLOCKED: G-LSAN-GTK has no hosted CI job (it needs a headless sway compositor; VERIFICATION_DEBT V33); it runs on the Linux VM (rvm)"
    exit 3
  fi
  need_nightly
  if ! command -v "${SWAY_BIN:-sway}" >/dev/null 2>&1; then
    echo "BLOCKED: G-LSAN-GTK runs its test inside a headless sway (.port/tools/gtk_live_headless_sway.sh); sway is not installed"
    exit 3
  fi
  if [ -z "${XDG_RUNTIME_DIR:-}" ]; then
    echo "BLOCKED: G-LSAN-GTK needs XDG_RUNTIME_DIR for the headless sway's sockets; it is unset"
    exit 3
  fi
  local f
  for f in "$LSAN_SUPP" "$LSAN_GTK_SUPP"; do
    if [ ! -r "$f" ]; then echo "G-LSAN-GTK: RED — $f is missing (SANITIZER_SUPPRESSIONS.md's file)"; exit 1; fi
  done
  echo "### G-LSAN-GTK: ASan + LeakSanitizer (detect_leaks=1) over a live GTK surface, created and closed ($OS/$ARCH) ###"
  echo "  test: $GTK_LSAN_SPEC --test $GTK_LSAN_TEST -- $GTK_LSAN_FILTER, inside ${SWAY_WRAPPER#"$ROOT"/}"
  # One suppressions file (LSAN_OPTIONS takes one): lsan.supp, then
  # lsan-gtk.supp, concatenated into the lane's own dir.
  local lroot="$SAN_TARGET_ROOT/lsan-gtk" comb
  mkdir -p "$lroot" || exit 1
  comb="$lroot/lsan-combined.supp"
  case "$comb" in *:*|*' '*) echo "G-LSAN-GTK: RED — the suppressions path '$comb' holds ':' or a space (LSAN_OPTIONS splits on them)"; exit 1 ;; esac
  cat "$LSAN_SUPP" "$LSAN_GTK_SUPP" >"$comb" || exit 1
  rm -rf -- "${SAN_LOG_ROOT:?}/lsan-gtk"
  # The first crate of a fresh target dir builds every instrumented
  # dependency of the GTK apprt: bound it at 2400 s unless overridden.
  LSAN_SUPP="$comb" SAN_TEST_TARGET="$GTK_LSAN_TEST" SAN_WRAP="$SWAY_WRAPPER" \
    SAN_CRATE_TIMEOUT_S="${SAN_CRATE_TIMEOUT_S:-2400}" SAN_TARGET_ROOT="$lroot" SAN_LOG_ROOT="$SAN_LOG_ROOT/lsan-gtk" \
    matrix_run lsan "$GTK_LSAN_FILTER" "$GTK_LSAN_SPEC"
  matrix_summary lsan
  local extra=0
  if [ "$M_RED" -eq 0 ] && [ "$M_BLOCKED" -eq 0 ] && [ "$M_TOTAL" -ne 1 ]; then
    echo "  ❌ G-LSAN-GTK: $M_TOTAL test(s) ran; exactly 1 ($GTK_LSAN_TEST::window_hosts_a_live_terminal) must run"
    extra=1
  fi
  if [ "$M_BLOCKED" -eq 0 ]; then
    local ran=0
    [ "$M_TOTAL" -ge 1 ] && ran=1
    lsan_gtk_ceiling_check "$SAN_LOG_ROOT/lsan-gtk/lsan/$(printf '%s' "$GTK_LSAN_SPEC" | tr ':,' '__').log" "$LSAN_GTK_SUPP" "$ran" || extra=1
  fi
  rm -f -- "$comb"; rmdir -- "$lroot" 2>/dev/null
  gate_verdict "G-LSAN-GTK" "$extra"
}

MSAN_AARCH64_NOTE=""
gate_msan() {
  own_job_blocked G-MSAN
  need_nightly
  if [ "$OS" != "Linux" ]; then
    echo "BLOCKED: G-MSAN needs MemorySanitizer, which exists on Linux only (not on $OS: compiler-rt builds MSan for Linux/FreeBSD/NetBSD only, and rustc rejects -Zsanitizer=memory for this target). It runs on the Linux VM (rvm) and in ci.yml's sanitizers job; on macOS the uninitialized-read coverage is Miri's (G6/G6c) plus that Linux lane."
    exit 3
  fi
  if [ -n "$MSAN_AARCH64_NOTE" ] && [ "$ARCH" = "aarch64" ]; then
    echo "BLOCKED: $MSAN_AARCH64_NOTE"
    exit 3
  fi
  if ! rustup +nightly component list --installed 2>/dev/null | grep -q '^rust-src'; then
    echo "BLOCKED: G-MSAN needs -Zbuild-std, which needs the nightly rust-src component (rustup component add rust-src --toolchain nightly)"
    exit 3
  fi
  if ! command -v clang >/dev/null 2>&1; then
    echo "BLOCKED: G-MSAN compiles the vendored C with clang -fsanitize=memory; clang is not on PATH"
    exit 3
  fi
  echo "### G-MSAN: MemorySanitizer (-Zbuild-std, C via cc with -fsanitize=memory) over the phase-1 scope ($OS/$ARCH) ###"
  local e extra=0
  echo "excluded (link uninstrumented C/C++; MSan cannot run them):"
  for e in ${MSAN_EXCLUDES[@]+"${MSAN_EXCLUDES[@]}"}; do printf '  - %s: %s\n' "${e%%|*}" "${e#*|}"; done
  msan_partition_check || extra=1
  for e in ${MSAN_SKIPS[@]+"${MSAN_SKIPS[@]}"}; do printf '  skipped in %s: tests matching %s — %s\n' "${e%%|*}" "$(printf '%s' "$e" | cut -d'|' -f2)" "${e##*|}"; done
  rm -rf -- "${SAN_LOG_ROOT:?}/msan"
  matrix_run msan "" ${MSAN_SCOPE[@]+"${MSAN_SCOPE[@]}"}
  matrix_summary msan
  if [ -n "${SAN_MSAN_DIAG:-}" ]; then echo "  BLOCKED: SAN_MSAN_DIAG ran without the named skips (a diagnosis run)"; M_BLOCKED=1; fi
  gate_verdict "G-MSAN" "$extra"
}

# ---------------------------------------------------------------------------
# SELF-TEST (G-SAN-SELF).
# ---------------------------------------------------------------------------
# shellcheck disable=SC2016  # the arms hand single-quoted shell snippets to a sourced copy
self_test() {
  need_nightly
  local st fail=0 out rc
  st="$(mktemp -d "${TMPDIR:-/tmp}/san-self.XXXXXX")"
  echo "### G-SAN-SELF: sanitizers.sh --self-test (work dir $st) ###"
  # --- arm C: the log classifier over canned logs (no build) ---
  mkdir -p "$st/logs"
  printf 'running 3 tests\ntest a ... ok\n==1==ERROR: AddressSanitizer: heap-buffer-overflow\nSUMMARY: AddressSanitizer: heap-buffer-overflow\n' >"$st/logs/finding.log"
  # shellcheck disable=SC2016  # literal backticks, as cargo prints them
  printf 'error[E0425]: cannot find value `x`\nerror: could not compile `foo`\n' >"$st/logs/build.log"
  # shellcheck disable=SC2016  # literal backticks, as cargo prints them
  printf 'running 2 tests\ntest a ... ok\ntest b ... FAILED\nfailures:\ntest result: FAILED. 1 passed; 1 failed\nerror: test failed, to rerun pass `--lib`\n' >"$st/logs/fail.log"
  printf 'running 0 tests\ntest result: ok. 0 passed; 0 failed\n' >"$st/logs/vacuous.log"
  printf 'running 4 tests\ntest result: ok. 4 passed; 0 failed; 1 ignored\nrunning 1 test\ntest result: ok. 1 passed; 0 failed; 0 ignored\n' >"$st/logs/green.log"
  printf 'running 1 test\ntest result: ok. 0 passed; 0 failed; 1 ignored\n' >"$st/logs/allignored.log"
  # shellcheck disable=SC2016  # literal backticks, as cargo prints them
  printf '     Running unittests src/lib.rs\nrunning 3 tests\nerror: test failed, to rerun pass `--lib`\nprocess did not exit successfully: `x` (signal: 11, SIGSEGV)\n' >"$st/logs/crash.log"
  printf 'running 1 test\n==9==WARNING: MemorySanitizer: use-of-uninitialized-value\n' >"$st/logs/msan.log"
  printf 'running 2 tests\n=================================================================\n==7==ERROR: LeakSanitizer: detected memory leaks\nSUMMARY: AddressSanitizer: 64 byte(s) leaked in 1 allocation(s).\n' >"$st/logs/leak.log"
  ck() {  # ck NAME RC WANT
    local got; got="$(classify "$st/logs/$1.log" "$2")"
    if [ "$got" = "$3" ]; then echo "  ok   C classifier: $1 (rc=$2) -> $got"; else echo "  FAIL C classifier: $1 (rc=$2) -> '$got', want '$3'"; fail=1; fi
  }
  ck finding 1 FINDING; ck build 101 BUILD; ck fail 101 FAIL; ck vacuous 0 VACUOUS
  # allocator_may_return_null=1 makes MSan/ASan print "failed to allocate" for
  # the out-of-memory tests' huge requests: informational, not a finding.
  printf 'running 1 test\n==9==WARNING: MemorySanitizer failed to allocate 0x7fffffffffffffff bytes\ntest result: ok. 1 passed; 0 failed; 0 ignored\n' >"$st/logs/oominfo.log"
  ck green 0 "GREEN 5"; ck msan 77 FINDING; ck leak 23 FINDING; ck oominfo 0 "GREEN 1"
  ck allignored 0 VACUOUS; ck crash 101 FAIL
  printf 'running 1 test\n==5==ERROR: HWAddressSanitizer: tag-mismatch on address 0xec0dfffe01e8\nSUMMARY: HWAddressSanitizer: tag-mismatch\n' >"$st/logs/hwasan.log"
  ck hwasan 99 FINDING
  # A hang is RED even when the part of the log written before it looks clean.
  printf 'running 4 tests\ntest result: ok. 4 passed; 0 failed; 0 ignored\n\nSANITIZERS-HANG after 9s (no verdict)\n' >"$st/logs/hang.log"
  ck hang 0 HANG; ck hang 143 HANG
  # --- arm D: drop_target removes only san-<san> and san-<san>-<crate> dirs ---
  mkdir -p "$st/t/san-asan" "$st/t/san-asan-foo" "$st/t/san-other"
  if ( SAN_TARGET_ROOT="$st/t"; drop_target "$st" ) 2>/dev/null; then echo "  FAIL D drop_target accepted a non-sanitizer dir"; fail=1
  elif ( SAN_TARGET_ROOT="$st/t"; drop_target "$st/t/san-other" ) 2>/dev/null; then echo "  FAIL D drop_target accepted san-other"; fail=1
  elif ! ( SAN_TARGET_ROOT="$st/t"; drop_target "$st/t/san-asan" && drop_target "$st/t/san-asan-foo" ) || [ -d "$st/t/san-asan" ] || [ -d "$st/t/san-asan-foo" ]; then
    echo "  FAIL D drop_target did not remove the shared and the per-crate dir"; fail=1
  else echo "  ok   D drop_target removes san-asan and san-asan-<crate>, refuses anything else"; fi
  # --- arm E: the Miri-exception cross-check over a synthetic source + logs ---
  mkdir -p "$st/src/crates/foo/src" "$st/xlogs/asan"
  printf '[package]\nname = "foo"\n' >"$st/src/crates/foo/Cargo.toml"
  printf 'mod a;\n' >"$st/src/crates/foo/src/lib.rs"
  cat >"$st/src/crates/foo/src/a.rs" <<'RS'
// A comment that quotes `#[cfg_attr(miri, ignore)]` is not an annotation.
#[cfg(test)]
mod tests {
    #[test]
    #[cfg_attr(miri, ignore = "foreign function")]
    fn ran() {}
    #[test]
    #[cfg_attr(
        miri,
        ignore = "multi-line attribute"
    )]
    fn ignored_one() {}
    #[test]
    #[cfg_attr(all(miri, debug_assertions), ignore = "G6c only")]
    fn absent_one() {}
    #[test]
    #[cfg_attr(not(miri), ignore = "not a Miri exception")]
    fn not_excepted() {}
}
RS
  : >"$st/exp-empty.tsv"
  xc() {  # xc LOGTEXT EXPECTED-TSV-TEXT -> sets out, rc
    printf '%b' "$1" >"$st/xlogs/asan/foo.log"
    printf '%b' "$2" >"$st/exp.tsv"
    out="$(SAN_CLAIMS_SRC="$st/src" SAN_LOG_ROOT="$st/xlogs" SAN_EXPECTED_ABSENT="$st/exp.tsv" asan_claims_crosscheck asan 2>&1)"; rc=$?
  }
  xe() {  # xe NAME WANT_RC MUST_GREP
    if [ "$rc" -eq "$2" ] && printf '%s' "$out" | grep -q -- "$3"; then echo "  ok   E $1"
    else echo "  FAIL E $1 (rc=$rc): $out"; fail=1; fi
  }
  R='     Running unittests src/lib.rs (x)\n'
  xc "${R}test a::tests::ran ... ok\ntest a::tests::ignored_one ... ok\ntest a::tests::absent_one ... ok\ntest a::tests::not_excepted ... ignored\n" ""
  xe "control: all 3 Miri-excepted tests ran and passed -> GREEN (comment and not(miri) not counted)" 0 "3 Miri-excepted test(s); 3 ran and passed"
  xc "${R}test a::tests::ran ... ok\nhellotest a::tests::ignored_one ... ok\ntest a::tests::absent_one ... ok\n" ""
  xe "a result line with another test's unterminated stdout glued in front still counts (P7-W16b)" 0 "3 Miri-excepted test(s); 3 ran and passed"
  xc "${R}test a::tests::ran ... ok\ntest a::tests::ignored_one ... ignored\ntest a::tests::absent_one ... ok\n" ""
  xe "an ignored Miri-excepted test is RED (multi-line attribute parsed)" 1 "ignored_one (src/a.rs): Miri-excepted but ignored"
  xc "${R}test a::tests::ran ... ok\ntest a::tests::ignored_one ... ok\n" ""
  xe "an absent Miri-excepted test (all(miri, ..) form) is RED" 1 "a::absent_one (src/a.rs): Miri-excepted but absent"
  xc "${R}test a::tests::ran ... ok\ntest a::tests::ignored_one ... ok\n" "*\tfoo\ta::absent_one\tfixture reason\n"
  xe "the same absence on the expected-absent list is GREEN" 0 "1 expected absent"
  xc "${R}test a::tests::ran ... ok\ntest a::tests::ignored_one ... ok\ntest a::tests::absent_one ... ok\n" "*\tfoo\ta::absent_one\tfixture reason\n"
  xe "an expected-absent row whose test ran and passed is stale (RED)" 1 "ran and passed here (stale)"
  xc "${R}test a::tests::ran ... ok\ntest a::tests::ignored_one ... ok\ntest a::tests::absent_one ... ok\n" "*\tfoo\tz::nothing\tfixture reason\n"
  xe "an expected-absent row that matches nothing is stale (RED)" 1 "matches no Miri-excepted test (stale)"
  xc "${R}test b::tests::ran ... ok\ntest a::tests::ignored_one ... ok\ntest a::tests::absent_one ... ok\n" ""
  xe "an ok in another module (b::tests::ran) does not satisfy a::ran (module-path match)" 1 "a::ran (src/a.rs): Miri-excepted but absent"
  xc "${R}test a::tests::ran ... ok\nhellotest a::tests::ignored_one ... ok\n test a::tests::absent_one ... ok\n" ""
  xe "a result line prefixed by a test's unterminated stdout still counts" 0 "3 Miri-excepted test(s); 3 ran and passed"
  # A same-named test in a CHILD file (a/b.rs -> a::b) or a sibling inline mod
  # of a.rs must not satisfy a::tests::ran: only the inline mods enclosing the
  # annotation in its own file may sit between the file module and the fn.
  mkdir -p "$st/src/crates/foo/src/a"
  printf '#[test]\nfn ran() {}\n' >"$st/src/crates/foo/src/a/b.rs"
  xc "${R}test a::b::ran ... ok\ntest a::tests::ignored_one ... ok\ntest a::tests::absent_one ... ok\n" ""
  xe "an ok in a child file (a::b::ran) does not satisfy a::tests::ran" 1 "a::ran (src/a.rs): Miri-excepted but absent"
  xc "${R}test a::other::ran ... ok\ntest a::tests::ignored_one ... ok\ntest a::tests::absent_one ... ok\n" ""
  xe "an ok in a sibling inline mod (a::other::ran) does not satisfy a::tests::ran" 1 "a::ran (src/a.rs): Miri-excepted but absent"
  xc "${R}test a::tests::ran ... ok\ntest a::tests::ignored_one ... ok\ntest a::tests::absent_one ... FAILED\n" "*\tfoo\ta::absent_one\tfixture reason\n"
  xe "an expected-absent test that ran and FAILED is RED" 1 "on the .* expected-absent list, but it ran and FAILED"
  printf '#[cfg_attr(miri, ignore)]\nconst X: u8 = 0;\n' >>"$st/src/crates/foo/src/a.rs"
  xc "${R}test a::tests::ran ... ok\ntest a::tests::ignored_one ... ok\ntest a::tests::absent_one ... ok\n" ""
  xe "an annotation followed by something other than a fn is RED (never attached to a later fn)" 1 "not attached to a fn: foo src/a.rs"
  rm -f "$st/xlogs/asan/foo.log"
  xc "" ""; rm -f "$st/xlogs/asan/foo.log"
  out="$(SAN_CLAIMS_SRC="$st/src" SAN_LOG_ROOT="$st/xlogs" SAN_EXPECTED_ABSENT="$st/exp-empty.tsv" asan_claims_crosscheck asan 2>&1)"; rc=$?
  xe "a crate with Miri-excepted tests but no log (excluded) is RED" 1 "its crate has no asan log"

  # --- arm H: scope bookkeeping and verdicts over the REAL workspace (no build) ---
  hx() {  # hx NAME WANT_RC MUST_GREP SHELL-SNIPPET (run after sourcing this script)
    local o r
    o="$(bash -c '. "$0" --source-only; '"$4" "$GATES_DIR/sanitizers.sh" 2>&1)"; r=$?
    if [ "$r" -eq "$2" ] && { [ -z "$3" ] || printf "%s" "$o" | grep -q -- "$3"; }; then echo "  ok   H $1"
    else echo "  FAIL H $1 (rc=$r): $o" | head -5; fail=1; fi
  }
  hx "control: the real exclusions and feature lanes name workspace members" 0 "" \
    'check_members x ${ASAN_EXCLUDES[@]+"${ASAN_EXCLUDES[@]}"} ${LSAN_EXCLUDES[@]+"${LSAN_EXCLUDES[@]}"} ${HWASAN_EXCLUDES[@]+"${HWASAN_EXCLUDES[@]}"} ${HWASAN_UNINSTRUMENTED[@]+"${HWASAN_UNINSTRUMENTED[@]}"} ${FEATURE_LANES[@]+"${FEATURE_LANES[@]}"} ${LINUX_FEATURE_LANES[@]+"${LINUX_FEATURE_LANES[@]}"} ${NO_TESTS_ON[@]+"${NO_TESTS_ON[@]}"} ${MSAN_EXCLUDES[@]+"${MSAN_EXCLUDES[@]}"}'
  hx "a stale *_EXCLUDES entry is RED" 1 "'no-such-crate' is listed but is not a workspace member" \
    'ASAN_EXCLUDES=("no-such-crate|fixture"); check_members "G-ASAN exclusion" "${ASAN_EXCLUDES[@]}"'
  hx "control: the real TSAN_SCOPE names workspace members" 0 "" \
    'check_members "G-TSAN scope" "${TSAN_SCOPE[@]}"'
  hx "a stale TSAN_SCOPE entry is RED" 1 "'rustty-gone' is listed but is not a workspace member" \
    'TSAN_SCOPE+=("rustty-gone|fixture"); check_members "G-TSAN scope" "${TSAN_SCOPE[@]}"'
  hx "control: every TSAN_SERIAL spec is a TSAN_SCOPE spec" 0 "tests one at a time on Linux: rustty-xev" 'tsan_serial_check'
  hx "a TSAN_SERIAL spec outside TSAN_SCOPE is stale (RED)" 1 "TSAN_SERIAL names 'rustty-gone'" \
    'TSAN_SERIAL+=("rustty-gone|fixture"); tsan_serial_check'
  hx "spec_args: --test-threads=1 for a TSAN_SERIAL spec under tsan on Linux only" 0 "" \
    'OS=Linux; [ "$(spec_args tsan rustty-xev)" = " --test-threads=1" ] && [ -z "$(spec_args asan rustty-xev)" ] && [ -z "$(spec_args tsan rustty-core)" ]'
  hx "spec_args: no --test-threads=1 for a TSAN_SERIAL spec on macOS" 0 "" \
    'OS=Darwin; [ -z "$(spec_args tsan rustty-xev)" ]'
  hx "a stale feature-lane entry is RED" 1 "'rustty-renamed' is listed but is not a workspace member" \
    'FEATURE_LANES=("rustty-renamed:wuffs|fixture"); check_members "G-ASAN feature lane" "${FEATURE_LANES[@]}"'
  hx "control: the real MSAN_SCOPE/MSAN_EXCLUDES/MSAN_SKIPS partition is complete" 0 "" 'msan_partition_check'
  hx "a member in neither MSAN_SCOPE nor MSAN_EXCLUDES is RED" 1 "workspace member 'rustty-zf' is in neither" \
    'n=(); for c in "${MSAN_SCOPE[@]}"; do [ "$c" = rustty-zf ] || n+=("$c"); done; MSAN_SCOPE=("${n[@]}"); msan_partition_check'
  hx "an MSAN_SKIPS entry for a crate out of scope is stale (RED)" 1 "MSAN_SKIPS names 'xtask'" \
    'MSAN_SKIPS+=("xtask|x|fixture"); msan_partition_check'
  hx "a BUILD row makes gate_verdict BLOCKED (exit 3)" 3 "G-X: BLOCKED" \
    'M_RED=0; M_BLOCKED=1; M_TOTAL=5; gate_verdict G-X 0'
  hx "control: a clean matrix makes gate_verdict GREEN" 0 "G-X: GREEN" \
    'M_RED=0; M_BLOCKED=0; M_TOTAL=5; gate_verdict G-X 0'
  hx "RED dominates BLOCKED" 1 "G-X: RED" \
    'M_RED=1; M_BLOCKED=1; M_TOTAL=5; gate_verdict G-X 0'
  printf 'opengl::gl::glad::tests::loads: test\nopengl::tests::draws: test\ncell::tests::w: test\n\n3 tests, 0 benchmarks\n' >"$st/fixture.list"
  hx "control: every MSAN_SKIPS filter matches a listed test" 0 "" \
    'MSAN_SKIPS=("x|opengl::tests::|r" "x|cell::|r"); [ -z "$(skips_unmatched x '"'$st/fixture.list'"')" ]'
  hx "an MSAN_SKIPS filter that matches no test is reported (RED in the matrix)" 1 "opengl::gone::" \
    'MSAN_SKIPS=("x|opengl::tests::|r" "x|opengl::gone::|r"); u="$(skips_unmatched x '"'$st/fixture.list'"')"; echo "$u"; [ -z "$u" ]'

  # --- arm I: G-HWASAN's C-instrumentation check over fixture archives (any
  # host: plain cc/ar; an object that calls __hwasan_init stands for an
  # instrumented one). The fixtures sit under <td>/trip/debug/build, the
  # instrumented target's build dir; <td>/debug/build is the host's (N539). ---
  local fx="$st/fx"
  mkdir -p "$fx/t/trip/debug/build/inst-sys-1/out" "$fx/t/trip/debug/build/plain-sys-1/out" "$fx/t/trip/debug/build/mixed-sys-1/out" "$fx/t/trip/debug/build/none-sys-1/out" "$fx/src" \
    "$fx/t/trip/debug/build/hostcopy-sys-1/out" "$fx/t/debug/build/hostcopy-sys-2/out" "$fx/t/trip/debug/build/tgtplain-sys-1/out" "$fx/t/debug/build/tgtplain-sys-2/out"
  printf 'void __hwasan_init(void);\nvoid f(void) { __hwasan_init(); }\n' >"$fx/src/i.c"
  printf 'int g(int x) { return x + 1; }\n' >"$fx/src/p.c"
  if cc -c "$fx/src/i.c" -o "$fx/src/i.o" 2>/dev/null && cc -c "$fx/src/p.c" -o "$fx/src/p.o" 2>/dev/null; then
    ar rcs "$fx/t/trip/debug/build/inst-sys-1/out/libinst.a" "$fx/src/i.o"
    ar rcs "$fx/t/trip/debug/build/plain-sys-1/out/libplain.a" "$fx/src/p.o"
    ar rcs "$fx/t/trip/debug/build/mixed-sys-1/out/liba.a" "$fx/src/i.o"
    ar rcs "$fx/t/trip/debug/build/mixed-sys-1/out/libb.a" "$fx/src/p.o"
    # N539: an instrumented target copy beside an uninstrumented HOST copy (a
    # build-dependency's, e.g. rustty-cli's zlib-sys), and the reverse.
    ar rcs "$fx/t/trip/debug/build/hostcopy-sys-1/out/libz.a" "$fx/src/i.o"
    ar rcs "$fx/t/debug/build/hostcopy-sys-2/out/libz.a" "$fx/src/p.o"
    ar rcs "$fx/t/trip/debug/build/tgtplain-sys-1/out/libz.a" "$fx/src/p.o"
    ar rcs "$fx/t/debug/build/tgtplain-sys-2/out/libz.a" "$fx/src/i.o"
    ic() {  # ic NAME WANT_RC MUST_GREP SNIPPET
      local o r
      o="$(bash -c '. "$0" --source-only; '"$4" "$GATES_DIR/sanitizers.sh" 2>&1)"; r=$?
      if [ "$r" -eq "$2" ] && { if [ -z "$3" ]; then [ -z "$o" ]; else printf '%s' "$o" | grep -q -- "$3"; fi; }; then echo "  ok   I $1"
      else echo "  FAIL I $1 (rc=$r): $o"; fail=1; fi
    }
    ic "control: a -sys crate whose archive is instrumented passes" 0 "all instrumented" "hwasan_archive_check inst-sys '$fx/t' trip"
    ic "an uninstrumented archive is RED (the cc wrapper was bypassed)" 1 "have no __hwasan_ reference" "hwasan_archive_check plain-sys '$fx/t' trip"
    ic "one uninstrumented archive among instrumented ones is RED" 1 "1 of 2 C archive(s)" "hwasan_archive_check mixed-sys '$fx/t' trip"
    ic "a -sys crate that built no archive is RED" 1 "produced no lib\\*.a" "hwasan_archive_check none-sys '$fx/t' trip"
    ic "a listed exception with no instrumented archive is a named skip" 0 "not checked for instrumentation — fixture" \
      "HWASAN_UNINSTRUMENTED=('plain-sys|fixture'); hwasan_archive_check plain-sys '$fx/t' trip"
    ic "a listed exception whose archive IS instrumented is stale (RED)" 1 "the entry is stale" \
      "HWASAN_UNINSTRUMENTED=('inst-sys|fixture'); hwasan_archive_check inst-sys '$fx/t' trip"
    ic "a crate that is not -sys is not checked (no output)" 0 "" "hwasan_archive_check rustty-x '$fx/t' trip"
    ic "N539: a host build-dependency's uninstrumented copy is not looked at" 0 "hostcopy-sys: 1 C archive(s), all instrumented" "hwasan_archive_check hostcopy-sys '$fx/t' trip"
    ic "N539: an uninstrumented TARGET archive is RED although the host copy is instrumented" 1 "1 of 1 C archive(s) have no __hwasan_ reference (the cc wrapper was bypassed): trip/debug/build/tgtplain-sys-1/out/libz.a" "hwasan_archive_check tgtplain-sys '$fx/t' trip"
    ic "nothing built for the instrumented triple is RED" 1 "produced no lib\\*.a for other-trip" "hwasan_archive_check inst-sys '$fx/t' other-trip"
    # G-TSAN's check (tsan_archive_check): every archive built for the
    # instrumented target (<td>/<triple>/debug/build), whatever the package.
    mkdir -p "$fx/tt/trip/debug/build/inst-sys-1/out" "$fx/tt/trip/debug/build/plain-sys-1/out" "$fx/tt/debug/build/host-sys-1/out" "$fx/tu/trip/debug/build/inst-sys-1/out"
    printf 'void __tsan_init(void);\nvoid f(void) { __tsan_init(); }\n' >"$fx/src/t.c"
    if cc -c "$fx/src/t.c" -o "$fx/src/t.o" 2>/dev/null; then
      ar rcs "$fx/tt/trip/debug/build/inst-sys-1/out/libinst.a" "$fx/src/t.o"
      ar rcs "$fx/tu/trip/debug/build/inst-sys-1/out/libinst.a" "$fx/src/t.o"
      ar rcs "$fx/tt/trip/debug/build/plain-sys-1/out/libplain.a" "$fx/src/p.o"
      ar rcs "$fx/tt/debug/build/host-sys-1/out/libhost.a" "$fx/src/p.o"
      ic "tsan control: instrumented target archives pass (a host build-dependency's is not looked at)" 0 "1 built for the instrumented target, 1 instrumented" "tsan_archive_check '$fx/tu' trip"
      ic "tsan: an uninstrumented target archive is RED" 1 "plain-sys: trip/debug/build/plain-sys-1/out/libplain.a has no __tsan_ reference" "tsan_archive_check '$fx/tt' trip"
      ic "tsan: a listed exception is a named skip" 0 "libplain.a not checked for instrumentation — fixture" \
        "TSAN_UNINSTRUMENTED=('plain-sys|fixture'); tsan_archive_check '$fx/tt' trip"
      ic "tsan: a listed exception whose archive IS instrumented is stale (RED)" 1 "the entry is stale" \
        "TSAN_UNINSTRUMENTED=('inst-sys|fixture'); tsan_archive_check '$fx/tu' trip"
      # Non-vacuity: zero archives where C was declared, or nothing built for
      # the triple, is RED; zero archives with no C declared passes.
      mkdir -p "$fx/tz/trip/debug/build/c-sys-1/out" "$fx/tn/trip/debug/build/pure-1/out"
      printf 'cargo:rustc-link-search=native=%s\ncargo:rustc-link-lib=static=c\n' "$fx/tz/trip/debug/build/c-sys-1/out" >"$fx/tz/trip/debug/build/c-sys-1/output"
      printf 'cargo:rerun-if-changed=build.rs\n' >"$fx/tn/trip/debug/build/pure-1/output"
      ic "tsan: zero archives although a build script declared a static C library is RED" 1 "1 build script(s) declared a static C library" "tsan_archive_check '$fx/tz' trip"
      ic "tsan: nothing built for the instrumented triple is RED" 1 "nothing was built for the instrumented target" "tsan_archive_check '$fx/tz' other-trip"
      ic "tsan control: zero archives and no C declared passes" 0 "0 built for the instrumented target" "tsan_archive_check '$fx/tn' trip"
    else
      echo "  FAIL I: cc could not compile the tsan fixture object"; fail=1
    fi
  else
    echo "  FAIL I: cc could not compile the fixture objects"; fail=1
  fi

  # --- arm F: the BLOCKED arms (no build) ---
  out="$(RUSTTY_SANITIZERS_OWN_JOB="the fixture job" bash "$GATES_DIR/sanitizers.sh" --gate asan 2>&1)"; rc=$?
  if [ $rc -eq 3 ] && printf '%s' "$out" | grep -q 'defers to the fixture job'; then echo "  ok   F RUSTTY_SANITIZERS_OWN_JOB is BLOCKED (exit 3), naming the job"
  else echo "  FAIL F OWN_JOB (rc=$rc): $out"; fail=1; fi
  if [ "$OS" != "Linux" ]; then
    out="$(env -u RUSTTY_SANITIZERS_OWN_JOB bash "$GATES_DIR/sanitizers.sh" --gate msan 2>&1)"; rc=$?
    if [ $rc -eq 3 ] && printf '%s' "$out" | grep -q 'exists on Linux only'; then echo "  ok   F --gate msan on $OS is BLOCKED (exit 3), Linux-only"
    else echo "  FAIL F --gate msan on $OS (rc=$rc): $out"; fail=1; fi
  fi
  if [ "$OS" != "Linux" ] || [ "$ARCH" != "aarch64" ]; then
    # Even with RUSTTY_SANITIZERS_OWN_JOB set, the host reason comes first: no
    # job of ci.yml runs G-HWASAN, so deferring to one would be a false claim.
    out="$(RUSTTY_SANITIZERS_OWN_JOB="the fixture job" bash "$GATES_DIR/sanitizers.sh" --gate hwasan 2>&1)"; rc=$?
    if [ $rc -eq 3 ] && printf '%s' "$out" | grep -q 'exists on aarch64 Linux only'; then echo "  ok   F --gate hwasan on $OS/$ARCH is BLOCKED (exit 3), aarch64-Linux-only"
    else echo "  FAIL F --gate hwasan on $OS/$ARCH (rc=$rc): $out"; fail=1; fi
  fi
  # G-LSAN runs on Linux and macOS; any other host is BLOCKED (unverified there).
  hx "--gate lsan on a host other than Linux/macOS is BLOCKED (exit 3)" 3 "verified on Linux and macOS only, not on FreeBSD" \
    'unset RUSTTY_SANITIZERS_OWN_JOB; OS=FreeBSD; gate_asan_like lsan'
  hx "--gate tsan on a host other than Linux/macOS is BLOCKED (exit 3)" 3 "verified on macOS and Linux only, not on FreeBSD" \
    'unset RUSTTY_SANITIZERS_OWN_JOB; OS=FreeBSD; gate_tsan'
  hx "--gate tsan under RUSTTY_SANITIZERS_OWN_JOB is BLOCKED (exit 3), naming the job" 3 "defers to the fixture job" \
    'RUSTTY_SANITIZERS_OWN_JOB="the fixture job"; gate_tsan'
  hx "--gate tsan with its suppressions file missing is RED" 1 "tsan-missing.supp is missing" \
    'unset RUSTTY_SANITIZERS_OWN_JOB; TSAN_SUPP=/nonexistent/tsan-missing.supp; gate_tsan'
  # G-LSAN-GTK (P7-W24): Linux only; no hosted job, so OWN_JOB is BLOCKED
  # naming V33, never a deferral; no sway or no XDG_RUNTIME_DIR is BLOCKED; a
  # missing suppressions file is RED. Each checked before anything builds.
  hx "--gate lsan-gtk on a host other than Linux is BLOCKED (exit 3)" 3 "the GTK runtime is Linux's" \
    'unset RUSTTY_SANITIZERS_OWN_JOB; OS=Darwin; gate_lsan_gtk'
  hx "--gate lsan-gtk under RUSTTY_SANITIZERS_OWN_JOB is BLOCKED (exit 3), naming V33, not the job" 3 "has no hosted CI job.*V33" \
    'OS=Linux; RUSTTY_SANITIZERS_OWN_JOB="the fixture job"; gate_lsan_gtk'
  hx "--gate lsan-gtk without sway is BLOCKED (exit 3)" 3 "sway is not installed" \
    'unset RUSTTY_SANITIZERS_OWN_JOB; OS=Linux; SWAY_BIN=no-such-sway-fixture; gate_lsan_gtk'
  hx "--gate lsan-gtk without XDG_RUNTIME_DIR is BLOCKED (exit 3)" 3 "XDG_RUNTIME_DIR" \
    'unset RUSTTY_SANITIZERS_OWN_JOB XDG_RUNTIME_DIR; OS=Linux; SWAY_BIN=true; gate_lsan_gtk'
  # LSAN_GTK_CEILINGS (P7-W25): the real ceilings pin exactly the real
  # entries; a fixture log within its ceilings is GREEN, one above a ceiling,
  # an unpinned entry and a stale ceiling are RED.
  hx "control: the real LSAN_GTK_CEILINGS pin every lsan-gtk.supp entry" 0 "allocations matched" \
    'lsan_gtk_ceiling_check /dev/null'
  hx "lsan_gtk_ceiling_check: matches within their ceilings are GREEN" 0 "a.so': 5 allocations matched (ceiling 5)" \
    'd="$(mktemp -d)"; printf "leak:a.so\nleak:b.so\n" >"$d/s"; printf "Suppressions used:\n  count      bytes template\n      5        100 a.so\n      2         10 b.so\n-----\n" >"$d/l"; LSAN_GTK_CEILINGS=("a.so|5" "b.so|3"); lsan_gtk_ceiling_check "$d/l" "$d/s"; r=$?; rm -rf -- "$d"; exit $r'
  hx "lsan_gtk_ceiling_check: a match count above its ceiling is RED" 1 "matched 5 allocations, above its ceiling 4" \
    'd="$(mktemp -d)"; printf "leak:a.so\n" >"$d/s"; printf "Suppressions used:\n  count      bytes template\n      5        100 a.so\n-----\n" >"$d/l"; LSAN_GTK_CEILINGS=("a.so|4"); lsan_gtk_ceiling_check "$d/l" "$d/s"; r=$?; rm -rf -- "$d"; exit $r'
  hx "lsan_gtk_ceiling_check: an entry with no ceiling is RED" 1 "'b.so' has no LSAN_GTK_CEILINGS row" \
    'd="$(mktemp -d)"; printf "leak:a.so\nleak:b.so\n" >"$d/s"; : >"$d/l"; LSAN_GTK_CEILINGS=("a.so|4"); lsan_gtk_ceiling_check "$d/l" "$d/s"; r=$?; rm -rf -- "$d"; exit $r'
  hx "lsan_gtk_ceiling_check: a ceiling with no entry is stale (RED)" 1 "names 'gone.so', which is not a lsan-gtk.supp entry" \
    'd="$(mktemp -d)"; printf "leak:a.so\n" >"$d/s"; : >"$d/l"; LSAN_GTK_CEILINGS=("a.so|4" "gone.so|1"); lsan_gtk_ceiling_check "$d/l" "$d/s"; r=$?; rm -rf -- "$d"; exit $r'
  hx "lsan_gtk_ceiling_check: the test ran but the log has no Suppressions used table (RED)" 1 "has no LSan \"Suppressions used\" table" \
    'd="$(mktemp -d)"; printf "leak:a.so\n" >"$d/s"; printf "running 1 test\n" >"$d/l"; LSAN_GTK_MUST_MATCH=a.so; LSAN_GTK_CEILINGS=("a.so|4"); lsan_gtk_ceiling_check "$d/l" "$d/s" 1; r=$?; rm -rf -- "$d"; exit $r'
  hx "lsan_gtk_ceiling_check: the test ran but the always-matching entry matched nothing (RED)" 1 "'a.so' matched nothing" \
    'd="$(mktemp -d)"; printf "leak:a.so\nleak:b.so\n" >"$d/s"; printf "Suppressions used:\n  count      bytes template\n      2         10 b.so\n-----\n" >"$d/l"; LSAN_GTK_MUST_MATCH=a.so; LSAN_GTK_CEILINGS=("a.so|4" "b.so|3"); lsan_gtk_ceiling_check "$d/l" "$d/s" 1; r=$?; rm -rf -- "$d"; exit $r'
  hx "control: the real LSAN_GTK_MUST_MATCH is a lsan-gtk.supp entry" 0 "" \
    'sed -n "s/^leak://p" "$LSAN_GTK_SUPP" | grep -qxF -- "$LSAN_GTK_MUST_MATCH"'
  hx "--gate lsan-gtk with lsan-gtk.supp missing is RED" 1 "lsan-gtk-missing.supp is missing" \
    'unset RUSTTY_SANITIZERS_OWN_JOB; OS=Linux; XDG_RUNTIME_DIR=/tmp; SWAY_BIN=true; LSAN_GTK_SUPP=/nonexistent/lsan-gtk-missing.supp; gate_lsan_gtk'

  # --- arms A/B: the synthetic crate under the real sanitizers ---
  local cr="$st/crate"
  mkdir -p "$cr/src"
  cat >"$cr/Cargo.toml" <<'TOML'
[package]
name = "san-selftest"
version = "0.0.0"
edition = "2021"
publish = false

[features]
hook = []
glib = []

[workspace]
TOML
  cat >"$cr/src/lib.rs" <<'RS'
//! G-SAN-SELF's synthetic crate: one deliberate defect per sanitizer, plus a
//! clean control. Each arm selects its test by exact name.
#[cfg(test)]
mod tests {
    #[inline(never)]
    fn oob_read(v: &[u8]) -> u8 {
        // 8 bytes past a 16-byte heap allocation: inside ASan's redzone, so
        // ASan stops at the first read. HWASan compares tags per 16-byte
        // granule, and one neighbouring granule shares the block's tag with
        // probability 1/256 (G-SAN-SELF's A4 went GREEN once on rvm, P7-W24,
        // N513), so the read walks eight granules, as the C arm does: all
        // eight match with probability 256^-8.
        let mut s = 0u8;
        for k in 0..8 {
            s = s.wrapping_add(unsafe { v.as_ptr().add(v.len() + 8 + 16 * k).read_volatile() });
        }
        s
    }
    #[test]
    fn defect_heap_overflow() {
        let v: Vec<u8> = (0u8..16).collect();
        std::hint::black_box(oob_read(std::hint::black_box(&v)));
    }

    #[inline(never)]
    fn leak_some() {
        // Many unreachable blocks, so no stale register or stack slot can keep
        // them all reachable when LeakSanitizer scans at exit.
        for i in 0..64u8 {
            let b = Box::new([i; 256]);
            std::mem::forget(std::hint::black_box(b));
        }
    }
    #[test]
    fn defect_leak() {
        leak_some();
    }

    #[inline(never)]
    fn uninit_byte() -> u8 {
        let mut v: Vec<u8> = Vec::with_capacity(16);
        // SAFETY: deliberately NOT safe — reads uninitialized heap memory so
        // MemorySanitizer reports it. This crate exists only for the self-test.
        unsafe {
            v.set_len(16);
        }
        let b = v[std::hint::black_box(3)];
        std::hint::black_box(&v);
        b
    }
    #[test]
    fn defect_uninit_read() {
        if uninit_byte() == 7 {
            println!("seven");
        }
    }

    // A wild index from one heap block straight into the INTERIOR of another
    // live block: no redzone is touched, so ASan cannot see it; HWASan does
    // (the two blocks' tags differ, except with probability 1/256 per run, so
    // the arm retags by allocating a fresh pair until the tags differ).
    #[test]
    fn defect_wild_index_into_live_block() {
        let m = 0x00ff_ffff_ffff_ffffusize;
        let mut tries = 0;
        let (a, b) = loop {
            let a = Box::new([1u8; 64]);
            let b = Box::new([2u8; 64]);
            let (ta, tb) = (&*a as *const _ as usize >> 56, &*b as *const _ as usize >> 56);
            tries += 1;
            // Untagged builds (tags always 0) stop after the first pair.
            if ta != tb || tries == 64 {
                break (a, b);
            }
            std::mem::forget((a, b));
        };
        let pa = std::hint::black_box(&*a as *const [u8; 64] as *const u8);
        let pb = &*b as *const [u8; 64] as *const u8;
        let d = ((pb as usize & m) as isize) - ((pa as usize & m) as isize) + 16;
        // SAFETY: deliberately NOT safe; this crate exists only for the self-test.
        let v = unsafe { pa.wrapping_offset(std::hint::black_box(d)).read_volatile() };
        assert_eq!(v, 2);
        std::hint::black_box((&a, &b));
    }

    // G-TSAN's negative control: two threads write one static with no
    // happens-before edge between them (a sleep orders them in time, but is
    // not synchronization), so TSan reports it however the threads interleave.
    static mut RACY: u64 = 0;
    #[inline(never)]
    fn racy_write(v: u64) {
        // SAFETY: deliberately NOT safe (a data race); self-test only.
        unsafe { (&raw mut RACY).write_volatile(v) }
    }
    #[test]
    fn defect_data_race() {
        let t = std::thread::spawn(|| racy_write(1));
        std::thread::sleep(std::time::Duration::from_millis(200));
        racy_write(2);
        t.join().unwrap();
    }
    // Its control: the same two writes ordered by a Mutex.
    #[test]
    fn synchronized_control() {
        let m = std::sync::Arc::new(std::sync::Mutex::new(0u64));
        let m2 = std::sync::Arc::clone(&m);
        let t = std::thread::spawn(move || *m2.lock().unwrap() = 1);
        std::thread::sleep(std::time::Duration::from_millis(200));
        *m.lock().unwrap() = 2;
        t.join().unwrap();
    }

    #[test]
    fn clean_control() {
        let v: Vec<u8> = (0u8..16).collect();
        assert_eq!(v.iter().map(|&b| b as u32).sum::<u32>(), 120);
    }

    // The hang arm: sleeps far past the arm's bound, so only the bound ends it.
    #[test]
    fn slow_hang() {
        std::thread::sleep(std::time::Duration::from_secs(900));
    }
}

/// LeakSanitizer's opt-out hook, only with the `hook` feature (arm "A2 opt-out
/// hook"): returning 1 turns the exit-time leak check off.
#[cfg(all(test, feature = "hook"))]
#[no_mangle]
extern "C" fn __lsan_is_turned_off() -> core::ffi::c_int {
    1
}

/// Linux negative controls for G-LSAN-GTK's lsan-gtk.supp (P7-W24), with the
/// `glib` feature: objects Rustty could leak through GLib and GObject (a
/// g_strdup'd string, a GObject never unreffed) must stay RED under lsan.supp +
/// lsan-gtk.supp; the allocate-and-free control must be GREEN.
#[cfg(all(test, target_os = "linux", feature = "glib"))]
mod glib_leaks {
    use core::ffi::{c_char, c_void};
    #[link(name = "glib-2.0")]
    #[link(name = "gobject-2.0")]
    #[link(name = "fontconfig")]
    extern "C" {
        fn g_strdup(s: *const c_char) -> *mut c_char;
        fn g_free(p: *mut c_void);
        fn g_object_new(t: usize, first_property: *const c_char, ...) -> *mut c_void;
        fn g_object_unref(o: *mut c_void);
        fn g_closure_new_simple(sizeof_closure: u32, data: *mut c_void) -> *mut c_void;
        fn g_closure_ref(c: *mut c_void) -> *mut c_void;
        fn g_closure_sink(c: *mut c_void);
        fn FcNameParse(name: *const u8) -> *mut c_void;
        fn FcPatternDestroy(p: *mut c_void);
    }
    // G_TYPE_OBJECT = G_TYPE_MAKE_FUNDAMENTAL(20) = 20 << 2 (gtype.h).
    const G_TYPE_OBJECT: usize = 20 << 2;
    #[inline(never)]
    fn sink(p: *mut c_void) {
        std::hint::black_box(p);
    }
    #[test]
    fn glib_clean_control() {
        for _ in 0..4 {
            unsafe {
                g_free(g_strdup(c"rustty".as_ptr()).cast());
                g_object_unref(g_object_new(G_TYPE_OBJECT, core::ptr::null()));
            }
        }
    }
    #[test]
    fn leak_gstrdup() {
        for _ in 0..16 {
            sink(unsafe { g_strdup(c"rustty-leaked-string".as_ptr()) }.cast());
        }
    }
    #[test]
    fn leak_gobject() {
        for _ in 0..8 {
            sink(unsafe { g_object_new(G_TYPE_OBJECT, core::ptr::null()) });
        }
    }
    // lsan-gtk.supp G1's control: a fontconfig pattern Rust code leaks has no
    // libpangoft2 frame, so it stays RED.
    #[test]
    fn leak_fcpattern() {
        unsafe { FcPatternDestroy(FcNameParse(c"Monospace:size=11".as_ptr().cast())) };
        for _ in 0..8 {
            sink(unsafe { FcNameParse(c"Monospace:size=12:weight=200".as_ptr().cast()) });
        }
    }
    // N516's shape: a sunk GClosure with a reference nobody drops (what
    // gtk4-rs's BuilderRustScope left behind for every template callback,
    // until P7-W25 bound them through GTK's C scope and removed lsan-gtk.supp
    // G2) stays RED under the lane's suppressions.
    #[test]
    fn leak_gclosure() {
        for _ in 0..8 {
            let c = unsafe { g_closure_new_simple(64, core::ptr::null_mut()) };
            unsafe {
                g_closure_ref(c);
                g_closure_sink(c);
            }
            sink(c);
        }
    }
}

/// Darwin negative controls for lsan-darwin.supp (fix round 1): each `leak_*`
/// test leaks one object Rustty could leak through the system APIs whose
/// internals the file suppresses, and must stay RED under the real file; the
/// create-and-release control must be GREEN (the suppressions cover exactly
/// the system's own state).
#[cfg(all(test, target_os = "macos"))]
mod darwin {
    use core::ffi::{c_char, c_void};
    #[link(name = "CoreFoundation", kind = "framework")]
    #[link(name = "CoreText", kind = "framework")]
    #[link(name = "CoreGraphics", kind = "framework")]
    #[link(name = "Metal", kind = "framework")]
    extern "C" {
        fn CFStringCreateWithCString(a: *const c_void, s: *const c_char, enc: u32) -> *const c_void;
        fn CFRelease(p: *const c_void);
        fn CTFontCreateWithName(name: *const c_void, size: f64, m: *const c_void) -> *const c_void;
        fn CTFontDescriptorCreateWithNameAndSize(name: *const c_void, size: f64) -> *const c_void;
        fn CTFontCopyFontDescriptor(font: *const c_void) -> *const c_void;
        fn CGFontCreateWithFontName(name: *const c_void) -> *const c_void;
        fn MTLCopyAllDevices() -> *const c_void;
    }
    fn menlo() -> *const c_void {
        // kCFStringEncodingUTF8
        unsafe { CFStringCreateWithCString(core::ptr::null(), c"Menlo-Regular".as_ptr(), 0x0800_0100) }
    }
    #[inline(never)]
    fn sink(p: *const c_void) {
        std::hint::black_box(p);
    }
    #[test]
    fn create_release_control() {
        let n = menlo();
        let f = unsafe { CTFontCreateWithName(n, 12.0, core::ptr::null()) };
        let d = unsafe { CTFontCopyFontDescriptor(f) };
        unsafe {
            CFRelease(d);
            CFRelease(f);
            CFRelease(n);
        }
    }
    #[test]
    fn leak_ctfont() {
        let n = menlo();
        for i in 0..4 {
            sink(unsafe { CTFontCreateWithName(n, 13.0 + i as f64, core::ptr::null()) });
        }
        unsafe { CFRelease(n) };
    }
    #[test]
    fn leak_font_descriptor_created() {
        let n = menlo();
        for i in 0..4 {
            sink(unsafe { CTFontDescriptorCreateWithNameAndSize(n, 13.0 + i as f64) });
        }
        unsafe { CFRelease(n) };
    }
    #[test]
    fn leak_font_descriptor_copied() {
        let n = menlo();
        for i in 0..4 {
            let f = unsafe { CTFontCreateWithName(n, 17.0 + i as f64, core::ptr::null()) };
            sink(unsafe { CTFontCopyFontDescriptor(f) });
            unsafe { CFRelease(f) };
        }
        unsafe { CFRelease(n) };
    }
    extern "C" {
        fn CTFontCopyCharacterSet(font: *const c_void) -> *const c_void;
        fn CFCharacterSetCreateMutableCopy(a: *const c_void, s: *const c_void) -> *const c_void;
    }
    #[test]
    fn leak_charset_derived() {
        let n = menlo();
        let f = unsafe { CTFontCreateWithName(n, 25.0, core::ptr::null()) };
        let cs = unsafe { CTFontCopyCharacterSet(f) };
        for _ in 0..4 {
            sink(unsafe { CFCharacterSetCreateMutableCopy(core::ptr::null(), cs) });
        }
        unsafe {
            CFRelease(cs);
            CFRelease(f);
            CFRelease(n);
        }
    }
    #[test]
    fn leak_cgfont() {
        let n = menlo();
        for _ in 0..4 {
            sink(unsafe { CGFontCreateWithFontName(n) });
        }
        unsafe { CFRelease(n) };
    }
    #[test]
    fn leak_mtl_device_array() {
        for _ in 0..4 {
            sink(unsafe { MTLCopyAllDevices() });
        }
    }
}
RS
  local sroot="$st/target"
  arm() {  # arm NAME SAN FILTER WANT(RED|GREEN) MUST_GREP [SPEC]
    local name="$1" san="$2" filter="$3" want="$4" must="$5" spec="${6:-${ARM_SPEC:-san-selftest}}" o r
    o="$(ARM_LSAN_SUPP="${ARM_LSAN_SUPP:-}" SAN_CRATE_TIMEOUT_S="${ARM_TIMEOUT:-}" SAN_PER_CRATE_TARGET="${ARM_PER_CRATE:-0}" SAN_MANIFEST_PATH="${ARM_MANIFEST:-$cr/Cargo.toml}" SAN_TARGET_ROOT="$sroot" SAN_LOG_ROOT="$st/slogs-$name" SAN_MIN_FREE_GB=0 \
         M_RED=0 M_BLOCKED=0 M_TOTAL=0 M_ROWS="" bash -c '. "$0" --source-only; [ -z "${ARM_LSAN_SUPP:-}" ] || LSAN_SUPP="$ARM_LSAN_SUPP"; matrix_run "$1" "$2" "$3"; matrix_summary "$1"; gate_verdict SELF 0' \
         "$GATES_DIR/sanitizers.sh" "$san" "$filter" "$spec" 2>&1)"; r=$?
    local okv=0
    case "$want" in
      RED) [ $r -eq 1 ] && printf '%s' "$o" | grep -qE "$must" && okv=1 ;;
      GREEN) [ $r -eq 0 ] && printf '%s' "$o" | grep -qE "$must" && okv=1 ;;
    esac
    if [ $okv -eq 1 ]; then echo "  ok   $name: $san '$filter' -> $want ($(printf '%s' "$o" | grep -m1 -oE "$must"))"
    else echo "  FAIL $name: $san '$filter' wanted $want matching /$must/, got rc=$r:"; printf '%s\n' "$o" | sed 's/^/       /' | tail -15; fail=1; fi
  }
  arm "A1 heap overflow" asan "tests::defect_heap_overflow --exact" RED "heap-buffer-overflow"
  arm "A0 clean control" asan "tests::clean_control --exact" GREEN "1 test\\(s\\) ran"
  if [ -d "$sroot/san-asan" ]; then echo "  FAIL A0: the shared target dir $sroot/san-asan outlived its pass"; fail=1
  else echo "  ok   A0 the shared target dir is deleted when the pass ends"; fi
  ARM_PER_CRATE=1 arm "A0 per-crate target dirs (--per-crate-target)" asan "tests::defect_heap_overflow --exact" RED "per-crate target dirs"
  if [ -d "$sroot/san-asan-san-selftest" ]; then echo "  FAIL A0: the per-crate target dir outlived its crate"; fail=1; fi
  arm "B  empty filter" asan "no_such_test_anywhere" RED "VACUOUS"
  # The hang bound: a test that never finishes is RED (HANG), the pass goes
  # on, and nothing of the killed tree is left running. 60 s covers the
  # synthetic crate's instrumented build on a loaded host; the test sleeps 900.
  ARM_TIMEOUT=60 arm "B  hang past the bound" asan "tests::slow_hang --exact" RED "HANG after 60s \\(no verdict\\)"
  if pgrep -f 'tests::slow_hang' >/dev/null 2>&1; then
    echo "  FAIL B  the killed test is still running: $(pgrep -f 'tests::slow_hang' | tr '\n' ' ')"; fail=1
  else echo "  ok   B  the hung test's process tree is gone after the kill"; fi
  # BUILD: a crate that does not compile is BLOCKED (exit 3), never GREEN or RED.
  local bc="$st/badcrate"
  mkdir -p "$bc/src"
  printf '[package]\nname = "san-selftest-bad"\nversion = "0.0.0"\nedition = "2021"\npublish = false\n\n[workspace]\n' >"$bc/Cargo.toml"
  printf '#[test]\nfn t() { let x: u8 = "not a number"; }\n' >"$bc/src/lib.rs"
  o="$(SAN_PER_CRATE_TARGET=0 SAN_MANIFEST_PATH="$bc/Cargo.toml" SAN_TARGET_ROOT="$sroot" SAN_LOG_ROOT="$st/slogs-bad" SAN_MIN_FREE_GB=0 \
       bash -c '. "$0" --source-only; matrix_run asan "" san-selftest-bad; gate_verdict SELF 0' "$GATES_DIR/sanitizers.sh" 2>&1)"; r=$?
  if [ $r -eq 3 ] && printf '%s' "$o" | grep -q 'BLOCKED — did not build'; then echo "  ok   H a crate that does not build is BLOCKED (exit 3)"
  else echo "  FAIL H build failure (rc=$r):"; printf '%s\n' "$o" | tail -5; fail=1; fi
  # The disk floor: BLOCKED before building anything.
  o="$(SAN_PER_CRATE_TARGET=0 SAN_MANIFEST_PATH="$cr/Cargo.toml" SAN_TARGET_ROOT="$sroot" SAN_LOG_ROOT="$st/slogs-disk" SAN_MIN_FREE_GB=999999 \
       bash -c '. "$0" --source-only; matrix_run asan "" san-selftest; gate_verdict SELF 0' "$GATES_DIR/sanitizers.sh" 2>&1)"; r=$?
  if [ $r -eq 3 ] && printf '%s' "$o" | grep -q 'below the SAN_MIN_FREE_GB=999999 floor'; then echo "  ok   H the disk floor stops the pass BLOCKED (exit 3)"
  else echo "  FAIL H disk floor (rc=$r):"; printf '%s\n' "$o" | tail -5; fail=1; fi
  # SAN_ONLY: a narrowed gate is never GREEN, even when what it ran is clean.
  o="$(SAN_PER_CRATE_TARGET=0 SAN_TARGET_ROOT="$sroot" SAN_LOG_ROOT="$st/slogs-only" SAN_ONLY=rustty-identity SAN_MIN_FREE_GB=0 \
       bash "$GATES_DIR/sanitizers.sh" --gate asan 2>&1)"; r=$?
  if [ $r -eq 3 ] && printf '%s' "$o" | grep -q 'asan rustty-identity: 1 test(s) ran' && printf '%s' "$o" | grep -q 'G-ASAN: BLOCKED'; then
    echo "  ok   H SAN_ONLY=rustty-identity: its 1 test GREEN, the gate BLOCKED (exit 3), never GREEN"
  else echo "  FAIL H SAN_ONLY (rc=$r):"; printf '%s\n' "$o" | tail -6; fail=1; fi
  # NO_TESTS_ON: a listed crate with zero tests is a named skip, not RED; a
  # listed crate where a test DID run is a stale entry, RED.
  local nto nrc
  for nrc in "no_such_test_anywhere|0 tests on $OS by construction" "tests::clean_control --exact|the entry is stale"; do
    nto="$(SAN_MANIFEST_PATH="$cr/Cargo.toml" SAN_TARGET_ROOT="$sroot" SAN_LOG_ROOT="$st/slogs-nt" SAN_MIN_FREE_GB=0 \
         bash -c '. "$0" --source-only; NO_TESTS_ON=("san-selftest|$(uname -s)|fixture"); matrix_run asan "$1" san-selftest; echo "M_RED=$M_RED"' \
         "$GATES_DIR/sanitizers.sh" "${nrc%%|*}" 2>&1)"
    case "${nrc#*|}" in
      "the entry is stale") want="M_RED=1" ;;
      *) want="M_RED=0" ;;
    esac
    if printf '%s' "$nto" | grep -q "${nrc#*|}" && printf '%s' "$nto" | grep -q "$want"; then echo "  ok   G NO_TESTS_ON, filter '${nrc%%|*}': ${nrc#*|} ($want)"
    else echo "  FAIL G NO_TESTS_ON, filter '${nrc%%|*}':"; printf '%s\n' "$nto" | tail -5 | sed 's/^/       /'; fail=1; fi
  done
  if [ "$OS" = "Linux" ] || [ "$OS" = "Darwin" ]; then
    arm "A2 leak" lsan "tests::defect_leak --exact" RED "LeakSanitizer: detected memory leaks"
    arm "A2 control: clean under LSan" lsan "tests::clean_control --exact" GREEN "1 test\\(s\\) ran"
    arm "A2 control: the leak under G-ASAN (leak checker off)" asan "tests::defect_leak --exact" GREEN "1 test\\(s\\) ran"
    # The documented opt-out hook (__lsan_is_turned_off) that rustty-termio's
    # fork-child tests define must reach the runtime: with the `hook` feature
    # the synthetic crate's hook returns 1, so its deliberate leak is NOT
    # reported. On macOS that holds only because run_crate exports the
    # executable's symbols (-Zexport-executable-symbols); without the flag this
    # arm is RED (measured P7-W16a).
    arm "A2 opt-out hook reaches the runtime" "lsan" "tests::defect_leak --exact" GREEN "1 test\\(s\\) ran" "san-selftest:hook"
    if [ "$OS" = "Linux" ]; then
      # G-LSAN-GTK's suppressions (P7-W24): the lane's own combined file
      # (lsan.supp + lsan-gtk.supp, as gate_lsan_gtk builds it) must not hide a
      # Rust leak, nor a GLib string or a GObject that Rust code leaks; the
      # GLib allocate-and-free control must be GREEN.
      cat "$LSAN_SUPP" "$LSAN_GTK_SUPP" >"$st/lsan-gtk-combined.supp"
      ARM_LSAN_SUPP="$st/lsan-gtk-combined.supp" \
        arm "A2g Rust leak under lsan.supp + lsan-gtk.supp" lsan "tests::defect_leak --exact" RED "LeakSanitizer: detected memory leaks"
      ARM_LSAN_SUPP="$st/lsan-gtk-combined.supp" \
        arm "A2g control: GLib/GObject allocate and free, clean under lsan-gtk.supp" lsan "glib_leaks::glib_clean_control --exact" GREEN "1 test\\(s\\) ran" "san-selftest:glib"
      ARM_LSAN_SUPP="$st/lsan-gtk-combined.supp" \
        arm "A2g leaked g_strdup string" lsan "glib_leaks::leak_gstrdup --exact" RED "LeakSanitizer: detected memory leaks" "san-selftest:glib"
      ARM_LSAN_SUPP="$st/lsan-gtk-combined.supp" \
        arm "A2g leaked GObject" lsan "glib_leaks::leak_gobject --exact" RED "LeakSanitizer: detected memory leaks" "san-selftest:glib"
      ARM_LSAN_SUPP="$st/lsan-gtk-combined.supp" \
        arm "A2g leaked FcPattern (lsan-gtk.supp G1)" lsan "glib_leaks::leak_fcpattern --exact" RED "LeakSanitizer: detected memory leaks" "san-selftest:glib"
      ARM_LSAN_SUPP="$st/lsan-gtk-combined.supp" \
        arm "A2g leaked GClosure, sunk and referenced (the N516 shape; no entry may hide it)" lsan "glib_leaks::leak_gclosure --exact" RED "LeakSanitizer: detected memory leaks" "san-selftest:glib"
    fi
    if [ "$OS" = "Darwin" ]; then
      # lsan-darwin.supp's negative controls (fix round 1): the system state the
      # file suppresses must not hide an object Rustty leaks through those APIs.
      arm "A2d control: CoreText create and release, clean under lsan-darwin.supp" lsan "darwin::create_release_control --exact" GREEN "1 test\\(s\\) ran"
      arm "A2d leaked CTFont (S9, S10, S13)" lsan "darwin::leak_ctfont --exact" RED "LeakSanitizer: detected memory leaks"
      arm "A2d leaked CTFontDescriptor, created (S10)" lsan "darwin::leak_font_descriptor_created --exact" RED "LeakSanitizer: detected memory leaks"
      arm "A2d leaked CTFontDescriptor, copied from a font (S10)" lsan "darwin::leak_font_descriptor_copied --exact" RED "LeakSanitizer: detected memory leaks"
      arm "A2d leaked character set derived from a font's (S9)" lsan "darwin::leak_charset_derived --exact" RED "LeakSanitizer: detected memory leaks"
      arm "A2d leaked CGFont (S13)" lsan "darwin::leak_cgfont --exact" RED "LeakSanitizer: detected memory leaks"
      arm "A2d leaked MTLCopyAllDevices array (S17, S18)" lsan "darwin::leak_mtl_device_array --exact" RED "LeakSanitizer: detected memory leaks"
    fi
  else
    echo "  skip A2 leak under LSan: verified on Linux and macOS only, not on $OS"
  fi
  if [ "$OS" = "Linux" ]; then
    if [ -n "$MSAN_AARCH64_NOTE" ] && [ "$ARCH" = "aarch64" ]; then
      echo "  skip A3 uninit read under MSan: BLOCKED on $ARCH ($MSAN_AARCH64_NOTE)"
    else
      arm "A3 uninit read" msan "tests::defect_uninit_read --exact" RED "MemorySanitizer: use-of-uninitialized-value"
      arm "A3 control: clean under MSan" msan "tests::clean_control --exact" GREEN "1 test\\(s\\) ran"
    fi
  else
    echo "  skip A3 uninit read under MSan: Linux-only (compiler-rt has no MSan for $OS); runs on rvm / ci.yml"
  fi
  if [ "$OS" = "Linux" ] && [ "$ARCH" = "aarch64" ]; then
    arm "A4 heap overflow" hwasan "tests::defect_heap_overflow --exact" RED "HWAddressSanitizer: tag-mismatch"
    arm "A4 wild index into a live block (what ASan cannot see)" hwasan "tests::defect_wild_index_into_live_block --exact" RED "HWAddressSanitizer: tag-mismatch"
    arm "A4 control: clean under HWASan" hwasan "tests::clean_control --exact" GREEN "1 test\\(s\\) ran"
    # The C side: a -sys crate whose build script compiles C through $CC (the
    # cc wrapper, as the cc crate does). Its heap overflow is in C, which HWASan
    # sees only if that C is instrumented: RED here proves the wrapper path, and
    # the clean control also passes hwasan_archive_check on a real archive.
    local cc_cr="$st/ccrate"
    mkdir -p "$cc_cr/src"
    printf '[package]\nname = "san-selftest-c-sys"\nversion = "0.0.0"\nedition = "2021"\npublish = false\nbuild = "build.rs"\n\n[workspace]\n' >"$cc_cr/Cargo.toml"
    cat >"$cc_cr/src/ovf.c" <<'C'
#include <stdlib.h>
/* Reads the granules after a 16-byte block: each belongs to another chunk
   (tagged independently), so one of eight reads mismatches except with
   probability 256^-8. */
int ovf_read(void) {
  volatile unsigned char *p = malloc(16);
  int s = 0;
  for (int i = 1; i <= 8; i++) s += p[16 * i];
  free((void *)p);
  return s;
}
int ovf_clean(void) {
  unsigned char *p = malloc(16);
  int s = 0;
  for (int i = 0; i < 16; i++) p[i] = (unsigned char)i;
  for (int i = 0; i < 16; i++) s += p[i];
  free(p);
  return s;
}
C
    cat >"$cc_cr/build.rs" <<'RS'
use std::{env, path::PathBuf, process::Command};
fn main() {
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    let cc = env::var("CC").unwrap_or_else(|_| "cc".into());
    let obj = out.join("ovf.o");
    let ok = Command::new(&cc).args(["-c", "-O0", "-fPIC", "src/ovf.c", "-o"]).arg(&obj).status().unwrap();
    assert!(ok.success(), "{cc} failed");
    let ok = Command::new("ar").arg("rcs").arg(out.join("libovf.a")).arg(&obj).status().unwrap();
    assert!(ok.success(), "ar failed");
    println!("cargo:rustc-link-search=native={}", out.display());
    println!("cargo:rustc-link-lib=static=ovf");
    println!("cargo:rerun-if-changed=src/ovf.c");
}
RS
    cat >"$cc_cr/src/lib.rs" <<'RS'
//! G-SAN-SELF's C arm: a heap overflow inside C compiled through $CC.
extern "C" {
    fn ovf_read() -> i32;
    fn ovf_clean() -> i32;
}
#[cfg(test)]
mod tests {
    #[test]
    fn c_heap_overflow() {
        std::hint::black_box(unsafe { super::ovf_read() });
    }
    #[test]
    fn c_clean_control() {
        assert_eq!(unsafe { super::ovf_clean() }, 120);
    }
}
RS
    ARM_MANIFEST="$cc_cr/Cargo.toml" ARM_SPEC=san-selftest-c-sys \
      arm "A4 heap overflow in C (the cc wrapper instruments it)" hwasan "tests::c_heap_overflow --exact" RED "HWAddressSanitizer: tag-mismatch"
    ARM_MANIFEST="$cc_cr/Cargo.toml" ARM_SPEC=san-selftest-c-sys \
      arm "A4 control: clean C under HWASan, its archive instrumented" hwasan "tests::c_clean_control --exact" GREEN "all instrumented"
  else
    echo "  skip A4 heap overflow under HWASan: aarch64 Linux only (rustc rejects -Zsanitizer=hwaddress on $OS/$ARCH); runs on rvm"
  fi
  if [ "$OS" = "Linux" ] || [ "$OS" = "Darwin" ]; then
    # G-TSAN (P7-W17): the deliberate data race must be RED, the same writes
    # under a Mutex GREEN.
    arm "A5 data race" tsan "tests::defect_data_race --exact" RED "ThreadSanitizer: data race"
    arm "A5 control: Mutex-ordered writes, clean under TSan" tsan "tests::synchronized_control --exact" GREEN "1 test\\(s\\) ran"
    # TSAN_SERIAL (P7-W24, N514): tests run one at a time still have their own
    # threads checked against each other; the race stays RED.
    arm "A5s data race between one test's threads, tests run one at a time (TSAN_SERIAL)" tsan "tests::defect_data_race --exact --test-threads=1" RED "ThreadSanitizer: data race"
    # The C side: a race between two threads inside C compiled through $CC
    # (the tsan-cc wrapper). TSan sees it only if that C is instrumented, so
    # RED here proves the wrapper path; the clean control also passes
    # tsan_archive_check on the real archive.
    local tc="$st/tcrate"
    mkdir -p "$tc/src"
    printf '[package]\nname = "san-selftest-tsan-c-sys"\nversion = "0.0.0"\nedition = "2021"\npublish = false\nbuild = "build.rs"\n\n[workspace]\n' >"$tc/Cargo.toml"
    cat >"$tc/src/race.c" <<'C'
/* A plain global written from two threads with no synchronization. */
static volatile long racy;
void race_write(long v) { racy = v; }
long clean_sum(void) {
  long s = 0;
  for (long i = 0; i < 16; i++) s += i;
  return s;
}
C
    cat >"$tc/build.rs" <<'RS'
use std::{env, path::PathBuf, process::Command};
fn main() {
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    let cc = env::var("CC").unwrap_or_else(|_| "cc".into());
    let obj = out.join("race.o");
    let ok = Command::new(&cc).args(["-c", "-O0", "-fPIC", "src/race.c", "-o"]).arg(&obj).status().unwrap();
    assert!(ok.success(), "{cc} failed");
    let ok = Command::new("ar").arg("rcs").arg(out.join("librace.a")).arg(&obj).status().unwrap();
    assert!(ok.success(), "ar failed");
    println!("cargo:rustc-link-search=native={}", out.display());
    println!("cargo:rustc-link-lib=static=race");
    println!("cargo:rerun-if-changed=src/race.c");
}
RS
    cat >"$tc/src/lib.rs" <<'RS'
//! G-SAN-SELF's TSan C arm: a data race inside C compiled through $CC.
extern "C" {
    fn race_write(v: core::ffi::c_long);
    fn clean_sum() -> core::ffi::c_long;
}
#[cfg(test)]
mod tests {
    #[test]
    fn c_data_race() {
        let t = std::thread::spawn(|| unsafe { super::race_write(1) });
        std::thread::sleep(std::time::Duration::from_millis(200));
        unsafe { super::race_write(2) };
        t.join().unwrap();
    }
    #[test]
    fn c_clean_control() {
        assert_eq!(unsafe { super::clean_sum() }, 120);
    }
}
RS
    ARM_MANIFEST="$tc/Cargo.toml" ARM_SPEC=san-selftest-tsan-c-sys \
      arm "A5 data race in C (the cc wrapper instruments it)" tsan "tests::c_data_race --exact" RED "ThreadSanitizer: data race"
    ARM_MANIFEST="$tc/Cargo.toml" ARM_SPEC=san-selftest-tsan-c-sys \
      arm "A5 control: clean C under TSan, its archive instrumented" tsan "tests::c_clean_control --exact" GREEN "tsan C archives: 1 built for the instrumented target, 1 instrumented"
  else
    echo "  skip A5 data race under TSan: verified on Linux and macOS only, not on $OS"
  fi
  # The synthetic crate's own build dirs (san-<san>-san-selftest) are dropped by
  # matrix_run; remove the work dir itself.
  rm -rf -- "${st:?}"
  if [ $fail -eq 0 ]; then echo "G-SAN-SELF: GREEN"; return 0; fi
  echo "G-SAN-SELF: RED"; return 1
}

# ---------------------------------------------------------------------------
# Dispatch.
# ---------------------------------------------------------------------------
# shellcheck disable=SC2317  # `exit` is reached only when run, not sourced
if [ "${1:-}" = "--source-only" ]; then return 0 2>/dev/null || exit 0; fi
if [ "${1:-}" = "--per-crate-target" ]; then SAN_PER_CRATE_TARGET=1; shift; fi
case "${1:-}" in
  --self-test) self_test; exit $? ;;
  --crosscheck)
    # Re-run only the ASan-claim cross-check over an existing run's logs.
    case "${2:-}" in asan|lsan|hwasan) asan_claims_crosscheck "$2"; exit $? ;; esac
    echo "usage: sanitizers.sh --crosscheck asan|lsan|hwasan" >&2; exit 2 ;;
  --gate)
    case "${2:-}" in
      asan) gate_asan_like asan; exit $? ;;
      lsan) gate_asan_like lsan; exit $? ;;
      hwasan) gate_asan_like hwasan; exit $? ;;
      msan) gate_msan; exit $? ;;
      tsan) gate_tsan; exit $? ;;
      lsan-gtk) gate_lsan_gtk; exit $? ;;
      *) echo "usage: sanitizers.sh --gate asan|lsan|msan|hwasan|tsan|lsan-gtk" >&2; exit 2 ;;
    esac ;;
  --matrix)
    shift
    need_nightly
    SAN=""; FILTER=""; WS=0; LIST=""; EXCL=""
    while [ "$#" -gt 0 ]; do
      case "$1" in
        --san) SAN="$2"; shift 2 ;;
        --crates)
          # Space- or comma-separated; a spec with features (`crate:a,b`) keeps
          # its commas, so separate such specs with spaces.
          for tok in $2; do
            case "$tok" in
              *:*) LIST="$LIST $tok" ;;
              *) LIST="$LIST $(printf '%s' "$tok" | tr ',' ' ')" ;;
            esac
          done
          shift 2 ;;
        --workspace) WS=1; shift ;;
        --filter) FILTER="$2"; shift 2 ;;
        --exclude) EXCL="$EXCL$2|"$'\n'; shift 2 ;;
        *) echo "sanitizers.sh --matrix: unknown argument '$1'" >&2; exit 2 ;;
      esac
    done
    case "$SAN" in asan|lsan|msan|hwasan|tsan) ;; *) echo "sanitizers.sh --matrix: --san asan|lsan|msan|hwasan|tsan" >&2; exit 2 ;; esac
    if [ "$WS" -eq 1 ]; then LIST="$LIST $(scope_minus "$EXCL" | tr '\n' ' ')"; fi
    # shellcheck disable=SC2086  # LIST is a word list of crate specs
    set -- $LIST
    [ "$#" -gt 0 ] || { echo "sanitizers.sh --matrix: no crates (use --crates or --workspace)" >&2; exit 2; }
    matrix_run "$SAN" "$FILTER" "$@"
    matrix_summary "$SAN"
    gate_verdict "MATRIX($SAN)" 0; exit $? ;;
  ""|-h|--help)
    sed -n '2,50p' "$0"; exit 2 ;;
  *) legacy_main "$@"; exit $? ;;
esac
