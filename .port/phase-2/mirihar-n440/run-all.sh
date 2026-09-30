#!/usr/bin/env bash
# Run every candidate under BOTH aliasing models, over 16 scheduler seeds each,
# and print a verdict table. Usage: bash run-all.sh    (from this directory)
set -u
printf '%-28s %-6s %-6s\n' candidate SB TB
for c in shipped_async_handler fixed_async_handler fixed_async_and_sync_draw; do
  sb=CLEAN; tb=CLEAN
  MIRIFLAGS="-Zmiri-many-seeds=0..16" \
    cargo +nightly miri run -q -- "$c" >"/tmp/mirihar-n440-$c-sb.log" 2>&1 || sb=UB
  MIRIFLAGS="-Zmiri-tree-borrows -Zmiri-many-seeds=0..16" \
    cargo +nightly miri run -q -- "$c" >"/tmp/mirihar-n440-$c-tb.log" 2>&1 || tb=UB
  printf '%-28s %-6s %-6s\n' "$c" "$sb" "$tb"
done
