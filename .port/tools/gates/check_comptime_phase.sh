#!/usr/bin/env bash
# check_comptime_phase.sh — GATE D4 (comptime-phase preservation + golden VT
# bytes) — thin runner.
#
# Detector: D4 (.port/phase-2/BUN_PORT_LEARNINGS.md §4). Kills Bun regression
# Class 3 (oven-sh/bun#30693): a comptime-string builder ported to a runtime
# function let an OSC 8 ST (ESC \) collide with the tag grammar and corrupt
# output. VERY-HIGH likelihood for this codebase (ghostty comptime-builds
# CSI/SGR/DCS/OSC responses; the data domain IS escape sequences).
#
# This runner is the executable half of D4(b). It:
#   1. asserts the D4 checklist (check_comptime_phase.md) is present (the
#      completeness-critic half, D4(a)),
#   2. asserts each covered crate ships its byte-exact golden + ST-integrity
#      test file (crates/<crate>/tests/d4_golden_vt_bytes.rs), and
#   3. runs those golden tests. Any golden/ST-integrity failure is RED.
#
# Exit codes (matches ci.sh's run_gate contract):
#   0  -> PASS  (checklist + test files present; golden tests green)
#   3  -> BLOCKED (cargo unavailable / skipped) -> WARNING, never a silent green
#   other non-zero -> FAIL (missing checklist/test file, or a golden test failed)
#
# Usage:
#   check_comptime_phase.sh                 # full: static presence + cargo test
#   check_comptime_phase.sh --static-only   # presence checks only (no cargo)
#   check_comptime_phase.sh --self-test     # self-test the runner wiring
set -uo pipefail

GATES_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PORT_DIR="$(cd "$GATES_DIR/../.." && pwd)"
RUSTTY_ROOT="$(cd "$PORT_DIR/.." && pwd)"

CHECKLIST="$GATES_DIR/check_comptime_phase.md"

# Crates that emit VT bytes and MUST carry a D4 golden test file. Extend this
# list when a new VT-emitting crate lands (renderer/apprt/termio title reports,
# etc.), per check_comptime_phase.md.
COVERED_CRATES=(
  "rustty-vt"
)
GOLDEN_TEST_REL="tests/d4_golden_vt_bytes.rs"

MODE="full"
case "${1:-}" in
  --static-only) MODE="static" ;;
  --self-test)   MODE="self-test" ;;
  "" )           MODE="full" ;;
  *) echo "check_comptime_phase.sh: unknown arg '${1:-}'" >&2; exit 2 ;;
esac

echo "=== D4 :: comptime-phase preservation + golden VT bytes (runner) ==="

fail=0

check_presence() {
  echo "--- presence checks ---"
  if [ -f "$CHECKLIST" ]; then
    echo "  OK   checklist present: $CHECKLIST"
  else
    echo "  FAIL checklist missing: $CHECKLIST (D4(a) completeness-critic item)"
    fail=1
  fi

  # The golden test file must exist AND actually contain the ST-integrity
  # helper + at least one byte-exact golden assertion (guards against an empty
  # or renamed stub that would vacuously pass — R5).
  for crate in "${COVERED_CRATES[@]}"; do
    local tf="$RUSTTY_ROOT/crates/$crate/$GOLDEN_TEST_REL"
    if [ ! -f "$tf" ]; then
      echo "  FAIL golden test missing: $tf"
      fail=1
      continue
    fi
    if ! grep -q 'fn st_integrity' "$tf"; then
      echo "  FAIL $tf lacks the st_integrity helper (D4(b))"
      fail=1
    elif ! grep -q 'assert_st_integrity' "$tf"; then
      echo "  FAIL $tf lacks any assert_st_integrity call (D4(b))"
      fail=1
    elif ! grep -Eq 'assert_eq!|fn golden' "$tf"; then
      echo "  FAIL $tf lacks byte-exact golden assertions (D4(b))"
      fail=1
    else
      echo "  OK   golden+ST test present: crates/$crate/$GOLDEN_TEST_REL"
    fi
  done
}

run_golden_tests() {
  echo "--- golden byte tests (byte-exact + ST-integrity) ---"
  if ! command -v cargo >/dev/null 2>&1; then
    echo "  BLOCKED: cargo not on PATH; cannot run the golden tests here."
    return 3
  fi
  if [ "${CI_SKIP_CARGO:-0}" = "1" ]; then
    echo "  BLOCKED: CI_SKIP_CARGO=1; golden tests deferred to the cargo lane."
    return 3
  fi
  local rc=0
  for crate in "${COVERED_CRATES[@]}"; do
    echo "  cargo test -p $crate --test d4_golden_vt_bytes"
    if ! ( cd "$RUSTTY_ROOT" && cargo test -p "$crate" --test d4_golden_vt_bytes ); then
      echo "  FAIL golden tests RED for $crate"
      rc=1
    fi
  done
  return $rc
}

run_self_test() {
  echo "--- runner self-test ---"
  local ok=1
  [ -f "$CHECKLIST" ] || { echo "  self-test FAIL: checklist path wrong"; ok=0; }
  for crate in "${COVERED_CRATES[@]}"; do
    [ -f "$RUSTTY_ROOT/crates/$crate/$GOLDEN_TEST_REL" ] \
      || { echo "  self-test FAIL: golden test path wrong for $crate"; ok=0; }
  done
  if [ "$ok" = "1" ]; then echo "  self-test PASS: paths resolve"; exit 0; fi
  exit 1
}

case "$MODE" in
  self-test) run_self_test ;;
  static)
    check_presence
    [ "$fail" -eq 0 ] && { echo ">> PASS (static): D4 checklist + golden files present"; exit 0; }
    echo ">> FAIL (static): D4 presence checks failed" >&2; exit 1
    ;;
  full)
    check_presence
    if [ "$fail" -ne 0 ]; then
      echo ">> FAIL: D4 presence checks failed" >&2
      exit 1
    fi
    run_golden_tests; trc=$?
    if [ "$trc" -eq 3 ]; then
      echo ">> BLOCKED: D4 golden tests could not run here (surfaced, not greened)"
      exit 3
    elif [ "$trc" -ne 0 ]; then
      echo ">> FAIL: D4 golden byte / ST-integrity tests RED" >&2
      exit 1
    fi
    echo ">> PASS: D4 checklist present; golden byte + ST-integrity tests green"
    exit 0
    ;;
esac
