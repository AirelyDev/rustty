#!/usr/bin/env bash
# Run every candidate under BOTH aliasing models, over 16 scheduler seeds each,
# and print a verdict table. Usage: bash run-all.sh    (from this directory)
set -u
printf '%-34s %-6s %-6s\n' candidate SB TB
for c in n441_inline_renderer n441_boxed_renderer \
         n441_residual_terminal_inline n441_residual_terminal_boxed \
         n442_shipped_key_route n442_shipped_binding_new_split \
         n442_shipped_binding_new_tab n442_shipped_binding_new_window \
         n442_new_window_app_mut n442_raw_this_spans \
         n442_raw_key_route n442_raw_binding_all \
         n443_quit_app_mut n443_quit_app_ref; do
  sb=CLEAN; tb=CLEAN
  MIRIFLAGS="-Zmiri-many-seeds=0..16" \
    cargo +nightly miri run -q -- "$c" >"/tmp/mirihar-n441-$c-sb.log" 2>&1 || sb=UB
  MIRIFLAGS="-Zmiri-tree-borrows -Zmiri-many-seeds=0..16" \
    cargo +nightly miri run -q -- "$c" >"/tmp/mirihar-n441-$c-tb.log" 2>&1 || tb=UB
  printf '%-34s %-6s %-6s\n' "$c" "$sb" "$tb"
done
