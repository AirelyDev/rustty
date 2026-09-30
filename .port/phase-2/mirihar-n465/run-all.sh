#!/usr/bin/env bash
# Run every candidate under BOTH aliasing models, over 16 seeds each, and print
# a verdict table. Usage: bash run-all.sh    (from this directory)
# A non-zero Miri exit is reported as UB, or LEAK when Miri's leak check fired.
set -u
printf '%-40s %-6s %-6s\n' candidate SB TB
verdict() {
  if [ "$1" -eq 0 ]; then echo CLEAN
  elif grep -q "Undefined Behavior" "$2"; then echo UB
  elif grep -q "memory leaked" "$2"; then echo LEAK
  else echo "ERR($1)"; fi
}
for c in n465_shipped_macos_child_exited n465_shipped_gtk_child_exited \
         n465_core_only_macos_child_exited \
         n465_fixed_macos_child_exited n465_fixed_gtk_child_exited \
         n465_fixed_macos_all n465_fixed_gtk_all; do
  sbl="/tmp/mirihar-n465-$c-sb.log"; tbl="/tmp/mirihar-n465-$c-tb.log"
  MIRIFLAGS="-Zmiri-many-seeds=0..16" cargo +nightly miri run -q -- "$c" >"$sbl" 2>&1; sb=$?
  MIRIFLAGS="-Zmiri-tree-borrows -Zmiri-many-seeds=0..16" cargo +nightly miri run -q -- "$c" >"$tbl" 2>&1; tb=$?
  printf '%-40s %-6s %-6s\n' "$c" "$(verdict $sb "$sbl")" "$(verdict $tb "$tbl")"
done
