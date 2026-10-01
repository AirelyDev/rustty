//! PORTS: ghostty src/terminal/Terminal.zig — the `deleteLines` / `insertLines` / `insertBlanks` / insert-mode tests.

use super::*;

// test "Terminal: deleteLines simple"
#[test]
fn terminal_delete_lines_simple() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.print_string("ABC".as_bytes()).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print_string("DEF".as_bytes()).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print_string("GHI".as_bytes()).unwrap();
    t.set_cursor_pos(2, 2);

    // PORTS: oracle 0bad91adb — a full-width line operation moves whole
    // rows, so the affected page generations are renewed.
    let node = unsafe { (*(*t.screens.active).cursor.page_pin).node };
    let serial = unsafe { (*node).serial };
    t.clear_dirty();
    t.delete_lines(1);
    assert!(!unsafe { (*t.screens.active).pages.node_is_valid(node, serial) });

    assert!(!t.is_dirty(active_pt(0, 0)));
    assert!(t.is_dirty(active_pt(0, 1)));
    assert!(t.is_dirty(active_pt(0, 2)));
    assert!(t.is_dirty(active_pt(0, 3)));

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "ABC\nGHI".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: deleteLines with scroll region"
#[test]
fn terminal_delete_lines_with_scroll_region() {
    let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

    // Initial value
    t.print('A' as u32).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print('B' as u32).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print('C' as u32).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print('D' as u32).unwrap();

    t.set_top_and_bottom_margin(1, 3);
    t.set_cursor_pos(1, 1);

    t.clear_dirty();
    t.delete_lines(1);

    assert!(t.is_dirty(active_pt(0, 0)));
    assert!(t.is_dirty(active_pt(0, 1)));
    assert!(t.is_dirty(active_pt(0, 2)));
    assert!(!t.is_dirty(active_pt(0, 3)));

    t.print('E' as u32).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "E\nC\n\nD".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: deleteLines with scroll region, large count"
#[test]
fn terminal_delete_lines_with_scroll_region_large_count() {
    let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

    // Initial value
    t.print('A' as u32).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print('B' as u32).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print('C' as u32).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print('D' as u32).unwrap();

    t.set_top_and_bottom_margin(1, 3);
    t.set_cursor_pos(1, 1);

    t.clear_dirty();
    t.delete_lines(5);

    assert!(t.is_dirty(active_pt(0, 0)));
    assert!(t.is_dirty(active_pt(0, 1)));
    assert!(t.is_dirty(active_pt(0, 2)));
    assert!(!t.is_dirty(active_pt(0, 3)));

    t.print('E' as u32).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "E\n\n\nD".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: insertBlanks zero"
#[test]
fn terminal_insert_blanks_zero() {
    let mut t = Terminal::init(Global, opts(5, 2)).unwrap();

    t.print('A' as u32).unwrap();
    t.print('B' as u32).unwrap();
    t.print('C' as u32).unwrap();
    t.set_cursor_pos(1, 1);

    t.insert_blanks(0);

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "ABC".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: insertBlanks"
#[test]
fn terminal_insert_blanks() {
    // NOTE: this is not verified with conformance tests, so these
    // tests might actually be verifying wrong behavior.
    let mut t = Terminal::init(Global, opts(5, 2)).unwrap();

    t.print('A' as u32).unwrap();
    t.print('B' as u32).unwrap();
    t.print('C' as u32).unwrap();
    t.set_cursor_pos(1, 1);

    t.clear_dirty();
    t.insert_blanks(2);
    assert!(t.is_dirty(active_pt(0, 0)));
    assert!(!t.is_dirty(active_pt(0, 1)));

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "  ABC".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: insertBlanks pushes off end"
#[test]
fn terminal_insert_blanks_pushes_off_end() {
    // NOTE: this is not verified with conformance tests, so these
    // tests might actually be verifying wrong behavior.
    let mut t = Terminal::init(Global, opts(3, 2)).unwrap();

    t.print('A' as u32).unwrap();
    t.print('B' as u32).unwrap();
    t.print('C' as u32).unwrap();
    t.set_cursor_pos(1, 1);

    t.clear_dirty();
    t.insert_blanks(2);
    assert!(t.is_dirty(active_pt(0, 0)));

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "  A".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: insertBlanks more than size"
#[test]
fn terminal_insert_blanks_more_than_size() {
    // NOTE: this is not verified with conformance tests, so these
    // tests might actually be verifying wrong behavior.
    let mut t = Terminal::init(Global, opts(3, 2)).unwrap();

    t.print('A' as u32).unwrap();
    t.print('B' as u32).unwrap();
    t.print('C' as u32).unwrap();
    t.set_cursor_pos(1, 1);

    t.clear_dirty();
    t.insert_blanks(5);
    assert!(t.is_dirty(active_pt(0, 0)));

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: insertBlanks no scroll region, fits"
#[test]
fn terminal_insert_blanks_no_scroll_region_fits() {
    let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

    for c in "ABC".chars() {
        t.print(c as u32).unwrap();
    }
    t.set_cursor_pos(1, 1);

    t.clear_dirty();
    t.insert_blanks(2);
    assert!(t.is_dirty(active_pt(0, 0)));

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "  ABC".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: insertBlanks shift off screen"
#[test]
fn terminal_insert_blanks_shift_off_screen() {
    let mut t = Terminal::init(Global, opts(5, 10)).unwrap();

    for c in "  ABC".chars() {
        t.print(c as u32).unwrap();
    }
    t.set_cursor_pos(1, 3);
    t.clear_dirty();
    t.insert_blanks(2);
    assert!(t.is_dirty(active_pt(0, 0)));
    t.print('X' as u32).unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "  X A".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: insertBlanks split multi-cell character"
#[test]
fn terminal_insert_blanks_split_multi_cell_character() {
    let mut t = Terminal::init(Global, opts(5, 10)).unwrap();

    for c in "123".chars() {
        t.print(c as u32).unwrap();
    }
    t.print('橋' as u32).unwrap();
    t.set_cursor_pos(1, 1);
    t.clear_dirty();
    t.insert_blanks(1);
    assert!(t.is_dirty(active_pt(0, 0)));

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), " 123".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: insertBlanks inside left/right scroll region"
#[test]
fn terminal_insert_blanks_inside_left_right_scroll_region() {
    let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

    t.scrolling_region.left = 2;
    t.scrolling_region.right = 4;
    t.set_cursor_pos(1, 3);
    for c in "ABC".chars() {
        t.print(c as u32).unwrap();
    }
    t.set_cursor_pos(1, 3);

    t.clear_dirty();
    t.insert_blanks(2);
    assert!(t.is_dirty(active_pt(0, 0)));
    t.print('X' as u32).unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "  X A".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: insertBlanks outside left/right scroll region"
#[test]
fn terminal_insert_blanks_outside_left_right_scroll_region() {
    let mut t = Terminal::init(Global, opts(6, 10)).unwrap();

    t.set_cursor_pos(1, 4);
    for c in "ABC".chars() {
        t.print(c as u32).unwrap();
    }
    t.scrolling_region.left = 2;
    t.scrolling_region.right = 4;
    assert!(unsafe { (*t.screens.active).cursor.pending_wrap });
    t.clear_dirty();
    t.insert_blanks(2);
    assert!(!t.is_dirty(active_pt(0, 0)));
    assert!(!unsafe { (*t.screens.active).cursor.pending_wrap });
    t.print('X' as u32).unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "   ABX".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: insertLines colors with bg color"
#[test]
fn terminal_insert_lines_colors_with_bg_color() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.print_string("ABC".as_bytes()).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print_string("DEF".as_bytes()).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print_string("GHI".as_bytes()).unwrap();
    t.set_cursor_pos(2, 2);

    t.set_attribute(sgr::Attribute::DirectColorBg(color::RGB {
        r: 0xFF,
        g: 0,
        b: 0,
    }))
    .unwrap();
    t.insert_lines(1);

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "ABC\n\nDEF\nGHI".as_bytes());
    }

    for x in 0..t.cols {
        let list_cell = t.active_mut().pages.get_cell(active_pt(x, 1)).unwrap();
        let cell = list_cell.cell;
        assert!(unsafe { (*cell).content_tag() } == pagepkg::ContentTag::BgColorRgb);
        assert_eq!(
            pagepkg::CellRgb {
                r: 0xFF,
                g: 0,
                b: 0,
            },
            unsafe { (*cell).content_color_rgb() }
        );
    }

    t.deinit(Global);
}

// test "Terminal: insertLines handles style refs"
#[test]
fn terminal_insert_lines_handles_style_refs() {
    let mut t = Terminal::init(Global, opts(5, 3)).unwrap();

    t.print_string("ABC".as_bytes()).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print_string("DEF".as_bytes()).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();

    // For the line being deleted, create a refcounted style
    t.set_attribute(sgr::Attribute::Bold).unwrap();
    t.print_string("GHI".as_bytes()).unwrap();
    t.set_attribute(sgr::Attribute::Unset).unwrap();

    // verify we have styles in our style map
    let page = unsafe { crate::pagelist::Node::page((*(*t.screens.active).cursor.page_pin).node) };
    assert_eq!(1usize, unsafe { (*page).styles.count() });

    t.set_cursor_pos(2, 2);
    t.insert_lines(1);

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "ABC\n\nDEF".as_bytes());
    }

    // verify we have no styles in our style map
    assert_eq!(0usize, unsafe { (*page).styles.count() });

    t.deinit(Global);
}

// test "Terminal: insertLines across page boundary marks all shifted rows dirty"
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
)]
#[test]
fn terminal_insert_lines_across_page_boundary_marks_all_shifted_rows_dirty() {
    let mut t = Terminal::init(Global, opts_sb(10, 5, 1024)).unwrap();

    let first_page = t.active_mut().pages.pages.first;
    let first_page_nrows = unsafe { crate::pagelist::Node::capacity(first_page).rows };

    // Fill up the first page minus 3 rows
    for _ in 0..(first_page_nrows - 3) {
        t.linefeed().unwrap();
    }

    // Add content that will cross a page boundary
    t.print_string("1AAAA".as_bytes()).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print_string("2BBBB".as_bytes()).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print_string("3CCCC".as_bytes()).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print_string("4DDDD".as_bytes()).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print_string("5EEEE".as_bytes()).unwrap();

    // Verify we now have a second page
    let second_page = unsafe { (*first_page).next };
    assert!(!second_page.is_null());
    // PORTS: oracle 0bad91adb — every page in the shifted range is renewed.
    let first_serial = unsafe { (*first_page).serial };
    let second_serial = unsafe { (*second_page).serial };

    t.set_cursor_pos(1, 1);
    t.clear_dirty();
    t.insert_lines(1);
    assert!(!unsafe {
        (*t.screens.active)
            .pages
            .node_is_valid(first_page, first_serial)
    });
    assert!(!unsafe {
        (*t.screens.active)
            .pages
            .node_is_valid(second_page, second_serial)
    });

    assert!(t.is_dirty(active_pt(0, 0)));
    assert!(t.is_dirty(active_pt(0, 1)));
    assert!(t.is_dirty(active_pt(0, 2)));
    assert!(t.is_dirty(active_pt(0, 3)));
    assert!(t.is_dirty(active_pt(0, 4)));

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "\n1AAAA\n2BBBB\n3CCCC\n4DDDD".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: insertLines (legacy test)"
#[test]
fn terminal_insert_lines_legacy_test() {
    let mut t = Terminal::init(Global, opts(2, 5)).unwrap();

    // Initial value
    t.print('A' as u32).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print('B' as u32).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print('C' as u32).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print('D' as u32).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print('E' as u32).unwrap();

    // Move to row 2
    t.set_cursor_pos(2, 1);

    // Insert two lines
    t.insert_lines(2);

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "A\n\n\nB\nC".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: insertLines multi-codepoint graphemes"
#[test]
fn terminal_insert_lines_multi_codepoint_graphemes() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    // Disable grapheme clustering
    t.modes.set(modespkg::Mode::GraphemeCluster, true);

    t.print_string("ABC".as_bytes()).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();

    // This is: 👨‍👩‍👧 (which may or may not render correctly)
    t.print(0x1F468).unwrap();
    t.print(0x200D).unwrap();
    t.print(0x1F469).unwrap();
    t.print(0x200D).unwrap();
    t.print(0x1F467).unwrap();

    t.carriage_return();
    t.linefeed().unwrap();
    t.print_string("GHI".as_bytes()).unwrap();
    t.set_cursor_pos(2, 2);
    t.insert_lines(1);

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(
            str.as_slice(),
            "ABC\n\n👨\u{200D}👩\u{200D}👧\nGHI".as_bytes()
        );
    }

    t.deinit(Global);
}

// test "Terminal: deleteLines colors with bg color"
#[test]
fn terminal_delete_lines_colors_with_bg_color() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.print_string("ABC".as_bytes()).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print_string("DEF".as_bytes()).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print_string("GHI".as_bytes()).unwrap();
    t.set_cursor_pos(2, 2);

    t.set_attribute(sgr::Attribute::DirectColorBg(color::RGB {
        r: 0xFF,
        g: 0,
        b: 0,
    }))
    .unwrap();
    t.delete_lines(1);

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "ABC\nGHI".as_bytes());
    }

    for x in 0..t.cols {
        let list_cell = t.active_mut().pages.get_cell(active_pt(x, 4)).unwrap();
        let cell = list_cell.cell;
        assert!(unsafe { (*cell).content_tag() } == pagepkg::ContentTag::BgColorRgb);
        assert_eq!(
            pagepkg::CellRgb {
                r: 0xFF,
                g: 0,
                b: 0,
            },
            unsafe { (*cell).content_color_rgb() }
        );
    }

    t.deinit(Global);
}

// test "Terminal: deleteLines across page boundary marks all shifted rows dirty"
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
)]
#[test]
fn terminal_delete_lines_across_page_boundary_marks_all_shifted_rows_dirty() {
    let mut t = Terminal::init(Global, opts_sb(10, 5, 1024)).unwrap();

    let first_page = t.active_mut().pages.pages.first;
    let first_page_nrows = unsafe { crate::pagelist::Node::capacity(first_page).rows };

    // Fill up the first page minus 3 rows
    for _ in 0..(first_page_nrows - 3) {
        t.linefeed().unwrap();
    }

    // Add content that will cross a page boundary
    t.print_string("1AAAA".as_bytes()).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print_string("2BBBB".as_bytes()).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print_string("3CCCC".as_bytes()).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print_string("4DDDD".as_bytes()).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print_string("5EEEE".as_bytes()).unwrap();

    // Verify we now have a second page
    let second_page = unsafe { (*first_page).next };
    assert!(!second_page.is_null());
    // PORTS: oracle 0bad91adb — every page in the shifted range is renewed.
    let first_serial = unsafe { (*first_page).serial };
    let second_serial = unsafe { (*second_page).serial };

    t.set_cursor_pos(1, 1);
    t.clear_dirty();
    t.delete_lines(1);
    assert!(!unsafe {
        (*t.screens.active)
            .pages
            .node_is_valid(first_page, first_serial)
    });
    assert!(!unsafe {
        (*t.screens.active)
            .pages
            .node_is_valid(second_page, second_serial)
    });

    assert!(t.is_dirty(active_pt(0, 0)));
    assert!(t.is_dirty(active_pt(0, 1)));
    assert!(t.is_dirty(active_pt(0, 2)));
    assert!(t.is_dirty(active_pt(0, 3)));
    assert!(t.is_dirty(active_pt(0, 4)));

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "2BBBB\n3CCCC\n4DDDD\n5EEEE".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: deleteLines (legacy)"
#[test]
fn terminal_delete_lines_legacy() {
    let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

    // Initial value
    t.print('A' as u32).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print('B' as u32).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print('C' as u32).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print('D' as u32).unwrap();

    t.cursor_up(2);
    t.delete_lines(1);

    t.print('E' as u32).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();

    // We should be
    assert_eq!(0usize, unsafe { (*t.screens.active).cursor.x } as usize);
    assert_eq!(2usize, unsafe { (*t.screens.active).cursor.y } as usize);

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "A\nE\nD".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: deleteLines with scroll region, cursor outside of region"
#[test]
fn terminal_delete_lines_with_scroll_region_cursor_outside_of_region() {
    let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

    // Initial value
    t.print('A' as u32).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print('B' as u32).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print('C' as u32).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print('D' as u32).unwrap();

    t.set_top_and_bottom_margin(1, 3);
    t.set_cursor_pos(4, 1);

    t.clear_dirty();
    t.delete_lines(1);

    for y in 0..4 {
        assert!(!t.is_dirty(active_pt(0, y)));
    }

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "A\nB\nC\nD".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: deleteLines resets pending wrap"
#[test]
fn terminal_delete_lines_resets_pending_wrap() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    for c in "ABCDE".chars() {
        t.print(c as u32).unwrap();
    }
    assert!(unsafe { (*t.screens.active).cursor.pending_wrap });
    t.delete_lines(1);
    assert!(!unsafe { (*t.screens.active).cursor.pending_wrap });
    t.print('B' as u32).unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "B".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: deleteLines resets wrap"
#[test]
fn terminal_delete_lines_resets_wrap() {
    let mut t = Terminal::init(Global, opts(3, 3)).unwrap();

    t.print('1' as u32).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    for c in "ABCDEF".chars() {
        t.print(c as u32).unwrap();
    }

    t.set_top_and_bottom_margin(1, 2);
    t.set_cursor_pos(1, 1);
    t.delete_lines(1);
    t.print('X' as u32).unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "XBC\n\nDEF".as_bytes());
    }

    for y in 0..t.rows {
        let list_cell = t
            .active_mut()
            .pages
            .get_cell(active_pt(0, y as u32))
            .unwrap();
        let row = list_cell.row;
        assert!(!unsafe { (*row).wrap() });
    }

    t.deinit(Global);
}

// test "Terminal: deleteLines left/right scroll region"
#[test]
fn terminal_delete_lines_left_right_scroll_region() {
    let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

    t.print_string("ABC123".as_bytes()).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print_string("DEF456".as_bytes()).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print_string("GHI789".as_bytes()).unwrap();
    t.scrolling_region.left = 1;
    t.scrolling_region.right = 3;
    t.set_cursor_pos(2, 2);

    t.clear_dirty();
    t.delete_lines(1);

    assert!(!t.is_dirty(active_pt(0, 0)));
    for y in 1..3 {
        assert!(t.is_dirty(active_pt(0, y)));
    }

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "ABC123\nDHI756\nG   89".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: deleteLines left/right scroll region from top"
#[test]
fn terminal_delete_lines_left_right_scroll_region_from_top() {
    let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

    t.print_string("ABC123".as_bytes()).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print_string("DEF456".as_bytes()).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print_string("GHI789".as_bytes()).unwrap();
    t.scrolling_region.left = 1;
    t.scrolling_region.right = 3;
    t.set_cursor_pos(1, 2);

    t.clear_dirty();
    t.delete_lines(1);

    for y in 0..3 {
        assert!(t.is_dirty(active_pt(0, y)));
    }

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "AEF423\nDHI756\nG   89".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: deleteLines left/right scroll region high count"
#[test]
fn terminal_delete_lines_left_right_scroll_region_high_count() {
    let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

    t.print_string("ABC123".as_bytes()).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print_string("DEF456".as_bytes()).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print_string("GHI789".as_bytes()).unwrap();
    t.scrolling_region.left = 1;
    t.scrolling_region.right = 3;
    t.set_cursor_pos(2, 2);

    t.clear_dirty();
    t.delete_lines(100);

    assert!(!t.is_dirty(active_pt(0, 0)));
    for y in 1..3 {
        assert!(t.is_dirty(active_pt(0, y)));
    }

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "ABC123\nD   56\nG   89".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: deleteLines wide character spacer head"
#[test]
fn terminal_delete_lines_wide_character_spacer_head() {
    let mut t = Terminal::init(Global, opts(5, 3)).unwrap();

    // Initial value
    // +-----+
    // |AAAAA| < Wrapped
    // |BBBB*| < Wrapped     (continued)
    // |WWCCC| < Non-wrapped (continued)
    // +-----+
    // where * represents a spacer head cell
    // and WW is the wide character.
    t.print_string("AAAAABBBB\u{1F600}CCC".as_bytes()).unwrap();

    // Delete the top line
    // +-----+
    // |BBBB | < Non-wrapped
    // |WWCCC| < Non-wrapped
    // |     | < Non-wrapped
    // +-----+
    // This should convert the spacer head to
    // a regular empty cell, and un-set wrap.
    t.set_cursor_pos(1, 1);
    t.delete_lines(1);

    {
        let str = t.plain_string(Global).unwrap();
        let unwrapped_str = t.plain_string_unwrapped(Global).unwrap();
        assert_eq!(str.as_slice(), "BBBB\n\u{1F600}CCC".as_bytes());
        assert_eq!(unwrapped_str.as_slice(), "BBBB\n\u{1F600}CCC".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: deleteLines wide character spacer head left scroll margin"
#[test]
fn terminal_delete_lines_wide_character_spacer_head_left_scroll_margin() {
    let mut t = Terminal::init(Global, opts(5, 3)).unwrap();

    // Initial value
    // +-----+
    // |AAAAA| < Wrapped
    // |BBBB*| < Wrapped     (continued)
    // |WWCCC| < Non-wrapped (continued)
    // +-----+
    // where * represents a spacer head cell
    // and WW is the wide character.
    t.print_string("AAAAABBBB\u{1F600}CCC".as_bytes()).unwrap();

    t.scrolling_region.left = 2;

    // Delete the top line
    //    ###  <- scrolling region
    // +-----+
    // |AABB | < Wrapped
    // |BBCCC| < Wrapped     (continued)
    // |WW   | < Non-wrapped (continued)
    // +-----+
    // This should convert the spacer head to
    // a regular empty cell, but due to the
    // left scrolling margin, wrap state should
    // remain.
    t.set_cursor_pos(1, 3);
    t.delete_lines(1);

    {
        let str = t.plain_string(Global).unwrap();
        let unwrapped_str = t.plain_string_unwrapped(Global).unwrap();
        assert_eq!(str.as_slice(), "AABB\nBBCCC\n\u{1F600}".as_bytes());
        assert_eq!(unwrapped_str.as_slice(), "AABB BBCCC\u{1F600}".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: deleteLines wide character spacer head right scroll margin"
#[test]
fn terminal_delete_lines_wide_character_spacer_head_right_scroll_margin() {
    let mut t = Terminal::init(Global, opts(5, 3)).unwrap();

    // Initial value
    // +-----+
    // |AAAAA| < Wrapped
    // |BBBB*| < Wrapped     (continued)
    // |WWCCC| < Non-wrapped (continued)
    // +-----+
    // where * represents a spacer head cell
    // and WW is the wide character.
    t.print_string("AAAAABBBB\u{1F600}CCC".as_bytes()).unwrap();

    t.scrolling_region.right = 3;

    // Delete the top line
    //  ####   <- scrolling region
    // +-----+
    // |BBBBA| < Wrapped
    // |WWCC | < Wrapped     (continued)
    // |    C| < Non-wrapped (continued)
    // +-----+
    // This should convert the spacer head to
    // a regular empty cell, but due to the
    // right scrolling margin, wrap state should
    // remain.
    t.set_cursor_pos(1, 1);
    t.delete_lines(1);

    {
        let str = t.plain_string(Global).unwrap();
        let unwrapped_str = t.plain_string_unwrapped(Global).unwrap();
        assert_eq!(str.as_slice(), "BBBBA\n\u{1F600}CC\n    C".as_bytes());
        assert_eq!(
            unwrapped_str.as_slice(),
            "BBBBA\u{1F600}CC     C".as_bytes()
        );
    }

    t.deinit(Global);
}

// test "Terminal: deleteLines wide character spacer head left and right scroll margin"
#[test]
fn terminal_delete_lines_wide_character_spacer_head_left_and_right_scroll_margin() {
    let mut t = Terminal::init(Global, opts(5, 3)).unwrap();

    // Initial value
    // +-----+
    // |AAAAA| < Wrapped
    // |BBBB*| < Wrapped     (continued)
    // |WWCCC| < Non-wrapped (continued)
    // +-----+
    // where * represents a spacer head cell
    // and WW is the wide character.
    t.print_string("AAAAABBBB\u{1F600}CCC".as_bytes()).unwrap();

    t.scrolling_region.right = 3;
    t.scrolling_region.left = 2;

    // Delete the top line
    //    ##   <- scrolling region
    // +-----+
    // |AABBA| < Wrapped
    // |BBCC*| < Wrapped     (continued)
    // |WW  C| < Non-wrapped (continued)
    // +-----+
    // Because there is both a left scrolling
    // margin > 1 and a right scrolling margin
    // the spacer head should remain, and the
    // wrap state should be untouched.
    t.set_cursor_pos(1, 3);
    t.delete_lines(1);

    {
        let str = t.plain_string(Global).unwrap();
        let unwrapped_str = t.plain_string_unwrapped(Global).unwrap();
        assert_eq!(str.as_slice(), "AABBA\nBBCC\n\u{1F600}  C".as_bytes());
        assert_eq!(unwrapped_str.as_slice(), "AABBABBCC\u{1F600}  C".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: deleteLines wide character spacer head left (< 2) and right scroll margin"
#[test]
fn terminal_delete_lines_wide_character_spacer_head_left_2_and_right_scroll_margin() {
    let mut t = Terminal::init(Global, opts(5, 3)).unwrap();

    // Initial value
    // +-----+
    // |AAAAA| < Wrapped
    // |BBBB*| < Wrapped     (continued)
    // |WWCCC| < Non-wrapped (continued)
    // +-----+
    // where * represents a spacer head cell
    // and WW is the wide character.
    t.print_string("AAAAABBBB\u{1F600}CCC".as_bytes()).unwrap();

    t.scrolling_region.right = 3;
    t.scrolling_region.left = 1;

    // Delete the top line
    //   ###   <- scrolling region
    // +-----+
    // |ABBBA| < Wrapped
    // |B CC | < Wrapped     (continued)
    // |    C| < Non-wrapped (continued)
    // +-----+
    // Because the left margin is 1, the wide
    // char is split, and therefore removed,
    // along with the spacer head - however,
    // wrap state should be untouched.
    t.set_cursor_pos(1, 2);
    t.delete_lines(1);

    {
        let str = t.plain_string(Global).unwrap();
        let unwrapped_str = t.plain_string_unwrapped(Global).unwrap();
        assert_eq!(str.as_slice(), "ABBBA\nB CC\n    C".as_bytes());
        assert_eq!(unwrapped_str.as_slice(), "ABBBAB CC     C".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: deleteLines wide characters split by left/right scroll region boundaries"
#[test]
fn terminal_delete_lines_wide_characters_split_by_left_right_scroll_region_boundaries() {
    let mut t = Terminal::init(Global, opts(5, 2)).unwrap();

    // Initial value
    // +-----+
    // |AAAAA|
    // |WWBWW|
    // +-----+
    // where WW represents a wide character
    t.print_string("AAAAA\n\u{1F600}B\u{1F600}".as_bytes())
        .unwrap();

    t.scrolling_region.right = 3;
    t.scrolling_region.left = 1;

    // Delete the top line
    //   ###   <- scrolling region
    // +-----+
    // |A B A|
    // |     |
    // +-----+
    // The two wide chars, because they're
    // split by the edge of the scrolling
    // region, get removed.
    t.set_cursor_pos(1, 2);
    t.delete_lines(1);

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "A B A".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: deleteLines zero"
#[test]
fn terminal_delete_lines_zero() {
    let mut t = Terminal::init(Global, opts(2, 5)).unwrap();

    // This should do nothing
    t.set_cursor_pos(1, 1);
    t.delete_lines(0);

    t.deinit(Global);
}

// Reproduces a crash found by AFL++ fuzzer (afl-out/stream/default/crashes/
// id:000007,sig:06,src:004522). The crash is a page integrity violation
// "spacer tail not following wide" triggered during scrollUp -> deleteLines
// -> clearCells. When deleteLines count >= scroll region height, all rows
// are cleared (no shifting), so rowWillBeShifted is never called and wide
// characters straddling the right margin boundary leave orphaned spacer_tails.
// test "Terminal: deleteLines wide char at right margin with full clear"
#[test]
fn terminal_delete_lines_wide_char_at_right_margin_with_full_clear() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

    // Place a wide character at col 39 (1-indexed) on several rows.
    // The wide cell lands at col 38 (0-indexed) with spacer_tail at col 39.
    t.set_cursor_pos(10, 39);
    t.print(0x4E2D).unwrap(); // '中'

    // Set left/right scroll margins so scrolling_region.right = 38.
    // clearCells will clear cells[4..39], which includes the wide cell
    // at col 38 but NOT the spacer_tail at col 39.
    t.modes.set(modespkg::Mode::EnableLeftAndRightMargin, true);
    t.set_left_and_right_margin(5, 39);

    // scrollUp with count >= region height causes deleteLines to clear
    // ALL rows without any shifting, so rowWillBeShifted is never called
    // and the orphaned spacer_tail at col 39 triggers a page integrity
    // violation in clearCells.
    t.scroll_up(t.rows as usize).unwrap();

    t.deinit(Global);
}

// test "Terminal: insertBlanks preserves background sgr"
#[test]
fn terminal_insert_blanks_preserves_background_sgr() {
    let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

    for c in "ABC".chars() {
        t.print(c as u32).unwrap();
    }
    t.set_cursor_pos(1, 1);
    t.set_attribute(sgr::Attribute::DirectColorBg(color::RGB {
        r: 0xFF,
        g: 0,
        b: 0,
    }))
    .unwrap();
    t.insert_blanks(2);

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "  ABC".as_bytes());
    }
    {
        let list_cell = t.active_mut().pages.get_cell(active_pt(0, 0)).unwrap();
        let cell = list_cell.cell;
        assert!(unsafe { (*cell).content_tag() } == pagepkg::ContentTag::BgColorRgb);
        assert_eq!(
            pagepkg::CellRgb {
                r: 0xFF,
                g: 0,
                b: 0,
            },
            unsafe { (*cell).content_color_rgb() }
        );
    }

    t.deinit(Global);
}

// test "Terminal: insertBlanks left/right scroll region large count"
#[test]
fn terminal_insert_blanks_left_right_scroll_region_large_count() {
    let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

    t.modes.set(modespkg::Mode::Origin, true);
    t.modes.set(modespkg::Mode::EnableLeftAndRightMargin, true);
    t.set_left_and_right_margin(3, 5);
    t.set_cursor_pos(1, 1);
    t.clear_dirty();
    t.insert_blanks(140);
    assert!(t.is_dirty(active_pt(0, 0)));
    t.print('X' as u32).unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "  X".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: insertBlanks deleting graphemes"
#[test]
fn terminal_insert_blanks_deleting_graphemes() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    // Disable grapheme clustering
    t.modes.set(modespkg::Mode::GraphemeCluster, true);

    t.print_string("ABC".as_bytes()).unwrap();

    // This is: 👨‍👩‍👧 (which may or may not render correctly)
    t.print(0x1F468).unwrap();
    t.print(0x200D).unwrap();
    t.print(0x1F469).unwrap();
    t.print(0x200D).unwrap();
    t.print(0x1F467).unwrap();

    // We should have one cell with graphemes
    let page = unsafe { crate::pagelist::Node::page((*(*t.screens.active).cursor.page_pin).node) };
    assert_eq!(1usize, unsafe { (*page).grapheme_count() });

    t.set_cursor_pos(1, 1);
    t.clear_dirty();
    t.insert_blanks(4);
    assert!(t.is_dirty(active_pt(0, 0)));

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "    A".as_bytes());
    }

    // We should have no graphemes
    assert_eq!(0usize, unsafe { (*page).grapheme_count() });

    t.deinit(Global);
}

// test "Terminal: insertBlanks shift graphemes"
#[test]
fn terminal_insert_blanks_shift_graphemes() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    // Enable grapheme clustering
    t.modes.set(modespkg::Mode::GraphemeCluster, true);

    t.print_string("A".as_bytes()).unwrap();

    // This is: 👨‍👩‍👧 (which may or may not render correctly)
    t.print(0x1F468).unwrap();
    t.print(0x200D).unwrap();
    t.print(0x1F469).unwrap();
    t.print(0x200D).unwrap();
    t.print(0x1F467).unwrap();

    // We should have one cell with graphemes
    let page = unsafe { crate::pagelist::Node::page((*(*t.screens.active).cursor.page_pin).node) };
    assert_eq!(1usize, unsafe { (*page).grapheme_count() });

    t.set_cursor_pos(1, 1);
    t.clear_dirty();
    t.insert_blanks(1);
    assert!(t.is_dirty(active_pt(0, 0)));

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), " A👨\u{200D}👩\u{200D}👧".as_bytes());
    }

    // We should have no graphemes
    assert_eq!(1usize, unsafe { (*page).grapheme_count() });

    t.deinit(Global);
}

// test "Terminal: insertBlanks split multi-cell character from tail"
#[test]
fn terminal_insert_blanks_split_multi_cell_character_from_tail() {
    let mut t = Terminal::init(Global, opts(5, 10)).unwrap();

    t.print_string("橋123".as_bytes()).unwrap();
    t.set_cursor_pos(1, 2);
    t.insert_blanks(1);

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "   12".as_bytes());
    }

    t.deinit(Global);
}

#[allow(non_snake_case)]
// test "Terminal: insertLines hyperlink-dense row crosses page boundary"
#[test]
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'Parity tail C-1 — the G6 triage'): past a 15-minute Miri bound in the debug-assertions-ON G6 lane, where `Page::assert_integrity` runs on nearly every page mutation (measured per test; see the C-1 table there). It RUNS in the G6c lane (`check_leaks.sh --debug-assertions-off`). Do NOT widen this to a plain `miri` ignore: that would drop the test from BOTH lanes."
)]
fn terminal_insertLines_hyperlink_dense_row_crosses_page_boundary() {
    // Regression test for the cross-page copy of insertLines: when the
    // shifted row carries more unique hyperlinks than the destination
    // page's hyperlink capacity, the copy must increase the destination
    // page's capacity and retry rather than corrupting the page list.
    let mut t = Terminal::init(Global, opts(10, 5)).unwrap();

    // Fill the first page so it is exactly full, then two more rows so
    // the second page holds the last two active rows (y=3 and y=4).
    let first_page_rows =
        unsafe { crate::pagelist::Node::capacity(t.active_mut().pages.pages.first).rows };
    for _ in 0..(first_page_rows + 1) {
        t.linefeed().unwrap();
    }
    assert!(!std::ptr::eq(
        t.active_mut().pages.pages.first,
        t.active_mut().pages.pages.last
    ));
    assert_eq!(2, unsafe {
        crate::pagelist::Node::rows(t.active_mut().pages.pages.last)
    });

    // Marker rows so we can verify the shift afterwards.
    t.set_cursor_pos(1, 1);
    t.print_string(b"0").unwrap();
    t.set_cursor_pos(2, 1);
    t.print_string(b"1").unwrap();
    t.set_cursor_pos(4, 1);
    t.print_string(b"3").unwrap();
    t.set_cursor_pos(5, 1);
    t.print_string(b"4").unwrap();

    // Fill the last row of the first page (active y=2) with unique
    // hyperlinks: more than the second page can hold with its default
    // hyperlink capacity. Writing them grows the first page's capacity
    // as needed; the second page keeps its default capacity.
    t.set_cursor_pos(3, 1);
    for i in 0..10u32 {
        let uri = format!("http://example.com/{i}").into_bytes();
        t.active_mut().start_hyperlink(&uri, None).unwrap();
        t.print('A' as u32 + i).unwrap();
        t.active_mut().end_hyperlink();
    }
    {
        let pin = t
            .active_mut()
            .pages
            .pin(point::Point::Active(point::Coordinate { x: 0, y: 2 }))
            .unwrap();
        assert!(std::ptr::eq(t.active_mut().pages.pages.first, pin.node));
        assert_eq!(
            unsafe { crate::pagelist::Node::rows(pin.node) } as usize - 1,
            pin.y as usize
        );
    }
    assert!(
        unsafe {
            (*crate::pagelist::Node::page(t.active_mut().pages.pages.last))
                .hyperlink_set
                .layout
                .cap
        } < 10,
        "the destination page must NOT already fit the dense row, or the retry never runs"
    );

    // Insert a line at the top: every row shifts down by one and the
    // dense row crosses the page boundary into the second page.
    t.set_cursor_pos(1, 1);
    t.insert_lines(1);

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), b"\n0\n1\nABCDEFGHIJ\n3");
    }

    // The second page's hyperlink capacity had to grow to receive the
    // row, proving the capacity-retry path ran.
    assert!(
        unsafe {
            (*crate::pagelist::Node::page(t.active_mut().pages.pages.last))
                .hyperlink_set
                .layout
                .cap
        } >= 10
    );

    // Every cell of the dense row must still resolve to a real
    // hyperlink entry with the correct URI. A half-applied shift
    // leaves cells whose hyperlink flag is set but that have no map
    // entry, which aborts in clearCells later.
    for x in 0..10u16 {
        let list_cell = t
            .active_mut()
            .pages
            .get_cell(point::Point::Active(point::Coordinate { x, y: 3 }))
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
    let mut node_ = t.active_mut().pages.pages.first;
    while !node_.is_null() {
        unsafe { (*crate::pagelist::Node::page(node_)).assert_integrity() };
        node_ = unsafe { (*node_).next };
    }

    t.deinit(Global);
}

#[allow(non_snake_case)]
// test "Terminal: deleteLines hyperlink-dense row crosses page boundary"
#[test]
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'Parity tail C-1 — the G6 triage'): past a 15-minute Miri bound in the debug-assertions-ON G6 lane, where `Page::assert_integrity` runs on nearly every page mutation (measured per test; see the C-1 table there). It RUNS in the G6c lane (`check_leaks.sh --debug-assertions-off`). Do NOT widen this to a plain `miri` ignore: that would drop the test from BOTH lanes."
)]
fn terminal_deleteLines_hyperlink_dense_row_crosses_page_boundary() {
    // Regression test for the cross-page copy of deleteLines: when the
    // shifted row carries more unique hyperlinks than the destination
    // page's hyperlink capacity, the copy must increase the destination
    // page's capacity and retry rather than corrupting the page list.
    //
    // This is the mirror of the insertLines variant: the dense row
    // starts as the first row of the second page and is pulled up into
    // the first page, which also happens to be the cursor's page so
    // this exercises the cursor accounting of the capacity increase.
    let mut t = Terminal::init(Global, opts(10, 5)).unwrap();

    let first_page_rows =
        unsafe { crate::pagelist::Node::capacity(t.active_mut().pages.pages.first).rows };
    for _ in 0..(first_page_rows + 1) {
        t.linefeed().unwrap();
    }
    assert!(!std::ptr::eq(
        t.active_mut().pages.pages.first,
        t.active_mut().pages.pages.last
    ));
    assert_eq!(2, unsafe {
        crate::pagelist::Node::rows(t.active_mut().pages.pages.last)
    });

    // Marker rows so we can verify the shift afterwards.
    t.set_cursor_pos(1, 1);
    t.print_string(b"0").unwrap();
    t.set_cursor_pos(2, 1);
    t.print_string(b"1").unwrap();
    t.set_cursor_pos(3, 1);
    t.print_string(b"2").unwrap();
    t.set_cursor_pos(5, 1);
    t.print_string(b"4").unwrap();

    // Fill the first row of the second page (active y=3) with unique
    // hyperlinks: more than the first page can hold with its default
    // hyperlink capacity.
    t.set_cursor_pos(4, 1);
    for i in 0..10u32 {
        let uri = format!("http://example.com/{i}").into_bytes();
        t.active_mut().start_hyperlink(&uri, None).unwrap();
        t.print('A' as u32 + i).unwrap();
        t.active_mut().end_hyperlink();
    }
    {
        let pin = t
            .active_mut()
            .pages
            .pin(point::Point::Active(point::Coordinate { x: 0, y: 3 }))
            .unwrap();
        assert!(std::ptr::eq(t.active_mut().pages.pages.last, pin.node));
        assert_eq!(0, pin.y);
    }
    assert!(
        unsafe {
            (*crate::pagelist::Node::page(t.active_mut().pages.pages.first))
                .hyperlink_set
                .layout
                .cap
        } < 10,
        "the destination page must NOT already fit the dense row, or the retry never runs"
    );

    // Delete the top line: every row shifts up by one and the dense
    // row crosses the page boundary into the first page.
    t.set_cursor_pos(1, 1);
    t.delete_lines(1);

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), b"1\n2\nABCDEFGHIJ\n4");
    }

    // The first page's hyperlink capacity had to grow to receive the
    // row, proving the capacity-retry path ran.
    assert!(
        unsafe {
            (*crate::pagelist::Node::page(t.active_mut().pages.pages.first))
                .hyperlink_set
                .layout
                .cap
        } >= 10
    );

    for x in 0..10u16 {
        let list_cell = t
            .active_mut()
            .pages
            .get_cell(point::Point::Active(point::Coordinate { x, y: 2 }))
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

    let mut node_ = t.active_mut().pages.pages.first;
    while !node_.is_null() {
        unsafe { (*crate::pagelist::Node::page(node_)).assert_integrity() };
        node_ = unsafe { (*node_).next };
    }

    t.deinit(Global);
}

// test "Terminal: insertBlanks shifts hyperlinks"
#[test]
fn terminal_insert_blanks_shifts_hyperlinks() {
    // osc "8;;http://example.com"
    // printf "link"
    // printf "\r"
    // csi "3@"
    // echo
    //
    // link should be preserved, blanks should not be linked

    let mut t = Terminal::init(Global, opts(10, 2)).unwrap();

    t.active_mut()
        .start_hyperlink("http://example.com".as_bytes(), None)
        .unwrap();
    t.print_string("ABC".as_bytes()).unwrap();
    t.set_cursor_pos(1, 1);
    t.insert_blanks(2);

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "  ABC".as_bytes());
    }

    // Verify all our cells have a hyperlink
    for x in 2..5 {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(x, 0)).unwrap();
        let row = list_cell.row;
        assert!(unsafe { (*row).hyperlink() });
        let cell = list_cell.cell;
        assert!(unsafe { (*cell).hyperlink() });
        let id = unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_hyperlink(cell) }
            .unwrap();
        assert_eq!(id, 1 as crate::hyperlink::Id);
    }
    for x in 0..2 {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(x, 0)).unwrap();
        let cell = list_cell.cell;
        assert!(!unsafe { (*cell).hyperlink() });
        let id = unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_hyperlink(cell) };
        assert!(id.is_none());
    }

    t.deinit(Global);
}

// test "Terminal: insertBlanks pushes hyperlink off end completely"
#[test]
fn terminal_insert_blanks_pushes_hyperlink_off_end_completely() {
    let mut t = Terminal::init(Global, opts(3, 2)).unwrap();

    t.active_mut()
        .start_hyperlink("http://example.com".as_bytes(), None)
        .unwrap();
    t.print_string("ABC".as_bytes()).unwrap();
    t.set_cursor_pos(1, 1);
    t.insert_blanks(3);

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "".as_bytes());
    }

    for x in 0..3 {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(x, 0)).unwrap();
        let row = list_cell.row;
        assert!(!unsafe { (*row).hyperlink() });
        let cell = list_cell.cell;
        assert!(!unsafe { (*cell).hyperlink() });
        let id = unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_hyperlink(cell) };
        assert!(id.is_none());
    }

    t.deinit(Global);
}

// test "Terminal: insertBlanks wide char straddling right margin"
#[test]
fn terminal_insert_blanks_wide_char_straddling_right_margin() {
    // Crash found by AFL++ fuzzer.
    //
    // When a wide character straddles the right scroll margin (head at the
    // margin, spacer_tail just beyond it), insertBlanks shifts the wide head
    // away via swapCells but leaves the orphaned spacer_tail in place,
    // causing a page integrity violation.
    let mut t = Terminal::init(Global, opts(10, 5)).unwrap();

    // Fill row: A B C D 橋 _ _ _ _ _
    // Positions: 0 1 2 3 4W 5T 6 7 8 9
    t.set_cursor_pos(1, 1);
    for c in "ABCD".chars() {
        t.print(c as u32).unwrap();
    }
    t.print('橋' as u32).unwrap(); // wide char: head at 4, spacer_tail at 5

    // Set right margin so the wide head is AT the boundary and the
    // spacer_tail is just outside it.
    t.scrolling_region.right = 4;

    // Position cursor at x=2 (1-indexed col 3) and insert one blank.
    // This triggers the swap loop which displaces the wide head at
    // position 4 without clearing the spacer_tail at position 5.
    t.set_cursor_pos(1, 3);
    t.insert_blanks(1);

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "AB CD".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: insertBlanks wide char spacer_tail orphaned beyond right margin"
#[test]
fn terminal_insert_blanks_wide_char_spacer_tail_orphaned_beyond_right_margin() {
    // Regression test for AFL++ crash.
    //
    // When insertBlanks clears the entire region from cursor to the right
    // margin (scroll_amount == 0), a wide character whose head is AT the
    // right margin gets cleared but its spacer_tail just beyond the margin
    // is left behind, causing a page integrity violation:
    //   "spacer tail not following wide"
    let mut t = Terminal::init(Global, opts(10, 5)).unwrap();

    // Fill cols 0–9 with wide chars: 中中中中中
    // Positions: 0W 1T 2W 3T 4W 5T 6W 7T 8W 9T
    for _ in 0..5 {
        t.print(0x4E2D).unwrap();
    }

    // Set left/right margins so that the last wide char (cols 8–9)
    // straddles the boundary: head at col 8 (inside), tail at col 9 (outside).
    t.modes.set(modespkg::Mode::EnableLeftAndRightMargin, true);
    t.set_left_and_right_margin(1, 9); // 1-indexed: left=0, right=8

    // Cursor is now at (0, 0) after DECSLRM.  Print a narrow char to
    // advance cursor to col 1.
    t.print('a' as u32).unwrap();

    // ICH 8: insert 8 blanks at cursor x=1.
    // rem = right(8) - x(1) + 1 = 8, adjusted_count = 8, scroll_amount = 0.
    // The code clears cols 1–8 without noticing the spacer_tail at col 9.
    t.insert_blanks(8);

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "a".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: insert mode with space"
#[test]
fn terminal_insert_mode_with_space() {
    let mut t = Terminal::init(Global, opts(10, 2)).unwrap();

    for c in "hello".chars() {
        t.print(c as u32).unwrap();
    }
    t.set_cursor_pos(1, 2);
    t.modes.set(modespkg::Mode::Insert, true);
    t.print('X' as u32).unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "hXello".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: insert mode doesn't wrap pushed characters"
#[test]
fn terminal_insert_mode_doesn_t_wrap_pushed_characters() {
    let mut t = Terminal::init(Global, opts(5, 2)).unwrap();

    for c in "hello".chars() {
        t.print(c as u32).unwrap();
    }
    t.set_cursor_pos(1, 2);
    t.modes.set(modespkg::Mode::Insert, true);
    t.print('X' as u32).unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "hXell".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: insert mode does nothing at the end of the line"
#[test]
fn terminal_insert_mode_does_nothing_at_the_end_of_the_line() {
    let mut t = Terminal::init(Global, opts(5, 2)).unwrap();

    for c in "hello".chars() {
        t.print(c as u32).unwrap();
    }
    t.modes.set(modespkg::Mode::Insert, true);
    t.print('X' as u32).unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "hello\nX".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: insert mode with wide characters"
#[test]
fn terminal_insert_mode_with_wide_characters() {
    let mut t = Terminal::init(Global, opts(5, 2)).unwrap();

    for c in "hello".chars() {
        t.print(c as u32).unwrap();
    }
    t.set_cursor_pos(1, 2);
    t.modes.set(modespkg::Mode::Insert, true);
    t.print('😀' as u32).unwrap(); // 0x1F600

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "h😀el".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: insert mode with wide characters at end"
#[test]
fn terminal_insert_mode_with_wide_characters_at_end() {
    let mut t = Terminal::init(Global, opts(5, 2)).unwrap();

    for c in "well".chars() {
        t.print(c as u32).unwrap();
    }
    t.modes.set(modespkg::Mode::Insert, true);
    t.print('😀' as u32).unwrap(); // 0x1F600

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "well\n😀".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: insert mode pushing off wide character"
#[test]
fn terminal_insert_mode_pushing_off_wide_character() {
    let mut t = Terminal::init(Global, opts(5, 2)).unwrap();

    for c in "123".chars() {
        t.print(c as u32).unwrap();
    }
    t.print('😀' as u32).unwrap(); // 0x1F600
    t.modes.set(modespkg::Mode::Insert, true);
    t.set_cursor_pos(1, 1);
    t.print('X' as u32).unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "X123".as_bytes());
    }

    t.deinit(Global);
}

// PORT NOTE: NOT a port of a reference `test {}` block. This is the
// page-pool tracking-allocator gate for this batch's page-touching methods
// (scroll_up/scroll_down/insert_lines/delete_lines, which delegate row
// movement and scrollback creation to `pagelist`). Under `cfg(test)` the
// page pool's backing allocator records into the global `page_tracking`
// tracker; this test drives multi-page scroll + insert churn and asserts
// every page allocated during the scope is returned after `deinit` (no
// page leak across the tracked-pin / page-split paths).
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
)]
#[test]
fn terminal_pagepool_no_leak_scroll_insert_multi_page() {
    let scope = crate::pagelist::page_tracking::PageLeakScope::new();
    {
        let mut t = Terminal::init(Global, opts(20, 5)).unwrap();

        // Fill enough rows (with scrollback) to force several pages of
        // churn through the scroll path.
        for i in 0..400u32 {
            let line = format!("row{i:04}");
            t.print_string(line.as_bytes()).unwrap();
            t.carriage_return();
            t.linefeed().unwrap();
        }

        // scroll_up at the full-screen top creates scrollback (cursorScrollAbove)
        for _ in 0..50 {
            t.scroll_up(1).unwrap();
        }

        // scroll_down delegates to insert_lines.
        for _ in 0..20 {
            t.scroll_down(1);
        }

        // Drive insert/delete lines with a scroll region too.
        t.set_top_and_bottom_margin(1, 4);
        t.set_cursor_pos(1, 1);
        for _ in 0..30 {
            t.insert_lines(2);
            t.delete_lines(1);
        }

        // We should have produced real page churn.
        assert!(
            scope.delta_allocs() > 0,
            "expected scroll/insert churn to allocate at least one new page"
        );

        t.deinit(Global);
    }
    // After deinit every page allocated during this scope must be returned.
    scope.assert_returned_to_baseline();
}

// PORT NOTE: NOT a port of a reference `test {}` block. This is the
// page-pool tracking-allocator gate for this batch's `resize` method, which
// delegates the active-screen resize (reflow) to `Screen::resize` ->
// `pagelist` (page split/merge + scrollback trimming). Under `cfg(test)` the
// page pool's backing allocator records into the global `page_tracking`
// tracker; this test drives resize grow + shrink churn (with reflow) and
// asserts every page allocated during the scope is returned after `deinit`
// (no page leak across the resize page-churn path).
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): TIMEOUT in BOTH Miri lanes — re-measured per test in single-worker isolation at a 6-minute bound with `-C debug-assertions=off`, so removing the integrity assertions does not rescue it; the cost is page-mapping/LZ4 volume, not assertion overhead. Covered natively + by ASan."
)]
#[test]
fn terminal_pagepool_no_leak_resize_grow() {
    let scope = crate::pagelist::page_tracking::PageLeakScope::new();
    {
        let alloc = Global;
        let mut t = Terminal::init(alloc, opts(20, 5)).unwrap();

        // Enable wraparound so resize reflows soft-wrapped content (driving
        // page split/merge through the reflow path).
        t.modes.set(modespkg::Mode::Wraparound, true);

        // Fill enough wrapped content to force several pages of churn.
        for i in 0..400u32 {
            let line = format!("row{i:04}-padding-text-to-wrap");
            t.print_string(line.as_bytes()).unwrap();
            t.carriage_return();
            t.linefeed().unwrap();
        }

        // Grow then shrink columns repeatedly to drive reflow page churn.
        for _ in 0..10 {
            t.resize(
                alloc,
                Resize {
                    cols: 60,
                    rows: 5,
                    cell_size_px: None,
                },
            )
            .unwrap();
            t.resize(
                alloc,
                Resize {
                    cols: 8,
                    rows: 5,
                    cell_size_px: None,
                },
            )
            .unwrap();
            t.resize(
                alloc,
                Resize {
                    cols: 20,
                    rows: 5,
                    cell_size_px: None,
                },
            )
            .unwrap();
        }

        // We should have produced real page churn.
        assert!(
            scope.delta_allocs() > 0,
            "expected resize reflow churn to allocate at least one new page"
        );

        t.deinit(alloc);
    }
    // After deinit every page allocated during this scope must be returned.
    scope.assert_returned_to_baseline();
}
