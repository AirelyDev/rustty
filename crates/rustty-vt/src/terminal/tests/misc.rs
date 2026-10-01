//! PORTS: ghostty src/terminal/Terminal.zig — the input-sweep family and full-diff stragglers, the cross-page wide-grapheme transfer gate and the recycled-row metadata tests.

use super::*;

// ===========================================================================
// vt-12 recovery: Terminal input-sweep family + full-diff stragglers.
// ===========================================================================

// test "Terminal: setCursorPos saturates overflowing origin offsets"
// PORT NOTE (parity 0aaedf436): test hunk of the upstream saturating
// origin-offset fix in setCursorPos.
#[test]
fn terminal_set_cursor_pos_saturates_overflowing_origin_offsets() {
    let alloc = Global;
    let mut t = Terminal::init(alloc, opts(10, 10)).unwrap();

    t.scrolling_region = ScrollingRegion {
        top: 2,
        bottom: 7,
        left: 3,
        right: 8,
    };
    t.modes.set(modespkg::Mode::Origin, true);

    t.set_cursor_pos(usize::MAX, usize::MAX);
    // try testing.expectEqual(@as(size.CellCountInt, 8), t.screens.active.cursor.x);
    assert_eq!(8 as size::CellCountInt, unsafe {
        (*t.screens.active).cursor.x
    });
    // try testing.expectEqual(@as(size.CellCountInt, 7), t.screens.active.cursor.y);
    assert_eq!(7 as size::CellCountInt, unsafe {
        (*t.screens.active).cursor.y
    });

    t.deinit(alloc);
}

// test "Terminal: input with no control characters"
#[test]
fn terminal_input_with_no_control_characters() {
    let alloc = Global;
    let mut t = Terminal::init(alloc, opts(40, 40)).unwrap();

    // Basic grid writing
    for c in "hello".chars() {
        t.print(c as u32).unwrap();
    }
    assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
    assert_eq!(unsafe { (*t.screens.active).cursor.x }, 5);
    {
        let str = t.plain_string(alloc).unwrap();
        assert_eq!(str.as_slice(), "hello".as_bytes());
    }

    // The first row should be dirty
    assert!(t.is_dirty(screen_pt(5, 0)));
    assert!(!t.is_dirty(screen_pt(5, 1)));

    t.deinit(alloc);
}

// test "Terminal: input with basic wraparound"
#[test]
fn terminal_input_with_basic_wraparound() {
    let alloc = Global;
    let mut t = Terminal::init(alloc, opts(5, 40)).unwrap();

    // Basic grid writing
    for c in "helloworldabc12".chars() {
        t.print(c as u32).unwrap();
    }
    assert_eq!(unsafe { (*t.screens.active).cursor.y }, 2);
    assert_eq!(unsafe { (*t.screens.active).cursor.x }, 4);
    assert!(unsafe { (*t.screens.active).cursor.pending_wrap });
    {
        let str = t.plain_string(alloc).unwrap();
        assert_eq!(str.as_slice(), "hello\nworld\nabc12".as_bytes());
    }

    t.deinit(alloc);
}

// test "Terminal: input with basic wraparound dirty"
#[test]
fn terminal_input_with_basic_wraparound_dirty() {
    let alloc = Global;
    let mut t = Terminal::init(alloc, opts(5, 40)).unwrap();

    for c in "hello".chars() {
        t.print(c as u32).unwrap();
    }
    assert!(t.is_dirty(screen_pt(4, 0)));
    t.clear_dirty();
    t.print('w' as u32).unwrap();

    // Old row is dirty because cursor moved from there
    assert!(t.is_dirty(screen_pt(4, 0)));
    assert!(t.is_dirty(screen_pt(0, 1)));

    t.deinit(alloc);
}

// test "Terminal: input that forces scroll"
#[test]
fn terminal_input_that_forces_scroll() {
    let alloc = Global;
    let mut t = Terminal::init(alloc, opts(1, 5)).unwrap();

    // Basic grid writing
    for c in "abcdef".chars() {
        t.print(c as u32).unwrap();
    }
    assert_eq!(unsafe { (*t.screens.active).cursor.y }, 4);
    assert_eq!(unsafe { (*t.screens.active).cursor.x }, 0);
    {
        let str = t.plain_string(alloc).unwrap();
        assert_eq!(str.as_slice(), "b\nc\nd\ne\nf".as_bytes());
    }

    t.deinit(alloc);
}

// test "Terminal: input unique style per cell"
#[test]
fn terminal_input_unique_style_per_cell() {
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

    t.deinit(alloc);
}

// test "Terminal: input glitch text"
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
)]
#[test]
fn terminal_input_glitch_text() {
    const GLITCH: &[u8] = include_bytes!("../../res/glitch.txt");
    let alloc = Global;
    let mut t = Terminal::init(alloc, opts(30, 30)).unwrap();

    // Get our initial grapheme capacity.
    let grapheme_cap = {
        let page = t.active_mut().pages.pages.first;
        unsafe { crate::pagelist::Node::capacity(page).grapheme_bytes }
    };

    // Print glitch text until our capacity changes
    loop {
        let page = t.active_mut().pages.pages.first;
        if unsafe { crate::pagelist::Node::capacity(page).grapheme_bytes } != grapheme_cap {
            break;
        }
        t.print_string(GLITCH).unwrap();
    }

    // We're testing to make sure that grapheme capacity gets increased.
    let page = t.active_mut().pages.pages.first;
    assert!(unsafe { crate::pagelist::Node::capacity(page).grapheme_bytes } > grapheme_cap);

    t.deinit(alloc);
}

// test "Terminal: zero-width character at start"
#[test]
fn terminal_zero_width_character_at_start() {
    let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

    // This used to crash the terminal. This is not allowed so we should
    // just ignore it.
    //
    // S141-F4: `print_inner_zero_width`'s `x == 0` guard logs
    // `log.warn("zero-width character with no prior character,
    // ignoring", .{})` (Terminal.zig:1480) — this used to crash the
    // terminal, so it's ignored instead.
    let cap = crate::log_capture::start();
    t.print(0x200D).unwrap();
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

    assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
    assert_eq!(unsafe { (*t.screens.active).cursor.x }, 0);

    // Should not be dirty since we changed nothing.
    assert!(!t.is_dirty(screen_pt(0, 0)));

    t.deinit(Global);
}

// test "Terminal: ignored VS16 doesn't mark dirty"
#[test]
fn terminal_ignored_vs16_doesnt_mark_dirty() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    // Disable grapheme clustering
    t.modes.set(modespkg::Mode::GraphemeCluster, false);

    t.print(0x2764).unwrap(); // Heart
    assert!(t.is_dirty(screen_pt(0, 0)));

    t.clear_dirty();
    t.print(0xFE0F).unwrap(); // VS16 to make wide
    assert!(!t.is_dirty(screen_pt(0, 0)));

    t.deinit(Global);
}

// test "Terminal: invalid VS16 doesn't mark dirty"
#[test]
fn terminal_invalid_vs16_doesnt_mark_dirty() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    // Disable grapheme clustering
    t.modes.set(modespkg::Mode::GraphemeCluster, false);

    t.print('x' as u32).unwrap();
    assert!(t.is_dirty(screen_pt(0, 0)));

    t.clear_dirty();
    t.print(0xFE0F).unwrap(); // VS16 to make wide
    assert!(!t.is_dirty(screen_pt(0, 0)));

    t.deinit(Global);
}

// test "Terminal: multicodepoint grapheme marks dirty on every codepoint"
#[test]
fn terminal_multicodepoint_grapheme_marks_dirty_on_every_codepoint() {
    let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

    // Enable grapheme clustering
    t.modes.set(modespkg::Mode::GraphemeCluster, true);

    // https://github.com/mitchellh/ghostty/issues/289
    // This is: 👨‍👩‍👧 (which may or may not render correctly)
    t.print(0x1F468).unwrap();
    assert!(t.is_dirty(screen_pt(0, 0)));
    t.clear_dirty();
    t.print(0x200D).unwrap();
    assert!(t.is_dirty(screen_pt(0, 0)));
    t.clear_dirty();
    t.print(0x1F469).unwrap();
    assert!(t.is_dirty(screen_pt(0, 0)));
    t.clear_dirty();
    t.print(0x200D).unwrap();
    assert!(t.is_dirty(screen_pt(0, 0)));
    t.clear_dirty();
    t.print(0x1F467).unwrap();
    assert!(t.is_dirty(screen_pt(0, 0)));

    // We should have 2 cells taken up. It is one character but "wide".
    assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
    assert_eq!(unsafe { (*t.screens.active).cursor.x }, 2);

    t.deinit(Global);
}

// test "Terminal: horizontal tab back with cursor before left margin"
#[test]
fn terminal_horizontal_tab_back_with_cursor_before_left_margin() {
    let alloc = Global;
    let mut t = Terminal::init(alloc, opts(20, 5)).unwrap();

    t.modes.set(modespkg::Mode::Origin, true);
    t.save_cursor();
    t.modes.set(modespkg::Mode::EnableLeftAndRightMargin, true);
    t.set_left_and_right_margin(5, 0);
    t.restore_cursor();
    t.horizontal_tab_back();
    t.print('X' as u32).unwrap();

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "X".as_bytes());
    }

    t.deinit(alloc);
}

// Probably outdated, but dates back to the original terminal implementation.
// test "Terminal: setCursorPos (original test)"
#[test]
fn terminal_set_cursor_pos_original_test() {
    let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

    assert_eq!(unsafe { (*t.screens.active).cursor.x }, 0);
    assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);

    // Setting it to 0 should keep it zero (1 based)
    t.set_cursor_pos(0, 0);
    assert_eq!(unsafe { (*t.screens.active).cursor.x }, 0);
    assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);

    // Should clamp to size
    t.set_cursor_pos(81, 81);
    assert_eq!(unsafe { (*t.screens.active).cursor.x }, 79);
    assert_eq!(unsafe { (*t.screens.active).cursor.y }, 79);

    // Should reset pending wrap
    t.set_cursor_pos(0, 80);
    t.print('c' as u32).unwrap();
    assert!(unsafe { (*t.screens.active).cursor.pending_wrap });
    t.set_cursor_pos(0, 80);
    assert!(!unsafe { (*t.screens.active).cursor.pending_wrap });

    // Origin mode
    t.modes.set(modespkg::Mode::Origin, true);

    // No change without a scroll region
    t.set_cursor_pos(81, 81);
    assert_eq!(unsafe { (*t.screens.active).cursor.x }, 79);
    assert_eq!(unsafe { (*t.screens.active).cursor.y }, 79);

    // Set the scroll region
    t.set_top_and_bottom_margin(10, t.rows as usize);
    t.set_cursor_pos(0, 0);
    assert_eq!(unsafe { (*t.screens.active).cursor.x }, 0);
    assert_eq!(unsafe { (*t.screens.active).cursor.y }, 9);

    t.set_cursor_pos(1, 1);
    assert_eq!(unsafe { (*t.screens.active).cursor.x }, 0);
    assert_eq!(unsafe { (*t.screens.active).cursor.y }, 9);

    t.set_cursor_pos(100, 0);
    assert_eq!(unsafe { (*t.screens.active).cursor.x }, 0);
    assert_eq!(unsafe { (*t.screens.active).cursor.y }, 79);

    t.set_top_and_bottom_margin(10, 11);
    t.set_cursor_pos(2, 0);
    assert_eq!(unsafe { (*t.screens.active).cursor.x }, 0);
    assert_eq!(unsafe { (*t.screens.active).cursor.y }, 10);

    t.deinit(Global);
}

// test "Terminal: garbage collect overwritten"
#[test]
fn terminal_garbage_collect_overwritten() {
    let alloc = Global;
    let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();

    t.set_attribute(sgr::Attribute::Bold).unwrap();
    t.print('A' as u32).unwrap();
    t.set_cursor_pos(1, 1);
    t.set_attribute(sgr::Attribute::Unset).unwrap();
    t.print('B' as u32).unwrap();

    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).content_codepoint() }, 'B' as u32);
        assert!(unsafe { (*cell).style_id() } == 0);
    }

    // verify we have no styles in our style map
    let page = unsafe { crate::pagelist::Node::page((*(*t.screens.active).cursor.page_pin).node) };
    assert_eq!(0usize, unsafe { (*page).styles.count() });

    t.deinit(alloc);
}

// test "Terminal: do not garbage collect old styles in use"
#[test]
fn terminal_do_not_garbage_collect_old_styles_in_use() {
    let alloc = Global;
    let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();

    t.set_attribute(sgr::Attribute::Bold).unwrap();
    t.print('A' as u32).unwrap();
    t.set_attribute(sgr::Attribute::Unset).unwrap();
    t.print('B' as u32).unwrap();

    {
        let list_cell = t.active_mut().pages.get_cell(screen_pt(1, 0)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).content_codepoint() }, 'B' as u32);
        assert!(unsafe { (*cell).style_id() } == 0);
    }

    // verify we have no styles in our style map
    let page = unsafe { crate::pagelist::Node::page((*(*t.screens.active).cursor.page_pin).node) };
    assert_eq!(1usize, unsafe { (*page).styles.count() });

    t.deinit(alloc);
}

// test "Terminal: DECALN"
#[test]
fn terminal_decaln() {
    let alloc = Global;
    let mut t = Terminal::init(alloc, opts(2, 2)).unwrap();

    // Initial value
    t.print('A' as u32).unwrap();
    t.carriage_return();
    t.linefeed().unwrap();
    t.print('B' as u32).unwrap();
    t.decaln().unwrap();

    assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
    assert_eq!(unsafe { (*t.screens.active).cursor.x }, 0);

    for y in 0..t.rows {
        assert!(t.is_dirty(active_pt(0, y as u32)));
    }

    {
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "EE\nEE".as_bytes());
    }

    t.deinit(alloc);
}

// test "Terminal: setProtectedMode"
#[test]
fn terminal_set_protected_mode() {
    let alloc = Global;
    let mut t = Terminal::init(alloc, opts(3, 3)).unwrap();

    assert!(!unsafe { (*t.screens.active).cursor.protected });
    t.set_protected_mode(ansi::ProtectedMode::Off);
    assert!(!unsafe { (*t.screens.active).cursor.protected });
    t.set_protected_mode(ansi::ProtectedMode::Iso);
    assert!(unsafe { (*t.screens.active).cursor.protected });
    t.set_protected_mode(ansi::ProtectedMode::Dec);
    assert!(unsafe { (*t.screens.active).cursor.protected });
    t.set_protected_mode(ansi::ProtectedMode::Off);
    assert!(!unsafe { (*t.screens.active).cursor.protected });

    t.deinit(alloc);
}

/// The activity token changes when the page list mutates, and does NOT
/// change merely because compression ran.
///
/// PORT NOTE: this test has no reference counterpart; the compression API
/// is exercised upstream only through `PageList`. It pins the two
/// properties `Terminal::compression_activity` promises to its callers.
#[test]
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION — BOTH LANES (rustty-vt, parity tail C-1; .port/phase-2/MIRI_EXCEPTIONS.md, 'Parity tail C-1 — the G6 triage'): past a 15-minute Miri bound with debug assertions on (G6) and past an 8-minute bound with them off (G6c), both measured one test per process; runs natively in cargo test --workspace and under ASan (see MIRI_EXCEPTIONS.md)"
)]
fn terminal_compression_activity_tracks_pagelist_mutation_not_compression() {
    let alloc = Global;
    let mut t = Terminal::init(alloc, opts_sb(80, 24, 10 * 1024 * 1024)).unwrap();
    let initial = t.compression_activity();

    let mut stream = t.vt_stream();
    let line = b"repeated and compressible terminal history\r\n";
    for _ in 0..4_000 {
        stream.next_slice(line);
    }
    stream.deinit();
    drop(stream);

    let activity = t.compression_activity();
    assert_ne!(activity, initial);

    let mut result = TerminalCompressionResult::Pending;
    for _ in 0..1_000 {
        result = t.compress(TerminalCompressionMode::Incremental);
        match result {
            TerminalCompressionResult::Pending => continue,
            TerminalCompressionResult::Complete => break,
            TerminalCompressionResult::Unsupported => {
                panic!("test builds always support strict reclamation")
            }
        }
    }
    assert_eq!(result, TerminalCompressionResult::Complete);

    // Compression changes storage representation, not the activity token.
    assert_eq!(t.compression_activity(), activity);

    t.deinit(alloc);
}

/// PINS THE `compress(Incremental)` RESTART (WS19 T18, audit row N10).
///
/// A fresh incremental call issued immediately after a pass returned `Complete`
/// returns **`Pending`**, not `Complete`: the incremental traversal restarts from
/// the beginning instead of remembering that it converged. That makes the obvious
/// "is compression done?" probe — call it once and look for `Complete` — invalid,
/// and it is the opposite of what the reference doc comment ("whether they should
/// continue calling it in the future") suggests to a reader.
///
/// This test exists so the `PORT NOTE` on `Terminal::compress` cannot rot: if a
/// future change ever makes a converged pass STAY `Complete`, this test fails and
/// whoever changes it must update the doc comment in the same commit. Referenced
/// BY NAME from that PORT NOTE, so a reader who doubts the claim can run it.
///
/// PORT NOTE: no reference counterpart — upstream exercises compression only
/// through `PageList`, and documents nothing about the restart.
#[test]
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION — BOTH LANES (rustty-vt, parity tail C-1; .port/phase-2/MIRI_EXCEPTIONS.md, 'Parity tail C-1 — the G6 triage'): past a 15-minute Miri bound with debug assertions on (G6) and past an 8-minute bound with them off (G6c), both measured one test per process; runs natively in cargo test --workspace and under ASan (see MIRI_EXCEPTIONS.md)"
)]
fn terminal_compress_incremental_restarts_after_complete() {
    let alloc = Global;
    let mut t = Terminal::init(alloc, opts_sb(80, 24, 10 * 1024 * 1024)).unwrap();

    let mut stream = t.vt_stream();
    let line = b"repeated and compressible terminal history\r\n";
    for _ in 0..4_000 {
        stream.next_slice(line);
    }
    stream.deinit();
    drop(stream);

    // Drive one incremental pass to convergence.
    let mut result = TerminalCompressionResult::Pending;
    let mut steps = 0usize;
    for _ in 0..10_000 {
        result = t.compress(TerminalCompressionMode::Incremental);
        steps += 1;
        match result {
            TerminalCompressionResult::Pending => continue,
            TerminalCompressionResult::Complete => break,
            TerminalCompressionResult::Unsupported => {
                panic!("test builds always support strict reclamation")
            }
        }
    }
    assert_eq!(
        result,
        TerminalCompressionResult::Complete,
        "the incremental pass must converge first, or this test proves nothing \
         (steps={steps})"
    );
    assert!(
        steps > 1,
        "the pass converged in ONE call, so `Complete` here carries no information \
         about a restart — the fixture is not compressible enough to be a valid \
         discriminator (steps={steps})"
    );

    // THE PROPERTY: the very next call is Pending again, not Complete.
    let again = t.compress(TerminalCompressionMode::Incremental);
    assert_eq!(
        again,
        TerminalCompressionResult::Pending,
        "a fresh compress(Incremental) right after a Complete pass must return \
         Pending, because the incremental traversal RESTARTS. If this now returns \
         Complete, the behavior changed — update the PORT NOTE on \
         `Terminal::compress` and the note in .port/phase-2/MIRI_EXCEPTIONS.md in \
         the same commit."
    );

    t.deinit(alloc);
}

// -- P3-W1 T4 step 1: stored grapheme boundaries (oracle b287f6d1a) -------

// ref: test "Terminal: enabling grapheme mode handles stored breaks"
#[test]
fn terminal_enabling_grapheme_mode_handles_stored_breaks() {
    let mut t = Terminal::init(Global, opts(5, 1)).unwrap();

    t.modes.set(modespkg::Mode::GraphemeCluster, false);
    t.print(b'a' as u21).unwrap();
    t.print(0x200B).unwrap(); // Zero width space is stored on the prior cell.

    t.modes.set(modespkg::Mode::GraphemeCluster, true);
    t.print(0x0301).unwrap();

    let str = t.plain_string(Global).unwrap();
    assert_eq!(b"a\xE2\x80\x8B\xCC\x81".as_slice(), str.as_slice());

    t.deinit(Global);
}

/// `REACHABILITY.md` row 5 demanded that this be driven END TO END through
/// `Stream`, not by calling `print` directly, before the row could be
/// ranked. This is that drive: the mode toggles are real DECSET/DECRST
/// sequences (`CSI ? 2027 h` / `l`) and the codepoints are real UTF-8 bytes
/// off the wire, so nothing here depends on a test reaching into `print`.
/// On today's pre-fix tree this panics in `print`'s stored-cluster walk.
///
/// The release-mode consequence, which row 5 also asked for: the site is a
/// `debug_assert!`, so in release it is COMPILED OUT and the pre-fix code
/// silently produced the same output as the fixed code — the defect is a
/// debug/ReleaseSafe-only crash, not a release miscompute. That is why this
/// test is a plain assertion on the resulting string: it is meaningful in
/// both profiles, and in debug it also catches the panic.
#[test]
fn terminal_enabling_grapheme_mode_handles_stored_breaks_via_stream() {
    let mut t = Terminal::init(Global, opts(5, 1)).unwrap();

    {
        let mut stream = t.vt_stream();
        // CSI ? 2027 l  -- disable grapheme clustering
        stream.next_slice(b"\x1b[?2027l");
        // "a" then U+200B ZERO WIDTH SPACE, attached with no boundary rules
        stream.next_slice("a\u{200B}".as_bytes());
        // CSI ? 2027 h  -- enable grapheme clustering
        stream.next_slice(b"\x1b[?2027h");
        // U+0301 COMBINING ACUTE ACCENT
        stream.next_slice("\u{0301}".as_bytes());
        stream.deinit();
    }

    let str = t.plain_string(Global).unwrap();
    assert_eq!(b"a\xE2\x80\x8B\xCC\x81".as_slice(), str.as_slice());

    t.deinit(Global);
}

// =========================================================================
// W29-B03: cross-page wide-grapheme transfer (`print`'s `transfer_graphemes`
// block, Terminal.zig:1247-1266).
//
// PORT NOTE: NOT a port of a reference `test {}` block — the reference has
// no test for this block at all. It is the correctness gate for the
// raw-ptr+len iteration of the OLD page's grapheme slice: the copy it
// replaced (`cps.to_vec()`) was a Rust-only allocation, and the ONLY thing
// that makes iterating the old page in place legal is that this arm requires
// `old_pin.node != new_pin.node` while `Screen::append_grapheme` mutates and
// reallocates the CURSOR's node only. The negative control for the
// allocation itself lives in `tests/print_grapheme_transfer_alloc.rs`, which
// counts allocations; this one gates the DATA.
// =========================================================================

/// Drive `print` until a wide-grapheme wrap lands on a freshly-created page
/// (`new_pin.node != old_pin.node`), then hand back the terminal parked
/// right after that print. `None` if the seam never happened.
const COLS: size::CellCountInt = 120;

/// Drive `print` until a wide-grapheme wrap lands on a freshly-created page
/// (`new_pin.node != old_pin.node`, the cross-page arm of
/// `transfer_graphemes`). Returns false if that never happened.
fn drive_wide_grapheme_page_seam(t: &mut Terminal) -> bool {
    t.modes.set(modespkg::Mode::GraphemeCluster, true);

    // Pad the pagelist with blank rows until the cursor's page has EXACTLY
    // one free row left. Then the `linefeed` below takes that last row and
    // the soft wrap after it is forced to start a new page — which is the
    // only way to reach the `old_pin.node != new_pin.node` arm.
    let mut guard = 0usize;
    loop {
        // SAFETY: stable pointers owned by the active screen; the cursor is
        // parked on the last page because it sits on the bottom row.
        let node = unsafe { (*(*t.screens.active).cursor.page_pin).node };
        let page = unsafe { crate::pagelist::Node::page(node) };
        let free = unsafe { (*page).capacity.rows - (*page).size.rows };
        if free == 1 {
            break;
        }
        guard += 1;
        if guard > 100_000 {
            return false;
        }
        t.carriage_return();
        t.linefeed().unwrap();
    }

    t.carriage_return();
    t.linefeed().unwrap();

    // Park the cursor on the last column with a NARROW base that already
    // carries grapheme data...
    for _ in 0..(COLS - 1) {
        t.print('a' as u32).unwrap();
    }
    t.print(0x261D).unwrap(); // U+261D BLACK UP POINTING INDEX (ExtPict, width 1)
    for _ in 0..7 {
        t.print(0x0301).unwrap(); // U+0301 COMBINING ACUTE ACCENT
    }
    t.print(0x200D).unwrap(); // U+200D ZERO WIDTH JOINER

    // SAFETY: stable pointers owned by the active screen.
    let before = unsafe { (*(*t.screens.active).cursor.page_pin).node };
    // ...then make the cluster WIDE, which wraps it to the next row and has
    // to carry the old page's grapheme data with it.
    t.print(0x1F600).unwrap(); // U+1F600 GRINNING FACE (ExtPict, width 2)
    let after = unsafe { (*(*t.screens.active).cursor.page_pin).node };

    !core::ptr::eq(before, after)
}

#[test]
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'Parity tail C-1 — the G6 triage'): past a 15-minute Miri bound in the debug-assertions-ON G6 lane, where `Page::assert_integrity` runs on nearly every page mutation (measured per test; see the C-1 table there). It RUNS in the G6c lane (`check_leaks.sh --debug-assertions-off`). Do NOT widen this to a plain `miri` ignore: that would drop the test from BOTH lanes."
)]
fn terminal_print_wide_grapheme_transfers_across_a_page_seam() {
    let mut t = Terminal::init(Global, opts(COLS, 5)).unwrap();

    assert!(
        drive_wide_grapheme_page_seam(&mut t),
        "VACUOUS: never produced a wrap that crossed a page seam, so the \
         cross-page arm of `transfer_graphemes` was never exercised"
    );

    // SAFETY: stable pointers owned by the active screen.
    let pin = unsafe { *(*t.screens.active).cursor.page_pin };

    // The wide cluster now lives at x=0 of the new page's row, carrying
    // every codepoint that was on the old page plus the one just printed.
    let mut new_pin = pin;
    new_pin.x = 0;
    let new_rac = new_pin.row_and_cell();
    let new_page = unsafe { crate::pagelist::Node::page(new_pin.node) };
    assert_eq!(0x261D, unsafe { (*new_rac.cell).content_codepoint() });
    assert_eq!(pagepkg::Wide::Wide, unsafe { (*new_rac.cell).wide() });
    assert!(unsafe { (*new_rac.cell).has_grapheme() });
    assert_eq!(
        &[0x0301, 0x0301, 0x0301, 0x0301, 0x0301, 0x0301, 0x0301, 0x200D, 0x1F600][..],
        unsafe { (*new_page).lookup_grapheme(new_rac.cell) }.unwrap(),
    );
    assert!(unsafe { (*new_rac.row).grapheme() });

    // ...and the old page's cell has been emptied: it is the spacer head of
    // the wrap, with its grapheme data cleared.
    let mut old_pin = new_pin.up(1).unwrap();
    old_pin.x = COLS - 1;
    assert!(
        !core::ptr::eq(old_pin.node, new_pin.node),
        "VACUOUS: the rows are on the same page"
    );
    let old_rac = old_pin.row_and_cell();
    let old_page = unsafe { crate::pagelist::Node::page(old_pin.node) };
    assert_eq!(0, unsafe { (*old_rac.cell).content_codepoint() });
    assert_eq!(pagepkg::Wide::SpacerHead, unsafe { (*old_rac.cell).wide() });
    assert!(!unsafe { (*old_rac.cell).has_grapheme() });
    assert!(unsafe { (*old_page).lookup_grapheme(old_rac.cell) }.is_none());

    t.deinit(Global);
}

// test "Terminal: scroll region linefeed recycled row has default metadata"
#[test]
fn terminal_scroll_region_linefeed_recycled_row_has_default_metadata() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    // A soft-wrapped line across rows 0-2 so that row 1 has both wrap
    // flags set. Mark row 1 as a prompt as well (OSC 133 A would).
    for _ in 0..12 {
        t.print('A' as u32).unwrap();
    }
    unsafe {
        (*t.active_mut().pages.get_cell(active_pt(0, 1)).unwrap().row)
            .set_semantic_prompt(pagepkg::SemanticPrompt::Prompt)
    };

    // DECSTBM rows 2-4, cursor to the region bottom, and linefeed:
    // row 1 is discarded and its Row storage recycled as the new
    // blank region-bottom row.
    t.set_top_and_bottom_margin(2, 4);
    t.set_cursor_pos(4, 1);
    t.linefeed().unwrap();

    {
        let rac = t.active_mut().pages.get_cell(active_pt(0, 3)).unwrap();
        unsafe {
            assert!(!(*rac.row).wrap());
            assert!(!(*rac.row).wrap_continuation());
            assert_eq!(pagepkg::SemanticPrompt::None, (*rac.row).semantic_prompt());
        }
    }

    t.deinit(Global);
}

// test "Terminal: alt screen scroll up recycled row has default metadata"
#[test]
fn terminal_alt_screen_scroll_up_recycled_row_has_default_metadata() {
    let mut t = Terminal::init(Global, opts(5, 3)).unwrap();

    t.switch_screen_mode(SwitchScreenMode::M1049, true).unwrap();

    // A soft-wrapped line across rows 0-1 and a prompt mark on row 0.
    for _ in 0..7 {
        t.print('A' as u32).unwrap();
    }
    unsafe {
        (*t.active_mut().pages.get_cell(active_pt(0, 0)).unwrap().row)
            .set_semantic_prompt(pagepkg::SemanticPrompt::Prompt)
    };

    // Scroll up: with no scrollback, row 0 is discarded and its Row
    // storage recycled as the new blank bottom row.
    t.scroll_up(1).unwrap();

    {
        let rac = t.active_mut().pages.get_cell(active_pt(0, 2)).unwrap();
        unsafe {
            assert!(!(*rac.row).wrap());
            assert!(!(*rac.row).wrap_continuation());
            assert_eq!(pagepkg::SemanticPrompt::None, (*rac.row).semantic_prompt());
        }
    }

    t.deinit(Global);
}

// test "Terminal: insertLines count over region blanks row metadata"
#[test]
fn terminal_insert_lines_count_over_region_blanks_row_metadata() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    // A soft-wrapped line across rows 0-2 so that row 1 has both wrap
    // flags set, plus a prompt mark on row 1.
    for _ in 0..12 {
        t.print('A' as u32).unwrap();
    }
    unsafe {
        (*t.active_mut().pages.get_cell(active_pt(0, 1)).unwrap().row)
            .set_semantic_prompt(pagepkg::SemanticPrompt::Prompt)
    };

    // Insert more lines than remain in the region: every row from the
    // cursor to the region bottom is blanked in place, with no shifts.
    t.set_cursor_pos(2, 1);
    t.insert_lines(10);

    for y in 1..5u32 {
        let rac = t.active_mut().pages.get_cell(active_pt(0, y)).unwrap();
        unsafe {
            assert!(!(*rac.row).wrap());
            assert!(!(*rac.row).wrap_continuation());
            assert_eq!(pagepkg::SemanticPrompt::None, (*rac.row).semantic_prompt());
        }
    }

    t.deinit(Global);
}

// test "Terminal: deleteLines count over region blanks row metadata"
#[test]
fn terminal_delete_lines_count_over_region_blanks_row_metadata() {
    let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

    for _ in 0..12 {
        t.print('A' as u32).unwrap();
    }
    unsafe {
        (*t.active_mut().pages.get_cell(active_pt(0, 1)).unwrap().row)
            .set_semantic_prompt(pagepkg::SemanticPrompt::Prompt)
    };

    t.set_cursor_pos(2, 1);
    t.delete_lines(10);

    for y in 1..5u32 {
        let rac = t.active_mut().pages.get_cell(active_pt(0, y)).unwrap();
        unsafe {
            assert!(!(*rac.row).wrap());
            assert!(!(*rac.row).wrap_continuation());
            assert_eq!(pagepkg::SemanticPrompt::None, (*rac.row).semantic_prompt());
        }
    }

    t.deinit(Global);
}

// test "Terminal: deleteLines blank row does not retain semantic prompt"
#[test]
fn terminal_delete_lines_blank_row_does_not_retain_semantic_prompt() {
    let mut t = Terminal::init(Global, opts(5, 3)).unwrap();

    // Mark row 0 as a prompt row, then delete it. The blank row that
    // appears at the region bottom reuses the deleted row's storage
    // and must not read as a prompt (e.g. for prompt navigation).
    t.print('$' as u32).unwrap();
    unsafe {
        (*t.active_mut().pages.get_cell(active_pt(0, 0)).unwrap().row)
            .set_semantic_prompt(pagepkg::SemanticPrompt::Prompt)
    };

    t.set_cursor_pos(1, 1);
    t.delete_lines(1);

    {
        let rac = t.active_mut().pages.get_cell(active_pt(0, 2)).unwrap();
        unsafe {
            assert!(!(*rac.row).wrap());
            assert!(!(*rac.row).wrap_continuation());
            assert_eq!(pagepkg::SemanticPrompt::None, (*rac.row).semantic_prompt());
        }
    }

    t.deinit(Global);
}

// test "Terminal: eraseDisplay complete ignores stale prompt on recycled row"
#[test]
fn terminal_erase_display_complete_ignores_stale_prompt_on_recycled_row() {
    let mut t = Terminal::init(Global, opts(10, 3)).unwrap();

    // Screen content that must NOT enter the scrollback on a clear.
    t.print_string(b"hello").unwrap();

    // Mark row 1 as a prompt row and then discard it with a region
    // scroll, recycling its storage as the blank bottom row.
    unsafe {
        (*t.active_mut().pages.get_cell(active_pt(0, 1)).unwrap().row)
            .set_semantic_prompt(pagepkg::SemanticPrompt::Prompt)
    };
    t.set_top_and_bottom_margin(2, 3);
    t.set_cursor_pos(3, 1);
    t.linefeed().unwrap();
    t.set_top_and_bottom_margin(0, 0);

    // ED2: since no prompt is on screen, this must NOT take the
    // scroll-and-clear path that pushes content into scrollback. A
    // stale prompt flag on the recycled blank bottom row would.
    t.erase_display(csi::EraseDisplay::Complete, false);

    assert_eq!(
        t.active_mut().pages.rows as usize,
        t.active_mut().pages.total_rows
    );

    t.deinit(Global);
}
