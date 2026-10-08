#!/usr/bin/env bash
# build-examples.sh — build the reference c-vt-* / cpp-vt-* example programs
# that run-cvt-corpora.sh runs to capture .port/phase-0/corpora/cvt/.
#
# Usage: .port/phase-0/build-examples.sh
#        .port/phase-0/build-examples.sh --print-oracle   (G-ORACLE-SH-SELF arm 5)
#
# Paths (N525): the repository root is derived from this script's location, so
# a worktree builds into ITS OWN tree; the oracle is `rustty_oracle_root`
# (.port/tools/gates/_oracle_root.sh), and it must be at the pin
# (.port/tools/gates/ORACLE_PIN) with `example/` and `include/` clean, or the
# script is BLOCKED (exit 3). Binaries go to
# .port/phase-0/examples-out/<dir>/zig-out/bin/<exe> (git-ignored); the oracle
# tree is only read.
#
# Supported method (N395, P8-W0 T3): each example's own source in the oracle
# (`example/<dir>/src/main.c`, `main.cpp` for cpp-vt-stream) compiled with
# Xcode clang against the oracle's libghostty-vt, which must be built first
# with `zig build -Demit-lib-vt=true` in the oracle (zig-out/include,
# zig-out/lib/libghostty-vt.a). The flags mirror each example's build.zig:
# `GHOSTTY_STATIC` for c-vt-static and c-vt-snapshot, libc++ for cpp-vt-stream,
# Zig's default Debug build (-O0 -g).
#
# Why not the examples' own `zig build` (N525): on a macOS host the oracle's
# `GhosttyLibVt.initShared` links the shared library with Apple's linker
# (`initLibApple`, upstream d65cb5128, 2026-07-23): a Run step, not a Compile
# artifact, so the dependency offers only `ghostty-vt-static`, and every
# example's `dep.artifact("ghostty-vt")` panics with "unable to find artifact
# 'ghostty-vt'". Upstream builds the examples only on Linux CI, where the Zig
# linker path still registers the artifact. That is an oracle-side build issue,
# not a Rustty one.
#
# NOTE: `cc` may be aliased in an interactive shell; the compiler is resolved
# with `xcrun --find`, as .port/phase-0/harness/build_ref_driver.sh does.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
# shellcheck source=SCRIPTDIR/../tools/gates/_oracle_root.sh
. "$ROOT/.port/tools/gates/_oracle_root.sh" || exit 3
GHOSTTY="$(rustty_oracle_root "$ROOT")"
if [ "${1:-}" = "--print-oracle" ]; then
  printf '%s\n' "$GHOSTTY"
  exit 0
fi

OUT="$ROOT/.port/phase-0/examples-out"
EX="$GHOSTTY/example"
INC="$GHOSTTY/zig-out/include"

if ! why="$(rustty_oracle_pin_status "$GHOSTTY" "$(rustty_oracle_pin)" example include)"; then
  echo "BLOCKED: $why"
  exit 3
fi
# The built headers must be the pin's (a stale zig-out would capture another revision).
if ! diff -r "$GHOSTTY/include/ghostty" "$INC/ghostty" >/dev/null 2>&1; then
  echo "BLOCKED: $INC/ghostty is missing or differs from the oracle's include/ghostty;"
  echo "  rebuild it in the oracle: zig build -Demit-lib-vt=true"
  exit 3
fi
# A Mach-O (ld64-linkable) libghostty-vt.a: the top-level one, else the
# xcframework slice (see build_ref_driver.sh).
LIBA=""
for cand in "$GHOSTTY/zig-out/lib/libghostty-vt.a" \
  "$GHOSTTY/zig-out/lib/ghostty-vt.xcframework/macos-arm64_x86_64/libghostty-vt.a"; do
  if [ -f "$cand" ] && lipo -info "$cand" >/dev/null 2>&1; then
    LIBA="$cand"
    break
  fi
done
if [ -z "$LIBA" ]; then
  echo "BLOCKED: no Mach-O libghostty-vt.a under $GHOSTTY/zig-out/lib"
  exit 3
fi
if ! CC="$(xcrun --find clang 2>/dev/null)" || ! CXX="$(xcrun --find clang++ 2>/dev/null)"; then
  echo "BLOCKED: xcrun cannot find clang/clang++ (Xcode command line tools)"
  exit 3
fi
SYSROOT="$(xcrun --show-sdk-path)"

examples=(c-vt c-vt-build-info c-vt-colors c-vt-effects c-vt-encode-focus c-vt-encode-key
  c-vt-encode-mouse c-vt-formatter c-vt-grid-ref-tracked c-vt-grid-traverse c-vt-kitty-graphics
  c-vt-modes c-vt-paste c-vt-render c-vt-selection c-vt-selection-gesture c-vt-sgr
  c-vt-size-report c-vt-static c-vt-stream cpp-vt-stream c-vt-search c-vt-snapshot)

fail=0
for d in "${examples[@]}"; do
  exe="${d//-/_}"
  bindir="$OUT/$d/zig-out/bin"
  mkdir -p "$bindir"
  defs=()
  case "$d" in c-vt-static | c-vt-snapshot) defs=(-DGHOSTTY_STATIC) ;; esac
  if [ -f "$EX/$d/src/main.cpp" ]; then
    cmd=("$CXX" -isysroot "$SYSROOT" -O0 -g -I"$INC" "$EX/$d/src/main.cpp" "$LIBA" -lc++)
  else
    cmd=("$CC" -isysroot "$SYSROOT" -O0 -g ${defs[@]+"${defs[@]}"} -I"$INC" "$EX/$d/src/main.c" "$LIBA")
  fi
  "${cmd[@]}" -framework CoreFoundation -framework CoreText -framework CoreGraphics \
    -framework Foundation -o "$bindir/$exe" >"$OUT/$d.log" 2>&1
  ec=$?
  echo "EXIT $d = $ec"
  [ "$ec" = 0 ] || fail=1
done
echo "ALL_EXAMPLES_DONE"
exit $fail
