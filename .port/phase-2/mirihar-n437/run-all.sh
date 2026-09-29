#!/usr/bin/env bash
# Run every candidate under BOTH aliasing models, over 16 scheduler seeds each,
# and print a verdict table. Usage: bash run-all.sh    (from this directory)
set -u
printf '%-26s %-6s %-6s\n' candidate SB TB
for c in shipped_update_frame shipped_animation_wake shipped_set_focus \
         inline_lock_whole_mut latest_frame_inline \
         fixed_vs_sync_draw fixed_vs_gtk_main \
         split_vs_sync_draw_whole split_vs_gtk_main_whole \
         split_vs_sync_draw_narrow split_vs_gtk_main_narrow; do
  sb=CLEAN; tb=CLEAN
  MIRIFLAGS="-Zmiri-many-seeds=0..16" \
    cargo +nightly miri run -q -- "$c" >"/tmp/mirihar-n437-$c-sb.log" 2>&1 || sb=UB
  MIRIFLAGS="-Zmiri-tree-borrows -Zmiri-many-seeds=0..16" \
    cargo +nightly miri run -q -- "$c" >"/tmp/mirihar-n437-$c-tb.log" 2>&1 || tb=UB
  printf '%-26s %-6s %-6s\n' "$c" "$sb" "$tb"
done
