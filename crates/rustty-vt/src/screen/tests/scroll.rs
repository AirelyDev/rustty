//! PORTS: ghostty src/terminal/Screen.zig — the scrolling, `cursorScrollRegionUp`, `scrollClear` and `scroll above` tests.

use super::*;

// PORT NOTE (f1948d505): the `test "selectionString map allocation failure
// cleanup"` that lived here was DELETED upstream with the tripwire it armed —
// `selectionStringMap` now transfers the formatter output into the returned
// map, so the duplicate string the test guarded against no longer exists.

// PORT NOTE: these tests were DEFERRED from vt-11 (recorded in
// DEFERRALS.md) because their fixtures needed Selection / the dirty
// tracking / page-churn machinery that landed across later sub-batches.
// They are ported here (vt-12c-screen) assertion-for-assertion.

// test "Screen: scrolling"
#[test]
fn screen_scrolling() {
    let mut s = mkscreen(10, 3, 0);
    s.set_attribute(crate::sgr::Attribute::DirectColorBg(crate::color::RGB {
        r: 155,
        g: 0,
        b: 0,
    }))
    .unwrap();
    s.test_write_string(b"1ABCD\n2EFGH\n3IJKL").unwrap();

    // Scroll down, should still be bottom
    s.cursor_down_scroll().unwrap();
    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"2EFGH\n3IJKL");
    }
    {
        let list_cell = s
            .pages
            .get_cell(point::Point::Active(point::Coordinate { x: 0, y: 2 }))
            .unwrap();
        let cell = list_cell.cell;
        assert!(unsafe { (*cell).content_tag() } == pagepkg::ContentTag::BgColorRgb);
        assert_eq!(pagepkg::CellRgb { r: 155, g: 0, b: 0 }, unsafe {
            (*cell).content_color_rgb()
        });
    }

    // Everything is dirty because we have no scrollback
    assert!(s
        .pages
        .is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 0 })));
    assert!(s
        .pages
        .is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 1 })));
    assert!(s
        .pages
        .is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 2 })));

    // Scrolling to the bottom does nothing
    s.scroll(Scroll::Active);

    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"2EFGH\n3IJKL");
    }
    s.deinit();
}

// test "Screen: scrolling with a single-row screen no scrollback"
#[test]
fn screen_scrolling_with_a_single_row_screen_no_scrollback() {
    let mut s = mkscreen(10, 1, 0);
    s.test_write_string(b"1ABCD").unwrap();

    // Scroll down, should still be bottom
    s.cursor_down_scroll().unwrap();
    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"");
    }

    // Screen should be dirty
    assert!(s
        .pages
        .is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 0 })));
    s.deinit();
}

// test "Screen: scrolling across pages preserves style"
#[test]
fn screen_scrolling_across_pages_preserves_style() {
    let mut s = mkscreen(10, 3, 1);
    s.set_attribute(crate::sgr::Attribute::Bold).unwrap();
    s.test_write_string(b"1ABCD\n2EFGH\n3IJKL").unwrap();
    let start_page: *mut Page = unsafe { crate::pagelist::Node::page(s.pages.pages.last) };

    // Scroll down enough to go to another page
    let rem = unsafe { (*start_page).capacity.rows - (*start_page).size.rows + 1 };
    unsafe { (*start_page).pause_integrity_checks(true) };
    for _ in 0..rem {
        s.cursor_down_or_scroll().unwrap();
    }
    unsafe { (*start_page).pause_integrity_checks(false) };

    // We need our page to change for this test o make sense. If this
    // assertion fails then the bug is in the test: we should be scrolling
    // above enough for a new page to show up.
    let page: *mut Page = unsafe { crate::pagelist::Node::page(s.pages.pages.last) };
    assert!(!std::ptr::eq(start_page, page));

    let styleval = unsafe {
        let base = (*page).memory_ptr();
        &*(*page).styles.get(base, s.cursor.style_id)
    };
    assert!(styleval.flags.bold);
    s.deinit();
}

// test "Screen: scrolling with a single-row screen with scrollback"
#[test]
fn screen_scrolling_with_a_single_row_screen_with_scrollback() {
    let mut s = mkscreen(10, 1, 1);
    s.test_write_string(b"1ABCD").unwrap();

    // Scroll down, should still be bottom
    s.cursor_down_scroll().unwrap();
    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"");
    }

    // Active should be dirty
    assert!(s
        .pages
        .is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 0 })));

    // Scrollback also dirty because cursor moved from there
    assert!(s
        .pages
        .is_dirty(point::Point::Screen(point::Coordinate { x: 0, y: 0 })));

    s.scroll(Scroll::DeltaRow(-1));
    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"1ABCD");
    }
    s.deinit();
}

// test "Screen: scroll down from 0"
#[test]
fn screen_scroll_down_from_0() {
    let mut s = mkscreen(10, 3, 0);
    s.test_write_string(b"1ABCD\n2EFGH\n3IJKL").unwrap();

    // Scrolling up does nothing, but allows it
    s.scroll(Scroll::DeltaRow(-1));
    assert!(s.pages.viewport == pagelist::Viewport::Active);

    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"1ABCD\n2EFGH\n3IJKL");
    }
    s.deinit();
}

// test "Screen: scrollback various cases"
#[test]
fn screen_scrollback_various_cases() {
    let mut s = mkscreen(10, 3, 1);
    s.test_write_string(b"1ABCD\n2EFGH\n3IJKL").unwrap();
    s.cursor_down_scroll().unwrap();

    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"2EFGH\n3IJKL");
    }

    // Scrolling to the bottom
    s.scroll(Scroll::Active);
    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"2EFGH\n3IJKL");
    }

    // Scrolling back should make it visible again
    s.scroll(Scroll::DeltaRow(-1));
    assert!(s.pages.viewport != pagelist::Viewport::Active);
    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"1ABCD\n2EFGH\n3IJKL");
    }

    // Scrolling back again should do nothing
    s.scroll(Scroll::DeltaRow(-1));
    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"1ABCD\n2EFGH\n3IJKL");
    }

    // Scrolling to the bottom
    s.scroll(Scroll::Active);
    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"2EFGH\n3IJKL");
    }

    // Scrolling forward with no grow should do nothing
    s.scroll(Scroll::DeltaRow(1));
    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"2EFGH\n3IJKL");
    }

    // Scrolling to the top should work
    s.scroll(Scroll::Top);
    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"1ABCD\n2EFGH\n3IJKL");
    }

    // Should be able to easily clear active area only
    s.clear_rows(act(), None, false);
    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"1ABCD");
    }

    // Scrolling to the bottom
    s.scroll(Scroll::Active);
    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"");
    }
    s.deinit();
}

// test "Screen: scrollback with multi-row delta"
#[test]
fn screen_scrollback_with_multi_row_delta() {
    let mut s = mkscreen(10, 3, 3);
    s.test_write_string(b"1ABCD\n2EFGH\n3IJKL\n4ABCD\n5EFGH\n6IJKL")
        .unwrap();

    // Scroll to top
    s.scroll(Scroll::Top);
    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"1ABCD\n2EFGH\n3IJKL");
    }

    // Scroll down multiple
    s.scroll(Scroll::DeltaRow(5));
    assert!(s.pages.viewport == pagelist::Viewport::Active);
    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"4ABCD\n5EFGH\n6IJKL");
    }
    s.deinit();
}

// test "Screen: scrollback empty"
#[test]
fn screen_scrollback_empty() {
    let mut s = mkscreen(10, 3, 50);
    s.test_write_string(b"1ABCD\n2EFGH\n3IJKL").unwrap();
    s.scroll(Scroll::DeltaRow(1));
    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"1ABCD\n2EFGH\n3IJKL");
    }
    s.deinit();
}

// test "Screen: scrollback doesn't move viewport if not at bottom"
#[test]
fn screen_scrollback_doesnt_move_viewport_if_not_at_bottom() {
    let mut s = mkscreen(10, 3, 3);
    s.test_write_string(b"1ABCD\n2EFGH\n3IJKL\n4ABCD\n5EFGH")
        .unwrap();

    // First test: we scroll up by 1, so we're not at the bottom anymore.
    s.scroll(Scroll::DeltaRow(-1));
    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"2EFGH\n3IJKL\n4ABCD");
    }

    // Next, we scroll back down by 1, this grows the scrollback but we
    // shouldn't move.
    s.cursor_down_scroll().unwrap();
    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"2EFGH\n3IJKL\n4ABCD");
    }

    // Scroll again, this clears scrollback so we should move viewports
    // but still see the same thing since our original view fits.
    s.cursor_down_scroll().unwrap();
    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"2EFGH\n3IJKL\n4ABCD");
    }
    s.deinit();
}

// test "Screen: scrolling when viewport is pruned"
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
)]
#[test]
fn screen_scrolling_when_viewport_is_pruned() {
    let mut s = mkscreen(215, 3, 1);

    // Write some to create scrollback and move back into our scrollback.
    s.test_write_string(b"1ABCD\n2EFGH\n3IJKL\n").unwrap();
    s.test_write_string(b"1ABCD\n2EFGH\n3IJKL").unwrap();
    s.scroll(Scroll::DeltaRow(-2));

    // Our viewport is now somewhere pinned. Create so much scrollback
    // that we prune it.
    s.test_write_string(b"\n").unwrap();
    for _ in 0..1000 {
        s.test_write_string(b"1ABCD\n2EFGH\n3IJKL\n").unwrap();
    }
    s.test_write_string(b"1ABCD\n2EFGH\n3IJKL").unwrap();

    {
        assert_eq!(
            point::Point::Screen(point::Coordinate { x: 0, y: 0 }),
            s.pages
                .point_from_pin(
                    point::Tag::Screen,
                    s.pages.get_top_left(point::Tag::Viewport)
                )
                .unwrap()
        );
    }
    s.deinit();
}

// test "Screen: scrolling moves selection"
#[test]
fn screen_scrolling_moves_selection() {
    let mut s = mkscreen(5, 3, 1);
    s.test_write_string(b"1ABCD\n2EFGH\n3IJKL").unwrap();

    // Select a single line
    s.select(Some(Selection::init(
        s.pages
            .pin(point::Point::Active(point::Coordinate { x: 0, y: 1 }))
            .unwrap(),
        s.pages
            .pin(point::Point::Active(point::Coordinate {
                x: s.pages.cols - 1,
                y: 1,
            }))
            .unwrap(),
        false,
    )))
    .unwrap();

    // Scroll down, should still be bottom
    s.cursor_down_scroll().unwrap();

    // Our selection should've moved up
    {
        let sel = s.selection.unwrap();
        assert_eq!(
            point::Point::Active(point::Coordinate { x: 0, y: 0 }),
            s.pages
                .point_from_pin(point::Tag::Active, sel.start())
                .unwrap()
        );
        assert_eq!(
            point::Point::Active(point::Coordinate {
                x: s.pages.cols - 1,
                y: 0,
            }),
            s.pages
                .point_from_pin(point::Tag::Active, sel.end())
                .unwrap()
        );
    }

    {
        // Test our contents rotated
        let contents = dump(&s, vp());
        assert_eq!(contents, b"2EFGH\n3IJKL");
    }

    // Scrolling to the bottom does nothing
    s.scroll(Scroll::Active);

    // Our selection should've stayed the same
    {
        let sel = s.selection.unwrap();
        assert_eq!(
            point::Point::Active(point::Coordinate { x: 0, y: 0 }),
            s.pages
                .point_from_pin(point::Tag::Active, sel.start())
                .unwrap()
        );
        assert_eq!(
            point::Point::Active(point::Coordinate {
                x: s.pages.cols - 1,
                y: 0,
            }),
            s.pages
                .point_from_pin(point::Tag::Active, sel.end())
                .unwrap()
        );
    }

    {
        // Test our contents rotated
        let contents = dump(&s, vp());
        assert_eq!(contents, b"2EFGH\n3IJKL");
    }

    // Scroll up again
    s.cursor_down_scroll().unwrap();

    {
        // Test our contents rotated
        let contents = dump(&s, vp());
        assert_eq!(contents, b"3IJKL");
    }

    // Our selection should be null because it left the screen.
    {
        let sel = s.selection.unwrap();
        assert!(s
            .pages
            .point_from_pin(point::Tag::Active, sel.start())
            .is_none());
        assert!(s
            .pages
            .point_from_pin(point::Tag::Active, sel.end())
            .is_none());
    }
    s.deinit();
}

// -- oracle 77190bd02: Screen.cursorScrollRegionUp (`P3-W5 T3`) ----------

// test "Screen: cursorScrollRegionUp simple"
#[test]
fn screen_cursor_scroll_region_up_simple() {
    let mut s = mkscreen(5, 5, 0);
    s.test_write_string(b"1ABCD\n2EFGH\n3IJKL\n4MNOP\n5QRST")
        .unwrap();

    // Scroll a region ending at row 2 (zero-indexed) up by one. This
    // emulates a scroll region of rows 0-2 with the cursor at the
    // region bottom.
    s.cursor_absolute(1, 2);
    s.cursor_scroll_region_up(2).unwrap();

    // The cursor stays in place, on the new blank row.
    assert_eq!(1 as size::CellCountInt, s.cursor.x);
    assert_eq!(2 as size::CellCountInt, s.cursor.y);

    // Rows in the region scrolled, rows below are unchanged, and
    // nothing was moved into scrollback.
    {
        let contents = dump(&s, scr());
        assert_eq!(contents, b"2EFGH\n3IJKL\n\n4MNOP\n5QRST");
    }
    s.deinit();
}

// test "Screen: cursorScrollRegionUp renews page generation"
//
// PORTS: oracle 0bad91adb — the in-place region rotation renews the page
// generation so cached row coordinates cannot pass `node_is_valid`.
#[test]
fn screen_cursor_scroll_region_up_renews_page_generation() {
    let mut s = mkscreen(5, 5, 0);
    s.test_write_string(b"1ABCD\n2EFGH\n3IJKL\n4MNOP\n5QRST")
        .unwrap();
    s.cursor_absolute(0, 2);

    let node = unsafe { (*s.cursor.page_pin).node };
    let serial = unsafe { (*node).serial };
    s.cursor_scroll_region_up(2).unwrap();

    assert!(!s.pages.node_is_valid(node, serial));
    s.deinit();
}

// test "Screen: cursorScrollRegionUp moves selection"
#[test]
fn screen_cursor_scroll_region_up_moves_selection() {
    let mut s = mkscreen(5, 5, 0);
    s.test_write_string(b"1ABCD\n2EFGH\n3IJKL\n4MNOP\n5QRST")
        .unwrap();

    // Select the second row.
    s.select(Some(Selection::init(
        s.pages
            .pin(point::Point::Active(point::Coordinate { x: 0, y: 1 }))
            .unwrap(),
        s.pages
            .pin(point::Point::Active(point::Coordinate {
                x: s.pages.cols - 1,
                y: 1,
            }))
            .unwrap(),
        false,
    )))
    .unwrap();

    s.cursor_absolute(0, 2);
    s.cursor_scroll_region_up(2).unwrap();

    // Our selection should've moved up with its row.
    {
        let sel = s.selection.unwrap();
        assert_eq!(
            point::Point::Active(point::Coordinate { x: 0, y: 0 }),
            s.pages
                .point_from_pin(point::Tag::Active, sel.start())
                .unwrap()
        );
        assert_eq!(
            point::Point::Active(point::Coordinate {
                x: s.pages.cols - 1,
                y: 0,
            }),
            s.pages
                .point_from_pin(point::Tag::Active, sel.end())
                .unwrap()
        );
    }

    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"2EFGH\n3IJKL\n\n4MNOP\n5QRST");
    }
    s.deinit();
}

// test "Screen: cursorScrollRegionUp region spans pages"
#[test]
fn screen_cursor_scroll_region_up_region_spans_pages() {
    let mut s = mkscreen(10, 5, 10);

    // We need to get the cursor to a new page
    let first_page_size = unsafe { crate::pagelist::Node::capacity(s.pages.pages.first).rows };
    unsafe { (*crate::pagelist::Node::page(s.pages.pages.first)).pause_integrity_checks(true) };
    for _ in 0..(first_page_size - 3) {
        s.test_write_string(b"\n").unwrap();
    }
    unsafe { (*crate::pagelist::Node::page(s.pages.pages.first)).pause_integrity_checks(false) };
    s.test_write_string(b"1A\n2B\n3C\n4D\n5E").unwrap();

    // At this point:
    //      +----------+ = PAGE 0
    //  ... :          :
    //     +-------------+ ACTIVE
    // 4305 |1A00000000| | 0
    // 4306 |2B00000000| | 1
    // 4307 |3C00000000| | 2
    //      +----------+ :
    //      +----------+ : = PAGE 1
    //    0 |4D00000000| | 3
    //      :^         : : = PIN 0
    //    1 |5E00000000| | 4
    //      +----------+ :
    //     +-------------+

    // Move the cursor to the first row of the second page and give it
    // a non-default style. This is important: it verifies that the
    // cursor's style ref stays accounted on the correct page even
    // though eraseRowBounded moves the cursor's tracked pin across
    // the page boundary.
    s.cursor_absolute(0, 3);
    s.set_attribute(crate::sgr::Attribute::Bold).unwrap();
    assert!(std::ptr::eq(
        unsafe { (*s.cursor.page_pin).node },
        s.pages.pages.last
    ));
    assert_eq!(0 as size::CellCountInt, unsafe { (*s.cursor.page_pin).y });

    // Scroll a region of active rows 1-3 with the cursor at the region
    // bottom. The region spans the page boundary so this exercises the
    // slow path.
    s.cursor_scroll_region_up(2).unwrap();

    // The cursor stays in place, on the new blank row.
    assert_eq!(0 as size::CellCountInt, s.cursor.x);
    assert_eq!(3 as size::CellCountInt, s.cursor.y);

    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"1A\n3C\n4D\n\n5E");
    }

    // Our cursor style must remain usable: write a styled cell and
    // verify the style ref counting is intact on the cursor's page.
    s.test_write_string(b"X").unwrap();
    {
        let page = unsafe { crate::pagelist::Node::page((*s.cursor.page_pin).node) };
        assert_eq!(1usize, unsafe { (*page).styles.count() });
    }
    s.deinit();
}

// test "Screen: cursorScrollRegionUp region spans pages with background SGR"
#[test]
fn screen_cursor_scroll_region_up_region_spans_pages_with_background_sgr() {
    let mut s = mkscreen(10, 5, 10);

    // We need to get the cursor to a new page. See the previous test
    // for a diagram of the page layout.
    let first_page_size = unsafe { crate::pagelist::Node::capacity(s.pages.pages.first).rows };
    unsafe { (*crate::pagelist::Node::page(s.pages.pages.first)).pause_integrity_checks(true) };
    for _ in 0..(first_page_size - 3) {
        s.test_write_string(b"\n").unwrap();
    }
    unsafe { (*crate::pagelist::Node::page(s.pages.pages.first)).pause_integrity_checks(false) };
    s.test_write_string(b"1A\n2B\n3C\n4D\n5E").unwrap();

    s.cursor_absolute(0, 3);
    s.set_attribute(crate::sgr::Attribute::DirectColorBg(crate::color::RGB {
        r: 0xFF,
        g: 0,
        b: 0,
    }))
    .unwrap();
    assert!(std::ptr::eq(
        unsafe { (*s.cursor.page_pin).node },
        s.pages.pages.last
    ));
    assert_eq!(0 as size::CellCountInt, unsafe { (*s.cursor.page_pin).y });

    s.cursor_scroll_region_up(2).unwrap();

    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"1A\n3C\n4D\n\n5E");
    }

    // The new blank row must be filled with our background color.
    for x in 0..s.pages.cols {
        let list_cell = s
            .pages
            .get_cell(point::Point::Active(point::Coordinate { x, y: 3 }))
            .unwrap();
        let cell = list_cell.cell;
        assert!(unsafe { (*cell).content_tag() } == pagepkg::ContentTag::BgColorRgb);
        assert_eq!(
            pagepkg::CellRgb {
                r: 0xFF,
                g: 0,
                b: 0
            },
            unsafe { (*cell).content_color_rgb() }
        );
    }
    s.deinit();
}

// test "Screen: cursorScrollRegionUp with styled erased row"
#[test]
fn screen_cursor_scroll_region_up_with_styled_erased_row() {
    let mut s = mkscreen(5, 3, 0);

    // Write a styled row at the top so the erased row has managed
    // memory that must be released.
    s.set_attribute(crate::sgr::Attribute::Bold).unwrap();
    s.test_write_string(b"1ABCD").unwrap();
    s.set_attribute(crate::sgr::Attribute::Unset).unwrap();
    s.test_write_string(b"\n2EFGH\n3IJKL").unwrap();

    s.cursor_absolute(0, 2);
    s.cursor_scroll_region_up(2).unwrap();

    {
        let contents = dump(&s, scr());
        assert_eq!(contents, b"2EFGH\n3IJKL");
    }

    // The style should be gone from the page since the only user
    // was the erased row.
    let page = unsafe { crate::pagelist::Node::page((*s.cursor.page_pin).node) };
    assert_eq!(0usize, unsafe { (*page).styles.count() });
    s.deinit();
}

// NEGATIVE CONTROL (`P3-W5 T3 Step 4(a)`) — the fast in-page path and the
// `_slow` PageList path must be observationally identical.
//
// The oracle has NO such test: upstream only covers a handful of shapes by
// hand. The differential is what makes the fast path falsifiable — it is
// the ONLY guard that can see a one-cell/one-row error in the rotated
// region base or extent, because every hand-written case would still
// produce "something plausible".
//
// MUTATIONS EXECUTED AND OBSERVED TO FAIL (`P3-W5 T3`), each reverted:
//
//   1. `rows = ...ptr(...).add(pin_y - limit)` -> `.add(pin_y - limit + 1)`
//      (region base off by one row)
//   2. `from_raw_parts_mut(rows, limit + 1)` -> `(rows, limit)`
//      (region extent off by one row)
//   3. `page_cell = ...add(self.cursor.x)` -> `.add(self.cursor.x + 1)`
//      (cursor cell off by one)
//
// The grid is deliberately built so BOTH clear branches of the fast path
// are exercised: `bg` toggles the zero-fill hot path vs. `clear_cells`, and
// the `styled` arm puts managed memory (a style ref) in the erased row.
#[test]
fn screen_cursor_scroll_region_up_fast_matches_slow_differential() {
    // One screen, written identically for both arms.
    fn build(
        rows: size::CellCountInt,
        bg: bool,
        styled: bool,
        cursor_y: size::CellCountInt,
    ) -> Screen {
        let mut s = mkscreen(5, rows, 0);
        if styled {
            s.set_attribute(crate::sgr::Attribute::Bold).unwrap();
        }
        for y in 0..rows {
            if y > 0 {
                s.test_write_string(b"\n").unwrap();
            }
            let line = [b'A' + (y as u8), b'B', b'C', b'D', b'E'];
            s.test_write_string(&line).unwrap();
            if styled && y == 0 {
                s.set_attribute(crate::sgr::Attribute::Unset).unwrap();
            }
        }
        if bg {
            s.set_attribute(crate::sgr::Attribute::DirectColorBg(crate::color::RGB {
                r: 0x12,
                g: 0x34,
                b: 0x56,
            }))
            .unwrap();
        }
        s.cursor_absolute(2, cursor_y);
        s
    }

    // Every cell of the whole screen, as raw backing bits, plus the
    // cursor's own position AND its cached `page_row`/`page_cell`
    // pointers expressed as offsets from the page bases. Raw bits catch
    // style ids and content tags that a string dump cannot see.
    //
    // VACUITY NOTE (`P3-W5 T3`): the first version of this snapshot omitted
    // the two cached-pointer offsets, and mutation 3 (`page_cell` off by
    // one cell) PASSED it — the differential was vacuous for the one field
    // the fast path computes by hand rather than via `row_and_cell()`. The
    // offsets below are what make it bind; keep them.
    #[allow(clippy::type_complexity)]
    fn snapshot(
        s: &Screen,
    ) -> (
        Vec<u64>,
        size::CellCountInt,
        size::CellCountInt,
        usize,
        usize,
    ) {
        let mut out = Vec::new();
        for y in 0..s.pages.rows {
            for x in 0..s.pages.cols {
                let lc = s
                    .pages
                    .get_cell(point::Point::Active(point::Coordinate {
                        x,
                        y: u32::from(y),
                    }))
                    .unwrap();
                out.push(unsafe { *lc.cell }.to_bits());
            }
        }
        let page = unsafe { pagelist::Node::page((*s.cursor.page_pin).node) };
        let rows_base = unsafe { (*page).rows.ptr((*page).memory_ptr()) };
        let row_off =
            (s.cursor.page_row as usize - rows_base as usize) / core::mem::size_of::<Row>();
        let cells_base = unsafe { (*page).get_cells(s.cursor.page_row) } as *mut Cell;
        let cell_off =
            (s.cursor.page_cell as usize - cells_base as usize) / core::mem::size_of::<Cell>();
        (out, s.cursor.x, s.cursor.y, row_off, cell_off)
    }

    let mut cases = 0usize;
    for rows in [2 as size::CellCountInt, 3, 5, 8] {
        for cursor_y in 1..rows {
            for limit in 1..=cursor_y {
                for bg in [false, true] {
                    for styled in [false, true] {
                        let mut fast = build(rows, bg, styled, cursor_y);
                        let mut slow = build(rows, bg, styled, cursor_y);

                        // The fast arm goes through the public entry point.
                        // With a single page and `pin.y >= limit` this is
                        // always the in-page rotate.
                        assert!(
                            unsafe { (*fast.cursor.page_pin).y } as usize >= limit as usize,
                            "case is not on the fast path; the differential would be vacuous"
                        );
                        fast.cursor_scroll_region_up(limit as usize).unwrap();

                        // The slow arm calls the PageList path directly.
                        slow.cursor_scroll_region_up_slow(limit as usize).unwrap();
                        slow.assert_integrity();

                        assert_eq!(
                            snapshot(&fast),
                            snapshot(&slow),
                            "fast != slow for rows={rows} cursor_y={cursor_y} \
                             limit={limit} bg={bg} styled={styled}"
                        );
                        assert_eq!(
                            dump(&fast, scr()),
                            dump(&slow, scr()),
                            "dump differs for rows={rows} cursor_y={cursor_y} \
                             limit={limit} bg={bg} styled={styled}"
                        );

                        fast.deinit();
                        slow.deinit();
                        cases += 1;
                    }
                }
            }
        }
    }

    // Anti-vacuity floor: if the grid ever collapses to nothing the
    // assertions above would never run and the test would still pass.
    // (R-1)R/2 (cursor_y, limit) pairs per R, x 4 (bg x styled):
    // R=2 -> 1, R=3 -> 3, R=5 -> 10, R=8 -> 28; (1+3+10+28) * 4 = 168.
    assert_eq!(cases, 168, "grid size changed");
}

// test "Screen: scrolling moves viewport"
#[test]
fn screen_scrolling_moves_viewport() {
    let mut s = mkscreen(10, 3, 1);
    s.test_write_string(b"1ABCD\n2EFGH\n3IJKL\n").unwrap();
    s.test_write_string(b"1ABCD\n2EFGH\n3IJKL").unwrap();
    s.scroll(Scroll::DeltaRow(-2));

    {
        // Test our contents rotated
        let contents = dump(&s, vp());
        assert_eq!(contents, b"2EFGH\n3IJKL\n1ABCD");
    }

    {
        assert_eq!(
            point::Point::Screen(point::Coordinate { x: 0, y: 1 }),
            s.pages
                .point_from_pin(
                    point::Tag::Screen,
                    s.pages.get_top_left(point::Tag::Viewport)
                )
                .unwrap()
        );
    }
    s.deinit();
}

// test "Screen: scroll and clear full screen"
#[test]
fn screen_scroll_and_clear_full_screen() {
    let mut s = mkscreen(10, 3, 5);
    s.test_write_string(b"1ABCD\n2EFGH\n3IJKL").unwrap();

    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"1ABCD\n2EFGH\n3IJKL");
    }

    s.scroll_clear().unwrap();
    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"");
    }
    {
        let contents = dump(&s, scr());
        assert_eq!(contents, b"1ABCD\n2EFGH\n3IJKL");
    }
    s.deinit();
}

// test "Screen: scroll and clear partial screen"
#[test]
fn screen_scroll_and_clear_partial_screen() {
    let mut s = mkscreen(10, 3, 5);
    s.test_write_string(b"1ABCD\n2EFGH").unwrap();

    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"1ABCD\n2EFGH");
    }

    s.scroll_clear().unwrap();
    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"");
    }
    {
        let contents = dump(&s, scr());
        assert_eq!(contents, b"1ABCD\n2EFGH");
    }
    s.deinit();
}

// test "Screen: scroll and clear empty screen"
#[test]
fn screen_scroll_and_clear_empty_screen() {
    let mut s = mkscreen(10, 3, 5);
    s.scroll_clear().unwrap();
    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"");
    }
    {
        let contents = dump(&s, scr());
        assert_eq!(contents, b"");
    }
    s.deinit();
}

// test "Screen: scroll and clear ignore blank lines"
#[test]
fn screen_scroll_and_clear_ignore_blank_lines() {
    let mut s = mkscreen(10, 3, 10);
    s.test_write_string(b"1ABCD\n2EFGH").unwrap();
    s.scroll_clear().unwrap();
    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"");
    }

    // Move back to top-left
    s.cursor_absolute(0, 0);

    // Write and clear
    s.test_write_string(b"3ABCD\n").unwrap();
    {
        let contents = dump(&s, act());
        assert_eq!(contents, b"3ABCD");
    }

    s.scroll_clear().unwrap();
    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"");
    }

    // Move back to top-left
    s.cursor_absolute(0, 0);
    s.test_write_string(b"X").unwrap();

    {
        let contents = dump(&s, scr());
        assert_eq!(contents, b"1ABCD\n2EFGH\n3ABCD\nX");
    }
    s.deinit();
}

// test "Screen: scroll above same page"
#[test]
fn screen_scroll_above_same_page() {
    let mut s = mkscreen(10, 3, 10);
    s.set_attribute(crate::sgr::Attribute::DirectColorBg(crate::color::RGB {
        r: 155,
        g: 0,
        b: 0,
    }))
    .unwrap();
    s.test_write_string(b"1ABCD\n2EFGH\n3IJKL").unwrap();
    s.cursor_absolute(0, 1);
    s.pages.clear_dirty();

    // At this point:
    //  +-------------+ ACTIVE
    //   +----------+ : = PAGE 0
    // 0 |1ABCD00000| | 0
    // 1 |2EFGH00000| | 1
    //   :^         : : = PIN 0
    // 2 |3IJKL00000| | 2
    //   +----------+ :
    //  +-------------+

    // PORTS: oracle 0bad91adb — the in-place row rotation renews this
    // node's generation, so a cached (node, serial) pair is rejected.
    let node = unsafe { (*s.cursor.page_pin).node };
    let serial = unsafe { (*node).serial };
    s.cursor_scroll_above().unwrap();
    assert!(!s.pages.node_is_valid(node, serial));

    //   +----------+ = PAGE 0
    // 0 |1ABCD00000|
    //  +-------------+ ACTIVE
    // 1 |2EFGH00000| | 0
    // 2 |          | | 1
    //   :^         : : = PIN 0
    // 3 |3IJKL00000| | 2
    //   +----------+ :
    //  +-------------+

    // try s.pages.diagram(std.io.getStdErr().writer());

    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"2EFGH\n\n3IJKL");
    }
    {
        let list_cell = s
            .pages
            .get_cell(point::Point::Active(point::Coordinate { x: 0, y: 1 }))
            .unwrap();
        let cell = list_cell.cell;
        assert!(unsafe { (*cell).content_tag() } == pagepkg::ContentTag::BgColorRgb);
        assert_eq!(pagepkg::CellRgb { r: 155, g: 0, b: 0 }, unsafe {
            (*cell).content_color_rgb()
        });
    }

    // Page 0 row 1 (active row 0) is dirty because the cursor moved off of it.
    assert!(s
        .pages
        .is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 0 })));
    // Page 0 row 2 (active row 1) is dirty because it was cleared.
    assert!(s
        .pages
        .is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 1 })));
    // Page 0 row 3 (active row 2) is dirty because it's new.
    assert!(s
        .pages
        .is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 2 })));
    s.deinit();
}

// test "Screen: scroll above same page but cursor on previous page"
#[test]
fn screen_scroll_above_same_page_but_cursor_on_previous_page() {
    let mut s = mkscreen(10, 5, 10);

    // We need to get the cursor to a new page
    let first_page_size = unsafe { crate::pagelist::Node::capacity(s.pages.pages.first).rows };
    unsafe { (*crate::pagelist::Node::page(s.pages.pages.first)).pause_integrity_checks(true) };
    for _ in 0..(first_page_size - 3) {
        s.test_write_string(b"\n").unwrap();
    }
    unsafe { (*crate::pagelist::Node::page(s.pages.pages.first)).pause_integrity_checks(false) };

    s.set_attribute(crate::sgr::Attribute::DirectColorBg(crate::color::RGB {
        r: 155,
        g: 0,
        b: 0,
    }))
    .unwrap();
    s.test_write_string(b"1A\n2B\n3C\n4D\n5E").unwrap();
    s.cursor_absolute(0, 1);
    s.pages.clear_dirty();

    // Ensure we're still on the first page and have a second
    assert!(std::ptr::eq(
        unsafe { (*s.cursor.page_pin).node },
        s.pages.pages.first
    ));
    assert!(!unsafe { (*s.pages.pages.first).next }.is_null());

    // At this point:
    //      +----------+ = PAGE 0
    //  ... :          :
    //     +-------------+ ACTIVE
    // 4305 |1A00000000| | 0
    // 4306 |2B00000000| | 1
    //      :^         : : = PIN 0
    // 4307 |3C00000000| | 2
    //      +----------+ :
    //      +----------+ : = PAGE 1
    //    0 |4D00000000| | 3
    //    1 |5E00000000| | 4
    //      +----------+ :
    //     +-------------+

    // PORTS: oracle 0bad91adb — both pages are rotated in place, so both
    // generations are renewed.
    let first_node = s.pages.pages.first;
    let second_node = unsafe { (*first_node).next };
    let first_serial = unsafe { (*first_node).serial };
    let second_serial = unsafe { (*second_node).serial };
    s.cursor_scroll_above().unwrap();
    assert!(!s.pages.node_is_valid(first_node, first_serial));
    assert!(!s.pages.node_is_valid(second_node, second_serial));

    //      +----------+ = PAGE 0
    //  ... :          :
    // 4305 |1A00000000|
    //     +-------------+ ACTIVE
    // 4306 |2B00000000| | 0
    // 4307 |          | | 1
    //      :^         : : = PIN 0
    //      +----------+ :
    //      +----------+ : = PAGE 1
    //    0 |3C00000000| | 2
    //    1 |4D00000000| | 3
    //    2 |5E00000000| | 4
    //      +----------+ :
    //     +-------------+

    // try s.pages.diagram(std.io.getStdErr().writer());

    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"2B\n\n3C\n4D\n5E");
    }
    {
        let list_cell = s
            .pages
            .get_cell(point::Point::Active(point::Coordinate { x: 0, y: 1 }))
            .unwrap();
        let cell = list_cell.cell;
        assert!(unsafe { (*cell).content_tag() } == pagepkg::ContentTag::BgColorRgb);
        assert_eq!(pagepkg::CellRgb { r: 155, g: 0, b: 0 }, unsafe {
            (*cell).content_color_rgb()
        });
    }

    // Page 0's penultimate row is dirty because the cursor moved off of it.
    assert!(s
        .pages
        .is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 0 })));
    // The rest of the rows are dirty because they've been modified or are new.
    assert!(s
        .pages
        .is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 1 })));
    assert!(s
        .pages
        .is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 2 })));
    assert!(s
        .pages
        .is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 3 })));
    assert!(s
        .pages
        .is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 4 })));
    s.deinit();
}

// test "Screen: scroll above same page but cursor on previous page last row"
#[test]
fn screen_scroll_above_same_page_but_cursor_on_previous_page_last_row() {
    let mut s = mkscreen(10, 5, 10);

    // We need to get the cursor to a new page
    let first_page_size = unsafe { crate::pagelist::Node::capacity(s.pages.pages.first).rows };
    unsafe { (*crate::pagelist::Node::page(s.pages.pages.first)).pause_integrity_checks(true) };
    for _ in 0..(first_page_size - 2) {
        s.test_write_string(b"\n").unwrap();
    }
    unsafe { (*crate::pagelist::Node::page(s.pages.pages.first)).pause_integrity_checks(false) };

    s.set_attribute(crate::sgr::Attribute::DirectColorBg(crate::color::RGB {
        r: 155,
        g: 0,
        b: 0,
    }))
    .unwrap();
    s.test_write_string(b"1A\n2B\n3C\n4D\n5E").unwrap();
    s.cursor_absolute(0, 1);
    s.pages.clear_dirty();

    // Ensure we're still on the first page and have a second
    assert!(std::ptr::eq(
        unsafe { (*s.cursor.page_pin).node },
        s.pages.pages.first
    ));
    assert!(!unsafe { (*s.pages.pages.first).next }.is_null());

    // At this point:
    //      +----------+ = PAGE 0
    //  ... :          :
    //     +-------------+ ACTIVE
    // 4306 |1A00000000| | 0
    // 4307 |2B00000000| | 1
    //      :^         : : = PIN 0
    //      +----------+ :
    //      +----------+ : = PAGE 1
    //    0 |3C00000000| | 2
    //    1 |4D00000000| | 3
    //    2 |5E00000000| | 4
    //      +----------+ :
    //     +-------------+

    s.cursor_scroll_above().unwrap();

    //      +----------+ = PAGE 0
    //  ... :          :
    // 4306 |1A00000000|
    //     +-------------+ ACTIVE
    // 4307 |2B00000000| | 0
    //      +----------+ :
    //      +----------+ : = PAGE 1
    //    0 |          | | 1
    //      :^         : : = PIN 0
    //    1 |3C00000000| | 2
    //    2 |4D00000000| | 3
    //    3 |5E00000000| | 4
    //      +----------+ :
    //     +-------------+

    // try s.pages.diagram(std.io.getStdErr().writer());

    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"2B\n\n3C\n4D\n5E");
    }
    {
        let list_cell = s
            .pages
            .get_cell(point::Point::Active(point::Coordinate { x: 0, y: 1 }))
            .unwrap();
        let cell = list_cell.cell;
        assert!(unsafe { (*cell).content_tag() } == pagepkg::ContentTag::BgColorRgb);
        assert_eq!(pagepkg::CellRgb { r: 155, g: 0, b: 0 }, unsafe {
            (*cell).content_color_rgb()
        });
    }

    // Page 0's final row is dirty because the cursor moved off of it.
    assert!(s
        .pages
        .is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 0 })));
    // Page 1's rows are all dirty because every row was moved.
    assert!(s
        .pages
        .is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 1 })));
    assert!(s
        .pages
        .is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 2 })));
    assert!(s
        .pages
        .is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 3 })));
    assert!(s
        .pages
        .is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 4 })));

    // Attempt to clear the style from the cursor and
    // then assert the integrity of both of our pages.
    //
    // This catches a case of memory corruption where the cursor
    // is moved between pages without accounting for style refs.
    s.set_attribute(crate::sgr::Attribute::ResetBg).unwrap();
    unsafe { (*crate::pagelist::Node::page(s.pages.pages.first)).assert_integrity() };
    unsafe { (*crate::pagelist::Node::page(s.pages.pages.last)).assert_integrity() };
    s.deinit();
}

// test "Screen: scroll above creates new page"
#[test]
fn screen_scroll_above_creates_new_page() {
    let mut s = mkscreen(10, 3, 10);

    // We need to get the cursor to a new page
    let first_page_size = unsafe { crate::pagelist::Node::capacity(s.pages.pages.first).rows };
    unsafe { (*crate::pagelist::Node::page(s.pages.pages.first)).pause_integrity_checks(true) };
    for _ in 0..(first_page_size - 3) {
        s.test_write_string(b"\n").unwrap();
    }
    unsafe { (*crate::pagelist::Node::page(s.pages.pages.first)).pause_integrity_checks(false) };

    s.set_attribute(crate::sgr::Attribute::DirectColorBg(crate::color::RGB {
        r: 155,
        g: 0,
        b: 0,
    }))
    .unwrap();
    s.test_write_string(b"1ABCD\n2EFGH\n3IJKL").unwrap();
    s.cursor_absolute(0, 1);
    s.pages.clear_dirty();

    // Ensure we're still on the first page
    assert!(std::ptr::eq(
        unsafe { (*s.cursor.page_pin).node },
        s.pages.pages.first
    ));

    // At this point:
    //      +----------+ = PAGE 0
    //  ... :          :
    //     +-------------+ ACTIVE
    // 4305 |1ABCD00000| | 0
    // 4306 |2EFGH00000| | 1
    //      :^         : : = PIN 0
    // 4307 |3IJKL00000| | 2
    //      +----------+ :
    //     +-------------+
    // PORTS: oracle 0bad91adb — rotating rows into a freshly grown page
    // still renews the existing source page's generation.
    let node = s.pages.pages.first;
    let serial = unsafe { (*node).serial };
    s.cursor_scroll_above().unwrap();
    assert!(!s.pages.node_is_valid(node, serial));

    //      +----------+ = PAGE 0
    //  ... :          :
    // 4305 |1ABCD00000|
    //     +-------------+ ACTIVE
    // 4306 |2EFGH00000| | 0
    // 4307 |          | | 1
    //      :^         : : = PIN 0
    //      +----------+ :
    //      +----------+ : = PAGE 1
    //    0 |3IJKL00000| | 2
    //      +----------+ :
    //     +-------------+

    // try s.pages.diagram(std.io.getStdErr().writer());

    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"2EFGH\n\n3IJKL");
    }
    {
        let list_cell = s
            .pages
            .get_cell(point::Point::Active(point::Coordinate { x: 0, y: 1 }))
            .unwrap();
        let cell = list_cell.cell;
        assert!(unsafe { (*cell).content_tag() } == pagepkg::ContentTag::BgColorRgb);
        assert_eq!(pagepkg::CellRgb { r: 155, g: 0, b: 0 }, unsafe {
            (*cell).content_color_rgb()
        });
    }

    // Page 0's penultimate row is dirty because the cursor moved off of it.
    assert!(s
        .pages
        .is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 0 })));
    // Page 0's final row is dirty because it was cleared.
    assert!(s
        .pages
        .is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 1 })));
    // Page 1's row is dirty because it's new.
    assert!(s
        .pages
        .is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 2 })));
    s.deinit();
}

// test "Screen: scroll above with cursor on non-final row"
#[test]
fn screen_scroll_above_with_cursor_on_non_final_row() {
    let mut s = mkscreen(10, 4, 10);

    // Get the cursor to be 2 rows above a new page
    let first_page_size = unsafe { crate::pagelist::Node::capacity(s.pages.pages.first).rows };
    unsafe { (*crate::pagelist::Node::page(s.pages.pages.first)).pause_integrity_checks(true) };
    for _ in 0..(first_page_size - 3) {
        s.test_write_string(b"\n").unwrap();
    }
    unsafe { (*crate::pagelist::Node::page(s.pages.pages.first)).pause_integrity_checks(false) };

    // Write 3 lines of text, forcing the last line into the first
    // row of a new page. Move our cursor onto the previous page.
    s.set_attribute(crate::sgr::Attribute::DirectColorBg(crate::color::RGB {
        r: 155,
        g: 0,
        b: 0,
    }))
    .unwrap();
    s.test_write_string(b"1AB\n2BC\n3DE\n4FG").unwrap();
    s.cursor_absolute(0, 1);
    s.pages.clear_dirty();

    // Ensure we're still on the first page. So our cursor is on the first
    // page but we have two pages of data.
    assert!(std::ptr::eq(
        unsafe { (*s.cursor.page_pin).node },
        s.pages.pages.first
    ));

    //      +----------+ = PAGE 0
    //  ... :          :
    //     +-------------+ ACTIVE
    // 4305 |1AB0000000| | 0
    // 4306 |2BC0000000| | 1
    //      :^         : : = PIN 0
    // 4307 |3DE0000000| | 2
    //      +----------+ :
    //      +----------+ : = PAGE 1
    //    0 |4FG0000000| | 3
    //      +----------+ :
    //     +-------------+
    s.cursor_scroll_above().unwrap();

    //     +----------+ = PAGE 0
    //  ... :          :
    // 4305 |1AB0000000|
    //     +-------------+ ACTIVE
    // 4306 |2BC0000000| | 0
    // 4307 |          | | 1
    //      :^         : : = PIN 0
    //      +----------+ :
    //      +----------+ : = PAGE 1
    //    0 |3DE0000000| | 2
    //    1 |4FG0000000| | 3
    //      +----------+ :
    //     +-------------+
    // try s.pages.diagram(std.io.getStdErr().writer());

    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"2BC\n\n3DE\n4FG");
    }
    {
        let list_cell = s
            .pages
            .get_cell(point::Point::Active(point::Coordinate { x: 0, y: 1 }))
            .unwrap();
        let cell = list_cell.cell;
        assert!(unsafe { (*cell).content_tag() } == pagepkg::ContentTag::BgColorRgb);
        assert_eq!(pagepkg::CellRgb { r: 155, g: 0, b: 0 }, unsafe {
            (*cell).content_color_rgb()
        });
    }

    // Page 0's penultimate row is dirty because the cursor moved off of it.
    assert!(s
        .pages
        .is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 0 })));
    // Page 0's final row is dirty because it was cleared.
    assert!(s
        .pages
        .is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 1 })));
    // Page 1's row is dirty because it's new.
    assert!(s
        .pages
        .is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 2 })));
    s.deinit();
}

// test "Screen: scroll above no scrollback bottom of page"
#[test]
fn screen_scroll_above_no_scrollback_bottom_of_page() {
    let mut s = mkscreen(10, 3, 0);

    let first_page_size = unsafe { crate::pagelist::Node::capacity(s.pages.pages.first).rows };
    unsafe { (*crate::pagelist::Node::page(s.pages.pages.first)).pause_integrity_checks(true) };
    for _ in 0..(first_page_size - 3) {
        s.test_write_string(b"\n").unwrap();
    }
    unsafe { (*crate::pagelist::Node::page(s.pages.pages.first)).pause_integrity_checks(false) };

    s.set_attribute(crate::sgr::Attribute::DirectColorBg(crate::color::RGB {
        r: 155,
        g: 0,
        b: 0,
    }))
    .unwrap();
    s.test_write_string(b"1ABCD\n2EFGH\n3IJKL").unwrap();
    s.cursor_absolute(0, 1);
    s.pages.clear_dirty();

    // At this point:
    //  +-------------+ ACTIVE
    //   +----------+ : = PAGE 0
    // 0 |1ABCD00000| | 0
    // 1 |2EFGH00000| | 1
    //   :^         : : = PIN 0
    // 2 |3IJKL00000| | 2
    //   +----------+ :
    //  +-------------+

    s.cursor_scroll_above().unwrap();

    //   +----------+ = PAGE 0
    // 0 |1ABCD00000|
    //  +-------------+ ACTIVE
    // 1 |2EFGH00000| | 0
    // 2 |          | | 1
    //   :^         : : = PIN 0
    // 3 |3IJKL00000| | 2
    //   +----------+ :
    //  +-------------+

    //try s.pages.diagram(std.io.getStdErr().writer());

    {
        let contents = dump(&s, vp());
        assert_eq!(contents, b"2EFGH\n\n3IJKL");
    }
    {
        let list_cell = s
            .pages
            .get_cell(point::Point::Active(point::Coordinate { x: 0, y: 1 }))
            .unwrap();
        let cell = list_cell.cell;
        assert!(unsafe { (*cell).content_tag() } == pagepkg::ContentTag::BgColorRgb);
        assert_eq!(pagepkg::CellRgb { r: 155, g: 0, b: 0 }, unsafe {
            (*cell).content_color_rgb()
        });
    }

    // Page 0 row 1 (active row 0) is dirty because the cursor moved off of it.
    assert!(s
        .pages
        .is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 0 })));
    // Page 0 row 2 (active row 1) is dirty because it was cleared.
    assert!(s
        .pages
        .is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 1 })));
    // Page 0 row 3 (active row 2) is dirty because it is new.
    assert!(s
        .pages
        .is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 2 })));
    s.deinit();
}

// test "Screen: scroll above hyperlink-dense row to fresh page"
#[test]
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'Parity tail C-1 — the G6 triage'): past a 15-minute Miri bound in the debug-assertions-ON G6 lane, where `Page::assert_integrity` runs on nearly every page mutation (measured per test; see the C-1 table there). It RUNS in the G6c lane (`check_leaks.sh --debug-assertions-off`). Do NOT widen this to a plain `miri` ignore: that would drop the test from BOTH lanes."
)]
fn screen_scroll_above_hyperlink_dense_row_to_fresh_page() {
    // Regression test for https://github.com/ghostty-org/ghostty/discussions/13160
    //
    // When a scroll-above operation pushes a row carrying more unique
    // hyperlinks than a fresh page's default hyperlink capacity across
    // a page boundary, the cross-page row clone must increase the
    // destination page's capacity (like insertLines/deleteLines do)
    // rather than error out mid-operation, which leaves the page list
    // half-mutated and aborts later (e.g. in clearCells).
    let mut s = mkscreen(10, 5, 10000);

    // Fill the first page so it is exactly full and the cursor is on
    // its last row (which is also the bottom row of the active area).
    // The next grow() will then allocate a fresh page.
    let first_page_rows = unsafe { crate::pagelist::Node::capacity(s.pages.pages.first).rows };
    unsafe { (*crate::pagelist::Node::page(s.pages.pages.first)).pause_integrity_checks(true) };
    for _ in 0..(first_page_rows - 1) {
        s.test_write_string(b"\n").unwrap();
    }
    unsafe { (*crate::pagelist::Node::page(s.pages.pages.first)).pause_integrity_checks(false) };
    assert!(std::ptr::eq(s.pages.pages.first, s.pages.pages.last));
    assert_eq!(
        unsafe { crate::pagelist::Node::capacity(s.pages.pages.first).rows },
        unsafe {
            (*crate::pagelist::Node::page(s.pages.pages.first))
                .size
                .rows
        },
    );
    assert_eq!(s.pages.rows - 1, s.cursor.y);

    // Fill the bottom row with unique hyperlinks: more than a fresh
    // page can hold with default hyperlink capacity.
    for i in 0..s.pages.cols {
        let uri = format!("http://example.com/{i}").into_bytes();
        s.start_hyperlink(&uri, None).unwrap();
        s.test_write_string(b"A").unwrap();
        s.end_hyperlink();
    }
    assert_eq!(s.pages.cols as usize, unsafe {
        (*crate::pagelist::Node::page((*s.cursor.page_pin).node))
            .hyperlink_set
            .count() as usize
    });

    // Move the cursor above the bottom row and scroll. The dense row is
    // pushed across the page boundary into the freshly allocated page.
    s.cursor_absolute(0, 1);
    s.cursor_scroll_above().unwrap();

    // We must have created a second page and the dense row must now be
    // the top row of that page.
    assert!(!std::ptr::eq(s.pages.pages.first, s.pages.pages.last));

    // All hyperlinks must have survived the scroll intact: every cell
    // flagged as a hyperlink must resolve to a real entry in its page's
    // hyperlink map. A half-applied scroll leaves cells whose hyperlink
    // flag is set but that have no map entry, which aborts in
    // clearCells later.
    let mut node_ = s.pages.pages.first;
    while !node_.is_null() {
        let page: *mut Page = unsafe { crate::pagelist::Node::page(node_) };
        unsafe { (*page).assert_integrity() };
        for y in 0..unsafe { (*page).size.rows } as usize {
            let row = unsafe { (*page).get_row(y) };
            if !unsafe { (*row).hyperlink() } {
                continue;
            }
            let cells = unsafe { (*page).get_cells(row) };
            let cells: &mut [pagepkg::Cell] = unsafe { &mut *cells };
            for cell in cells.iter_mut() {
                if !cell.hyperlink() {
                    continue;
                }
                assert!(unsafe { (*page).lookup_hyperlink(cell) }.is_some());
            }
        }
        node_ = unsafe { (*node_).next };
    }
    assert_eq!(s.pages.cols as usize, unsafe {
        (*crate::pagelist::Node::page(s.pages.pages.last))
            .hyperlink_set
            .count() as usize
    });

    // The dense row is still the bottom row of the active area.
    for x in 0..s.pages.cols {
        let list_cell = s
            .pages
            .get_cell(point::Point::Active(point::Coordinate { x, y: 4 }))
            .unwrap();
        assert!(unsafe { (*list_cell.cell).hyperlink() });
        let page: *mut Page = unsafe { crate::pagelist::Node::page(list_cell.node) };
        let id = unsafe { (*page).lookup_hyperlink(list_cell.cell) }.unwrap();
        let link = unsafe { &*(*page).hyperlink_set.get((*page).memory_ptr(), id) };
        let expected = format!("http://example.com/{x}");
        assert_eq!(expected.as_bytes(), unsafe {
            link.uri.slice((*page).memory_ptr())
        });
    }

    s.deinit();
}

// test "Screen: scroll above hyperlink-dense row to existing page"
#[test]
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'Parity tail C-1 — the G6 triage'): past a 15-minute Miri bound in the debug-assertions-ON G6 lane, where `Page::assert_integrity` runs on nearly every page mutation (measured per test; see the C-1 table there). It RUNS in the G6c lane (`check_leaks.sh --debug-assertions-off`). Do NOT widen this to a plain `miri` ignore: that would drop the test from BOTH lanes."
)]
fn screen_scroll_above_hyperlink_dense_row_to_existing_page() {
    // Same as the fresh page variant above but the destination page
    // already exists (fresh_node == null path in cursorScrollAboveRotate).
    let mut s = mkscreen(10, 5, 10000);

    // Fill the first page so it is exactly full and the cursor is on
    // its last row.
    let first_page_rows = unsafe { crate::pagelist::Node::capacity(s.pages.pages.first).rows };
    unsafe { (*crate::pagelist::Node::page(s.pages.pages.first)).pause_integrity_checks(true) };
    for _ in 0..(first_page_rows - 1) {
        s.test_write_string(b"\n").unwrap();
    }
    unsafe { (*crate::pagelist::Node::page(s.pages.pages.first)).pause_integrity_checks(false) };
    assert_eq!(s.pages.rows - 1, s.cursor.y);

    // Fill the last row of the first page with unique hyperlinks:
    // more than a page can hold with default hyperlink capacity.
    for i in 0..s.pages.cols {
        let uri = format!("http://example.com/{i}").into_bytes();
        s.start_hyperlink(&uri, None).unwrap();
        s.test_write_string(b"A").unwrap();
        s.end_hyperlink();
    }

    // Scroll twice so the active area straddles the page boundary:
    // the last two active rows are on a second page while the dense
    // row remains the last row of the first page.
    s.test_write_string(b"\n\n").unwrap();
    assert!(!std::ptr::eq(s.pages.pages.first, s.pages.pages.last));
    assert!(std::ptr::eq(
        unsafe { (*s.cursor.page_pin).node },
        s.pages.pages.last
    ));

    // Move the cursor to an active row that is still on the first page
    // and above the dense row, then scroll. grow() has capacity in the
    // last page so no fresh page is allocated, but the dense row still
    // crosses the page boundary during the rotate.
    s.cursor_absolute(0, 0);
    assert!(std::ptr::eq(
        unsafe { (*s.cursor.page_pin).node },
        s.pages.pages.first
    ));
    s.cursor_scroll_above().unwrap();

    // All hyperlinks must have survived the scroll intact: every cell
    // flagged as a hyperlink must resolve to a real entry in its page's
    // hyperlink map.
    let mut node_ = s.pages.pages.first;
    while !node_.is_null() {
        let page: *mut Page = unsafe { crate::pagelist::Node::page(node_) };
        unsafe { (*page).assert_integrity() };
        for y in 0..unsafe { (*page).size.rows } as usize {
            let row = unsafe { (*page).get_row(y) };
            if !unsafe { (*row).hyperlink() } {
                continue;
            }
            let cells = unsafe { (*page).get_cells(row) };
            let cells: &mut [pagepkg::Cell] = unsafe { &mut *cells };
            for cell in cells.iter_mut() {
                if !cell.hyperlink() {
                    continue;
                }
                assert!(unsafe { (*page).lookup_hyperlink(cell) }.is_some());
            }
        }
        node_ = unsafe { (*node_).next };
    }
    assert_eq!(s.pages.cols as usize, unsafe {
        (*crate::pagelist::Node::page(s.pages.pages.last))
            .hyperlink_set
            .count() as usize
    });

    s.deinit();
}
