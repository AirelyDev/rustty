#!/usr/bin/env bash
# GATE G-XTARGET — the cross-target build lane.
#
# PORTS: ghostty `.github/workflows/test.yml`'s
# `build-libghostty-vt-{wasm,macos,android,windows}` and
# `build-ghostty-windows-gnu` jobs — the legs that prove librustty-vt still
# COMPILES for targets nobody develops on.
#
# WHY A BUILD AND NOT A CHECK. rustty already has G-WINCHK
# (w2f001_windows_check_gate.sh), which is `cargo check`-level. That catches
# type errors but not link-time or codegen-time breakage, and it never runs the
# `crates-sys/*` C compiles for the target — which is where cross-target
# breakage actually lives (see the c-vt-cmake-cross defects earlier on this
# branch: an ar/ranlib mismatch and a missing link interface, neither of which
# a `cargo check` can see).
#
# TARGET AVAILABILITY IS EXPLICIT. A target whose rustup std is not installed
# is reported `skipped <target> (rustup target add ...)` BY NAME. If NONE are
# available the gate exits 3 BLOCKED rather than 0 — a green over an empty
# target set is not a pass. Targets that additionally need a C toolchain the
# host lacks (Android's NDK, Windows-GNU's mingw) say so specifically instead
# of failing.
#
# NAMED TARGETS ARE REQUIRED. With no arguments the gate builds what the host
# can and lists the rest under `Not built:`. When targets are named (N429's
# Android job names all three Android triples), every named target must build:
# a named target that is skipped, or that is not a G-XTARGET target at all,
# makes the run BLOCKED (exit 3) even if the others built (review A-1 F1).
#
# Exit contract (`.port/tools/CI.md`): 0 = pass, 1 = red, 3 = BLOCKED.
#
# Usage:
#   bash .port/tools/gates/check_cross_targets.sh
#   bash .port/tools/gates/check_cross_targets.sh wasm32-unknown-unknown
#   bash .port/tools/gates/check_cross_targets.sh --self-test
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$ROOT" || exit 1

command -v cargo >/dev/null 2>&1 || { echo "G-XTARGET BLOCKED: cargo not installed"; exit 3; }

SELF_TEST=0
WANT=()
for arg in "$@"; do
  case "$arg" in
    --self-test) SELF_TEST=1 ;;
    -*) echo "G-XTARGET BLOCKED: unknown option $arg"; exit 3 ;;
    *) WANT+=("$arg") ;;
  esac
done
# `${WANT[@]+...}`: bash 3.2 (macOS /bin/bash) treats an empty array as unset under `set -u`.
want() { [ "${#WANT[@]}" -eq 0 ] || printf '%s\n' ${WANT[@]+"${WANT[@]}"} | grep -qx "$1"; }

RAN=0
FAILED=0
SKIPPED=""
FAILED_NAMES=""

ok()   { echo "  ok      $1"; RAN=$((RAN+1)); }
bad()  { echo "  FAIL    $1"; RAN=$((RAN+1)); FAILED=1; FAILED_NAMES="$FAILED_NAMES $1"; }
skip() { echo "  skipped $1 ($2)"; SKIPPED="$SKIPPED $1"; }

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# The verdict, from RAN/FAILED/SKIPPED and whether targets were named. Prints
# the summary and returns the exit code.
verdict() {
  echo "--- G-XTARGET summary: $RAN built, $(printf '%s' "$SKIPPED" | wc -w | tr -d ' ') skipped ---"
  if [ "$FAILED" -ne 0 ]; then
    echo "G-XTARGET RED:$FAILED_NAMES"
    return 1
  fi
  if [ "$RAN" -eq 0 ]; then
    echo "G-XTARGET BLOCKED: no cross target was available on this host; skipped:$SKIPPED"
    return 3
  fi
  if [ "${#WANT[@]}" -ne 0 ] && [ -n "$SKIPPED" ]; then
    echo "G-XTARGET BLOCKED: named target(s) not built:$SKIPPED"
    return 3
  fi
  echo "G-XTARGET GREEN ($RAN target(s) built).${SKIPPED:+ Not built:$SKIPPED}"
  return 0
}

# Cargo's target directory, from `cargo metadata` so that CARGO_TARGET_DIR and
# `build.target-dir` are honoured (review A-1 F2). Empty on failure.
cargo_target_dir() {
  cargo metadata --format-version 1 --no-deps 2>/dev/null \
    | python3 -c 'import json, sys; print(json.load(sys.stdin)["target_directory"])' 2>/dev/null
}

# Does the host C compiler actually support this target?
#
# Every crates-sys/* crate compiles C/C++ through cc-rs, so a target is only
# buildable here if `cc --target=<triple>` can produce an object. Probing with
# a trivial translation unit FIRST is what separates "this host cannot cross to
# that platform" from "rustty's C is broken" — the two look nearly identical in
# a cc-rs failure, and an earlier version of this gate reported RED for
# x86_64-unknown-freebsd purely because the macOS clang has no FreeBSD sysroot.
probe_cc_target() {
  local triple="$1" probe rc
  probe="$WORK/probe-$triple"
  mkdir -p "$probe"
  # A C++ TU that INCLUDES A STANDARD HEADER, not `int main(void){return 0;}`.
  # The trivial C version is useless as a probe: clang happily accepts
  # `--target=x86_64-unknown-freebsd` for it because compiling an empty
  # translation unit needs no sysroot at all. What crates-sys actually does —
  # highway-sys compiling C++17 against libc++ — is what has to be probed, and
  # that is where the missing sysroot shows up.
  printf '#include <cstddef>\n#include <cstdint>\nint main(void){return 0;}\n' > "$probe/t.cc"
  # cc-rs compiles each target with `CXX_<triple>` when it is set (the NDK's
  # `<triple><api>-clang++`, for example), so the probe asks that compiler.
  local cxx_var="CXX_${triple//-/_}"
  "${!cxx_var:-${CXX:-c++}}" --target="$triple" -std=c++17 -stdlib=libc++ \
    -c "$probe/t.cc" -o "$probe/t.o" > "$probe/log" 2>&1
  rc=$?
  return $rc
}

# Does the C compiler cc-rs would use for this target produce an object for it?
#
# The wasm legs compile C and no C++: wuffs-sys (the kitty graphics swizzles
# and, since P7-W5 T2 / DECISIONS B8, the kitty zlib decoder, which the
# reference builds for wasm32 too). They need a clang with the WebAssembly
# backend, which Apple's clang does not have; `CC_<triple>` selects another
# (e.g. Homebrew's llvm). No libc++ and no sysroot: the TU includes only
# <stddef.h>, a compiler header, as wuffs-sys's minimal libc headers do.
probe_c_target() {
  local triple="$1" probe rc
  probe="$WORK/probe-c-$triple"
  mkdir -p "$probe"
  printf '#include <stddef.h>\nsize_t f(size_t x){return x+1;}\n' > "$probe/t.c"
  local cc_var="CC_${triple//-/_}"
  "${!cc_var:-${CC:-clang}}" --target="$triple" -c "$probe/t.c" -o "$probe/t.o" > "$probe/log" 2>&1
  rc=$?
  return $rc
}

# S64-F9: `GhosttyLibVt.zig:273-276` links every Android lib-vt with
# `link_z_max_page_size = 16384` ("Support 16kb page sizes, required for
# Android 15+"). An Android leg passes only if every PT_LOAD segment of the
# linked librustty_vt_c.so is aligned to at least 16 KiB. llvm-readelf is taken
# from beside the target's cc-rs compiler (the NDK's bin/), else from PATH.
# Arguments: the triple and the path of the shared library to check.
android_page_align() {
  local triple="$1" so="$2" cc_var readelf aligns a bad_align=""
  cc_var="CC_${triple//-/_}"
  readelf="$(dirname "${!cc_var:-/nonexistent/x}")/llvm-readelf"
  [ -x "$readelf" ] || readelf="$(command -v llvm-readelf || true)"
  if [ -z "$readelf" ]; then
    echo "          no llvm-readelf to check $so's PT_LOAD alignment"
    bad "$triple"
    return
  fi
  if [ ! -f "$so" ]; then
    echo "          $so does not exist (S64-F9 alignment not checked)"
    bad "$triple"
    return
  fi
  aligns="$("$readelf" -lW "$so" | awk '$1 == "LOAD" { print $NF }')"
  for a in $aligns; do
    # bash arithmetic reads readelf's 0x-prefixed hex.
    if (( a < 16384 )); then bad_align="$bad_align $a"; fi
  done
  if [ -z "$aligns" ] || [ -n "$bad_align" ]; then
    echo "          $so PT_LOAD alignments: $(printf '%s' "$aligns" | tr '\n' ' ') (need >= 0x4000, S64-F9)"
    bad "$triple"
  else
    ok "$triple (PT_LOAD align $(printf '%s\n' "$aligns" | sort -u | tr '\n' ' '| sed 's/ $//'))"
  fi
}

# --self-test: prove the verdict and the PT_LOAD check flip (review A-1 F1, F3).
if [ "$SELF_TEST" -eq 1 ]; then
  echo "=== G-XTARGET --self-test ==="
  st_fail=0
  expect() { # <label> <expected> <actual>
    if [ "$2" = "$3" ]; then echo "  pass    $1 (exit $3)"
    else echo "  FAIL    $1: expected $2, got $3"; st_fail=1; fi
  }
  reset_state() { RAN=0; FAILED=0; SKIPPED=""; FAILED_NAMES=""; }

  # F1: the verdict. One target built and one skipped.
  reset_state; WANT=(a-target b-target); ok a-target > /dev/null; skip b-target "x" > /dev/null
  verdict > /dev/null; expect "named target skipped while another built -> BLOCKED" 3 $?
  # Negative control: the same outcome in a default (unnamed) run is GREEN,
  # so the BLOCKED above comes from the targets being named.
  reset_state; WANT=(); ok a-target > /dev/null; skip b-target "x" > /dev/null
  verdict > /dev/null; expect "unnamed run with a skip -> GREEN (control)" 0 $?
  reset_state; WANT=(a-target); ok a-target > /dev/null
  verdict > /dev/null; expect "every named target built -> GREEN" 0 $?
  reset_state; WANT=(a-target b-target); bad a-target > /dev/null; skip b-target "x" > /dev/null
  verdict > /dev/null; expect "named target failed -> RED" 1 $?
  # End to end: a name that is not a G-XTARGET target is never built.
  bash "${BASH_SOURCE[0]}" not-a-real-target > "$WORK/e2e.log" 2>&1
  rc=$?
  grep -q "skipped not-a-real-target" "$WORK/e2e.log" || { echo "  FAIL    end-to-end run did not name the skip"; st_fail=1; }
  expect "end to end: named unknown target -> BLOCKED" 3 "$rc"
  WANT=()

  # P7-W5 T2: the wasm legs' C-compiler probe reports a compiler that cannot
  # target the triple (`false`), and passes one that can (`true`, control).
  CC_selftest_c_target=false probe_c_target selftest-c-target; st_rc=$?
  [ "$st_rc" -ne 0 ] && st_rc=1
  expect "C probe with a compiler that fails -> not buildable" 1 "$st_rc"
  CC_selftest_c_target=true probe_c_target selftest-c-target
  expect "C probe with a compiler that succeeds -> buildable (control)" 0 $?

  # F2: the .so is looked for under cargo's target directory, not ./target.
  st_tdir="$(CARGO_TARGET_DIR="$WORK/tdir" cargo_target_dir)"
  if [ "$st_tdir" = "$WORK/tdir" ]; then echo "  pass    target dir follows CARGO_TARGET_DIR ($st_tdir)"
  else echo "  FAIL    target dir with CARGO_TARGET_DIR=$WORK/tdir: got '$st_tdir'"; st_fail=1; fi

  # F3: the 16 KiB PT_LOAD check on two ELF shared libraries built here.
  st_cc="${CC:-cc}"
  printf 'int rustty_g_xtarget_probe(void) { return 1; }\n' > "$WORK/st.c"
  elf_ok=1
  for size in 4096 16384; do
    "$st_cc" -shared -fPIC -Wl,-z,max-page-size=$size -o "$WORK/lib$size.so" "$WORK/st.c" \
      > "$WORK/cc-$size.log" 2>&1 || elf_ok=0
    [ "$elf_ok" -eq 1 ] && [ "$(head -c 4 "$WORK/lib$size.so" | od -An -c | tr -d ' ')" = '177ELF' ] || elf_ok=0
  done
  if [ "$elf_ok" -ne 1 ] || ! command -v llvm-readelf > /dev/null 2>&1; then
    echo "  the PT_LOAD arm needs a host cc that links ELF shared libraries with"
    echo "  -Wl,-z,max-page-size and llvm-readelf on PATH (a Linux host); this host has not"
    [ "$st_fail" -ne 0 ] && { echo "G-XTARGET SELF-TEST RED"; exit 1; }
    echo "G-XTARGET SELF-TEST BLOCKED: the verdict and target-dir cases passed; the PT_LOAD cases need an ELF toolchain"
    exit 3
  fi
  st_triple=selftest-linux-android
  unset "CC_${st_triple//-/_}"
  reset_state; android_page_align "$st_triple" "$WORK/lib16384.so" > "$WORK/pa.log"
  expect "16 KiB-aligned .so -> ok" 0 "$FAILED"
  grep -q "PT_LOAD align 0x4000" "$WORK/pa.log" || { echo "  FAIL    ok line lacks the 0x4000 alignment"; cat "$WORK/pa.log"; st_fail=1; }
  reset_state; android_page_align "$st_triple" "$WORK/lib4096.so" > "$WORK/pa.log"
  expect "4 KiB-aligned .so -> FAIL" 1 "$FAILED"
  grep -q "0x1000" "$WORK/pa.log" || { echo "  FAIL    FAIL line lacks the 0x1000 alignment"; cat "$WORK/pa.log"; st_fail=1; }
  reset_state; android_page_align "$st_triple" "$WORK/missing.so" > "$WORK/pa.log"
  expect "missing .so -> FAIL" 1 "$FAILED"
  if [ "$st_fail" -ne 0 ]; then echo "G-XTARGET SELF-TEST RED"; exit 1; fi
  echo "G-XTARGET SELF-TEST GREEN"
  exit 0
fi

INSTALLED="$(rustup target list --installed 2>/dev/null)"
TARGET_DIR=""

# target|extra cargo args|needs a cross C++ toolchain (1), a C-only one (c),
# or none (0)
TARGETS=(
  # The wasm artifact release-tip.yml's build-lib-vt-wasm ships. SIMD is off
  # for wasm by rustty-vt's own options.rs (the reference cannot build its
  # SIMD deps for wasm either), so this is the default feature set. It
  # compiles C since P7-W5 T2 (wuffs-sys, DECISIONS B8).
  "wasm32-unknown-unknown||c"
  # The WASI variant .port/tools/zig-cc-wasm.sh exists for.
  "wasm32-wasip1||c"
  # The reference's build-libghostty-vt-windows. MSVC needs no host C
  # toolchain for the Rust half; the crates-sys C compiles need one, so this
  # is the leg most likely to report a toolchain skip off Windows.
  "x86_64-pc-windows-msvc||1"
  # FreeBSD: DEFERRALS.md's "the platform's explicit ledger row".
  "x86_64-unknown-freebsd||1"
  # The reference's build-ghostty-windows-gnu. A different C ABI and linker
  # from the MSVC arm above, which is why it is a separate leg there too.
  "x86_64-pc-windows-gnu||1"
  # The reference's build-libghostty-vt-android. Needs the NDK's clang, so it
  # reports skipped on any host without it — which is the honest outcome
  # rather than the silence of not listing the target at all. This gate's
  # header named android before the target existed; now it is real.
  # The reference's matrix is aarch64-linux-android, x86_64-linux-android and
  # arm-linux-androideabi (test.yml:891-895); Zig's arm baseline cpu is v7a,
  # which is Rust's armv7-linux-androideabi. Point cc-rs and cargo at the NDK
  # with CC_/CXX_/AR_<triple> and CARGO_TARGET_<TRIPLE>_LINKER (task A-1).
  "aarch64-linux-android||1"
  "x86_64-linux-android||1"
  "armv7-linux-androideabi||1"
)

echo "=== G-XTARGET :: cross-target builds of rustty-vt-c ==="
# A named target that is not in TARGETS is never built: report it by name.
for name in ${WANT[@]+"${WANT[@]}"}; do
  known=0
  for spec in "${TARGETS[@]}"; do [ "${spec%%|*}" = "$name" ] && known=1; done
  [ "$known" -eq 1 ] || skip "$name" "not a G-XTARGET target"
done
# The simd128 arm's checker must itself work before its verdicts count (R5).
if ! python3 .port/tools/gates/wasm_target_features.py --self-test > "$WORK/wtf-self" 2>&1; then
  sed 's/^/          /' "$WORK/wtf-self"
  echo "G-XTARGET RED: wasm_target_features.py --self-test failed"
  exit 1
fi
for spec in "${TARGETS[@]}"; do
  target="${spec%%|*}"
  rest="${spec#*|}"
  extra="${rest%%|*}"
  needs_cc="${rest##*|}"
  want "$target" || continue

  if ! printf '%s\n' "$INSTALLED" | grep -qx "$target"; then
    skip "$target" "rustup target add $target"
    continue
  fi

  if [ "$needs_cc" = "c" ] && ! probe_c_target "$target"; then
    tv="${target//-/_}"
    skip "$target" "the C compiler cannot target it: set CC_$tv to a clang with the WebAssembly backend (Apple's clang has none)"
    continue
  fi
  if [ "$needs_cc" = "1" ] && ! probe_cc_target "$target"; then
    case "$target" in
      *-android*)
        tv="${target//-/_}"
        skip "$target" "no Android NDK clang for it: set CC_$tv and CXX_$tv (and AR_$tv, CARGO_TARGET_$(printf '%s' "$tv" | tr '[:lower:]' '[:upper:]')_LINKER) to the NDK's clang" ;;
      *) skip "$target" "host C compiler cannot target it (no sysroot/toolchain)" ;;
    esac
    continue
  fi

  log="$WORK/$target.log"
  # shellcheck disable=SC2086  # $extra is an intentional empty-or-flags word.
  if cargo build -p rustty-vt-c --target "$target" --release $extra > "$log" 2>&1; then
    case "$target" in
      *-android*)
        [ -n "$TARGET_DIR" ] || TARGET_DIR="$(cargo_target_dir)"
        if [ -z "$TARGET_DIR" ]; then
          echo "G-XTARGET BLOCKED: cargo metadata did not give the target directory"
          exit 3
        fi
        android_page_align "$target" "$TARGET_DIR/$target/release/librustty_vt_c.so" ;;
      *) ok "$target" ;;
    esac
    # The wasm `simd128` default (N422, `Config.zig:113-130`): the reference's
    # CI requires the artifact to need simd128 ("Verify WASM simd128"). Here
    # the module's target_features section must list it, which it does only
    # when `.cargo/config.toml`'s wasm32 rustflags applied (a RUSTFLAGS in the
    # environment replaces them, as `-Dcpu` does the reference's default).
    case "$target" in
      wasm32-*)
        [ -n "$TARGET_DIR" ] || TARGET_DIR="$(cargo_target_dir)"
        if [ -z "$TARGET_DIR" ]; then
          echo "G-XTARGET BLOCKED: cargo metadata did not give the target directory"
          exit 3
        fi
        wasm="$TARGET_DIR/$target/release/rustty_vt_c.wasm"
        # (Not counted as a build: `RAN` counts targets built.)
        if python3 .port/tools/gates/wasm_target_features.py "$wasm" simd128 > "$log.simd" 2>&1; then
          echo "  ok      $target requires simd128"
        else
          sed 's/^/          /' "$log.simd"
          echo "  FAIL    $target requires simd128"
          FAILED=1
          FAILED_NAMES="$FAILED_NAMES $target(simd128)"
        fi
        ;;
    esac
  else
    # A missing cross C toolchain is an ENVIRONMENT fact, not a rustty defect.
    # Distinguish it so a developer box without mingw/NDK does not read as a
    # code regression.
    if grep -qE "failed to find tool|No such file or directory .*(gcc|clang|cc)|linker .* not found|error: linker" "$log"; then
      tool="$(grep -oE 'failed to find tool "[^"]+"' "$log" | head -1 | sed 's/.*"\(.*\)"/\1/')"
      skip "$target" "no cross toolchain${tool:+ ($tool)} on this host"
    else
      tail -20 "$log" | sed 's/^/          /'
      bad "$target"
    fi
  fi
done

verdict
exit $?
