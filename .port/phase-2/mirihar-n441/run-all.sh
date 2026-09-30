#!/usr/bin/env bash
# Run every candidate under BOTH aliasing models, over 16 scheduler seeds each,
# and print a verdict table. Usage: bash run-all.sh    (from this directory)
# A non-zero Miri exit is reported as UB, or LEAK when Miri's leak check fired.
set -u
printf '%-34s %-6s %-6s\n' candidate SB TB
verdict() {
  if [ "$1" -eq 0 ]; then echo CLEAN
  elif grep -q "Undefined Behavior" "$2"; then echo UB
  elif grep -q "memory leaked" "$2"; then echo LEAK
  else echo "ERR($1)"; fi
}
for c in n441_inline_renderer n441_boxed_renderer \
         n441_residual_terminal_inline n441_residual_terminal_boxed \
         n441_box_drop_leak n441_box_dealloc_after_deinit \
         n441_r1_drop_only_fields_leak n441_r1_display_after_free n441_r2_deinit_then_drop \
         n442_shipped_key_route n442_shipped_binding_new_split \
         n442_shipped_binding_new_tab n442_shipped_binding_new_window \
         n442_raw_this_spans \
         r0_binding_new_window r0_key_route_new_split \
         r0_binding_reload_config r0_binding_quit \
         r0_app_tick_reload_config r0_app_key_global_reload_config \
         r0_all_new_split_live_iter \
         fixed_key_route_all fixed_binding_all fixed_app_tick_all fixed_app_key_global_all \
         fixed_all_new_split_snapshot; do
  sbl="/tmp/mirihar-n441-$c-sb.log"; tbl="/tmp/mirihar-n441-$c-tb.log"
  MIRIFLAGS="-Zmiri-many-seeds=0..16" cargo +nightly miri run -q -- "$c" >"$sbl" 2>&1; sb=$?
  MIRIFLAGS="-Zmiri-tree-borrows -Zmiri-many-seeds=0..16" cargo +nightly miri run -q -- "$c" >"$tbl" 2>&1; tb=$?
  printf '%-34s %-6s %-6s\n' "$c" "$(verdict $sb "$sbl")" "$(verdict $tb "$tbl")"
done
