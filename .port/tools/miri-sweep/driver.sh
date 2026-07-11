#!/bin/bash
LIST="$1"; WORKERS="${2:-4}"; TMO="${3:-20}"
DIR="$(dirname "$LIST")"; OUT="$DIR/out"; PROG="$DIR/progress.log"; PT="$DIR/ptimeout.pl"
mkdir -p "$OUT"
echo "=== driver: $(grep -c . "$LIST") tests, $WORKERS workers, ${TMO}m timeout ===" > "$PROG"
run_one() {
  local t="$1"; [ -z "$t" ] && return; local safe="${t//::/__}"
  local start=$SECONDS
  perl "$PT" $((TMO*60)) cargo +nightly miri test -p rustty-vt --lib -- --exact "$t" --test-threads=1 > "$OUT/$safe" 2>&1
  local rc=$?; local dur=$((SECONDS-start)); local res
  if [ $rc -eq 0 ]; then res=ok
  elif [ $rc -eq 137 ]; then res=TIMEOUT
  elif grep -q "Undefined Behavior" "$OUT/$safe" 2>/dev/null; then res=UB
  elif grep -q "internal compiler error" "$OUT/$safe" 2>/dev/null; then res=ICE
  else res=FAIL; fi
  echo "$(date +%s) DONE $t rc=$rc dur=${dur}s res=$res" >> "$PROG"
}
export -f run_one; export OUT PROG TMO PT
grep . "$LIST" | xargs -P "$WORKERS" -I{} bash -c 'run_one "$@"' _ {}
echo "$(date +%s) === driver complete ===" >> "$PROG"
