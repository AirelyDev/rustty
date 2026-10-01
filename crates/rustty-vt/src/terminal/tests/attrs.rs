//! PORTS: ghostty src/terminal/Terminal.zig — the SGR (`setAttribute` / `printAttributes`), printSlice and protected-mode tests.

use super::*;

// ===================================================================
// vt-12b-4: SGR (setAttribute / printAttributes) + protected-mode tests
// (the groups deferred from vt-12b-3a/3b that drive `set_attribute` /
//  `set_protected_mode`, plus the eraseDisplay group now unblocked).
// ===================================================================

// test "Terminal: bold style"
#[test]
fn terminal_bold_style() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.set_attribute(sgr::Attribute::Bold).unwrap();
    t.print('A' as u32).unwrap();

    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).content_codepoint() }, 'A' as u21);
        assert!(unsafe { (*cell).style_id() } != 0);
        let page =
            unsafe { crate::pagelist::Node::page((*(*t.screens.active).cursor.page_pin).node) };
        assert!(
            unsafe {
                (*page)
                    .styles
                    .ref_count((*page).memory_ptr(), (*t.screens.active).cursor.style_id)
            } > 1
        );
    }

    t.deinit(Global);
}

// test "Terminal: eraseChars preserves background sgr"
#[test]
fn terminal_erase_chars_preserves_background_sgr() {
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
    t.erase_chars(2);

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "  C".as_bytes());
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
        {
            let list_cell = t.active_mut().pages.get_cell(active_pt(1, 0)).unwrap();
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

// test "Terminal: eraseChars handles refcounted styles"
#[test]
fn terminal_erase_chars_handles_refcounted_styles() {
    let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

    t.set_attribute(sgr::Attribute::Bold).unwrap();
    t.print('A' as u32).unwrap();
    t.print('B' as u32).unwrap();
    t.set_attribute(sgr::Attribute::Unset).unwrap();
    t.print('C' as u32).unwrap();

    // verify we have styles in our style map
    let page = unsafe { crate::pagelist::Node::page((*(*t.screens.active).cursor.page_pin).node) };
    assert_eq!(1usize, unsafe { (*page).styles.count() });

    t.set_cursor_pos(1, 1);
    t.erase_chars(2);

    // verify we have no styles in our style map
    assert_eq!(0usize, unsafe { (*page).styles.count() });

    t.deinit(Global);
}

// test "Terminal: eraseChars protected attributes respected with iso"
#[test]
fn terminal_erase_chars_protected_attributes_respected_with_iso() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.set_protected_mode(ansi::ProtectedMode::Iso);
    for c in "ABC".chars() {
        t.print(c as u32).unwrap();
    }
    t.set_cursor_pos(1, 1);
    t.erase_chars(2);

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "ABC".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: eraseChars protected attributes ignored with dec most recent"
#[test]
fn terminal_erase_chars_protected_attributes_ignored_with_dec_most_recent() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.set_protected_mode(ansi::ProtectedMode::Iso);
    for c in "ABC".chars() {
        t.print(c as u32).unwrap();
    }
    t.set_protected_mode(ansi::ProtectedMode::Dec);
    t.set_protected_mode(ansi::ProtectedMode::Off);
    t.set_cursor_pos(1, 1);
    t.erase_chars(2);

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "  C".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: eraseChars protected attributes ignored with dec set"
#[test]
fn terminal_erase_chars_protected_attributes_ignored_with_dec_set() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.set_protected_mode(ansi::ProtectedMode::Dec);
    for c in "ABC".chars() {
        t.print(c as u32).unwrap();
    }
    t.set_cursor_pos(1, 1);
    t.erase_chars(2);

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "  C".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: deleteChars preserves background sgr"
#[test]
fn terminal_delete_chars_preserves_background_sgr() {
    let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

    for c in "ABC123".chars() {
        t.print(c as u32).unwrap();
    }
    t.set_cursor_pos(1, 3);
    t.set_attribute(sgr::Attribute::DirectColorBg(color::RGB {
        r: 0xFF,
        g: 0,
        b: 0,
    }))
    .unwrap();
    t.delete_chars(2);

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "AB23".as_bytes());
    }
    for x in t.cols - 2..t.cols {
        let list_cell = t.active_mut().pages.get_cell(active_pt(x, 0)).unwrap();
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

// test "Terminal: eraseLine right preserves background sgr"
#[test]
fn terminal_erase_line_right_preserves_background_sgr() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    for c in "ABCDE".chars() {
        t.print(c as u32).unwrap();
    }
    t.set_cursor_pos(1, 2);
    t.set_attribute(sgr::Attribute::DirectColorBg(color::RGB {
        r: 0xFF,
        g: 0,
        b: 0,
    }))
    .unwrap();
    t.erase_line(csi::EraseLine::Right, false);

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "A".as_bytes());
        for x in 1..5 {
            let list_cell = t.active_mut().pages.get_cell(active_pt(x, 0)).unwrap();
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

// test "Terminal: eraseLine right protected attributes respected with iso"
#[test]
fn terminal_erase_line_right_protected_attributes_respected_with_iso() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.set_protected_mode(ansi::ProtectedMode::Iso);
    for c in "ABC".chars() {
        t.print(c as u32).unwrap();
    }
    t.set_cursor_pos(1, 1);
    t.clear_dirty();
    t.erase_line(csi::EraseLine::Right, false);
    assert!(t.is_dirty(active_pt(0, 0)));

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "ABC".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: eraseLine right protected attributes ignored with dec most recent"
#[test]
fn terminal_erase_line_right_protected_attributes_ignored_with_dec_most_recent() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.set_protected_mode(ansi::ProtectedMode::Iso);
    for c in "ABC".chars() {
        t.print(c as u32).unwrap();
    }
    t.set_protected_mode(ansi::ProtectedMode::Dec);
    t.set_protected_mode(ansi::ProtectedMode::Off);
    t.set_cursor_pos(1, 2);
    t.clear_dirty();
    t.erase_line(csi::EraseLine::Right, false);
    assert!(t.is_dirty(active_pt(0, 0)));

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "A".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: eraseLine right protected attributes ignored with dec set"
#[test]
fn terminal_erase_line_right_protected_attributes_ignored_with_dec_set() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.set_protected_mode(ansi::ProtectedMode::Dec);
    for c in "ABC".chars() {
        t.print(c as u32).unwrap();
    }
    t.set_cursor_pos(1, 2);
    t.clear_dirty();
    t.erase_line(csi::EraseLine::Right, false);
    assert!(t.is_dirty(active_pt(0, 0)));

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "A".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: eraseLine right protected requested"
#[test]
fn terminal_erase_line_right_protected_requested() {
    let mut t = Terminal::init(Global, opts(10, 5)).unwrap();

    for c in "12345678".chars() {
        t.print(c as u32).unwrap();
    }
    t.set_cursor_pos((unsafe { (*t.screens.active).cursor.y } as usize) + 1, 6);
    t.set_protected_mode(ansi::ProtectedMode::Dec);
    t.print('X' as u32).unwrap();
    t.set_cursor_pos((unsafe { (*t.screens.active).cursor.y } as usize) + 1, 4);
    t.clear_dirty();
    t.erase_line(csi::EraseLine::Right, true);
    assert!(t.is_dirty(active_pt(0, 0)));

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "123  X".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: eraseLine left preserves background sgr"
#[test]
fn terminal_erase_line_left_preserves_background_sgr() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    for c in "ABCDE".chars() {
        t.print(c as u32).unwrap();
    }
    t.set_cursor_pos(1, 2);
    t.set_attribute(sgr::Attribute::DirectColorBg(color::RGB {
        r: 0xFF,
        g: 0,
        b: 0,
    }))
    .unwrap();
    t.erase_line(csi::EraseLine::Left, false);

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "  CDE".as_bytes());
        for x in 0..2 {
            let list_cell = t.active_mut().pages.get_cell(active_pt(x, 0)).unwrap();
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

// test "Terminal: eraseLine left protected attributes respected with iso"
#[test]
fn terminal_erase_line_left_protected_attributes_respected_with_iso() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.set_protected_mode(ansi::ProtectedMode::Iso);
    for c in "ABC".chars() {
        t.print(c as u32).unwrap();
    }
    t.set_cursor_pos(1, 1);
    t.clear_dirty();
    t.erase_line(csi::EraseLine::Left, false);
    assert!(t.is_dirty(active_pt(0, 0)));

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "ABC".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: eraseLine left protected attributes ignored with dec most recent"
#[test]
fn terminal_erase_line_left_protected_attributes_ignored_with_dec_most_recent() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.set_protected_mode(ansi::ProtectedMode::Iso);
    for c in "ABC".chars() {
        t.print(c as u32).unwrap();
    }
    t.set_protected_mode(ansi::ProtectedMode::Dec);
    t.set_protected_mode(ansi::ProtectedMode::Off);
    t.set_cursor_pos(1, 2);
    t.clear_dirty();
    t.erase_line(csi::EraseLine::Left, false);
    assert!(t.is_dirty(active_pt(0, 0)));

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "  C".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: eraseLine left protected attributes ignored with dec set"
#[test]
fn terminal_erase_line_left_protected_attributes_ignored_with_dec_set() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.set_protected_mode(ansi::ProtectedMode::Dec);
    for c in "ABC".chars() {
        t.print(c as u32).unwrap();
    }
    t.set_cursor_pos(1, 2);
    t.clear_dirty();
    t.erase_line(csi::EraseLine::Left, false);
    assert!(t.is_dirty(active_pt(0, 0)));

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "  C".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: eraseLine left protected requested"
#[test]
fn terminal_erase_line_left_protected_requested() {
    let mut t = Terminal::init(Global, opts(10, 5)).unwrap();

    for c in "123456789".chars() {
        t.print(c as u32).unwrap();
    }
    t.set_cursor_pos((unsafe { (*t.screens.active).cursor.y } as usize) + 1, 6);
    t.set_protected_mode(ansi::ProtectedMode::Dec);
    t.print('X' as u32).unwrap();
    t.set_cursor_pos((unsafe { (*t.screens.active).cursor.y } as usize) + 1, 8);
    t.clear_dirty();
    t.erase_line(csi::EraseLine::Left, true);
    assert!(t.is_dirty(active_pt(0, 0)));

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "     X  9".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: eraseLine complete preserves background sgr"
#[test]
fn terminal_erase_line_complete_preserves_background_sgr() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    for c in "ABCDE".chars() {
        t.print(c as u32).unwrap();
    }
    t.set_cursor_pos(1, 2);
    t.set_attribute(sgr::Attribute::DirectColorBg(color::RGB {
        r: 0xFF,
        g: 0,
        b: 0,
    }))
    .unwrap();
    t.erase_line(csi::EraseLine::Complete, false);

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "".as_bytes());
        for x in 0..5 {
            let list_cell = t.active_mut().pages.get_cell(active_pt(x, 0)).unwrap();
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

// test "Terminal: eraseLine complete resets wrap"
#[test]
fn terminal_erase_line_complete_resets_wrap() {
    let alloc = Global;
    let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();

    // for ("ABCDE123") |c| try t.print(c);
    for c in "ABCDE123".chars() {
        t.print(c as u32).unwrap();
    }
    {
        let list_cell = t.active_mut().pages.get_cell(active_pt(0, 0)).unwrap();
        // try testing.expect(list_cell.row.wrap);
        assert!(unsafe { (*list_cell.row).wrap() });
    }

    t.set_cursor_pos(1, 1);
    t.erase_line(csi::EraseLine::Complete, false);

    {
        let list_cell = t.active_mut().pages.get_cell(active_pt(0, 0)).unwrap();
        // try testing.expect(!list_cell.row.wrap);
        assert!(!unsafe { (*list_cell.row).wrap() });
    }
    t.print('X' as u32).unwrap();
    // try t.resize(alloc, .{ .rows = 5, .cols = 10 });
    t.resize(
        alloc,
        Resize {
            rows: 5,
            cols: 10,
            cell_size_px: None,
        },
    )
    .unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "X\n123".as_bytes());
    }

    t.deinit(alloc);
}

// test "Terminal: eraseLine complete protected attributes respected with iso"
#[test]
fn terminal_erase_line_complete_protected_attributes_respected_with_iso() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.set_protected_mode(ansi::ProtectedMode::Iso);
    for c in "ABC".chars() {
        t.print(c as u32).unwrap();
    }
    t.set_cursor_pos(1, 1);
    t.clear_dirty();
    t.erase_line(csi::EraseLine::Complete, false);
    assert!(t.is_dirty(active_pt(0, 0)));

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "ABC".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: eraseLine complete protected attributes ignored with dec most recent"
#[test]
fn terminal_erase_line_complete_protected_attributes_ignored_with_dec_most_recent() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.set_protected_mode(ansi::ProtectedMode::Iso);
    for c in "ABC".chars() {
        t.print(c as u32).unwrap();
    }
    t.set_protected_mode(ansi::ProtectedMode::Dec);
    t.set_protected_mode(ansi::ProtectedMode::Off);
    t.set_cursor_pos(1, 2);
    t.clear_dirty();
    t.erase_line(csi::EraseLine::Complete, false);
    assert!(t.is_dirty(active_pt(0, 0)));

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: eraseLine complete protected attributes ignored with dec set"
#[test]
fn terminal_erase_line_complete_protected_attributes_ignored_with_dec_set() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.set_protected_mode(ansi::ProtectedMode::Dec);
    for c in "ABC".chars() {
        t.print(c as u32).unwrap();
    }
    t.set_cursor_pos(1, 2);
    t.clear_dirty();
    t.erase_line(csi::EraseLine::Complete, false);
    assert!(t.is_dirty(active_pt(0, 0)));

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: eraseLine complete protected requested"
#[test]
fn terminal_erase_line_complete_protected_requested() {
    let mut t = Terminal::init(Global, opts(10, 5)).unwrap();

    for c in "123456789".chars() {
        t.print(c as u32).unwrap();
    }
    t.set_cursor_pos((unsafe { (*t.screens.active).cursor.y } as usize) + 1, 6);
    t.set_protected_mode(ansi::ProtectedMode::Dec);
    t.print('X' as u32).unwrap();
    t.set_cursor_pos((unsafe { (*t.screens.active).cursor.y } as usize) + 1, 8);
    t.clear_dirty();
    t.erase_line(csi::EraseLine::Complete, true);
    assert!(t.is_dirty(active_pt(0, 0)));

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "     X".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: printSlice simple ascii"
#[test]
fn terminal_print_slice_simple_ascii() {
    let mut t = Terminal::init(Global, opts(10, 3)).unwrap();

    t.print_slice(&['h' as u32, 'e' as u32, 'l' as u32, 'l' as u32, 'o' as u32])
        .unwrap();
    assert_eq!(5, unsafe { (*t.screens.active).cursor.x });
    assert_eq!('o' as u32, t.previous_char.unwrap());
    assert!(t.is_dirty(active_pt(0, 0)));

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "hello".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: printSlice charset batched fill"
#[test]
fn terminal_print_slice_charset_batched_fill() {
    let mut t = Terminal::init(Global, opts(5, 2)).unwrap();

    t.configure_charset(charsets::Slots::G1, charsets::Charset::DecSpecial);
    t.invoke_charset(charsets::ActiveSlot::GL, charsets::Slots::G1, false);
    t.modes.set(modespkg::Mode::GraphemeCluster, true);

    // Background-only cells use the general fill path.
    t.set_attribute(sgr::Attribute::Color8Bg(color::Name::Red))
        .unwrap();
    t.erase_display(csi::EraseDisplay::Complete, false);

    // Require batching when reusing cells and replacing styles.
    let cps = [
        'l' as u32, 'q' as u32, 'q' as u32, 'q' as u32, 'k' as u32, 'm' as u32, 'q' as u32,
        'q' as u32, 'q' as u32, 'j' as u32,
    ];
    for attr in [
        sgr::Attribute::Unset,
        sgr::Attribute::Unset,
        sgr::Attribute::Bold,
        sgr::Attribute::Bold,
        sgr::Attribute::Unset,
    ] {
        t.set_cursor_pos(1, 1);
        t.set_attribute(attr).unwrap();
        assert_eq!(cps.len(), t.print_slice_fast(&cps, true, false).unwrap());
        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "┌───┐\n└───┘".as_bytes());
        }
        assert_eq!('j' as u32, t.previous_char.unwrap());
        assert!(unsafe { (*t.screens.active).cursor.pending_wrap });
        for y in 0..2u32 {
            for x in 0..5 {
                let cell = unsafe {
                    (*t.screens.active)
                        .pages
                        .get_cell(active_pt(x, y))
                        .unwrap()
                        .cell
                };
                assert_eq!(unsafe { (*t.screens.active).cursor.style_id }, unsafe {
                    (*cell).style_id()
                });
            }
        }
        unsafe {
            let page = crate::pagelist::Node::page((*(*t.screens.active).cursor.page_pin).node);
            (*page).verify_integrity().unwrap();
        }
    }

    t.deinit(Global);
}

// test "Terminal: printSlice charset matches scalar printing"
#[test]
fn terminal_print_slice_charset_matches_scalar_printing() {
    for set in [charsets::Charset::DecSpecial, charsets::Charset::British] {
        for grapheme_cluster in [false, true] {
            let mut scalar = Terminal::init(Global, opts(17, 4)).unwrap();
            let mut batched = Terminal::init(Global, opts(17, 4)).unwrap();

            for t in [&mut scalar, &mut batched] {
                t.configure_charset(charsets::Slots::G0, set);
                t.modes
                    .set(modespkg::Mode::GraphemeCluster, grapheme_cluster);
            }

            let mut bytes = [0u32; 240];
            for (i, cp) in bytes.iter_mut().enumerate() {
                *cp = (0x10 + i) as u32;
            }
            let mixed: [u32; 9] = [
                0x100, 'q' as u32, 0x301, 'x' as u32, 0x4E00, '#' as u32, 0xFE0F, 0x1F600,
                'j' as u32,
            ];
            for cps in [bytes.as_slice(), mixed.as_slice()] {
                for &cp in cps {
                    scalar.print(cp).unwrap();
                }
                batched.print_slice(cps).unwrap();

                let expected = unsafe {
                    (*scalar.screens.active)
                        .dump_string_alloc(
                            Global,
                            point::Point::Screen(point::Coordinate::default()),
                        )
                        .unwrap()
                };
                let actual = unsafe {
                    (*batched.screens.active)
                        .dump_string_alloc(
                            Global,
                            point::Point::Screen(point::Coordinate::default()),
                        )
                        .unwrap()
                };
                assert_eq!(expected, actual);
                assert_eq!(unsafe { (*scalar.screens.active).cursor.x }, unsafe {
                    (*batched.screens.active).cursor.x
                });
                assert_eq!(unsafe { (*scalar.screens.active).cursor.y }, unsafe {
                    (*batched.screens.active).cursor.y
                });
                assert_eq!(
                    unsafe { (*scalar.screens.active).cursor.pending_wrap },
                    unsafe { (*batched.screens.active).cursor.pending_wrap }
                );
                assert_eq!(scalar.previous_char, batched.previous_char);
            }
        }
    }
}

// test "Terminal: printSlice charset single shift and repeat"
#[test]
fn terminal_print_slice_charset_single_shift_and_repeat() {
    let mut t = Terminal::init(Global, opts(10, 2)).unwrap();

    t.configure_charset(charsets::Slots::G0, charsets::Charset::DecSpecial);
    t.configure_charset(charsets::Slots::G2, charsets::Charset::British);
    t.invoke_charset(charsets::ActiveSlot::GL, charsets::Slots::G2, true);
    t.print_slice(&['#' as u32, 'q' as u32]).unwrap();
    assert_eq!(None, unsafe { (*t.screens.active).charset.single_shift });
    t.print_repeat(2).unwrap();

    // REP uses the original byte with the current charset.
    t.configure_charset(charsets::Slots::G0, charsets::Charset::Ascii);
    t.print_repeat(2).unwrap();
    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "£───qq".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: printSlice wraps and scrolls"
#[test]
fn terminal_print_slice_wraps_and_scrolls() {
    let mut t = Terminal::init(Global, opts(5, 2)).unwrap();

    // 12 chars: fills row 1 (5), row 2 (5), wraps+scrolls, 2 more.
    t.print_slice(&[
        'a' as u32, 'b' as u32, 'c' as u32, 'd' as u32, 'e' as u32, 'f' as u32, 'g' as u32,
        'h' as u32, 'i' as u32, 'j' as u32, 'k' as u32, 'l' as u32,
    ])
    .unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "fghij\nkl".as_bytes());
    }
    assert_eq!(2, unsafe { (*t.screens.active).cursor.x });
    assert!(!unsafe { (*t.screens.active).cursor.pending_wrap });

    t.deinit(Global);
}

// test "Terminal: printSlice pending wrap state"
#[test]
fn terminal_print_slice_pending_wrap_state() {
    let mut t = Terminal::init(Global, opts(5, 2)).unwrap();

    t.print_slice(&['a' as u32, 'b' as u32, 'c' as u32, 'd' as u32, 'e' as u32])
        .unwrap();
    assert_eq!(4, unsafe { (*t.screens.active).cursor.x });
    assert!(unsafe { (*t.screens.active).cursor.pending_wrap });

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "abcde".as_bytes());
    }

    t.deinit(Global);
}

/// Differential testing helper: applies the same logical print
/// operations to two terminals, one using per-codepoint print() and
/// the other using printSlice() with random chunking, verifying that
/// the results are identical.
//
// PORT NOTE: `std.Random.DefaultPrng` + `rand.intRangeAtMost` become a
// seeded LCG plus a `range` helper below. The generator differs from Zig's
// xoshiro, so the exact operation sequence differs from upstream's — the
// test's value is the differential invariant (print vs print_slice must
// agree after EVERY op), which holds for any deterministic sequence, and
// the seeds are fixed so failures are reproducible.
fn test_print_slice_differential(
    rand: &mut TestRand,
    ops: usize,
    cols: size::CellCountInt,
    rows: size::CellCountInt,
) {
    let mut t1 = Terminal::init(Global, opts(cols, rows)).unwrap();
    let mut t2 = Terminal::init(Global, opts(cols, rows)).unwrap();

    // Alphabet of interesting codepoints: ascii, latin-1, combining
    // marks, CJK (wide), emoji (wide), ZWJ, variation selectors.
    let alphabet: [u32; 40] = [
        'a' as u32,
        'b' as u32,
        'Z' as u32,
        '0' as u32,
        ' ' as u32,
        0x10,
        0x1F,
        0x7F,
        'é' as u32,
        0xFF,
        0x301,
        0x4E00,
        0x4E01,
        0x1F600,
        0x200D,
        0xFE0F,
        'x' as u32,
        'y' as u32,
        0x1F9D1,
        0x0308,
        0xAD,
        0x3042,
        0xAC00,
        'q' as u32,
        'r' as u32,
        's' as u32,
        't' as u32,
        'u' as u32,
        'v' as u32,
        'w' as u32,
        '1' as u32,
        '2' as u32,
        0x1F1E6,
        0x1F1E7,
        0x1100,
        0x1161,
        0x11A8,
        0x200C,
        0x0430,
        0x03B1,
    ];

    let mut cps_buf: [u32; 64] = [0; 64];
    let mut last_n: usize = 0;

    for _ in 0..ops {
        match rand.range(0, 20) {
            // Print a run of codepoints (most common op).
            0..=9 => {
                let n = rand.range(1, cps_buf.len());
                last_n = n;
                for cp in cps_buf[0..n].iter_mut() {
                    *cp = alphabet[rand.range_less_than(alphabet.len())];
                }

                // t1: per-codepoint print
                for &cp in &cps_buf[0..n] {
                    t1.print(cp).unwrap();
                }

                // t2: printSlice with random chunking
                let mut i: usize = 0;
                while i < n {
                    let chunk = rand.range(1, n - i);
                    t2.print_slice(&cps_buf[i..i + chunk]).unwrap();
                    i += chunk;
                }
            }
            10 => {
                t1.carriage_return();
                t2.carriage_return();
                t1.linefeed().unwrap();
                t2.linefeed().unwrap();
            }
            11 => {
                let row = rand.range(1, rows as usize);
                let col = rand.range(1, cols as usize);
                t1.set_cursor_pos(row, col);
                t2.set_cursor_pos(row, col);
            }
            12 => {
                let attr: sgr::Attribute = match rand.range(0, 3) {
                    0 => sgr::Attribute::Unset,
                    1 => sgr::Attribute::Bold,
                    2 => sgr::Attribute::DirectColorFg(color::RGB {
                        r: rand.byte(),
                        g: rand.byte(),
                        b: rand.byte(),
                    }),
                    3 => sgr::Attribute::Color8Fg(color::Name::Red),
                    _ => unreachable!(),
                };
                t1.set_attribute(attr).unwrap();
                t2.set_attribute(attr).unwrap();
            }
            13 => {
                let v = rand.boolean();
                t1.modes.set(modespkg::Mode::Insert, v);
                t2.modes.set(modespkg::Mode::Insert, v);
            }
            14 => {
                let v = rand.boolean();
                t1.modes.set(modespkg::Mode::Wraparound, v);
                t2.modes.set(modespkg::Mode::Wraparound, v);
            }
            15 => {
                let v = rand.boolean();
                // PORT NOTE (oracle b287f6d1a, `P3-W1 T4`): the
                // `erase_display` workaround that used to sit here is
                // DELETED, exactly as upstream deletes it in
                // `testPrintSliceDifferential`. It existed to dodge the
                // `debug_assert!(!brk)` in `print`'s stored-cluster walk,
                // which that commit removes. The differential now exercises
                // live mode-2027 changes over existing screen content, which
                // is the coverage the workaround was suppressing.
                t1.modes.set(modespkg::Mode::GraphemeCluster, v);
                t2.modes.set(modespkg::Mode::GraphemeCluster, v);
            }
            16 => {
                // Margins.
                t1.modes.set(modespkg::Mode::EnableLeftAndRightMargin, true);
                t2.modes.set(modespkg::Mode::EnableLeftAndRightMargin, true);
                let left = rand.range(1, cols as usize / 2);
                let right = rand.range(cols as usize / 2, cols as usize);
                t1.set_left_and_right_margin(left, right);
                t2.set_left_and_right_margin(left, right);
            }
            17 => {
                t1.set_left_and_right_margin(0, 0);
                t2.set_left_and_right_margin(0, 0);
            }
            18 => {
                t1.active_mut()
                    .start_hyperlink(b"http://example.com", None)
                    .unwrap();
                t2.active_mut()
                    .start_hyperlink(b"http://example.com", None)
                    .unwrap();
            }
            19 => {
                t1.active_mut().end_hyperlink();
                t2.active_mut().end_hyperlink();
            }
            20 => {
                // rand.enumValue(charsets.Charset)
                let set: charsets::Charset = match rand.range_less_than(4) {
                    0 => charsets::Charset::Utf8,
                    1 => charsets::Charset::Ascii,
                    2 => charsets::Charset::British,
                    3 => charsets::Charset::DecSpecial,
                    _ => unreachable!(),
                };
                t1.configure_charset(charsets::Slots::G0, set);
                t2.configure_charset(charsets::Slots::G0, set);
            }
            _ => unreachable!(),
        }

        // Cursor state must match exactly after every op.
        assert_eq!(unsafe { (*t1.screens.active).cursor.x }, unsafe {
            (*t2.screens.active).cursor.x
        });
        assert_eq!(unsafe { (*t1.screens.active).cursor.y }, unsafe {
            (*t2.screens.active).cursor.y
        });
        assert_eq!(
            unsafe { (*t1.screens.active).cursor.pending_wrap },
            unsafe { (*t2.screens.active).cursor.pending_wrap }
        );

        // Full screen contents must match after every op. On failure,
        // dump diagnostics that make the failure reproducible.
        {
            let str1 = t1
                .active_mut()
                .dump_string_alloc(Global, point::Point::Screen(point::Coordinate::default()))
                .unwrap();
            let str2 = t2
                .active_mut()
                .dump_string_alloc(Global, point::Point::Screen(point::Coordinate::default()))
                .unwrap();
            if str1 != str2 {
                eprintln!("last print cps: {:?}", &cps_buf[0..last_n]);
                eprintln!(
                    "modes: 2027={} insert={} wrap={} sr.left={} sr.right={} cols={}",
                    t1.modes.get(modespkg::Mode::GraphemeCluster),
                    t1.modes.get(modespkg::Mode::Insert),
                    t1.modes.get(modespkg::Mode::Wraparound),
                    t1.scrolling_region.left,
                    t1.scrolling_region.right,
                    cols,
                );
                assert_eq!(
                    String::from_utf8_lossy(&str1),
                    String::from_utf8_lossy(&str2)
                );
            }
        }
    }

    // Page integrity (styles refcounts, grapheme maps, etc.) must hold.
    unsafe {
        let p1 = crate::pagelist::Node::page((*(*t1.screens.active).cursor.page_pin).node);
        (*p1).verify_integrity().unwrap();
        let p2 = crate::pagelist::Node::page((*(*t2.screens.active).cursor.page_pin).node);
        (*p2).verify_integrity().unwrap();
    }

    t1.deinit(Global);
    t2.deinit(Global);
}

/// Deterministic PRNG standing in for `std.Random.DefaultPrng` (see the
/// PORT NOTE on `test_print_slice_differential`).
struct TestRand(u64);

impl TestRand {
    fn init(seed: u64) -> TestRand {
        TestRand(seed)
    }
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 17
    }
    // rand.intRangeAtMost(T, lo, hi)
    fn range(&mut self, lo: usize, hi: usize) -> usize {
        if hi <= lo {
            return lo;
        }
        lo + (self.next() as usize) % (hi - lo + 1)
    }
    // rand.intRangeLessThan(T, 0, n)
    fn range_less_than(&mut self, n: usize) -> usize {
        (self.next() as usize) % n
    }
    // rand.boolean()
    fn boolean(&mut self) -> bool {
        self.next() & 1 == 1
    }
    // rand.int(u8)
    fn byte(&mut self) -> u8 {
        self.next() as u8
    }
}

// test "Terminal: printSlice differential fuzz vs print"
#[test]
// MIRI: 1700 randomized ops across four terminal geometries, each op
// followed by two full-screen dumps and a cursor comparison, plus two
// `verify_integrity` page walks at the end. Measured: killed at >40 min of
// interpreter time against this repo's 6-minute hard bound (the three
// sibling `terminal_print_slice_*` tests finish in ~10s each and DO run
// under Miri, covering the same `print_slice_fill` raw-pointer writes).
// Registered in .port/phase-2/MIRI_EXCEPTIONS.md (WS10 T16).
#[cfg_attr(miri, ignore)]
fn terminal_print_slice_differential_fuzz_vs_print() {
    // Multiple seeds and terminal sizes for coverage, including a
    // tiny terminal to stress wrap/scroll edge cases.
    let mut prng = TestRand::init(0xC0FFEE);
    test_print_slice_differential(&mut prng, 500, 80, 24);
    test_print_slice_differential(&mut prng, 500, 10, 4);
    test_print_slice_differential(&mut prng, 500, 5, 2);
    test_print_slice_differential(&mut prng, 200, 2, 2);
}

// test "Terminal: printAttributes"
#[test]
fn terminal_print_attributes() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    let mut storage: [u8; 64] = [0; 64];

    {
        t.set_attribute(sgr::Attribute::DirectColorFg(color::RGB {
            r: 1,
            g: 2,
            b: 3,
        }))
        .unwrap();
        let buf = t.print_attributes(&mut storage).unwrap();
        assert_eq!(buf, "0;38:2::1:2:3".as_bytes());
        t.set_attribute(sgr::Attribute::Unset).unwrap();
    }

    {
        t.set_attribute(sgr::Attribute::Bold).unwrap();
        t.set_attribute(sgr::Attribute::DirectColorBg(color::RGB {
            r: 1,
            g: 2,
            b: 3,
        }))
        .unwrap();
        let buf = t.print_attributes(&mut storage).unwrap();
        assert_eq!(buf, "0;1;48:2::1:2:3".as_bytes());
        t.set_attribute(sgr::Attribute::Unset).unwrap();
    }

    {
        t.set_attribute(sgr::Attribute::Bold).unwrap();
        t.set_attribute(sgr::Attribute::Faint).unwrap();
        t.set_attribute(sgr::Attribute::Italic).unwrap();
        t.set_attribute(sgr::Attribute::Underline(sgr::Underline::Single))
            .unwrap();
        t.set_attribute(sgr::Attribute::Blink).unwrap();
        t.set_attribute(sgr::Attribute::Inverse).unwrap();
        t.set_attribute(sgr::Attribute::Invisible).unwrap();
        t.set_attribute(sgr::Attribute::Strikethrough).unwrap();
        t.set_attribute(sgr::Attribute::Overline).unwrap();
        t.set_attribute(sgr::Attribute::DirectColorFg(color::RGB {
            r: 100,
            g: 200,
            b: 255,
        }))
        .unwrap();
        t.set_attribute(sgr::Attribute::DirectColorBg(color::RGB {
            r: 101,
            g: 102,
            b: 103,
        }))
        .unwrap();
        let buf = t.print_attributes(&mut storage).unwrap();
        assert_eq!(
            buf,
            "0;1;2;3;4;53;5;7;8;9;38:2::100:200:255;48:2::101:102:103".as_bytes()
        );
        t.set_attribute(sgr::Attribute::Unset).unwrap();
    }

    // PORT NOTE (oracle cb2fef390, `P3-W1 T6`): the single-underline case
    // that used to stand alone here is now case 1 of a 5-case table. It is
    // deliberately RETAINED rather than replaced: without an
    // `underline == Single` case the test would pass even if the `;4:{n}`
    // branch were taken unconditionally.
    for (underline, expected) in [
        (sgr::Underline::Single, "0;4"),
        (sgr::Underline::Double, "0;4:2"),
        (sgr::Underline::Curly, "0;4:3"),
        (sgr::Underline::Dotted, "0;4:4"),
        (sgr::Underline::Dashed, "0;4:5"),
    ] {
        t.set_attribute(sgr::Attribute::Underline(underline))
            .unwrap();
        let buf = t.print_attributes(&mut storage).unwrap();
        assert_eq!(buf, expected.as_bytes(), "underline {underline:?}");
    }

    t.set_attribute(sgr::Attribute::Unset).unwrap();
    {
        let buf = t.print_attributes(&mut storage).unwrap();
        assert_eq!(buf, "0".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: eraseDisplay simple erase below"
#[test]
fn terminal_erase_display_simple_erase_below() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    for c in "ABC".chars() {
        t.print(c as u32).unwrap();
    }
    t.carriage_return();
    t.linefeed().unwrap();
    for c in "DEF".chars() {
        t.print(c as u32).unwrap();
    }
    t.carriage_return();
    t.linefeed().unwrap();
    for c in "GHI".chars() {
        t.print(c as u32).unwrap();
    }
    t.set_cursor_pos(2, 2);

    t.clear_dirty();
    t.erase_display(csi::EraseDisplay::Below, false);

    assert!(!t.is_dirty(active_pt(0, 0)));
    assert!(t.is_dirty(active_pt(0, 1)));
    assert!(t.is_dirty(active_pt(0, 2)));

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "ABC\nD".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: eraseDisplay erase below preserves SGR bg"
#[test]
fn terminal_erase_display_erase_below_preserves_sgr_bg() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    for c in "ABC".chars() {
        t.print(c as u32).unwrap();
    }
    t.carriage_return();
    t.linefeed().unwrap();
    for c in "DEF".chars() {
        t.print(c as u32).unwrap();
    }
    t.carriage_return();
    t.linefeed().unwrap();
    for c in "GHI".chars() {
        t.print(c as u32).unwrap();
    }
    t.set_cursor_pos(2, 2);

    t.set_attribute(sgr::Attribute::DirectColorBg(color::RGB {
        r: 0xFF,
        g: 0,
        b: 0,
    }))
    .unwrap();
    t.erase_display(csi::EraseDisplay::Below, false);

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "ABC\nD".as_bytes());
        for x in 1..5 {
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
    }

    t.deinit(Global);
}

// test "Terminal: eraseDisplay below split multi-cell"
#[test]
fn terminal_erase_display_below_split_multi_cell() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.print_string("AB橋C".as_bytes()).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print_string("DE橋F".as_bytes()).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print_string("GH橋I".as_bytes()).unwrap();
    t.set_cursor_pos(2, 4);
    t.erase_display(csi::EraseDisplay::Below, false);

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "AB橋C\nDE".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: eraseDisplay below protected attributes respected with iso"
#[test]
fn terminal_erase_display_below_protected_attributes_respected_with_iso() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.set_protected_mode(ansi::ProtectedMode::Iso);
    for c in "ABC".chars() {
        t.print(c as u32).unwrap();
    }
    t.carriage_return();
    t.linefeed().unwrap();
    for c in "DEF".chars() {
        t.print(c as u32).unwrap();
    }
    t.carriage_return();
    t.linefeed().unwrap();
    for c in "GHI".chars() {
        t.print(c as u32).unwrap();
    }
    t.set_cursor_pos(2, 2);
    t.erase_display(csi::EraseDisplay::Below, false);

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "ABC\nDEF\nGHI".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: eraseDisplay below protected attributes ignored with dec most recent"
#[test]
fn terminal_erase_display_below_protected_attributes_ignored_with_dec_most_recent() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.set_protected_mode(ansi::ProtectedMode::Iso);
    for c in "ABC".chars() {
        t.print(c as u32).unwrap();
    }
    t.carriage_return();
    t.linefeed().unwrap();
    for c in "DEF".chars() {
        t.print(c as u32).unwrap();
    }
    t.carriage_return();
    t.linefeed().unwrap();
    for c in "GHI".chars() {
        t.print(c as u32).unwrap();
    }
    t.set_protected_mode(ansi::ProtectedMode::Dec);
    t.set_protected_mode(ansi::ProtectedMode::Off);
    t.set_cursor_pos(2, 2);
    t.erase_display(csi::EraseDisplay::Below, false);

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "ABC\nD".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: eraseDisplay below protected attributes ignored with dec set"
#[test]
fn terminal_erase_display_below_protected_attributes_ignored_with_dec_set() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.set_protected_mode(ansi::ProtectedMode::Dec);
    for c in "ABC".chars() {
        t.print(c as u32).unwrap();
    }
    t.carriage_return();
    t.linefeed().unwrap();
    for c in "DEF".chars() {
        t.print(c as u32).unwrap();
    }
    t.carriage_return();
    t.linefeed().unwrap();
    for c in "GHI".chars() {
        t.print(c as u32).unwrap();
    }
    t.set_cursor_pos(2, 2);
    t.erase_display(csi::EraseDisplay::Below, false);

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "ABC\nD".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: eraseDisplay below protected attributes respected with force"
#[test]
fn terminal_erase_display_below_protected_attributes_respected_with_force() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.set_protected_mode(ansi::ProtectedMode::Dec);
    for c in "ABC".chars() {
        t.print(c as u32).unwrap();
    }
    t.carriage_return();
    t.linefeed().unwrap();
    for c in "DEF".chars() {
        t.print(c as u32).unwrap();
    }
    t.carriage_return();
    t.linefeed().unwrap();
    for c in "GHI".chars() {
        t.print(c as u32).unwrap();
    }
    t.set_cursor_pos(2, 2);
    t.erase_display(csi::EraseDisplay::Below, true);

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "ABC\nDEF\nGHI".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: eraseDisplay simple erase above"
#[test]
fn terminal_erase_display_simple_erase_above() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    for c in "ABC".chars() {
        t.print(c as u32).unwrap();
    }
    t.carriage_return();
    t.linefeed().unwrap();
    for c in "DEF".chars() {
        t.print(c as u32).unwrap();
    }
    t.carriage_return();
    t.linefeed().unwrap();
    for c in "GHI".chars() {
        t.print(c as u32).unwrap();
    }
    t.set_cursor_pos(2, 2);

    t.clear_dirty();
    t.erase_display(csi::EraseDisplay::Above, false);
    assert!(t.is_dirty(active_pt(0, 0)));
    assert!(t.is_dirty(active_pt(0, 1)));
    assert!(!t.is_dirty(active_pt(0, 2)));

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "\n  F\nGHI".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: eraseDisplay erase above preserves SGR bg"
#[test]
fn terminal_erase_display_erase_above_preserves_sgr_bg() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    for c in "ABC".chars() {
        t.print(c as u32).unwrap();
    }
    t.carriage_return();
    t.linefeed().unwrap();
    for c in "DEF".chars() {
        t.print(c as u32).unwrap();
    }
    t.carriage_return();
    t.linefeed().unwrap();
    for c in "GHI".chars() {
        t.print(c as u32).unwrap();
    }
    t.set_cursor_pos(2, 2);

    t.set_attribute(sgr::Attribute::DirectColorBg(color::RGB {
        r: 0xFF,
        g: 0,
        b: 0,
    }))
    .unwrap();
    t.erase_display(csi::EraseDisplay::Above, false);

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "\n  F\nGHI".as_bytes());
        for x in 0..2 {
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
    }

    t.deinit(Global);
}

// test "Terminal: eraseDisplay above split multi-cell"
#[test]
fn terminal_erase_display_above_split_multi_cell() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.print_string("AB橋C".as_bytes()).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print_string("DE橋F".as_bytes()).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print_string("GH橋I".as_bytes()).unwrap();
    t.set_cursor_pos(2, 3);
    t.erase_display(csi::EraseDisplay::Above, false);

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "\n    F\nGH橋I".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: eraseDisplay above protected attributes respected with iso"
#[test]
fn terminal_erase_display_above_protected_attributes_respected_with_iso() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.set_protected_mode(ansi::ProtectedMode::Iso);
    for c in "ABC".chars() {
        t.print(c as u32).unwrap();
    }
    t.carriage_return();
    t.linefeed().unwrap();
    for c in "DEF".chars() {
        t.print(c as u32).unwrap();
    }
    t.carriage_return();
    t.linefeed().unwrap();
    for c in "GHI".chars() {
        t.print(c as u32).unwrap();
    }
    t.set_cursor_pos(2, 2);
    t.erase_display(csi::EraseDisplay::Above, false);

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "ABC\nDEF\nGHI".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: eraseDisplay above protected attributes ignored with dec most recent"
#[test]
fn terminal_erase_display_above_protected_attributes_ignored_with_dec_most_recent() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.set_protected_mode(ansi::ProtectedMode::Iso);
    for c in "ABC".chars() {
        t.print(c as u32).unwrap();
    }
    t.carriage_return();
    t.linefeed().unwrap();
    for c in "DEF".chars() {
        t.print(c as u32).unwrap();
    }
    t.carriage_return();
    t.linefeed().unwrap();
    for c in "GHI".chars() {
        t.print(c as u32).unwrap();
    }
    t.set_protected_mode(ansi::ProtectedMode::Dec);
    t.set_protected_mode(ansi::ProtectedMode::Off);
    t.set_cursor_pos(2, 2);
    t.erase_display(csi::EraseDisplay::Above, false);

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "\n  F\nGHI".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: eraseDisplay above protected attributes ignored with dec set"
#[test]
fn terminal_erase_display_above_protected_attributes_ignored_with_dec_set() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.set_protected_mode(ansi::ProtectedMode::Dec);
    for c in "ABC".chars() {
        t.print(c as u32).unwrap();
    }
    t.carriage_return();
    t.linefeed().unwrap();
    for c in "DEF".chars() {
        t.print(c as u32).unwrap();
    }
    t.carriage_return();
    t.linefeed().unwrap();
    for c in "GHI".chars() {
        t.print(c as u32).unwrap();
    }
    t.set_cursor_pos(2, 2);
    t.erase_display(csi::EraseDisplay::Above, false);

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "\n  F\nGHI".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: eraseDisplay above protected attributes respected with force"
#[test]
fn terminal_erase_display_above_protected_attributes_respected_with_force() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.set_protected_mode(ansi::ProtectedMode::Dec);
    for c in "ABC".chars() {
        t.print(c as u32).unwrap();
    }
    t.carriage_return();
    t.linefeed().unwrap();
    for c in "DEF".chars() {
        t.print(c as u32).unwrap();
    }
    t.carriage_return();
    t.linefeed().unwrap();
    for c in "GHI".chars() {
        t.print(c as u32).unwrap();
    }
    t.set_cursor_pos(2, 2);
    t.erase_display(csi::EraseDisplay::Above, true);

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "ABC\nDEF\nGHI".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: eraseDisplay protected complete"
#[test]
fn terminal_erase_display_protected_complete() {
    let mut t = Terminal::init(Global, opts(10, 5)).unwrap();

    t.print('A' as u32).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    for c in "123456789".chars() {
        t.print(c as u32).unwrap();
    }
    t.set_cursor_pos((unsafe { (*t.screens.active).cursor.y } as usize) + 1, 6);
    t.set_protected_mode(ansi::ProtectedMode::Dec);
    t.print('X' as u32).unwrap();
    t.set_cursor_pos((unsafe { (*t.screens.active).cursor.y } as usize) + 1, 4);

    t.clear_dirty();
    t.erase_display(csi::EraseDisplay::Complete, true);
    for y in 0..t.rows {
        assert!(t.is_dirty(active_pt(0, y as u32)));
    }

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "\n     X".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: eraseDisplay protected below"
#[test]
fn terminal_erase_display_protected_below() {
    let mut t = Terminal::init(Global, opts(10, 5)).unwrap();

    t.print('A' as u32).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    for c in "123456789".chars() {
        t.print(c as u32).unwrap();
    }
    t.set_cursor_pos((unsafe { (*t.screens.active).cursor.y } as usize) + 1, 6);
    t.set_protected_mode(ansi::ProtectedMode::Dec);
    t.print('X' as u32).unwrap();
    t.set_cursor_pos((unsafe { (*t.screens.active).cursor.y } as usize) + 1, 4);
    t.erase_display(csi::EraseDisplay::Below, true);

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "A\n123  X".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: eraseDisplay scroll complete"
#[test]
fn terminal_erase_display_scroll_complete() {
    let mut t = Terminal::init(Global, opts(10, 5)).unwrap();

    t.print('A' as u32).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.erase_display(csi::EraseDisplay::ScrollComplete, false);

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: eraseDisplay protected above"
#[test]
fn terminal_erase_display_protected_above() {
    let mut t = Terminal::init(Global, opts(10, 3)).unwrap();

    t.print('A' as u32).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    for c in "123456789".chars() {
        t.print(c as u32).unwrap();
    }
    t.set_cursor_pos((unsafe { (*t.screens.active).cursor.y } as usize) + 1, 6);
    t.set_protected_mode(ansi::ProtectedMode::Dec);
    t.print('X' as u32).unwrap();
    t.set_cursor_pos((unsafe { (*t.screens.active).cursor.y } as usize) + 1, 8);
    t.erase_display(csi::EraseDisplay::Above, true);

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "\n     X  9".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: eraseDisplay complete preserves cursor"
#[test]
fn terminal_erase_display_complete_preserves_cursor() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    // Set our cursur
    t.set_attribute(sgr::Attribute::Bold).unwrap();
    t.print_string("AAAA".as_bytes()).unwrap();
    assert!(unsafe { (*t.screens.active).cursor.style_id } != style::DEFAULT_ID);

    // Erasing the display may detect that our style is no longer in use
    // and prune our style, which we don't want because its still our
    // active cursor.
    t.erase_display(csi::EraseDisplay::Complete, false);
    assert!(unsafe { (*t.screens.active).cursor.style_id } != style::DEFAULT_ID);

    t.deinit(Global);
}
