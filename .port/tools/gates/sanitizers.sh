#!/usr/bin/env bash
# sanitizers.sh — sanitizer matrix gate. Complements Miri (R6: right tool per job).
#
# Miri catches Stacked-Borrows/provenance/uninit/leak on pure Rust but is ~100-400x
# native (a few tests are intractable). The sanitizers run at ~2x native and catch
# real memory CORRUPTION (UAF/OOB/double-free) + races across ALL tests incl. the
# Miri-intractable ones, and cover the C-FFI boundary Miri can't model.
#
# Platform support (re-verified P7-W14, 2026-10-01; aarch64-apple-darwin dev box
# and the aarch64 Fedora VM `rvm`):
#   address (ASan)  ✅ macOS + Linux                         -> G-ASAN
#   leak    (LSan)  ❌ macOS (rustc-rejected; Apple ASan has no leak checker)
#                   ✅ Linux, run as ASan with detect_leaks=1 -> G-LSAN
#   memory  (MSan)  ❌ macOS ; ✅ Linux x86_64 and aarch64 (verified on rvm,
#                   2026-10-01), needs -Zbuild-std + instrumented C -> G-MSAN
#                   (MSAN_AARCH64_NOTE turns aarch64 into BLOCKED if that regresses)
#   thread  (TSan)  ✅ macOS + Linux  (needs -Zbuild-std)  -> G-TSAN, not yet a gate
#   hwaddress(HWASan)❌ macOS ; aarch64-Linux/Android only -> not a gate
#   cfi             mitigation, not a bug-checker; needs -Clto -> optional Goal-2 hardening
#
# Design: docs/superpowers/specs/2026-10-01-memory-safety-lanes-design.md, section B.
#
# MODES
#   sanitizers.sh <crate> [filter] [sanitizer ...]
#       LEGACY single-crate run (check_leaks.sh's G6 ASan companion arms call it
#       this way). Sanitizers are rustc names: address thread leak memory hwaddress.
#       Prints "SANITIZERS: GREEN|PROBLEM"; exit 0 / 1.
#   sanitizers.sh --matrix --san <asan|lsan|msan> (--crates <a,b c> | --workspace)
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
#   sanitizers.sh --gate <asan|lsan|msan>
#       The ci.sh gates G-ASAN / G-LSAN / G-MSAN: the matrix over the workspace
#       minus that sanitizer's cited exclusions, plus (asan, lsan) the Miri-
#       exception cross-check (every `cfg_attr(miri, ignore = "...")` test whose
#       reason claims ASan coverage must have RUN and PASSED here).
#   sanitizers.sh --self-test
#       G-SAN-SELF: a synthetic crate with a deliberate heap overflow (RED under
#       ASan), leak (RED under LSan), and uninitialized read (RED under MSan); a
#       clean control (GREEN); an empty filter (vacuous RED); and the log
#       classifier and cross-check over canned logs.
#
# EXIT CONTRACT (ci.sh): 0 GREEN ; 1 RED (a sanitizer finding, a test failure,
# zero tests run, a stale exclusion, an unverified ASan claim) ; 3 BLOCKED (no
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
LSAN_SUPP="$SUPP_DIR/lsan.supp"
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
# Feature lanes G-ASAN and G-LSAN add on top of the members (a plain
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
  "rustty-renderer|opengl::|the OpenGL backend's tests: EGL contexts (gl::sys, gl::test_support, opengl_impl) and shader builds through glslang (uninstrumented libstdc++, M2); the module's few pure tests (gpu_abi layout checks) go with it, since --skip is a substring filter (M3)"
  "rustty-renderer|shadertoy::|custom-shader translation through glslang and spirv-cross, whose glslang preprocessor reads libstdc++-constructed stream state (M2)"
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
classify() {
  local log="$1" rc="$2" ran
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
write_msan_wrappers() {
  local d="$SAN_TARGET_ROOT/san-cc-wrappers"
  mkdir -p "$d" || return 1
  cat >"$d/msan-stat-shim.h" <<'SHIM'
/* G-MSAN (sanitizers.sh): see write_msan_wrappers. */
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
/* G-MSAN (sanitizers.sh): see write_msan_wrappers (M5). */
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
  done
}

# ---------------------------------------------------------------------------
# One crate under one sanitizer. run_crate SAN SPEC FILTER TD LOG
# SAN is a matrix id (asan|lsan|msan) or a legacy rustc name.
# ---------------------------------------------------------------------------
run_crate() {
  local san="$1" spec="$2" filter="$3" td="$4" log="$5"
  local crate="${spec%%:*}" feats="" tgt rflags="" build_std="" rc
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
      envs+=("ASAN_OPTIONS=detect_leaks=0:allocator_may_return_null=1") ;;  # WHY: Apple ASan has no leak checker (rustc rejects -Zsanitizer=leak on Darwin); on Linux, G-LSAN is the leak lane, so G-ASAN keeps its verdict to memory errors only
    lsan)
      rflags="-Zsanitizer=address"
      envs+=("ASAN_OPTIONS=detect_leaks=1:allocator_may_return_null=1")      # WHY: not suppressive — turns the leak checker ON; this IS G-LSAN (design §B: LSan run as ASan with detect_leaks=1)
      envs+=("LSAN_OPTIONS=suppressions=$LSAN_SUPP:print_suppressions=1") ;;  # WHY: every entry in lsan.supp is a third-party leak with a cited reason in .port/phase-2/SANITIZER_SUPPRESSIONS.md; print_suppressions=1 lists every match in the log so a suppression is never silent
    msan|memory)
      rflags="-Zsanitizer=memory -Zsanitizer-memory-track-origins ${SAN_MSAN_DIAG:+-Zsanitizer-recover=memory}"
      build_std="-Zbuild-std"
      # The C/C++ we compile ourselves through `cc` is instrumented too, through
      # the msan-cc wrappers (see write_msan_wrappers): plain CFLAGS would also
      # reach build-dependencies compiled for the build script, which is not
      # instrumented, and fail to link (`undefined reference to __msan_init`,
      # rustty-cli's zlib-sys build-dependency, P7-W14).
      write_msan_wrappers || return 2
      envs+=("CC=$SAN_TARGET_ROOT/san-cc-wrappers/msan-cc" "CXX=$SAN_TARGET_ROOT/san-cc-wrappers/msan-c++")
      envs+=("MSAN_OPTIONS=poison_in_dtor=1:allocator_may_return_null=1${SAN_MSAN_DIAG:+:halt_on_error=0}") ;;  # SAN_MSAN_DIAG=1: a diagnosis run that reports every use instead of stopping at the first (never a gate setting; the verdict is still RED on any report)
    thread)
      rflags="-Zsanitizer=thread"; build_std="-Zbuild-std" ;;
    leak)
      rflags="-Zsanitizer=leak" ;;
    hwaddress)
      rflags="-Zsanitizer=hwaddress" ;;
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
  if [ "$san" != leak ] && [ "$san" != hwaddress ]; then envs+=("RUST_MIN_STACK=8388608"); fi
  # Line tables only: sanitizer reports still name every function, file and
  # line, but the instrumented debug binaries link in a fraction of the memory
  # and disk. With full debuginfo the GTK apprt lane's test links (ld.bfd, 8 at
  # once) exhausted the 10 GB Linux VM and ld died with SIGBUS (P7-W14).
  envs+=("CARGO_PROFILE_DEV_DEBUG=line-tables-only" "CARGO_PROFILE_TEST_DEBUG=line-tables-only")
  if [ -n "$SAN_MANIFEST_PATH" ]; then cargo_args+=(--manifest-path "$SAN_MANIFEST_PATH"); fi
  cargo_args+=(-p "$crate" --target "$tgt" --no-fail-fast)
  if [ -n "$feats" ]; then cargo_args+=(--features "$feats"); fi
  if [ -n "$build_std" ]; then cargo_args+=("$build_std"); fi
  mkdir -p "$(dirname "$log")"
  # shellcheck disable=SC2086  # FILTER is an intentional word list (libtest takes several)
  env "${envs[@]}" RUSTFLAGS="$rflags" RUSTDOCFLAGS="$rflags" CARGO_TARGET_DIR="$td" \
    cargo +nightly test "${cargo_args[@]}" -- $filter >"$log" 2>&1
  rc=$?
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
      leak|hwaddress) if [ "$OS" = "Darwin" ]; then echo "  ⏭  $s: skipped (Linux-only)"; continue; fi ;;
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
    "$SAN_TARGET_ROOT"/san-asan|"$SAN_TARGET_ROOT"/san-lsan|"$SAN_TARGET_ROOT"/san-msan) ;;
    "$SAN_TARGET_ROOT"/san-asan-*|"$SAN_TARGET_ROOT"/san-lsan-*|"$SAN_TARGET_ROOT"/san-msan-*) ;;
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
    run_crate "$san" "$spec" "$filter$(spec_skips "$san" "$spec")" "$td" "$log"; rc=$?
    t1=$(date +%s)
    v="$(classify "$log" "$rc")"
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

gate_asan_like() {  # gate_asan_like asan|lsan
  local san="$1" id excl extra=0 e
  if [ "$san" = asan ]; then id="G-ASAN"; else id="G-LSAN"; fi
  own_job_blocked "$id"
  need_nightly
  if [ "$san" = lsan ] && [ "$OS" != "Linux" ]; then
    echo "BLOCKED: $id needs LeakSanitizer, which exists on Linux only (rustc rejects -Zsanitizer=leak on $OS and Apple's ASan has no leak checker). It runs on the Linux VM (rvm) and in ci.yml's sanitizers job."
    exit 3
  fi
  if [ "$san" = lsan ] && [ ! -r "$LSAN_SUPP" ]; then
    echo "$id: RED — $LSAN_SUPP is missing (SANITIZER_SUPPRESSIONS.md's file)"; exit 1
  fi
  excl=""
  if [ "$san" = asan ]; then
    for e in ${ASAN_EXCLUDES[@]+"${ASAN_EXCLUDES[@]}"}; do excl="$excl$e"$'\n'; done
  else
    for e in ${LSAN_EXCLUDES[@]+"${LSAN_EXCLUDES[@]}"}; do excl="$excl$e"$'\n'; done
  fi
  echo "### $id: AddressSanitizer$( [ "$san" = lsan ] && echo ' + LeakSanitizer (detect_leaks=1)') over the workspace test suites ($OS/$ARCH) ###"
  if [ -n "$excl" ]; then
    echo "excluded (cannot run under the sanitizer):"
    printf '%s' "$excl" | awk -F'|' 'NF {printf "  - %s: %s\n", $1, $2}'
  fi
  local -a check=()
  while IFS= read -r e; do [ -n "$e" ] && check+=("$e"); done <<EOF
$excl
EOF
  check_members "$id exclusion" ${check[@]+"${check[@]}"} || extra=1
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

MSAN_AARCH64_NOTE=""
gate_msan() {
  own_job_blocked G-MSAN
  need_nightly
  if [ "$OS" != "Linux" ]; then
    echo "BLOCKED: G-MSAN needs MemorySanitizer, which exists on Linux only (not on $OS). It runs on the Linux VM (rvm) and in ci.yml's sanitizers job."
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
    'check_members x ${ASAN_EXCLUDES[@]+"${ASAN_EXCLUDES[@]}"} ${LSAN_EXCLUDES[@]+"${LSAN_EXCLUDES[@]}"} ${FEATURE_LANES[@]+"${FEATURE_LANES[@]}"} ${LINUX_FEATURE_LANES[@]+"${LINUX_FEATURE_LANES[@]}"} ${NO_TESTS_ON[@]+"${NO_TESTS_ON[@]}"} ${MSAN_EXCLUDES[@]+"${MSAN_EXCLUDES[@]}"}'
  hx "a stale *_EXCLUDES entry is RED" 1 "'no-such-crate' is listed but is not a workspace member" \
    'ASAN_EXCLUDES=("no-such-crate|fixture"); check_members "G-ASAN exclusion" "${ASAN_EXCLUDES[@]}"'
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

  # --- arm F: the BLOCKED arms (no build) ---
  out="$(RUSTTY_SANITIZERS_OWN_JOB="the fixture job" bash "$GATES_DIR/sanitizers.sh" --gate asan 2>&1)"; rc=$?
  if [ $rc -eq 3 ] && printf '%s' "$out" | grep -q 'defers to the fixture job'; then echo "  ok   F RUSTTY_SANITIZERS_OWN_JOB is BLOCKED (exit 3), naming the job"
  else echo "  FAIL F OWN_JOB (rc=$rc): $out"; fail=1; fi
  if [ "$OS" != "Linux" ]; then
    for g in lsan msan; do
      out="$(env -u RUSTTY_SANITIZERS_OWN_JOB bash "$GATES_DIR/sanitizers.sh" --gate "$g" 2>&1)"; rc=$?
      if [ $rc -eq 3 ] && printf '%s' "$out" | grep -q 'exists on Linux only'; then echo "  ok   F --gate $g on $OS is BLOCKED (exit 3), Linux-only"
      else echo "  FAIL F --gate $g on $OS (rc=$rc): $out"; fail=1; fi
    done
  fi

  # --- arms A/B: the synthetic crate under the real sanitizers ---
  local cr="$st/crate"
  mkdir -p "$cr/src"
  cat >"$cr/Cargo.toml" <<'TOML'
[package]
name = "san-selftest"
version = "0.0.0"
edition = "2021"
publish = false

[workspace]
TOML
  cat >"$cr/src/lib.rs" <<'RS'
//! G-SAN-SELF's synthetic crate: one deliberate defect per sanitizer, plus a
//! clean control. Each arm selects its test by exact name.
#[cfg(test)]
mod tests {
    #[inline(never)]
    fn oob_read(v: &[u8]) -> u8 {
        // 8 bytes past a 16-byte heap allocation: inside ASan's redzone.
        unsafe { *v.as_ptr().add(v.len() + 8) }
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

    #[test]
    fn clean_control() {
        let v: Vec<u8> = (0u8..16).collect();
        assert_eq!(v.iter().map(|&b| b as u32).sum::<u32>(), 120);
    }
}
RS
  local sroot="$st/target"
  arm() {  # arm NAME SAN FILTER WANT(RED|GREEN) MUST_GREP
    local name="$1" san="$2" filter="$3" want="$4" must="$5" o r
    o="$(SAN_PER_CRATE_TARGET="${ARM_PER_CRATE:-0}" SAN_MANIFEST_PATH="$cr/Cargo.toml" SAN_TARGET_ROOT="$sroot" SAN_LOG_ROOT="$st/slogs-$name" SAN_MIN_FREE_GB=0 \
         M_RED=0 M_BLOCKED=0 M_TOTAL=0 M_ROWS="" bash -c '. "$0" --source-only; matrix_run "$1" "$2" san-selftest; matrix_summary "$1"; gate_verdict SELF 0' \
         "$GATES_DIR/sanitizers.sh" "$san" "$filter" 2>&1)"; r=$?
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
  if [ "$OS" = "Linux" ]; then
    arm "A2 leak" lsan "tests::defect_leak --exact" RED "LeakSanitizer: detected memory leaks"
    arm "A2 control: the leak under G-ASAN (leak checker off)" asan "tests::defect_leak --exact" GREEN "1 test\\(s\\) ran"
    if [ -n "$MSAN_AARCH64_NOTE" ] && [ "$ARCH" = "aarch64" ]; then
      echo "  skip A3 uninit read under MSan: BLOCKED on $ARCH ($MSAN_AARCH64_NOTE)"
    else
      arm "A3 uninit read" msan "tests::defect_uninit_read --exact" RED "MemorySanitizer: use-of-uninitialized-value"
      arm "A3 control: clean under MSan" msan "tests::clean_control --exact" GREEN "1 test\\(s\\) ran"
    fi
  else
    echo "  skip A2 leak under LSan: Linux-only (rustc rejects -Zsanitizer=leak on $OS); runs on rvm / ci.yml"
    echo "  skip A3 uninit read under MSan: Linux-only; runs on rvm / ci.yml"
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
    case "${2:-}" in asan|lsan) asan_claims_crosscheck "$2"; exit $? ;; esac
    echo "usage: sanitizers.sh --crosscheck asan|lsan" >&2; exit 2 ;;
  --gate)
    case "${2:-}" in
      asan) gate_asan_like asan; exit $? ;;
      lsan) gate_asan_like lsan; exit $? ;;
      msan) gate_msan; exit $? ;;
      *) echo "usage: sanitizers.sh --gate asan|lsan|msan" >&2; exit 2 ;;
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
    case "$SAN" in asan|lsan|msan) ;; *) echo "sanitizers.sh --matrix: --san asan|lsan|msan" >&2; exit 2 ;; esac
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
