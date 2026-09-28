#!/usr/bin/env bash
# miri-run.sh — Miri verification with LIVE progress, stall detection, and a
# bounded per-test HARD timeout (so the run always terminates and cleanly
# partitions tests into PASS / UB / FAIL / TIMEOUT).
#
# Why: `cargo miri test` is silent until the whole (multi-hour) run ends, and a
# few heavy tests (large/non-standard/many-page allocations) are CPU-bound for
# HOURS under Miri's byte-level borrow tracking — so a full serial run never
# practically finishes and looks hung. This harness runs every matching test
# under Miri in parallel workers, emits per-test START/DONE+timing plus a 60s
# heartbeat rollup, and applies a hard timeout that marks a test TIMEOUT (a
# candidate Miri-exception) instead of hanging the whole gate.
#
# Usage: bash miri-run.sh <crate> <filter> <nchunks> <ceiling_min> <timeout_min>
#   timeout_min  HARD per-test kill budget in minutes (0 = unlimited)
#        bash miri-run.sh --self-test
#
# Buckets (NEW-FINDINGS N31: classify on CONTENT, never on an abort string,
# which every compile failure and interpreter error also prints):
#   ok       `test result: ok. 1 passed` and none of the below
#   UB       Miri's `Undefined Behavior` banner
#   LEAK     Miri's `error: memory leaked` (the test itself may have passed)
#   INTERP   a Miri interpreter limit, not UB: `resource exhaustion` (Miri does
#            not model allocation failure) or `unsupported operation`
#   BUILD    the test binary did not compile (`error[E....]` / `could not
#            compile`). A BUILD result ABORTS the sweep: the tree changed under
#            the run (each worker re-invokes `cargo miri test`), so every later
#            result would be about a different tree.
#   TIMEOUT  killed at the hard per-test budget
#   FAIL     anything else (an ordinary test failure)
# The work directory is unique per run (mktemp), so two sessions on the same
# crate cannot delete each other's evidence. `--self-test` classifies the real
# Miri outputs captured in `miri-run-fixtures/` (one per bucket) and checks the
# work directories of two runs differ.
set -u

# classify <output-file> <duration-s> <timeout-min>
classify() {
  local o="$1" d="$2" tmo="$3"
  if grep -q 'Undefined Behavior' "$o"; then echo UB
  elif grep -q 'error: memory leaked' "$o"; then echo LEAK
  elif grep -qE '^error\[E[0-9]+\]|error: could not compile' "$o"; then echo BUILD
  elif grep -qE 'error: (resource exhaustion|unsupported operation)' "$o"; then echo INTERP
  elif grep -q 'test result: ok. 1 passed' "$o"; then echo ok
  elif [ "$tmo" -gt 0 ] && [ "$d" -ge $((tmo*60 - 10)) ]; then echo TIMEOUT
  else echo FAIL; fi
}

new_workdir() { local t="${TMPDIR:-/tmp}"; mktemp -d "${t%/}/mirirun.$1.XXXXXX"; }

if [ "${1:-}" = "--self-test" ]; then
  FIX="$(cd "$(dirname "$0")" && pwd)/miri-run-fixtures"
  st_ok=1
  for pair in passes:ok use_after_free:UB leaks:LEAK exhausts:INTERP unsupported:INTERP build:BUILD fails:FAIL; do
    f="${pair%%:*}"; want="${pair#*:}"
    got="$(classify "$FIX/$f.out" 1 0)"
    if [ "$got" = "$want" ]; then echo "  self-test PASS: $f.out -> $got"
    else echo "  self-test FAIL: $f.out -> $got (want $want)"; st_ok=0; fi
  done
  # The old classifier's abort-string rule would call four of these UB.
  n=0; for f in build exhausts unsupported leaks; do grep -qE 'Undefined Behavior|memory leaked|error: aborting' "$FIX/$f.out" && n=$((n+1)); done
  if [ "$n" -ge 3 ]; then echo "  self-test PASS: the abort-string rule would misfile $n of build/exhausts/unsupported/leaks"
  else echo "  self-test FAIL: fixtures no longer exercise the abort-string trap ($n)"; st_ok=0; fi
  # A timed-out run with no verdict in its output is TIMEOUT, not FAIL.
  : > "${TMPDIR:-/tmp}/mirirun-selftest.$$"
  got="$(classify "${TMPDIR:-/tmp}/mirirun-selftest.$$" 600 10)"; rm -f "${TMPDIR:-/tmp}/mirirun-selftest.$$"
  if [ "$got" = TIMEOUT ]; then echo "  self-test PASS: empty output at the budget -> TIMEOUT"
  else echo "  self-test FAIL: empty output at the budget -> $got"; st_ok=0; fi
  a="$(new_workdir selftest)"; b="$(new_workdir selftest)"
  if [ -n "$a" ] && [ -n "$b" ] && [ "$a" != "$b" ] && [ -d "$a" ] && [ -d "$b" ]; then
    echo "  self-test PASS: two runs get distinct work dirs"
  else echo "  self-test FAIL: work dirs '$a' '$b'"; st_ok=0; fi
  rmdir "$a" "$b" 2>/dev/null
  [ "$st_ok" -eq 1 ] && { echo "SELF-TEST: PASS"; exit 0; }
  echo "SELF-TEST: FAIL"; exit 1
fi

CRATE="${1:?crate}"; FILTER="${2:-}"; NCHUNKS="${3:-6}"; CEIL_MIN="${4:-45}"; TMO_MIN="${5:-0}"
# Derive ROOT from git rather than hardcoding the main checkout: a hardcoded
# path silently tests the wrong tree when this script is invoked from a
# worktree, which can report a green Miri lane for code that was never tested.
ROOT="$(git rev-parse --show-toplevel)" || exit 2
cd "$ROOT" || exit 2
# Unique per run (N31): a fixed `/tmp/mirirun.$CRATE` that each run `rm -rf`d
# let a second session delete a first one's in-flight output, which then read
# as failures for tests that had passed.
WORK="$(new_workdir "$CRATE")" || exit 2
mkdir -p "$WORK/out"
PROG="$WORK/progress.log"; : > "$PROG"
ABORT="$WORK/ABORT"
now() { date +%s; }
hms() { local s=$1; printf '%dh%02dm%02ds' $((s/3600)) $(((s%3600)/60)) $((s%60)); }
log() { echo "[$(date +%H:%M:%S)] $*" | tee -a "$PROG"; }
# Recursively kill a process and all descendants (cargo -> miri runner -> test binary).
killtree() { local p=$1 c; for c in $(pgrep -P "$p" 2>/dev/null); do killtree "$c"; done; kill -9 "$p" 2>/dev/null; }
export -f killtree 2>/dev/null || true

log "=== miri-run: crate=$CRATE filter='$FILTER' chunks=$NCHUNKS ceiling=${CEIL_MIN}m hard_timeout=${TMO_MIN}m work=$WORK ==="
# Each worker re-invokes `cargo miri test`, which rebuilds if the tree changed:
# this prebuild only warms the cache. Do not edit the tree during a run (a
# rebuild that fails is reported as BUILD and aborts the sweep).
log "prebuilding the Miri test binary..."
if ! cargo +nightly miri test -p "$CRATE" --no-run >"$WORK/build.log" 2>&1; then
  log "BUILD FAILED — see $WORK/build.log"; tail -5 "$WORK/build.log" | tee -a "$PROG"; exit 1
fi
cargo +nightly test -p "$CRATE" -- $FILTER --list 2>/dev/null | grep ': test$' | sed 's/: test$//' | sort > "$WORK/all.txt"
TOTAL=$(wc -l < "$WORK/all.txt" | tr -d ' ')
[ "$TOTAL" -eq 0 ] && { log "NO TESTS matched '$FILTER' — abort"; exit 1; }
awk -v n="$NCHUNKS" -v w="$WORK" '{print > (w"/chunk." (NR % n) ".txt")}' "$WORK/all.txt"
log "discovered $TOTAL tests -> $NCHUNKS workers"
START_TS=$(now)

worker() {
  local id="$1" cf="$2"
  while IFS= read -r t; do
    [ -n "$t" ] || continue
    [ -e "$ABORT" ] && break
    echo "$(now) START w$id $t" >> "$PROG"
    local o s0 rc
    o="$WORK/out/$id.$(echo "$t" | tr '/:' '__')"
    s0=$(now)
    if [ "$TMO_MIN" -gt 0 ]; then
      ( MIRIFLAGS="-Zmiri-disable-isolation" cargo +nightly miri test -p "$CRATE" -- --exact "$t" --test-threads=1 >"$o" 2>&1 ) &
      local cpid=$!
      ( sleep $((TMO_MIN*60)); killtree "$cpid" 2>/dev/null ) & local kpid=$!
      wait "$cpid" 2>/dev/null; rc=$?
      kill "$kpid" 2>/dev/null; wait "$kpid" 2>/dev/null
    else
      MIRIFLAGS="-Zmiri-disable-isolation" cargo +nightly miri test -p "$CRATE" -- --exact "$t" --test-threads=1 >"$o" 2>&1; rc=$?
    fi
    local d=$(( $(now) - s0 )) res
    res="$(classify "$o" "$d" "$TMO_MIN")"
    echo "$(now) DONE  w$id $t rc=$rc dur=${d}s res=$res" >> "$PROG"
    if [ "$res" = BUILD ]; then echo "$t" > "$ABORT"; break; fi
  done < "$cf"
  echo "$(now) WORKER_DONE w$id" >> "$PROG"
}
for i in $(seq 0 $((NCHUNKS-1))); do [ -f "$WORK/chunk.$i.txt" ] && worker "$i" "$WORK/chunk.$i.txt" & done

heartbeat() {
  while :; do
    sleep 60
    # `ndone`, not `done`: a bare `done` here parses as the loop keyword.
    local ndone ubn running miricpu
    ndone=$(grep -c ' DONE ' "$PROG"); ubn=$(grep -c ' res=UB' "$PROG")
    running=""
    for i in $(seq 0 $((NCHUNKS-1))); do
      local last; last=$(grep " w$i " "$PROG" | tail -1)
      case "$last" in *"START w$i "*)
        local nm st el; nm=$(echo "$last"|awk '{print $4}'); st=$(echo "$last"|awk '{print $1}'); el=$(( $(now)-st ))
        running+="w$i:$(basename "$nm")(${el}s)"; [ $((el/60)) -ge "$CEIL_MIN" ] && running+="[SLOW]"; running+=" ";;
      esac
    done
    miricpu=$(ps -A -o %cpu,command | grep -E 'miri|rustty_vt-' | grep -v grep | awk '{s+=$1} END{printf "%.0f", s}')
    echo "[$(date +%H:%M:%S)] HEARTBEAT done=$ndone/$TOTAL ub=$ubn elapsed=$(hms $(( $(now)-START_TS ))) miri_cpu=${miricpu}% running: ${running:-<none>}" >> "$PROG"
    [ "$(grep -c WORKER_DONE "$PROG")" -ge "$NCHUNKS" ] && break
  done
}
heartbeat & HB=$!
for j in $(jobs -p); do [ "$j" = "$HB" ] || wait "$j" 2>/dev/null; done
kill "$HB" 2>/dev/null

DONEN=$(grep -c ' DONE ' "$PROG"); OKN=$(grep -c ' res=ok' "$PROG")
UBN=$(grep -c ' res=UB' "$PROG"); FAILN=$(grep -c ' res=FAIL' "$PROG"); TMON=$(grep -c ' res=TIMEOUT' "$PROG")
LEAKN=$(grep -c ' res=LEAK' "$PROG"); INTN=$(grep -c ' res=INTERP' "$PROG")
if [ -e "$ABORT" ]; then
  log "=== ABORTED: BUILD failure while running $(cat "$ABORT") — the tree changed under the run; results are void. See $WORK/out/"
  log "VERDICT: BUILD-BROKEN"
  exit 1
fi
log "=== FINAL: completed=$DONEN/$TOTAL  ok=$OKN  UB=$UBN  LEAK=$LEAKN  FAIL=$FAILN  INTERP=$INTN  TIMEOUT=$TMON  elapsed=$(hms $(( $(now)-START_TS )))"
if [ "$UBN" -gt 0 ] || [ "$LEAKN" -gt 0 ] || [ "$FAILN" -gt 0 ]; then
  log "PROBLEMS (UB/LEAK/FAIL — must fix):"; grep -E ' res=(UB|LEAK|FAIL)' "$PROG" | sed 's/^/  /' | tee -a "$PROG"
fi
if [ "$INTN" -gt 0 ]; then
  log "INTERP (a Miri interpreter limit, not UB — needs a Miri exception or a cfg(miri) scope):"
  grep ' res=INTERP' "$PROG" | awk '{print "  "$4}' | tee -a "$PROG"
fi
if [ "$TMON" -gt 0 ]; then
  log "TIMEOUT (>${TMO_MIN}m — candidate Miri-exceptions, need strict-criteria review):"
  grep ' res=TIMEOUT' "$PROG" | awk '{print "  "$4" ("$6")"}' | tee -a "$PROG"
fi
if [ "$DONEN" -eq "$TOTAL" ] && [ "$UBN" -eq 0 ] && [ "$LEAKN" -eq 0 ] && [ "$FAILN" -eq 0 ] && [ "$TMON" -eq 0 ] && [ "$INTN" -eq 0 ]; then
  log "VERDICT: ALL-$TOTAL-MIRI-CLEAN"
elif [ "$DONEN" -eq "$TOTAL" ] && [ "$UBN" -eq 0 ] && [ "$LEAKN" -eq 0 ] && [ "$FAILN" -eq 0 ]; then
  log "VERDICT: MIRI-CLEAN-EXCEPT-$TMON-TIMEOUTS-$INTN-INTERP (ok=$OKN, these need exception review)"
else
  log "VERDICT: PROBLEM (completed $DONEN/$TOTAL, ub=$UBN, leak=$LEAKN, fail=$FAILN, interp=$INTN, timeout=$TMON)"
  exit 1
fi
