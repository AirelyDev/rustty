#!/usr/bin/env bash
# Run every candidate under BOTH aliasing models and print a verdict table.
# Usage: bash run-all.sh    (from this directory)
set -u
printf '%-26s %-6s %-6s\n' candidate SB TB
for m in mutref raw; do
  for s in set_hyperlink clear_hyperlink set_graphemes append_grapheme move_hyperlink held_across; do
    c="${m}_${s}"
    sb=CLEAN; tb=CLEAN
    cargo +nightly miri run -q -- "$c" >"/tmp/mirihar-p7-hashmap-$c-sb.log" 2>&1 || sb=UB
    MIRIFLAGS=-Zmiri-tree-borrows cargo +nightly miri run -q -- "$c" >"/tmp/mirihar-p7-hashmap-$c-tb.log" 2>&1 || tb=UB
    printf '%-26s %-6s %-6s\n' "$c" "$sb" "$tb"
  done
done
