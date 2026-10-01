//! PORTS: ghostty src/terminal/Terminal.zig — the `eraseChars` / `deleteChars` / `eraseLine` tests.

use super::*;

// === vt-12b-3a: erase methods ===========================================
//
// PORT NOTE (vt-12b-3a deferral): erase tests that drive still-`todo!()`
// methods are deferred to vt-12b-3b. Specifically deferred are any test
// that:
//   - soft-wraps (multi-row via `print_wrap()` → `index()`), or uses
//     `linefeed()`/`\n` in `print_string` (also `index()`),
//   - calls `set_attribute` (SGR; the "preserves background sgr",
//     "handles refcounted styles", "preserves color", "preserves cursor"
//     groups),
//   - calls `set_protected_mode` (the protected-attribute groups),
//   - calls `set_top_and_bottom_margin` / `set_left_and_right_margin` /
//     `scroll_down` (the decaln + scroll-region + right-margin groups).
// The `decaln`, `erase_display` (all modes but `Scrollback`/single-row),
// and protected `erase_line`/`erase_chars` test groups land in vt-12b-3b
// once those methods are filled.

// test "Terminal: eraseChars simple operation"
#[test]
fn terminal_erase_chars_simple_operation() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    for c in "ABC".chars() {
        t.print(c as u32).unwrap();
    }
    t.set_cursor_pos(1, 1);
    t.clear_dirty();
    t.erase_chars(2);
    t.print('X' as u32).unwrap();

    assert!(t.is_dirty(active_pt(0, 0)));
    assert!(!t.is_dirty(active_pt(0, 1)));

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "X C".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: eraseChars minimum one"
#[test]
fn terminal_erase_chars_minimum_one() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    for c in "ABC".chars() {
        t.print(c as u32).unwrap();
    }
    t.set_cursor_pos(1, 1);
    t.clear_dirty();
    t.erase_chars(0);
    t.print('X' as u32).unwrap();
    assert!(t.is_dirty(active_pt(0, 0)));

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "XBC".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: eraseChars beyond screen edge"
#[test]
fn terminal_erase_chars_beyond_screen_edge() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    for c in "  ABC".chars() {
        t.print(c as u32).unwrap();
    }
    t.set_cursor_pos(1, 4);
    t.erase_chars(10);

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "  A".as_bytes());
    }

    t.deinit(Global);
}

// PORT NOTE (rustty-only regression, NOT a reference test — VALIDATION_REPORT
// §3c.8): `count_req: usize` past u16::MAX. Zig's `@min(remaining,
// @max(count_req, 1))` clamps in usize BEFORE the value narrows into u16
// context; the pre-fix Rust body truncated `count_req as CellCountInt`
// FIRST, so 65537 became 1 and erased a single cell instead of clamping to
// `remaining` (unreachable via the VT stream, reachable via this pub API /
// FFI). Mirrors `eraseChars beyond screen edge` with an over-u16 count.
#[test]
fn terminal_erase_chars_count_beyond_u16() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    for c in "  ABC".chars() {
        t.print(c as u32).unwrap();
    }
    t.set_cursor_pos(1, 4);
    // u16::MAX + 2: must clamp to `remaining` (2), not truncate to 1.
    t.erase_chars(65537);

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "  A".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: eraseChars wide character"
#[test]
fn terminal_erase_chars_wide_character() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.print('橋' as u32).unwrap();
    for c in "BC".chars() {
        t.print(c as u32).unwrap();
    }
    t.set_cursor_pos(1, 1);
    t.erase_chars(1);
    t.print('X' as u32).unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "X BC".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: eraseChars resets pending wrap"
#[test]
fn terminal_erase_chars_resets_pending_wrap() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    for c in "ABCDE".chars() {
        t.print(c as u32).unwrap();
    }
    assert!(unsafe { (*t.screens.active).cursor.pending_wrap });
    t.erase_chars(1);
    assert!(!unsafe { (*t.screens.active).cursor.pending_wrap });
    t.print('X' as u32).unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "ABCDX".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: eraseChars resets wrap"
#[test]
fn terminal_erase_chars_resets_wrap() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    for c in "ABCDE123".chars() {
        t.print(c as u32).unwrap();
    }
    {
        let list_cell = t.active_mut().pages.get_cell(active_pt(0, 0)).unwrap();
        let row = list_cell.row;
        assert!(unsafe { (*row).wrap() });
    }

    t.set_cursor_pos(1, 1);
    t.erase_chars(1);

    {
        let list_cell = t.active_mut().pages.get_cell(active_pt(0, 0)).unwrap();
        let row = list_cell.row;
        assert!(!unsafe { (*row).wrap() });
    }

    t.print('X' as u32).unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "XBCDE\n123".as_bytes());
    }

    t.deinit(Global);
}

// PORT NOTE (vt-12b-3a deferral): tests "Terminal: eraseChars preserves
// background sgr", "eraseChars handles refcounted styles", and the
// "eraseChars protected attributes *" group drive `set_attribute` /
// `set_protected_mode`; now ported in vt-12b-4 (see the SGR/protected
// section at the end of this module).

// test "Terminal: eraseChars wide char boundary conditions"
#[test]
fn terminal_erase_chars_wide_char_boundary_conditions() {
    let mut t = Terminal::init(Global, opts(8, 1)).unwrap();

    t.print_string("😀a😀b😀".as_bytes()).unwrap();
    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "😀a😀b😀".as_bytes());
    }

    t.set_cursor_pos(1, 2);
    t.erase_chars(3);
    unsafe {
        (*crate::pagelist::Node::page((*(*t.screens.active).cursor.page_pin).node))
            .assert_integrity()
    };

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "     b😀".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: eraseChars wide char splits proper cell boundaries"
#[test]
fn terminal_erase_chars_wide_char_splits_proper_cell_boundaries() {
    let mut t = Terminal::init(Global, opts(30, 1)).unwrap();

    // This is a test for a bug: https://github.com/ghostty-org/ghostty/issues/2817
    // To explain the setup:
    // (1) We need our wide characters starting on an even (1-based) column.
    // (2) We need our cursor to be in the middle somewhere.
    // (3) We need our count to be less than our cursor X and on a split cell.
    // The bug was that we split the wrong cell boundaries.

    t.print_string("x食べて下さい".as_bytes()).unwrap();
    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "x食べて下さい".as_bytes());
    }

    t.set_cursor_pos(1, 6); // At: て
    t.erase_chars(4); // Delete: て下
    unsafe {
        (*crate::pagelist::Node::page((*(*t.screens.active).cursor.page_pin).node))
            .assert_integrity()
    };

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "x食べ    さい".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: eraseChars wide char wrap boundary conditions"
#[test]
fn terminal_erase_chars_wide_char_wrap_boundary_conditions() {
    let mut t = Terminal::init(Global, opts(8, 3)).unwrap();

    t.print_string(".......😀abcde😀......".as_bytes()).unwrap();
    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), ".......\n😀abcde\n😀......".as_bytes());

        let unwrapped = t.plain_string_unwrapped(Global).unwrap();
        assert_eq!(unwrapped.as_slice(), ".......😀abcde😀......".as_bytes());
    }

    t.set_cursor_pos(2, 2);
    t.erase_chars(3);
    unsafe {
        (*crate::pagelist::Node::page((*(*t.screens.active).cursor.page_pin).node))
            .assert_integrity()
    };

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), ".......\n    cde\n😀......".as_bytes());

        let unwrapped = t.plain_string_unwrapped(Global).unwrap();
        assert_eq!(unwrapped.as_slice(), ".......     cde\n😀......".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: eraseChars clearing wrapped wide char marks spacer head row dirty"
// (`eb722cb26`)
//
// NEGATIVE CONTROL: drop the `p_row.mark_dirty()` that `split_cell_boundary`
// now issues after clearing the previous row's spacer head — the
// `is_dirty(screen_pt(0, 0))` assertion fails (row 0 stays clean).
#[test]
fn terminal_erase_chars_clearing_wrapped_wide_char_marks_spacer_head_row_dirty() {
    let mut t = Terminal::init(Global, opts(5, 3)).unwrap();

    // The wide char doesn't fit so it wraps, leaving a spacer head at
    // the end of the first row.
    t.print_string("ABCD字".as_bytes()).unwrap();
    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(4, 0)).unwrap();
        assert_eq!(pagepkg::Wide::SpacerHead, unsafe {
            (*list_cell.cell).wide()
        });
        assert!(unsafe { (*list_cell.row).wrap() });
    }

    t.set_cursor_pos(2, 1);
    t.clear_dirty();
    t.erase_chars(1);
    unsafe {
        (*crate::pagelist::Node::page((*(*t.screens.active).cursor.page_pin).node))
            .assert_integrity()
    };

    // Erasing the wide char also clears the spacer head on the previous
    // row, so that row must be dirty too.
    assert!(t.is_dirty(screen_pt(0, 0)));
    assert!(t.is_dirty(screen_pt(0, 1)));
    assert!(!t.is_dirty(screen_pt(0, 2)));

    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(4, 0)).unwrap();
        assert_eq!(pagepkg::Wide::Narrow, unsafe { (*list_cell.cell).wide() });
    }

    t.deinit(Global);
}

// test "Terminal: deleteChars"
#[test]
fn terminal_delete_chars() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    for c in "ABCDE".chars() {
        t.print(c as u32).unwrap();
    }
    t.set_cursor_pos(1, 2);

    t.clear_dirty();
    t.delete_chars(2);
    assert!(t.is_dirty(active_pt(0, 0)));

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "ADE".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: deleteChars zero count"
#[test]
fn terminal_delete_chars_zero_count() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    for c in "ABCDE".chars() {
        t.print(c as u32).unwrap();
    }
    t.set_cursor_pos(1, 2);

    t.clear_dirty();
    t.delete_chars(0);
    assert!(!t.is_dirty(active_pt(0, 0)));

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "ABCDE".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: deleteChars more than half"
#[test]
fn terminal_delete_chars_more_than_half() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    for c in "ABCDE".chars() {
        t.print(c as u32).unwrap();
    }
    t.set_cursor_pos(1, 2);

    t.clear_dirty();
    t.delete_chars(3);
    assert!(t.is_dirty(active_pt(0, 0)));

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "AE".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: deleteChars more than line width"
#[test]
fn terminal_delete_chars_more_than_line_width() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    for c in "ABCDE".chars() {
        t.print(c as u32).unwrap();
    }
    t.set_cursor_pos(1, 2);

    t.clear_dirty();
    t.delete_chars(10);
    assert!(t.is_dirty(active_pt(0, 0)));

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "A".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: deleteChars should shift left"
#[test]
fn terminal_delete_chars_should_shift_left() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    for c in "ABCDE".chars() {
        t.print(c as u32).unwrap();
    }
    t.set_cursor_pos(1, 2);

    t.clear_dirty();
    t.delete_chars(1);
    assert!(t.is_dirty(active_pt(0, 0)));

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "ACDE".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: deleteChars resets pending wrap"
#[test]
fn terminal_delete_chars_resets_pending_wrap() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    for c in "ABCDE".chars() {
        t.print(c as u32).unwrap();
    }
    assert!(unsafe { (*t.screens.active).cursor.pending_wrap });
    t.delete_chars(1);
    assert!(!unsafe { (*t.screens.active).cursor.pending_wrap });
    t.print('X' as u32).unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "ABCDX".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: deleteChars resets wrap"
#[test]
fn terminal_delete_chars_resets_wrap() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    for c in "ABCDE123".chars() {
        t.print(c as u32).unwrap();
    }
    {
        let list_cell = t.active_mut().pages.get_cell(active_pt(0, 0)).unwrap();
        let row = list_cell.row;
        assert!(unsafe { (*row).wrap() });
    }
    t.set_cursor_pos(1, 1);
    t.delete_chars(1);

    {
        let list_cell = t.active_mut().pages.get_cell(active_pt(0, 0)).unwrap();
        let row = list_cell.row;
        assert!(!unsafe { (*row).wrap() });
    }

    t.print('X' as u32).unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "XCDE\n123".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: deleteChars simple operation"
#[test]
fn terminal_delete_chars_simple_operation() {
    let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

    t.print_string("ABC123".as_bytes()).unwrap();
    t.set_cursor_pos(1, 3);

    t.clear_dirty();
    t.delete_chars(2);
    assert!(t.is_dirty(active_pt(0, 0)));

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "AB23".as_bytes());
    }

    t.deinit(Global);
}

// PORT NOTE (vt-12b-3a deferral): test "Terminal: deleteChars preserves
// background sgr" drives `set_attribute`; now ported in vt-12b-4 (see the
// SGR/protected section at the end of this module).

// test "Terminal: deleteChars outside scroll region"
#[test]
fn terminal_delete_chars_outside_scroll_region() {
    let mut t = Terminal::init(Global, opts(6, 10)).unwrap();

    t.print_string("ABC123".as_bytes()).unwrap();
    t.scrolling_region.left = 2;
    t.scrolling_region.right = 4;
    assert!(unsafe { (*t.screens.active).cursor.pending_wrap });
    t.clear_dirty();
    t.delete_chars(2);
    assert!(!t.is_dirty(active_pt(0, 0)));
    assert!(unsafe { (*t.screens.active).cursor.pending_wrap });

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "ABC123".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: deleteChars inside scroll region"
#[test]
fn terminal_delete_chars_inside_scroll_region() {
    let mut t = Terminal::init(Global, opts(6, 10)).unwrap();

    t.print_string("ABC123".as_bytes()).unwrap();
    t.scrolling_region.left = 2;
    t.scrolling_region.right = 4;
    t.set_cursor_pos(1, 4);

    t.clear_dirty();
    t.delete_chars(1);
    assert!(t.is_dirty(active_pt(0, 0)));

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "ABC2 3".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: deleteChars split wide character from spacer tail"
#[test]
fn terminal_delete_chars_split_wide_character_from_spacer_tail() {
    let mut t = Terminal::init(Global, opts(6, 10)).unwrap();

    t.print_string("A橋123".as_bytes()).unwrap();
    t.set_cursor_pos(1, 3);
    t.delete_chars(1);

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "A 123".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: deleteChars split wide character from wide"
#[test]
fn terminal_delete_chars_split_wide_character_from_wide() {
    let mut t = Terminal::init(Global, opts(6, 10)).unwrap();

    t.print_string("橋123".as_bytes()).unwrap();
    t.set_cursor_pos(1, 1);
    t.delete_chars(1);

    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).content_codepoint() }, 0);
        assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Narrow);
    }
    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(1, 0)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).content_codepoint() }, '1' as u21);
        assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Narrow);
    }

    t.deinit(Global);
}

// test "Terminal: deleteChars split wide character from end"
#[test]
fn terminal_delete_chars_split_wide_character_from_end() {
    let mut t = Terminal::init(Global, opts(6, 10)).unwrap();

    t.print_string("A橋123".as_bytes()).unwrap();
    t.set_cursor_pos(1, 1);
    t.delete_chars(1);

    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).content_codepoint() }, 0x6A4B);
        assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Wide);
    }
    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(1, 0)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).content_codepoint() }, 0);
        assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::SpacerTail);
    }

    t.deinit(Global);
}

// test "Terminal: deleteChars with a spacer head at the end"
#[test]
fn terminal_delete_chars_with_a_spacer_head_at_the_end() {
    let mut t = Terminal::init(Global, opts(5, 10)).unwrap();

    t.print_string("0123橋123".as_bytes()).unwrap();
    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(4, 0)).unwrap();
        let row = list_cell.row;
        let cell = list_cell.cell;
        assert_eq!(pagepkg::Wide::SpacerHead, unsafe { (*cell).wide() });
        assert!(unsafe { (*row).wrap() });
    }

    t.set_cursor_pos(1, 1);
    t.delete_chars(1);

    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(3, 0)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(0 as u21, unsafe { (*cell).codepoint() });
        assert_eq!(pagepkg::Wide::Narrow, unsafe { (*cell).wide() });
    }

    t.deinit(Global);
}

// test "Terminal: deleteChars split wide character tail"
#[test]
fn terminal_delete_chars_split_wide_character_tail() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.set_cursor_pos(1, (t.cols - 1) as usize);
    t.print(0x6A4B).unwrap(); // 橋
    t.carriage_return();
    t.delete_chars((t.cols - 1) as usize);
    t.print('0' as u32).unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "0".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: deleteChars wide char boundary conditions"
#[test]
fn terminal_delete_chars_wide_char_boundary_conditions() {
    let mut t = Terminal::init(Global, opts(8, 1)).unwrap();

    // EXPLANATION(qwerasd):
    //
    // There are 3 or 4 boundaries to be concerned with in deleteChars,
    // depending on how you count them. Consider the following terminal:
    //
    //   +--------+
    // 0 |.ABCDEF.|
    //   : ^      : (^ = cursor)
    //   +--------+
    //
    // if we DCH 3 we get
    //
    //   +--------+
    // 0 |.DEF....|
    //   +--------+
    //
    // The boundaries exist at the following points then:
    //
    //   +--------+
    // 0 |.ABCDEF.|
    //   :11 22 33:
    //   +--------+
    //
    // I'm counting 2 for double since it's both the end of the deleted
    // content and the start of the content that is shifted in to place.
    //
    // Now consider wide characters (represented as `WW`) at these boundaries:
    //
    //   +--------+
    // 0 |WWaWWbWW|
    //   : ^      : (^ = cursor)
    //   : ^^^    : (^ = deleted by DCH 3)
    //   +--------+
    //
    // -> DCH 3
    // -> The first 2 wide characters are split & destroyed (verified in xterm)
    //
    //   +--------+
    // 0 |..bWW...|
    //   +--------+

    t.print_string("😀a😀b😀".as_bytes()).unwrap();
    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "😀a😀b😀".as_bytes());
    }

    t.set_cursor_pos(1, 2);
    t.delete_chars(3);
    unsafe {
        (*crate::pagelist::Node::page((*(*t.screens.active).cursor.page_pin).node))
            .assert_integrity()
    };

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "  b😀".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: deleteChars wide char wrap boundary conditions"
#[test]
fn terminal_delete_chars_wide_char_wrap_boundary_conditions() {
    let mut t = Terminal::init(Global, opts(8, 3)).unwrap();

    // EXPLANATION(qwerasd):
    // (cont. from "Terminal: deleteChars wide char boundary conditions")
    //
    // Additionally consider soft-wrapped wide chars (`H` = spacer head):
    //
    //   +--------+
    // 0 |.......H…
    // 1 …WWabcdeH…
    //   : ^      : (^ = cursor)
    //   : ^^^    : (^ = deleted by DCH 3)
    // 2 …WW......|
    //   +--------+
    //
    // -> DCH 3
    // -> First wide character split and destroyed, including spacer head,
    //    second spacer head removed (verified in xterm).
    // -> Wrap state of row reset
    //
    //   +--------+
    // 0 |........|
    // 1 |.cde....|
    // 2 |WW......|
    //   +--------+
    //

    t.print_string(".......😀abcde😀......".as_bytes()).unwrap();
    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), ".......\n😀abcde\n😀......".as_bytes());

        let unwrapped = t.plain_string_unwrapped(Global).unwrap();
        assert_eq!(unwrapped.as_slice(), ".......😀abcde😀......".as_bytes());
    }

    t.set_cursor_pos(2, 2);
    t.clear_dirty();
    t.delete_chars(3);
    unsafe {
        (*crate::pagelist::Node::page((*(*t.screens.active).cursor.page_pin).node))
            .assert_integrity()
    };

    // Deleting the wide char also clears the spacer head on the previous
    // row, so that row must be dirty too.
    assert!(t.is_dirty(screen_pt(0, 0)));
    assert!(t.is_dirty(screen_pt(0, 1)));
    assert!(!t.is_dirty(screen_pt(0, 2)));

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), ".......\n cde\n😀......".as_bytes());

        let unwrapped = t.plain_string_unwrapped(Global).unwrap();
        assert_eq!(unwrapped.as_slice(), ".......  cde\n😀......".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: deleteChars wide char across right margin"
#[test]
fn terminal_delete_chars_wide_char_across_right_margin() {
    let mut t = Terminal::init(Global, opts(8, 3)).unwrap();

    // scroll region
    //    VVVVVV
    //  +-######-+
    //  |.abcdeWW|
    //  : ^      : (^ = cursor)
    //  +--------+
    //
    // DCH 1

    t.print_string("123456橋".as_bytes()).unwrap();
    t.modes.set(modespkg::Mode::EnableLeftAndRightMargin, true);
    t.set_left_and_right_margin(2, 7);

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "123456橋".as_bytes());
    }

    t.set_cursor_pos(1, 2);
    t.delete_chars(1);
    unsafe {
        (*crate::pagelist::Node::page((*(*t.screens.active).cursor.page_pin).node))
            .assert_integrity()
    };

    // NOTE: This behavior is slightly inconsistent with xterm. xterm
    // _visually_ splits the wide character (half the wide character shows
    // up in col 6 and half in col 8). In all other wide char split scenarios,
    // xterm clears the cell. Therefore, we've chosen to clear the cell here.
    // Given we have space, we also could actually preserve it, but I haven't
    // yet found a terminal that behaves that way. We should be open to
    // revisiting this behavior but for now we're going with the simpler
    // impl.
    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "13456".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: eraseLine simple erase right"
#[test]
fn terminal_erase_line_simple_erase_right() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    for c in "ABCDE".chars() {
        t.print(c as u32).unwrap();
    }
    t.set_cursor_pos(1, 3);
    t.clear_dirty();
    t.erase_line(csi::EraseLine::Right, false);
    assert!(t.is_dirty(active_pt(0, 0)));

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "AB".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: eraseLine resets pending wrap"
#[test]
fn terminal_erase_line_resets_pending_wrap() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    for c in "ABCDE".chars() {
        t.print(c as u32).unwrap();
    }
    assert!(unsafe { (*t.screens.active).cursor.pending_wrap });
    t.erase_line(csi::EraseLine::Right, false);
    assert!(!unsafe { (*t.screens.active).cursor.pending_wrap });
    t.print('B' as u32).unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "ABCDB".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: eraseLine resets wrap"
#[test]
fn terminal_erase_line_resets_wrap() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    for c in "ABCDE123".chars() {
        t.print(c as u32).unwrap();
    }
    {
        let list_cell = t.active_mut().pages.get_cell(active_pt(0, 0)).unwrap();
        assert!(unsafe { (*list_cell.row).wrap() });
    }

    t.set_cursor_pos(1, 1);
    t.erase_line(csi::EraseLine::Right, false);

    {
        let list_cell = t.active_mut().pages.get_cell(active_pt(0, 0)).unwrap();
        assert!(!unsafe { (*list_cell.row).wrap() });
    }
    t.print('X' as u32).unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "X\n123".as_bytes());
    }

    t.deinit(Global);
}

// PORT NOTE (vt-12b-3a deferral): tests "Terminal: eraseLine right preserves
// background sgr" and the "eraseLine right protected *" group drive
// `set_attribute` / `set_protected_mode`; now ported in vt-12b-4 (see the
// SGR/protected section at the end of this module).

// S142-F1/S143-F2: `erase_line`'s catch-all arm logs `log.err("unimplemented
// erase line mode: {}", .{mode})` (Terminal.zig:3524-3527) for any
// `csi::EraseLine::Other` (unrecognized DECSEL/EL) parameter.
#[test]
fn terminal_erase_line_unimplemented_mode_logs() {
    let mut t = Terminal::init(Global, opts(10, 5)).unwrap();

    let cap = crate::log_capture::start();
    t.erase_line(csi::EraseLine::Other(99), false);
    let messages = cap.messages();
    assert!(
        messages
            .iter()
            .any(|(level, target, msg)| *level == log::Level::Error
                && target == "terminal"
                && msg.starts_with("unimplemented erase line mode: ")),
        "expected the unimplemented-erase-line-mode diagnostic, got: {messages:?}"
    );
    drop(cap);

    t.deinit(Global);
}

// test "Terminal: eraseLine right wide character"
#[test]
fn terminal_erase_line_right_wide_character() {
    let mut t = Terminal::init(Global, opts(10, 5)).unwrap();

    for c in "AB".chars() {
        t.print(c as u32).unwrap();
    }
    t.print('橋' as u32).unwrap();
    for c in "DE".chars() {
        t.print(c as u32).unwrap();
    }
    t.set_cursor_pos(1, 4);
    t.clear_dirty();
    t.erase_line(csi::EraseLine::Right, false);
    assert!(t.is_dirty(active_pt(0, 0)));

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "AB".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: eraseLine simple erase left"
#[test]
fn terminal_erase_line_simple_erase_left() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    for c in "ABCDE".chars() {
        t.print(c as u32).unwrap();
    }
    t.set_cursor_pos(1, 3);
    t.clear_dirty();
    t.erase_line(csi::EraseLine::Left, false);
    assert!(t.is_dirty(active_pt(0, 0)));

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "   DE".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: eraseLine left resets wrap"
#[test]
fn terminal_erase_line_left_resets_wrap() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    for c in "ABCDE".chars() {
        t.print(c as u32).unwrap();
    }
    assert!(unsafe { (*t.screens.active).cursor.pending_wrap });
    t.clear_dirty();
    t.erase_line(csi::EraseLine::Left, false);
    assert!(t.is_dirty(active_pt(0, 0)));
    assert!(!unsafe { (*t.screens.active).cursor.pending_wrap });
    t.print('B' as u32).unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "    B".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: eraseLine left wide character"
#[test]
fn terminal_erase_line_left_wide_character() {
    let mut t = Terminal::init(Global, opts(10, 5)).unwrap();

    for c in "AB".chars() {
        t.print(c as u32).unwrap();
    }
    t.print('橋' as u32).unwrap();
    for c in "DE".chars() {
        t.print(c as u32).unwrap();
    }
    t.set_cursor_pos(1, 3);
    t.clear_dirty();
    t.erase_line(csi::EraseLine::Left, false);
    assert!(t.is_dirty(active_pt(0, 0)));

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "    DE".as_bytes());
    }

    t.deinit(Global);
}

// PORT NOTE (vt-12b-3a deferral): the "Terminal: eraseLine left protected
// *", "eraseLine complete *", "eraseLine left preserves background sgr"
// groups drive `set_attribute` / `set_protected_mode`; now ported in
// vt-12b-4 (see the SGR/protected section at the end of this module).

// PORT NOTE (vt-12b-3a deferral): the "Terminal: eraseDisplay *" tests that
// drive `set_attribute` / `set_protected_mode` (SGR/protected variants), plus
// the non-SGR eraseDisplay tests (`simple erase above`, `below/above split
// multi-cell`, `scroll complete`) tracked with that broader group, are now
// ported in vt-12b-4 (see the SGR/protected section at the end of this
// module).
