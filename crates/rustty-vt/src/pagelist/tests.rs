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

use super::compress::{CompressionMode, IncrementalCompressionResult};
use super::grow::{IncreaseCapacity, IncreaseCapacityError, Scrollbar, SplitError};
use super::resize::Resize;
use super::scroll::Scroll;
use super::{std_capacity, std_size, NodeData, Owned, PageList, Pin, Storage, Viewport};
use crate::page::{Cell, CellRgb, ContentTag, Page, SemanticContent, SemanticPrompt};
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

// Helper mirroring the reference cell-literal `.{ .content_tag = .codepoint,
// .content = .{ .codepoint = cp }, .semantic_content = sc }`.
fn cell_codepoint_sc(cp: u32, sc: SemanticContent) -> Cell {
    let mut c = Cell::zeroed();
    c.set_content_tag(ContentTag::Codepoint);
    c.set_content_codepoint(cp);
    c.set_semantic_content(sc);
    c
}

// Helper mirroring the reference cell-literal `.{ .content_tag = .bg_color_rgb,
// .content = .{ .color_rgb = .{ .r, .g, .b } } }`.
fn cell_bg_color_rgb(r: u8, g: u8, b: u8) -> Cell {
    let mut c = Cell::zeroed();
    c.set_content_tag(ContentTag::BgColorRgb);
    c.set_content_color_rgb(CellRgb { r, g, b });
    c
}

// Helper mirroring the reference `p.*` dereference of a tracked-pin pointer
// into a by-value `Pin`. The CORE `Pin` struct is a raw-pointer-bearing POD that
// does not derive `Copy`/`Clone`, so we read its fields field-for-field.
fn pin_copy(p: *mut Pin) -> Pin {
    let p = unsafe { &*p };
    Pin {
        node: p.node,
        y: p.y,
        x: p.x,
        garbage: p.garbage,
    }
}

// Helper asserting a `Pin` equals the reference `Pin{ .node, .y, .x }` literal
// (which leaves `.garbage` at its `false` default).
//
// PORT NOTE: the CORE `Pin` struct does not derive `PartialEq`/`Debug` (it is a
// raw-pointer-bearing POD), so the reference `expectEqual(Pin{...}, ...)` is
// reproduced field-for-field including `garbage` (default `false`).
// Deep-compare analog of the reference's `testing.expectEqual(Pin{...}, ...)`.
// Returns the comparison instead of asserting so the ASSERTION itself lives in
// each test body, one per reference `expectEqual` (assertion-for-assertion;
// G2 counts source-level asserts per test).
fn pin_eql(expected: Pin, got: Pin) -> bool {
    core::ptr::eq(expected.node, got.node)
        && expected.y == got.y
        && expected.x == got.x
        && expected.garbage == got.garbage
}

// ref: test "PageList"
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
    assert!(pin_eql(
        Pin {
            node: s.pages.first,
            y: 0,
            x: 0,
            garbage: false,
        },
        s.get_top_left(point::Tag::Active),
    ));

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

// ref: test "PageList init error"
#[test]
// WS18 release lane: this test injects a failure through a `rustty_util::tripwire`
// fail point, and `tripwire::Module::enabled()` is `cfg!(debug_assertions)` (the
// faithful stand-in for Zig's `builtin.is_test`; see its PORT NOTE). Without
// `debug_assertions` the fail point never trips, so the test can only run in
// debug — where it still runs and passes. NOT disabled to hide a failure.
#[cfg_attr(not(debug_assertions), ignore)]
fn pagelist_init_error() {
    use super::{InitFp, InitPagesFp, OutOfMemory, INIT_PAGES_TW, INIT_TW};
    use rustty_util::tripwire::ResetMode;

    // PORT NOTE: the reference iterates `std.meta.tags(init_tw.FailPoint)` /
    // `std.meta.tags(initPages_tw.FailPoint)`. Rust has no reflection over enum
    // variants, so we list the fieldless-enum tags explicitly. These must match
    // the `InitFp`/`InitPagesFp` definitions in `mod.rs` exactly.

    // Test every failure point in `init` and ensure that we don't
    // leak memory (testing.allocator verifies) since we're exiting early.
    for tag in [
        InitFp::InitMemoryPool,
        InitFp::InitPages,
        InitFp::ViewportPin,
        InitFp::ViewportPinTrack,
    ] {
        // defer tw.end(.reset) catch unreachable;
        // PORT NOTE: Rust has no `defer`; we drain the tripwire after the
        // assertion via `end(.reset)` and assert it reports no untripped point.
        INIT_TW.with(|tw| tw.borrow_mut().errorAlways(tag, OutOfMemory));
        // try std.testing.expectError(error.OutOfMemory, init(...));
        // PORT NOTE: `PageList` has no `Debug`, so we use `matches!` rather than
        // `assert_eq!` (which would require `Debug` on the `Ok` type).
        assert!(matches!(PageList::init(80, 24, None), Err(OutOfMemory)));
        INIT_TW.with(|tw| tw.borrow_mut().end(ResetMode::Reset).unwrap());
    }

    // init calls initPages transitively, so let's check that if
    // any failures happen in initPages, we also don't leak memory.
    for tag in [
        InitPagesFp::PageNode,
        InitPagesFp::PageBufStd,
        InitPagesFp::PageBufNonStd,
    ] {
        INIT_PAGES_TW.with(|tw| tw.borrow_mut().errorAlways(tag, OutOfMemory));

        let cols: size::CellCountInt = if tag == InitPagesFp::PageBufStd {
            80
        } else {
            std_capacity().max_cols().unwrap() + 1
        };
        assert!(matches!(PageList::init(cols, 24, None), Err(OutOfMemory)));
        INIT_PAGES_TW.with(|tw| tw.borrow_mut().end(ResetMode::Reset).unwrap());
    }

    // Try non-standard pages since they don't go in our pool.
    // PORT NOTE: the reference loops over a single-element array
    // `[_]initPages_tw.FailPoint{.page_buf_non_std}`; we reproduce that loop
    // form verbatim, so the single-element-loop lint is suppressed.
    #[allow(clippy::single_element_loop)]
    for tag in [InitPagesFp::PageBufNonStd] {
        INIT_PAGES_TW.with(|tw| tw.borrow_mut().errorAfter(tag, OutOfMemory, 1));
        assert!(matches!(
            PageList::init(
                std_capacity().max_cols().unwrap() + 1,
                std_capacity().rows + 1,
                None,
            ),
            Err(OutOfMemory)
        ));
        INIT_PAGES_TW.with(|tw| tw.borrow_mut().end(ResetMode::Reset).unwrap());
    }
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
    let page_cap = unsafe { super::Node::capacity(s.pages.last).rows };
    let mut cur_page = s.pages.last;
    unsafe { (*super::Node::page(cur_page)).pause_integrity_checks(true) };
    for _ in 0..(page_cap as usize * 5) {
        if let Some(new_page) = s.grow().unwrap() {
            unsafe { (*super::Node::page(cur_page)).pause_integrity_checks(false) };
            cur_page = new_page;
            unsafe { (*super::Node::page(cur_page)).pause_integrity_checks(true) };
        }
    }
    unsafe { (*super::Node::page(cur_page)).pause_integrity_checks(false) };

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
    let page_cap = unsafe { super::Node::capacity(s.pages.last).rows };
    let mut cur_page = s.pages.last;
    unsafe { (*super::Node::page(cur_page)).pause_integrity_checks(true) };
    for _ in 0..(page_cap as usize * 2) {
        if let Some(new_page) = s.grow().unwrap() {
            unsafe { (*super::Node::page(cur_page)).pause_integrity_checks(false) };
            cur_page = new_page;
            unsafe { (*super::Node::page(cur_page)).pause_integrity_checks(true) };
        }
    }
    unsafe { (*super::Node::page(cur_page)).pause_integrity_checks(false) };

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

// ref: test "PageList init rows across two pages"
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

// ref: test "PageList init more than max cols"
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
    assert!(unsafe { (*super::Node::page(s.pages.first)).memory_len } > std_size());

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

// ref: test "PageList active after grow"
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

// ref: test "PageList grow allows exceeding max size for active area"
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
                (*super::Node::page(page)).size.rows = 1;
                (*super::Node::page(page)).capacity.rows = 1;
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

// ref: test "PageList grow prune required with a single page"
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION (vt multi-page-PageList intractable, .port/phase-2/MIRI_EXCEPTIONS.md): builds/reflows multi-page PageLists that exceed Miri's interpreter budget (>20min); covered natively + by ASan (all PageList tests ~18s)"
)]
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
            let layout = Page::layout(unsafe { super::Node::capacity(s.pages.first) });
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
        unsafe { super::Node::capacity(page).rows - super::Node::rows(page) }
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

// PORT NOTE (rustty-only, NOT a reference test — VALIDATION_REPORT §3c.6 /
// MIRI_EXCEPTIONS rows 8-13): a Miri-TRACTABLE grow-PRUNE test. The six
// excepted `grow_*` tests reach the prune branch only after filling whole pages
// one `grow()` call at a time (each call runs the O(cells) integrity walk —
// quadratic under Miri, hours), so before this test the prune branch had no
// distinct Miri-passing coverage (the exception table self-cited an excepted
// test). This variant reaches the SAME branch — `pop_first` → total_rows
// rebook → viewport/tracked-pin remap → buffer `write_bytes` reset →
// `init_buf` reuse → `insert_after` → serial rebump — with a CONSTANT number
// of `grow()` calls by filling pages surgically (the
// `grow_prune_scrollback_with_viewport_pin…` precedent; `init_buf` formats
// rows to full capacity, so bumping `size.rows` is valid), keeping it well
// inside the 20-min Miri budget (criterion 3 coverage for rows 8-13).
#[test]
fn pagelist_grow_prune_miri_tractable() {
    let mut s = PageList::init(80, 24, Some(0)).unwrap();

    // Fill page 1 to capacity surgically (constant-time).
    unsafe {
        let first = &mut (*super::Node::page(s.pages.first));
        first.size.rows = first.capacity.rows;
    }
    s.total_rows = s.total_rows();

    // Allocate page 2 (grow_alloc path; the prune condition is skipped while
    // first == last).
    let new = s.grow().unwrap().expect("page 2 allocated");
    assert!(core::ptr::eq(new, s.pages.last));
    assert_eq!(2, s.total_pages());

    // Fill page 2 to capacity surgically.
    unsafe {
        let last = &mut (*super::Node::page(s.pages.last));
        last.size.rows = last.capacity.rows;
    }
    s.total_rows = s.total_rows();

    // Track a pin inside the to-be-pruned first page so the prune's
    // tracked-pin remap executes under Miri too.
    let first_before = s.pages.first;
    let p = s
        .track_pin(Pin {
            node: first_before,
            x: 1,
            y: 1,
            garbage: false,
        })
        .unwrap();

    // The next grow must PRUNE: with max_size(=min_max_size, two std pages)
    // already consumed, `page_size + std_size > max_size` and total_rows is far
    // above the active area, so the first page is popped, zeroed, re-inited and
    // reused as the new last page — the page count stays 2.
    let reused = s.grow().unwrap().expect("prune reuses the popped page");
    assert!(core::ptr::eq(first_before, reused));
    assert!(core::ptr::eq(s.pages.last, reused));
    assert_eq!(2, s.total_pages());
    assert_eq!(1, unsafe { super::Node::rows(reused) });

    // The tracked pin in the pruned page was remapped to the new first page's
    // top-left and marked garbage.
    unsafe {
        assert!(core::ptr::eq((*p).node, s.pages.first));
        assert_eq!(0, (*p).x);
        assert_eq!(0, (*p).y);
        assert!((*p).garbage);
    }
    s.untrack_pin(p);

    s.deinit();
}

// ref: test "PageList scrollbar with max_size 0 after grow"
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

// ref: test "PageList grow fit in capacity"
#[test]
fn pagelist_grow_fit_in_capacity() {
    let mut s = PageList::init(80, 24, None).unwrap();

    // So we know we're using capacity to grow
    {
        let last = unsafe { &(*super::Node::page(s.pages.last)) };
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

// ref: test "PageList grow allocate"
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION (vt multi-page-PageList intractable, .port/phase-2/MIRI_EXCEPTIONS.md): builds/reflows multi-page PageLists that exceed Miri's interpreter budget (>20min); covered natively + by ASan (all PageList tests ~18s)"
)]
#[test]
fn pagelist_grow_allocate() {
    let mut s = PageList::init(80, 24, None).unwrap();

    // Grow to capacity
    let last_node = s.pages.last;
    let (last_cap_rows, last_size_rows) = {
        let last = unsafe { &(*super::Node::page(s.pages.last)) };
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

// ref: test "PageList increaseCapacity to increase styles"
#[test]
fn pagelist_increaseCapacity_to_increase_styles() {
    let mut s = PageList::init(2, 2, Some(0)).unwrap();

    let original_styles_cap = unsafe { super::Node::capacity(s.pages.first).styles };

    {
        assert!(core::ptr::eq(s.pages.first, s.pages.last));
        let page = unsafe { super::Node::page(s.pages.first) };

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
        let page = unsafe { &(*super::Node::page(s.pages.first)) };

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

// ref: test "PageList increaseCapacity to increase graphemes"
#[test]
fn pagelist_increaseCapacity_to_increase_graphemes() {
    let mut s = PageList::init(2, 2, Some(0)).unwrap();

    let original_cap = unsafe { super::Node::capacity(s.pages.first).grapheme_bytes };

    {
        assert!(core::ptr::eq(s.pages.first, s.pages.last));
        let page = unsafe { super::Node::page(s.pages.first) };

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
        let page = unsafe { &(*super::Node::page(s.pages.first)) };

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

// ref: test "PageList increaseCapacity to increase hyperlinks"
#[test]
fn pagelist_increaseCapacity_to_increase_hyperlinks() {
    let mut s = PageList::init(2, 2, Some(0)).unwrap();

    let original_cap = unsafe { super::Node::capacity(s.pages.first).hyperlink_bytes };

    {
        assert!(core::ptr::eq(s.pages.first, s.pages.last));
        let page = unsafe { super::Node::page(s.pages.first) };

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
        let page = unsafe { &(*super::Node::page(s.pages.first)) };

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
    let mut s = PageList::init(2, 2, Some(0)).unwrap();

    let original_cap = unsafe { super::Node::capacity(s.pages.first).string_bytes };

    {
        assert!(core::ptr::eq(s.pages.first, s.pages.last));
        let page = unsafe { super::Node::page(s.pages.first) };

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
        let page = unsafe { &(*super::Node::page(s.pages.first)) };

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

// ref: test "PageList increaseCapacity returns OutOfSpace at max capacity"
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
                assert_eq!(max_styles, unsafe { super::Node::capacity(s.pages.first).styles });
                break;
            }
        }
    }

    s.deinit();
}

// ref: test "PageList increaseCapacity multi-page"
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION (vt multi-page-PageList intractable, .port/phase-2/MIRI_EXCEPTIONS.md): builds/reflows multi-page PageLists that exceed Miri's interpreter budget (>20min); covered natively + by ASan (all PageList tests ~18s)"
)]
#[test]
fn pagelist_increaseCapacity_multi_page() {
    let mut s = PageList::init(80, 24, None).unwrap();

    // Grow to create a second page
    let page1_node = s.pages.last;
    unsafe { (*super::Node::page(page1_node)).pause_integrity_checks(true) };
    for _ in 0..(unsafe { super::Node::capacity(page1_node).rows - super::Node::rows(page1_node) })
    {
        assert!(s.grow().unwrap().is_none());
    }
    unsafe { (*super::Node::page(page1_node)).pause_integrity_checks(false) };
    assert!(s.grow().unwrap().is_some());

    // Now we have two pages
    assert!(!core::ptr::eq(s.pages.first, s.pages.last));
    let page2_node = s.pages.last;

    let page1_styles_cap = unsafe { super::Node::capacity(s.pages.first).styles };
    let page2_styles_cap = unsafe { super::Node::capacity(page2_node).styles };

    // Increase capacity on the first page only
    let _ = s
        .increase_capacity(s.pages.first, Some(IncreaseCapacity::Styles))
        .unwrap();

    // First page capacity should be doubled
    assert_eq!(page1_styles_cap * 2, unsafe {
        super::Node::capacity(s.pages.first).styles
    });

    // Second page should be unchanged
    assert_eq!(page2_styles_cap, unsafe {
        super::Node::capacity(s.pages.last).styles
    });

    s.deinit();
}

// ref: test "PageList increaseCapacity preserves dirty flag"
#[test]
fn pagelist_increaseCapacity_preserves_dirty_flag() {
    let mut s = PageList::init(2, 4, Some(0)).unwrap();

    // Set page dirty flag and mark some rows as dirty
    let new_node = {
        let page = unsafe { super::Node::page(s.pages.first) };
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
    assert!(unsafe { (*super::Node::page(new_node)).dirty });

    // Row dirty flags should be preserved
    let new_rows = unsafe {
        (*super::Node::page(new_node))
            .rows
            .ptr((*super::Node::page(new_node)).memory_ptr())
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
    let mut s = PageList::init(80, 24, None).unwrap();
    s.reset();
    assert!(s.viewport == super::Viewport::Active);
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
            let rows = unsafe { super::Node::rows(node) };
            let cols = unsafe { super::Node::cols(node) };
            for y in 0..rows {
                for x in 0..cols {
                    let rac = unsafe { (*super::Node::page(node)).get_row_and_cell(x as usize, y as usize) };
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
    let mut s = PageList::init(80, 24, None).unwrap();
    assert_eq!(
        unsafe { (*super::Node::page(s.pages.first)).memory_len },
        std_size()
    );
    assert_all_cells_zero(&s, "fresh");

    // Reused slot after reset: scribble non-zero bytes across the ENTIRE page
    // buffer, then reset. Reset retains + zeroes the arena and rebuilds the
    // pages from the retained free-list, so the cell region must read zero again
    // even though the slot was not freshly mmap'd.
    {
        let first = s.pages.first;
        let ptr = unsafe { (*super::Node::page(first)).memory_ptr() } as *mut u8;
        let len = unsafe { (*super::Node::page(first)).memory_len };
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
#[test]
fn pagelist_only_reset_advances_the_serial_epoch() {
    let mut s = PageList::init(80, 24, Some(10 * 1024 * 1024)).unwrap();
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
#[test]
fn pagelist_reset_invalidates_stale_untracked_refs_even_if_node_memory_is_reused() {
    let mut s = PageList::init(80, 24, None).unwrap();

    let old_serial = unsafe { (*s.pages.first).serial };
    assert!(old_serial >= s.page_serial_epoch);
    assert!(old_serial < s.page_serial);

    s.reset();

    // The important safety property is that stale serials are rejected before
    // the node pointer is inspected. Reset rebuilds the page list from the
    // pools, so old untracked refs may contain node pointers that are no
    // longer safe to dereference.
    assert!(old_serial < s.page_serial_epoch);

    let new_serial = unsafe { (*s.pages.first).serial };
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
    let mut s = PageList::init(cap.cols, rows, None).unwrap();
    s.reset();
    assert!(s.viewport == super::Viewport::Active);
    assert!(!s.pages.first.is_null());
    assert_eq!(s.rows as usize, s.total_rows());

    s.deinit();
}

// ref: test "PageList reset moves tracked pins and marks them as garbage"
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

// ref: test "PageList clears history"
#[test]
fn pagelist_clears_history() {
    let mut s = PageList::init(80, 24, None).unwrap();
    s.grow_rows(30).unwrap();
    s.reset();
    assert!(s.viewport == super::Viewport::Active);
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

// =====================================================================
// grow non-standard / compact
// =====================================================================

// ref: test "PageList grow reuses non-standard page without leak"
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION (vt multi-page-PageList intractable, .port/phase-2/MIRI_EXCEPTIONS.md): builds/reflows multi-page PageLists that exceed Miri's interpreter budget (>20min); covered natively + by ASan (all PageList tests ~18s)"
)]
#[test]
fn pagelist_grow_reuses_non_standard_page_without_leak() {
    // Create a PageList with 3 * std_size max so we can fit multiple pages
    // but will still trigger reuse.
    let mut s = PageList::init(80, 24, Some(3 * std_size())).unwrap();

    // Increase the first page capacity to make it non-standard (larger than std_size).
    while unsafe { (*super::Node::page(s.pages.first)).memory_len } <= std_size() {
        let _ = s
            .increase_capacity(s.pages.first, Some(IncreaseCapacity::GraphemeBytes))
            .unwrap();
    }

    // The first page should now have non-standard memory size.
    assert!(unsafe { (*super::Node::page(s.pages.first)).memory_len } > std_size());

    // First, fill up the first page's capacity
    let first_page = s.pages.first;
    while unsafe { super::Node::rows(first_page) < super::Node::capacity(first_page).rows } {
        let _ = s.grow().unwrap();
    }

    // Now grow to create a second page
    let _ = s.grow().unwrap();
    assert!(!core::ptr::eq(s.pages.first, s.pages.last));

    // Continue growing until we exceed max_size AND the last page is full
    while s.page_size + std_size() <= s.max_size()
        || unsafe { super::Node::rows(s.pages.last) < super::Node::capacity(s.pages.last).rows }
    {
        let _ = s.grow().unwrap();
    }

    // The first page should still be non-standard
    assert!(unsafe { (*super::Node::page(s.pages.first)).memory_len } > std_size());

    // Verify we have enough rows for active area (so prune path isn't skipped)
    assert!(s.total_rows() >= s.rows as usize);

    // Verify last page is full (so grow will need to allocate/reuse)
    assert!(unsafe { super::Node::rows(s.pages.last) == super::Node::capacity(s.pages.last).rows });

    // Remember the first page memory pointer before the reuse attempt
    let first_page_ptr = s.pages.first;
    let first_page_mem_ptr = unsafe { (*super::Node::page(s.pages.first)).memory_ptr() };

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
        unsafe { (*super::Node::page(s.pages.last)).memory_ptr() },
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
    miri,
    ignore = "MIRI EXCEPTION (vt multi-page-PageList intractable, .port/phase-2/MIRI_EXCEPTIONS.md): builds/reflows multi-page PageLists that exceed Miri's interpreter budget (>20min); covered natively + by ASan (all PageList tests ~18s)"
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
    let mut s = PageList::init(80, rows_count, Some(std_size())).unwrap();

    // Make the first page non-standard
    while unsafe { (*super::Node::page(s.pages.first)).memory_len } <= std_size() {
        let _ = s
            .increase_capacity(s.pages.first, Some(IncreaseCapacity::GraphemeBytes))
            .unwrap();
    }
    assert!(unsafe { (*super::Node::page(s.pages.first)).memory_len } > std_size());

    let first_page_node = s.pages.first;
    let first_page_cap = unsafe { super::Node::capacity(first_page_node).rows };

    // Fill first page to capacity
    while unsafe { super::Node::rows(first_page_node) } < first_page_cap {
        let _ = s.grow().unwrap();
    }

    // Grow until we have a second page (first page fills up first)
    while core::ptr::eq(s.pages.first, s.pages.last) {
        let _ = s.grow().unwrap();
    }
    assert!(!core::ptr::eq(s.pages.first, s.pages.last));

    // Fill the second page to capacity so that the next grow() triggers prune
    let last_node = s.pages.last;
    let second_cap = unsafe { super::Node::capacity(last_node).rows };
    while unsafe { super::Node::rows(last_node) } < second_cap {
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
    assert!(s.page_size + std_size() > s.max_size());
    assert!(s.total_rows() >= s.rows as usize);

    // Verify last page is at capacity (so grow must prune or allocate new)
    assert_eq!(second_cap, unsafe { super::Node::rows(last_node) });

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
    let mut s = PageList::init(80, 24, Some(0)).unwrap();

    // A freshly created page is pool-owned at std_size.
    let node = s.pages.first;
    assert_eq!(unsafe { (*node).owned }, Owned::Pool);
    assert!(unsafe { (*super::Node::page(node)).memory_len } <= std_size());
    let original_size = unsafe { (*super::Node::page(node)).size };

    // Compacting it should produce a much smaller exact-size heap page.
    let new_node = s.compact(node).unwrap().unwrap();
    assert_eq!(unsafe { (*new_node).owned }, Owned::Heap);
    assert!(unsafe { (*super::Node::page(new_node)).memory_len } < std_size());
    assert_eq!(original_size.rows, unsafe { super::Node::rows(new_node) });
    assert_eq!(original_size.cols, unsafe { super::Node::cols(new_node) });
    assert!(core::ptr::eq(new_node, s.pages.first));

    // Our page size accounting should exactly match the compacted page since it
    // is the only page in the list.
    assert_eq!(
        unsafe { (*super::Node::page(new_node)).memory_len },
        s.page_size
    );

    // Compacting again should be a no-op since it is already exact.
    assert!(s.compact(new_node).unwrap().is_none());

    s.deinit();
}

// ref: test "PageList compact then grow allocates new page"
#[test]
fn pagelist_compact_then_grow_allocates_new_page() {
    let mut s = PageList::init(80, 24, None).unwrap();

    // Compact the only page. It now has no spare row capacity.
    let node = s.compact(s.pages.first).unwrap().unwrap();
    assert_eq!(unsafe { super::Node::rows(node) }, unsafe {
        super::Node::capacity(node).rows
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
    let mut s = PageList::init(80, 24, Some(0)).unwrap();

    // Compact the only page so the list contains a sub-std_size heap-owned
    // page.
    let node = s.compact(s.pages.first).unwrap().unwrap();
    assert_eq!(unsafe { (*node).owned }, Owned::Heap);
    assert!(unsafe { (*super::Node::page(node)).memory_len } < std_size());

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
    let mut s = PageList::init(80, 24, None).unwrap();

    // Write a marker so we can verify contents survive.
    {
        let node = s.pages.first;
        let rac = unsafe { (*super::Node::page(node)).get_row_and_cell(1, 2) };
        unsafe { *rac.cell = cell_codepoint('X' as u32) };
    }

    // Compact so the source list contains a sub-std_size heap page.
    let node = s.compact(s.pages.first).unwrap().unwrap();
    assert_eq!(unsafe { (*node).owned }, Owned::Heap);
    assert!(unsafe { (*super::Node::page(node)).memory_len } < std_size());

    let mut opts = super::access::Clone {
        top: point::Point::Screen(point::Coordinate { x: 0, y: 0 }),
        bot: None,
        tracked_pins: None,
    };
    let mut s2 = s.clone(&mut opts).unwrap();
    assert_eq!(s.rows as usize, s2.total_rows());

    // Verify the marker survived the clone.
    {
        let node2 = s2.pages.first;
        let rac = unsafe { (*super::Node::page(node2)).get_row_and_cell(1, 2) };
        assert_eq!('X' as u32, unsafe { (*rac.cell).codepoint() });
    }

    s2.deinit();
    s.deinit();
}

// ref: test "PageList compact oversized page"
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION (vt multi-page-PageList intractable, .port/phase-2/MIRI_EXCEPTIONS.md): builds/reflows multi-page PageLists that exceed Miri's interpreter budget (>20min); covered natively + by ASan (all PageList tests ~18s)"
)]
#[test]
fn pagelist_compact_oversized_page() {
    let mut s = PageList::init(80, 24, None).unwrap();

    // Grow until we have multiple pages
    let page1_node = s.pages.first;
    unsafe { (*super::Node::page(page1_node)).pause_integrity_checks(true) };
    for _ in 0..(unsafe { super::Node::capacity(page1_node).rows - super::Node::rows(page1_node) })
    {
        let _ = s.grow().unwrap();
    }
    unsafe { (*super::Node::page(page1_node)).pause_integrity_checks(false) };
    let _ = s.grow().unwrap();
    assert!(!core::ptr::eq(s.pages.first, s.pages.last));

    let mut node = s.pages.first;

    // Write content to verify it's preserved
    {
        let page = unsafe { super::Node::page(node) };
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
    while unsafe { (*super::Node::page(node)).memory_len } <= std_size() {
        node = s
            .increase_capacity(node, Some(IncreaseCapacity::GraphemeBytes))
            .unwrap();
    }
    assert!(unsafe { (*super::Node::page(node)).memory_len } > std_size());
    let oversized_len = unsafe { (*super::Node::page(node)).memory_len };
    let original_size = unsafe { (*super::Node::page(node)).size };
    let second_node = unsafe { (*node).next };

    // Set dirty flag after increaseCapacity
    unsafe { (*super::Node::page(node)).dirty = true };

    // Compact the page
    let new_node = s.compact(node).unwrap();
    assert!(new_node.is_some());
    let new_node = new_node.unwrap();

    // Verify memory is smaller
    assert!(unsafe { (*super::Node::page(new_node)).memory_len } < oversized_len);

    // Verify size preserved
    assert_eq!(original_size.rows, unsafe { super::Node::rows(new_node) });
    assert_eq!(original_size.cols, unsafe { super::Node::cols(new_node) });

    // Verify dirty flag preserved
    assert!(unsafe { (*super::Node::page(new_node)).dirty });

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
        let page = unsafe { &(*super::Node::page(new_node)) };
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
/// Write highly compressible rows until the list holds at least `pages` pages.
fn fill_compressible_history(s: &mut PageList, pages: usize) {
    let mut iterations = 0usize;
    while s.total_pages() < pages {
        let _ = s.grow().unwrap();
        // Write a compressible marker so the page is not entirely zeroes (a
        // fully-zero page compresses too, but this keeps the workload honest).
        let node = s.pages.last;
        unsafe {
            let page = super::Node::page(node);
            let rows = (*page).size.rows as usize;
            let cols = (*page).size.cols as usize;
            if rows > 0 && cols > 0 {
                let rac = (*page).get_row_and_cell(0, rows - 1);
                *rac.cell = cell_codepoint('a' as u32);
            }
        }
        iterations += 1;
        assert!(
            iterations < 100_000,
            "history did not grow to {pages} pages"
        );
    }
}

/// Fill the current tail page to capacity without allocating a successor.
/// Capturing the tail before the loop makes this stop at the allocation
/// boundary needed by bounded-pruning tests.
///
/// PORTS: oracle@main PageList.zig `fillLastPageForTest` (f1a5fab45).
#[allow(dead_code)] // first used by the C3 bounded-pruning tests
fn fill_last_page_for_test(s: &mut PageList) {
    let last = s.pages.last;
    while unsafe { super::Node::rows(last) } < unsafe { super::Node::capacity(last) }.rows {
        let _ = s.grow().unwrap();
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
#[test]
fn pagelist_row_erasure_renews_affected_page_generations() {
    let mut s = PageList::init(80, 24, None).unwrap();
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
        unsafe { super::Node::rows(first) } as usize + 1,
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
    let mut s = PageList::init(80, 24, None).unwrap();

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
#[test]
fn pagelist_memory_stats_accounts_compressed_and_resident_bytes() {
    let mut s = PageList::init(80, 24, Some(10 * 1024 * 1024)).unwrap();
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
#[test]
fn pagelist_compress_incremental_converges_to_complete() {
    let mut s = PageList::init(80, 24, Some(10 * 1024 * 1024)).unwrap();
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
        unsafe { super::Node::storage(s.pages.first) },
        Storage::Compressed
    );
    s.deinit();
}

/// `Drain` performs incremental steps until the pass and its verification pass
/// finish, in one call.
#[test]
fn pagelist_compress_drain_finishes_in_one_call() {
    let mut s = PageList::init(80, 24, Some(10 * 1024 * 1024)).unwrap();
    fill_compressible_history(&mut s, 6);
    assert_eq!(
        s.compress(CompressionMode::Drain),
        IncrementalCompressionResult::Complete
    );
    assert_eq!(
        unsafe { super::Node::storage(s.pages.first) },
        Storage::Compressed
    );
    s.deinit();
}

/// PageList mutations bump the activity token so schedulers know to recompress.
#[test]
fn pagelist_mutations_mark_compression_activity() {
    let mut s = PageList::init(80, 24, Some(10 * 1024 * 1024)).unwrap();
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
#[test]
fn pagelist_compress_incremental_restarts_after_list_mutation() {
    let mut s = PageList::init(80, 24, Some(10 * 1024 * 1024)).unwrap();
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
#[test]
fn pagelist_compress_full_mode_leaves_no_continuation() {
    let mut s = PageList::init(80, 24, Some(10 * 1024 * 1024)).unwrap();
    fill_compressible_history(&mut s, 4);

    assert_eq!(
        s.compress(CompressionMode::Full),
        IncrementalCompressionResult::Complete
    );
    assert_eq!(
        unsafe { super::Node::storage(s.pages.first) },
        Storage::Compressed
    );
    assert_eq!(s.page_compression.last_serial, None);
    assert!(!s.page_compression.flags.verifying);
    assert!(!s.page_compression.flags.did_compress);
    s.deinit();
}

/// A cold history page compresses, its mapping is discarded, and reading it
/// back restores identical bytes.
#[test]
fn pagelist_compress_page_round_trips_a_cold_history_page() {
    let mut s = PageList::init(80, 24, Some(10 * 1024 * 1024)).unwrap();
    fill_compressible_history(&mut s, 3);

    let node = s.pages.first;
    let (ptr, len, expected) = unsafe {
        let p = super::Node::page(node);
        let ptr = (*p).memory_ptr() as *mut u8;
        let len = (*p).memory_len();
        (ptr, len, core::slice::from_raw_parts(ptr, len).to_vec())
    };

    assert!(s.compress_page(node));
    assert_eq!(unsafe { super::Node::storage(node) }, Storage::Compressed);
    // The test build of `mem::decommit` zeroes instead of calling madvise.
    assert!(unsafe { core::slice::from_raw_parts(ptr, len) }
        .iter()
        .all(|&b| b == 0));

    let restored = unsafe { super::Node::page(node) };
    assert_eq!(unsafe { (*restored).memory_ptr() }, ptr as *const u8);
    assert_eq!(
        unsafe { core::slice::from_raw_parts(ptr, len) },
        &expected[..]
    );
    s.deinit();
}

/// Compressing twice is a no-op: `compress_page` only accepts resident nodes.
#[test]
fn pagelist_compress_page_rejects_an_already_compressed_node() {
    let mut s = PageList::init(80, 24, Some(10 * 1024 * 1024)).unwrap();
    fill_compressible_history(&mut s, 3);
    let node = s.pages.first;
    assert!(s.compress_page(node));
    assert!(!s.compress_page(node));
    let _ = unsafe { super::Node::page(node) };
    s.deinit();
}

/// A failing strict decommit leaves the node resident and its mapping intact.
///
/// PORT NOTE (WS7 B10): tripwire injection is compiled out when
/// `debug_assertions` is off, so this test is inert under `--release`. It is
/// ignored there rather than left to fail (the other tripwire tests in this file
/// predate that convention and are part of the known-red `--release` set).
#[cfg_attr(
    not(debug_assertions),
    ignore = "tripwire failure injection is disabled without debug_assertions"
)]
#[test]
fn pagelist_compress_page_decommit_failure_leaves_the_page_resident() {
    use rustty_util::tripwire::ResetMode;

    let mut s = PageList::init(80, 24, Some(10 * 1024 * 1024)).unwrap();
    fill_compressible_history(&mut s, 3);

    let node = s.pages.first;
    let (ptr, len, expected) = unsafe {
        let p = super::Node::page(node);
        let ptr = (*p).memory_ptr() as *mut u8;
        let len = (*p).memory_len();
        (ptr, len, core::slice::from_raw_parts(ptr, len).to_vec())
    };

    super::compress::COMPRESS_PAGE_TW.with(|tw| {
        tw.borrow_mut().errorAlways(
            super::compress::CompressPageFp::Decommit,
            super::compress::DecommitFailed,
        )
    });
    assert!(!s.compress_page(node));
    assert_eq!(unsafe { super::Node::storage(node) }, Storage::Resident);
    assert_eq!(
        unsafe { core::slice::from_raw_parts(ptr, len) },
        &expected[..]
    );

    // With the tripwire disarmed, the same page compresses normally.
    super::compress::COMPRESS_PAGE_TW.with(|tw| tw.borrow_mut().end(ResetMode::Reset).unwrap());
    assert!(s.compress_page(node));
    let _ = unsafe { super::Node::page(node) };
    s.deinit();
}

/// A full pass compresses every complete offscreen history page and leaves the
/// viewport and active pages resident.
#[test]
fn pagelist_compress_full_leaves_viewport_and_active_resident() {
    let mut s = PageList::init(80, 24, Some(10 * 1024 * 1024)).unwrap();
    fill_compressible_history(&mut s, 4);

    s.compress_full();

    let active = s.get_top_left(point::Tag::Active).node;
    let viewport_first = s.get_top_left(point::Tag::Viewport).node;
    let viewport_last = s.get_bottom_right(point::Tag::Viewport).unwrap().node;
    assert_eq!(unsafe { super::Node::storage(active) }, Storage::Resident);
    assert_eq!(
        unsafe { super::Node::storage(viewport_first) },
        Storage::Resident
    );
    assert_eq!(
        unsafe { super::Node::storage(viewport_last) },
        Storage::Resident
    );
    assert_eq!(
        unsafe { super::Node::storage(s.pages.first) },
        Storage::Compressed
    );
    s.deinit();
}

/// A node built from a compressed representation restores to the SAME mapping
/// and yields identical bytes; `page()` transparently restores.
#[test]
fn pagelist_node_restore_preserve_recovers_the_same_mapping() {
    let mut s = PageList::init(20, 5, Some(1000)).unwrap();
    let node = s.pages.first;
    unsafe {
        let page = super::Node::page(node);
        let ptr = (*page).memory_ptr() as *mut u8;
        let len = (*page).memory_len();
        let expected: Vec<u8> = core::slice::from_raw_parts(ptr, len).to_vec();

        let mut scratch = vec![0u8; crate::compress::required_scratch(len).unwrap()];
        let mut table: crate::compress::lz4::HashTable = [0; crate::compress::lz4::HASH_TABLE_LEN];
        let compressed =
            crate::compress::CompressedPage::init(page as *const Page, &mut scratch, &mut table)
                .unwrap()
                .unwrap();

        // Model a successful decommit, then publish the compressed state.
        core::ptr::write_bytes(ptr, 0, len);
        (*node).data = NodeData::Compressed(compressed);
        assert_eq!(super::Node::storage(node), Storage::Compressed);
        assert!(super::Node::page_if_resident(node).is_null());

        // `page()` restores transparently.
        let restored = super::Node::page(node);
        assert_eq!(super::Node::storage(node), Storage::Resident);
        assert_eq!((*restored).memory_ptr(), ptr as *const u8);
        assert_eq!(core::slice::from_raw_parts(ptr, len), &expected[..]);
    }
    s.deinit();
}

/// `page_preserving_state` reads a compressed node WITHOUT restoring it.
#[test]
fn pagelist_page_preserving_state_leaves_a_compressed_node_compressed() {
    let mut s = PageList::init(20, 5, Some(1000)).unwrap();
    let node = s.pages.first;
    unsafe {
        let page = super::Node::page(node);
        let ptr = (*page).memory_ptr() as *mut u8;
        let len = (*page).memory_len();
        let expected: Vec<u8> = core::slice::from_raw_parts(ptr, len).to_vec();

        let mut scratch = vec![0u8; crate::compress::required_scratch(len).unwrap()];
        let mut table: crate::compress::lz4::HashTable = [0; crate::compress::lz4::HASH_TABLE_LEN];
        let compressed =
            crate::compress::CompressedPage::init(page as *const Page, &mut scratch, &mut table)
                .unwrap()
                .unwrap();
        core::ptr::write_bytes(ptr, 0, len);
        (*node).data = NodeData::Compressed(compressed);

        let mut preserved = super::Node::page_preserving_state(node);
        let p = preserved.page();
        assert_ne!((*p).memory_ptr(), ptr as *const u8);
        assert_eq!(
            core::slice::from_raw_parts((*p).memory_ptr(), len),
            &expected[..]
        );
        preserved.deinit();

        // Still compressed, and the discarded mapping is untouched.
        assert_eq!(super::Node::storage(node), Storage::Compressed);
        assert!(core::slice::from_raw_parts(ptr, len)
            .iter()
            .all(|&b| b == 0));

        // Restore so deinit's accounting sees a resident page.
        let _ = super::Node::page(node);
    }
    s.deinit();
}

/// PORTS: oracle@main PageList.zig `Node.Owned` (16e4b5e98) — every node
/// records how its backing memory was allocated. A heap-owned page smaller than
/// the standard pool item must still be freed to the heap, never returned to
/// the pool.
#[test]
fn pagelist_node_ownership_is_explicit() {
    let mut s = PageList::init(80, 24, Some(1000)).unwrap();
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
    let mut s = PageList::init(80, 24, Some(0)).unwrap();

    // Increase capacity until the layout no longer fits a pool item.
    let mut node = s.pages.first;
    while unsafe { (*super::Node::page(node)).memory_len() } <= std_size() {
        node = s
            .increase_capacity(node, Some(IncreaseCapacity::GraphemeBytes))
            .unwrap();
    }
    assert_eq!(unsafe { (*node).owned }, Owned::Heap);

    // A compacted page is SMALLER than a pool item but still heap owned: the
    // ownership tag is the only correct source of truth.
    let compacted = s.compact(node).unwrap().unwrap();
    assert!(unsafe { (*super::Node::page(compacted)).memory_len() } < std_size());
    assert_eq!(unsafe { (*compacted).owned }, Owned::Heap);

    s.deinit();
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
    // SAFETY: `ptr` is a live allocation of `len` bytes.
    let buf = unsafe { core::slice::from_raw_parts_mut(ptr, len) };

    // Fully dirty item.
    buf.iter_mut().for_each(|b| *b = 0xAA);
    super::decommit_pool_item(ptr, len, len);
    assert!(buf.iter().all(|&b| b == 0));

    // Partially dirty item: an item that hosted a page smaller than the item is
    // only dirty up to the page length; the rest is zero by invariant and must
    // remain zero.
    buf[0..1024].iter_mut().for_each(|b| *b = 0xAA);
    super::decommit_pool_item(ptr, len, 1024);
    assert!(buf.iter().all(|&b| b == 0));

    // SAFETY: `ptr`/`layout` are the pair returned by `alloc_zeroed` above and
    // `buf` is dead here.
    unsafe { std::alloc::dealloc(ptr, layout) };
}

// ref: test "PageList destroyed pool page reuse is zeroed"
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION (vt multi-page-PageList intractable, .port/phase-2/MIRI_EXCEPTIONS.md): scribbles over a full std_size page buffer; covered natively + by ASan"
)]
#[test]
fn pagelist_destroyed_pool_page_reuse_is_zeroed() {
    let mut s = PageList::init(80, 24, None).unwrap();

    // Create a page and scribble over its entire backing memory, then destroy
    // it so the buffer returns to the pool free list.
    let node = s
        .create_page(super::CreatePage::new(PageList::initial_capacity(80)))
        .unwrap();
    unsafe { (*super::Node::page(node)).size.rows = 1 };
    let mem_ptr = unsafe { (*super::Node::page(node)).memory_ptr };
    let mem_len = unsafe { (*super::Node::page(node)).memory_len };
    unsafe { core::ptr::write_bytes(mem_ptr, 0xAA, mem_len) };
    s.destroy_node(node);

    // Reusing the buffer must produce a fully valid, zeroed page.
    let node2 = s
        .create_page(super::CreatePage::new(PageList::initial_capacity(80)))
        .unwrap();
    assert_eq!(mem_ptr, unsafe { (*super::Node::page(node2)).memory_ptr });
    unsafe { (*super::Node::page(node2)).size.rows = super::Node::capacity(node2).rows };

    let cells_len = unsafe {
        super::Node::capacity(node2).cols as usize * super::Node::capacity(node2).rows as usize
    };
    {
        let page = unsafe { &(*super::Node::page(node2)) };
        for i in 0..cells_len {
            let rac =
                page.get_row_and_cell(i % page.size.cols as usize, i / page.size.cols as usize);
            let bytes = unsafe {
                core::slice::from_raw_parts(rac.cell as *const u8, core::mem::size_of::<Cell>())
            };
            assert!(bytes.iter().all(|&b| b == 0));
        }
    }
    unsafe { (*super::Node::page(node2)).assert_integrity() };
    s.destroy_node(node2);

    s.deinit();
}

// ref: test "PageList compact after increaseCapacity"
#[test]
fn pagelist_compact_after_increase_capacity() {
    let mut s = PageList::init(80, 24, Some(0)).unwrap();

    let mut node = s.pages.first;

    // Grow the page capacity. The content is unchanged, so compaction should
    // always shrink it back down to an exact-size heap page.
    node = s
        .increase_capacity(node, Some(IncreaseCapacity::GraphemeBytes))
        .unwrap();
    let grown_len = unsafe { (*super::Node::page(node)).memory_len };

    let new_node = s.compact(node).unwrap().unwrap();
    assert_eq!(unsafe { (*new_node).owned }, Owned::Heap);
    assert!(unsafe { (*super::Node::page(new_node)).memory_len } < grown_len);
    assert!(unsafe { (*super::Node::page(new_node)).memory_len } < std_size());

    s.deinit();
}

// =====================================================================
// split
// =====================================================================

// ref: test "PageList split at middle row"
#[test]
fn pagelist_split_at_middle_row() {
    let mut s = PageList::init(10, 10, Some(0)).unwrap();

    {
        let page = unsafe { super::Node::page(s.pages.first) };

        // Write content to rows: row 0 gets codepoint 0, row 1 gets 1, etc.
        for y in 0..unsafe { (*page).size.rows } as usize {
            let rac = unsafe { (*page).get_row_and_cell(0, y) };
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

    let first_page = unsafe { &(*super::Node::page(s.pages.first)) };
    let second_page = unsafe { &(*super::Node::page((*s.pages.first).next)) };

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

// ref: test "PageList split at row 0 is no-op"
#[test]
fn pagelist_split_at_row_0_is_no_op() {
    let mut s = PageList::init(10, 10, Some(0)).unwrap();

    let page = unsafe { super::Node::page(s.pages.first) };

    // Write content to all rows
    for y in 0..unsafe { (*page).size.rows } as usize {
        let rac = unsafe { (*page).get_row_and_cell(0, y) };
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
    let page = unsafe { &(*super::Node::page(s.pages.first)) };
    assert_eq!(10, page.size.rows as usize);
    for y in 0..10 {
        let rac = page.get_row_and_cell(0, y);
        assert_eq!(y as u32, unsafe { (*rac.cell).content_codepoint() });
    }

    s.deinit();
}

// ref: test "PageList split at last row"
#[test]
fn pagelist_split_at_last_row() {
    let mut s = PageList::init(10, 10, Some(0)).unwrap();

    {
        let page = unsafe { super::Node::page(s.pages.first) };

        // Write content to all rows
        for y in 0..unsafe { (*page).size.rows } as usize {
            let rac = unsafe { (*page).get_row_and_cell(0, y) };
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

    let first_page = unsafe { &(*super::Node::page(s.pages.first)) };
    let second_page = unsafe { &(*super::Node::page((*s.pages.first).next)) };

    // First page should have 9 rows
    assert_eq!(9, first_page.size.rows as usize);
    // Second page should have 1 row
    assert_eq!(1, second_page.size.rows as usize);

    // Verify content in second page (original row 9, now at y=0)
    let rac = second_page.get_row_and_cell(0, 0);
    assert_eq!(9, unsafe { (*rac.cell).content_codepoint() });

    s.deinit();
}

// ref: test "PageList split single row page returns OutOfSpace"
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

// ref: test "PageList split moves tracked pins"
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

// ref: test "PageList split tracked pin before split point unchanged"
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

// ref: test "PageList split tracked pin at split point moves to new page"
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

// ref: test "PageList split multiple tracked pins across regions"
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

// ref: test "PageList split tracked viewport_pin in split region moves correctly"
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

// ref: test "PageList split middle page preserves linked list order"
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
    assert_eq!(4, unsafe { super::Node::rows(page1) } as usize);
    assert_eq!(8, unsafe { super::Node::rows(page2) } as usize);

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
    assert_eq!(4, unsafe { super::Node::rows(first) } as usize);
    assert_eq!(4, unsafe { super::Node::rows(middle) } as usize);
    assert_eq!(4, unsafe { super::Node::rows(last) } as usize);

    s.deinit();
}

// ref: test "PageList split last page makes new page the last"
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
    assert_eq!(5, unsafe { super::Node::rows(last_before_split) } as usize);

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
    assert_eq!(2, unsafe { super::Node::rows(last_before_split) } as usize);
    assert_eq!(3, unsafe { super::Node::rows(new_last) } as usize);

    s.deinit();
}

// ref: test "PageList split first page keeps original as first"
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
    assert_eq!(2, unsafe { super::Node::rows(s.pages.first) } as usize);
    assert_eq!(3, unsafe { super::Node::rows(inserted) } as usize);
    assert_eq!(5, unsafe { super::Node::rows(second_page) } as usize);

    s.deinit();
}

// ref: test "PageList split preserves wrap flags"
#[test]
fn pagelist_split_preserves_wrap_flags() {
    let mut s = PageList::init(10, 10, Some(0)).unwrap();

    {
        let page = unsafe { super::Node::page(s.pages.first) };

        // Set wrap flags on rows that will be in the second page after split
        // Row 5: wrap = true (this is the start of a wrapped line)
        // Row 6: wrap_continuation = true (this continues the wrap)
        // Row 7: wrap = true, wrap_continuation = true (wrapped and continues)
        {
            let rac5 = unsafe { (*page).get_row_and_cell(0, 5) };
            unsafe { (*rac5.row).set_wrap(true) };

            let rac6 = unsafe { (*page).get_row_and_cell(0, 6) };
            unsafe { (*rac6.row).set_wrap_continuation(true) };

            let rac7 = unsafe { (*page).get_row_and_cell(0, 7) };
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

    let second_page = unsafe { &(*super::Node::page((*s.pages.first).next)) };

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

// ref: test "PageList split preserves styled cells"
#[test]
fn pagelist_split_preserves_styled_cells() {
    let mut s = PageList::init(10, 10, Some(0)).unwrap();

    {
        let page = unsafe { super::Node::page(s.pages.first) };

        // Create a style and apply it to cells in rows 5-7 (which will be in the second page)
        let style = Style {
            flags: stylepkg::Flags {
                bold: true,
                ..stylepkg::Flags::default()
            },
            ..Style::default()
        };
        let base = unsafe { (*page).memory_ptr() };
        let style_id = unsafe { (*page).styles.add(base, style) }.unwrap();

        for y in 5..8 {
            let rac = unsafe { (*page).get_row_and_cell(0, y) };
            unsafe {
                let mut c = cell_codepoint(b'S' as u32);
                c.set_style_id(style_id);
                *rac.cell = c;
                (*rac.row).set_styled(true);
                (*page).styles.use_id(base, style_id);
            }
        }
        // Release the extra ref from add
        unsafe { (*page).styles.release(base, style_id) };
    }

    // Split at row 5
    let split_pin = Pin {
        node: s.pages.first,
        y: 5,
        x: 0,
        garbage: false,
    };
    s.split(split_pin).unwrap();

    let first_page = unsafe { &(*super::Node::page(s.pages.first)) };
    let second_page = unsafe { &(*super::Node::page((*s.pages.first).next)) };

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

// PORT NOTE (rustty-only, NOT a reference test — VALIDATION_REPORT §3e/§3c.9):
// pins the ERROR-PATH integrity gate of `erase_row`. Zig's
// `defer self.assertIntegrity()` runs on the `try cloneRowFrom` ERROR return;
// the pre-fix Rust body skipped the gate via `?`, so an OOM at exactly the
// point the oracle checks for corruption went ungated. The test drives the
// real error path (cross-page clone into a page whose style set is FULL →
// `CloneFromError::StyleSet*` → `Err(OutOfMemory)`) with a deliberately
// corrupted `total_rows`, and requires the integrity gate to PANIC on that
// error exit (pre-fix: the call returned `Err` silently — fail-first).
#[test]
// WS18 release lane: this test drives a slow-runtime-safety mechanism (the
// pagelist integrity gate / `pin_is_valid`), which the port compiles out when
// `debug_assertions` is off — faithful to the reference's
// `build_options.slow_runtime_safety`, under which the oracle's test only
// exists. Debug-only; it still runs and passes there.
#[cfg_attr(not(debug_assertions), ignore)]
fn pagelist_erase_row_error_path_runs_integrity_gate() {
    let mut s = PageList::init(10, 10, Some(0)).unwrap();

    // Give the (future) second page's top row a styled cell, then split so
    // pages.first (rows 0-4) has a next page whose row 0 carries the style.
    {
        let page = unsafe { super::Node::page(s.pages.first) };
        let base = unsafe { (*page).memory_ptr() };
        let style = Style {
            flags: stylepkg::Flags {
                bold: true,
                ..stylepkg::Flags::default()
            },
            ..Style::default()
        };
        let style_id = unsafe { (*page).styles.add(base, style) }.unwrap();
        let rac = unsafe { (*page).get_row_and_cell(0, 5) };
        unsafe {
            let mut c = cell_codepoint(b'S' as u32);
            c.set_style_id(style_id);
            *rac.cell = c;
            (*rac.row).set_styled(true);
        }
    }
    let split_pin = Pin {
        node: s.pages.first,
        y: 5,
        x: 0,
        garbage: false,
    };
    s.split(split_pin).unwrap();
    assert_eq!(2, s.total_pages());

    // Fill the FIRST page's style set to capacity with unique (orphan) styles
    // so the erase's cross-page clone cannot add the styled row's style there.
    {
        let page = unsafe { super::Node::page(s.pages.first) };
        let base = unsafe { (*page).memory_ptr() };
        let mut n: u32 = 1;
        loop {
            let style = Style {
                fg_color: stylepkg::Color::Rgb(crate::color::RGB {
                    r: (n & 0xFF) as u8,
                    g: ((n >> 8) & 0xFF) as u8,
                    b: ((n >> 16) & 0xFF) as u8,
                }),
                ..Style::default()
            };
            if unsafe { (*page).styles.add(base, style) }.is_err() {
                break;
            }
            n += 1;
        }
    }

    // Corrupt a PageList invariant the integrity gate checks (cached row count),
    // then drive the erase ERROR path: the gate must fire ON THE ERROR EXIT.
    s.total_rows += 1;
    let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        s.erase_row(point::Point::Active(point::Coordinate { x: 0, y: 0 }))
    }));
    match panicked {
        // post-fix: the defer-image integrity gate panicked on the error exit
        Err(_) => {}
        Ok(result) => panic!(
            "erase_row must run the integrity gate on its error exit \
             (Zig `defer assertIntegrity()`); got silent {result:?}"
        ),
    }

    // Undo the corruption so deinit-time state is consistent again.
    s.total_rows -= 1;
    s.deinit();
}

// ref: test "PageList split preserves grapheme clusters"
#[test]
fn pagelist_split_preserves_grapheme_clusters() {
    let mut s = PageList::init(10, 10, Some(0)).unwrap();

    {
        let page = unsafe { super::Node::page(s.pages.first) };

        // Add a grapheme cluster to row 6 (will be row 1 in second page after split at 5)
        {
            let rac = unsafe { (*page).get_row_and_cell(0, 6) };
            unsafe {
                *rac.cell = cell_codepoint(0x1F468); // Man emoji
            }
            unsafe {
                (*page).set_graphemes(
                    rac.row,
                    rac.cell,
                    &[
                        0x200D,  // ZWJ
                        0x1F469, // Woman emoji
                    ],
                )
            }
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

    let first_page = unsafe { &(*super::Node::page(s.pages.first)) };
    let second_page = unsafe { &(*super::Node::page((*s.pages.first).next)) };

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

// ref: test "PageList split preserves hyperlinks"
#[test]
fn pagelist_split_preserves_hyperlinks() {
    let mut s = PageList::init(10, 10, Some(0)).unwrap();

    let hyperlink_id = {
        let page = unsafe { super::Node::page(s.pages.first) };

        // Add a hyperlink to row 7 (will be row 2 in second page after split at 5)
        let hyperlink_id = unsafe {
            (*page).insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Implicit(0),
                uri: b"https://example.com".to_vec(),
            })
        }
        .unwrap();
        {
            let rac = unsafe { (*page).get_row_and_cell(0, 7) };
            unsafe {
                *rac.cell = cell_codepoint(b'L' as u32);
            }
            unsafe { (*page).set_hyperlink(rac.row, rac.cell, hyperlink_id) }.unwrap();
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

    let first_page = unsafe { &(*super::Node::page(s.pages.first)) };
    let second_page = unsafe { &(*super::Node::page((*s.pages.first).next)) };

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
    let page = unsafe { super::Node::page(s.pages.first) };
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 1) };
        unsafe {
            (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt);
        }
    }
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 5) };
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
    let page = unsafe { super::Node::page(s.pages.first) };
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 1) };
        unsafe {
            (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt);
        }
    }
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 5) };
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
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION (vt multi-page-PageList intractable, .port/phase-2/MIRI_EXCEPTIONS.md): builds/reflows multi-page PageLists that exceed Miri's interpreter budget (>20min); covered natively + by ASan (all PageList tests ~18s)"
)]
#[test]
fn pagelist_grow_prune_scrollback() {
    // Use std_size to limit scrollback so pruning is triggered.
    let mut s = PageList::init(80, 24, Some(std_size())).unwrap();

    // Grow to capacity
    let page1_node = s.pages.last;
    let (page1_cap_rows, page1_size_rows) = {
        let page1 = unsafe { &(*super::Node::page(page1_node)) };
        (page1.capacity.rows, page1.size.rows)
    };
    for _ in 0..(page1_cap_rows - page1_size_rows) {
        assert!(s.grow().unwrap().is_none());
    }

    // Grow and allocate one more page. Then fill that page up.
    let page2_node = s.grow().unwrap().unwrap();
    let (page2_cap_rows, page2_size_rows) = {
        let page2 = unsafe { &(*super::Node::page(page2_node)) };
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
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION (vt multi-page-PageList intractable, .port/phase-2/MIRI_EXCEPTIONS.md): builds/reflows multi-page PageLists that exceed Miri's interpreter budget (>20min); covered natively + by ASan (all PageList tests ~18s)"
)]
#[test]
fn pagelist_grow_prune_scrollback_with_viewport_pin_not_in_pruned_page() {
    // Use std_size to limit scrollback so pruning is triggered.
    let mut s = PageList::init(80, 24, Some(std_size())).unwrap();

    // Grow to capacity of first page
    let page1_node = s.pages.last;
    let (page1_cap_rows, page1_size_rows) = {
        let page1 = unsafe { &(*super::Node::page(page1_node)) };
        (page1.capacity.rows, page1.size.rows)
    };
    for _ in 0..(page1_cap_rows - page1_size_rows) {
        assert!(s.grow().unwrap().is_none());
    }

    // Grow and allocate second page, then fill it up
    let page2_node = s.grow().unwrap().unwrap();
    let (page2_cap_rows, page2_size_rows) = {
        let page2 = unsafe { &(*super::Node::page(page2_node)) };
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

// ref: test "PageList eraseRows invalidates viewport offset cache"
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION (vt multi-page-PageList intractable, .port/phase-2/MIRI_EXCEPTIONS.md): builds/reflows multi-page PageLists that exceed Miri's interpreter budget (>20min); covered natively + by ASan (all PageList tests ~18s)"
)]
#[test]
fn pagelist_eraseRows_invalidates_viewport_offset_cache() {
    let mut s = PageList::init(80, 24, None).unwrap();

    // Grow so we take up several pages worth of history
    let page_cap_rows = unsafe { super::Node::capacity(s.pages.last).rows };
    {
        let mut cur_page = s.pages.last;
        for _ in 0..(page_cap_rows as usize * 3) {
            if let Some(new_page) = s.grow().unwrap() {
                cur_page = new_page;
            }
        }
        let _ = cur_page;
    }

    // Scroll back to create a pinned viewport somewhere in the middle
    // of the scrollback
    let pin_y = page_cap_rows;
    s.scroll(Scroll::Pin(
        s.pin(point::Point::Screen(point::Coordinate {
            x: 0,
            y: pin_y as u32,
        }))
        .unwrap(),
    ));
    assert!(s.viewport == super::Viewport::Pin);
    assert_eq!(
        Scrollbar {
            total: s.total_rows,
            offset: pin_y as usize,
            len: s.rows as usize,
        },
        s.scrollbar(),
    );

    // Erase some history rows BEFORE the viewport pin.
    // This removes rows from before our pin, which changes its absolute
    // offset from the top, but the cache is not invalidated.
    let rows_to_erase = page_cap_rows / 2;
    s.erase_history(Some(point::Point::History(point::Coordinate {
        x: 0,
        y: (rows_to_erase - 1) as u32,
    })));

    assert_eq!(
        Scrollbar {
            total: s.total_rows,
            offset: (pin_y - rows_to_erase) as usize,
            len: s.rows as usize,
        },
        s.scrollbar(),
    );

    s.deinit();
}

// ref: test "PageList eraseRow invalidates viewport offset cache"
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION (vt multi-page-PageList intractable, .port/phase-2/MIRI_EXCEPTIONS.md): builds/reflows multi-page PageLists that exceed Miri's interpreter budget (>20min); covered natively + by ASan (all PageList tests ~18s)"
)]
#[test]
fn pagelist_eraseRow_invalidates_viewport_offset_cache() {
    let mut s = PageList::init(80, 24, None).unwrap();

    // Grow so we take up several pages worth of history
    let page_cap_rows = unsafe { super::Node::capacity(s.pages.last).rows };
    {
        let mut cur_page = s.pages.last;
        for _ in 0..(page_cap_rows as usize * 3) {
            if let Some(new_page) = s.grow().unwrap() {
                cur_page = new_page;
            }
        }
        let _ = cur_page;
    }

    // Scroll back to create a pinned viewport somewhere in the middle
    // of the scrollback
    let pin_y = page_cap_rows;
    s.scroll(Scroll::Pin(
        s.pin(point::Point::Screen(point::Coordinate {
            x: 0,
            y: pin_y as u32,
        }))
        .unwrap(),
    ));
    assert!(s.viewport == super::Viewport::Pin);
    assert_eq!(
        Scrollbar {
            total: s.total_rows,
            offset: pin_y as usize,
            len: s.rows as usize,
        },
        s.scrollbar(),
    );

    // Erase a single row from the history BEFORE the viewport pin.
    // This removes one row from before our pin, which changes its absolute
    // offset from the top by 1, but the cache is not invalidated.
    s.erase_row(point::Point::History(point::Coordinate { x: 0, y: 0 }))
        .unwrap();

    assert_eq!(
        Scrollbar {
            total: s.total_rows,
            offset: (pin_y - 1) as usize,
            len: s.rows as usize,
        },
        s.scrollbar(),
    );

    s.deinit();
}

// ref: test "PageList eraseRowBounded invalidates viewport offset cache"
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION (vt multi-page-PageList intractable, .port/phase-2/MIRI_EXCEPTIONS.md): builds/reflows multi-page PageLists that exceed Miri's interpreter budget (>20min); covered natively + by ASan (all PageList tests ~18s)"
)]
#[test]
fn pagelist_eraseRowBounded_invalidates_viewport_offset_cache() {
    let mut s = PageList::init(80, 24, None).unwrap();

    // Grow so we take up several pages worth of history
    let page_cap_rows = unsafe { super::Node::capacity(s.pages.last).rows };
    {
        let mut cur_page = s.pages.last;
        for _ in 0..(page_cap_rows as usize * 3) {
            if let Some(new_page) = s.grow().unwrap() {
                cur_page = new_page;
            }
        }
        let _ = cur_page;
    }

    // Scroll back to create a pinned viewport somewhere in the middle
    // of the scrollback
    let pin_y: u16 = 4;
    s.scroll(Scroll::Pin(
        s.pin(point::Point::Screen(point::Coordinate {
            x: 0,
            y: pin_y as u32,
        }))
        .unwrap(),
    ));
    assert!(s.viewport == super::Viewport::Pin);
    assert_eq!(
        Scrollbar {
            total: s.total_rows,
            offset: pin_y as usize,
            len: s.rows as usize,
        },
        s.scrollbar(),
    );

    // Erase a row from the history BEFORE the viewport pin with a bounded
    // shift. This removes one row from before our pin, which changes its
    // absolute offset from the top by 1, but the cache is not invalidated.
    s.erase_row_bounded(point::Point::History(point::Coordinate { x: 0, y: 0 }), 10)
        .unwrap();

    // Verify the scrollbar reflects the change (offset decreased by 1)
    assert_eq!(
        Scrollbar {
            total: s.total_rows,
            offset: (pin_y - 1) as usize,
            len: s.rows as usize,
        },
        s.scrollbar(),
    );

    s.deinit();
}

// ref: test "PageList eraseRowBounded multi-page invalidates viewport offset cache"
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION (vt multi-page-PageList intractable, .port/phase-2/MIRI_EXCEPTIONS.md): builds/reflows multi-page PageLists that exceed Miri's interpreter budget (>20min); covered natively + by ASan (all PageList tests ~18s)"
)]
#[test]
fn pagelist_eraseRowBounded_multi_page_invalidates_viewport_offset_cache() {
    let mut s = PageList::init(80, 24, None).unwrap();

    // Grow so we take up several pages worth of history
    let page_cap_rows = unsafe { super::Node::capacity(s.pages.last).rows };
    {
        let mut cur_page = s.pages.last;
        for _ in 0..(page_cap_rows as usize * 3) {
            if let Some(new_page) = s.grow().unwrap() {
                cur_page = new_page;
            }
        }
        let _ = cur_page;
    }

    // Scroll back to create a pinned viewport somewhere in the middle
    // of the scrollback, after the first page
    let pin_y = page_cap_rows + 1;
    s.scroll(Scroll::Pin(
        s.pin(point::Point::Screen(point::Coordinate {
            x: 0,
            y: pin_y as u32,
        }))
        .unwrap(),
    ));
    assert!(s.viewport == super::Viewport::Pin);
    assert_eq!(
        Scrollbar {
            total: s.total_rows,
            offset: pin_y as usize,
            len: s.rows as usize,
        },
        s.scrollbar(),
    );

    // Erase a row from the beginning of history with a limit that spans
    // across multiple pages. This ensures we hit the code path where
    // eraseRowBounded finds the limit boundary in a subsequent page.
    let limit = page_cap_rows as usize + 10;
    s.erase_row_bounded(
        point::Point::History(point::Coordinate { x: 0, y: 0 }),
        limit,
    )
    .unwrap();

    // Verify the scrollbar reflects the change (offset decreased by 1)
    assert_eq!(
        Scrollbar {
            total: s.total_rows,
            offset: (pin_y - 1) as usize,
            len: s.rows as usize,
        },
        s.scrollbar(),
    );

    s.deinit();
}

// ref: test "PageList eraseRowBounded full page shift invalidates viewport offset cache"
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION (vt multi-page-PageList intractable, .port/phase-2/MIRI_EXCEPTIONS.md): builds/reflows multi-page PageLists that exceed Miri's interpreter budget (>20min); covered natively + by ASan (all PageList tests ~18s)"
)]
#[test]
fn pagelist_eraseRowBounded_full_page_shift_invalidates_viewport_offset_cache() {
    let mut s = PageList::init(80, 24, None).unwrap();

    // Grow so we take up several pages worth of history
    let page_cap_rows = unsafe { super::Node::capacity(s.pages.last).rows };
    {
        let mut cur_page = s.pages.last;
        for _ in 0..(page_cap_rows as usize * 4) {
            if let Some(new_page) = s.grow().unwrap() {
                cur_page = new_page;
            }
        }
        let _ = cur_page;
    }

    // Scroll back to create a pinned viewport somewhere well beyond
    // the first two pages
    let pin_y: u16 = 5;
    s.scroll(Scroll::Pin(
        s.pin(point::Point::Screen(point::Coordinate {
            x: 0,
            y: pin_y as u32,
        }))
        .unwrap(),
    ));
    assert!(s.viewport == super::Viewport::Pin);
    assert_eq!(
        Scrollbar {
            total: s.total_rows,
            offset: pin_y as usize,
            len: s.rows as usize,
        },
        s.scrollbar(),
    );

    // Erase a row from the beginning of history with a limit that is
    // larger than multiple full pages. This ensures we hit the code path
    // where eraseRowBounded continues looping through entire pages,
    // rotating all rows in each page until it reaches the limit or
    // runs out of pages.
    let limit = page_cap_rows as usize * 2 + 10;
    s.erase_row_bounded(
        point::Point::History(point::Coordinate { x: 0, y: 0 }),
        limit,
    )
    .unwrap();

    // Verify the scrollbar reflects the change (offset decreased by 1)
    assert_eq!(
        Scrollbar {
            total: s.total_rows,
            offset: (pin_y - 1) as usize,
            len: s.rows as usize,
        },
        s.scrollbar(),
    );

    s.deinit();
}

// ref: test "PageList eraseRowBounded exhausts pages invalidates viewport offset cache"
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION (vt multi-page-PageList intractable, .port/phase-2/MIRI_EXCEPTIONS.md): builds/reflows multi-page PageLists that exceed Miri's interpreter budget (>20min); covered natively + by ASan (all PageList tests ~18s)"
)]
#[test]
fn pagelist_eraseRowBounded_exhausts_pages_invalidates_viewport_offset_cache() {
    let mut s = PageList::init(80, 24, None).unwrap();

    // Grow so we take up several pages worth of history
    let page_cap_rows = unsafe { super::Node::capacity(s.pages.last).rows };
    {
        let mut cur_page = s.pages.last;
        for _ in 0..(page_cap_rows as usize * 3) {
            if let Some(new_page) = s.grow().unwrap() {
                cur_page = new_page;
            }
        }
        let _ = cur_page;
    }

    // Our total rows should include history
    let total_rows_before = s.total_rows();
    assert!(total_rows_before > s.rows as usize);

    // Scroll back to create a pinned viewport somewhere in the history,
    // well after the erase will complete
    let pin_y = page_cap_rows as usize * 2 + 10;
    s.scroll(Scroll::Pin(
        s.pin(point::Point::Screen(point::Coordinate {
            x: 0,
            y: pin_y as u32,
        }))
        .unwrap(),
    ));
    assert!(s.viewport == super::Viewport::Pin);
    assert_eq!(
        Scrollbar {
            total: s.total_rows,
            offset: pin_y,
            len: s.rows as usize,
        },
        s.scrollbar(),
    );

    // Erase a row from the beginning of history with a limit that is
    // LARGER than all remaining pages combined. This ensures we exhaust
    // all pages in the while loop and reach the cleanup code after the loop.
    let limit = total_rows_before * 2;
    s.erase_row_bounded(
        point::Point::History(point::Coordinate { x: 0, y: 0 }),
        limit,
    )
    .unwrap();

    // Verify the scrollbar reflects the change (offset decreased by 1)
    assert_eq!(
        Scrollbar {
            total: s.total_rows,
            offset: pin_y - 1,
            len: s.rows as usize,
        },
        s.scrollbar(),
    );

    s.deinit();
}

// ref: test "PageList increaseCapacity after col shrink"
#[test]
fn pagelist_increaseCapacity_after_col_shrink() {
    let mut s = PageList::init(10, 2, Some(0)).unwrap();

    // Shrink columns
    s.resize(super::resize::Resize {
        cols: Some(5),
        rows: None,
        reflow: false,
        cursor: None,
    })
    .unwrap();
    assert_eq!(5, s.cols);

    {
        let page = unsafe { &(*super::Node::page(s.pages.first)) };
        assert_eq!(5, page.size.cols);
        assert!(page.capacity.cols >= 10);
    }

    // Increase capacity
    let _ = s
        .increase_capacity(s.pages.first, Some(IncreaseCapacity::Styles))
        .unwrap();

    {
        let page = unsafe { &(*super::Node::page(s.pages.first)) };
        // size.cols should still be 5, not reverted to capacity.cols
        assert_eq!(5, page.size.cols);
        assert_eq!(5, s.cols);
    }

    s.deinit();
}

// ref: test "PageList pageIterator single page"
#[test]
fn pagelist_pageIterator_single_page() {
    let mut s = PageList::init(80, 24, None).unwrap();

    // The viewport should be within a single page
    assert!(unsafe { (*s.pages.first).next.is_null() });

    // Iterate the active area
    let mut it = s.page_iterator(
        super::iter::Direction::RightDown,
        point::Point::Active(point::Coordinate::default()),
        None,
    );
    {
        let chunk = it.next().unwrap();
        assert!(core::ptr::eq(chunk.node, s.pages.first));
        assert_eq!(0_usize, chunk.start as usize);
        assert_eq!(s.rows as usize, chunk.end as usize);
    }

    // Should only have one chunk
    assert!(it.next().is_none());

    s.deinit();
}

// ref: test "PageList pageIterator two pages"
#[test]
fn pagelist_pageIterator_two_pages() {
    let mut s = PageList::init(80, 24, None).unwrap();

    // Grow to capacity
    let page1_node = s.pages.last;
    let (page1_cap_rows, page1_size_rows) = {
        let page1 = unsafe { &(*super::Node::page(page1_node)) };
        (page1.capacity.rows, page1.size.rows)
    };
    unsafe { (*super::Node::page(page1_node)).pause_integrity_checks(true) };
    for _ in 0..(page1_cap_rows - page1_size_rows) {
        assert!(s.grow().unwrap().is_none());
    }
    unsafe { (*super::Node::page(page1_node)).pause_integrity_checks(false) };
    assert!(s.grow().unwrap().is_some());

    // Iterate the active area
    let mut it = s.page_iterator(
        super::iter::Direction::RightDown,
        point::Point::Active(point::Coordinate::default()),
        None,
    );
    {
        let chunk = it.next().unwrap();
        assert!(core::ptr::eq(chunk.node, s.pages.first));
        let start = unsafe { super::Node::rows(chunk.node) } - s.rows + 1;
        assert_eq!(start, chunk.start);
        assert_eq!(unsafe { super::Node::rows(chunk.node) }, chunk.end);
    }
    {
        let chunk = it.next().unwrap();
        assert!(core::ptr::eq(chunk.node, s.pages.last));
        let start: size::CellCountInt = 0;
        assert_eq!(start, chunk.start);
        assert_eq!(start + 1, chunk.end);
    }
    assert!(it.next().is_none());

    s.deinit();
}

// ref: test "PageList pageIterator history two pages"
#[test]
fn pagelist_pageIterator_history_two_pages() {
    let mut s = PageList::init(80, 24, None).unwrap();

    // Grow to capacity
    let page1_node = s.pages.last;
    let (page1_cap_rows, page1_size_rows) = {
        let page1 = unsafe { &(*super::Node::page(page1_node)) };
        (page1.capacity.rows, page1.size.rows)
    };
    unsafe { (*super::Node::page(page1_node)).pause_integrity_checks(true) };
    for _ in 0..(page1_cap_rows - page1_size_rows) {
        assert!(s.grow().unwrap().is_none());
    }
    unsafe { (*super::Node::page(page1_node)).pause_integrity_checks(false) };
    assert!(s.grow().unwrap().is_some());

    // Iterate the active area
    let mut it = s.page_iterator(
        super::iter::Direction::RightDown,
        point::Point::History(point::Coordinate::default()),
        None,
    );
    {
        let active_tl = s.get_top_left(point::Tag::Active);
        let chunk = it.next().unwrap();
        assert!(core::ptr::eq(chunk.node, s.pages.first));
        let start: size::CellCountInt = 0;
        assert_eq!(start, chunk.start);
        assert_eq!(active_tl.y, chunk.end);
    }
    assert!(it.next().is_none());

    s.deinit();
}

// ref: test "PageList pageIterator reverse single page"
#[test]
fn pagelist_pageIterator_reverse_single_page() {
    let mut s = PageList::init(80, 24, None).unwrap();

    // The viewport should be within a single page
    assert!(unsafe { (*s.pages.first).next.is_null() });

    // Iterate the active area
    let mut it = s.page_iterator(
        super::iter::Direction::LeftUp,
        point::Point::Active(point::Coordinate::default()),
        None,
    );
    {
        let chunk = it.next().unwrap();
        assert!(core::ptr::eq(chunk.node, s.pages.first));
        assert_eq!(0_usize, chunk.start as usize);
        assert_eq!(s.rows as usize, chunk.end as usize);
    }

    // Should only have one chunk
    assert!(it.next().is_none());

    s.deinit();
}

// ref: test "PageList pageIterator reverse two pages"
#[test]
fn pagelist_pageIterator_reverse_two_pages() {
    let mut s = PageList::init(80, 24, None).unwrap();

    // Grow to capacity
    let page1_node = s.pages.last;
    let (page1_cap_rows, page1_size_rows) = {
        let page1 = unsafe { &(*super::Node::page(page1_node)) };
        (page1.capacity.rows, page1.size.rows)
    };
    unsafe { (*super::Node::page(page1_node)).pause_integrity_checks(true) };
    for _ in 0..(page1_cap_rows - page1_size_rows) {
        assert!(s.grow().unwrap().is_none());
    }
    unsafe { (*super::Node::page(page1_node)).pause_integrity_checks(false) };
    assert!(s.grow().unwrap().is_some());

    // Iterate the active area
    let mut it = s.page_iterator(
        super::iter::Direction::LeftUp,
        point::Point::Active(point::Coordinate::default()),
        None,
    );
    let mut count: usize = 0;
    {
        let chunk = it.next().unwrap();
        assert!(core::ptr::eq(chunk.node, s.pages.last));
        let start: size::CellCountInt = 0;
        assert_eq!(start, chunk.start);
        assert_eq!(start + 1, chunk.end);
        count += (chunk.end - chunk.start) as usize;
    }
    {
        let chunk = it.next().unwrap();
        assert!(core::ptr::eq(chunk.node, s.pages.first));
        let start = unsafe { super::Node::rows(chunk.node) } - s.rows + 1;
        assert_eq!(start, chunk.start);
        assert_eq!(unsafe { super::Node::rows(chunk.node) }, chunk.end);
        count += (chunk.end - chunk.start) as usize;
    }
    assert!(it.next().is_none());
    assert_eq!(s.rows as usize, count);

    s.deinit();
}

// ref: test "PageList pageIterator reverse history two pages"
#[test]
fn pagelist_pageIterator_reverse_history_two_pages() {
    let mut s = PageList::init(80, 24, None).unwrap();

    // Grow to capacity
    let page1_node = s.pages.last;
    let (page1_cap_rows, page1_size_rows) = {
        let page1 = unsafe { &(*super::Node::page(page1_node)) };
        (page1.capacity.rows, page1.size.rows)
    };
    unsafe { (*super::Node::page(page1_node)).pause_integrity_checks(true) };
    for _ in 0..(page1_cap_rows - page1_size_rows) {
        assert!(s.grow().unwrap().is_none());
    }
    unsafe { (*super::Node::page(page1_node)).pause_integrity_checks(false) };
    assert!(s.grow().unwrap().is_some());

    // Iterate the active area
    let mut it = s.page_iterator(
        super::iter::Direction::LeftUp,
        point::Point::History(point::Coordinate::default()),
        None,
    );
    {
        let active_tl = s.get_top_left(point::Tag::Active);
        let chunk = it.next().unwrap();
        assert!(core::ptr::eq(chunk.node, s.pages.first));
        let start: size::CellCountInt = 0;
        assert_eq!(start, chunk.start);
        assert_eq!(active_tl.y, chunk.end);
    }
    assert!(it.next().is_none());

    s.deinit();
}

// ref: test "PageList cellIterator"
#[test]
fn pagelist_cellIterator() {
    let mut s = PageList::init(2, 2, Some(0)).unwrap();
    assert!(core::ptr::eq(s.pages.first, s.pages.last));
    let page = unsafe { super::Node::page(s.pages.first) };
    for y in 0..s.rows as usize {
        for x in 0..s.cols as usize {
            let rac = unsafe { (*page).get_row_and_cell(x, y) };
            unsafe {
                *rac.cell = cell_codepoint(x as u32);
            }
        }
    }

    let mut it = s.cell_iterator(
        super::iter::Direction::RightDown,
        point::Point::Screen(point::Coordinate::default()),
        None,
    );
    {
        let p = it.next().unwrap();
        assert_eq!(
            point::Point::Screen(point::Coordinate { x: 0, y: 0 }),
            s.point_from_pin(point::Tag::Screen, p).unwrap()
        );
    }
    {
        let p = it.next().unwrap();
        assert_eq!(
            point::Point::Screen(point::Coordinate { x: 1, y: 0 }),
            s.point_from_pin(point::Tag::Screen, p).unwrap()
        );
    }
    {
        let p = it.next().unwrap();
        assert_eq!(
            point::Point::Screen(point::Coordinate { x: 0, y: 1 }),
            s.point_from_pin(point::Tag::Screen, p).unwrap()
        );
    }
    {
        let p = it.next().unwrap();
        assert_eq!(
            point::Point::Screen(point::Coordinate { x: 1, y: 1 }),
            s.point_from_pin(point::Tag::Screen, p).unwrap()
        );
    }
    assert!(it.next().is_none());

    s.deinit();
}

// ref: test "PageList cellIterator reverse"
#[test]
fn pagelist_cellIterator_reverse() {
    let mut s = PageList::init(2, 2, Some(0)).unwrap();
    assert!(core::ptr::eq(s.pages.first, s.pages.last));
    let page = unsafe { super::Node::page(s.pages.first) };
    for y in 0..s.rows as usize {
        for x in 0..s.cols as usize {
            let rac = unsafe { (*page).get_row_and_cell(x, y) };
            unsafe {
                *rac.cell = cell_codepoint(x as u32);
            }
        }
    }

    let mut it = s.cell_iterator(
        super::iter::Direction::LeftUp,
        point::Point::Screen(point::Coordinate::default()),
        None,
    );
    {
        let p = it.next().unwrap();
        assert_eq!(
            point::Point::Screen(point::Coordinate { x: 1, y: 1 }),
            s.point_from_pin(point::Tag::Screen, p).unwrap()
        );
    }
    {
        let p = it.next().unwrap();
        assert_eq!(
            point::Point::Screen(point::Coordinate { x: 0, y: 1 }),
            s.point_from_pin(point::Tag::Screen, p).unwrap()
        );
    }
    {
        let p = it.next().unwrap();
        assert_eq!(
            point::Point::Screen(point::Coordinate { x: 1, y: 0 }),
            s.point_from_pin(point::Tag::Screen, p).unwrap()
        );
    }
    {
        let p = it.next().unwrap();
        assert_eq!(
            point::Point::Screen(point::Coordinate { x: 0, y: 0 }),
            s.point_from_pin(point::Tag::Screen, p).unwrap()
        );
    }
    assert!(it.next().is_none());

    s.deinit();
}

// ref: test "PageList promptIterator left_up"
#[test]
fn pagelist_promptIterator_left_up() {
    let mut s = PageList::init(2, 20, Some(0)).unwrap();
    assert!(core::ptr::eq(s.pages.first, s.pages.last));
    let page = unsafe { super::Node::page(s.pages.first) };
    // Normal prompt
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 3) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };
    }
    // Continuation
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 6) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };
    }
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 7) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::PromptContinuation) };
    }
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 8) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::PromptContinuation) };
    }
    // Broken continuation that has non-prompts in between
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 12) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::PromptContinuation) };
    }

    let mut it = s.prompt_iterator(
        super::iter::Direction::LeftUp,
        point::Point::Screen(point::Coordinate::default()),
        None,
    );
    {
        let p = it.next().unwrap();
        assert_eq!(
            point::Point::Screen(point::Coordinate { x: 0, y: 12 }),
            s.point_from_pin(point::Tag::Screen, p).unwrap()
        );
    }
    {
        let p = it.next().unwrap();
        assert_eq!(
            point::Point::Screen(point::Coordinate { x: 0, y: 6 }),
            s.point_from_pin(point::Tag::Screen, p).unwrap()
        );
    }
    {
        let p = it.next().unwrap();
        assert_eq!(
            point::Point::Screen(point::Coordinate { x: 0, y: 3 }),
            s.point_from_pin(point::Tag::Screen, p).unwrap()
        );
    }
    assert!(it.next().is_none());

    s.deinit();
}

// ref: test "PageList promptIterator right_down"
#[test]
fn pagelist_promptIterator_right_down() {
    let mut s = PageList::init(2, 20, Some(0)).unwrap();
    assert!(core::ptr::eq(s.pages.first, s.pages.last));
    let page = unsafe { super::Node::page(s.pages.first) };
    // Normal prompt
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 3) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };
    }
    // Continuation (prompt on row 6, continuation on rows 7-8)
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 6) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };
    }
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 7) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::PromptContinuation) };
    }
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 8) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::PromptContinuation) };
    }
    // Broken continuation that has non-prompts in between (orphaned continuation at row 12)
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 12) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::PromptContinuation) };
    }

    let mut it = s.prompt_iterator(
        super::iter::Direction::RightDown,
        point::Point::Screen(point::Coordinate::default()),
        None,
    );
    {
        let p = it.next().unwrap();
        assert_eq!(
            point::Point::Screen(point::Coordinate { x: 0, y: 3 }),
            s.point_from_pin(point::Tag::Screen, p).unwrap()
        );
    }
    {
        let p = it.next().unwrap();
        assert_eq!(
            point::Point::Screen(point::Coordinate { x: 0, y: 6 }),
            s.point_from_pin(point::Tag::Screen, p).unwrap()
        );
    }
    {
        let p = it.next().unwrap();
        assert_eq!(
            point::Point::Screen(point::Coordinate { x: 0, y: 12 }),
            s.point_from_pin(point::Tag::Screen, p).unwrap()
        );
    }
    assert!(it.next().is_none());

    s.deinit();
}

// ref: test "PageList promptIterator right_down continuation at start"
#[test]
fn pagelist_promptIterator_right_down_continuation_at_start() {
    let mut s = PageList::init(2, 20, Some(0)).unwrap();
    assert!(core::ptr::eq(s.pages.first, s.pages.last));
    let page = unsafe { super::Node::page(s.pages.first) };

    // Prompt continuation at row 0 (no prior rows - simulates trimmed scrollback)
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 0) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::PromptContinuation) };
    }
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 1) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::PromptContinuation) };
    }
    // Normal prompt later
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 5) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };
    }

    let mut it = s.prompt_iterator(
        super::iter::Direction::RightDown,
        point::Point::Screen(point::Coordinate::default()),
        None,
    );
    {
        // Should return the first continuation line since there's no prior prompt
        let p = it.next().unwrap();
        assert_eq!(
            point::Point::Screen(point::Coordinate { x: 0, y: 0 }),
            s.point_from_pin(point::Tag::Screen, p).unwrap()
        );
    }
    {
        let p = it.next().unwrap();
        assert_eq!(
            point::Point::Screen(point::Coordinate { x: 0, y: 5 }),
            s.point_from_pin(point::Tag::Screen, p).unwrap()
        );
    }
    assert!(it.next().is_none());

    s.deinit();
}

// ref: test "PageList promptIterator right_down with prompt before continuation"
#[test]
fn pagelist_promptIterator_right_down_with_prompt_before_continuation() {
    let mut s = PageList::init(2, 20, Some(0)).unwrap();
    assert!(core::ptr::eq(s.pages.first, s.pages.last));
    let page = unsafe { super::Node::page(s.pages.first) };

    // Prompt on row 2, continuation on rows 3-4
    // Starting iteration from row 3 should still find the prompt at row 2
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 2) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };
    }
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 3) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::PromptContinuation) };
    }
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 4) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::PromptContinuation) };
    }

    // Start iteration from row 3 (middle of the continuation)
    // Since we start on a continuation line, we treat it as the prompt start
    // (handles case where scrollback pruned the actual prompt)
    let mut it = s.prompt_iterator(
        super::iter::Direction::RightDown,
        point::Point::Screen(point::Coordinate { x: 0, y: 3 }),
        None,
    );
    {
        let p = it.next().unwrap();
        // Returns row 3 since that's the first prompt-related line we encounter
        assert_eq!(
            point::Point::Screen(point::Coordinate { x: 0, y: 3 }),
            s.point_from_pin(point::Tag::Screen, p).unwrap()
        );
    }
    assert!(it.next().is_none());

    s.deinit();
}

// ref: test "PageList promptIterator right_down limit inclusive"
#[test]
fn pagelist_promptIterator_right_down_limit_inclusive() {
    let mut s = PageList::init(2, 20, Some(0)).unwrap();
    assert!(core::ptr::eq(s.pages.first, s.pages.last));
    let page = unsafe { super::Node::page(s.pages.first) };

    // Prompt on row 5
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 5) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };
    }
    // Prompt on row 10
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 10) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };
    }

    // Iterate with limit at row 5 (the prompt row) - should include it
    let mut it = s.prompt_iterator(
        super::iter::Direction::RightDown,
        point::Point::Screen(point::Coordinate::default()),
        Some(point::Point::Screen(point::Coordinate { x: 0, y: 5 })),
    );
    {
        let p = it.next().unwrap();
        assert_eq!(
            point::Point::Screen(point::Coordinate { x: 0, y: 5 }),
            s.point_from_pin(point::Tag::Screen, p).unwrap()
        );
    }
    assert!(it.next().is_none());

    s.deinit();
}

// ref: test "PageList promptIterator left_up limit inclusive"
#[test]
fn pagelist_promptIterator_left_up_limit_inclusive() {
    let mut s = PageList::init(2, 20, Some(0)).unwrap();
    assert!(core::ptr::eq(s.pages.first, s.pages.last));
    let page = unsafe { super::Node::page(s.pages.first) };

    // Prompt on row 5
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 5) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };
    }
    // Prompt on row 10
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 10) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };
    }

    // Iterate with limit at row 10 (the prompt row) - should include it
    // tl_pt is the limit (upper bound), bl_pt is the start point for left_up
    let mut it = s.prompt_iterator(
        super::iter::Direction::LeftUp,
        point::Point::Screen(point::Coordinate { x: 0, y: 10 }),
        Some(point::Point::Screen(point::Coordinate { x: 0, y: 15 })),
    );
    {
        let p = it.next().unwrap();
        assert_eq!(
            point::Point::Screen(point::Coordinate { x: 0, y: 10 }),
            s.point_from_pin(point::Tag::Screen, p).unwrap()
        );
    }
    assert!(it.next().is_none());

    s.deinit();
}

// ref: test "PageList highlightSemanticContent prompt"
#[test]
fn pagelist_highlightSemanticContent_prompt() {
    let mut s = PageList::init(10, 20, Some(0)).unwrap();
    assert!(core::ptr::eq(s.pages.first, s.pages.last));
    let page = unsafe { super::Node::page(s.pages.first) };

    // Prompt on row 5
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 5) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };

        // Start the prompt for the first 5 cols
        for x in 0..5 {
            let cell = unsafe { (*page).get_row_and_cell(x, 5) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'A' as u32, SemanticContent::Prompt);
            }
        }

        // Next 3 let's make input
        for x in 5..8 {
            let cell = unsafe { (*page).get_row_and_cell(x, 5) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'B' as u32, SemanticContent::Input);
            }
        }
    }
    // Prompt on row 10
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 10) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };
    }

    let hl = s
        .highlight_semantic_content(
            s.pin(point::Point::Screen(point::Coordinate { x: 2, y: 5 }))
                .unwrap(),
            SemanticContent::Prompt,
        )
        .unwrap();
    assert_eq!(
        point::Point::Screen(point::Coordinate { x: 0, y: 5 }),
        s.point_from_pin(point::Tag::Screen, hl.start).unwrap()
    );
    assert_eq!(
        point::Point::Screen(point::Coordinate { x: 7, y: 5 }),
        s.point_from_pin(point::Tag::Screen, hl.end).unwrap()
    );

    s.deinit();
}

// ref: test "PageList highlightSemanticContent prompt with output"
#[test]
fn pagelist_highlightSemanticContent_prompt_with_output() {
    let mut s = PageList::init(10, 20, Some(0)).unwrap();
    assert!(core::ptr::eq(s.pages.first, s.pages.last));
    let page = unsafe { super::Node::page(s.pages.first) };

    // Prompt on row 5
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 5) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };

        // First 3 cols are prompt
        for x in 0..3 {
            let cell = unsafe { (*page).get_row_and_cell(x, 5) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'$' as u32, SemanticContent::Prompt);
            }
        }

        // Next 4 are input
        for x in 3..7 {
            let cell = unsafe { (*page).get_row_and_cell(x, 5) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'l' as u32, SemanticContent::Input);
            }
        }

        // Rest is output (shouldn't be included in prompt highlight)
        for x in 7..10 {
            let cell = unsafe { (*page).get_row_and_cell(x, 5) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'o' as u32, SemanticContent::Output);
            }
        }
    }
    // Prompt on row 10
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 10) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };
    }

    // Highlighting from prompt should include prompt and input, but stop at output
    let hl = s
        .highlight_semantic_content(
            s.pin(point::Point::Screen(point::Coordinate { x: 0, y: 5 }))
                .unwrap(),
            SemanticContent::Prompt,
        )
        .unwrap();
    assert_eq!(
        point::Point::Screen(point::Coordinate { x: 0, y: 5 }),
        s.point_from_pin(point::Tag::Screen, hl.start).unwrap()
    );
    assert_eq!(
        point::Point::Screen(point::Coordinate { x: 6, y: 5 }),
        s.point_from_pin(point::Tag::Screen, hl.end).unwrap()
    );

    s.deinit();
}

// ref: test "PageList highlightSemanticContent prompt multiline"
#[test]
fn pagelist_highlightSemanticContent_prompt_multiline() {
    let mut s = PageList::init(10, 20, Some(0)).unwrap();
    assert!(core::ptr::eq(s.pages.first, s.pages.last));
    let page = unsafe { super::Node::page(s.pages.first) };

    // Prompt starts on row 5
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 5) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };

        // First row is all prompt
        for x in 0..10 {
            let cell = unsafe { (*page).get_row_and_cell(x, 5) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'$' as u32, SemanticContent::Prompt);
            }
        }
    }
    // Row 6 continues with input
    {
        for x in 0..5 {
            let cell = unsafe { (*page).get_row_and_cell(x, 6) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'c' as u32, SemanticContent::Input);
            }
        }
    }
    // Prompt on row 10
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 10) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };
    }

    // Highlighting should span both rows
    let hl = s
        .highlight_semantic_content(
            s.pin(point::Point::Screen(point::Coordinate { x: 2, y: 5 }))
                .unwrap(),
            SemanticContent::Prompt,
        )
        .unwrap();
    assert_eq!(
        point::Point::Screen(point::Coordinate { x: 0, y: 5 }),
        s.point_from_pin(point::Tag::Screen, hl.start).unwrap()
    );
    assert_eq!(
        point::Point::Screen(point::Coordinate { x: 4, y: 6 }),
        s.point_from_pin(point::Tag::Screen, hl.end).unwrap()
    );

    s.deinit();
}

// ref: test "PageList highlightSemanticContent prompt only"
#[test]
fn pagelist_highlightSemanticContent_prompt_only() {
    let mut s = PageList::init(10, 20, Some(0)).unwrap();
    assert!(core::ptr::eq(s.pages.first, s.pages.last));
    let page = unsafe { super::Node::page(s.pages.first) };

    // Prompt on row 5 with only prompt content (no input)
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 5) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };

        for x in 0..5 {
            let cell = unsafe { (*page).get_row_and_cell(x, 5) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'$' as u32, SemanticContent::Prompt);
            }
        }
    }
    // Prompt on row 10
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 10) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };
    }

    // Highlighting should only include the prompt cells
    let hl = s
        .highlight_semantic_content(
            s.pin(point::Point::Screen(point::Coordinate { x: 0, y: 5 }))
                .unwrap(),
            SemanticContent::Prompt,
        )
        .unwrap();
    assert_eq!(
        point::Point::Screen(point::Coordinate { x: 0, y: 5 }),
        s.point_from_pin(point::Tag::Screen, hl.start).unwrap()
    );
    assert_eq!(
        point::Point::Screen(point::Coordinate { x: 4, y: 5 }),
        s.point_from_pin(point::Tag::Screen, hl.end).unwrap()
    );

    s.deinit();
}

// ref: test "PageList highlightSemanticContent prompt to end of screen"
#[test]
fn pagelist_highlightSemanticContent_prompt_to_end_of_screen() {
    let mut s = PageList::init(10, 20, Some(0)).unwrap();
    assert!(core::ptr::eq(s.pages.first, s.pages.last));
    let page = unsafe { super::Node::page(s.pages.first) };

    // Single prompt on row 15, no following prompt
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 15) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };

        for x in 0..3 {
            let cell = unsafe { (*page).get_row_and_cell(x, 15) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'$' as u32, SemanticContent::Prompt);
            }
        }

        for x in 3..8 {
            let cell = unsafe { (*page).get_row_and_cell(x, 15) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'c' as u32, SemanticContent::Input);
            }
        }
    }

    // Highlighting should include prompt and input up to column 7
    let hl = s
        .highlight_semantic_content(
            s.pin(point::Point::Screen(point::Coordinate { x: 0, y: 15 }))
                .unwrap(),
            SemanticContent::Prompt,
        )
        .unwrap();
    assert_eq!(
        point::Point::Screen(point::Coordinate { x: 0, y: 15 }),
        s.point_from_pin(point::Tag::Screen, hl.start).unwrap()
    );
    assert_eq!(
        point::Point::Screen(point::Coordinate { x: 7, y: 15 }),
        s.point_from_pin(point::Tag::Screen, hl.end).unwrap()
    );

    s.deinit();
}

// ref: test "PageList highlightSemanticContent input basic"
#[test]
fn pagelist_highlightSemanticContent_input_basic() {
    let mut s = PageList::init(10, 20, Some(0)).unwrap();
    assert!(core::ptr::eq(s.pages.first, s.pages.last));
    let page = unsafe { super::Node::page(s.pages.first) };

    // Prompt on row 5
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 5) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };

        // First 3 cols are prompt
        for x in 0..3 {
            let cell = unsafe { (*page).get_row_and_cell(x, 5) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'$' as u32, SemanticContent::Prompt);
            }
        }

        // Next 5 are input
        for x in 3..8 {
            let cell = unsafe { (*page).get_row_and_cell(x, 5) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'l' as u32, SemanticContent::Input);
            }
        }
    }
    // Prompt on row 10
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 10) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };
    }

    // Highlighting input should only include input cells
    let hl = s
        .highlight_semantic_content(
            s.pin(point::Point::Screen(point::Coordinate { x: 0, y: 5 }))
                .unwrap(),
            SemanticContent::Input,
        )
        .unwrap();
    assert_eq!(
        point::Point::Screen(point::Coordinate { x: 3, y: 5 }),
        s.point_from_pin(point::Tag::Screen, hl.start).unwrap()
    );
    assert_eq!(
        point::Point::Screen(point::Coordinate { x: 7, y: 5 }),
        s.point_from_pin(point::Tag::Screen, hl.end).unwrap()
    );

    s.deinit();
}

// ref: test "PageList highlightSemanticContent input with output"
#[test]
fn pagelist_highlightSemanticContent_input_with_output() {
    let mut s = PageList::init(10, 20, Some(0)).unwrap();
    assert!(core::ptr::eq(s.pages.first, s.pages.last));
    let page = unsafe { super::Node::page(s.pages.first) };

    // Prompt on row 5
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 5) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };

        // First 2 cols are prompt
        for x in 0..2 {
            let cell = unsafe { (*page).get_row_and_cell(x, 5) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'$' as u32, SemanticContent::Prompt);
            }
        }

        // Next 3 are input
        for x in 2..5 {
            let cell = unsafe { (*page).get_row_and_cell(x, 5) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'c' as u32, SemanticContent::Input);
            }
        }

        // Rest is output
        for x in 5..10 {
            let cell = unsafe { (*page).get_row_and_cell(x, 5) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'o' as u32, SemanticContent::Output);
            }
        }
    }
    // Prompt on row 10
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 10) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };
    }

    // Highlighting input should stop at output
    let hl = s
        .highlight_semantic_content(
            s.pin(point::Point::Screen(point::Coordinate { x: 0, y: 5 }))
                .unwrap(),
            SemanticContent::Input,
        )
        .unwrap();
    assert_eq!(
        point::Point::Screen(point::Coordinate { x: 2, y: 5 }),
        s.point_from_pin(point::Tag::Screen, hl.start).unwrap()
    );
    assert_eq!(
        point::Point::Screen(point::Coordinate { x: 4, y: 5 }),
        s.point_from_pin(point::Tag::Screen, hl.end).unwrap()
    );

    s.deinit();
}

// ref: test "PageList highlightSemanticContent input multiline with continuation"
#[test]
fn pagelist_highlightSemanticContent_input_multiline_with_continuation() {
    let mut s = PageList::init(10, 20, Some(0)).unwrap();
    assert!(core::ptr::eq(s.pages.first, s.pages.last));
    let page = unsafe { super::Node::page(s.pages.first) };

    // Prompt on row 5
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 5) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };

        // First 2 cols are prompt
        for x in 0..2 {
            let cell = unsafe { (*page).get_row_and_cell(x, 5) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'$' as u32, SemanticContent::Prompt);
            }
        }

        // Rest is input
        for x in 2..10 {
            let cell = unsafe { (*page).get_row_and_cell(x, 5) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'c' as u32, SemanticContent::Input);
            }
        }
    }
    // Row 6 has continuation prompt then more input
    {
        // Continuation prompt
        for x in 0..2 {
            let cell = unsafe { (*page).get_row_and_cell(x, 6) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'>' as u32, SemanticContent::Prompt);
            }
        }

        // More input
        for x in 2..6 {
            let cell = unsafe { (*page).get_row_and_cell(x, 6) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'd' as u32, SemanticContent::Input);
            }
        }
    }
    // Prompt on row 10
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 10) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };
    }

    // Highlighting input should span both rows, skipping continuation prompts
    let hl = s
        .highlight_semantic_content(
            s.pin(point::Point::Screen(point::Coordinate { x: 0, y: 5 }))
                .unwrap(),
            SemanticContent::Input,
        )
        .unwrap();
    assert_eq!(
        point::Point::Screen(point::Coordinate { x: 2, y: 5 }),
        s.point_from_pin(point::Tag::Screen, hl.start).unwrap()
    );
    assert_eq!(
        point::Point::Screen(point::Coordinate { x: 5, y: 6 }),
        s.point_from_pin(point::Tag::Screen, hl.end).unwrap()
    );

    s.deinit();
}

// ref: test "PageList highlightSemanticContent input no input returns null"
#[test]
fn pagelist_highlightSemanticContent_input_no_input_returns_null() {
    let mut s = PageList::init(10, 20, Some(0)).unwrap();
    assert!(core::ptr::eq(s.pages.first, s.pages.last));
    let page = unsafe { super::Node::page(s.pages.first) };

    // Prompt on row 5 with only prompt, then immediately output
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 5) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };

        // First 3 cols are prompt
        for x in 0..3 {
            let cell = unsafe { (*page).get_row_and_cell(x, 5) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'$' as u32, SemanticContent::Prompt);
            }
        }

        // Rest is output (no input!)
        for x in 3..10 {
            let cell = unsafe { (*page).get_row_and_cell(x, 5) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'o' as u32, SemanticContent::Output);
            }
        }
    }
    // Prompt on row 10
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 10) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };
    }

    // Highlighting input should return null when there's no input
    let hl = s.highlight_semantic_content(
        s.pin(point::Point::Screen(point::Coordinate { x: 0, y: 5 }))
            .unwrap(),
        SemanticContent::Input,
    );
    assert!(hl.is_none());

    s.deinit();
}

// ref: test "PageList highlightSemanticContent input to end of screen"
#[test]
fn pagelist_highlightSemanticContent_input_to_end_of_screen() {
    let mut s = PageList::init(10, 20, Some(0)).unwrap();
    assert!(core::ptr::eq(s.pages.first, s.pages.last));
    let page = unsafe { super::Node::page(s.pages.first) };

    // Single prompt on row 15, no following prompt
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 15) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };

        for x in 0..2 {
            let cell = unsafe { (*page).get_row_and_cell(x, 15) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'$' as u32, SemanticContent::Prompt);
            }
        }

        for x in 2..7 {
            let cell = unsafe { (*page).get_row_and_cell(x, 15) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'c' as u32, SemanticContent::Input);
            }
        }
    }

    // Highlighting input with no following prompt
    let hl = s
        .highlight_semantic_content(
            s.pin(point::Point::Screen(point::Coordinate { x: 0, y: 15 }))
                .unwrap(),
            SemanticContent::Input,
        )
        .unwrap();
    assert_eq!(
        point::Point::Screen(point::Coordinate { x: 2, y: 15 }),
        s.point_from_pin(point::Tag::Screen, hl.start).unwrap()
    );
    assert_eq!(
        point::Point::Screen(point::Coordinate { x: 6, y: 15 }),
        s.point_from_pin(point::Tag::Screen, hl.end).unwrap()
    );

    s.deinit();
}

// ref: test "PageList highlightSemanticContent input prompt only returns null"
#[test]
fn pagelist_highlightSemanticContent_input_prompt_only_returns_null() {
    let mut s = PageList::init(10, 20, Some(0)).unwrap();
    assert!(core::ptr::eq(s.pages.first, s.pages.last));
    let page = unsafe { super::Node::page(s.pages.first) };

    // Prompt on row 5 with only prompt content, no input or output
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 5) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };

        // All cells are prompt
        for x in 0..10 {
            let cell = unsafe { (*page).get_row_and_cell(x, 5) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'$' as u32, SemanticContent::Prompt);
            }
        }
    }
    // Mark rows 6-9 as prompt to ensure no input before next prompt
    {
        for y in 6..10 {
            for x in 0..10 {
                let cell = unsafe { (*page).get_row_and_cell(x, y) }.cell;
                unsafe { (*cell).set_semantic_content(SemanticContent::Prompt) };
            }
        }
    }
    // Prompt on row 10
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 10) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };
    }

    // Highlighting input should return null when there's only prompts
    let hl = s.highlight_semantic_content(
        s.pin(point::Point::Screen(point::Coordinate { x: 0, y: 5 }))
            .unwrap(),
        SemanticContent::Input,
    );
    assert!(hl.is_none());

    s.deinit();
}

// ref: test "PageList highlightSemanticContent output basic"
#[test]
fn pagelist_highlightSemanticContent_output_basic() {
    let mut s = PageList::init(10, 20, Some(0)).unwrap();
    assert!(core::ptr::eq(s.pages.first, s.pages.last));
    let page = unsafe { super::Node::page(s.pages.first) };

    // Prompt on row 5
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 5) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };

        // First 2 cols are prompt
        for x in 0..2 {
            let cell = unsafe { (*page).get_row_and_cell(x, 5) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'$' as u32, SemanticContent::Prompt);
            }
        }

        // Next 3 are input
        for x in 2..5 {
            let cell = unsafe { (*page).get_row_and_cell(x, 5) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'l' as u32, SemanticContent::Input);
            }
        }

        // Cols 5-7 are output
        for x in 5..8 {
            let cell = unsafe { (*page).get_row_and_cell(x, 5) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'o' as u32, SemanticContent::Output);
            }
        }

        // Mark remaining cells as prompt to bound the output
        for x in 8..10 {
            let cell = unsafe { (*page).get_row_and_cell(x, 5) }.cell;
            unsafe { (*cell).set_semantic_content(SemanticContent::Prompt) };
        }
    }
    // Prompt on row 10
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 10) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };
    }

    // Highlighting output should only include output cells
    let hl = s
        .highlight_semantic_content(
            s.pin(point::Point::Screen(point::Coordinate { x: 0, y: 5 }))
                .unwrap(),
            SemanticContent::Output,
        )
        .unwrap();
    assert_eq!(
        point::Point::Screen(point::Coordinate { x: 5, y: 5 }),
        s.point_from_pin(point::Tag::Screen, hl.start).unwrap()
    );
    assert_eq!(
        point::Point::Screen(point::Coordinate { x: 7, y: 5 }),
        s.point_from_pin(point::Tag::Screen, hl.end).unwrap()
    );

    s.deinit();
}

// ref: test "PageList highlightSemanticContent output multiline"
#[test]
fn pagelist_highlightSemanticContent_output_multiline() {
    let mut s = PageList::init(10, 20, Some(0)).unwrap();
    assert!(core::ptr::eq(s.pages.first, s.pages.last));
    let page = unsafe { super::Node::page(s.pages.first) };

    // Prompt on row 5
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 5) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };

        // First 2 cols are prompt
        for x in 0..2 {
            let cell = unsafe { (*page).get_row_and_cell(x, 5) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'$' as u32, SemanticContent::Prompt);
            }
        }

        // Next 2 are input
        for x in 2..4 {
            let cell = unsafe { (*page).get_row_and_cell(x, 5) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'l' as u32, SemanticContent::Input);
            }
        }

        // Rest of row 5 is output
        for x in 4..10 {
            let cell = unsafe { (*page).get_row_and_cell(x, 5) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'o' as u32, SemanticContent::Output);
            }
        }
    }
    // Row 6 is all output
    {
        for x in 0..10 {
            let cell = unsafe { (*page).get_row_and_cell(x, 6) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'o' as u32, SemanticContent::Output);
            }
        }
    }
    // Row 7 has partial output then input to bound it
    {
        for x in 0..5 {
            let cell = unsafe { (*page).get_row_and_cell(x, 7) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'o' as u32, SemanticContent::Output);
            }
        }
        for x in 5..10 {
            let cell = unsafe { (*page).get_row_and_cell(x, 7) }.cell;
            unsafe { (*cell).set_semantic_content(SemanticContent::Input) };
        }
    }
    // Prompt on row 10
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 10) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };
    }

    // Highlighting output should span multiple rows
    let hl = s
        .highlight_semantic_content(
            s.pin(point::Point::Screen(point::Coordinate { x: 0, y: 5 }))
                .unwrap(),
            SemanticContent::Output,
        )
        .unwrap();
    assert_eq!(
        point::Point::Screen(point::Coordinate { x: 4, y: 5 }),
        s.point_from_pin(point::Tag::Screen, hl.start).unwrap()
    );
    assert_eq!(
        point::Point::Screen(point::Coordinate { x: 4, y: 7 }),
        s.point_from_pin(point::Tag::Screen, hl.end).unwrap()
    );

    s.deinit();
}

// ref: test "PageList highlightSemanticContent output stops at next prompt"
#[test]
fn pagelist_highlightSemanticContent_output_stops_at_next_prompt() {
    let mut s = PageList::init(10, 20, Some(0)).unwrap();
    assert!(core::ptr::eq(s.pages.first, s.pages.last));
    let page = unsafe { super::Node::page(s.pages.first) };

    // Prompt on row 5
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 5) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };

        // First 2 cols are prompt
        for x in 0..2 {
            let cell = unsafe { (*page).get_row_and_cell(x, 5) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'$' as u32, SemanticContent::Prompt);
            }
        }

        // Next 2 are input
        for x in 2..4 {
            let cell = unsafe { (*page).get_row_and_cell(x, 5) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'l' as u32, SemanticContent::Input);
            }
        }

        // Rest is output
        for x in 4..10 {
            let cell = unsafe { (*page).get_row_and_cell(x, 5) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'o' as u32, SemanticContent::Output);
            }
        }
    }
    // Row 6 has output then prompt starts
    {
        for x in 0..3 {
            let cell = unsafe { (*page).get_row_and_cell(x, 6) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'o' as u32, SemanticContent::Output);
            }
        }
        // Next prompt marker on same row
        for x in 3..6 {
            let cell = unsafe { (*page).get_row_and_cell(x, 6) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'$' as u32, SemanticContent::Prompt);
            }
        }
    }
    // Prompt on row 10
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 10) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };
    }

    // Highlighting output should stop before prompt/input
    let hl = s
        .highlight_semantic_content(
            s.pin(point::Point::Screen(point::Coordinate { x: 0, y: 5 }))
                .unwrap(),
            SemanticContent::Output,
        )
        .unwrap();
    assert_eq!(
        point::Point::Screen(point::Coordinate { x: 4, y: 5 }),
        s.point_from_pin(point::Tag::Screen, hl.start).unwrap()
    );
    assert_eq!(
        point::Point::Screen(point::Coordinate { x: 2, y: 6 }),
        s.point_from_pin(point::Tag::Screen, hl.end).unwrap()
    );

    s.deinit();
}

// ref: test "PageList highlightSemanticContent output to end of screen"
#[test]
fn pagelist_highlightSemanticContent_output_to_end_of_screen() {
    let mut s = PageList::init(10, 20, Some(0)).unwrap();
    assert!(core::ptr::eq(s.pages.first, s.pages.last));
    let page = unsafe { super::Node::page(s.pages.first) };

    // Single prompt on row 15, no following prompt
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 15) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };

        for x in 0..2 {
            let cell = unsafe { (*page).get_row_and_cell(x, 15) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'$' as u32, SemanticContent::Prompt);
            }
        }

        for x in 2..4 {
            let cell = unsafe { (*page).get_row_and_cell(x, 15) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'c' as u32, SemanticContent::Input);
            }
        }

        for x in 4..10 {
            let cell = unsafe { (*page).get_row_and_cell(x, 15) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'o' as u32, SemanticContent::Output);
            }
        }
    }
    // Row 16 has output then prompt to bound it
    {
        for x in 0..8 {
            let cell = unsafe { (*page).get_row_and_cell(x, 16) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'o' as u32, SemanticContent::Output);
            }
        }
        for x in 8..10 {
            let cell = unsafe { (*page).get_row_and_cell(x, 16) }.cell;
            unsafe { (*cell).set_semantic_content(SemanticContent::Prompt) };
        }
    }

    // Highlighting output with no following prompt
    let hl = s
        .highlight_semantic_content(
            s.pin(point::Point::Screen(point::Coordinate { x: 0, y: 15 }))
                .unwrap(),
            SemanticContent::Output,
        )
        .unwrap();
    assert_eq!(
        point::Point::Screen(point::Coordinate { x: 4, y: 15 }),
        s.point_from_pin(point::Tag::Screen, hl.start).unwrap()
    );
    assert_eq!(
        point::Point::Screen(point::Coordinate { x: 7, y: 16 }),
        s.point_from_pin(point::Tag::Screen, hl.end).unwrap()
    );

    s.deinit();
}

// ref: test "PageList highlightSemanticContent output no output returns null"
#[test]
fn pagelist_highlightSemanticContent_output_no_output_returns_null() {
    let mut s = PageList::init(10, 20, Some(0)).unwrap();
    assert!(core::ptr::eq(s.pages.first, s.pages.last));
    let page = unsafe { super::Node::page(s.pages.first) };

    // Prompt on row 5 with only prompt and input, no output
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 5) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };

        // First 3 cols are prompt
        for x in 0..3 {
            let cell = unsafe { (*page).get_row_and_cell(x, 5) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'$' as u32, SemanticContent::Prompt);
            }
        }

        // Rest is input (must explicitly mark all cells to avoid default .output)
        for x in 3..10 {
            let cell = unsafe { (*page).get_row_and_cell(x, 5) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'c' as u32, SemanticContent::Input);
            }
        }
    }
    // Mark rows 6-9 as input to ensure no output between prompts
    {
        for y in 6..10 {
            for x in 0..10 {
                let cell = unsafe { (*page).get_row_and_cell(x, y) }.cell;
                unsafe { (*cell).set_semantic_content(SemanticContent::Input) };
            }
        }
    }
    // Prompt on row 10 (no output between prompts)
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 10) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };
    }

    // Highlighting output should return null when there's no output
    let hl = s.highlight_semantic_content(
        s.pin(point::Point::Screen(point::Coordinate { x: 0, y: 5 }))
            .unwrap(),
        SemanticContent::Output,
    );
    assert!(hl.is_none());

    s.deinit();
}

// ref: test "PageList highlightSemanticContent output skips empty cells"
#[test]
fn pagelist_highlightSemanticContent_output_skips_empty_cells() {
    // Tests that empty cells with default .output semantic content are
    // not selected as output. This can happen when a prompt/input line
    // doesn't fill the entire row - trailing cells have default .output.
    let mut s = PageList::init(10, 20, Some(0)).unwrap();
    assert!(core::ptr::eq(s.pages.first, s.pages.last));
    let page = unsafe { super::Node::page(s.pages.first) };

    // Prompt on row 5 - only fills first 3 cells, rest are empty with default .output
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 5) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };

        // First 3 cols are prompt with text
        for x in 0..3 {
            let cell = unsafe { (*page).get_row_and_cell(x, 5) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'$' as u32, SemanticContent::Prompt);
            }
        }
        // Cells 3-9 are empty (codepoint = 0) with default .output semantic content
        // This simulates what happens when a short prompt is written
    }

    // Row 6 has input (short, doesn't fill line)
    {
        for x in 0..4 {
            let cell = unsafe { (*page).get_row_and_cell(x, 6) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'l' as u32, SemanticContent::Input);
            }
        }
        // Cells 4-9 are empty with default .output
    }

    // Row 7-8 have actual output with text
    {
        for y in 7..9 {
            for x in 0..5 {
                let cell = unsafe { (*page).get_row_and_cell(x, y) }.cell;
                unsafe {
                    *cell = cell_codepoint_sc(b'o' as u32, SemanticContent::Output);
                }
            }
        }
    }

    // Prompt on row 10
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 10) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };
    }

    // Highlighting output should skip empty cells on rows 5-6 and find
    // the actual output starting at row 7
    let hl = s
        .highlight_semantic_content(
            s.pin(point::Point::Screen(point::Coordinate { x: 0, y: 5 }))
                .unwrap(),
            SemanticContent::Output,
        )
        .unwrap();
    // Output should start at row 7, not row 5 (where empty cells have default .output)
    assert_eq!(
        point::Point::Screen(point::Coordinate { x: 0, y: 7 }),
        s.point_from_pin(point::Tag::Screen, hl.start).unwrap()
    );
    assert_eq!(
        point::Point::Screen(point::Coordinate { x: 4, y: 8 }),
        s.point_from_pin(point::Tag::Screen, hl.end).unwrap()
    );

    s.deinit();
}

// ref: test "PageList erase"
#[test]
fn pagelist_erase() {
    let mut s = PageList::init(80, 24, None).unwrap();
    assert_eq!(1_usize, s.total_pages());

    // Grow so we take up at least 5 pages.
    let page_cap_rows = unsafe { super::Node::capacity(s.pages.last).rows };
    let mut cur_page = s.pages.last;
    unsafe { (*super::Node::page(cur_page)).pause_integrity_checks(true) };
    for _ in 0..(page_cap_rows as usize * 5) {
        if let Some(new_page) = s.grow().unwrap() {
            unsafe { (*super::Node::page(cur_page)).pause_integrity_checks(false) };
            cur_page = new_page;
            unsafe { (*super::Node::page(cur_page)).pause_integrity_checks(true) };
        }
    }
    unsafe { (*super::Node::page(cur_page)).pause_integrity_checks(false) };
    assert_eq!(6_usize, s.total_pages());

    // Our total rows should be large
    assert!(s.total_rows > s.rows as usize);

    // Erase the entire history, we should be back to just our active set.
    s.erase_history(None);
    assert_eq!(s.rows as usize, s.total_rows);

    // We should be back to just one page
    assert_eq!(1_usize, s.total_pages());
    assert!(core::ptr::eq(s.pages.first, s.pages.last));

    s.deinit();
}

// ref: test "PageList erase reaccounts page size"
#[test]
fn pagelist_erase_reaccounts_page_size() {
    let mut s = PageList::init(80, 24, None).unwrap();
    let start_size = s.page_size;

    // Grow so we take up at least 5 pages.
    let page_cap_rows = unsafe { super::Node::capacity(s.pages.last).rows };
    let mut cur_page = s.pages.last;
    unsafe { (*super::Node::page(cur_page)).pause_integrity_checks(true) };
    for _ in 0..(page_cap_rows as usize * 5) {
        if let Some(new_page) = s.grow().unwrap() {
            unsafe { (*super::Node::page(cur_page)).pause_integrity_checks(false) };
            cur_page = new_page;
            unsafe { (*super::Node::page(cur_page)).pause_integrity_checks(true) };
        }
    }
    unsafe { (*super::Node::page(cur_page)).pause_integrity_checks(false) };
    assert!(s.page_size > start_size);

    // Erase the entire history, we should be back to just our active set.
    s.erase_history(None);
    assert_eq!(start_size, s.page_size);

    s.deinit();
}

// ref: test "PageList erase row with tracked pin resets to top-left"
#[test]
fn pagelist_erase_row_with_tracked_pin_resets_to_top_left() {
    let mut s = PageList::init(80, 24, None).unwrap();

    // Grow so we take up at least 5 pages.
    let page_cap_rows = unsafe { super::Node::capacity(s.pages.last).rows };
    let mut cur_page = s.pages.last;
    unsafe { (*super::Node::page(cur_page)).pause_integrity_checks(true) };
    for _ in 0..(page_cap_rows as usize * 5) {
        if let Some(new_page) = s.grow().unwrap() {
            unsafe { (*super::Node::page(cur_page)).pause_integrity_checks(false) };
            cur_page = new_page;
            unsafe { (*super::Node::page(cur_page)).pause_integrity_checks(true) };
        }
    }
    unsafe { (*super::Node::page(cur_page)).pause_integrity_checks(false) };

    // Our total rows should be large
    assert!(s.total_rows > s.rows as usize);

    // Put a tracked pin in the history
    let p = s
        .track_pin(
            s.pin(point::Point::History(point::Coordinate { x: 0, y: 0 }))
                .unwrap(),
        )
        .unwrap();

    // Erase the entire history, we should be back to just our active set.
    s.erase_history(None);
    assert_eq!(s.rows as usize, s.total_rows);

    // Our pin should move to the first page
    assert!(core::ptr::eq(s.pages.first, unsafe { (*p).node }));
    assert_eq!(0 as size::CellCountInt, unsafe { (*p).y });
    assert_eq!(0 as size::CellCountInt, unsafe { (*p).x });

    s.untrack_pin(p);
    s.deinit();
}

// ref: test "PageList erase row with tracked pin shifts"
#[test]
fn pagelist_erase_row_with_tracked_pin_shifts() {
    let mut s = PageList::init(80, 24, None).unwrap();

    // Put a tracked pin in the history
    let p = s
        .track_pin(
            s.pin(point::Point::Active(point::Coordinate { y: 4, x: 2 }))
                .unwrap(),
        )
        .unwrap();

    // Erase only a few rows in our active
    s.erase_active(3);
    assert_eq!(s.rows as usize, s.total_rows);

    // Our pin should move to the first page
    assert!(core::ptr::eq(s.pages.first, unsafe { (*p).node }));
    assert_eq!(0 as size::CellCountInt, unsafe { (*p).y });
    assert_eq!(2 as size::CellCountInt, unsafe { (*p).x });

    s.untrack_pin(p);
    s.deinit();
}

// ref: test "PageList erase row with tracked pin is erased"
#[test]
fn pagelist_erase_row_with_tracked_pin_is_erased() {
    let mut s = PageList::init(80, 24, None).unwrap();

    // Put a tracked pin in the history
    let p = s
        .track_pin(
            s.pin(point::Point::Active(point::Coordinate { y: 2, x: 2 }))
                .unwrap(),
        )
        .unwrap();

    // Erase the entire history, we should be back to just our active set.
    s.erase_active(3);
    assert_eq!(s.rows as usize, s.total_rows);

    // Our pin should move to the first page
    assert!(core::ptr::eq(s.pages.first, unsafe { (*p).node }));
    assert_eq!(0 as size::CellCountInt, unsafe { (*p).y });
    assert_eq!(0 as size::CellCountInt, unsafe { (*p).x });

    s.untrack_pin(p);
    s.deinit();
}

// ref: test "PageList erase resets viewport to active if moves within active"
#[test]
fn pagelist_erase_resets_viewport_to_active_if_moves_within_active() {
    let mut s = PageList::init(80, 24, None).unwrap();

    // Grow so we take up at least 5 pages.
    let page_cap_rows = unsafe { super::Node::capacity(s.pages.last).rows };
    let mut cur_page = s.pages.last;
    unsafe { (*super::Node::page(cur_page)).pause_integrity_checks(true) };
    for _ in 0..(page_cap_rows as usize * 5) {
        if let Some(new_page) = s.grow().unwrap() {
            unsafe { (*super::Node::page(cur_page)).pause_integrity_checks(false) };
            cur_page = new_page;
            unsafe { (*super::Node::page(cur_page)).pause_integrity_checks(true) };
        }
    }
    unsafe { (*super::Node::page(cur_page)).pause_integrity_checks(false) };

    // Move our viewport to the top
    s.scroll(Scroll::DeltaRow(-(s.total_rows as isize)));
    assert!(s.viewport == super::Viewport::Top);

    // Erase the entire history, we should be back to just our active set.
    s.erase_history(None);
    assert!(s.viewport == super::Viewport::Active);

    s.deinit();
}

// ref: test "PageList erase resets viewport if inside erased page but not active"
#[test]
fn pagelist_erase_resets_viewport_if_inside_erased_page_but_not_active() {
    let mut s = PageList::init(80, 24, None).unwrap();

    // Grow so we take up at least 5 pages.
    let page_cap_rows = unsafe { super::Node::capacity(s.pages.last).rows };
    let mut cur_page = s.pages.last;
    unsafe { (*super::Node::page(cur_page)).pause_integrity_checks(true) };
    for _ in 0..(page_cap_rows as usize * 5) {
        if let Some(new_page) = s.grow().unwrap() {
            unsafe { (*super::Node::page(cur_page)).pause_integrity_checks(false) };
            cur_page = new_page;
            unsafe { (*super::Node::page(cur_page)).pause_integrity_checks(true) };
        }
    }
    unsafe { (*super::Node::page(cur_page)).pause_integrity_checks(false) };

    // Move our viewport to the top
    s.scroll(Scroll::DeltaRow(-(s.total_rows as isize)));
    assert!(s.viewport == super::Viewport::Top);

    // Erase the entire history, we should be back to just our active set.
    s.erase_history(Some(point::Point::History(point::Coordinate {
        x: 0,
        y: 2,
    })));
    assert!(s.viewport == super::Viewport::Top);

    s.deinit();
}

// ref: test "PageList erase resets viewport to active if top is inside active"
#[test]
fn pagelist_erase_resets_viewport_to_active_if_top_is_inside_active() {
    let mut s = PageList::init(80, 24, None).unwrap();

    // Grow so we take up at least 5 pages.
    let page_cap_rows = unsafe { super::Node::capacity(s.pages.last).rows };
    let mut cur_page = s.pages.last;
    unsafe { (*super::Node::page(cur_page)).pause_integrity_checks(true) };
    for _ in 0..(page_cap_rows as usize * 5) {
        if let Some(new_page) = s.grow().unwrap() {
            unsafe { (*super::Node::page(cur_page)).pause_integrity_checks(false) };
            cur_page = new_page;
            unsafe { (*super::Node::page(cur_page)).pause_integrity_checks(true) };
        }
    }
    unsafe { (*super::Node::page(cur_page)).pause_integrity_checks(false) };

    // Move our viewport to the top
    s.scroll(Scroll::Top);

    // Erase the entire history, we should be back to just our active set.
    s.erase_history(None);
    assert!(s.viewport == super::Viewport::Active);

    s.deinit();
}

// ref: test "PageList erase active regrows automatically"
#[test]
fn pagelist_erase_active_regrows_automatically() {
    let mut s = PageList::init(80, 24, None).unwrap();
    assert!(s.total_rows() == s.rows as usize);
    s.erase_active(10);
    assert!(s.total_rows() == s.rows as usize);

    s.deinit();
}

// ref: test "PageList erase a one-row active"
#[test]
fn pagelist_erase_a_one_row_active() {
    let mut s = PageList::init(10, 1, None).unwrap();
    assert_eq!(1_usize, s.total_pages());

    // Write our letter
    {
        let page = unsafe { super::Node::page(s.pages.first) };
        for y in 0..s.rows as usize {
            let rac = unsafe { (*page).get_row_and_cell(0, y) };
            unsafe {
                *rac.cell = cell_codepoint('A' as u32);
            }
        }
    }

    s.erase_active(0);
    assert_eq!(s.rows as usize, s.total_rows);

    // The row should be empty
    {
        let get = s
            .get_cell(point::Point::Active(point::Coordinate { x: 0, y: 0 }))
            .unwrap();
        assert_eq!(0_u32, unsafe { (*get.cell).content_codepoint() });
    }

    s.deinit();
}

// ref: test "PageList eraseRowBounded less than full row"
#[test]
fn pagelist_eraseRowBounded_less_than_full_row() {
    let mut s = PageList::init(80, 10, None).unwrap();

    // Pins
    let p_top = s
        .track_pin(
            s.pin(point::Point::Active(point::Coordinate { y: 5, x: 0 }))
                .unwrap(),
        )
        .unwrap();
    let p_bot = s
        .track_pin(
            s.pin(point::Point::Active(point::Coordinate { y: 8, x: 0 }))
                .unwrap(),
        )
        .unwrap();
    let p_out = s
        .track_pin(
            s.pin(point::Point::Active(point::Coordinate { y: 9, x: 0 }))
                .unwrap(),
        )
        .unwrap();

    // Erase only a few rows in our active
    s.erase_row_bounded(point::Point::Active(point::Coordinate { x: 0, y: 5 }), 3)
        .unwrap();
    assert_eq!(s.rows as usize, s.total_rows());

    // The erased rows should be dirty
    assert!(s.is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 5 })));
    assert!(s.is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 6 })));
    assert!(s.is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 7 })));

    assert!(core::ptr::eq(s.pages.first, unsafe { (*p_top).node }));
    assert_eq!(4 as size::CellCountInt, unsafe { (*p_top).y });
    assert_eq!(0 as size::CellCountInt, unsafe { (*p_top).x });

    assert!(core::ptr::eq(s.pages.first, unsafe { (*p_bot).node }));
    assert_eq!(7 as size::CellCountInt, unsafe { (*p_bot).y });
    assert_eq!(0 as size::CellCountInt, unsafe { (*p_bot).x });

    assert!(core::ptr::eq(s.pages.first, unsafe { (*p_out).node }));
    assert_eq!(9 as size::CellCountInt, unsafe { (*p_out).y });
    assert_eq!(0 as size::CellCountInt, unsafe { (*p_out).x });

    s.untrack_pin(p_out);
    s.untrack_pin(p_bot);
    s.untrack_pin(p_top);
    s.deinit();
}

// ref: test "PageList eraseRowBounded with pin at top"
#[test]
fn pagelist_eraseRowBounded_with_pin_at_top() {
    let mut s = PageList::init(80, 10, None).unwrap();

    // Pins
    let p_top = s
        .track_pin(
            s.pin(point::Point::Active(point::Coordinate { y: 0, x: 5 }))
                .unwrap(),
        )
        .unwrap();

    // Erase only a few rows in our active
    s.erase_row_bounded(point::Point::Active(point::Coordinate { x: 0, y: 0 }), 3)
        .unwrap();
    assert_eq!(s.rows as usize, s.total_rows());

    // The erased rows should be dirty
    assert!(s.is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 0 })));
    assert!(s.is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 1 })));
    assert!(s.is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 2 })));

    assert!(core::ptr::eq(s.pages.first, unsafe { (*p_top).node }));
    assert_eq!(0 as size::CellCountInt, unsafe { (*p_top).y });
    assert_eq!(0 as size::CellCountInt, unsafe { (*p_top).x });

    s.untrack_pin(p_top);
    s.deinit();
}

// ref: test "PageList eraseRowBounded full rows single page"
#[test]
fn pagelist_eraseRowBounded_full_rows_single_page() {
    let mut s = PageList::init(80, 10, None).unwrap();

    // Pins
    let p_in = s
        .track_pin(
            s.pin(point::Point::Active(point::Coordinate { y: 7, x: 0 }))
                .unwrap(),
        )
        .unwrap();
    let p_out = s
        .track_pin(
            s.pin(point::Point::Active(point::Coordinate { y: 9, x: 0 }))
                .unwrap(),
        )
        .unwrap();

    // Erase only a few rows in our active
    s.erase_row_bounded(point::Point::Active(point::Coordinate { x: 0, y: 5 }), 10)
        .unwrap();
    assert_eq!(s.rows as usize, s.total_rows());

    // The erased rows should be dirty
    for y in 5..10 {
        assert!(s.is_dirty(point::Point::Active(point::Coordinate { x: 0, y })));
    }

    // Our pin should move to the first page
    assert!(core::ptr::eq(s.pages.first, unsafe { (*p_in).node }));
    assert_eq!(6 as size::CellCountInt, unsafe { (*p_in).y });
    assert_eq!(0 as size::CellCountInt, unsafe { (*p_in).x });

    assert!(core::ptr::eq(s.pages.first, unsafe { (*p_out).node }));
    assert_eq!(8 as size::CellCountInt, unsafe { (*p_out).y });
    assert_eq!(0 as size::CellCountInt, unsafe { (*p_out).x });

    s.untrack_pin(p_out);
    s.untrack_pin(p_in);
    s.deinit();
}

// ref: test "PageList eraseRowBounded full rows two pages"
#[test]
fn pagelist_eraseRowBounded_full_rows_two_pages() {
    let mut s = PageList::init(80, 10, None).unwrap();

    // Grow to two pages so our active area straddles
    {
        let page = unsafe { super::Node::page(s.pages.last) };
        unsafe { (*page).pause_integrity_checks(true) };
        let to_grow =
            unsafe { (*page).capacity.rows } as usize - unsafe { (*page).size.rows } as usize;
        for _ in 0..to_grow {
            let _ = s.grow().unwrap();
        }
        unsafe { (*super::Node::page(s.pages.last)).pause_integrity_checks(false) };
        s.grow_rows(5).unwrap();
        assert_eq!(2_usize, s.total_pages());
        assert_eq!(5 as size::CellCountInt, unsafe {
            super::Node::rows(s.pages.last)
        });
    }

    // Pins
    let p_first = s
        .track_pin(
            s.pin(point::Point::Active(point::Coordinate { y: 4, x: 0 }))
                .unwrap(),
        )
        .unwrap();
    let p_first_out = s
        .track_pin(
            s.pin(point::Point::Active(point::Coordinate { y: 3, x: 0 }))
                .unwrap(),
        )
        .unwrap();
    let p_in = s
        .track_pin(
            s.pin(point::Point::Active(point::Coordinate { y: 8, x: 0 }))
                .unwrap(),
        )
        .unwrap();
    let p_out = s
        .track_pin(
            s.pin(point::Point::Active(point::Coordinate { y: 9, x: 0 }))
                .unwrap(),
        )
        .unwrap();

    {
        let prev = unsafe { (*s.pages.last).prev };
        assert!(core::ptr::eq(prev, unsafe { (*p_first).node }));
        let first_rows = unsafe { super::Node::rows((*p_first).node) };
        assert_eq!((first_rows - 1) as usize, unsafe { (*p_first).y } as usize);
        assert_eq!(0 as size::CellCountInt, unsafe { (*p_first).x });

        assert!(core::ptr::eq(prev, unsafe { (*p_first_out).node }));
        let first_out_rows = unsafe { super::Node::rows((*p_first_out).node) };
        assert_eq!((first_out_rows - 2) as usize, unsafe { (*p_first_out).y }
            as usize);
        assert_eq!(0 as size::CellCountInt, unsafe { (*p_first_out).x });

        assert!(core::ptr::eq(s.pages.last, unsafe { (*p_in).node }));
        assert_eq!(3 as size::CellCountInt, unsafe { (*p_in).y });
        assert_eq!(0 as size::CellCountInt, unsafe { (*p_in).x });

        assert!(core::ptr::eq(s.pages.last, unsafe { (*p_out).node }));
        assert_eq!(4 as size::CellCountInt, unsafe { (*p_out).y });
        assert_eq!(0 as size::CellCountInt, unsafe { (*p_out).x });
    }

    // Erase only a few rows in our active
    s.erase_row_bounded(point::Point::Active(point::Coordinate { x: 0, y: 4 }), 4)
        .unwrap();

    // The erased rows should be dirty
    for y in 4..8 {
        assert!(s.is_dirty(point::Point::Active(point::Coordinate { x: 0, y })));
    }

    // In page in first page is shifted
    let prev = unsafe { (*s.pages.last).prev };
    assert!(core::ptr::eq(prev, unsafe { (*p_first).node }));
    let first_rows = unsafe { super::Node::rows((*p_first).node) };
    assert_eq!((first_rows - 2) as usize, unsafe { (*p_first).y } as usize);
    assert_eq!(0 as size::CellCountInt, unsafe { (*p_first).x });

    // Out page in first page should not be shifted
    assert!(core::ptr::eq(prev, unsafe { (*p_first_out).node }));
    let first_out_rows = unsafe { super::Node::rows((*p_first_out).node) };
    assert_eq!((first_out_rows - 2) as usize, unsafe { (*p_first_out).y }
        as usize);
    assert_eq!(0 as size::CellCountInt, unsafe { (*p_first_out).x });

    // In page is shifted
    assert!(core::ptr::eq(s.pages.last, unsafe { (*p_in).node }));
    assert_eq!(2 as size::CellCountInt, unsafe { (*p_in).y });
    assert_eq!(0 as size::CellCountInt, unsafe { (*p_in).x });

    // Out page is not shifted
    assert!(core::ptr::eq(s.pages.last, unsafe { (*p_out).node }));
    assert_eq!(4 as size::CellCountInt, unsafe { (*p_out).y });
    assert_eq!(0 as size::CellCountInt, unsafe { (*p_out).x });

    s.untrack_pin(p_out);
    s.untrack_pin(p_in);
    s.untrack_pin(p_first_out);
    s.untrack_pin(p_first);
    s.deinit();
}

// ref: test "PageList clone"
#[test]
fn pagelist_clone() {
    let mut s = PageList::init(80, 24, None).unwrap();
    assert_eq!(s.rows as usize, s.total_rows());

    let mut opts = super::access::Clone {
        top: point::Point::Screen(point::Coordinate { x: 0, y: 0 }),
        bot: None,
        tracked_pins: None,
    };
    let mut s2 = s.clone(&mut opts).unwrap();
    assert_eq!(s.rows as usize, s2.total_rows());

    s2.deinit();
    s.deinit();
}

// ref: test "PageList clone partial trimmed right"
#[test]
fn pagelist_clone_partial_trimmed_right() {
    let mut s = PageList::init(80, 20, None).unwrap();
    assert_eq!(s.rows as usize, s.total_rows());
    s.grow_rows(30).unwrap();

    let mut opts = super::access::Clone {
        top: point::Point::Screen(point::Coordinate { x: 0, y: 0 }),
        bot: Some(point::Point::Screen(point::Coordinate { x: 0, y: 39 })),
        tracked_pins: None,
    };
    let mut s2 = s.clone(&mut opts).unwrap();
    assert_eq!(40usize, s2.total_rows());

    s2.deinit();
    s.deinit();
}

// ref: test "PageList clone partial trimmed left"
#[test]
fn pagelist_clone_partial_trimmed_left() {
    let mut s = PageList::init(80, 20, None).unwrap();
    assert_eq!(s.rows as usize, s.total_rows());
    s.grow_rows(30).unwrap();

    let mut opts = super::access::Clone {
        top: point::Point::Screen(point::Coordinate { x: 0, y: 10 }),
        bot: None,
        tracked_pins: None,
    };
    let mut s2 = s.clone(&mut opts).unwrap();
    assert_eq!(40usize, s2.total_rows());

    s2.deinit();
    s.deinit();
}

// ref: test "PageList clone partial trimmed left reclaims styles"
#[test]
fn pagelist_clone_partial_trimmed_left_reclaims_styles() {
    let mut s = PageList::init(80, 20, None).unwrap();
    assert_eq!(s.rows as usize, s.total_rows());
    s.grow_rows(30).unwrap();

    // Style the rows we're trimming
    {
        assert!(core::ptr::eq(s.pages.first, s.pages.last));
        // PORT NOTE: `page` is a raw `*mut PageData` (the reference's
        // `&...first.?.data` is a Zig pointer). A real `&mut` held across the
        // loop below is invalidated under Stacked Borrows by the shared retag
        // through `Pin::row_and_cell`, so we keep a raw pointer and deref per use.
        let page = unsafe { super::Node::page(s.pages.first) };

        let style = Style {
            flags: stylepkg::Flags {
                bold: true,
                ..stylepkg::Flags::default()
            },
            ..Style::default()
        };
        let base = unsafe { (*page).memory_ptr() };
        let style_id = unsafe { (*page).styles.add(base, style) }.unwrap();

        let mut it = s.row_iterator(
            super::iter::Direction::LeftUp,
            point::Point::Screen(point::Coordinate { x: 0, y: 0 }),
            Some(point::Point::Screen(point::Coordinate { x: 0, y: 9 })),
        );
        let page = unsafe { super::Node::page(s.pages.first) };
        let base = unsafe { (*page).memory_ptr() };
        while let Some(p) = it.next() {
            let rac = p.row_and_cell();
            unsafe {
                (*rac.row).set_styled(true);
                let mut c = cell_codepoint(b'A' as u32);
                c.set_style_id(style_id);
                *rac.cell = c;
                (*page).styles.use_id(base, style_id);
            }
        }

        // We're over-counted by 1 because `add` implies `use`.
        unsafe { (*page).styles.release(base, style_id) };

        // Expect to have one style
        assert_eq!(1, unsafe { (*page).styles.count() });
    }

    let mut opts = super::access::Clone {
        top: point::Point::Screen(point::Coordinate { x: 0, y: 10 }),
        bot: None,
        tracked_pins: None,
    };
    let mut s2 = s.clone(&mut opts).unwrap();
    assert_eq!(40usize, s2.total_rows());

    {
        assert!(core::ptr::eq(s2.pages.first, s2.pages.last));
        let page = unsafe { &(*super::Node::page(s2.pages.first)) };
        assert_eq!(0, page.styles.count());
    }

    s2.deinit();
    s.deinit();
}

// ref: test "PageList clone partial trimmed both"
#[test]
fn pagelist_clone_partial_trimmed_both() {
    let mut s = PageList::init(80, 20, None).unwrap();
    assert_eq!(s.rows as usize, s.total_rows());
    s.grow_rows(30).unwrap();

    let mut opts = super::access::Clone {
        top: point::Point::Screen(point::Coordinate { x: 0, y: 10 }),
        bot: Some(point::Point::Screen(point::Coordinate { x: 0, y: 35 })),
        tracked_pins: None,
    };
    let mut s2 = s.clone(&mut opts).unwrap();
    assert_eq!(26usize, s2.total_rows());

    s2.deinit();
    s.deinit();
}

// ref: test "PageList clone less than active"
#[test]
fn pagelist_clone_less_than_active() {
    let mut s = PageList::init(80, 24, None).unwrap();
    assert_eq!(s.rows as usize, s.total_rows());

    let mut opts = super::access::Clone {
        top: point::Point::Active(point::Coordinate { x: 0, y: 5 }),
        bot: None,
        tracked_pins: None,
    };
    let mut s2 = s.clone(&mut opts).unwrap();
    assert_eq!(s.rows as usize, s2.total_rows());

    s2.deinit();
    s.deinit();
}

// ref: test "PageList clone remap tracked pin"
#[test]
fn pagelist_clone_remap_tracked_pin() {
    let mut s = PageList::init(80, 24, None).unwrap();
    assert_eq!(s.rows as usize, s.total_rows());

    // Put a tracked pin in the screen
    let p = s
        .track_pin(
            s.pin(point::Point::Active(point::Coordinate { x: 0, y: 6 }))
                .unwrap(),
        )
        .unwrap();

    let mut opts = super::access::Clone {
        top: point::Point::Active(point::Coordinate { x: 0, y: 5 }),
        bot: None,
        tracked_pins: Some(super::access::TrackedPinsRemap::default()),
    };
    let mut s2 = s.clone(&mut opts).unwrap();

    // We should be able to find our tracked pin
    let p2 = *opts.tracked_pins.as_ref().unwrap().get(&p).unwrap();
    assert_eq!(
        point::Point::Active(point::Coordinate { x: 0, y: 1 }),
        s2.point_from_pin(point::Tag::Active, unsafe { (*p2).pin_copy() })
            .unwrap(),
    );

    s2.deinit();
    s.untrack_pin(p);
    s.deinit();
}

// ref: test "PageList clone remap tracked pin not in cloned area"
#[test]
fn pagelist_clone_remap_tracked_pin_not_in_cloned_area() {
    let mut s = PageList::init(80, 24, None).unwrap();
    assert_eq!(s.rows as usize, s.total_rows());

    // Put a tracked pin in the screen
    let p = s
        .track_pin(
            s.pin(point::Point::Active(point::Coordinate { x: 0, y: 3 }))
                .unwrap(),
        )
        .unwrap();

    let mut opts = super::access::Clone {
        top: point::Point::Active(point::Coordinate { x: 0, y: 5 }),
        bot: None,
        tracked_pins: Some(super::access::TrackedPinsRemap::default()),
    };
    let mut s2 = s.clone(&mut opts).unwrap();

    // We should be able to find our tracked pin
    assert!(opts.tracked_pins.as_ref().unwrap().get(&p).is_none());

    s2.deinit();
    s.untrack_pin(p);
    s.deinit();
}

// ref: test "PageList clone full dirty"
#[test]
fn pagelist_clone_full_dirty() {
    let mut s = PageList::init(80, 24, None).unwrap();
    assert_eq!(s.rows as usize, s.total_rows());

    // Mark a row as dirty
    s.mark_dirty(point::Point::Active(point::Coordinate { x: 0, y: 0 }));
    s.mark_dirty(point::Point::Active(point::Coordinate { x: 0, y: 12 }));
    s.mark_dirty(point::Point::Active(point::Coordinate { x: 0, y: 23 }));

    let mut opts = super::access::Clone {
        top: point::Point::Screen(point::Coordinate { x: 0, y: 0 }),
        bot: None,
        tracked_pins: None,
    };
    let mut s2 = s.clone(&mut opts).unwrap();
    assert_eq!(s.rows as usize, s2.total_rows());

    // Should still be dirty
    assert!(s2.is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 0 })));
    assert!(!s2.is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 1 })));
    assert!(s2.is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 12 })));
    assert!(!s2.is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 14 })));
    assert!(s2.is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 23 })));

    s2.deinit();
    s.deinit();
}

// =====================================================================
// resize (no reflow)
// =====================================================================

// ref: test "PageList resize (no reflow) more rows"
#[test]
fn pagelist_resize_no_reflow_more_rows() {
    let mut s = PageList::init(10, 3, Some(0)).unwrap();
    assert_eq!(3usize, s.total_rows());

    // Put a tracked pin in the history
    let p = s
        .track_pin(
            s.pin(point::Point::Active(point::Coordinate { x: 0, y: 2 }))
                .unwrap(),
        )
        .unwrap();

    // Resize
    s.resize(Resize {
        rows: Some(10),
        reflow: false,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(10usize, s.rows as usize);
    assert_eq!(10usize, s.total_rows());

    // Our cursor should not move because we have no scrollback so
    // we just grew.
    assert_eq!(
        point::Point::Active(point::Coordinate { x: 0, y: 2 }),
        s.point_from_pin(point::Tag::Active, pin_copy(p)).unwrap()
    );

    {
        let pt = s
            .get_cell(point::Point::Active(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 0 }), pt);
    }

    s.untrack_pin(p);
    s.deinit();
}

// ref: test "PageList resize (no reflow) more rows with history"
#[test]
fn pagelist_resize_no_reflow_more_rows_with_history() {
    let mut s = PageList::init(10, 3, None).unwrap();
    s.grow_rows(50).unwrap();
    {
        let pt = s
            .get_cell(point::Point::Active(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 50 }), pt);
    }

    // Put a tracked pin in the history
    let p = s
        .track_pin(
            s.pin(point::Point::Active(point::Coordinate { x: 0, y: 2 }))
                .unwrap(),
        )
        .unwrap();

    // Resize
    s.resize(Resize {
        rows: Some(5),
        reflow: false,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(5usize, s.rows as usize);
    assert_eq!(53usize, s.total_rows());

    // Our cursor should move since it's in the scrollback
    assert_eq!(
        point::Point::Active(point::Coordinate { x: 0, y: 4 }),
        s.point_from_pin(point::Tag::Active, pin_copy(p)).unwrap()
    );

    {
        let pt = s
            .get_cell(point::Point::Active(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 48 }), pt);
    }

    s.untrack_pin(p);
    s.deinit();
}

// ref: test "PageList resize (no reflow) less rows"
#[test]
fn pagelist_resize_no_reflow_less_rows() {
    let mut s = PageList::init(10, 10, Some(0)).unwrap();
    assert_eq!(10usize, s.total_rows());

    // This is required for our writing below to work
    assert!(s.pages.first == s.pages.last);
    let page = unsafe { super::Node::page(s.pages.first) };

    // Write into all rows so we don't get trim behavior
    for y in 0..s.rows as usize {
        let rac = unsafe { (*page).get_row_and_cell(0, y) };
        unsafe {
            *rac.cell = cell_codepoint('A' as u32);
        }
    }

    // Resize
    s.resize(Resize {
        rows: Some(5),
        reflow: false,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(5usize, s.rows as usize);
    assert_eq!(10usize, s.total_rows());
    {
        let pt = s
            .get_cell(point::Point::Active(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 5 }), pt);
    }

    s.deinit();
}

// ref: test "PageList resize (no reflow) one rows"
#[test]
fn pagelist_resize_no_reflow_one_rows() {
    let mut s = PageList::init(10, 10, Some(0)).unwrap();
    assert_eq!(10usize, s.total_rows());

    // This is required for our writing below to work
    assert!(s.pages.first == s.pages.last);
    let page = unsafe { super::Node::page(s.pages.first) };

    // Write into all rows so we don't get trim behavior
    for y in 0..s.rows as usize {
        let rac = unsafe { (*page).get_row_and_cell(0, y) };
        unsafe {
            *rac.cell = cell_codepoint('A' as u32);
        }
    }

    // Resize
    s.resize(Resize {
        rows: Some(1),
        reflow: false,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(1usize, s.rows as usize);
    assert_eq!(10usize, s.total_rows());
    {
        let pt = s
            .get_cell(point::Point::Active(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 9 }), pt);
    }

    s.deinit();
}

// ref: test "PageList resize (no reflow) less rows cursor on bottom"
#[test]
fn pagelist_resize_no_reflow_less_rows_cursor_on_bottom() {
    let mut s = PageList::init(10, 10, Some(0)).unwrap();
    assert_eq!(10usize, s.total_rows());

    // This is required for our writing below to work
    assert!(s.pages.first == s.pages.last);
    let page = unsafe { super::Node::page(s.pages.first) };

    // Write into all rows so we don't get trim behavior
    for y in 0..s.rows as usize {
        let rac = unsafe { (*page).get_row_and_cell(0, y) };
        unsafe {
            *rac.cell = cell_codepoint(y as u32);
        }
    }

    // Put a tracked pin in the history
    let p = s
        .track_pin(
            s.pin(point::Point::Active(point::Coordinate { x: 0, y: 9 }))
                .unwrap(),
        )
        .unwrap();
    {
        let cursor = match s.point_from_pin(point::Tag::Active, pin_copy(p)).unwrap() {
            point::Point::Active(c) => c,
            _ => unreachable!(),
        };
        let get = s
            .get_cell(point::Point::Active(point::Coordinate {
                x: cursor.x,
                y: cursor.y,
            }))
            .unwrap();
        assert_eq!(9u32, unsafe { (*get.cell).content_codepoint() });
    }

    // Resize
    s.resize(Resize {
        rows: Some(5),
        reflow: false,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(5usize, s.rows as usize);
    assert_eq!(10usize, s.total_rows());

    // Our cursor should move since it's in the scrollback
    assert_eq!(
        point::Point::Active(point::Coordinate { x: 0, y: 4 }),
        s.point_from_pin(point::Tag::Active, pin_copy(p)).unwrap()
    );

    {
        let pt = s
            .get_cell(point::Point::Active(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 5 }), pt);
    }

    s.untrack_pin(p);
    s.deinit();
}

// ref: test "PageList resize (no reflow) less rows cursor in scrollback"
#[test]
fn pagelist_resize_no_reflow_less_rows_cursor_in_scrollback() {
    let mut s = PageList::init(10, 10, Some(0)).unwrap();
    assert_eq!(10usize, s.total_rows());

    // This is required for our writing below to work
    assert!(s.pages.first == s.pages.last);
    let page = unsafe { super::Node::page(s.pages.first) };

    // Write into all rows so we don't get trim behavior
    for y in 0..s.rows as usize {
        let rac = unsafe { (*page).get_row_and_cell(0, y) };
        unsafe {
            *rac.cell = cell_codepoint(y as u32);
        }
    }

    // Put a tracked pin in the history
    let p = s
        .track_pin(
            s.pin(point::Point::Active(point::Coordinate { x: 0, y: 2 }))
                .unwrap(),
        )
        .unwrap();
    {
        let cursor = match s.point_from_pin(point::Tag::Active, pin_copy(p)).unwrap() {
            point::Point::Active(c) => c,
            _ => unreachable!(),
        };
        let get = s
            .get_cell(point::Point::Active(point::Coordinate {
                x: cursor.x,
                y: cursor.y,
            }))
            .unwrap();
        assert_eq!(2u32, unsafe { (*get.cell).content_codepoint() });
    }

    // Resize
    s.resize(Resize {
        rows: Some(5),
        reflow: false,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(5usize, s.rows as usize);
    assert_eq!(10usize, s.total_rows());

    // Our cursor should move since it's in the scrollback
    assert!(s.point_from_pin(point::Tag::Active, pin_copy(p)).is_none());
    assert_eq!(
        point::Point::Screen(point::Coordinate { x: 0, y: 2 }),
        s.point_from_pin(point::Tag::Screen, pin_copy(p)).unwrap()
    );

    {
        let pt = s
            .get_cell(point::Point::Active(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 5 }), pt);
    }

    s.untrack_pin(p);
    s.deinit();
}

// ref: test "PageList resize (no reflow) less rows trims blank lines"
#[test]
fn pagelist_resize_no_reflow_less_rows_trims_blank_lines() {
    let mut s = PageList::init(10, 5, Some(0)).unwrap();
    assert!(s.pages.first == s.pages.last);
    let page = unsafe { super::Node::page(s.pages.first) };

    // Write codepoint into first line
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 0) };
        unsafe {
            *rac.cell = cell_codepoint('A' as u32);
        }
    }

    // Fill remaining lines with a background color
    for y in 1..s.rows as usize {
        let rac = unsafe { (*page).get_row_and_cell(0, y) };
        unsafe {
            *rac.cell = cell_bg_color_rgb(0xFF, 0, 0);
        }
    }

    // Put a tracked pin in the history
    let p = s
        .track_pin(
            s.pin(point::Point::Active(point::Coordinate { x: 0, y: 0 }))
                .unwrap(),
        )
        .unwrap();
    {
        let cursor = match s.point_from_pin(point::Tag::Active, pin_copy(p)).unwrap() {
            point::Point::Active(c) => c,
            _ => unreachable!(),
        };
        let get = s
            .get_cell(point::Point::Active(point::Coordinate {
                x: cursor.x,
                y: cursor.y,
            }))
            .unwrap();
        assert_eq!('A' as u32, unsafe { (*get.cell).content_codepoint() });
    }

    // Resize
    s.resize(Resize {
        rows: Some(2),
        reflow: false,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(2usize, s.rows as usize);
    assert_eq!(2usize, s.total_rows());

    // Our cursor should not move since we trimmed
    assert_eq!(
        point::Point::Active(point::Coordinate { x: 0, y: 0 }),
        s.point_from_pin(point::Tag::Active, pin_copy(p)).unwrap()
    );

    {
        let pt = s
            .get_cell(point::Point::Active(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 0 }), pt);
    }

    s.untrack_pin(p);
    s.deinit();
}

// ref: test "PageList resize (no reflow) less rows trims blank lines cursor in blank line"
#[test]
fn pagelist_resize_no_reflow_less_rows_trims_blank_lines_cursor_in_blank_line() {
    let mut s = PageList::init(10, 5, Some(0)).unwrap();
    assert!(s.pages.first == s.pages.last);
    let page = unsafe { super::Node::page(s.pages.first) };

    // Write codepoint into first line
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 0) };
        unsafe {
            *rac.cell = cell_codepoint('A' as u32);
        }
    }

    // Fill remaining lines with a background color
    for y in 1..s.rows as usize {
        let rac = unsafe { (*page).get_row_and_cell(0, y) };
        unsafe {
            *rac.cell = cell_bg_color_rgb(0xFF, 0, 0);
        }
    }

    // Put a tracked pin in a blank line
    let p = s
        .track_pin(
            s.pin(point::Point::Active(point::Coordinate { x: 0, y: 3 }))
                .unwrap(),
        )
        .unwrap();

    // Resize
    s.resize(Resize {
        rows: Some(2),
        reflow: false,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(2usize, s.rows as usize);
    assert_eq!(4usize, s.total_rows());

    // Our cursor should not move since we trimmed
    assert_eq!(
        point::Point::Active(point::Coordinate { x: 0, y: 1 }),
        s.point_from_pin(point::Tag::Active, pin_copy(p)).unwrap()
    );

    s.untrack_pin(p);
    s.deinit();
}

// ref: test "PageList resize (no reflow) less rows trims blank lines erases pages"
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION (vt multi-page-PageList intractable, .port/phase-2/MIRI_EXCEPTIONS.md): builds/reflows multi-page PageLists that exceed Miri's interpreter budget (>20min); covered natively + by ASan (all PageList tests ~18s)"
)]
#[test]
fn pagelist_resize_no_reflow_less_rows_trims_blank_lines_erases_pages() {
    let mut s = PageList::init(100, 5, Some(0)).unwrap();
    assert!(s.pages.first == s.pages.last);
    let page = unsafe { super::Node::page(s.pages.first) };

    // Resize to take up two pages
    {
        let rows = unsafe { (*page).capacity.rows } + 10;
        s.resize(Resize {
            rows: Some(rows),
            reflow: false,
            ..Default::default()
        })
        .unwrap();
        assert_eq!(2usize, s.total_pages());
    }

    // Write codepoint into first line
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 0) };
        unsafe {
            *rac.cell = cell_codepoint('A' as u32);
        }
    }

    // Resize down. Every row except the first is blank so we
    // should erase the second page.
    s.resize(Resize {
        rows: Some(5),
        reflow: false,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(5usize, s.rows as usize);
    assert_eq!(5usize, s.total_rows());
    assert_eq!(1usize, s.total_pages());

    s.deinit();
}

// ref: test "PageList resize (no reflow) more rows extends blank lines"
#[test]
fn pagelist_resize_no_reflow_more_rows_extends_blank_lines() {
    let mut s = PageList::init(10, 3, Some(0)).unwrap();
    assert!(s.pages.first == s.pages.last);
    let page = unsafe { super::Node::page(s.pages.first) };

    // Write codepoint into first line
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 0) };
        unsafe {
            *rac.cell = cell_codepoint('A' as u32);
        }
    }

    // Fill remaining lines with a background color
    for y in 1..s.rows as usize {
        let rac = unsafe { (*page).get_row_and_cell(0, y) };
        unsafe {
            *rac.cell = cell_bg_color_rgb(0xFF, 0, 0);
        }
    }

    // Resize
    s.resize(Resize {
        rows: Some(7),
        reflow: false,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(7usize, s.rows as usize);
    assert_eq!(7usize, s.total_rows());
    {
        let pt = s
            .get_cell(point::Point::Active(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 0 }), pt);
    }

    s.deinit();
}

// ref: test "PageList resize (no reflow) more rows contains viewport"
#[test]
fn pagelist_resize_no_reflow_more_rows_contains_viewport() {
    // When the rows are increased we need to make sure that the viewport
    // doesn't end up below the active area if it's currently in pin mode.

    let mut s = PageList::init(5, 5, Some(1)).unwrap();
    assert!(s.pages.first == s.pages.last);

    // Make it so we have scrollback
    s.grow().unwrap();

    assert_eq!(5usize, s.rows as usize);
    assert_eq!(6usize, s.total_rows());

    // Set viewport above active by scrolling up one.
    s.scroll(Scroll::DeltaRow(-1));
    // The viewport should be a pin now.
    assert!(s.viewport == Viewport::Top);

    // Resize
    s.resize(Resize {
        rows: Some(7),
        reflow: false,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(7usize, s.rows as usize);
    assert_eq!(7usize, s.total_rows());

    // Question: maybe the viewport should actually be in the active
    // here and not pinned to the top.
    assert!(s.viewport == Viewport::Top);

    s.deinit();
}

// ref: test "PageList resize (no reflow) less cols"
#[test]
fn pagelist_resize_no_reflow_less_cols() {
    let mut s = PageList::init(10, 10, Some(0)).unwrap();

    // Resize
    s.resize(Resize {
        cols: Some(5),
        reflow: false,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(5usize, s.cols as usize);
    assert_eq!(10usize, s.total_rows());

    let mut it = s.row_iterator(
        super::iter::Direction::RightDown,
        point::Point::Screen(point::Coordinate::default()),
        None,
    );
    while let Some(offset) = it.next() {
        let rac = offset.row_and_cell();
        let cells = unsafe { (*super::Node::page(offset.node)).get_cells(rac.row) };
        assert_eq!(5usize, cells.len());
    }

    s.deinit();
}

// ref: test "PageList resize (no reflow) less cols pin in trimmed cols"
#[test]
fn pagelist_resize_no_reflow_less_cols_pin_in_trimmed_cols() {
    let mut s = PageList::init(10, 10, Some(0)).unwrap();

    // Put a tracked pin in the history
    let p = s
        .track_pin(
            s.pin(point::Point::Active(point::Coordinate { x: 8, y: 2 }))
                .unwrap(),
        )
        .unwrap();

    // Resize
    s.resize(Resize {
        cols: Some(5),
        reflow: false,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(5usize, s.cols as usize);
    assert_eq!(10usize, s.total_rows());

    let mut it = s.row_iterator(
        super::iter::Direction::RightDown,
        point::Point::Screen(point::Coordinate::default()),
        None,
    );
    while let Some(offset) = it.next() {
        let rac = offset.row_and_cell();
        let cells = unsafe { (*super::Node::page(offset.node)).get_cells(rac.row) };
        assert_eq!(5usize, cells.len());
    }

    assert_eq!(
        point::Point::Active(point::Coordinate { x: 4, y: 2 }),
        s.point_from_pin(point::Tag::Active, pin_copy(p)).unwrap()
    );

    s.untrack_pin(p);
    s.deinit();
}

// ref: test "PageList resize (no reflow) less cols clears graphemes"
#[test]
fn pagelist_resize_no_reflow_less_cols_clears_graphemes() {
    let mut s = PageList::init(10, 10, Some(0)).unwrap();

    // Add a grapheme.
    let page = unsafe { super::Node::page(s.pages.first) };
    {
        let rac = unsafe { (*page).get_row_and_cell(9, 0) };
        unsafe {
            *rac.cell = cell_codepoint('A' as u32);
            (*page)
                .append_grapheme(rac.row, rac.cell, 'A' as u32)
                .unwrap();
        }
    }
    assert_eq!(1usize, unsafe { (*page).grapheme_count() });

    // Resize
    s.resize(Resize {
        cols: Some(5),
        reflow: false,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(5usize, s.cols as usize);
    assert_eq!(10usize, s.total_rows());

    let mut it = s.page_iterator(
        super::iter::Direction::RightDown,
        point::Point::Screen(point::Coordinate::default()),
        None,
    );
    while let Some(chunk) = it.next() {
        assert_eq!(0usize, unsafe { (*super::Node::page(chunk.node)).grapheme_count() });
    }

    s.deinit();
}

// ref: test "PageList resize (no reflow) more cols"
#[test]
fn pagelist_resize_no_reflow_more_cols() {
    let mut s = PageList::init(5, 3, Some(0)).unwrap();

    // Resize
    s.resize(Resize {
        cols: Some(10),
        reflow: false,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(10usize, s.cols as usize);
    assert_eq!(3usize, s.total_rows());

    let mut it = s.row_iterator(
        super::iter::Direction::RightDown,
        point::Point::Screen(point::Coordinate::default()),
        None,
    );
    while let Some(offset) = it.next() {
        let rac = offset.row_and_cell();
        let cells = unsafe { (*super::Node::page(offset.node)).get_cells(rac.row) };
        assert_eq!(10usize, cells.len());
    }

    s.deinit();
}

// ref: test "PageList resize (no reflow) more cols with spacer head"
#[test]
fn pagelist_resize_no_reflow_more_cols_with_spacer_head() {
    let mut s = PageList::init(2, 3, Some(0)).unwrap();
    {
        assert!(s.pages.first == s.pages.last);
        let page = unsafe { super::Node::page(s.pages.first) };

        {
            let rac = unsafe { (*page).get_row_and_cell(0, 0) };
            unsafe {
                (*rac.row).set_wrap(true);
                *rac.cell = cell_codepoint('x' as u32);
            }
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(1, 0) };
            unsafe {
                let mut c = cell_codepoint(0);
                c.set_wide(crate::page::Wide::SpacerHead);
                *rac.cell = c;
            }
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(0, 1) };
            unsafe {
                let mut c = cell_codepoint('😀' as u32);
                c.set_wide(crate::page::Wide::Wide);
                *rac.cell = c;
            }
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(1, 1) };
            unsafe {
                let mut c = cell_codepoint(0);
                c.set_wide(crate::page::Wide::SpacerTail);
                *rac.cell = c;
            }
        }
    }

    // Resize
    s.resize(Resize {
        cols: Some(3),
        reflow: false,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(3usize, s.cols as usize);
    assert_eq!(3usize, s.total_rows());

    {
        assert!(s.pages.first == s.pages.last);
        let page = unsafe { super::Node::page(s.pages.first) };

        {
            let rac = unsafe { (*page).get_row_and_cell(0, 0) };
            assert_eq!('x' as u32, unsafe { (*rac.cell).content_codepoint() });
            assert_eq!(crate::page::Wide::Narrow, unsafe { (*rac.cell).wide() });
            // try testing.expect(!rac.row.wrap);
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(1, 0) };
            assert_eq!(0u32, unsafe { (*rac.cell).content_codepoint() });
            assert_eq!(crate::page::Wide::Narrow, unsafe { (*rac.cell).wide() });
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(2, 0) };
            assert_eq!(0u32, unsafe { (*rac.cell).content_codepoint() });
            assert_eq!(crate::page::Wide::Narrow, unsafe { (*rac.cell).wide() });
        }
    }

    s.deinit();
}

// Regression test for fuzz crash. When we shrink cols and then
// grow back, the page retains capacity from the original size so the grow
// takes the fast path (just bumps page.size.cols). If any row has a
// spacer_head at the old last column, that cell is no longer at the end
// of the wider row, violating page integrity.
// ref: test "PageList resize (no reflow) grow cols fast path with spacer head"
#[test]
fn pagelist_resize_no_reflow_grow_cols_fast_path_with_spacer_head() {
    let mut s = PageList::init(10, 3, Some(0)).unwrap();

    // Shrink to 5 cols. The page keeps capacity for 10 cols.
    s.resize(Resize {
        cols: Some(5),
        reflow: false,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(5usize, s.cols as usize);

    // Place a spacer_head at the last column (col 4) on two rows
    // to simulate a wide character that didn't fit at the right edge.
    {
        let page = unsafe { super::Node::page(s.pages.first) };

        // Row 0: 'x' at col 0..3, spacer_head at col 4, wrap = true
        {
            let rac = unsafe { (*page).get_row_and_cell(0, 0) };
            unsafe {
                *rac.cell = cell_codepoint('x' as u32);
            }
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(4, 0) };
            unsafe {
                let mut c = cell_codepoint(0);
                c.set_wide(crate::page::Wide::SpacerHead);
                *rac.cell = c;
                (*rac.row).set_wrap(true);
            }
        }

        // Row 1: spacer_head at col 4, wrap = true
        {
            let rac = unsafe { (*page).get_row_and_cell(4, 1) };
            unsafe {
                let mut c = cell_codepoint(0);
                c.set_wide(crate::page::Wide::SpacerHead);
                *rac.cell = c;
                (*rac.row).set_wrap(true);
            }
        }
    }

    // Grow back to 10 cols. This must not leave stale spacer_head
    // cells at col 4 (which is no longer the last column).
    s.resize(Resize {
        cols: Some(10),
        reflow: false,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(10usize, s.cols as usize);

    // Verify the old spacer_head positions are now narrow.
    {
        let page = unsafe { super::Node::page(s.pages.first) };
        {
            let rac = unsafe { (*page).get_row_and_cell(4, 0) };
            assert_eq!(crate::page::Wide::Narrow, unsafe { (*rac.cell).wide() });
            assert!(!unsafe { (*rac.row).wrap() });
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(4, 1) };
            assert_eq!(crate::page::Wide::Narrow, unsafe { (*rac.cell).wide() });
            assert!(!unsafe { (*rac.row).wrap() });
        }
    }

    s.deinit();
}

// This test is a bit convoluted so I want to explain: what we are trying
// to verify here is that when we increase cols such that our rows per page
// shrinks, we don't fragment our rows across many pages because this ends
// up wasting a lot of memory.
//
// This is particularly important for alternate screen buffers where we
// don't have scrollback so our max size is very small. If we don't do this,
// we end up pruning our pages and that causes resizes to fail!
// ref: test "PageList resize (no reflow) more cols forces less rows per page"
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION (vt multi-page-PageList intractable, .port/phase-2/MIRI_EXCEPTIONS.md): builds/reflows multi-page PageLists that exceed Miri's interpreter budget (>20min); covered natively + by ASan (all PageList tests ~18s)"
)]
#[test]
fn pagelist_resize_no_reflow_more_cols_forces_less_rows_per_page() {
    // This test requires initially that our rows fit into one page.
    let cols: size::CellCountInt = 5;
    let rows: size::CellCountInt = 150;
    assert!(
        std_capacity()
            .adjust(crate::page::Adjustment { cols: Some(cols) })
            .unwrap()
            .rows
            >= rows
    );
    let mut s = PageList::init(cols, rows, Some(0)).unwrap();

    // Then we need to resize our cols so that our rows per page shrinks.
    // This will force our resize to split our rows across two pages.
    {
        let new_cols = {
            let mut new_cols: size::CellCountInt = 50;
            let mut cap = std_capacity()
                .adjust(crate::page::Adjustment {
                    cols: Some(new_cols),
                })
                .unwrap();
            while cap.rows >= rows {
                new_cols += 50;
                cap = std_capacity()
                    .adjust(crate::page::Adjustment {
                        cols: Some(new_cols),
                    })
                    .unwrap();
            }
            new_cols
        };
        s.resize(Resize {
            cols: Some(new_cols),
            reflow: false,
            ..Default::default()
        })
        .unwrap();
        assert_eq!(new_cols as usize, s.cols as usize);
        assert_eq!(rows as usize, s.total_rows());
    }

    // Every page except the last should be full
    {
        let mut it = s.pages.first;
        while !it.is_null() {
            let page = it;
            if page == s.pages.last {
                break;
            }
            assert_eq!(unsafe { super::Node::capacity(page).rows }, unsafe {
                super::Node::rows(page)
            });
            it = unsafe { (*page).next };
        }
    }

    // Now we need to resize again to a col size that further shrinks
    // our last capacity.
    {
        let page = unsafe { &(*super::Node::page(s.pages.first)) };
        assert!(page.size.rows == page.capacity.rows);
        let new_cols = {
            let mut new_cols = page.size.cols + 50;
            let mut cap = std_capacity()
                .adjust(crate::page::Adjustment {
                    cols: Some(new_cols),
                })
                .unwrap();
            while cap.rows >= page.size.rows {
                new_cols += 50;
                cap = std_capacity()
                    .adjust(crate::page::Adjustment {
                        cols: Some(new_cols),
                    })
                    .unwrap();
            }
            new_cols
        };

        s.resize(Resize {
            cols: Some(new_cols),
            reflow: false,
            ..Default::default()
        })
        .unwrap();
        assert_eq!(new_cols as usize, s.cols as usize);
        assert_eq!(rows as usize, s.total_rows());
    }

    // Every page except the last should be full
    {
        let mut it = s.pages.first;
        while !it.is_null() {
            let page = it;
            if page == s.pages.last {
                break;
            }
            assert_eq!(unsafe { super::Node::capacity(page).rows }, unsafe {
                super::Node::rows(page)
            });
            it = unsafe { (*page).next };
        }
    }

    s.deinit();
}

// ref: test "PageList resize (no reflow) less cols then more cols"
#[test]
fn pagelist_resize_no_reflow_less_cols_then_more_cols() {
    let mut s = PageList::init(5, 3, Some(0)).unwrap();

    // Resize less
    s.resize(Resize {
        cols: Some(2),
        reflow: false,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(2usize, s.cols as usize);

    // Resize
    s.resize(Resize {
        cols: Some(5),
        reflow: false,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(5usize, s.cols as usize);
    assert_eq!(3usize, s.total_rows());

    let mut it = s.row_iterator(
        super::iter::Direction::RightDown,
        point::Point::Screen(point::Coordinate::default()),
        None,
    );
    while let Some(offset) = it.next() {
        let rac = offset.row_and_cell();
        let cells = unsafe { (*super::Node::page(offset.node)).get_cells(rac.row) };
        assert_eq!(5usize, cells.len());
    }

    s.deinit();
}

// ref: test "PageList resize (no reflow) less rows and cols"
#[test]
fn pagelist_resize_no_reflow_less_rows_and_cols() {
    let mut s = PageList::init(10, 10, Some(0)).unwrap();

    // Resize less
    s.resize(Resize {
        cols: Some(5),
        rows: Some(7),
        reflow: false,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(5usize, s.cols as usize);
    assert_eq!(7usize, s.rows as usize);

    let mut it = s.row_iterator(
        super::iter::Direction::RightDown,
        point::Point::Screen(point::Coordinate::default()),
        None,
    );
    while let Some(offset) = it.next() {
        let rac = offset.row_and_cell();
        let cells = unsafe { (*super::Node::page(offset.node)).get_cells(rac.row) };
        assert_eq!(5usize, cells.len());
    }

    s.deinit();
}

// ref: test "PageList resize less rows and cols cursor at bottom"
#[test]
fn pagelist_resize_less_rows_and_cols_cursor_at_bottom() {
    let mut s = PageList::init(80, 24, Some(0)).unwrap();

    let cursor_pin = s
        .track_pin(
            s.pin(point::Point::Active(point::Coordinate {
                x: 0,
                y: (s.rows - 1) as u32,
            }))
            .unwrap(),
        )
        .unwrap();

    // Shrink both axes such that the original cursor.y is strictly past the
    // new row count, so resizeWithoutReflow leaves self.rows < c.y + 1.
    s.resize(Resize {
        cols: Some(79),
        rows: Some(20),
        reflow: true,
        cursor: Some(super::resize::Cursor {
            x: 0,
            y: 23,
            pin: Some(cursor_pin),
        }),
    })
    .unwrap();
    assert_eq!(79usize, s.cols as usize);
    assert_eq!(20usize, s.rows as usize);

    // remaining_rows saturates to 0, so the cursor lands on the new bottom row.
    assert_eq!(
        point::Point::Active(point::Coordinate {
            x: 0,
            y: (s.rows - 1) as u32,
        }),
        s.point_from_pin(point::Tag::Active, pin_copy(cursor_pin))
            .unwrap()
    );

    s.untrack_pin(cursor_pin);
    s.deinit();
}

// ref: test "PageList resize less rows and cols cursor near top pushed to scrollback"
#[test]
fn pagelist_resize_less_rows_and_cols_cursor_near_top_pushed_to_scrollback() {
    let mut s = PageList::init(80, 24, None).unwrap();

    // Fill every active row with non-blank content so that shrinking rows
    // can't trim trailing blank lines and instead pushes the top rows into
    // scrollback.
    {
        let mut it = s.row_iterator(
            super::iter::Direction::RightDown,
            point::Point::Active(point::Coordinate::default()),
            None,
        );
        while let Some(p) = it.next() {
            let rac = p.row_and_cell();
            let cells = unsafe { (*super::Node::page(p.node)).get_cells(rac.row) };
            let len = cells.len();
            for x in 0..len {
                unsafe {
                    (*cells)[x] = cell_codepoint(('A' as u32) + (x as u32 % 26));
                }
            }
        }
    }

    // Cursor near the top of the active area. After we shrink rows the active
    // area top moves down past this pin, so it ends up in scrollback.
    let cursor_pin = s
        .track_pin(
            s.pin(point::Point::Active(point::Coordinate { x: 0, y: 0 }))
                .unwrap(),
        )
        .unwrap();

    // Shrink both axes with reflow. resizeWithoutReflow shrinks self.rows
    // first, leaving the cursor pin above the new active area, then resizeCols
    // walks .left_up from the cursor pin toward the active-area top.
    s.resize(Resize {
        cols: Some(79),
        rows: Some(20),
        reflow: true,
        cursor: Some(super::resize::Cursor {
            x: 0,
            y: 0,
            pin: Some(cursor_pin),
        }),
    })
    .unwrap();
    assert_eq!(79usize, s.cols as usize);
    assert_eq!(20usize, s.rows as usize);

    // The active area is anchored to the bottom, so shrinking rows pushed the
    // top-of-screen cursor into scrollback: it no longer resolves to an
    // active-area coordinate, but it remains a valid screen pin.
    assert!(s
        .point_from_pin(point::Tag::Active, pin_copy(cursor_pin))
        .is_none());
    assert!(s
        .point_from_pin(point::Tag::Screen, pin_copy(cursor_pin))
        .is_some());

    // Integrity must hold after the resize.
    s.assert_integrity();

    s.untrack_pin(cursor_pin);
    s.deinit();
}

// ref: test "PageList resize (no reflow) more rows and less cols"
#[test]
fn pagelist_resize_no_reflow_more_rows_and_less_cols() {
    let mut s = PageList::init(10, 10, Some(0)).unwrap();

    // Resize less
    s.resize(Resize {
        cols: Some(5),
        rows: Some(20),
        reflow: false,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(5usize, s.cols as usize);
    assert_eq!(20usize, s.rows as usize);
    assert_eq!(20usize, s.total_rows());

    let mut it = s.row_iterator(
        super::iter::Direction::RightDown,
        point::Point::Screen(point::Coordinate::default()),
        None,
    );
    while let Some(offset) = it.next() {
        let rac = offset.row_and_cell();
        let cells = unsafe { (*super::Node::page(offset.node)).get_cells(rac.row) };
        assert_eq!(5usize, cells.len());
    }

    s.deinit();
}

// ref: test "PageList resize more rows and cols doesn't fit in single std page"
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION (vt multi-page-PageList intractable, .port/phase-2/MIRI_EXCEPTIONS.md): builds/reflows multi-page PageLists that exceed Miri's interpreter budget (>20min); covered natively + by ASan (all PageList tests ~18s)"
)]
#[test]
fn pagelist_resize_more_rows_and_cols_doesnt_fit_in_single_std_page() {
    let mut s = PageList::init(10, 10, Some(0)).unwrap();

    // Resize to a size that requires more than one page to fit our rows.
    let new_cols = 600;
    let new_rows = 600;
    let cap = std_capacity()
        .adjust(crate::page::Adjustment {
            cols: Some(new_cols),
        })
        .unwrap();
    assert!((cap.rows as usize) < new_rows as usize);

    s.resize(Resize {
        cols: Some(new_cols),
        rows: Some(new_rows),
        reflow: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(new_cols as usize, s.cols as usize);
    assert_eq!(new_rows as usize, s.rows as usize);
    assert_eq!(new_rows as usize, s.total_rows());

    s.deinit();
}

// ref: test "PageList resize (no reflow) empty screen"
#[test]
fn pagelist_resize_no_reflow_empty_screen() {
    let mut s = PageList::init(5, 5, Some(0)).unwrap();

    // Resize
    s.resize(Resize {
        cols: Some(10),
        rows: Some(10),
        reflow: false,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(10usize, s.cols as usize);
    assert_eq!(10usize, s.rows as usize);
    assert_eq!(10usize, s.total_rows());

    let mut it = s.row_iterator(
        super::iter::Direction::RightDown,
        point::Point::Screen(point::Coordinate::default()),
        None,
    );
    while let Some(offset) = it.next() {
        let rac = offset.row_and_cell();
        let cells = unsafe { (*super::Node::page(offset.node)).get_cells(rac.row) };
        assert_eq!(10usize, cells.len());
    }

    s.deinit();
}

// ref: test "PageList resize (no reflow) more cols forces smaller cap"
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION (vt multi-page-PageList intractable, .port/phase-2/MIRI_EXCEPTIONS.md): builds/reflows multi-page PageLists that exceed Miri's interpreter budget (>20min); covered natively + by ASan (all PageList tests ~18s)"
)]
#[test]
fn pagelist_resize_no_reflow_more_cols_forces_smaller_cap() {
    // We want a cap that forces us to have less rows
    let cap = std_capacity()
        .adjust(crate::page::Adjustment { cols: Some(100) })
        .unwrap();
    let cap2 = std_capacity()
        .adjust(crate::page::Adjustment { cols: Some(500) })
        .unwrap();
    assert_eq!(500 as size::CellCountInt, cap2.cols);
    assert!(cap2.rows < cap.rows);

    // Create initial cap, fits in one page
    let mut s = PageList::init(cap.cols, cap.rows, None).unwrap();
    assert!(core::ptr::eq(s.pages.first, s.pages.last));
    {
        let page = unsafe { super::Node::page(s.pages.first) };
        for y in 0..s.rows as usize {
            for x in 0..s.cols as usize {
                let rac = unsafe { (*page).get_row_and_cell(x, y) };
                unsafe {
                    *rac.cell = cell_codepoint('A' as u32);
                }
            }
        }
    }

    // Resize to our large cap
    let rows = s.total_rows();
    s.resize(Resize {
        cols: Some(cap2.cols),
        reflow: false,
        ..Default::default()
    })
    .unwrap();

    // Our total rows should be the same, and contents should be the same.
    assert_eq!(rows, s.total_rows());
    let mut it = s.row_iterator(
        super::iter::Direction::RightDown,
        point::Point::Screen(point::Coordinate::default()),
        None,
    );
    while let Some(offset) = it.next() {
        let rac = offset.row_and_cell();
        let cells = unsafe { (*super::Node::page(offset.node)).get_cells(rac.row) };
        assert_eq!(cap2.cols as usize, cells.len());
        assert_eq!('A' as u32, unsafe {
            (*(*cells).as_ptr()).content_codepoint()
        });
    }

    s.deinit();
}

// ref: test "PageList resize (no reflow) more rows adds blank rows if cursor at bottom"
#[test]
fn pagelist_resize_no_reflow_more_rows_adds_blank_rows_if_cursor_at_bottom() {
    let mut s = PageList::init(5, 3, None).unwrap();

    // Grow to 5 total rows, simulating 3 active + 2 scrollback
    s.grow_rows(2).unwrap();
    assert!(core::ptr::eq(s.pages.first, s.pages.last));
    {
        let page = unsafe { super::Node::page(s.pages.first) };
        for y in 0..s.total_rows() {
            let rac = unsafe { (*page).get_row_and_cell(0, y) };
            unsafe {
                *rac.cell = cell_codepoint(y as u32);
            }
        }
    }

    // Active should be on row 3
    {
        let pt = s
            .get_cell(point::Point::Active(point::Coordinate::default()))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 2 }), pt);
    }

    // Put a tracked pin in the history
    let p = s
        .track_pin(
            s.pin(point::Point::Active(point::Coordinate {
                x: 0,
                y: (s.rows - 2) as u32,
            }))
            .unwrap(),
        )
        .unwrap();
    let original_cursor = match s.point_from_pin(point::Tag::Active, pin_copy(p)).unwrap() {
        point::Point::Active(c) => c,
        _ => unreachable!(),
    };
    {
        let get = s
            .get_cell(point::Point::Active(point::Coordinate {
                x: original_cursor.x,
                y: original_cursor.y,
            }))
            .unwrap();
        assert_eq!(3u32, unsafe { (*get.cell).content_codepoint() });
    }

    // Resize
    s.resize_without_reflow(Resize {
        rows: Some(10),
        reflow: false,
        cursor: Some(super::resize::Cursor {
            x: 0,
            y: s.rows - 2,
            pin: None,
        }),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(5usize, s.cols as usize);
    assert_eq!(10usize, s.rows as usize);

    // Our cursor should not change
    assert_eq!(
        point::Point::Active(original_cursor),
        s.point_from_pin(point::Tag::Active, pin_copy(p)).unwrap()
    );

    // 12 because we have our 10 rows in the active + 2 in the scrollback
    // because we're preserving the cursor.
    assert_eq!(12usize, s.total_rows());

    // Active should be at the same place it was.
    {
        let pt = s
            .get_cell(point::Point::Active(point::Coordinate::default()))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 2 }), pt);
    }

    // Go through our active, we should get only 3,4,5
    for y in 0..3usize {
        let get = s
            .get_cell(point::Point::Active(point::Coordinate {
                x: 0,
                y: y as u32,
            }))
            .unwrap();
        let expected: u32 = (y + 2) as u32;
        assert_eq!(expected, unsafe { (*get.cell).content_codepoint() });
    }

    s.untrack_pin(p);
    s.deinit();
}

// ref: test "PageList resize reflow more cols no wrapped rows"
#[test]
fn pagelist_resize_reflow_more_cols_no_wrapped_rows() {
    let mut s = PageList::init(5, 3, Some(0)).unwrap();
    assert!(core::ptr::eq(s.pages.first, s.pages.last));
    {
        let page = unsafe { super::Node::page(s.pages.first) };
        for y in 0..s.rows as usize {
            for x in 0..s.cols as usize {
                let rac = unsafe { (*page).get_row_and_cell(x, y) };
                unsafe {
                    *rac.cell = cell_codepoint('A' as u32);
                }
            }
        }
    }

    // Resize
    s.resize(Resize {
        cols: Some(10),
        reflow: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(10usize, s.cols as usize);
    assert_eq!(3usize, s.total_rows());

    let mut it = s.row_iterator(
        super::iter::Direction::RightDown,
        point::Point::Screen(point::Coordinate::default()),
        None,
    );
    while let Some(offset) = it.next() {
        let rac = offset.row_and_cell();
        let cells = unsafe { (*super::Node::page(offset.node)).get_cells(rac.row) };
        assert_eq!(10usize, cells.len());
        assert_eq!('A' as u32, unsafe {
            (*(*cells).as_ptr()).content_codepoint()
        });
    }

    s.deinit();
}

// ref: test "PageList resize reflow more cols wrapped rows"
#[test]
fn pagelist_resize_reflow_more_cols_wrapped_rows() {
    let mut s = PageList::init(2, 4, Some(0)).unwrap();
    assert!(core::ptr::eq(s.pages.first, s.pages.last));
    {
        let page = unsafe { super::Node::page(s.pages.first) };
        for y in 0..s.rows as usize {
            if y % 2 == 0 {
                let rac = unsafe { (*page).get_row_and_cell(0, y) };
                unsafe { (*rac.row).set_wrap(true) };
            } else {
                let rac = unsafe { (*page).get_row_and_cell(0, y) };
                unsafe { (*rac.row).set_wrap_continuation(true) };
            }

            for x in 0..s.cols as usize {
                let rac = unsafe { (*page).get_row_and_cell(x, y) };
                unsafe {
                    *rac.cell = cell_codepoint('A' as u32);
                }
            }
        }
    }

    // Resize
    s.resize(Resize {
        cols: Some(4),
        reflow: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(4usize, s.cols as usize);
    assert_eq!(4usize, s.total_rows());

    // Active should still be on top
    {
        let pt = s
            .get_cell(point::Point::Active(point::Coordinate::default()))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 0 }), pt);
    }

    let mut it = s.row_iterator(
        super::iter::Direction::RightDown,
        point::Point::Screen(point::Coordinate::default()),
        None,
    );
    {
        // First row should be unwrapped
        let offset = it.next().unwrap();
        let rac = offset.row_and_cell();
        let cells = unsafe { (*super::Node::page(offset.node)).get_cells(rac.row) };
        assert!(!unsafe { (*rac.row).wrap() });
        assert_eq!(4usize, cells.len());
        assert_eq!('A' as u32, unsafe {
            (*(*cells).as_ptr()).content_codepoint()
        });
        assert_eq!('A' as u32, unsafe {
            (*(*cells).as_ptr().add(2)).content_codepoint()
        });
    }

    s.deinit();
}

// ref: test "PageList resize reflow invalidates viewport offset cache"
#[test]
fn pagelist_resize_reflow_invalidates_viewport_offset_cache() {
    let mut s = PageList::init(2, 4, None).unwrap();
    s.grow_rows(20).unwrap();

    {
        let page = unsafe { super::Node::page(s.pages.last) };
        for y in 0..s.rows as usize {
            if y % 2 == 0 {
                let rac = unsafe { (*page).get_row_and_cell(0, y) };
                unsafe { (*rac.row).set_wrap(true) };
            } else {
                let rac = unsafe { (*page).get_row_and_cell(0, y) };
                unsafe { (*rac.row).set_wrap_continuation(true) };
            }

            for x in 0..s.cols as usize {
                let rac = unsafe { (*page).get_row_and_cell(x, y) };
                unsafe {
                    *rac.cell = cell_codepoint('A' as u32);
                }
            }
        }
    }

    // Scroll to a pinned viewport in history
    let pin_y = 10;
    s.scroll(Scroll::Pin(
        s.pin(point::Point::Screen(point::Coordinate { x: 0, y: pin_y }))
            .unwrap(),
    ));
    assert!(s.viewport == super::Viewport::Pin);
    assert_eq!(
        Scrollbar {
            total: s.total_rows,
            offset: pin_y as usize,
            len: s.rows as usize,
        },
        s.scrollbar()
    );

    // Resize with reflow - unwrapping rows changes total_rows
    s.resize(Resize {
        cols: Some(4),
        reflow: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(4usize, s.cols as usize);

    // Verify scrollbar cache was invalidated during reflow
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

// ref: test "PageList resize reflow more cols creates multiple pages"
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION (vt multi-page-PageList intractable, .port/phase-2/MIRI_EXCEPTIONS.md): builds/reflows multi-page PageLists that exceed Miri's interpreter budget (>20min); covered natively + by ASan (all PageList tests ~18s)"
)]
#[test]
fn pagelist_resize_reflow_more_cols_creates_multiple_pages() {
    // We want a wide viewport so our row limit is rather small. This will
    // force the reflow below to create multiple pages, which we assert.
    let cap = {
        let mut current: size::CellCountInt = 100;
        loop {
            let cap = std_capacity()
                .adjust(crate::page::Adjustment {
                    cols: Some(current),
                })
                .unwrap();
            if cap.rows < 100 {
                break cap;
            }
            current += 100;
        }
    };

    let mut s = PageList::init(cap.cols, cap.rows, None).unwrap();

    // Wrap every other row so every line is wrapped for reflow
    {
        assert!(core::ptr::eq(s.pages.first, s.pages.last));
        let page = unsafe { super::Node::page(s.pages.first) };
        for y in 0..s.rows as usize {
            if y % 2 == 0 {
                let rac = unsafe { (*page).get_row_and_cell(0, y) };
                unsafe { (*rac.row).set_wrap(true) };
            } else {
                let rac = unsafe { (*page).get_row_and_cell(0, y) };
                unsafe { (*rac.row).set_wrap_continuation(true) };
            }

            let rac = unsafe { (*page).get_row_and_cell(0, y) };
            unsafe {
                *rac.cell = cell_codepoint('A' as u32);
            }
        }
    }

    // Resize
    let newcap = cap
        .adjust(crate::page::Adjustment {
            cols: Some(cap.cols + 100),
        })
        .unwrap();
    assert!(newcap.rows < cap.rows);
    s.resize(Resize {
        cols: Some(newcap.cols),
        reflow: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(newcap.cols as usize, s.cols as usize);
    assert_eq!(cap.rows as usize, s.total_rows());

    {
        let mut count: usize = 0;
        let mut it = s.pages.first;
        while !it.is_null() {
            count += 1;

            // All pages should have the new capacity
            assert_eq!(newcap.cols, unsafe { super::Node::capacity(it).cols });
            assert_eq!(newcap.rows, unsafe { super::Node::capacity(it).rows });

            it = unsafe { (*it).next };
        }

        // We should have more than one page, meaning we created at least
        // one page. This is the critical aspect of this test so if this
        // ever goes false we need to adjust this test.
        assert!(count > 1);
    }

    s.deinit();
}

// ref: test "PageList resize reflow more cols wrap across page boundary"
#[test]
fn pagelist_resize_reflow_more_cols_wrap_across_page_boundary() {
    let mut s = PageList::init(2, 10, Some(0)).unwrap();
    assert_eq!(1usize, s.total_pages());

    // Grow to the capacity of the first page.
    {
        let page = unsafe { super::Node::page(s.pages.first) };
        unsafe { (*page).pause_integrity_checks(true) };
        for _ in unsafe { (*page).size.rows..(*page).capacity.rows } {
            let _ = s.grow().unwrap();
        }
        unsafe { (*super::Node::page(s.pages.first)).pause_integrity_checks(false) };
        assert_eq!(1usize, s.total_pages());
        s.grow_rows(1).unwrap();
        assert_eq!(2usize, s.total_pages());
    }

    // At this point, we have some rows on the first page, and some on the second.
    // We can now wrap across the boundary condition.
    {
        let page = unsafe { super::Node::page(s.pages.first) };
        let y = unsafe { (*page).size.rows as usize - 1 };
        {
            let rac = unsafe { (*page).get_row_and_cell(0, y) };
            unsafe { (*rac.row).set_wrap(true) };
        }
        for x in 0..s.cols as usize {
            let rac = unsafe { (*page).get_row_and_cell(x, y) };
            unsafe {
                *rac.cell = cell_codepoint(x as u32);
            }
        }
    }
    {
        let page2 = unsafe { super::Node::page(s.pages.last) };
        let y = 0;
        {
            let rac = unsafe { (*page2).get_row_and_cell(0, y) };
            unsafe { (*rac.row).set_wrap_continuation(true) };
        }
        for x in 0..s.cols as usize {
            let rac = unsafe { (*page2).get_row_and_cell(x, y) };
            unsafe {
                *rac.cell = cell_codepoint(x as u32);
            }
        }
    }

    // We expect one fewer rows since we unwrapped a row.
    let end_rows = s.total_rows() - 1;

    // Resize
    s.resize(Resize {
        cols: Some(4),
        reflow: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(4usize, s.cols as usize);
    assert_eq!(end_rows, s.total_rows());

    {
        // PAGE 1 ROW 6280, ACTIVE 8
        let p = s
            .pin(point::Point::Active(point::Coordinate { x: 0, y: 8 }))
            .unwrap();
        let row = p.row_and_cell().row;
        assert!(!unsafe { (*row).wrap() });
        assert!(!unsafe { (*row).wrap_continuation() });

        let cells = p.cells(super::pin::CellSubset::All);
        assert!(!unsafe { (*(cells as *const Cell)).has_text() });
        assert!(!unsafe { (*(cells as *const Cell).add(1)).has_text() });
        assert!(!unsafe { (*(cells as *const Cell).add(2)).has_text() });
        assert!(!unsafe { (*(cells as *const Cell).add(3)).has_text() });
    }
    {
        // PAGE 1 ROW 6281, ACTIVE 9
        let p = s
            .pin(point::Point::Active(point::Coordinate { x: 0, y: 9 }))
            .unwrap();
        let row = p.row_and_cell().row;
        assert!(!unsafe { (*row).wrap() });
        assert!(!unsafe { (*row).wrap_continuation() });

        let cells = p.cells(super::pin::CellSubset::All);
        assert_eq!(0u32, unsafe {
            (*(cells as *const Cell)).content_codepoint()
        });
        assert_eq!(1u32, unsafe {
            (*(cells as *const Cell).add(1)).content_codepoint()
        });
        assert_eq!(0u32, unsafe {
            (*(cells as *const Cell).add(2)).content_codepoint()
        });
        assert_eq!(1u32, unsafe {
            (*(cells as *const Cell).add(3)).content_codepoint()
        });
    }

    s.deinit();
}

// ref: test "PageList resize reflow more cols wrap across page boundary cursor in second page"
#[test]
fn pagelist_resize_reflow_more_cols_wrap_across_page_boundary_cursor_in_second_page() {
    let mut s = PageList::init(2, 10, Some(0)).unwrap();
    assert_eq!(1usize, s.total_pages());

    // Grow to the capacity of the first page.
    {
        let page = unsafe { super::Node::page(s.pages.first) };
        unsafe { (*page).pause_integrity_checks(true) };
        for _ in unsafe { (*page).size.rows..(*page).capacity.rows } {
            let _ = s.grow().unwrap();
        }
        unsafe { (*super::Node::page(s.pages.first)).pause_integrity_checks(false) };
        assert_eq!(1usize, s.total_pages());
        s.grow_rows(1).unwrap();
        assert_eq!(2usize, s.total_pages());
    }

    // At this point, we have some rows on the first page, and some on the second.
    // We can now wrap across the boundary condition.
    {
        let page = unsafe { super::Node::page(s.pages.first) };
        let y = unsafe { (*page).size.rows as usize - 1 };
        {
            let rac = unsafe { (*page).get_row_and_cell(0, y) };
            unsafe { (*rac.row).set_wrap(true) };
        }
        for x in 0..s.cols as usize {
            let rac = unsafe { (*page).get_row_and_cell(x, y) };
            unsafe {
                *rac.cell = cell_codepoint(x as u32);
            }
        }
    }
    {
        let page2 = unsafe { super::Node::page(s.pages.last) };
        let y = 0;
        {
            let rac = unsafe { (*page2).get_row_and_cell(0, y) };
            unsafe { (*rac.row).set_wrap_continuation(true) };
        }
        for x in 0..s.cols as usize {
            let rac = unsafe { (*page2).get_row_and_cell(x, y) };
            unsafe {
                *rac.cell = cell_codepoint(x as u32);
            }
        }
    }

    // Put a tracked pin in wrapped row on the last page
    let p = s
        .track_pin(
            s.pin(point::Point::Active(point::Coordinate { x: 1, y: 9 }))
                .unwrap(),
        )
        .unwrap();
    assert!(core::ptr::eq(unsafe { (*p).node }, s.pages.last));

    // We expect one fewer rows since we unwrapped a row.
    let end_rows = s.total_rows() - 1;

    // Resize
    s.resize(Resize {
        cols: Some(4),
        reflow: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(4usize, s.cols as usize);
    assert_eq!(end_rows, s.total_rows());

    // Our cursor should move to the first row
    assert_eq!(
        point::Point::Active(point::Coordinate { x: 3, y: 9 }),
        s.point_from_pin(point::Tag::Active, pin_copy(p)).unwrap()
    );

    {
        let p2 = s
            .pin(point::Point::Active(point::Coordinate { x: 0, y: 9 }))
            .unwrap();
        let row = p2.row_and_cell().row;
        assert!(!unsafe { (*row).wrap() });

        let cells = p2.cells(super::pin::CellSubset::All);
        assert_eq!(0u32, unsafe {
            (*(cells as *const Cell)).content_codepoint()
        });
        assert_eq!(1u32, unsafe {
            (*(cells as *const Cell).add(1)).content_codepoint()
        });
        assert_eq!(0u32, unsafe {
            (*(cells as *const Cell).add(2)).content_codepoint()
        });
        assert_eq!(1u32, unsafe {
            (*(cells as *const Cell).add(3)).content_codepoint()
        });
    }

    s.untrack_pin(p);
    s.deinit();
}

// ref: test "PageList resize reflow less cols wrap across page boundary cursor in second page"
#[test]
fn pagelist_resize_reflow_less_cols_wrap_across_page_boundary_cursor_in_second_page() {
    let mut s = PageList::init(5, 10, None).unwrap();
    assert_eq!(1usize, s.total_pages());

    // Grow to the capacity of the first page.
    {
        let page = unsafe { super::Node::page(s.pages.first) };
        unsafe { (*page).pause_integrity_checks(true) };
        for _ in unsafe { (*page).size.rows..(*page).capacity.rows } {
            let _ = s.grow().unwrap();
        }
        unsafe { (*super::Node::page(s.pages.first)).pause_integrity_checks(false) };
        assert_eq!(1usize, s.total_pages());
        s.grow_rows(5).unwrap();
        assert_eq!(2usize, s.total_pages());
    }

    // At this point, we have some rows on the first page, and some on the second.
    // We can now wrap across the boundary condition.
    {
        let page = unsafe { super::Node::page(s.pages.first) };
        let y = unsafe { (*page).size.rows as usize - 1 };
        {
            let rac = unsafe { (*page).get_row_and_cell(0, y) };
            unsafe { (*rac.row).set_wrap(true) };
        }
        for x in 0..s.cols as usize {
            let rac = unsafe { (*page).get_row_and_cell(x, y) };
            unsafe {
                *rac.cell = cell_codepoint(x as u32);
            }
        }
    }
    {
        let page2 = unsafe { super::Node::page(s.pages.last) };
        let y = 0;
        {
            let rac = unsafe { (*page2).get_row_and_cell(0, y) };
            unsafe { (*rac.row).set_wrap_continuation(true) };
        }
        for x in 0..s.cols as usize {
            let rac = unsafe { (*page2).get_row_and_cell(x, y) };
            unsafe {
                *rac.cell = cell_codepoint(x as u32);
            }
        }
    }

    // Put a tracked pin in wrapped row on the last page
    let p = s
        .track_pin(
            s.pin(point::Point::Active(point::Coordinate { x: 2, y: 5 }))
                .unwrap(),
        )
        .unwrap();
    assert!(core::ptr::eq(unsafe { (*p).node }, s.pages.last));
    assert_eq!(0, unsafe { (*p).y });

    // Resize
    s.resize(Resize {
        cols: Some(4),
        rows: None,
        reflow: true,
        cursor: Some(super::resize::Cursor {
            x: 2,
            y: 5,
            pin: None,
        }),
    })
    .unwrap();
    assert_eq!(4usize, s.cols as usize);

    // Our cursor should remain on the same cell
    assert_eq!(
        point::Point::Active(point::Coordinate { x: 3, y: 5 }),
        s.point_from_pin(point::Tag::Active, pin_copy(p)).unwrap()
    );

    {
        // PAGE 0 ROW 7895, ACTIVE 3
        let p2 = s
            .pin(point::Point::Active(point::Coordinate { x: 0, y: 3 }))
            .unwrap();
        let row = p2.row_and_cell().row;
        assert!(!unsafe { (*row).wrap() });
        assert!(!unsafe { (*row).wrap_continuation() });

        let cells = p2.cells(super::pin::CellSubset::All);
        assert!(!unsafe { (*(cells as *const Cell)).has_text() });
        assert!(!unsafe { (*(cells as *const Cell).add(1)).has_text() });
        assert!(!unsafe { (*(cells as *const Cell).add(2)).has_text() });
        assert!(!unsafe { (*(cells as *const Cell).add(3)).has_text() });
    }
    {
        // PAGE 0 ROW 7896, ACTIVE 4
        let p2 = s
            .pin(point::Point::Active(point::Coordinate { x: 0, y: 4 }))
            .unwrap();
        let row = p2.row_and_cell().row;
        assert!(unsafe { (*row).wrap() });
        assert!(!unsafe { (*row).wrap_continuation() });

        let cells = p2.cells(super::pin::CellSubset::All);
        assert_eq!(0u32, unsafe {
            (*(cells as *const Cell)).content_codepoint()
        });
        assert_eq!(1u32, unsafe {
            (*(cells as *const Cell).add(1)).content_codepoint()
        });
        assert_eq!(2u32, unsafe {
            (*(cells as *const Cell).add(2)).content_codepoint()
        });
        assert_eq!(3u32, unsafe {
            (*(cells as *const Cell).add(3)).content_codepoint()
        });
    }
    {
        // PAGE 0 ROW 7897, ACTIVE 5
        let p2 = s
            .pin(point::Point::Active(point::Coordinate { x: 0, y: 5 }))
            .unwrap();
        let row = p2.row_and_cell().row;
        assert!(unsafe { (*row).wrap() });
        assert!(unsafe { (*row).wrap_continuation() });

        let cells = p2.cells(super::pin::CellSubset::All);
        assert_eq!(4u32, unsafe {
            (*(cells as *const Cell)).content_codepoint()
        });
        assert_eq!(0u32, unsafe {
            (*(cells as *const Cell).add(1)).content_codepoint()
        });
        assert_eq!(1u32, unsafe {
            (*(cells as *const Cell).add(2)).content_codepoint()
        });
        assert_eq!(2u32, unsafe {
            (*(cells as *const Cell).add(3)).content_codepoint()
        });
    }
    {
        // PAGE 0 ROW 7898, ACTIVE 6
        let p2 = s
            .pin(point::Point::Active(point::Coordinate { x: 0, y: 6 }))
            .unwrap();
        let row = p2.row_and_cell().row;
        assert!(!unsafe { (*row).wrap() });
        assert!(unsafe { (*row).wrap_continuation() });

        let cells = p2.cells(super::pin::CellSubset::All);
        assert_eq!(3u32, unsafe {
            (*(cells as *const Cell)).content_codepoint()
        });
        assert_eq!(4u32, unsafe {
            (*(cells as *const Cell).add(1)).content_codepoint()
        });
    }
    {
        // PAGE 0 ROW 7899, ACTIVE 7
        let p2 = s
            .pin(point::Point::Active(point::Coordinate { x: 0, y: 7 }))
            .unwrap();
        let row = p2.row_and_cell().row;
        assert!(!unsafe { (*row).wrap() });
        assert!(!unsafe { (*row).wrap_continuation() });

        let cells = p2.cells(super::pin::CellSubset::All);
        assert!(!unsafe { (*(cells as *const Cell)).has_text() });
        assert!(!unsafe { (*(cells as *const Cell).add(1)).has_text() });
        assert!(!unsafe { (*(cells as *const Cell).add(2)).has_text() });
        assert!(!unsafe { (*(cells as *const Cell).add(3)).has_text() });
    }

    s.untrack_pin(p);
    s.deinit();
}

// ref: test "PageList resize reflow more cols cursor in wrapped row"
#[test]
fn pagelist_resize_reflow_more_cols_cursor_in_wrapped_row() {
    let mut s = PageList::init(2, 4, Some(0)).unwrap();
    assert!(core::ptr::eq(s.pages.first, s.pages.last));
    let page = unsafe { super::Node::page(s.pages.first) };
    {
        {
            let rac = unsafe { (*page).get_row_and_cell(0, 0) };
            unsafe { (*rac.row).set_wrap(true) };
        }
        for x in 0..s.cols as usize {
            let rac = unsafe { (*page).get_row_and_cell(x, 0) };
            unsafe {
                *rac.cell = cell_codepoint(x as u32);
            }
        }
    }
    {
        {
            let rac = unsafe { (*page).get_row_and_cell(0, 1) };
            unsafe { (*rac.row).set_wrap_continuation(true) };
        }
        for x in 0..s.cols as usize {
            let rac = unsafe { (*page).get_row_and_cell(x, 1) };
            unsafe {
                *rac.cell = cell_codepoint(x as u32);
            }
        }
    }

    // Put a tracked pin in the history
    let p = s
        .track_pin(
            s.pin(point::Point::Active(point::Coordinate { x: 1, y: 1 }))
                .unwrap(),
        )
        .unwrap();

    // Resize
    s.resize(Resize {
        cols: Some(4),
        reflow: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(4usize, s.cols as usize);
    assert_eq!(4usize, s.total_rows());

    // Our cursor should move to the first row
    assert_eq!(
        point::Point::Active(point::Coordinate { x: 3, y: 0 }),
        s.point_from_pin(point::Tag::Active, pin_copy(p)).unwrap()
    );

    s.untrack_pin(p);
    s.deinit();
}

// ref: test "PageList resize reflow more cols cursor in not wrapped row"
#[test]
fn pagelist_resize_reflow_more_cols_cursor_in_not_wrapped_row() {
    let mut s = PageList::init(2, 4, Some(0)).unwrap();
    assert!(core::ptr::eq(s.pages.first, s.pages.last));
    let page = unsafe { super::Node::page(s.pages.first) };
    {
        {
            let rac = unsafe { (*page).get_row_and_cell(0, 0) };
            unsafe { (*rac.row).set_wrap(true) };
        }
        for x in 0..s.cols as usize {
            let rac = unsafe { (*page).get_row_and_cell(x, 0) };
            unsafe {
                *rac.cell = cell_codepoint(x as u32);
            }
        }
    }
    {
        {
            let rac = unsafe { (*page).get_row_and_cell(0, 1) };
            unsafe { (*rac.row).set_wrap_continuation(true) };
        }
        for x in 0..s.cols as usize {
            let rac = unsafe { (*page).get_row_and_cell(x, 1) };
            unsafe {
                *rac.cell = cell_codepoint(x as u32);
            }
        }
    }

    // Put a tracked pin in the history
    let p = s
        .track_pin(
            s.pin(point::Point::Active(point::Coordinate { x: 1, y: 0 }))
                .unwrap(),
        )
        .unwrap();

    // Resize
    s.resize(Resize {
        cols: Some(4),
        reflow: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(4usize, s.cols as usize);
    assert_eq!(4usize, s.total_rows());

    // Our cursor should move to the first row
    assert_eq!(
        point::Point::Active(point::Coordinate { x: 1, y: 0 }),
        s.point_from_pin(point::Tag::Active, pin_copy(p)).unwrap()
    );

    s.untrack_pin(p);
    s.deinit();
}

// ref: test "PageList resize reflow more cols cursor in wrapped row that isn't unwrapped"
#[test]
fn pagelist_resize_reflow_more_cols_cursor_in_wrapped_row_that_isnt_unwrapped() {
    let mut s = PageList::init(2, 4, Some(0)).unwrap();
    assert!(core::ptr::eq(s.pages.first, s.pages.last));
    let page = unsafe { super::Node::page(s.pages.first) };
    {
        {
            let rac = unsafe { (*page).get_row_and_cell(0, 0) };
            unsafe { (*rac.row).set_wrap(true) };
        }
        for x in 0..s.cols as usize {
            let rac = unsafe { (*page).get_row_and_cell(x, 0) };
            unsafe {
                *rac.cell = cell_codepoint(x as u32);
            }
        }
    }
    {
        {
            let rac = unsafe { (*page).get_row_and_cell(0, 1) };
            unsafe {
                (*rac.row).set_wrap(true);
                (*rac.row).set_wrap_continuation(true);
            }
        }
        for x in 0..s.cols as usize {
            let rac = unsafe { (*page).get_row_and_cell(x, 1) };
            unsafe {
                *rac.cell = cell_codepoint(x as u32);
            }
        }
    }
    {
        {
            let rac = unsafe { (*page).get_row_and_cell(0, 2) };
            unsafe { (*rac.row).set_wrap_continuation(true) };
        }
        for x in 0..s.cols as usize {
            let rac = unsafe { (*page).get_row_and_cell(x, 2) };
            unsafe {
                *rac.cell = cell_codepoint(x as u32);
            }
        }
    }

    // Put a tracked pin in the history
    let p = s
        .track_pin(
            s.pin(point::Point::Active(point::Coordinate { x: 1, y: 2 }))
                .unwrap(),
        )
        .unwrap();

    // Resize
    s.resize(Resize {
        cols: Some(4),
        reflow: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(4usize, s.cols as usize);
    assert_eq!(4usize, s.total_rows());

    // Our cursor should move to the first row
    assert_eq!(
        point::Point::Active(point::Coordinate { x: 1, y: 1 }),
        s.point_from_pin(point::Tag::Active, pin_copy(p)).unwrap()
    );

    s.untrack_pin(p);
    s.deinit();
}

// ref: test "PageList resize reflow more cols no reflow preserves semantic prompt"
#[test]
fn pagelist_resize_reflow_more_cols_no_reflow_preserves_semantic_prompt() {
    let mut s = PageList::init(2, 4, Some(0)).unwrap();
    {
        assert!(core::ptr::eq(s.pages.first, s.pages.last));
        let page = unsafe { super::Node::page(s.pages.first) };
        let rac = unsafe { (*page).get_row_and_cell(0, 1) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };
    }

    // Resize
    s.resize(Resize {
        cols: Some(4),
        reflow: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(4usize, s.cols as usize);
    assert_eq!(4usize, s.total_rows());

    {
        assert!(core::ptr::eq(s.pages.first, s.pages.last));
        let page = unsafe { super::Node::page(s.pages.first) };
        let rac = unsafe { (*page).get_row_and_cell(0, 1) };
        assert!(unsafe { (*rac.row).semantic_prompt() } == SemanticPrompt::Prompt);
    }

    s.deinit();
}

// ref: test "PageList resize reflow exceeds hyperlink memory forcing capacity increase"
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION (vt multi-page-PageList intractable, .port/phase-2/MIRI_EXCEPTIONS.md): builds/reflows multi-page PageLists that exceed Miri's interpreter budget (>20min); covered natively + by ASan (all PageList tests ~18s)"
)]
#[test]
fn pagelist_resize_reflow_exceeds_hyperlink_memory_forcing_capacity_increase() {
    let mut s = PageList::init(2, 10, Some(0)).unwrap();
    assert_eq!(1usize, s.total_pages());

    // Grow to the capacity of the first page and add
    // one more row so that we have two pages total.
    {
        let page = unsafe { super::Node::page(s.pages.first) };
        unsafe { (*page).pause_integrity_checks(true) };
        for _ in unsafe { (*page).size.rows..(*page).capacity.rows } {
            let _ = s.grow().unwrap();
        }
        unsafe { (*super::Node::page(s.pages.first)).pause_integrity_checks(false) };
        assert_eq!(1usize, s.total_pages());
        s.grow_rows(1).unwrap();
        assert_eq!(2usize, s.total_pages());

        // We now have two pages.
        assert!(!core::ptr::eq(s.pages.first, s.pages.last));
        assert!(core::ptr::eq(s.pages.last, unsafe {
            (*s.pages.first).next
        }));
    }

    // We use almost all string alloc capacity with a hyperlink in the final
    // row of the first page, and do the same on the first row of the second
    // page. We also mark the row as wrapped so that when we resize with more
    // cols the row unwraps and we have a single row that requires almost two
    // times the base string alloc capacity.
    //
    // This forces the reflow to increase capacity.
    //
    //  +--+ = PAGE 0
    //  :  :
    //  | X… <- where X is hyperlinked with almost all string cap.
    //  +--+
    //  +--+ = PAGE 1
    //  …X | <- X here also almost hits string cap with a hyperlink.
    //  +--+

    // Almost hit string alloc cap in bottom right of first page.
    // Mark the final row as wrapped.
    {
        let page = unsafe { super::Node::page(s.pages.first) };
        let id = unsafe {
            (*page).insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Implicit(0),
                uri: vec![b'a'; crate::page::STRING_BYTES_DEFAULT - 1],
            })
        }
        .unwrap();
        let rac = unsafe {
            (*page).get_row_and_cell(
                (*page).size.cols as usize - 1,
                (*page).size.rows as usize - 1,
            )
        };
        unsafe {
            (*rac.row).set_wrap(true);
            *rac.cell = cell_codepoint('X' as u32);
        }
        unsafe { (*page).set_hyperlink(rac.row, rac.cell, id) }.unwrap();
        assert!(matches!(
            unsafe {
                (*page).insert_hyperlink(hyperlink::Hyperlink {
                    id: hyperlink::HyperlinkId::Implicit(1),
                    uri: b"AAAAAAAAAAAAAAAAAAAAAAAAAA".to_vec(),
                })
            },
            Err(crate::page::InsertHyperlinkError::StringsOutOfMemory)
        ));
    }

    // Almost hit string alloc cap in top left of second page.
    // Mark the first row as a wrap continuation.
    {
        let page = unsafe { super::Node::page(s.pages.last) };
        let id = unsafe {
            (*page).insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Implicit(1),
                uri: vec![b'a'; crate::page::STRING_BYTES_DEFAULT - 1],
            })
        }
        .unwrap();
        let rac = unsafe { (*page).get_row_and_cell(0, 0) };
        unsafe {
            (*rac.row).set_wrap_continuation(true);
            *rac.cell = cell_codepoint('X' as u32);
        }
        unsafe { (*page).set_hyperlink(rac.row, rac.cell, id) }.unwrap();
        assert!(matches!(
            unsafe {
                (*page).insert_hyperlink(hyperlink::Hyperlink {
                    id: hyperlink::HyperlinkId::Implicit(2),
                    uri: b"AAAAAAAAAAAAAAAAAAAAAAAAAA".to_vec(),
                })
            },
            Err(crate::page::InsertHyperlinkError::StringsOutOfMemory)
        ));
    }

    // Resize to 1 column wider, unwrapping the row.
    s.resize(Resize {
        cols: Some(s.cols + 1),
        reflow: true,
        ..Default::default()
    })
    .unwrap();

    s.deinit();
}

// ref: test "PageList resize reflow exceeds grapheme memory forcing capacity increase"
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION (vt multi-page-PageList intractable, .port/phase-2/MIRI_EXCEPTIONS.md): builds/reflows multi-page PageLists that exceed Miri's interpreter budget (>20min); covered natively + by ASan (all PageList tests ~18s)"
)]
#[test]
fn pagelist_resize_reflow_exceeds_grapheme_memory_forcing_capacity_increase() {
    let mut s = PageList::init(2, 10, Some(0)).unwrap();
    assert_eq!(1usize, s.total_pages());

    // Grow to the capacity of the first page and add
    // one more row so that we have two pages total.
    {
        let page = unsafe { super::Node::page(s.pages.first) };
        unsafe { (*page).pause_integrity_checks(true) };
        for _ in unsafe { (*page).size.rows..(*page).capacity.rows } {
            let _ = s.grow().unwrap();
        }
        unsafe { (*super::Node::page(s.pages.first)).pause_integrity_checks(false) };
        assert_eq!(1usize, s.total_pages());
        s.grow_rows(1).unwrap();
        assert_eq!(2usize, s.total_pages());

        // We now have two pages.
        assert!(!core::ptr::eq(s.pages.first, s.pages.last));
        assert!(core::ptr::eq(s.pages.last, unsafe {
            (*s.pages.first).next
        }));
    }

    // We use almost all grapheme alloc capacity with a grapheme in the final
    // row of the first page, and do the same on the first row of the second
    // page. We also mark the row as wrapped so that when we resize with more
    // cols the row unwraps and we have a single row that requires almost two
    // times the base grapheme alloc capacity.
    //
    // This forces the reflow to increase capacity.
    //
    //  +--+ = PAGE 0
    //  :  :
    //  | X… <- where X is a grapheme which uses almost all the capacity.
    //  +--+
    //  +--+ = PAGE 1
    //  …X | <- X here also almost hits grapheme cap.
    //  +--+

    // grapheme count: @divFloor(grapheme_bytes_default - 1, @sizeOf(u21))
    // where u21 occupies 4 bytes; the Rust port stores graphemes as u32.
    let grapheme_n = (crate::page::GRAPHEME_BYTES_DEFAULT - 1) / core::mem::size_of::<u32>();
    let grapheme_cps: Vec<u32> = vec!['a' as u32; grapheme_n];

    // Almost hit grapheme alloc cap in bottom right of first page.
    // Mark the final row as wrapped.
    {
        let page = unsafe { super::Node::page(s.pages.first) };
        let rac = unsafe {
            (*page).get_row_and_cell(
                (*page).size.cols as usize - 1,
                (*page).size.rows as usize - 1,
            )
        };
        unsafe {
            (*rac.row).set_wrap(true);
            *rac.cell = cell_codepoint('X' as u32);
        }
        unsafe { (*page).set_graphemes(rac.row, rac.cell, &grapheme_cps) }.unwrap();
        let base = unsafe { (*page).memory_ptr() };
        assert!(unsafe { (*page).grapheme_alloc.alloc::<u32, _>(base, 16) }.is_err());
    }

    // Almost hit grapheme alloc cap in top left of second page.
    // Mark the first row as a wrap continuation.
    {
        let page = unsafe { super::Node::page(s.pages.last) };
        let rac = unsafe { (*page).get_row_and_cell(0, 0) };
        unsafe {
            (*rac.row).set_wrap(true);
            *rac.cell = cell_codepoint('X' as u32);
        }
        unsafe { (*page).set_graphemes(rac.row, rac.cell, &grapheme_cps) }.unwrap();
        let base = unsafe { (*page).memory_ptr() };
        assert!(unsafe { (*page).grapheme_alloc.alloc::<u32, _>(base, 16) }.is_err());
    }

    // Resize to 1 column wider, unwrapping the row.
    s.resize(Resize {
        cols: Some(s.cols + 1),
        reflow: true,
        ..Default::default()
    })
    .unwrap();

    s.deinit();
}

// ref: test "PageList resize reflow exceeds style memory forcing capacity increase"
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION (vt multi-page-PageList intractable, .port/phase-2/MIRI_EXCEPTIONS.md): builds/reflows multi-page PageLists that exceed Miri's interpreter budget (>20min); covered natively + by ASan (all PageList tests ~18s)"
)]
#[test]
fn pagelist_resize_reflow_exceeds_style_memory_forcing_capacity_increase() {
    let mut s = PageList::init(std_capacity().styles - 1, 10, Some(0)).unwrap();
    assert_eq!(1usize, s.total_pages());

    // Grow to the capacity of the first page and add
    // one more row so that we have two pages total.
    {
        let page = unsafe { super::Node::page(s.pages.first) };
        unsafe { (*page).pause_integrity_checks(true) };
        for _ in unsafe { (*page).size.rows..(*page).capacity.rows } {
            let _ = s.grow().unwrap();
        }
        unsafe { (*super::Node::page(s.pages.first)).pause_integrity_checks(false) };
        assert_eq!(1usize, s.total_pages());
        s.grow_rows(1).unwrap();
        assert_eq!(2usize, s.total_pages());

        // We now have two pages.
        assert!(!core::ptr::eq(s.pages.first, s.pages.last));
        assert!(core::ptr::eq(s.pages.last, unsafe {
            (*s.pages.first).next
        }));
    }

    // Give each cell in the final row of the first page a unique style.
    // Mark the final row as wrapped.
    {
        let page = unsafe { super::Node::page(s.pages.first) };
        let base = unsafe { (*page).memory_ptr() };
        for x in 0..s.cols as usize {
            let style = Style {
                bg_color: stylepkg::Color::Rgb(crate::color::RGB {
                    r: x as u8,
                    g: (x >> 8) as u8,
                    b: (x >> 16) as u8,
                }),
                ..Style::default()
            };
            let id = match unsafe { (*page).styles.add(base, style) } {
                Ok(id) => id,
                Err(_) => break,
            };

            let rac = unsafe { (*page).get_row_and_cell(x, (*page).size.rows as usize - 1) };
            unsafe {
                (*rac.row).set_wrap(true);
                (*rac.row).set_styled(true);
                let mut c = cell_codepoint('X' as u32);
                c.set_style_id(id);
                *rac.cell = c;
            }
        }
    }

    // Do the same for the first row of the second page.
    // Mark the first row as a wrap continuation.
    {
        let page = unsafe { super::Node::page(s.pages.last) };
        let base = unsafe { (*page).memory_ptr() };
        for x in 0..s.cols as usize {
            let style = Style {
                fg_color: stylepkg::Color::Rgb(crate::color::RGB {
                    r: x as u8,
                    g: (x >> 8) as u8,
                    b: (x >> 16) as u8,
                }),
                ..Style::default()
            };
            let id = match unsafe { (*page).styles.add(base, style) } {
                Ok(id) => id,
                Err(_) => break,
            };

            let rac = unsafe { (*page).get_row_and_cell(x, 0) };
            unsafe {
                (*rac.row).set_wrap_continuation(true);
                (*rac.row).set_styled(true);
                let mut c = cell_codepoint('X' as u32);
                c.set_style_id(id);
                *rac.cell = c;
            }
        }
    }

    // Resize to twice as wide, fully unwrapping the row.
    s.resize(Resize {
        cols: Some(s.cols * 2),
        reflow: true,
        ..Default::default()
    })
    .unwrap();

    s.deinit();
}

// ref: test "PageList resize reflow more cols unwrap wide spacer head"
#[test]
fn pagelist_resize_reflow_more_cols_unwrap_wide_spacer_head() {
    let mut s = PageList::init(2, 2, Some(0)).unwrap();
    {
        assert!(s.pages.first == s.pages.last);
        let page = unsafe { super::Node::page(s.pages.first) };

        {
            let rac = unsafe { (*page).get_row_and_cell(0, 0) };
            unsafe {
                (*rac.row).set_wrap(true);
                *rac.cell = cell_codepoint('x' as u32);
            }
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(1, 0) };
            unsafe {
                let mut c = cell_codepoint(0);
                c.set_wide(crate::page::Wide::SpacerHead);
                *rac.cell = c;
            }
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(0, 1) };
            unsafe {
                (*rac.row).set_wrap_continuation(true);
                let mut c = cell_codepoint('😀' as u32);
                c.set_wide(crate::page::Wide::Wide);
                *rac.cell = c;
            }
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(1, 1) };
            unsafe {
                let mut c = cell_codepoint(0);
                c.set_wide(crate::page::Wide::SpacerTail);
                *rac.cell = c;
            }
        }
    }

    // Resize
    s.resize(Resize {
        cols: Some(4),
        reflow: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(4usize, s.cols as usize);
    assert_eq!(2usize, s.total_rows());

    {
        assert!(s.pages.first == s.pages.last);
        let page = unsafe { super::Node::page(s.pages.first) };

        {
            let rac = unsafe { (*page).get_row_and_cell(0, 0) };
            assert_eq!('x' as u32, unsafe { (*rac.cell).content_codepoint() });
            assert_eq!(crate::page::Wide::Narrow, unsafe { (*rac.cell).wide() });
            assert!(!unsafe { (*rac.row).wrap() });
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(1, 0) };
            assert_eq!('😀' as u32, unsafe { (*rac.cell).content_codepoint() });
            assert_eq!(crate::page::Wide::Wide, unsafe { (*rac.cell).wide() });
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(2, 0) };
            assert_eq!(0u32, unsafe { (*rac.cell).content_codepoint() });
            assert_eq!(crate::page::Wide::SpacerTail, unsafe { (*rac.cell).wide() });
        }
    }

    s.deinit();
}

// ref: test "PageList resize reflow more cols unwrap wide spacer head across two rows"
#[test]
fn pagelist_resize_reflow_more_cols_unwrap_wide_spacer_head_across_two_rows() {
    let mut s = PageList::init(2, 3, Some(0)).unwrap();
    {
        assert!(s.pages.first == s.pages.last);
        let page = unsafe { super::Node::page(s.pages.first) };

        {
            let rac = unsafe { (*page).get_row_and_cell(0, 0) };
            unsafe {
                (*rac.row).set_wrap(true);
                *rac.cell = cell_codepoint('x' as u32);
            }
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(1, 0) };
            unsafe {
                *rac.cell = cell_codepoint('x' as u32);
            }
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(0, 1) };
            unsafe {
                (*rac.row).set_wrap_continuation(true);
                (*rac.row).set_wrap(true);
                *rac.cell = cell_codepoint('x' as u32);
            }
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(1, 1) };
            unsafe {
                let mut c = cell_codepoint(0);
                c.set_wide(crate::page::Wide::SpacerHead);
                *rac.cell = c;
            }
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(0, 2) };
            unsafe {
                (*rac.row).set_wrap_continuation(true);
                let mut c = cell_codepoint('😀' as u32);
                c.set_wide(crate::page::Wide::Wide);
                *rac.cell = c;
            }
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(1, 2) };
            unsafe {
                let mut c = cell_codepoint(0);
                c.set_wide(crate::page::Wide::SpacerTail);
                *rac.cell = c;
            }
        }
    }

    // Resize
    s.resize(Resize {
        cols: Some(4),
        reflow: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(4usize, s.cols as usize);
    assert_eq!(3usize, s.total_rows());

    {
        assert!(s.pages.first == s.pages.last);
        let page = unsafe { super::Node::page(s.pages.first) };

        {
            let rac = unsafe { (*page).get_row_and_cell(0, 0) };
            assert_eq!('x' as u32, unsafe { (*rac.cell).content_codepoint() });
            assert_eq!(crate::page::Wide::Narrow, unsafe { (*rac.cell).wide() });
            assert!(unsafe { (*rac.row).wrap() });
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(1, 0) };
            assert_eq!('x' as u32, unsafe { (*rac.cell).content_codepoint() });
            assert_eq!(crate::page::Wide::Narrow, unsafe { (*rac.cell).wide() });
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(2, 0) };
            assert_eq!('x' as u32, unsafe { (*rac.cell).content_codepoint() });
            assert_eq!(crate::page::Wide::Narrow, unsafe { (*rac.cell).wide() });
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(3, 0) };
            assert_eq!(0u32, unsafe { (*rac.cell).content_codepoint() });
            assert_eq!(crate::page::Wide::SpacerHead, unsafe { (*rac.cell).wide() });
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(0, 1) };
            assert_eq!('😀' as u32, unsafe { (*rac.cell).content_codepoint() });
            assert_eq!(crate::page::Wide::Wide, unsafe { (*rac.cell).wide() });
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(1, 1) };
            assert_eq!(0u32, unsafe { (*rac.cell).content_codepoint() });
            assert_eq!(crate::page::Wide::SpacerTail, unsafe { (*rac.cell).wide() });
        }
    }

    s.deinit();
}

// ref: test "PageList resize reflow more cols unwrap still requires wide spacer head"
#[test]
fn pagelist_resize_reflow_more_cols_unwrap_still_requires_wide_spacer_head() {
    let mut s = PageList::init(2, 2, Some(0)).unwrap();
    {
        assert!(s.pages.first == s.pages.last);
        let page = unsafe { super::Node::page(s.pages.first) };

        {
            let rac = unsafe { (*page).get_row_and_cell(0, 0) };
            unsafe {
                (*rac.row).set_wrap(true);
                *rac.cell = cell_codepoint('x' as u32);
            }
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(1, 0) };
            unsafe {
                *rac.cell = cell_codepoint('x' as u32);
            }
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(0, 1) };
            unsafe {
                (*rac.row).set_wrap_continuation(true);
                let mut c = cell_codepoint('😀' as u32);
                c.set_wide(crate::page::Wide::Wide);
                *rac.cell = c;
            }
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(1, 1) };
            unsafe {
                let mut c = cell_codepoint(0);
                c.set_wide(crate::page::Wide::SpacerTail);
                *rac.cell = c;
            }
        }
    }

    // Resize
    s.resize(Resize {
        cols: Some(3),
        reflow: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(3usize, s.cols as usize);
    assert_eq!(2usize, s.total_rows());

    {
        assert!(s.pages.first == s.pages.last);
        let page = unsafe { super::Node::page(s.pages.first) };

        {
            let rac = unsafe { (*page).get_row_and_cell(0, 0) };
            assert_eq!('x' as u32, unsafe { (*rac.cell).content_codepoint() });
            assert_eq!(crate::page::Wide::Narrow, unsafe { (*rac.cell).wide() });
            assert!(unsafe { (*rac.row).wrap() });
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(1, 0) };
            assert_eq!('x' as u32, unsafe { (*rac.cell).content_codepoint() });
            assert_eq!(crate::page::Wide::Narrow, unsafe { (*rac.cell).wide() });
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(2, 0) };
            assert_eq!(0u32, unsafe { (*rac.cell).content_codepoint() });
            assert_eq!(crate::page::Wide::SpacerHead, unsafe { (*rac.cell).wide() });
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(0, 1) };
            assert_eq!('😀' as u32, unsafe { (*rac.cell).content_codepoint() });
            assert_eq!(crate::page::Wide::Wide, unsafe { (*rac.cell).wide() });
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(1, 1) };
            assert_eq!(0u32, unsafe { (*rac.cell).content_codepoint() });
            assert_eq!(crate::page::Wide::SpacerTail, unsafe { (*rac.cell).wide() });
        }
    }

    s.deinit();
}

// ref: test "PageList resize reflow less cols no reflow preserves semantic prompt"
#[test]
fn pagelist_resize_reflow_less_cols_no_reflow_preserves_semantic_prompt() {
    let mut s = PageList::init(4, 4, Some(0)).unwrap();
    {
        assert!(s.pages.first == s.pages.last);
        let page = unsafe { super::Node::page(s.pages.first) };
        {
            let rac = unsafe { (*page).get_row_and_cell(0, 1) };
            unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };
        }
        for x in 0..s.cols as usize {
            let rac = unsafe { (*page).get_row_and_cell(x, 1) };
            unsafe {
                *rac.cell = cell_codepoint(x as u32);
            }
        }
    }

    // Resize
    s.resize(Resize {
        cols: Some(2),
        reflow: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(2usize, s.cols as usize);
    assert_eq!(4usize, s.total_rows());

    {
        assert!(s.pages.first == s.pages.last);
        {
            let p = s
                .pin(point::Point::Active(point::Coordinate { x: 0, y: 1 }))
                .unwrap();
            let rac = p.row_and_cell();
            assert!(unsafe { (*rac.row).wrap() });
            assert!(unsafe { (*rac.row).semantic_prompt() } == SemanticPrompt::Prompt);
        }
        {
            let p = s
                .pin(point::Point::Active(point::Coordinate { x: 0, y: 2 }))
                .unwrap();
            let rac = p.row_and_cell();
            assert!(unsafe { (*rac.row).semantic_prompt() } == SemanticPrompt::Prompt);
        }
    }

    s.deinit();
}

// ref: test "PageList resize reflow less cols no reflow preserves semantic prompt on first line"
#[test]
fn pagelist_resize_reflow_less_cols_no_reflow_preserves_semantic_prompt_on_first_line() {
    let mut s = PageList::init(4, 4, Some(0)).unwrap();
    {
        assert!(s.pages.first == s.pages.last);
        let page = unsafe { super::Node::page(s.pages.first) };
        let rac = unsafe { (*page).get_row_and_cell(0, 0) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };
    }

    // Resize
    s.resize(Resize {
        cols: Some(2),
        reflow: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(2usize, s.cols as usize);
    assert_eq!(4usize, s.total_rows());

    {
        assert!(s.pages.first == s.pages.last);
        let page = unsafe { super::Node::page(s.pages.first) };
        let rac = unsafe { (*page).get_row_and_cell(0, 0) };
        assert!(unsafe { (*rac.row).semantic_prompt() } == SemanticPrompt::Prompt);
    }

    s.deinit();
}

// ref: test "PageList resize reflow less cols wrap preserves semantic prompt"
#[test]
fn pagelist_resize_reflow_less_cols_wrap_preserves_semantic_prompt() {
    let mut s = PageList::init(4, 4, Some(0)).unwrap();
    {
        assert!(s.pages.first == s.pages.last);
        let page = unsafe { super::Node::page(s.pages.first) };
        let rac = unsafe { (*page).get_row_and_cell(0, 0) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };
    }

    // Resize
    s.resize(Resize {
        cols: Some(2),
        reflow: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(2usize, s.cols as usize);
    assert_eq!(4usize, s.total_rows());

    {
        assert!(s.pages.first == s.pages.last);
        let page = unsafe { super::Node::page(s.pages.first) };
        let rac = unsafe { (*page).get_row_and_cell(0, 0) };
        assert!(unsafe { (*rac.row).semantic_prompt() } == SemanticPrompt::Prompt);
    }

    s.deinit();
}

// ref: test "PageList resize reflow less cols no wrapped rows"
#[test]
fn pagelist_resize_reflow_less_cols_no_wrapped_rows() {
    let mut s = PageList::init(10, 3, Some(0)).unwrap();
    assert!(s.pages.first == s.pages.last);
    let page = unsafe { super::Node::page(s.pages.first) };
    for y in 0..s.rows as usize {
        let end = 4;
        assert!(end < s.cols as usize);
        for x in 0..4usize {
            let rac = unsafe { (*page).get_row_and_cell(x, y) };
            unsafe {
                *rac.cell = cell_codepoint(x as u32);
            }
        }
    }

    // Resize
    s.resize(Resize {
        cols: Some(5),
        reflow: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(5usize, s.cols as usize);
    assert_eq!(3usize, s.total_rows());

    let mut it = s.row_iterator(
        super::iter::Direction::RightDown,
        point::Point::Screen(point::Coordinate::default()),
        None,
    );
    while let Some(offset) = it.next() {
        for x in 0..4usize {
            let mut offset_copy = pin_copy(&offset as *const Pin as *mut Pin);
            offset_copy.x = x as size::CellCountInt;
            let rac = offset_copy.row_and_cell();
            let cells = unsafe { (*super::Node::page(offset.node)).get_cells(rac.row) };
            assert_eq!(5usize, cells.len());
            assert_eq!(x as u32, unsafe {
                (*(*cells).as_ptr().add(x)).content_codepoint()
            });
        }
    }

    s.deinit();
}

// ref: test "PageList resize reflow less cols wrapped rows"
#[test]
fn pagelist_resize_reflow_less_cols_wrapped_rows() {
    let mut s = PageList::init(4, 2, None).unwrap();
    assert!(s.pages.first == s.pages.last);
    let page = unsafe { super::Node::page(s.pages.first) };
    for y in 0..s.rows as usize {
        for x in 0..s.cols as usize {
            let rac = unsafe { (*page).get_row_and_cell(x, y) };
            unsafe {
                *rac.cell = cell_codepoint(x as u32);
            }
        }
    }

    // Resize
    s.resize(Resize {
        cols: Some(2),
        reflow: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(2usize, s.cols as usize);
    assert_eq!(4usize, s.total_rows());

    // Active moves due to scrollback
    {
        let pt = s
            .get_cell(point::Point::Active(point::Coordinate::default()))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 2 }), pt);
    }

    let mut it = s.row_iterator(
        super::iter::Direction::RightDown,
        point::Point::Screen(point::Coordinate::default()),
        None,
    );
    {
        // First row should be wrapped
        let offset = it.next().unwrap();
        let rac = offset.row_and_cell();
        let cells = unsafe { (*super::Node::page(offset.node)).get_cells(rac.row) };
        assert!(unsafe { (*rac.row).wrap() });
        assert_eq!(2usize, cells.len());
        assert_eq!(0u32, unsafe { (*(*cells).as_ptr()).content_codepoint() });
    }
    {
        let offset = it.next().unwrap();
        let rac = offset.row_and_cell();
        let cells = unsafe { (*super::Node::page(offset.node)).get_cells(rac.row) };
        assert!(!unsafe { (*rac.row).wrap() });
        assert_eq!(2usize, cells.len());
        assert_eq!(2u32, unsafe { (*(*cells).as_ptr()).content_codepoint() });
    }
    {
        // First row should be wrapped
        let offset = it.next().unwrap();
        let rac = offset.row_and_cell();
        let cells = unsafe { (*super::Node::page(offset.node)).get_cells(rac.row) };
        assert!(unsafe { (*rac.row).wrap() });
        assert_eq!(2usize, cells.len());
        assert_eq!(0u32, unsafe { (*(*cells).as_ptr()).content_codepoint() });
    }
    {
        let offset = it.next().unwrap();
        let rac = offset.row_and_cell();
        let cells = unsafe { (*super::Node::page(offset.node)).get_cells(rac.row) };
        assert!(!unsafe { (*rac.row).wrap() });
        assert_eq!(2usize, cells.len());
        assert_eq!(2u32, unsafe { (*(*cells).as_ptr()).content_codepoint() });
    }

    s.deinit();
}

// ref: test "PageList resize reflow less cols wrapped rows with graphemes"
#[test]
fn pagelist_resize_reflow_less_cols_wrapped_rows_with_graphemes() {
    let mut s = PageList::init(4, 2, None).unwrap();
    {
        assert!(s.pages.first == s.pages.last);
        let page = unsafe { super::Node::page(s.pages.first) };
        for y in 0..s.rows as usize {
            for x in 0..s.cols as usize {
                let rac = unsafe { (*page).get_row_and_cell(x, y) };
                unsafe {
                    *rac.cell = cell_codepoint(x as u32);
                }
            }

            let rac = unsafe { (*page).get_row_and_cell(2, y) };
            unsafe { (*page).append_grapheme(rac.row, rac.cell, 'A' as u32) }.unwrap();
        }
    }

    // Resize
    s.resize(Resize {
        cols: Some(2),
        reflow: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(2usize, s.cols as usize);
    assert_eq!(4usize, s.total_rows());

    // Active moves due to scrollback
    {
        let pt = s
            .get_cell(point::Point::Active(point::Coordinate::default()))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 2 }), pt);
    }

    assert!(s.pages.first == s.pages.last);
    let page = unsafe { super::Node::page(s.pages.first) };
    let mut it = s.row_iterator(
        super::iter::Direction::RightDown,
        point::Point::Screen(point::Coordinate::default()),
        None,
    );
    {
        // First row should be wrapped
        let offset = it.next().unwrap();
        let rac = offset.row_and_cell();
        let cells = unsafe { (*super::Node::page(offset.node)).get_cells(rac.row) };
        assert!(unsafe { (*rac.row).wrap() });
        assert_eq!(2usize, cells.len());
        assert_eq!(0u32, unsafe { (*(*cells).as_ptr()).content_codepoint() });
    }
    {
        let offset = it.next().unwrap();
        let rac = offset.row_and_cell();
        let cells = unsafe { (*super::Node::page(offset.node)).get_cells(rac.row) };
        assert!(!unsafe { (*rac.row).wrap() });
        assert!(unsafe { (*rac.row).grapheme() });
        assert_eq!(2usize, cells.len());
        assert_eq!(2u32, unsafe { (*(*cells).as_ptr()).content_codepoint() });

        let cps = unsafe { (*page).lookup_grapheme(rac.cell) }.unwrap();
        assert_eq!(1usize, cps.len());
        assert_eq!('A' as u32, cps[0]);
    }
    {
        // First row should be wrapped
        let offset = it.next().unwrap();
        let rac = offset.row_and_cell();
        let cells = unsafe { (*super::Node::page(offset.node)).get_cells(rac.row) };
        assert!(unsafe { (*rac.row).wrap() });
        assert_eq!(2usize, cells.len());
        assert_eq!(0u32, unsafe { (*(*cells).as_ptr()).content_codepoint() });
    }
    {
        let offset = it.next().unwrap();
        let rac = offset.row_and_cell();
        let cells = unsafe { (*super::Node::page(offset.node)).get_cells(rac.row) };
        assert!(!unsafe { (*rac.row).wrap() });
        assert!(unsafe { (*rac.row).grapheme() });
        assert_eq!(2usize, cells.len());
        assert_eq!(2u32, unsafe { (*(*cells).as_ptr()).content_codepoint() });

        let cps = unsafe { (*page).lookup_grapheme(rac.cell) }.unwrap();
        assert_eq!(1usize, cps.len());
        assert_eq!('A' as u32, cps[0]);
    }

    s.deinit();
}

// ref: test "PageList resize reflow less cols cursor in wrapped row"
#[test]
fn pagelist_resize_reflow_less_cols_cursor_in_wrapped_row() {
    let mut s = PageList::init(4, 2, None).unwrap();
    assert!(s.pages.first == s.pages.last);
    let page = unsafe { super::Node::page(s.pages.first) };
    for y in 0..s.rows as usize {
        for x in 0..s.cols as usize {
            let rac = unsafe { (*page).get_row_and_cell(x, y) };
            unsafe {
                *rac.cell = cell_codepoint(x as u32);
            }
        }
    }

    // Put a tracked pin in the history
    let p = s
        .track_pin(
            s.pin(point::Point::Active(point::Coordinate { x: 2, y: 1 }))
                .unwrap(),
        )
        .unwrap();

    // Resize
    s.resize(Resize {
        cols: Some(2),
        reflow: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(2usize, s.cols as usize);
    assert_eq!(4usize, s.total_rows());

    // Our cursor should move to the first row
    assert_eq!(
        point::Point::Active(point::Coordinate { x: 0, y: 1 }),
        s.point_from_pin(point::Tag::Active, pin_copy(p)).unwrap()
    );

    s.untrack_pin(p);
    s.deinit();
}

// ref: test "PageList resize reflow less cols wraps spacer head"
#[test]
fn pagelist_resize_reflow_less_cols_wraps_spacer_head() {
    let mut s = PageList::init(4, 3, Some(0)).unwrap();
    {
        assert!(s.pages.first == s.pages.last);
        let page = unsafe { super::Node::page(s.pages.first) };

        {
            let rac = unsafe { (*page).get_row_and_cell(0, 0) };
            unsafe {
                (*rac.row).set_wrap(true);
                *rac.cell = cell_codepoint('x' as u32);
            }
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(1, 0) };
            unsafe {
                *rac.cell = cell_codepoint('x' as u32);
            }
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(2, 0) };
            unsafe {
                *rac.cell = cell_codepoint('x' as u32);
            }
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(3, 0) };
            unsafe {
                let mut c = cell_codepoint(0);
                c.set_wide(crate::page::Wide::SpacerHead);
                *rac.cell = c;
            }
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(0, 1) };
            unsafe {
                (*rac.row).set_wrap_continuation(true);
                let mut c = cell_codepoint('😀' as u32);
                c.set_wide(crate::page::Wide::Wide);
                *rac.cell = c;
            }
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(1, 1) };
            unsafe {
                let mut c = cell_codepoint(0);
                c.set_wide(crate::page::Wide::SpacerTail);
                *rac.cell = c;
            }
        }
    }

    // Resize
    s.resize(Resize {
        cols: Some(3),
        reflow: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(3usize, s.cols as usize);
    assert_eq!(3usize, s.total_rows());

    {
        assert!(s.pages.first == s.pages.last);
        let page = unsafe { super::Node::page(s.pages.first) };

        {
            let rac = unsafe { (*page).get_row_and_cell(0, 0) };
            assert_eq!('x' as u32, unsafe { (*rac.cell).content_codepoint() });
            assert_eq!(crate::page::Wide::Narrow, unsafe { (*rac.cell).wide() });
            assert!(unsafe { (*rac.row).wrap() });
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(1, 0) };
            assert_eq!('x' as u32, unsafe { (*rac.cell).content_codepoint() });
            assert_eq!(crate::page::Wide::Narrow, unsafe { (*rac.cell).wide() });
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(2, 0) };
            assert_eq!('x' as u32, unsafe { (*rac.cell).content_codepoint() });
            assert_eq!(crate::page::Wide::Narrow, unsafe { (*rac.cell).wide() });
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(0, 1) };
            assert_eq!('😀' as u32, unsafe { (*rac.cell).content_codepoint() });
            assert_eq!(crate::page::Wide::Wide, unsafe { (*rac.cell).wide() });
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(1, 1) };
            assert_eq!(0u32, unsafe { (*rac.cell).content_codepoint() });
            assert_eq!(crate::page::Wide::SpacerTail, unsafe { (*rac.cell).wide() });
        }
    }

    s.deinit();
}

// ref: test "PageList resize reflow less cols cursor goes to scrollback"
#[test]
fn pagelist_resize_reflow_less_cols_cursor_goes_to_scrollback() {
    let mut s = PageList::init(4, 2, None).unwrap();
    assert!(s.pages.first == s.pages.last);
    let page = unsafe { super::Node::page(s.pages.first) };
    for y in 0..s.rows {
        for x in 0..s.cols {
            let rac = unsafe { (*page).get_row_and_cell(x as usize, y as usize) };
            unsafe {
                *rac.cell = cell_codepoint(x as u32);
            }
        }
    }

    // Put a tracked pin in the history
    let p = s
        .track_pin(
            s.pin(point::Point::Active(point::Coordinate { x: 2, y: 0 }))
                .unwrap(),
        )
        .unwrap();

    // Resize
    s.resize(Resize {
        cols: Some(2),
        reflow: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(2usize, s.cols as usize);
    assert_eq!(4usize, s.total_rows());

    // Our cursor should move to the first row
    assert!(s.point_from_pin(point::Tag::Active, pin_copy(p)).is_none());

    s.untrack_pin(p);
    s.deinit();
}

// ref: test "PageList resize reflow less cols cursor in unchanged row"
#[test]
fn pagelist_resize_reflow_less_cols_cursor_in_unchanged_row() {
    let mut s = PageList::init(4, 2, None).unwrap();
    assert!(s.pages.first == s.pages.last);
    let page = unsafe { super::Node::page(s.pages.first) };
    for y in 0..s.rows {
        for x in 0..2 {
            let rac = unsafe { (*page).get_row_and_cell(x as usize, y as usize) };
            unsafe {
                *rac.cell = cell_codepoint(x as u32);
            }
        }
    }

    // Put a tracked pin in the history
    let p = s
        .track_pin(
            s.pin(point::Point::Active(point::Coordinate { x: 1, y: 0 }))
                .unwrap(),
        )
        .unwrap();

    // Resize
    s.resize(Resize {
        cols: Some(2),
        reflow: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(2usize, s.cols as usize);
    assert_eq!(2usize, s.total_rows());

    // Our cursor should move to the first row
    assert_eq!(
        point::Point::Active(point::Coordinate { x: 1, y: 0 }),
        s.point_from_pin(point::Tag::Active, pin_copy(p)).unwrap()
    );

    s.untrack_pin(p);
    s.deinit();
}

// ref: test "PageList resize reflow less cols cursor in blank cell"
#[test]
fn pagelist_resize_reflow_less_cols_cursor_in_blank_cell() {
    let mut s = PageList::init(6, 2, None).unwrap();
    assert!(s.pages.first == s.pages.last);
    let page = unsafe { super::Node::page(s.pages.first) };
    for y in 0..s.rows {
        for x in 0..2 {
            let rac = unsafe { (*page).get_row_and_cell(x as usize, y as usize) };
            unsafe {
                *rac.cell = cell_codepoint(x as u32);
            }
        }
    }

    // Put a tracked pin in the history
    let p = s
        .track_pin(
            s.pin(point::Point::Active(point::Coordinate { x: 2, y: 0 }))
                .unwrap(),
        )
        .unwrap();

    // Resize
    s.resize(Resize {
        cols: Some(4),
        reflow: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(4usize, s.cols as usize);
    assert_eq!(2usize, s.total_rows());

    // Our cursor should not move
    assert_eq!(
        point::Point::Active(point::Coordinate { x: 2, y: 0 }),
        s.point_from_pin(point::Tag::Active, pin_copy(p)).unwrap()
    );

    s.untrack_pin(p);
    s.deinit();
}

// ref: test "PageList resize reflow less cols cursor in final blank cell"
#[test]
fn pagelist_resize_reflow_less_cols_cursor_in_final_blank_cell() {
    let mut s = PageList::init(6, 2, None).unwrap();
    assert!(s.pages.first == s.pages.last);
    let page = unsafe { super::Node::page(s.pages.first) };
    for y in 0..s.rows {
        for x in 0..2 {
            let rac = unsafe { (*page).get_row_and_cell(x as usize, y as usize) };
            unsafe {
                *rac.cell = cell_codepoint(x as u32);
            }
        }
    }

    // Put a tracked pin in the history
    let p = s
        .track_pin(
            s.pin(point::Point::Active(point::Coordinate { x: 3, y: 0 }))
                .unwrap(),
        )
        .unwrap();

    // Resize
    s.resize(Resize {
        cols: Some(4),
        reflow: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(4usize, s.cols as usize);
    assert_eq!(2usize, s.total_rows());

    // Our cursor should move to the first row
    assert_eq!(
        point::Point::Active(point::Coordinate { x: 3, y: 0 }),
        s.point_from_pin(point::Tag::Active, pin_copy(p)).unwrap()
    );

    s.untrack_pin(p);
    s.deinit();
}

// ref: test "PageList resize reflow less cols cursor in wrapped blank cell"
#[test]
fn pagelist_resize_reflow_less_cols_cursor_in_wrapped_blank_cell() {
    let mut s = PageList::init(6, 2, None).unwrap();
    assert!(s.pages.first == s.pages.last);
    let page = unsafe { super::Node::page(s.pages.first) };
    for y in 0..s.rows {
        for x in 0..2 {
            let rac = unsafe { (*page).get_row_and_cell(x as usize, y as usize) };
            unsafe {
                *rac.cell = cell_codepoint(x as u32);
            }
        }
    }

    // Put a tracked pin in the history
    let p = s
        .track_pin(
            s.pin(point::Point::Active(point::Coordinate { x: 5, y: 0 }))
                .unwrap(),
        )
        .unwrap();

    // Resize
    s.resize(Resize {
        cols: Some(4),
        reflow: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(4usize, s.cols as usize);
    assert_eq!(2usize, s.total_rows());

    // Our cursor should move to the first row
    assert_eq!(
        point::Point::Active(point::Coordinate { x: 3, y: 0 }),
        s.point_from_pin(point::Tag::Active, pin_copy(p)).unwrap()
    );

    s.untrack_pin(p);
    s.deinit();
}

// ref: test "PageList resize reflow less cols blank lines"
#[test]
fn pagelist_resize_reflow_less_cols_blank_lines() {
    let mut s = PageList::init(4, 3, Some(0)).unwrap();
    assert!(s.pages.first == s.pages.last);
    let page = unsafe { super::Node::page(s.pages.first) };
    for y in 0..1 {
        for x in 0..4 {
            let rac = unsafe { (*page).get_row_and_cell(x as usize, y as usize) };
            unsafe {
                *rac.cell = cell_codepoint(x as u32);
            }
        }
    }

    // Resize
    s.resize(Resize {
        cols: Some(2),
        reflow: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(2usize, s.cols as usize);
    assert_eq!(3usize, s.total_rows());

    let mut it = s.row_iterator(
        super::iter::Direction::RightDown,
        point::Point::Active(point::Coordinate::default()),
        None,
    );
    {
        // First row should be wrapped
        let offset = it.next().unwrap();
        let rac = offset.row_and_cell();
        let cells = unsafe { (*super::Node::page(offset.node)).get_cells(rac.row) };
        assert!(unsafe { (*rac.row).wrap() });
        assert_eq!(2usize, cells.len());
        assert_eq!(0u32, unsafe { (*cells)[0].content_codepoint() });
    }
    {
        let offset = it.next().unwrap();
        let rac = offset.row_and_cell();
        let cells = unsafe { (*super::Node::page(offset.node)).get_cells(rac.row) };
        assert!(!unsafe { (*rac.row).wrap() });
        assert_eq!(2usize, cells.len());
        assert_eq!(2u32, unsafe { (*cells)[0].content_codepoint() });
    }

    s.deinit();
}

// ref: test "PageList resize reflow less cols blank lines between"
#[test]
fn pagelist_resize_reflow_less_cols_blank_lines_between() {
    let mut s = PageList::init(4, 3, Some(0)).unwrap();
    assert!(s.pages.first == s.pages.last);
    let page = unsafe { super::Node::page(s.pages.first) };
    {
        for x in 0..4 {
            let rac = unsafe { (*page).get_row_and_cell(x as usize, 0) };
            unsafe {
                *rac.cell = cell_codepoint(x as u32);
            }
        }
    }
    {
        for x in 0..4 {
            let rac = unsafe { (*page).get_row_and_cell(x as usize, 2) };
            unsafe {
                *rac.cell = cell_codepoint(x as u32);
            }
        }
    }

    // Resize
    s.resize(Resize {
        cols: Some(2),
        reflow: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(2usize, s.cols as usize);
    assert_eq!(5usize, s.total_rows());

    let mut it = s.row_iterator(
        super::iter::Direction::RightDown,
        point::Point::Active(point::Coordinate::default()),
        None,
    );
    {
        let offset = it.next().unwrap();
        let rac = offset.row_and_cell();
        assert!(!unsafe { (*rac.row).wrap() });
    }
    {
        let offset = it.next().unwrap();
        let rac = offset.row_and_cell();
        let cells = unsafe { (*super::Node::page(offset.node)).get_cells(rac.row) };
        assert!(unsafe { (*rac.row).wrap() });
        assert_eq!(2usize, cells.len());
        assert_eq!(0u32, unsafe { (*cells)[0].content_codepoint() });
    }
    {
        let offset = it.next().unwrap();
        let rac = offset.row_and_cell();
        let cells = unsafe { (*super::Node::page(offset.node)).get_cells(rac.row) };
        assert!(!unsafe { (*rac.row).wrap() });
        assert_eq!(2usize, cells.len());
        assert_eq!(2u32, unsafe { (*cells)[0].content_codepoint() });
    }

    s.deinit();
}

// ref: test "PageList resize reflow less cols blank lines between no scrollback"
#[test]
fn pagelist_resize_reflow_less_cols_blank_lines_between_no_scrollback() {
    let mut s = PageList::init(5, 3, Some(0)).unwrap();
    assert!(s.pages.first == s.pages.last);
    let page = unsafe { super::Node::page(s.pages.first) };
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 0) };
        unsafe {
            *rac.cell = cell_codepoint('A' as u32);
        }
    }
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 2) };
        unsafe {
            *rac.cell = cell_codepoint('C' as u32);
        }
    }

    // Resize
    s.resize(Resize {
        cols: Some(2),
        reflow: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(2usize, s.cols as usize);
    assert_eq!(3usize, s.total_rows());

    let mut it = s.row_iterator(
        super::iter::Direction::RightDown,
        point::Point::Active(point::Coordinate::default()),
        None,
    );
    {
        let offset = it.next().unwrap();
        let rac = offset.row_and_cell();
        let cells = unsafe { (*super::Node::page(offset.node)).get_cells(rac.row) };
        assert!(!unsafe { (*rac.row).wrap() });
        assert_eq!(2usize, cells.len());
        assert_eq!('A' as u32, unsafe { (*cells)[0].content_codepoint() });
    }
    {
        let offset = it.next().unwrap();
        let rac = offset.row_and_cell();
        let cells = unsafe { (*super::Node::page(offset.node)).get_cells(rac.row) };
        assert_eq!(0u32, unsafe { (*cells)[0].content_codepoint() });
    }
    {
        let offset = it.next().unwrap();
        let rac = offset.row_and_cell();
        let cells = unsafe { (*super::Node::page(offset.node)).get_cells(rac.row) };
        assert!(!unsafe { (*rac.row).wrap() });
        assert_eq!(2usize, cells.len());
        assert_eq!('C' as u32, unsafe { (*cells)[0].content_codepoint() });
    }

    s.deinit();
}

// ref: test "PageList resize reflow less cols cursor not on last line preserves location"
#[test]
fn pagelist_resize_reflow_less_cols_cursor_not_on_last_line_preserves_location() {
    let mut s = PageList::init(5, 5, Some(1)).unwrap();
    assert!(s.pages.first == s.pages.last);
    let page = unsafe { super::Node::page(s.pages.first) };
    for y in 0..s.rows {
        for x in 0..2 {
            let rac = unsafe { (*page).get_row_and_cell(x as usize, y as usize) };
            unsafe {
                *rac.cell = cell_codepoint(x as u32);
            }
        }
    }

    // Grow blank rows to push our rows back into scrollback
    s.grow_rows(5).unwrap();
    assert_eq!(10usize, s.total_rows());

    // Put a tracked pin in the history
    let p = s
        .track_pin(
            s.pin(point::Point::Active(point::Coordinate { x: 0, y: 0 }))
                .unwrap(),
        )
        .unwrap();

    // Resize
    s.resize(Resize {
        cols: Some(4),
        reflow: true,

        // Important: not on last row
        cursor: Some(super::resize::Cursor {
            x: 1,
            y: 1,
            pin: None,
        }),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(4usize, s.cols as usize);
    assert_eq!(10usize, s.total_rows());

    // Our cursor should move to the first row
    assert_eq!(
        point::Point::Active(point::Coordinate { x: 0, y: 0 }),
        s.point_from_pin(point::Tag::Active, pin_copy(p)).unwrap()
    );

    s.untrack_pin(p);
    s.deinit();
}

// =====================================================================
// resize reflow (style / wide / grapheme / kitty placeholder), grapheme
// map capacity, viewport-pin/backfill remap (batch vt-3c)
// =====================================================================

// ref: test "PageList resize reflow less cols copy style"
#[test]
fn pagelist_resize_reflow_less_cols_copy_style() {
    let mut s = PageList::init(4, 2, Some(0)).unwrap();
    {
        assert!(s.pages.first == s.pages.last);
        let page = unsafe { super::Node::page(s.pages.first) };

        // Create a style
        let style = Style {
            flags: stylepkg::Flags {
                bold: true,
                ..stylepkg::Flags::default()
            },
            ..Style::default()
        };
        let base = unsafe { (*page).memory_ptr() };
        let style_id = unsafe { (*page).styles.add(base, style) }.unwrap();

        for x in 0..s.cols as usize - 1 {
            let rac = unsafe { (*page).get_row_and_cell(x, 0) };
            unsafe {
                let mut c = cell_codepoint(x as u32);
                c.set_style_id(style_id);
                *rac.cell = c;
                (*page).styles.use_id(base, style_id);
            }
        }

        // We're over-counted by 1 because `add` implies `use`.
        unsafe { (*page).styles.release(base, style_id) };
    }

    // Resize
    s.resize(Resize {
        cols: Some(2),
        reflow: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(2usize, s.cols as usize);
    assert_eq!(2usize, s.total_rows());

    let mut it = s.row_iterator(
        super::iter::Direction::RightDown,
        point::Point::Active(point::Coordinate::default()),
        None,
    );
    while let Some(offset) = it.next() {
        for x in 0..s.cols as usize - 1 {
            let mut offset_copy = offset.pin_copy();
            offset_copy.x = x as u16;
            let rac = offset_copy.row_and_cell();
            let style_id = unsafe { (*rac.cell).style_id() };
            assert!(style_id != 0);

            let style = unsafe {
                &*(*super::Node::page(offset.node))
                    .styles
                    .get((*super::Node::page(offset.node)).memory_ptr(), style_id)
            };
            assert!(style.flags.bold);

            assert!(unsafe { (*rac.row).styled() });
        }
    }

    s.deinit();
}

// ref: test "PageList resize reflow less cols to eliminate a wide char"
#[test]
fn pagelist_resize_reflow_less_cols_to_eliminate_a_wide_char() {
    let mut s = PageList::init(2, 1, Some(0)).unwrap();
    {
        assert!(s.pages.first == s.pages.last);
        let page = unsafe { super::Node::page(s.pages.first) };

        {
            let rac = unsafe { (*page).get_row_and_cell(0, 0) };
            unsafe {
                let mut c = cell_codepoint('😀' as u32);
                c.set_wide(crate::page::Wide::Wide);
                *rac.cell = c;
            }
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(1, 0) };
            unsafe {
                let mut c = cell_codepoint(0);
                c.set_wide(crate::page::Wide::SpacerTail);
                *rac.cell = c;
            }
        }
    }

    // Resize
    s.resize(Resize {
        cols: Some(1),
        reflow: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(1usize, s.cols as usize);
    assert_eq!(1usize, s.total_rows());

    {
        assert!(s.pages.first == s.pages.last);
        let page = unsafe { super::Node::page(s.pages.first) };

        {
            let rac = unsafe { (*page).get_row_and_cell(0, 0) };
            assert_eq!(0u32, unsafe { (*rac.cell).content_codepoint() });
            assert_eq!(crate::page::Wide::Narrow, unsafe { (*rac.cell).wide() });
        }
    }

    s.deinit();
}

// ref: test "PageList resize reflow less cols to wrap a wide char"
#[test]
fn pagelist_resize_reflow_less_cols_to_wrap_a_wide_char() {
    let mut s = PageList::init(3, 1, Some(0)).unwrap();
    {
        assert!(s.pages.first == s.pages.last);
        let page = unsafe { super::Node::page(s.pages.first) };

        {
            let rac = unsafe { (*page).get_row_and_cell(0, 0) };
            unsafe {
                *rac.cell = cell_codepoint('x' as u32);
            }
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(1, 0) };
            unsafe {
                let mut c = cell_codepoint('😀' as u32);
                c.set_wide(crate::page::Wide::Wide);
                *rac.cell = c;
            }
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(2, 0) };
            unsafe {
                let mut c = cell_codepoint(0);
                c.set_wide(crate::page::Wide::SpacerTail);
                *rac.cell = c;
            }
        }
    }

    // Resize
    s.resize(Resize {
        cols: Some(2),
        reflow: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(2usize, s.cols as usize);
    assert_eq!(2usize, s.total_rows());

    {
        assert!(s.pages.first == s.pages.last);
        let page = unsafe { super::Node::page(s.pages.first) };

        {
            let rac = unsafe { (*page).get_row_and_cell(0, 0) };
            assert_eq!('x' as u32, unsafe { (*rac.cell).content_codepoint() });
            assert_eq!(crate::page::Wide::Narrow, unsafe { (*rac.cell).wide() });
            assert!(unsafe { (*rac.row).wrap() });
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(1, 0) };
            assert_eq!(0u32, unsafe { (*rac.cell).content_codepoint() });
            assert_eq!(crate::page::Wide::SpacerHead, unsafe { (*rac.cell).wide() });
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(0, 1) };
            assert_eq!('😀' as u32, unsafe { (*rac.cell).content_codepoint() });
            assert_eq!(crate::page::Wide::Wide, unsafe { (*rac.cell).wide() });
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(1, 1) };
            assert_eq!(0u32, unsafe { (*rac.cell).content_codepoint() });
            assert_eq!(crate::page::Wide::SpacerTail, unsafe { (*rac.cell).wide() });
        }
    }

    s.deinit();
}

// ref: test "PageList resize reflow less cols to wrap a multi-codepoint grapheme with a spacer head"
#[test]
fn pagelist_resize_reflow_less_cols_to_wrap_a_multi_codepoint_grapheme_with_a_spacer_head() {
    let mut s = PageList::init(4, 2, Some(0)).unwrap();
    {
        assert!(s.pages.first == s.pages.last);
        let page = unsafe { super::Node::page(s.pages.first) };

        // We want to make the screen look like this:
        //
        // 👨‍👨‍👦‍👦👨‍👨‍👦‍👦

        // First family emoji at (0, 0)
        {
            let rac = unsafe { (*page).get_row_and_cell(0, 0) };
            unsafe {
                let mut c = cell_codepoint(0x1F468); // First codepoint of the grapheme
                c.set_wide(crate::page::Wide::Wide);
                *rac.cell = c;
            }
            unsafe {
                (*page)
                    .set_graphemes(
                        rac.row,
                        rac.cell,
                        &[0x200D, 0x1F468, 0x200D, 0x1F466, 0x200D, 0x1F466],
                    )
                    .unwrap();
            }
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(1, 0) };
            unsafe {
                let mut c = cell_codepoint(0);
                c.set_wide(crate::page::Wide::SpacerTail);
                *rac.cell = c;
            }
        }
        // Second family emoji at (2, 0)
        {
            let rac = unsafe { (*page).get_row_and_cell(2, 0) };
            unsafe {
                let mut c = cell_codepoint(0x1F468); // First codepoint of the grapheme
                c.set_wide(crate::page::Wide::Wide);
                *rac.cell = c;
            }
            unsafe {
                (*page)
                    .set_graphemes(
                        rac.row,
                        rac.cell,
                        &[0x200D, 0x1F468, 0x200D, 0x1F466, 0x200D, 0x1F466],
                    )
                    .unwrap();
            }
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(3, 0) };
            unsafe {
                let mut c = cell_codepoint(0);
                c.set_wide(crate::page::Wide::SpacerTail);
                *rac.cell = c;
            }
        }
    }

    // Resize
    s.resize(Resize {
        cols: Some(3),
        reflow: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(3usize, s.cols as usize);
    assert_eq!(2usize, s.total_rows());

    {
        assert!(s.pages.first == s.pages.last);
        let page = unsafe { super::Node::page(s.pages.first) };

        {
            let rac = unsafe { (*page).get_row_and_cell(0, 0) };
            assert_eq!(0x1F468u32, unsafe { (*rac.cell).content_codepoint() });
            assert_eq!(crate::page::Wide::Wide, unsafe { (*rac.cell).wide() });

            let cps = unsafe { (*page).lookup_grapheme(rac.cell).unwrap() };
            assert_eq!(6usize, cps.len());
            assert_eq!(0x200D, cps[0]);
            assert_eq!(0x1F468, cps[1]);
            assert_eq!(0x200D, cps[2]);
            assert_eq!(0x1F466, cps[3]);
            assert_eq!(0x200D, cps[4]);
            assert_eq!(0x1F466, cps[5]);

            // Row should be wrapped
            assert!(unsafe { (*rac.row).wrap() });
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(1, 0) };
            assert_eq!(0u32, unsafe { (*rac.cell).content_codepoint() });
            assert_eq!(crate::page::Wide::SpacerTail, unsafe { (*rac.cell).wide() });
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(2, 0) };
            assert_eq!(0u32, unsafe { (*rac.cell).content_codepoint() });
            assert_eq!(crate::page::Wide::SpacerHead, unsafe { (*rac.cell).wide() });
        }

        {
            let rac = unsafe { (*page).get_row_and_cell(0, 0) };
            assert_eq!(0x1F468u32, unsafe { (*rac.cell).content_codepoint() });
            assert_eq!(crate::page::Wide::Wide, unsafe { (*rac.cell).wide() });

            let cps = unsafe { (*page).lookup_grapheme(rac.cell).unwrap() };
            assert_eq!(6usize, cps.len());
            assert_eq!(0x200D, cps[0]);
            assert_eq!(0x1F468, cps[1]);
            assert_eq!(0x200D, cps[2]);
            assert_eq!(0x1F466, cps[3]);
            assert_eq!(0x200D, cps[4]);
            assert_eq!(0x1F466, cps[5]);
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(1, 1) };
            assert_eq!(0u32, unsafe { (*rac.cell).content_codepoint() });
            assert_eq!(crate::page::Wide::SpacerTail, unsafe { (*rac.cell).wide() });
        }
    }

    s.deinit();
}

// ref: test "PageList resize reflow less cols copy kitty placeholder"
// PORT NOTE: reference gates on `comptime !build_options.kitty_graphics`
// (SkipZigTest). Rustty's `KITTY_GRAPHICS` const is always `true`, so the
// test always runs. `kitty.graphics.unicode.placeholder` is U+10EEEE.
#[test]
fn pagelist_resize_reflow_less_cols_copy_kitty_placeholder() {
    const KITTY_PLACEHOLDER: u32 = 0x10EEEE;

    let mut s = PageList::init(4, 2, Some(0)).unwrap();
    {
        assert!(s.pages.first == s.pages.last);
        let page = unsafe { super::Node::page(s.pages.first) };

        // Write unicode placeholders
        for x in 0..s.cols as usize - 1 {
            let rac = unsafe { (*page).get_row_and_cell(x, 0) };
            unsafe {
                (*rac.row).set_kitty_virtual_placeholder(true);
                *rac.cell = cell_codepoint(KITTY_PLACEHOLDER);
            }
        }
    }

    // Resize
    s.resize(Resize {
        cols: Some(2),
        reflow: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(2usize, s.cols as usize);
    assert_eq!(2usize, s.total_rows());

    let mut it = s.row_iterator(
        super::iter::Direction::RightDown,
        point::Point::Active(point::Coordinate::default()),
        None,
    );
    while let Some(offset) = it.next() {
        for x in 0..s.cols as usize - 1 {
            let mut offset_copy = offset.pin_copy();
            offset_copy.x = x as u16;
            let rac = offset_copy.row_and_cell();

            assert!(unsafe { (*rac.row).kitty_virtual_placeholder() });
        }
    }

    s.deinit();
}

// ref: test "PageList resize reflow more cols clears kitty placeholder"
// PORT NOTE: see kitty placeholder note above; `KITTY_GRAPHICS` is always true.
#[test]
fn pagelist_resize_reflow_more_cols_clears_kitty_placeholder() {
    const KITTY_PLACEHOLDER: u32 = 0x10EEEE;

    let mut s = PageList::init(4, 2, Some(0)).unwrap();
    {
        assert!(s.pages.first == s.pages.last);
        let page = unsafe { super::Node::page(s.pages.first) };

        // Write unicode placeholders
        for x in 0..s.cols as usize - 1 {
            let rac = unsafe { (*page).get_row_and_cell(x, 0) };
            unsafe {
                (*rac.row).set_kitty_virtual_placeholder(true);
                *rac.cell = cell_codepoint(KITTY_PLACEHOLDER);
            }
        }
    }

    // Resize smaller then larger
    s.resize(Resize {
        cols: Some(2),
        reflow: true,
        ..Default::default()
    })
    .unwrap();
    s.resize(Resize {
        cols: Some(4),
        reflow: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(4usize, s.cols as usize);
    assert_eq!(2usize, s.total_rows());

    let mut it = s.row_iterator(
        super::iter::Direction::RightDown,
        point::Point::Active(point::Coordinate::default()),
        None,
    );
    {
        let row = it.next().unwrap();
        let rac = row.row_and_cell();
        assert!(unsafe { (*rac.row).kitty_virtual_placeholder() });
    }
    {
        let row = it.next().unwrap();
        let rac = row.row_and_cell();
        assert!(!unsafe { (*rac.row).kitty_virtual_placeholder() });
    }
    assert!(it.next().is_none());

    s.deinit();
}

// ref: test "PageList resize reflow wrap moves kitty placeholder"
// PORT NOTE: see kitty placeholder note above; `KITTY_GRAPHICS` is always true.
#[test]
fn pagelist_resize_reflow_wrap_moves_kitty_placeholder() {
    const KITTY_PLACEHOLDER: u32 = 0x10EEEE;

    let mut s = PageList::init(4, 2, Some(0)).unwrap();
    {
        assert!(s.pages.first == s.pages.last);
        let page = unsafe { super::Node::page(s.pages.first) };

        // Write unicode placeholders
        for x in 2..s.cols as usize - 1 {
            let rac = unsafe { (*page).get_row_and_cell(x, 0) };
            unsafe {
                (*rac.row).set_kitty_virtual_placeholder(true);
                *rac.cell = cell_codepoint(KITTY_PLACEHOLDER);
            }
        }
    }

    s.resize(Resize {
        cols: Some(2),
        reflow: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(2usize, s.cols as usize);
    assert_eq!(2usize, s.total_rows());

    let mut it = s.row_iterator(
        super::iter::Direction::RightDown,
        point::Point::Active(point::Coordinate::default()),
        None,
    );
    {
        let row = it.next().unwrap();
        let rac = row.row_and_cell();
        assert!(!unsafe { (*rac.row).kitty_virtual_placeholder() });
    }
    {
        let row = it.next().unwrap();
        let rac = row.row_and_cell();
        assert!(unsafe { (*rac.row).kitty_virtual_placeholder() });
    }
    assert!(it.next().is_none());

    s.deinit();
}

// ref: test "PageList resize reflow grapheme map capacity exceeded"
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION (vt multi-page-PageList intractable, .port/phase-2/MIRI_EXCEPTIONS.md): builds/reflows multi-page PageLists that exceed Miri's interpreter budget (>20min); covered natively + by ASan (all PageList tests ~18s)"
)]
#[test]
fn pagelist_resize_reflow_grapheme_map_capacity_exceeded() {
    // This test verifies that when reflowing content with many graphemes,
    // the grapheme map capacity is correctly increased when needed.
    let mut s = PageList::init(4, 10, Some(0)).unwrap();
    assert_eq!(1usize, s.total_pages());

    // Get the grapheme capacity from the page. We need more than this many
    // graphemes in a single destination page to trigger capacity increase
    // during reflow. Since each source page can only hold this many graphemes,
    // we create two source pages with graphemes that will merge into one
    // destination page.
    let grapheme_capacity = unsafe { (*super::Node::page(s.pages.first)).grapheme_capacity() };
    // Use slightly more than half the capacity per page, so combined they
    // exceed the capacity of a single destination page.
    let graphemes_per_page = grapheme_capacity / 2 + grapheme_capacity / 4;

    // Grow to the capacity of the first page and add more rows
    // so that we have two pages total.
    {
        let cap_rows = unsafe { super::Node::capacity(s.pages.first).rows };
        let size_rows = unsafe { super::Node::rows(s.pages.first) };
        unsafe { (*super::Node::page(s.pages.first)).pause_integrity_checks(true) };
        for _ in size_rows..cap_rows {
            let _ = s.grow().unwrap();
        }
        unsafe { (*super::Node::page(s.pages.first)).pause_integrity_checks(false) };
        assert_eq!(1usize, s.total_pages());
        s.grow_rows(graphemes_per_page).unwrap();
        assert_eq!(2usize, s.total_pages());

        // We now have two pages.
        assert!(s.pages.first != s.pages.last);
        assert!(core::ptr::eq(s.pages.last, unsafe {
            (*s.pages.first).next
        }));
    }

    // Add graphemes to both pages. We add graphemes to rows at the END of the
    // first page, and graphemes to rows at the START of the second page.
    // When reflowing to 2 columns, these rows will wrap and stay together
    // on the same destination page, requiring capacity increase.

    // Add graphemes to the end of the first page (last rows)
    {
        let page = unsafe { super::Node::page(s.pages.first) };
        let start_row = unsafe { (*page).size.rows as usize - graphemes_per_page };
        for i in 0..graphemes_per_page {
            let y = start_row + i;
            let rac = unsafe { (*page).get_row_and_cell(0, y) };
            unsafe {
                *rac.cell = cell_codepoint('A' as u32);
            }
            unsafe {
                (*page).append_grapheme(rac.row, rac.cell, 0x0301).unwrap();
            }
        }
    }

    // Add graphemes to the beginning of the second page
    {
        let page = unsafe { super::Node::page(s.pages.last) };
        let count = unsafe { core::cmp::min(graphemes_per_page, (*page).size.rows as usize) };
        for y in 0..count {
            let rac = unsafe { (*page).get_row_and_cell(0, y) };
            unsafe {
                *rac.cell = cell_codepoint('B' as u32);
            }
            unsafe {
                (*page).append_grapheme(rac.row, rac.cell, 0x0302).unwrap();
            }
        }
    }

    // Resize to fewer columns to trigger reflow.
    // The graphemes from both pages will be copied to destination pages.
    // They will all end up in a contiguous region of the destination.
    // If the bug exists (hyperlink_bytes increased instead of grapheme_bytes),
    // this will fail with GraphemeMapOutOfMemory when we exceed capacity.
    s.resize(Resize {
        cols: Some(2),
        reflow: true,
        ..Default::default()
    })
    .unwrap();

    // Verify the resize succeeded
    assert_eq!(2usize, s.cols as usize);

    s.deinit();
}

// ref: test "PageList resize grow cols with unwrap fixes viewport pin"
#[test]
fn pagelist_resize_grow_cols_with_unwrap_fixes_viewport_pin() {
    // Regression test: after resize/reflow, the viewport pin can end up at a
    // position where pin.y + rows > total_rows, causing getBottomRight to panic.

    // The plan is to pin viewport in history, then grow columns to unwrap rows.
    // The unwrap reduces total_rows, but the tracked pin moves to a position
    // that no longer has enough rows below it for the viewport height.
    let mut s = PageList::init(2, 10, None).unwrap();

    // Make sure we have some history, in this case we have 30 rows of history
    s.grow_rows(30).unwrap();
    assert_eq!(40usize, s.total_rows());

    // Fill all rows with wrapped content (pairs that unwrap when cols increase)
    let mut it = s.page_iterator(
        super::iter::Direction::RightDown,
        point::Point::Screen(point::Coordinate::default()),
        None,
    );
    while let Some(chunk) = it.next() {
        let page = unsafe { super::Node::page(chunk.node) };
        for y in chunk.start as usize..chunk.end as usize {
            let rac = unsafe { (*page).get_row_and_cell(0, y) };
            if y % 2 == 0 {
                unsafe { (*rac.row).set_wrap(true) };
            } else {
                unsafe { (*rac.row).set_wrap_continuation(true) };
            }
            for x in 0..s.cols as usize {
                let rac = unsafe { (*page).get_row_and_cell(x, y) };
                unsafe {
                    *rac.cell = cell_codepoint('A' as u32);
                }
            }
        }
    }

    // Pin viewport at row 28 (in history, 2 rows before active area at row 30).
    // After unwrap: row 28 -> row 14, total_rows 40 -> 20, active starts at 10.
    // Pin at 14 needs rows 14-23, but only 0-19 exist -> overflow.
    s.scroll(Scroll::Pin(
        s.pin(point::Point::Screen(point::Coordinate { x: 0, y: 28 }))
            .unwrap(),
    ));
    assert!(s.viewport == super::Viewport::Pin);
    assert!(s.get_bottom_right(point::Tag::Viewport).is_some());

    // Resize with reflow: unwraps rows, reducing total_rows
    s.resize(Resize {
        cols: Some(4),
        reflow: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(4usize, s.cols as usize);
    assert!(s.total_rows() < 40);

    // Used to panic here, so test that we can get the bottom right.
    let br_after = s.get_bottom_right(point::Tag::Viewport);
    assert!(br_after.is_some());

    s.deinit();
}

// ref: test "PageList resize (no reflow) more cols remaps pins in backfill path"
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION (vt multi-page-PageList intractable, .port/phase-2/MIRI_EXCEPTIONS.md): builds/reflows multi-page PageLists that exceed Miri's interpreter budget (>20min); covered natively + by ASan (all PageList tests ~18s)"
)]
#[test]
fn pagelist_resize_no_reflow_more_cols_remaps_pins_in_backfill_path() {
    // Regression test: when resizeWithoutReflowGrowCols copies rows to a previous
    // page with spare capacity, tracked pins in those rows must be remapped.
    // Without the fix, pins become dangling pointers when the original page is destroyed.
    let cols: size::CellCountInt = 5;
    let cap = std_capacity()
        .adjust(crate::page::Adjustment { cols: Some(cols) })
        .unwrap();
    let mut s = PageList::init(cols, cap.rows, None).unwrap();

    // Grow until we have two pages.
    while s.pages.first == s.pages.last {
        let _ = s.grow().unwrap();
    }
    let first_page = s.pages.first;
    let second_page = s.pages.last;
    assert!(first_page != second_page);

    // Trim a history row so the first page has spare capacity.
    // This triggers the backfill path in resizeWithoutReflowGrowCols.
    s.erase_history(Some(point::Point::History(point::Coordinate {
        x: 0,
        y: 0,
    })));
    assert!(unsafe { super::Node::rows(first_page) < super::Node::capacity(first_page).rows });

    // Ensure the resize takes the slow path (new capacity > current capacity).
    let new_cols: size::CellCountInt = cols + 1;
    let adjusted = unsafe { super::Node::capacity(second_page) }
        .adjust(crate::page::Adjustment {
            cols: Some(new_cols),
        })
        .unwrap();
    assert!(unsafe { super::Node::capacity(second_page).cols } < adjusted.cols);

    // Track a pin in row 0 of the second page. This row will be copied
    // to the first page during backfill and the pin must be remapped.
    let tracked = s
        .track_pin(Pin {
            node: second_page,
            x: 0,
            y: 0,
            garbage: false,
        })
        .unwrap();

    // Write a marker character to the tracked cell so we can verify
    // the pin points to the correct cell after resize.
    let marker: u32 = 'X' as u32;
    {
        let rac = unsafe { (*tracked).row_and_cell() };
        unsafe {
            *rac.cell = cell_codepoint(marker);
        }
    }

    s.resize(Resize {
        cols: Some(new_cols),
        reflow: false,
        ..Default::default()
    })
    .unwrap();

    // Verify the pin points to a valid node still in the page list.
    let mut found = false;
    let mut it = s.pages.first;
    while !it.is_null() {
        if it == unsafe { (*tracked).node } {
            found = true;
            break;
        }
        it = unsafe { (*it).next };
    }
    assert!(found);
    assert!(unsafe { (*tracked).y < super::Node::rows((*tracked).node) });

    // Verify the pin still points to the cell with our marker content.
    let cell = unsafe { (*tracked).row_and_cell().cell };
    assert_eq!(ContentTag::Codepoint, unsafe { (*cell).content_tag() });
    assert_eq!(marker, unsafe { (*cell).content_codepoint() });

    s.untrack_pin(tracked);
    s.deinit();
}

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

use super::page_tracking::PageLeakScope;

/// Grow scrollback to several pages, scroll around, then deinit — assert every
/// page allocated during the churn was freed (no page leak).
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION (vt multi-page-PageList intractable, .port/phase-2/MIRI_EXCEPTIONS.md): builds/reflows multi-page PageLists that exceed Miri's interpreter budget (>20min); covered natively + by ASan (all PageList tests ~18s)"
)]
#[test]
fn pagelist_pagepool_no_leak_grow_and_scroll() {
    let scope = PageLeakScope::new();
    {
        let mut s = PageList::init(80, 24, None).unwrap();

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
    miri,
    ignore = "MIRI EXCEPTION (vt multi-page-PageList intractable, .port/phase-2/MIRI_EXCEPTIONS.md): builds/reflows multi-page PageLists that exceed Miri's interpreter budget (>20min); covered natively + by ASan (all PageList tests ~18s)"
)]
#[test]
fn pagelist_pagepool_no_leak_clone_many_pages() {
    let scope = PageLeakScope::new();
    {
        let mut s = PageList::init(80, 24, None).unwrap();
        // Grow a large scrollback so the clone copies many pages.
        s.grow_rows(500).unwrap();

        assert!(
            scope.delta_allocs() > 0,
            "expected grow_rows to allocate scrollback pages"
        );

        let mut opts = super::access::Clone {
            top: point::Point::Screen(point::Coordinate { x: 0, y: 0 }),
            bot: None,
            tracked_pins: None,
        };
        let mut s2 = s.clone(&mut opts).unwrap();
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
    miri,
    ignore = "MIRI EXCEPTION (vt multi-page-PageList intractable, .port/phase-2/MIRI_EXCEPTIONS.md): builds/reflows multi-page PageLists that exceed Miri's interpreter budget (>20min); covered natively + by ASan (all PageList tests ~18s)"
)]
#[test]
fn pagelist_pagepool_no_leak_resize_reflow() {
    let scope = PageLeakScope::new();
    {
        let mut s = PageList::init(80, 24, None).unwrap();
        s.grow_rows(300).unwrap();

        // Fill rows with content so reflow actually has to move/repack cells
        // (and so column shrink can't just trim trailing blanks).
        {
            let mut it = s.row_iterator(
                super::iter::Direction::RightDown,
                point::Point::Active(point::Coordinate::default()),
                None,
            );
            while let Some(p) = it.next() {
                let rac = p.row_and_cell();
                let cells = unsafe { (*super::Node::page(p.node)).get_cells(rac.row) };
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
        s.resize(super::resize::Resize {
            cols: Some(40),
            rows: None,
            reflow: true,
            cursor: None,
        })
        .unwrap();
        assert_eq!(40, s.cols);

        // Grow columns back with reflow (heavy: unpacks rows again).
        s.resize(super::resize::Resize {
            cols: Some(100),
            rows: None,
            reflow: true,
            cursor: None,
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
    miri,
    ignore = "MIRI EXCEPTION (vt multi-page-PageList intractable, .port/phase-2/MIRI_EXCEPTIONS.md): builds/reflows multi-page PageLists that exceed Miri's interpreter budget (>20min); covered natively + by ASan (all PageList tests ~18s)"
)]
#[test]
fn pagelist_pagepool_no_leak_reset_then_deinit() {
    let scope = PageLeakScope::new();
    {
        let mut s = PageList::init(80, 24, None).unwrap();
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
