#!/usr/bin/env bash
# run_fuzz_smoke.sh — GATE G5 (differential fuzz clean) — thin runner.
#
# DoP id:  G5  (DEFINITION_OF_PARITY §1, "Differential fuzz clean")
# Drives:  crates/rustty-vt/fuzz/  (cargo-fuzz target `vt_diff`, shares the G4
#                                    harness) when it exists.
# Pass:    exit 0, ZERO new entries in fuzz/artifacts/ for the configured budget;
#          the regression corpus replays clean.
#
# THIS IS A WIRED PROGRESS-MODE RUNNER. The fuzz target does not exist while the
# port is empty, and cargo builds are out of scope for this gate authoring task.
# So this runner:
#   * if `cargo fuzz` + the `vt_diff` target are present, runs a SMOKE budget
#     (short -max_total_time) and fails on any new artifact (the real gate),
#   * otherwise runs in progress mode: it verifies the fuzz INVENTORY + corpus
#     wiring is present (FUZZ-INVENTORY.md, synthetic generator, seed corpus dir)
#     and exits 0 with a FUZZ-WIRED status — it does NOT fabricate a fuzz green.
#
# Budgets (override via env): FUZZ_SECS (default 10 smoke), FUZZ_RUNS (default 0).
#
# Usage:
#   run_fuzz_smoke.sh                 # auto: real smoke if target exists, else progress
#   run_fuzz_smoke.sh --progress      # force progress (wiring check) mode
#   run_fuzz_smoke.sh --self-test     # self-test the runner wiring itself
#
# Exit 0 = GREEN (real smoke) or FUZZ-WIRED (progress); non-zero = RED / BLOCKED.
set -euo pipefail

GATES_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PORT_DIR="$(cd "$GATES_DIR/../.." && pwd)"
RUSTTY_ROOT="$(cd "$PORT_DIR/.." && pwd)"
HARNESS="$PORT_DIR/phase-0/harness"
FUZZ_DIR="$RUSTTY_ROOT/crates/rustty-vt/fuzz"
FUZZ_INVENTORY="$HARNESS/FUZZ-INVENTORY.md"
FUZZ_SECS="${FUZZ_SECS:-10}"
FUZZ_RUNS="${FUZZ_RUNS:-0}"

MODE="auto"
case "${1:-}" in
  --progress)  MODE="progress" ;;
  --self-test) MODE="self-test" ;;
  "" )         MODE="auto" ;;
  *) echo "run_fuzz_smoke.sh: unknown arg '${1:-}'" >&2; exit 2 ;;
esac

echo "=== G5 :: differential fuzz clean (runner) ==="

run_self_test() {
  echo "--- runner self-test: assert fuzz wiring ---"
  local ok=1
  if [ -f "$FUZZ_INVENTORY" ]; then
    echo "  FUZZ-INVENTORY.md present: OK"
  else
    echo "  FAIL: FUZZ-INVENTORY.md missing ($FUZZ_INVENTORY)"; ok=0
  fi
  # synthetic generator (corpus source) — crate dir presence is enough at scaffold
  if [ -d "$RUSTTY_ROOT/crates/rustty-synthetic" ]; then
    echo "  rustty-synthetic crate present (corpus generator): OK"
  else
    echo "  WARN: rustty-synthetic crate absent"
  fi
  if command -v cargo >/dev/null 2>&1; then
    echo "  cargo present: OK"
  else
    echo "  WARN: cargo not on PATH (real fuzz run unavailable)"
  fi
  if [ "$ok" = "1" ]; then echo "SELF-TEST: PASS"; return 0; else echo "SELF-TEST: FAIL"; return 1; fi
}

if [ "$MODE" = "self-test" ]; then
  run_self_test
  exit $?
fi

# --- BLOCKED: the fuzz inventory must exist (the wiring contract) ---
if [ ! -f "$FUZZ_INVENTORY" ]; then
  echo "BLOCKED: FUZZ-INVENTORY.md missing ($FUZZ_INVENTORY)"
  exit 3
fi

have_cargo_fuzz=0
if command -v cargo >/dev/null 2>&1 && cargo fuzz --help >/dev/null 2>&1; then
  have_cargo_fuzz=1
fi

if [ "$MODE" = "auto" ] && [ "$have_cargo_fuzz" = "1" ] && [ -d "$FUZZ_DIR" ]; then
  echo "--- REAL fuzz smoke (cargo fuzz + vt_diff target found) ---"
  ARTIFACTS="$FUZZ_DIR/artifacts/vt_diff"
  before=0
  [ -d "$ARTIFACTS" ] && before="$(ls -1 "$ARTIFACTS" 2>/dev/null | wc -l | tr -d ' ')"
  set +e
  ( cd "$RUSTTY_ROOT" && cargo +nightly fuzz run vt_diff -- \
      -max_total_time="$FUZZ_SECS" ${FUZZ_RUNS:+-runs=$FUZZ_RUNS} )
  rc=$?
  set -e
  after=0
  [ -d "$ARTIFACTS" ] && after="$(ls -1 "$ARTIFACTS" 2>/dev/null | wc -l | tr -d ' ')"
  if [ "$rc" -eq 0 ] && [ "$after" -le "$before" ]; then
    echo "GREEN: fuzz smoke clean — no new artifacts (G5)"
    exit 0
  else
    echo "RED: fuzz produced a new artifact / non-zero exit (divergence or crash repro) (G5)"
    echo "     artifacts before=$before after=$after rc=$rc"
    exit 1
  fi
fi

# --- progress mode ---
echo "--- PROGRESS mode: cargo-fuzz target 'vt_diff' not built yet ---"
echo "    fuzz dir: $FUZZ_DIR (exists: $([ -d "$FUZZ_DIR" ] && echo yes || echo no))"
echo "    cargo fuzz available: $([ "$have_cargo_fuzz" = "1" ] && echo yes || echo no)"
if run_self_test; then
  echo "FUZZ-WIRED (port empty): fuzz inventory + corpus wiring present; G5 in progress mode"
  exit 0
else
  echo "BLOCKED: fuzz wiring incomplete"
  exit 3
fi
