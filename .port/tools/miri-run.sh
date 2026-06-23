#!/usr/bin/env bash
# miri-run.sh — full Miri verification with LIVE progress + stall detection.
#
# Why this exists: `cargo miri test` prints nothing until the whole (multi-hour)
# run finishes, so a slow-but-progressing run is indistinguishable from a hang.
# This harness runs EVERY matching test under Miri (no scoping/skipping — R6 full
# coverage), parallelized into workers, and emits a per-test START/DONE log plus a
# periodic heartbeat rollup so progress is always observable and a true stall
# (dead worker / CPU-idle test) is distinguishable from heavy grinding.
#
# Usage: bash miri-run.sh <crate> <filter> <nchunks> [ceiling_min]
#   crate      e.g. rustty-vt
#   filter     test-name substring, e.g. pagelist::tests::   (use '' for all)
#   nchunks    parallel workers, e.g. 6
#   ceiling_min soft per-test "running unusually long" threshold (default 45)
set -u
CRATE="${1:?crate}"; FILTER="${2:-}"; NCHUNKS="${3:-6}"; CEIL_MIN="${4:-45}"
ROOT="/Users/gael/projects/rustty"; cd "$ROOT" || exit 2
WORK="/tmp/mirirun.$CRATE"; rm -rf "$WORK"; mkdir -p "$WORK/out"
PROG="$WORK/progress.log"; : > "$PROG"
now() { date +%s; }
hms() { local s=$1; printf '%dh%02dm%02ds' $((s/3600)) $(((s%3600)/60)) $((s%60)); }
log() { echo "[$(date +%H:%M:%S)] $*" | tee -a "$PROG"; }

log "=== miri-run: crate=$CRATE filter='$FILTER' chunks=$NCHUNKS ceiling=${CEIL_MIN}m ==="
log "building Miri test binary once (cargo +nightly miri test --no-run)..."
if ! cargo +nightly miri test -p "$CRATE" --no-run >"$WORK/build.log" 2>&1; then
  log "BUILD FAILED — see $WORK/build.log"; tail -5 "$WORK/build.log" | tee -a "$PROG"; exit 1
fi
# Authoritative test list (exact names), partitioned round-robin into NCHUNKS files.
cargo +nightly test -p "$CRATE" -- $FILTER --list 2>/dev/null | grep ': test$' | sed 's/: test$//' | sort > "$WORK/all.txt"
TOTAL=$(wc -l < "$WORK/all.txt" | tr -d ' ')
if [ "$TOTAL" -eq 0 ]; then log "NO TESTS matched filter '$FILTER' — abort"; exit 1; fi
awk -v n="$NCHUNKS" -v w="$WORK" '{print > (w"/chunk." (NR % n) ".txt")}' "$WORK/all.txt"
log "discovered $TOTAL tests -> $NCHUNKS workers"
START_TS=$(now)

# Worker: run its tests one-at-a-time, logging START/DONE+dur+result+UB per test.
worker() {
  local id="$1" cf="$2"
  while IFS= read -r t; do
    [ -n "$t" ] || continue
    echo "$(now) START w$id $t" >> "$PROG"
    local o="$WORK/out/$id.$(echo "$t" | tr '/:' '__')"
    local s0=$(now)
    MIRIFLAGS="-Zmiri-disable-isolation" cargo +nightly miri test -p "$CRATE" -- --exact "$t" --test-threads=1 >"$o" 2>&1
    local rc=$? d=$(( $(now) - s0 )) res ub=0
    grep -qE 'Undefined Behavior|memory leaked|error: aborting' "$o" && ub=1
    if [ $ub -eq 1 ]; then res=UB; elif grep -q '1 passed' "$o"; then res=ok; else res=FAIL; fi
    echo "$(now) DONE  w$id $t rc=$rc dur=${d}s res=$res" >> "$PROG"
  done < "$cf"
  echo "$(now) WORKER_DONE w$id" >> "$PROG"
}
for i in $(seq 0 $((NCHUNKS-1))); do [ -f "$WORK/chunk.$i.txt" ] && worker "$i" "$WORK/chunk.$i.txt" & done

# Heartbeat: every 60s emit a rollup (done/total, per-worker current test + elapsed,
# CPU of the running miri procs => grinding vs hung). Runs until all workers finish.
heartbeat() {
  while :; do
    sleep 60
    local done running cpu
    done=$(grep -c ' DONE ' "$PROG")
    local ubn; ubn=$(grep -c ' res=UB' "$PROG")
    # current test per worker = last START after its last DONE
    running=""
    for i in $(seq 0 $((NCHUNKS-1))); do
      local last; last=$(grep " w$i " "$PROG" | tail -1)
      case "$last" in
        *"START w$i "*)
          local nm st el; nm=$(echo "$last" | awk '{print $4}'); st=$(echo "$last" | awk '{print $1}')
          el=$(( $(now) - st )); running+="w$i:$(basename "$nm")(${el}s) "
          [ $((el/60)) -ge "$CEIL_MIN" ] && running+="[SLOW] "
          ;;
      esac
    done
    cpu=$(ps -A -o %cpu,comm | grep -c 'rustty_vt\|miri' 2>/dev/null)
    local miricpu; miricpu=$(ps -A -o %cpu,command | grep -E 'miri|rustty_vt-' | grep -v grep | awk '{s+=$1} END{printf "%.0f", s}')
    echo "[$(date +%H:%M:%S)] HEARTBEAT done=$done/$TOTAL ub=$ubn elapsed=$(hms $(( $(now)-START_TS ))) miri_cpu=${miricpu}% running: ${running:-<none>}" >> "$PROG"
    # stop heartbeat when all workers done
    [ "$(grep -c WORKER_DONE "$PROG")" -ge "$NCHUNKS" ] && break
  done
}
heartbeat &
HB=$!
wait $(jobs -p | grep -v "$HB" 2>/dev/null) 2>/dev/null; wait  # wait for workers (and heartbeat exits on its own)
kill "$HB" 2>/dev/null

# Final aggregation — verify count == TOTAL so a 0-run can't masquerade as clean.
DONEN=$(grep -c ' DONE ' "$PROG"); OKN=$(grep -c ' res=ok' "$PROG")
UBN=$(grep -c ' res=UB' "$PROG"); FAILN=$(grep -c ' res=FAIL' "$PROG")
log "=== FINAL: completed=$DONEN/$TOTAL  ok=$OKN  UB=$UBN  FAIL=$FAILN  elapsed=$(hms $(( $(now)-START_TS )))"
if [ "$UBN" -gt 0 ] || [ "$FAILN" -gt 0 ]; then
  log "PROBLEMS:"; grep -E ' res=(UB|FAIL)' "$PROG" | sed 's/^/  /' | tee -a "$PROG"
  grep -rlE 'Undefined Behavior|memory leaked' "$WORK/out" 2>/dev/null | head | tee -a "$PROG"
fi
if [ "$DONEN" -eq "$TOTAL" ] && [ "$UBN" -eq 0 ] && [ "$FAILN" -eq 0 ]; then
  log "VERDICT: ALL-$TOTAL-MIRI-CLEAN"
else
  log "VERDICT: INCOMPLETE-OR-PROBLEM (completed $DONEN/$TOTAL, ub=$UBN, fail=$FAILN)"
fi
