# Provisional findings, G6c/G6t runner (Mac)

## N542 (id renumbered at P8-W9 T3): G6t RED, Tree Borrows UB in rustty-vt page::tests::page_moveCells_{text_only,graphemes}
- Gate: `check_leaks.sh --tree-borrows`, rustty-vt scope, debug assertions on. Failing tests:
  `page::tests::page_moveCells_text_only`, `page::tests::page_moveCells_graphemes`.
- Miri: `write access through <tag> at alloc[0x8] is forbidden` at crates/rustty-vt/src/page.rs:604
  (`set_bits`, via `Row::set_styled` <- `Page::move_cells_inner` page_impl.rs:2287).
- Cause per Miri: the test passes `&mut *dst` (crates/rustty-vt/src/page_tests.rs:2319, tag Reserved); a later
  reborrow `core::slice::from_raw_parts(self.rows.ptr(base), ...)` at page_impl.rs:735 acts as a foreign read
  and makes that tag Frozen; the subsequent write through the child tag in set_bits is then forbidden.
  A `&mut Row` argument re-read via the page's rows slice across the call (known class: &mut args vs raw-ptr args).
- Not fixed, not relaxed (R5). Evidence: .port/phase-8/w9/G6t-at-a60e9e2a5.txt.
- SB (G6) passes these tests; this is TB-only. Unknown whether present at the prior pin (not checked).

## N543 (id renumbered at P8-W9 T3): G6t incomplete, rustty-core not run to completion
- Mac free disk fell from 62 GB to ~12 GB over ~2 h while other tasks built (my target dir was a stable ~7 GB);
  runner stopped at the 12 GB floor, killing its own processes by PID. rustty-core G6t is BLOCKED, not green.
