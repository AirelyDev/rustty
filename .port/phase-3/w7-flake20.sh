#!/usr/bin/env bash
# P3-W7 T3 Step 7 — run `cargo test -p rustty-os` 20x with DEFAULT parallelism
# and record zero flakes. The pre-fix `setenv`-from-test race is intermittent,
# so a single green run is not evidence.
set -uo pipefail
PKG="${1:-rustty-os}"
N="${2:-20}"
fail=0
for i in $(seq 1 "$N"); do
  if out=$(cargo test -p "$PKG" --no-fail-fast 2>&1); then
    printf 'run %2d: PASS\n' "$i"
  else
    printf 'run %2d: FAIL\n' "$i"
    printf '%s\n' "$out" | grep -E 'FAILED|panicked|test result: FAILED' | head -10
    fail=$((fail + 1))
  fi
done
echo "----"
echo "$PKG: $N runs, $fail failures"
exit "$fail"
