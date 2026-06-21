#!/usr/bin/env bash
# build_port_driver.sh — Compile the PORT-side VT oracle (the SAME vt_driver.c
# the reference uses) against rustty's own librustty-vt static library, built
# with the `identity-ghostty` parity profile (R2).
#
# This is the sibling of build_ref_driver.sh. Both compile the IDENTICAL C
# source (ref_driver/vt_driver.c) so the only thing that differs between the two
# binaries is which VT implementation they link:
#   * vt_driver_ref  -> ghostty's zig-out/libghostty-vt.a   (the external oracle)
#   * vt_driver_port -> rustty's  librustty-vt.a            (the port under test)
#
# IMPORTANT (R3): rustty NEVER depends on the ghostty tree as a build/run dep.
# This script reads the ghostty include layout ONLY to satisfy `#include
# <ghostty/vt.h>` in the shared C source — and ONLY because, under the
# `identity-ghostty` profile (R2), rustty's cbindgen header is emitted with the
# reference's identity (header path `ghostty/vt.h`, `ghostty_*` symbols) so the
# one C source compiles unchanged against either library and the symbols resolve.
# When rustty-vt-c emits its own header (cbindgen) under identity-ghostty, point
# RUSTTY_VT_INCLUDE at it instead; the ghostty include dir is only the fallback
# that proves the ABI is reference-identical under the parity profile.
#
# CURRENT STATE: rustty-vt-c is a SCAFFOLD STUB — it exports no C ABI symbols and
# produces no usable librustty-vt.a. This script therefore FAILS LOUDLY with a
# precise blocker (it does NOT fabricate a binary). That is correct: with no port
# driver, the EMPTY-WORKSPACE GATE keeps measuring 100% divergence, which is the
# soundness control. As rustty-vt-c lands the real C ABI (break #4), this script
# starts producing a real bin/vt_driver_port and the gate begins to flip toward
# 0% (see GATE.md).
#
# NOTE: in this environment `cc` is aliased to the Claude CLI, so we resolve a
# real C compiler explicitly (xcrun clang on macOS), exactly like the ref build.
set -euo pipefail

HARNESS_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RUSTTY_ROOT="${RUSTTY_ROOT:-/Users/gael/projects/rustty}"
OUT="$HARNESS_DIR/bin/vt_driver_port"

# --- Locate rustty's librustty-vt.a (built from rustty-vt-c, staticlib). ------
# Cargo emits staticlibs under target/<profile>/. We accept an explicit override
# and otherwise probe the conventional locations.
LIBA="${RUSTTY_VT_LIB:-}"
if [ -z "$LIBA" ]; then
  for cand in \
    "$RUSTTY_ROOT/target/release/librustty_vt_c.a" \
    "$RUSTTY_ROOT/target/release/librustty-vt.a" \
    "$RUSTTY_ROOT/target/debug/librustty_vt_c.a" \
    "$RUSTTY_ROOT/target/debug/librustty-vt.a"; do
    if [ -f "$cand" ]; then LIBA="$cand"; break; fi
  done
fi

# --- Locate the cbindgen-emitted header dir (identity-ghostty layout). --------
# Under the parity profile the header is `ghostty/vt.h` with ghostty_* symbols,
# so the shared vt_driver.c compiles unchanged. Default to rustty's own emitted
# include dir; fall back to the reference include layout (read-only) which, under
# identity-ghostty, is byte-identical to what cbindgen emits for the port.
INC="${RUSTTY_VT_INCLUDE:-}"
if [ -z "$INC" ]; then
  for cand in \
    "$RUSTTY_ROOT/target/include" \
    "$RUSTTY_ROOT/include" \
    "/Users/gael/projects/ghostty/zig-out/include"; do
    if [ -f "$cand/ghostty/vt.h" ]; then INC="$cand"; break; fi
  done
fi

# --- Build rustty-vt-c if no staticlib is present yet (best-effort). ----------
# This is the wiring that "calls the rustty vt driver once rustty-vt-c exists":
# when the crate produces real C ABI, `cargo build` materializes the staticlib.
if [ -z "$LIBA" ]; then
  if command -v cargo >/dev/null 2>&1 && [ -f "$RUSTTY_ROOT/Cargo.toml" ]; then
    echo "build_port_driver: no librustty-vt.a found; attempting cargo build of rustty-vt-c (identity-ghostty)..." >&2
    # The `identity-ghostty` feature must reach rustty-identity through the dep
    # graph. Once rustty-vt-c re-exports it, this is `--features identity-ghostty`.
    ( cd "$RUSTTY_ROOT" && cargo build --release -p rustty-vt-c --features identity-ghostty ) >&2 2>&1 || \
      echo "build_port_driver: cargo build did not produce a usable C ABI (expected while rustty-vt-c is a stub)." >&2
    for cand in \
      "$RUSTTY_ROOT/target/release/librustty_vt_c.a" \
      "$RUSTTY_ROOT/target/release/librustty-vt.a"; do
      if [ -f "$cand" ]; then LIBA="$cand"; break; fi
    done
  fi
fi

# --- Honest blocker if prerequisites are absent (no fabrication, R5). ---------
if [ -z "$LIBA" ] || [ ! -f "$LIBA" ]; then
  cat >&2 <<EOF
build_port_driver: BLOCKED — no rustty librustty-vt.a available.
  rustty-vt-c does not yet emit a linkable C ABI (it is a scaffold stub).
  This is EXPECTED in Phase 0/1: with no port driver, the empty-workspace gate
  keeps measuring 100% divergence (the soundness control). When break #4 lands
  the real C ABI, re-run this script (or set RUSTTY_VT_LIB) to produce
  bin/vt_driver_port and begin flipping the gate toward 0% (see GATE.md).
  Probed:
    RUSTTY_VT_LIB=${RUSTTY_VT_LIB:-<unset>}
    target/{release,debug}/librustty[_-]vt[_c].a
EOF
  exit 11
fi

if [ -z "$INC" ] || [ ! -f "$INC/ghostty/vt.h" ]; then
  cat >&2 <<EOF
build_port_driver: BLOCKED — no C ABI header (ghostty/vt.h) found.
  Under the identity-ghostty profile (R2) rustty-vt-c's cbindgen header is
  emitted with the reference identity so the shared vt_driver.c compiles
  unchanged. Set RUSTTY_VT_INCLUDE to the cbindgen output dir.
  Probed:
    RUSTTY_VT_INCLUDE=${RUSTTY_VT_INCLUDE:-<unset>}
    target/include, include/, ghostty/zig-out/include
EOF
  exit 12
fi

# --- Resolve a real C compiler (avoid the `cc` alias), like the ref build. ----
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

mkdir -p "$HARNESS_DIR/bin"

echo "CC=$CC"
echo "INC=$INC"
echo "LIBA=$LIBA   (rustty port, identity-ghostty)"
echo "OUT=$OUT"

# Same compile/link recipe as the reference, only the linked library differs.
# A symbol-less scaffold staticlib (rustty-vt-c stub) makes this fail at link;
# that is the honest blocker — we surface it precisely and DO NOT emit a binary.
if ! "$CC" -std=c11 -O2 -g \
  "${SYSROOT_FLAG[@]}" \
  -I"$INC" \
  "$HARNESS_DIR/ref_driver/vt_driver.c" \
  "$LIBA" \
  -framework CoreFoundation -framework CoreText -framework CoreGraphics \
  -framework Foundation \
  -o "$OUT" 2>"$HARNESS_DIR/bin/.port_driver_link.log"; then
  cat "$HARNESS_DIR/bin/.port_driver_link.log" >&2
  rm -f "$OUT"
  cat >&2 <<EOF
build_port_driver: BLOCKED — link against $LIBA failed.
  The most common cause in Phase 0/1 is that rustty-vt-c is still a SCAFFOLD: it
  builds a staticlib but exports ZERO C ABI symbols (no ghostty_terminal_new /
  ghostty_formatter_* under the identity-ghostty profile), so the shared
  vt_driver.c cannot link. This is EXPECTED and CORRECT: with no working port
  driver, the empty-workspace gate keeps measuring 100% divergence (the soundness
  control). When break #4 lands the real cbindgen C ABI, this link succeeds and
  bin/vt_driver_port appears, flipping the gate toward 0% (see GATE.md).
EOF
  exit 13
fi

echo "OK: built $OUT"
"$OUT" --help >/dev/null 2>&1 || true
file "$OUT" 2>/dev/null || true
