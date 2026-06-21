#!/usr/bin/env bash
# gate_empty_divergence.sh — THE EMPTY-WORKSPACE GATE.
#
# Purpose: PROVE the differential harness correctly detects TOTAL divergence.
# It runs the comparator over a corpus of VT fixtures against the CURRENT
# (empty) rustty port, and asserts the harness reports ~100% divergence on
# every case — because the empty port produces no canonical output at all.
#
# This is a *harness self-test*, not a parity gate: it must PASS now (while
# rustty is empty) and is expected to FAIL/flip once the port starts producing
# real output (at which point the real parity gates take over). It also runs a
# control (reference-vs-reference) that MUST report 0% divergence, proving the
# metric is not trivially stuck at 100%.
#
# Pass criteria (ALL must hold):
#   1. CONTROL: reference vs reference over the corpus => 0.0000% mean divergence
#      (identical on every case). If this is not 0, the metric is broken.
#   2. EMPTY GATE: reference vs empty port over the corpus => mean byte
#      divergence >= THRESHOLD (default 99.9%); and EVERY non-empty-output case
#      reaches exactly 100% (port emits nothing).
#
# Output: prints per-case and aggregate divergence %, then PASS/FAIL.
# Exit 0 if the gate holds; non-zero otherwise.
set -euo pipefail

HARNESS_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PY="${PYTHON:-python3}"
REF="$HARNESS_DIR/bin/vt_driver_ref"
PORT="$HARNESS_DIR/port_hook/vt_driver_port.sh"
FIXTURES_DIR="$HARNESS_DIR/fixtures"
RESULTS="$HARNESS_DIR/results"
THRESHOLD="${EMPTY_DIVERGENCE_THRESHOLD:-99.9}"

mkdir -p "$RESULTS"

if [ ! -x "$REF" ]; then
  echo "GATE ERROR: reference driver not built ($REF)." >&2
  echo "  -> run: bash $HARNESS_DIR/build_ref_driver.sh" >&2
  exit 11
fi

# Collect non-empty fixtures for the divergence corpus. We deliberately also
# keep empty.vt around but EXCLUDE it from the headline metric (ref output is
# itself empty there, so ref==port==empty => 0% divergence, which is correct but
# uninformative for a "total divergence" proof).
# (bash 3.2 compatible: no mapfile.)
FIXTURES=()
while IFS= read -r line; do
  [ -n "$line" ] && FIXTURES+=("$line")
done < <(find "$FIXTURES_DIR" -maxdepth 1 -name '*.vt' ! -name 'empty.vt' | sort)
if [ "${#FIXTURES[@]}" -eq 0 ]; then
  echo "GATE ERROR: no fixtures in $FIXTURES_DIR" >&2
  exit 12
fi

echo "=================================================================="
echo " EMPTY-WORKSPACE GATE  (rustty port is empty)"
echo " reference: $REF"
echo " port:      $PORT  (stub)"
echo " fixtures:  ${#FIXTURES[@]}  threshold: ${THRESHOLD}%"
echo "=================================================================="

run_pct() {  # $1=ref $2=port $3=fixture ; echoes byte_divergence_pct
  # comparator.py exits 1 when output diverges (the normal case here), which is
  # expected; capture its JSON regardless of exit code (do not let set -e abort).
  local json
  json="$("$PY" "$HARNESS_DIR/comparator.py" --json --ref "$1" --port "$2" "$3" || true)"
  printf '%s' "$json" | "$PY" -c 'import sys,json; print(json.load(sys.stdin)["byte_divergence_pct"])'
}

# --- CONTROL: reference vs reference (must be 0%) ---
echo
echo "--- CONTROL: reference vs reference (expect 0% on every case) ---"
ctrl_sum=0; ctrl_n=0; ctrl_bad=0
for fx in "${FIXTURES[@]}"; do
  pct="$(run_pct "$REF" "$REF" "$fx")"
  printf "  %-28s %8s%%\n" "$(basename "$fx")" "$pct"
  ctrl_sum="$($PY -c "print($ctrl_sum + $pct)")"
  ctrl_n=$((ctrl_n + 1))
  if [ "$($PY -c "print(1 if $pct > 0.0 else 0)")" = "1" ]; then ctrl_bad=$((ctrl_bad+1)); fi
done
ctrl_mean="$($PY -c "print(round($ctrl_sum/$ctrl_n, 4))")"
echo "  CONTROL mean divergence = ${ctrl_mean}%   (bad cases: $ctrl_bad)"

# --- EMPTY GATE: reference vs empty port (must be ~100%) ---
echo
echo "--- EMPTY GATE: reference vs empty port (expect ~100% on every case) ---"
gate_sum=0; gate_n=0; gate_below=0
{
  echo "fixture,byte_divergence_pct"
} > "$RESULTS/empty_gate.csv"
for fx in "${FIXTURES[@]}"; do
  pct="$(run_pct "$REF" "$PORT" "$fx")"
  printf "  %-28s %8s%%\n" "$(basename "$fx")" "$pct"
  echo "$(basename "$fx"),$pct" >> "$RESULTS/empty_gate.csv"
  gate_sum="$($PY -c "print($gate_sum + $pct)")"
  gate_n=$((gate_n + 1))
  if [ "$($PY -c "print(1 if $pct < 100.0 else 0)")" = "1" ]; then gate_below=$((gate_below+1)); fi
done
gate_mean="$($PY -c "print(round($gate_sum/$gate_n, 4))")"

echo
echo "=================================================================="
echo " MEASURED EMPTY-DIVERGENCE = ${gate_mean}%   (over ${gate_n} fixtures)"
echo " cases below 100%: $gate_below"
echo "=================================================================="

# --- verdict ---
fail=0
if [ "$($PY -c "print(1 if $ctrl_mean > 0.0 else 0)")" = "1" ]; then
  echo "FAIL: control (ref vs ref) divergence is ${ctrl_mean}% (expected 0%) — metric is broken." >&2
  fail=1
fi
if [ "$($PY -c "print(1 if $gate_mean < $THRESHOLD else 0)")" = "1" ]; then
  echo "FAIL: empty-divergence ${gate_mean}% < threshold ${THRESHOLD}%." >&2
  fail=1
fi
if [ "$gate_below" -ne 0 ]; then
  echo "FAIL: $gate_below fixture(s) did not reach 100% divergence against the empty port." >&2
  fail=1
fi

# Emit a machine-readable summary too.
if [ "$fail" -eq 0 ]; then pass_lit=True; else pass_lit=False; fi
"$PY" -c "
import json
print(json.dumps({
  'gate': 'empty_workspace_divergence',
  'fixtures': $gate_n,
  'control_mean_divergence_pct': $ctrl_mean,
  'empty_divergence_pct': $gate_mean,
  'threshold_pct': $THRESHOLD,
  'cases_below_100': $gate_below,
  'pass': $pass_lit,
}, indent=2))
" | tee "$RESULTS/empty_gate_summary.json"

if [ "$fail" -eq 0 ]; then
  echo "GATE PASS: harness correctly detects total divergence (${gate_mean}%)."
  exit 0
else
  echo "GATE FAIL." >&2
  exit 1
fi
