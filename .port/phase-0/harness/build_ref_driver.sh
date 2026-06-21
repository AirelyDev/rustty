#!/usr/bin/env bash
# build_ref_driver.sh — Compile the reference VT oracle (vt_driver.c) against
# the READ-ONLY ghostty reference's libghostty-vt static library.
#
# This NEVER writes into the ghostty tree. It only READS:
#   - headers:  $GHOSTTY/zig-out/include
#   - lib:      $GHOSTTY/zig-out/lib/libghostty-vt.a
# and writes the binary into .port/phase-0/harness/bin/vt_driver_ref.
#
# NOTE: in this environment the `cc` command is aliased to the Claude CLI, so
# we MUST resolve a real C compiler explicitly (xcrun clang on macOS).
set -euo pipefail

HARNESS_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
GHOSTTY="${GHOSTTY:-/Users/gael/projects/ghostty}"
INC="$GHOSTTY/zig-out/include"
LIBA="$GHOSTTY/zig-out/lib/libghostty-vt.a"
OUT="$HARNESS_DIR/bin/vt_driver_ref"

# Resolve a real compiler (avoid the `cc` alias in this shell).
SYSROOT=""
if command -v xcrun >/dev/null 2>&1; then
  CC="$(xcrun --find clang)"
  SYSROOT="$(xcrun --show-sdk-path 2>/dev/null || true)"
elif [ -x /usr/bin/clang ]; then
  CC=/usr/bin/clang
else
  echo "ERROR: no real C compiler found (xcrun clang / /usr/bin/clang)" >&2
  exit 10
fi
SYSROOT_FLAG=()
if [ -n "$SYSROOT" ]; then SYSROOT_FLAG=(-isysroot "$SYSROOT"); fi

[ -f "$LIBA" ] || { echo "ERROR: missing $LIBA (run reference lib build first)" >&2; exit 11; }
[ -d "$INC" ]  || { echo "ERROR: missing include dir $INC" >&2; exit 11; }

mkdir -p "$HARNESS_DIR/bin"

echo "CC=$CC"
echo "INC=$INC"
echo "LIBA=$LIBA"
echo "OUT=$OUT"

# Static link of libghostty-vt.a. On macOS, libghostty-vt needs the system
# frameworks it was built against (CoreFoundation etc. are pulled in by the
# objc/Foundation paths even in the VT lib for some targets). We link a small,
# safe set and let the linker drop unused ones.
"$CC" -std=c11 -O2 -g \
  "${SYSROOT_FLAG[@]}" \
  -I"$INC" \
  "$HARNESS_DIR/ref_driver/vt_driver.c" \
  "$LIBA" \
  -framework CoreFoundation -framework CoreText -framework CoreGraphics \
  -framework Foundation \
  -o "$OUT"

echo "OK: built $OUT"
"$OUT" --help >/dev/null 2>&1 || true
file "$OUT" 2>/dev/null || true
