# `cursor_scroll_region_up` fast-path Miri harness

`P3-W5` **Task 3 Step 3** (prove on Miri before wiring in) and **Step 4(b)**
(the "reintroduce `&mut Page` → Miri errors" negative control).

Isolated, **non-FFI** reproduction of the borrow structure of
`crates/rustty-vt/src/screen.rs` `cursor_scroll_region_up_inner` (oracle
`77190bd02`): one heap block per page holding `[Row; size_rows][Cell; …]`, the
row base and every cell run materialized as RAW pointers off that base, a
`Screen` whose cursor caches a `*mut Row` / `*mut Cell` **into** that block, and
tracked pins held as raw pointers.

Each candidate is one executed path (Miri only checks executed code), selected
by **argv**. Not by an environment variable: Miri does not forward host env,
which is what made an earlier env-switched control vacuous
(`../../phase-2/mirihar-chunk3b/README.md`).

Run (from this dir):

```sh
for c in raw_ok mutpage_ub rowslice_ub; do
  echo "== $c =="
  cargo +nightly miri run --quiet -- "$c"                            # Stacked Borrows (default)
  MIRIFLAGS=-Zmiri-tree-borrows cargo +nightly miri run -q -- "$c"   # Tree Borrows
done
```

## Results (the load-bearing evidence), 2026-07-31

| candidate | models | SB | TB |
|---|---|---|---|
| `raw_ok` | the **shipped** shape: raw pointers throughout, the only `&mut` is the narrowly-scoped `&mut [Row]` for `rotate_once`, cursor row/cell recomputed from the RAW base | **CLEAN** (`raw_ok: 17571`) | **CLEAN** |
| `mutpage_ub` | `&mut Page` formed from the node and held across `clear_cells`, which re-derives `*mut Page` from the same node | **UB** | **UB** |
| `rowslice_ub` | cursor `page_row` taken FROM the rotate's `&mut [Row]` instead of from the raw base | **UB** | **UB** |

`raw_ok` and `rowslice_ub` execute the **identical** follow-on sequence (a
`clear_dirty`-shaped raw write over every row, an integrity re-walk, then a write
through the cached cursor row); they differ in exactly one thing — where
`cursor_row` came from. Without that the contrast would be confounded.

### `mutpage_ub` — Stacked Borrows

```
error: Undefined Behavior: attempting a write access using <3087> at alloc1126[0x20],
       but that tag does not exist in the borrow stack for this location
   --> src/main.rs:301:5
    |
301 |     page.dirty = true;
help: <3087> was created by a Unique retag at offsets [0x0..0x28]
   --> src/main.rs:293:36
    |
293 |     let page: &mut Page = unsafe { &mut *Node::page(pin_node) };
help: <3087> was later invalidated at offsets [0x20..0x21] by a write access
   --> src/main.rs:193:18
    |
193 |         unsafe { (*page).dirty = true };        // inside Screen::clear_cells
```

Tree Borrows reports the same site: *"the accessed tag `<3031>` has state Disabled
which forbids this child write access"*.

### `rowslice_ub` — Stacked Borrows

```
error: Undefined Behavior: attempting a write access using <3196> at alloc878[0x20],
       but that tag does not exist in the borrow stack for this location
   --> src/main.rs:352:14
    |
352 |     unsafe { *cursor_row = Row(0) };
help: <3196> was created by a SharedReadWrite retag at offsets [0x20..0x28]
   --> src/main.rs:338:32
    |
338 |     let cursor_row: *mut Row = &mut slice[limit];
help: <3196> was later invalidated at offsets [0x20..0x28] by a write access
   --> src/main.rs:345:18
```

## VACUITY FOUND AND FIXED while building this harness

The first two versions of `rowslice_ub` were **vacuous**: they popped the
slice-derived tag with a raw *read* (the integrity walk) and then with a raw
*write to a different row*. Both ran **CLEAN** under Stacked Borrows — SB stacks
are per-location, and a read through the parent raw tag did not remove the
`SharedReadWrite` item the `&mut slice[limit]` coercion had pushed. The control
only became real once the follow-on wrote **that same row** through the page
base.

This is recorded because the intermediate state was a Miri run that printed
`rowslice_ub: 17571` with no diagnostics — i.e. "Miri is green" would have been
reported as evidence of soundness when it was evidence of nothing.
