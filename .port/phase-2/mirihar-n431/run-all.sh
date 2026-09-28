#!/usr/bin/env bash
# Run every candidate under BOTH aliasing models and print a verdict table.
# Usage: bash run-all.sh    (from this directory)
set -u
cands="shipped_thread_root shipped_local_root shipped_thread_l shipped_local_l shipped_thread_wasi shipped_local_wasi shipped_thread_sync
wrapper_thread_root wrapper_local_root wrapper_thread_l wrapper_local_l
rawmutstart_thread_sync
raw_thread_root raw_local_root raw_thread_l raw_local_l raw_thread_wasi raw_local_wasi raw_thread_sync raw_local_sync"
printf '%-26s %-6s %-6s\n' candidate SB TB
for c in $cands; do
  sb=CLEAN; tb=CLEAN
  cargo +nightly miri run -q -- "$c" >"/tmp/mirihar-n431-$c-sb.log" 2>&1 || sb=UB
  MIRIFLAGS=-Zmiri-tree-borrows cargo +nightly miri run -q -- "$c" >"/tmp/mirihar-n431-$c-tb.log" 2>&1 || tb=UB
  printf '%-26s %-6s %-6s\n' "$c" "$sb" "$tb"
done
