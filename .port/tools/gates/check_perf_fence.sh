#!/usr/bin/env bash
# check_perf_fence.sh — GATE G8b (paired same-session perf fence) [R5]
#
# DoP id:  G8b (companion to G8; DEFINITION_OF_PARITY §1 perf axis)
# Reads:   the pinned oracle bench binary (~/rustty-oracle-artifacts, checked
#          against ws10_suite.py's ORACLE_SHA256) + the four corpora (BOTH
#          UNTRACKED; the corpora live in the shared checkout only), a candidate
#          rustty-bench, and optionally the stored entry binary in .perf-bins/.
# Pass:    rustty beats the pinned oracle on all 14 configs (Fence 1) and has
#          not self-regressed against the entry binary (Fence 2).
#
# WHY G8b EXISTS ALONGSIDE G8
# ---------------------------
# G8 compares rustty against the ORACLE's frozen June numbers with a +/-7% band.
# Because rustty is 7-60% FASTER than the oracle, G8 spends that entire margin
# as headroom: measured on 2026-07-30, G8 would not fire until rustty got
#   terminal-parser      60.4% slower
#   screen-clone.clone  110.6% slower
#   terminal-stream      40.3% slower
# than it is today. G8 is therefore an ORACLE-PARITY fence, not a regression
# fence, and fixing its (id, optimize) indexing bug in WS10 T2 did not change
# that. G8b is the real regression fence: it re-measures a STORED BINARY in the
# same process as the candidate, which removes session variance (the same
# unchanged oracle binary spans 11.8% across sessions) and lets the band be +5%.
#
# EXIT CODES (deliberately four-valued)
#   0 GREEN    fence held
#   1 RED      real regression
#   2 RED      ws10_suite.py's own exit 2: a measurement that turned out
#              provisional (load rose above IDLE_MAX while it ran), or an
#              argparse usage error. ci.sh reads it as RED (only 0 and 3 are
#              not), and it stays RED: a fence verdict is never overridden.
#   3 BLOCKED  the dedicated PRE-MEASUREMENT load check (load_check below):
#              the 1-minute load stayed above IDLE_MAX through a bounded
#              cooldown (G8B_COOLDOWN_S, default 600 s), checked before G8b's
#              own builds and again before measuring. Nothing was measured, so
#              no metrics file exists for G8. Its own reason line says so.
#   other      a crash or a kill of the harness: RED, never BLOCKED.
#   3 BLOCKED  measurement substrate absent (oracle binary / corpora missing --
#              they are untracked and live only in the shared checkout, so this
#              is the normal state inside a fresh worktree or on CI).
#
# Usage:
#   check_perf_fence.sh [--build] [--rust PATH] [--self PATH]
#                       [--emit-metrics PATH] [-n N]
#   check_perf_fence.sh --self-test   (G8b-SELF: the exit mapping, against a
#                       fake harness; no oracle, no measurement)
#
# --build (what ci.sh passes; P7-W22 review): once the substrate is known to be
# present, run every size label's `rustty_build` from BASELINE.json (ws10_suite
# --print-size-builds: the bench, rustty-gen and the vt-c release libraries, the
# perf README's list) at the repo root, so a fresh tree measures fresh artifacts
# and a stale one is never measured silently. CARGO_TARGET_DIR is unset for
# those builds because the counterparts are measured under <root>/target. A
# failed build is RED (exit 1). CI_SKIP_CARGO=1 skips them with a NOTE.
# --emit-metrics PATH is removed first, so a BLOCKED run leaves no earlier run's
# file behind for G8 to read.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../../.." && pwd)"
SUITE="$HERE/../perf/ws10_suite.py"
RUST="${RUSTTY_BENCH_BIN:-$ROOT/target/release-fast/rustty-bench}"
ENTRY=".perf-bins/rustty-bench-ws10-entry-RF"
BUILD=0
EMIT=""
ARGS=()

# ---- the pre-measurement load check (the only BLOCKED for load) ------------
# IDLE_MAX is ws10_suite.py's own threshold, read from it (one source).
G8B_COOLDOWN_S="${G8B_COOLDOWN_S:-600}"
G8B_COOLDOWN_STEP_S="${G8B_COOLDOWN_STEP_S:-15}"
load1() { python3 -c 'import os; print("%.2f" % os.getloadavg()[0])'; }
load_check() {  # load_check PHASE — returns when idle, exits 3 after the cooldown
  local phase="$1" waited=0 l idle
  idle="$(sed -n 's/^IDLE_MAX = \([0-9.]*\).*/\1/p' "$SUITE")"
  [ -n "$idle" ] || { echo "RED: cannot read IDLE_MAX from $SUITE"; exit 1; }
  while :; do
    l="$(load1)"
    if python3 -c 'import sys; sys.exit(0 if float(sys.argv[1]) <= float(sys.argv[2]) else 1)' "$l" "$idle"; then
      echo "load check ($phase): 1-minute load $l <= IDLE_MAX $idle (after ${waited}s)"
      return 0
    fi
    if [ "$waited" -ge "$G8B_COOLDOWN_S" ]; then
      echo "BLOCKED: G8b pre-measurement load check ($phase): 1-minute load $l > IDLE_MAX $idle after a ${waited}s cooldown; nothing was measured"
      exit 3
    fi
    sleep "$G8B_COOLDOWN_STEP_S"; waited=$((waited + G8B_COOLDOWN_STEP_S))
  done
}

# ---- --self-test (G8b-SELF) -------------------------------------------------
# Copies of this script against a fake ws10_suite.py (preflight and builds no-op,
# then crash / usage error / fence RED / green as asked) and a stubbed load1.
# A crash and a usage error stay RED, a fence RED stays RED, only the load
# check is BLOCKED (and measures nothing). Control: the reviewed blanket remap
# (`*) ... exit 3`) turns the crash BLOCKED, and the crash arm notices.
self_test() {
  local st fail=0 mode want
  set +e   # the arms run copies that exit non-zero on purpose
  st="$(mktemp -d "${TMPDIR:-/tmp}/g8bself.XXXXXX")"
  mk() {  # mk DIR [MUTATION-OLD MUTATION-NEW]
    mkdir -p "$1/.port/tools/gates" "$1/.port/tools/perf"
    python3 - "$HERE/check_perf_fence.sh" "$1/.port/tools/gates/check_perf_fence.sh" "${2:-}" "${3:-}" <<'PY'
import sys
s = open(sys.argv[1]).read()
old = "load1() { python3 -c 'import os; print(\"%.2f\" % os.getloadavg()[0])'; }"
assert s.count(old) == 1
s = s.replace(old, 'load1() { cat "$ROOT/load"; }')
if sys.argv[3]:
    # Mutate the gate's own code, never this self-test's text.
    a = s.index("\n# ---- --self-test (G8b-SELF)")
    b = s.rindex('if [ "${1:-}" = "--self-test" ]; then self_test; exit $?; fi')
    rest = s[:a] + s[b:]
    assert rest.count(sys.argv[3]) == 1, sys.argv[3]
    s = s[:a].replace(sys.argv[3], sys.argv[4]) + s[a:b] + s[b:].replace(sys.argv[3], sys.argv[4])
open(sys.argv[2], "w").write(s)
PY
    cat > "$1/.port/tools/perf/ws10_suite.py" <<'PY'
import os, sys
root = os.path.dirname(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))
IDLE_MAX = 2.0
if "--preflight" in sys.argv or "--print-size-builds" in sys.argv:
    sys.exit(0)
open(os.path.join(root, "measured"), "a").write("measured\n")
mode = open(os.path.join(root, "mode")).read().strip()
if mode == "crash":
    os.kill(os.getpid(), 11)
if mode == "usage":
    print("usage: ws10_suite.py [-h]\nws10_suite.py: error: unrecognized arguments", file=sys.stderr); sys.exit(2)
sys.exit(1 if mode == "red" else 0)
PY
    printf '#!/bin/sh\nexit 0\n' > "$1/bench"; chmod +x "$1/bench"
  }
  run() {  # run DIR MODE LOAD -> rc in RC, output in OUT
    printf '%s\n' "$2" > "$1/mode"; printf '%s\n' "$3" > "$1/load"; : > "$1/measured"
    OUT="$(CI_SKIP_CARGO=1 G8B_COOLDOWN_S=0 bash "$1/.port/tools/gates/check_perf_fence.sh" --build --rust "$1/bench" ${G8B_ST_ARGS:-} 2>&1)"; RC=$?
  }
  mk "$st/base"
  for mode in green:0 red:1 usage:2 crash:139; do
    want="${mode#*:}"; mode="${mode%%:*}"
    run "$st/base" "$mode" 0.10
    if [ "$RC" = "$want" ] && [ -s "$st/base/measured" ]; then echo "  ok    harness $mode -> exit $RC ($( [ "$RC" = 0 ] && echo green || echo RED in ci.sh))"
    else echo "  FAIL  harness $mode -> exit $RC, want $want"; printf '%s\n' "$OUT" | tail -3 | sed 's/^/        /'; fail=1; fi
  done
  G8B_ST_ARGS="--emit-metrics $st/base/m.json" run "$st/base" green 0.10
  if [ "$RC" = 0 ]; then echo "  ok    with --emit-metrics (as ci.sh runs it) -> exit 0"; else echo "  FAIL  with --emit-metrics -> exit $RC"; fail=1; fi
  run "$st/base" green 9.90
  if [ "$RC" = 3 ] && printf '%s\n' "$OUT" | grep -q "BLOCKED: G8b pre-measurement load check" && [ ! -s "$st/base/measured" ]; then
    echo "  ok    loaded host -> exit 3 (pre-measurement load check), nothing measured"
  else echo "  FAIL  loaded host -> exit $RC"; printf '%s\n' "$OUT" | tail -3 | sed 's/^/        /'; fail=1; fi
  mk "$st/remap" '  *) echo "RED: unexpected harness exit $rc (a crash or a kill)" ;;' '  *) echo "BLOCKED: unexpected harness exit $rc"; exit 3 ;;'
  run "$st/remap" crash 0.10
  if [ "$RC" = 3 ]; then echo "  ok    control: the blanket remap turns a harness crash into BLOCKED (exit 3); the crash arm above requires RED"
  else echo "  FAIL  control: the remapped copy did not turn the crash BLOCKED (exit $RC)"; fail=1; fi
  if [ "$fail" = 0 ]; then echo "G8b-SELF GREEN"; return 0; fi
  echo "G8b-SELF RED (fixtures in $st)"; return 1
}
if [ "${1:-}" = "--self-test" ]; then self_test; exit $?; fi

while [ $# -gt 0 ]; do
  case "$1" in
    --rust) RUST="$2"; shift 2 ;;
    --self) ENTRY="$2"; shift 2 ;;
    --build) BUILD=1; shift ;;
    --emit-metrics) EMIT="$2"; ARGS+=("$1" "$2"); shift 2 ;;
    *) ARGS+=("$1"); shift ;;
  esac
done

echo "=== G8b :: paired same-session perf fence (the pinned oracle only) ==="

if [ -n "$EMIT" ]; then
  rm -f -- "$EMIT"
fi

if [ "$BUILD" = 1 ]; then
  # Substrate first: on CI (no oracle) this BLOCKs without paying for builds.
  set +e
  python3 "$SUITE" --preflight
  rc=$?
  set -e
  if [ "$rc" -ne 0 ]; then
    [ "$rc" -eq 3 ] && echo "BLOCKED: measurement substrate absent (untracked oracle binary/corpora)"
    exit "$rc"
  fi
  load_check "before the builds"
  if [ "${CI_SKIP_CARGO:-0}" = "1" ]; then
    echo "NOTE: CI_SKIP_CARGO=1 -> size counterparts NOT rebuilt; measuring what is on disk"
  else
    builds="$(python3 "$SUITE" --print-size-builds)"
    while IFS= read -r cmd; do
      [ -n "$cmd" ] || continue
      read -r -a argv <<<"$cmd"
      if [ "${argv[0]}" != "cargo" ] || [ "${argv[1]:-}" != "build" ]; then
        echo "RED: BASELINE.json rustty_build is not a cargo build: $cmd"
        exit 1
      fi
      echo "+ $cmd"
      if ! (cd "$ROOT" && env -u CARGO_TARGET_DIR "${argv[@]}"); then
        echo "RED: size counterpart build failed: $cmd"
        exit 1
      fi
    done <<<"$builds"
  fi
fi

if [ ! -x "$RUST" ]; then
  echo "BLOCKED: candidate bench binary not found: $RUST"
  echo "   build it with: cargo build -p rustty-bench --profile release-fast"
  exit 3
fi

# Fence 2 is only run when an entry binary has been stashed; its absence is not
# a failure (a fresh clone has none), it just narrows the gate to Fence 1.
if [ -x "$ENTRY" ]; then
  ARGS+=("--self" "$ENTRY")
else
  echo "NOTE: no entry binary at $ENTRY -> Fence 1 (oracle) only."
fi

load_check "before measuring"
set +e
# ${ARGS[@]+...}: an empty array under `set -u` is an unbound-variable error on
# bash 3.2 (macOS), which made this line exit 1 whenever no flag added an ARG.
python3 "$SUITE" --rust "$RUST" ${ARGS[@]+"${ARGS[@]}"}
rc=$?
set -e

case "$rc" in
  0) echo "GREEN: perf fence held" ;;
  1) echo "RED: perf fence regressed; see LOST / SELF-REGRESS above" ;;
  2) echo "RED (exit 2): the harness reported a provisional (loaded) measurement or a usage error; see above. Only the pre-measurement load check is BLOCKED." ;;
  3) echo "BLOCKED: measurement substrate absent (untracked oracle binary/corpora)" ;;
  *) echo "RED: unexpected harness exit $rc (a crash or a kill)" ;;
esac
exit "$rc"
