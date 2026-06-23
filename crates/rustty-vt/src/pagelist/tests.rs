//! Ported PageList `test {}` blocks (batch vt-3c part 1/4).
//!
//! PORTS: ghostty src/terminal/PageList.zig — the init/deinit/reset,
//! grow/increaseCapacity/compact/split, scrollbar, and
//! getTopLeft/totalRows/totalPages tests.
//!
//! PORT NOTE: these are integration tests that exercise many PageList methods,
//! so per the port plan they are co-located in a dedicated `tests.rs` rather
//! than split per-cluster. Each `#[test]` is an assertion-for-assertion port of
//! the reference `test {}` block of the same name, with expected values taken
//! from an independent reading of the reference (no tautologies).
//!
//! DEFERRED (land with their dependency cluster, NOT dropped, NOT stubbed):
//!   - `PageList init error` — exercises the `init_tw`/`initPages_tw` tripwire
//!     failpoints. The tripwire failpoint infrastructure is not yet ported
//!     (the `init`/`initPages` PORT NOTE in `mod.rs` defers it to the test
//!     phase); this test lands once that infra exists.
//!   - `PageList increaseCapacity after col shrink` — calls `PageList.resize`,
//!     which is DEFERRED in `resize.rs`. It lands with the resize cluster.

#![allow(non_snake_case)]
// PORT NOTE: these ported tests reproduce the reference's exact loop and range
// forms verbatim (e.g. `for (0..s.rows) |y|` nested loops); we keep them
// char-for-char rather than rewriting to iterator combinators.
#![allow(clippy::needless_range_loop)]
// PORT NOTE: several ported loops are `while (cond) _ = try s.grow()` where the
// condition reads page fields through a raw `*mut Node` pointer that `grow()`
// mutates indirectly (clippy can't see the mutation through the pointer), so
// the `while_immutable_condition` lint fires spuriously. The reference loops
// are reproduced verbatim.
#![allow(clippy::while_immutable_condition)]

use super::grow::{IncreaseCapacity, IncreaseCapacityError, Scrollbar, SplitError};
use super::scroll::Scroll;
use super::{std_capacity, std_size, PageList, PagePool, Pin};
use crate::page::{Cell, ContentTag, Page, SemanticPrompt};
use crate::style::{self as stylepkg, Style};
use crate::{hyperlink, point};
use rustty_render_types::term_size as size;

// Helper mirroring the reference cell-literal `.{ .content_tag = .codepoint,
// .content = .{ .codepoint = cp } }`.
fn cell_codepoint(cp: u32) -> Cell {
    let mut c = Cell::zeroed();
    c.set_content_tag(ContentTag::Codepoint);
    c.set_content_codepoint(cp);
    c
}

// Helper asserting a `Pin` equals the reference `Pin{ .node, .y, .x }` literal
// (which leaves `.garbage` at its `false` default).
//
// PORT NOTE: the CORE `Pin` struct does not derive `PartialEq`/`Debug` (it is a
// raw-pointer-bearing POD), so the reference `expectEqual(Pin{...}, ...)` is
// reproduced field-for-field including `garbage` (default `false`).
fn expect_pin(expected: Pin, got: Pin) {
    assert!(core::ptr::eq(expected.node, got.node));
    assert_eq!(expected.y, got.y);
    assert_eq!(expected.x, got.x);
    assert_eq!(expected.garbage, got.garbage);
}

#[test]
fn pagelist() {
    let mut s = PageList::init(80, 24, None).unwrap();

    assert!(s.viewport == super::Viewport::Active);
    assert!(!s.pages.first.is_null());
    assert_eq!(s.rows as usize, s.total_rows());

    // Initial total rows should be our row count
    assert_eq!(s.rows as usize, s.total_rows);

    // Our viewport pin must be defined. It isn't used until the
    // viewport is a pin but it prevents undefined access on clone.
    assert!(core::ptr::eq(
        unsafe { (*s.viewport_pin).node },
        s.pages.first
    ));

    // Active area should be the top
    expect_pin(
        Pin {
            node: s.pages.first,
            y: 0,
            x: 0,
            garbage: false,
        },
        s.get_top_left(point::Tag::Active),
    );

    // Scrollbar should be where we expect it
    assert_eq!(
        Scrollbar {
            total: s.rows as usize,
            offset: 0,
            len: s.rows as usize,
        },
        s.scrollbar(),
    );

    s.deinit();
}

// ref: test "PageList pointFromPin active no history"
#[test]
fn pagelist_pointFromPin_active_no_history() {
    let mut s = PageList::init(80, 24, None).unwrap();

    {
        assert_eq!(
            point::Point::Active(point::Coordinate { y: 0, x: 0 }),
            s.point_from_pin(
                point::Tag::Active,
                Pin {
                    node: s.pages.first,
                    y: 0,
                    x: 0,
                    garbage: false,
                }
            )
            .unwrap()
        );
    }
    {
        assert_eq!(
            point::Point::Active(point::Coordinate { y: 2, x: 4 }),
            s.point_from_pin(
                point::Tag::Active,
                Pin {
                    node: s.pages.first,
                    y: 2,
                    x: 4,
                    garbage: false,
                }
            )
            .unwrap()
        );
    }

    s.deinit();
}

// ref: test "PageList pointFromPin active with history"
#[test]
fn pagelist_pointFromPin_active_with_history() {
    let mut s = PageList::init(80, 24, None).unwrap();
    s.grow_rows(30).unwrap();

    {
        assert_eq!(
            point::Point::Active(point::Coordinate { y: 0, x: 2 }),
            s.point_from_pin(
                point::Tag::Active,
                Pin {
                    node: s.pages.first,
                    y: 30,
                    x: 2,
                    garbage: false,
                }
            )
            .unwrap()
        );
    }

    // In history, invalid
    {
        assert!(s
            .point_from_pin(
                point::Tag::Active,
                Pin {
                    node: s.pages.first,
                    y: 21,
                    x: 2,
                    garbage: false,
                }
            )
            .is_none());
    }

    s.deinit();
}

// ref: test "PageList pointFromPin active from prior page"
#[test]
fn pagelist_pointFromPin_active_from_prior_page() {
    let mut s = PageList::init(80, 24, None).unwrap();
    // Grow so we take up at least 5 pages.
    let page_cap = unsafe { (*s.pages.last).data.capacity.rows };
    let mut cur_page = s.pages.last;
    unsafe { (*cur_page).data.pause_integrity_checks(true) };
    for _ in 0..(page_cap as usize * 5) {
        if let Some(new_page) = s.grow().unwrap() {
            unsafe { (*cur_page).data.pause_integrity_checks(false) };
            cur_page = new_page;
            unsafe { (*cur_page).data.pause_integrity_checks(true) };
        }
    }
    unsafe { (*cur_page).data.pause_integrity_checks(false) };

    {
        assert_eq!(
            point::Point::Active(point::Coordinate { y: 0, x: 2 }),
            s.point_from_pin(
                point::Tag::Active,
                Pin {
                    node: s.pages.last,
                    y: 0,
                    x: 2,
                    garbage: false,
                }
            )
            .unwrap()
        );
    }

    // Prior page
    {
        assert!(s
            .point_from_pin(
                point::Tag::Active,
                Pin {
                    node: s.pages.first,
                    y: 0,
                    x: 0,
                    garbage: false,
                }
            )
            .is_none());
    }

    s.deinit();
}

// ref: test "PageList pointFromPin traverse pages"
#[test]
fn pagelist_pointFromPin_traverse_pages() {
    let mut s = PageList::init(80, 24, None).unwrap();

    // Grow so we take up at least 2 pages.
    let page_cap = unsafe { (*s.pages.last).data.capacity.rows };
    let mut cur_page = s.pages.last;
    unsafe { (*cur_page).data.pause_integrity_checks(true) };
    for _ in 0..(page_cap as usize * 2) {
        if let Some(new_page) = s.grow().unwrap() {
            unsafe { (*cur_page).data.pause_integrity_checks(false) };
            cur_page = new_page;
            unsafe { (*cur_page).data.pause_integrity_checks(true) };
        }
    }
    unsafe { (*cur_page).data.pause_integrity_checks(false) };

    {
        let pages = s.total_pages();
        let page_cap = page_cap as usize;
        let expected_y = page_cap * (pages - 2) + 5;

        assert_eq!(
            point::Point::Screen(point::Coordinate {
                y: expected_y as u32,
                x: 2,
            }),
            s.point_from_pin(
                point::Tag::Screen,
                Pin {
                    node: unsafe { (*s.pages.last).prev },
                    y: 5,
                    x: 2,
                    garbage: false,
                }
            )
            .unwrap()
        );
    }

    // Prior page
    {
        assert!(s
            .point_from_pin(
                point::Tag::Active,
                Pin {
                    node: s.pages.first,
                    y: 0,
                    x: 0,
                    garbage: false,
                }
            )
            .is_none());
    }

    s.deinit();
}

// ref: test "PageList scroll with max_size 0 no history"
#[test]
fn pagelist_scroll_with_max_size_0_no_history() {
    let mut s = PageList::init(80, 24, Some(0)).unwrap();

    s.grow_rows(10).unwrap();

    // Remember initial viewport position
    let pt_before = s
        .get_cell(point::Point::Viewport(point::Coordinate { x: 0, y: 0 }))
        .unwrap()
        .screen_point();

    // Try to scroll backwards into "history" - should be no-op
    s.scroll(Scroll::DeltaRow(-5));
    assert!(s.viewport == super::Viewport::Active);

    // Scroll to top - should also be no-op with no scrollback
    s.scroll(Scroll::Top);
    let pt_after = s
        .get_cell(point::Point::Viewport(point::Coordinate { x: 0, y: 0 }))
        .unwrap()
        .screen_point();
    assert_eq!(pt_before, pt_after);

    s.deinit();
}

// ref: test "PageList scroll top"
#[test]
fn pagelist_scroll_top() {
    let mut s = PageList::init(80, 24, None).unwrap();
    s.grow_rows(10).unwrap();

    {
        let pt = s
            .get_cell(point::Point::Viewport(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 10 }), pt);
    }

    s.scroll(Scroll::Top);

    {
        let pt = s
            .get_cell(point::Point::Viewport(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 0 }), pt);
    }

    assert_eq!(
        Scrollbar {
            total: s.total_rows(),
            offset: 0,
            len: s.rows as usize,
        },
        s.scrollbar()
    );

    s.grow_rows(10).unwrap();
    {
        let pt = s
            .get_cell(point::Point::Viewport(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 0 }), pt);
    }

    assert_eq!(
        Scrollbar {
            total: s.total_rows(),
            offset: 0,
            len: s.rows as usize,
        },
        s.scrollbar()
    );

    s.scroll(Scroll::Active);
    {
        let pt = s
            .get_cell(point::Point::Viewport(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 20 }), pt);
    }

    assert_eq!(
        Scrollbar {
            total: s.total_rows(),
            offset: s.total_rows - s.rows as usize,
            len: s.rows as usize,
        },
        s.scrollbar()
    );

    s.deinit();
}

// ref: test "PageList scroll delta row back"
#[test]
fn pagelist_scroll_delta_row_back() {
    let mut s = PageList::init(80, 24, None).unwrap();
    s.grow_rows(10).unwrap();

    {
        let pt = s
            .get_cell(point::Point::Viewport(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 10 }), pt);
    }

    s.scroll(Scroll::DeltaRow(-1));

    assert_eq!(
        Scrollbar {
            total: s.total_rows(),
            offset: s.total_rows - s.rows as usize - 1,
            len: s.rows as usize,
        },
        s.scrollbar()
    );

    {
        let pt = s
            .get_cell(point::Point::Viewport(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 9 }), pt);
    }

    s.grow_rows(10).unwrap();
    {
        let pt = s
            .get_cell(point::Point::Viewport(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 9 }), pt);
    }

    assert_eq!(
        Scrollbar {
            total: s.total_rows(),
            offset: s.total_rows - s.rows as usize - 11,
            len: s.rows as usize,
        },
        s.scrollbar()
    );

    s.scroll(Scroll::DeltaRow(-1));

    assert_eq!(
        Scrollbar {
            total: s.total_rows(),
            offset: s.total_rows - s.rows as usize - 12,
            len: s.rows as usize,
        },
        s.scrollbar()
    );

    s.deinit();
}

// ref: test "PageList scroll delta row back overflow"
#[test]
fn pagelist_scroll_delta_row_back_overflow() {
    let mut s = PageList::init(80, 24, None).unwrap();
    s.grow_rows(10).unwrap();

    {
        let pt = s
            .get_cell(point::Point::Viewport(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 10 }), pt);
    }

    s.scroll(Scroll::DeltaRow(-100));

    {
        let pt = s
            .get_cell(point::Point::Viewport(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 0 }), pt);
    }

    assert_eq!(
        Scrollbar {
            total: s.total_rows(),
            offset: 0,
            len: s.rows as usize,
        },
        s.scrollbar()
    );

    s.grow_rows(10).unwrap();
    {
        let pt = s
            .get_cell(point::Point::Viewport(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 0 }), pt);
    }

    assert_eq!(
        Scrollbar {
            total: s.total_rows(),
            offset: 0,
            len: s.rows as usize,
        },
        s.scrollbar()
    );

    s.deinit();
}

// ref: test "PageList scroll delta row forward"
#[test]
fn pagelist_scroll_delta_row_forward() {
    let mut s = PageList::init(80, 24, None).unwrap();
    s.grow_rows(10).unwrap();

    {
        let pt = s
            .get_cell(point::Point::Viewport(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 10 }), pt);
    }

    s.scroll(Scroll::Top);
    s.scroll(Scroll::DeltaRow(2));

    assert_eq!(
        Scrollbar {
            total: s.total_rows(),
            offset: 2,
            len: s.rows as usize,
        },
        s.scrollbar()
    );

    {
        let pt = s
            .get_cell(point::Point::Viewport(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 2 }), pt);
    }

    s.grow_rows(10).unwrap();
    {
        let pt = s
            .get_cell(point::Point::Viewport(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 2 }), pt);
    }

    assert_eq!(
        Scrollbar {
            total: s.total_rows(),
            offset: 2,
            len: s.rows as usize,
        },
        s.scrollbar()
    );

    s.deinit();
}

// ref: test "PageList scroll delta row forward into active"
#[test]
fn pagelist_scroll_delta_row_forward_into_active() {
    let mut s = PageList::init(80, 24, None).unwrap();

    s.scroll(Scroll::DeltaRow(2));

    {
        let pt = s
            .get_cell(point::Point::Viewport(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 0 }), pt);
    }

    assert_eq!(
        Scrollbar {
            total: s.total_rows(),
            offset: s.total_rows - s.rows as usize,
            len: s.rows as usize,
        },
        s.scrollbar()
    );

    s.deinit();
}

#[test]
fn pagelist_init_rows_across_two_pages() {
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
    let mut s = PageList::init(cap.cols, rows, None).unwrap();
    assert!(s.viewport == super::Viewport::Active);
    assert!(!s.pages.first.is_null());
    assert_eq!(s.rows as usize, s.total_rows());

    // Initial total rows should be our row count
    assert_eq!(s.rows as usize, s.total_rows);

    // Scrollbar should be where we expect it
    assert_eq!(
        Scrollbar {
            total: s.rows as usize,
            offset: 0,
            len: s.rows as usize,
        },
        s.scrollbar(),
    );

    s.deinit();
}

#[test]
fn pagelist_init_more_than_max_cols() {
    // Initialize with more columns than we can fit in our standard
    // capacity. This is going to force us to go to a non-standard page
    // immediately.
    let mut s = PageList::init(std_capacity().max_cols().unwrap() + 1, 80, None).unwrap();

    assert!(s.viewport == super::Viewport::Active);
    assert_eq!(s.rows as usize, s.total_rows());

    // We expect a single, non-standard page
    assert!(!s.pages.first.is_null());
    assert!(unsafe { (*s.pages.first).data.memory_len } > std_size());

    // Initial total rows should be our row count
    assert_eq!(s.rows as usize, s.total_rows);

    // Scrollbar should be where we expect it
    assert_eq!(
        Scrollbar {
            total: s.rows as usize,
            offset: 0,
            len: s.rows as usize,
        },
        s.scrollbar(),
    );

    s.deinit();
}

#[test]
fn pagelist_active_after_grow() {
    let mut s = PageList::init(80, 24, None).unwrap();
    assert_eq!(s.rows as usize, s.total_rows());

    s.grow_rows(10).unwrap();
    assert_eq!(s.rows as usize + 10, s.total_rows());

    // Make sure all points make sense
    {
        let pt = s
            .get_cell(point::Point::Viewport(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 10 }), pt);
    }
    {
        let pt = s
            .get_cell(point::Point::Screen(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 0 }), pt);
    }
    {
        let pt = s
            .get_cell(point::Point::Active(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 10 }), pt);
    }

    // Scrollbar should be in the active area
    assert_eq!(
        Scrollbar {
            total: s.total_rows(),
            offset: 10,
            len: s.rows as usize,
        },
        s.scrollbar(),
    );

    s.deinit();
}

#[test]
fn pagelist_grow_allows_exceeding_max_size_for_active_area() {
    // Setup our initial page so that we fully take up one page.
    let cap = std_capacity()
        .adjust(crate::page::Adjustment { cols: Some(5) })
        .unwrap();
    let mut s = PageList::init(5, cap.rows, Some(0)).unwrap();
    assert_eq!(s.rows as usize, s.total_rows());

    // Grow once because we guarantee at least two pages of
    // capacity so we want to get to that.
    let _ = s.grow().unwrap();
    let start_pages = s.total_pages();
    assert!(start_pages >= 2);

    // Surgically modify our pages so that they have a smaller size.
    {
        let mut it = s.pages.first;
        while !it.is_null() {
            let page = it;
            unsafe {
                (*page).data.size.rows = 1;
                (*page).data.capacity.rows = 1;
            }
            it = unsafe { (*page).next };
        }

        // Avoid integrity check failures
        s.total_rows = s.total_rows();
    }

    // Grow our row and ensure we don't prune pages because we need
    // enough for the active area.
    let _ = s.grow().unwrap();
    assert_eq!(start_pages + 1, s.total_pages());

    s.deinit();
}

#[test]
fn pagelist_grow_prune_required_with_a_single_page() {
    // Need scrollback > 0 to have a scrollbar to test
    let mut s = PageList::init(80, 24, None).unwrap();

    // This block is all test setup. There is nothing required about this
    // behavior during a refactor. This is setting up a scenario that is
    // possible to trigger a bug (#2280).
    {
        // Increase our capacity until our page is larger than the standard size.
        // This is important because it triggers a scenario where our calculated
        // minSize() which is supposed to accommodate 2 pages is no longer true.
        loop {
            let layout = Page::layout(unsafe { (*s.pages.first).data.capacity });
            if layout.total_size > std_size() {
                break;
            }
            let _ = s
                .increase_capacity(s.pages.first, Some(IncreaseCapacity::GraphemeBytes))
                .unwrap();
        }
        assert!(!s.pages.first.is_null());
        assert!(core::ptr::eq(s.pages.first, s.pages.last));
    }

    // Figure out the remaining number of rows. This is the amount that
    // can be added to the current page before we need to allocate a new
    // page.
    let rem = {
        let page = s.pages.first;
        unsafe { (*page).data.capacity.rows - (*page).data.size.rows }
    };
    for _ in 0..rem {
        assert!(s.grow().unwrap().is_none());
    }

    // The next one we add will trigger a new page.
    let new = s.grow().unwrap();
    assert!(new.is_some());
    assert!(!core::ptr::eq(new.unwrap(), s.pages.first));

    // Scrollbar should be in the active area
    assert_eq!(
        Scrollbar {
            total: s.total_rows(),
            offset: s.total_rows - s.rows as usize,
            len: s.rows as usize,
        },
        s.scrollbar(),
    );

    s.deinit();
}

#[test]
fn pagelist_scrollbar_with_max_size_0_after_grow() {
    let mut s = PageList::init(80, 24, Some(0)).unwrap();

    // Grow some rows (simulates normal terminal output)
    s.grow_rows(10).unwrap();

    let sb = s.scrollbar();

    // With no scrollback (max_size = 0), total should equal rows
    assert_eq!(s.rows as usize, sb.total);

    // With no scrollback, offset should be 0 (nowhere to scroll back to)
    assert_eq!(0, sb.offset);

    s.deinit();
}

#[test]
fn pagelist_grow_fit_in_capacity() {
    let mut s = PageList::init(80, 24, None).unwrap();

    // So we know we're using capacity to grow
    {
        let last = unsafe { &(*s.pages.last).data };
        assert!(last.size.rows < last.capacity.rows);
    }

    // Grow
    assert!(s.grow().unwrap().is_none());
    {
        let pt = s
            .get_cell(point::Point::Active(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 1 }), pt);
    }

    s.deinit();
}

#[test]
fn pagelist_grow_allocate() {
    let mut s = PageList::init(80, 24, None).unwrap();

    // Grow to capacity
    let last_node = s.pages.last;
    let (last_cap_rows, last_size_rows) = {
        let last = unsafe { &(*s.pages.last).data };
        (last.capacity.rows, last.size.rows)
    };
    for _ in 0..(last_cap_rows - last_size_rows) {
        assert!(s.grow().unwrap().is_none());
    }

    // Grow, should allocate
    let new = s.grow().unwrap().unwrap();
    assert!(core::ptr::eq(s.pages.last, new));
    assert!(core::ptr::eq(unsafe { (*last_node).next }, new));
    {
        let cell = s
            .get_cell(point::Point::Active(point::Coordinate {
                x: 0,
                y: (s.rows - 1) as u32,
            }))
            .unwrap();
        assert!(core::ptr::eq(cell.node, new));
        assert_eq!(
            point::Point::Screen(point::Coordinate {
                x: 0,
                y: last_cap_rows as u32,
            }),
            cell.screen_point(),
        );
    }

    s.deinit();
}

// =====================================================================
// increaseCapacity
// =====================================================================

#[test]
fn pagelist_increaseCapacity_to_increase_styles() {
    let mut s = PageList::init(2, 2, Some(0)).unwrap();

    let original_styles_cap = unsafe { (*s.pages.first).data.capacity.styles };

    {
        assert!(core::ptr::eq(s.pages.first, s.pages.last));
        let page = unsafe { &mut (*s.pages.first).data };

        // Write all our data so we can assert its the same after
        for y in 0..s.rows as usize {
            for x in 0..s.cols as usize {
                let rac = page.get_row_and_cell(x, y);
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
        let page = unsafe { &(*s.pages.first).data };

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

#[test]
fn pagelist_increaseCapacity_to_increase_graphemes() {
    let mut s = PageList::init(2, 2, Some(0)).unwrap();

    let original_cap = unsafe { (*s.pages.first).data.capacity.grapheme_bytes };

    {
        assert!(core::ptr::eq(s.pages.first, s.pages.last));
        let page = unsafe { &mut (*s.pages.first).data };

        for y in 0..s.rows as usize {
            for x in 0..s.cols as usize {
                let rac = page.get_row_and_cell(x, y);
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
        let page = unsafe { &(*s.pages.first).data };

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

#[test]
fn pagelist_increaseCapacity_to_increase_hyperlinks() {
    let mut s = PageList::init(2, 2, Some(0)).unwrap();

    let original_cap = unsafe { (*s.pages.first).data.capacity.hyperlink_bytes };

    {
        assert!(core::ptr::eq(s.pages.first, s.pages.last));
        let page = unsafe { &mut (*s.pages.first).data };

        for y in 0..s.rows as usize {
            for x in 0..s.cols as usize {
                let rac = page.get_row_and_cell(x, y);
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
        let page = unsafe { &(*s.pages.first).data };

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

#[test]
fn pagelist_increaseCapacity_to_increase_string_bytes() {
    let mut s = PageList::init(2, 2, Some(0)).unwrap();

    let original_cap = unsafe { (*s.pages.first).data.capacity.string_bytes };

    {
        assert!(core::ptr::eq(s.pages.first, s.pages.last));
        let page = unsafe { &mut (*s.pages.first).data };

        for y in 0..s.rows as usize {
            for x in 0..s.cols as usize {
                let rac = page.get_row_and_cell(x, y);
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
        let page = unsafe { &(*s.pages.first).data };

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

#[test]
fn pagelist_increaseCapacity_tracked_pins() {
    let mut s = PageList::init(2, 2, Some(0)).unwrap();

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

#[test]
fn pagelist_increaseCapacity_returns_OutOfSpace_at_max_capacity() {
    let mut s = PageList::init(2, 2, Some(0)).unwrap();

    // Keep increasing styles capacity until we get OutOfSpace
    let max_styles = size::StyleCountInt::MAX;
    loop {
        match s.increase_capacity(s.pages.first, Some(IncreaseCapacity::Styles)) {
            Ok(_) => {}
            Err(err) => {
                // Before OutOfSpace, we should have reached maxInt
                assert_eq!(IncreaseCapacityError::OutOfSpace, err);
                assert_eq!(max_styles, unsafe { (*s.pages.first).data.capacity.styles });
                break;
            }
        }
    }

    s.deinit();
}

#[test]
fn pagelist_increaseCapacity_multi_page() {
    let mut s = PageList::init(80, 24, None).unwrap();

    // Grow to create a second page
    let page1_node = s.pages.last;
    unsafe { (*page1_node).data.pause_integrity_checks(true) };
    for _ in 0..(unsafe { (*page1_node).data.capacity.rows - (*page1_node).data.size.rows }) {
        assert!(s.grow().unwrap().is_none());
    }
    unsafe { (*page1_node).data.pause_integrity_checks(false) };
    assert!(s.grow().unwrap().is_some());

    // Now we have two pages
    assert!(!core::ptr::eq(s.pages.first, s.pages.last));
    let page2_node = s.pages.last;

    let page1_styles_cap = unsafe { (*s.pages.first).data.capacity.styles };
    let page2_styles_cap = unsafe { (*page2_node).data.capacity.styles };

    // Increase capacity on the first page only
    let _ = s
        .increase_capacity(s.pages.first, Some(IncreaseCapacity::Styles))
        .unwrap();

    // First page capacity should be doubled
    assert_eq!(page1_styles_cap * 2, unsafe {
        (*s.pages.first).data.capacity.styles
    });

    // Second page should be unchanged
    assert_eq!(page2_styles_cap, unsafe {
        (*s.pages.last).data.capacity.styles
    });

    s.deinit();
}

#[test]
fn pagelist_increaseCapacity_preserves_dirty_flag() {
    let mut s = PageList::init(2, 4, Some(0)).unwrap();

    // Set page dirty flag and mark some rows as dirty
    let new_node = {
        let page = unsafe { &mut (*s.pages.first).data };
        page.dirty = true;

        let base = page.memory_ptr();
        let rows = unsafe { page.rows.ptr(base) };
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
    assert!(unsafe { (*new_node).data.dirty });

    // Row dirty flags should be preserved
    let new_rows = unsafe { (*new_node).data.rows.ptr((*new_node).data.memory_ptr()) };
    assert!(unsafe { (*new_rows.add(0)).dirty() });
    assert!(!unsafe { (*new_rows.add(1)).dirty() });
    assert!(unsafe { (*new_rows.add(2)).dirty() });
    assert!(!unsafe { (*new_rows.add(3)).dirty() });

    s.deinit();
}

// =====================================================================
// reset
// =====================================================================

#[test]
fn pagelist_reset() {
    let mut s = PageList::init(80, 24, None).unwrap();
    s.reset();
    assert!(s.viewport == super::Viewport::Active);
    assert!(!s.pages.first.is_null());
    assert_eq!(s.rows as usize, s.total_rows());

    // Active area should be the top
    expect_pin(
        Pin {
            node: s.pages.first,
            y: 0,
            x: 0,
            garbage: false,
        },
        s.get_top_left(point::Tag::Active),
    );

    s.deinit();
}

#[test]
fn pagelist_reset_invalidates_stale_untracked_refs_even_if_node_memory_is_reused() {
    let mut s = PageList::init(80, 24, None).unwrap();

    let old_serial = unsafe { (*s.pages.first).serial };
    assert!(old_serial >= s.page_serial_min);
    assert!(old_serial < s.page_serial);

    s.reset();

    // The important safety property is that stale serials are rejected before
    // the node pointer is inspected. Reset rebuilds the page list from the
    // pools, so old untracked refs may contain node pointers that are no
    // longer safe to dereference.
    assert!(old_serial < s.page_serial_min);

    let new_serial = unsafe { (*s.pages.first).serial };
    assert!(new_serial >= s.page_serial_min);
    assert!(new_serial < s.page_serial);

    s.deinit();
}

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
    let mut s = PageList::init(cap.cols, rows, None).unwrap();
    s.reset();
    assert!(s.viewport == super::Viewport::Active);
    assert!(!s.pages.first.is_null());
    assert_eq!(s.rows as usize, s.total_rows());

    s.deinit();
}

#[test]
fn pagelist_reset_moves_tracked_pins_and_marks_them_as_garbage() {
    let mut s = PageList::init(80, 24, None).unwrap();

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

#[test]
fn pagelist_clears_history() {
    let mut s = PageList::init(80, 24, None).unwrap();
    s.grow_rows(30).unwrap();
    s.reset();
    assert!(s.viewport == super::Viewport::Active);
    assert!(!s.pages.first.is_null());
    assert_eq!(s.rows as usize, s.total_rows());

    // Active area should be the top
    expect_pin(
        Pin {
            node: s.pages.first,
            y: 0,
            x: 0,
            garbage: false,
        },
        s.get_top_left(point::Tag::Active),
    );

    s.deinit();
}

// =====================================================================
// grow non-standard / compact
// =====================================================================

#[test]
fn pagelist_grow_reuses_non_standard_page_without_leak() {
    // Create a PageList with 3 * std_size max so we can fit multiple pages
    // but will still trigger reuse.
    let mut s = PageList::init(80, 24, Some(3 * std_size())).unwrap();

    // Increase the first page capacity to make it non-standard (larger than std_size).
    while unsafe { (*s.pages.first).data.memory_len } <= std_size() {
        let _ = s
            .increase_capacity(s.pages.first, Some(IncreaseCapacity::GraphemeBytes))
            .unwrap();
    }

    // The first page should now have non-standard memory size.
    assert!(unsafe { (*s.pages.first).data.memory_len } > std_size());

    // First, fill up the first page's capacity
    let first_page = s.pages.first;
    while unsafe { (*first_page).data.size.rows < (*first_page).data.capacity.rows } {
        let _ = s.grow().unwrap();
    }

    // Now grow to create a second page
    let _ = s.grow().unwrap();
    assert!(!core::ptr::eq(s.pages.first, s.pages.last));

    // Continue growing until we exceed max_size AND the last page is full
    while s.page_size + PagePool::item_size() <= s.max_size()
        || unsafe { (*s.pages.last).data.size.rows < (*s.pages.last).data.capacity.rows }
    {
        let _ = s.grow().unwrap();
    }

    // The first page should still be non-standard
    assert!(unsafe { (*s.pages.first).data.memory_len } > std_size());

    // Verify we have enough rows for active area (so prune path isn't skipped)
    assert!(s.total_rows() >= s.rows as usize);

    // Verify last page is full (so grow will need to allocate/reuse)
    assert!(unsafe { (*s.pages.last).data.size.rows == (*s.pages.last).data.capacity.rows });

    // Remember the first page memory pointer before the reuse attempt
    let first_page_ptr = s.pages.first;
    let first_page_mem_ptr = unsafe { (*s.pages.first).data.memory_ptr() };

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
        unsafe { (*s.pages.last).data.memory_ptr() },
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
    let mut s = PageList::init(80, rows_count, Some(std_size())).unwrap();

    // Make the first page non-standard
    while unsafe { (*s.pages.first).data.memory_len } <= std_size() {
        let _ = s
            .increase_capacity(s.pages.first, Some(IncreaseCapacity::GraphemeBytes))
            .unwrap();
    }
    assert!(unsafe { (*s.pages.first).data.memory_len } > std_size());

    let first_page_node = s.pages.first;
    let first_page_cap = unsafe { (*first_page_node).data.capacity.rows };

    // Fill first page to capacity
    while unsafe { (*first_page_node).data.size.rows } < first_page_cap {
        let _ = s.grow().unwrap();
    }

    // Grow until we have a second page (first page fills up first)
    while core::ptr::eq(s.pages.first, s.pages.last) {
        let _ = s.grow().unwrap();
    }
    assert!(!core::ptr::eq(s.pages.first, s.pages.last));

    // Fill the second page to capacity so that the next grow() triggers prune
    let last_node = s.pages.last;
    let second_cap = unsafe { (*last_node).data.capacity.rows };
    while unsafe { (*last_node).data.size.rows } < second_cap {
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
    assert!(s.page_size + PagePool::item_size() > s.max_size());
    assert!(s.total_rows() >= s.rows as usize);

    // Verify last page is at capacity (so grow must prune or allocate new)
    assert_eq!(second_cap, unsafe { (*last_node).data.size.rows });

    // The next grow should trigger prune consideration.
    // Without the fix, this would destroy the non-standard first page,
    // leaving only second_cap + 1 rows, which is < self.rows.
    let _ = s.grow().unwrap();

    // Verify the invariant holds - the fix prevents the destructive prune
    assert!(s.total_rows() >= s.rows as usize);

    s.deinit();
}

#[test]
fn pagelist_compact_std_size_page_returns_null() {
    let mut s = PageList::init(80, 24, Some(0)).unwrap();

    // A freshly created page should be at std_size
    let node = s.pages.first;
    assert!(unsafe { (*node).data.memory_len } <= std_size());

    // compact should return null since there's nothing to compact
    let result = s.compact(node).unwrap();
    assert!(result.is_none());

    // Page should still be the same
    assert!(core::ptr::eq(node, s.pages.first));

    s.deinit();
}

#[test]
fn pagelist_compact_oversized_page() {
    let mut s = PageList::init(80, 24, None).unwrap();

    // Grow until we have multiple pages
    let page1_node = s.pages.first;
    unsafe { (*page1_node).data.pause_integrity_checks(true) };
    for _ in 0..(unsafe { (*page1_node).data.capacity.rows - (*page1_node).data.size.rows }) {
        let _ = s.grow().unwrap();
    }
    unsafe { (*page1_node).data.pause_integrity_checks(false) };
    let _ = s.grow().unwrap();
    assert!(!core::ptr::eq(s.pages.first, s.pages.last));

    let mut node = s.pages.first;

    // Write content to verify it's preserved
    {
        let page = unsafe { &mut (*node).data };
        for y in 0..page.size.rows as usize {
            for x in 0..s.cols as usize {
                let rac = page.get_row_and_cell(x, y);
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
    while unsafe { (*node).data.memory_len } <= std_size() {
        node = s
            .increase_capacity(node, Some(IncreaseCapacity::GraphemeBytes))
            .unwrap();
    }
    assert!(unsafe { (*node).data.memory_len } > std_size());
    let oversized_len = unsafe { (*node).data.memory_len };
    let original_size = unsafe { (*node).data.size };
    let second_node = unsafe { (*node).next };

    // Set dirty flag after increaseCapacity
    unsafe { (*node).data.dirty = true };

    // Compact the page
    let new_node = s.compact(node).unwrap();
    assert!(new_node.is_some());
    let new_node = new_node.unwrap();

    // Verify memory is smaller
    assert!(unsafe { (*new_node).data.memory_len } < oversized_len);

    // Verify size preserved
    assert_eq!(original_size.rows, unsafe { (*new_node).data.size.rows });
    assert_eq!(original_size.cols, unsafe { (*new_node).data.size.cols });

    // Verify dirty flag preserved
    assert!(unsafe { (*new_node).data.dirty });

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
        let page = unsafe { &(*new_node).data };
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

#[test]
fn pagelist_compact_insufficient_savings_returns_null() {
    let mut s = PageList::init(80, 24, Some(0)).unwrap();

    let mut node = s.pages.first;

    // Make the page slightly oversized (just one increase)
    // This might not provide enough savings to justify compaction
    node = s
        .increase_capacity(node, Some(IncreaseCapacity::GraphemeBytes))
        .unwrap();

    // If the page is still at or below std_size, compact returns null
    if unsafe { (*node).data.memory_len } <= std_size() {
        let result = s.compact(node).unwrap();
        assert!(result.is_none());
    } else {
        // If it did grow beyond std_size, verify that compaction
        // works or returns null based on savings calculation
        let result = s.compact(node).unwrap();
        // Either it compacted or determined insufficient savings
        if let Some(new_node) = result {
            assert!(unsafe { (*new_node).data.memory_len } < unsafe { (*node).data.memory_len });
        }
    }

    s.deinit();
}

// =====================================================================
// split
// =====================================================================

#[test]
fn pagelist_split_at_middle_row() {
    let mut s = PageList::init(10, 10, Some(0)).unwrap();

    {
        let page = unsafe { &mut (*s.pages.first).data };

        // Write content to rows: row 0 gets codepoint 0, row 1 gets 1, etc.
        for y in 0..page.size.rows as usize {
            let rac = page.get_row_and_cell(0, y);
            unsafe {
                *rac.cell = cell_codepoint(y as u32);
            }
        }
    }

    // Split at row 5 (middle)
    let split_pin = Pin {
        node: s.pages.first,
        y: 5,
        x: 0,
        garbage: false,
    };
    s.split(split_pin).unwrap();

    // Verify two pages exist
    assert!(!s.pages.first.is_null());
    assert!(!unsafe { (*s.pages.first).next }.is_null());

    let first_page = unsafe { &(*s.pages.first).data };
    let second_page = unsafe { &(*(*s.pages.first).next).data };

    // First page should have rows 0-4 (5 rows)
    assert_eq!(5, first_page.size.rows as usize);
    // Second page should have rows 5-9 (5 rows)
    assert_eq!(5, second_page.size.rows as usize);

    // Verify content in first page is preserved (rows 0-4 have codepoints 0-4)
    for y in 0..5 {
        let rac = first_page.get_row_and_cell(0, y);
        assert_eq!(y as u32, unsafe { (*rac.cell).content_codepoint() });
    }

    // Verify content in second page (original rows 5-9, now at y=0-4)
    for y in 0..5 {
        let rac = second_page.get_row_and_cell(0, y);
        assert_eq!((y + 5) as u32, unsafe { (*rac.cell).content_codepoint() });
    }

    s.deinit();
}

#[test]
fn pagelist_split_at_row_0_is_no_op() {
    let mut s = PageList::init(10, 10, Some(0)).unwrap();

    let page = unsafe { &mut (*s.pages.first).data };

    // Write content to all rows
    for y in 0..page.size.rows as usize {
        let rac = page.get_row_and_cell(0, y);
        unsafe {
            *rac.cell = cell_codepoint(y as u32);
        }
    }

    // Split at row 0 should be a no-op
    let split_pin = Pin {
        node: s.pages.first,
        y: 0,
        x: 0,
        garbage: false,
    };
    s.split(split_pin).unwrap();

    // Verify only one page exists (no split occurred)
    assert!(!s.pages.first.is_null());
    assert!(unsafe { (*s.pages.first).next }.is_null());

    // Verify all content is still in the original page
    let page = unsafe { &(*s.pages.first).data };
    assert_eq!(10, page.size.rows as usize);
    for y in 0..10 {
        let rac = page.get_row_and_cell(0, y);
        assert_eq!(y as u32, unsafe { (*rac.cell).content_codepoint() });
    }

    s.deinit();
}

#[test]
fn pagelist_split_at_last_row() {
    let mut s = PageList::init(10, 10, Some(0)).unwrap();

    {
        let page = unsafe { &mut (*s.pages.first).data };

        // Write content to all rows
        for y in 0..page.size.rows as usize {
            let rac = page.get_row_and_cell(0, y);
            unsafe {
                *rac.cell = cell_codepoint(y as u32);
            }
        }
    }

    // Split at last row (row 9)
    let split_pin = Pin {
        node: s.pages.first,
        y: 9,
        x: 0,
        garbage: false,
    };
    s.split(split_pin).unwrap();

    // Verify two pages exist
    assert!(!s.pages.first.is_null());
    assert!(!unsafe { (*s.pages.first).next }.is_null());

    let first_page = unsafe { &(*s.pages.first).data };
    let second_page = unsafe { &(*(*s.pages.first).next).data };

    // First page should have 9 rows
    assert_eq!(9, first_page.size.rows as usize);
    // Second page should have 1 row
    assert_eq!(1, second_page.size.rows as usize);

    // Verify content in second page (original row 9, now at y=0)
    let rac = second_page.get_row_and_cell(0, 0);
    assert_eq!(9, unsafe { (*rac.cell).content_codepoint() });

    s.deinit();
}

#[test]
fn pagelist_split_single_row_page_returns_OutOfSpace() {
    // Initialize with 1 row
    let mut s = PageList::init(10, 1, Some(0)).unwrap();

    let split_pin = Pin {
        node: s.pages.first,
        y: 0,
        x: 0,
        garbage: false,
    };
    let result = s.split(split_pin);

    assert_eq!(Err(SplitError::OutOfSpace), result);

    s.deinit();
}

#[test]
fn pagelist_split_moves_tracked_pins() {
    let mut s = PageList::init(10, 10, Some(0)).unwrap();

    // Track a pin at row 7
    let tracked = s
        .track_pin(Pin {
            node: s.pages.first,
            y: 7,
            x: 3,
            garbage: false,
        })
        .unwrap();

    // Split at row 5
    let split_pin = Pin {
        node: s.pages.first,
        y: 5,
        x: 0,
        garbage: false,
    };
    s.split(split_pin).unwrap();

    // The tracked pin should now be in the second page
    assert!(core::ptr::eq(unsafe { (*tracked).node }, unsafe {
        (*s.pages.first).next
    }));
    // y should be adjusted: was 7, split at 5, so new y = 7 - 5 = 2
    assert_eq!(2, unsafe { (*tracked).y } as usize);
    // x should remain unchanged
    assert_eq!(3, unsafe { (*tracked).x } as usize);

    s.untrack_pin(tracked);
    s.deinit();
}

#[test]
fn pagelist_split_tracked_pin_before_split_point_unchanged() {
    let mut s = PageList::init(10, 10, Some(0)).unwrap();

    let original_node = s.pages.first;

    // Track a pin at row 2 (before the split point)
    let tracked = s
        .track_pin(Pin {
            node: original_node,
            y: 2,
            x: 5,
            garbage: false,
        })
        .unwrap();

    // Split at row 5
    let split_pin = Pin {
        node: original_node,
        y: 5,
        x: 0,
        garbage: false,
    };
    s.split(split_pin).unwrap();

    // The tracked pin should remain in the original page
    assert!(core::ptr::eq(unsafe { (*tracked).node }, s.pages.first));
    // y and x should be unchanged
    assert_eq!(2, unsafe { (*tracked).y } as usize);
    assert_eq!(5, unsafe { (*tracked).x } as usize);

    s.untrack_pin(tracked);
    s.deinit();
}

#[test]
fn pagelist_split_tracked_pin_at_split_point_moves_to_new_page() {
    let mut s = PageList::init(10, 10, Some(0)).unwrap();

    let original_node = s.pages.first;

    // Track a pin at the exact split point (row 5)
    let tracked = s
        .track_pin(Pin {
            node: original_node,
            y: 5,
            x: 4,
            garbage: false,
        })
        .unwrap();

    // Split at row 5
    let split_pin = Pin {
        node: original_node,
        y: 5,
        x: 0,
        garbage: false,
    };
    s.split(split_pin).unwrap();

    // The tracked pin should be in the new page
    assert!(core::ptr::eq(unsafe { (*tracked).node }, unsafe {
        (*s.pages.first).next
    }));
    // y should be 0 since it was at the split point: 5 - 5 = 0
    assert_eq!(0, unsafe { (*tracked).y } as usize);
    // x should remain unchanged
    assert_eq!(4, unsafe { (*tracked).x } as usize);

    s.untrack_pin(tracked);
    s.deinit();
}

#[test]
fn pagelist_split_multiple_tracked_pins_across_regions() {
    let mut s = PageList::init(10, 10, Some(0)).unwrap();

    let original_node = s.pages.first;

    // Track multiple pins in different regions
    let pin_before = s
        .track_pin(Pin {
            node: original_node,
            y: 1,
            x: 0,
            garbage: false,
        })
        .unwrap();
    let pin_at_split = s
        .track_pin(Pin {
            node: original_node,
            y: 5,
            x: 2,
            garbage: false,
        })
        .unwrap();
    let pin_after1 = s
        .track_pin(Pin {
            node: original_node,
            y: 7,
            x: 3,
            garbage: false,
        })
        .unwrap();
    let pin_after2 = s
        .track_pin(Pin {
            node: original_node,
            y: 9,
            x: 8,
            garbage: false,
        })
        .unwrap();

    // Split at row 5
    let split_pin = Pin {
        node: original_node,
        y: 5,
        x: 0,
        garbage: false,
    };
    s.split(split_pin).unwrap();

    let first_page = s.pages.first;
    let second_page = unsafe { (*first_page).next };

    // Pin before split point stays in original page
    assert!(core::ptr::eq(unsafe { (*pin_before).node }, first_page));
    assert_eq!(1, unsafe { (*pin_before).y } as usize);
    assert_eq!(0, unsafe { (*pin_before).x } as usize);

    // Pin at split point moves to new page with y=0
    assert!(core::ptr::eq(unsafe { (*pin_at_split).node }, second_page));
    assert_eq!(0, unsafe { (*pin_at_split).y } as usize);
    assert_eq!(2, unsafe { (*pin_at_split).x } as usize);

    // Pins after split point move to new page with adjusted y
    assert!(core::ptr::eq(unsafe { (*pin_after1).node }, second_page));
    assert_eq!(2, unsafe { (*pin_after1).y } as usize); // 7 - 5 = 2
    assert_eq!(3, unsafe { (*pin_after1).x } as usize);

    assert!(core::ptr::eq(unsafe { (*pin_after2).node }, second_page));
    assert_eq!(4, unsafe { (*pin_after2).y } as usize); // 9 - 5 = 4
    assert_eq!(8, unsafe { (*pin_after2).x } as usize);

    s.untrack_pin(pin_before);
    s.untrack_pin(pin_at_split);
    s.untrack_pin(pin_after1);
    s.untrack_pin(pin_after2);
    s.deinit();
}

#[test]
fn pagelist_split_tracked_viewport_pin_in_split_region_moves_correctly() {
    let mut s = PageList::init(10, 10, Some(0)).unwrap();

    let original_node = s.pages.first;

    // Set viewport_pin to row 7 (after split point)
    unsafe {
        (*s.viewport_pin).node = original_node;
        (*s.viewport_pin).y = 7;
        (*s.viewport_pin).x = 6;
    }

    // Split at row 5
    let split_pin = Pin {
        node: original_node,
        y: 5,
        x: 0,
        garbage: false,
    };
    s.split(split_pin).unwrap();

    // viewport_pin should be in the new page
    assert!(core::ptr::eq(unsafe { (*s.viewport_pin).node }, unsafe {
        (*s.pages.first).next
    }));
    // y should be adjusted: 7 - 5 = 2
    assert_eq!(2, unsafe { (*s.viewport_pin).y } as usize);
    // x should remain unchanged
    assert_eq!(6, unsafe { (*s.viewport_pin).x } as usize);

    s.deinit();
}

#[test]
fn pagelist_split_middle_page_preserves_linked_list_order() {
    // Create a single page with 12 rows
    let mut s = PageList::init(10, 12, Some(0)).unwrap();

    // Split at row 4 to create: page1 (rows 0-3), page2 (rows 4-11)
    let first_node = s.pages.first;
    let split_pin1 = Pin {
        node: first_node,
        y: 4,
        x: 0,
        garbage: false,
    };
    s.split(split_pin1).unwrap();

    // Now we have 2 pages
    let page1 = s.pages.first;
    let page2 = unsafe { (*s.pages.first).next };
    assert_eq!(4, unsafe { (*page1).data.size.rows } as usize);
    assert_eq!(8, unsafe { (*page2).data.size.rows } as usize);

    // Split page2 at row 4 to create: page1 -> page2 (rows 0-3) -> page3 (rows 4-7)
    let split_pin2 = Pin {
        node: page2,
        y: 4,
        x: 0,
        garbage: false,
    };
    s.split(split_pin2).unwrap();

    // Now we have 3 pages
    let first = s.pages.first;
    let middle = unsafe { (*first).next };
    let last = unsafe { (*middle).next };

    // Verify linked list order: first -> middle -> last
    assert!(core::ptr::eq(page1, first));
    assert!(core::ptr::eq(page2, middle));
    assert!(core::ptr::eq(s.pages.last, last));

    // Verify prev pointers
    assert!(unsafe { (*first).prev }.is_null());
    assert!(core::ptr::eq(first, unsafe { (*middle).prev }));
    assert!(core::ptr::eq(middle, unsafe { (*last).prev }));

    // Verify next pointers
    assert!(core::ptr::eq(middle, unsafe { (*first).next }));
    assert!(core::ptr::eq(last, unsafe { (*middle).next }));
    assert!(unsafe { (*last).next }.is_null());

    // Verify row counts
    assert_eq!(4, unsafe { (*first).data.size.rows } as usize);
    assert_eq!(4, unsafe { (*middle).data.size.rows } as usize);
    assert_eq!(4, unsafe { (*last).data.size.rows } as usize);

    s.deinit();
}

#[test]
fn pagelist_split_last_page_makes_new_page_the_last() {
    // Create a single page with 10 rows
    let mut s = PageList::init(10, 10, Some(0)).unwrap();

    // Split to create 2 pages first
    let first_node = s.pages.first;
    let split_pin1 = Pin {
        node: first_node,
        y: 5,
        x: 0,
        garbage: false,
    };
    s.split(split_pin1).unwrap();

    // Now split the last page
    let last_before_split = s.pages.last;
    assert_eq!(5, unsafe { (*last_before_split).data.size.rows } as usize);

    let split_pin2 = Pin {
        node: last_before_split,
        y: 2,
        x: 0,
        garbage: false,
    };
    s.split(split_pin2).unwrap();

    // The new page should be the new last
    let new_last = s.pages.last;
    assert!(!core::ptr::eq(new_last, last_before_split));
    assert!(core::ptr::eq(last_before_split, unsafe {
        (*new_last).prev
    }));
    assert!(unsafe { (*new_last).next }.is_null());

    // Verify row counts: original last has 2 rows, new last has 3 rows
    assert_eq!(2, unsafe { (*last_before_split).data.size.rows } as usize);
    assert_eq!(3, unsafe { (*new_last).data.size.rows } as usize);

    s.deinit();
}

#[test]
fn pagelist_split_first_page_keeps_original_as_first() {
    // Create 2 pages by splitting
    let mut s = PageList::init(10, 10, Some(0)).unwrap();

    let original_first = s.pages.first;
    let split_pin1 = Pin {
        node: original_first,
        y: 5,
        x: 0,
        garbage: false,
    };
    s.split(split_pin1).unwrap();

    // Get second page (created by first split)
    let second_page = unsafe { (*s.pages.first).next };

    // Now split the first page again
    let split_pin2 = Pin {
        node: s.pages.first,
        y: 2,
        x: 0,
        garbage: false,
    };
    s.split(split_pin2).unwrap();

    // Original first should still be first
    assert!(core::ptr::eq(original_first, s.pages.first));
    assert!(unsafe { (*s.pages.first).prev }.is_null());

    // New page should be inserted between first and second
    let inserted = unsafe { (*s.pages.first).next };
    assert!(!core::ptr::eq(inserted, second_page));
    assert!(core::ptr::eq(second_page, unsafe { (*inserted).next }));

    // Verify row counts: first has 2, inserted has 3, second has 5
    assert_eq!(2, unsafe { (*s.pages.first).data.size.rows } as usize);
    assert_eq!(3, unsafe { (*inserted).data.size.rows } as usize);
    assert_eq!(5, unsafe { (*second_page).data.size.rows } as usize);

    s.deinit();
}

#[test]
fn pagelist_split_preserves_wrap_flags() {
    let mut s = PageList::init(10, 10, Some(0)).unwrap();

    {
        let page = unsafe { &mut (*s.pages.first).data };

        // Set wrap flags on rows that will be in the second page after split
        // Row 5: wrap = true (this is the start of a wrapped line)
        // Row 6: wrap_continuation = true (this continues the wrap)
        // Row 7: wrap = true, wrap_continuation = true (wrapped and continues)
        {
            let rac5 = page.get_row_and_cell(0, 5);
            unsafe { (*rac5.row).set_wrap(true) };

            let rac6 = page.get_row_and_cell(0, 6);
            unsafe { (*rac6.row).set_wrap_continuation(true) };

            let rac7 = page.get_row_and_cell(0, 7);
            unsafe {
                (*rac7.row).set_wrap(true);
                (*rac7.row).set_wrap_continuation(true);
            }
        }
    }

    // Split at row 5
    let split_pin = Pin {
        node: s.pages.first,
        y: 5,
        x: 0,
        garbage: false,
    };
    s.split(split_pin).unwrap();

    let second_page = unsafe { &(*(*s.pages.first).next).data };

    // Verify wrap flags are preserved in new page
    // Original row 5 is now row 0 in second page
    {
        let rac0 = second_page.get_row_and_cell(0, 0);
        assert!(unsafe { (*rac0.row).wrap() });
        assert!(!unsafe { (*rac0.row).wrap_continuation() });
    }

    // Original row 6 is now row 1 in second page
    {
        let rac1 = second_page.get_row_and_cell(0, 1);
        assert!(!unsafe { (*rac1.row).wrap() });
        assert!(unsafe { (*rac1.row).wrap_continuation() });
    }

    // Original row 7 is now row 2 in second page
    {
        let rac2 = second_page.get_row_and_cell(0, 2);
        assert!(unsafe { (*rac2.row).wrap() });
        assert!(unsafe { (*rac2.row).wrap_continuation() });
    }

    s.deinit();
}

#[test]
fn pagelist_split_preserves_styled_cells() {
    let mut s = PageList::init(10, 10, Some(0)).unwrap();

    {
        let page = unsafe { &mut (*s.pages.first).data };

        // Create a style and apply it to cells in rows 5-7 (which will be in the second page)
        let style = Style {
            flags: stylepkg::Flags {
                bold: true,
                ..stylepkg::Flags::default()
            },
            ..Style::default()
        };
        let base = page.memory_ptr();
        let style_id = unsafe { page.styles.add(base, style) }.unwrap();

        for y in 5..8 {
            let rac = page.get_row_and_cell(0, y);
            unsafe {
                let mut c = cell_codepoint(b'S' as u32);
                c.set_style_id(style_id);
                *rac.cell = c;
                (*rac.row).set_styled(true);
                page.styles.use_id(base, style_id);
            }
        }
        // Release the extra ref from add
        unsafe { page.styles.release(base, style_id) };
    }

    // Split at row 5
    let split_pin = Pin {
        node: s.pages.first,
        y: 5,
        x: 0,
        garbage: false,
    };
    s.split(split_pin).unwrap();

    let first_page = unsafe { &(*s.pages.first).data };
    let second_page = unsafe { &(*(*s.pages.first).next).data };

    // First page should have no styles (all styled rows moved to second page)
    assert_eq!(0, first_page.styles.count());

    // Second page should have exactly 1 style (the bold style, used by 3 cells)
    assert_eq!(1, second_page.styles.count());

    // Verify styled cells are preserved in new page
    for y in 0..3 {
        let rac = second_page.get_row_and_cell(0, y);
        assert_eq!(b'S' as u32, unsafe { (*rac.cell).content_codepoint() });
        assert!(unsafe { (*rac.cell).style_id() } != 0);

        let got_style = unsafe {
            &*second_page
                .styles
                .get(second_page.memory_ptr(), (*rac.cell).style_id())
        };
        assert!(got_style.flags.bold);
        assert!(unsafe { (*rac.row).styled() });
    }

    s.deinit();
}

#[test]
fn pagelist_split_preserves_grapheme_clusters() {
    let mut s = PageList::init(10, 10, Some(0)).unwrap();

    {
        let page = unsafe { &mut (*s.pages.first).data };

        // Add a grapheme cluster to row 6 (will be row 1 in second page after split at 5)
        {
            let rac = page.get_row_and_cell(0, 6);
            unsafe {
                *rac.cell = cell_codepoint(0x1F468); // Man emoji
            }
            page.set_graphemes(
                rac.row,
                rac.cell,
                &[
                    0x200D,  // ZWJ
                    0x1F469, // Woman emoji
                ],
            )
            .unwrap();
        }
    }

    // Split at row 5
    let split_pin = Pin {
        node: s.pages.first,
        y: 5,
        x: 0,
        garbage: false,
    };
    s.split(split_pin).unwrap();

    let first_page = unsafe { &(*s.pages.first).data };
    let second_page = unsafe { &(*(*s.pages.first).next).data };

    // First page should have no graphemes (the grapheme row moved to second page)
    assert_eq!(0, first_page.grapheme_count());

    // Second page should have exactly 1 grapheme
    assert_eq!(1, second_page.grapheme_count());

    // Verify grapheme is preserved in new page (original row 6 is now row 1)
    {
        let rac = second_page.get_row_and_cell(0, 1);
        assert_eq!(0x1F468, unsafe { (*rac.cell).content_codepoint() });
        assert!(unsafe { (*rac.row).grapheme() });

        let cps = second_page.lookup_grapheme(rac.cell).unwrap();
        assert_eq!(2, cps.len());
        assert_eq!(0x200D, cps[0]);
        assert_eq!(0x1F469, cps[1]);
    }

    s.deinit();
}

#[test]
fn pagelist_split_preserves_hyperlinks() {
    let mut s = PageList::init(10, 10, Some(0)).unwrap();

    let hyperlink_id = {
        let page = unsafe { &mut (*s.pages.first).data };

        // Add a hyperlink to row 7 (will be row 2 in second page after split at 5)
        let hyperlink_id = page
            .insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Implicit(0),
                uri: b"https://example.com".to_vec(),
            })
            .unwrap();
        {
            let rac = page.get_row_and_cell(0, 7);
            unsafe {
                *rac.cell = cell_codepoint(b'L' as u32);
            }
            page.set_hyperlink(rac.row, rac.cell, hyperlink_id).unwrap();
        }
        hyperlink_id
    };
    let _ = hyperlink_id;

    // Split at row 5
    let split_pin = Pin {
        node: s.pages.first,
        y: 5,
        x: 0,
        garbage: false,
    };
    s.split(split_pin).unwrap();

    let first_page = unsafe { &(*s.pages.first).data };
    let second_page = unsafe { &(*(*s.pages.first).next).data };

    // First page should have no hyperlinks (the hyperlink row moved to second page)
    assert_eq!(0, first_page.hyperlink_set.count());

    // Second page should have exactly 1 hyperlink
    assert_eq!(1, second_page.hyperlink_set.count());

    // Verify hyperlink is preserved in new page (original row 7 is now row 2)
    {
        let rac = second_page.get_row_and_cell(0, 2);
        assert_eq!(b'L' as u32, unsafe { (*rac.cell).content_codepoint() });
        assert!(unsafe { (*rac.cell).hyperlink() });

        let link_id = second_page.lookup_hyperlink(rac.cell).unwrap();
        let link = unsafe {
            &*second_page
                .hyperlink_set
                .get(second_page.memory_ptr(), link_id)
        };
        assert_eq!(&b"https://example.com"[..], unsafe {
            link.uri.slice(second_page.memory_ptr())
        });
    }

    s.deinit();
}

// ref: test "PageList scroll delta row back without space preserves active"
#[test]
fn pagelist_scroll_delta_row_back_without_space_preserves_active() {
    let mut s = PageList::init(80, 24, None).unwrap();
    s.scroll(Scroll::DeltaRow(-1));

    {
        let pt = s
            .get_cell(point::Point::Viewport(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 0 }), pt);
    }

    assert!(s.viewport == super::Viewport::Active);

    assert_eq!(
        Scrollbar {
            total: s.total_rows(),
            offset: s.total_rows - s.rows as usize,
            len: s.rows as usize,
        },
        s.scrollbar()
    );

    s.deinit();
}

// ref: test "PageList scroll to pin"
#[test]
fn pagelist_scroll_to_pin() {
    let mut s = PageList::init(80, 24, None).unwrap();
    s.grow_rows(10).unwrap();

    s.scroll(Scroll::Pin(
        s.pin(point::Point::Screen(point::Coordinate { x: 2, y: 4 }))
            .unwrap(),
    ));

    assert_eq!(
        Scrollbar {
            total: s.total_rows(),
            offset: 4,
            len: s.rows as usize,
        },
        s.scrollbar()
    );

    {
        let pt = s
            .get_cell(point::Point::Viewport(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 4 }), pt);
    }

    s.scroll(Scroll::Pin(
        s.pin(point::Point::Screen(point::Coordinate { x: 2, y: 5 }))
            .unwrap(),
    ));

    assert_eq!(
        Scrollbar {
            total: s.total_rows(),
            offset: 5,
            len: s.rows as usize,
        },
        s.scrollbar()
    );

    {
        let pt = s
            .get_cell(point::Point::Viewport(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 5 }), pt);
    }

    s.deinit();
}

// ref: test "PageList scroll to pin in active"
#[test]
fn pagelist_scroll_to_pin_in_active() {
    let mut s = PageList::init(80, 24, None).unwrap();
    s.grow_rows(10).unwrap();

    s.scroll(Scroll::Pin(
        s.pin(point::Point::Screen(point::Coordinate { x: 2, y: 30 }))
            .unwrap(),
    ));

    assert_eq!(
        Scrollbar {
            total: s.total_rows(),
            offset: s.total_rows - s.rows as usize,
            len: s.rows as usize,
        },
        s.scrollbar()
    );

    {
        let pt = s
            .get_cell(point::Point::Viewport(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 10 }), pt);
    }

    s.deinit();
}

// ref: test "PageList scroll to pin at top"
#[test]
fn pagelist_scroll_to_pin_at_top() {
    let mut s = PageList::init(80, 24, None).unwrap();
    s.grow_rows(10).unwrap();

    s.scroll(Scroll::Pin(
        s.pin(point::Point::Screen(point::Coordinate { x: 2, y: 0 }))
            .unwrap(),
    ));

    assert!(s.viewport == super::Viewport::Top);

    assert_eq!(
        Scrollbar {
            total: s.total_rows(),
            offset: 0,
            len: s.rows as usize,
        },
        s.scrollbar()
    );

    {
        let pt = s
            .get_cell(point::Point::Viewport(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 0 }), pt);
    }

    s.deinit();
}

// ref: test "PageList scroll to row 0"
#[test]
fn pagelist_scroll_to_row_0() {
    let mut s = PageList::init(80, 24, None).unwrap();
    s.grow_rows(10).unwrap();

    {
        let pt = s
            .get_cell(point::Point::Viewport(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 10 }), pt);
    }

    s.scroll(Scroll::Row(0));
    assert!(s.viewport == super::Viewport::Top);

    {
        let pt = s
            .get_cell(point::Point::Viewport(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 0 }), pt);
    }

    assert_eq!(
        Scrollbar {
            total: s.total_rows,
            offset: 0,
            len: s.rows as usize,
        },
        s.scrollbar()
    );

    s.grow_rows(10).unwrap();
    {
        let pt = s
            .get_cell(point::Point::Viewport(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 0 }), pt);
    }

    assert_eq!(
        Scrollbar {
            total: s.total_rows,
            offset: 0,
            len: s.rows as usize,
        },
        s.scrollbar()
    );

    s.deinit();
}

// ref: test "PageList scroll to row in scrollback"
#[test]
fn pagelist_scroll_to_row_in_scrollback() {
    let mut s = PageList::init(80, 24, None).unwrap();
    s.grow_rows(20).unwrap();

    {
        let pt = s
            .get_cell(point::Point::Viewport(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 20 }), pt);
    }

    s.scroll(Scroll::Row(5));
    assert!(s.viewport == super::Viewport::Pin);
    assert_eq!(
        Scrollbar {
            total: s.total_rows,
            offset: 5,
            len: s.rows as usize,
        },
        s.scrollbar()
    );

    {
        let pt = s
            .get_cell(point::Point::Viewport(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 5 }), pt);
    }

    s.grow_rows(10).unwrap();
    {
        let pt = s
            .get_cell(point::Point::Viewport(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 5 }), pt);
    }

    assert_eq!(
        Scrollbar {
            total: s.total_rows,
            offset: 5,
            len: s.rows as usize,
        },
        s.scrollbar()
    );

    s.deinit();
}

// ref: test "PageList scroll to row in middle"
#[test]
fn pagelist_scroll_to_row_in_middle() {
    let mut s = PageList::init(80, 24, None).unwrap();
    s.grow_rows(50).unwrap();

    let total = s.total_rows;
    let midpoint = total / 2;
    s.scroll(Scroll::Row(midpoint));

    assert!(s.viewport == super::Viewport::Pin);
    assert_eq!(
        Scrollbar {
            total: s.total_rows,
            offset: midpoint,
            len: s.rows as usize,
        },
        s.scrollbar()
    );

    {
        let pt = s
            .get_cell(point::Point::Viewport(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(
            point::Point::Screen(point::Coordinate {
                x: 0,
                y: midpoint as u32,
            }),
            pt
        );
    }

    s.grow_rows(10).unwrap();
    {
        let pt = s
            .get_cell(point::Point::Viewport(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(
            point::Point::Screen(point::Coordinate {
                x: 0,
                y: midpoint as u32,
            }),
            pt
        );
    }

    assert_eq!(
        Scrollbar {
            total: s.total_rows,
            offset: midpoint,
            len: s.rows as usize,
        },
        s.scrollbar()
    );

    s.deinit();
}

// ref: test "PageList scroll to row at active boundary"
#[test]
fn pagelist_scroll_to_row_at_active_boundary() {
    let mut s = PageList::init(80, 24, None).unwrap();
    s.grow_rows(20).unwrap();

    let active_start = s.total_rows - s.rows as usize;

    s.scroll(Scroll::Row(active_start));

    assert!(s.viewport == super::Viewport::Active);

    {
        let pt = s
            .get_cell(point::Point::Viewport(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(
            point::Point::Screen(point::Coordinate {
                x: 0,
                y: active_start as u32,
            }),
            pt
        );
    }

    assert_eq!(
        Scrollbar {
            total: s.total_rows,
            offset: s.total_rows - s.rows as usize,
            len: s.rows as usize,
        },
        s.scrollbar()
    );

    s.grow_rows(10).unwrap();

    assert!(s.viewport == super::Viewport::Active);

    assert_eq!(
        Scrollbar {
            total: s.total_rows,
            offset: s.total_rows - s.rows as usize,
            len: s.rows as usize,
        },
        s.scrollbar()
    );

    s.deinit();
}

// ref: test "PageList scroll to row beyond active"
#[test]
fn pagelist_scroll_to_row_beyond_active() {
    let mut s = PageList::init(80, 24, None).unwrap();
    s.grow_rows(10).unwrap();

    s.scroll(Scroll::Row(1000));

    assert!(s.viewport == super::Viewport::Active);

    {
        let pt = s
            .get_cell(point::Point::Viewport(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 10 }), pt);
    }

    assert_eq!(
        Scrollbar {
            total: s.total_rows,
            offset: s.total_rows - s.rows as usize,
            len: s.rows as usize,
        },
        s.scrollbar()
    );

    s.deinit();
}

// ref: test "PageList scroll to row without scrollback"
#[test]
fn pagelist_scroll_to_row_without_scrollback() {
    let mut s = PageList::init(80, 24, None).unwrap();

    s.scroll(Scroll::Row(5));

    assert!(s.viewport == super::Viewport::Active);

    {
        let pt = s
            .get_cell(point::Point::Viewport(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 0 }), pt);
    }

    assert_eq!(
        Scrollbar {
            total: s.total_rows,
            offset: s.total_rows - s.rows as usize,
            len: s.rows as usize,
        },
        s.scrollbar()
    );

    s.deinit();
}

// ref: test "PageList scroll to row then delta"
#[test]
fn pagelist_scroll_to_row_then_delta() {
    let mut s = PageList::init(80, 24, None).unwrap();
    s.grow_rows(30).unwrap();

    s.scroll(Scroll::Row(10));

    assert!(s.viewport == super::Viewport::Pin);

    {
        let pt = s
            .get_cell(point::Point::Viewport(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 10 }), pt);
    }

    assert_eq!(
        Scrollbar {
            total: s.total_rows,
            offset: 10,
            len: s.rows as usize,
        },
        s.scrollbar()
    );

    s.scroll(Scroll::DeltaRow(5));

    assert!(s.viewport == super::Viewport::Pin);

    {
        let pt = s
            .get_cell(point::Point::Viewport(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 15 }), pt);
    }

    assert_eq!(
        Scrollbar {
            total: s.total_rows,
            offset: 15,
            len: s.rows as usize,
        },
        s.scrollbar()
    );

    s.scroll(Scroll::DeltaRow(-3));

    assert!(s.viewport == super::Viewport::Pin);

    {
        let pt = s
            .get_cell(point::Point::Viewport(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 12 }), pt);
    }

    assert_eq!(
        Scrollbar {
            total: s.total_rows,
            offset: 12,
            len: s.rows as usize,
        },
        s.scrollbar()
    );

    s.deinit();
}

// ref: test "PageList scroll to row with cache fast path down"
#[test]
fn pagelist_scroll_to_row_with_cache_fast_path_down() {
    let mut s = PageList::init(80, 24, None).unwrap();
    s.grow_rows(50).unwrap();

    s.scroll(Scroll::Row(10));

    assert!(s.viewport == super::Viewport::Pin);
    assert_eq!(
        Scrollbar {
            total: s.total_rows,
            offset: 10,
            len: s.rows as usize,
        },
        s.scrollbar()
    );

    {
        let pt = s
            .get_cell(point::Point::Viewport(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 10 }), pt);
    }

    // Verify cache is populated
    assert!(s.viewport_pin_row_offset.is_some());
    assert_eq!(10_usize, s.viewport_pin_row_offset.unwrap());

    // Now scroll to a different row - this should use the fast path
    s.scroll(Scroll::Row(20));

    assert!(s.viewport == super::Viewport::Pin);
    assert_eq!(
        Scrollbar {
            total: s.total_rows,
            offset: 20,
            len: s.rows as usize,
        },
        s.scrollbar()
    );

    {
        let pt = s
            .get_cell(point::Point::Viewport(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 20 }), pt);
    }

    s.grow_rows(10).unwrap();
    {
        let pt = s
            .get_cell(point::Point::Viewport(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 20 }), pt);
    }

    assert_eq!(
        Scrollbar {
            total: s.total_rows,
            offset: 20,
            len: s.rows as usize,
        },
        s.scrollbar()
    );

    s.deinit();
}

// ref: test "PageList scroll to row with cache fast path up"
#[test]
fn pagelist_scroll_to_row_with_cache_fast_path_up() {
    let mut s = PageList::init(80, 24, None).unwrap();
    s.grow_rows(50).unwrap();

    s.scroll(Scroll::Row(30));

    assert!(s.viewport == super::Viewport::Pin);
    assert_eq!(
        Scrollbar {
            total: s.total_rows,
            offset: 30,
            len: s.rows as usize,
        },
        s.scrollbar()
    );

    {
        let pt = s
            .get_cell(point::Point::Viewport(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 30 }), pt);
    }

    // Verify cache is populated
    assert!(s.viewport_pin_row_offset.is_some());
    assert_eq!(30_usize, s.viewport_pin_row_offset.unwrap());

    // Now scroll up to a different row - this should use the fast path
    s.scroll(Scroll::Row(15));

    assert!(s.viewport == super::Viewport::Pin);
    assert_eq!(
        Scrollbar {
            total: s.total_rows,
            offset: 15,
            len: s.rows as usize,
        },
        s.scrollbar()
    );

    {
        let pt = s
            .get_cell(point::Point::Viewport(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 15 }), pt);
    }

    s.grow_rows(10).unwrap();
    {
        let pt = s
            .get_cell(point::Point::Viewport(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 15 }), pt);
    }

    assert_eq!(
        Scrollbar {
            total: s.total_rows,
            offset: 15,
            len: s.rows as usize,
        },
        s.scrollbar()
    );

    s.deinit();
}

// ref: test "PageList scroll clear"
#[test]
fn pagelist_scroll_clear() {
    let mut s = PageList::init(80, 24, None).unwrap();

    {
        let cell = s
            .get_cell(point::Point::Active(point::Coordinate { x: 0, y: 0 }))
            .unwrap();
        unsafe {
            *cell.cell = cell_codepoint('A' as u32);
        }
    }
    {
        let cell = s
            .get_cell(point::Point::Active(point::Coordinate { x: 0, y: 1 }))
            .unwrap();
        unsafe {
            *cell.cell = cell_codepoint('A' as u32);
        }
    }

    s.scroll_clear().unwrap();

    {
        let pt = s
            .get_cell(point::Point::Viewport(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 2 }), pt);
    }

    s.deinit();
}

// ref: test "PageList: jump zero prompts"
#[test]
fn pagelist_jump_zero_prompts() {
    let mut s = PageList::init(5, 3, None).unwrap();
    s.grow_rows(3).unwrap();
    assert!(core::ptr::eq(s.pages.first, s.pages.last));
    let page = unsafe { &mut (*s.pages.first).data };
    {
        let rac = page.get_row_and_cell(0, 1);
        unsafe {
            (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt);
        }
    }
    {
        let rac = page.get_row_and_cell(0, 5);
        unsafe {
            (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt);
        }
    }

    s.scroll(Scroll::DeltaPrompt(0));
    assert!(s.viewport == super::Viewport::Active);

    assert_eq!(
        Scrollbar {
            total: s.total_rows,
            offset: s.total_rows - s.rows as usize,
            len: s.rows as usize,
        },
        s.scrollbar()
    );

    s.deinit();
}

// ref: test "Screen: jump back one prompt"
#[test]
fn screen_jump_back_one_prompt() {
    let mut s = PageList::init(5, 3, None).unwrap();
    s.grow_rows(3).unwrap();
    assert!(core::ptr::eq(s.pages.first, s.pages.last));
    let page = unsafe { &mut (*s.pages.first).data };
    {
        let rac = page.get_row_and_cell(0, 1);
        unsafe {
            (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt);
        }
    }
    {
        let rac = page.get_row_and_cell(0, 5);
        unsafe {
            (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt);
        }
    }

    // Jump back
    {
        s.scroll(Scroll::DeltaPrompt(-1));
        assert!(s.viewport == super::Viewport::Pin);
        assert_eq!(
            point::Point::Screen(point::Coordinate { x: 0, y: 1 }),
            s.point_from_pin(
                point::Tag::Screen,
                s.pin(point::Point::Viewport(point::Coordinate { x: 0, y: 0 }))
                    .unwrap()
            )
            .unwrap()
        );

        assert_eq!(
            Scrollbar {
                total: s.total_rows,
                offset: 1,
                len: s.rows as usize,
            },
            s.scrollbar()
        );
    }
    {
        s.scroll(Scroll::DeltaPrompt(-1));
        assert!(s.viewport == super::Viewport::Pin);
        assert_eq!(
            point::Point::Screen(point::Coordinate { x: 0, y: 1 }),
            s.point_from_pin(
                point::Tag::Screen,
                s.pin(point::Point::Viewport(point::Coordinate { x: 0, y: 0 }))
                    .unwrap()
            )
            .unwrap()
        );

        assert_eq!(
            Scrollbar {
                total: s.total_rows,
                offset: 1,
                len: s.rows as usize,
            },
            s.scrollbar()
        );
    }

    // Jump forward
    {
        s.scroll(Scroll::DeltaPrompt(1));
        assert!(s.viewport == super::Viewport::Active);
        assert_eq!(
            Scrollbar {
                total: s.total_rows,
                offset: s.total_rows - s.rows as usize,
                len: s.rows as usize,
            },
            s.scrollbar()
        );
    }
    {
        s.scroll(Scroll::DeltaPrompt(1));
        assert!(s.viewport == super::Viewport::Active);
        assert_eq!(
            Scrollbar {
                total: s.total_rows,
                offset: s.total_rows - s.rows as usize,
                len: s.rows as usize,
            },
            s.scrollbar()
        );
    }

    s.deinit();
}

// ref: test "Screen: jump forward prompt skips multiline continuation"
#[test]
fn screen_jump_forward_prompt_skips_multiline_continuation() {
    let mut s = PageList::init(5, 3, None).unwrap();
    s.grow_rows(7).unwrap();

    // Multiline prompt on rows 1-3.
    {
        let p = s
            .pin(point::Point::Screen(point::Coordinate { x: 0, y: 1 }))
            .unwrap();
        unsafe {
            (*p.row_and_cell().row).set_semantic_prompt(SemanticPrompt::Prompt);
        }
    }
    {
        let p = s
            .pin(point::Point::Screen(point::Coordinate { x: 0, y: 2 }))
            .unwrap();
        unsafe {
            (*p.row_and_cell().row).set_semantic_prompt(SemanticPrompt::PromptContinuation);
        }
    }
    {
        let p = s
            .pin(point::Point::Screen(point::Coordinate { x: 0, y: 3 }))
            .unwrap();
        unsafe {
            (*p.row_and_cell().row).set_semantic_prompt(SemanticPrompt::PromptContinuation);
        }
    }

    // Next prompt after command output.
    {
        let p = s
            .pin(point::Point::Screen(point::Coordinate { x: 0, y: 6 }))
            .unwrap();
        unsafe {
            (*p.row_and_cell().row).set_semantic_prompt(SemanticPrompt::Prompt);
        }
    }

    // Starting at the first prompt line should jump to the next prompt,
    // not to continuation lines.
    s.scroll(Scroll::Row(1));
    s.scroll(Scroll::DeltaPrompt(1));
    assert!(s.viewport == super::Viewport::Pin);
    assert_eq!(
        point::Point::Screen(point::Coordinate { x: 0, y: 6 }),
        s.point_from_pin(
            point::Tag::Screen,
            s.pin(point::Point::Viewport(point::Coordinate { x: 0, y: 0 }))
                .unwrap()
        )
        .unwrap()
    );

    // Starting in the middle of continuation lines should also jump to
    // the next prompt.
    s.scroll(Scroll::Row(2));
    s.scroll(Scroll::DeltaPrompt(1));
    assert!(s.viewport == super::Viewport::Pin);
    assert_eq!(
        point::Point::Screen(point::Coordinate { x: 0, y: 6 }),
        s.point_from_pin(
            point::Tag::Screen,
            s.pin(point::Point::Viewport(point::Coordinate { x: 0, y: 0 }))
                .unwrap()
        )
        .unwrap()
    );

    s.deinit();
}

// ref: test "PageList grow prune scrollback"
#[test]
fn pagelist_grow_prune_scrollback() {
    // Use std_size to limit scrollback so pruning is triggered.
    let mut s = PageList::init(80, 24, Some(std_size())).unwrap();

    // Grow to capacity
    let page1_node = s.pages.last;
    let (page1_cap_rows, page1_size_rows) = {
        let page1 = unsafe { &(*page1_node).data };
        (page1.capacity.rows, page1.size.rows)
    };
    for _ in 0..(page1_cap_rows - page1_size_rows) {
        assert!(s.grow().unwrap().is_none());
    }

    // Grow and allocate one more page. Then fill that page up.
    let page2_node = s.grow().unwrap().unwrap();
    let (page2_cap_rows, page2_size_rows) = {
        let page2 = unsafe { &(*page2_node).data };
        (page2.capacity.rows, page2.size.rows)
    };
    for _ in 0..(page2_cap_rows - page2_size_rows) {
        assert!(s.grow().unwrap().is_none());
    }

    // Get our page size
    let old_page_size = s.page_size;

    // Create a tracked pin in the first page
    let p = s
        .track_pin(
            s.pin(point::Point::Screen(point::Coordinate { x: 0, y: 0 }))
                .unwrap(),
        )
        .unwrap();
    assert!(core::ptr::eq(unsafe { (*p).node }, s.pages.first));

    // Scroll back to create a pinned viewport (not active)
    let pin_y = page1_cap_rows as usize / 2;
    s.scroll(Scroll::Pin(
        s.pin(point::Point::Screen(point::Coordinate {
            x: 0,
            y: pin_y as u32,
        }))
        .unwrap(),
    ));
    assert!(s.viewport == super::Viewport::Pin);

    // Get the scrollbar state to populate the cache
    let scrollbar_before = s.scrollbar();
    assert_eq!(pin_y, scrollbar_before.offset);

    // Next should create a new page, but it should reuse our first
    // page since we're at max size.
    let new = s.grow().unwrap().unwrap();
    assert!(core::ptr::eq(s.pages.last, new));
    assert_eq!(s.page_size, old_page_size);

    // Our first should now be page2 and our last should be page1
    assert!(core::ptr::eq(page2_node, s.pages.first));
    assert!(core::ptr::eq(page1_node, s.pages.last));

    // Our tracked pin should point to the top-left of the first page
    assert!(core::ptr::eq(unsafe { (*p).node }, s.pages.first));
    assert!(unsafe { (*p).x } == 0);
    assert!(unsafe { (*p).y } == 0);
    assert!(unsafe { (*p).garbage });

    // Verify the viewport offset cache was invalidated. After pruning,
    // the offset should have changed because we removed rows from
    // the beginning.
    {
        let scrollbar_after = s.scrollbar();
        let rows_pruned = page1_cap_rows as usize;
        // PORT NOTE: the reference computes this with an explicit
        // `if pin_y >= rows_pruned ... else 0` branch; reproduced verbatim
        // (clippy would prefer `saturating_sub`, allowed here for fidelity).
        #[allow(clippy::implicit_saturating_sub)]
        let expected_offset = if pin_y >= rows_pruned {
            pin_y - rows_pruned
        } else {
            0
        };
        assert_eq!(expected_offset, scrollbar_after.offset);
    }

    s.untrack_pin(p);
    s.deinit();
}

// ref: test "PageList grow prune scrollback with viewport pin not in pruned page"
#[test]
fn pagelist_grow_prune_scrollback_with_viewport_pin_not_in_pruned_page() {
    // Use std_size to limit scrollback so pruning is triggered.
    let mut s = PageList::init(80, 24, Some(std_size())).unwrap();

    // Grow to capacity of first page
    let page1_node = s.pages.last;
    let (page1_cap_rows, page1_size_rows) = {
        let page1 = unsafe { &(*page1_node).data };
        (page1.capacity.rows, page1.size.rows)
    };
    for _ in 0..(page1_cap_rows - page1_size_rows) {
        assert!(s.grow().unwrap().is_none());
    }

    // Grow and allocate second page, then fill it up
    let page2_node = s.grow().unwrap().unwrap();
    let (page2_cap_rows, page2_size_rows) = {
        let page2 = unsafe { &(*page2_node).data };
        (page2.capacity.rows, page2.size.rows)
    };
    for _ in 0..(page2_cap_rows - page2_size_rows) {
        assert!(s.grow().unwrap().is_none());
    }

    // Get our page size
    let old_page_size = s.page_size;

    // Scroll back to create a pinned viewport in page2 (NOT page1)
    // This is the key difference from the previous test - the viewport
    // pin is NOT in the page that will be pruned.
    let pin_y = page1_cap_rows as usize + 5;
    s.scroll(Scroll::Pin(
        s.pin(point::Point::Screen(point::Coordinate {
            x: 0,
            y: pin_y as u32,
        }))
        .unwrap(),
    ));
    assert!(s.viewport == super::Viewport::Pin);
    assert!(core::ptr::eq(unsafe { (*s.viewport_pin).node }, page2_node));

    // Get the scrollbar state to populate the cache
    let scrollbar_before = s.scrollbar();
    assert_eq!(pin_y, scrollbar_before.offset);

    // Next grow will trigger pruning of the first page.
    // The viewport_pin.node is page2, not page1, so it won't be moved
    // by the pin update loop, but the cached offset still needs to be
    // invalidated because rows were removed from the beginning.
    let new = s.grow().unwrap().unwrap();
    assert!(core::ptr::eq(s.pages.last, new));
    assert_eq!(s.page_size, old_page_size);

    // Our first should now be page2 (page1 was pruned)
    assert!(core::ptr::eq(page2_node, s.pages.first));

    // The viewport pin should still be on page2, unchanged
    assert!(core::ptr::eq(unsafe { (*s.viewport_pin).node }, page2_node));

    // Verify the viewport offset cache was invalidated/updated.
    // After pruning, the offset should have decreased by the number
    // of rows that were pruned.
    let scrollbar_after = s.scrollbar();
    let rows_pruned = page1_cap_rows as usize;
    let expected_offset = pin_y - rows_pruned;
    assert_eq!(expected_offset, scrollbar_after.offset);

    s.deinit();
}
