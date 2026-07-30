#!/usr/bin/env bash
# Run every candidate under BOTH aliasing models and print a verdict table.
# Usage: bash run-all.sh    (from this directory)
set -u
cands="plain_naive plain_selfptr cell_selfptr inline_cached plain_rawtick boxq
       xthread_naive xthread_shipped xthread_boxq
       wakeup_naive wakeup_copied wakeup_completion_cached
       taskloop_lineage_naive taskloop_lineage_rooted"
printf '%-26s %-6s %-6s\n' candidate SB TB
for c in $cands; do
  sb=CLEAN; tb=CLEAN
  cargo +nightly miri run -q -- "$c" >/dev/null 2>&1 || sb=UB
  MIRIFLAGS=-Zmiri-tree-borrows cargo +nightly miri run -q -- "$c" >/dev/null 2>&1 || tb=UB
  printf '%-26s %-6s %-6s\n' "$c" "$sb" "$tb"
done
