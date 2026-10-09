#!/bin/sh
# c-vt example parity gate (G4 leg B, per example): diff a rustty c-vt example's
# stdout against its ghostty twin. The twin is NOT built with the example's own
# `zig build run`: on a macOS host at the pin (`gates/ORACLE_PIN`) that aborts (N525, upstream
# d65cb5128). It is built the way .port/phase-0/build-examples.sh documents
# (N395, P8-W0 T3/T7): the oracle's own `example/<name>/src/main.c[pp]` compiled
# with Xcode clang, in a scratch dir, against the oracle's `zig-out` libghostty-vt
# (built first with `zig build -Demit-lib-vt=true` in the oracle). The oracle tree is
# only read. A missing/stale oracle build is BLOCKED (exit 3), never a skip. The one
# nondeterministic field (the OSC 5522 `pw=` token, N395) is masked on both sides,
# exactly as run_differential.sh `cvt_normalize` does.
# Exit codes: 0 identical; 1 differs / a build or run failed; 2 usage; 3 BLOCKED (oracle
# absent, off the pin or not built); 4 NOT COVERED (the example has no Rustty `build.sh`,
# i.e. the CMake examples: leg B has no Rustty twin to run for it; this is reported, never
# counted as a pass).
# Usage: .port/tools/cvt-parity.sh <example-name>
#        .port/tools/cvt-parity.sh --all   (every example/c-vt* + cpp-vt*; prints the verdict
#          file's body; exit 0 only if every covered example is identical and none is
#          BLOCKED/failed; NOT COVERED examples are listed and do not fail the run)
#        .port/tools/cvt-parity.sh --print-oracle   (the derived oracle root; G-ORACLE-SH-SELF arm 5)
set -e
if [ "${1:-}" = "--all" ]; then
  self="$0"; rootd=$(cd "$(dirname "$0")/../.." && pwd); fail=0; ran=0; nc=0
  for p in "$rootd"/example/*; do
    d=${p##*/}
    case $d in c-vt*|cpp-vt*) ;; *) continue ;; esac
    sh "$self" "$d" && rc=0 || rc=$?
    case $rc in 0) ran=$((ran + 1)) ;; 4) nc=$((nc + 1)) ;; *) fail=1 ;; esac
  done
  echo "VERDICT: $([ $fail = 0 ] && echo GREEN || echo RED) (identical: $ran, not covered: $nc)"
  exit $fail
fi
name="${1:-}"; [ -n "$name" ] || { echo "usage: cvt-parity.sh <c-vt-example>"; exit 2; }
root=$(cd "$(dirname "$0")/../.." && pwd)
# N483: the oracle is `ghostty` beside the main checkout (`rustty_oracle_root`), not a fixed path.
ORACLE="$(bash -c '. "$1" || exit 3; rustty_oracle_root "$2"' oracle_root "$root/.port/tools/gates/_oracle_root.sh" "$root")"
[ "$name" = "--print-oracle" ] && { printf '%s\n' "$ORACLE"; exit 0; }
gdir="$ORACLE/example/$name"
rdir="$root/example/$name"
T="$(mktemp -d "${TMPDIR:-/tmp}/cvt-parity.XXXXXX")" || exit 3
trap 'rm -rf "$T"' EXIT
[ -d "$gdir" ] || { echo "BLOCKED: ghostty example $gdir absent"; exit 3; }
[ -d "$rdir" ] || { echo "$name: rustty port missing ($rdir)"; exit 1; }
[ -f "$rdir/build.sh" ] || { echo "$name: NOT COVERED (no Rustty build.sh: no twin to run in leg B)"; exit 4; }
# The oracle must be at the pin with example/ and include/ clean, and its built headers the pin's.
if ! why="$(bash -c '. "$1" || exit 3; rustty_oracle_pin_status "$2" "$(rustty_oracle_pin)" example include' pin_status "$root/.port/tools/gates/_oracle_root.sh" "$ORACLE")"; then
  echo "BLOCKED: $why"; exit 3
fi
INC="$ORACLE/zig-out/include"
diff -r "$ORACLE/include/ghostty" "$INC/ghostty" >/dev/null 2>&1 \
  || { echo "BLOCKED: $INC/ghostty missing or differs from the oracle include/ghostty (zig build -Demit-lib-vt=true in the oracle)"; exit 3; }
LIBA=""
for cand in "$ORACLE/zig-out/lib/libghostty-vt.a" \
  "$ORACLE/zig-out/lib/ghostty-vt.xcframework/macos-arm64_x86_64/libghostty-vt.a"; do
  if [ -f "$cand" ] && lipo -info "$cand" >/dev/null 2>&1; then LIBA="$cand"; break; fi
done
[ -n "$LIBA" ] || { echo "BLOCKED: no Mach-O libghostty-vt.a under $ORACLE/zig-out/lib"; exit 3; }
CC="$(xcrun --find clang 2>/dev/null)" && CXX="$(xcrun --find clang++ 2>/dev/null)" \
  || { echo "BLOCKED: xcrun cannot find clang/clang++"; exit 3; }
SYSROOT="$(xcrun --show-sdk-path)"
defs=""
case "$name" in c-vt-static|c-vt-snapshot) defs="-DGHOSTTY_STATIC" ;; esac
fw="-framework CoreFoundation -framework CoreText -framework CoreGraphics -framework Foundation"
if [ -f "$gdir/src/main.cpp" ]; then
  # shellcheck disable=SC2086
  "$CXX" -isysroot "$SYSROOT" -O0 -g -I"$INC" "$gdir/src/main.cpp" "$LIBA" -lc++ $fw -o "$T/ref" >"$T/ref-build.log" 2>&1
else
  # shellcheck disable=SC2086
  "$CC" -isysroot "$SYSROOT" -O0 -g $defs -I"$INC" "$gdir/src/main.c" "$LIBA" $fw -o "$T/ref" >"$T/ref-build.log" 2>&1
fi || { echo "$name: ghostty ref build failed"; cat "$T/ref-build.log"; exit 1; }
"$T/ref" > "$T/g-raw.txt" 2>&1 || { echo "$name: ghostty ref run failed"; exit 1; }
sh "$rdir/build.sh" > "$T/r-raw.txt" 2>/dev/null || { echo "$name: rustty build/run failed"; exit 1; }
mask() { LC_ALL=C sed -E 's#(pw=)[A-Za-z0-9+/]+=*#\1<PW>#g' "$1"; }
mask "$T/g-raw.txt" > "$T/g.txt"; mask "$T/r-raw.txt" > "$T/r.txt"
if diff "$T/g.txt" "$T/r.txt" >/dev/null; then
  echo "$name: BYTE-IDENTICAL ✓ ($(wc -l < "$T/r.txt" | tr -d ' ') lines)"
else
  echo "$name: DIFFERS"; diff "$T/g.txt" "$T/r.txt"; exit 1
fi
