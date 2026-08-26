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
# Exit contract (`.port/tools/CI.md`): 0 = pass, 1 = red, 3 = BLOCKED.
#
# Usage:
#   bash .port/tools/gates/check_cross_targets.sh
#   bash .port/tools/gates/check_cross_targets.sh wasm32-unknown-unknown
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$ROOT" || exit 1

command -v cargo >/dev/null 2>&1 || { echo "G-XTARGET BLOCKED: cargo not installed"; exit 3; }

WANT="$*"
want() { [ -z "$WANT" ] || printf '%s\n' $WANT | grep -qx "$1"; }

RAN=0
FAILED=0
SKIPPED=""
FAILED_NAMES=""

ok()   { echo "  ok      $1"; RAN=$((RAN+1)); }
bad()  { echo "  FAIL    $1"; RAN=$((RAN+1)); FAILED=1; FAILED_NAMES="$FAILED_NAMES $1"; }
skip() { echo "  skipped $1 ($2)"; SKIPPED="$SKIPPED $1"; }

INSTALLED="$(rustup target list --installed 2>/dev/null)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

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
  "${CXX:-c++}" --target="$triple" -std=c++17 -stdlib=libc++ \
    -c "$probe/t.cc" -o "$probe/t.o" > "$probe/log" 2>&1
  rc=$?
  return $rc
}

# target|extra cargo args|needs a cross C toolchain (1) or not (0)
TARGETS=(
  # The wasm artifact release-tip.yml's build-lib-vt-wasm ships. SIMD is off
  # for wasm by rustty-vt's own options.rs (the reference cannot build its
  # SIMD deps for wasm either), so this is the default feature set.
  "wasm32-unknown-unknown||0"
  # The WASI variant .port/tools/zig-cc-wasm.sh exists for.
  "wasm32-wasip1||0"
  # The reference's build-libghostty-vt-windows. MSVC needs no host C
  # toolchain for the Rust half; the crates-sys C compiles need one, so this
  # is the leg most likely to report a toolchain skip off Windows.
  "x86_64-pc-windows-msvc||1"
  # FreeBSD: DEFERRALS.md's "the platform's explicit ledger row".
  "x86_64-unknown-freebsd||1"
)

echo "=== G-XTARGET :: cross-target builds of rustty-vt-c ==="
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

  if [ "$needs_cc" = "1" ] && ! probe_cc_target "$target"; then
    skip "$target" "host C compiler cannot target it (no sysroot/toolchain)"
    continue
  fi

  log="$WORK/$target.log"
  # shellcheck disable=SC2086  # $extra is an intentional empty-or-flags word.
  if cargo build -p rustty-vt-c --target "$target" --release $extra > "$log" 2>&1; then
    ok "$target"
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

echo "--- G-XTARGET summary: $RAN built, $(printf '%s' "$SKIPPED" | wc -w | tr -d ' ') skipped ---"
if [ "$RAN" -eq 0 ]; then
  echo "G-XTARGET BLOCKED: no cross target was available on this host; skipped:$SKIPPED"
  exit 3
fi
if [ "$FAILED" -ne 0 ]; then
  echo "G-XTARGET RED:$FAILED_NAMES"
  exit 1
fi
echo "G-XTARGET GREEN ($RAN target(s) built).${SKIPPED:+ Not built:$SKIPPED}"
exit 0
