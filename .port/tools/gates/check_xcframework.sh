#!/usr/bin/env bash
# GATE G-XCF — the XCFramework structure/ABI lane.
#
# PORTS: ghostty `.github/scripts/check-apple-libghostty-vt.nu`, which its
# release pipeline runs over each slice of ghostty-vt.xcframework.
#
# ADAPTED, not transliterated, because the artifact differs. The reference
# ships a DYLIB per slice, so its script can check the `@rpath/...` install
# name, the single LC_BUILD_VERSION platform value, LC_ENCRYPTION_INFO_64 for
# physical-iOS slices, and — the sharp one — that `nm -gjU` lists ONLY
# `_ghostty_*` exports.
#
# rustty's `xtask xcframework-vt` ships a STATIC ARCHIVE. None of the Mach-O
# dylib load commands exist in a .a, and the export check inverts: a static
# archive necessarily carries every internal symbol of every object it
# contains (15,000+ here, mostly Rust std and the vendored C), so "only
# rustty_* is exported" is FALSE BY CONSTRUCTION and asserting it would be
# asserting something untrue of this artifact.
#
# What carries over, and what replaces the export check:
#   1. bundle structure — Info.plist, the slice directory, Headers, the library
#   2. `lipo -archs` == the expected slice architecture
#   3. the public C API IS present (a floor on `_rustty_*` count, so a build
#      that silently dropped the exports cannot pass)
#   4. NO `_ghostty_*` symbol leaked in (R2 identity: the shipped artifact must
#      not carry the oracle's spelling)
#   5. the clang module map names the module and its umbrella header RESOLVES
#      to a file that exists — a Swift consumer fails at import otherwise, and
#      G-EXAMPLES' swift lane would only catch that after a full SwiftPM build
#
# BLOCKED (exit 3), never green, off macOS: lipo/nm/xcodebuild are Apple tools
# and `xtask xcframework-vt` needs Xcode.
#
# Exit contract (`.port/tools/CI.md`): 0 = pass, 1 = red, 3 = BLOCKED.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$ROOT" || exit 1

BUNDLE="dist/rustty-vt.xcframework"
SLICE="macos-arm64"
LIB="librustty-vt.a"
MODULE="RusttyVt"
UMBRELLA="rustty/vt.h"
# Measured at landing: 204 `_rustty_*` exports in the simd build. A floor, not
# an equality, so ADDING C API does not spuriously fail; G-SYMMANIFEST is what
# pins the exact header-vs-export set.
MIN_EXPORTS=180

blocked() { echo "G-XCF BLOCKED: $*"; exit 3; }
red()     { echo "G-XCF RED: $*" >&2; exit 1; }

[ "$(uname -s)" = "Darwin" ] || blocked "macOS-only (lipo/nm/xcodebuild)"
for t in lipo nm xcodebuild; do
  command -v "$t" >/dev/null 2>&1 || blocked "$t not installed (Xcode command-line tools)"
done

echo "=== G-XCF 1/4 :: build the framework ==="
# Rebuild unconditionally: a stale bundle would let this gate bless an artifact
# built from different sources. Same reasoning as WS19 T16.
if ! cargo run -q -p xtask -- xcframework-vt > /dev/null 2>&1; then
  cargo run -q -p xtask -- xcframework-vt 2>&1 | tail -20 | sed 's/^/    /'
  red "xtask xcframework-vt failed"
fi
echo "    ok      $BUNDLE"

echo "=== G-XCF 2/4 :: bundle structure ==="
for f in "Info.plist" "$SLICE/$LIB" "$SLICE/Headers/module.modulemap" "$SLICE/Headers/$UMBRELLA"; do
  if [ -e "$ROOT/$BUNDLE/$f" ]; then
    echo "    ok      $f"
  else
    echo "    MISSING $f"
    red "the bundle is incomplete"
  fi
done

echo "=== G-XCF 3/4 :: architecture ==="
actual_arch="$(lipo -archs "$ROOT/$BUNDLE/$SLICE/$LIB" 2>/dev/null | tr -d ' ')"
want_arch="arm64"
[ "$actual_arch" = "$want_arch" ] || red "expected architecture $want_arch, found '${actual_arch:-none}'"
echo "    ok      $want_arch"

echo "=== G-XCF 4/4 :: symbols and module map ==="
exports="$(nm -gjU "$ROOT/$BUNDLE/$SLICE/$LIB" 2>/dev/null)"
n_rustty="$(printf '%s\n' "$exports" | grep -c '^_rustty_' || true)"
[ "$n_rustty" -ge "$MIN_EXPORTS" ] \
  || red "only $n_rustty _rustty_* exports (floor $MIN_EXPORTS) — did the C API get dropped?"
echo "    ok      $n_rustty _rustty_* exports (floor $MIN_EXPORTS)"

# R2: the shipped artifact must not carry the oracle's identity.
n_ghostty="$(printf '%s\n' "$exports" | grep -c '^_ghostty_' || true)"
if [ "$n_ghostty" -ne 0 ]; then
  printf '%s\n' "$exports" | grep '^_ghostty_' | head -10 | sed 's/^/      /'
  red "$n_ghostty _ghostty_* symbol(s) leaked into the shipped framework (R2)"
fi
echo "    ok      no _ghostty_* symbols (R2 identity)"

mm="$ROOT/$BUNDLE/$SLICE/Headers/module.modulemap"
grep -q "module $MODULE " "$mm" || red "module.modulemap does not declare module $MODULE"
grep -q "umbrella header \"$UMBRELLA\"" "$mm" \
  || red "module.modulemap's umbrella header is not \"$UMBRELLA\""
echo "    ok      module $MODULE, umbrella $UMBRELLA (and the file exists)"

echo "G-XCF GREEN: $BUNDLE is well-formed ($want_arch, $n_rustty exports, module $MODULE)."
exit 0
