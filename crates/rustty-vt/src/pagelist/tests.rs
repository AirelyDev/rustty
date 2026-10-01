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

use super::grow::{IncreaseCapacity, Scrollbar};
use super::scroll::Scroll;
use super::{std_capacity, std_size, NodeData, Options, PageList, Pin};
use crate::alloc::Global;
use crate::page::{Cell, CellRgb, ContentTag, Page, SemanticContent};
use crate::point;
use rustty_render_types::term_size as size;

// Topic children, in the reference `test {}` order.
mod builder;
mod capacity_reset;
mod compress;
mod iter_clone;
mod pool_internals;
mod pool_leak;
mod resize_noreflow_a;
mod resize_noreflow_b;
mod resize_reflow;
mod split_erase_scroll;

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

/// Grow a test PageList until it contains at least `count` complete history
/// pages. The production cold-page boundary is intentionally reused here so
/// tests do not duplicate the row-to-page arithmetic.
///
/// PORTS: oracle@main PageList.zig `growColdPagesForTest` (9156ada16).
fn grow_cold_pages_for_test(s: &mut PageList, count: usize) {
    loop {
        let active_node = s.get_top_left(point::Tag::Active).node;
        let mut cold_count: usize = 0;
        let mut current = s.pages.first;
        while !current.is_null() {
            if current == active_node {
                break;
            }
            cold_count += 1;
            current = unsafe { (*current).next };
        }
        if cold_count >= count {
            return;
        }
        let _ = s.grow().unwrap();
    }
}

/// Read a compressed node's encoded bytes without restoring it.
///
/// PORT NOTE: the reference spells this `node.data.compressed.encoded`. rustty's
/// `NodeData` payload is reached through the same raw `compressed_payload`
/// projection the module itself uses, so the read forms no `&NodeData`.
///
/// # Panics
/// Panics if the node is resident.
fn compressed_encoded_for_test(node: *const super::Node) -> Vec<u8> {
    unsafe {
        let data = core::ptr::addr_of!((*node).data);
        assert_eq!(NodeData::tag(data), NodeData::TAG_COMPRESSED);
        (*NodeData::compressed_payload(data)).encoded.to_vec()
    }
}

// ref: test "PageList"
#[test]
fn pagelist() {
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();

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
// WS18 release lane (CORRECTED — fix round 1 of parity S166-F2): the sentence
// that used to stand here, "`tripwire::Module::enabled()` is
// `cfg!(debug_assertions)`", is FALSE and had this test `#[ignore]`d in
// release for a reason that did not exist. `enabled()` is
// `cfg!(any(test, debug_assertions, feature = "tripwire-test"))`, and
// rustty-vt turns `rustty-util/tripwire-test` on as a dev-dependency feature
// precisely so this crate's fail points trip in the RELEASE test lane too —
// which is what Zig's `builtin.is_test` does at every optimize mode. Verified:
// this test RUNS and PASSES under `cargo test -p rustty-vt --release`.
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
        assert!(matches!(
            PageList::init(Global, Options::new(80, 24)),
            Err(OutOfMemory)
        ));
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
        assert!(matches!(
            PageList::init(Global, Options::new(cols, 24)),
            Err(OutOfMemory)
        ));
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
                Global,
                Options::new(
                    std_capacity().max_cols().unwrap() + 1,
                    std_capacity().rows + 1
                )
            ),
            Err(OutOfMemory)
        ));
        INIT_PAGES_TW.with(|tw| tw.borrow_mut().end(ResetMode::Reset).unwrap());
    }
}

// ref: test "PageList pointFromPin active no history"
#[test]
fn pagelist_pointFromPin_active_no_history() {
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();

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
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();
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
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();
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
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();

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
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(80, 24)
        },
    )
    .unwrap();

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
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();
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
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();
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
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();
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
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();
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
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();

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
    let mut s = PageList::init(Global, Options::new(cap.cols, rows)).unwrap();
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
    let mut s = PageList::init(
        Global,
        Options::new(std_capacity().max_cols().unwrap() + 1, 80),
    )
    .unwrap();

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
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();
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
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(5, cap.rows)
        },
    )
    .unwrap();
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
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
)]
#[test]
fn pagelist_grow_prune_required_with_a_single_page() {
    // Need scrollback > 0 to have a scrollbar to test
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();

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
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(80, 24)
        },
    )
    .unwrap();

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
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(80, 24)
        },
    )
    .unwrap();

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
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();

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
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
)]
#[test]
fn pagelist_grow_allocate() {
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();

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
