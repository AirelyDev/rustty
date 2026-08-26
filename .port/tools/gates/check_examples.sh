#!/usr/bin/env bash
# GATE G-EXAMPLES — build (and where possible run) every example/ program.
#
# PORTS: ghostty `.github/workflows/test.yml`'s `list-examples` +
# `build-examples-zig` / `build-examples-cmake` / `build-examples-swift` jobs.
# The reference enumerates example/ at CI time and builds each one; rustty
# checked in 32 example programs and built none of them.
#
# example/README.md already claims "CI discovers examples by globbing
# `example/*/build.sh`, so a new example needs no workflow edit" — this gate is
# what makes that sentence true. It globs, so adding an example needs no edit
# here either.
#
# FOUR FAMILIES, discovered not listed:
#   example/*/build.sh          compile the C/C++ sources against the published
#                               artifacts and RUN the result. 24 of them.
#   example/c-vt-cmake*/        CMakeLists-driven; they demonstrate the
#                               FetchContent/find_package integration itself,
#                               so they are configured + built through cmake.
#                               The -cross one needs a non-host Rust target
#                               installed and reports skipped without it.
#   swift-vt-xcframework/       SwiftPM against dist/rustty-vt.xcframework;
#                               macOS-only, and it rebuilds the framework first
#                               so a stale one cannot produce a false green.
#   example/wasm-*/             browser lanes; covered by ci.sh's opt-in
#                               `--webgl` lane, not here. Listed as skipped so
#                               the omission is visible.
#
# Exit contract (`.port/tools/CI.md`): 0 = pass, 1 = red, 3 = BLOCKED.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$ROOT" || exit 1

command -v cargo >/dev/null 2>&1 || { echo "G-EXAMPLES BLOCKED: cargo not installed"; exit 3; }

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

RAN=0
FAILED=0
SKIPPED=""
FAILED_NAMES=""

ok()   { echo "  ok      $1"; RAN=$((RAN+1)); }
bad()  { echo "  FAIL    $1"; RAN=$((RAN+1)); FAILED=1; FAILED_NAMES="$FAILED_NAMES $1"; }
skip() { echo "  skipped $1 ($2)"; SKIPPED="$SKIPPED $1"; }

# --- family 1: build.sh examples --------------------------------------------
echo "=== G-EXAMPLES 1/4 :: example/*/build.sh (compile + run) ==="
found_sh=0
for f in "$ROOT"/example/*/build.sh; do
  [ -f "$f" ] || continue
  found_sh=$((found_sh+1))
  name="$(basename "$(dirname "$f")")"
  if sh "$f" > "$WORK/$name.log" 2>&1; then
    ok "$name"
  else
    tail -20 "$WORK/$name.log" | sed 's/^/          /'
    bad "$name"
  fi
done
if [ "$found_sh" -eq 0 ]; then
  echo "G-EXAMPLES BLOCKED: no example/*/build.sh matched — the glob this gate"
  echo "  and example/README.md both rely on has stopped finding anything."
  exit 3
fi

# --- family 2: CMake examples ------------------------------------------------
echo "=== G-EXAMPLES 2/4 :: example/c-vt-cmake* (cmake configure + build) ==="
if ! command -v cmake >/dev/null 2>&1; then
  skip "c-vt-cmake*" "cmake not installed"
else
  for d in "$ROOT"/example/c-vt-cmake*/; do
    [ -f "$d/CMakeLists.txt" ] || continue
    name="$(basename "$d")"

    # The cross example builds for a NON-host triple; without that target
    # installed via rustup it cannot work, and reporting red for a missing
    # toolchain would be a lie about the example.
    extra=()
    if [ "$name" = "c-vt-cmake-cross" ]; then
      # Mirror the example's own RUST_TARGET derivation (its CMakeLists.txt
      # normalises arm64/ARM64 -> aarch64 and AMD64 -> x86_64, then picks an
      # OS different from the host).
      arch="$(uname -m)"
      case "$arch" in
        x86_64|AMD64)         arch="x86_64" ;;
        aarch64|arm64|ARM64)  arch="aarch64" ;;
      esac
      case "$(uname -s)" in
        Linux)  want="$arch-pc-windows-gnu" ;;
        Darwin) want="$arch-unknown-linux-gnu" ;;
        *)      want="" ;;
      esac
      if [ -z "$want" ] || ! rustup target list --installed 2>/dev/null | grep -qx "$want"; then
        skip "$name" "rust target ${want:-unknown} not installed (rustup target add $want)"
        continue
      fi
      extra=(-DRUST_TARGET="$want")
    fi

    bd="$WORK/build-$name"
    # `${arr[@]+"${arr[@]}"}`, not `"${arr[@]}"`: bash 3.2 (what macOS ships)
    # treats an empty array as unset under `set -u` and aborts.
    if cmake -B "$bd" -S "$d" -DFETCHCONTENT_SOURCE_DIR_RUSTTY="$ROOT" ${extra[@]+"${extra[@]}"} \
         > "$WORK/$name.log" 2>&1 \
       && cmake --build "$bd" >> "$WORK/$name.log" 2>&1; then
      # Run it when it is a host binary. The cross example produces a real
      # executable for another OS, so it is built and inspected but not run.
      if [ "$name" = "c-vt-cmake-cross" ]; then
        bin="$(find "$bd" -maxdepth 1 -type f -name 'c_vt_cmake_cross*' 2>/dev/null | head -1)"
        if [ -n "$bin" ]; then
          ok "$name -> $(file -b "$bin" 2>/dev/null | cut -c1-60)"
        else
          bad "$name (no cross executable produced)"
        fi
      else
        bin="$(find "$bd" -maxdepth 1 -type f -perm -u+x ! -name '*.cmake' ! -name 'Makefile' 2>/dev/null | head -1)"
        if [ -n "$bin" ] && "$bin" > "$WORK/$name.run.log" 2>&1; then
          ok "$name -> $(head -1 "$WORK/$name.run.log")"
        elif [ -n "$bin" ]; then
          tail -10 "$WORK/$name.run.log" | sed 's/^/          /'
          bad "$name (built but did not run)"
        else
          ok "$name (built; no runnable binary found)"
        fi
      fi
    else
      tail -20 "$WORK/$name.log" | sed 's/^/          /'
      bad "$name"
    fi
  done
fi

# --- family 3: the SwiftPM example -------------------------------------------
#
# PORTS: the reference's `build-examples-swift` job. Needs the VT XCFramework,
# which `xtask xcframework-vt` builds — and that needs Xcode, so this lane is
# macOS-only and reports skipped elsewhere BY NAME rather than vanishing.
echo "=== G-EXAMPLES 3/4 :: example/swift-vt-xcframework ==="
swiftdir="$ROOT/example/swift-vt-xcframework"
if [ ! -f "$swiftdir/Package.swift" ]; then
  skip "swift-vt-xcframework" "not present"
elif [ "$(uname -s)" != "Darwin" ]; then
  skip "swift-vt-xcframework" "macOS-only (SwiftPM + xcodebuild)"
elif ! command -v swift >/dev/null 2>&1; then
  skip "swift-vt-xcframework" "swift not installed"
elif ! command -v xcodebuild >/dev/null 2>&1; then
  skip "swift-vt-xcframework" "xcodebuild not installed"
else
  # Rebuild the framework unconditionally, for the reason WS19 T16 gave for the
  # example build.sh scripts: a stale dist/rustty-vt.xcframework would let this
  # lane link an XCFramework built from different sources and report green.
  if ! (cd "$ROOT" && cargo run -q -p xtask -- xcframework-vt) > "$WORK/xcf-vt.log" 2>&1; then
    tail -20 "$WORK/xcf-vt.log" | sed 's/^/          /'
    bad "swift-vt-xcframework (xtask xcframework-vt failed)"
  elif ! (cd "$swiftdir" && swift build) > "$WORK/swift.log" 2>&1; then
    tail -20 "$WORK/swift.log" | sed 's/^/          /'
    bad "swift-vt-xcframework (swift build)"
  elif ! (cd "$swiftdir" && swift run) > "$WORK/swift-run.log" 2>&1; then
    tail -20 "$WORK/swift-run.log" | sed 's/^/          /'
    bad "swift-vt-xcframework (swift run)"
  else
    ok "swift-vt-xcframework -> $(grep -m1 'Hello from' "$WORK/swift-run.log" || echo 'ran')"
  fi
fi

# --- family 4: wasm examples -------------------------------------------------
echo "=== G-EXAMPLES 4/4 :: example/wasm-* ==="
for d in "$ROOT"/example/wasm-*/; do
  [ -d "$d" ] || continue
  skip "$(basename "$d")" "browser lane — run 'bash .port/tools/ci.sh --webgl'"
done

echo "--- G-EXAMPLES summary: $RAN built, $(printf '%s' "$SKIPPED" | wc -w | tr -d ' ') skipped ---"
if [ "$FAILED" -ne 0 ]; then
  echo "G-EXAMPLES RED:$FAILED_NAMES"
  exit 1
fi
echo "G-EXAMPLES GREEN ($RAN example(s) built).${SKIPPED:+ Not built:$SKIPPED}"
exit 0
