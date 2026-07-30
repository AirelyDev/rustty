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

### WS7 Phase E — the page mapping fed to LZ4 must be fully INITIALIZED

`compress::CompressedPage::init` compresses the page's whole backing mapping as
one `&[u8]` (faithfully: the oracle's `compress/Page.zig` compresses
`page.memory`). So no byte of that mapping may be uninitialized — and a typed
Rust write of a value with padding, or of an enum whose active variant does not
cover its whole slot, leaves exactly such bytes behind.

`RefCountedSet::init` used to `@memset`-equivalent the whole item array with
`fill(Item::default())`, which de-initialized padding across the styles and
hyperlink item tables of every page. `lz4::compress` then read it and Miri
aborted with "constructing invalid value of type `[u8; 8]`: encountered
uninitialized memory". Fixed by zeroing the item bytes and writing the metadata
field by field (see the PORT NOTE at `ref_counted_set.rs`'s `init`).

Scenarios:

* `mapinit` — allocates one small `Page` and reads every byte of its mapping,
  which is the read `CompressedPage::init` performs. Aborts at the first
  uninitialized byte AND NAMES ITS OFFSET, in **seconds** — this is the cheap
  probe to use, not the multi-ten-minute `roundtrip`/`leak`.
* `mapinit_styled` — same, but stores one non-default `style::Style` first. This
  is the KNOWN RESIDUAL and is EXPECTED TO ABORT: `RefCountedSet::add` must place
  the caller's `T` with a typed write, and `style::Color` is a Rust enum whose
  `None`/`Palette` variants do not cover all four bytes of their slot. See
  `../MIRI_EXCEPTIONS.md`.

### WS7 Phase E — `leakcheck` is now a real gate, with a negative control

`.port/tools/gates/check_pagelist_compress_leaks.sh` runs the native `leakcheck`
scenario (milliseconds, no Miri) and is registered in `ci.sh` as **G6b**.
`--self-test` runs `leakcheck_negative`, which reproduces the real leak through
the real path (`mem::forget` of a compressed list, so no discard-restore runs)
under the same assertion — that is the proof the gate's FAIL arm is reachable.
`leakcheck_negative` is only run when named explicitly; it is not part of the
no-argument "run everything" default because it is expected to panic.

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
