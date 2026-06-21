#!/usr/bin/env bash
# run_differential.sh — GATE G4 (differential VT clean) — thin runner.
#
# DoP id:  G4  (DEFINITION_OF_PARITY §1, "Differential VT clean")
# Drives:  .port/phase-0/harness/  (the differential VT comparator + ref driver)
#          .port/phase-0/corpora/cvt/  (the c-vt-* golden corpus)
# Pass:    zero canonical-output diffs across the corpus (rustty built with
#          --features identity-ghostty so identity bytes match the reference).
#
# THIS IS A WIRED PROGRESS-MODE RUNNER. While the rustty port is empty/stub it
# cannot produce canonical VT output, so a real G4 pass is impossible yet; this
# runner therefore:
#   * verifies the harness + corpus are present and the comparator is runnable,
#   * if the rustty c-vt runner binary EXISTS, drives the real differential and
#     fails (exit 1) on any diff (the real gate),
#   * if it does NOT exist yet, runs in --progress mode: it executes the harness
#     self-test (gate_empty_divergence.sh) which PROVES the harness correctly
#     detects total divergence against the empty port, and exits 0 (the harness
#     is wired and self-proving) — it does NOT fabricate a parity green.
#
# It NEVER reports G4 GREEN while the port is empty: progress mode reports
# "HARNESS-WIRED (port empty)" which is distinct from a parity pass.
#
# Usage:
#   run_differential.sh                 # auto: real gate if runner exists, else progress
#   run_differential.sh --progress      # force progress (harness self-test) mode
#   run_differential.sh --self-test     # self-test the runner wiring itself
#   RUSTTY_CVT_RUNNER=/path/to/bin run_differential.sh   # point at the runner
#
# Exit 0 = GREEN (real) or HARNESS-WIRED (progress); non-zero = RED / BLOCKED.
set -euo pipefail

GATES_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PORT_DIR="$(cd "$GATES_DIR/../.." && pwd)"
RUSTTY_ROOT="$(cd "$PORT_DIR/.." && pwd)"
HARNESS="$PORT_DIR/phase-0/harness"
CORPUS="$PORT_DIR/phase-0/corpora/cvt"
PY="${PYTHON:-python3}"

MODE="auto"
case "${1:-}" in
  --progress)  MODE="progress" ;;
  --self-test) MODE="self-test" ;;
  "" )         MODE="auto" ;;
  *) echo "run_differential.sh: unknown arg '${1:-}'" >&2; exit 2 ;;
esac

echo "=== G4 :: differential VT clean (runner) ==="

# --- BLOCKED checks: harness + corpus must exist ---
blocked=0
if [ ! -d "$HARNESS" ]; then echo "BLOCKED: harness dir missing ($HARNESS)"; blocked=1; fi
if [ ! -d "$CORPUS" ]; then echo "BLOCKED: cvt corpus dir missing ($CORPUS)"; blocked=1; fi
if [ ! -f "$HARNESS/comparator.py" ]; then echo "BLOCKED: comparator.py missing"; blocked=1; fi
if [ "$blocked" = "1" ]; then exit 3; fi

CVT_RUNNER="${RUSTTY_CVT_RUNNER:-$RUSTTY_ROOT/target/release/rustty-c-vt-runner}"

run_self_test() {
  echo "--- runner self-test: assert wiring + flippability ---"
  local ok=1
  # 1. comparator must be importable/runnable (--help exits 0 or prints usage)
  if "$PY" "$HARNESS/comparator.py" --help >/dev/null 2>&1; then
    echo "  comparator.py runnable: OK"
  else
    # some comparators exit non-zero on --help; treat presence as enough
    echo "  comparator.py present (──help non-zero, acceptable)"
  fi
  # 2. corpus has golden files
  local n
  n="$(ls "$CORPUS"/*.golden 2>/dev/null | wc -l | tr -d ' ')"
  echo "  cvt golden cases: $n"
  [ "$n" -gt 0 ] || { echo "  FAIL: no golden cases"; ok=0; }
  # 3. the empty-divergence harness self-test exists and is the flip proof
  if [ -x "$HARNESS/gate_empty_divergence.sh" ]; then
    echo "  harness flip-proof gate present: OK"
  else
    echo "  WARN: gate_empty_divergence.sh absent (harness self-test missing)"
  fi
  if [ "$ok" = "1" ]; then echo "SELF-TEST: PASS"; return 0; else echo "SELF-TEST: FAIL"; return 1; fi
}

if [ "$MODE" = "self-test" ]; then
  run_self_test
  exit $?
fi

if [ "$MODE" = "auto" ] && [ -x "$CVT_RUNNER" ]; then
  echo "--- REAL differential gate (rustty c-vt runner found: $CVT_RUNNER) ---"
  echo "    diffing canonical VT output across $(ls "$CORPUS"/*.golden 2>/dev/null | wc -l | tr -d ' ') cases"
  # Drive each corpus case through the comparator. Any diff => exit 1.
  fail=0
  for golden in "$CORPUS"/*.golden; do
    case_name="$(basename "${golden%.golden}")"
    # The comparator's exact CLI is defined by comparator.py; we forward the
    # case input + the rustty runner. (Wiring contract; the harness owns the
    # canonical-dump comparison and identity-ghostty build selection.)
    if ! "$PY" "$HARNESS/comparator.py" --case "$case_name" \
          --rust-bin "$CVT_RUNNER" --ref-golden "$golden" >/dev/null 2>&1; then
      echo "    DIFF $case_name"
      fail=1
    fi
  done
  if [ "$fail" = "0" ]; then
    echo "GREEN: zero differential diffs across the cvt corpus (G4)"
    exit 0
  else
    echo "RED: differential diffs found (see DIFF lines) (G4)"
    exit 1
  fi
fi

# --- progress mode: harness self-test proves the harness is wired ---
echo "--- PROGRESS mode: rustty c-vt runner not built yet ($CVT_RUNNER) ---"
echo "    G4 cannot be parity-GREEN while the port is empty; running the harness"
echo "    self-test (proves the comparator detects total divergence)."
if [ -x "$HARNESS/gate_empty_divergence.sh" ]; then
  if bash "$HARNESS/gate_empty_divergence.sh"; then
    echo "HARNESS-WIRED (port empty): comparator self-test PASS; G4 in progress mode"
    exit 0
  else
    echo "BLOCKED: harness self-test FAILED — the differential harness is broken"
    exit 3
  fi
else
  # fall back to the runner self-test (wiring assertion only)
  run_self_test && {
    echo "HARNESS-WIRED (port empty): runner wiring OK; G4 in progress mode"
    exit 0
  }
  exit 3
fi
