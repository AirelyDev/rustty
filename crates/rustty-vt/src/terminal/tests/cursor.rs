//! PORTS: ghostty src/terminal/Terminal.zig — the carriage return / backspace / tab / `setCursorPos` / `cursor*` / save-restore cursor tests.

use super::*;

// PORT NOTE (vt-12b-2 deferral): test "Terminal: linefeed mode automatic
// carriage return" calls `linefeed()`, which dispatches to `index()` —
// still `todo!()` until vt-12b-3b. Deferred there with the other
// `index()`/scroll-dependent tests.

// test "Terminal: carriage return unsets pending wrap"
#[test]
fn terminal_carriage_return_unsets_pending_wrap() {
    let mut t = Terminal::init(Global, opts(5, 80)).unwrap();

    // Basic grid writing
    for c in "hello".chars() {
        t.print(c as u32).unwrap();
    }
    assert!(unsafe { (*t.screens.active).cursor.pending_wrap });
    t.carriage_return();
    assert!(!unsafe { (*t.screens.active).cursor.pending_wrap });

    t.deinit(Global);
}

// test "Terminal: carriage return origin mode moves to left margin"
#[test]
fn terminal_carriage_return_origin_mode_moves_to_left_margin() {
    let mut t = Terminal::init(Global, opts(5, 80)).unwrap();

    t.modes.set(modespkg::Mode::Origin, true);
    unsafe { (*t.screens.active).cursor.x = 0 };
    t.scrolling_region.left = 2;
    t.carriage_return();
    assert_eq!(unsafe { (*t.screens.active).cursor.x }, 2);

    t.deinit(Global);
}

// test "Terminal: carriage return left of left margin moves to zero"
#[test]
fn terminal_carriage_return_left_of_left_margin_moves_to_zero() {
    let mut t = Terminal::init(Global, opts(5, 80)).unwrap();

    unsafe { (*t.screens.active).cursor.x = 1 };
    t.scrolling_region.left = 2;
    t.carriage_return();
    assert_eq!(unsafe { (*t.screens.active).cursor.x }, 0);

    t.deinit(Global);
}

// test "Terminal: carriage return right of left margin moves to left margin"
#[test]
fn terminal_carriage_return_right_of_left_margin_moves_to_left_margin() {
    let mut t = Terminal::init(Global, opts(5, 80)).unwrap();

    unsafe { (*t.screens.active).cursor.x = 3 };
    t.scrolling_region.left = 2;
    t.carriage_return();
    assert_eq!(unsafe { (*t.screens.active).cursor.x }, 2);

    t.deinit(Global);
}

// test "Terminal: backspace"
#[test]
fn terminal_backspace() {
    let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

    // BS
    for c in "hello".chars() {
        t.print(c as u32).unwrap();
    }
    t.backspace();
    t.print('y' as u32).unwrap();
    assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
    assert_eq!(unsafe { (*t.screens.active).cursor.x }, 5);
    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "helly".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: horizontal tabs"
#[test]
fn terminal_horizontal_tabs() {
    let mut t = Terminal::init(Global, opts(20, 5)).unwrap();

    // HT
    t.print('1' as u32).unwrap();
    t.horizontal_tab();
    assert_eq!(unsafe { (*t.screens.active).cursor.x }, 8);

    // HT
    t.horizontal_tab();
    assert_eq!(unsafe { (*t.screens.active).cursor.x }, 16);

    // HT at the end
    t.horizontal_tab();
    assert_eq!(unsafe { (*t.screens.active).cursor.x }, 19);
    t.horizontal_tab();
    assert_eq!(unsafe { (*t.screens.active).cursor.x }, 19);

    t.deinit(Global);
}

// test "Terminal: horizontal tabs starting on tabstop"
#[test]
fn terminal_horizontal_tabs_starting_on_tabstop() {
    let mut t = Terminal::init(Global, opts(20, 5)).unwrap();

    let cursor_y = unsafe { (*t.screens.active).cursor.y } as usize;
    t.set_cursor_pos(cursor_y, 9);
    t.print('X' as u32).unwrap();
    let cursor_y = unsafe { (*t.screens.active).cursor.y } as usize;
    t.set_cursor_pos(cursor_y, 9);
    t.horizontal_tab();
    t.print('A' as u32).unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "        X       A".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: horizontal tabs with right margin"
#[test]
fn terminal_horizontal_tabs_with_right_margin() {
    let mut t = Terminal::init(Global, opts(20, 5)).unwrap();

    t.scrolling_region.left = 2;
    t.scrolling_region.right = 5;
    let cursor_y = unsafe { (*t.screens.active).cursor.y } as usize;
    t.set_cursor_pos(cursor_y, 1);
    t.print('X' as u32).unwrap();
    t.horizontal_tab();
    t.print('A' as u32).unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "X    A".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: horizontal tabs back"
#[test]
fn terminal_horizontal_tabs_back() {
    let mut t = Terminal::init(Global, opts(20, 5)).unwrap();

    // Edge of screen
    let cursor_y = unsafe { (*t.screens.active).cursor.y } as usize;
    t.set_cursor_pos(cursor_y, 20);

    // HT
    t.horizontal_tab_back();
    assert_eq!(unsafe { (*t.screens.active).cursor.x }, 16);

    // HT
    t.horizontal_tab_back();
    assert_eq!(unsafe { (*t.screens.active).cursor.x }, 8);

    // HT
    t.horizontal_tab_back();
    assert_eq!(unsafe { (*t.screens.active).cursor.x }, 0);
    t.horizontal_tab_back();
    assert_eq!(unsafe { (*t.screens.active).cursor.x }, 0);

    t.deinit(Global);
}

// test "Terminal: horizontal tabs back starting on tabstop"
#[test]
fn terminal_horizontal_tabs_back_starting_on_tabstop() {
    let mut t = Terminal::init(Global, opts(20, 5)).unwrap();

    let cursor_y = unsafe { (*t.screens.active).cursor.y } as usize;
    t.set_cursor_pos(cursor_y, 9);
    t.print('X' as u32).unwrap();
    let cursor_y = unsafe { (*t.screens.active).cursor.y } as usize;
    t.set_cursor_pos(cursor_y, 9);
    t.horizontal_tab_back();
    t.print('A' as u32).unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "A       X".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: horizontal tabs with left margin in origin mode"
#[test]
fn terminal_horizontal_tabs_with_left_margin_in_origin_mode() {
    let mut t = Terminal::init(Global, opts(20, 5)).unwrap();

    t.modes.set(modespkg::Mode::Origin, true);
    t.scrolling_region.left = 2;
    t.scrolling_region.right = 5;
    t.set_cursor_pos(1, 2);
    t.print('X' as u32).unwrap();
    t.horizontal_tab_back();
    t.print('A' as u32).unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "  AX".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: tabClear single"
#[test]
fn terminal_tab_clear_single() {
    let mut t = Terminal::init(Global, opts(30, 5)).unwrap();

    t.horizontal_tab();
    t.tab_clear(csi::TabClear::Current);
    assert!(!t.is_dirty(active_pt(0, 0)));
    t.set_cursor_pos(1, 1);
    t.horizontal_tab();
    assert_eq!(unsafe { (*t.screens.active).cursor.x }, 16);

    t.deinit(Global);
}

// test "Terminal: tabClear all"
#[test]
fn terminal_tab_clear_all() {
    let mut t = Terminal::init(Global, opts(30, 5)).unwrap();

    t.tab_clear(csi::TabClear::All);
    assert!(!t.is_dirty(active_pt(0, 0)));
    t.set_cursor_pos(1, 1);
    t.horizontal_tab();
    assert_eq!(unsafe { (*t.screens.active).cursor.x }, 29);

    t.deinit(Global);
}

// S142-F1: `tab_clear`'s `else` arm logs `log.warn("invalid or unknown
// tab clear setting: {}", .{cmd})` (Terminal.zig:2288-2293) for any
// unrecognized DECST/TBC parameter.
#[test]
fn terminal_tab_clear_unknown_setting_logs() {
    let mut t = Terminal::init(Global, opts(30, 5)).unwrap();

    let cap = crate::log_capture::start();
    t.tab_clear(csi::TabClear::Other(99));
    let messages = cap.messages();
    assert!(
        messages
            .iter()
            .any(|(level, target, msg)| *level == log::Level::Warn
                && target == "terminal"
                && msg.starts_with("invalid or unknown tab clear setting: ")),
        "expected the unknown-tab-clear diagnostic, got: {messages:?}"
    );
    drop(cap);

    t.deinit(Global);
}

// test "Terminal: cursorPos resets wrap"
#[test]
fn terminal_cursor_pos_resets_wrap() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    for c in "ABCDE".chars() {
        t.print(c as u32).unwrap();
    }
    assert!(unsafe { (*t.screens.active).cursor.pending_wrap });
    t.set_cursor_pos(1, 1);
    assert!(!unsafe { (*t.screens.active).cursor.pending_wrap });
    t.print('X' as u32).unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "XBCDE".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: cursorPos off the screen"
#[test]
fn terminal_cursor_pos_off_the_screen() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.set_cursor_pos(500, 500);
    t.print('X' as u32).unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "\n\n\n\n    X".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: cursorPos relative to origin"
#[test]
fn terminal_cursor_pos_relative_to_origin() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.scrolling_region.top = 2;
    t.scrolling_region.bottom = 3;
    t.modes.set(modespkg::Mode::Origin, true);
    t.set_cursor_pos(1, 1);
    t.print('X' as u32).unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "\n\nX".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: cursorPos relative to origin with left/right"
#[test]
fn terminal_cursor_pos_relative_to_origin_with_left_right() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.scrolling_region.top = 2;
    t.scrolling_region.bottom = 3;
    t.scrolling_region.left = 2;
    t.scrolling_region.right = 4;
    t.modes.set(modespkg::Mode::Origin, true);
    t.set_cursor_pos(1, 1);
    t.print('X' as u32).unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "\n\n  X".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: cursorPos limits with full scroll region"
#[test]
fn terminal_cursor_pos_limits_with_full_scroll_region() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.scrolling_region.top = 2;
    t.scrolling_region.bottom = 3;
    t.scrolling_region.left = 2;
    t.scrolling_region.right = 4;
    t.modes.set(modespkg::Mode::Origin, true);
    t.set_cursor_pos(500, 500);
    t.print('X' as u32).unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "\n\n\n    X".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: cursorUp basic"
#[test]
fn terminal_cursor_up_basic() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.set_cursor_pos(3, 1);
    t.print('A' as u32).unwrap();
    t.cursor_up(10);
    t.print('X' as u32).unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), " X\n\nA".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: cursorUp below top scroll margin"
#[test]
fn terminal_cursor_up_below_top_scroll_margin() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.set_top_and_bottom_margin(2, 4);
    t.set_cursor_pos(3, 1);
    t.print('A' as u32).unwrap();
    t.cursor_up(5);
    t.print('X' as u32).unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "\n X\nA".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: cursorUp above top scroll margin"
#[test]
fn terminal_cursor_up_above_top_scroll_margin() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.set_top_and_bottom_margin(3, 5);
    t.set_cursor_pos(3, 1);
    t.print('A' as u32).unwrap();
    t.set_cursor_pos(2, 1);
    t.cursor_up(10);
    t.print('X' as u32).unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "X\n\nA".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: cursorUp resets wrap"
#[test]
fn terminal_cursor_up_resets_wrap() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    for c in "ABCDE".chars() {
        t.print(c as u32).unwrap();
    }
    assert!(unsafe { (*t.screens.active).cursor.pending_wrap });
    t.cursor_up(1);
    assert!(!unsafe { (*t.screens.active).cursor.pending_wrap });
    t.print('X' as u32).unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "ABCDX".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: cursorLeft no wrap"
#[test]
fn terminal_cursor_left_no_wrap() {
    let mut t = Terminal::init(Global, opts(10, 5)).unwrap();

    t.print('A' as u32).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print('B' as u32).unwrap();
    t.cursor_left(10);

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "A\nB".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: cursorLeft unsets pending wrap state"
#[test]
fn terminal_cursor_left_unsets_pending_wrap_state() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    for c in "ABCDE".chars() {
        t.print(c as u32).unwrap();
    }
    assert!(unsafe { (*t.screens.active).cursor.pending_wrap });
    t.cursor_left(1);
    assert!(!unsafe { (*t.screens.active).cursor.pending_wrap });
    t.print('X' as u32).unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "ABCXE".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: cursorLeft unsets pending wrap state with longer jump"
#[test]
fn terminal_cursor_left_unsets_pending_wrap_state_with_longer_jump() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    for c in "ABCDE".chars() {
        t.print(c as u32).unwrap();
    }
    assert!(unsafe { (*t.screens.active).cursor.pending_wrap });
    t.cursor_left(3);
    assert!(!unsafe { (*t.screens.active).cursor.pending_wrap });
    t.print('X' as u32).unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "AXCDE".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: cursorLeft reverse wrap with pending wrap state"
#[test]
fn terminal_cursor_left_reverse_wrap_with_pending_wrap_state() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.modes.set(modespkg::Mode::Wraparound, true);
    t.modes.set(modespkg::Mode::ReverseWrap, true);

    for c in "ABCDE".chars() {
        t.print(c as u32).unwrap();
    }
    assert!(unsafe { (*t.screens.active).cursor.pending_wrap });
    t.cursor_left(1);
    assert!(!unsafe { (*t.screens.active).cursor.pending_wrap });
    t.print('X' as u32).unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "ABCDX".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: cursorLeft reverse wrap with pending wrap above top margin"
#[test]
fn terminal_cursor_left_reverse_wrap_with_pending_wrap_above_top_margin() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.modes.set(modespkg::Mode::Wraparound, true);
    t.modes.set(modespkg::Mode::ReverseWrap, true);
    t.modes.set(modespkg::Mode::EnableLeftAndRightMargin, true);
    t.set_left_and_right_margin(1, 2);
    for c in "AB".chars() {
        t.print(c as u32).unwrap();
    }
    t.save_cursor();

    // Restore pending wrap at the left margin, above the top margin.
    t.set_left_and_right_margin(2, 5);
    t.set_top_and_bottom_margin(3, 5);
    t.restore_cursor();
    assert!(unsafe { (*t.screens.active).cursor.pending_wrap });

    t.cursor_left(1);
    assert!(!unsafe { (*t.screens.active).cursor.pending_wrap });
    assert_eq!(1, unsafe { (*t.screens.active).cursor.x });
    assert_eq!(0, unsafe { (*t.screens.active).cursor.y });
    t.print('X' as u32).unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "AX".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: cursorLeft reverse wrap extended with pending wrap state"
#[test]
fn terminal_cursor_left_reverse_wrap_extended_with_pending_wrap_state() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.modes.set(modespkg::Mode::Wraparound, true);
    t.modes.set(modespkg::Mode::ReverseWrapExtended, true);

    for c in "ABCDE".chars() {
        t.print(c as u32).unwrap();
    }
    assert!(unsafe { (*t.screens.active).cursor.pending_wrap });
    t.cursor_left(1);
    assert!(!unsafe { (*t.screens.active).cursor.pending_wrap });
    t.print('X' as u32).unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "ABCDX".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: cursorLeft reverse wrap"
#[test]
fn terminal_cursor_left_reverse_wrap() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.modes.set(modespkg::Mode::Wraparound, true);
    t.modes.set(modespkg::Mode::ReverseWrap, true);

    for c in "ABCDE1".chars() {
        t.print(c as u32).unwrap();
    }
    t.cursor_left(2);
    t.print('X' as u32).unwrap();
    assert!(unsafe { (*t.screens.active).cursor.pending_wrap });

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "ABCDX\n1".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: cursorLeft reverse wrap with no soft wrap"
#[test]
fn terminal_cursor_left_reverse_wrap_with_no_soft_wrap() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.modes.set(modespkg::Mode::Wraparound, true);
    t.modes.set(modespkg::Mode::ReverseWrap, true);

    for c in "ABCDE".chars() {
        t.print(c as u32).unwrap();
    }
    t.carriage_return();
    t.linefeed().unwrap();
    t.print('1' as u32).unwrap();
    t.cursor_left(2);
    t.print('X' as u32).unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "ABCDE\nX".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: cursorLeft reverse wrap before left margin"
#[test]
fn terminal_cursor_left_reverse_wrap_before_left_margin() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.modes.set(modespkg::Mode::Wraparound, true);
    t.modes.set(modespkg::Mode::ReverseWrap, true);
    t.set_top_and_bottom_margin(3, 0);
    t.cursor_left(1);
    t.print('X' as u32).unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "\n\nX".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: cursorLeft extended reverse wrap"
#[test]
fn terminal_cursor_left_extended_reverse_wrap() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.modes.set(modespkg::Mode::Wraparound, true);
    t.modes.set(modespkg::Mode::ReverseWrapExtended, true);

    for c in "ABCDE".chars() {
        t.print(c as u32).unwrap();
    }
    t.carriage_return();
    t.linefeed().unwrap();
    t.print('1' as u32).unwrap();
    t.cursor_left(2);
    t.print('X' as u32).unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "ABCDX\n1".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: cursorLeft extended reverse wrap bottom wraparound"
#[test]
fn terminal_cursor_left_extended_reverse_wrap_bottom_wraparound() {
    let mut t = Terminal::init(Global, opts(5, 3)).unwrap();

    t.modes.set(modespkg::Mode::Wraparound, true);
    t.modes.set(modespkg::Mode::ReverseWrapExtended, true);

    for c in "ABCDE".chars() {
        t.print(c as u32).unwrap();
    }
    t.carriage_return();
    t.linefeed().unwrap();
    t.print('1' as u32).unwrap();
    t.cursor_left(1 + t.cols as usize + 1);
    t.print('X' as u32).unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "ABCDE\n1\n    X".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: cursorLeft extended reverse wrap is priority if both set"
#[test]
fn terminal_cursor_left_extended_reverse_wrap_is_priority_if_both_set() {
    let mut t = Terminal::init(Global, opts(5, 3)).unwrap();

    t.modes.set(modespkg::Mode::Wraparound, true);
    t.modes.set(modespkg::Mode::ReverseWrap, true);
    t.modes.set(modespkg::Mode::ReverseWrapExtended, true);

    for c in "ABCDE".chars() {
        t.print(c as u32).unwrap();
    }
    t.carriage_return();
    t.linefeed().unwrap();
    t.print('1' as u32).unwrap();
    t.cursor_left(1 + t.cols as usize + 1);
    t.print('X' as u32).unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "ABCDE\n1\n    X".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: cursorLeft extended reverse wrap above top scroll region"
#[test]
fn terminal_cursor_left_extended_reverse_wrap_above_top_scroll_region() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.modes.set(modespkg::Mode::Wraparound, true);
    t.modes.set(modespkg::Mode::ReverseWrapExtended, true);

    t.set_top_and_bottom_margin(3, 0);
    t.set_cursor_pos(2, 1);
    t.cursor_left(1000);

    assert_eq!(unsafe { (*t.screens.active).cursor.x }, 0);
    assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);

    t.deinit(Global);
}

// test "Terminal: cursorLeft reverse wrap on first row"
#[test]
fn terminal_cursor_left_reverse_wrap_on_first_row() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.modes.set(modespkg::Mode::Wraparound, true);
    t.modes.set(modespkg::Mode::ReverseWrap, true);

    t.set_top_and_bottom_margin(3, 0);
    t.set_cursor_pos(1, 2);
    t.cursor_left(1000);

    assert_eq!(unsafe { (*t.screens.active).cursor.x }, 0);
    assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);

    t.deinit(Global);
}

// test "Terminal: cursorDown basic"
#[test]
fn terminal_cursor_down_basic() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.print('A' as u32).unwrap();
    t.cursor_down(10);
    t.print('X' as u32).unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "A\n\n\n\n X".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: cursorDown above bottom scroll margin"
#[test]
fn terminal_cursor_down_above_bottom_scroll_margin() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.set_top_and_bottom_margin(1, 3);
    t.print('A' as u32).unwrap();
    t.cursor_down(10);
    t.print('X' as u32).unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "A\n\n X".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: cursorDown below bottom scroll margin"
#[test]
fn terminal_cursor_down_below_bottom_scroll_margin() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.set_top_and_bottom_margin(1, 3);
    t.print('A' as u32).unwrap();
    t.set_cursor_pos(4, 1);
    t.cursor_down(10);
    t.print('X' as u32).unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "A\n\n\n\nX".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: cursorDown resets wrap"
#[test]
fn terminal_cursor_down_resets_wrap() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    for c in "ABCDE".chars() {
        t.print(c as u32).unwrap();
    }
    assert!(unsafe { (*t.screens.active).cursor.pending_wrap });
    t.cursor_down(1);
    assert!(!unsafe { (*t.screens.active).cursor.pending_wrap });
    t.print('X' as u32).unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "ABCDE\n    X".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: cursorRight resets wrap"
#[test]
fn terminal_cursor_right_resets_wrap() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    for c in "ABCDE".chars() {
        t.print(c as u32).unwrap();
    }
    assert!(unsafe { (*t.screens.active).cursor.pending_wrap });
    t.cursor_right(1);
    assert!(!unsafe { (*t.screens.active).cursor.pending_wrap });
    t.print('X' as u32).unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "ABCDX".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: cursorRight to the edge of screen"
#[test]
fn terminal_cursor_right_to_the_edge_of_screen() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.cursor_right(100);
    t.print('X' as u32).unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "    X".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: cursorRight left of right margin"
#[test]
fn terminal_cursor_right_left_of_right_margin() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.scrolling_region.right = 2;
    t.cursor_right(100);
    t.print('X' as u32).unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "  X".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: cursorRight right of right margin"
#[test]
fn terminal_cursor_right_right_of_right_margin() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.scrolling_region.right = 2;
    t.set_cursor_pos(1, 4);
    t.cursor_right(100);
    t.print('X' as u32).unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "    X".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: saveCursor"
#[test]
fn terminal_save_cursor() {
    let mut t = Terminal::init(Global, opts(3, 3)).unwrap();

    t.set_attribute(sgr::Attribute::Bold).unwrap();
    unsafe { (*t.screens.active).charset.gr = charsets::Slots::G3 };
    t.modes.set(modespkg::Mode::Origin, true);
    t.save_cursor();
    unsafe { (*t.screens.active).charset.gr = charsets::Slots::G0 };
    t.set_attribute(sgr::Attribute::Unset).unwrap();
    t.modes.set(modespkg::Mode::Origin, false);
    t.restore_cursor();
    assert!(unsafe { (*t.screens.active).cursor.style.flags.bold });
    assert!(unsafe { (*t.screens.active).charset.gr } == charsets::Slots::G3);
    assert!(t.modes.get(modespkg::Mode::Origin));

    t.deinit(Global);
}

// test "Terminal: saveCursor position"
#[test]
fn terminal_save_cursor_position() {
    let mut t = Terminal::init(Global, opts(10, 5)).unwrap();

    t.set_cursor_pos(1, 5);
    t.print('A' as u32).unwrap();
    t.save_cursor();
    t.set_cursor_pos(1, 1);
    t.print('B' as u32).unwrap();
    t.restore_cursor();
    t.print('X' as u32).unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "B   AX".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: saveCursor pending wrap state"
#[test]
fn terminal_save_cursor_pending_wrap_state() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.set_cursor_pos(1, 5);
    t.print('A' as u32).unwrap();
    t.save_cursor();
    t.set_cursor_pos(1, 1);
    t.print('B' as u32).unwrap();
    t.restore_cursor();
    t.print('X' as u32).unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "B   A\nX".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: saveCursor origin mode"
#[test]
fn terminal_save_cursor_origin_mode() {
    let mut t = Terminal::init(Global, opts(10, 5)).unwrap();

    t.modes.set(modespkg::Mode::Origin, true);
    t.save_cursor();
    t.modes.set(modespkg::Mode::EnableLeftAndRightMargin, true);
    t.set_left_and_right_margin(3, 5);
    t.set_top_and_bottom_margin(2, 4);
    t.restore_cursor();
    t.print('X' as u32).unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "X".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: saveCursor protected pen"
#[test]
fn terminal_save_cursor_protected_pen() {
    let mut t = Terminal::init(Global, opts(10, 5)).unwrap();

    t.set_protected_mode(ansi::ProtectedMode::Iso);
    assert!(unsafe { (*t.screens.active).cursor.protected });
    t.set_cursor_pos(1, 10);
    t.save_cursor();
    t.set_protected_mode(ansi::ProtectedMode::Off);
    assert!(!unsafe { (*t.screens.active).cursor.protected });
    t.restore_cursor();
    assert!(unsafe { (*t.screens.active).cursor.protected });

    t.deinit(Global);
}

// test "Terminal: saveCursor doesn't modify hyperlink state"
#[test]
fn terminal_save_cursor_doesn_t_modify_hyperlink_state() {
    let mut t = Terminal::init(Global, opts(3, 3)).unwrap();

    t.active_mut()
        .start_hyperlink("http://example.com".as_bytes(), None)
        .unwrap();
    let id = unsafe { (*t.screens.active).cursor.hyperlink_id };
    t.save_cursor();
    assert_eq!(id, unsafe { (*t.screens.active).cursor.hyperlink_id });
    t.restore_cursor();
    assert_eq!(id, unsafe { (*t.screens.active).cursor.hyperlink_id });

    t.deinit(Global);
}

// test "Terminal: restoreCursor uses default style on OutOfSpace"
#[test]
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION — BOTH LANES (rustty-vt, parity tail C-1; .port/phase-2/MIRI_EXCEPTIONS.md, 'Parity tail C-1 — the G6 triage'): past a 15-minute Miri bound with debug assertions on (G6) and past an 8-minute bound with them off (G6c), both measured one test per process; runs natively in cargo test --workspace and under ASan (see MIRI_EXCEPTIONS.md)"
)]
fn terminal_restore_cursor_uses_default_style_on_out_of_space() {
    // Tests that restoreCursor falls back to default style when
    // manualStyleUpdate fails with OutOfSpace (can't split a 1-row page
    // and styles are at max capacity).

    // Use a single row so the page can't be split
    let mut t = Terminal::init(Global, opts(10, 1)).unwrap();

    // Set a style and save the cursor
    t.set_attribute(sgr::Attribute::Bold).unwrap();
    t.save_cursor();

    // Clear the style
    t.set_attribute(sgr::Attribute::Unset).unwrap();
    assert!(!unsafe { (*t.screens.active).cursor.style.flags.bold });

    // Fill the style map to max capacity
    let max_styles = size::CellCountInt::MAX;
    loop {
        let node = unsafe { (*(*t.screens.active).cursor.page_pin).node };
        if unsafe { crate::pagelist::Node::capacity(node).styles } >= max_styles {
            break;
        }
        if t.active_mut()
            .increase_capacity(node, Some(pagelist::grow::IncreaseCapacity::Styles))
            .is_err()
        {
            break;
        }
    }

    let page: *mut pagepkg::Page =
        unsafe { crate::pagelist::Node::page((*(*t.screens.active).cursor.page_pin).node) };
    assert_eq!(unsafe { (*page).capacity.styles }, max_styles);

    // Fill all style slots using the StyleSet's layout capacity which accounts
    // for the load factor. The capacity in the layout is the actual max number
    // of items that can be stored.
    {
        unsafe { (*page).pause_integrity_checks(true) };

        let max_items = unsafe { (*page).styles.layout.cap };
        let mut n: usize = 1;
        while n < max_items {
            let base = unsafe { (*page).memory_ptr() };
            if unsafe {
                (*page).styles.add(
                    base,
                    style::Style {
                        bg_color: style::Color::Rgb(color::RGB {
                            r: (n & 0xFF) as u8,
                            g: ((n >> 8) & 0xFF) as u8,
                            b: ((n >> 16) & 0xFF) as u8,
                        }),
                        ..Default::default()
                    },
                )
            }
            .is_err()
            {
                break;
            }
            n += 1;
        }

        unsafe { (*page).assert_integrity() };
        unsafe { (*page).pause_integrity_checks(false) };
    }

    // Restore cursor - should fall back to default style since page
    // can't be split (1 row) and styles are at max capacity
    t.restore_cursor();

    // The style should be reset to default because OutOfSpace occurred
    assert!(!unsafe { (*t.screens.active).cursor.style.flags.bold });
    assert_eq!(
        unsafe { (*t.screens.active).cursor.style_id },
        style::DEFAULT_ID
    );

    t.deinit(Global);
}
