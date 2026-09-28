#!/usr/bin/env bash
# GATE G-XCF — the XCFramework structure/ABI lane.
#
# PORTS: ghostty `.github/scripts/check-apple-libghostty-vt.nu`, which the
# reference's `test.yml` job `build-libghostty-vt-macos` runs over each Apple
# target's `zig-out/lib/libghostty-vt.dylib` (`--expect-platform`,
# `--expect-arch`, `--expect-encryption`), not over the xcframework.
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
# The slices are the reference's Apple universal set
# (`GhosttyLibVt.initStaticAppleUniversal`, GhosttyLibVt.zig:146-215): one
# lipo'd macOS slice, `macos-arm64_x86_64`, always; and `ios-arm64` plus
# `ios-arm64-simulator` when the iOS SDK is installed (the same `xcrun --sdk
# iphoneos --show-sdk-path` probe `xtask` uses). The bundle must carry exactly
# that set: a leftover `macos-arm64` slice (the pre-universal layout) is RED.
#
# Per slice, what carries over, and what replaces the export check:
#   1. bundle structure — Info.plist, the slice directory, Headers, the library
#   2. `lipo -archs` == the slice's expected architecture set
#   3. the public C API IS present (a floor on `_rustty_*` count, so a build
#      that silently dropped the exports cannot pass)
#   4. NO `_ghostty_*` symbol leaked in (R2 identity: the shipped artifact must
#      not carry the oracle's spelling)
#   5. the clang module map names the module and its umbrella header RESOLVES
#      to a file that exists — a Swift consumer fails at import otherwise, and
#      G-EXAMPLES' swift lane would only catch that after a full SwiftPM build
#
# `--self-test` (R5) builds synthetic bundles from tiny clang-compiled archives
# and requires the checks to pass the good one and fail each broken one: the
# old arm64-only macOS slice, a macOS slice missing x86_64, a missing iOS slice,
# a stray extra slice, an export count under the floor, a leaked `_ghostty_*`
# symbol, and an umbrella header that does not resolve.
#
# BLOCKED (exit 3), never green, off macOS: lipo/nm/xcodebuild are Apple tools
# and `xtask xcframework-vt` needs Xcode.
#
# Exit contract (`.port/tools/CI.md`): 0 = pass, 1 = red, 3 = BLOCKED.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$ROOT" || exit 1

BUNDLE="dist/rustty-vt.xcframework"
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

# The expected "<slice dir>:<space-separated sorted archs>" list. $1 = 1 when
# the iOS slices are expected.
expected_slices() {
  echo "macos-arm64_x86_64:arm64 x86_64"
  if [ "$1" = 1 ]; then
    echo "ios-arm64:arm64"
    echo "ios-arm64-simulator:arm64"
  fi
}

ios_sdk_available() {
  xcrun --sdk iphoneos --show-sdk-path >/dev/null 2>&1
}

# check_bundle <bundle dir> <want ios 0|1>: prints progress, and on the first
# failure prints "RED: <why>" and returns 1.
check_bundle() {
  local bundle="$1" want_ios="$2" spec slice want_arch actual_arch f exports n_rustty n_ghostty mm
  local fail=0
  _fail() { echo "RED: $*"; fail=1; }

  echo "--- structure"
  if [ ! -e "$bundle/Info.plist" ]; then _fail "no Info.plist"; return 1; fi
  echo "    ok      Info.plist"
  local want_dirs actual_dirs
  want_dirs="$(expected_slices "$want_ios" | cut -d: -f1 | sort | tr '\n' ' ')"
  actual_dirs="$(find "$bundle" -mindepth 1 -maxdepth 1 -type d -exec basename {} \; | sort | tr '\n' ' ')"
  if [ "$want_dirs" != "$actual_dirs" ]; then
    _fail "slices are [${actual_dirs% }], expected [${want_dirs% }]"
    return 1
  fi
  echo "    ok      slices ${want_dirs% }"

  while IFS= read -r spec; do
    slice="${spec%%:*}"
    want_arch="${spec#*:}"
    echo "--- slice $slice"
    for f in "$slice/$LIB" "$slice/Headers/module.modulemap" "$slice/Headers/$UMBRELLA"; do
      if [ -e "$bundle/$f" ]; then
        echo "    ok      $f"
      else
        _fail "missing $f"
        return 1
      fi
    done

    actual_arch="$(lipo -archs "$bundle/$slice/$LIB" 2>/dev/null | tr ' ' '\n' | grep -v '^$' | sort | tr '\n' ' ')"
    actual_arch="${actual_arch% }"
    if [ "$actual_arch" != "$want_arch" ]; then
      _fail "$slice: expected architectures [$want_arch], found [${actual_arch:-none}]"
      return 1
    fi
    echo "    ok      architectures $want_arch"

    # `nm -gjU` over a fat archive lists every member of every architecture,
    # so count distinct names.
    exports="$(nm -gjU "$bundle/$slice/$LIB" 2>/dev/null | sort -u)"
    n_rustty="$(printf '%s\n' "$exports" | grep -c '^_rustty_' || true)"
    if [ "$n_rustty" -lt "$MIN_EXPORTS" ]; then
      _fail "$slice: only $n_rustty _rustty_* exports (floor $MIN_EXPORTS) — did the C API get dropped?"
      return 1
    fi
    echo "    ok      $n_rustty _rustty_* exports (floor $MIN_EXPORTS)"

    # R2: the shipped artifact must not carry the oracle's identity.
    n_ghostty="$(printf '%s\n' "$exports" | grep -c '^_ghostty_' || true)"
    if [ "$n_ghostty" -ne 0 ]; then
      printf '%s\n' "$exports" | grep '^_ghostty_' | head -10 | sed 's/^/      /'
      _fail "$slice: $n_ghostty _ghostty_* symbol(s) leaked into the shipped framework (R2)"
      return 1
    fi
    echo "    ok      no _ghostty_* symbols (R2 identity)"

    mm="$bundle/$slice/Headers/module.modulemap"
    if ! grep -q "module $MODULE " "$mm"; then
      _fail "$slice: module.modulemap does not declare module $MODULE"
      return 1
    fi
    if ! grep -q "umbrella header \"$UMBRELLA\"" "$mm"; then
      _fail "$slice: module.modulemap's umbrella header is not \"$UMBRELLA\""
      return 1
    fi
    echo "    ok      module $MODULE, umbrella $UMBRELLA (and the file exists)"
  done < <(expected_slices "$want_ios")
  return "$fail"
}

# --- self-test ---------------------------------------------------------------
self_test() {
  command -v clang >/dev/null 2>&1 || blocked "--self-test needs clang"
  local tmp
  tmp="$(mktemp -d "${TMPDIR:-/tmp}/g-xcf-self.XXXXXX")" || exit 1
  # shellcheck disable=SC2064
  trap "rm -rf '$tmp'" EXIT

  # One C file with MIN_EXPORTS rustty_* functions, one with a stray ghostty_*.
  { for i in $(seq 1 "$MIN_EXPORTS"); do echo "void rustty_f$i(void) {}"; done; } > "$tmp/ok.c"
  { for i in $(seq 1 "$((MIN_EXPORTS - 1))"); do echo "void rustty_f$i(void) {}"; done; } > "$tmp/few.c"
  { cat "$tmp/ok.c"; echo "void ghostty_leak(void) {}"; } > "$tmp/leak.c"

  # archive <src> <clang target> <out.a>
  archive() {
    clang -target "$2" -c "$1" -o "$3.o" 2>/dev/null || return 1
    rm -f "$3"
    ar -rcs "$3" "$3.o" 2>/dev/null || return 1
  }
  # slice <bundle> <dir> <src> <targets...>: one (lipo'd) archive plus headers.
  slice() {
    local b="$1" d="$2" src="$3" t parts=()
    shift 3
    mkdir -p "$b/$d/Headers/rustty"
    for t in "$@"; do
      archive "$src" "$t" "$tmp/part-$d-$t.a" || return 1
      parts+=("$tmp/part-$d-$t.a")
    done
    if [ "${#parts[@]}" -eq 1 ]; then
      cp "${parts[0]}" "$b/$d/$LIB"
    else
      lipo -create "${parts[@]}" -output "$b/$d/$LIB" || return 1
    fi
    printf 'module %s {\n  umbrella header "%s"\n  export *\n}\n' "$MODULE" "$UMBRELLA" \
      > "$b/$d/Headers/module.modulemap"
    : > "$b/$d/Headers/$UMBRELLA"
  }
  # good <bundle>: the correct universal bundle with iOS slices.
  good() {
    mkdir -p "$1"; : > "$1/Info.plist"
    slice "$1" macos-arm64_x86_64 "$tmp/ok.c" arm64-apple-macos13 x86_64-apple-macos13 || return 1
    slice "$1" ios-arm64 "$tmp/ok.c" arm64-apple-ios13 || return 1
    slice "$1" ios-arm64-simulator "$tmp/ok.c" arm64-apple-ios13-simulator || return 1
  }

  local failures=0 n=0
  # expect <want 0=pass|1=red> <label> <bundle> <want ios>
  expect() {
    local want="$1" label="$2" out rc why
    n=$((n + 1))
    out="$(check_bundle "$3" "$4" 2>&1)"; rc=$?
    why="$(printf '%s\n' "$out" | grep '^RED:' | head -1)"
    if { [ "$want" = 0 ] && [ "$rc" = 0 ]; } || { [ "$want" = 1 ] && [ "$rc" != 0 ]; }; then
      echo "  ok    $label${why:+ — $why}"
    else
      echo "  FAIL  $label (exit $rc)"
      printf '%s\n' "$out" | sed 's/^/        /'
      failures=$((failures + 1))
    fi
  }

  good "$tmp/good" || blocked "--self-test could not build its fixtures (clang/ar/lipo)"
  expect 0 "the universal bundle with iOS slices passes" "$tmp/good" 1

  mkdir -p "$tmp/nios"; : > "$tmp/nios/Info.plist"
  slice "$tmp/nios" macos-arm64_x86_64 "$tmp/ok.c" arm64-apple-macos13 x86_64-apple-macos13
  expect 0 "without the iOS SDK, the macOS slice alone passes" "$tmp/nios" 0
  expect 1 "the iOS slices are required when the iOS SDK is present" "$tmp/nios" 1
  expect 1 "iOS slices present but not expected (no iOS SDK) is RED" "$tmp/good" 0

  mkdir -p "$tmp/old"; : > "$tmp/old/Info.plist"
  slice "$tmp/old" macos-arm64 "$tmp/ok.c" arm64-apple-macos13
  expect 1 "the old arm64-only macos-arm64 slice is RED" "$tmp/old" 0

  mkdir -p "$tmp/thin"; : > "$tmp/thin/Info.plist"
  slice "$tmp/thin" macos-arm64_x86_64 "$tmp/ok.c" arm64-apple-macos13
  expect 1 "a macOS slice missing x86_64 is RED" "$tmp/thin" 0

  good "$tmp/extra"
  slice "$tmp/extra" macos-arm64 "$tmp/ok.c" arm64-apple-macos13
  expect 1 "a stray extra slice is RED" "$tmp/extra" 1

  good "$tmp/few"
  slice "$tmp/few" ios-arm64 "$tmp/few.c" arm64-apple-ios13
  expect 1 "an iOS slice under the export floor is RED" "$tmp/few" 1

  good "$tmp/leak"
  slice "$tmp/leak" macos-arm64_x86_64 "$tmp/leak.c" arm64-apple-macos13 x86_64-apple-macos13
  expect 1 "a leaked _ghostty_* symbol is RED" "$tmp/leak" 1

  good "$tmp/umb"
  rm "$tmp/umb/ios-arm64-simulator/Headers/$UMBRELLA"
  expect 1 "an umbrella header that does not resolve is RED" "$tmp/umb" 1

  if [ "$failures" -ne 0 ]; then
    red "--self-test: $failures of $n case(s) wrong"
  fi
  echo "G-XCF SELF-TEST GREEN: $n case(s)."
  exit 0
}

if [ "${1:-}" = "--self-test" ]; then
  echo "=== G-XCF --self-test ==="
  self_test
fi

echo "=== G-XCF 1/2 :: build the framework ==="
# Rebuild unconditionally: a stale bundle would let this gate bless an artifact
# built from different sources. Same reasoning as WS19 T16. `xtask
# xcframework-vt` recreates the bundle directory, so no stale slice survives.
if ! cargo run -q -p xtask -- xcframework-vt > /dev/null 2>&1; then
  cargo run -q -p xtask -- xcframework-vt 2>&1 | tail -20 | sed 's/^/    /'
  red "xtask xcframework-vt failed"
fi
echo "    ok      $BUNDLE"

want_ios=0
ios_sdk_available && want_ios=1
echo "=== G-XCF 2/2 :: the bundle (iOS slices expected: $([ $want_ios = 1 ] && echo yes || echo no)) ==="
out="$(check_bundle "$ROOT/$BUNDLE" "$want_ios")"; rc=$?
printf '%s\n' "$out"
[ "$rc" -eq 0 ] || red "$(printf '%s\n' "$out" | grep '^RED:' | head -1 | sed 's/^RED: //')"

echo "G-XCF GREEN: $BUNDLE is well-formed ($(expected_slices "$want_ios" | cut -d: -f1 | tr '\n' ' ' | sed 's/ $//'), module $MODULE)."
exit 0
