//! PORTS: ghostty src/terminal/Terminal.zig — the `deccolm` / `resize` / `switchScreen` / `switchScreenMode` / `fullReset` / `softReset` tests.

use super::*;

// ===================================================================
// vt-12b-5: Terminal modes/resize/switch tests (deccolm / resize /
// switchScreen / switchScreenMode / fullReset), now unblocked.
// ===================================================================

// test "Terminal: saveCursor resize"
#[test]
fn terminal_save_cursor_resize() {
    let alloc = Global;
    let mut t = Terminal::init(alloc, opts(10, 5)).unwrap();

    t.set_cursor_pos(1, 10);
    t.save_cursor();
    t.resize(
        alloc,
        Resize {
            cols: 5,
            rows: 5,
            cell_size_px: None,
        },
    )
    .unwrap();
    t.restore_cursor();
    t.print('X' as u32).unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "    X".as_bytes());
    }

    t.deinit(alloc);
}

// test "Terminal: cursor defaults update current default cursor"
#[test]
fn terminal_cursor_defaults_update_current_default_cursor() {
    let mut o = opts(10, 10);
    o.default_cursor_style = crate::screen::CursorStyle::Bar;
    o.default_cursor_blink = Some(true);
    let mut t = Terminal::init(Global, o).unwrap();

    // Initialization applies the configured defaults.
    assert!(t.cursor.is_default);
    assert_eq!(crate::screen::CursorStyle::Bar, unsafe {
        (*t.screens.active).cursor.cursor_style
    });
    assert!(t.modes.get(modespkg::Mode::CursorBlinking));

    // Configuration changes are immediately visible while the cursor still
    // follows its defaults.
    t.set_default_cursor_style(crate::screen::CursorStyle::Underline);
    t.set_default_cursor_blink(Some(false));
    assert!(t.cursor.is_default);
    assert_eq!(crate::screen::CursorStyle::Underline, unsafe {
        (*t.screens.active).cursor.cursor_style
    });
    assert!(!t.modes.get(modespkg::Mode::CursorBlinking));

    // Null restores the terminal emulator's blinking default.
    t.set_default_cursor_blink(None);
    assert!(t.modes.get(modespkg::Mode::CursorBlinking));

    t.deinit(Global);
}

// test "Terminal: cursor defaults do not override explicit cursor"
//
// NEGATIVE CONTROL (P3-W4 T4 Step 4): the last block is upstream's RIS
// round-trip — set a style via DECSCUSR, `full_reset`, and assert the
// CONFIGURED default is restored, NOT the hardcoded initial one. A port that
// leaves the restore in the handler (or drops it) fails here.
#[test]
fn terminal_cursor_defaults_do_not_override_explicit_cursor() {
    let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

    t.set_cursor_style(ansi::CursorStyle::BlinkingBar);
    assert!(!t.cursor.is_default);
    assert_eq!(crate::screen::CursorStyle::Bar, unsafe {
        (*t.screens.active).cursor.cursor_style
    });
    assert!(t.modes.get(modespkg::Mode::CursorBlinking));

    // New defaults are retained without replacing the explicit appearance.
    t.set_default_cursor_style(crate::screen::CursorStyle::Underline);
    t.set_default_cursor_blink(Some(false));
    assert_eq!(
        crate::screen::CursorStyle::Underline,
        t.cursor.default_style
    );
    assert_eq!(Some(false), t.cursor.default_blink);
    assert_eq!(crate::screen::CursorStyle::Bar, unsafe {
        (*t.screens.active).cursor.cursor_style
    });
    assert!(t.modes.get(modespkg::Mode::CursorBlinking));

    // Selecting the default applies the values that changed above.
    t.set_cursor_style(ansi::CursorStyle::Default);
    assert!(t.cursor.is_default);
    assert_eq!(crate::screen::CursorStyle::Underline, unsafe {
        (*t.screens.active).cursor.cursor_style
    });
    assert!(!t.modes.get(modespkg::Mode::CursorBlinking));

    // A full reset also leaves the cursor on the configured defaults.
    t.set_cursor_style(ansi::CursorStyle::SteadyBlock);
    t.full_reset();
    assert!(t.cursor.is_default);
    assert_eq!(crate::screen::CursorStyle::Underline, unsafe {
        (*t.screens.active).cursor.cursor_style
    });
    assert!(!t.modes.get(modespkg::Mode::CursorBlinking));

    t.deinit(Global);
}

// test "Terminal: fullReset with a non-empty pen"
#[test]
fn terminal_full_reset_with_a_non_empty_pen() {
    let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

    t.set_attribute(sgr::Attribute::DirectColorFg(color::RGB {
        r: 0xFF,
        g: 0,
        b: 0x7F,
    }))
    .unwrap();
    t.set_attribute(sgr::Attribute::DirectColorBg(color::RGB {
        r: 0xFF,
        g: 0,
        b: 0x7F,
    }))
    .unwrap();
    unsafe { (*t.screens.active).cursor.semantic_content = pagepkg::SemanticContent::Input };
    t.full_reset();

    {
        let cursor_x = unsafe { (*t.screens.active).cursor.x };
        let cursor_y = unsafe { (*t.screens.active).cursor.y };
        let list_cell = t
            .active_mut()
            .pages
            .get_cell(active_pt(cursor_x, cursor_y as u32))
            .unwrap();
        let cell = list_cell.cell;
        assert!(unsafe { (*cell).style_id() } == 0);
    }

    assert_eq!(0 as style::Id, unsafe {
        (*t.screens.active).cursor.style_id
    });
    assert_eq!(pagepkg::SemanticContent::Output, unsafe {
        (*t.screens.active).cursor.semantic_content
    });

    t.deinit(Global);
}

// test "Terminal: fullReset hyperlink"
#[test]
fn terminal_full_reset_hyperlink() {
    let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

    t.active_mut()
        .start_hyperlink("http://example.com".as_bytes(), None)
        .unwrap();
    t.full_reset();
    assert_eq!(0, unsafe { (*t.screens.active).cursor.hyperlink_id });

    t.deinit(Global);
}

// test "Terminal: fullReset with a non-empty saved cursor"
#[test]
fn terminal_full_reset_with_a_non_empty_saved_cursor() {
    let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

    t.set_attribute(sgr::Attribute::DirectColorFg(color::RGB {
        r: 0xFF,
        g: 0,
        b: 0x7F,
    }))
    .unwrap();
    t.set_attribute(sgr::Attribute::DirectColorBg(color::RGB {
        r: 0xFF,
        g: 0,
        b: 0x7F,
    }))
    .unwrap();
    t.save_cursor();
    t.full_reset();

    {
        let cursor_x = unsafe { (*t.screens.active).cursor.x };
        let cursor_y = unsafe { (*t.screens.active).cursor.y };
        let list_cell = t
            .active_mut()
            .pages
            .get_cell(active_pt(cursor_x, cursor_y as u32))
            .unwrap();
        let cell = list_cell.cell;
        assert!(unsafe { (*cell).style_id() } == 0);
    }

    assert_eq!(0 as style::Id, unsafe {
        (*t.screens.active).cursor.style_id
    });

    t.deinit(Global);
}

// test "Terminal: fullReset origin mode"
#[test]
fn terminal_full_reset_origin_mode() {
    let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

    t.set_cursor_pos(3, 5);
    t.modes.set(modespkg::Mode::Origin, true);
    t.full_reset();

    // Origin mode should be reset and the cursor should be moved
    assert_eq!(0_usize, unsafe { (*t.screens.active).cursor.y } as usize);
    assert_eq!(0_usize, unsafe { (*t.screens.active).cursor.x } as usize);
    assert!(!t.modes.get(modespkg::Mode::Origin));

    t.deinit(Global);
}

// test "Terminal: fullReset status display"
#[test]
fn terminal_full_reset_status_display() {
    let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

    t.status_display = ansi::StatusDisplay::StatusLine;
    t.full_reset();
    assert!(t.status_display == ansi::StatusDisplay::Main);

    t.deinit(Global);
}

// test "Terminal: fullReset preserves kitty graphics limits"
//
#[test]
#[cfg_attr(
    not(all(feature = "kitty-graphics", not(target_os = "none"))),
    ignore = "SkipZigTest: needs options::KITTY_GRAPHICS (R14)"
)]
fn terminal_full_reset_preserves_kitty_graphics_limits() {
    use crate::kitty::graphics_image::{Limits, TemporaryFileLimit};

    // if (comptime !build_options.kitty_graphics) return error.SkipZigTest;
    //
    // PORT NOTE: the `cfg_attr(.., ignore)` above is this crate's idiom for
    // `error.SkipZigTest` behind a build option (R14; see `dcs.rs`'s
    // `tmux_enter_and_implicit_exit`); `kitty_images` is only maintained
    // when the option is on.

    let temp_dir = b"/tmp/rustty-kitty-images";

    let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

    // SAFETY: `temp_dir` is `&'static [u8]`, live for the whole test.
    t.set_kitty_graphics_loading_limits(unsafe { Limits::all_with_temp_dir(temp_dir) });
    for total_limit in [1234_usize, 0] {
        t.set_kitty_graphics_size_limit(Global, total_limit);
        t.full_reset();

        // SAFETY: `screens.active` is the live primary screen for the whole
        // borrow; we only read its kitty storage.
        let storage = unsafe { &(*t.screens.active).kitty_images };
        assert_eq!(total_limit, storage.total_limit);
        assert!(storage.image_limits.file);
        assert!(storage.image_limits.shared_memory);
        match &storage.image_limits.temporary_file {
            TemporaryFileLimit::Enabled { directory } => {
                assert_eq!(temp_dir.as_slice(), directory.as_slice());
            }
            TemporaryFileLimit::Disabled => panic!("temporary file limit was reset"),
        }
    }

    t.deinit(Global);
}

// https://github.com/mitchellh/ghostty/issues/1607
// test "Terminal: fullReset clears alt screen kitty keyboard state"
#[test]
fn terminal_full_reset_clears_alt_screen_kitty_keyboard_state() {
    let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

    t.switch_screen_mode(SwitchScreenMode::M1049, true).unwrap();
    unsafe {
        (*t.screens.active).kitty_keyboard.push(kitty::key::Flags {
            disambiguate: true,
            report_events: false,
            report_alternates: true,
            report_all: true,
            report_associated: true,
        });
    }
    t.switch_screen_mode(SwitchScreenMode::M1049, false)
        .unwrap();

    t.full_reset();
    assert!(t.screens.get(crate::screen_set::Key::Alternate).is_none());

    t.deinit(Global);
}

// test "Terminal: fullReset default modes"
#[test]
fn terminal_full_reset_default_modes() {
    // PORT NOTE: the reference `init(alloc, .{ .cols, .rows, .default_modes =
    // .{ .grapheme_cluster = true } })`. Our `opts` helper supplies the field
    // defaults, so we override `default_modes` after building it. The packed
    // bit-set helpers are private, so we build the `ModePacked` value via the
    // public `ModeState::set` and take its `values` (faithfully producing
    // `.{ .grapheme_cluster = true }`).
    let mut o = opts(10, 10);
    let mut ms = modespkg::ModeState::default();
    ms.set(modespkg::Mode::GraphemeCluster, true);
    o.default_modes = ms.values;
    let mut t = Terminal::init(Global, o).unwrap();
    assert!(t.modes.get(modespkg::Mode::GraphemeCluster));
    t.full_reset();
    assert!(t.modes.get(modespkg::Mode::GraphemeCluster));

    t.deinit(Global);
}

// test "Terminal: fullReset tracked pins"
#[test]
// WS18 release lane: this test drives a slow-runtime-safety mechanism (the
// pagelist integrity gate / `pin_is_valid`), which the port compiles out when
// `debug_assertions` is off — faithful to the reference's
// `build_options.slow_runtime_safety`, under which the oracle's test only
// exists. Debug-only; it still runs and passes there.
#[cfg_attr(
    not(debug_assertions),
    ignore = "DEBUG-ONLY BY DESIGN, and this reason was MEASURED (fix round 1 of parity S166-F2): this test drives the slow-runtime-safety mechanism — `PageList::assert_integrity` / `pin_is_valid` — which the port compiles out when `debug_assertions` is off, faithful to the reference's `build_options.slow_runtime_safety`, under which the oracle's counterpart only exists. It is NOT ignored because of the tripwire gate (that claim was false and is gone): un-ignoring it turns the release lane RED, which is why it stays."
)]
fn terminal_full_reset_tracked_pins() {
    let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

    // Create a tracked pin
    let cursor_pin = unsafe { *(*t.screens.active).cursor.page_pin };
    let p = t.active_mut().pages.track_pin(cursor_pin).unwrap();
    t.full_reset();
    assert!(t.active_mut().pages.pin_is_valid(unsafe { *p }));

    t.deinit(Global);
}

// test "Terminal: default xt checksum survives resets" (oracle 9272a2f7c)
//
// PORT NOTE: this also pins P8-W2 T1's `full_reset` slot: RIS builds `Flags`
// with a `..Flags::default()` spread, which would silently reset
// `xt_checksum` to the zero value instead of `default_xt_checksum`.
#[test]
fn terminal_default_xt_checksum_survives_resets() {
    use crate::xt_checksum::Flags;

    let mut t = Terminal::init(
        Global,
        Options {
            default_xt_checksum: Flags::default().with_positive(true),
            ..opts(10, 10)
        },
    )
    .unwrap();
    assert_eq!(Flags::default().with_positive(true), t.flags.xt_checksum);

    t.flags.xt_checksum = Flags::default().with_full(true);
    t.full_reset();
    assert_eq!(Flags::default().with_positive(true), t.flags.xt_checksum);

    t.set_default_xt_checksum(Flags::default().with_no_trim(true));
    assert_eq!(Flags::default().with_no_trim(true), t.flags.xt_checksum);
    t.full_reset();
    assert_eq!(Flags::default().with_no_trim(true), t.flags.xt_checksum);

    // t.flags.xt_checksum = .{ .full = true };
    // t.softReset();
    // try testing.expectEqual(xt_checksum.Flags{ .no_trim = true }, t.flags.xt_checksum);
    t.flags.xt_checksum = Flags::default().with_full(true);
    t.soft_reset();
    assert_eq!(Flags::default().with_no_trim(true), t.flags.xt_checksum);

    t.deinit(Global);
}

// test "Terminal: softReset modes" (oracle f473f3109)
#[test]
fn terminal_soft_reset_modes() {
    use modespkg::Mode;

    let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

    t.modes.set(Mode::Insert, true);
    t.modes.set(Mode::Origin, true);
    t.modes.set(Mode::Wraparound, false);
    t.modes.set(Mode::ReverseWrap, true);
    t.modes.set(Mode::CursorVisible, false);
    t.modes.set(Mode::CursorKeys, true);
    t.modes.set(Mode::KeypadKeys, true);
    t.modes.set(Mode::BracketedPaste, true);
    t.soft_reset();

    assert!(!t.modes.get(Mode::Insert));
    assert!(!t.modes.get(Mode::Origin));
    assert!(t.modes.get(Mode::Wraparound));
    assert!(!t.modes.get(Mode::ReverseWrap));
    assert!(t.modes.get(Mode::CursorVisible));
    assert!(!t.modes.get(Mode::CursorKeys));
    assert!(!t.modes.get(Mode::KeypadKeys));

    // Modes DECSTR doesn't cover are left alone.
    assert!(t.modes.get(Mode::BracketedPaste));

    t.deinit(Global);
}

// test "Terminal: softReset margins" (oracle f473f3109)
#[test]
fn terminal_soft_reset_margins() {
    let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

    t.set_top_and_bottom_margin(3, 4);
    t.modes.set(modespkg::Mode::EnableLeftAndRightMargin, true);
    t.set_left_and_right_margin(5, 6);
    t.soft_reset();

    assert!(!t.modes.get(modespkg::Mode::EnableLeftAndRightMargin));
    assert_eq!(
        ScrollingRegion {
            top: 0,
            bottom: 9,
            left: 0,
            right: 9,
        },
        t.scrolling_region
    );

    t.deinit(Global);
}

// test "Terminal: softReset keeps the cursor and screen" (oracle f473f3109)
#[test]
fn terminal_soft_reset_keeps_the_cursor_and_screen() {
    let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

    t.print_string(b"hello").unwrap();
    t.set_cursor_pos(6, 5);
    t.soft_reset();

    assert_eq!(4, t.active_mut().cursor.x);
    assert_eq!(5, t.active_mut().cursor.y);

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "hello".as_bytes());
    }

    t.deinit(Global);
}

// test "Terminal: softReset saved cursor" (oracle f473f3109)
#[test]
fn terminal_soft_reset_saved_cursor() {
    let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

    t.set_cursor_pos(6, 5);
    t.set_attribute(sgr::Attribute::Bold).unwrap();
    t.save_cursor();
    t.soft_reset();
    t.restore_cursor();

    assert_eq!(0, t.active_mut().cursor.x);
    assert_eq!(0, t.active_mut().cursor.y);
    assert!(!t.active_mut().cursor.style.flags.bold);

    t.deinit(Global);
}

// test "Terminal: softReset pen, protection, and charsets" (oracle f473f3109)
#[test]
fn terminal_soft_reset_pen_protection_and_charsets() {
    let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

    t.set_attribute(sgr::Attribute::Bold).unwrap();
    t.set_protected_mode(ansi::ProtectedMode::Dec);
    t.configure_charset(charsets::Slots::G0, charsets::Charset::DecSpecial);
    t.soft_reset();

    assert_eq!(0 as style::Id, t.active_mut().cursor.style_id);
    assert!(!t.active_mut().cursor.protected);
    assert_eq!(
        charsets::Charset::Utf8,
        t.active_mut().charset.charsets.get(charsets::Slots::G0)
    );

    t.deinit(Global);
}

// test "Terminal: softReset status display and checksum" (oracle f473f3109)
#[test]
fn terminal_soft_reset_status_display_and_checksum() {
    use crate::xt_checksum::Flags;

    let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

    t.status_display = ansi::StatusDisplay::StatusLine;
    t.flags.xt_checksum = Flags::default().with_positive(true);
    t.soft_reset();

    assert!(t.status_display == ansi::StatusDisplay::Main);
    assert_eq!(Flags::default(), t.flags.xt_checksum);

    t.deinit(Global);
}

// test "Terminal: softReset palette" (oracle f473f3109)
#[test]
fn terminal_soft_reset_palette() {
    let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

    // Nothing changed, so nothing is redrawn.
    t.soft_reset();
    assert!(!t.flags.dirty.palette);

    let red = color::RGB {
        r: 0xff,
        g: 0,
        b: 0,
    };
    t.colors.palette.set(1, red);
    t.colors.palette.set(200, red);
    t.soft_reset();

    assert!(t.flags.dirty.palette);
    assert_eq!(0_usize, t.colors.palette.mask.count());
    assert_eq!(color::DEFAULT[1], t.colors.palette.current[1]);
    assert_eq!(color::DEFAULT[200], t.colors.palette.current[200]);

    t.deinit(Global);
}

// test "Terminal: glyph APC stores session glossary entries"
#[test]
#[cfg_attr(
    not(feature = "glyph-protocol"),
    ignore = "SkipZigTest: needs options::GLYPH_PROTOCOL (R14)"
)]
fn terminal_glyph_apc_stores_session_glossary_entries() {
    // if (comptime !build_options.glyph_protocol) return error.SkipZigTest;

    use crate::apc::glyph::response::{Coverage, Response, ResponseQuery, ResponseRegister};

    let alloc = Global;
    let mut t = Terminal::init(alloc, opts(80, 24)).unwrap();

    // var register_parser = glyph.CommandParser.init(alloc, 1024 * 1024);
    // defer register_parser.deinit();
    let mut register_parser = glyph::CommandParser::init(crate::alloc::Global, 1024 * 1024);
    // for ("r;cp=e0a0;AAAAAAAAAAAAAA==") |byte| try register_parser.feed(byte);
    for byte in *b"r;cp=e0a0;AAAAAAAAAAAAAA==" {
        register_parser.feed(byte).unwrap();
    }
    // var register_req = try register_parser.complete(alloc);
    // defer register_req.deinit(alloc);
    let register_req = register_parser.complete(crate::alloc::Global).unwrap();

    // try testing.expectEqual(glyph.Response{ .register = .{ .cp = 0xE0A0 } },
    //     t.glyphProtocol(alloc, &register_req).?);
    assert_eq!(
        Response::Register(ResponseRegister::new(0xE0A0)),
        t.glyph_protocol(alloc, &register_req).unwrap()
    );
    // try testing.expect(t.glyph_glossary.contains(0xE0A0));
    assert!(t.glyph_glossary.contains(0xE0A0));
    // try testing.expect(t.flags.dirty.glyph_glossary);
    assert!(t.flags.dirty.glyph_glossary);

    // var query_parser = glyph.CommandParser.init(alloc, 1024 * 1024);
    // defer query_parser.deinit();
    let mut query_parser = glyph::CommandParser::init(crate::alloc::Global, 1024 * 1024);
    // for ("q;cp=e0a0") |byte| try query_parser.feed(byte);
    for byte in *b"q;cp=e0a0" {
        query_parser.feed(byte).unwrap();
    }
    // var query_req = try query_parser.complete(alloc);
    // defer query_req.deinit(alloc);
    let query_req = query_parser.complete(crate::alloc::Global).unwrap();

    // try testing.expectEqual(glyph.Response{ .query = .{
    //     .cp = 0xE0A0, .status = .{ .glossary = true } } },
    //     t.glyphProtocol(alloc, &query_req).?);
    assert_eq!(
        Response::Query(ResponseQuery {
            cp: 0xE0A0,
            status: Coverage {
                glossary: true,
                ..Coverage::default()
            },
        }),
        t.glyph_protocol(alloc, &query_req).unwrap()
    );

    // t.fullReset();
    t.full_reset();
    // try testing.expect(!t.glyph_glossary.contains(0xE0A0));
    assert!(!t.glyph_glossary.contains(0xE0A0));

    t.deinit(alloc);
}

// https://github.com/mitchellh/ghostty/issues/272
// This is also tested in depth in screen resize tests but I want to keep
// this test around to ensure we don't regress at multiple layers.
// test "Terminal: resize less cols with wide char then print"
#[test]
fn terminal_resize_less_cols_with_wide_char_then_print() {
    let alloc = Global;
    let mut t = Terminal::init(alloc, opts(3, 3)).unwrap();

    t.print('x' as u32).unwrap();
    t.print('😀' as u32).unwrap(); // 0x1F600
    t.resize(
        alloc,
        Resize {
            cols: 2,
            rows: 3,
            cell_size_px: None,
        },
    )
    .unwrap();
    t.set_cursor_pos(1, 2);
    t.print('😀' as u32).unwrap(); // 0x1F600

    t.deinit(alloc);
}

// test "Terminal: resize less cols without reflow cutting wide char tail"
#[test]
fn terminal_resize_less_cols_without_reflow_cutting_wide_char_tail() {
    let alloc = Global;
    let mut t = Terminal::init(alloc, opts(3, 1)).unwrap();

    t.print('a' as u32).unwrap();
    t.print('一' as u32).unwrap();
    t.modes.set(modespkg::Mode::Wraparound, false);
    t.resize(
        alloc,
        Resize {
            cols: 2,
            rows: 1,
            cell_size_px: None,
        },
    )
    .unwrap();

    let list_cell = t.active_mut().pages.get_cell(active_pt(1, 0)).unwrap();
    assert!(unsafe { (*list_cell.cell).is_empty() });

    t.deinit(alloc);
}

// PORT NOTE: NOT a port of a reference `test {}` block (Rustty-only, N518,
// upstream e6db5b633). The reference test above only checks the cut cell;
// this drives the whole reachable sequence through the stream: narrow the
// alt screen (no reflow) so a wide character straddles the new right edge,
// then ICH (`CSI @`) and EL 1 (`CSI 1 K`), which both pair a wide head with
// the cell after it. Before the fix they hit the `debug_assert!` in
// `Screen::clear_cells` (debug) or wrote `cells[cols]` (release).
// test "Terminal: resize less cols on alt screen then ICH and EL1 with a cut wide char"
#[test]
fn terminal_resize_less_cols_on_alt_screen_then_ich_and_el1_with_a_cut_wide_char() {
    let alloc = Global;
    let mut t = Terminal::init(alloc, opts(10, 3)).unwrap();

    {
        let mut stream = t.vt_stream();
        // CSI ? 1049 h -- alternate screen
        stream.next_slice(b"\x1b[?1049h");
        // Head of the wide character at column 8, its spacer tail at 9.
        stream.next_slice("abcdefgh\u{6A4B}".as_bytes());
        stream.deinit();
    }

    // Column 8 becomes cols - 1, cutting the spacer tail off.
    t.resize(
        alloc,
        Resize {
            cols: 9,
            rows: 3,
            cell_size_px: None,
        },
    )
    .unwrap();

    // The cut must not leave a wide head in the new last column.
    {
        let list_cell = t.active_mut().pages.get_cell(active_pt(8, 0)).unwrap();
        assert_ne!(crate::page::Wide::Wide, unsafe { (*list_cell.cell).wide() });
    }

    {
        let mut stream = t.vt_stream();
        // A background color makes every blank cell a BgColorPalette cell,
        // which `is_empty` rejects, so a stray write past the row shows up
        // in the capacity loop below even in release builds.
        stream.next_slice(b"\x1b[41m");
        // ICH from column 0: a row shift over the narrowed row.
        stream.next_slice(b"\x1b[1;1H");
        stream.next_slice(b"\x1b[@");
        // Cursor to the last column, then ICH and EL 1.
        stream.next_slice(b"\x1b[1;9H");
        stream.next_slice(b"\x1b[@");
        stream.next_slice(b"\x1b[1K");
        stream.deinit();
    }

    let list_cell = t.active_mut().pages.get_cell(active_pt(8, 0)).unwrap();
    let cell = unsafe { *list_cell.cell };
    assert_eq!(crate::page::Wide::Narrow, cell.wide());
    assert!(!cell.has_text());

    // Nothing at or past `cols` was written, up to the page capacity.
    let page = unsafe { &*crate::pagelist::Node::page(list_cell.node) };
    assert_eq!(9, page.size.cols);
    let cells = unsafe { (*list_cell.row).cells().ptr(page.memory_ptr()) };
    for x in 9..page.capacity.cols as usize {
        assert!(unsafe { (*cells.add(x)).is_empty() }, "cell {x} written");
    }

    t.deinit(alloc);
}

// test "Terminal: resize resets synchronized output"
#[test]
fn terminal_resize_resets_synchronized_output() {
    let alloc = Global;
    let mut t = Terminal::init(alloc, opts(10, 5)).unwrap();

    t.modes.set(modespkg::Mode::SynchronizedOutput, true);
    t.resize(
        alloc,
        Resize {
            cols: 10,
            rows: 5,
            cell_size_px: None,
        },
    )
    .unwrap();
    assert!(!t.modes.get(modespkg::Mode::SynchronizedOutput));

    t.deinit(alloc);
}

// test "Terminal: resize rejects zero dimensions before mutation"
#[test]
fn terminal_resize_rejects_zero_dimensions_before_mutation() {
    let alloc = Global;
    let mut t = Terminal::init(alloc, opts(10, 5)).unwrap();

    t.width_px = 100;
    t.height_px = 100;
    t.flags.dirty.clear = false;

    assert_eq!(
        Err(ResizeError::InvalidValue),
        t.resize(
            alloc,
            Resize {
                cols: 0,
                rows: 5,
                cell_size_px: Some(ResizeCellSize {
                    width: 9,
                    height: 18
                }),
            }
        )
    );
    assert_eq!(
        Err(ResizeError::InvalidValue),
        t.resize(
            alloc,
            Resize {
                cols: 10,
                rows: 0,
                cell_size_px: Some(ResizeCellSize {
                    width: 9,
                    height: 18
                }),
            }
        )
    );

    assert_eq!(10 as size::CellCountInt, t.cols);
    assert_eq!(5 as size::CellCountInt, t.rows);
    assert_eq!(100_u32, t.width_px);
    assert_eq!(100_u32, t.height_px);
    assert!(!t.flags.dirty.clear);

    t.deinit(alloc);
}

// test "Terminal: resize preserves pixel dimensions when omitted"
#[test]
fn terminal_resize_preserves_pixel_dimensions_when_omitted() {
    let alloc = Global;
    let mut t = Terminal::init(alloc, opts(10, 5)).unwrap();

    t.width_px = 90;
    t.height_px = 90;
    t.resize(
        alloc,
        Resize {
            cols: 20,
            rows: 10,
            cell_size_px: None,
        },
    )
    .unwrap();

    assert_eq!(90_u32, t.width_px);
    assert_eq!(90_u32, t.height_px);

    t.deinit(alloc);
}

// test "Terminal: resize updates pixels without changing cell dimensions"
#[test]
fn terminal_resize_updates_pixels_without_changing_cell_dimensions() {
    let alloc = Global;
    let mut t = Terminal::init(alloc, opts(10, 5)).unwrap();

    t.resize(
        alloc,
        Resize {
            cols: 10,
            rows: 5,
            cell_size_px: Some(ResizeCellSize {
                width: 9,
                height: 18,
            }),
        },
    )
    .unwrap();

    assert_eq!(90_u32, t.width_px);
    assert_eq!(90_u32, t.height_px);

    t.deinit(alloc);
}

// test "Terminal: resize pixel dimensions saturate"
#[test]
fn terminal_resize_pixel_dimensions_saturate() {
    let alloc = Global;
    let mut t = Terminal::init(alloc, opts(2, 3)).unwrap();

    t.resize(
        alloc,
        Resize {
            cols: 2,
            rows: 3,
            cell_size_px: Some(ResizeCellSize {
                width: u32::MAX,
                height: u32::MAX,
            }),
        },
    )
    .unwrap();

    assert_eq!(u32::MAX, t.width_px);
    assert_eq!(u32::MAX, t.height_px);

    t.deinit(alloc);
}

// PORT FINDING GUARD (P3-W4 T2): the alternate screen's `Screen::resize`
// must use the reference's `Screen.Resize.prompt_redraw` DEFAULT, which is
// `.false`. rustty passed `.true` here, citing a default the reference has
// never had. With `.true` the alternate screen clears the cursor's prompt
// row on every resize whenever the cursor's semantic content is not
// `.output`; with `.false` it does not. No upstream test covers this
// because upstream never had the bug.
#[test]
fn terminal_resize_does_not_prompt_redraw_the_alternate_screen() {
    let alloc = Global;
    let mut t = Terminal::init(alloc, opts(10, 3)).unwrap();

    // Enter the alternate screen and put a prompt-ish line on it with the
    // cursor still marked as prompt input (the condition
    // `clear_prompt_for_redraw` keys off).
    t.switch_screen_mode(SwitchScreenMode::M1049, true).unwrap();
    unsafe {
        (*t.screens.active).cursor_set_semantic_content(crate::screen::SemanticContentArg::Prompt(
            osc::semantic_prompt::PromptKind::Initial,
        ));
    }
    t.print_string(b"> abc").unwrap();

    // shell_redraws_prompt is what the PRIMARY screen keys off; the
    // alternate screen must not honor it regardless.
    t.flags.shell_redraws_prompt = osc::semantic_prompt::Redraw::True;

    t.resize(
        alloc,
        Resize {
            cols: 20,
            rows: 3,
            cell_size_px: None,
        },
    )
    .unwrap();

    let s = t.plain_string(Global).unwrap();
    assert_eq!(b"> abc".as_slice(), s.as_slice());

    t.deinit(alloc);
}

// test "Terminal: resize preserves tabstops on allocation failure"
#[test]
fn terminal_resize_preserves_tabstops_on_allocation_failure() {
    // var failing = testing.FailingAllocator.init(testing.allocator, .{});
    // const alloc = failing.allocator();
    //
    // Ported as written (parity tail S7-3; a `Tabstops` tripwire stood in
    // while the stops lived on the Rust heap). 513 columns is past
    // `PREALLOC_COLUMNS`, so the dynamic-stops allocation IS attempted.
    let failing = crate::alloc::testing::FailingAllocator::over_heap(
        crate::alloc::testing::Config::default(),
    );
    let c = failing.allocator();
    crate::alloc::testing::with_handle(&c, |alloc| {
        let mut t = Terminal::init(alloc, opts(10, 1)).unwrap();
        // defer t.deinit(alloc);

        // failing.fail_index = failing.alloc_index;
        failing.fail_index.set(failing.alloc_index.get());
        assert_eq!(
            Err(ResizeError::OutOfMemory),
            t.resize(
                alloc,
                Resize {
                    cols: 513,
                    rows: 1,
                    cell_size_px: None
                }
            )
        );
        assert!(failing.has_induced_failure.get());

        // The original Tabstops was never replaced: cols is unchanged and the
        // original interval-8 stop is still present.
        assert_eq!(10 as size::CellCountInt, t.cols);
        assert!(t.tabstops.get(8));

        t.deinit(alloc);
    });
}

// test "Terminal: resize failure paths preserve consistent state"
//
// NEGATIVE CONTROL (P3-W4 T3 Step 3): one arm per fail point, each asserting
// a DIFFERENT piece of rolled-back state, because a single "resize failed
// and nothing changed" test cannot say which rollback arm regressed:
//   - `Tabstops`      trips BEFORE the tabstop table is replaced.
//   - `PrimaryScreen` trips after the tabstops were built (they must be
//                     freed) but before any screen mutated.
//   - `AlternateScreen` is the RECOVERY arm, not a restoration arm: the
//                     primary already resized, so the resize SUCCEEDS and
//                     the inactive alternate is dropped instead.
//
// WS18 release lane (CORRECTED — fix round 1 of parity S166-F2): the sentence
// that used to stand here, "`tripwire::Module::enabled()` is
// `cfg!(debug_assertions)`", is FALSE and had this test `#[ignore]`d in
// release for a reason that did not exist. `enabled()` is
// `cfg!(any(test, debug_assertions, feature = "tripwire-test"))`, and
// rustty-vt turns `rustty-util/tripwire-test` on as a dev-dependency feature
// precisely so this crate's fail points trip in the RELEASE test lane too —
// which is what Zig's `builtin.is_test` does at every optimize mode. Verified:
// this test RUNS and PASSES under `cargo test -p rustty-vt --release`.
#[test]
fn terminal_resize_failure_paths_preserve_consistent_state() {
    let alloc = Global;

    for tag in [
        ResizeFp::Tabstops,
        ResizeFp::PrimaryScreen,
        ResizeFp::AlternateScreen,
    ] {
        let mut t = Terminal::init(alloc, opts(10, 3)).unwrap();

        t.print_string(b"primary").unwrap();
        let _ = t.switch_screen(crate::screen_set::Key::Alternate).unwrap();
        t.print_string(b"alternate").unwrap();
        let _ = t.switch_screen(crate::screen_set::Key::Primary).unwrap();

        t.width_px = 100;
        t.height_px = 50;
        t.modes.set(modespkg::Mode::SynchronizedOutput, true);
        t.flags.dirty.clear = false;
        t.scrolling_region = ScrollingRegion {
            top: 1,
            bottom: 2,
            left: 1,
            right: 8,
        };
        t.tabstops.unset(8);
        t.tabstops.set(3);

        let primary = t.screens.get(crate::screen_set::Key::Primary).unwrap();
        let alternate = t.screens.get(crate::screen_set::Key::Alternate).unwrap();
        let alternate_generation = t.screens.generation(crate::screen_set::Key::Alternate);
        assert!(std::ptr::eq(
            unsafe { (*primary).pages.pages.first },
            unsafe { (*primary).pages.pages.last }
        ));
        assert!(std::ptr::eq(
            unsafe { (*alternate).pages.pages.first },
            unsafe { (*alternate).pages.pages.last }
        ));

        // PORT NOTE: `const before = t;` is a Zig by-value struct copy; the
        // Rust port snapshots the fields the reference compares.
        //
        // CORRECTION (S143-F1): the note that stood here said the port
        // "snapshots exactly the fields the reference compares". It did
        // not. The reference compares whole values —
        // `expectEqual(before.flags, t.flags)` (every `Flags` field),
        // `std.meta.eql(before.tabstops, t.tabstops)` (the entire tabstop
        // table) and `std.meta.eql(before_*.cursor, *.cursor)` (the entire
        // `Cursor`) — where the port compared `flags.dirty.clear`, the two
        // tabstop bits at columns 8 and 3, and the cursor `(x, y)` pair. A
        // rollback that cleared another `Flags` field, disturbed another
        // tabstop or altered `cursor.pending_wrap` passed here and failed
        // upstream.
        let before_width_px = t.width_px;
        let before_height_px = t.height_px;
        let before_modes = (t.modes.values, t.modes.saved, t.modes.default);
        let before_cols = t.cols;
        let before_rows = t.rows;
        let before_scrolling_region = (
            t.scrolling_region.top,
            t.scrolling_region.bottom,
            t.scrolling_region.left,
            t.scrolling_region.right,
        );
        let before_flags = t.flags;
        let before_tabstops = t.tabstops.meta_eql_snapshot();
        let before_active_key = t.screens.active_key;
        let before_active = t.screens.active;
        let (before_primary_cols, before_primary_rows, before_primary_total_rows) = unsafe {
            (
                (*primary).pages.cols,
                (*primary).pages.rows,
                (*primary).pages.total_rows,
            )
        };
        let before_primary_cursor = unsafe { (*primary).cursor };
        let before_primary_page: Vec<u8> = unsafe {
            let page: *const Page = crate::pagelist::Node::page((*primary).pages.pages.first);
            core::slice::from_raw_parts((*page).memory_ptr(), (*page).memory_len()).to_vec()
        };
        let (before_alt_cols, before_alt_rows, before_alt_total_rows) = unsafe {
            (
                (*alternate).pages.cols,
                (*alternate).pages.rows,
                (*alternate).pages.total_rows,
            )
        };
        let before_alternate_cursor = unsafe { (*alternate).cursor };
        let before_alternate_page: Vec<u8> = unsafe {
            let page: *const Page = crate::pagelist::Node::page((*alternate).pages.pages.first);
            core::slice::from_raw_parts((*page).memory_ptr(), (*page).memory_len()).to_vec()
        };

        resize_tw_error_always(tag);

        // A failure after the primary screen has resized is recovered by
        // dropping the inactive alternate and completing the resize. This
        // also proves the alternate tripwire is after the primary resize
        // rather than acting as a preflight check.
        if tag == ResizeFp::AlternateScreen {
            t.resize(
                alloc,
                Resize {
                    cols: 513,
                    rows: 4,
                    cell_size_px: Some(ResizeCellSize {
                        width: 9,
                        height: 18,
                    }),
                },
            )
            .unwrap();

            assert_eq!(513 as size::CellCountInt, t.cols);
            assert_eq!(4 as size::CellCountInt, t.rows);
            assert_eq!(4617_u32, t.width_px);
            assert_eq!(72_u32, t.height_px);
            assert!(!t.modes.get(modespkg::Mode::SynchronizedOutput));
            assert!(t.flags.dirty.clear);
            assert_eq!(513 as size::CellCountInt, unsafe { (*primary).pages.cols });
            assert_eq!(4 as size::CellCountInt, unsafe { (*primary).pages.rows });
            assert!(t.screens.get(crate::screen_set::Key::Alternate).is_none());
            assert_eq!(
                alternate_generation.wrapping_add(1),
                t.screens.generation(crate::screen_set::Key::Alternate)
            );
            assert_eq!(crate::screen_set::Key::Primary, t.screens.active_key);
            assert!(std::ptr::eq(primary, t.screens.active));
            assert!(t.tabstops.get(8));
            assert!(!t.tabstops.get(3));

            // The alternate is recreated lazily at the terminal's new size.
            let _ = t.switch_screen(crate::screen_set::Key::Alternate).unwrap();
            let replacement = t.screens.get(crate::screen_set::Key::Alternate).unwrap();
            assert_eq!(513 as size::CellCountInt, unsafe {
                (*replacement).pages.cols
            });
            assert_eq!(4 as size::CellCountInt, unsafe {
                (*replacement).pages.rows
            });
            assert!(unsafe {
                (*(*replacement)
                    .pages
                    .get_cell(point::Point::Active(point::Coordinate::default()))
                    .unwrap()
                    .cell)
                    .is_empty()
            });

            resize_tw_end(rustty_util::tripwire::ResetMode::Reset).unwrap();
            t.deinit(alloc);
            continue;
        }

        assert_eq!(
            Err(ResizeError::OutOfMemory),
            t.resize(
                alloc,
                Resize {
                    cols: 513,
                    rows: 4,
                    cell_size_px: Some(ResizeCellSize {
                        width: 9,
                        height: 18
                    }),
                }
            )
        );

        assert_eq!(before_width_px, t.width_px);
        assert_eq!(before_height_px, t.height_px);
        assert_eq!(
            before_modes,
            (t.modes.values, t.modes.saved, t.modes.default)
        );
        assert_eq!(before_cols, t.cols);
        assert_eq!(before_rows, t.rows);
        assert_eq!(
            before_scrolling_region,
            (
                t.scrolling_region.top,
                t.scrolling_region.bottom,
                t.scrolling_region.left,
                t.scrolling_region.right,
            )
        );
        // try testing.expectEqual(before.flags, t.flags);
        assert_eq!(before_flags, t.flags);
        // try testing.expect(std.meta.eql(before.tabstops, t.tabstops));
        assert_eq!(before_tabstops, t.tabstops.meta_eql_snapshot());
        assert_eq!(before_active_key, t.screens.active_key);
        assert!(std::ptr::eq(before_active, t.screens.active));

        unsafe {
            assert_eq!(before_primary_cols, (*primary).pages.cols);
            assert_eq!(before_primary_rows, (*primary).pages.rows);
            assert_eq!(before_primary_total_rows, (*primary).pages.total_rows);
            // try testing.expect(std.meta.eql(before_primary.cursor, primary.cursor));
            assert!(
                before_primary_cursor.meta_eql(&(*primary).cursor),
                "primary cursor changed after a failed resize"
            );
            let page: *const Page = crate::pagelist::Node::page((*primary).pages.pages.first);
            let after = core::slice::from_raw_parts((*page).memory_ptr(), (*page).memory_len());
            assert!(
                before_primary_page == after,
                "primary page memory changed after a failed resize"
            );

            assert_eq!(before_alt_cols, (*alternate).pages.cols);
            assert_eq!(before_alt_rows, (*alternate).pages.rows);
            assert_eq!(before_alt_total_rows, (*alternate).pages.total_rows);
            // try testing.expect(std.meta.eql(before_alternate.cursor, alternate.cursor));
            assert!(
                before_alternate_cursor.meta_eql(&(*alternate).cursor),
                "alternate cursor changed after a failed resize"
            );
            let page: *const Page = crate::pagelist::Node::page((*alternate).pages.pages.first);
            let after = core::slice::from_raw_parts((*page).memory_ptr(), (*page).memory_len());
            assert!(
                before_alternate_page == after,
                "alternate page memory changed after a failed resize"
            );
        }

        resize_tw_end(rustty_util::tripwire::ResetMode::Reset).unwrap();
        t.deinit(alloc);
    }
}

// test "Terminal: alternate resize failure replaces active alternate screen"
//
// WS18 release lane: tripwire fail point; see the PORT NOTE above.
#[test]
fn terminal_alternate_resize_failure_replaces_active_alternate_screen() {
    let alloc = Global;
    let mut t = Terminal::init(alloc, opts(10, 3)).unwrap();

    let _ = t.switch_screen(crate::screen_set::Key::Alternate).unwrap();
    assert_eq!(crate::screen_set::Key::Alternate, t.screens.active_key);
    unsafe { (*t.screens.active).charset.gl = charsets::Slots::G1 };
    t.print_string(b"alternate").unwrap();
    let generation = t.screens.generation(crate::screen_set::Key::Alternate);

    resize_tw_error_always(ResizeFp::AlternateScreen);
    // S142-F1/S143-F2: `log.warn("alternate screen resize failed,
    // replacing it err={}", .{err})` (Terminal.zig:4110).
    let cap = crate::log_capture::start();
    t.resize(
        alloc,
        Resize {
            cols: 20,
            rows: 4,
            cell_size_px: None,
        },
    )
    .unwrap();
    let messages = cap.messages();
    assert!(
        messages
            .iter()
            .any(|(level, target, msg)| *level == log::Level::Warn
                && target == "terminal"
                && msg.starts_with("alternate screen resize failed, replacing it err=")),
        "expected the alternate-screen-resize-failed diagnostic, got: {messages:?}"
    );
    drop(cap);
    resize_tw_end(rustty_util::tripwire::ResetMode::Reset).unwrap();

    let alternate = t.screens.get(crate::screen_set::Key::Alternate).unwrap();
    assert_eq!(crate::screen_set::Key::Alternate, t.screens.active_key);
    assert!(std::ptr::eq(alternate, t.screens.active));
    assert_eq!(20 as size::CellCountInt, unsafe { (*alternate).pages.cols });
    assert_eq!(4 as size::CellCountInt, unsafe { (*alternate).pages.rows });
    assert!(unsafe {
        (*(*alternate)
            .pages
            .get_cell(point::Point::Active(point::Coordinate::default()))
            .unwrap()
            .cell)
            .is_empty()
    });
    assert_eq!(charsets::Slots::G1, unsafe { (*alternate).charset.gl });
    assert_eq!(
        generation.wrapping_add(1),
        t.screens.generation(crate::screen_set::Key::Alternate)
    );

    t.deinit(alloc);
}

// test "Terminal: alternate resize replacement failure falls back to primary"
//
// WS18 release lane: tripwire fail point; see the PORT NOTE above.
#[test]
fn terminal_alternate_resize_replacement_failure_falls_back_to_primary() {
    let alloc = Global;
    let mut t = Terminal::init(alloc, opts(10, 3)).unwrap();

    let _ = t.switch_screen(crate::screen_set::Key::Alternate).unwrap();
    resize_tw_error_always(ResizeFp::AlternateScreen);
    // NOT `ResizeFp::AlternateScreenInit`: that tripwire matches the
    // reference's OWN silent `tw.check(.alternate_screen_init) catch
    // break :alt;` guard (Terminal.zig:4131, no log call at all) — using
    // it would short-circuit BEFORE the real `ScreenSet::get_init` call
    // this test needs to fail. Force `PageList::init` itself (which
    // `Screen::init` propagates through `?`) to fail instead, via the
    // pagelist crate's own init tripwire.
    crate::pagelist::INIT_TW.with(|tw| {
        tw.borrow_mut()
            .errorAlways(crate::pagelist::InitFp::InitMemoryPool, OutOfMemory)
    });
    // S142-F1/S143-F2: `log.warn("alternate screen replacement failed,
    // falling back to primary err={}", .{init_err})` (Terminal.zig:4149-4152).
    let cap = crate::log_capture::start();
    t.resize(
        alloc,
        Resize {
            cols: 20,
            rows: 4,
            cell_size_px: None,
        },
    )
    .unwrap();
    let messages = cap.messages();
    assert!(
        messages
            .iter()
            .any(|(level, target, msg)| *level == log::Level::Warn
                && target == "terminal"
                && msg.starts_with(
                    "alternate screen replacement failed, falling back to primary err="
                )),
        "expected the alternate-screen-replacement-failed diagnostic, got: {messages:?}"
    );
    drop(cap);
    crate::pagelist::INIT_TW
        .with(|tw| tw.borrow_mut().end(rustty_util::tripwire::ResetMode::Reset))
        .unwrap();
    resize_tw_end(rustty_util::tripwire::ResetMode::Reset).unwrap();

    let primary = t.screens.get(crate::screen_set::Key::Primary).unwrap();
    assert!(t.screens.get(crate::screen_set::Key::Alternate).is_none());
    assert_eq!(crate::screen_set::Key::Primary, t.screens.active_key);
    assert!(std::ptr::eq(primary, t.screens.active));
    assert_eq!(20 as size::CellCountInt, unsafe { (*primary).pages.cols });
    assert_eq!(4 as size::CellCountInt, unsafe { (*primary).pages.rows });

    t.deinit(alloc);
}

// https://github.com/mitchellh/ghostty/issues/723
// This was found via fuzzing so its highly specific.
// test "Terminal: resize with left and right margin set"
#[test]
fn terminal_resize_with_left_and_right_margin_set() {
    let alloc = Global;
    let cols: size::CellCountInt = 70;
    let rows: size::CellCountInt = 23;
    let mut t = Terminal::init(alloc, opts(cols, rows)).unwrap();

    t.modes.set(modespkg::Mode::EnableLeftAndRightMargin, true);
    t.print('0' as u32).unwrap();
    t.modes.set(modespkg::Mode::EnableMode3, true);
    t.resize(
        alloc,
        Resize {
            cols,
            rows,
            cell_size_px: None,
        },
    )
    .unwrap();
    t.set_left_and_right_margin(2, 0);
    t.print_repeat(1850).unwrap();
    let _ = t.modes.restore(modespkg::Mode::EnableMode3);
    t.resize(
        alloc,
        Resize {
            cols,
            rows,
            cell_size_px: None,
        },
    )
    .unwrap();

    t.deinit(alloc);
}

// test "Terminal: resize without scrollback pull"
#[test]
fn terminal_resize_without_scrollback_pull() {
    let alloc = Global;
    let mut t = Terminal::init(alloc, opts(5, 3)).unwrap();
    // defer t.deinit(alloc);
    t.flags.resize_pull_scrollback = false;

    // This is configuration so it should survive a reset.
    t.full_reset();
    assert!(!t.flags.resize_pull_scrollback);

    t.print_string(b"1\n2\n3\n4\n5").unwrap();
    t.resize(
        alloc,
        Resize {
            cols: 5,
            rows: 5,
            cell_size_px: None,
        },
    )
    .unwrap();
    assert_eq!(2 as size::CellCountInt, unsafe {
        (*t.screens.active).cursor.y
    });
    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "3\n4\n5".as_bytes());
    }

    t.deinit(alloc);
}

// https://github.com/mitchellh/ghostty/issues/1343
// test "Terminal: resize with wraparound off"
#[test]
fn terminal_resize_with_wraparound_off() {
    let alloc = Global;
    let cols: size::CellCountInt = 4;
    let rows: size::CellCountInt = 2;
    let mut t = Terminal::init(alloc, opts(cols, rows)).unwrap();

    t.modes.set(modespkg::Mode::Wraparound, false);
    t.print('0' as u32).unwrap();
    t.print('1' as u32).unwrap();
    t.print('2' as u32).unwrap();
    t.print('3' as u32).unwrap();
    let new_cols: size::CellCountInt = 2;
    t.resize(
        alloc,
        Resize {
            cols: new_cols,
            rows,
            cell_size_px: None,
        },
    )
    .unwrap();

    let str = t.plain_string(Global).unwrap();
    assert_eq!(str.as_slice(), "01".as_bytes());

    t.deinit(alloc);
}

// test "Terminal: resize with wraparound on"
#[test]
fn terminal_resize_with_wraparound_on() {
    let alloc = Global;
    let cols: size::CellCountInt = 4;
    let rows: size::CellCountInt = 2;
    let mut t = Terminal::init(alloc, opts(cols, rows)).unwrap();

    t.modes.set(modespkg::Mode::Wraparound, true);
    t.print('0' as u32).unwrap();
    t.print('1' as u32).unwrap();
    t.print('2' as u32).unwrap();
    t.print('3' as u32).unwrap();
    let new_cols: size::CellCountInt = 2;
    t.resize(
        alloc,
        Resize {
            cols: new_cols,
            rows,
            cell_size_px: None,
        },
    )
    .unwrap();

    let str = t.plain_string(Global).unwrap();
    assert_eq!(str.as_slice(), "01\n23".as_bytes());

    t.deinit(alloc);
}

// test "Terminal: resize with high unique style per cell"
#[test]
fn terminal_resize_with_high_unique_style_per_cell() {
    let alloc = Global;
    let mut t = Terminal::init(alloc, opts(30, 30)).unwrap();

    for y in 0..t.rows {
        for x in 0..t.cols {
            t.set_cursor_pos(y as usize, x as usize);
            t.set_attribute(sgr::Attribute::DirectColorBg(color::RGB {
                r: x as u8,
                g: y as u8,
                b: 0,
            }))
            .unwrap();
            t.print('x' as u32).unwrap();
        }
    }

    t.resize(
        alloc,
        Resize {
            cols: 60,
            rows: 30,
            cell_size_px: None,
        },
    )
    .unwrap();

    t.deinit(alloc);
}

// test "Terminal: resize with high unique style per cell with wrapping"
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
)]
#[test]
fn terminal_resize_with_high_unique_style_per_cell_with_wrapping() {
    let alloc = Global;
    let mut t = Terminal::init(alloc, opts(30, 30)).unwrap();

    let cell_count: u16 = (t.rows as u32 * t.cols as u32) as u16;
    for i in 0..cell_count {
        let r: u8 = (i >> 8) as u8;
        let g: u8 = (i & 0xFF) as u8;

        t.set_attribute(sgr::Attribute::DirectColorBg(color::RGB { r, g, b: 0 }))
            .unwrap();
        t.print('x' as u32).unwrap();
    }

    t.resize(
        alloc,
        Resize {
            cols: 60,
            rows: 30,
            cell_size_px: None,
        },
    )
    .unwrap();

    t.deinit(alloc);
}

// test "Terminal: resize with reflow and saved cursor"
#[test]
fn terminal_resize_with_reflow_and_saved_cursor() {
    let alloc = Global;
    let mut t = Terminal::init(alloc, opts(2, 3)).unwrap();
    t.print_string("1A2B".as_bytes()).unwrap();
    t.set_cursor_pos(2, 2);
    {
        let cursor_x = unsafe { (*t.screens.active).cursor.x };
        let cursor_y = unsafe { (*t.screens.active).cursor.y };
        let list_cell = t
            .active_mut()
            .pages
            .get_cell(active_pt(cursor_x, cursor_y as u32))
            .unwrap();
        let cell = list_cell.cell;
        assert_eq!('B' as u32, unsafe { (*cell).content_codepoint() } as u32);
    }

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "1A\n2B".as_bytes());
    }

    t.save_cursor();
    t.resize(
        alloc,
        Resize {
            cols: 5,
            rows: 3,
            cell_size_px: None,
        },
    )
    .unwrap();
    t.restore_cursor();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "1A2B".as_bytes());
    }

    // Verify our cursor is still in the same place
    {
        let cursor_x = unsafe { (*t.screens.active).cursor.x };
        let cursor_y = unsafe { (*t.screens.active).cursor.y };
        let list_cell = t
            .active_mut()
            .pages
            .get_cell(active_pt(cursor_x, cursor_y as u32))
            .unwrap();
        let cell = list_cell.cell;
        assert_eq!('B' as u32, unsafe { (*cell).content_codepoint() } as u32);
    }

    t.deinit(alloc);
}

// test "Terminal: resize with reflow and saved cursor pending wrap"
#[test]
fn terminal_resize_with_reflow_and_saved_cursor_pending_wrap() {
    let alloc = Global;
    let mut t = Terminal::init(alloc, opts(2, 3)).unwrap();
    t.print_string("1A2B".as_bytes()).unwrap();
    {
        let cursor_x = unsafe { (*t.screens.active).cursor.x };
        let cursor_y = unsafe { (*t.screens.active).cursor.y };
        let list_cell = t
            .active_mut()
            .pages
            .get_cell(active_pt(cursor_x, cursor_y as u32))
            .unwrap();
        let cell = list_cell.cell;
        assert_eq!('B' as u32, unsafe { (*cell).content_codepoint() } as u32);
    }

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "1A\n2B".as_bytes());
    }

    t.save_cursor();
    t.resize(
        alloc,
        Resize {
            cols: 5,
            rows: 3,
            cell_size_px: None,
        },
    )
    .unwrap();
    t.restore_cursor();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "1A2B".as_bytes());
    }

    // Pending wrap should be reset
    t.print('X' as u32).unwrap();
    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "1A2BX".as_bytes());
    }

    t.deinit(alloc);
}

// test "Terminal: saved cursor survives repeated widening"
#[test]
fn terminal_saved_cursor_survives_repeated_widening() {
    let alloc = Global;
    let mut t = Terminal::init(alloc, opts(4, 5)).unwrap();

    t.print_string("abc\nAAA|".as_bytes()).unwrap();
    t.save_cursor();
    for cols in [5, 6] {
        t.resize(
            alloc,
            Resize {
                cols,
                rows: 5,
                cell_size_px: None,
            },
        )
        .unwrap();
    }
    t.restore_cursor();
    t.print('X' as u32).unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "abc\nAAA|X".as_bytes());
    }

    t.deinit(alloc);
}

// test "Terminal: resize pending wrap live and saved cursors"
#[test]
fn terminal_resize_pending_wrap_live_and_saved_cursors() {
    let alloc = Global;
    struct Case {
        text: &'static str,
        cols: u16,
        pending_wrap: bool,
        expected: &'static str,
    }
    let cases = [
        // Widening leaves room after the formerly full line.
        Case {
            text: "ABCD",
            cols: 6,
            pending_wrap: false,
            expected: "ABCDX",
        },
        // Narrowing can move the last character into the middle of a row.
        Case {
            text: "ABCD",
            cols: 3,
            pending_wrap: false,
            expected: "ABC\nDX",
        },
        // Keep pending wrap when the last character still fills a row.
        Case {
            text: "ABCD",
            cols: 2,
            pending_wrap: true,
            expected: "AB\nCD\nX",
        },
        // A height-only resize also preserves pending wrap.
        Case {
            text: "ABCD",
            cols: 4,
            pending_wrap: true,
            expected: "ABCD\nX",
        },
        // Reflow can merge previously wrapped rows.
        Case {
            text: "ABCDEFGH",
            cols: 6,
            pending_wrap: false,
            expected: "ABCDEF\nGHX",
        },
        // A wide character at the old right edge must not be overwritten.
        Case {
            text: "AB界",
            cols: 6,
            pending_wrap: false,
            expected: "AB界X",
        },
        // A cursor without pending wrap must not advance an extra cell.
        Case {
            text: "ABC",
            cols: 6,
            pending_wrap: false,
            expected: "ABCX",
        },
    ];

    for case in cases {
        for restore in [false, true] {
            let mut t = Terminal::init(alloc, opts(4, 5)).unwrap();
            t.print_string(case.text.as_bytes()).unwrap();
            if restore {
                t.save_cursor();
            }

            t.resize(
                alloc,
                Resize {
                    cols: case.cols as _,
                    rows: 6,
                    cell_size_px: None,
                },
            )
            .unwrap();
            if restore {
                t.restore_cursor();
            }
            assert_eq!(case.pending_wrap, unsafe {
                (*t.screens.active).cursor.pending_wrap
            });

            t.print('X' as u32).unwrap();
            {
                let str = t.plain_string(Global).unwrap();
                assert_eq!(str.as_slice(), case.expected.as_bytes());
            }

            t.deinit(alloc);
        }
    }
}

// test "Terminal: DECCOLM without DEC mode 40"
#[test]
fn terminal_deccolm_without_dec_mode_40() {
    let alloc = Global;
    let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();

    t.modes.set(modespkg::Mode::Column132, true);
    t.deccolm(alloc, DeccolmMode::Cols132).unwrap();
    assert_eq!(5_usize, t.cols as usize);
    assert_eq!(5_usize, t.rows as usize);
    assert!(!t.modes.get(modespkg::Mode::Column132));

    t.deinit(alloc);
}

// test "Terminal: DECCOLM unset"
#[test]
fn terminal_deccolm_unset() {
    let alloc = Global;
    let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();

    t.modes.set(modespkg::Mode::EnableMode3, true);
    t.deccolm(alloc, DeccolmMode::Cols80).unwrap();
    assert_eq!(80_usize, t.cols as usize);
    assert_eq!(5_usize, t.rows as usize);

    t.deinit(alloc);
}

// test "Terminal: DECCOLM resets pending wrap"
#[test]
fn terminal_deccolm_resets_pending_wrap() {
    let alloc = Global;
    let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();

    for c in "ABCDE".chars() {
        t.print(c as u32).unwrap();
    }
    assert!(unsafe { (*t.screens.active).cursor.pending_wrap });

    t.modes.set(modespkg::Mode::EnableMode3, true);
    t.deccolm(alloc, DeccolmMode::Cols80).unwrap();
    assert_eq!(80_usize, t.cols as usize);
    assert_eq!(5_usize, t.rows as usize);
    assert!(!unsafe { (*t.screens.active).cursor.pending_wrap });

    t.deinit(alloc);
}

// test "Terminal: DECCOLM preserves SGR bg"
#[test]
fn terminal_deccolm_preserves_sgr_bg() {
    let alloc = Global;
    let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();

    t.set_attribute(sgr::Attribute::DirectColorBg(color::RGB {
        r: 0xFF,
        g: 0,
        b: 0,
    }))
    .unwrap();
    t.modes.set(modespkg::Mode::EnableMode3, true);
    t.deccolm(alloc, DeccolmMode::Cols80).unwrap();

    {
        let list_cell = t.active_mut().pages.get_cell(active_pt(0, 0)).unwrap();
        assert!(unsafe { (*list_cell.cell).content_tag() } == pagepkg::ContentTag::BgColorRgb);
        assert_eq!(
            pagepkg::CellRgb {
                r: 0xFF,
                g: 0,
                b: 0,
            },
            unsafe { (*list_cell.cell).content_color_rgb() }
        );
    }

    t.deinit(alloc);
}

// test "Terminal: DECCOLM resets scroll region"
#[test]
fn terminal_deccolm_resets_scroll_region() {
    let alloc = Global;
    let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();

    t.modes.set(modespkg::Mode::EnableLeftAndRightMargin, true);
    t.set_top_and_bottom_margin(2, 3);
    t.set_left_and_right_margin(3, 5);

    t.modes.set(modespkg::Mode::EnableMode3, true);
    t.deccolm(alloc, DeccolmMode::Cols80).unwrap();

    assert!(t.modes.get(modespkg::Mode::EnableLeftAndRightMargin));
    assert_eq!(0_usize, t.scrolling_region.top as usize);
    assert_eq!(4_usize, t.scrolling_region.bottom as usize);
    assert_eq!(0_usize, t.scrolling_region.left as usize);
    assert_eq!(79_usize, t.scrolling_region.right as usize);

    t.deinit(alloc);
}

// test "Terminal: mode 47 alt screen plain"
#[test]
fn terminal_mode_47_alt_screen_plain() {
    let alloc = Global;
    let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();

    // Print on primary screen
    t.print_string("1A".as_bytes()).unwrap();

    // Go to alt screen with mode 47
    t.switch_screen_mode(SwitchScreenMode::M47, true).unwrap();
    assert_eq!(crate::screen_set::Key::Alternate, t.screens.active_key);

    // Screen should be empty
    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "".as_bytes());
    }

    // Print on alt screen. This should be off center because
    // we copy the cursor over from the primary screen
    t.print_string("2B".as_bytes()).unwrap();
    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "  2B".as_bytes());
    }

    // Go back to primary
    t.switch_screen_mode(SwitchScreenMode::M47, false).unwrap();
    assert_eq!(crate::screen_set::Key::Primary, t.screens.active_key);

    // Primary screen should still have the original content
    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "1A".as_bytes());
    }

    // Go back to alt screen with mode 47
    t.switch_screen_mode(SwitchScreenMode::M47, true).unwrap();
    assert_eq!(crate::screen_set::Key::Alternate, t.screens.active_key);

    // Screen should retain content
    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "  2B".as_bytes());
    }

    t.deinit(alloc);
}

// test "Terminal: mode 47 copies cursor both directions"
#[test]
fn terminal_mode_47_copies_cursor_both_directions() {
    let alloc = Global;
    let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();

    // Color our cursor red
    t.set_attribute(sgr::Attribute::DirectColorFg(color::RGB {
        r: 0xFF,
        g: 0,
        b: 0x7F,
    }))
    .unwrap();

    // Go to alt screen with mode 47
    t.switch_screen_mode(SwitchScreenMode::M47, true).unwrap();
    assert_eq!(crate::screen_set::Key::Alternate, t.screens.active_key);

    // Verify that our style is set
    {
        assert!(unsafe { (*t.screens.active).cursor.style_id } != style::DEFAULT_ID);
        let page =
            unsafe { crate::pagelist::Node::page((*(*t.screens.active).cursor.page_pin).node) };
        assert_eq!(1_usize, unsafe { (*page).styles.count() });
        assert!(
            unsafe {
                (*page)
                    .styles
                    .ref_count((*page).memory_ptr(), (*t.screens.active).cursor.style_id)
            } > 0
        );
    }

    // Set a new style
    t.set_attribute(sgr::Attribute::DirectColorFg(color::RGB {
        r: 0,
        g: 0xFF,
        b: 0,
    }))
    .unwrap();

    // Go back to primary
    t.switch_screen_mode(SwitchScreenMode::M47, false).unwrap();
    assert_eq!(crate::screen_set::Key::Primary, t.screens.active_key);

    // Verify that our style is still set
    {
        assert!(unsafe { (*t.screens.active).cursor.style_id } != style::DEFAULT_ID);
        let page =
            unsafe { crate::pagelist::Node::page((*(*t.screens.active).cursor.page_pin).node) };
        assert_eq!(1_usize, unsafe { (*page).styles.count() });
        assert!(
            unsafe {
                (*page)
                    .styles
                    .ref_count((*page).memory_ptr(), (*t.screens.active).cursor.style_id)
            } > 0
        );
    }

    t.deinit(alloc);
}

// test "Terminal: mode 1047 alt screen plain"
#[test]
fn terminal_mode_1047_alt_screen_plain() {
    let alloc = Global;
    let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();

    // Print on primary screen
    t.print_string("1A".as_bytes()).unwrap();

    // Go to alt screen with mode 47
    t.switch_screen_mode(SwitchScreenMode::M1047, true).unwrap();
    assert_eq!(crate::screen_set::Key::Alternate, t.screens.active_key);

    // Screen should be empty
    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "".as_bytes());
    }

    // Print on alt screen. This should be off center because
    // we copy the cursor over from the primary screen
    t.print_string("2B".as_bytes()).unwrap();
    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "  2B".as_bytes());
    }

    // Go back to primary
    t.switch_screen_mode(SwitchScreenMode::M1047, false)
        .unwrap();
    assert_eq!(crate::screen_set::Key::Primary, t.screens.active_key);

    // Primary screen should still have the original content
    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "1A".as_bytes());
    }

    // Go back to alt screen with mode 1047
    t.switch_screen_mode(SwitchScreenMode::M1047, true).unwrap();
    assert_eq!(crate::screen_set::Key::Alternate, t.screens.active_key);

    // Screen should be empty
    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "".as_bytes());
    }

    t.deinit(alloc);
}

// test "Terminal: mode 1047 copies cursor both directions"
#[test]
fn terminal_mode_1047_copies_cursor_both_directions() {
    let alloc = Global;
    let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();

    // Color our cursor red
    t.set_attribute(sgr::Attribute::DirectColorFg(color::RGB {
        r: 0xFF,
        g: 0,
        b: 0x7F,
    }))
    .unwrap();

    // Go to alt screen with mode 47
    t.switch_screen_mode(SwitchScreenMode::M1047, true).unwrap();
    assert_eq!(crate::screen_set::Key::Alternate, t.screens.active_key);

    // Verify that our style is set
    {
        assert!(unsafe { (*t.screens.active).cursor.style_id } != style::DEFAULT_ID);
        let page =
            unsafe { crate::pagelist::Node::page((*(*t.screens.active).cursor.page_pin).node) };
        assert_eq!(1_usize, unsafe { (*page).styles.count() });
        assert!(
            unsafe {
                (*page)
                    .styles
                    .ref_count((*page).memory_ptr(), (*t.screens.active).cursor.style_id)
            } > 0
        );
    }

    // Set a new style
    t.set_attribute(sgr::Attribute::DirectColorFg(color::RGB {
        r: 0,
        g: 0xFF,
        b: 0,
    }))
    .unwrap();

    // Go back to primary
    t.switch_screen_mode(SwitchScreenMode::M1047, false)
        .unwrap();
    assert_eq!(crate::screen_set::Key::Primary, t.screens.active_key);

    // Verify that our style is still set
    {
        assert!(unsafe { (*t.screens.active).cursor.style_id } != style::DEFAULT_ID);
        let page =
            unsafe { crate::pagelist::Node::page((*(*t.screens.active).cursor.page_pin).node) };
        assert_eq!(1_usize, unsafe { (*page).styles.count() });
        assert!(
            unsafe {
                (*page)
                    .styles
                    .ref_count((*page).memory_ptr(), (*t.screens.active).cursor.style_id)
            } > 0
        );
    }

    t.deinit(alloc);
}

// test "Terminal: mode 1049 alt screen plain"
#[test]
fn terminal_mode_1049_alt_screen_plain() {
    let alloc = Global;
    let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();

    // Print on primary screen
    t.print_string("1A".as_bytes()).unwrap();

    // Go to alt screen with mode 47
    t.switch_screen_mode(SwitchScreenMode::M1049, true).unwrap();
    assert_eq!(crate::screen_set::Key::Alternate, t.screens.active_key);

    // Screen should be empty
    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "".as_bytes());
    }

    // Print on alt screen. This should be off center because
    // we copy the cursor over from the primary screen
    t.print_string("2B".as_bytes()).unwrap();
    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "  2B".as_bytes());
    }

    // Go back to primary
    t.switch_screen_mode(SwitchScreenMode::M1049, false)
        .unwrap();
    assert_eq!(crate::screen_set::Key::Primary, t.screens.active_key);

    // Primary screen should still have the original content
    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "1A".as_bytes());
    }

    // Write, our cursor should be restored back.
    t.print_string("C".as_bytes()).unwrap();
    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "1AC".as_bytes());
    }

    // Go back to alt screen with mode 1049
    t.switch_screen_mode(SwitchScreenMode::M1049, true).unwrap();
    assert_eq!(crate::screen_set::Key::Alternate, t.screens.active_key);

    // Screen should be empty
    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "".as_bytes());
    }

    t.deinit(alloc);
}
