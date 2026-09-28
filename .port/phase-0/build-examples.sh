#!/bin/zsh
set -u
OUT=/Users/gael/projects/rustty/.port/phase-0/examples-out
EX=/Users/gael/projects/ghostty/example
examples=(c-vt c-vt-build-info c-vt-colors c-vt-effects c-vt-encode-focus c-vt-encode-key c-vt-encode-mouse c-vt-formatter c-vt-grid-ref-tracked c-vt-grid-traverse c-vt-kitty-graphics c-vt-modes c-vt-paste c-vt-render c-vt-selection c-vt-selection-gesture c-vt-sgr c-vt-size-report c-vt-static c-vt-stream cpp-vt-stream c-vt-search c-vt-snapshot)
for d in $examples; do
  echo "===== BUILD $d ====="
  prefix="$OUT/$d/zig-out"
  cache="$OUT/$d/.zig-cache"
  mkdir -p "$prefix" "$cache"
  ( cd "$EX/$d" && zig build --prefix "$prefix" --cache-dir "$cache" ) > "$OUT/$d.log" 2>&1
  echo "EXIT $d = $?"
done
echo "ALL_EXAMPLES_DONE"
