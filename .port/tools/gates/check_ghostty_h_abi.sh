#!/usr/bin/env bash
# check_ghostty_h_abi.sh — the ghostty.h ABI freeze gate (Task A3, I9).
#
# Branch A (PREPORT_SWIFT_APP §3): the xcframework's public header IS the upstream
# `include/ghostty.h` VERBATIM, so the reused Swift app's `#import` + every
# `ghostty_*_s/_u/_e` typedef resolves unchanged, and the port's `#[repr(C)]`
# types must match that header field-for-field. This gate proves both halves:
#
#   1. HEADER BYTE-IDENTITY. The pinned copy checked into rustty
#      (`include/ghostty.h`) must be byte-identical to the upstream oracle
#      (`$ORACLE/include/ghostty.h`). This is what freezes the ABI: an upstream
#      re-sync that changes a struct, adds an export, or "fixes" one of the
#      preserved typos (R-A `GHOSTTY_POINT_SURFACE`↔`history`, R-B
#      `chostty_ipc_target_s`, R-C `timetime_ms`) trips the diff so a human
#      reconciles it deliberately, never silently. If B1 has staged the header
#      into the built xcframework, that staged copy is diffed against the pin too.
#
#   2. RUST LAYOUT PARITY. `cargo test -p rustty-apprt-c --test abi_layout` runs
#      the `const size_of/align_of` + enum-ordinal asserts that pin every ABI
#      struct/union/enum to the header's C layout. (Drift is ALSO a compile
#      failure of that target — the asserts are `const` — so this both builds and
#      runs it.)
#
# Exit codes (per ci.sh run_gate contract):
#   0  PASS
#   3  BLOCKED (oracle checkout / cargo / diff unavailable) -> WARNING, not green
#   1  FAIL    (header drifted from the pinned upstream, or a Rust layout assert
#              failed -> an R1 ABI bug: fix the #[repr(C)] type or reconcile the
#              header pin)
#
# Registered PROGRESS in ci.sh (R5): brand-new, and the pin tracks a moving
# upstream oracle, so a legitimate upstream re-sync should surface as a RED to be
# reconciled rather than wedge CI un-passable. A genuine RED (non-0/non-3) is
# still escalated to a HARD failure by ci.sh's run_gate for a PROGRESS gate.

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
ORACLE="${GHOSTTY_ORACLE:-/Users/gael/projects/ghostty}"

PIN="$ROOT/include/ghostty.h"                 # rustty's frozen app header (shipped)
UPSTREAM="$ORACLE/include/ghostty.h"          # the read-only oracle (source of truth)

note() { printf '  %s\n' "$*"; }

# --- tooling / inputs (BLOCKED, not FAIL, if absent) ------------------------
command -v diff  >/dev/null 2>&1 || { echo "BLOCKED: diff unavailable";  exit 3; }
command -v cargo >/dev/null 2>&1 || { echo "BLOCKED: cargo unavailable"; exit 3; }
if [ ! -f "$UPSTREAM" ]; then
  echo "BLOCKED: upstream oracle header not found at $UPSTREAM"
  echo "         (set GHOSTTY_ORACLE to the read-only ghostty checkout)"
  exit 3
fi
if [ ! -f "$PIN" ]; then
  echo "BLOCKED: pinned header not found at $PIN (A3 not yet seeded here)"
  exit 3
fi

fail=0

echo "ghostty.h ABI freeze gate (A3)"
echo "  pin      : $PIN"
echo "  upstream : $UPSTREAM"

# --- 1. header byte-identity (preserves R-A/R-B/R-C typos verbatim) ---------
if diff -u "$UPSTREAM" "$PIN" >/tmp/ghostty_h_abi.diff 2>&1; then
  echo ">> header: pin is BYTE-IDENTICAL to the upstream oracle."
else
  note "HEADER DRIFT: $PIN differs from the upstream oracle. Diff:"
  sed 's/^/    /' /tmp/ghostty_h_abi.diff
  note "Reconcile deliberately: re-pin (cp \$ORACLE/include/ghostty.h $PIN) only"
  note "after confirming the ABI change is intended, and update abi_layout.rs."
  fail=1
fi

# The three PRESERVED upstream typos MUST survive in the pin (a well-meaning
# 'fix' to any of them is itself ABI drift under Branch A). Assert directly.
for tok in "GHOSTTY_POINT_SURFACE" "chostty_ipc_target_s" "timetime_ms"; do
  if ! grep -qF -- "$tok" "$PIN"; then
    note "PRESERVED-TYPO MISSING: '$tok' not in the pin (do not 'fix' upstream typos)"
    fail=1
  fi
done

# --- 1b. if B1 staged the header into the xcframework, diff that too --------
# The default location mirrors the oracle xcframework layout; overridable.
STAGED="${GHOSTTY_STAGED_HEADER:-$ROOT/dist/GhosttyKit.xcframework}"
staged_found=0
if [ -e "$STAGED" ]; then
  # Accept either a direct file path or the xcframework dir (glob its slices).
  if [ -f "$STAGED" ]; then
    STAGED_HEADERS="$STAGED"
  else
    STAGED_HEADERS="$(find "$STAGED" -path '*/Headers/ghostty.h' 2>/dev/null)"
  fi
  for h in $STAGED_HEADERS; do
    staged_found=1
    if diff -u "$PIN" "$h" >/dev/null 2>&1; then
      echo ">> staged xcframework header BYTE-IDENTICAL to the pin: $h"
    else
      note "STAGED HEADER DRIFT: $h differs from the pin (re-stage from A3's pin)"
      fail=1
    fi
  done
fi
[ "$staged_found" -eq 0 ] && echo "   (no staged xcframework header yet — B1 not built; skipped)"

# --- 2. Rust layout parity (const size/align/ordinal asserts + runtime tests) -
echo
echo "Running: cargo test -p rustty-apprt-c --test abi_layout"
if cargo test -p rustty-apprt-c --test abi_layout; then
  echo ">> abi_layout: Rust #[repr(C)] layouts match the header."
else
  rc=$?
  note "abi_layout FAILED (exit $rc): a Rust struct/union/enum layout no longer"
  note "matches ghostty.h. This is an R1 ABI bug — fix the #[repr(C)] type to"
  note "match the header's field types (see the failing assertion's C name)."
  fail=1
fi

echo
if [ "$fail" -ne 0 ]; then
  echo ">> A3 FAIL: ghostty.h ABI freeze violated (header drift or layout mismatch)."
  exit 1
fi
echo ">> A3 PASS: header byte-identical to upstream + all Rust ABI layouts pinned."
exit 0
