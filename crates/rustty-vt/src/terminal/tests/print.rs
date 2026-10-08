//! PORTS: ghostty src/terminal/Terminal.zig — the `print*` tests (graphemes, VS15/VS16, wide chars, charsets, wraparound, right margin, hyperlinks, styles, `printRepeat`).

use super::*;

// S141-F4: `print_inner_zero_width`'s "previous cell has no text" guard
// logs the SAME `log.warn("zero-width character with no prior
// character, ignoring", .{})` (Terminal.zig:1491) as the `x == 0` guard
// above, but on the OTHER branch: cursor not at column 0, yet the cell
// to its left was never written.
#[test]
fn terminal_zero_width_character_with_blank_prior_cell_logs() {
    let mut t = Terminal::init(Global, opts(10, 2)).unwrap();
    t.modes.set(modespkg::Mode::GraphemeCluster, false);

    // Move the cursor to a column whose left neighbor is blank, without
    // ever writing there (`set_cursor_pos` alone writes nothing).
    t.set_cursor_pos(1, 5);

    let cap = crate::log_capture::start();
    t.print(0x0332).unwrap(); // Combining low line — a zero-width mark.
    let messages = cap.messages();
    assert!(
        messages
            .iter()
            .any(|(level, target, msg)| *level == log::Level::Warn
                && target == "terminal"
                && *msg == "zero-width character with no prior character, ignoring"),
        "expected the zero-width-no-prior-character diagnostic, got: {messages:?}"
    );
    drop(cap);

    t.deinit(Global);
}

// Task S-1c (no reference test): a grapheme attached to the cell on its
// left logs the reference's debug line (Terminal.zig:1437).
#[test]
fn terminal_grapheme_attach_logs_a_debug_line() {
    let mut t = Terminal::init(Global, opts(10, 2)).unwrap();
    t.modes.set(modespkg::Mode::GraphemeCluster, true);
    t.print('e' as u32).unwrap();

    let cap = crate::log_capture::start();
    t.print(0x0301).unwrap(); // Combining acute accent.
    let messages = cap.messages();
    drop(cap);
    assert!(
        messages.contains(&(
            log::Level::Debug,
            "terminal".to_string(),
            "c=301 grapheme attach to left=1 primary_cp=65".to_string(),
        )),
        "expected the grapheme-attach debug line, got: {messages:?}"
    );

    t.deinit(Global);
}

// Task S-1c (no reference test): when the cursor copy fails on entering
// the alternate screen, `switchScreenMode` logs the reference's warning
// (Terminal.zig:4837 for 47/1047, :4857 for 1049) and carries on.
#[test]
fn terminal_alt_screen_cursor_copy_failure_logs_a_warning() {
    use crate::screen::{CursorCopyFp, CURSOR_COPY_TW};
    use rustty_util::tripwire::ResetMode;

    for (name, mode) in [
        ("47", SwitchScreenMode::M47),
        ("1047", SwitchScreenMode::M1047),
        ("1049", SwitchScreenMode::M1049),
    ] {
        let mut t = Terminal::init(Global, opts(10, 3)).unwrap();
        CURSOR_COPY_TW.with(|tw| {
            tw.borrow_mut().errorAlways(
                CursorCopyFp::ManualStyleUpdate,
                crate::pagelist::grow::IncreaseCapacityError::OutOfSpace,
            )
        });
        let cap = crate::log_capture::start();
        let result = t.switch_screen_mode(mode, true);
        let messages = cap.messages();
        drop(cap);
        CURSOR_COPY_TW
            .with(|tw| tw.borrow_mut().end(ResetMode::Reset))
            .unwrap();
        assert!(
            result.is_ok(),
            "{name}: the failure is logged, not returned"
        );
        assert_eq!(crate::screen_set::Key::Alternate, t.screens.active_key);
        assert!(
            messages.contains(&(
                log::Level::Warn,
                "terminal".to_string(),
                "cursor copy failed entering alt screen err=OutOfSpace".to_string(),
            )),
            "{name}: expected the cursor-copy warning, got: {messages:?}"
        );
        t.deinit(Global);
    }
}

// test "Terminal: zero-width character attaches to pending wrap cell"
// https://github.com/ghostty-org/ghostty/pull/12581
#[test]
fn terminal_zero_width_character_attaches_to_pending_wrap_cell() {
    let mut t = Terminal::init(Global, opts(2, 2)).unwrap();

    // Disable grapheme clustering to exercise the fallback path.
    t.modes.set(modespkg::Mode::GraphemeCluster, false);

    t.print('x' as u32).unwrap();
    t.print('å' as u32).unwrap();
    t.print(0x0332).unwrap(); // Combining low line.

    let str = t.plain_string(Global).unwrap();
    assert_eq!(str.as_slice(), "xå̲".as_bytes());

    t.deinit(Global);
}

// test "Terminal: caps zero-width codepoints attached to one cell"
#[test]
fn terminal_caps_zero_width_codepoints_attached_to_one_cell() {
    let mut t = Terminal::init(Global, opts(2, 2)).unwrap();

    t.modes.set(modespkg::Mode::GraphemeCluster, false);
    t.print('A' as u32).unwrap();

    let initial_capacity =
        unsafe { crate::pagelist::Node::capacity((*(*t.screens.active).cursor.page_pin).node) }
            .grapheme_bytes;
    for _ in 0..pagepkg::GRAPHEME_MAX_LEN * 4 {
        t.print(0x0301).unwrap();
    }

    let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
    assert_eq!(
        pagepkg::GRAPHEME_MAX_LEN,
        unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_grapheme(list_cell.cell) }
            .unwrap()
            .len(),
    );
    assert_eq!(
        initial_capacity,
        unsafe { crate::pagelist::Node::capacity(list_cell.node) }.grapheme_bytes
    );

    t.deinit(Global);
}

// test "Terminal: VS16 doesn't make character with 2027 disabled"
#[test]
fn terminal_vs16_doesnt_make_character_with_2027_disabled() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    // Disable grapheme clustering
    t.modes.set(modespkg::Mode::GraphemeCluster, false);

    t.print(0x2764).unwrap(); // Heart
    t.print(0xFE0F).unwrap(); // VS16 to make wide

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "❤️".as_bytes());
    }

    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).content_codepoint() }, 0x2764);
        assert!(unsafe { (*cell).has_grapheme() });
        assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Narrow);
        let cps = unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_grapheme(cell) }
            .unwrap();
        assert_eq!(cps.len(), 1);
    }

    t.deinit(Global);
}

// Terminal.print receives one codepoint at a time, so it can't use
// unicode::grapheme::grapheme_width directly; that API requires a complete
// buffered cluster or string end. This keeps the streaming printer's
// cursor advance in sync with the buffered measurement API. (oracle 65e61282a)
fn expect_grapheme_width_parity(cps: &[u32]) {
    let mut t = Terminal::init(Global, opts(80, 5)).unwrap();
    t.modes.set(modespkg::Mode::GraphemeCluster, true);

    let mut expected: usize = 0;
    let mut i: usize = 0;
    while i < cps.len() {
        let result = unicode::grapheme::grapheme_width(&cps[i..]);
        assert!(result.len > 0);
        i += result.len;
        expected += result.width as usize;
    }

    for &cp in cps {
        t.print(cp).unwrap();
    }
    assert_eq!(0, unsafe { (*t.screens.active).cursor.y });
    assert_eq!(expected, unsafe { (*t.screens.active).cursor.x } as usize);
    t.deinit(Global);
}

// test "Terminal: graphemeWidth parity" (oracle 65e61282a)
#[test]
fn terminal_grapheme_width_parity() {
    expect_grapheme_width_parity(&[0x2764, 0xFE0F]);
    expect_grapheme_width_parity(&['x' as u32, 0xFE0F, 0xFE0F]);
    expect_grapheme_width_parity(&[0x231A, 0xFE0E, 0xFE0F]);
    expect_grapheme_width_parity(&[0x1F3F4, 0x200D, 0x2620, 0xFE0F]);
    expect_grapheme_width_parity(&[0x1F468, 0x200D, 0x1F469, 0x200D, 0x1F467]);
    expect_grapheme_width_parity(&[0x23, 0xFE0F, 0x20E3]);
    expect_grapheme_width_parity(&['1' as u32, 0x20E3]);
    expect_grapheme_width_parity(&[0x1F44B, 0x1F3FF]);
    expect_grapheme_width_parity(&[0x1F1E6, 0x1F1E7, 0x1F1E8]);
    expect_grapheme_width_parity(&['a' as u32, 'b' as u32]);
    expect_grapheme_width_parity(&[0x0301, 0x0302]);
}

// test "Terminal: print invalid VS16 non-grapheme"
#[test]
fn terminal_print_invalid_vs16_non_grapheme() {
    let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

    // https://github.com/mitchellh/ghostty/issues/1482
    t.print('x' as u32).unwrap();
    t.print(0xFE0F).unwrap();

    // We should have 1 narrow cell.
    assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
    assert_eq!(unsafe { (*t.screens.active).cursor.x }, 1);

    // Assert various properties about our screen to verify
    // we have all expected cells.
    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).content_codepoint() }, 'x' as u21);
        assert!(!unsafe { (*cell).has_grapheme() });
        assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Narrow);
    }
    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(1, 0)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).content_codepoint() }, 0);
    }

    t.deinit(Global);
}

// test "Terminal: variation selectors apply to preceding codepoint"
// https://github.com/ghostty-org/ghostty/pull/12596
#[test]
fn terminal_variation_selectors_apply_to_preceding_codepoint() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    // Enable grapheme clustering
    t.modes.set(modespkg::Mode::GraphemeCluster, true);

    // Pirate flag: black flag + ZWJ + skull and crossbones + VS16.
    t.print(0x1F3F4).unwrap();
    t.print(0x200D).unwrap();
    t.print(0x2620).unwrap();
    t.print(0xFE0F).unwrap();

    let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
    let cell = list_cell.cell;
    assert_eq!(unsafe { (*cell).content_codepoint() }, 0x1F3F4);
    assert!(unsafe { (*cell).has_grapheme() });
    assert_eq!(
        unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_grapheme(cell) }.unwrap(),
        &[0x200D, 0x2620, 0xFE0F]
    );

    t.deinit(Global);
}

// test "Terminal: keypad sequence VS15"
#[test]
fn terminal_keypad_sequence_vs15() {
    let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

    // Enable grapheme clustering
    t.modes.set(modespkg::Mode::GraphemeCluster, true);

    // This is: "#︎" (number sign with text presentation selector)
    t.print(0x23).unwrap(); // # Number sign (valid base)
    t.print(0xFE0E).unwrap(); // VS15 (text presentation selector)

    // VS15 should combine with the base character into a single grapheme cluster,
    // taking 1 cell (narrow character).
    assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
    assert_eq!(unsafe { (*t.screens.active).cursor.x }, 1);

    // Row should be dirty
    assert!(t.is_dirty(screen_pt(0, 0)));

    // The base emoji should be in cell 0 with the skin tone as a grapheme
    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).content_codepoint() }, 0x23);
        assert!(unsafe { (*cell).has_grapheme() });
        assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Narrow);
    }

    t.deinit(Global);
}

// test "Terminal: keypad sequence VS16"
#[test]
fn terminal_keypad_sequence_vs16() {
    let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

    // Enable grapheme clustering
    t.modes.set(modespkg::Mode::GraphemeCluster, true);

    // This is: "#️" (number sign with emoji presentation selector)
    t.print(0x23).unwrap(); // # Number sign (valid base)
    t.print(0xFE0F).unwrap(); // VS16 (emoji presentation selector)

    // VS16 should combine with the base character into a single grapheme cluster,
    // taking 2 cells (wide character).
    assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
    assert_eq!(unsafe { (*t.screens.active).cursor.x }, 2);

    // Row should be dirty
    assert!(t.is_dirty(screen_pt(0, 0)));

    // The base emoji should be in cell 0 with the skin tone as a grapheme
    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).content_codepoint() }, 0x23);
        assert!(unsafe { (*cell).has_grapheme() });
        assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Wide);
    }

    t.deinit(Global);
}

// test "Terminal: print invalid VS15 following emoji is wide"
#[test]
fn terminal_print_invalid_vs15_following_emoji_is_wide() {
    let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

    // Enable grapheme clustering
    t.modes.set(modespkg::Mode::GraphemeCluster, true);

    t.print('\u{1F9E0}' as u32).unwrap(); // 🧠
    t.print(0xFE0E).unwrap(); // not valid with U+1F9E0 as base

    // We should have 2 cells taken up. It is one character but "wide".
    assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
    assert_eq!(unsafe { (*t.screens.active).cursor.x }, 2);

    // Assert various properties about our screen to verify
    // we have all expected cells.
    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).content_codepoint() }, '\u{1F9E0}' as u21);
        assert!(!unsafe { (*cell).has_grapheme() });
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

// test "Terminal: print invalid VS15 in emoji ZWJ sequence"
#[test]
fn terminal_print_invalid_vs15_in_emoji_zwj_sequence() {
    let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

    // Enable grapheme clustering
    t.modes.set(modespkg::Mode::GraphemeCluster, true);

    t.print('\u{1F469}' as u32).unwrap(); // 👩
    t.print(0xFE0E).unwrap(); // not valid with U+1F469 as base
    t.print('\u{200D}' as u32).unwrap(); // ZWJ
    t.print('\u{1F466}' as u32).unwrap(); // 👦

    // We should have 2 cells taken up. It is one character but "wide".
    assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
    assert_eq!(unsafe { (*t.screens.active).cursor.x }, 2);

    // Assert various properties about our screen to verify
    // we have all expected cells.
    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).content_codepoint() }, '\u{1F469}' as u21);
        assert!(unsafe { (*cell).has_grapheme() });
        assert_eq!(
            unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_grapheme(cell) }
                .unwrap(),
            &['\u{200D}' as u21, '\u{1F466}' as u21]
        );
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

// test "Terminal: print invalid VS16 grapheme"
#[test]
fn terminal_print_invalid_vs16_grapheme() {
    let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

    // Enable grapheme clustering
    t.modes.set(modespkg::Mode::GraphemeCluster, true);

    // https://github.com/mitchellh/ghostty/issues/1482
    t.print('x' as u32).unwrap();
    t.print(0xFE0F).unwrap(); // invalid VS16

    // We should have 1 cells taken up, and narrow.
    assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
    assert_eq!(unsafe { (*t.screens.active).cursor.x }, 1);

    // Assert various properties about our screen to verify
    // we have all expected cells.
    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).content_codepoint() }, 'x' as u21);
        assert!(!unsafe { (*cell).has_grapheme() });
        assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Narrow);
    }
    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(1, 0)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).content_codepoint() }, 0);
        assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Narrow);
    }

    t.deinit(Global);
}

// test "Terminal: print invalid VS16 with second char"
#[test]
fn terminal_print_invalid_vs16_with_second_char() {
    let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

    // Enable grapheme clustering
    t.modes.set(modespkg::Mode::GraphemeCluster, true);

    // https://github.com/mitchellh/ghostty/issues/1482
    t.print('x' as u32).unwrap();
    t.print(0xFE0F).unwrap();
    t.print('y' as u32).unwrap();

    // We should have 2 cells taken up, from two separate narrow characters.
    assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
    assert_eq!(unsafe { (*t.screens.active).cursor.x }, 2);

    // Assert various properties about our screen to verify
    // we have all expected cells.
    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).content_codepoint() }, 'x' as u21);
        assert!(!unsafe { (*cell).has_grapheme() });
        assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Narrow);
    }

    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(1, 0)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).content_codepoint() }, 'y' as u21);
        assert!(!unsafe { (*cell).has_grapheme() });
        assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Narrow);
    }

    t.deinit(Global);
}

// test "Terminal: print grapheme ò (o with nonspacing mark) should be narrow"
#[test]
fn terminal_print_grapheme_o_with_nonspacing_mark_should_be_narrow() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    // Enable grapheme clustering
    t.modes.set(modespkg::Mode::GraphemeCluster, true);

    t.print('o' as u32).unwrap();
    t.print(0x0300).unwrap(); // combining grave accent

    // We should have 1 cell taken up.
    assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
    assert_eq!(unsafe { (*t.screens.active).cursor.x }, 1);

    // Assert various properties about our screen to verify
    // we have all expected cells.
    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).content_codepoint() }, 'o' as u21);
        assert!(unsafe { (*cell).has_grapheme() });
        assert_eq!(
            unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_grapheme(cell) }
                .unwrap(),
            &[0x0300]
        );
        assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Narrow);
    }

    t.deinit(Global);
}

// test "Terminal: print Devanagari grapheme should be wide"
#[test]
fn terminal_print_devanagari_grapheme_should_be_wide() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    // Enable grapheme clustering
    t.modes.set(modespkg::Mode::GraphemeCluster, true);

    // क्‍ष
    t.print(0x0915).unwrap();
    t.print(0x094D).unwrap();
    t.print(0x200D).unwrap();
    t.print(0x0937).unwrap();

    // We should have 2 cells taken up. It is one character but "wide".
    assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
    assert_eq!(unsafe { (*t.screens.active).cursor.x }, 2);

    // Assert various properties about our screen to verify
    // we have all expected cells.
    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).content_codepoint() }, 0x0915);
        assert!(unsafe { (*cell).has_grapheme() });
        assert_eq!(
            unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_grapheme(cell) }
                .unwrap(),
            &[0x094D, 0x200D, 0x0937]
        );
        assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Wide);
    }
    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(1, 0)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::SpacerTail);
    }

    t.deinit(Global);
}

// test "Terminal: print invalid VS16 with second char (combining)"
#[test]
fn terminal_print_invalid_vs16_with_second_char_combining() {
    let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

    // Enable grapheme clustering
    t.modes.set(modespkg::Mode::GraphemeCluster, true);

    // https://github.com/mitchellh/ghostty/issues/1482
    t.print('n' as u32).unwrap();
    t.print(0xFE0F).unwrap(); // invalid VS16
    t.print(0x0303).unwrap(); // combining tilde

    // We should have 1 cells taken up, and narrow.
    assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
    assert_eq!(unsafe { (*t.screens.active).cursor.x }, 1);

    // Assert various properties about our screen to verify
    // we have all expected cells.
    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).content_codepoint() }, 'n' as u21);
        assert!(unsafe { (*cell).has_grapheme() });
        assert_eq!(
            unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_grapheme(cell) }
                .unwrap(),
            &['\u{0303}' as u21]
        );
        assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Narrow);
    }
    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(1, 0)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).content_codepoint() }, 0);
        assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Narrow);
    }

    t.deinit(Global);
}

// test "Terminal: print breaks valid grapheme cluster with Prepend + ASCII for speed"
#[test]
fn terminal_print_breaks_valid_grapheme_cluster_with_prepend_ascii_for_speed() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();
    t.modes.set(modespkg::Mode::GraphemeCluster, true);

    // Make sure we're not at cursor.x == 0 for the next char.
    t.print('_' as u32).unwrap();

    // U+0600 ARABIC NUMBER SIGN (Prepend)
    t.print(0x0600).unwrap();
    t.print('1' as u32).unwrap();

    // We should have 3 cells taken up, each narrow. Note that this is
    // **incorrect** grapheme break behavior, since a Prepend code point should
    // not break with the one following it per UAX #29 GB9b. However, as an
    // optimization we assume a grapheme break when c <= 255, and note that
    // this deviation only affects these very uncommon scenarios (e.g. the
    // Arabic number sign should precede Arabic-script digits).
    assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
    assert_eq!(unsafe { (*t.screens.active).cursor.x }, 3);
    // This is what we'd expect if we did break correctly:
    //try testing.expectEqual(@as(usize, 2), t.screens.active.cursor.x);

    // Assert various properties about our screen to verify
    // we have all expected cells.
    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(1, 0)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).content_codepoint() }, 0x0600);
        assert!(!unsafe { (*cell).has_grapheme() });
        // This is what we'd expect if we did break correctly:
        //try testing.expect(cell.hasGrapheme());
        //try testing.expectEqualSlices(u21, &.{'1'}, list_cell.node.data.lookupGrapheme(cell).?);
        assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Narrow);
    }
    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(2, 0)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).content_codepoint() }, '1' as u21);
        // This is what we'd expect if we did break correctly:
        //try testing.expectEqual(@as(u21, 0), cell.content.codepoint);
        assert!(!unsafe { (*cell).has_grapheme() });
        assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Narrow);
    }

    t.deinit(Global);
}

// test "Terminal: print invoke charset single"
#[test]
fn terminal_print_invoke_charset_single() {
    let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

    t.configure_charset(charsets::Slots::G1, charsets::Charset::DecSpecial);

    // Basic grid writing
    t.print('`' as u32).unwrap();
    t.invoke_charset(charsets::ActiveSlot::GL, charsets::Slots::G1, true);
    t.print('`' as u32).unwrap();
    t.print('`' as u32).unwrap();
    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "`◆`".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: print single very long line"
#[test]
fn terminal_print_single_very_long_line() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    // This would crash for issue 1400. So the assertion here is
    // that we simply do not crash.
    for _ in 0..1000 {
        t.print('x' as u32).unwrap();
    }

    t.deinit(Global);
}

// test "Terminal: print wide char"
#[test]
fn terminal_print_wide_char() {
    let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

    t.print(0x1F600).unwrap(); // Smiley face
    assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
    assert_eq!(unsafe { (*t.screens.active).cursor.x }, 2);

    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).content_codepoint() }, 0x1F600);
        assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Wide);
    }
    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(1, 0)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::SpacerTail);
    }

    assert!(t.is_dirty(screen_pt(0, 0)));

    t.deinit(Global);
}

// test "Terminal: print wide char at edge creates spacer head"
#[test]
fn terminal_print_wide_char_at_edge_creates_spacer_head() {
    let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

    t.set_cursor_pos(1, 10);
    t.print(0x1F600).unwrap(); // Smiley face
    assert_eq!(unsafe { (*t.screens.active).cursor.y }, 1);
    assert_eq!(unsafe { (*t.screens.active).cursor.x }, 2);

    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(9, 0)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::SpacerHead);
    }
    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 1)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).content_codepoint() }, 0x1F600);
        assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Wide);
    }
    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(1, 1)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::SpacerTail);
    }

    // Our first row just had a spacer head added which does not affect
    // rendering so only the place where the wide char was printed
    // should be marked.
    // BUT old row is dirty because cursor moved from there
    assert!(t.is_dirty(screen_pt(0, 0)));
    assert!(t.is_dirty(screen_pt(0, 1)));

    t.deinit(Global);
}

// test "Terminal: print wide char with 1-column width"
#[test]
fn terminal_print_wide_char_with_1_column_width() {
    let mut t = Terminal::init(Global, opts(1, 2)).unwrap();

    t.print('😀' as u32).unwrap(); // 0x1F600

    // This prints a space so we should be dirty.
    assert!(t.is_dirty(screen_pt(0, 0)));

    t.deinit(Global);
}

// test "Terminal: print wide char in single-width terminal"
#[test]
fn terminal_print_wide_char_in_single_width_terminal() {
    let mut t = Terminal::init(Global, opts(1, 80)).unwrap();

    t.print(0x1F600).unwrap(); // Smiley face
    assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
    assert_eq!(unsafe { (*t.screens.active).cursor.x }, 0);
    assert!(unsafe { (*t.screens.active).cursor.pending_wrap });

    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).content_codepoint() }, 0);
        assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Narrow);
    }

    assert!(t.is_dirty(screen_pt(0, 0)));

    t.deinit(Global);
}

// test "Terminal: print over wide char at 0,0"
#[test]
fn terminal_print_over_wide_char_at_0_0() {
    let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

    t.print(0x1F600).unwrap(); // Smiley face
    t.set_cursor_pos(0, 0);
    t.print('A' as u32).unwrap();

    assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
    assert_eq!(unsafe { (*t.screens.active).cursor.x }, 1);

    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).content_codepoint() }, 'A' as u21);
        assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Narrow);
    }
    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(1, 0)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).content_codepoint() }, 0);
        assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Narrow);
    }

    assert!(t.is_dirty(screen_pt(0, 0)));
    assert!(!t.is_dirty(screen_pt(0, 1)));

    t.deinit(Global);
}

// test "Terminal: print over wide char at col 0 corrupts previous row"
#[test]
fn terminal_print_over_wide_char_at_col_0_corrupts_previous_row() {
    // Crash found by AFL++ fuzzer (afl-out/stream/default/crashes/id:000002).
    //
    // printCell, when overwriting a wide cell with a narrow cell at x<=1
    // and y>0, sets the last cell of the previous row to .narrow — even
    // when that cell is a .spacer_tail rather than a .spacer_head. This
    // orphans the .wide cell at cols-2.
    let mut t = Terminal::init(Global, opts(10, 3)).unwrap();

    // Fill rows 0 and 1 with wide chars (5 per row on a 10-col terminal).
    for _ in 0..10 {
        t.print(0x4E2D).unwrap();
    }

    // Move cursor to row 1, col 0 (on top of a wide char) and print a
    // narrow character. This triggers printCell's .wide branch which
    // corrupts row 0's last cell: col 9 changes from .spacer_tail to
    // .narrow, orphaning the .wide at col 8.
    t.set_cursor_pos(2, 1);
    t.print('A' as u32).unwrap();

    // Row 1, col 0 should be narrow (we just overwrote the wide char).
    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 1)).unwrap();
        assert_eq!(unsafe { (*list_cell.cell).wide() }, pagepkg::Wide::Narrow);
    }
    // Row 0, col 8 should still be .wide (the last wide char on the row).
    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(8, 0)).unwrap();
        assert_eq!(unsafe { (*list_cell.cell).wide() }, pagepkg::Wide::Wide);
    }
    // Row 0, col 9 must remain .spacer_tail to pair with the .wide at col 8.
    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(9, 0)).unwrap();
        assert_eq!(
            unsafe { (*list_cell.cell).wide() },
            pagepkg::Wide::SpacerTail
        );
    }

    t.deinit(Global);
}

// test "Terminal: print over wide spacer tail"
#[test]
fn terminal_print_over_wide_spacer_tail() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.print('橋' as u32).unwrap();
    t.set_cursor_pos(1, 2);
    t.print('X' as u32).unwrap();

    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).content_codepoint() }, 0);
        assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Narrow);
    }
    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(1, 0)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).content_codepoint() }, 'X' as u21);
        assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Narrow);
    }

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), " X".as_bytes());
    }

    assert!(t.is_dirty(screen_pt(0, 0)));

    t.deinit(Global);
}

// test "Terminal: print over wide char with bold"
#[test]
fn terminal_print_over_wide_char_with_bold() {
    let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

    t.set_attribute(sgr::Attribute::Bold).unwrap();
    t.print(0x1F600).unwrap(); // Smiley face
                               // verify we have styles in our style map
    {
        let page =
            unsafe { crate::pagelist::Node::page((*(*t.screens.active).cursor.page_pin).node) };
        assert_eq!(1usize, unsafe { (*page).styles.count() });
    }

    // Go back and overwrite with no style
    t.set_cursor_pos(0, 0);
    t.set_attribute(sgr::Attribute::Unset).unwrap();
    t.print('A' as u32).unwrap(); // Smiley face

    // verify our style is gone
    {
        let page =
            unsafe { crate::pagelist::Node::page((*(*t.screens.active).cursor.page_pin).node) };
        assert_eq!(0usize, unsafe { (*page).styles.count() });
    }

    assert!(t.is_dirty(screen_pt(0, 0)));

    t.deinit(Global);
}

// test "Terminal: print over wide char with bg color"
#[test]
fn terminal_print_over_wide_char_with_bg_color() {
    let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

    t.set_attribute(sgr::Attribute::DirectColorBg(color::RGB {
        r: 0xFF,
        g: 0,
        b: 0,
    }))
    .unwrap();
    t.print(0x1F600).unwrap(); // Smiley face
                               // verify we have styles in our style map
    {
        let page =
            unsafe { crate::pagelist::Node::page((*(*t.screens.active).cursor.page_pin).node) };
        assert_eq!(1usize, unsafe { (*page).styles.count() });
    }

    // Go back and overwrite with no style
    t.set_cursor_pos(0, 0);
    t.set_attribute(sgr::Attribute::Unset).unwrap();
    t.print('A' as u32).unwrap(); // Smiley face

    // verify our style is gone
    {
        let page =
            unsafe { crate::pagelist::Node::page((*(*t.screens.active).cursor.page_pin).node) };
        assert_eq!(0usize, unsafe { (*page).styles.count() });
    }

    assert!(t.is_dirty(screen_pt(0, 0)));

    t.deinit(Global);
}

// test "Terminal: print multicodepoint grapheme, disabled mode 2027"
#[test]
fn terminal_print_multicodepoint_grapheme_disabled_mode_2027() {
    let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

    // https://github.com/mitchellh/ghostty/issues/289
    // This is: 👨‍👩‍👧 (which may or may not render correctly)
    t.print(0x1F468).unwrap();
    t.print(0x200D).unwrap();
    t.print(0x1F469).unwrap();
    t.print(0x200D).unwrap();
    t.print(0x1F467).unwrap();

    // We should have 6 cells taken up
    assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
    assert_eq!(unsafe { (*t.screens.active).cursor.x }, 6);

    // Assert various properties about our screen to verify
    // we have all expected cells.
    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).content_codepoint() }, 0x1F468);
        assert!(unsafe { (*cell).has_grapheme() });
        assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Wide);
        let cps = unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_grapheme(cell) }
            .unwrap();
        assert_eq!(cps.len(), 1);
    }
    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(1, 0)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).content_codepoint() }, 0);
        assert!(!unsafe { (*cell).has_grapheme() });
        assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::SpacerTail);
        assert!(
            unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_grapheme(cell) }
                .is_none()
        );
    }
    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(2, 0)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).content_codepoint() }, 0x1F469);
        assert!(unsafe { (*cell).has_grapheme() });
        assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Wide);
        let cps = unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_grapheme(cell) }
            .unwrap();
        assert_eq!(cps.len(), 1);
    }
    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(3, 0)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).content_codepoint() }, 0);
        assert!(!unsafe { (*cell).has_grapheme() });
        assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::SpacerTail);
        assert!(
            unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_grapheme(cell) }
                .is_none()
        );
    }
    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(4, 0)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).content_codepoint() }, 0x1F467);
        assert!(!unsafe { (*cell).has_grapheme() });
        assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Wide);
        assert!(
            unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_grapheme(cell) }
                .is_none()
        );
    }
    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(5, 0)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).content_codepoint() }, 0);
        assert!(!unsafe { (*cell).has_grapheme() });
        assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::SpacerTail);
        assert!(
            unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_grapheme(cell) }
                .is_none()
        );
    }

    assert!(t.is_dirty(screen_pt(0, 0)));

    t.deinit(Global);
}

// test "Terminal: print multicodepoint grapheme, mode 2027"
#[test]
fn terminal_print_multicodepoint_grapheme_mode_2027() {
    let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

    // Enable grapheme clustering
    t.modes.set(modespkg::Mode::GraphemeCluster, true);

    // https://github.com/mitchellh/ghostty/issues/289
    // This is: 👨‍👩‍👧 (which may or may not render correctly)
    t.print(0x1F468).unwrap();
    t.print(0x200D).unwrap();
    t.print(0x1F469).unwrap();
    t.print(0x200D).unwrap();
    t.print(0x1F467).unwrap();

    // We should have 2 cells taken up. It is one character but "wide".
    assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
    assert_eq!(unsafe { (*t.screens.active).cursor.x }, 2);

    // Row should be dirty
    assert!(t.is_dirty(screen_pt(0, 0)));

    // Assert various properties about our screen to verify
    // we have all expected cells.
    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).content_codepoint() }, 0x1F468);
        assert!(unsafe { (*cell).has_grapheme() });
        assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Wide);
        let cps = unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_grapheme(cell) }
            .unwrap();
        assert_eq!(cps.len(), 4);
    }
    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(1, 0)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).content_codepoint() }, 0);
        assert!(!unsafe { (*cell).has_grapheme() });
        assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::SpacerTail);
    }

    t.deinit(Global);
}

// RUSTTY: P7 Review Focus 4 (uucode 9d555245, Unicode 18, ghostty
// 12542b392). Upstream changed no terminal test for the bump; these pin
// the new East_Asian_Width=W ranges end to end through `print`:
// Jurchen U+18E00, Small Seal U+3D000 and the E18.0 emoji U+1F6D9 each
// advance the cursor by two cells.
#[test]
fn terminal_prints_unicode18_wide_ranges_as_two_cells() {
    let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

    for (i, cp) in [0x18E00u32, 0x3D000, 0x1F6D9].into_iter().enumerate() {
        t.print(cp).unwrap();
        let x = 2 * (i as u16 + 1);
        assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
        assert_eq!(
            unsafe { (*t.screens.active).cursor.x },
            x,
            "U+{cp:X} should be two cells wide"
        );

        let list_cell = t.active_mut().pages.get_cell(screen_pt(x - 2, 0)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).content_codepoint() }, cp);
        assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Wide);
        let list_cell = t.active_mut().pages.get_cell(screen_pt(x - 1, 0)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::SpacerTail);
    }

    t.deinit(Global);
}

// RUSTTY: P7 Review Focus 4. Unicode 18 gives U+1CF5 VEDIC SIGN
// JIHVAMULIYA InCB=Linker while its Grapheme_Cluster_Break stays Other
// (uucode's new `indic_conjunct_break_linker_other`). With mode 2027 it
// starts a new cluster after 'a' (no GB9 extend), but GB9c now joins
// `Linker Extend* x Consonant`, so U+0300 and U+0915 DEVANAGARI KA stay in
// its cell; KA is not zero-width in a grapheme, so the cluster is wide.
#[test]
fn terminal_keeps_incb_linker_other_in_one_cluster() {
    let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

    // Enable grapheme clustering
    t.modes.set(modespkg::Mode::GraphemeCluster, true);

    t.print('a' as u32).unwrap();
    t.print(0x1CF5).unwrap();
    t.print(0x0300).unwrap();
    t.print(0x0915).unwrap();

    assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
    assert_eq!(unsafe { (*t.screens.active).cursor.x }, 3);

    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).content_codepoint() }, 'a' as u32);
        assert!(!unsafe { (*cell).has_grapheme() });
    }
    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(1, 0)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).content_codepoint() }, 0x1CF5);
        assert!(unsafe { (*cell).has_grapheme() });
        assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Wide);
        let cps = unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_grapheme(cell) }
            .unwrap();
        assert_eq!(cps, &[0x0300, 0x0915]);
    }
    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(2, 0)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).content_codepoint() }, 0);
        assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::SpacerTail);
    }

    t.deinit(Global);
}

// test "Terminal: Fitzpatrick skin tone next valid base"
#[test]
fn terminal_fitzpatrick_skin_tone_next_valid_base() {
    let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

    // Enable grapheme clustering
    t.modes.set(modespkg::Mode::GraphemeCluster, true);

    // This is: "👋🏿" (waving hand with dark skin tone)
    t.print(0x1F44B).unwrap(); // 👋 Waving hand (valid base)
    t.print(0x1F3FF).unwrap(); // 🏿 Dark skin tone modifier

    // The skin tone should combine with the base emoji into a single grapheme cluster,
    // taking 2 cells (wide character).
    assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
    assert_eq!(unsafe { (*t.screens.active).cursor.x }, 2);

    // Row should be dirty
    assert!(t.is_dirty(screen_pt(0, 0)));

    // The base emoji should be in cell 0 with the skin tone as a grapheme
    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).content_codepoint() }, 0x1F44B);
        assert!(unsafe { (*cell).has_grapheme() });
        assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Wide);
    }

    t.deinit(Global);
}

// test "Terminal: Fitzpatrick skin tone next to non-base"
#[test]
fn terminal_fitzpatrick_skin_tone_next_to_non_base() {
    let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

    // Enable grapheme clustering
    t.modes.set(modespkg::Mode::GraphemeCluster, true);

    // This is: "🏿" (which may not render correctly in your editor!)
    t.print(0x22).unwrap(); // "
    t.print(0x1F3FF).unwrap(); // Dark skin tone
    t.print(0x22).unwrap(); // "

    // We should have 4 cells taken up. Importantly, the skin tone
    // should not join with the quotes.
    assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
    assert_eq!(unsafe { (*t.screens.active).cursor.x }, 4);

    // Row should be dirty
    assert!(t.is_dirty(screen_pt(0, 0)));

    // Assert various properties about our screen to verify
    // we have all expected cells.
    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).content_codepoint() }, 0x22);
        assert!(!unsafe { (*cell).has_grapheme() });
        assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Narrow);
    }
    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(1, 0)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).content_codepoint() }, 0x1F3FF);
        assert!(!unsafe { (*cell).has_grapheme() });
        assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Wide);
    }
    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(3, 0)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).content_codepoint() }, 0x22);
        assert!(!unsafe { (*cell).has_grapheme() });
        assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Narrow);
    }

    t.deinit(Global);
}

// test "Terminal: VS15 to make narrow character"
#[test]
fn terminal_vs15_to_make_narrow_character() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    // Enable grapheme clustering
    t.modes.set(modespkg::Mode::GraphemeCluster, true);

    t.print(0x2614).unwrap(); // Umbrella with rain drops, width=2
    assert!(t.is_dirty(screen_pt(0, 0)));
    t.clear_dirty();

    // We should have 2 cells taken up. It is one character but "wide".
    assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
    assert_eq!(unsafe { (*t.screens.active).cursor.x }, 2);

    t.print(0xFE0E).unwrap(); // VS15 to make narrow
    assert!(t.is_dirty(screen_pt(0, 0)));
    t.clear_dirty();

    // VS15 should send us back a cell since our char is no longer wide.
    assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
    assert_eq!(unsafe { (*t.screens.active).cursor.x }, 1);

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "☔︎".as_bytes());
    }

    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).content_codepoint() }, 0x2614);
        assert!(unsafe { (*cell).has_grapheme() });
        assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Narrow);
        let cps = unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_grapheme(cell) }
            .unwrap();
        assert_eq!(cps.len(), 1);
    }

    t.deinit(Global);
}

// test "Terminal: VS15 on already narrow emoji"
#[test]
fn terminal_vs15_on_already_narrow_emoji() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    // Enable grapheme clustering
    t.modes.set(modespkg::Mode::GraphemeCluster, true);

    t.print(0x26C8).unwrap(); // Thunder cloud and rain, width=1
    assert!(t.is_dirty(screen_pt(0, 0)));
    t.clear_dirty();
    t.print(0xFE0E).unwrap(); // VS15 to make narrow
    assert!(t.is_dirty(screen_pt(0, 0)));
    t.clear_dirty();

    // Character takes up one cell
    assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
    assert_eq!(unsafe { (*t.screens.active).cursor.x }, 1);

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "⛈︎".as_bytes());
    }

    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).content_codepoint() }, 0x26C8);
        assert!(unsafe { (*cell).has_grapheme() });
        assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Narrow);
        let cps = unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_grapheme(cell) }
            .unwrap();
        assert_eq!(cps.len(), 1);
    }

    t.deinit(Global);
}

// test "Terminal: VS15 to make narrow character with pending wrap"
#[test]
fn terminal_vs15_to_make_narrow_character_with_pending_wrap() {
    let mut t = Terminal::init(Global, opts(4, 5)).unwrap();

    // Enable grapheme clustering
    t.modes.set(modespkg::Mode::GraphemeCluster, true);

    assert!(t.modes.get(modespkg::Mode::Wraparound));

    t.print(0x1F34B).unwrap(); // Lemon, width=2
    t.print(0x2614).unwrap(); // Umbrella with rain drops, width=2
    assert!(t.is_dirty(screen_pt(0, 0)));
    t.clear_dirty();

    // We only move to the end of the line because we're in a pending wrap
    // state.
    assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
    assert_eq!(unsafe { (*t.screens.active).cursor.x }, 3);
    assert!(unsafe { (*t.screens.active).cursor.pending_wrap });

    t.print(0xFE0E).unwrap(); // VS15 to make narrow
    assert!(t.is_dirty(screen_pt(0, 0)));
    t.clear_dirty();

    // VS15 should clear the pending wrap state
    assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
    assert_eq!(unsafe { (*t.screens.active).cursor.x }, 3);
    assert!(!unsafe { (*t.screens.active).cursor.pending_wrap });

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "🍋☔︎".as_bytes());
    }

    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(2, 0)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).content_codepoint() }, 0x2614);
        assert!(unsafe { (*cell).has_grapheme() });
        assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Narrow);
        let cps = unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_grapheme(cell) }
            .unwrap();
        assert_eq!(cps.len(), 1);
    }

    // VS15 should not affect the previous grapheme
    {
        let lemon_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap().cell;
        assert_eq!(unsafe { (*lemon_cell).content_codepoint() }, 0x1F34B);
        assert_eq!(unsafe { (*lemon_cell).wide() }, pagepkg::Wide::Wide);
        let spacer_cell = t.active_mut().pages.get_cell(screen_pt(1, 0)).unwrap().cell;
        assert_eq!(unsafe { (*spacer_cell).content_codepoint() }, 0);
        assert_eq!(unsafe { (*spacer_cell).wide() }, pagepkg::Wide::SpacerTail);
    }

    t.deinit(Global);
}

// test "Terminal: VS15 narrows wide cell under cursor with wraparound disabled"
#[test]
fn terminal_vs15_narrows_wide_cell_under_cursor_with_wraparound_disabled() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.modes.set(modespkg::Mode::GraphemeCluster, true);
    t.modes.set(modespkg::Mode::Wraparound, false);

    // First create a wide cell spanning columns 4 and 5.
    t.set_cursor_pos(1, 4);
    t.print(0x2614).unwrap();

    // Make column 4 the right margin and put the cursor on the wide base.
    // With wraparound disabled, grapheme lookup selects the cell under the
    // cursor when it has content.
    t.modes.set(modespkg::Mode::EnableLeftAndRightMargin, true);
    t.set_left_and_right_margin(1, 4);
    t.set_cursor_pos(1, 4);
    t.print(0xFE0E).unwrap();

    assert_eq!(unsafe { (*t.screens.active).cursor.x }, 3);
    assert!(!unsafe { (*t.screens.active).cursor.pending_wrap });
    let base = t.active_mut().pages.get_cell(screen_pt(3, 0)).unwrap().cell;
    assert_eq!(unsafe { (*base).wide() }, pagepkg::Wide::Narrow);
    assert!(unsafe { (*base).has_grapheme() });
    let tail = t.active_mut().pages.get_cell(screen_pt(4, 0)).unwrap().cell;
    assert_eq!(unsafe { (*tail).wide() }, pagepkg::Wide::Narrow);

    t.deinit(Global);
}

// test "Terminal: VS15 narrows wide cell under restored pending cursor"
#[test]
fn terminal_vs15_narrows_wide_cell_under_restored_pending_cursor() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.modes.set(modespkg::Mode::GraphemeCluster, true);
    t.modes.set(modespkg::Mode::EnableLeftAndRightMargin, true);
    t.set_left_and_right_margin(1, 4);

    // Save a pending-wrap cursor at column 4.
    t.set_cursor_pos(1, 4);
    t.print('X' as u32).unwrap();
    assert!(unsafe { (*t.screens.active).cursor.pending_wrap });
    t.save_cursor();

    // Widen the margin and replace that cell with a wide character.
    t.set_left_and_right_margin(1, 5);
    t.set_cursor_pos(1, 4);
    t.print(0x2614).unwrap();

    // Restoring also restores pending_wrap, so grapheme lookup selects the
    // wide base under the cursor rather than its spacer tail.
    t.restore_cursor();
    assert!(unsafe { (*t.screens.active).cursor.pending_wrap });
    t.print(0xFE0E).unwrap();

    assert_eq!(unsafe { (*t.screens.active).cursor.x }, 4);
    assert!(!unsafe { (*t.screens.active).cursor.pending_wrap });
    let base = t.active_mut().pages.get_cell(screen_pt(3, 0)).unwrap().cell;
    assert_eq!(unsafe { (*base).wide() }, pagepkg::Wide::Narrow);
    assert!(unsafe { (*base).has_grapheme() });
    let tail = t.active_mut().pages.get_cell(screen_pt(4, 0)).unwrap().cell;
    assert_eq!(unsafe { (*tail).wide() }, pagepkg::Wide::Narrow);

    t.deinit(Global);
}

// test "Terminal: VS16 to make wide character on next line"
#[test]
fn terminal_vs16_to_make_wide_character_on_next_line() {
    let mut t = Terminal::init(Global, opts(3, 5)).unwrap();

    // Enable grapheme clustering
    t.modes.set(modespkg::Mode::GraphemeCluster, true);

    t.cursor_right(2);
    t.print('#' as u32).unwrap();
    assert_eq!(unsafe { (*t.screens.active).cursor.x }, 2);
    assert!(unsafe { (*t.screens.active).cursor.pending_wrap });
    assert!(t.is_dirty(screen_pt(2, 0)));
    t.clear_dirty();

    t.print(0xFE0F).unwrap(); // VS16 to make wide

    assert!(t.is_dirty(screen_pt(2, 0)));
    t.clear_dirty();
    assert_eq!(unsafe { (*t.screens.active).cursor.y }, 1);
    assert_eq!(unsafe { (*t.screens.active).cursor.x }, 2);
    assert!(!unsafe { (*t.screens.active).cursor.pending_wrap });

    {
        // Previous cell turns into spacer_head
        let list_cell = t.active_mut().pages.get_cell(screen_pt(2, 0)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).content_codepoint() }, 0);
        assert!(!unsafe { (*cell).has_grapheme() });
        assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::SpacerHead);
    }
    {
        // '#' cell is wide
        let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 1)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).content_codepoint() }, '#' as u21);
        assert!(unsafe { (*cell).has_grapheme() });
        assert_eq!(
            unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_grapheme(cell) }
                .unwrap(),
            &[0xFE0F]
        );
        assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Wide);
    }
    {
        // spacer_tail
        let list_cell = t.active_mut().pages.get_cell(screen_pt(1, 1)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).content_codepoint() }, 0);
        assert!(!unsafe { (*cell).has_grapheme() });
        assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::SpacerTail);
    }

    t.deinit(Global);
}

// test "Terminal: VS16 to make wide character on next line with hyperlink"
#[test]
fn terminal_vs16_to_make_wide_character_on_next_line_with_hyperlink() {
    // Regression test for the crash fixed in print's grapheme `.wide` path:
    // writing a spacer_head at the screen edge before row.wrap was set.
    let mut t = Terminal::init(Global, opts(3, 5)).unwrap();

    // Enable grapheme clustering and activate a hyperlink so printCell
    // calls cursorSetHyperlink (which runs page integrity checks).
    t.modes.set(modespkg::Mode::GraphemeCluster, true);
    t.active_mut()
        .start_hyperlink("http://example.com".as_bytes(), None)
        .unwrap();

    t.cursor_right(2);
    t.print('#' as u32).unwrap();
    assert_eq!(unsafe { (*t.screens.active).cursor.x }, 2);
    assert!(unsafe { (*t.screens.active).cursor.pending_wrap });

    // Without the fix, this panicked with UnwrappedSpacerHead.
    t.print(0xFE0F).unwrap(); // VS16 to make wide

    assert_eq!(unsafe { (*t.screens.active).cursor.y }, 1);
    assert_eq!(unsafe { (*t.screens.active).cursor.x }, 2);
    assert!(!unsafe { (*t.screens.active).cursor.pending_wrap });

    {
        // Previous cell turns into spacer_head and remains hyperlinked.
        let list_cell = t.active_mut().pages.get_cell(screen_pt(2, 0)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).content_codepoint() }, 0);
        assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::SpacerHead);
        assert!(unsafe { (*cell).hyperlink() });
        assert!(unsafe { (*list_cell.row).wrap() });
    }
    {
        // '#' cell is now wide and still hyperlinked.
        let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 1)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).content_codepoint() }, '#' as u21);
        assert!(unsafe { (*cell).has_grapheme() });
        assert_eq!(
            unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_grapheme(cell) }
                .unwrap(),
            &[0xFE0F]
        );
        assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Wide);
        assert!(unsafe { (*cell).hyperlink() });
    }
    {
        // spacer_tail inherits hyperlink as part of the same grapheme cell.
        let list_cell = t.active_mut().pages.get_cell(screen_pt(1, 1)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).content_codepoint() }, 0);
        assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::SpacerTail);
        assert!(unsafe { (*cell).hyperlink() });
    }

    t.deinit(Global);
}

// test "Terminal: VS16 widening when the spacer tail grows the page"
#[test]
fn terminal_vs16_widening_when_the_spacer_tail_grows_the_page() {
    // Regression test for a stale cell pointer in print's grapheme `.wide`
    // path: writing the spacer tail can grow the page to fit the hyperlink,
    // which replaces the page and invalidates the pointer to the wide cell.
    let mut t = Terminal::init(Global, opts(20, 10)).unwrap();

    t.modes.set(modespkg::Mode::GraphemeCluster, true);
    t.active_mut()
        .start_hyperlink(b"http://example.com", None)
        .unwrap();

    // Fill the page hyperlink map until a single slot is left. The '#' below
    // takes that slot so the spacer tail is what forces the page to grow.
    loop {
        let page: *mut Page =
            unsafe { crate::pagelist::Node::page((*(*t.screens.active).cursor.page_pin).node) };
        let map = unsafe { (*page).hyperlink_map.map((*page).memory_ptr()) };
        if map.max_load() - map.count() == 1 {
            break;
        }
        t.print('x' as u32).unwrap();
    }

    let x = unsafe { (*t.screens.active).cursor.x };
    let y = unsafe { (*t.screens.active).cursor.y };
    t.print('#' as u32).unwrap();

    // Without the fix this crashed appending to a freed page.
    t.print(0xFE0F).unwrap();

    {
        // '#' is wide and carries the VS16 grapheme.
        let list_cell = t
            .active_mut()
            .pages
            .get_cell(active_pt(x, y as u32))
            .unwrap();
        let cell = list_cell.cell;
        assert_eq!('#' as u21, unsafe { (*cell).content_codepoint() });
        assert_eq!(pagepkg::Wide::Wide, unsafe { (*cell).wide() });
        assert!(unsafe { (*cell).has_grapheme() });
        assert_eq!(
            unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_grapheme(cell) }
                .unwrap(),
            &[0xFE0F]
        );
    }
    {
        let list_cell = t
            .active_mut()
            .pages
            .get_cell(active_pt(x + 1, y as u32))
            .unwrap();
        assert_eq!(pagepkg::Wide::SpacerTail, unsafe {
            (*list_cell.cell).wide()
        });
    }

    t.deinit(Global);
}

// test "Terminal: grapheme transfer when widening wraps to the next line"
#[test]
fn terminal_grapheme_transfer_when_widening_wraps_to_the_next_line() {
    // Covers print's grapheme `.wide` path where the previous cell already
    // holds grapheme data and has to be moved to the wrapped row.
    let mut t = Terminal::init(Global, opts(3, 5)).unwrap();

    t.modes.set(modespkg::Mode::GraphemeCluster, true);
    t.cursor_right(2);

    // A narrow emoji, then ZWJ, then a second emoji. The ZWJ attaches
    // without changing the width, so the cell has grapheme data by the time
    // the second emoji widens it.
    t.print(0x263A).unwrap();
    t.print(0x200D).unwrap();
    t.print(0x2764).unwrap();

    {
        // The old cell becomes a spacer head on the wrapped row.
        let list_cell = t.active_mut().pages.get_cell(screen_pt(2, 0)).unwrap();
        assert_eq!(pagepkg::Wide::SpacerHead, unsafe {
            (*list_cell.cell).wide()
        });
        assert!(unsafe { (*list_cell.row).wrap() });
    }
    {
        // The grapheme moved with the base codepoint.
        let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 1)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(0x263A as u21, unsafe { (*cell).content_codepoint() });
        assert_eq!(pagepkg::Wide::Wide, unsafe { (*cell).wide() });
        assert_eq!(
            unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_grapheme(cell) }
                .unwrap(),
            &[0x200D, 0x2764]
        );
    }
    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(1, 1)).unwrap();
        assert_eq!(pagepkg::Wide::SpacerTail, unsafe {
            (*list_cell.cell).wide()
        });
    }

    t.deinit(Global);
}

// test "Terminal: VS16 to make wide character with pending wrap"
#[test]
fn terminal_vs16_to_make_wide_character_with_pending_wrap() {
    let mut t = Terminal::init(Global, opts(3, 5)).unwrap();

    // Enable grapheme clustering
    t.modes.set(modespkg::Mode::GraphemeCluster, true);

    t.cursor_right(1);
    t.print('#' as u32).unwrap();
    assert_eq!(unsafe { (*t.screens.active).cursor.x }, 2);
    assert!(!unsafe { (*t.screens.active).cursor.pending_wrap });

    t.print(0xFE0F).unwrap(); // VS16 to make wide

    assert_eq!(unsafe { (*t.screens.active).cursor.x }, 2);
    assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
    assert!(unsafe { (*t.screens.active).cursor.pending_wrap });

    {
        // '#' cell is wide
        let list_cell = t.active_mut().pages.get_cell(screen_pt(1, 0)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).content_codepoint() }, '#' as u21);
        assert!(unsafe { (*cell).has_grapheme() });
        assert_eq!(
            unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_grapheme(cell) }
                .unwrap(),
            &[0xFE0F]
        );
        assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Wide);
    }
    {
        // spacer_tail
        let list_cell = t.active_mut().pages.get_cell(screen_pt(2, 0)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).content_codepoint() }, 0);
        assert!(!unsafe { (*cell).has_grapheme() });
        assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::SpacerTail);
    }

    t.deinit(Global);
}

// test "Terminal: VS16 to make wide character with mode 2027"
#[test]
fn terminal_vs16_to_make_wide_character_with_mode_2027() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    // Enable grapheme clustering
    t.modes.set(modespkg::Mode::GraphemeCluster, true);

    t.print(0x2764).unwrap(); // Heart
    assert!(t.is_dirty(screen_pt(0, 0)));
    t.clear_dirty();
    t.print(0xFE0F).unwrap(); // VS16 to make wide
    assert!(t.is_dirty(screen_pt(0, 0)));
    t.clear_dirty();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "❤️".as_bytes());
    }

    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).content_codepoint() }, 0x2764);
        assert!(unsafe { (*cell).has_grapheme() });
        assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Wide);
        let cps = unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_grapheme(cell) }
            .unwrap();
        assert_eq!(cps.len(), 1);
    }

    t.deinit(Global);
}

// test "Terminal: VS16 repeated with mode 2027"
#[test]
fn terminal_vs16_repeated_with_mode_2027() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    // Enable grapheme clustering
    t.modes.set(modespkg::Mode::GraphemeCluster, true);

    t.print(0x2764).unwrap(); // Heart
    t.print(0xFE0F).unwrap(); // VS16 to make wide
    t.print(0x2764).unwrap(); // Heart
    t.print(0xFE0F).unwrap(); // VS16 to make wide

    assert!(t.is_dirty(screen_pt(0, 0)));

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "❤️❤️".as_bytes());
    }

    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).content_codepoint() }, 0x2764);
        assert!(unsafe { (*cell).has_grapheme() });
        assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Wide);
        let cps = unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_grapheme(cell) }
            .unwrap();
        assert_eq!(cps.len(), 1);
    }
    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(2, 0)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).content_codepoint() }, 0x2764);
        assert!(unsafe { (*cell).has_grapheme() });
        assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Wide);
        let cps = unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_grapheme(cell) }
            .unwrap();
        assert_eq!(cps.len(), 1);
    }

    t.deinit(Global);
}

// test "Terminal: print Devanagari grapheme should be wide on next line"
#[test]
fn terminal_print_devanagari_grapheme_should_be_wide_on_next_line() {
    let mut t = Terminal::init(Global, opts(3, 5)).unwrap();

    // Enable grapheme clustering
    t.modes.set(modespkg::Mode::GraphemeCluster, true);

    t.cursor_right(2);

    // क्‍ष
    t.print(0x0915).unwrap();
    t.print(0x094D).unwrap();
    t.print(0x200D).unwrap();
    assert_eq!(unsafe { (*t.screens.active).cursor.x }, 2);
    assert!(unsafe { (*t.screens.active).cursor.pending_wrap });

    // This one increases the width to wide
    t.print(0x0937).unwrap();

    // We should have 2 cells taken up. It is one character but "wide".
    assert_eq!(unsafe { (*t.screens.active).cursor.y }, 1);
    assert_eq!(unsafe { (*t.screens.active).cursor.x }, 2);
    assert!(!unsafe { (*t.screens.active).cursor.pending_wrap });

    {
        // Previous cell turns into spacer_head
        let list_cell = t.active_mut().pages.get_cell(screen_pt(2, 0)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).content_codepoint() }, 0);
        assert!(!unsafe { (*cell).has_grapheme() });
        assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::SpacerHead);
    }
    {
        // Devanagari grapheme is wide
        let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 1)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).content_codepoint() }, 0x0915);
        assert!(unsafe { (*cell).has_grapheme() });
        assert_eq!(
            unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_grapheme(cell) }
                .unwrap(),
            &[0x094D, 0x200D, 0x0937]
        );
        assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Wide);
    }
    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(1, 1)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::SpacerTail);
    }

    t.deinit(Global);
}

// test "Terminal: print Devanagari grapheme should be wide on next page"
#[test]
fn terminal_print_devanagari_grapheme_should_be_wide_on_next_page() {
    let rows = pagepkg::std_capacity().rows;
    let cols = pagepkg::std_capacity().cols;
    let mut t = Terminal::init(Global, opts(cols, rows)).unwrap();

    // Enable grapheme clustering
    t.modes.set(modespkg::Mode::GraphemeCluster, true);

    t.cursor_down((rows - 1) as usize);

    let page_cap_rows =
        unsafe { crate::pagelist::Node::capacity(t.active_mut().pages.pages.first).rows };
    for _ in rows..page_cap_rows {
        t.index().unwrap();
    }

    t.cursor_right((cols - 1) as usize);

    assert_eq!(unsafe { (*t.screens.active).cursor.x }, cols - 1);
    assert_eq!(unsafe { (*t.screens.active).cursor.y }, rows - 1);

    // क्‍ष
    t.print(0x0915).unwrap();
    t.print(0x094D).unwrap();
    t.print(0x200D).unwrap();
    assert_eq!(unsafe { (*t.screens.active).cursor.x }, cols - 1);
    assert!(unsafe { (*t.screens.active).cursor.pending_wrap });

    // This one increases the width to wide
    t.print(0x0937).unwrap();

    // We should have 2 cells taken up. It is one character but "wide".
    assert_eq!(unsafe { (*t.screens.active).cursor.y }, rows - 1);
    assert_eq!(unsafe { (*t.screens.active).cursor.x }, 2);
    assert!(!unsafe { (*t.screens.active).cursor.pending_wrap });

    {
        // Previous cell turns into spacer_head
        let list_cell = t
            .active_mut()
            .pages
            .get_cell(active_pt(cols - 1, (rows - 2) as u32))
            .unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).content_codepoint() }, 0);
        assert!(!unsafe { (*cell).has_grapheme() });
        assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::SpacerHead);
    }
    {
        // Devanagari grapheme is wide
        let list_cell = t
            .active_mut()
            .pages
            .get_cell(active_pt(0, (rows - 1) as u32))
            .unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).content_codepoint() }, 0x0915);
        assert!(unsafe { (*cell).has_grapheme() });
        assert_eq!(
            unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_grapheme(cell) }
                .unwrap(),
            &[0x094D, 0x200D, 0x0937]
        );
        assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Wide);
    }
    {
        let list_cell = t
            .active_mut()
            .pages
            .get_cell(active_pt(1, (rows - 1) as u32))
            .unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::SpacerTail);
    }

    t.deinit(Global);
}

// test "Terminal: overwrite grapheme should clear grapheme data"
#[test]
fn terminal_overwrite_grapheme_should_clear_grapheme_data() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    // Enable grapheme clustering
    t.modes.set(modespkg::Mode::GraphemeCluster, true);

    t.print(0x26C8).unwrap(); // Thunder cloud and rain
    t.print(0xFE0E).unwrap(); // VS15 to make narrow
    assert!(t.is_dirty(screen_pt(0, 0)));
    t.clear_dirty();

    t.set_cursor_pos(1, 1);
    t.print('A' as u32).unwrap();
    assert!(t.is_dirty(screen_pt(0, 0)));

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "A".as_bytes());
    }

    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).content_codepoint() }, 'A' as u21);
        assert!(!unsafe { (*cell).has_grapheme() });
        assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Narrow);
    }

    t.deinit(Global);
}

// test "Terminal: overwrite multicodepoint grapheme clears grapheme data"
#[test]
fn terminal_overwrite_multicodepoint_grapheme_clears_grapheme_data() {
    let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

    // Enable grapheme clustering
    t.modes.set(modespkg::Mode::GraphemeCluster, true);

    // https://github.com/mitchellh/ghostty/issues/289
    // This is: 👨‍👩‍👧 (which may or may not render correctly)
    t.print(0x1F468).unwrap();
    t.print(0x200D).unwrap();
    t.print(0x1F469).unwrap();
    t.print(0x200D).unwrap();
    t.print(0x1F467).unwrap();

    // We should have 2 cells taken up. It is one character but "wide".
    assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
    assert_eq!(unsafe { (*t.screens.active).cursor.x }, 2);

    // We should have one cell with graphemes
    let page = unsafe { crate::pagelist::Node::page((*(*t.screens.active).cursor.page_pin).node) };
    assert_eq!(unsafe { (*page).grapheme_count() }, 1);

    // Move back and overwrite wide
    t.set_cursor_pos(1, 1);
    t.clear_dirty();
    t.print('X' as u32).unwrap();
    assert!(t.is_dirty(screen_pt(0, 0)));

    assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
    assert_eq!(unsafe { (*t.screens.active).cursor.x }, 1);
    assert_eq!(unsafe { (*page).grapheme_count() }, 0);

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "X".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: overwrite multicodepoint grapheme tail clears grapheme data"
#[test]
fn terminal_overwrite_multicodepoint_grapheme_tail_clears_grapheme_data() {
    let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

    // Enable grapheme clustering
    t.modes.set(modespkg::Mode::GraphemeCluster, true);

    // https://github.com/mitchellh/ghostty/issues/289
    // This is: 👨‍👩‍👧 (which may or may not render correctly)
    t.print(0x1F468).unwrap();
    t.print(0x200D).unwrap();
    t.print(0x1F469).unwrap();
    t.print(0x200D).unwrap();
    t.print(0x1F467).unwrap();

    // We should have 2 cells taken up. It is one character but "wide".
    assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
    assert_eq!(unsafe { (*t.screens.active).cursor.x }, 2);

    // We should have one cell with graphemes
    let page = unsafe { crate::pagelist::Node::page((*(*t.screens.active).cursor.page_pin).node) };
    assert_eq!(unsafe { (*page).grapheme_count() }, 1);

    // Move back and overwrite wide
    t.set_cursor_pos(1, 2);
    t.print('X' as u32).unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), " X".as_bytes());
    }

    assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
    assert_eq!(unsafe { (*t.screens.active).cursor.x }, 2);
    assert_eq!(unsafe { (*page).grapheme_count() }, 0);

    t.deinit(Global);
}

// test "Terminal: print writes to bottom if scrolled"
#[test]
fn terminal_print_writes_to_bottom_if_scrolled() {
    let mut t = Terminal::init(Global, opts(5, 2)).unwrap();

    // Basic grid writing
    for c in "hello".chars() {
        t.print(c as u32).unwrap();
    }
    t.set_cursor_pos(0, 0);

    // Make newlines so we create scrollback
    // 3 pushes hello off the screen
    t.index().unwrap();
    t.index().unwrap();
    t.index().unwrap();
    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "".as_bytes());
    }

    // Scroll to the top
    t.active_mut().scroll(crate::screen::Scroll::Top);
    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "hello".as_bytes());
    }

    // Type
    t.print('A' as u32).unwrap();
    t.active_mut().scroll(crate::screen::Scroll::Active);
    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "\nA".as_bytes());
    }

    assert!(
        t.is_dirty(active_pt(unsafe { (*t.screens.active).cursor.x }, unsafe {
            (*t.screens.active).cursor.y
        }
            as u32))
    );

    t.deinit(Global);
}

// test "Terminal: scroll_viewport row" — engine-level mirror of the C API
// scroll_viewport-row KAT (oracle 3a2e28329).
#[test]
fn terminal_scroll_viewport_row() {
    let mut t = Terminal::init(Global, opts(5, 2)).unwrap();

    // Write 4 rows so rows "1" and "2" are pushed into scrollback:
    // total rows is 4, viewport length is 2. (The oracle drives this via
    // vt_write "1\r\n2\r\n3\r\n4"; here we invoke the engine primitives
    // directly since print_string does not process control bytes.)
    for (i, ch) in ['1', '2', '3', '4'].iter().enumerate() {
        t.print(*ch as u32).unwrap();
        if i != 3 {
            t.carriage_return();
            t.linefeed().unwrap();
        }
    }

    // Row 0 is the top of the scrollback.
    t.scroll_viewport(ScrollViewport::Row(0));
    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "1\n2".as_bytes());
    }

    // An absolute row within the scrollback becomes the first visible row.
    t.scroll_viewport(ScrollViewport::Row(1));
    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "2\n3".as_bytes());
    }

    // A row past the end clamps to the active area.
    t.scroll_viewport(ScrollViewport::Row(9999));
    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "3\n4".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: print charset"
#[test]
fn terminal_print_charset() {
    let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

    // G1 should have no effect
    t.configure_charset(charsets::Slots::G1, charsets::Charset::DecSpecial);
    t.configure_charset(charsets::Slots::G2, charsets::Charset::DecSpecial);
    t.configure_charset(charsets::Slots::G3, charsets::Charset::DecSpecial);

    // No dirty to configure charset
    assert!(!t.is_dirty(screen_pt(0, 0)));

    // Basic grid writing
    t.print('`' as u32).unwrap();
    t.configure_charset(charsets::Slots::G0, charsets::Charset::Utf8);
    t.print('`' as u32).unwrap();
    t.configure_charset(charsets::Slots::G0, charsets::Charset::Ascii);
    t.print('`' as u32).unwrap();
    t.configure_charset(charsets::Slots::G0, charsets::Charset::DecSpecial);
    t.print('`' as u32).unwrap();
    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "```◆".as_bytes());
    }

    assert!(t.is_dirty(screen_pt(0, 0)));

    t.deinit(Global);
}

// test "Terminal: print charset outside of ASCII"
#[test]
fn terminal_print_charset_outside_of_ascii() {
    let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

    // G1 should have no effect
    t.configure_charset(charsets::Slots::G1, charsets::Charset::DecSpecial);
    t.configure_charset(charsets::Slots::G2, charsets::Charset::DecSpecial);
    t.configure_charset(charsets::Slots::G3, charsets::Charset::DecSpecial);

    // No dirty to configure charset
    assert!(!t.is_dirty(screen_pt(0, 0)));

    // Basic grid writing
    t.configure_charset(charsets::Slots::G0, charsets::Charset::DecSpecial);
    t.print('`' as u32).unwrap();
    t.print(0x1F600).unwrap();
    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "◆😀".as_bytes());
    }

    assert!(t.is_dirty(screen_pt(0, 0)));

    t.deinit(Global);
}

// test "Terminal: print wide codepoint in a charset prints it unmapped"
#[test]
fn terminal_print_wide_codepoint_in_a_charset_prints_it_unmapped() {
    let mut t = Terminal::init(Global, opts(10, 2)).unwrap();

    // The charset has no such codepoint, so it prints as it is, two
    // columns wide, and the characters after it are mapped again.
    t.configure_charset(charsets::Slots::G0, charsets::Charset::DecSpecial);
    t.print('`' as u32).unwrap();
    t.print(0x1F600).unwrap();
    t.print('a' as u32).unwrap();
    assert_eq!(unsafe { (*t.screens.active).cursor.x }, 4);
    let cell = t.active_mut().pages.get_cell(screen_pt(1, 0)).unwrap().cell;
    assert_eq!(unsafe { (*cell).content_codepoint() }, 0x1F600);
    assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Wide);
    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "◆😀▒".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: print single shift is used up by a combining character"
#[test]
fn terminal_print_single_shift_is_used_up_by_a_combining_character() {
    // As in xterm, the single shift applies to the combining mark, the next
    // character printed, whether or not it clusters with the one before.
    for cluster in [false, true] {
        let mut t = Terminal::init(Global, opts(10, 2)).unwrap();
        t.modes.set(modespkg::Mode::GraphemeCluster, cluster);

        t.configure_charset(charsets::Slots::G2, charsets::Charset::British);
        t.print('#' as u32).unwrap();
        t.invoke_charset(charsets::ActiveSlot::GL, charsets::Slots::G2, true);
        t.print(0x0301).unwrap();
        assert_eq!(unsafe { (*t.screens.active).charset.single_shift }, None);
        t.print('#' as u32).unwrap();

        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 2);
        let cell = t.active_mut().pages.get_cell(screen_pt(1, 0)).unwrap().cell;
        assert_eq!(unsafe { (*cell).content_codepoint() }, '#' as u21);

        t.deinit(Global);
    }
}

// test "Terminal: VS16 moving a character to the next line keeps it unmapped"
#[test]
fn terminal_vs16_moving_a_character_to_the_next_line_keeps_it_unmapped() {
    let mut t = Terminal::init(Global, opts(3, 5)).unwrap();

    // The '#' is printed with ASCII in the last column. VS16 widens it after
    // the British set (which maps '#' to '£') is selected, which moves it to
    // the next line: it is the same character, not one to map again.
    t.modes.set(modespkg::Mode::GraphemeCluster, true);
    t.cursor_right(2);
    t.print('#' as u32).unwrap();
    t.configure_charset(charsets::Slots::G0, charsets::Charset::British);
    t.print(0xFE0F).unwrap();

    let cell = t.active_mut().pages.get_cell(screen_pt(0, 1)).unwrap().cell;
    assert_eq!(unsafe { (*cell).content_codepoint() }, '#' as u21);
    assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Wide);

    t.deinit(Global);
}

// Rustty-only (Review Focus 5): a single shift applies to exactly one
// printed codepoint, whatever its shape, and the next one prints from GL.
// test "Terminal: single shift applies to exactly one codepoint"
#[test]
fn terminal_single_shift_applies_to_exactly_one_codepoint() {
    // (shifted sequence after ESC N, expected first-cell codepoint)
    let cases: [(&[u32], u32); 3] = [
        // (a) wide CJK: above 0xFF, so unmapped; it still uses the shift up
        (&[0x6A4B, 'q' as u32], 0x6A4B),
        // (b) a base followed by VS16: the base is shifted, VS16 is not
        (&['q' as u32, 0xFE0F, 'q' as u32], 0x2500),
        // (c) a base followed by a combining mark
        (&['q' as u32, 0x0301, 'q' as u32], 0x2500),
    ];
    for cluster in [false, true] {
        for (seq, first) in cases {
            let mut t = Terminal::init(Global, opts(10, 2)).unwrap();
            t.modes.set(modespkg::Mode::GraphemeCluster, cluster);
            t.configure_charset(charsets::Slots::G2, charsets::Charset::DecSpecial);
            t.invoke_charset(charsets::ActiveSlot::GL, charsets::Slots::G2, true);
            for &c in seq {
                t.print(c).unwrap();
            }
            assert_eq!(unsafe { (*t.screens.active).charset.single_shift }, None);

            let head = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap().cell;
            assert_eq!(unsafe { (*head).content_codepoint() }, first as u21);

            // The final 'q' is in the last cell written, printed from GL.
            let x = unsafe { (*t.screens.active).cursor.x };
            let last = t
                .active_mut()
                .pages
                .get_cell(screen_pt(x - 1, 0))
                .unwrap()
                .cell;
            assert_eq!(unsafe { (*last).content_codepoint() }, 'q' as u21);

            t.deinit(Global);
        }
    }
}

// test "Terminal: print invoke charset"
#[test]
fn terminal_print_invoke_charset() {
    let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

    t.configure_charset(charsets::Slots::G1, charsets::Charset::DecSpecial);

    t.print('`' as u32).unwrap();

    // Invokecharset but should not mark dirty on its own
    t.clear_dirty();
    t.invoke_charset(charsets::ActiveSlot::GL, charsets::Slots::G1, false);
    assert!(!t.is_dirty(screen_pt(0, 0)));
    t.print('`' as u32).unwrap();
    assert!(t.is_dirty(screen_pt(0, 0)));
    t.print('`' as u32).unwrap();
    t.invoke_charset(charsets::ActiveSlot::GL, charsets::Slots::G0, false);
    t.print('`' as u32).unwrap();
    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "`◆◆`".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: print kitty unicode placeholder"
#[test]
#[cfg_attr(
    not(all(feature = "kitty-graphics", not(target_os = "none"))),
    ignore = "SkipZigTest: needs options::KITTY_GRAPHICS (R14)"
)]
fn terminal_print_kitty_unicode_placeholder() {
    let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

    t.print(kitty::graphics_unicode::PLACEHOLDER).unwrap();
    assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
    assert_eq!(unsafe { (*t.screens.active).cursor.x }, 1);

    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(
            unsafe { (*cell).content_codepoint() },
            kitty::graphics_unicode::PLACEHOLDER
        );
        assert!(unsafe { (*list_cell.row).kitty_virtual_placeholder() });
    }

    assert!(t.is_dirty(screen_pt(0, 0)));

    t.deinit(Global);
}

/// PORT NOTE: NOT a reference test. The reference's `Screen.clearCells`
/// kitty block (`Screen.zig:1818-1828`) was carried only as a comment, and
/// nothing in either tree exercised it. It clears
/// `row.kitty_virtual_placeholder` on a FULL-row clear only when no cell in
/// the row still carries the placeholder codepoint (the Zig `for ... else`
/// runs its `else` when the loop did not `break`), so both arms are pinned
/// here.
#[test]
#[cfg_attr(
    not(all(feature = "kitty-graphics", not(target_os = "none"))),
    ignore = "SkipZigTest: needs options::KITTY_GRAPHICS (R14)"
)]
fn terminal_erase_line_clears_the_kitty_virtual_placeholder_flag() {
    let mut t = Terminal::init(Global, opts(10, 10)).unwrap();
    let flag = |t: &mut Terminal| -> bool {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
        unsafe { (*list_cell.row).kitty_virtual_placeholder() }
    };

    // Printing the placeholder marks the row.
    t.print(kitty::graphics_unicode::PLACEHOLDER).unwrap();
    assert!(flag(&mut t));

    // Overwriting the only placeholder cell does NOT clear the flag (the
    // reference never clears it on print either), so the row is now marked
    // but holds no placeholder.
    t.set_cursor_pos(1, 1);
    t.print('A' as u32).unwrap();
    assert!(flag(&mut t));

    // A full-row clear with no placeholder left in the row clears the flag.
    t.set_cursor_pos(1, 1);
    t.erase_line(csi::EraseLine::Complete, false);
    assert!(!flag(&mut t));

    // The `break` arm: a row that still holds a placeholder keeps the flag,
    // because the loop breaks before the `else`.
    t.set_cursor_pos(1, 1);
    t.print(kitty::graphics_unicode::PLACEHOLDER).unwrap();
    assert!(flag(&mut t));
    t.set_cursor_pos(1, 1);
    t.erase_line(csi::EraseLine::Complete, false);
    assert!(flag(&mut t));

    t.deinit(Global);
}

// test "Terminal: soft wrap"
#[test]
fn terminal_soft_wrap() {
    let mut t = Terminal::init(Global, opts(3, 80)).unwrap();

    // Basic grid writing
    for c in "hello".chars() {
        t.print(c as u32).unwrap();
    }
    assert_eq!(unsafe { (*t.screens.active).cursor.y }, 1);
    assert_eq!(unsafe { (*t.screens.active).cursor.x }, 2);
    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "hel\nlo".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: soft wrap with semantic prompt"
#[test]
fn terminal_soft_wrap_with_semantic_prompt() {
    let mut t = Terminal::init(Global, opts(3, 80)).unwrap();

    // Mark our prompt.
    t.semantic_prompt(osc::SemanticPrompt::init(
        osc::semantic_prompt::Action::PromptStart,
    ))
    .unwrap();
    // Should not make anything dirty on its own.
    assert!(!t.is_dirty(screen_pt(0, 0)));

    // Write and wrap
    for c in "hello".chars() {
        t.print(c as u32).unwrap();
    }
    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
        assert_eq!(
            unsafe { (*list_cell.row).semantic_prompt() },
            pagepkg::SemanticPrompt::Prompt
        );
    }
    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 1)).unwrap();
        assert_eq!(
            unsafe { (*list_cell.row).semantic_prompt() },
            pagepkg::SemanticPrompt::PromptContinuation
        );
    }

    t.deinit(Global);
}

// (no reference test) PORT-ADDITIVE proving test (W29-B02): invalid UTF-8
// must surface as `PrintStringError::InvalidUtf8` — the Zig
// `try std.unicode.Utf8View.init` error — not `OutOfMemory`, and must
// print nothing (validation happens before any codepoint is printed).
#[test]
fn terminal_print_string_invalid_utf8() {
    let mut t = Terminal::init(Global, opts(20, 5)).unwrap();

    assert_eq!(
        t.print_string(b"foo\xFFbar"),
        Err(PrintStringError::InvalidUtf8)
    );

    // Nothing was printed: validation fails before iteration begins.
    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "".as_bytes());
    }

    // Valid input still prints (regression arm).
    t.print_string(b"hello").unwrap();
    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "hello".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: disabled wraparound with wide char and one space"
#[test]
fn terminal_disabled_wraparound_with_wide_char_and_one_space() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.modes.set(modespkg::Mode::Wraparound, false);

    // This puts our cursor at the end and there is NO SPACE for a
    // wide character.
    t.print_string("AAAA".as_bytes()).unwrap();
    t.clear_dirty();
    t.print(0x1F6A8).unwrap(); // Police car light
    assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
    assert_eq!(unsafe { (*t.screens.active).cursor.x }, 4);

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "AAAA".as_bytes());
    }

    // Make sure we printed nothing
    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(4, 0)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).content_codepoint() }, 0);
        assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Narrow);
    }

    // Should not be dirty since we didn't modify anything
    assert!(!t.is_dirty(screen_pt(0, 0)));

    t.deinit(Global);
}

// test "Terminal: disabled wraparound with wide char and no space"
#[test]
fn terminal_disabled_wraparound_with_wide_char_and_no_space() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.modes.set(modespkg::Mode::Wraparound, false);

    // This puts our cursor at the end and there is NO SPACE for a
    // wide character.
    t.print_string("AAAAA".as_bytes()).unwrap();
    t.clear_dirty();
    t.print(0x1F6A8).unwrap(); // Police car light
    assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
    assert_eq!(unsafe { (*t.screens.active).cursor.x }, 4);

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "AAAAA".as_bytes());
    }

    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(4, 0)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).content_codepoint() }, 'A' as u21);
        assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Narrow);
    }

    // Should not be dirty since we didn't modify anything
    assert!(!t.is_dirty(screen_pt(0, 0)));

    t.deinit(Global);
}

// test "Terminal: disabled wraparound with wide grapheme and half space"
#[test]
fn terminal_disabled_wraparound_with_wide_grapheme_and_half_space() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.modes.set(modespkg::Mode::GraphemeCluster, true);
    t.modes.set(modespkg::Mode::Wraparound, false);

    // This puts our cursor at the end and there is NO SPACE for a
    // wide character.
    t.print_string("AAAA".as_bytes()).unwrap();
    t.print(0x2764).unwrap(); // Heart
    t.clear_dirty();
    t.print(0xFE0F).unwrap(); // VS16 to make wide
    assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
    assert_eq!(unsafe { (*t.screens.active).cursor.x }, 4);

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "AAAA❤".as_bytes());
    }

    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(4, 0)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).content_codepoint() }, '❤' as u21);
        assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Narrow);
    }

    // Should not be dirty since we didn't modify anything
    assert!(!t.is_dirty(screen_pt(0, 0)));

    t.deinit(Global);
}

// test "Terminal: print right margin wrap"
#[test]
fn terminal_print_right_margin_wrap() {
    let mut t = Terminal::init(Global, opts(10, 5)).unwrap();

    t.print_string("123456789".as_bytes()).unwrap();
    t.modes.set(modespkg::Mode::EnableLeftAndRightMargin, true);
    t.set_left_and_right_margin(3, 5);
    t.set_cursor_pos(1, 5);
    t.print_string("XY".as_bytes()).unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "1234X6789\n  Y".as_bytes());
    }

    {
        let list_cell = t.active_mut().pages.get_cell(active_pt(0, 0)).unwrap();
        let row = list_cell.row;
        assert!(!unsafe { (*row).wrap() });
    }

    t.deinit(Global);
}

// test "Terminal: print right margin wrap dirty tracking"
#[test]
fn terminal_print_right_margin_wrap_dirty_tracking() {
    let mut t = Terminal::init(Global, opts(10, 5)).unwrap();

    t.print_string("123456789".as_bytes()).unwrap();
    t.modes.set(modespkg::Mode::EnableLeftAndRightMargin, true);
    t.set_left_and_right_margin(3, 5);
    t.set_cursor_pos(1, 5);

    // Writing our X on the first line should mark only that line dirty.
    t.clear_dirty();
    t.print('X' as u32).unwrap();
    assert!(t.is_dirty(screen_pt(4, 0)));
    assert!(!t.is_dirty(screen_pt(2, 1)));

    // Writing our Y should wrap. It marks both rows dirty because the
    // cursor moved.
    t.clear_dirty();
    t.print('Y' as u32).unwrap();
    assert!(t.is_dirty(screen_pt(4, 0)));
    assert!(t.is_dirty(screen_pt(2, 1)));

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "1234X6789\n  Y".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: print right margin outside"
#[test]
fn terminal_print_right_margin_outside() {
    let mut t = Terminal::init(Global, opts(10, 5)).unwrap();

    t.print_string("123456789".as_bytes()).unwrap();
    t.modes.set(modespkg::Mode::EnableLeftAndRightMargin, true);
    t.set_left_and_right_margin(3, 5);
    t.set_cursor_pos(1, 6);
    t.clear_dirty();
    t.print_string("XY".as_bytes()).unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "12345XY89".as_bytes());
    }

    assert!(t.is_dirty(screen_pt(5, 0)));

    t.deinit(Global);
}

// test "Terminal: print right margin outside wrap"
#[test]
fn terminal_print_right_margin_outside_wrap() {
    let mut t = Terminal::init(Global, opts(10, 5)).unwrap();

    t.print_string("123456789".as_bytes()).unwrap();
    t.modes.set(modespkg::Mode::EnableLeftAndRightMargin, true);
    t.set_left_and_right_margin(3, 5);
    t.set_cursor_pos(1, 10);
    t.print_string("XY".as_bytes()).unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "123456789X\n  Y".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: print wide char at right margin does not create spacer head"
#[test]
fn terminal_print_wide_char_at_right_margin_does_not_create_spacer_head() {
    let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

    t.modes.set(modespkg::Mode::EnableLeftAndRightMargin, true);
    t.set_left_and_right_margin(3, 5);
    t.set_cursor_pos(1, 5);
    t.print(0x1F600).unwrap(); // Smiley face
    assert_eq!(unsafe { (*t.screens.active).cursor.y }, 1);
    assert_eq!(unsafe { (*t.screens.active).cursor.x }, 4);

    // Both rows dirty because the cursor moved
    assert!(t.is_dirty(screen_pt(4, 0)));
    assert!(t.is_dirty(screen_pt(4, 1)));

    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(4, 0)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).content_codepoint() }, 0);
        assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Narrow);

        let row = list_cell.row;
        assert!(!unsafe { (*row).wrap() });
    }
    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(2, 1)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).content_codepoint() }, 0x1F600);
        assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Wide);
    }
    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(3, 1)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::SpacerTail);
    }

    t.deinit(Global);
}

// test "Terminal: print with hyperlink"
#[test]
fn terminal_print_with_hyperlink() {
    let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

    // Setup our hyperlink and print
    t.active_mut()
        .start_hyperlink("http://example.com".as_bytes(), None)
        .unwrap();
    t.print_string("123456".as_bytes()).unwrap();

    // Verify all our cells have a hyperlink
    for x in 0..6 {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(x, 0)).unwrap();
        let row = list_cell.row;
        assert!(unsafe { (*row).hyperlink() });
        let cell = list_cell.cell;
        assert!(unsafe { (*cell).hyperlink() });
        let id = unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_hyperlink(cell) }
            .unwrap();
        assert_eq!(id, 1 as crate::hyperlink::Id);
    }

    assert!(t.is_dirty(screen_pt(0, 0)));

    t.deinit(Global);
}

// test "Terminal: print over cell with same hyperlink"
#[test]
fn terminal_print_over_cell_with_same_hyperlink() {
    let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

    // Setup our hyperlink and print
    t.active_mut()
        .start_hyperlink("http://example.com".as_bytes(), None)
        .unwrap();
    t.print_string("123456".as_bytes()).unwrap();
    t.set_cursor_pos(1, 1);
    t.print_string("123456".as_bytes()).unwrap();

    // Verify all our cells have a hyperlink
    for x in 0..6 {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(x, 0)).unwrap();
        let row = list_cell.row;
        assert!(unsafe { (*row).hyperlink() });
        let cell = list_cell.cell;
        assert!(unsafe { (*cell).hyperlink() });
        let id = unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_hyperlink(cell) }
            .unwrap();
        assert_eq!(id, 1 as crate::hyperlink::Id);
    }

    assert!(t.is_dirty(screen_pt(0, 0)));

    t.deinit(Global);
}

// test "Terminal: print and end hyperlink"
#[test]
fn terminal_print_and_end_hyperlink() {
    let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

    // Setup our hyperlink and print
    t.active_mut()
        .start_hyperlink("http://example.com".as_bytes(), None)
        .unwrap();
    t.print_string("123".as_bytes()).unwrap();
    t.active_mut().end_hyperlink();
    t.print_string("456".as_bytes()).unwrap();

    // Verify all our cells have a hyperlink
    for x in 0..3 {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(x, 0)).unwrap();
        let row = list_cell.row;
        assert!(unsafe { (*row).hyperlink() });
        let cell = list_cell.cell;
        assert!(unsafe { (*cell).hyperlink() });
        let id = unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_hyperlink(cell) }
            .unwrap();
        assert_eq!(id, 1 as crate::hyperlink::Id);
    }
    for x in 3..6 {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(x, 0)).unwrap();
        let row = list_cell.row;
        assert!(unsafe { (*row).hyperlink() });
        let cell = list_cell.cell;
        assert!(!unsafe { (*cell).hyperlink() });
    }

    assert!(t.is_dirty(screen_pt(0, 0)));

    t.deinit(Global);
}

// test "Terminal: print and change hyperlink"
#[test]
fn terminal_print_and_change_hyperlink() {
    let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

    // Setup our hyperlink and print
    t.active_mut()
        .start_hyperlink("http://one.example.com".as_bytes(), None)
        .unwrap();
    t.print_string("123".as_bytes()).unwrap();
    t.active_mut()
        .start_hyperlink("http://two.example.com".as_bytes(), None)
        .unwrap();
    t.print_string("456".as_bytes()).unwrap();

    // Verify all our cells have a hyperlink
    for x in 0..3 {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(x, 0)).unwrap();
        let cell = list_cell.cell;
        assert!(unsafe { (*cell).hyperlink() });
        let id = unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_hyperlink(cell) }
            .unwrap();
        assert_eq!(id, 1 as crate::hyperlink::Id);
    }
    for x in 3..6 {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(x, 0)).unwrap();
        let cell = list_cell.cell;
        assert!(unsafe { (*cell).hyperlink() });
        let id = unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_hyperlink(cell) }
            .unwrap();
        assert_eq!(id, 2 as crate::hyperlink::Id);
    }

    assert!(t.is_dirty(screen_pt(0, 0)));

    t.deinit(Global);
}

// test "Terminal: overwrite hyperlink"
#[test]
fn terminal_overwrite_hyperlink() {
    let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

    // Setup our hyperlink and print
    t.active_mut()
        .start_hyperlink("http://one.example.com".as_bytes(), None)
        .unwrap();
    t.print_string("123".as_bytes()).unwrap();
    t.set_cursor_pos(1, 1);
    t.active_mut().end_hyperlink();
    t.print_string("456".as_bytes()).unwrap();

    // Verify all our cells have a hyperlink
    for x in 0..3 {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(x, 0)).unwrap();
        let page = unsafe { crate::pagelist::Node::page(list_cell.node) };
        let row = list_cell.row;
        assert!(!unsafe { (*row).hyperlink() });
        let cell = list_cell.cell;
        assert!(!unsafe { (*cell).hyperlink() });
        assert!(unsafe { (*page).lookup_hyperlink(cell) }.is_none());
        assert_eq!(unsafe { (*page).hyperlink_set.count() }, 0);
    }

    assert!(t.is_dirty(screen_pt(0, 0)));

    t.deinit(Global);
}

// Printing a wide char at the right edge with an active hyperlink causes
// writeCell to write a spacer_head before printWrap sets the row wrap
// flag. The integrity check inside setHyperlink (or increaseCapacity)
// sees the unwrapped spacer head and panics. Found via fuzzing.
// test "Terminal: print wide char at right edge with hyperlink"
#[test]
fn terminal_print_wide_char_at_right_edge_with_hyperlink() {
    let mut t = Terminal::init(Global, opts(10, 5)).unwrap();

    t.active_mut()
        .start_hyperlink("http://example.com".as_bytes(), None)
        .unwrap();

    // Move cursor to the last column (1-indexed)
    t.set_cursor_pos(1, 10);

    // Print a wide character; this will call writeCell(0, .spacer_head)
    // at the right edge before calling printWrap, triggering the
    // integrity violation.
    t.print(0x4E2D).unwrap(); // U+4E2D '中'

    // Cursor wraps to row 2, after the wide char + spacer tail
    assert_eq!(unsafe { (*t.screens.active).cursor.y }, 1);
    assert_eq!(unsafe { (*t.screens.active).cursor.x }, 2);

    // Row 0, col 9: spacer head with hyperlink
    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(9, 0)).unwrap();
        assert_eq!(
            unsafe { (*list_cell.cell).wide() },
            pagepkg::Wide::SpacerHead
        );
        assert!(unsafe { (*list_cell.cell).hyperlink() });
        assert!(unsafe { (*list_cell.row).wrap() });
    }
    // Row 1, col 0: the wide char with hyperlink
    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 1)).unwrap();
        assert_eq!(unsafe { (*list_cell.cell).content_codepoint() }, 0x4E2D);
        assert_eq!(unsafe { (*list_cell.cell).wide() }, pagepkg::Wide::Wide);
        assert!(unsafe { (*list_cell.cell).hyperlink() });
    }
    // Row 1, col 1: spacer tail with hyperlink
    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(1, 1)).unwrap();
        assert_eq!(
            unsafe { (*list_cell.cell).wide() },
            pagepkg::Wide::SpacerTail
        );
        assert!(unsafe { (*list_cell.cell).hyperlink() });
    }

    t.deinit(Global);
}

// A cursor style or hyperlink ID is an index into a set stored in the
// page memory of the page the cursor pin points at. When scrollClear
// (here via ED 22, kitty's scroll_complete) pushes the active area onto
// a later page while the cursor pin is still on an earlier one,
// cursorReload must migrate the cursor's style and hyperlink references
// to the destination page. It previously replaced the pin directly,
// leaving the cursor holding an ID that was dead or aliased an unrelated
// entry on the new page, and the next print attached a live cell to it.
// Found via fuzzing.
// test "Terminal: scrollClear across pages keeps cursor hyperlink refs page-local"
#[test]
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'Parity tail C-1 — the G6 triage'): past a 15-minute Miri bound in the debug-assertions-ON G6 lane, where `Page::assert_integrity` runs on nearly every page mutation (measured per test; see the C-1 table there). It RUNS in the G6c lane (`check_leaks.sh --debug-assertions-off`). Do NOT widen this to a plain `miri` ignore: that would drop the test from BOTH lanes."
)]
fn terminal_scroll_clear_across_pages_keeps_cursor_hyperlink_refs_page_local() {
    // Minimized from a 774-byte AFL fuzz input. Reading it:
    //
    //   A                 print, so REP has something to repeat
    //   ESC [ 48111 b     REP, filling the page and spilling onto a second
    //   ESC ] 8 ; ; 0x93  OSC 8; the C1 byte terminates the OSC and makes
    //                     the URI non-empty, so a hyperlink starts
    //   ESC [ 11 A        CUU, moving the cursor back onto the first page
    //   ESC [ 22 J        ED 22, i.e. scroll_complete -> Screen.scrollClear
    //   B                 print, which attaches the cursor hyperlink
    //   ESC ] 8 ; ; ESC   OSC 8 with an empty URI, ending the hyperlink
    //
    // The grid must be wide enough to fill a page from a single REP, so
    // this does not reproduce at 80x24.
    let input = b"A\x1b[48111b\x1b]8;;\x93\x1b[11A\x1b[22JB\x1b]8;;\x1b";

    let mut t = Terminal::init(Global, opts(200, 50)).unwrap();

    {
        let mut s = t.vt_stream();
        s.next_slice(input);
        s.deinit();
    }

    // With slow runtime safety on, the page integrity checks during the
    // stream above already catch the bug. Verify the ref counts explicitly
    // as well so this test is meaningful with runtime safety off: every
    // cell holding a hyperlink ID owns a reference, so a count below the
    // number of holding cells means a live cell points at an entry that
    // was already freed.
    let mut node_ = t.active_mut().pages.pages.first;
    while !node_.is_null() {
        let node = node_;
        node_ = unsafe { (*node).next };

        let page: *mut Page = unsafe { crate::pagelist::Node::page(node) };
        let base = unsafe { (*page).memory_ptr() };
        let cap = unsafe { (*page).hyperlink_set.layout.cap };
        if cap == 0 {
            continue;
        }

        let mut holders = vec![0u32; cap];

        let rows = unsafe { (*page).rows.ptr(base) };
        for y in 0..(unsafe { (*page).size.rows } as usize) {
            let row: *mut Row = unsafe { rows.add(y) };
            if !unsafe { (*row).hyperlink() } {
                continue;
            }
            let cells = unsafe { (*row).cells().ptr(base) };
            for x in 0..(unsafe { (*page).size.cols } as usize) {
                let cell: *mut pagepkg::Cell = unsafe { cells.add(x) };
                if !unsafe { (*cell).hyperlink() } {
                    continue;
                }
                let id = match unsafe { (*page).lookup_hyperlink(cell) } {
                    Some(id) => id,
                    None => continue,
                };
                if (id as usize) < cap {
                    holders[id as usize] += 1;
                }
            }
        }

        for (id, &held) in holders.iter().enumerate() {
            if held == 0 {
                continue;
            }
            let refs = unsafe {
                (*page)
                    .hyperlink_set
                    .ref_count(base, id as crate::hyperlink::Id)
            };
            assert!(refs as u32 >= held);
        }
    }

    // If the cursor still has an active hyperlink, its own extra
    // reference must live on the cursor's page.
    let cursor_hyperlink_id = unsafe { (*t.screens.active).cursor.hyperlink_id };
    if cursor_hyperlink_id != 0 {
        let page: *mut Page =
            unsafe { crate::pagelist::Node::page((*(*t.screens.active).cursor.page_pin).node) };
        assert!(
            unsafe {
                (*page)
                    .hyperlink_set
                    .ref_count((*page).memory_ptr(), cursor_hyperlink_id)
            } > 0
        );
    }

    t.deinit(Global);
}

// test "Terminal: default style is empty"
#[test]
fn terminal_default_style_is_empty() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.print('A' as u32).unwrap();

    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).content_codepoint() }, 'A' as u21);
        assert_eq!(unsafe { (*cell).style_id() }, 0);
    }

    t.deinit(Global);
}

// test "Terminal: print with style marks the row as styled"
#[test]
fn terminal_print_with_style_marks_the_row_as_styled() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.set_attribute(sgr::Attribute::Bold).unwrap();
    t.print('A' as u32).unwrap();
    t.set_attribute(sgr::Attribute::Unset).unwrap();
    t.print('B' as u32).unwrap();

    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
        assert!(unsafe { (*list_cell.row).styled() });
    }

    t.deinit(Global);
}

// test "Terminal: printRepeat simple"
#[test]
fn terminal_print_repeat_simple() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.print_string("A".as_bytes()).unwrap();
    t.print_repeat(1).unwrap();
    assert!(t.is_dirty(active_pt(0, 0)));

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "AA".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: printRepeat wrap"
#[test]
fn terminal_print_repeat_wrap() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.print_string("    A".as_bytes()).unwrap();
    t.print_repeat(1).unwrap();
    assert!(t.is_dirty(active_pt(0, 0)));

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "    A\nA".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: printRepeat no previous character"
#[test]
fn terminal_print_repeat_no_previous_character() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    t.print_repeat(1).unwrap();
    assert!(!t.is_dirty(active_pt(0, 0)));

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "".as_bytes());
    }

    t.deinit(Global);
}
