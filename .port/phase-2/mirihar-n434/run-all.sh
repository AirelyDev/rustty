#!/usr/bin/env bash
# Run every candidate under BOTH aliasing models and print a verdict table.
# Usage: bash run-all.sh    (from this directory)
set -u
printf '%-30s %-6s %-6s\n' candidate SB TB
for b in kqueue iocp wasi epoll; do
  for s in shipped fixed; do
    for p in reset cancel; do
      for t in root inner; do
        c="${b}_${s}_${p}_${t}"
        sb=CLEAN; tb=CLEAN
        cargo +nightly miri run -q -- "$c" >"/tmp/mirihar-n434-$c-sb.log" 2>&1 || sb=UB
        MIRIFLAGS=-Zmiri-tree-borrows cargo +nightly miri run -q -- "$c" >"/tmp/mirihar-n434-$c-tb.log" 2>&1 || tb=UB
        printf '%-30s %-6s %-6s\n' "$c" "$sb" "$tb"
      done
    done
  done
done
