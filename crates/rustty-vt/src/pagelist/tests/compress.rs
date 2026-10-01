//! PORTS: ghostty src/terminal/PageList.zig — the grow-non-standard-page,
//! `compact`, `decommitPoolItem`, page-compression and incremental-compression
//! tests.

use super::{
    cell_codepoint, compressed_encoded_for_test, fill_compressible_history,
    grow_cold_pages_for_test,
};
use crate::alloc::testing::{with_handle, Config as FailingConfig, FailingAllocator};
use crate::alloc::Global;
use crate::page::{Capacity, Cell, Page};
use crate::pagelist::compress::{
    CompressionIterator, CompressionMode, IncrementalCompressionResult,
};
use crate::pagelist::grow::IncreaseCapacity;
use crate::pagelist::resize::Resize;
use crate::pagelist::scroll::Scroll;
use crate::pagelist::{
    std_size, Key, NodeData, Options, OutOfMemory, Owned, PageList, Pin, PreservedPage, Storage,
};
use crate::point;
use rustty_render_types::term_size as size;

// =====================================================================
// grow non-standard / compact
// =====================================================================

// ref: test "PageList grow reuses non-standard page without leak"
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
)]
#[test]
fn pagelist_grow_reuses_non_standard_page_without_leak() {
    // Create a PageList with 3 * std_size max so we can fit multiple pages
    // but will still trigger reuse.
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(3 * std_size()),
            ..Options::new(80, 24)
        },
    )
    .unwrap();

    // Increase the first page capacity to make it non-standard (larger than std_size).
    while unsafe { (*crate::pagelist::Node::page(s.pages.first)).memory_len } <= std_size() {
        let _ = s
            .increase_capacity(s.pages.first, Some(IncreaseCapacity::GraphemeBytes))
            .unwrap();
    }

    // The first page should now have non-standard memory size.
    assert!(unsafe { (*crate::pagelist::Node::page(s.pages.first)).memory_len } > std_size());

    // First, fill up the first page's capacity
    let first_page = s.pages.first;
    while unsafe {
        crate::pagelist::Node::rows(first_page) < crate::pagelist::Node::capacity(first_page).rows
    } {
        let _ = s.grow().unwrap();
    }

    // Now grow to create a second page
    let _ = s.grow().unwrap();
    assert!(!core::ptr::eq(s.pages.first, s.pages.last));

    // Continue growing until we exceed max_size AND the last page is full
    while s.page_size + std_size() <= s.limits.max(Key::Bytes)
        || unsafe {
            crate::pagelist::Node::rows(s.pages.last)
                < crate::pagelist::Node::capacity(s.pages.last).rows
        }
    {
        let _ = s.grow().unwrap();
    }

    // The first page should still be non-standard
    assert!(unsafe { (*crate::pagelist::Node::page(s.pages.first)).memory_len } > std_size());

    // Verify we have enough rows for active area (so prune path isn't skipped)
    assert!(s.total_rows() >= s.rows as usize);

    // Verify last page is full (so grow will need to allocate/reuse)
    assert!(unsafe {
        crate::pagelist::Node::rows(s.pages.last)
            == crate::pagelist::Node::capacity(s.pages.last).rows
    });

    // Remember the first page memory pointer before the reuse attempt
    let first_page_ptr = s.pages.first;
    let first_page_mem_ptr = unsafe { (*crate::pagelist::Node::page(s.pages.first)).memory_ptr() };

    // Create a tracked pin pointing to the non-standard first page
    let tracked_pin = s
        .track_pin(Pin {
            node: first_page_ptr,
            x: 0,
            y: 0,
            garbage: false,
        })
        .unwrap();

    // Now grow one more time to trigger the reuse path. Since the first page
    // is non-standard, it should be destroyed (not reused). The testing
    // allocator will detect a leak if destroyNode doesn't properly free
    // the non-standard memory.
    let _ = s.grow().unwrap();

    // After grow, check if the first page is a different one
    // (meaning the non-standard page was pruned, not reused at the end)
    // The original first page should no longer be the first page
    assert!(!core::ptr::eq(s.pages.first, first_page_ptr));

    // If the non-standard page was properly destroyed and not reused,
    // the last page should not have the same memory pointer
    assert!(!core::ptr::eq(
        unsafe { (*crate::pagelist::Node::page(s.pages.last)).memory_ptr() },
        first_page_mem_ptr,
    ));

    // The tracked pin should have been moved to the new first page and marked as garbage
    assert!(core::ptr::eq(s.pages.first, unsafe { (*tracked_pin).node }));
    assert_eq!(0 as size::CellCountInt, unsafe { (*tracked_pin).x });
    assert_eq!(0 as size::CellCountInt, unsafe { (*tracked_pin).y });
    assert!(unsafe { (*tracked_pin).garbage });

    s.untrack_pin(tracked_pin);
    s.deinit();
}

// ref: test "PageList grow non-standard page prune protection"
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
)]
#[test]
fn pagelist_grow_non_standard_page_prune_protection() {
    // This test specifically verifies the fix for the bug where pruning a
    // non-standard page would cause totalRows() < self.rows.
    //
    // Bug trigger conditions (all must be true simultaneously):
    // 1. first page is non-standard (memory.len > std_size)
    // 2. page_size + PagePool.item_size > maxSize() (triggers prune consideration)
    // 3. pages.first != pages.last (have multiple pages)
    // 4. total_rows >= self.rows (have enough rows for active area)
    // 5. total_rows - first.size.rows + 1 < self.rows (prune would lose too many)

    // This is kind of magic and likely depends on std_size.
    let rows_count = 600;
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(std_size()),
            ..Options::new(80, rows_count)
        },
    )
    .unwrap();

    // Make the first page non-standard
    while unsafe { (*crate::pagelist::Node::page(s.pages.first)).memory_len } <= std_size() {
        let _ = s
            .increase_capacity(s.pages.first, Some(IncreaseCapacity::GraphemeBytes))
            .unwrap();
    }
    assert!(unsafe { (*crate::pagelist::Node::page(s.pages.first)).memory_len } > std_size());

    let first_page_node = s.pages.first;
    let first_page_cap = unsafe { crate::pagelist::Node::capacity(first_page_node).rows };

    // Fill first page to capacity
    while unsafe { crate::pagelist::Node::rows(first_page_node) } < first_page_cap {
        let _ = s.grow().unwrap();
    }

    // Grow until we have a second page (first page fills up first)
    while core::ptr::eq(s.pages.first, s.pages.last) {
        let _ = s.grow().unwrap();
    }
    assert!(!core::ptr::eq(s.pages.first, s.pages.last));

    // Fill the second page to capacity so that the next grow() triggers prune
    let last_node = s.pages.last;
    let second_cap = unsafe { crate::pagelist::Node::capacity(last_node).rows };
    while unsafe { crate::pagelist::Node::rows(last_node) } < second_cap {
        let _ = s.grow().unwrap();
    }

    // Now the last page is full. The next grow must either:
    // 1. Prune the first page and reuse it, OR
    // 2. Allocate a new page
    let total = s.total_rows();
    let would_remain = total - first_page_cap as usize + 1;

    // Verify the bug condition is present: pruning first page would leave < rows
    assert!(would_remain < s.rows as usize);

    // Verify prune path conditions are met
    assert!(!core::ptr::eq(s.pages.first, s.pages.last));
    assert!(s.page_size + std_size() > s.limits.max(Key::Bytes));
    assert!(s.total_rows() >= s.rows as usize);

    // Verify last page is at capacity (so grow must prune or allocate new)
    assert_eq!(second_cap, unsafe {
        crate::pagelist::Node::rows(last_node)
    });

    // The next grow should trigger prune consideration.
    // Without the fix, this would destroy the non-standard first page,
    // leaving only second_cap + 1 rows, which is < self.rows.
    let _ = s.grow().unwrap();

    // Verify the invariant holds - the fix prevents the destructive prune
    assert!(s.total_rows() >= s.rows as usize);

    s.deinit();
}

// ref: test "PageList compact pool page produces exact-size heap page"
#[test]
fn pagelist_compact_pool_page_produces_exact_size_heap_page() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(80, 24)
        },
    )
    .unwrap();

    // A freshly created page is pool-owned at std_size.
    let node = s.pages.first;
    assert_eq!(unsafe { (*node).owned }, Owned::Pool);
    assert!(unsafe { (*crate::pagelist::Node::page(node)).memory_len } <= std_size());
    let original_size = unsafe { (*crate::pagelist::Node::page(node)).size };

    // Compacting it should produce a much smaller exact-size heap page.
    let new_node = s.compact(node).unwrap().unwrap();
    assert_eq!(unsafe { (*new_node).owned }, Owned::Heap);
    assert!(unsafe { (*crate::pagelist::Node::page(new_node)).memory_len } < std_size());
    assert_eq!(original_size.rows, unsafe {
        crate::pagelist::Node::rows(new_node)
    });
    assert_eq!(original_size.cols, unsafe {
        crate::pagelist::Node::cols(new_node)
    });
    assert!(core::ptr::eq(new_node, s.pages.first));

    // Our page size accounting should exactly match the compacted page since it
    // is the only page in the list.
    assert_eq!(
        unsafe { (*crate::pagelist::Node::page(new_node)).memory_len },
        s.page_size
    );

    // Compacting again should be a no-op since it is already exact.
    assert!(s.compact(new_node).unwrap().is_none());

    s.deinit();
}

// ref: test "PageList compact then grow allocates new page"
#[test]
fn pagelist_compact_then_grow_allocates_new_page() {
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();

    // Compact the only page. It now has no spare row capacity.
    let node = s.compact(s.pages.first).unwrap().unwrap();
    assert_eq!(unsafe { crate::pagelist::Node::rows(node) }, unsafe {
        crate::pagelist::Node::capacity(node).rows
    });

    // Growing must allocate a fresh standard page from the pool, exercising
    // that a compacted page remains a valid live page.
    let _ = s.grow().unwrap();
    assert!(!core::ptr::eq(s.pages.first, s.pages.last));
    assert_eq!(unsafe { (*s.pages.last).owned }, Owned::Pool);
    assert_eq!(25usize, s.total_rows());

    s.deinit();
}

// ref: test "PageList compact then reset frees heap pages"
#[test]
fn pagelist_compact_then_reset_frees_heap_pages() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(80, 24)
        },
    )
    .unwrap();

    // Compact the only page so the list contains a sub-std_size heap-owned
    // page.
    let node = s.compact(s.pages.first).unwrap().unwrap();
    assert_eq!(unsafe { (*node).owned }, Owned::Heap);
    assert!(unsafe { (*crate::pagelist::Node::page(node)).memory_len } < std_size());

    // Reset must free the heap page (the leak gate catches leaks and invalid
    // frees) and rebuild from the pool.
    s.reset();
    assert_eq!(unsafe { (*s.pages.first).owned }, Owned::Pool);
    assert_eq!(s.rows as usize, s.total_rows());

    s.deinit();
}

// ref: test "PageList compact then clone"
#[test]
fn pagelist_compact_then_clone() {
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();

    // Write a marker so we can verify contents survive.
    {
        let node = s.pages.first;
        let rac = unsafe { (*crate::pagelist::Node::page(node)).get_row_and_cell(1, 2) };
        unsafe { *rac.cell = cell_codepoint('X' as u32) };
    }

    // Compact so the source list contains a sub-std_size heap page.
    let node = s.compact(s.pages.first).unwrap().unwrap();
    assert_eq!(unsafe { (*node).owned }, Owned::Heap);
    assert!(unsafe { (*crate::pagelist::Node::page(node)).memory_len } < std_size());

    let mut opts = crate::pagelist::access::Clone {
        top: point::Point::Screen(point::Coordinate { x: 0, y: 0 }),
        bot: None,
        tracked_pins: None,
    };
    let mut s2 = s.clone(Global, &mut opts).unwrap();
    assert_eq!(s.rows as usize, s2.total_rows());

    // Verify the marker survived the clone.
    {
        let node2 = s2.pages.first;
        let rac = unsafe { (*crate::pagelist::Node::page(node2)).get_row_and_cell(1, 2) };
        assert_eq!('X' as u32, unsafe { (*rac.cell).codepoint() });
    }

    s2.deinit();
    s.deinit();
}

// ref: test "PageList compact oversized page"
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
)]
#[test]
fn pagelist_compact_oversized_page() {
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();

    // Grow until we have multiple pages
    let page1_node = s.pages.first;
    unsafe { (*crate::pagelist::Node::page(page1_node)).pause_integrity_checks(true) };
    for _ in 0..(unsafe {
        crate::pagelist::Node::capacity(page1_node).rows - crate::pagelist::Node::rows(page1_node)
    }) {
        let _ = s.grow().unwrap();
    }
    unsafe { (*crate::pagelist::Node::page(page1_node)).pause_integrity_checks(false) };
    let _ = s.grow().unwrap();
    assert!(!core::ptr::eq(s.pages.first, s.pages.last));

    let mut node = s.pages.first;

    // Write content to verify it's preserved
    {
        let page = unsafe { crate::pagelist::Node::page(node) };
        for y in 0..unsafe { (*page).size.rows } as usize {
            for x in 0..s.cols as usize {
                let rac = unsafe { (*page).get_row_and_cell(x, y) };
                unsafe {
                    *rac.cell = cell_codepoint((x + y * s.cols as usize) as u32);
                }
            }
        }
    }

    // Create a tracked pin on this page
    let tracked = s
        .track_pin(Pin {
            node,
            x: 5,
            y: 10,
            garbage: false,
        })
        .unwrap();

    // Make the page oversized
    while unsafe { (*crate::pagelist::Node::page(node)).memory_len } <= std_size() {
        node = s
            .increase_capacity(node, Some(IncreaseCapacity::GraphemeBytes))
            .unwrap();
    }
    assert!(unsafe { (*crate::pagelist::Node::page(node)).memory_len } > std_size());
    let oversized_len = unsafe { (*crate::pagelist::Node::page(node)).memory_len };
    let original_size = unsafe { (*crate::pagelist::Node::page(node)).size };
    let second_node = unsafe { (*node).next };

    // Set dirty flag after increaseCapacity
    unsafe { (*crate::pagelist::Node::page(node)).dirty = true };

    // Compact the page
    let new_node = s.compact(node).unwrap();
    assert!(new_node.is_some());
    let new_node = new_node.unwrap();

    // Verify memory is smaller
    assert!(unsafe { (*crate::pagelist::Node::page(new_node)).memory_len } < oversized_len);

    // Verify size preserved
    assert_eq!(original_size.rows, unsafe {
        crate::pagelist::Node::rows(new_node)
    });
    assert_eq!(original_size.cols, unsafe {
        crate::pagelist::Node::cols(new_node)
    });

    // Verify dirty flag preserved
    assert!(unsafe { (*crate::pagelist::Node::page(new_node)).dirty });

    // Verify linked list integrity
    assert!(core::ptr::eq(new_node, s.pages.first));
    assert!(unsafe { (*new_node).prev }.is_null());
    assert!(core::ptr::eq(unsafe { (*new_node).next }, second_node));
    assert!(core::ptr::eq(unsafe { (*second_node).prev }, new_node));

    // Verify pin updated correctly
    assert!(core::ptr::eq(new_node, unsafe { (*tracked).node }));
    assert_eq!(5 as size::CellCountInt, unsafe { (*tracked).x });
    assert_eq!(10 as size::CellCountInt, unsafe { (*tracked).y });

    // Verify content preserved
    {
        let page = unsafe { &(*crate::pagelist::Node::page(new_node)) };
        for y in 0..page.size.rows as usize {
            for x in 0..s.cols as usize {
                let rac = page.get_row_and_cell(x, y);
                assert_eq!((x + y * s.cols as usize) as u32, unsafe {
                    (*rac.cell).content_codepoint()
                });
            }
        }
    }

    s.untrack_pin(tracked);
    s.deinit();
}

// ref: test "PageList decommitPoolItem zeroes the dirty region"
//
// A pooled page buffer returning to the free list must read back as all
// zeroes, so the next `create()` that reuses it hands out observably-zero
// memory. `decommit_pool_item` is what upholds this.
//
// PORT NOTE: in test builds `decommit_pool_item` always takes the memset path;
// the madvise-based paths rely on kernel zero-fill semantics that unit tests
// cannot exercise (see `decommit_pool_item`). The reference allocates a
// page-aligned `std_size` buffer from `testing.allocator`; the Rust port uses a
// plain `Vec<u8>` because the test path never touches the pointer alignment.
/// Fill the current tail page to capacity without allocating a successor.
/// Capturing the tail before the loop makes this stop at the allocation
/// boundary needed by bounded-pruning tests.
///
/// PORTS: oracle@main PageList.zig `fillLastPageForTest` (f1a5fab45).
fn fill_last_page_for_test(s: &mut PageList) {
    let last = s.pages.last;
    while unsafe { crate::pagelist::Node::rows(last) }
        < unsafe { crate::pagelist::Node::capacity(last) }.rows
    {
        let _ = s.grow().unwrap();
    }
}

/// Fill a node's whole page mapping with deterministic pseudo-random bytes, so
/// that LZ4 cannot shrink it past the break-even point.
///
/// The oracle uses `std.Random.DefaultPrng`; reproducing its exact stream is not
/// required (nothing asserts specific bytes — only that the page is
/// incompressible), so this uses the xorshift64* already established for the
/// same purpose in `compress/lz4.rs`'s incompressible-input tests.
fn fill_incompressible(node: *mut crate::pagelist::Node, seed: u64) {
    let mut state = seed;
    let (ptr, len) = unsafe {
        let p = crate::pagelist::Node::page(node);
        ((*p).memory_ptr() as *mut u8, (*p).memory_len())
    };
    let mem = unsafe { core::slice::from_raw_parts_mut(ptr, len) };
    for b in mem.iter_mut() {
        state ^= state >> 12;
        state ^= state << 25;
        state ^= state >> 27;
        *b = (state.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 32) as u8;
    }
}

/// Verify every live page belongs to the current validity epoch, has an
/// allocated generation below the next serial, and validates through the same
/// pointer-plus-generation lookup used by external references.
///
/// PORTS: oracle@main PageList.zig `expectLivePageSerialsValidForTest`
/// (f1a5fab45 / f6d9a582e).
fn expect_live_page_serials_valid_for_test(s: &PageList) {
    let mut node = s.pages.first;
    while !node.is_null() {
        let serial = unsafe { (*node).serial };
        assert!(serial >= s.page_serial_epoch);
        assert!(serial < s.page_serial);
        assert!(s.node_is_valid(node, serial));
        node = unsafe { (*node).next };
    }
}

/// PORTS: oracle dd956f37c `test "PageList row erasure renews affected page generations"`
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
)]
#[test]
fn pagelist_row_erasure_renews_affected_page_generations() {
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();
    while s.total_pages() < 2 {
        let _ = s.grow().unwrap();
    }

    let first = s.pages.first;
    let second = unsafe { (*first).next };
    let mut first_serial = unsafe { (*first).serial };
    let mut second_serial = unsafe { (*second).serial };
    let mut activity = s.page_compression.activity_serial;

    s.erase_row(point::Point::History(point::Coordinate { x: 0, y: 0 }))
        .unwrap();
    assert!(!s.node_is_valid(first, first_serial));
    assert!(!s.node_is_valid(second, second_serial));
    assert_ne!(activity, s.page_compression.activity_serial);

    first_serial = unsafe { (*first).serial };
    second_serial = unsafe { (*second).serial };
    activity = s.page_compression.activity_serial;
    s.erase_row_bounded(
        point::Point::History(point::Coordinate { x: 0, y: 0 }),
        unsafe { crate::pagelist::Node::rows(first) } as usize + 1,
    )
    .unwrap();
    assert!(!s.node_is_valid(first, first_serial));
    assert!(!s.node_is_valid(second, second_serial));
    assert_ne!(activity, s.page_compression.activity_serial);

    s.deinit();
}

/// PORTS: oracle dd956f37c `test "PageList trailing row truncation renews page generation"`
#[test]
fn pagelist_trailing_row_truncation_renews_page_generation() {
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();

    let node = s.pages.last;
    let old_serial = unsafe { (*node).serial };
    let trimmed = s.trim_trailing_blank_rows(1);
    s.total_rows -= trimmed as usize;
    assert_eq!(trimmed, 1);
    assert!(!s.node_is_valid(node, old_serial));

    let _ = s.grow().unwrap();
    expect_live_page_serials_valid_for_test(&s);
    s.deinit();
}

/// PORTS: the memoryStats assertions in oracle 0d015b2fc / 6d5dda40d.
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): TIMEOUT in BOTH Miri lanes — re-measured per test in single-worker isolation at a 6-minute bound with `-C debug-assertions=off`, so removing the integrity assertions does not rescue it; the cost is page-mapping/LZ4 volume, not assertion overhead. Covered natively + by ASan."
)]
#[test]
fn pagelist_memory_stats_accounts_compressed_and_resident_bytes() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(10 * 1024 * 1024),
            ..Options::new(80, 24)
        },
    )
    .unwrap();
    fill_compressible_history(&mut s, 4);

    let before = s.memory_stats();
    assert_eq!(before.compressed_pages, 0);
    assert_eq!(before.encoded_bytes, 0);
    assert_eq!(before.decommitted_raw_bytes, 0);
    assert_eq!(before.resident_raw_bytes, before.raw_bytes);

    let _ = s.compress(CompressionMode::Drain);
    let after = s.memory_stats();
    assert!(after.compressed_pages >= 1);
    assert_eq!(
        after.resident_pages + after.compressed_pages,
        s.total_pages()
    );
    assert_eq!(after.raw_bytes, before.raw_bytes);
    assert_eq!(
        after.resident_raw_bytes + after.decommitted_raw_bytes,
        after.raw_bytes
    );
    assert!(after.encoded_bytes > 0);
    // Compression only pays if the encoding is smaller than what it replaced.
    assert!(after.encoded_bytes < after.decommitted_raw_bytes);
    assert!(after.estimated_savings() > 0);
    assert!(after.estimated_resident_bytes() < before.estimated_resident_bytes());
    s.deinit();
}

/// Incremental compression converges: repeated steps eventually report
/// `Complete`, having compressed the cold history.
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): TIMEOUT in BOTH Miri lanes — re-measured per test in single-worker isolation at a 6-minute bound with `-C debug-assertions=off`, so removing the integrity assertions does not rescue it; the cost is page-mapping/LZ4 volume, not assertion overhead. Covered natively + by ASan."
)]
#[test]
fn pagelist_compress_incremental_converges_to_complete() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(10 * 1024 * 1024),
            ..Options::new(80, 24)
        },
    )
    .unwrap();
    fill_compressible_history(&mut s, 6);

    let mut steps = 0usize;
    loop {
        match s.compress(CompressionMode::Incremental) {
            IncrementalCompressionResult::Pending => {
                steps += 1;
                assert!(steps < 10_000, "incremental compression did not converge");
            }
            IncrementalCompressionResult::Complete => break,
            IncrementalCompressionResult::Unsupported => {
                panic!("test builds always support strict reclamation")
            }
        }
    }
    assert_eq!(
        unsafe { crate::pagelist::Node::storage(s.pages.first) },
        Storage::Compressed
    );
    s.deinit();
}

/// `Drain` performs incremental steps until the pass and its verification pass
/// finish, in one call.
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): TIMEOUT in BOTH Miri lanes — re-measured per test in single-worker isolation at a 6-minute bound with `-C debug-assertions=off`, so removing the integrity assertions does not rescue it; the cost is page-mapping/LZ4 volume, not assertion overhead. Covered natively + by ASan."
)]
#[test]
fn pagelist_compress_drain_finishes_in_one_call() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(10 * 1024 * 1024),
            ..Options::new(80, 24)
        },
    )
    .unwrap();
    fill_compressible_history(&mut s, 6);
    assert_eq!(
        s.compress(CompressionMode::Drain),
        IncrementalCompressionResult::Complete
    );
    assert_eq!(
        unsafe { crate::pagelist::Node::storage(s.pages.first) },
        Storage::Compressed
    );
    s.deinit();
}

/// PageList mutations bump the activity token so schedulers know to recompress.
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
)]
#[test]
fn pagelist_mutations_mark_compression_activity() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(10 * 1024 * 1024),
            ..Options::new(80, 24)
        },
    )
    .unwrap();
    let before = s.page_compression.activity_serial;
    fill_compressible_history(&mut s, 2);
    assert_ne!(s.page_compression.activity_serial, before);

    // Compression itself changes representation, not the activity token.
    let after_growth = s.page_compression.activity_serial;
    let _ = s.compress(CompressionMode::Drain);
    assert_eq!(s.page_compression.activity_serial, after_growth);
    s.deinit();
}

/// A node destroyed between steps must not strand the traversal cursor: the
/// next step restarts safely rather than skipping newly allocated pages.
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): TIMEOUT in BOTH Miri lanes — re-measured per test in single-worker isolation at a 6-minute bound with `-C debug-assertions=off`, so removing the integrity assertions does not rescue it; the cost is page-mapping/LZ4 volume, not assertion overhead. Covered natively + by ASan."
)]
#[test]
fn pagelist_compress_incremental_restarts_after_list_mutation() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(10 * 1024 * 1024),
            ..Options::new(80, 24)
        },
    )
    .unwrap();
    fill_compressible_history(&mut s, 6);
    assert_eq!(
        s.compress(CompressionMode::Incremental),
        IncrementalCompressionResult::Pending
    );

    // Erase the history the cursor is standing on, then keep stepping.
    s.erase_history(None);
    fill_compressible_history(&mut s, 4);

    let mut steps = 0usize;
    while s.compress(CompressionMode::Incremental) == IncrementalCompressionResult::Pending {
        steps += 1;
        assert!(steps < 10_000);
    }
    s.deinit();
}

/// Full mode compresses everything eligible and leaves no continuation state.
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): TIMEOUT in BOTH Miri lanes — re-measured per test in single-worker isolation at a 6-minute bound with `-C debug-assertions=off`, so removing the integrity assertions does not rescue it; the cost is page-mapping/LZ4 volume, not assertion overhead. Covered natively + by ASan."
)]
#[test]
fn pagelist_compress_full_mode_leaves_no_continuation() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(10 * 1024 * 1024),
            ..Options::new(80, 24)
        },
    )
    .unwrap();
    fill_compressible_history(&mut s, 4);

    assert_eq!(
        s.compress(CompressionMode::Full),
        IncrementalCompressionResult::Complete
    );
    assert_eq!(
        unsafe { crate::pagelist::Node::storage(s.pages.first) },
        Storage::Compressed
    );
    assert_eq!(s.page_compression.last_serial, None);
    assert!(!s.page_compression.flags.verifying);
    assert!(!s.page_compression.flags.did_compress);
    s.deinit();
}

/// A cold history page compresses, its mapping is discarded, and reading it
/// back restores identical bytes.
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
)]
#[test]
fn pagelist_compress_page_round_trips_a_cold_history_page() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(10 * 1024 * 1024),
            ..Options::new(80, 24)
        },
    )
    .unwrap();
    fill_compressible_history(&mut s, 3);

    let node = s.pages.first;
    let (ptr, len, expected) = unsafe {
        let p = crate::pagelist::Node::page(node);
        let ptr = (*p).memory_ptr() as *mut u8;
        let len = (*p).memory_len();
        (ptr, len, core::slice::from_raw_parts(ptr, len).to_vec())
    };

    assert!(s.compress_page(node));
    assert_eq!(
        unsafe { crate::pagelist::Node::storage(node) },
        Storage::Compressed
    );
    // The test build of `mem::decommit` zeroes instead of calling madvise.
    assert!(unsafe { core::slice::from_raw_parts(ptr, len) }
        .iter()
        .all(|&b| b == 0));

    let restored = unsafe { crate::pagelist::Node::page(node) };
    assert_eq!(unsafe { (*restored).memory_ptr() }, ptr as *const u8);
    assert_eq!(
        unsafe { core::slice::from_raw_parts(ptr, len) },
        &expected[..]
    );
    s.deinit();
}

/// Compressing twice is a no-op: `compress_page` only accepts resident nodes.
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): TIMEOUT in BOTH Miri lanes — re-measured per test in single-worker isolation at a 6-minute bound with `-C debug-assertions=off`, so removing the integrity assertions does not rescue it; the cost is page-mapping/LZ4 volume, not assertion overhead. Covered natively + by ASan."
)]
/// PORTS: oracle@main PageList.zig `test "PageList cold compression continues
/// after an incompressible page"` (9156ada16).
///
/// The point is the CONTINUE, not the skip: a page that fails to compress must
/// not abort the pass, and must stay a candidate for later passes while the page
/// that did compress is skipped. `compress_page`'s two `return false` arms leave
/// the node untouched, which is what makes both halves true — but nothing
/// asserted it, so a change that aborted the pass on first failure would have
/// been invisible to the other eight compression tests.
#[test]
fn pagelist_cold_compression_continues_after_an_incompressible_page() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(10 * 1024 * 1024),
            ..Options::new(80, 24)
        },
    )
    .unwrap();
    grow_cold_pages_for_test(&mut s, 2);

    let first = s.pages.first;
    let second = unsafe { (*first).next };
    assert!(!second.is_null(), "need two cold pages");
    fill_incompressible(first, 0x434F_4C44_5041_4745);

    let page_size = s.page_size;
    let _ = s.compress(CompressionMode::Full);
    let memory = s.memory_stats();

    assert_eq!(1, memory.compressed_pages);
    assert!(!unsafe { crate::pagelist::Node::is_compressed(first) });
    assert!(unsafe { crate::pagelist::Node::is_compressed(second) });
    // try testing.expectEqual(second.metadata().memory.len,
    //     memory.decommitted_raw_bytes);
    //
    // Exactly the second page's mapping was decommitted — nothing more (the
    // incompressible first page must still be resident) and nothing less.
    assert_eq!(
        unsafe { (*crate::pagelist::Node::metadata(second)).memory_len() },
        memory.decommitted_raw_bytes
    );
    assert!(
        memory.encoded_bytes < memory.decommitted_raw_bytes,
        "the compressed page must be smaller than the mapping it replaced"
    );
    assert_eq!(page_size, s.page_size);

    // Failed resident candidates are retried on later passes, while the
    // successful page stays compressed and is skipped — so a second full pass
    // must change nothing at all.
    let _ = s.compress(CompressionMode::Full);
    assert_eq!(memory, s.memory_stats());

    // defer s.deinit(); (the reference's; it was missing here, so the second
    // page's encoded allocation leaked: NEW-FINDINGS N418)
    s.deinit();
}

/// PORTS: oracle@main PageList.zig `test "PageList compression leaves
/// incompressible pages resident"` (9156ada16).
///
/// The single-page counterpart of the test above: `compress_page` returns false
/// and leaves the node resident, rather than decommitting a mapping it cannot
/// shrink. `page_size` is asserted unchanged because a failed candidate must not
/// be accounted as if it had been compressed.
#[test]
fn pagelist_compression_leaves_incompressible_pages_resident() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(10 * 1024 * 1024),
            ..Options::new(80, 24)
        },
    )
    .unwrap();

    let node = s.pages.first;
    fill_incompressible(node, 0x5041_4745_4C49_5354);
    let page_size = s.page_size;

    assert!(!s.compress_page(node));
    assert!(!unsafe { crate::pagelist::Node::is_compressed(node) });
    assert_eq!(page_size, s.page_size);

    // defer s.deinit(); (the reference's; it was missing here, NEW-FINDINGS N418)
    s.deinit();
}

#[test]
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION — BOTH LANES (rustty-vt, parity tail C-1; .port/phase-2/MIRI_EXCEPTIONS.md, 'Parity tail C-1 — the G6 triage'): past a 15-minute Miri bound with debug assertions on (G6) and past an 8-minute bound with them off (G6c), both measured one test per process; runs natively in cargo test --workspace and under ASan (see MIRI_EXCEPTIONS.md)"
)]
fn pagelist_compress_page_rejects_an_already_compressed_node() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(10 * 1024 * 1024),
            ..Options::new(80, 24)
        },
    )
    .unwrap();
    fill_compressible_history(&mut s, 3);
    let node = s.pages.first;
    assert!(s.compress_page(node));
    assert!(!s.compress_page(node));
    let _ = unsafe { crate::pagelist::Node::page(node) };
    s.deinit();
}

/// A failing strict decommit leaves the node resident and its mapping intact.
///
/// PORT NOTE (WS7 B10): tripwire injection is compiled out when
/// `debug_assertions` is off, so this test is inert under `--release`. It is
/// ignored there rather than left to fail (the other tripwire tests in this file
/// predate that convention and are part of the known-red `--release` set).
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): out of budget in the debug-assertions-ON G6 lane, and NOT rescuable by the G6c debug-assertions-off lane — this test drives a `debug_assertions`-only mechanism and is already `cfg_attr(not(debug_assertions), ignore)`, so turning the assertions off makes it inert rather than fast. Covered natively + by ASan."
)]
#[test]
fn pagelist_compress_page_decommit_failure_leaves_the_page_resident() {
    use rustty_util::tripwire::ResetMode;

    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(10 * 1024 * 1024),
            ..Options::new(80, 24)
        },
    )
    .unwrap();
    fill_compressible_history(&mut s, 3);

    let node = s.pages.first;
    let (ptr, len, expected) = unsafe {
        let p = crate::pagelist::Node::page(node);
        let ptr = (*p).memory_ptr() as *mut u8;
        let len = (*p).memory_len();
        (ptr, len, core::slice::from_raw_parts(ptr, len).to_vec())
    };

    crate::pagelist::compress::COMPRESS_PAGE_TW.with(|tw| {
        tw.borrow_mut().errorAlways(
            crate::pagelist::compress::CompressPageFp::Decommit,
            crate::pagelist::compress::DecommitFailed,
        )
    });
    assert!(!s.compress_page(node));
    assert_eq!(
        unsafe { crate::pagelist::Node::storage(node) },
        Storage::Resident
    );
    assert_eq!(
        unsafe { core::slice::from_raw_parts(ptr, len) },
        &expected[..]
    );

    // With the tripwire disarmed, the same page compresses normally.
    crate::pagelist::compress::COMPRESS_PAGE_TW
        .with(|tw| tw.borrow_mut().end(ResetMode::Reset).unwrap());
    assert!(s.compress_page(node));
    let _ = unsafe { crate::pagelist::Node::page(node) };
    s.deinit();
}

/// A full pass compresses every complete offscreen history page and leaves the
/// viewport and active pages resident.
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): TIMEOUT in BOTH Miri lanes — re-measured per test in single-worker isolation at a 6-minute bound with `-C debug-assertions=off`, so removing the integrity assertions does not rescue it; the cost is page-mapping/LZ4 volume, not assertion overhead. Covered natively + by ASan."
)]
#[test]
fn pagelist_compress_full_leaves_viewport_and_active_resident() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(10 * 1024 * 1024),
            ..Options::new(80, 24)
        },
    )
    .unwrap();
    fill_compressible_history(&mut s, 4);

    s.compress_full();

    let active = s.get_top_left(point::Tag::Active).node;
    let viewport_first = s.get_top_left(point::Tag::Viewport).node;
    let viewport_last = s.get_bottom_right(point::Tag::Viewport).unwrap().node;
    assert_eq!(
        unsafe { crate::pagelist::Node::storage(active) },
        Storage::Resident
    );
    assert_eq!(
        unsafe { crate::pagelist::Node::storage(viewport_first) },
        Storage::Resident
    );
    assert_eq!(
        unsafe { crate::pagelist::Node::storage(viewport_last) },
        Storage::Resident
    );
    assert_eq!(
        unsafe { crate::pagelist::Node::storage(s.pages.first) },
        Storage::Compressed
    );
    s.deinit();
}

/// PORTS: oracle d0a26d146
/// `test "PageList replacements preserve compression continuation and mark activity"`
///
/// NOTE: contrary to the WS7 plan's prose, the oracle does NOT reset the
/// continuation here. `increase_capacity` / `compact` only bump the activity
/// token; `last_serial` and `next_serial` are preserved so a still-valid
/// traversal keeps its progress.
#[test]
fn pagelist_replacements_preserve_compression_continuation_and_mark_activity() {
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();

    let state = crate::pagelist::compress::IncrementalCompressionState {
        flags: crate::pagelist::compress::IncrementalCompressionFlags {
            did_compress: false,
            verifying: true,
        },
        activity_serial: 42,
        last_serial: Some(7),
        next_serial: 8,
    };
    let expected = crate::pagelist::compress::IncrementalCompressionState {
        activity_serial: 43,
        ..state
    };

    s.page_compression = state;
    let replacement = s.increase_capacity(s.pages.first, None).unwrap();
    assert_eq!(s.page_compression, expected);

    s.page_compression = state;
    let _ = s.compact(replacement).unwrap().unwrap();
    assert_eq!(s.page_compression, expected);
    s.deinit();
}

/// PORTS: oracle 91a63e528
/// `test "PageList bounded pruning after split invalidation preserves live serials"`
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
)]
#[test]
fn pagelist_bounded_pruning_after_split_invalidation_preserves_live_serials() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(2 * std_size()),
            ..Options::new(80, 24)
        },
    )
    .unwrap();

    while s.total_pages() < 2 {
        let _ = s.grow().unwrap();
    }
    let first = s.pages.first;
    let old_serial = unsafe { (*first).serial };
    let activity = s.page_compression.activity_serial;

    s.split(Pin {
        node: first,
        y: unsafe { crate::pagelist::Node::rows(first) } / 2,
        x: 0,
        garbage: false,
    })
    .unwrap();
    assert!(!s.node_is_valid(first, old_serial));
    assert_ne!(activity, s.page_compression.activity_serial);

    fill_last_page_for_test(&mut s);
    let _ = s.grow().unwrap();
    expect_live_page_serials_valid_for_test(&s);
    s.deinit();
}

/// PORTS: oracle a804e3ac7
/// `test "PageList bounded pruning after partial erase preserves live serials"`
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
)]
#[test]
fn pagelist_bounded_pruning_after_partial_erase_preserves_live_serials() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(2 * std_size()),
            ..Options::new(80, 24)
        },
    )
    .unwrap();

    while s.total_pages() < 2 {
        let _ = s.grow().unwrap();
    }
    let first = s.pages.first;
    let old_serial = unsafe { (*first).serial };
    let old_rows = unsafe { crate::pagelist::Node::rows(first) };

    s.erase_history(Some(point::Point::History(point::Coordinate {
        x: 0,
        y: 0,
    })));
    assert!(core::ptr::eq(first, s.pages.first));
    assert_eq!(old_rows - 1, unsafe { crate::pagelist::Node::rows(first) });
    assert!(!s.node_is_valid(first, old_serial));

    fill_last_page_for_test(&mut s);
    let _ = s.grow().unwrap();
    expect_live_page_serials_valid_for_test(&s);
    s.deinit();
}

/// PORTS: oracle a804e3ac7
/// `test "PageList partial erase restarts compression before continuation"`
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): TIMEOUT in BOTH Miri lanes — re-measured per test in single-worker isolation at a 6-minute bound with `-C debug-assertions=off`, so removing the integrity assertions does not rescue it; the cost is page-mapping/LZ4 volume, not assertion overhead. Covered natively + by ASan."
)]
#[test]
fn pagelist_partial_erase_restarts_compression_before_continuation() {
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();
    fill_compressible_history(
        &mut s,
        crate::pagelist::compress::INCREMENTAL_COMPRESSION_MAX_INSPECTED + 2,
    );
    let _ = s.compress(CompressionMode::Full);

    let first = s.pages.first;
    assert!(unsafe { crate::pagelist::Node::is_compressed(first) });

    // Park the incremental continuation on a marker page well past `first`.
    let mut marker = first;
    for _ in 1..crate::pagelist::compress::INCREMENTAL_COMPRESSION_MAX_INSPECTED {
        marker = unsafe { (*marker).next };
    }
    s.page_compression = crate::pagelist::compress::IncrementalCompressionState {
        flags: crate::pagelist::compress::IncrementalCompressionFlags {
            did_compress: false,
            verifying: true,
        },
        activity_serial: s.page_compression.activity_serial,
        last_serial: Some(unsafe { (*marker).serial }),
        next_serial: s.page_serial,
    };

    let activity = s.page_compression.activity_serial;
    s.erase_history(Some(point::Point::History(point::Coordinate {
        x: 0,
        y: 0,
    })));
    assert!(!unsafe { crate::pagelist::Node::is_compressed(first) });
    assert_ne!(activity, s.page_compression.activity_serial);

    // The changed generation is before the saved marker, so continuation must
    // restart and recompress it instead of reporting verification complete.
    assert_eq!(
        s.compress(CompressionMode::Incremental),
        IncrementalCompressionResult::Pending
    );
    assert!(unsafe { crate::pagelist::Node::is_compressed(first) });
    s.deinit();
}

/// PORTS: oracle f1a5fab45 (+ f6d9a582e's epoch assertion)
/// `test "PageList repeated bounded pruning after split preserves live serials"`
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
)]
#[test]
fn pagelist_repeated_bounded_pruning_after_split_preserves_live_serials() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(3 * std_size()),
            ..Options::new(80, 24)
        },
    )
    .unwrap();

    let epoch = s.page_serial_epoch;
    while s.total_pages() < 3 {
        let _ = s.grow().unwrap();
    }
    let first = s.pages.first;
    s.split(Pin {
        node: first,
        y: unsafe { crate::pagelist::Node::rows(first) } / 2,
        x: 0,
        garbage: false,
    })
    .unwrap();

    // The split target has a fresh serial but precedes older successor pages.
    // Prune both the old source and then that target while verifying ordinary
    // list mutation does not advance the whole-list validity epoch.
    for _ in 0..2 {
        while unsafe { crate::pagelist::Node::rows(s.pages.last) }
            < unsafe { crate::pagelist::Node::capacity(s.pages.last) }.rows
        {
            let _ = s.grow().unwrap();
        }
        let _ = s.grow().unwrap();

        // Ordinary pruning invalidates one generation at a time through live
        // list validation; only reset may begin a new whole-list epoch.
        assert_eq!(epoch, s.page_serial_epoch);
        expect_live_page_serials_valid_for_test(&s);
    }
    s.deinit();
}

/// PORTS: oracle f1a5fab45
/// `test "PageList bounded pruning after front replacement preserves live serials"`
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
)]
#[test]
fn pagelist_bounded_pruning_after_front_replacement_preserves_live_serials() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(2 * std_size()),
            ..Options::new(80, 24)
        },
    )
    .unwrap();

    while s.total_pages() < 2 {
        let _ = s.grow().unwrap();
    }
    let old = s.pages.first;
    let old_serial = unsafe { (*old).serial };
    let replacement = s.increase_capacity(old, None).unwrap();
    assert!(!core::ptr::eq(replacement, old));
    assert!(!s.node_is_valid(old, old_serial));

    fill_last_page_for_test(&mut s);
    let _ = s.grow().unwrap();

    expect_live_page_serials_valid_for_test(&s);
    s.deinit();
}

/// PORTS: oracle f1a5fab45
/// `test "PageList bounded pruning after middle replacement preserves live serials"`
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
)]
#[test]
fn pagelist_bounded_pruning_after_middle_replacement_preserves_live_serials() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(3 * std_size()),
            ..Options::new(80, 24)
        },
    )
    .unwrap();

    while s.total_pages() < 3 {
        let _ = s.grow().unwrap();
    }
    let old = unsafe { (*s.pages.first).next };
    let old_serial = unsafe { (*old).serial };
    let replacement = s.increase_capacity(old, None).unwrap();
    assert!(!core::ptr::eq(replacement, old));
    assert!(!s.node_is_valid(old, old_serial));

    // Prune the original first page and then the fresh middle replacement.
    for _ in 0..2 {
        fill_last_page_for_test(&mut s);
        let _ = s.grow().unwrap();
        expect_live_page_serials_valid_for_test(&s);
    }
    s.deinit();
}

/// A node built from a compressed representation restores to the SAME mapping
/// and yields identical bytes; `page()` transparently restores.
#[test]
fn pagelist_node_restore_preserve_recovers_the_same_mapping() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(1000),
            ..Options::new(20, 5)
        },
    )
    .unwrap();
    let node = s.pages.first;
    unsafe {
        let page = crate::pagelist::Node::page(node);
        let ptr = (*page).memory_ptr() as *mut u8;
        let len = (*page).memory_len();
        let expected: Vec<u8> = core::slice::from_raw_parts(ptr, len).to_vec();

        let mut scratch = vec![0u8; crate::compress::required_scratch(len).unwrap()];
        let mut table: crate::compress::lz4::HashTable = [0; crate::compress::lz4::HASH_TABLE_LEN];
        let compressed = crate::compress::CompressedPage::init(
            Global,
            page as *const Page,
            &mut scratch,
            &mut table,
        )
        .unwrap()
        .unwrap();

        // Model a successful decommit, then publish the compressed state.
        core::ptr::write_bytes(ptr, 0, len);
        (*node).data = NodeData::Compressed(compressed);
        assert_eq!(crate::pagelist::Node::storage(node), Storage::Compressed);
        assert!(crate::pagelist::Node::page_if_resident(node).is_null());

        // `page()` restores transparently.
        let restored = crate::pagelist::Node::page(node);
        assert_eq!(crate::pagelist::Node::storage(node), Storage::Resident);
        assert_eq!((*restored).memory_ptr(), ptr as *const u8);
        assert_eq!(core::slice::from_raw_parts(ptr, len), &expected[..]);
    }
    s.deinit();
}

/// `page_preserving_state` reads a compressed node WITHOUT restoring it.
#[test]
fn pagelist_page_preserving_state_leaves_a_compressed_node_compressed() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(1000),
            ..Options::new(20, 5)
        },
    )
    .unwrap();
    let node = s.pages.first;
    unsafe {
        let page = crate::pagelist::Node::page(node);
        let ptr = (*page).memory_ptr() as *mut u8;
        let len = (*page).memory_len();
        let expected: Vec<u8> = core::slice::from_raw_parts(ptr, len).to_vec();

        let mut scratch = vec![0u8; crate::compress::required_scratch(len).unwrap()];
        let mut table: crate::compress::lz4::HashTable = [0; crate::compress::lz4::HASH_TABLE_LEN];
        let compressed = crate::compress::CompressedPage::init(
            Global,
            page as *const Page,
            &mut scratch,
            &mut table,
        )
        .unwrap()
        .unwrap();
        core::ptr::write_bytes(ptr, 0, len);
        (*node).data = NodeData::Compressed(compressed);

        let mut preserved = crate::pagelist::Node::page_preserving_state(node, Global).unwrap();
        let p = preserved.page();
        assert_ne!((*p).memory_ptr(), ptr as *const u8);
        assert_eq!(
            core::slice::from_raw_parts((*p).memory_ptr(), len),
            &expected[..]
        );
        preserved.deinit();

        // Still compressed, and the discarded mapping is untouched.
        assert_eq!(crate::pagelist::Node::storage(node), Storage::Compressed);
        assert!(core::slice::from_raw_parts(ptr, len)
            .iter()
            .all(|&b| b == 0));

        // Restore so deinit's accounting sees a resident page.
        let _ = crate::pagelist::Node::page(node);
    }
    s.deinit();
}

/// S122-F1: `page_preserving_state`'s decode-failure path
/// (PageList.zig:186 `log.err("failed to clone compressed page err={}", ...)`,
/// mod.rs:572-573). A codec error here is "internal corruption" per the
/// reference's own comment — reproduced by corrupting a real compressed
/// node's `encoded` bytes (the same `0xFF`-fill technique the malformed-data
/// tests above use) so `clone_buf`'s LZ4 decode genuinely fails, then
/// asserting the diagnostic fires on the panicking path.
#[test]
fn pagelist_page_preserving_state_logs_and_panics_on_decode_failure() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(1000),
            ..Options::new(20, 5)
        },
    )
    .unwrap();
    let node = s.pages.first;
    unsafe {
        let page = crate::pagelist::Node::page(node);
        let len = (*page).memory_len();
        let mut scratch = vec![0u8; crate::compress::required_scratch(len).unwrap()];
        let mut table: crate::compress::lz4::HashTable = [0; crate::compress::lz4::HASH_TABLE_LEN];
        let compressed = crate::compress::CompressedPage::init(
            Global,
            page as *const Page,
            &mut scratch,
            &mut table,
        )
        .unwrap()
        .unwrap();
        (*node).data = NodeData::Compressed(compressed);
    }
    compressed_encoded_fill_for_test(node, 0xFF);

    let cap = crate::log_capture::start();
    let node_addr = node as usize;
    let result = std::panic::catch_unwind(move || unsafe {
        crate::pagelist::Node::page_preserving_state(
            node_addr as *const crate::pagelist::Node,
            Global,
        )
    });
    assert!(result.is_err(), "expected the corrupted decode to panic");
    let messages = cap.messages();
    assert!(
        messages
            .iter()
            .any(|(level, target, msg)| *level == log::Level::Error
                && target == "page_list"
                && msg.starts_with("failed to clone compressed page err=")),
        "expected the clone-compressed-page diagnostic, got: {messages:?}"
    );
    drop(cap);

    // The panicking path never writes back to `node.data` on failure, so the
    // node is still `Compressed` with its corrupted encoding. `deinit` tears
    // every node down through `RestoreMode::Discard` (mod.rs:2124), which
    // never decodes, so it is safe to call directly.
    s.deinit();
}

/// S122-F1: `Node::restore`'s `RestoreMode::Preserve` decode-failure path
/// (PageList.zig:278 `log.err("failed to restore compressed page err={}",
/// ...)`, mod.rs:624-625) — reached through `Node::page`, the ordinary
/// "decompress on access" accessor, when the retained encoding is corrupted.
#[test]
fn pagelist_node_page_logs_and_panics_on_restore_decode_failure() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(1000),
            ..Options::new(20, 5)
        },
    )
    .unwrap();
    let node = s.pages.first;
    unsafe {
        let page = crate::pagelist::Node::page(node);
        let len = (*page).memory_len();
        let mut scratch = vec![0u8; crate::compress::required_scratch(len).unwrap()];
        let mut table: crate::compress::lz4::HashTable = [0; crate::compress::lz4::HASH_TABLE_LEN];
        let compressed = crate::compress::CompressedPage::init(
            Global,
            page as *const Page,
            &mut scratch,
            &mut table,
        )
        .unwrap()
        .unwrap();
        (*node).data = NodeData::Compressed(compressed);
    }
    compressed_encoded_fill_for_test(node, 0xFF);

    let cap = crate::log_capture::start();
    let node_addr = node as usize;
    let result = std::panic::catch_unwind(move || unsafe {
        crate::pagelist::Node::page(node_addr as *mut crate::pagelist::Node)
    });
    assert!(result.is_err(), "expected the corrupted decode to panic");
    let messages = cap.messages();
    assert!(
        messages
            .iter()
            .any(|(level, target, msg)| *level == log::Level::Error
                && target == "page_list"
                && msg.starts_with("failed to restore compressed page err=")),
        "expected the restore-compressed-page diagnostic, got: {messages:?}"
    );
    drop(cap);

    // As above: the node is still `Compressed` (corrupted) after the panic;
    // `deinit`'s discard-restore teardown handles it without decoding.
    s.deinit();
}

/// PORTS: oracle@main PageList.zig `test "PageList preserved page keeps
/// compressed storage"` (9156ada16).
///
/// The reference brackets `pagePreservingState` with two
/// `testing.FailingAllocator` arms — one proving the RESIDENT case never
/// allocates, one proving that a failed decode allocation leaves the node and
/// its stats untouched. Both are ported as written (parity tail S7-3: the
/// function takes the reference's `alloc`; this replaced a tripwire stand-in).
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): TIMEOUT in BOTH Miri lanes — the cost is page-mapping/LZ4 volume, not assertion overhead, so the debug-assertions-off G6c lane does not rescue it either. Covered natively + by ASan."
)]
#[test]
fn pagelist_preserved_page_keeps_compressed_storage() {
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();

    let node = s.pages.first;
    let resident = unsafe { crate::pagelist::Node::page(node) };
    unsafe {
        (*resident).dirty = true;
        *(*resident).get_row_and_cell(3, 2).cell = Cell::init('X' as u32);
    }

    // Resident nodes can be borrowed without allocating an unnecessary copy.
    {
        // var failing = testing.FailingAllocator.init(alloc, .{
        //     .fail_index = 0,
        // });
        let failing = FailingAllocator::over_heap(FailingConfig {
            fail_index: 0,
            ..FailingConfig::default()
        });
        let c = failing.allocator();
        with_handle(&c, |failing_alloc| {
            let mut preserved =
                unsafe { crate::pagelist::Node::page_preserving_state(node, failing_alloc) }
                    .expect("a resident page must not allocate");
            // defer preserved.deinit();
            match preserved {
                PreservedPage::Borrowed(p) => assert_eq!(p, resident as *const Page),
                PreservedPage::Owned { .. } => {
                    panic!("a resident page must be borrowed, not copied")
                }
            }
            preserved.deinit();
        });
        assert!(!failing.has_induced_failure.get());
    }

    let (retained_ptr, memory_len, expected) = unsafe {
        let ptr = (*resident).memory_ptr() as *mut u8;
        let len = (*resident).memory_len();
        (ptr, len, core::slice::from_raw_parts(ptr, len).to_vec())
    };

    assert!(s.compress_page(node));
    let stats = s.memory_stats();
    let expected_encoded = compressed_encoded_for_test(node);

    // Test decommit simulates physical reclamation by clearing the retained
    // mapping. A preserved page must decode elsewhere rather than restoring it.
    assert!(
        unsafe { core::slice::from_raw_parts(retained_ptr, memory_len) }
            .iter()
            .all(|&b| b == 0)
    );

    // Preserved-page allocation is opportunistic for callers. Failure leaves
    // the node and its compressed representation untouched.
    {
        let failing = FailingAllocator::over_heap(FailingConfig {
            fail_index: 0,
            ..FailingConfig::default()
        });
        let c = failing.allocator();
        with_handle(&c, |failing_alloc| {
            match unsafe { crate::pagelist::Node::page_preserving_state(node, failing_alloc) } {
                Err(OutOfMemory) => {}
                Ok(mut p) => {
                    p.deinit();
                    panic!("a failed decode-buffer allocation must be reported");
                }
            }
        });
        assert!(failing.has_induced_failure.get());
        assert_eq!(
            unsafe { crate::pagelist::Node::storage(node) },
            Storage::Compressed
        );
        assert_eq!(stats, s.memory_stats());
        assert_eq!(expected_encoded, compressed_encoded_for_test(node));
    }

    let mut preserved =
        unsafe { crate::pagelist::Node::page_preserving_state(node, Global) }.unwrap();
    match preserved {
        PreservedPage::Borrowed(_) => {
            panic!("a compressed page must be decoded into an owned copy")
        }
        PreservedPage::Owned { .. } => {}
    }
    let page_ = preserved.page();

    unsafe {
        assert_ne!((*page_).memory_ptr(), retained_ptr as *const u8);
        assert_eq!(
            core::slice::from_raw_parts((*page_).memory_ptr(), memory_len),
            &expected[..]
        );
        assert!((*page_).dirty);
        assert_eq!(
            'X' as u32,
            (*(*page_).get_row_and_cell(3, 2).cell).content_codepoint()
        );
    }
    preserved.deinit();

    // The node still owns the same compressed representation, and neither its
    // storage accounting nor its discarded raw mapping changed while cloning.
    assert_eq!(
        unsafe { crate::pagelist::Node::storage(node) },
        Storage::Compressed
    );
    assert_eq!(stats, s.memory_stats());
    assert_eq!(
        unsafe { (*crate::pagelist::Node::metadata(node)).memory_ptr() },
        retained_ptr as *const u8
    );
    assert!(
        unsafe { core::slice::from_raw_parts(retained_ptr, memory_len) }
            .iter()
            .all(|&b| b == 0)
    );
    assert_eq!(expected_encoded, compressed_encoded_for_test(node));

    s.deinit();
}

/// PORTS: oracle@main PageList.zig `test "PageList compresses only complete cold
/// history pages"` (9156ada16).
///
/// The boundary node — the one holding the active area's top-left row — has a
/// historical prefix, so compressing it would decommit rows that are on screen.
/// The pass must stop there, and every node from it onward must stay resident.
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): TIMEOUT in BOTH Miri lanes — the cost is page-mapping/LZ4 volume, not assertion overhead, so the debug-assertions-off G6c lane does not rescue it either. Covered natively + by ASan."
)]
#[test]
fn pagelist_compresses_only_complete_cold_history_pages() {
    // More active rows than one page at these dimensions can hold ensures the
    // active area spans multiple nodes when the pass chooses its boundary.
    let active_rows = PageList::initial_capacity(80).rows + 1;
    let mut s = PageList::init(Global, Options::new(80, active_rows)).unwrap();
    grow_cold_pages_for_test(&mut s, 2);

    // Move the active top into the boundary page so it has both a historical
    // prefix and active rows while the active area still spans later pages.
    let _ = s.grow().unwrap();

    let active = s.get_top_left(point::Tag::Active);
    let active_node = active.node;
    assert!(active.y > 0);
    assert_ne!(active_node, s.pages.last);

    let mut expected_compressed: usize = 0;
    let mut expected_raw_bytes: usize = 0;
    let mut current = s.pages.first;
    while !current.is_null() {
        if current == active_node {
            break;
        }
        expected_compressed += 1;
        expected_raw_bytes += unsafe { (*crate::pagelist::Node::metadata(current)).memory_len() };
        current = unsafe { (*current).next };
    }
    assert_eq!(2, expected_compressed);

    let first = s.pages.first;
    let (first_memory, first_len, expected) = unsafe {
        let page = crate::pagelist::Node::page(first);
        *(*page).get_row_and_cell(0, 0).cell = Cell::init('X' as u32);
        let ptr = (*page).memory_ptr();
        let len = (*page).memory_len();
        (ptr, len, core::slice::from_raw_parts(ptr, len).to_vec())
    };
    let page_size = s.page_size;

    let _ = s.compress(CompressionMode::Full);
    let memory = s.memory_stats();
    assert_eq!(expected_compressed, memory.compressed_pages);
    assert_eq!(expected_raw_bytes, memory.decommitted_raw_bytes);
    assert!(memory.encoded_bytes < memory.decommitted_raw_bytes);
    assert_eq!(page_size, s.page_size);

    let mut actual_encoded_bytes: usize = 0;
    current = s.pages.first;
    while !current.is_null() {
        if current == active_node {
            break;
        }
        assert!(unsafe { crate::pagelist::Node::is_compressed(current) });
        actual_encoded_bytes += compressed_encoded_for_test(current).len();
        current = unsafe { (*current).next };
    }
    assert_eq!(actual_encoded_bytes, memory.encoded_bytes);
    current = active_node;
    while !current.is_null() {
        assert!(!unsafe { crate::pagelist::Node::is_compressed(current) });
        current = unsafe { (*current).next };
    }

    // Restoring the oldest page preserves both the mapping identity and all of
    // its bytes even though the pass discarded its physical pages.
    assert_eq!(first_memory, unsafe {
        (*crate::pagelist::Node::metadata(first)).memory_ptr()
    });
    unsafe {
        let page = crate::pagelist::Node::page(first);
        assert_eq!(
            core::slice::from_raw_parts((*page).memory_ptr(), first_len),
            &expected[..]
        );
        assert_eq!(first_memory, (*page).memory_ptr());
    }

    s.deinit();
}

/// PORTS: oracle@main PageList.zig `test "PageList compression uses temporary
/// scratch for oversized pages"` (9156ada16).
///
/// A page grown past `std_size` cannot borrow a pool item for its encode
/// scratch, so `compress_page` falls back to a temporary allocation. The node
/// must still end up compressed, keep its exact retained mapping, and decode
/// back byte-identically.
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): TIMEOUT in BOTH Miri lanes — the cost is page-mapping/LZ4 volume, not assertion overhead, so the debug-assertions-off G6c lane does not rescue it either. Covered natively + by ASan."
)]
#[test]
fn pagelist_compression_uses_temporary_scratch_for_oversized_pages() {
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();

    let mut node = s.pages.first;
    while unsafe { (*crate::pagelist::Node::page(node)).memory_len() } <= std_size() {
        node = s
            .increase_capacity(node, Some(IncreaseCapacity::GraphemeBytes))
            .unwrap();
    }

    let (memory_ptr, memory_len, expected) = unsafe {
        let page = crate::pagelist::Node::page(node);
        let ptr = (*page).memory_ptr();
        let len = (*page).memory_len();
        (ptr, len, core::slice::from_raw_parts(ptr, len).to_vec())
    };
    let page_size = s.page_size;

    assert!(s.compress_page(node));
    assert!(unsafe { crate::pagelist::Node::is_compressed(node) });
    assert_eq!(page_size, s.page_size);
    assert_eq!(memory_ptr, unsafe {
        (*crate::pagelist::Node::metadata(node)).memory_ptr()
    });
    assert_eq!(memory_len, unsafe {
        (*crate::pagelist::Node::metadata(node)).memory_len()
    });

    unsafe {
        let page = crate::pagelist::Node::page(node);
        assert_eq!(
            core::slice::from_raw_parts((*page).memory_ptr(), memory_len),
            &expected[..]
        );
        assert!(!crate::pagelist::Node::is_compressed(node));
        assert_eq!(memory_ptr, (*page).memory_ptr());
    }

    s.deinit();
}

/// Overwrite a compressed node's encoded bytes in place, without restoring it.
///
/// PORT NOTE: the reference spells this `@memset(node.data.compressed.encoded,
/// b)`. As with `compressed_encoded_for_test`, the payload is reached through
/// the module's own raw `compressed_payload` projection so no `&NodeData` is
/// formed; the pointer is derived from `addr_of_mut!` so the write carries
/// mutable provenance.
///
/// # Panics
/// Panics if the node is resident.
fn compressed_encoded_fill_for_test(node: *mut crate::pagelist::Node, byte: u8) {
    unsafe {
        let data = core::ptr::addr_of_mut!((*node).data);
        assert_eq!(NodeData::tag(data), NodeData::TAG_COMPRESSED);
        let payload = NodeData::compressed_payload(data) as *mut crate::compress::CompressedPage;
        (*payload).encoded.fill(byte);
    }
}

/// PORTS: oracle@main PageList.zig `test "PageList incremental compression skips
/// visible history"` (9156ada16).
#[test]
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'Parity tail C-1 — the G6 triage'): past a 15-minute Miri bound in the debug-assertions-ON G6 lane, where `Page::assert_integrity` runs on nearly every page mutation (measured per test; see the C-1 table there). It RUNS in the G6c lane (`check_leaks.sh --debug-assertions-off`). Do NOT widen this to a plain `miri` ignore: that would drop the test from BOTH lanes."
)]
fn pagelist_incremental_compression_skips_visible_history() {
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();
    grow_cold_pages_for_test(&mut s, 3);

    let initial_activity = s.page_compression.activity_serial;
    assert!(initial_activity > 0);

    s.scroll(Scroll::Top);
    let top_activity = s.page_compression.activity_serial;
    assert_ne!(top_activity, initial_activity);
    assert_eq!(
        IncrementalCompressionResult::Complete,
        s.compress(CompressionMode::Drain)
    );

    let first = s.pages.first;
    let second = unsafe { (*first).next };
    assert_eq!(first, s.get_top_left(point::Tag::Viewport).node);
    assert_eq!(
        first,
        s.get_bottom_right(point::Tag::Viewport).unwrap().node
    );
    assert!(!unsafe { crate::pagelist::Node::is_compressed(first) });

    let mut eligible = crate::pagelist::compress::CompressionIterator::init(&s);
    let mut compressed_pages: usize = 0;
    while let Some(node) = eligible.next() {
        compressed_pages += 1;
        assert!(unsafe { crate::pagelist::Node::is_compressed(node) });
    }
    assert!(compressed_pages > 0);

    // Move the viewport to the start of the second page. Rendering it restores
    // that page, while the first page which just left view becomes eligible.
    s.scroll(Scroll::Row(
        unsafe { crate::pagelist::Node::rows(first) } as usize
    ));
    assert_eq!(second, s.get_top_left(point::Tag::Viewport).node);
    let _ = unsafe { crate::pagelist::Node::page(second) };
    assert!(!unsafe { crate::pagelist::Node::is_compressed(second) });
    let _ = s.compress(CompressionMode::Drain);
    assert!(unsafe { crate::pagelist::Node::is_compressed(first) });
    assert!(!unsafe { crate::pagelist::Node::is_compressed(second) });

    // Returning to the active area makes every complete historical page
    // eligible again, including the page which was just visible.
    s.scroll(Scroll::Active);
    let _ = s.compress(CompressionMode::Drain);
    assert!(unsafe { crate::pagelist::Node::is_compressed(second) });
    assert!(!s.page_compression.flags.did_compress);
    assert!(!s.page_compression.flags.verifying);
    assert_eq!(None, s.page_compression.last_serial);
    assert_eq!(0u64, s.page_compression.next_serial);

    s.deinit();
}

/// PORTS: oracle@main PageList.zig `test "PageList owns incremental compression
/// state"` (9156ada16).
#[test]
fn pagelist_owns_incremental_compression_state() {
    use crate::pagelist::compress::{IncrementalCompressionFlags, IncrementalCompressionState};

    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();

    let state = IncrementalCompressionState {
        flags: IncrementalCompressionFlags {
            did_compress: true,
            verifying: true,
        },
        activity_serial: 42,
        last_serial: Some(42),
        next_serial: 43,
    };
    let bumped = IncrementalCompressionState {
        activity_serial: 43,
        ..Default::default()
    };
    let kept = IncrementalCompressionState {
        activity_serial: 42,
        ..Default::default()
    };

    s.page_compression = state;
    s.scroll(Scroll::Top);
    assert_eq!(bumped, s.page_compression);

    // Every scroll restarts traversal, even if clamping leaves the viewport in
    // the same place. Missing an eligible page is worse than a no-op pass.
    s.page_compression = state;
    s.scroll(Scroll::Top);
    assert_eq!(bumped, s.page_compression);

    s.page_compression = state;
    s.scroll(Scroll::Active);
    assert_eq!(bumped, s.page_compression);

    s.page_compression = state;
    s.resize(Resize {
        cols: Some(80),
        rows: Some(24),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(bumped, s.page_compression);

    s.page_compression = state;
    s.reset();
    assert_eq!(kept, s.page_compression);

    s.page_compression = state;
    assert_eq!(
        IncrementalCompressionResult::Complete,
        s.compress(CompressionMode::Full)
    );
    assert_eq!(kept, s.page_compression);

    s.page_compression = state;
    let mut opts = crate::pagelist::access::Clone {
        top: point::Point::Active(point::Coordinate { x: 0, y: 0 }),
        bot: None,
        tracked_pins: None,
    };
    let mut cloned = s.clone(Global, &mut opts).unwrap();
    assert_eq!(
        IncrementalCompressionState::default(),
        cloned.page_compression
    );
    cloned.deinit();

    s.deinit();
}

/// PORTS: oracle@main PageList.zig `test "PageList incremental compression
/// bounds inspected pages"` (9156ada16).
#[test]
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'Parity tail C-1 — the G6 triage'): past a 15-minute Miri bound in the debug-assertions-ON G6 lane, where `Page::assert_integrity` runs on nearly every page mutation (measured per test; see the C-1 table there). It RUNS in the G6c lane (`check_leaks.sh --debug-assertions-off`). Do NOT widen this to a plain `miri` ignore: that would drop the test from BOTH lanes."
)]
fn pagelist_incremental_compression_bounds_inspected_pages() {
    const MAX: usize = crate::pagelist::compress::INCREMENTAL_COMPRESSION_MAX_INSPECTED;

    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();
    grow_cold_pages_for_test(&mut s, MAX + 1);

    // Precompress every candidate so the incremental pass exercises its
    // metadata-only skip budget without stopping at a resident attempt.
    let _ = s.compress(CompressionMode::Full);
    s.page_compression.reset();
    s.page_compression.mark_activity();
    assert_eq!(MAX + 1, s.memory_stats().compressed_pages);

    let mut expected_last = s.pages.first;
    for _ in 1..MAX {
        expected_last = unsafe { (*expected_last).next };
    }

    let first = s.compress(CompressionMode::Incremental);
    assert_eq!(IncrementalCompressionResult::Pending, first);
    assert_eq!(
        unsafe { (*expected_last).serial },
        s.page_compression.last_serial.unwrap()
    );

    let second = s.compress(CompressionMode::Incremental);
    assert_eq!(IncrementalCompressionResult::Pending, second);
    assert!(s.page_compression.flags.verifying);
    assert!(s.page_compression.last_serial.is_none());

    // The verification pass is bounded independently, too.
    assert_eq!(
        IncrementalCompressionResult::Pending,
        s.compress(CompressionMode::Incremental)
    );
    assert_eq!(
        IncrementalCompressionResult::Complete,
        s.compress(CompressionMode::Incremental)
    );

    s.deinit();
}

/// PORTS: oracle@main PageList.zig `test "PageList incremental compression
/// advances after failure"` (9156ada16).
#[test]
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'Parity tail C-1 — the G6 triage'): past a 15-minute Miri bound in the debug-assertions-ON G6 lane, where `Page::assert_integrity` runs on nearly every page mutation (measured per test; see the C-1 table there). It RUNS in the G6c lane (`check_leaks.sh --debug-assertions-off`). Do NOT widen this to a plain `miri` ignore: that would drop the test from BOTH lanes."
)]
fn pagelist_incremental_compression_advances_after_failure() {
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();
    grow_cold_pages_for_test(&mut s, 2);

    let first = s.pages.first;
    let second = unsafe { (*first).next };
    fill_incompressible(first, 0x494E_4352_5041_5353);

    let failed = s.compress(CompressionMode::Incremental);
    assert_eq!(IncrementalCompressionResult::Pending, failed);
    assert!(!unsafe { crate::pagelist::Node::is_compressed(first) });

    // The unsuccessful first page does not stall the pass. The next step
    // continues at the following serial and compresses that page.
    let continued = s.compress(CompressionMode::Incremental);
    assert_eq!(IncrementalCompressionResult::Pending, continued);
    assert!(unsafe { crate::pagelist::Node::is_compressed(second) });

    s.deinit();
}

/// PORTS: oracle@main PageList.zig `test "PageList incremental compression
/// advances after decommit failure"` (9156ada16).
///
/// PORT NOTE (WS7 B10): tripwire injection is compiled out when
/// `debug_assertions` is off, so this test is inert under `--release` and is
/// ignored there rather than left to fail — same convention as
/// `pagelist_compress_page_decommit_failure_leaves_the_page_resident`.
#[test]
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'Parity tail C-1 — the G6 triage'): past a 15-minute Miri bound in the debug-assertions-ON G6 lane, where `Page::assert_integrity` runs on nearly every page mutation (measured per test; see the C-1 table there). It RUNS in the G6c lane (`check_leaks.sh --debug-assertions-off`). Do NOT widen this to a plain `miri` ignore: that would drop the test from BOTH lanes."
)]
fn pagelist_incremental_compression_advances_after_decommit_failure() {
    use rustty_util::tripwire::ResetMode;

    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();
    grow_cold_pages_for_test(&mut s, 2);

    crate::pagelist::compress::COMPRESS_PAGE_TW.with(|tw| {
        tw.borrow_mut().errorAlways(
            crate::pagelist::compress::CompressPageFp::Decommit,
            crate::pagelist::compress::DecommitFailed,
        )
    });
    let failed = s.compress(CompressionMode::Incremental);
    assert_eq!(IncrementalCompressionResult::Pending, failed);
    assert!(!unsafe { crate::pagelist::Node::is_compressed(s.pages.first) });
    crate::pagelist::compress::COMPRESS_PAGE_TW
        .with(|tw| tw.borrow_mut().end(ResetMode::Reset).unwrap());

    // The failed candidate remains resident and the pass continues at the
    // next serial once reclamation is available again.
    let continued = s.compress(CompressionMode::Incremental);
    assert_eq!(IncrementalCompressionResult::Pending, continued);
    assert!(unsafe { crate::pagelist::Node::is_compressed((*s.pages.first).next) });

    s.deinit();
}

/// PORTS: oracle@main PageList.zig `test "PageList incremental compression
/// restarts after replacement"` (9156ada16).
///
/// PORT NOTE: the audit matcher credited this oracle test to
/// `pagelist_compress_incremental_restarts_after_list_mutation` on name
/// similarity alone. That test erases history; it never replaces a node through
/// `increase_capacity`, so the oracle's actual claim — a continuation serial
/// that disappeared with the old node makes the pass restart and consider the
/// oversized replacement — was unasserted. Ported here.
#[test]
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION — BOTH LANES (rustty-vt, parity tail C-1; .port/phase-2/MIRI_EXCEPTIONS.md, 'Parity tail C-1 — the G6 triage'): past a 15-minute Miri bound with debug assertions on (G6) and past an 8-minute bound with them off (G6c), both measured one test per process; runs natively in cargo test --workspace and under ASan (see MIRI_EXCEPTIONS.md)"
)]
fn pagelist_incremental_compression_restarts_after_replacement() {
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();
    grow_cold_pages_for_test(&mut s, 1);

    let initial = s.compress(CompressionMode::Incremental);
    assert_eq!(IncrementalCompressionResult::Pending, initial);
    assert!(unsafe { crate::pagelist::Node::is_compressed(s.pages.first) });

    let old = s.pages.first;
    let old_serial = unsafe { (*old).serial };
    let mut replacement = old;
    while unsafe { (*crate::pagelist::Node::page(replacement)).memory_len() } <= std_size() {
        replacement = s
            .increase_capacity(replacement, Some(IncreaseCapacity::GraphemeBytes))
            .unwrap();
    }
    assert_ne!(unsafe { (*replacement).serial }, old_serial);
    assert!(unsafe { (*crate::pagelist::Node::page(replacement)).memory_len() } > std_size());
    assert!(!unsafe { crate::pagelist::Node::is_compressed(replacement) });

    // The exact continuation serial disappeared with the old node. The pass
    // restarts at the first page and considers the oversized replacement.
    let restarted = s.compress(CompressionMode::Incremental);
    assert_eq!(IncrementalCompressionResult::Pending, restarted);
    assert!(unsafe { crate::pagelist::Node::is_compressed(replacement) });

    s.deinit();
}

/// PORTS: oracle@main PageList.zig `test "PageList incremental compression
/// restarts after reset"` (9156ada16).
///
/// PORT NOTE: like the test above, this oracle test was being credited to
/// `pagelist_compress_incremental_restarts_after_list_mutation` purely by name
/// overlap; nothing in the tree called `reset` between incremental steps.
#[test]
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'Parity tail C-1 — the G6 triage'): past a 15-minute Miri bound in the debug-assertions-ON G6 lane, where `Page::assert_integrity` runs on nearly every page mutation (measured per test; see the C-1 table there). It RUNS in the G6c lane (`check_leaks.sh --debug-assertions-off`). Do NOT widen this to a plain `miri` ignore: that would drop the test from BOTH lanes."
)]
fn pagelist_incremental_compression_restarts_after_reset() {
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();
    grow_cold_pages_for_test(&mut s, 1);

    let initial = s.compress(CompressionMode::Incremental);
    assert_eq!(IncrementalCompressionResult::Pending, initial);
    assert!(unsafe { crate::pagelist::Node::is_compressed(s.pages.first) });

    // Reset replaces every page and clears the PageList-owned traversal.
    s.reset();
    grow_cold_pages_for_test(&mut s, 1);
    let restarted = s.compress(CompressionMode::Incremental);
    assert_eq!(IncrementalCompressionResult::Pending, restarted);
    assert!(unsafe { crate::pagelist::Node::is_compressed(s.pages.first) });

    s.deinit();
}

/// PORTS: oracle@main PageList.zig `test "PageList incremental compression
/// restarts after active boundary resize"` (9156ada16).
#[test]
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION — BOTH LANES (rustty-vt, parity tail C-1; .port/phase-2/MIRI_EXCEPTIONS.md, 'Parity tail C-1 — the G6 triage'): past a 15-minute Miri bound with debug assertions on (G6) and past an 8-minute bound with them off (G6c), both measured one test per process; runs natively in cargo test --workspace and under ASan (see MIRI_EXCEPTIONS.md)"
)]
fn pagelist_incremental_compression_restarts_after_active_boundary_resize() {
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();
    grow_cold_pages_for_test(&mut s, 1);

    let initial = s.compress(CompressionMode::Incremental);
    assert_eq!(IncrementalCompressionResult::Pending, initial);
    assert!(unsafe { crate::pagelist::Node::is_compressed(s.pages.first) });

    let first = s.pages.first;
    let all_rows = s.total_rows as size::CellCountInt;
    s.resize(Resize {
        rows: Some(all_rows),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(first, s.get_top_left(point::Tag::Active).node);

    // Restore the page while it is active. Resize reset the traversal, and
    // active contents remain ineligible.
    let _ = unsafe { crate::pagelist::Node::page(first) };
    let active = s.compress(CompressionMode::Incremental);
    assert_eq!(IncrementalCompressionResult::Pending, active);
    assert_eq!(
        IncrementalCompressionResult::Complete,
        s.compress(CompressionMode::Incremental)
    );

    // Shrinking the active area makes the page fully historical again. The
    // resize reset the PageList-owned cursor, so the next step can reclaim it.
    s.resize(Resize {
        rows: Some(24),
        ..Default::default()
    })
    .unwrap();
    grow_cold_pages_for_test(&mut s, 1);
    assert_eq!(
        IncrementalCompressionResult::Pending,
        s.compress(CompressionMode::Incremental)
    );
    assert!(unsafe { crate::pagelist::Node::is_compressed(first) });

    s.deinit();
}

/// PORTS: oracle@main PageList.zig `test "PageList incremental compression
/// restarts after prune reuse"` (9156ada16).
#[test]
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'Parity tail C-1 — the G6 triage'): past a 15-minute Miri bound in the debug-assertions-ON G6 lane, where `Page::assert_integrity` runs on nearly every page mutation (measured per test; see the C-1 table there). It RUNS in the G6c lane (`check_leaks.sh --debug-assertions-off`). Do NOT widen this to a plain `miri` ignore: that would drop the test from BOTH lanes."
)]
fn pagelist_incremental_compression_restarts_after_prune_reuse() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(2 * std_size()),
            ..Options::new(80, 24)
        },
    )
    .unwrap();
    grow_cold_pages_for_test(&mut s, 1);

    let initial = s.compress(CompressionMode::Incremental);
    assert_eq!(IncrementalCompressionResult::Pending, initial);
    assert!(unsafe { crate::pagelist::Node::is_compressed(s.pages.first) });

    let reused = s.pages.first;
    let old_serial = unsafe { (*reused).serial };
    while unsafe { crate::pagelist::Node::rows(s.pages.last) }
        < unsafe { crate::pagelist::Node::capacity(s.pages.last) }.rows
    {
        let _ = s.grow().unwrap();
    }
    assert_eq!(reused, s.grow().unwrap().unwrap());
    assert_ne!(unsafe { (*reused).serial }, old_serial);

    // Make the remaining old page fully historical. The continuation serial
    // disappeared when its node was recycled, so the pass safely restarts.
    grow_cold_pages_for_test(&mut s, 1);
    let _ = s.compress(CompressionMode::Incremental);
    assert_eq!(1usize, s.memory_stats().compressed_pages);

    s.deinit();
}

/// PORTS: oracle@main PageList.zig `test "PageList incremental compression
/// restarts after earlier replacement"` (9156ada16).
#[test]
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION — BOTH LANES (rustty-vt, parity tail C-1; .port/phase-2/MIRI_EXCEPTIONS.md, 'Parity tail C-1 — the G6 triage'): past a 15-minute Miri bound with debug assertions on (G6) and past an 8-minute bound with them off (G6c), both measured one test per process; runs natively in cargo test --workspace and under ASan (see MIRI_EXCEPTIONS.md)"
)]
fn pagelist_incremental_compression_restarts_after_earlier_replacement() {
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();
    grow_cold_pages_for_test(&mut s, 3);

    let _ = s.compress(CompressionMode::Incremental);
    let _ = s.compress(CompressionMode::Incremental);
    assert!(unsafe { crate::pagelist::Node::is_compressed(s.pages.first) });
    assert!(unsafe { crate::pagelist::Node::is_compressed((*s.pages.first).next) });

    // Replace a page before the still-valid continuation marker. The list's
    // allocation serial changes even though the marker itself remains, so the
    // next step must restart and inspect the replacement.
    let old_first = s.pages.first;
    let old_serial = unsafe { (*old_first).serial };
    let replacement = s
        .increase_capacity(old_first, Some(IncreaseCapacity::GraphemeBytes))
        .unwrap();
    assert_ne!(unsafe { (*replacement).serial }, old_serial);
    assert!(!unsafe { crate::pagelist::Node::is_compressed(replacement) });

    let _ = s.compress(CompressionMode::Incremental);
    assert!(unsafe { crate::pagelist::Node::is_compressed(replacement) });

    s.deinit();
}

/// PORTS: oracle@main PageList.zig `test "PageList incremental compression keeps
/// progress after tail growth"` (9156ada16).
#[test]
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'Parity tail C-1 — the G6 triage'): past a 15-minute Miri bound in the debug-assertions-ON G6 lane, where `Page::assert_integrity` runs on nearly every page mutation (measured per test; see the C-1 table there). It RUNS in the G6c lane (`check_leaks.sh --debug-assertions-off`). Do NOT widen this to a plain `miri` ignore: that would drop the test from BOTH lanes."
)]
fn pagelist_incremental_compression_keeps_progress_after_tail_growth() {
    const MAX: usize = crate::pagelist::compress::INCREMENTAL_COMPRESSION_MAX_INSPECTED;

    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();
    grow_cold_pages_for_test(&mut s, MAX + 1);
    let _ = s.compress(CompressionMode::Full);
    s.page_compression.reset();
    s.page_compression.mark_activity();

    let mut expected_last = s.pages.first;
    for _ in 1..MAX {
        expected_last = unsafe { (*expected_last).next };
    }

    let first = s.compress(CompressionMode::Incremental);
    assert_eq!(IncrementalCompressionResult::Pending, first);
    assert_eq!(
        unsafe { (*expected_last).serial },
        s.page_compression.last_serial.unwrap()
    );

    // Allocate a new page at the active tail between steps. It is after the
    // continuation marker and must not restart progress through cold history.
    let next_serial = s.page_serial;
    while s.page_serial == next_serial {
        let _ = s.grow().unwrap();
    }
    let continued = s.compress(CompressionMode::Incremental);
    assert_eq!(IncrementalCompressionResult::Pending, continued);
    assert!(s.page_compression.flags.verifying);

    s.deinit();
}

/// PORTS: oracle@main PageList.zig `test "PageList lazily restores compressed
/// history made active by resize"` (9156ada16).
#[test]
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'Parity tail C-1 — the G6 triage'): past a 15-minute Miri bound in the debug-assertions-ON G6 lane, where `Page::assert_integrity` runs on nearly every page mutation (measured per test; see the C-1 table there). It RUNS in the G6c lane (`check_leaks.sh --debug-assertions-off`). Do NOT widen this to a plain `miri` ignore: that would drop the test from BOTH lanes."
)]
fn pagelist_lazily_restores_compressed_history_made_active_by_resize() {
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();
    grow_cold_pages_for_test(&mut s, 1);

    let first = s.pages.first;
    let (memory_ptr, memory_len) = unsafe {
        let page = crate::pagelist::Node::page(first);
        *(*page).get_row_and_cell(0, 0).cell = Cell::init('X' as u32);
        ((*page).memory_ptr(), (*page).memory_len())
    };
    let page_size = s.page_size;

    let _ = s.compress(CompressionMode::Full);
    assert!(unsafe { crate::pagelist::Node::is_compressed(first) });

    // Pull all scrollback into the active area by making the viewport as tall
    // as the complete screen. A row-only resize needs only page metadata, so
    // the newly active page can remain compressed until its contents are used.
    let all_rows = s.total_rows as size::CellCountInt;
    s.resize(Resize {
        rows: Some(all_rows),
        ..Default::default()
    })
    .unwrap();
    let active = s.get_top_left(point::Tag::Active);
    assert_eq!(first, active.node);
    assert_eq!(0 as size::CellCountInt, active.y);
    assert!(unsafe { crate::pagelist::Node::is_compressed(first) });
    assert_eq!(page_size, s.page_size);

    // The compression pass must not reconsider the node now that it is active.
    // Content access follows the normal page boundary, which recommits and
    // restores the retained mapping before returning the cell.
    let _ = s.compress(CompressionMode::Full);
    assert!(unsafe { crate::pagelist::Node::is_compressed(first) });
    let cell = s
        .get_cell(point::Point::Active(point::Coordinate { x: 0, y: 0 }))
        .unwrap();
    assert_eq!('X' as u32, unsafe { (*cell.cell).content_codepoint() });
    assert!(!unsafe { crate::pagelist::Node::is_compressed(first) });
    assert_eq!(memory_ptr, unsafe {
        (*crate::pagelist::Node::page(first)).memory_ptr()
    });
    assert_eq!(memory_len, unsafe {
        (*crate::pagelist::Node::page(first)).memory_len()
    });
    assert_eq!(page_size, s.page_size);

    s.deinit();
}

/// PORTS: oracle@main PageList.zig `test "PageList compression restores through
/// page access"` (9156ada16).
#[test]
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION — BOTH LANES (rustty-vt, parity tail C-1; .port/phase-2/MIRI_EXCEPTIONS.md, 'Parity tail C-1 — the G6 triage'): past a 15-minute Miri bound with debug assertions on (G6) and past an 8-minute bound with them off (G6c), both measured one test per process; runs natively in cargo test --workspace and under ASan (see MIRI_EXCEPTIONS.md)"
)]
fn pagelist_compression_restores_through_page_access() {
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();

    let node = s.pages.first;
    let (memory_ptr, memory_len, expected) = unsafe {
        let page = crate::pagelist::Node::page(node);
        (*page).dirty = true;
        *(*page).get_row_and_cell(3, 2).cell = Cell::init('X' as u32);
        let ptr = (*page).memory_ptr();
        let len = (*page).memory_len();
        (ptr, len, core::slice::from_raw_parts(ptr, len).to_vec())
    };
    let page_size = s.page_size;

    assert!(s.compress_page(node));
    assert!(unsafe { crate::pagelist::Node::is_compressed(node) });
    assert_eq!(24 as size::CellCountInt, unsafe {
        crate::pagelist::Node::rows(node)
    });
    assert_eq!(80 as size::CellCountInt, unsafe {
        crate::pagelist::Node::cols(node)
    });
    assert_eq!(memory_ptr, unsafe {
        (*crate::pagelist::Node::metadata(node)).memory_ptr()
    });
    assert_eq!(memory_len, unsafe {
        (*crate::pagelist::Node::metadata(node)).memory_len()
    });
    assert_eq!(page_size, s.page_size);

    // Pin access restores the page without changing its retained mapping.
    let page_pin = Pin {
        node,
        x: 3,
        y: 2,
        garbage: false,
    };
    assert_eq!('X' as u32, unsafe {
        (*page_pin.row_and_cell().cell).content_codepoint()
    });
    assert!(!unsafe { crate::pagelist::Node::is_compressed(node) });
    unsafe {
        let page = crate::pagelist::Node::page(node);
        assert_eq!(memory_ptr, (*page).memory_ptr());
        assert_eq!(
            core::slice::from_raw_parts((*page).memory_ptr(), memory_len),
            &expected[..]
        );
        assert!((*page).dirty);
    }

    // Recompressing exercises reuse of the page-pool scratch item. Page
    // iterator chunks also restore before exposing row memory.
    assert!(s.compress_page(node));
    let mut page_it = Pin {
        node,
        x: 0,
        y: 0,
        garbage: false,
    }
    .page_iterator(crate::pagelist::iter::Direction::RightDown, None);
    let chunk = page_it.next().unwrap();
    assert_eq!(
        unsafe { crate::pagelist::Node::rows(node) } as usize,
        chunk.rows().len()
    );
    assert!(!unsafe { crate::pagelist::Node::is_compressed(node) });
    unsafe {
        let page = crate::pagelist::Node::page(node);
        assert_eq!(
            core::slice::from_raw_parts((*page).memory_ptr(), memory_len),
            &expected[..]
        );
    }

    // Read-only PageList operations restore through the same boundary.
    assert!(s.compress_page(node));
    let mut opts = crate::pagelist::access::Clone {
        top: point::Point::Screen(point::Coordinate { x: 0, y: 0 }),
        bot: None,
        tracked_pins: None,
    };
    let mut cloned = s.clone(Global, &mut opts).unwrap();
    assert!(!unsafe { crate::pagelist::Node::is_compressed(node) });
    assert_eq!('X' as u32, unsafe {
        (*(*crate::pagelist::Node::page(cloned.pages.first))
            .get_row_and_cell(3, 2)
            .cell)
            .content_codepoint()
    });
    cloned.deinit();

    s.deinit();
}

/// PORTS: oracle@main PageList.zig `test "PageList reset discards malformed
/// compressed data"` (9156ada16).
#[test]
fn pagelist_reset_discards_malformed_compressed_data() {
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();

    let node = s.pages.first;
    assert!(s.compress_page(node));
    compressed_encoded_fill_for_test(node, 0xFF);

    s.reset();
    assert!(!unsafe { crate::pagelist::Node::is_compressed(s.pages.first) });
    assert_eq!(1usize, s.total_pages());

    s.deinit();
}

/// PORTS: oracle@main PageList.zig `test "PageList deinit discards malformed
/// compressed data"` (9156ada16).
#[test]
fn pagelist_deinit_discards_malformed_compressed_data() {
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();
    let node = s.pages.first;
    assert!(s.compress_page(node));
    compressed_encoded_fill_for_test(node, 0xFF);

    s.deinit();
}

/// PORTS: oracle@main PageList.zig `test "PageList prune reuses malformed
/// compressed page memory"` (9156ada16).
#[test]
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'Parity tail C-1 — the G6 triage'): past a 15-minute Miri bound in the debug-assertions-ON G6 lane, where `Page::assert_integrity` runs on nearly every page mutation (measured per test; see the C-1 table there). It RUNS in the G6c lane (`check_leaks.sh --debug-assertions-off`). Do NOT widen this to a plain `miri` ignore: that would drop the test from BOTH lanes."
)]
fn pagelist_prune_reuses_malformed_compressed_page_memory() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(2 * std_size()),
            ..Options::new(80, 24)
        },
    )
    .unwrap();

    // Allocate the second page so the first one can be pruned and reused.
    while core::ptr::eq(s.pages.first, s.pages.last) {
        let _ = s.grow().unwrap();
    }
    let first = s.pages.first;
    assert!(s.compress_page(first));
    compressed_encoded_fill_for_test(first, 0xFF);

    let mut reused = false;
    let growth_limit = unsafe { crate::pagelist::Node::capacity(s.pages.last) }.rows as usize + 1;
    for _ in 0..growth_limit {
        if let Some(new_node) = s.grow().unwrap() {
            if core::ptr::eq(new_node, first) {
                reused = true;
                break;
            }
        }
    }

    assert!(reused);
    assert_eq!(first, s.pages.last);
    assert!(!unsafe { crate::pagelist::Node::is_compressed(first) });
    assert_eq!(1 as size::CellCountInt, unsafe {
        crate::pagelist::Node::rows(first)
    });
    unsafe { (*crate::pagelist::Node::page(first)).assert_integrity() };

    s.deinit();
}

/// PORTS: oracle@main PageList.zig `Node.Owned` (16e4b5e98) — every node
/// records how its backing memory was allocated. A heap-owned page smaller than
/// the standard pool item must still be freed to the heap, never returned to
/// the pool.
#[test]
fn pagelist_node_ownership_is_explicit() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(1000),
            ..Options::new(80, 24)
        },
    )
    .unwrap();
    // Every page allocated for a standard-capacity list is pool owned.
    let mut it = s.pages.first;
    while !it.is_null() {
        assert_eq!(unsafe { (*it).owned }, Owned::Pool);
        it = unsafe { (*it).next };
    }
    s.deinit();
}

/// An OVERSIZED page (grown past a pool item by capacity increases) is heap
/// owned, and so is an UNDERSIZED compacted page. Neither may be inferred from
/// the memory length.
#[test]
fn pagelist_oversized_page_is_heap_owned() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(80, 24)
        },
    )
    .unwrap();

    // Increase capacity until the layout no longer fits a pool item.
    let mut node = s.pages.first;
    while unsafe { (*crate::pagelist::Node::page(node)).memory_len() } <= std_size() {
        node = s
            .increase_capacity(node, Some(IncreaseCapacity::GraphemeBytes))
            .unwrap();
    }
    assert_eq!(unsafe { (*node).owned }, Owned::Heap);

    // A compacted page is SMALLER than a pool item but still heap owned: the
    // ownership tag is the only correct source of truth.
    let compacted = s.compact(node).unwrap().unwrap();
    assert!(unsafe { (*crate::pagelist::Node::page(compacted)).memory_len() } < std_size());
    assert_eq!(unsafe { (*compacted).owned }, Owned::Heap);

    s.deinit();
}

// =====================================================================
// Incremental compression traversal (P3 G3b-vt lane)
//
// PORTS: the `9156ada16` / `70e788f06` / `181254d36` / `d0a26d146` /
// `f8c217e55` incremental-compression test block of oracle
// src/terminal/PageList.zig. The implementation was ported by WS7; these
// blocks are the traversal-state and memory-accounting assertions that were
// not carried over with it.
// =====================================================================

/// PORTS: oracle@main PageList.zig
/// `test "PageList incremental compression advances after allocation failure"`.
///
/// Ported as written (parity tail S7-3): the PageList runs over a
/// `FailingAllocator` and the allocator's very next request, `compressPage`'s
/// exact-sized encoded allocation, is failed. (This replaced a tripwire
/// stand-in used while the encoding lived on the Rust heap.)
#[test]
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'Parity tail C-1 — the G6 triage'): past a 15-minute Miri bound in the debug-assertions-ON G6 lane, where `Page::assert_integrity` runs on nearly every page mutation (measured per test; see the C-1 table there). It RUNS in the G6c lane (`check_leaks.sh --debug-assertions-off`). Do NOT widen this to a plain `miri` ignore: that would drop the test from BOTH lanes."
)]
fn pagelist_incremental_compression_advances_after_allocation_failure() {
    // var failing = testing.FailingAllocator.init(testing.allocator, .{});
    // const alloc = failing.allocator();
    let failing = FailingAllocator::over_heap(FailingConfig::default());
    let c = failing.allocator();
    with_handle(&c, |alloc| {
        let mut s = PageList::init(alloc, Options::new(80, 24)).unwrap();
        // defer s.deinit();
        grow_cold_pages_for_test(&mut s, 2);
        let first = s.pages.first;
        let second = unsafe { (*first).next };

        // Pool preheating supplies compression scratch. Failing the allocator's
        // next request therefore rejects the exact encoded allocation while the
        // source page and pass remain valid.
        failing.fail_index.set(failing.alloc_index.get());
        let failed = s.compress(CompressionMode::Incremental);
        assert!(failing.has_induced_failure.get());
        assert_eq!(IncrementalCompressionResult::Pending, failed);
        assert!(!unsafe { crate::pagelist::Node::is_compressed(first) });

        // Allow allocations again. The pass must continue with the following page
        // rather than retrying the failed candidate.
        failing.fail_index.set(usize::MAX);
        let continued = s.compress(CompressionMode::Incremental);
        assert_eq!(IncrementalCompressionResult::Pending, continued);
        assert!(unsafe { crate::pagelist::Node::is_compressed(second) });

        s.deinit();
    });
}

/// PORTS: oracle@main PageList.zig
/// `test "PageList memory stats do not restore compressed pages"`.
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION (.port/phase-3 G3b-vt lane): LZ4 page-compression volume. INDIVIDUALLY MEASURED 2026-07-31 (adversarial re-audit): TIMEOUT at a 600s hard bound under `cargo +nightly miri test -p rustty-vt --lib -- --include-ignored --exact <name> --test-threads=1`, debug-assertions ON. Two siblings that carried this same reason string were measured at 2.05s/1.57s and had their ignores REMOVED -- do not re-apply this string by class inference. Covered natively + by ASan."
)]
#[test]
fn pagelist_memory_stats_do_not_restore_compressed_pages() {
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();
    grow_cold_pages_for_test(&mut s, 2);

    let before = s.memory_stats();
    assert_eq!(s.total_pages(), before.resident_pages);
    assert_eq!(0, before.compressed_pages);
    assert_eq!(s.page_size, before.raw_bytes);
    assert_eq!(before.raw_bytes, before.resident_raw_bytes);
    assert_eq!(0, before.decommitted_raw_bytes);
    assert_eq!(s.page_size, before.resident_backing_bytes);
    assert_eq!(0, before.encoded_bytes);
    assert_eq!(
        before.resident_backing_bytes,
        before.estimated_resident_bytes()
    );
    assert_eq!(0, before.estimated_savings());

    let _ = s.compress(CompressionMode::Full);
    let first = s.pages.first;
    assert_eq!(Storage::Compressed, unsafe {
        crate::pagelist::Node::storage(first)
    });
    assert!(unsafe { crate::pagelist::Node::page_if_resident(first) }.is_null());

    let after = s.memory_stats();
    assert!(unsafe { crate::pagelist::Node::is_compressed(first) });
    assert_eq!(
        s.total_pages(),
        after.resident_pages + after.compressed_pages
    );
    assert_eq!(2, after.compressed_pages);
    assert_eq!(s.page_size, after.raw_bytes);
    assert_eq!(
        after.raw_bytes,
        after.resident_raw_bytes + after.decommitted_raw_bytes
    );
    assert_eq!(
        after.resident_backing_bytes + after.encoded_bytes,
        after.estimated_resident_bytes()
    );
    assert_eq!(
        after.decommitted_raw_bytes - after.encoded_bytes,
        after.estimated_savings()
    );

    let first_raw_len = unsafe { (*crate::pagelist::Node::metadata(first)).memory_len() };
    let first_encoded_len = compressed_encoded_for_test(first).len();
    let _ = unsafe { crate::pagelist::Node::page(first) };
    assert_eq!(Storage::Resident, unsafe {
        crate::pagelist::Node::storage(first)
    });
    assert!(!unsafe { crate::pagelist::Node::page_if_resident(first) }.is_null());

    let restored = s.memory_stats();
    assert_eq!(after.resident_pages + 1, restored.resident_pages);
    assert_eq!(after.compressed_pages - 1, restored.compressed_pages);
    assert_eq!(after.raw_bytes, restored.raw_bytes);
    assert_eq!(
        after.resident_raw_bytes + first_raw_len,
        restored.resident_raw_bytes
    );
    assert_eq!(
        after.decommitted_raw_bytes - first_raw_len,
        restored.decommitted_raw_bytes
    );
    assert_eq!(
        after.resident_backing_bytes + first_raw_len,
        restored.resident_backing_bytes
    );
    assert_eq!(
        after.encoded_bytes - first_encoded_len,
        restored.encoded_bytes
    );

    s.deinit();
}

/// PORTS: oracle@main PageList.zig
/// `test "PageList memory stats include unused pool backing"`.
// NO MIRI EXCEPTION. This test carried the G3b-vt "LZ4 page-compression volume /
// NOT individually re-measured" annotation until the 2026-07-31 adversarial
// re-audit measured it: it compresses ONE 1x1 pool page, and Miri runs it in
// 2.05s. The class inference was wrong for this test.
#[test]
fn pagelist_memory_stats_include_unused_pool_backing() {
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();

    // Pool allocation ownership is based on the requested layout fitting in a
    // standard item. The Page itself exposes only the initialized prefix.
    let node = s
        .create_page(crate::pagelist::CreatePage::new(Capacity::new(1, 1)))
        .unwrap();
    assert_eq!(Owned::Pool, unsafe { (*node).owned });
    assert!(unsafe { (*crate::pagelist::Node::page(node)).memory_len() } < std_size());
    unsafe { (*crate::pagelist::Node::page(node)).size.rows = 1 };
    unsafe { s.pages.append(node) };
    s.total_rows += 1;

    let raw_len = unsafe { (*crate::pagelist::Node::metadata(node)).memory_len() };
    let before = s.memory_stats();
    assert!(before.raw_bytes < s.page_size);
    assert_eq!(before.raw_bytes, before.resident_raw_bytes);
    assert_eq!(s.page_size, before.resident_backing_bytes);
    assert_eq!(s.page_size, before.estimated_resident_bytes());

    assert!(s.compress_page(node));
    let encoded_len = compressed_encoded_for_test(node).len();
    let compressed = s.memory_stats();
    assert_eq!(before.raw_bytes, compressed.raw_bytes);
    assert_eq!(
        before.resident_raw_bytes - raw_len,
        compressed.resident_raw_bytes
    );
    assert_eq!(raw_len, compressed.decommitted_raw_bytes);
    assert_eq!(
        before.resident_backing_bytes - raw_len,
        compressed.resident_backing_bytes
    );
    assert_eq!(encoded_len, compressed.encoded_bytes);
    assert_eq!(
        before.estimated_resident_bytes() - raw_len + encoded_len,
        compressed.estimated_resident_bytes()
    );

    s.deinit();
}

/// PORTS: oracle@main PageList.zig
/// `test "PageList does not compress the mixed history and active page"`.
// NO MIRI EXCEPTION. Same finding as the sibling above: measured 1.57s under
// Miri by the 2026-07-31 adversarial re-audit. It asserts that a mixed
// history/active page is NOT compressed, so the LZ4 path it was excepted for is
// the one path it never takes.
#[test]
fn pagelist_does_not_compress_the_mixed_history_and_active_page() {
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();

    // One additional row creates history, but the history and all active rows
    // still share the first page. The active boundary therefore has a
    // historical prefix and must remain resident as one indivisible mapping.
    let _ = s.grow().unwrap();
    let active = s.get_top_left(point::Tag::Active);
    assert_eq!(s.pages.first, active.node);
    assert!(active.y > 0);

    let _ = s.compress(CompressionMode::Full);
    assert!(!unsafe { crate::pagelist::Node::is_compressed(s.pages.first) });

    s.deinit();
}

/// PORTS: oracle@main PageList.zig
/// `test "PageList full and incremental compression skip a spanning viewport"`.
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION (.port/phase-3 G3b-vt lane): LZ4 page-compression volume. INDIVIDUALLY MEASURED 2026-07-31 (adversarial re-audit): TIMEOUT at a 600s hard bound under `cargo +nightly miri test -p rustty-vt --lib -- --include-ignored --exact <name> --test-threads=1`, debug-assertions ON. Two siblings that carried this same reason string were measured at 2.05s/1.57s and had their ignores REMOVED -- do not re-apply this string by class inference. Covered natively + by ASan."
)]
#[test]
fn pagelist_full_and_incremental_compression_skip_a_spanning_viewport() {
    let mut full = PageList::init(Global, Options::new(80, 24)).unwrap();
    grow_cold_pages_for_test(&mut full, 3);

    let mut incremental = PageList::init(Global, Options::new(80, 24)).unwrap();
    grow_cold_pages_for_test(&mut incremental, 3);

    // Start near the end of the first page so the viewport intersects both
    // the first and second historical page mappings.
    let first = full.pages.first;
    let overlap_rows = full.rows as usize / 2;
    let viewport_row = unsafe { crate::pagelist::Node::rows(first) } as usize - overlap_rows;
    full.scroll(Scroll::Row(viewport_row));
    incremental.scroll(Scroll::Row(viewport_row));
    assert_ne!(
        full.get_top_left(point::Tag::Viewport).node,
        full.get_bottom_right(point::Tag::Viewport).unwrap().node
    );
    let second = unsafe { (*first).next };
    assert_eq!(first, full.get_top_left(point::Tag::Viewport).node);
    assert_eq!(
        second,
        full.get_bottom_right(point::Tag::Viewport).unwrap().node
    );

    let _ = full.compress(CompressionMode::Full);
    let _ = incremental.compress(CompressionMode::Drain);
    assert_eq!(full.memory_stats(), incremental.memory_stats());
    assert!(!unsafe { crate::pagelist::Node::is_compressed(first) });
    assert!(!unsafe { crate::pagelist::Node::is_compressed(second) });

    let full_active = full.get_top_left(point::Tag::Active).node;
    let incremental_active = incremental.get_top_left(point::Tag::Active).node;
    let mut full_node = full.pages.first;
    let mut incremental_node = incremental.pages.first;
    while !core::ptr::eq(full_node, full_active) {
        assert_eq!(
            unsafe { crate::pagelist::Node::is_compressed(full_node) },
            unsafe { crate::pagelist::Node::is_compressed(incremental_node) }
        );

        full_node = unsafe { (*full_node).next };
        incremental_node = unsafe { (*incremental_node).next };
    }
    assert_eq!(incremental_active, incremental_node);

    let mut eligible = CompressionIterator::init(&full);
    let mut compressed_pages: usize = 0;
    while let Some(node) = eligible.next() {
        compressed_pages += 1;
        assert!(unsafe { crate::pagelist::Node::is_compressed(node) });
    }
    assert!(compressed_pages > 0);

    incremental.deinit();
    full.deinit();
}

#[test]
fn pagelist_decommit_pool_item_zeroes_the_dirty_region() {
    // PORT NOTE (WS7 B1): `decommit_pool_item` now delegates to
    // `crate::mem::decommit`, whose (oracle-faithful) contract asserts that the
    // range is page-aligned and a page-size multiple — exactly what a real pool
    // item is. The buffer is therefore an aligned allocation rather than a
    // plain `Vec`, mirroring the oracle's own `alignedAlloc(page_size_min)` in
    // `mem.zig`'s tests.
    let len = 2 * crate::page::PAGE_SIZE_MIN;
    let layout = std::alloc::Layout::from_size_align(len, crate::page::PAGE_SIZE_MIN).unwrap();
    // SAFETY: len > 0 and the layout is valid.
    let ptr = unsafe { std::alloc::alloc_zeroed(layout) };
    assert!(!ptr.is_null());
    // PORT NOTE (WS7 Phase E): every slice over the buffer is derived FRESH
    // from `ptr` and dropped before `decommit_pool_item` runs. Holding one
    // `&mut [u8]` across the call and reading it afterwards is Stacked-Borrows
    // UB: `decommit_pool_item` mutates the same bytes through its own `*mut u8`,
    // which pops the outer reference's tag off the borrow stack, so the
    // post-call read through the stale slice is a use of an invalidated tag
    // (deterministic under Miri). This is a defect in the *test*, not in
    // `decommit_pool_item`/`mem::decommit`, which only ever hold the raw
    // pointer. Same convention as the raw-pointer arguments used throughout the
    // page/row code.

    // SAFETY (each block): `ptr` is a live allocation of `len` zeroed bytes and
    // no other reference to it exists for the duration of the block.

    // Fully dirty item.
    unsafe { core::slice::from_raw_parts_mut(ptr, len) }
        .iter_mut()
        .for_each(|b| *b = 0xAA);
    crate::pagelist::decommit_pool_item(ptr, len, len);
    assert!(unsafe { core::slice::from_raw_parts(ptr, len) }
        .iter()
        .all(|&b| b == 0));

    // Partially dirty item: an item that hosted a page smaller than the item is
    // only dirty up to the page length; the rest is zero by invariant and must
    // remain zero.
    unsafe { core::slice::from_raw_parts_mut(ptr, 1024) }
        .iter_mut()
        .for_each(|b| *b = 0xAA);
    crate::pagelist::decommit_pool_item(ptr, len, 1024);
    assert!(unsafe { core::slice::from_raw_parts(ptr, len) }
        .iter()
        .all(|&b| b == 0));

    // SAFETY: `ptr`/`layout` are the pair returned by `alloc_zeroed` above and
    // no slice over the buffer is live here.
    unsafe { std::alloc::dealloc(ptr, layout) };
}

// ref: test "PageList destroyed pool page reuse is zeroed"
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
)]
#[test]
fn pagelist_destroyed_pool_page_reuse_is_zeroed() {
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();

    // Create a page and scribble over its entire backing memory, then destroy
    // it so the buffer returns to the pool free list.
    let node = s
        .create_page(crate::pagelist::CreatePage::new(
            PageList::initial_capacity(80),
        ))
        .unwrap();
    unsafe { (*crate::pagelist::Node::page(node)).size.rows = 1 };
    let mem_ptr = unsafe { (*crate::pagelist::Node::page(node)).memory_ptr };
    let mem_len = unsafe { (*crate::pagelist::Node::page(node)).memory_len };
    unsafe { core::ptr::write_bytes(mem_ptr, 0xAA, mem_len) };
    s.destroy_node(node);

    // Reusing the buffer must produce a fully valid, zeroed page.
    let node2 = s
        .create_page(crate::pagelist::CreatePage::new(
            PageList::initial_capacity(80),
        ))
        .unwrap();
    assert_eq!(mem_ptr, unsafe {
        (*crate::pagelist::Node::page(node2)).memory_ptr
    });
    unsafe {
        (*crate::pagelist::Node::page(node2)).size.rows =
            crate::pagelist::Node::capacity(node2).rows
    };

    let cells_len = unsafe {
        crate::pagelist::Node::capacity(node2).cols as usize
            * crate::pagelist::Node::capacity(node2).rows as usize
    };
    {
        let page = unsafe { &(*crate::pagelist::Node::page(node2)) };
        for i in 0..cells_len {
            let rac =
                page.get_row_and_cell(i % page.size.cols as usize, i / page.size.cols as usize);
            let bytes = unsafe {
                core::slice::from_raw_parts(rac.cell as *const u8, core::mem::size_of::<Cell>())
            };
            assert!(bytes.iter().all(|&b| b == 0));
        }
    }
    unsafe { (*crate::pagelist::Node::page(node2)).assert_integrity() };
    s.destroy_node(node2);

    s.deinit();
}

// ref: test "PageList compact after increaseCapacity"
#[test]
fn pagelist_compact_after_increase_capacity() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(80, 24)
        },
    )
    .unwrap();

    let mut node = s.pages.first;

    // Grow the page capacity. The content is unchanged, so compaction should
    // always shrink it back down to an exact-size heap page.
    node = s
        .increase_capacity(node, Some(IncreaseCapacity::GraphemeBytes))
        .unwrap();
    let grown_len = unsafe { (*crate::pagelist::Node::page(node)).memory_len };

    let new_node = s.compact(node).unwrap().unwrap();
    assert_eq!(unsafe { (*new_node).owned }, Owned::Heap);
    assert!(unsafe { (*crate::pagelist::Node::page(new_node)).memory_len } < grown_len);
    assert!(unsafe { (*crate::pagelist::Node::page(new_node)).memory_len } < std_size());

    s.deinit();
}
