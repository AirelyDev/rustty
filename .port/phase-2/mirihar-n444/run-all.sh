#!/usr/bin/env bash
# Run every candidate under BOTH aliasing models, over 16 scheduler seeds each,
# and print a verdict table. Usage: bash run-all.sh    (from this directory)
# A non-zero Miri exit is reported as UB, or LEAK when Miri's leak check fired.
set -u
printf '%-36s %-6s %-6s\n' candidate SB TB
verdict() {
  if [ "$1" -eq 0 ]; then echo CLEAN
  elif grep -q "Undefined Behavior" "$2"; then echo UB
  elif grep -q "memory leaked" "$2"; then echo LEAK
  else echo "ERR($1)"; fi
}
for c in n444_inline_io_terminal n444_inline_io_thread \
         n444_inline_renderer_thread_state n444_inline_renderer_state_atomics \
         n444_inline_search_state \
         n444_boxed_all \
         n444_boxed_queue_io_termio_mut n444_boxed_queue_io_send_mut \
         n444_drop_box_leak n444_init_error_io_slot_leak n444_teardown_all_paths \
         n444_search_cb_reads_surface n444_search_cb_ctx \
         n449_writer_termio_mut n449_writer_termio_raw \
         n449_shipped n449_main_backend_mut n449_pty_set_size_mut \
         n449_close_confirm_backend_ref n449_change_config_terminal_early \
         n449_fixed \
         ud367_start_unordered ud367_ready_in_subprocess ud367_ready_in_exec \
         ud368_size_before_lock ud368_size_under_lock; do
  sbl="/tmp/mirihar-n444-$c-sb.log"; tbl="/tmp/mirihar-n444-$c-tb.log"
  MIRIFLAGS="-Zmiri-many-seeds=0..16" cargo +nightly miri run -q -- "$c" >"$sbl" 2>&1; sb=$?
  MIRIFLAGS="-Zmiri-tree-borrows -Zmiri-many-seeds=0..16" cargo +nightly miri run -q -- "$c" >"$tbl" 2>&1; tb=$?
  printf '%-36s %-6s %-6s\n' "$c" "$(verdict $sb "$sbl")" "$(verdict $tb "$tbl")"
done
