# W2 reflow page-recycle + bulk copyRun aliasing Miri harness

Isolated, **non-FFI** reproduction of the exact borrow structure of the W2
reflow end-state chain (upstream `46276d046` "recycle pages within a column
reflow" riding on `c249b9de3`/`88ed6bebf` `bulkRunLength`/`copyRun`):
`crates/rustty-vt/src/pagelist/resize.rs` (`resize_cols_inner`,
`ReflowCursor::{reflow_row, copy_run}`) over
`crates/rustty-vt/src/pagelist/mod.rs` (`PageList::create_page` recycle path),
used to PROVE Stacked-Borrows + Tree-Borrows soundness of the
src-page-memory-recycled-as-dst discipline named the top SB hazard of this
range (assessment A §4.1–4.2, plan Global Constraints).

What it proves:

* `copy_run` mutating the DESTINATION page — a momentary `&mut` style item
  inside the dst page buffer (`add_with_id`/`use_multiple` ref-count bump)
  plus a raw `copy_nonoverlapping` of the cells — is clean while the SOURCE
  row's `&[Cell]` slice stays live, because src and dst page buffers are
  always different allocations.
* The `46276d046` recycle is clean **while a source slice is live**: after
  node A's last row is reflowed (its slice already dead), a mid-row
  `create_page` during node B's reflow memsets node A's page buffer through
  its stored root pointer, rewrites the node metadata in place, and hands it
  out as the new destination — with node B's source slice live across the
  whole sequence. The ordering (stash only after the last row; the row
  iterator has advanced past the node) is what keeps every live slice out of
  the recycled buffer.
* The defer (`a stashed node must not outlive the reflow`) destroy path and
  fresh readback of every destination page are clean.
* The FOIL (`ub_recycle_live_slice`) shows the banned shape: recycling a
  buffer while a `&[Cell]` slice into that SAME buffer is live — the memset
  through the root provenance invalidates the slice tag and Miri flags the
  very next slice read.

Each candidate is one executed path (Miri only checks executed code),
selected by argv.

Run (from this dir):

```sh
cargo +nightly miri run -q -- ported                                # Stacked Borrows: PASS
MIRIFLAGS=-Zmiri-tree-borrows cargo +nightly miri run -q -- ported  # Tree Borrows: PASS
cargo +nightly miri run -q -- ub_recycle_live_slice                 # documented UB: Miri flags it
```

Proven 2026-08-18 (miri 0.1.0 6f72b5dd5f 2026-07-22): `ported` clean under
both SB and TB; `ub_recycle_live_slice` flagged at the post-recycle slice
read ("attempting a read access using <tag> ... but that tag does not exist
in the borrow stack") — exactly the doctrine's prediction
(`rust-mut-args-strongly-protected` / `shaper-hook-sb-clean-interleave`).
