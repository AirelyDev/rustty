#!/usr/bin/env bash
# Repro harness for the subprocess_drop_repro hang (rustty-termio).
#
# Runs the integration-test binary N times with a hard per-run timeout, and
# reports whether any run hung. On a hang it captures the wedged children
# (`ps -o pid,ppid,stat,wchan,command`) and then CLEANS UP: kills the harness
# parent so the kernel reaps whatever it can, and verifies nothing is left.
#
# Usage: repro_subprocess_hang.sh [N] [TIMEOUT_SECONDS]
set -u

N="${1:-10}"
TMO="${2:-40}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
LOGDIR="$(mktemp -d)"

BIN="$(ls -t "$ROOT"/target/debug/deps/subprocess_drop_repro-* 2>/dev/null | grep -v '\.d$' | head -1)"
if [ -z "$BIN" ]; then
  echo "FATAL: no subprocess_drop_repro test binary; run: cargo build --tests -p rustty-termio" >&2
  exit 2
fi
echo "binary: $BIN"
echo "runs:   $N   timeout: ${TMO}s   logs: $LOGDIR"

hangs=0
for i in $(seq 1 "$N"); do
  log="$LOGDIR/run-$i.log"
  "$BIN" --test-threads="${TEST_THREADS:-3}" >"$log" 2>&1 &
  pid=$!

  waited=0
  while kill -0 "$pid" 2>/dev/null && [ "$waited" -lt "$TMO" ]; do
    sleep 1
    waited=$((waited + 1))
  done

  if kill -0 "$pid" 2>/dev/null; then
    hangs=$((hangs + 1))
    echo "=== run $i: HUNG after ${TMO}s (pid $pid) ==="
    ps -eo pid,ppid,stat,wchan,command | grep -E "subprocess_drop_repro|[/ ]login " | grep -v grep
    echo "--- test output so far ---"
    cat "$log"
    echo "--- cleanup ---"
    # Kill the harness parent; the kernel then reaps whatever children it can.
    kill -9 "$pid" 2>/dev/null
    wait "$pid" 2>/dev/null
    sleep 2
  else
    wait "$pid"
    rc=$?
    echo "run $i: exit $rc ($(grep -c '^test .* ok$' "$log" 2>/dev/null) tests ok)"
  fi
done

echo
echo "==================== SUMMARY ===================="
echo "runs=$N  hangs=$hangs"
echo "--- residual processes (must be empty) ---"
ps -eo pid,ppid,stat,command | grep -E "subprocess_drop_repro" | grep -v grep || echo "(none: subprocess_drop_repro)"
ps -eo pid,ppid,stat,command | awk '$3 ~ /E/ && /login/' | grep -v grep || echo "(none: E-state login)"
[ "$hangs" -eq 0 ]
