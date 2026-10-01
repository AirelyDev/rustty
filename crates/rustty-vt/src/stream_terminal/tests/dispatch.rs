//! stream_terminal tests: DECRQSS/XTGETTCAP, DCS, cursor/erase/mode
//! dispatch, OSC and Kitty color operations, semantic prompts and the bell.

use super::*;

// test "DECRQSS responses"
#[test]
fn decrqss_responses() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    calls_reset();

    {
        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(count_write_pty);

        // SGR
        s.next_slice(b"\x1B[1m\x1BP$qm\x1B\\");
        assert_eq!(1_usize, calls_get());
        assert_eq!(
            b"\x1BP1$r0;1m\x1B\\".as_slice(),
            cap_take().unwrap().as_slice()
        );
        calls_reset();

        // Overline
        s.next_slice(b"\x1B[0;53m\x1BP$qm\x1B\\");
        assert_eq!(1_usize, calls_get());
        assert_eq!(
            b"\x1BP1$r0;53m\x1B\\".as_slice(),
            cap_take().unwrap().as_slice()
        );
        calls_reset();

        // Requests larger than the parser's fixed request buffer are
        // ignored, and the next DCS command must still be processed
        // normally.
        s.next_slice(b"\x1BP$qfoo\x1B\\");
        assert_eq!(0_usize, calls_get());
        assert!(!s.handler.semantic_failure);
        s.next_slice(b"\x1BP$qm\x1B\\");
        assert_eq!(1_usize, calls_get());
        assert_eq!(
            b"\x1BP1$r0;53m\x1B\\".as_slice(),
            cap_take().unwrap().as_slice()
        );
    }

    t.deinit(Global);
}

// test "DECRQSS without write effect is ignored"
#[test]
fn decrqss_without_write_effect_is_ignored() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"\x1BP$qm\x1B\\");
        assert!(!s.handler.semantic_failure);
    }
    t.deinit(Global);
}

// `std.fmt.bytesToHex(bytes, .upper / .lower)` for the XTGETTCAP tests'
// query construction (the reference builds these at comptime with `++`).
fn test_hex(input: &[u8], upper: bool) -> Vec<u8> {
    let charset: &[u8; 16] = if upper {
        b"0123456789ABCDEF"
    } else {
        b"0123456789abcdef"
    };
    let mut out = Vec::with_capacity(input.len() * 2);
    for &b in input {
        out.push(charset[(b >> 4) as usize]);
        out.push(charset[(b & 0xf) as usize]);
    }
    out
}

// Query/response byte-string concatenation (the reference's comptime `++`).
fn test_concat(parts: &[&[u8]]) -> Vec<u8> {
    let mut out = Vec::new();
    for part in parts {
        out.extend_from_slice(part);
    }
    out
}

// PORT NOTE: the reference XTGETTCAP tests hang `var response/response_len/
// calls` + `writePty`/`expectResponse` on a local `S` struct; the callback
// is a bare `fn` pointer here, so the captures live in thread-locals (the
// module's established `CAPTURE`/`WRITE_CALLS` idiom, shared by both
// XTGETTCAP tests exactly as each reference test's `S` is test-local).
thread_local! {
    static XT_RESPONSE: core::cell::RefCell<Vec<u8>> =
        const { core::cell::RefCell::new(Vec::new()) };
    static XT_CALLS: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
}

// fn reset() void { response_len = 0; calls = 0; }
fn xt_reset() {
    XT_RESPONSE.with(|c| c.borrow_mut().clear());
    XT_CALLS.with(|c| c.set(0));
}

// fn writePty(_: *Handler, data: [:0]const u8) void { ... }
fn xt_write_pty(_: &mut Handler, data: &[u8]) {
    XT_RESPONSE.with(|c| *c.borrow_mut() = data.to_vec());
    XT_CALLS.with(|c| c.set(c.get() + 1));
}

// fn expectResponse(expected: []const u8) !void { ... }
fn xt_expect_response(expected: &[u8]) {
    assert_eq!(1usize, XT_CALLS.with(|c| c.get()));
    assert_eq!(
        expected,
        XT_RESPONSE.with(|c| c.borrow().clone()).as_slice(),
    );
    xt_reset();
}

// test "XTGETTCAP responses"
#[test]
fn xtgettcap_responses() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    xt_reset();

    {
        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(xt_write_pty);

        // The full capability table comes from the static terminfo map; this
        // checks the wiring for a valued and a valueless (boolean) capability.
        s.next_slice(&test_concat(&[
            b"\x1BP+q",
            &test_hex(b"Co", true),
            b"\x1B\\",
        ]));
        xt_expect_response(&test_concat(&[
            b"\x1BP1+r",
            &test_hex(b"Co", true),
            b"=",
            &test_hex(b"256", true),
            b"\x1B\\",
        ]));
        s.next_slice(&test_concat(&[
            b"\x1BP+q",
            &test_hex(b"am", true),
            b"\x1B\\",
        ]));
        xt_expect_response(&test_concat(&[
            b"\x1BP1+r",
            &test_hex(b"am", true),
            b"\x1B\\",
        ]));

        // One response per requested key; lowercase hex is normalized by the
        // DCS parser. The capture holds the last ("Co") reply.
        s.next_slice(&test_concat(&[
            b"\x1BP+q",
            &test_hex(b"am", false),
            b";",
            &test_hex(b"Co", false),
            b"\x1B\\",
        ]));
        assert_eq!(2usize, XT_CALLS.with(|c| c.get()));
        assert_eq!(
            test_concat(&[
                b"\x1BP1+r",
                &test_hex(b"Co", true),
                b"=",
                &test_hex(b"256", true),
                b"\x1B\\",
            ])
            .as_slice(),
            XT_RESPONSE.with(|c| c.borrow().clone()).as_slice(),
        );
        xt_reset();

        // Unknown and malformed keys are skipped without an error.
        s.next_slice(b"\x1BP+qWHO;5;GG\x1B\\");
        assert_eq!(0usize, XT_CALLS.with(|c| c.get()));
        assert!(!s.handler.semantic_failure);
    }

    t.deinit(Global);
}

// test "XTGETTCAP without write effect is ignored"
#[test]
fn xtgettcap_without_write_effect_is_ignored() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s = t.vt_stream();

        s.next_slice(&test_concat(&[
            b"\x1BP+q",
            &test_hex(b"TN", true),
            b";",
            &test_hex(b"am", true),
            b"\x1B\\",
        ]));
        assert!(!s.handler.semantic_failure);
    }
    t.deinit(Global);
}

// test "XTGETTCAP TN responses"
#[test]
fn xtgettcap_tn_responses() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    xt_reset();

    {
        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(xt_write_pty);

        // const tn_query = "\x1BP+q" ++ std.fmt.bytesToHex("TN", .upper) ++ "\x1B\\";
        let tn_query = test_concat(&[b"\x1BP+q", &test_hex(b"TN", true), b"\x1B\\"]);

        // While no name is configured the query goes unanswered.
        s.next_slice(&tn_query);
        assert_eq!(0usize, XT_CALLS.with(|c| c.get()));

        // A configured name is reported hex-encoded.
        s.handler.terminfo_name = Some(b"xterm-256color");
        s.next_slice(&tn_query);
        xt_expect_response(&test_concat(&[
            b"\x1BP1+r",
            &test_hex(b"TN", true),
            b"=",
            &test_hex(b"xterm-256color", true),
            b"\x1B\\",
        ]));

        // A maximum-length name is still reported in full.
        // const max_name = "a" ** Handler.max_terminfo_name_bytes;
        const MAX_NAME: [u8; MAX_TERMINFO_NAME_BYTES] = [b'a'; MAX_TERMINFO_NAME_BYTES];
        s.handler.terminfo_name = Some(&MAX_NAME);
        s.next_slice(&tn_query);
        xt_expect_response(&test_concat(&[
            b"\x1BP1+r",
            &test_hex(b"TN", true),
            b"=",
            &test_hex(&MAX_NAME, true),
            b"\x1B\\",
        ]));

        // An empty name is silent; "Co" is still answered.
        s.handler.terminfo_name = Some(b"");
        s.next_slice(&test_concat(&[
            b"\x1BP+q",
            &test_hex(b"TN", true),
            b";",
            &test_hex(b"Co", true),
            b"\x1B\\",
        ]));
        xt_expect_response(&test_concat(&[
            b"\x1BP1+r",
            &test_hex(b"Co", true),
            b"=",
            &test_hex(b"256", true),
            b"\x1B\\",
        ]));

        // As are names beyond the maximum length.
        // s.handler.terminfo_name = "a" ** (Handler.max_terminfo_name_bytes + 1);
        const OVERLONG_NAME: [u8; MAX_TERMINFO_NAME_BYTES + 1] =
            [b'a'; MAX_TERMINFO_NAME_BYTES + 1];
        s.handler.terminfo_name = Some(&OVERLONG_NAME);
        s.next_slice(&tn_query);
        assert_eq!(0usize, XT_CALLS.with(|c| c.get()));
        assert!(!s.handler.semantic_failure);
    }

    t.deinit(Global);
}

// test "DCS command memory is released"
//
// PORT NOTE: the reference relies on `testing.allocator` to detect a leak.
// rustty has no leak-detecting test allocator here, so on the normal lane
// the leak itself is only caught by Miri / `check_leaks.sh` — which left
// this test with NO assertion of any kind (parity S167-F1). The two
// `test_*` accessors below assert POSITIVELY that both halves the test
// names were reached: the completed command left the handler holding
// nothing, and the incomplete one left it holding bytes that only
// `deinit` can release. Without them a stream that silently dropped the
// sequences on the floor passed this test.
#[test]
fn dcs_command_memory_is_released() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s = t.vt_stream();

        // A completed command transfers its allocation to Command;
        // dcsCommand must release it even when there is no write effect.
        s.next_slice(b"\x1BP+q536D756C78\x1B\\");
        assert!(
            s.handler.dcs_handler.test_is_inactive(),
            "the completed command must leave nothing in the handler"
        );

        // An incomplete command remains owned by the handler and must be
        // released when the stream is deinitialized.
        s.next_slice(b"\x1BP+q536D756C78");
        assert_eq!(
            Some(b"536D756C78".len()),
            s.handler.dcs_handler.test_xtgettcap_buffered(),
            "the incomplete command must still be held by the handler, \
                 otherwise there is nothing for deinit to release"
        );
    }
    t.deinit(Global);
}

// WS3-A KAT (rustty addition, not a reference test): a kitty-graphics PNG
// transmit sequence through the vt Stream decodes via the wuffs DEFAULT
// hook — no embedder installed a decoder — proving the non-lib comptime
// default (sys.zig:18-24) survives the port. `q=2` (quiet always) keeps the
// handler from emitting an OK response to the (test) pty.
#[cfg(feature = "wuffs")]
#[test]
fn kitty_graphics_png_transmit_decodes_via_default_hook() {
    // Base64 of kitty/testdata/image-png-none-50x76-2147483647-raw.data
    // (the `t=d` direct-medium payload is base64 per the kitty protocol).
    const PNG_B64: &str = "iVBORw0KGgoAAAANSUhEUgAAADIAAABMAQMAAAAV2XZDAAAAA1BMVEUrLzc5wwm5AAAADklEQVR4AWOgIRgFowAAAmAAATnED6oAAAAASUVORK5CYII=";

    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        // APC G: a=t (transmit), f=100 (PNG), t=d (direct), i=31, q=2.
        let seq = format!("\x1b_Ga=t,f=100,t=d,i=31,q=2;{PNG_B64}\x1b\\");
        s.next_slice(seq.as_bytes());
    }
    // The graphics_exec.rs:172 storage-access idiom.
    let img = unsafe { &(*t.screens.active).kitty_images }
        .image_by_id(31)
        .expect("transmitted image stored (PNG decoded by the default hook)");
    assert_eq!(50, img.width);
    assert_eq!(76, img.height);
    assert_eq!(img.format, kitty::graphics_command::Format::Rgba);
    t.deinit(Global);
}

// test "cursor movement"
#[test]
fn cursor_movement() {
    let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

    {
        let mut s = t.vt_stream();
        // Move cursor using escape sequences
        s.next_slice(b"Hello\x1B[1;1H");
    }
    assert_eq!(0_usize, t.active_mut().cursor.x as usize);
    assert_eq!(0_usize, t.active_mut().cursor.y as usize);

    {
        let mut s = t.vt_stream();
        // Move to position 2,3
        s.next_slice(b"\x1B[2;3H");
    }
    assert_eq!(2_usize, t.active_mut().cursor.x as usize);
    assert_eq!(1_usize, t.active_mut().cursor.y as usize);

    t.deinit(Global);
}

// test "erase operations"
#[test]
fn erase_operations() {
    let mut t = Terminal::init(Global, opts(20, 10)).unwrap();

    {
        let mut s = t.vt_stream();
        // Print some text
        s.next_slice(b"Hello World");
    }
    assert_eq!(11_usize, t.active_mut().cursor.x as usize);
    assert_eq!(0_usize, t.active_mut().cursor.y as usize);

    {
        let mut s = t.vt_stream();
        // Move cursor to position 1,6 and erase from cursor to end of line
        s.next_slice(b"\x1B[1;6H");
        s.next_slice(b"\x1B[K");
    }

    let str = t.plain_string(Global).unwrap();
    assert_eq!(str.as_slice(), b"Hello");

    t.deinit(Global);
}

// test "tabs"
#[test]
fn tabs() {
    let mut t = Terminal::init(Global, opts(80, 10)).unwrap();

    {
        let mut s = t.vt_stream();
        s.next_slice(b"A\tB");
    }
    assert_eq!(9_usize, t.active_mut().cursor.x as usize);

    let str = t.plain_string(Global).unwrap();
    assert_eq!(str.as_slice(), b"A       B");

    t.deinit(Global);
}

// test "modes"
#[test]
fn modes() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

    // Test wraparound mode
    assert!(t.modes.get(modespkg::Mode::Wraparound));
    {
        let mut s = t.vt_stream();
        s.next_slice(b"\x1B[?7l"); // Disable wraparound
    }
    assert!(!t.modes.get(modespkg::Mode::Wraparound));
    {
        let mut s = t.vt_stream();
        s.next_slice(b"\x1B[?7h"); // Enable wraparound
    }
    assert!(t.modes.get(modespkg::Mode::Wraparound));

    t.deinit(Global);
}

// test "scrolling regions"
#[test]
fn scrolling_regions() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

    {
        let mut s = t.vt_stream();
        // Set scrolling region from line 5 to 20
        s.next_slice(b"\x1B[5;20r");
    }
    assert_eq!(4_usize, t.scrolling_region.top as usize);
    assert_eq!(19_usize, t.scrolling_region.bottom as usize);
    assert_eq!(0_usize, t.scrolling_region.left as usize);
    assert_eq!(79_usize, t.scrolling_region.right as usize);

    t.deinit(Global);
}

// test "charsets"
#[test]
fn charsets() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

    {
        let mut s = t.vt_stream();
        // Configure G0 as DEC special graphics
        s.next_slice(b"\x1B(0");
        s.next_slice(b"`"); // Should print diamond character
    }

    let str = t.plain_string(Global).unwrap();
    assert_eq!(str.as_slice(), "◆".as_bytes());

    t.deinit(Global);
}

// test "alt screen"
#[test]
fn alt_screen() {
    let mut t = Terminal::init(Global, opts(10, 5)).unwrap();

    {
        let mut s = t.vt_stream();
        // Write to primary screen
        s.next_slice(b"Primary");
    }
    assert_eq!(crate::screen_set::Key::Primary, t.screens.active_key);

    {
        let mut s = t.vt_stream();
        // Switch to alt screen
        s.next_slice(b"\x1B[?1049h");
    }
    assert_eq!(crate::screen_set::Key::Alternate, t.screens.active_key);

    {
        let mut s = t.vt_stream();
        // Write to alt screen
        s.next_slice(b"Alt");

        // Switch back to primary
        s.next_slice(b"\x1B[?1049l");
    }
    assert_eq!(crate::screen_set::Key::Primary, t.screens.active_key);

    let str = t.plain_string(Global).unwrap();
    assert_eq!(str.as_slice(), b"Primary");

    t.deinit(Global);
}

// test "cursor save and restore"
#[test]
fn cursor_save_and_restore() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

    {
        let mut s = t.vt_stream();
        // Move cursor to 10,15
        s.next_slice(b"\x1B[10;15H");
    }
    assert_eq!(14_usize, t.active_mut().cursor.x as usize);
    assert_eq!(9_usize, t.active_mut().cursor.y as usize);

    {
        let mut s = t.vt_stream();
        // Save cursor
        s.next_slice(b"\x1B7");
        // Move cursor elsewhere
        s.next_slice(b"\x1B[1;1H");
    }
    assert_eq!(0_usize, t.active_mut().cursor.x as usize);
    assert_eq!(0_usize, t.active_mut().cursor.y as usize);

    {
        let mut s = t.vt_stream();
        // Restore cursor
        s.next_slice(b"\x1B8");
    }
    assert_eq!(14_usize, t.active_mut().cursor.x as usize);
    assert_eq!(9_usize, t.active_mut().cursor.y as usize);

    t.deinit(Global);
}

// test "attributes"
#[test]
fn attributes() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

    {
        let mut s = t.vt_stream();
        // Set bold and write text
        s.next_slice(b"\x1B[1mBold\x1B[0m");
    }

    // Verify we can write attributes - just check the string was written
    let str = t.plain_string(Global).unwrap();
    assert_eq!(str.as_slice(), b"Bold");

    t.deinit(Global);
}

// test "DECALN screen alignment"
#[test]
fn decaln_screen_alignment() {
    let mut t = Terminal::init(Global, opts(10, 3)).unwrap();

    {
        let mut s = t.vt_stream();
        // Run DECALN
        s.next_slice(b"\x1B#8");
    }

    // Verify entire screen is filled with 'E'
    let str = t.plain_string(Global).unwrap();
    assert_eq!(str.as_slice(), b"EEEEEEEEEE\nEEEEEEEEEE\nEEEEEEEEEE");

    // Cursor should be at 1,1
    assert_eq!(0_usize, t.active_mut().cursor.x as usize);
    assert_eq!(0_usize, t.active_mut().cursor.y as usize);

    t.deinit(Global);
}

// test "OSC 4 set and reset palette"
#[test]
fn osc_4_set_and_reset_palette() {
    let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

    // Save default color
    let default_color_0 = t.colors.palette.original[0];

    {
        let mut s = t.vt_stream();
        // Set color 0 to red
        s.next_slice(b"\x1b]4;0;rgb:ff/00/00\x1b\\");
    }
    assert_eq!(0xff_u8, t.colors.palette.current[0].r);
    assert_eq!(0x00_u8, t.colors.palette.current[0].g);
    assert_eq!(0x00_u8, t.colors.palette.current[0].b);
    assert!(t.colors.palette.mask.is_set(0));

    {
        let mut s = t.vt_stream();
        // Reset color 0
        s.next_slice(b"\x1b]104;0\x1b\\");
    }
    assert_eq!(default_color_0, t.colors.palette.current[0]);
    assert!(!t.colors.palette.mask.is_set(0));

    t.deinit(Global);
}

// test "OSC 104 reset all palette colors"
#[test]
fn osc_104_reset_all_palette_colors() {
    let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

    {
        let mut s = t.vt_stream();
        // Set multiple colors
        s.next_slice(b"\x1b]4;0;rgb:ff/00/00\x1b\\");
        s.next_slice(b"\x1b]4;1;rgb:00/ff/00\x1b\\");
        s.next_slice(b"\x1b]4;2;rgb:00/00/ff\x1b\\");
    }
    assert!(t.colors.palette.mask.is_set(0));
    assert!(t.colors.palette.mask.is_set(1));
    assert!(t.colors.palette.mask.is_set(2));

    {
        let mut s = t.vt_stream();
        // Reset all palette colors
        s.next_slice(b"\x1b]104\x1b\\");
    }
    assert_eq!(t.colors.palette.original[0], t.colors.palette.current[0]);
    assert_eq!(t.colors.palette.original[1], t.colors.palette.current[1]);
    assert_eq!(t.colors.palette.original[2], t.colors.palette.current[2]);
    assert!(!t.colors.palette.mask.is_set(0));
    assert!(!t.colors.palette.mask.is_set(1));
    assert!(!t.colors.palette.mask.is_set(2));

    t.deinit(Global);
}

// test "OSC 10 set and reset foreground color"
#[test]
fn osc_10_set_and_reset_foreground_color() {
    let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

    // Initially unset
    assert!(t.colors.foreground.get().is_none());

    {
        let mut s = t.vt_stream();
        // Set foreground to red
        s.next_slice(b"\x1b]10;rgb:ff/00/00\x1b\\");
    }
    let fg = t.colors.foreground.get().unwrap();
    assert_eq!(0xff_u8, fg.r);
    assert_eq!(0x00_u8, fg.g);
    assert_eq!(0x00_u8, fg.b);

    {
        let mut s = t.vt_stream();
        // Reset foreground
        s.next_slice(b"\x1b]110\x1b\\");
    }
    assert!(t.colors.foreground.get().is_none());

    t.deinit(Global);
}

// test "OSC 11 set and reset background color"
#[test]
fn osc_11_set_and_reset_background_color() {
    let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

    let default = color::RGB {
        r: 0x10,
        g: 0x20,
        b: 0x30,
    };
    t.colors.background.default = Some(default);

    {
        let mut s = t.vt_stream();
        // Set background to green
        s.next_slice(b"\x1b]11;rgb:00/ff/00\x1b\\");
    }
    let bg = t.colors.background.get().unwrap();
    assert_eq!(0x00_u8, bg.r);
    assert_eq!(0xff_u8, bg.g);
    assert_eq!(0x00_u8, bg.b);

    {
        let mut s = t.vt_stream();
        // Reset background
        s.next_slice(b"\x1b]111\x1b\\");
    }
    assert_eq!(default, t.colors.background.get().unwrap());
    assert_eq!(None, t.colors.background.override_);

    // A reset color continues to follow later configuration changes.
    let updated = color::RGB {
        r: 0x40,
        g: 0x50,
        b: 0x60,
    };
    t.colors.background.default = Some(updated);
    assert_eq!(updated, t.colors.background.get().unwrap());

    t.deinit(Global);
}

// test "OSC 12 set and reset cursor color"
#[test]
fn osc_12_set_and_reset_cursor_color() {
    let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

    {
        let mut s = t.vt_stream();
        // Set cursor to blue
        s.next_slice(b"\x1b]12;rgb:00/00/ff\x1b\\");
    }
    let cursor = t.colors.cursor.get().unwrap();
    assert_eq!(0x00_u8, cursor.r);
    assert_eq!(0x00_u8, cursor.g);
    assert_eq!(0xff_u8, cursor.b);

    {
        let mut s = t.vt_stream();
        // Reset cursor
        s.next_slice(b"\x1b]112\x1b\\");
    }
    // After reset, cursor might be null (using default)

    t.deinit(Global);
}

// test "OSC color query responses" (oracle 14c829883)
#[test]
fn osc_color_query_responses() {
    let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

    cap_reset();
    let mut s = t.vt_stream();
    s.handler.effects.write_pty = Some(cap_write_pty);

    s.next_slice(b"\x1b]10;?\x1b\\");
    assert!(cap_take().is_none());

    s.next_slice(b"\x1b]11;?\x1b\\");
    assert!(cap_take().is_none());

    s.next_slice(b"\x1b]4;2;rgb:12/34/56;2;?\x1b\\");
    assert_eq!(
        cap_take().unwrap().as_slice(),
        b"\x1b]4;2;rgb:1212/3434/5656\x1b\\"
    );

    s.next_slice(b"\x1b]10;rgb:01/02/03\x1b\\");
    s.next_slice(b"\x1b]11;rgb:04/05/06\x1b\\");
    s.next_slice(b"\x1b]12;rgb:07/08/09\x1b\\");
    s.next_slice(b"\x1b]10;?;?;?\x1b\\");
    assert_eq!(
        cap_take().unwrap().as_slice(),
        [
            &b"\x1b]10;rgb:0101/0202/0303\x1b\\"[..],
            &b"\x1b]11;rgb:0404/0505/0606\x1b\\"[..],
            &b"\x1b]12;rgb:0707/0808/0909\x1b\\"[..],
        ]
        .concat()
        .as_slice()
    );

    s.next_slice(b"\x1b]112\x1b\\");
    s.next_slice(b"\x1b]12;?\x07");
    assert_eq!(
        cap_take().unwrap().as_slice(),
        b"\x1b]12;rgb:0101/0202/0303\x07"
    );

    drop(s);
    t.deinit(Global);
}

// test "kitty color protocol set palette"
#[test]
fn kitty_color_protocol_set_palette() {
    let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

    {
        let mut s = t.vt_stream();
        // Set palette color 5 to magenta using kitty protocol
        s.next_slice(b"\x1b]21;5=rgb:ff/00/ff\x1b\\");
    }
    assert_eq!(0xff_u8, t.colors.palette.current[5].r);
    assert_eq!(0x00_u8, t.colors.palette.current[5].g);
    assert_eq!(0xff_u8, t.colors.palette.current[5].b);
    assert!(t.colors.palette.mask.is_set(5));
    assert!(t.flags.dirty.palette);

    t.deinit(Global);
}

// test "kitty color protocol reset palette"
#[test]
fn kitty_color_protocol_reset_palette() {
    let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

    // Set and then reset palette color
    let original = t.colors.palette.original[7];
    {
        let mut s = t.vt_stream();
        s.next_slice(b"\x1b]21;7=rgb:aa/bb/cc\x1b\\");
    }
    assert!(t.colors.palette.mask.is_set(7));

    {
        let mut s = t.vt_stream();
        s.next_slice(b"\x1b]21;7=\x1b\\");
    }
    assert_eq!(original, t.colors.palette.current[7]);
    assert!(!t.colors.palette.mask.is_set(7));

    t.deinit(Global);
}

// test "kitty color protocol set foreground"
#[test]
fn kitty_color_protocol_set_foreground() {
    let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

    {
        let mut s = t.vt_stream();
        // Set foreground using kitty protocol
        s.next_slice(b"\x1b]21;foreground=rgb:12/34/56\x1b\\");
    }
    let fg = t.colors.foreground.get().unwrap();
    assert_eq!(0x12_u8, fg.r);
    assert_eq!(0x34_u8, fg.g);
    assert_eq!(0x56_u8, fg.b);

    t.deinit(Global);
}

// test "kitty color protocol set background"
#[test]
fn kitty_color_protocol_set_background() {
    let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

    {
        let mut s = t.vt_stream();
        // Set background using kitty protocol
        s.next_slice(b"\x1b]21;background=rgb:78/9a/bc\x1b\\");
    }
    let bg = t.colors.background.get().unwrap();
    assert_eq!(0x78_u8, bg.r);
    assert_eq!(0x9a_u8, bg.g);
    assert_eq!(0xbc_u8, bg.b);

    t.deinit(Global);
}

// test "kitty color protocol set cursor"
#[test]
fn kitty_color_protocol_set_cursor() {
    let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

    {
        let mut s = t.vt_stream();
        // Set cursor using kitty protocol
        s.next_slice(b"\x1b]21;cursor=rgb:de/f0/12\x1b\\");
    }
    let cursor = t.colors.cursor.get().unwrap();
    assert_eq!(0xde_u8, cursor.r);
    assert_eq!(0xf0_u8, cursor.g);
    assert_eq!(0x12_u8, cursor.b);

    t.deinit(Global);
}

// test "kitty color protocol reset foreground"
#[test]
fn kitty_color_protocol_reset_foreground() {
    let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

    {
        let mut s = t.vt_stream();
        // Set and reset foreground
        s.next_slice(b"\x1b]21;foreground=rgb:11/22/33\x1b\\");
    }
    assert!(t.colors.foreground.get().is_some());

    {
        let mut s = t.vt_stream();
        s.next_slice(b"\x1b]21;foreground=\x1b\\");
    }
    // After reset, should be unset
    assert!(t.colors.foreground.get().is_none());

    t.deinit(Global);
}

// test "kitty color protocol query responses" (oracle 14c829883)
#[test]
fn kitty_color_protocol_query_responses() {
    let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

    cap_reset();
    let mut s = t.vt_stream();
    s.handler.effects.write_pty = Some(cap_write_pty);

    s.next_slice(b"\x1b]21;background=?\x1b\\");
    assert_eq!(cap_take().unwrap().as_slice(), b"\x1b]21;background=\x1b\\");

    s.next_slice(b"\x1b]21;foreground=rgb:12/34/56;2=rgb:aa/bb/cc\x1b\\");
    s.next_slice(b"\x1b]21;foreground=?;background=?;2=?\x1b\\");
    assert_eq!(
        cap_take().unwrap().as_slice(),
        b"\x1b]21;foreground=rgb:12/34/56;background=;2=rgb:aa/bb/cc\x1b\\"
    );

    drop(s);
    t.deinit(Global);
}

// test "palette dirty flag set on color change"
#[test]
fn palette_dirty_flag_set_on_color_change() {
    let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

    // Clear dirty flag
    t.flags.dirty.palette = false;

    {
        let mut s = t.vt_stream();
        // Setting palette color should set dirty flag
        s.next_slice(b"\x1b]4;0;rgb:ff/00/00\x1b\\");
    }
    assert!(t.flags.dirty.palette);

    // Clear and test reset
    t.flags.dirty.palette = false;
    {
        let mut s = t.vt_stream();
        s.next_slice(b"\x1b]104;0\x1b\\");
    }
    assert!(t.flags.dirty.palette);

    // Clear and test kitty protocol
    t.flags.dirty.palette = false;
    {
        let mut s = t.vt_stream();
        s.next_slice(b"\x1b]21;1=rgb:00/ff/00\x1b\\");
    }
    assert!(t.flags.dirty.palette);

    t.deinit(Global);
}

// test "semantic prompt fresh line"
#[test]
fn semantic_prompt_fresh_line() {
    let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

    {
        let mut s = t.vt_stream();
        s.next_slice(b"Hello");
        s.next_slice(b"\x1b]133;L\x07");
    }
    assert_eq!(0_usize, t.active_mut().cursor.x as usize);
    assert_eq!(1_usize, t.active_mut().cursor.y as usize);

    t.deinit(Global);
}

// test "semantic prompt fresh line new prompt"
#[test]
fn semantic_prompt_fresh_line_new_prompt() {
    let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

    {
        let mut s = t.vt_stream();
        // Write some text and then send OSC 133;A (fresh_line_new_prompt)
        s.next_slice(b"Hello");
        s.next_slice(b"\x1b]133;A\x07");
    }

    // Should do a fresh line (carriage return + index)
    assert_eq!(0_usize, t.active_mut().cursor.x as usize);
    assert_eq!(1_usize, t.active_mut().cursor.y as usize);

    // Should set cursor semantic_content to prompt
    assert_eq!(
        crate::page::SemanticContent::Prompt,
        t.active_mut().cursor.semantic_content
    );

    {
        let mut s = t.vt_stream();
        // Test with redraw option
        s.next_slice(b"prompt$ ");
        s.next_slice(b"\x1b]133;A;redraw=1\x07");
    }
    assert_eq!(
        crate::osc::semantic_prompt::Redraw::True,
        t.flags.shell_redraws_prompt
    );

    t.deinit(Global);
}

// test "semantic prompt end of input, then start output"
#[test]
fn semantic_prompt_end_of_input_then_start_output() {
    let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

    {
        let mut s = t.vt_stream();
        // Write some text and then send OSC 133;A (fresh_line_new_prompt)
        s.next_slice(b"Hello");
        s.next_slice(b"\x1b]133;A\x07");
        s.next_slice(b"prompt$ ");
        s.next_slice(b"\x1b]133;B\x07");
    }
    assert_eq!(
        crate::page::SemanticContent::Input,
        t.active_mut().cursor.semantic_content
    );
    {
        let mut s = t.vt_stream();
        s.next_slice(b"\x1b]133;C\x07");
    }
    assert_eq!(
        crate::page::SemanticContent::Output,
        t.active_mut().cursor.semantic_content
    );

    t.deinit(Global);
}

// test "semantic prompt prompt_start"
#[test]
fn semantic_prompt_prompt_start() {
    let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

    {
        let mut s = t.vt_stream();
        // Write some text
        s.next_slice(b"Hello");

        // OSC 133;P marks the start of a prompt (without fresh line behavior)
        s.next_slice(b"\x1b]133;P\x07");
    }
    assert_eq!(
        crate::page::SemanticContent::Prompt,
        t.active_mut().cursor.semantic_content
    );
    assert_eq!(5_usize, t.active_mut().cursor.x as usize);
    assert_eq!(0_usize, t.active_mut().cursor.y as usize);

    t.deinit(Global);
}

// test "semantic prompt new_command"
#[test]
fn semantic_prompt_new_command() {
    let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

    {
        let mut s = t.vt_stream();
        // Write some text
        s.next_slice(b"Hello");
        s.next_slice(b"\x1b]133;N\x07");
    }

    // Should behave like fresh_line_new_prompt - cursor moves to column 0
    // on next line since we had content
    assert_eq!(0_usize, t.active_mut().cursor.x as usize);
    assert_eq!(1_usize, t.active_mut().cursor.y as usize);
    assert_eq!(
        crate::page::SemanticContent::Prompt,
        t.active_mut().cursor.semantic_content
    );

    t.deinit(Global);
}

// test "semantic prompt new_command at column zero"
#[test]
fn semantic_prompt_new_command_at_column_zero() {
    let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

    {
        let mut s = t.vt_stream();
        // OSC 133;N when already at column 0 should stay on same line
        s.next_slice(b"\x1b]133;N\x07");
    }
    assert_eq!(0_usize, t.active_mut().cursor.x as usize);
    assert_eq!(0_usize, t.active_mut().cursor.y as usize);
    assert_eq!(
        crate::page::SemanticContent::Prompt,
        t.active_mut().cursor.semantic_content
    );

    t.deinit(Global);
}

// test "semantic prompt end_prompt_start_input_terminate_eol clears on linefeed"
#[test]
fn semantic_prompt_end_prompt_start_input_terminate_eol_clears_on_linefeed() {
    let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

    {
        let mut s = t.vt_stream();
        // Set input terminated by EOL
        s.next_slice(b"\x1b]133;I\x07");
    }
    assert_eq!(
        crate::page::SemanticContent::Input,
        t.active_mut().cursor.semantic_content
    );

    {
        let mut s = t.vt_stream();
        // Linefeed should reset semantic content to output
        s.next_slice(b"\n");
    }
    assert_eq!(
        crate::page::SemanticContent::Output,
        t.active_mut().cursor.semantic_content
    );

    t.deinit(Global);
}

// test "full reset"
#[test]
fn full_reset() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

    {
        let mut s = t.vt_stream();
        // Make some changes
        s.next_slice(b"Hello");
        s.next_slice(b"\x1B[10;20H");
        s.next_slice(b"\x1B[5;20r"); // Set scroll region
        s.next_slice(b"\x1B[?7l"); // Disable wraparound
        s.next_slice(b"\x1B_25a1;r;cp=e0a0;AAAAAAAAAAAAAA==\x1B\\");
    }
    // try testing.expect(t.glyph_glossary.contains(0xE0A0));
    assert!(t.glyph_glossary.contains(0xE0A0));

    {
        let mut s = t.vt_stream();
        // Full reset
        s.next_slice(b"\x1Bc");
    }

    // Verify reset state
    assert_eq!(0usize, t.active_mut().cursor.x as usize);
    assert_eq!(0usize, t.active_mut().cursor.y as usize);
    assert_eq!(0usize, t.scrolling_region.top as usize);
    assert_eq!(23usize, t.scrolling_region.bottom as usize);
    assert!(t.modes.get(modespkg::Mode::Wraparound));
    // try testing.expect(!t.glyph_glossary.contains(0xE0A0));
    assert!(!t.glyph_glossary.contains(0xE0A0));

    t.deinit(Global);
}

// test "glyph protocol APC with write_pty callback"
#[test]
#[cfg_attr(
    not(feature = "glyph-protocol"),
    ignore = "SkipZigTest: needs options::GLYPH_PROTOCOL (R14)"
)]
fn glyph_protocol_apc_with_write_pty_callback() {
    // if (comptime !build_options.glyph_protocol) return error.SkipZigTest;

    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

    cap_reset();
    let mut s = t.vt_stream();
    // handler.effects.write_pty = &S.writePty;
    s.handler.effects.write_pty = Some(cap_write_pty);

    // s.nextSlice("\x1B_25a1;s\x1B\\");
    s.next_slice(b"\x1B_25a1;s\x1B\\");
    // try testing.expectEqualStrings("\x1B_25a1;s;fmt=glyf\x1B\\", S.last_response.?);
    assert_eq!(
        cap_take().unwrap().as_slice(),
        b"\x1B_25a1;s;fmt=glyf\x1B\\"
    );

    // s.nextSlice("\x1B_25a1;r;cp=e0a0;AAAAAAAAAAAAAA==\x1B\\");
    s.next_slice(b"\x1B_25a1;r;cp=e0a0;AAAAAAAAAAAAAA==\x1B\\");
    // try testing.expectEqualStrings("\x1B_25a1;r;cp=e0a0;status=0\x1B\\", S.last_response.?);
    assert_eq!(
        cap_take().unwrap().as_slice(),
        b"\x1B_25a1;r;cp=e0a0;status=0\x1B\\"
    );
    // try testing.expect(t.glyph_glossary.contains(0xE0A0));
    drop(s);
    assert!(t.glyph_glossary.contains(0xE0A0));

    t.deinit(Global);
}

// test "ignores query actions"
//
// PORT NOTE (vt-12b-7b): reactivated now that the RESPONSE/QUERY arms are
// filled. With default (readonly) effects, `\x1B[c` (device_attributes) and
// `\x1B[5n`/`\x1B[6n` (device_status) are silent no-ops, so the terminal
// remains functional afterward.
#[test]
fn ignores_query_actions() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

    {
        let mut s = t.vt_stream();
        // These should be ignored without error
        s.next_slice(b"\x1B[c"); // Device attributes
        s.next_slice(b"\x1B[5n"); // Device status report
        s.next_slice(b"\x1B[6n"); // Cursor position report
        s.next_slice(b"\x1B]4;0;?\x1B\\"); // OSC color query
        s.next_slice(b"\x1B]21;foreground=?\x1B\\"); // Kitty color query
        s.next_slice(b"\x1B]52;c;%%%invalid-base64%%%\x1B\\");
        s.next_slice(b"\x1B_Ga=p,i=999\x1B\\"); // Missing Kitty image
        s.next_slice(b"\x1B_25a1;r;cp=41;%%%invalid%%%\x1B\\"); // Rejected glyph

        // Query, malformed input, protocol failure responses, and
        // external-effect failures do not imply that terminal-owned
        // semantic state diverged.
        assert!(!s.handler.semantic_failure);

        // Terminal should still be functional
        s.next_slice(b"Test");
    }
    let str = t.plain_string(Global).unwrap();
    assert_eq!(str.as_slice(), b"Test");

    t.deinit(Global);
}

// test "stream: CSI W with intermediate but no params"
#[test]
fn stream_csi_w_with_intermediate_but_no_params() {
    // Regression test from AFL++ crash. CSI ? W without
    // parameters caused an out-of-bounds access on input.params[0].
    let mut t = Terminal::init(
        Global,
        Options {
            max_scrollback_bytes: Some(100),
            max_scrollback_lines: None,
            ..opts(80, 24)
        },
    )
    .unwrap();

    {
        let mut s = t.vt_stream();
        s.next_slice(b"\x1b[?W");
    }

    t.deinit(Global);
}

// test "bell effect callback"
//
// PORT NOTE (vt-12b-7a): the reference's callback-count half captures the
// count in a local-struct `var` static (`S.bell_count`); Rust's `bell`
// effect is a bare `fn` pointer (no closures/captures), so the count is
// routed through the tests-module `COUNTER` thread_local cell — the same
// analog the vt-12b-7b effect tests below use for `title_changed`.
#[test]
fn bell_effect_callback() {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

    // Test bell with null callback (default readonly effects) doesn't crash
    {
        let mut s = t.vt_stream();
        s.next_slice(b"\x07");

        // Terminal should still be functional after bell
        s.next_slice(b"AfterBell");
    }
    let str = t.plain_string(Global).unwrap();
    // try testing.expectEqualStrings("AfterBell", str);
    assert_eq!(str.as_slice(), b"AfterBell");

    // t.fullReset();
    t.full_reset();

    // Test bell with a callback
    {
        // const S = struct { var bell_count: usize = 0;
        //     fn bell(_: *Handler) void { bell_count += 1; } };
        fn bell(_: &mut Handler) {
            COUNTER.with(|c| c.set(c.get() + 1));
        }
        // S.bell_count = 0;
        cap_reset();

        // var handler: Handler = .init(&t); handler.effects.bell = &S.bell;
        let mut s = t.vt_stream();
        s.handler.effects.bell = Some(bell);

        s.next_slice(b"\x07");
        // try testing.expectEqual(@as(usize, 1), S.bell_count);
        assert_eq!(1_usize, COUNTER.with(|c| c.get()));

        s.next_slice(b"\x07\x07");
        // try testing.expectEqual(@as(usize, 3), S.bell_count);
        assert_eq!(3_usize, COUNTER.with(|c| c.get()));
    }

    t.deinit(Global);
}
