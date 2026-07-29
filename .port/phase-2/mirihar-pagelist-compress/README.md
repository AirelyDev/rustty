# WS7 Phase B — compressed scrollback pages: isolated Miri harness

Miri evidence for `crates/rustty-vt/src/pagelist/{mod.rs,grow.rs,compress.rs}`
(WS7 Phase B review items **I2** and **I3**).

Routed here for the same reason as `../mirihar-compress-lz4`: building the
2136-test `rustty-vt` test binary under Miri has never completed on this host
(see `../MIRI_EXCEPTIONS.md`). This harness is a plain **binary crate that
depends on the real `rustty-vt` by path**, so only the library is built under
Miri — not the test binary. It is not a copy: change `pagelist/mod.rs` and this
harness interprets the change.

Run (from this directory):

```sh
cargo +nightly miri run --quiet                                  # Stacked Borrows
MIRIFLAGS=-Zmiri-tree-borrows cargo +nightly miri run --quiet    # Tree Borrows
```

## What is being proven

### I2 — the `Node` page accessors must stay RAW-DERIVED

WS7 B6 rewrote them as `match &mut *addr_of_mut!((*node).data) { Resident(p) =>
p as *mut Page, .. }`. That returns a pointer carrying a fresh `Unique` tag, so
a later `Node::page`/`metadata`/`rows` on the SAME node invalidates it and a
subsequent deref is UB. `grow.rs::split` does exactly that: it derives `page`
at the top, calls `Node::page((*tracked).node)` inside its tracked-pin loop for
the same node by construction, and then dereferences `page` to clear rows and
shorten the source.

The fix makes `NodeData` `#[repr(C, u8)]` and projects the payload with a
constant `offset_of!` from the caller's own raw pointer (RFC 2195 layout), so no
reference is ever formed.

Scenarios: `split_holds_page_across_tracked_loop` (the real reproduction),
`page_pointer_survives_sibling_accessors` (the general shape),
`compress_restore_round_trip`.

### I3 — `CompressedPage::encoded` must not leak

`encoded` is a global-allocator `Vec`, and `Node` storage lives in a raw memory
pool that never runs `Drop`. The repo has no `#[global_allocator]` leak tracker,
so the page-allocator leak gate cannot see it: deleting the discard-restore
loops from `PageList::deinit`/`reset` leaves the ENTIRE native suite green.
Miri's exit-time leak checker does see it.

Scenario: `compress_reset_compress_deinit` (compress -> `reset()` -> compress ->
`deinit()`).

## Miri accommodations

* `rustty-os`'s page allocator already serves page memory from the std heap
  under `cfg(miri)`.
* `mem::{can_reclaim,decommit,recommit}` take their simulated arm under
  `cfg!(miri)` as well as `cfg!(test)` — Miri has no FFI, so `madvise` is
  uncallable, and the std heap is not an OS mapping anyway.
* `[profile.dev] debug-assertions = false` in this crate's manifest.
  `Page::assert_integrity` is O(page) and runs on nearly every PageList
  mutation; it dominates the interpreted workload without adding any SB/TB or
  leak coverage. The native debug suite is where it earns its keep.

## Results

See the WS7 Phase B review-fix report for the recorded run.
