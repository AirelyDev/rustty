//! PORTS: ghostty src/terminal/Terminal.zig — the linefeed, margins, `insertLines`, `scrollUp` / `scrollDown`, decaln-margin, `reverseIndex` and `index` tests.

use super::*;

// ===================================================================
// vt-12b-3b: scroll / lines / margins tests (+ deferred linefeed tests)
// ===================================================================

// test "Terminal: linefeed and carriage return"
#[test]
fn terminal_linefeed_and_carriage_return() {
    let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

    // Print and CR.
    for c in "hello".chars() {
        t.print(c as u32).unwrap();
    }
    t.clear_dirty();
    t.carriage_return();

    // CR should not mark row dirty because it doesn't change rendering.
    assert!(!t.is_dirty(screen_pt(0, 0)));

    t.linefeed().unwrap();

    // LF marks row dirty due to cursor movement
    assert!(t.is_dirty(screen_pt(0, 0)));
    assert!(t.is_dirty(screen_pt(0, 1)));

    for c in "world".chars() {
        t.print(c as u32).unwrap();
    }
    assert_eq!(unsafe { (*t.screens.active).cursor.y }, 1);
    assert_eq!(unsafe { (*t.screens.active).cursor.x }, 5);
    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "hello\nworld".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: linefeed unsets pending wrap"
#[test]
fn terminal_linefeed_unsets_pending_wrap() {
    let mut t = Terminal::init(Global, opts(5, 80)).unwrap();

    // Basic grid writing
    for c in "hello".chars() {
        t.print(c as u32).unwrap();
    }
    assert!(unsafe { (*t.screens.active).cursor.pending_wrap });
    t.clear_dirty();
    t.linefeed().unwrap();
    assert!(t.is_dirty(screen_pt(0, 0)));
    assert!(t.is_dirty(screen_pt(0, 1)));
    assert!(!unsafe { (*t.screens.active).cursor.pending_wrap });

    t.deinit(Global);
}

// test "Terminal: linefeed mode automatic carriage return"
#[test]
fn terminal_linefeed_mode_automatic_carriage_return() {
    let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

    // Basic grid writing
    t.modes.set(modespkg::Mode::Linefeed, true);
    t.print_string("123456".as_bytes()).unwrap();
    t.linefeed().unwrap();
    t.print('X' as u32).unwrap();
    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "123456\nX".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: setTopAndBottomMargin simple"
#[test]
fn terminal_set_top_and_bottom_margin_simple() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.print_string("ABC".as_bytes()).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print_string("DEF".as_bytes()).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print_string("GHI".as_bytes()).unwrap();
    t.set_top_and_bottom_margin(0, 0);

    t.clear_dirty();
    t.scroll_down(1);

    // Mark the rows we moved as dirty.
    assert!(t.is_dirty(active_pt(0, 0)));
    assert!(t.is_dirty(active_pt(0, 1)));
    assert!(t.is_dirty(active_pt(0, 2)));
    assert!(t.is_dirty(active_pt(0, 3)));

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "\nABC\nDEF\nGHI".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: setTopAndBottomMargin top only"
#[test]
fn terminal_set_top_and_bottom_margin_top_only() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.print_string("ABC".as_bytes()).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print_string("DEF".as_bytes()).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print_string("GHI".as_bytes()).unwrap();
    t.set_top_and_bottom_margin(2, 0);

    t.clear_dirty();
    t.scroll_down(1);

    // This is dirty because the cursor moves from this row
    assert!(t.is_dirty(active_pt(0, 0)));
    assert!(t.is_dirty(active_pt(0, 1)));
    assert!(t.is_dirty(active_pt(0, 2)));
    assert!(t.is_dirty(active_pt(0, 3)));

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "ABC\n\nDEF\nGHI".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: setTopAndBottomMargin top and bottom"
#[test]
fn terminal_set_top_and_bottom_margin_top_and_bottom() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.print_string("ABC".as_bytes()).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print_string("DEF".as_bytes()).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print_string("GHI".as_bytes()).unwrap();
    t.set_top_and_bottom_margin(1, 2);

    t.clear_dirty();
    t.scroll_down(1);

    assert!(t.is_dirty(active_pt(0, 0)));
    assert!(t.is_dirty(active_pt(0, 1)));
    assert!(!t.is_dirty(active_pt(0, 2)));

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "\nABC\nGHI".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: setTopAndBottomMargin top equal to bottom"
#[test]
fn terminal_set_top_and_bottom_margin_top_equal_to_bottom() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.print_string("ABC".as_bytes()).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print_string("DEF".as_bytes()).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print_string("GHI".as_bytes()).unwrap();
    t.set_top_and_bottom_margin(2, 2);

    t.clear_dirty();
    t.scroll_down(1);

    assert!(t.is_dirty(active_pt(0, 0)));
    assert!(t.is_dirty(active_pt(0, 1)));
    assert!(t.is_dirty(active_pt(0, 2)));
    assert!(t.is_dirty(active_pt(0, 3)));

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "\nABC\nDEF\nGHI".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: setLeftAndRightMargin simple"
#[test]
fn terminal_set_left_and_right_margin_simple() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.print_string("ABC".as_bytes()).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print_string("DEF".as_bytes()).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print_string("GHI".as_bytes()).unwrap();
    t.modes.set(modespkg::Mode::EnableLeftAndRightMargin, true);
    t.set_left_and_right_margin(0, 0);

    t.clear_dirty();
    t.erase_chars(1);

    assert!(t.is_dirty(active_pt(0, 0)));
    assert!(!t.is_dirty(active_pt(0, 1)));

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), " BC\nDEF\nGHI".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: setLeftAndRightMargin left only"
#[test]
fn terminal_set_left_and_right_margin_left_only() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.print_string("ABC".as_bytes()).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print_string("DEF".as_bytes()).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print_string("GHI".as_bytes()).unwrap();
    t.modes.set(modespkg::Mode::EnableLeftAndRightMargin, true);
    t.set_left_and_right_margin(2, 0);
    assert_eq!(t.scrolling_region.left as usize, 1);
    assert_eq!(t.scrolling_region.right as usize, (t.cols - 1) as usize);
    t.set_cursor_pos(1, 2);

    t.clear_dirty();
    t.insert_lines(1);

    assert!(t.is_dirty(active_pt(0, 0)));
    assert!(t.is_dirty(active_pt(0, 1)));
    assert!(t.is_dirty(active_pt(0, 2)));
    assert!(t.is_dirty(active_pt(0, 3)));

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "A\nDBC\nGEF\n HI".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: setLeftAndRightMargin left and right"
#[test]
fn terminal_set_left_and_right_margin_left_and_right() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.print_string("ABC".as_bytes()).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print_string("DEF".as_bytes()).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print_string("GHI".as_bytes()).unwrap();
    t.modes.set(modespkg::Mode::EnableLeftAndRightMargin, true);
    t.set_left_and_right_margin(1, 2);
    t.set_cursor_pos(1, 2);

    t.clear_dirty();
    t.insert_lines(1);

    assert!(t.is_dirty(active_pt(0, 0)));
    assert!(t.is_dirty(active_pt(0, 1)));
    assert!(t.is_dirty(active_pt(0, 2)));
    assert!(t.is_dirty(active_pt(0, 3)));

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "  C\nABF\nDEI\nGH".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: setLeftAndRightMargin left equal right"
#[test]
fn terminal_set_left_and_right_margin_left_equal_right() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.print_string("ABC".as_bytes()).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print_string("DEF".as_bytes()).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print_string("GHI".as_bytes()).unwrap();
    t.modes.set(modespkg::Mode::EnableLeftAndRightMargin, true);
    t.set_left_and_right_margin(2, 2);
    t.set_cursor_pos(1, 2);

    t.clear_dirty();
    t.insert_lines(1);

    assert!(t.is_dirty(active_pt(0, 0)));
    assert!(t.is_dirty(active_pt(0, 1)));
    assert!(t.is_dirty(active_pt(0, 2)));
    assert!(t.is_dirty(active_pt(0, 3)));

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "\nABC\nDEF\nGHI".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: setLeftAndRightMargin mode 69 unset"
#[test]
fn terminal_set_left_and_right_margin_mode_69_unset() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.print_string("ABC".as_bytes()).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print_string("DEF".as_bytes()).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print_string("GHI".as_bytes()).unwrap();
    t.modes.set(modespkg::Mode::EnableLeftAndRightMargin, false);
    t.set_left_and_right_margin(1, 2);
    t.set_cursor_pos(1, 2);

    t.clear_dirty();
    t.insert_lines(1);

    assert!(t.is_dirty(active_pt(0, 0)));
    assert!(t.is_dirty(active_pt(0, 1)));
    assert!(t.is_dirty(active_pt(0, 2)));
    assert!(t.is_dirty(active_pt(0, 3)));

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "\nABC\nDEF\nGHI".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: insertLines simple"
#[test]
fn terminal_insert_lines_simple() {
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
    t.insert_lines(1);
    assert!(!unsafe { (*t.screens.active).pages.node_is_valid(node, serial) });

    assert!(!t.is_dirty(active_pt(0, 0)));
    assert!(t.is_dirty(active_pt(0, 1)));
    assert!(t.is_dirty(active_pt(0, 2)));
    assert!(t.is_dirty(active_pt(0, 3)));

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "ABC\n\nDEF\nGHI".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: insertLines outside of scroll region"
#[test]
fn terminal_insert_lines_outside_of_scroll_region() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.print_string("ABC".as_bytes()).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print_string("DEF".as_bytes()).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print_string("GHI".as_bytes()).unwrap();
    t.set_top_and_bottom_margin(3, 4);
    t.set_cursor_pos(2, 2);

    t.clear_dirty();
    t.insert_lines(1);

    assert!(!t.is_dirty(active_pt(0, 0)));
    assert!(!t.is_dirty(active_pt(0, 1)));
    assert!(!t.is_dirty(active_pt(0, 2)));

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "ABC\nDEF\nGHI".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: insertLines top/bottom scroll region"
#[test]
fn terminal_insert_lines_top_bottom_scroll_region() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.print_string("ABC".as_bytes()).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print_string("DEF".as_bytes()).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print_string("GHI".as_bytes()).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print_string("123".as_bytes()).unwrap();
    t.set_top_and_bottom_margin(1, 3);
    t.set_cursor_pos(2, 2);

    t.clear_dirty();
    t.insert_lines(1);

    assert!(!t.is_dirty(active_pt(0, 0)));
    assert!(t.is_dirty(active_pt(0, 1)));
    assert!(t.is_dirty(active_pt(0, 2)));
    assert!(!t.is_dirty(active_pt(0, 3)));

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "ABC\n\nDEF\n123".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: insertLines zero"
#[test]
fn terminal_insert_lines_zero() {
    let mut t = Terminal::init(Global, opts(2, 5)).unwrap();

    // This should do nothing
    t.set_cursor_pos(1, 1);
    t.insert_lines(0);

    t.deinit(Global);
}

// test "Terminal: insertLines with scroll region"
#[test]
fn terminal_insert_lines_with_scroll_region() {
    let mut t = Terminal::init(Global, opts(2, 6)).unwrap();

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

    t.set_top_and_bottom_margin(1, 2);
    t.set_cursor_pos(1, 1);

    t.clear_dirty();
    t.insert_lines(1);

    assert!(t.is_dirty(active_pt(0, 0)));
    assert!(t.is_dirty(active_pt(0, 1)));
    assert!(!t.is_dirty(active_pt(0, 2)));

    t.print('X' as u32).unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "X\nA\nC\nD\nE".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: insertLines more than remaining"
#[test]
fn terminal_insert_lines_more_than_remaining() {
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

    // Insert a bunch of  lines
    t.clear_dirty();
    t.insert_lines(20);

    assert!(!t.is_dirty(active_pt(0, 0)));
    assert!(t.is_dirty(active_pt(0, 1)));
    assert!(t.is_dirty(active_pt(0, 2)));

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "A".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: insertLines resets pending wrap"
#[test]
fn terminal_insert_lines_resets_pending_wrap() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    for c in "ABCDE".chars() {
        t.print(c as u32).unwrap();
    }
    assert!(unsafe { (*t.screens.active).cursor.pending_wrap });
    t.insert_lines(1);
    assert!(!unsafe { (*t.screens.active).cursor.pending_wrap });
    t.print('B' as u32).unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "B\nABCDE".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: insertLines resets wrap"
#[test]
fn terminal_insert_lines_resets_wrap() {
    let mut t = Terminal::init(Global, opts(3, 3)).unwrap();

    t.print('1' as u32).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    for c in "ABCDEF".chars() {
        t.print(c as u32).unwrap();
    }
    t.set_cursor_pos(1, 1);
    t.insert_lines(1);
    t.print('X' as u32).unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "X\n1\nABC".as_bytes());
    }

    {
        let list_cell = unsafe { (*t.screens.active).pages.get_cell(active_pt(0, 2)) }.unwrap();
        let row = list_cell.row;
        assert!(!unsafe { (*row).wrap() });
    }

    t.deinit(Global);
}

// test "Terminal: insertLines left/right scroll region"
#[test]
fn terminal_insert_lines_left_right_scroll_region() {
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
    t.insert_lines(1);

    assert!(!t.is_dirty(active_pt(0, 0)));
    assert!(t.is_dirty(active_pt(0, 1)));
    assert!(t.is_dirty(active_pt(0, 2)));
    assert!(t.is_dirty(active_pt(0, 3)));

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "ABC123\nD   56\nGEF489\n HI7".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: scrollUp simple"
#[test]
fn terminal_scroll_up_simple() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.print_string("ABC".as_bytes()).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print_string("DEF".as_bytes()).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print_string("GHI".as_bytes()).unwrap();
    t.set_cursor_pos(2, 2);

    let cursor_x = unsafe { (*t.screens.active).cursor.x };
    let cursor_y = unsafe { (*t.screens.active).cursor.y };
    let viewport_before = t.active_mut().pages.get_top_left(point::Tag::Viewport);
    t.scroll_up(1).unwrap();
    assert_eq!(cursor_x, unsafe { (*t.screens.active).cursor.x });
    assert_eq!(cursor_y, unsafe { (*t.screens.active).cursor.y });

    // Viewport should have moved. Our entire page should've scrolled!
    // The viewport moving will cause our render state to make the full
    // frame as dirty.
    let viewport_after = t.active_mut().pages.get_top_left(point::Tag::Viewport);
    assert!(!viewport_before.eql(viewport_after));

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "DEF\nGHI".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: scrollUp moves hyperlink"
#[test]
fn terminal_scroll_up_moves_hyperlink() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.print_string("ABC".as_bytes()).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.active_mut()
        .start_hyperlink("http://example.com".as_bytes(), None)
        .unwrap();
    t.print_string("DEF".as_bytes()).unwrap();
    t.active_mut().end_hyperlink();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print_string("GHI".as_bytes()).unwrap();
    t.set_cursor_pos(2, 2);
    t.scroll_up(1).unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "DEF\nGHI".as_bytes());
    }

    for x in 0..3 {
        let list_cell = t.active_mut().pages.get_cell(viewport_pt(x, 0)).unwrap();
        let row = list_cell.row;
        assert!(unsafe { (*row).hyperlink() });
        let cell = list_cell.cell;
        assert!(unsafe { (*cell).hyperlink() });
        let id = unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_hyperlink(cell) }
            .unwrap();
        assert_eq!(id, 1 as crate::hyperlink::Id);
        let page = unsafe { crate::pagelist::Node::page(list_cell.node) };
        assert_eq!(unsafe { (*page).hyperlink_set.count() }, 1);
    }
    for x in 0..3 {
        let list_cell = t.active_mut().pages.get_cell(viewport_pt(x, 1)).unwrap();
        let row = list_cell.row;
        assert!(!unsafe { (*row).hyperlink() });
        let cell = list_cell.cell;
        assert!(!unsafe { (*cell).hyperlink() });
        let id = unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_hyperlink(cell) };
        assert!(id.is_none());
    }

    t.deinit(Global);
}

// test "Terminal: scrollUp clears hyperlink"
#[test]
fn terminal_scroll_up_clears_hyperlink() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.active_mut()
        .start_hyperlink("http://example.com".as_bytes(), None)
        .unwrap();
    t.print_string("ABC".as_bytes()).unwrap();
    t.active_mut().end_hyperlink();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print_string("DEF".as_bytes()).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print_string("GHI".as_bytes()).unwrap();
    t.set_cursor_pos(2, 2);
    t.scroll_up(1).unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "DEF\nGHI".as_bytes());
    }

    for x in 0..3 {
        let list_cell = t.active_mut().pages.get_cell(viewport_pt(x, 0)).unwrap();
        let row = list_cell.row;
        assert!(!unsafe { (*row).hyperlink() });
        let cell = list_cell.cell;
        assert!(!unsafe { (*cell).hyperlink() });
        let id = unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_hyperlink(cell) };
        assert!(id.is_none());
    }

    t.deinit(Global);
}

// test "Terminal: scrollUp top/bottom scroll region"
#[test]
fn terminal_scroll_up_top_bottom_scroll_region() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.print_string("ABC".as_bytes()).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print_string("DEF".as_bytes()).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print_string("GHI".as_bytes()).unwrap();
    t.set_top_and_bottom_margin(2, 3);
    t.set_cursor_pos(1, 1);

    t.clear_dirty();
    t.scroll_up(1).unwrap();

    // This is dirty because the cursor moves from this row
    assert!(t.is_dirty(active_pt(0, 0)));
    assert!(t.is_dirty(active_pt(0, 1)));
    assert!(t.is_dirty(active_pt(0, 2)));

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "ABC\nGHI".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: scrollUp left/right scroll region"
#[test]
fn terminal_scroll_up_left_right_scroll_region() {
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

    let cursor_x = unsafe { (*t.screens.active).cursor.x };
    let cursor_y = unsafe { (*t.screens.active).cursor.y };
    t.clear_dirty();
    t.scroll_up(1).unwrap();
    assert_eq!(cursor_x, unsafe { (*t.screens.active).cursor.x });
    assert_eq!(cursor_y, unsafe { (*t.screens.active).cursor.y });

    assert!(t.is_dirty(active_pt(0, 0)));
    assert!(t.is_dirty(active_pt(0, 1)));
    assert!(t.is_dirty(active_pt(0, 2)));

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "AEF423\nDHI756\nG   89".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: scrollUp left/right scroll region hyperlink"
#[test]
fn terminal_scroll_up_left_right_scroll_region_hyperlink() {
    let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

    t.print_string("ABC123".as_bytes()).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.active_mut()
        .start_hyperlink("http://example.com".as_bytes(), None)
        .unwrap();
    t.print_string("DEF456".as_bytes()).unwrap();
    t.active_mut().end_hyperlink();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print_string("GHI789".as_bytes()).unwrap();
    t.scrolling_region.left = 1;
    t.scrolling_region.right = 3;
    t.set_cursor_pos(2, 2);
    t.scroll_up(1).unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "AEF423\nDHI756\nG   89".as_bytes());
    }

    // First row gets some hyperlinks
    {
        for x in 0..1 {
            let list_cell = t.active_mut().pages.get_cell(viewport_pt(x, 0)).unwrap();
            let cell = list_cell.cell;
            assert!(!unsafe { (*cell).hyperlink() });
            let id =
                unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_hyperlink(cell) };
            assert!(id.is_none());
        }
        for x in 1..4 {
            let list_cell = t.active_mut().pages.get_cell(viewport_pt(x, 0)).unwrap();
            let row = list_cell.row;
            assert!(unsafe { (*row).hyperlink() });
            let cell = list_cell.cell;
            assert!(unsafe { (*cell).hyperlink() });
            let id =
                unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_hyperlink(cell) }
                    .unwrap();
            assert_eq!(id, 1 as crate::hyperlink::Id);
            let page = unsafe { crate::pagelist::Node::page(list_cell.node) };
            assert_eq!(unsafe { (*page).hyperlink_set.count() }, 1);
        }
        for x in 4..6 {
            let list_cell = t.active_mut().pages.get_cell(viewport_pt(x, 0)).unwrap();
            let cell = list_cell.cell;
            assert!(!unsafe { (*cell).hyperlink() });
            let id =
                unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_hyperlink(cell) };
            assert!(id.is_none());
        }
    }

    // Second row preserves hyperlink where we didn't scroll
    {
        for x in 0..1 {
            let list_cell = t.active_mut().pages.get_cell(viewport_pt(x, 1)).unwrap();
            let row = list_cell.row;
            assert!(unsafe { (*row).hyperlink() });
            let cell = list_cell.cell;
            assert!(unsafe { (*cell).hyperlink() });
            let id =
                unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_hyperlink(cell) }
                    .unwrap();
            assert_eq!(id, 1 as crate::hyperlink::Id);
            let page = unsafe { crate::pagelist::Node::page(list_cell.node) };
            assert_eq!(unsafe { (*page).hyperlink_set.count() }, 1);
        }
        for x in 1..4 {
            let list_cell = t.active_mut().pages.get_cell(viewport_pt(x, 1)).unwrap();
            let cell = list_cell.cell;
            assert!(!unsafe { (*cell).hyperlink() });
            let id =
                unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_hyperlink(cell) };
            assert!(id.is_none());
        }
        for x in 4..6 {
            let list_cell = t.active_mut().pages.get_cell(viewport_pt(x, 1)).unwrap();
            let row = list_cell.row;
            assert!(unsafe { (*row).hyperlink() });
            let cell = list_cell.cell;
            assert!(unsafe { (*cell).hyperlink() });
            let id =
                unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_hyperlink(cell) }
                    .unwrap();
            assert_eq!(id, 1 as crate::hyperlink::Id);
            let page = unsafe { crate::pagelist::Node::page(list_cell.node) };
            assert_eq!(unsafe { (*page).hyperlink_set.count() }, 1);
        }
    }

    t.deinit(Global);
}

// test "Terminal: scrollUp preserves pending wrap"
#[test]
fn terminal_scroll_up_preserves_pending_wrap() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.set_cursor_pos(1, 5);
    t.print('A' as u32).unwrap();
    t.set_cursor_pos(2, 5);
    t.print('B' as u32).unwrap();
    t.set_cursor_pos(3, 5);
    t.print('C' as u32).unwrap();
    t.scroll_up(1).unwrap();
    t.print('X' as u32).unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "    B\n    C\n\nX".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: scrollUp full top/bottom region"
#[test]
fn terminal_scroll_up_full_top_bottom_region() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.print_string("top".as_bytes()).unwrap();
    t.set_cursor_pos(5, 1);
    t.print_string("ABCDE".as_bytes()).unwrap();
    t.set_top_and_bottom_margin(2, 5);

    t.clear_dirty();
    t.scroll_up(4).unwrap();

    // This is dirty because the cursor moves from this row
    assert!(t.is_dirty(active_pt(0, 0)));
    assert!(t.is_dirty(active_pt(0, 1)));

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "top".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: scrollUp full top/bottomleft/right scroll region"
#[test]
fn terminal_scroll_up_full_top_bottomleft_right_scroll_region() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.print_string("top".as_bytes()).unwrap();
    t.set_cursor_pos(5, 1);
    t.print_string("ABCDE".as_bytes()).unwrap();
    t.modes.set(modespkg::Mode::EnableLeftAndRightMargin, true);
    t.set_top_and_bottom_margin(2, 5);
    t.set_left_and_right_margin(2, 4);

    t.clear_dirty();
    t.scroll_up(4).unwrap();

    // This is dirty because the cursor moves from this row
    assert!(t.is_dirty(active_pt(0, 0)));
    for y in 1..5u32 {
        assert!(t.is_dirty(active_pt(0, y)));
    }

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "top\n\n\n\nA   E".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: scrollUp creates scrollback in primary screen"
#[test]
fn terminal_scroll_up_creates_scrollback_in_primary_screen() {
    // When in primary screen with full-width scroll region at top,
    // scrollUp (CSI S) should push lines into scrollback like xterm.
    let mut t = Terminal::init(Global, opts_sb(5, 5, 10)).unwrap();

    // Fill the screen with content
    t.print_string("AAAAA".as_bytes()).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print_string("BBBBB".as_bytes()).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print_string("CCCCC".as_bytes()).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print_string("DDDDD".as_bytes()).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print_string("EEEEE".as_bytes()).unwrap();

    t.clear_dirty();

    // Scroll up by 1, which should push "AAAAA" into scrollback
    t.scroll_up(1).unwrap();

    // The cursor row (new empty row) should be dirty
    assert!(unsafe { (*(*t.screens.active).cursor.page_row).dirty() });

    // The active screen should now show BBBBB through EEEEE plus one blank line
    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "BBBBB\nCCCCC\nDDDDD\nEEEEE".as_bytes());
    }

    // Now scroll to the top to see scrollback - AAAAA should be there
    t.active_mut().scroll(crate::screen::Scroll::Top);
    {
        let str = t.plain_string(Global).unwrap();
        // Should see AAAAA in scrollback
        assert_eq!(
            str.as_slice(),
            "AAAAA\nBBBBB\nCCCCC\nDDDDD\nEEEEE".as_bytes()
        );
    }

    t.deinit(Global);
}

// test "Terminal: scrollUp with max_scrollback_bytes zero"
#[test]
fn terminal_scroll_up_with_max_scrollback_bytes_zero() {
    // When max_scrollback_bytes is 0, scrollUp should still work but not retain history
    let mut t = Terminal::init(Global, opts_sb(5, 5, 0)).unwrap();

    t.print_string("AAAAA".as_bytes()).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print_string("BBBBB".as_bytes()).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print_string("CCCCC".as_bytes()).unwrap();

    t.scroll_up(1).unwrap();

    // Active screen should show scrolled content
    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "BBBBB\nCCCCC".as_bytes());
    }

    // Scroll to top - should be same as active since no scrollback
    t.active_mut().scroll(crate::screen::Scroll::Top);
    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "BBBBB\nCCCCC".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: scrollUp with max_scrollback_bytes zero and top margin"
#[test]
fn terminal_scroll_up_with_max_scrollback_bytes_zero_and_top_margin() {
    // When max_scrollback_bytes is 0 and top margin is set, should use deleteLines path
    let mut t = Terminal::init(Global, opts_sb(5, 5, 0)).unwrap();

    t.print_string("AAAAA".as_bytes()).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print_string("BBBBB".as_bytes()).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print_string("CCCCC".as_bytes()).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print_string("DDDDD".as_bytes()).unwrap();

    // Set top margin (not at row 0)
    t.set_top_and_bottom_margin(2, 5);

    t.scroll_up(1).unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        // First row preserved, rest scrolled
        assert_eq!(str.as_slice(), "AAAAA\nCCCCC\nDDDDD".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: scrollUp with max_scrollback_bytes zero and left/right margin"
#[test]
fn terminal_scroll_up_with_max_scrollback_bytes_zero_and_left_right_margin() {
    // When max_scrollback_bytes is 0 with left/right margins, uses deleteLines path
    let mut t = Terminal::init(Global, opts_sb(10, 5, 0)).unwrap();

    t.print_string("AAAAABBBBB".as_bytes()).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print_string("CCCCCDDDDD".as_bytes()).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print_string("EEEEEFFFFF".as_bytes()).unwrap();

    // Set left/right margins (columns 2-6, 1-indexed = indices 1-5)
    t.modes.set(modespkg::Mode::EnableLeftAndRightMargin, true);
    t.set_left_and_right_margin(2, 6);

    t.scroll_up(1).unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        // cols 1-5 scroll, col 0 and cols 6+ preserved
        assert_eq!(
            str.as_slice(),
            "ACCCCDBBBB\nCEEEEFDDDD\nE     FFFF".as_bytes()
        );
    }

    t.deinit(Global);
}

// test "Terminal: scrollDown simple"
#[test]
fn terminal_scroll_down_simple() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.print_string("ABC".as_bytes()).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print_string("DEF".as_bytes()).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print_string("GHI".as_bytes()).unwrap();
    t.set_cursor_pos(2, 2);

    let cursor_x = unsafe { (*t.screens.active).cursor.x };
    let cursor_y = unsafe { (*t.screens.active).cursor.y };
    t.clear_dirty();
    t.scroll_down(1);
    assert_eq!(cursor_x, unsafe { (*t.screens.active).cursor.x });
    assert_eq!(cursor_y, unsafe { (*t.screens.active).cursor.y });

    for y in 0..5u32 {
        assert!(t.is_dirty(active_pt(0, y)));
    }

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "\nABC\nDEF\nGHI".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: scrollDown hyperlink moves"
#[test]
fn terminal_scroll_down_hyperlink_moves() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.active_mut()
        .start_hyperlink("http://example.com".as_bytes(), None)
        .unwrap();
    t.print_string("ABC".as_bytes()).unwrap();
    t.active_mut().end_hyperlink();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print_string("DEF".as_bytes()).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print_string("GHI".as_bytes()).unwrap();
    t.set_cursor_pos(2, 2);
    t.scroll_down(1);

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "\nABC\nDEF\nGHI".as_bytes());
    }

    for x in 0..3 {
        let list_cell = t.active_mut().pages.get_cell(viewport_pt(x, 1)).unwrap();
        let row = list_cell.row;
        assert!(unsafe { (*row).hyperlink() });
        let cell = list_cell.cell;
        assert!(unsafe { (*cell).hyperlink() });
        let id = unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_hyperlink(cell) }
            .unwrap();
        assert_eq!(id, 1 as crate::hyperlink::Id);
        let page = unsafe { crate::pagelist::Node::page(list_cell.node) };
        assert_eq!(unsafe { (*page).hyperlink_set.count() }, 1);
    }
    for x in 0..3 {
        let list_cell = t.active_mut().pages.get_cell(viewport_pt(x, 0)).unwrap();
        let row = list_cell.row;
        assert!(!unsafe { (*row).hyperlink() });
        let cell = list_cell.cell;
        assert!(!unsafe { (*cell).hyperlink() });
        let id = unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_hyperlink(cell) };
        assert!(id.is_none());
    }

    t.deinit(Global);
}

// test "Terminal: scrollDown outside of scroll region"
#[test]
fn terminal_scroll_down_outside_of_scroll_region() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.print_string("ABC".as_bytes()).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print_string("DEF".as_bytes()).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print_string("GHI".as_bytes()).unwrap();
    t.set_top_and_bottom_margin(3, 4);
    t.set_cursor_pos(2, 2);

    let cursor_x = unsafe { (*t.screens.active).cursor.x };
    let cursor_y = unsafe { (*t.screens.active).cursor.y };
    t.clear_dirty();
    t.scroll_down(1);
    assert_eq!(cursor_x, unsafe { (*t.screens.active).cursor.x });
    assert_eq!(cursor_y, unsafe { (*t.screens.active).cursor.y });

    assert!(!t.is_dirty(active_pt(0, 0)));

    // This is dirty because the cursor moves from this row
    assert!(t.is_dirty(active_pt(0, 1)));
    assert!(t.is_dirty(active_pt(0, 2)));
    assert!(t.is_dirty(active_pt(0, 3)));

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "ABC\nDEF\n\nGHI".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: scrollDown left/right scroll region"
#[test]
fn terminal_scroll_down_left_right_scroll_region() {
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

    let cursor_x = unsafe { (*t.screens.active).cursor.x };
    let cursor_y = unsafe { (*t.screens.active).cursor.y };
    t.clear_dirty();
    t.scroll_down(1);
    assert_eq!(cursor_x, unsafe { (*t.screens.active).cursor.x });
    assert_eq!(cursor_y, unsafe { (*t.screens.active).cursor.y });

    for y in 0..4u32 {
        assert!(t.is_dirty(active_pt(0, y)));
    }

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "A   23\nDBC156\nGEF489\n HI7".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: scrollDown left/right scroll region hyperlink"
#[test]
fn terminal_scroll_down_left_right_scroll_region_hyperlink() {
    let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

    t.active_mut()
        .start_hyperlink("http://example.com".as_bytes(), None)
        .unwrap();
    t.print_string("ABC123".as_bytes()).unwrap();
    t.active_mut().end_hyperlink();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print_string("DEF456".as_bytes()).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print_string("GHI789".as_bytes()).unwrap();
    t.scrolling_region.left = 1;
    t.scrolling_region.right = 3;
    t.set_cursor_pos(2, 2);
    t.scroll_down(1);

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "A   23\nDBC156\nGEF489\n HI7".as_bytes());
    }

    // First row preserves hyperlink where we didn't scroll
    {
        for x in 0..1 {
            let list_cell = t.active_mut().pages.get_cell(viewport_pt(x, 0)).unwrap();
            let row = list_cell.row;
            assert!(unsafe { (*row).hyperlink() });
            let cell = list_cell.cell;
            assert!(unsafe { (*cell).hyperlink() });
            let id =
                unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_hyperlink(cell) }
                    .unwrap();
            assert_eq!(id, 1 as crate::hyperlink::Id);
            let page = unsafe { crate::pagelist::Node::page(list_cell.node) };
            assert_eq!(unsafe { (*page).hyperlink_set.count() }, 1);
        }
        for x in 1..4 {
            let list_cell = t.active_mut().pages.get_cell(viewport_pt(x, 0)).unwrap();
            let cell = list_cell.cell;
            assert!(!unsafe { (*cell).hyperlink() });
            let id =
                unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_hyperlink(cell) };
            assert!(id.is_none());
        }
        for x in 4..6 {
            let list_cell = t.active_mut().pages.get_cell(viewport_pt(x, 0)).unwrap();
            let row = list_cell.row;
            assert!(unsafe { (*row).hyperlink() });
            let cell = list_cell.cell;
            assert!(unsafe { (*cell).hyperlink() });
            let id =
                unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_hyperlink(cell) }
                    .unwrap();
            assert_eq!(id, 1 as crate::hyperlink::Id);
            let page = unsafe { crate::pagelist::Node::page(list_cell.node) };
            assert_eq!(unsafe { (*page).hyperlink_set.count() }, 1);
        }
    }

    // Second row gets some hyperlinks
    {
        for x in 0..1 {
            let list_cell = t.active_mut().pages.get_cell(viewport_pt(x, 1)).unwrap();
            let cell = list_cell.cell;
            assert!(!unsafe { (*cell).hyperlink() });
            let id =
                unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_hyperlink(cell) };
            assert!(id.is_none());
        }
        for x in 1..4 {
            let list_cell = t.active_mut().pages.get_cell(viewport_pt(x, 1)).unwrap();
            let row = list_cell.row;
            assert!(unsafe { (*row).hyperlink() });
            let cell = list_cell.cell;
            assert!(unsafe { (*cell).hyperlink() });
            let id =
                unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_hyperlink(cell) }
                    .unwrap();
            assert_eq!(id, 1 as crate::hyperlink::Id);
            let page = unsafe { crate::pagelist::Node::page(list_cell.node) };
            assert_eq!(unsafe { (*page).hyperlink_set.count() }, 1);
        }
        for x in 4..6 {
            let list_cell = t.active_mut().pages.get_cell(viewport_pt(x, 1)).unwrap();
            let cell = list_cell.cell;
            assert!(!unsafe { (*cell).hyperlink() });
            let id =
                unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_hyperlink(cell) };
            assert!(id.is_none());
        }
    }

    t.deinit(Global);
}

// test "Terminal: scrollDown outside of left/right scroll region"
#[test]
fn terminal_scroll_down_outside_of_left_right_scroll_region() {
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
    t.set_cursor_pos(1, 1);

    let cursor_x = unsafe { (*t.screens.active).cursor.x };
    let cursor_y = unsafe { (*t.screens.active).cursor.y };
    t.clear_dirty();
    t.scroll_down(1);
    assert_eq!(cursor_x, unsafe { (*t.screens.active).cursor.x });
    assert_eq!(cursor_y, unsafe { (*t.screens.active).cursor.y });

    for y in 0..4u32 {
        assert!(t.is_dirty(active_pt(0, y)));
    }

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "A   23\nDBC156\nGEF489\n HI7".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: scrollDown preserves pending wrap"
#[test]
fn terminal_scroll_down_preserves_pending_wrap() {
    let mut t = Terminal::init(Global, opts(5, 10)).unwrap();

    t.set_cursor_pos(1, 5);
    t.print('A' as u32).unwrap();
    t.set_cursor_pos(2, 5);
    t.print('B' as u32).unwrap();
    t.set_cursor_pos(3, 5);
    t.print('C' as u32).unwrap();
    t.scroll_down(1);
    t.print('X' as u32).unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "\n    A\n    B\nX   C".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: decaln reset margins"
#[test]
fn terminal_decaln_reset_margins() {
    let mut t = Terminal::init(Global, opts(3, 3)).unwrap();

    // Initial value
    t.modes.set(modespkg::Mode::Origin, true);
    t.set_top_and_bottom_margin(2, 3);
    t.decaln().unwrap();
    t.scroll_down(1);

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "\nEEE\nEEE".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: decaln preserves color"
#[test]
fn terminal_decaln_preserves_color() {
    let mut t = Terminal::init(Global, opts(3, 3)).unwrap();

    // Initial value
    t.set_attribute(sgr::Attribute::DirectColorBg(color::RGB {
        r: 0xFF,
        g: 0,
        b: 0,
    }))
    .unwrap();
    t.modes.set(modespkg::Mode::Origin, true);
    t.set_top_and_bottom_margin(2, 3);
    t.decaln().unwrap();
    t.scroll_down(1);

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "\nEEE\nEEE".as_bytes());
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

// test "Terminal: DECALN resets graphemes with protected mode"
#[test]
fn terminal_decaln_resets_graphemes_with_protected_mode() {
    let mut t = Terminal::init(Global, opts(3, 3)).unwrap();

    // Add protected mode. A previous version of DECALN accidentally preserved
    // protected mode which left dangling managed memory.
    t.set_protected_mode(ansi::ProtectedMode::Iso);

    // This is: 👨‍👩‍👧 (which may or may not render correctly)
    t.modes.set(modespkg::Mode::GraphemeCluster, true);
    t.print(0x1F468).unwrap();
    t.print(0x200D).unwrap();
    t.print(0x1F469).unwrap();
    t.print(0x200D).unwrap();
    t.print(0x1F467).unwrap();

    t.decaln().unwrap();

    assert_eq!(0usize, unsafe { (*t.screens.active).cursor.y } as usize);
    assert_eq!(0usize, unsafe { (*t.screens.active).cursor.x } as usize);
    assert!(unsafe { (*t.screens.active).cursor.protected });
    assert!(unsafe { (*t.screens.active).protected_mode } == ansi::ProtectedMode::Iso);

    for y in 0..t.rows {
        assert!(t.is_dirty(active_pt(0, y as u32)));
    }

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "EEE\nEEE\nEEE".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: reverseIndex"
#[test]
fn terminal_reverse_index() {
    let mut t = Terminal::init(Global, opts(2, 5)).unwrap();

    // Initial value
    t.print('A' as u32).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print('B' as u32).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print('C' as u32).unwrap();
    t.reverse_index();
    t.print('D' as u32).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.carriage_return();
    t.linefeed().unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "A\nBD\nC".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: reverseIndex from the top"
#[test]
fn terminal_reverse_index_from_the_top() {
    let mut t = Terminal::init(Global, opts(2, 5)).unwrap();

    t.print('A' as u32).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print('B' as u32).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.carriage_return();
    t.linefeed().unwrap();

    t.set_cursor_pos(1, 1);
    t.reverse_index();
    t.print('D' as u32).unwrap();

    t.carriage_return();
    t.linefeed().unwrap();
    t.set_cursor_pos(1, 1);
    t.reverse_index();
    t.print('E' as u32).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "E\nD\nA\nB".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: reverseIndex top of scrolling region"
#[test]
fn terminal_reverse_index_top_of_scrolling_region() {
    let mut t = Terminal::init(Global, opts(2, 10)).unwrap();

    // Initial value
    t.set_cursor_pos(2, 1);
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

    // Set our scroll region
    t.set_top_and_bottom_margin(2, 5);
    t.set_cursor_pos(2, 1);
    t.reverse_index();
    t.print('X' as u32).unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "\nX\nA\nB\nC".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: reverseIndex top of screen"
#[test]
fn terminal_reverse_index_top_of_screen() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.print('A' as u32).unwrap();
    t.set_cursor_pos(2, 1);
    t.print('B' as u32).unwrap();
    t.set_cursor_pos(3, 1);
    t.print('C' as u32).unwrap();
    t.set_cursor_pos(1, 1);
    t.reverse_index();
    t.print('X' as u32).unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "X\nA\nB\nC".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: reverseIndex not top of screen"
#[test]
fn terminal_reverse_index_not_top_of_screen() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.print('A' as u32).unwrap();
    t.set_cursor_pos(2, 1);
    t.print('B' as u32).unwrap();
    t.set_cursor_pos(3, 1);
    t.print('C' as u32).unwrap();
    t.set_cursor_pos(2, 1);
    t.reverse_index();
    t.print('X' as u32).unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "X\nB\nC".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: reverseIndex top/bottom margins"
#[test]
fn terminal_reverse_index_top_bottom_margins() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.print('A' as u32).unwrap();
    t.set_cursor_pos(2, 1);
    t.print('B' as u32).unwrap();
    t.set_cursor_pos(3, 1);
    t.print('C' as u32).unwrap();
    t.set_top_and_bottom_margin(2, 3);
    t.set_cursor_pos(2, 1);
    t.reverse_index();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "A\n\nB".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: reverseIndex outside top/bottom margins"
#[test]
fn terminal_reverse_index_outside_top_bottom_margins() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.print('A' as u32).unwrap();
    t.set_cursor_pos(2, 1);
    t.print('B' as u32).unwrap();
    t.set_cursor_pos(3, 1);
    t.print('C' as u32).unwrap();
    t.set_top_and_bottom_margin(2, 3);
    t.set_cursor_pos(1, 1);
    t.reverse_index();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "A\nB\nC".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: reverseIndex left/right margins"
#[test]
fn terminal_reverse_index_left_right_margins() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.print_string("ABC".as_bytes()).unwrap();
    t.set_cursor_pos(2, 1);
    t.print_string("DEF".as_bytes()).unwrap();
    t.set_cursor_pos(3, 1);
    t.print_string("GHI".as_bytes()).unwrap();
    t.modes.set(modespkg::Mode::EnableLeftAndRightMargin, true);
    t.set_left_and_right_margin(2, 3);
    t.set_cursor_pos(1, 2);
    t.reverse_index();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "A\nDBC\nGEF\n HI".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: reverseIndex outside left/right margins"
#[test]
fn terminal_reverse_index_outside_left_right_margins() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.print_string("ABC".as_bytes()).unwrap();
    t.set_cursor_pos(2, 1);
    t.print_string("DEF".as_bytes()).unwrap();
    t.set_cursor_pos(3, 1);
    t.print_string("GHI".as_bytes()).unwrap();
    t.modes.set(modespkg::Mode::EnableLeftAndRightMargin, true);
    t.set_left_and_right_margin(2, 3);
    t.set_cursor_pos(1, 1);
    t.reverse_index();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "ABC\nDEF\nGHI".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: index"
#[test]
fn terminal_index() {
    let mut t = Terminal::init(Global, opts(2, 5)).unwrap();

    t.index().unwrap();
    t.print('A' as u32).unwrap();

    assert!(t.is_dirty(active_pt(0, 0)));
    assert!(t.is_dirty(active_pt(0, 1)));

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "\nA".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: index from the bottom"
#[test]
fn terminal_index_from_the_bottom() {
    let mut t = Terminal::init(Global, opts(2, 5)).unwrap();

    t.set_cursor_pos(5, 1);
    t.print('A' as u32).unwrap();
    t.cursor_left(1); // undo moving right from 'A'

    t.clear_dirty();
    t.index().unwrap();
    t.print('B' as u32).unwrap();

    assert!(t.is_dirty(active_pt(0, 3)));
    assert!(t.is_dirty(active_pt(0, 4)));

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "\n\n\nA\nB".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: index outside of scrolling region"
#[test]
fn terminal_index_outside_of_scrolling_region() {
    let mut t = Terminal::init(Global, opts(2, 5)).unwrap();

    assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
    t.set_top_and_bottom_margin(2, 5);
    t.index().unwrap();
    assert_eq!(unsafe { (*t.screens.active).cursor.y }, 1);

    t.deinit(Global);
}

// test "Terminal: index from the bottom outside of scroll region"
#[test]
fn terminal_index_from_the_bottom_outside_of_scroll_region() {
    let mut t = Terminal::init(Global, opts(2, 5)).unwrap();

    t.set_top_and_bottom_margin(1, 2);
    t.set_cursor_pos(5, 1);
    t.print('A' as u32).unwrap();
    t.clear_dirty();
    t.index().unwrap();
    t.print('B' as u32).unwrap();
    assert!(t.is_dirty(active_pt(0, 4)));

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "\n\n\n\nAB".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: index no scroll region, top of screen"
#[test]
fn terminal_index_no_scroll_region_top_of_screen() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.print('A' as u32).unwrap();
    t.clear_dirty();
    t.index().unwrap();
    t.print('X' as u32).unwrap();

    assert!(t.is_dirty(active_pt(0, 0)));
    assert!(t.is_dirty(active_pt(0, 1)));

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "A\n X".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: index scrolling with hyperlink"
#[test]
fn terminal_index_scrolling_with_hyperlink() {
    let mut t = Terminal::init(Global, opts(2, 5)).unwrap();

    t.set_cursor_pos(5, 1);
    t.active_mut()
        .start_hyperlink("http://example.com".as_bytes(), None)
        .unwrap();
    t.print('A' as u32).unwrap();
    t.active_mut().end_hyperlink();
    t.cursor_left(1); // undo moving right from 'A'
    t.index().unwrap();
    t.print('B' as u32).unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "\n\n\nA\nB".as_bytes());
    }

    {
        let list_cell = t.active_mut().pages.get_cell(viewport_pt(0, 3)).unwrap();
        let row = list_cell.row;
        assert!(unsafe { (*row).hyperlink() });
        let cell = list_cell.cell;
        assert!(unsafe { (*cell).hyperlink() });
        let id = unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_hyperlink(cell) }
            .unwrap();
        assert_eq!(id, 1 as crate::hyperlink::Id);
    }
    {
        let list_cell = t.active_mut().pages.get_cell(viewport_pt(0, 4)).unwrap();
        let row = list_cell.row;
        assert!(!unsafe { (*row).hyperlink() });
        let cell = list_cell.cell;
        assert!(!unsafe { (*cell).hyperlink() });
        let id = unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_hyperlink(cell) };
        assert!(id.is_none());
    }

    t.deinit(Global);
}

// test "Terminal: index bottom of primary screen"
#[test]
fn terminal_index_bottom_of_primary_screen() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.set_cursor_pos(5, 1);
    t.print('A' as u32).unwrap();
    t.clear_dirty();
    t.index().unwrap();
    t.print('X' as u32).unwrap();

    assert!(t.is_dirty(active_pt(0, 3)));
    assert!(t.is_dirty(active_pt(0, 4)));

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "\n\n\nA\n X".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: index bottom of primary screen background sgr"
#[test]
fn terminal_index_bottom_of_primary_screen_background_sgr() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.set_cursor_pos(5, 1);
    t.print('A' as u32).unwrap();
    t.set_attribute(sgr::Attribute::DirectColorBg(color::RGB {
        r: 0xFF,
        g: 0,
        b: 0,
    }))
    .unwrap();
    t.index().unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "\n\n\nA".as_bytes());
        for x in 0..5 {
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
    }

    t.deinit(Global);
}

// test "Terminal: index inside scroll region"
#[test]
fn terminal_index_inside_scroll_region() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.set_top_and_bottom_margin(1, 3);
    t.print('A' as u32).unwrap();
    t.clear_dirty();
    t.index().unwrap();
    t.print('X' as u32).unwrap();

    assert!(t.is_dirty(active_pt(0, 0)));
    assert!(t.is_dirty(active_pt(0, 1)));

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "A\n X".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: index bottom of scroll region with hyperlinks"
#[test]
fn terminal_index_bottom_of_scroll_region_with_hyperlinks() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.set_top_and_bottom_margin(1, 2);
    t.print('A' as u32).unwrap();
    t.index().unwrap();
    t.carriage_return();
    t.active_mut()
        .start_hyperlink("http://example.com".as_bytes(), None)
        .unwrap();
    t.print('B' as u32).unwrap();
    t.active_mut().end_hyperlink();
    t.index().unwrap();
    t.carriage_return();
    t.print('C' as u32).unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "B\nC".as_bytes());
    }

    {
        let list_cell = t.active_mut().pages.get_cell(viewport_pt(0, 0)).unwrap();
        let row = list_cell.row;
        assert!(unsafe { (*row).hyperlink() });
        let cell = list_cell.cell;
        assert!(unsafe { (*cell).hyperlink() });
        let id = unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_hyperlink(cell) }
            .unwrap();
        assert_eq!(id, 1 as crate::hyperlink::Id);
    }
    {
        let list_cell = t.active_mut().pages.get_cell(viewport_pt(0, 1)).unwrap();
        let row = list_cell.row;
        assert!(!unsafe { (*row).hyperlink() });
        let cell = list_cell.cell;
        assert!(!unsafe { (*cell).hyperlink() });
        let id = unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_hyperlink(cell) };
        assert!(id.is_none());
    }

    t.deinit(Global);
}

// test "Terminal: index bottom of scroll region clear hyperlinks"
#[test]
fn terminal_index_bottom_of_scroll_region_clear_hyperlinks() {
    let mut t = Terminal::init(Global, opts_sb(5, 5, 0)).unwrap();

    t.set_top_and_bottom_margin(2, 3);
    t.set_cursor_pos(2, 1);
    t.active_mut()
        .start_hyperlink("http://example.com".as_bytes(), None)
        .unwrap();
    t.print('A' as u32).unwrap();
    t.active_mut().end_hyperlink();
    t.index().unwrap();
    t.carriage_return();
    t.print('B' as u32).unwrap();
    t.index().unwrap();
    t.carriage_return();
    t.print('C' as u32).unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "\nB\nC".as_bytes());
    }

    for y in 1..3 {
        let list_cell = t.active_mut().pages.get_cell(viewport_pt(0, y)).unwrap();
        let row = list_cell.row;
        assert!(!unsafe { (*row).hyperlink() });
        let cell = list_cell.cell;
        assert!(!unsafe { (*cell).hyperlink() });
        let id = unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_hyperlink(cell) };
        assert!(id.is_none());
        let page = unsafe { crate::pagelist::Node::page(list_cell.node) };
        assert_eq!(unsafe { (*page).hyperlink_set.count() }, 0);
    }

    t.deinit(Global);
}

// test "Terminal: index bottom of scroll region with background SGR"
#[test]
fn terminal_index_bottom_of_scroll_region_with_background_sgr() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.set_top_and_bottom_margin(1, 3);
    t.set_cursor_pos(4, 1);
    t.print('B' as u32).unwrap();
    t.set_cursor_pos(3, 1);
    t.print('A' as u32).unwrap();
    t.set_attribute(sgr::Attribute::DirectColorBg(color::RGB {
        r: 0xFF,
        g: 0,
        b: 0,
    }))
    .unwrap();
    t.index().unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "\nA\n\nB".as_bytes());
    }

    for x in 0..t.cols {
        let list_cell = t.active_mut().pages.get_cell(active_pt(x, 2)).unwrap();
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

// test "Terminal: index bottom of primary screen with scroll region"
#[test]
fn terminal_index_bottom_of_primary_screen_with_scroll_region() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.set_top_and_bottom_margin(1, 3);
    t.set_cursor_pos(3, 1);
    t.print('A' as u32).unwrap();
    t.set_cursor_pos(5, 1);
    t.clear_dirty();
    t.index().unwrap();
    t.index().unwrap();
    t.index().unwrap();
    t.print('X' as u32).unwrap();

    for y in 0..4u32 {
        assert!(!t.is_dirty(active_pt(0, y)));
    }
    assert!(t.is_dirty(active_pt(0, 4)));

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "\n\nA\n\nX".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: index outside left/right margin"
#[test]
fn terminal_index_outside_left_right_margin() {
    let mut t = Terminal::init(Global, opts(10, 5)).unwrap();

    t.set_top_and_bottom_margin(1, 3);
    t.scrolling_region.left = 3;
    t.scrolling_region.right = 5;
    t.set_cursor_pos(3, 3);
    t.print('A' as u32).unwrap();
    t.set_cursor_pos(3, 1);
    t.clear_dirty();
    t.index().unwrap();
    t.print('X' as u32).unwrap();

    assert!(t.is_dirty(active_pt(0, 2)));

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "\n\nX A".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: index inside left/right margin"
#[test]
fn terminal_index_inside_left_right_margin() {
    let mut t = Terminal::init(Global, opts(10, 5)).unwrap();

    t.print_string("AAAAAA".as_bytes()).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print_string("AAAAAA".as_bytes()).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print_string("AAAAAA".as_bytes()).unwrap();
    t.modes.set(modespkg::Mode::EnableLeftAndRightMargin, true);
    t.set_top_and_bottom_margin(1, 3);
    t.set_left_and_right_margin(1, 3);
    t.set_cursor_pos(3, 1);

    t.clear_dirty();
    t.index().unwrap();

    assert!(t.is_dirty(active_pt(0, 0)));
    assert!(t.is_dirty(active_pt(0, 1)));
    assert!(t.is_dirty(active_pt(0, 2)));

    assert_eq!(unsafe { (*t.screens.active).cursor.y }, 2);
    assert_eq!(unsafe { (*t.screens.active).cursor.x }, 0);

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "AAAAAA\nAAAAAA\n   AAA".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: index bottom of scroll region creates scrollback"
#[test]
fn terminal_index_bottom_of_scroll_region_creates_scrollback() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.set_top_and_bottom_margin(1, 3);
    t.print_string("1\n2\n3".as_bytes()).unwrap();
    t.set_cursor_pos(4, 1);
    t.print('X' as u32).unwrap();
    t.set_cursor_pos(3, 1);
    t.index().unwrap();
    t.print('Y' as u32).unwrap();

    {
        let str = t
            .active_mut()
            .dump_string_alloc(Global, viewport_pt(0, 0))
            .unwrap();
        assert_eq!(str.as_slice(), "2\n3\nY\nX".as_bytes());
    }
    {
        let str = t
            .active_mut()
            .dump_string_alloc(Global, screen_pt(0, 0))
            .unwrap();
        assert_eq!(str.as_slice(), "1\n2\n3\nY\nX".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: index bottom of scroll region no scrollback"
#[test]
fn terminal_index_bottom_of_scroll_region_no_scrollback() {
    let mut t = Terminal::init(Global, opts_sb(5, 5, 0)).unwrap();

    t.set_top_and_bottom_margin(1, 3);
    t.set_cursor_pos(4, 1);
    t.print('B' as u32).unwrap();
    t.set_cursor_pos(3, 1);
    t.print('A' as u32).unwrap();
    t.clear_dirty();
    t.index().unwrap();
    t.print('X' as u32).unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "\nA\n X\nB".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: index bottom of scroll region blank line preserves SGR"
#[test]
fn terminal_index_bottom_of_scroll_region_blank_line_preserves_sgr() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.set_top_and_bottom_margin(1, 3);
    t.print_string("1\n2\n3".as_bytes()).unwrap();
    t.set_cursor_pos(4, 1);
    t.print('X' as u32).unwrap();
    t.set_cursor_pos(3, 1);
    t.set_attribute(sgr::Attribute::DirectColorBg(color::RGB {
        r: 0xFF,
        g: 0,
        b: 0,
    }))
    .unwrap();
    t.index().unwrap();

    {
        let str = t
            .active_mut()
            .dump_string_alloc(Global, viewport_pt(0, 0))
            .unwrap();
        assert_eq!(str.as_slice(), "2\n3\n\nX".as_bytes());
    }
    {
        let str = t
            .active_mut()
            .dump_string_alloc(Global, screen_pt(0, 0))
            .unwrap();
        assert_eq!(str.as_slice(), "1\n2\n3\n\nX".as_bytes());
    }
    for x in 0..t.cols {
        let list_cell = t.active_mut().pages.get_cell(active_pt(x, 2)).unwrap();
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

// -- oracle 77190bd02: index/scrollUp scroll-region routing (`P3-W5 T3`) --

// test "Terminal: index bottom of scroll region with top margin and background SGR"
#[test]
fn terminal_index_bottom_of_scroll_region_with_top_margin_and_background_sgr() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.print_string("1\n2\n3\n4\n5".as_bytes()).unwrap();
    t.set_top_and_bottom_margin(2, 4);
    t.set_cursor_pos(4, 1);
    t.set_attribute(sgr::Attribute::DirectColorBg(color::RGB {
        r: 0xFF,
        g: 0,
        b: 0,
    }))
    .unwrap();
    t.index().unwrap();

    // The region (rows 2-4) scrolled up, rows outside are unchanged.
    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "1\n3\n4\n\n5".as_bytes());
    }

    // The cursor is on the new blank row.
    assert_eq!(3 as size::CellCountInt, unsafe {
        (*t.screens.active).cursor.y
    });

    // The new blank row must be filled with our background color.
    for x in 0..t.cols {
        let list_cell = t.active_mut().pages.get_cell(active_pt(x, 3)).unwrap();
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

// test "Terminal: index bottom of alt screen full region"
#[test]
fn terminal_index_bottom_of_alt_screen_full_region() {
    let mut t = Terminal::init(Global, opts(5, 3)).unwrap();

    t.switch_screen_mode(SwitchScreenMode::M1049, true).unwrap();
    t.print_string("A\nB\nC".as_bytes()).unwrap();
    t.index().unwrap();
    t.carriage_return();
    t.print('D' as u32).unwrap();

    // Content scrolled up and the scrolled-out row is discarded, NOT
    // moved into scrollback (the alt screen has none).
    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "B\nC\nD".as_bytes());
    }
    {
        let str = t
            .active_mut()
            .dump_string_alloc(Global, screen_pt(0, 0))
            .unwrap();
        assert_eq!(str.as_slice(), "B\nC\nD".as_bytes());
    }

    // Primary screen is untouched.
    t.switch_screen_mode(SwitchScreenMode::M1049, false)
        .unwrap();
    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: index bottom of alt screen top region"
#[test]
fn terminal_index_bottom_of_alt_screen_top_region() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.switch_screen_mode(SwitchScreenMode::M1049, true).unwrap();
    t.print_string("1\n2\n3\n4\n5".as_bytes()).unwrap();

    // Region at the top of the screen, excluding the last row. On the
    // alt screen this must NOT create scrollback.
    t.set_top_and_bottom_margin(1, 4);
    t.set_cursor_pos(4, 1);
    t.index().unwrap();
    t.print('X' as u32).unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "2\n3\n4\nX\n5".as_bytes());
    }
    {
        let str = t
            .active_mut()
            .dump_string_alloc(Global, screen_pt(0, 0))
            .unwrap();
        assert_eq!(str.as_slice(), "2\n3\n4\nX\n5".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: scrollUp top region no scrollback"
#[test]
fn terminal_scroll_up_top_region_no_scrollback() {
    let mut t = Terminal::init(Global, opts_sb(5, 5, 0)).unwrap();

    t.print_string("A\nB\nC\nD\nE".as_bytes()).unwrap();
    t.set_top_and_bottom_margin(1, 3);
    t.scroll_up(1).unwrap();

    // The region scrolled and the scrolled-out row is discarded.
    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "B\nC\n\nD\nE".as_bytes());
    }
    {
        let str = t
            .active_mut()
            .dump_string_alloc(Global, screen_pt(0, 0))
            .unwrap();
        assert_eq!(str.as_slice(), "B\nC\n\nD\nE".as_bytes());
    }

    t.deinit(Global);
}
