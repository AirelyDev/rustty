# S7-6 search borrow-shape Miri harness

Isolated, **non-FFI** reproduction of the borrow shapes parity tail S7-6 added to the
vt search (`crates/rustty-vt/src/search/*`, `highlight.rs`, `zig_list.rs`), for the
controller's Miri lane. The functions are the port's own code with the allocator
handle replaced by allocator_api2's `Global` (the handle's Global arm forwards to it);
element types are reduced to what the borrow shapes need. Recorded in NEW-FINDINGS
N329.

| candidate | shapes |
|---|---|
| `window_next` | the window's private `CircBuf::get_ptr_slice` (two raw subslices from one `split_at_mut`) read in `next()`, then `self.highlight(..)` (a two-phase `&mut self` whose argument reads a subslice) that prunes the same buffer and returns a `FlattenedRef` borrowing `chunk_buf`; `append_slice_assume_capacity` through the same raw subslices; `AllocatingWriter::write`'s raw copy into spare capacity |
| `tick_sibling` | `self.active.next()`'s borrow held while a SIBLING field (`active_results`) grows; the same through `self.history.as_mut()` (`history.searcher` vs `self.history_results`, with `history.start_node` read); `match_at` shallow copies; `Flattened::clone` (the `MultiArrayList` clone); `shrink_and_free`'s raw-parts round trip |
| `list_helpers` | `append_n_times`'s raw fill, `shrink_and_free`, `set_capacity_alloc_copy_free`, the writer's drain growth |

Run (from this dir):

```sh
for c in window_next tick_sibling list_helpers; do
  echo "== $c =="
  cargo +nightly miri run --quiet -- "$c"                            # Stacked Borrows
  MIRIFLAGS=-Zmiri-tree-borrows cargo +nightly miri run -q -- "$c"   # Tree Borrows
done
```

Natively (`cargo run -q -- all`) every candidate passes (task S7-6). Miri results: not
run by the task (the controller's lane).
