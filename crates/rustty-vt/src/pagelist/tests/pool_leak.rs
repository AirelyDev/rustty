//! PORTS: ghostty src/terminal/PageList.zig — the page-pool leak coverage (no
//! reference `test {}` blocks) and the Pin-wrap, PageIterator-count,
//! screenPoint, pointFromPin, zero-capacity `increaseCapacity` and
//! hyperlink-dense `eraseRow` tests.

use super::cell_codepoint;
use crate::alloc::Global;
use crate::page::{Capacity, Page};
use crate::pagelist::grow::IncreaseCapacity;
use crate::pagelist::scroll::Scroll;
use crate::pagelist::{Options, PageList, Pin, Viewport};
use crate::{hyperlink, point};
use rustty_render_types::term_size as size;

// ===========================================================================
// Page-pool leak coverage (scoped `std.testing.allocator` equivalent, R6).
//
// These tests are NOT ports of reference `test {}` blocks — they are the Rust
// faithful equivalent of the leak detection Zig's test runner gives for free via
// `pageAllocator()` returning `std.testing.allocator`. Under `cfg(test)` the page
// pool's backing allocator is a `TrackingAllocator` recording into the global
// `page_tracking` tracker (see mod.rs). Each test exercises representative page
// churn and then asserts every page allocated during the test was returned to the
// pool after `deinit`/`reset`. This gives native-speed page-leak coverage on the
// heavy resize/reflow and clone-over-many-pages paths where Miri times out.
// ===========================================================================

use crate::pagelist::page_tracking::PageLeakScope;

/// Grow scrollback to several pages, scroll around, then deinit — assert every
/// page allocated during the churn was freed (no page leak).
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
)]
#[test]
fn pagelist_pagepool_no_leak_grow_and_scroll() {
    let scope = PageLeakScope::new();
    {
        let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();

        // Grow until we have at least several pages chained, forcing real page
        // allocations from the pool.
        let mut pages = 0usize;
        while pages < 4 {
            let _ = s.grow().unwrap();
            pages = 0;
            let mut it = s.pages.first;
            while !it.is_null() {
                pages += 1;
                it = unsafe { (*it).next };
            }
        }

        // We should have created real page churn (more than the baseline page).
        assert!(
            scope.delta_allocs() > 0,
            "expected the grow churn to allocate at least one new page"
        );

        // Scroll the viewport around the scrollback and back to active.
        s.scroll(Scroll::Top);
        s.scroll(Scroll::DeltaRow(5));
        s.scroll(Scroll::Active);

        s.deinit();
    }
    // After deinit every page allocated during this scope must be returned.
    scope.assert_returned_to_baseline();
}

/// HEAVY path: build a large scrollback, clone the whole thing (which builds a
/// second PageList over many pages), then deinit both — assert no page leak. This
/// is exactly the kind of many-page churn Miri cannot run to completion.
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
)]
#[test]
fn pagelist_pagepool_no_leak_clone_many_pages() {
    let scope = PageLeakScope::new();
    {
        let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();
        // Grow a large scrollback so the clone copies many pages.
        s.grow_rows(500).unwrap();

        assert!(
            scope.delta_allocs() > 0,
            "expected grow_rows to allocate scrollback pages"
        );

        let mut opts = crate::pagelist::access::Clone {
            top: point::Point::Screen(point::Coordinate { x: 0, y: 0 }),
            bot: None,
            tracked_pins: None,
        };
        let mut s2 = s.clone(Global, &mut opts).unwrap();
        // The clone really copied the rows.
        assert_eq!(s.total_rows(), s2.total_rows());

        // Deinit both; all pages from both PageLists must come back.
        s2.deinit();
        s.deinit();
    }
    scope.assert_returned_to_baseline();
}

/// HEAVY path: resize with reflow over a multi-page scrollback (shrink then grow
/// columns, forcing reflow to allocate and free pages), then deinit — assert no
/// page leak. Reflow is the most allocation-churny PageList path.
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
)]
#[test]
fn pagelist_pagepool_no_leak_resize_reflow() {
    let scope = PageLeakScope::new();
    {
        let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();
        s.grow_rows(300).unwrap();

        // Fill rows with content so reflow actually has to move/repack cells
        // (and so column shrink can't just trim trailing blanks).
        {
            let mut it = s.row_iterator(
                crate::pagelist::iter::Direction::RightDown,
                point::Point::Active(point::Coordinate::default()),
                None,
            );
            while let Some(p) = it.next() {
                let rac = p.row_and_cell();
                let cells = unsafe { (*crate::pagelist::Node::page(p.node)).get_cells(rac.row) };
                let len = cells.len();
                for x in 0..len {
                    unsafe {
                        (*cells)[x] = cell_codepoint(('A' as u32) + (x as u32 % 26));
                    }
                }
            }
        }

        assert!(
            scope.delta_allocs() > 0,
            "expected grow_rows to allocate scrollback pages before reflow"
        );

        // Shrink columns with reflow (heavy: repacks every row across pages).
        s.resize(crate::pagelist::resize::Resize {
            cols: Some(40),
            rows: None,
            reflow: true,
            cursor: None,
            pull_scrollback: true,
        })
        .unwrap();
        assert_eq!(40, s.cols);

        // Grow columns back with reflow (heavy: unpacks rows again).
        s.resize(crate::pagelist::resize::Resize {
            cols: Some(100),
            rows: None,
            reflow: true,
            cursor: None,
            pull_scrollback: true,
        })
        .unwrap();
        assert_eq!(100, s.cols);

        s.deinit();
    }
    scope.assert_returned_to_baseline();
}

/// `reset()` returns the pool to (near) baseline: build scrollback across several
/// pages, reset, then deinit — assert no page leak across the reset+deinit cycle.
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
)]
#[test]
fn pagelist_pagepool_no_leak_reset_then_deinit() {
    let scope = PageLeakScope::new();
    {
        let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();
        s.grow_rows(200).unwrap();
        assert!(scope.delta_allocs() > 0);

        // reset() retains a bounded amount of pool capacity but must not leak.
        s.reset();

        // Churn again after reset to exercise the retained/zeroed arena path.
        s.grow_rows(200).unwrap();

        s.deinit();
    }
    scope.assert_returned_to_baseline();
}

// -- P3-W1 T1: width-aware pin traversal (oracle d6e24d985, tests 0ff4e41b2) --

// ref: fn mixedWidthPinListForTest
//
// PORT NOTE: the reference fixture builds one arm as
// `Node{ .data = .{ .resident = undefined } }`; we do NOT port that as
// `MaybeUninit`. Every page here is a real page built with `create_page`,
// exactly as the mixed-width arm of the reference fixture does.
fn mixed_width_pin_list_for_test() -> PageList {
    let mut result = PageList::init(Global, Options::new(2, 1)).unwrap();

    // This deliberately constructs a layout that normal PageList operations
    // do not expose yet. Keep integrity checks paused through deinit so the
    // fixture can exercise mixed-width traversal in isolation.
    result.pause_integrity_checks(true);

    for cols in [4u16, 3u16] {
        let node = result
            .create_page(crate::pagelist::CreatePage::new(Capacity {
                cols,
                rows: 1,
                ..Capacity::default()
            }))
            .unwrap();
        unsafe { (*crate::pagelist::Node::page(node)).size.rows = 1 };
        unsafe { result.pages.append(node) };
        result.total_rows += 1;
    }

    // Desired geometry is wider than the first and last stored pages.
    result.cols = 4;
    result
}

// ref: test "PageList Pin rightWrap exact row multiple"
#[test]
fn pagelist_pin_rightWrap_exact_row_multiple() {
    let mut s = PageList::init(Global, Options::new(10, 3)).unwrap();

    let start = s
        .pin(point::Point::Active(point::Coordinate { x: 5, y: 0 }))
        .unwrap();
    let wrapped = start.right_wrap(14).unwrap();
    let _ = wrapped.row_and_cell();

    assert_eq!(
        point::Point::Active(point::Coordinate { x: 9, y: 1 }),
        s.point_from_pin(point::Tag::Active, wrapped).unwrap()
    );

    s.deinit();
}

// ref: test "PageList Pin leftWrap exact row multiple"
#[test]
fn pagelist_pin_leftWrap_exact_row_multiple() {
    let mut s = PageList::init(Global, Options::new(10, 3)).unwrap();

    let start = s
        .pin(point::Point::Active(point::Coordinate { x: 5, y: 2 }))
        .unwrap();
    let wrapped = start.left_wrap(15).unwrap();
    let _ = wrapped.row_and_cell();

    assert_eq!(
        point::Point::Active(point::Coordinate { x: 0, y: 1 }),
        s.point_from_pin(point::Tag::Active, wrapped).unwrap()
    );

    s.deinit();
}

// ref: test "PageList Pin rightWrap maximum distance"
#[test]
fn pagelist_pin_rightWrap_maximum_distance() {
    let mut s = PageList::init(Global, Options::new(1, 3)).unwrap();

    let start = s
        .pin(point::Point::Active(point::Coordinate { x: 0, y: 0 }))
        .unwrap();
    assert_eq!(None, start.right_wrap(usize::MAX));

    s.deinit();
}

// ref: test "PageList Pin leftWrap maximum distance"
#[test]
fn pagelist_pin_leftWrap_maximum_distance() {
    let mut s = PageList::init(Global, Options::new(1, 3)).unwrap();

    let start = s
        .pin(point::Point::Active(point::Coordinate { x: 0, y: 2 }))
        .unwrap();
    assert_eq!(None, start.left_wrap(usize::MAX));

    s.deinit();
}

/// `P3-W1` row 1b: a ONE-COLUMN pagelist is constructible in production
/// (`GridSize::update` floors the column count at 1, and `grid_size.columns`
/// flows straight into `Terminal::resize`). At `cols == 1` the four
/// `selection_gesture.rs` callers' literal `n = 1` DOES reach the wrap
/// remainder path, so rows 1/2 of `REACHABILITY.md` are LIVE, not LATENT.
/// This is the production-shaped reproducer: `right_wrap(1)` from the single
/// column of row 0 must land on the single column of row 1.
#[test]
fn pagelist_pin_wrap_one_column_selection_gesture_shape() {
    let mut s = PageList::init(Global, Options::new(1, 3)).unwrap();

    let start = s
        .pin(point::Point::Active(point::Coordinate { x: 0, y: 0 }))
        .unwrap();
    let right = start.right_wrap(1).expect("row 1 exists");
    let _ = right.row_and_cell();
    assert_eq!(
        point::Point::Active(point::Coordinate { x: 0, y: 1 }),
        s.point_from_pin(point::Tag::Active, right).unwrap()
    );

    let back = right.left_wrap(1).expect("row 0 exists");
    let _ = back.row_and_cell();
    assert_eq!(
        point::Point::Active(point::Coordinate { x: 0, y: 0 }),
        s.point_from_pin(point::Tag::Active, back).unwrap()
    );

    s.deinit();
}

// ref: test "PageList Pin row movement clamps across mixed-width pages"
#[test]
fn pagelist_pin_row_movement_clamps_across_mixed_width_pages() {
    let mut s = mixed_width_pin_list_for_test();

    let first = s.pages.first;
    let second = unsafe { (*first).next };
    let third = unsafe { (*second).next };

    let mk = |node: *mut crate::pagelist::Node, x: u16| Pin {
        node,
        y: 0,
        x,
        garbage: false,
    };

    assert!(mk(third, 2).eql(mk(second, 3).down(1).unwrap()));
    assert!(mk(first, 1).eql(mk(second, 3).up(1).unwrap()));

    match mk(second, 3).down_overflow(10) {
        crate::pagelist::pin::PinOverflow::Offset(_) => panic!("expected overflow"),
        crate::pagelist::pin::PinOverflow::Overflow { end, .. } => assert!(mk(third, 2).eql(end)),
    }
    match mk(second, 3).up_overflow(10) {
        crate::pagelist::pin::PinOverflow::Offset(_) => panic!("expected overflow"),
        crate::pagelist::pin::PinOverflow::Overflow { end, .. } => assert!(mk(first, 1).eql(end)),
    }

    s.deinit();
}

// ref: test "PageList Pin wrapping crosses mixed-width pages"
#[test]
fn pagelist_pin_wrapping_crosses_mixed_width_pages() {
    let mut s = mixed_width_pin_list_for_test();

    let first = s.pages.first;
    let second = unsafe { (*first).next };
    let third = unsafe { (*second).next };

    let mk = |node: *mut crate::pagelist::Node, x: u16| Pin {
        node,
        y: 0,
        x,
        garbage: false,
    };

    assert!(mk(third, 2).eql(mk(first, 1).right_wrap(7).unwrap()));
    assert!(mk(second, 0).eql(mk(third, 2).left_wrap(6).unwrap()));
    assert!(mk(first, 0).left_wrap(1).is_none());
    assert!(mk(third, 2).right_wrap(1).is_none());

    s.deinit();
}

// ref: test "PageList Pin rejects columns beyond mixed-width page bounds"
#[test]
fn pagelist_pin_rejects_columns_beyond_mixed_width_page_bounds() {
    let mut s = mixed_width_pin_list_for_test();

    assert!(s
        .pin(point::Point::Screen(point::Coordinate { x: 1, y: 0 }))
        .is_some());
    assert!(s
        .pin(point::Point::Screen(point::Coordinate { x: 2, y: 0 }))
        .is_none());
    assert!(s
        .pin(point::Point::Screen(point::Coordinate { x: 3, y: 1 }))
        .is_some());
    assert!(s
        .pin(point::Point::Screen(point::Coordinate { x: 3, y: 2 }))
        .is_none());

    s.deinit();
}

// -- P3-W1 T2: the PageIterator count limit (oracle 9b466e9c5) ----------------

// ref: test "PageList PageIterator reverse count includes row zero"
#[test]
fn pagelist_PageIterator_reverse_count_includes_row_zero() {
    let mut s = PageList::init(Global, Options::new(2, 2)).unwrap();

    let mut it = crate::pagelist::iter::PageIterator {
        row: Some(s.get_top_left(point::Tag::Screen)),
        limit: crate::pagelist::iter::Limit::Count(1),
        direction: crate::pagelist::iter::Direction::LeftUp,
    };
    let chunk = it.next().unwrap();
    assert_eq!(0 as size::CellCountInt, chunk.start);
    assert_eq!(1 as size::CellCountInt, chunk.end);
    assert!(it.next().is_none());

    s.deinit();
}

// ref: test "PageList PageIterator count crosses page boundaries"
#[test]
fn pagelist_PageIterator_count_crosses_page_boundaries() {
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();

    let first = s.pages.first;
    unsafe { (*crate::pagelist::Node::page(first)).pause_integrity_checks(true) };
    while unsafe { crate::pagelist::Node::rows(first) }
        < unsafe { crate::pagelist::Node::capacity(first) }.rows
    {
        let _ = s.grow().unwrap();
    }
    unsafe { (*crate::pagelist::Node::page(first)).pause_integrity_checks(false) };
    let second = s.grow().unwrap().unwrap();

    let mut down = crate::pagelist::iter::PageIterator {
        row: Some(Pin {
            node: first,
            y: unsafe { crate::pagelist::Node::rows(first) } - 1,
            x: 0,
            garbage: false,
        }),
        limit: crate::pagelist::iter::Limit::Count(2),
        direction: crate::pagelist::iter::Direction::RightDown,
    };
    {
        let chunk = down.next().unwrap();
        assert!(core::ptr::eq(first, chunk.node));
        assert_eq!(
            unsafe { crate::pagelist::Node::rows(first) } - 1,
            chunk.start
        );
        assert_eq!(unsafe { crate::pagelist::Node::rows(first) }, chunk.end);
    }
    {
        let chunk = down.next().unwrap();
        assert!(core::ptr::eq(second, chunk.node));
        assert_eq!(0 as size::CellCountInt, chunk.start);
        assert_eq!(1 as size::CellCountInt, chunk.end);
    }
    assert!(down.next().is_none());

    let mut up = crate::pagelist::iter::PageIterator {
        row: Some(Pin {
            node: second,
            y: 0,
            x: 0,
            garbage: false,
        }),
        limit: crate::pagelist::iter::Limit::Count(2),
        direction: crate::pagelist::iter::Direction::LeftUp,
    };
    {
        let chunk = up.next().unwrap();
        assert!(core::ptr::eq(second, chunk.node));
        assert_eq!(0 as size::CellCountInt, chunk.start);
        assert_eq!(1 as size::CellCountInt, chunk.end);
    }
    {
        let chunk = up.next().unwrap();
        assert!(core::ptr::eq(first, chunk.node));
        assert_eq!(
            unsafe { crate::pagelist::Node::rows(first) } - 1,
            chunk.start
        );
        assert_eq!(unsafe { crate::pagelist::Node::rows(first) }, chunk.end);
    }
    assert!(up.next().is_none());

    s.deinit();
}

// -- P3-W1 T3: the release-arithmetic subset ---------------------------------
// oracle c753fe4a4, afbf5ba15, e6e4a9fdc, 30b42f42a. See
// `.port/phase-3/REVIEW_CLASSES.md` §1: Zig traps only in safety builds, Rust
// panics in debug and WRAPS in release, so each of these tests asserts a VALUE
// and is deliberately NOT gated on `debug_assertions` — a release wrap produces
// a wrong answer with no panic, and a `#[should_panic]`-only control would be
// blind to exactly that case. Run the file with `--release` as well as debug.

// ref: test "PageList scroll minimum row delta"
#[test]
fn pagelist_scroll_minimum_row_delta() {
    let mut s = PageList::init(Global, Options::new(10, 3)).unwrap();

    // Create one row of history so scrolling all the way back has an
    // observable result.
    s.grow_rows(1).unwrap();
    s.scroll(Scroll::DeltaRow(isize::MIN));

    assert!(matches!(s.viewport, Viewport::Top));

    s.deinit();
}

// ref: test "PageList: jump minimum prompt delta"
#[test]
fn pagelist_jump_minimum_prompt_delta() {
    let mut s = PageList::init(Global, Options::new(10, 3)).unwrap();

    s.scroll(Scroll::DeltaPrompt(isize::MIN));
    assert!(matches!(s.viewport, Viewport::Active));

    s.deinit();
}

// ref: test "PageList Cell screenPoint supports long scrollback"
//
// The reference's setup comment:
//
//   A modest number of full-size page nodes is enough to exceed the u16
//   row range without allocating any page backing memory. screenPoint only
//   reads the linked metadata while calculating the absolute coordinate.
//
// PORT NOTE: the reference builds a `[]Node` of 307 metadata-only nodes with
// `.data = .{ .resident = undefined }`. rustty does NOT construct `undefined`
// pages (see the `mixed_width_pin_list_for_test` note); instead this uses THREE
// real pages and temporarily raises the two leading pages' `size.rows` to the
// `CellCountInt` maximum. `screen_point` reads nothing but that metadata field
// and the `prev` links, so the model is exact for the code under test, and the
// real sizes are restored before `deinit`.
#[test]
fn pagelist_Cell_screenPoint_supports_long_scrollback() {
    let mut s = PageList::init(Global, Options::new(1, 1)).unwrap();
    s.pause_integrity_checks(true);

    for _ in 0..2 {
        let node = s
            .create_page(crate::pagelist::CreatePage::new(Capacity {
                cols: 1,
                rows: 1,
                ..Capacity::default()
            }))
            .unwrap();
        unsafe { (*crate::pagelist::Node::page(node)).size.rows = 1 };
        unsafe { s.pages.append(node) };
        s.total_rows += 1;
    }

    let first = s.pages.first;
    let second = unsafe { (*first).next };
    let third = unsafe { (*second).next };

    let rows_per_page = size::CellCountInt::MAX;
    for node in [first, second] {
        unsafe { (*crate::pagelist::Node::page(node)).size.rows = rows_per_page };
    }

    // Two full-height leading pages exceed the u16 row range, which is the
    // whole point: the pre-fix accumulator was `size::CellCountInt`.
    let expected_y: u32 = 2 * rows_per_page as u32;
    assert!(expected_y > size::CellCountInt::MAX as u32);

    let rac = Pin {
        node: third,
        y: 0,
        x: 0,
        garbage: false,
    }
    .row_and_cell();
    let cell = crate::pagelist::access::Cell {
        node: third,
        row: rac.row,
        cell: rac.cell,
        row_idx: 0,
        col_idx: 0,
    };
    assert_eq!(
        point::Point::Screen(point::Coordinate {
            x: 0,
            y: expected_y
        }),
        cell.screen_point()
    );

    // Restore the true sizes so deinit sees a consistent list.
    for node in [first, second] {
        unsafe { (*crate::pagelist::Node::page(node)).size.rows = 1 };
    }
    s.deinit();
}

// ref: test "PageList pointFromPin rejects overflowing screen coordinate"
//
// The reference's setup comment:
//
//   Use maximum-height metadata-only pages to model a valid scrollback just
//   beyond the u32 coordinate range without allocating their backing cells.
//
// PORT NOTE: the reference allocates 65,539 metadata-only `Node`s so the
// accumulated Y exceeds `u32`. That is ~16 MB of `undefined` page structs in
// rustty terms, so instead this builds a THREE-page list, sets every page's
// `size.rows` to the `CellCountInt` maximum, and closes the `next` chain into a
// RING. `point_from_pin`'s walk is a pure accumulate-until-found loop over
// `next`, so a ring drives exactly the same arithmetic with 65k iterations
// instead of 65k allocations, and the pin it searches for is a page that is not
// in the ring. Pre-fix this panicked "attempt to add with overflow" in debug and
// LOOPED FOREVER on a wrapped accumulator in release; post-fix the checked add
// returns `None`, which is the oracle's fallback and also guarantees
// termination. The ring is undone before `deinit`.
#[test]
fn pagelist_pointFromPin_rejects_overflowing_screen_coordinate() {
    let mut s = PageList::init(Global, Options::new(1, 1)).unwrap();
    s.pause_integrity_checks(true);

    let mut extra: Vec<*mut crate::pagelist::Node> = Vec::new();
    for _ in 0..3 {
        let node = s
            .create_page(crate::pagelist::CreatePage::new(Capacity {
                cols: 1,
                rows: 1,
                ..Capacity::default()
            }))
            .unwrap();
        unsafe { (*crate::pagelist::Node::page(node)).size.rows = 1 };
        extra.push(node);
    }
    // The first two join the list; the third stays detached and is the pin we
    // search for, so the walk never terminates by finding it.
    for &node in &extra[..2] {
        unsafe { s.pages.append(node) };
        s.total_rows += 1;
    }
    let target = extra[2];

    let first = s.pages.first;
    let second = unsafe { (*first).next };
    let third = unsafe { (*second).next };
    let saved_next = unsafe { (*third).next };

    for node in [first, second, third] {
        unsafe { (*crate::pagelist::Node::page(node)).size.rows = size::CellCountInt::MAX };
    }
    // Close the ring.
    unsafe { (*third).next = first };

    assert_eq!(
        None,
        s.point_from_pin(
            point::Tag::Screen,
            Pin {
                node: target,
                y: 0,
                x: 0,
                garbage: false,
            }
        )
    );

    // Undo the ring and the sizes.
    unsafe { (*third).next = saved_next };
    for node in [first, second, third] {
        unsafe { (*crate::pagelist::Node::page(node)).size.rows = 1 };
    }
    // The detached page never entered the list, so free it explicitly.
    unsafe { s.pages.append(target) };
    s.total_rows += 1;
    s.deinit();
}

// -- P3-W1 T4 step 3: growth from a zero capacity dimension (oracle 8307349ec) --

// ref: test "PageList increaseCapacity from zero-capacity dimensions"
#[test]
fn pagelist_increaseCapacity_from_zero_capacity_dimensions() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(80, 24)
        },
    )
    .unwrap();

    // Compact the only page. A plain page has no styled, grapheme,
    // or hyperlink content so the exact capacity is zero in every
    // managed dimension.
    let mut node = s.compact(s.pages.first).unwrap().unwrap();
    let cap = |n: *mut crate::pagelist::Node| unsafe { crate::pagelist::Node::capacity(n) };
    assert_eq!(0, cap(node).styles);
    assert_eq!(0, cap(node).grapheme_bytes);
    assert_eq!(0, cap(node).string_bytes);
    assert_eq!(0, cap(node).hyperlink_bytes);

    // Increasing each dimension from zero must actually grow it.
    // Regression: 0 * 2 == 0 used to "succeed" without growing,
    // which turned caller retry loops into infinite loops.
    node = s
        .increase_capacity(node, Some(IncreaseCapacity::Styles))
        .unwrap();
    assert!(cap(node).styles > 0);
    node = s
        .increase_capacity(node, Some(IncreaseCapacity::GraphemeBytes))
        .unwrap();
    assert!(cap(node).grapheme_bytes > 0);
    node = s
        .increase_capacity(node, Some(IncreaseCapacity::StringBytes))
        .unwrap();
    assert!(cap(node).string_bytes > 0);
    node = s
        .increase_capacity(node, Some(IncreaseCapacity::HyperlinkBytes))
        .unwrap();
    assert!(cap(node).hyperlink_bytes > 0);

    // Increasing a non-zero dimension still doubles.
    let styles = cap(node).styles;
    node = s
        .increase_capacity(node, Some(IncreaseCapacity::Styles))
        .unwrap();
    assert_eq!(styles * 2, cap(node).styles);

    s.deinit();
}

/// `REACHABILITY.md` row 6's own note: the consequence of the pre-fix code is
/// NOT a panic — `0 * 2 == 0`, so the dimension never grows and the
/// unbounded-retry callers spin allocating a page per iteration. A
/// `#[should_panic]` control is therefore blind to it and an unguarded
/// reproduction of the caller loop would HANG the suite. This models the retry
/// loop with an explicit ceiling instead: pre-fix it exhausts the ceiling,
/// post-fix it terminates on the first iteration.
#[test]
fn pagelist_increaseCapacity_from_zero_terminates_within_a_bounded_retry() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(80, 24)
        },
    )
    .unwrap();

    let mut node = s.compact(s.pages.first).unwrap().unwrap();
    assert_eq!(0, unsafe { crate::pagelist::Node::capacity(node) }.styles);

    const CEILING: usize = 8;
    let mut iterations = 0usize;
    while unsafe { crate::pagelist::Node::capacity(node) }.styles == 0 {
        iterations += 1;
        assert!(
            iterations <= CEILING,
            "increase_capacity did not grow a zero dimension in {CEILING} \
             retries; every unbounded-retry caller (start_hyperlink, \
             cursor_set_hyperlink, the reflow probes, insert_lines/delete_lines) \
             is an infinite allocation loop in this state"
        );
        node = s
            .increase_capacity(node, Some(IncreaseCapacity::Styles))
            .unwrap();
    }
    assert_eq!(1, iterations, "one retry must be enough");

    s.deinit();
}

// ref: test "PageList eraseRow hyperlink-dense row crosses page boundary"
#[test]
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'Parity tail C-1 — the G6 triage'): past a 15-minute Miri bound in the debug-assertions-ON G6 lane, where `Page::assert_integrity` runs on nearly every page mutation (measured per test; see the C-1 table there). It RUNS in the G6c lane (`check_leaks.sh --debug-assertions-off`). Do NOT widen this to a plain `miri` ignore: that would drop the test from BOTH lanes."
)]
fn pagelist_eraseRow_hyperlink_dense_row_crosses_page_boundary() {
    // Regression test: when eraseRow shifts rows up across a page
    // boundary, the top row of the next page is cloned into the last
    // row of the previous page. If the previous page doesn't have
    // enough capacity for the managed memory of that row (hyperlinks,
    // styles, etc.) the error propagated out AFTER the previous page
    // had already been rotated and its tracked pins moved, leaving
    // the page list half-mutated.
    //
    // eraseRow must instead increase the destination page's capacity
    // and retry, the same way insertLines/deleteLines and
    // cursorScrollAbove handle their cross-page copies.
    let mut s = PageList::init(Global, Options::new(80, 10)).unwrap();

    // Grow to two pages so our active area straddles them: the first
    // page is exactly full and the second page holds the last 5 rows
    // of the active area.
    {
        let page = unsafe { crate::pagelist::Node::page(s.pages.last) };
        unsafe { (*page).pause_integrity_checks(true) };
        let n = unsafe { (*page).capacity.rows - (*page).size.rows };
        for _ in 0..n {
            let _ = s.grow().unwrap();
        }
        unsafe { (*page).pause_integrity_checks(false) };
        s.grow_rows(5).unwrap();
        assert_eq!(2, s.total_pages());
        assert_eq!(5, unsafe { crate::pagelist::Node::rows(s.pages.last) }
            as usize);
    }

    // Mark each active row with a codepoint so we can verify the
    // shift afterwards. Row y gets codepoint '0' + y at x = 0.
    for y in 0..10u32 {
        let row_pin = s
            .pin(point::Point::Active(point::Coordinate { x: 0, y }))
            .unwrap();
        let rac = unsafe {
            (*crate::pagelist::Node::page(row_pin.node))
                .get_row_and_cell(row_pin.x as usize, row_pin.y as usize)
        };
        unsafe { *rac.cell = cell_codepoint('0' as u32 + y) };
    }

    // Fill the top row of the second page (active y=5) with more
    // unique hyperlinks ('A' through 'J') than the first page's
    // default hyperlink capacity can hold. We must increase the
    // second page's capacity to even create such a row; the first
    // page keeps its default capacity.
    let link_count: usize = 10;
    while unsafe {
        (*crate::pagelist::Node::page(s.pages.last))
            .hyperlink_set
            .layout
            .cap
    } <= link_count
    {
        let _ = s
            .increase_capacity(s.pages.last, Some(IncreaseCapacity::HyperlinkBytes))
            .unwrap();
    }
    assert!(
        unsafe {
            (*crate::pagelist::Node::page(s.pages.first))
                .hyperlink_set
                .layout
                .cap
        } < link_count,
        "the first page must NOT already fit the dense row, or the retry path never runs"
    );
    {
        let page = unsafe { crate::pagelist::Node::page(s.pages.last) };
        for x in 0..link_count {
            let uri = format!("http://example.com/{x}").into_bytes();
            let id = unsafe {
                (*page).insert_hyperlink(hyperlink::Hyperlink {
                    id: hyperlink::HyperlinkId::Implicit(x as size::OffsetInt),
                    uri,
                })
            }
            .unwrap();
            let rac = unsafe { (*page).get_row_and_cell(x, 0) };
            unsafe { *rac.cell = cell_codepoint('A' as u32 + x as u32) };
            unsafe { (*page).set_hyperlink(rac.row, rac.cell, id) }.unwrap();
            unsafe { (*page).hyperlink_set.use_id((*page).memory_ptr(), id) };
        }
    }

    // Track a pin in the shifted region of the first page to verify
    // it survives the capacity change of its node.
    let p = s
        .track_pin(
            s.pin(point::Point::Active(point::Coordinate { x: 3, y: 1 }))
                .unwrap(),
        )
        .unwrap();

    // Erase the first active row. The dense hyperlink row must cross
    // the page boundary into the first page, which requires growing
    // the first page's hyperlink capacity.
    s.erase_row(point::Point::Active(point::Coordinate { x: 0, y: 0 }))
        .unwrap();

    // Every remaining row shifted up by one: the '0' marker row was
    // erased, the dense row moved up across the page boundary to
    // row 4, and the last row was cleared.
    let expected: [u32; 10] = [
        '1' as u32, '2' as u32, '3' as u32, '4' as u32, 'A' as u32, '6' as u32, '7' as u32,
        '8' as u32, '9' as u32, 0,
    ];
    for (y, cp) in expected.iter().enumerate() {
        let list_cell = s
            .get_cell(point::Point::Active(point::Coordinate {
                x: 0,
                y: y as u32,
            }))
            .unwrap();
        assert_eq!(*cp, unsafe { (*list_cell.cell).codepoint() });
    }

    // Every cell of the dense row must still resolve to a real
    // hyperlink entry with the correct URI. A half-applied erase
    // leaves cells whose hyperlink flag is set but that have no map
    // entry, which aborts in clearCells later.
    for x in 0..link_count {
        let list_cell = s
            .get_cell(point::Point::Active(point::Coordinate {
                x: x as u16,
                y: 4,
            }))
            .unwrap();
        assert!(unsafe { (*list_cell.cell).hyperlink() });
        let page: *mut Page = unsafe { crate::pagelist::Node::page(list_cell.node) };
        let id = unsafe { (*page).lookup_hyperlink(list_cell.cell) }.unwrap();
        let link = unsafe { (*page).hyperlink_set.get((*page).memory_ptr(), id) };
        let uri = format!("http://example.com/{x}").into_bytes();
        assert_eq!(uri, unsafe {
            (*link).uri.slice((*page).memory_ptr()).to_vec()
        });
    }

    // All pages must pass integrity checks.
    let mut node_ = s.pages.first;
    while !node_.is_null() {
        unsafe { (*crate::pagelist::Node::page(node_)).assert_integrity() };
        node_ = unsafe { (*node_).next };
    }

    // Our tracked pin shifted up by one row and still points into
    // the (possibly replaced) first page.
    assert!(std::ptr::eq(s.pages.first, unsafe { (*p).node }));
    let p_pt = s
        .point_from_pin(point::Tag::Active, unsafe { *p })
        .unwrap()
        .coord();
    assert_eq!(3, p_pt.x);
    assert_eq!(0, p_pt.y);

    s.untrack_pin(p);
    s.deinit();
}

// ref: test "PageList eraseRowBounded hyperlink-dense row crosses page boundary"
#[test]
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'Parity tail C-1 — the G6 triage'): past a 15-minute Miri bound in the debug-assertions-ON G6 lane, where `Page::assert_integrity` runs on nearly every page mutation (measured per test; see the C-1 table there). It RUNS in the G6c lane (`check_leaks.sh --debug-assertions-off`). Do NOT widen this to a plain `miri` ignore: that would drop the test from BOTH lanes."
)]
fn pagelist_eraseRowBounded_hyperlink_dense_row_crosses_page_boundary() {
    // Same as the eraseRow variant above but for eraseRowBounded,
    // which has the same rotate-then-clone structure and had the
    // same bug: a cross-page row clone failure propagated out after
    // the first page had already been rotated.
    let mut s = PageList::init(Global, Options::new(80, 10)).unwrap();

    {
        let page = unsafe { crate::pagelist::Node::page(s.pages.last) };
        unsafe { (*page).pause_integrity_checks(true) };
        let n = unsafe { (*page).capacity.rows - (*page).size.rows };
        for _ in 0..n {
            let _ = s.grow().unwrap();
        }
        unsafe { (*page).pause_integrity_checks(false) };
        s.grow_rows(5).unwrap();
        assert_eq!(2, s.total_pages());
        assert_eq!(5, unsafe { crate::pagelist::Node::rows(s.pages.last) }
            as usize);
    }

    for y in 0..10u32 {
        let row_pin = s
            .pin(point::Point::Active(point::Coordinate { x: 0, y }))
            .unwrap();
        let rac = unsafe {
            (*crate::pagelist::Node::page(row_pin.node))
                .get_row_and_cell(row_pin.x as usize, row_pin.y as usize)
        };
        unsafe { *rac.cell = cell_codepoint('0' as u32 + y) };
    }

    let link_count: usize = 10;
    while unsafe {
        (*crate::pagelist::Node::page(s.pages.last))
            .hyperlink_set
            .layout
            .cap
    } <= link_count
    {
        let _ = s
            .increase_capacity(s.pages.last, Some(IncreaseCapacity::HyperlinkBytes))
            .unwrap();
    }
    assert!(
        unsafe {
            (*crate::pagelist::Node::page(s.pages.first))
                .hyperlink_set
                .layout
                .cap
        } < link_count,
        "the first page must NOT already fit the dense row, or the retry path never runs"
    );
    {
        let page = unsafe { crate::pagelist::Node::page(s.pages.last) };
        for x in 0..link_count {
            let uri = format!("http://example.com/{x}").into_bytes();
            let id = unsafe {
                (*page).insert_hyperlink(hyperlink::Hyperlink {
                    id: hyperlink::HyperlinkId::Implicit(x as size::OffsetInt),
                    uri,
                })
            }
            .unwrap();
            let rac = unsafe { (*page).get_row_and_cell(x, 0) };
            unsafe { *rac.cell = cell_codepoint('A' as u32 + x as u32) };
            unsafe { (*page).set_hyperlink(rac.row, rac.cell, id) }.unwrap();
            unsafe { (*page).hyperlink_set.use_id((*page).memory_ptr(), id) };
        }
    }

    // Erase the first active row with a limit that extends into the
    // second page (5 rows remain in the first page, so a limit of 6
    // forces the cross-page path). The dense hyperlink row must cross
    // the page boundary into the first page.
    s.erase_row_bounded(point::Point::Active(point::Coordinate { x: 0, y: 0 }), 6)
        .unwrap();

    // Rows within the limit shifted up by one: the '0' marker row was
    // erased, the dense row moved up across the page boundary to
    // row 4, row 6 is the new blank row, and rows past the limit are
    // unchanged.
    let expected: [u32; 10] = [
        '1' as u32, '2' as u32, '3' as u32, '4' as u32, 'A' as u32, '6' as u32, 0, '7' as u32,
        '8' as u32, '9' as u32,
    ];
    for (y, cp) in expected.iter().enumerate() {
        let list_cell = s
            .get_cell(point::Point::Active(point::Coordinate {
                x: 0,
                y: y as u32,
            }))
            .unwrap();
        assert_eq!(*cp, unsafe { (*list_cell.cell).codepoint() });
    }

    for x in 0..link_count {
        let list_cell = s
            .get_cell(point::Point::Active(point::Coordinate {
                x: x as u16,
                y: 4,
            }))
            .unwrap();
        assert!(unsafe { (*list_cell.cell).hyperlink() });
        let page: *mut Page = unsafe { crate::pagelist::Node::page(list_cell.node) };
        let id = unsafe { (*page).lookup_hyperlink(list_cell.cell) }.unwrap();
        let link = unsafe { (*page).hyperlink_set.get((*page).memory_ptr(), id) };
        let uri = format!("http://example.com/{x}").into_bytes();
        assert_eq!(uri, unsafe {
            (*link).uri.slice((*page).memory_ptr()).to_vec()
        });
    }

    let mut node_ = s.pages.first;
    while !node_.is_null() {
        unsafe { (*crate::pagelist::Node::page(node_)).assert_integrity() };
        node_ = unsafe { (*node_).next };
    }

    s.deinit();
}
