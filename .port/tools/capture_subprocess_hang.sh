#!/usr/bin/env bash
# Capture the live wedged state of a subprocess_drop_repro hang, then clean up.
set -u
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
# cargo's own target dir (CARGO_TARGET_DIR honoured), not a literal $ROOT/target.
# shellcheck source=.port/tools/gates/_target_dir.sh
. "$ROOT/.port/tools/gates/_target_dir.sh"
TGT="$(rustty_target_dir "$ROOT")"
# `ls -t` is load-bearing here: cargo leaves several hash-suffixed
# subprocess_drop_repro-* binaries in deps/ and we want the newest. A glob
# cannot sort by mtime, so the ls|grep pipe stays.
# shellcheck disable=SC2010
BIN="$(ls -t "$TGT"/debug/deps/subprocess_drop_repro-* | grep -v '\.d$' | head -1)"
echo "binary: $BIN"

"$BIN" --test-threads="${TEST_THREADS:-3}" >/tmp/rustty-hang.log 2>&1 &
P=$!
sleep "${WAIT:-25}"

if ! kill -0 "$P" 2>/dev/null; then
  echo "NO HANG this run (exited)"; cat /tmp/rustty-hang.log; exit 0
fi

echo "=== test output so far ==="
cat /tmp/rustty-hang.log
echo "=== process tree: harness $P and its children ==="
ps -axo pid,ppid,pgid,sess,stat,wchan,command | awk -v p="$P" 'NR==1 || $1==p || $2==p'
echo "=== all login / sh procs ==="
ps -axo pid,ppid,pgid,stat,wchan,command | grep -Ei 'login|subprocess_drop' | grep -v grep
echo "=== descendants (recursive) ==="
pgrep -lP "$P" || echo "(no direct children visible)"
echo "=== sample harness ==="
sample "$P" 2 -mayDie 2>&1 | sed -n '1,140p'

echo "=== CLEANUP ==="
kill -9 "$P" 2>/dev/null
wait "$P" 2>/dev/null
sleep 2
ps -axo pid,stat,command | grep -E 'subprocess_drop' | grep -v grep || echo "(clean: no subprocess_drop_repro)"
ps -axo pid,stat,command | awk '$2 ~ /E/' | grep -i login || echo "(clean: no E-state login)"
