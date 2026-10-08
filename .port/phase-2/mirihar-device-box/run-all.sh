#!/usr/bin/env bash
# Run every candidate under BOTH aliasing models, over 8 scheduler seeds each,
# and print a verdict table. Usage: bash run-all.sh    (from this directory)
# A non-zero Miri exit is reported as UB, or LEAK when Miri's leak check fired.
set -u
printf '%-36s %-6s %-6s\n' candidate SB TB
verdict() {
  if [ "$1" -eq 0 ]; then echo CLEAN
  elif grep -q "Undefined Behavior" "$2"; then echo UB
  elif grep -q "memory leaked" "$2"; then echo LEAK
  else echo "ERR($1)"; fi
}
mkdir -p outputs
for c in inline_device boxed_device boxed_device_lifecycle boxed_device_null \
         boxed_device_freed_before_surfaces; do
  sbl="outputs/$c.sb.log"; tbl="outputs/$c.tb.log"
  MIRIFLAGS="-Zmiri-many-seeds=0..8" cargo +nightly miri run -q -- "$c" >"$sbl" 2>&1; sb=$?
  MIRIFLAGS="-Zmiri-tree-borrows -Zmiri-many-seeds=0..8" cargo +nightly miri run -q -- "$c" >"$tbl" 2>&1; tb=$?
  printf '%-36s %-6s %-6s\n' "$c" "$(verdict $sb "$sbl")" "$(verdict $tb "$tbl")"
done
