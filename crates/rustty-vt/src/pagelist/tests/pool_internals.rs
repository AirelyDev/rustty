//! PORTS: ghostty src/terminal/PageList.zig — the `UntouchedPool` page-pool /
//! node-pool internals tests (P7-W1 T5).

use super::grow_cold_pages_for_test;
use crate::alloc::testing::{with_handle, Config as FailingConfig, FailingAllocator};
use crate::alloc::Global;
use crate::page::Capacity;
use crate::pagelist::{std_capacity, std_size, Options, OutOfMemory, Owned, PageList, Pin};
use crate::point;
use rustty_render_types::term_size as size;

// ---------------------------------------------------------------------------
// P7-W1 T5 (`e01e75bbb`, `efa3c66ed`, `35a6fb747`): the page pool is an
// `UntouchedPool`, nodes are pooled one by one from the gpa.
// ---------------------------------------------------------------------------

/// PORT NOTE: `std.heap.FixedBufferAllocator` over a raw backing buffer, as
/// much of it as the upstream test observes (`end_index`, the bump allocation,
/// and `free` reclaiming only the last allocation). Every pointer it hands out
/// is derived from the one raw `buffer` pointer, so the test's writes through
/// that same pointer never invalidate an item (Stacked Borrows).
struct FixedBufferAllocator {
    buffer: *mut u8,
    len: usize,
    end_index: core::cell::Cell<usize>,
}

// SAFETY: every block returned lies inside `buffer`, is disjoint from every
// other live block (the index only moves past handed-out bytes, and moves back
// only over the last one when it is freed), and is aligned as asked.
unsafe impl allocator_api2::alloc::Allocator for FixedBufferAllocator {
    fn allocate(
        &self,
        layout: core::alloc::Layout,
    ) -> Result<core::ptr::NonNull<[u8]>, allocator_api2::alloc::AllocError> {
        use allocator_api2::alloc::AllocError;
        let end = self.end_index.get();
        let addr = (self.buffer as usize).checked_add(end).ok_or(AllocError)?;
        let adjusted_index = end + (addr.next_multiple_of(layout.align()) - addr);
        let new_end_index = adjusted_index
            .checked_add(layout.size())
            .ok_or(AllocError)?;
        if new_end_index > self.len {
            return Err(AllocError);
        }
        self.end_index.set(new_end_index);
        // SAFETY: `adjusted_index + size <= len`, inside the buffer.
        let ptr = unsafe { core::ptr::NonNull::new_unchecked(self.buffer.add(adjusted_index)) };
        Ok(core::ptr::NonNull::slice_from_raw_parts(ptr, layout.size()))
    }

    unsafe fn deallocate(&self, ptr: core::ptr::NonNull<u8>, layout: core::alloc::Layout) {
        // `if (!self.isLastAllocation(buf)) return;`
        let end = self.end_index.get();
        if ptr.as_ptr() as usize + layout.size() == self.buffer as usize + end {
            self.end_index.set(end - layout.size());
        }
    }
}

/// `std.mem.allEqual(u8, bytes, value)` over `len` raw bytes.
///
/// Compares in `CHUNK`-byte pieces against a pre-filled buffer with slice
/// `==` (one `memcmp` per chunk), not byte by byte: a byte loop over the
/// multi-megabyte pool buffer is intractable under Miri (N493).
fn raw_all_equal(ptr: *const u8, len: usize, value: u8) -> bool {
    const CHUNK: usize = 4096;
    let want = [value; CHUNK];
    let mut off = 0;
    while off < len {
        let n = CHUNK.min(len - off);
        // SAFETY: the callers pass live, initialized pool items or buffers
        // reached only through raw pointers (no live `&mut` aliases them);
        // each shared view ends before the bytes are next written.
        let got = unsafe { core::slice::from_raw_parts(ptr.add(off), n) };
        if got != &want[..n] {
            return false;
        }
        off += n;
    }
    true
}

/// `raw_all_equal` is true only when every byte matches: a difference in
/// the middle, in the last byte, or in a partial tail chunk is caught.
#[test]
fn raw_all_equal_detects_a_differing_byte() {
    let mut buf = vec![7u8; 10_000];
    assert!(raw_all_equal(buf.as_ptr(), buf.len(), 7));
    assert!(raw_all_equal(buf.as_ptr(), 0, 1));
    assert!(!raw_all_equal(buf.as_ptr(), buf.len(), 8));
    buf[5_000] = 0;
    assert!(!raw_all_equal(buf.as_ptr(), buf.len(), 7));
    buf[5_000] = 7;
    let last = buf.len() - 1;
    buf[last] = 0;
    assert!(!raw_all_equal(buf.as_ptr(), buf.len(), 7));
    assert!(raw_all_equal(buf.as_ptr(), last, 7));
}

/// PORTS: oracle@main PageList.zig
/// `test "PageList memory pool never touches idle page memory"`.
#[test]
fn pagelist_memory_pool_never_touches_idle_page_memory() {
    const PREHEAT: usize = crate::pagelist::PAGE_PREHEAT;

    // Back the page allocator with memory we can inspect.
    let backing_layout =
        core::alloc::Layout::from_size_align(PREHEAT * std_size(), crate::page::PAGE_SIZE_MIN)
            .unwrap();
    // SAFETY: non-zero size.
    let backing = unsafe { std::alloc::alloc(backing_layout) };
    assert!(!backing.is_null());
    // defer testing.allocator.free(backing);
    struct FreeBacking(*mut u8, core::alloc::Layout);
    impl Drop for FreeBacking {
        fn drop(&mut self) {
            // SAFETY: allocated above with this layout, freed once.
            unsafe { std::alloc::dealloc(self.0, self.1) };
        }
    }
    let _free_backing = FreeBacking(backing, backing_layout);
    let fba = FixedBufferAllocator {
        buffer: backing,
        len: backing_layout.size(),
        end_index: core::cell::Cell::new(0),
    };

    let mut pool =
        crate::pagelist::MemoryPool::<&FixedBufferAllocator>::init(Global, &fba, PREHEAT).unwrap();
    // defer pool.deinit(); (dropped explicitly below, before `backing`)

    // Preheat allocated exactly the items.
    assert_eq!(PREHEAT * std_size(), fba.end_index.get());

    // Lay the sentinel down after preheat: allocation itself may write
    // (the Allocator interface fills fresh memory with undefined in
    // safe builds, which valgrind also tracks). The sentinel must differ
    // from Zig's 0xAA undefined pattern so that any write is visible.
    let sentinel: u8 = 0x5A;
    // SAFETY: the whole backing buffer, through its one raw pointer.
    unsafe { core::ptr::write_bytes(backing, sentinel, backing_layout.size()) };

    // Every preheated item is handed out untouched and without going
    // back to the page allocator, and destroying it doesn't touch it.
    let mut items = [core::ptr::NonNull::<u8>::dangling(); PREHEAT];
    for item in items.iter_mut() {
        *item = pool.pages.create().unwrap();
        assert_eq!(PREHEAT * std_size(), fba.end_index.get());
        assert!(raw_all_equal(item.as_ptr(), std_size(), sentinel));
    }
    for item in items {
        // SAFETY: each item came from `pool.pages.create()` and goes back once.
        unsafe { pool.pages.destroy(item) };
    }
    assert!(raw_all_equal(backing, backing_layout.size(), sentinel));
    drop(pool);
}

/// PORTS: oracle@main PageList.zig
/// `test "PageList memory pool fast path does not allocate"`.
#[test]
fn pagelist_memory_pool_fast_path_does_not_allocate() {
    const PREHEAT: usize = crate::pagelist::PAGE_PREHEAT;
    // var counting: std.testing.FailingAllocator = .init(testing.allocator, .{});
    let counting = FailingAllocator::over_heap(FailingConfig::default());
    let c = counting.allocator();
    with_handle(&c, |page_alloc| {
        let mut pool = crate::pagelist::MemoryPool::<crate::alloc::VtAllocator>::init(
            Global, page_alloc, PREHEAT,
        )
        .unwrap();
        // defer pool.deinit(); (dropped at the end of this closure)
        assert_eq!(PREHEAT, counting.allocations.get());

        // Cycle a few thousand pages through the preheated items. As long
        // as no more than the preheat are live at once, create is a
        // free-list pop and never touches the page allocator.
        let mut items = [core::ptr::NonNull::<u8>::dangling(); PREHEAT];
        for _ in 0..1024 {
            for item in items.iter_mut() {
                *item = pool.pages.create().unwrap();
            }
            for item in items {
                // SAFETY: each item came from `create` just above.
                unsafe { pool.pages.destroy(item) };
            }
        }
        assert_eq!(PREHEAT, counting.allocations.get());
        assert_eq!(0, counting.deallocations.get());

        // Going past the preheat allocates the extra items once; they are
        // recycled from then on.
        let mut extra = [core::ptr::NonNull::<u8>::dangling(); PREHEAT + 2];
        for _ in 0..1024 {
            for item in extra.iter_mut() {
                *item = pool.pages.create().unwrap();
            }
            for item in extra {
                // SAFETY: as above.
                unsafe { pool.pages.destroy(item) };
            }
        }
        assert_eq!(extra.len(), counting.allocations.get());
        assert_eq!(0, counting.deallocations.get());
        drop(pool);
    });
    assert_eq!(counting.allocations.get(), counting.deallocations.get());
}

/// Rust-only (the zero-fill contract of `e01e75bbb`'s `PagePool`): every path
/// that hands a page back to the pool with `destroy` leaves it zeroed, because
/// `Page::init_buf` now builds its metadata from zeroed memory without writing
/// it. A pool-owned page is dirtied over its whole memory, destroyed, and its
/// item handed out again: it reads back as zero, and a page initializes from
/// it. The same holds for a compression scratch that returns only its dirty
/// prefix (`60b43068c`).
#[test]
fn pagelist_recycled_dirty_pool_page_is_zeroed_for_reuse() {
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();

    // A destroyed node's page (`destroy_node_ext`).
    let node = s
        .create_page(crate::pagelist::CreatePage::new(std_capacity()))
        .unwrap();
    assert_eq!(Owned::Pool, unsafe { (*node).owned });
    let (item, len) = PageList::page_memory(unsafe { crate::pagelist::Node::page(node) });
    assert!(len <= std_size());
    // SAFETY: the live page memory of the detached node.
    unsafe { core::ptr::write_bytes(item, 0xAA, len) };
    s.destroy_node(node);

    // The pool is LIFO: the next item is the one just destroyed.
    let reused = s.pool.pages.create().unwrap();
    assert_eq!(item, reused.as_ptr());
    assert!(raw_all_equal(reused.as_ptr(), std_size(), 0));
    // SAFETY: handed out just above.
    unsafe { s.pool.pages.destroy(reused) };

    // And a page initializes from it (a debug build samples the zero fill in
    // `init_buf`).
    let node = s
        .create_page(crate::pagelist::CreatePage::new(std_capacity()))
        .unwrap();
    assert_eq!(
        item,
        PageList::page_memory(unsafe { crate::pagelist::Node::page(node) }).0
    );
    s.destroy_node(node);

    // A compression scratch clears only the prefix it dirtied.
    let scratch =
        crate::pagelist::compress::CompressionScratch::init(&mut s.pool, 4096, 4096).unwrap();
    let ptr = match scratch {
        crate::pagelist::compress::CompressionScratch::Pooled(ptr) => ptr,
        crate::pagelist::compress::CompressionScratch::Allocated { .. } => {
            panic!("a standard scratch")
        }
    };
    assert_eq!(item, ptr);
    // SAFETY: the borrowed item, exclusively held by `scratch`.
    unsafe { core::ptr::write_bytes(ptr, 0xAA, 1000) };
    scratch.deinit(&mut s.pool, 1000);
    let reused = s.pool.pages.create().unwrap();
    assert_eq!(item, reused.as_ptr());
    assert!(raw_all_equal(reused.as_ptr(), std_size(), 0));
    // SAFETY: handed out just above.
    unsafe { s.pool.pages.destroy(reused) };

    s.deinit();
}

/// Run `f` once per allocation of the general-purpose allocator, failing that
/// allocation, until a run makes no induced failure. After every run nothing
/// the allocator handed out is still live (`CountingAllocator`), and no pool
/// assert fired (the run would have panicked). Returns the number of runs.
fn teardown_failure_sweep(
    what: &str,
    mut f: impl FnMut(crate::alloc::VtAllocator) -> bool,
) -> usize {
    use crate::alloc::testing::CountingAllocator;
    for n in 0.. {
        let counting = CountingAllocator::over_heap();
        // SAFETY: `counting` outlives `failing` (declared first, dropped last).
        let failing = unsafe {
            FailingAllocator::init(
                counting.allocator(),
                FailingConfig {
                    fail_index: n,
                    ..FailingConfig::default()
                },
            )
        };
        let c = failing.allocator();
        let ok = with_handle(&c, &mut f);
        assert_eq!(
            0,
            counting.live_bytes.get(),
            "{what}: {} bytes leaked with allocation {n} failing",
            counting.live_bytes.get()
        );
        if !failing.has_induced_failure.get() {
            assert!(ok, "{what}: no failure was induced, so it must succeed");
            return n + 1;
        }
        assert!(!ok, "{what}: allocation {n} failed, so it must fail");
    }
    unreachable!()
}

/// Rows that need three standard pages, so `init` builds a list of several
/// nodes (the node pool preheats one).
fn three_pages_of_rows() -> size::CellCountInt {
    PageList::initial_capacity(80).rows * 2 + 1
}

/// Review Focus 3 (P7-W1 T5): `init`/`initPages` failing part-way through a
/// multi-node list hands every node and page back before the pool drops.
#[test]
fn pagelist_teardown_after_init_failure_returns_every_node() {
    let runs = teardown_failure_sweep("init", |alloc| {
        match PageList::init(alloc, Options::new(80, three_pages_of_rows())) {
            Ok(mut s) => {
                assert_eq!(3, s.total_pages());
                s.deinit();
                true
            }
            Err(OutOfMemory) => false,
        }
    });
    // The node pool, the page pool's free list, the pin pool, the second and
    // third nodes and the tracked-pin set can each fail.
    assert!(runs > 5, "{runs}");
}

/// A source list of three pages with a tracked pin on each, for the clone
/// sweeps (built on the global heap, outside the swept allocator).
fn clone_source_for_test() -> (PageList, Vec<*mut Pin>) {
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();
    grow_cold_pages_for_test(&mut s, 2);
    let mut pins = Vec::new();
    let mut node = s.pages.first;
    while !node.is_null() {
        let p = s
            .track_pin(Pin {
                node,
                y: 0,
                x: 0,
                garbage: false,
            })
            .unwrap();
        pins.push(p);
        node = unsafe { (*node).next };
    }
    (s, pins)
}

fn clone_from_top_with_remap(
    s: &PageList,
    alloc: crate::alloc::VtAllocator,
) -> Result<PageList, crate::pagelist::access::CloneError> {
    let mut opts = crate::pagelist::access::Clone {
        top: point::Point::Screen(point::Coordinate { x: 0, y: 0 }),
        bot: None,
        tracked_pins: Some(crate::pagelist::access::TrackedPinsRemap::with_hasher_in(
            Default::default(),
            alloc,
        )),
    };
    s.clone(alloc, &mut opts)
}

/// Review Focus 3 (P7-W1 T5): every allocation failure inside `clone` (the
/// pools, a node, the tracked-pin set, the pin remap) hands the partial list's
/// nodes and pages back before the clone's pool drops.
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
)]
#[test]
fn pagelist_teardown_after_clone_failure_returns_every_node() {
    let (mut s, pins) = clone_source_for_test();
    let runs = teardown_failure_sweep("clone", |alloc| {
        match clone_from_top_with_remap(&s, alloc) {
            Ok(mut c) => {
                assert_eq!(s.total_pages(), c.total_pages());
                c.deinit();
                true
            }
            Err(crate::pagelist::access::CloneError::OutOfMemory) => false,
            Err(e) => panic!("unexpected {e:?}"),
        }
    });
    assert!(runs > 5, "{runs}");
    for p in pins {
        s.untrack_pin(p);
    }
    s.deinit();
}

/// Review Focus 3 (P7-W1 T5), the fail-first case: `cloneFrom` failing on the
/// second page. Before `e01e75bbb` moved the append ahead of `cloneFrom`, the
/// failing page's node was on no list, so nothing handed it back and the
/// clone's node pool asserted `live == free.len` when it dropped.
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
)]
#[test]
fn pagelist_teardown_after_clone_from_failure_returns_every_node() {
    use crate::alloc::testing::CountingAllocator;
    use crate::pagelist::access::{CloneFp, CLONE_TW};
    use rustty_util::tripwire::ResetMode;

    let (mut s, pins) = clone_source_for_test();
    let counting = CountingAllocator::over_heap();
    let c = counting.allocator();
    CLONE_TW.with(|tw| {
        tw.borrow_mut().errorAfter(
            CloneFp::CloneFrom,
            crate::page::CloneFromError::StyleSetOutOfMemory,
            1,
        )
    });
    let res = with_handle(&c, |alloc| {
        clone_from_top_with_remap(&s, alloc).map(|mut c| c.deinit())
    });
    CLONE_TW.with(|tw| tw.borrow_mut().end(ResetMode::Reset).unwrap());
    assert!(matches!(
        res,
        Err(crate::pagelist::access::CloneError::CloneFrom(
            crate::page::CloneFromError::StyleSetOutOfMemory
        ))
    ));
    assert_eq!(0, counting.live_bytes.get(), "the clone leaked");
    for p in pins {
        s.untrack_pin(p);
    }
    s.deinit();
}

/// Review Focus 3 (P7-W1 T5): a clone of fewer rows than the active area
/// grows the new list to fill it; a failing `grow` tears the new list down
/// through the pool that moved into it.
#[test]
fn pagelist_teardown_after_clone_grow_failure_returns_every_node() {
    // Two-row pages, so a one-row clone of the last row must grow new pages
    // (a node from the general-purpose allocator) to fill the active area.
    let mut builder =
        crate::pagelist::builder::Builder::init(Global, Options::new(80, 24)).unwrap();
    for _ in 0..12 {
        let page = builder
            .allocate_page(Capacity {
                cols: 80,
                rows: 2,
                ..Capacity::default()
            })
            .unwrap();
        unsafe { (*page).size.rows = 2 };
    }
    let mut s = builder.finish().unwrap();
    builder.deinit();
    let last_row = (s.total_rows - 1) as u32;
    let mut grew = false;
    let runs = teardown_failure_sweep("clone grow", |alloc| {
        let mut opts = crate::pagelist::access::Clone {
            top: point::Point::Screen(point::Coordinate { x: 0, y: last_row }),
            bot: None,
            tracked_pins: None,
        };
        match s.clone(alloc, &mut opts) {
            Ok(mut c) => {
                grew = c.total_rows == c.rows as usize;
                c.deinit();
                true
            }
            Err(crate::pagelist::access::CloneError::OutOfMemory) => false,
            Err(e) => panic!("unexpected {e:?}"),
        }
    });
    assert!(grew, "the clone must grow to the active area");
    assert!(runs > 3, "{runs}");
    s.deinit();
}

/// Review Focus 3 (P7-W1 T5): a `Builder` whose page allocation or `finish`
/// fails hands every page and node back through `deinit`.
#[test]
fn pagelist_teardown_after_builder_failure_returns_every_node() {
    use crate::pagelist::builder::{Builder, FinishError};
    let runs = teardown_failure_sweep("builder", |alloc| {
        let mut builder = match Builder::init(alloc, Options::new(80, 24)) {
            Ok(b) => b,
            Err(OutOfMemory) => return false,
        };
        let mut failed = false;
        for _ in 0..3 {
            match builder.allocate_page(std_capacity()) {
                Ok(page) => unsafe {
                    (*page).size.rows = 10;
                },
                Err(OutOfMemory) => {
                    failed = true;
                    break;
                }
            }
        }
        let ok = !failed
            && match builder.finish() {
                Ok(mut list) => {
                    list.deinit();
                    true
                }
                Err(FinishError::OutOfMemory) => false,
                Err(e) => panic!("unexpected {e:?}"),
            };
        builder.deinit();
        ok
    });
    assert!(runs > 4, "{runs}");
}

/// Review Focus 3 (P7-W1 T5): a detached `PageAllocation` whose creation
/// fails, or which is thrown away with `deinit`, leaves nothing live.
#[test]
fn pagelist_teardown_after_allocate_page_failure_returns_every_node() {
    let runs = teardown_failure_sweep("allocate_page", |alloc| {
        let mut s = match PageList::init(alloc, Options::new(80, 24)) {
            Ok(s) => s,
            Err(OutOfMemory) => return false,
        };
        let sp: *mut PageList = &mut s;
        let mut allocs = Vec::new();
        let mut ok = true;
        for _ in 0..3 {
            match unsafe { PageList::allocate_page(sp, std_capacity()) } {
                Ok(a) => allocs.push(a),
                Err(OutOfMemory) => {
                    ok = false;
                    break;
                }
            }
        }
        for mut a in allocs {
            a.deinit();
        }
        unsafe { (*sp).deinit() };
        ok
    });
    assert!(runs > 3, "{runs}");
}

/// Review Focus 3 (P7-W1 T5): `reset` hands every page and node back before it
/// trims the pools, and a later `deinit` returns everything.
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
)]
#[test]
fn pagelist_teardown_after_reset_returns_every_node() {
    use crate::alloc::testing::CountingAllocator;
    let counting = CountingAllocator::over_heap();
    let c = counting.allocator();
    with_handle(&c, |alloc| {
        let mut s = PageList::init(alloc, Options::new(80, 24)).unwrap();
        grow_cold_pages_for_test(&mut s, 3);
        s.reset();
        assert_eq!(1, s.total_pages());
        grow_cold_pages_for_test(&mut s, 1);
        s.deinit();
    });
    assert_eq!(0, counting.live_bytes.get());
}
