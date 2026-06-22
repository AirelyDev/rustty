#!/usr/bin/env bash
# ci.sh — the SINGLE entry point the rustty kickoff CI runs.
#
# Governed by .port/PORT_RULES.md (R1 fidelity, R2 identity, R3 self-contained /
# no ghostty Cargo dep, R4 acyclic crate graph, R5 gates-invariant) and
# .port/phase-0/plan/DEFINITION_OF_PARITY.md (the authoritative gate defs G1-G9).
# .github/workflows/ci.yml mirrors this file so CI is REAL (not a paper gate).
#
# WHAT IT DOES (in order; collects ALL failures, never stops at the first):
#   1. cargo fmt --check                       (HARD-FAIL)
#   2. cargo clippy --workspace -D warnings    (HARD-FAIL)
#   3. cargo build --workspace                 (HARD-FAIL; also the live R4/G7
#                                               proof — Cargo refuses a normal-dep
#                                               cycle, so a clean build IS hard
#                                               acyclicity)
#   4. cargo test --workspace                  (HARD-FAIL; PASS/FAIL oracle for
#                                               every ported test, R1/G1)
#   5. the parity gates in .port/tools/gates/  (the canonical Phase-0 gate suite)
#
# GATE SUITE (canonical scripts; ci.sh does NOT reimplement them — it runs them):
#   G7  check_acyclic.sh             crate graph acyclic (R4)        -> HARD-FAIL
#   G4  run_differential.sh          differential VT + harness ctrl  -> HARD-FAIL
#   G1  check_tests_ported.py        100% test{} ported (==3234)     -> PROGRESS*
#   G2  check_assertions.py          assertion-for-assertion         -> PROGRESS*
#   G3  check_comments_fidelity.py   zero comment-inventory delta    -> PROGRESS*
#   G3b check_comment_coverage.py    content-aware comment coverage  -> PROGRESS*
#                                    (every distinctive reference comment has a
#                                    match in the port; 0 MISSING — closes the
#                                    aggregate-count hole G3 can mask)
#   G9  check_parity_surface.py      config/keybind/action/terminfo  -> PROGRESS*
#   G8  check_baseline_regression.py no perf/mem/size/startup regress-> PROGRESS*
#   G5  run_fuzz_smoke.sh            differential fuzz clean         -> PROGRESS*
#   G6  check_leaks.sh               leak-free (tracking-alloc/Miri) -> PROGRESS*
#
#   HARD-FAIL gates must be green from day one (G7 holds via the topo-ordered
#   acyclic rustty Cargo graph; G4's harness CONTROL + empty-divergence proof
#   must pass now and auto-flips to the real byte-exact gate once the port emits
#   output). A red here fails CI.
#
#   PROGRESS* gates: these are the parity gates that "PRINT progress without
#   failing CI while crates are unported." They are NOT weakened (R5): each keeps
#   its full DoP pass condition. The canonical checkers implement this natively —
#   a STUB crate with 0 tests/comments/etc. is GREEN (progress, exit 0); a crate
#   that *claims done* yet is short is RED (exit non-zero). So while crates are
#   unported they all exit 0 and print progress; the moment a claimed-done crate
#   regresses they go RED and ci.sh fails. That self-flip is exactly R5-correct:
#   progress now, binding the instant a crate is completed. A BLOCKED checker
#   (missing contract/tooling) is surfaced as a WARNING, never a silent green.
#
# CI exits non-zero iff ANY *non-progress* check failed — i.e. any HARD-FAIL
# check is red, OR a PROGRESS gate went RED because a claimed-done crate
# regressed (which is no longer "while unported").
#
# Usage:
#   bash .port/tools/ci.sh                   # full run (what CI runs)
#   CI_SKIP_CARGO=1 bash .port/tools/ci.sh   # gates only (no cargo; local/fast)
set -uo pipefail   # NOTE: no `-e` — run every check, aggregate failures.

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
GATES="$ROOT/.port/tools/gates"
PY="${PYTHON:-python3}"

# Gate registry: "DoP_id|classification|runner...". classification is HARD or
# PROGRESS. HARD-fail gates run first (fail-fast visibility), then PROGRESS.
# Each runner is invoked verbatim; its exit code is interpreted below:
#   0  -> PASS (or, for a PROGRESS gate, "progress, not failing CI")
#   3  -> BLOCKED (missing contract/tooling) -> WARNING, never a silent green
#   other non-zero -> FAIL  (HARD gate: fails CI; PROGRESS gate: a claimed-done
#                            regression -> fails CI, per R5)
GATE_REGISTRY=(
  "G7|HARD|bash $GATES/check_acyclic.sh"
  "G4|HARD|bash $GATES/run_differential.sh"
  "G1|PROGRESS|$PY $GATES/check_tests_ported.py"
  "G2|PROGRESS|$PY $GATES/check_assertions.py"
  "G3|PROGRESS|$PY $GATES/check_comments_fidelity.py"
  "G3b|PROGRESS|$PY $GATES/check_comment_coverage.py"
  "G9|PROGRESS|$PY $GATES/check_parity_surface.py"
  "G8|PROGRESS|$PY $GATES/check_baseline_regression.py"
  "G5|PROGRESS|bash $GATES/run_fuzz_smoke.sh"
  "G6|PROGRESS|bash $GATES/check_leaks.sh"
)

HARD_FAILURES=()   # hard-fail checks that failed, or a claimed-done regression
WARNINGS=()        # BLOCKED gates (pending contract/tooling)
PROGRESS_NOTES=()  # progress gates reporting progress (exit 0 while unported)
PASSED=()          # checks that passed cleanly

hr() { printf '%s\n' "------------------------------------------------------------------"; }
section() { hr; printf '### %s\n' "$1"; hr; }

# Run a HARD-FAIL cargo step. Records failure; never aborts the script.
run_cargo() {
  local label="$1"; shift
  section "$label  (HARD-FAIL)"
  if [ "${CI_SKIP_CARGO:-0}" = "1" ]; then
    echo "SKIPPED (CI_SKIP_CARGO=1): $label"
    PROGRESS_NOTES+=("$label (skipped via CI_SKIP_CARGO)")
    return 0
  fi
  if "$@"; then
    echo ">> PASS: $label"; PASSED+=("$label")
  else
    local rc=$?; echo ">> FAIL ($rc): $label" >&2; HARD_FAILURES+=("$label (exit $rc)")
  fi
}

# Run one registry gate, interpreting its exit code per classification.
run_gate() {
  local id="$1" kind="$2"; shift 2
  section "gate $id  [$kind]  ->  $*"
  "$@"; local rc=$?
  if [ "$rc" -eq 0 ]; then
    echo ">> PASS: $id"
    if [ "$kind" = "PROGRESS" ]; then PROGRESS_NOTES+=("$id (progress/green)"); else PASSED+=("$id"); fi
  elif [ "$rc" -eq 3 ]; then
    echo ">> BLOCKED ($rc): $id [pending contract/tooling — surfaced, not greened]" >&2
    WARNINGS+=("$id BLOCKED (exit 3)")
  else
    # Non-zero, non-BLOCKED => a real RED.
    if [ "$kind" = "HARD" ]; then
      echo ">> FAIL ($rc): $id [HARD-FAIL gate]" >&2
      HARD_FAILURES+=("$id (exit $rc)")
    else
      echo ">> FAIL ($rc): $id [PROGRESS gate went RED — a claimed-done crate regressed; R5]" >&2
      HARD_FAILURES+=("$id (claimed-done regression, exit $rc)")
    fi
  fi
}

echo "=================================================================="
echo " rustty CI — single entry (.port/tools/ci.sh)"
echo " root: $ROOT"
echo " toolchain: $( (rustc --version 2>/dev/null) || echo 'rustc: n/a')"
echo "=================================================================="

# ---- 1..4: cargo steps (all HARD-FAIL) ------------------------------------
run_cargo "cargo fmt --check"        cargo fmt --check
run_cargo "cargo clippy -D warnings" cargo clippy --workspace --all-targets -- -D warnings
run_cargo "cargo build --workspace"  cargo build --workspace
run_cargo "cargo test --workspace"   cargo test --workspace

# ---- 5: parity gates (HARD first, then PROGRESS) ---------------------------
for phase in HARD PROGRESS; do
  for entry in "${GATE_REGISTRY[@]}"; do
    IFS='|' read -r gid gkind grun <<< "$entry"
    [ "$gkind" = "$phase" ] || continue
    # shellcheck disable=SC2086 — grun is an intentional word-split command line.
    run_gate "$gid" "$gkind" $grun
  done
done

# ---- summary ---------------------------------------------------------------
section "CI SUMMARY"
echo "PASSED (${#PASSED[@]}):";          for x in "${PASSED[@]:-}";        do [ -n "$x" ] && echo "  + $x"; done
echo "PROGRESS — not failing CI (${#PROGRESS_NOTES[@]}):"; for x in "${PROGRESS_NOTES[@]:-}"; do [ -n "$x" ] && echo "  ~ $x"; done
echo "WARNINGS — BLOCKED (pending contract/tooling) (${#WARNINGS[@]}):";   for x in "${WARNINGS[@]:-}";     do [ -n "$x" ] && echo "  ! $x"; done
echo "HARD FAILURES (${#HARD_FAILURES[@]}):"; for x in "${HARD_FAILURES[@]:-}"; do [ -n "$x" ] && echo "  X $x"; done

hr
if [ "${#HARD_FAILURES[@]}" -eq 0 ]; then
  echo "CI RESULT: PASS  (all hard-fail checks green; progress gates printed"
  echo "           progress without failing CI — R5: gates invariant, never weakened)"
  exit 0
else
  echo "CI RESULT: FAIL  (${#HARD_FAILURES[@]} non-progress check(s) red)" >&2
  exit 1
fi
