#!/usr/bin/env bash
# Run every candidate under both aliasing models and print a verdict table.
# Usage: bash run-all.sh    (from this directory)
set -u
printf '%-24s %-6s %-6s\n' candidate SB TB
verdict() {
  if [ "$1" -eq 0 ]; then echo CLEAN
  elif grep -q "Undefined Behavior" "$2"; then echo UB
  else echo "ERR($1)"; fi
}
mkdir -p outputs
for c in mut_parser_strict raw_place_strict mut_parser_exposed raw_place_exposed; do
  sbl="outputs/$c.sb.log"; tbl="outputs/$c.tb.log"
  cargo +nightly miri run -q -- "$c" >"$sbl" 2>&1; sb=$?
  MIRIFLAGS="-Zmiri-tree-borrows" cargo +nightly miri run -q -- "$c" >"$tbl" 2>&1; tb=$?
  printf '%-24s %-6s %-6s\n' "$c" "$(verdict $sb "$sbl")" "$(verdict $tb "$tbl")"
done
