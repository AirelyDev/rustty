#!/usr/bin/env bash
# Run every candidate under BOTH aliasing models and print a verdict table.
# Usage: bash run-all.sh    (from this directory)
set -u
cands="loopref_mutop loopref_sharedop rawread_mutop rawread_sharedop cached_mutop cached_sharedop"
printf '%-20s %-6s %-6s\n' candidate SB TB
for c in $cands; do
  sb=CLEAN; tb=CLEAN
  cargo +nightly miri run -q -- "$c" >"/tmp/mirihar-n385-$c-sb.log" 2>&1 || sb=UB
  MIRIFLAGS=-Zmiri-tree-borrows cargo +nightly miri run -q -- "$c" >"/tmp/mirihar-n385-$c-tb.log" 2>&1 || tb=UB
  printf '%-20s %-6s %-6s\n' "$c" "$sb" "$tb"
done
