#!/usr/bin/env bash
# Run every candidate under BOTH aliasing models, over 16 seeds each, and print
# a verdict table. Usage: bash run-all.sh    (from this directory)
# A non-zero Miri exit is reported as UB, or LEAK when Miri's leak check fired.
set -u
printf '%-32s %-6s %-6s\n' candidate SB TB
verdict() {
  if [ "$1" -eq 0 ]; then echo CLEAN
  elif grep -q "Undefined Behavior" "$2"; then echo UB
  elif grep -q "memory leaked" "$2"; then echo LEAK
  else echo "ERR($1)"; fi
}
for c in n452_shipped_resize n452_shipped_set n452_shipped_get_ground \
         n452_shipped_get_handler n452_shipped_continuation \
         n452_raw_stream_only_set n452_stash_only_get_ground \
         n452_raw_stream_whole_ground \
         n452_fixed_resize n452_fixed_set n452_fixed_get_ground \
         n452_fixed_get_handler n452_fixed_continuation n452_fixed_all; do
  sbl="/tmp/mirihar-n452-$c-sb.log"; tbl="/tmp/mirihar-n452-$c-tb.log"
  MIRIFLAGS="-Zmiri-many-seeds=0..16" cargo +nightly miri run -q -- "$c" >"$sbl" 2>&1; sb=$?
  MIRIFLAGS="-Zmiri-tree-borrows -Zmiri-many-seeds=0..16" cargo +nightly miri run -q -- "$c" >"$tbl" 2>&1; tb=$?
  printf '%-32s %-6s %-6s\n' "$c" "$(verdict $sb "$sbl")" "$(verdict $tb "$tbl")"
done
