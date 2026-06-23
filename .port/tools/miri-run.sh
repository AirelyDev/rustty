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
set -u
CRATE="${1:?crate}"; FILTER="${2:-}"; NCHUNKS="${3:-6}"; CEIL_MIN="${4:-45}"; TMO_MIN="${5:-0}"
ROOT="/Users/gael/projects/rustty"; cd "$ROOT" || exit 2
WORK="/tmp/mirirun.$CRATE"; rm -rf "$WORK"; mkdir -p "$WORK/out"
PROG="$WORK/progress.log"; : > "$PROG"
now() { date +%s; }
hms() { local s=$1; printf '%dh%02dm%02ds' $((s/3600)) $(((s%3600)/60)) $((s%60)); }
log() { echo "[$(date +%H:%M:%S)] $*" | tee -a "$PROG"; }
# Recursively kill a process and all descendants (cargo -> miri runner -> test binary).
killtree() { local p=$1 c; for c in $(pgrep -P "$p" 2>/dev/null); do killtree "$c"; done; kill -9 "$p" 2>/dev/null; }
export -f killtree 2>/dev/null || true

log "=== miri-run: crate=$CRATE filter='$FILTER' chunks=$NCHUNKS ceiling=${CEIL_MIN}m hard_timeout=${TMO_MIN}m ==="
log "building Miri test binary once..."
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
    echo "$(now) START w$id $t" >> "$PROG"
    local o="$WORK/out/$id.$(echo "$t" | tr '/:' '__')"
    local s0=$(now) rc
    if [ "$TMO_MIN" -gt 0 ]; then
      ( MIRIFLAGS="-Zmiri-disable-isolation" cargo +nightly miri test -p "$CRATE" -- --exact "$t" --test-threads=1 >"$o" 2>&1 ) &
      local cpid=$!
      ( sleep $((TMO_MIN*60)); killtree "$cpid" 2>/dev/null ) & local kpid=$!
      wait "$cpid" 2>/dev/null; rc=$?
      kill "$kpid" 2>/dev/null; wait "$kpid" 2>/dev/null
    else
      MIRIFLAGS="-Zmiri-disable-isolation" cargo +nightly miri test -p "$CRATE" -- --exact "$t" --test-threads=1 >"$o" 2>&1; rc=$?
    fi
    local d=$(( $(now) - s0 )) res ub=0
    grep -qE 'Undefined Behavior|memory leaked|error: aborting' "$o" && ub=1
    if [ $ub -eq 1 ]; then res=UB
    elif grep -q '1 passed' "$o"; then res=ok
    elif [ "$TMO_MIN" -gt 0 ] && [ $d -ge $((TMO_MIN*60 - 10)) ]; then res=TIMEOUT
    else res=FAIL; fi
    echo "$(now) DONE  w$id $t rc=$rc dur=${d}s res=$res" >> "$PROG"
  done < "$cf"
  echo "$(now) WORKER_DONE w$id" >> "$PROG"
}
for i in $(seq 0 $((NCHUNKS-1))); do [ -f "$WORK/chunk.$i.txt" ] && worker "$i" "$WORK/chunk.$i.txt" & done

heartbeat() {
  while :; do
    sleep 60
    local done ubn running miricpu
    done=$(grep -c ' DONE ' "$PROG"); ubn=$(grep -c ' res=UB' "$PROG")
    running=""
    for i in $(seq 0 $((NCHUNKS-1))); do
      local last; last=$(grep " w$i " "$PROG" | tail -1)
      case "$last" in *"START w$i "*)
        local nm st el; nm=$(echo "$last"|awk '{print $4}'); st=$(echo "$last"|awk '{print $1}'); el=$(( $(now)-st ))
        running+="w$i:$(basename "$nm")(${el}s)"; [ $((el/60)) -ge "$CEIL_MIN" ] && running+="[SLOW]"; running+=" ";;
      esac
    done
    miricpu=$(ps -A -o %cpu,command | grep -E 'miri|rustty_vt-' | grep -v grep | awk '{s+=$1} END{printf "%.0f", s}')
    echo "[$(date +%H:%M:%S)] HEARTBEAT done=$done/$TOTAL ub=$ubn elapsed=$(hms $(( $(now)-START_TS ))) miri_cpu=${miricpu}% running: ${running:-<none>}" >> "$PROG"
    [ "$(grep -c WORKER_DONE "$PROG")" -ge "$NCHUNKS" ] && break
  done
}
heartbeat & HB=$!
for j in $(jobs -p); do [ "$j" = "$HB" ] || wait "$j" 2>/dev/null; done
kill "$HB" 2>/dev/null

DONEN=$(grep -c ' DONE ' "$PROG"); OKN=$(grep -c ' res=ok' "$PROG")
UBN=$(grep -c ' res=UB' "$PROG"); FAILN=$(grep -c ' res=FAIL' "$PROG"); TMON=$(grep -c ' res=TIMEOUT' "$PROG")
log "=== FINAL: completed=$DONEN/$TOTAL  ok=$OKN  UB=$UBN  FAIL=$FAILN  TIMEOUT=$TMON  elapsed=$(hms $(( $(now)-START_TS )))"
if [ "$UBN" -gt 0 ] || [ "$FAILN" -gt 0 ]; then
  log "PROBLEMS (UB/FAIL — must fix):"; grep -E ' res=(UB|FAIL)' "$PROG" | sed 's/^/  /' | tee -a "$PROG"
fi
if [ "$TMON" -gt 0 ]; then
  log "TIMEOUT (>${TMO_MIN}m — candidate Miri-exceptions, need strict-criteria review):"
  grep ' res=TIMEOUT' "$PROG" | awk '{print "  "$4" ("$6")"}' | tee -a "$PROG"
fi
if [ "$DONEN" -eq "$TOTAL" ] && [ "$UBN" -eq 0 ] && [ "$FAILN" -eq 0 ] && [ "$TMON" -eq 0 ]; then
  log "VERDICT: ALL-$TOTAL-MIRI-CLEAN"
elif [ "$DONEN" -eq "$TOTAL" ] && [ "$UBN" -eq 0 ] && [ "$FAILN" -eq 0 ]; then
  log "VERDICT: MIRI-CLEAN-EXCEPT-$TMON-TIMEOUTS (ok=$OKN, timeouts need exception review)"
else
  log "VERDICT: PROBLEM (completed $DONEN/$TOTAL, ub=$UBN, fail=$FAILN, timeout=$TMON)"
fi
