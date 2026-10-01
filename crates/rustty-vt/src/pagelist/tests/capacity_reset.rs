//! PORTS: ghostty src/terminal/PageList.zig — the `increaseCapacity` and
//! `reset` tests.

use super::{cell_codepoint, fill_compressible_history, pin_eql};
use crate::alloc::Global;
use crate::page::Cell;
use crate::pagelist::grow::{IncreaseCapacity, IncreaseCapacityError};
use crate::pagelist::{std_capacity, std_size, Options, PageList, Pin};
use crate::point;
use crate::style::{self as stylepkg, Style};
use rustty_render_types::term_size as size;

// =====================================================================
// increaseCapacity
// =====================================================================

// ref: test "PageList increaseCapacity to increase styles"
#[test]
fn pagelist_increaseCapacity_to_increase_styles() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(2, 2)
        },
    )
    .unwrap();

    let original_styles_cap = unsafe { crate::pagelist::Node::capacity(s.pages.first).styles };

    {
        assert!(core::ptr::eq(s.pages.first, s.pages.last));
        let page = unsafe { crate::pagelist::Node::page(s.pages.first) };

        // Write all our data so we can assert its the same after
        for y in 0..s.rows as usize {
            for x in 0..s.cols as usize {
                let rac = unsafe { (*page).get_row_and_cell(x, y) };
                unsafe {
                    *rac.cell = cell_codepoint(x as u32);
                }
            }
        }
    }

    // Increase our styles
    let _ = s
        .increase_capacity(s.pages.first, Some(IncreaseCapacity::Styles))
        .unwrap();

    {
        assert!(core::ptr::eq(s.pages.first, s.pages.last));
        let page = unsafe { &(*crate::pagelist::Node::page(s.pages.first)) };

        // Verify capacity doubled
        assert_eq!(original_styles_cap * 2, page.capacity.styles);

        // Verify data preserved
        for y in 0..s.rows as usize {
            for x in 0..s.cols as usize {
                let rac = page.get_row_and_cell(x, y);
                assert_eq!(x as u32, unsafe { (*rac.cell).content_codepoint() });
            }
        }
    }

    s.deinit();
}

// S124-F1: `increaseCapacity` unconditionally logs `log.info("adjusting page
// capacity={}", .{cap})` (PageList.zig:4317) before creating the larger
// replacement page — reached by any ordinary capacity-increase call, no
// corruption needed.
#[test]
fn pagelist_increase_capacity_logs_the_adjusted_capacity() {
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();

    let cap = crate::log_capture::start();
    let _ = s
        .increase_capacity(s.pages.first, Some(IncreaseCapacity::Styles))
        .unwrap();
    let messages = cap.messages();
    assert!(
        messages
            .iter()
            .any(|(level, target, msg)| *level == log::Level::Info
                && target == "page_list"
                && msg.starts_with("adjusting page capacity=")),
        "expected the adjusting-page-capacity diagnostic, got: {messages:?}"
    );
    drop(cap);

    s.deinit();
}

// ref: test "PageList increaseCapacity styles projects capacity from page density"
#[test]
fn pagelist_increaseCapacity_styles_projects_capacity_from_page_density() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(2, 2)
        },
    )
    .unwrap();

    let original_cap = unsafe { crate::pagelist::Node::capacity(s.pages.first).styles };

    // Write styled cells so the page has a measurable per-row style
    // density (unlike the plain doubling test above, which grows a
    // page with no styles in use).
    let bold = Style {
        flags: stylepkg::Flags {
            bold: true,
            ..stylepkg::Flags::default()
        },
        ..Style::default()
    };
    {
        assert!(core::ptr::eq(s.pages.first, s.pages.last));
        let page = unsafe { crate::pagelist::Node::page(s.pages.first) };
        let base = unsafe { (*page).memory_ptr() };

        for y in 0..s.rows as usize {
            for x in 0..s.cols as usize {
                let rac = unsafe { (*page).get_row_and_cell(x, y) };
                // const style_id = try page.styles.add(page.memory, bold);
                let style_id = unsafe { (*page).styles.add(base, bold) }.unwrap();
                unsafe {
                    (*rac.row).set_styled(true);
                    // rac.cell.* = .{ .content_tag = .codepoint, .content = ...,
                    //                 .style_id = style_id };
                    let mut c = cell_codepoint((x + 1) as u32);
                    c.set_style_id(style_id);
                    *rac.cell = c;
                }
            }
        }
    }

    let _ = s
        .increase_capacity(s.pages.first, Some(IncreaseCapacity::Styles))
        .unwrap();

    {
        assert!(core::ptr::eq(s.pages.first, s.pages.last));
        let page = unsafe { &(*crate::pagelist::Node::page(s.pages.first)) };

        // The page uses only two active rows out of thousands of rows
        // of capacity, so the projected full-page need saturates the
        // 32x-per-event growth bound instead of merely doubling.
        assert_eq!(original_cap * 32, page.capacity.styles);

        // All cell content and styles are preserved by the growth.
        let base = page.memory_ptr();
        for y in 0..s.rows as usize {
            for x in 0..s.cols as usize {
                let rac = page.get_row_and_cell(x, y);
                assert_eq!((x + 1) as u32, unsafe { (*rac.cell).content_codepoint() });
                assert!(unsafe { (*rac.cell).style_id() } != stylepkg::DEFAULT_ID);
                assert!(bold.eql(unsafe { &*page.styles.get(base, (*rac.cell).style_id()) }));
            }
        }
    }

    s.deinit();
}

// ref: test "PageList increaseCapacity to increase graphemes"
#[test]
fn pagelist_increaseCapacity_to_increase_graphemes() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(2, 2)
        },
    )
    .unwrap();

    let original_cap = unsafe { crate::pagelist::Node::capacity(s.pages.first).grapheme_bytes };

    {
        assert!(core::ptr::eq(s.pages.first, s.pages.last));
        let page = unsafe { crate::pagelist::Node::page(s.pages.first) };

        for y in 0..s.rows as usize {
            for x in 0..s.cols as usize {
                let rac = unsafe { (*page).get_row_and_cell(x, y) };
                unsafe {
                    *rac.cell = cell_codepoint(x as u32);
                }
            }
        }
    }

    let _ = s
        .increase_capacity(s.pages.first, Some(IncreaseCapacity::GraphemeBytes))
        .unwrap();

    {
        assert!(core::ptr::eq(s.pages.first, s.pages.last));
        let page = unsafe { &(*crate::pagelist::Node::page(s.pages.first)) };

        assert_eq!(original_cap * 2, page.capacity.grapheme_bytes);

        for y in 0..s.rows as usize {
            for x in 0..s.cols as usize {
                let rac = page.get_row_and_cell(x, y);
                assert_eq!(x as u32, unsafe { (*rac.cell).content_codepoint() });
            }
        }
    }

    s.deinit();
}

// ref: test "PageList increaseCapacity graphemes projects capacity from page density"
#[test]
fn pagelist_increaseCapacity_graphemes_projects_capacity_from_page_density() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(2, 2)
        },
    )
    .unwrap();

    let original_cap = unsafe { crate::pagelist::Node::capacity(s.pages.first).grapheme_bytes };

    // Write cells with grapheme data so the page has a measurable
    // per-row grapheme density (unlike the plain doubling test above,
    // which grows a page with no grapheme usage).
    {
        assert!(core::ptr::eq(s.pages.first, s.pages.last));
        let page = unsafe { crate::pagelist::Node::page(s.pages.first) };

        for y in 0..s.rows as usize {
            for x in 0..s.cols as usize {
                let rac = unsafe { (*page).get_row_and_cell(x, y) };
                unsafe {
                    *rac.cell = cell_codepoint((x + 1) as u32);
                }
                unsafe { (*page).append_grapheme(rac.row, rac.cell, 0x0301) }.unwrap();
                unsafe { (*page).append_grapheme(rac.row, rac.cell, 0x0302) }.unwrap();
            }
        }
    }

    let _ = s
        .increase_capacity(s.pages.first, Some(IncreaseCapacity::GraphemeBytes))
        .unwrap();

    {
        assert!(core::ptr::eq(s.pages.first, s.pages.last));
        let page = unsafe { &(*crate::pagelist::Node::page(s.pages.first)) };

        // The page uses only two active rows out of thousands of rows
        // of capacity, so the projected full-page need saturates the
        // 32x-per-event growth bound instead of merely doubling.
        assert_eq!(original_cap * 32, page.capacity.grapheme_bytes);

        // All cell and grapheme content is preserved by the growth.
        assert_eq!((s.rows as usize) * (s.cols as usize), page.grapheme_count(),);
        for y in 0..s.rows as usize {
            for x in 0..s.cols as usize {
                let rac = page.get_row_and_cell(x, y);
                assert_eq!((x + 1) as u32, unsafe { (*rac.cell).content_codepoint() });
                let cps = page.lookup_grapheme(rac.cell).unwrap();
                assert_eq!(2usize, cps.len());
                assert_eq!(0x0301u32, cps[0]);
                assert_eq!(0x0302u32, cps[1]);
            }
        }
    }

    s.deinit();
}

// ref: test "PageList increaseCapacity to increase hyperlinks"
#[test]
fn pagelist_increaseCapacity_to_increase_hyperlinks() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(2, 2)
        },
    )
    .unwrap();

    let original_cap = unsafe { crate::pagelist::Node::capacity(s.pages.first).hyperlink_bytes };

    {
        assert!(core::ptr::eq(s.pages.first, s.pages.last));
        let page = unsafe { crate::pagelist::Node::page(s.pages.first) };

        for y in 0..s.rows as usize {
            for x in 0..s.cols as usize {
                let rac = unsafe { (*page).get_row_and_cell(x, y) };
                unsafe {
                    *rac.cell = cell_codepoint(x as u32);
                }
            }
        }
    }

    let _ = s
        .increase_capacity(s.pages.first, Some(IncreaseCapacity::HyperlinkBytes))
        .unwrap();

    {
        assert!(core::ptr::eq(s.pages.first, s.pages.last));
        let page = unsafe { &(*crate::pagelist::Node::page(s.pages.first)) };

        assert_eq!(original_cap * 2, page.capacity.hyperlink_bytes);

        for y in 0..s.rows as usize {
            for x in 0..s.cols as usize {
                let rac = page.get_row_and_cell(x, y);
                assert_eq!(x as u32, unsafe { (*rac.cell).content_codepoint() });
            }
        }
    }

    s.deinit();
}

// ref: test "PageList increaseCapacity to increase string_bytes"
#[test]
fn pagelist_increaseCapacity_to_increase_string_bytes() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(2, 2)
        },
    )
    .unwrap();

    let original_cap = unsafe { crate::pagelist::Node::capacity(s.pages.first).string_bytes };

    {
        assert!(core::ptr::eq(s.pages.first, s.pages.last));
        let page = unsafe { crate::pagelist::Node::page(s.pages.first) };

        for y in 0..s.rows as usize {
            for x in 0..s.cols as usize {
                let rac = unsafe { (*page).get_row_and_cell(x, y) };
                unsafe {
                    *rac.cell = cell_codepoint(x as u32);
                }
            }
        }
    }

    let _ = s
        .increase_capacity(s.pages.first, Some(IncreaseCapacity::StringBytes))
        .unwrap();

    {
        assert!(core::ptr::eq(s.pages.first, s.pages.last));
        let page = unsafe { &(*crate::pagelist::Node::page(s.pages.first)) };

        assert_eq!(original_cap * 2, page.capacity.string_bytes);

        for y in 0..s.rows as usize {
            for x in 0..s.cols as usize {
                let rac = page.get_row_and_cell(x, y);
                assert_eq!(x as u32, unsafe { (*rac.cell).content_codepoint() });
            }
        }
    }

    s.deinit();
}

// ref: test "PageList increaseCapacity tracked pins"
#[test]
fn pagelist_increaseCapacity_tracked_pins() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(2, 2)
        },
    )
    .unwrap();

    // Create a tracked pin on the first page
    let tracked = s
        .track_pin(
            s.pin(point::Point::Active(point::Coordinate { x: 1, y: 1 }))
                .unwrap(),
        )
        .unwrap();

    let old_node = s.pages.first;
    assert!(core::ptr::eq(old_node, unsafe { (*tracked).node }));

    // Increase capacity
    let new_node = s
        .increase_capacity(s.pages.first, Some(IncreaseCapacity::Styles))
        .unwrap();

    // Pin should now point to the new node
    assert!(core::ptr::eq(new_node, unsafe { (*tracked).node }));
    assert_eq!(1 as size::CellCountInt, unsafe { (*tracked).x });
    assert_eq!(1 as size::CellCountInt, unsafe { (*tracked).y });

    s.untrack_pin(tracked);
    s.deinit();
}

// ref: test "PageList increaseCapacity returns OutOfSpace at max capacity"
#[test]
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'Parity tail C-1 — the G6 triage'): past a 15-minute Miri bound in the debug-assertions-ON G6 lane, where `Page::assert_integrity` runs on nearly every page mutation (measured per test; see the C-1 table there). It RUNS in the G6c lane (`check_leaks.sh --debug-assertions-off`). Do NOT widen this to a plain `miri` ignore: that would drop the test from BOTH lanes."
)]
fn pagelist_increaseCapacity_returns_OutOfSpace_at_max_capacity() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(2, 2)
        },
    )
    .unwrap();

    // Keep increasing styles capacity until we get OutOfSpace
    let max_styles = size::StyleCountInt::MAX;
    loop {
        match s.increase_capacity(s.pages.first, Some(IncreaseCapacity::Styles)) {
            Ok(_) => {}
            Err(err) => {
                // Before OutOfSpace, we should have reached maxInt
                assert_eq!(IncreaseCapacityError::OutOfSpace, err);
                assert_eq!(max_styles, unsafe {
                    crate::pagelist::Node::capacity(s.pages.first).styles
                });
                break;
            }
        }
    }

    s.deinit();
}

// ref: test "PageList increaseCapacity multi-page"
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
)]
#[test]
fn pagelist_increaseCapacity_multi_page() {
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();

    // Grow to create a second page
    let page1_node = s.pages.last;
    unsafe { (*crate::pagelist::Node::page(page1_node)).pause_integrity_checks(true) };
    for _ in 0..(unsafe {
        crate::pagelist::Node::capacity(page1_node).rows - crate::pagelist::Node::rows(page1_node)
    }) {
        assert!(s.grow().unwrap().is_none());
    }
    unsafe { (*crate::pagelist::Node::page(page1_node)).pause_integrity_checks(false) };
    assert!(s.grow().unwrap().is_some());

    // Now we have two pages
    assert!(!core::ptr::eq(s.pages.first, s.pages.last));
    let page2_node = s.pages.last;

    let page1_styles_cap = unsafe { crate::pagelist::Node::capacity(s.pages.first).styles };
    let page2_styles_cap = unsafe { crate::pagelist::Node::capacity(page2_node).styles };

    // Increase capacity on the first page only
    let _ = s
        .increase_capacity(s.pages.first, Some(IncreaseCapacity::Styles))
        .unwrap();

    // First page capacity should be doubled
    assert_eq!(page1_styles_cap * 2, unsafe {
        crate::pagelist::Node::capacity(s.pages.first).styles
    });

    // Second page should be unchanged
    assert_eq!(page2_styles_cap, unsafe {
        crate::pagelist::Node::capacity(s.pages.last).styles
    });

    s.deinit();
}

// ref: test "PageList increaseCapacity preserves dirty flag"
#[test]
fn pagelist_increaseCapacity_preserves_dirty_flag() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(2, 4)
        },
    )
    .unwrap();

    // Set page dirty flag and mark some rows as dirty
    let new_node = {
        let page = unsafe { crate::pagelist::Node::page(s.pages.first) };
        unsafe { (*page).dirty = true };

        let base = unsafe { (*page).memory_ptr() };
        let rows = unsafe { (*page).rows.ptr(base) };
        unsafe {
            (*rows.add(0)).set_dirty(true);
            (*rows.add(1)).set_dirty(false);
            (*rows.add(2)).set_dirty(true);
            (*rows.add(3)).set_dirty(false);
        }

        // Increase capacity
        s.increase_capacity(s.pages.first, Some(IncreaseCapacity::Styles))
            .unwrap()
    };

    // The page dirty flag should be preserved
    assert!(unsafe { (*crate::pagelist::Node::page(new_node)).dirty });

    // Row dirty flags should be preserved
    let new_rows = unsafe {
        (*crate::pagelist::Node::page(new_node))
            .rows
            .ptr((*crate::pagelist::Node::page(new_node)).memory_ptr())
    };
    assert!(unsafe { (*new_rows.add(0)).dirty() });
    assert!(!unsafe { (*new_rows.add(1)).dirty() });
    assert!(unsafe { (*new_rows.add(2)).dirty() });
    assert!(!unsafe { (*new_rows.add(3)).dirty() });

    s.deinit();
}

// =====================================================================
// reset
// =====================================================================

// ref: test "PageList reset"
#[test]
fn pagelist_reset() {
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();
    s.reset();
    assert!(s.viewport == crate::pagelist::Viewport::Active);
    assert!(!s.pages.first.is_null());
    assert_eq!(s.rows as usize, s.total_rows());

    // Active area should be the top
    assert!(pin_eql(
        Pin {
            node: s.pages.first,
            y: 0,
            x: 0,
            garbage: false,
        },
        s.get_top_left(point::Tag::Active),
    ));

    s.deinit();
}

// PORT NOTE: AUDIT (#2/#4 lazy-zero): not a reference `test {}` block. Asserts the
// faithful zero CONTRACT after the PagePool -> MemoryPoolAligned/arena rework.
//
// `Page::init_buf` initializes the page structures (rows/styles/etc.) but does
// NOT write the CELL region — it relies on the handed-out page buffer being
// zero so cells read as empty. This test exercises that contract on both the
// fresh-pool-slot path (init) AND the reused-free-list-slot-after-reset path
// (the reset-time arena `@memset(0)` + the `destroyNodeExt` reuse `@memset(0)`):
// every active cell must read as empty/zeroed in both cases.
#[test]
fn pagelist_fresh_page_buffer_is_observably_zero() {
    fn assert_all_cells_zero(s: &PageList, label: &str) {
        // Walk every page's every cell directly (the active area of a fresh /
        // just-reset PageList is the whole page). A non-zero cells region would
        // surface here as a non-empty / non-zeroed cell.
        let mut it = s.pages.first;
        while !it.is_null() {
            let node = it;
            let rows = unsafe { crate::pagelist::Node::rows(node) };
            let cols = unsafe { crate::pagelist::Node::cols(node) };
            for y in 0..rows {
                for x in 0..cols {
                    let rac = unsafe {
                        (*crate::pagelist::Node::page(node))
                            .get_row_and_cell(x as usize, y as usize)
                    };
                    let cell = unsafe { *rac.cell };
                    assert!(
                        cell.is_empty() && cell.to_bits() == Cell::zeroed().to_bits(),
                        "{label}: cell ({x},{y}) must read as zeroed/empty",
                    );
                }
            }
            it = unsafe { (*node).next };
        }
    }

    // Fresh pool slot: the very first page handed out at init is lazily
    // OS-zeroed mmap memory (no eager memset); every active cell reads empty.
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();
    assert_eq!(
        unsafe { (*crate::pagelist::Node::page(s.pages.first)).memory_len },
        std_size()
    );
    assert_all_cells_zero(&s, "fresh");

    // Reused slot after reset: scribble non-zero bytes across the ENTIRE page
    // buffer, then reset. Reset retains + zeroes the arena and rebuilds the
    // pages from the retained free-list, so the cell region must read zero again
    // even though the slot was not freshly mmap'd.
    {
        let first = s.pages.first;
        let ptr = unsafe { (*crate::pagelist::Node::page(first)).memory_ptr() } as *mut u8;
        let len = unsafe { (*crate::pagelist::Node::page(first)).memory_len };
        unsafe {
            core::ptr::write_bytes(ptr, 0xAB, len);
        }
    }
    s.reset();
    assert_all_cells_zero(&s, "reused-after-reset");

    s.deinit();
}

/// PORTS: oracle f6d9a582e — only `reset` begins a new whole-list validity
/// epoch. Growth, replacement and splitting must NOT advance it, because
/// generations are not monotonic in list order.
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
)]
#[test]
fn pagelist_only_reset_advances_the_serial_epoch() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(10 * 1024 * 1024),
            ..Options::new(80, 24)
        },
    )
    .unwrap();
    let epoch0 = s.page_serial_epoch;
    fill_compressible_history(&mut s, 4);
    assert_eq!(
        s.page_serial_epoch, epoch0,
        "growth must not advance the epoch"
    );

    s.reset();
    assert!(s.page_serial_epoch > epoch0, "reset begins a new epoch");
    // Every live node belongs to the current epoch.
    let mut it = s.pages.first;
    while !it.is_null() {
        assert!(unsafe { (*it).serial } >= s.page_serial_epoch);
        it = unsafe { (*it).next };
    }
    s.deinit();
}

// ref: test "PageList reset invalidates stale untracked refs even if node memory is reused"
//
// Reset advances the epoch before rebuilding from the node pool. Reject
// the stale generation before inspecting its pointer, even when that exact
// address now belongs to a new live generation.
#[test]
fn pagelist_reset_invalidates_stale_untracked_refs_even_if_node_memory_is_reused() {
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();

    // var stale_nodes: [page_preheat * 4]*List.Node = undefined;
    // var stale_serials: [stale_nodes.len]u64 = undefined;
    let mut stale_nodes: [*mut crate::pagelist::Node; crate::pagelist::PAGE_PREHEAT * 4] =
        [core::ptr::null_mut(); crate::pagelist::PAGE_PREHEAT * 4];
    let mut stale_serials: [u64; crate::pagelist::PAGE_PREHEAT * 4] =
        [0; crate::pagelist::PAGE_PREHEAT * 4];
    let mut stale_len: usize = 0;
    let mut reused: Option<(*mut crate::pagelist::Node, u64)> = None;

    // Reset until the node pool hands the same ADDRESS back out, which is the
    // case the serial epoch exists to reject.
    while stale_len < stale_nodes.len() && reused.is_none() {
        let old_node = s.pages.first;
        let old_serial = unsafe { (*old_node).serial };
        assert!(old_serial >= s.page_serial_epoch);
        assert!(old_serial < s.page_serial);
        stale_nodes[stale_len] = old_node;
        stale_serials[stale_len] = old_serial;
        stale_len += 1;

        s.reset();

        let new_node = s.pages.first;
        for i in 0..stale_len {
            if core::ptr::eq(stale_nodes[i], new_node) {
                reused = Some((stale_nodes[i], stale_serials[i]));
                break;
            }
        }
    }

    assert!(reused.is_some());
    let (old_node, old_serial) = reused.unwrap();
    let new_node = s.pages.first;
    let new_serial = unsafe { (*new_node).serial };

    // Reset advances the epoch before rebuilding from the node pool. Reject
    // the stale generation before inspecting its pointer, even when that exact
    // address now belongs to a new live generation.
    assert!(core::ptr::eq(old_node, new_node));
    assert!(old_serial < s.page_serial_epoch);
    assert!(!s.node_is_valid(old_node, old_serial));
    assert!(s.node_is_valid(new_node, new_serial));
    assert!(new_serial >= s.page_serial_epoch);
    assert!(new_serial < s.page_serial);

    s.deinit();
}

// ref: test "PageList reset across two pages"
#[test]
fn pagelist_reset_across_two_pages() {
    // Find a cap that makes it so that rows don't fit on one page.
    let rows = 100;
    let cap = {
        let mut cap = std_capacity()
            .adjust(crate::page::Adjustment { cols: Some(50) })
            .unwrap();
        while cap.rows >= rows {
            cap = std_capacity()
                .adjust(crate::page::Adjustment {
                    cols: Some(cap.cols + 50),
                })
                .unwrap();
        }
        cap
    };

    // Init
    let mut s = PageList::init(Global, Options::new(cap.cols, rows)).unwrap();
    s.reset();
    assert!(s.viewport == crate::pagelist::Viewport::Active);
    assert!(!s.pages.first.is_null());
    assert_eq!(s.rows as usize, s.total_rows());

    s.deinit();
}

// ref: test "PageList reset moves tracked pins and marks them as garbage"
#[test]
fn pagelist_reset_moves_tracked_pins_and_marks_them_as_garbage() {
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();

    // Create a tracked pin into the active area
    let p = s
        .track_pin(
            s.pin(point::Point::Active(point::Coordinate { x: 42, y: 12 }))
                .unwrap(),
        )
        .unwrap();

    s.reset();

    // Our added pin should now be garbage
    assert!(unsafe { (*p).garbage });

    // Viewport pin should not be garbage because it makes sense.
    assert!(!unsafe { (*s.viewport_pin).garbage });

    s.untrack_pin(p);
    s.deinit();
}

// ref: test "PageList clears history"
#[test]
fn pagelist_clears_history() {
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();
    s.grow_rows(30).unwrap();
    s.reset();
    assert!(s.viewport == crate::pagelist::Viewport::Active);
    assert!(!s.pages.first.is_null());
    assert_eq!(s.rows as usize, s.total_rows());

    // Active area should be the top
    assert!(pin_eql(
        Pin {
            node: s.pages.first,
            y: 0,
            x: 0,
            garbage: false,
        },
        s.get_top_left(point::Tag::Active),
    ));

    s.deinit();
}
