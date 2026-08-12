#!/usr/bin/env bash
# run_fuzz_smoke.sh — GATE G5 (differential fuzz clean) — thin runner.
#
# DoP id:  G5  (DEFINITION_OF_PARITY §1, "Differential fuzz clean")
# Drives:  crates/rustty-vt/fuzz/  (cargo-fuzz target `vt_diff`, shares the G4
#                                    harness) when it exists.
# Pass:    exit 0, ZERO new entries in fuzz/artifacts/ for the configured budget;
#          the regression corpus replays clean.
#
# STATUS (codex-vet F-017, 2026-08-11): the `vt_diff` target NOW EXISTS
# (crates/rustty-vt/fuzz/, cargo-fuzz), so wherever nightly + cargo-fuzz are
# installed this runner takes the REAL branch below and G5 is a real gate —
# measured GREEN locally (3,299 runs over the 3,295-file seed corpus, no
# artifact). Progress mode survives only as the honest fallback for legs
# WITHOUT cargo-fuzz (e.g. GitHub CI, which does not install it); ci.sh still
# classes G5 as PROGRESS for that reason. This runner:
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
  # cargo-fuzz resolves `fuzz/` relative to cwd, so run from the crate that
  # owns the fuzz dir (crates/rustty-vt), not the workspace root.
  ( cd "$RUSTTY_ROOT/crates/rustty-vt" && cargo +nightly fuzz run vt_diff -- \
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
echo "--- PROGRESS mode: cannot run the REAL smoke on this leg ---"
echo "    fuzz dir: $FUZZ_DIR (exists: $([ -d "$FUZZ_DIR" ] && echo yes || echo no))"
echo "    cargo fuzz available: $([ "$have_cargo_fuzz" = "1" ] && echo yes || echo no)"
echo "    (both must be 'yes' for the real smoke; the vt_diff target ships in-tree,"
echo "     so a 'no' on cargo fuzz means nightly+cargo-fuzz are not installed here)"
if run_self_test; then
  echo "FUZZ-WIRED: fuzz inventory + corpus wiring present; G5 in progress mode on this leg"
  exit 0
else
  echo "BLOCKED: fuzz wiring incomplete"
  exit 3
fi
