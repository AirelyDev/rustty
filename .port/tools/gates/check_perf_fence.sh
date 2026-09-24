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
#   2 BLOCKED  unusable measurement (load average above the idle threshold).
#              A contended run is neither a pass nor a failure; it must never
#              become a recorded baseline. Surfaced as BLOCKED, not green.
#   3 BLOCKED  measurement substrate absent (oracle binary / corpora missing --
#              they are untracked and live only in the shared checkout, so this
#              is the normal state inside a fresh worktree or on CI).
#
# Usage:
#   check_perf_fence.sh [--rust PATH] [--self PATH] [--emit-metrics PATH] [-n N]
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SUITE="$HERE/../perf/ws10_suite.py"
RUST="${RUSTTY_BENCH_BIN:-target/release-fast/rustty-bench}"
ENTRY=".perf-bins/rustty-bench-ws10-entry-RF"
ARGS=()

while [ $# -gt 0 ]; do
  case "$1" in
    --rust) RUST="$2"; shift 2 ;;
    --self) ENTRY="$2"; shift 2 ;;
    *) ARGS+=("$1"); shift ;;
  esac
done

echo "=== G8b :: paired same-session perf fence (the pinned oracle only) ==="

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

set +e
python3 "$SUITE" --rust "$RUST" "${ARGS[@]}"
rc=$?
set -e

case "$rc" in
  0) echo "GREEN: perf fence held" ;;
  1) echo "RED: perf fence regressed; see LOST / SELF-REGRESS above" ;;
  2) echo "BLOCKED: load average above the idle threshold — measurement unusable" ;;
  3) echo "BLOCKED: measurement substrate absent (untracked oracle binary/corpora)" ;;
  *) echo "BLOCKED: unexpected harness exit $rc" ;;
esac
exit "$rc"
