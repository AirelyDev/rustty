//! PORTS: ghostty src/terminal/c/terminal.zig — tests: `get` data arms,
//! default cursor style / resize-pull options, and `get_multi` (incl. the
//! scoped-log invalid-selector diagnostics).

use super::*;

// test "get cols and rows"
#[test]
fn get_cols_and_rows() {
    with_test_allocator(|alloc| {
        // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);

        let mut cols: size::CellCountInt = 0;
        let mut rows: size::CellCountInt = 0;
        assert_eq!(Result::Success, unsafe {
            get(t, TerminalData::Cols, &mut cols as *mut _ as *mut c_void)
        });
        assert_eq!(Result::Success, unsafe {
            get(t, TerminalData::Rows, &mut rows as *mut _ as *mut c_void)
        });
        assert_eq!(80, cols);
        assert_eq!(24, rows);
        unsafe { free(t) };
    });
}

// test "get cursor position"
#[test]
fn get_cursor_position() {
    with_test_allocator(|alloc| {
        // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);

        unsafe { vt_write(t, b"Hello".as_ptr(), 5) };

        let mut x: size::CellCountInt = 0;
        let mut y: size::CellCountInt = 0;
        assert_eq!(Result::Success, unsafe {
            get(t, TerminalData::CursorX, &mut x as *mut _ as *mut c_void)
        });
        assert_eq!(Result::Success, unsafe {
            get(t, TerminalData::CursorY, &mut y as *mut _ as *mut c_void)
        });
        assert_eq!(5, x);
        assert_eq!(0, y);
        unsafe { free(t) };
    });
}

// test "get vt_processing_error"
//
// Ported as written (parity tail S7-3): the active screen's allocator is
// swapped for `std.mem.Allocator.failing` (`rustty_vt::alloc::failing()`)
// around the write, as the reference swaps in `testing.failing_allocator`.
// (A `set_title` tripwire stood in for it while `title` lived on the Rust
// heap.)
#[test]
fn get_vt_processing_error() {
    with_test_allocator(|alloc| {
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);

        let mut processing_error: bool = true;
        assert_eq!(Result::Success, unsafe {
            get(
                t,
                TerminalData::VtProcessingError,
                &mut processing_error as *mut _ as *mut c_void,
            )
        });
        assert!(!processing_error);

        // Force a non-graceful terminal-owned update failure through the
        // public VT write path.
        {
            // const alloc = t.?.terminal.screens.active.alloc;
            // t.?.terminal.screens.active.alloc = testing.failing_allocator;
            // defer t.?.terminal.screens.active.alloc = alloc;
            // SAFETY: `t` is live; the active screen is a stable heap
            // pointer owned by the terminal, written through a raw place.
            let active = unsafe { (*zig_terminal(t)).screens.active };
            let alloc = unsafe { (*active).alloc };
            unsafe { (*active).alloc = rustty_vt::alloc::failing() };

            vtw(t, b"\x1B]2;unavailable\x1B\\");

            unsafe { (*active).alloc = alloc };
        }

        assert_eq!(Result::Success, unsafe {
            get(
                t,
                TerminalData::VtProcessingError,
                &mut processing_error as *mut _ as *mut c_void,
            )
        });
        assert!(processing_error);

        // rustty_terminal_reset() does NOT clear it.
        unsafe { reset(t) };
        assert_eq!(Result::Success, unsafe {
            get(
                t,
                TerminalData::VtProcessingError,
                &mut processing_error as *mut _ as *mut c_void,
            )
        });
        assert!(processing_error);

        unsafe { free(t) };
    });
}

// test "get null"
#[test]
fn get_null() {
    let mut cols: size::CellCountInt = 0;
    assert_eq!(Result::InvalidValue, unsafe {
        get(
            std::ptr::null_mut(),
            TerminalData::Cols,
            &mut cols as *mut _ as *mut c_void,
        )
    });
}

// test "get cursor_visible"
#[test]
fn get_cursor_visible() {
    with_test_allocator(|alloc| {
        // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);

        let mut visible: bool = false;
        assert_eq!(Result::Success, unsafe {
            get(
                t,
                TerminalData::CursorVisible,
                &mut visible as *mut bool as *mut c_void,
            )
        });
        assert!(visible);

        // DEC mode 25 controls cursor visibility
        // var config: ModeConfig = .{ .mode = @bitCast(modes.ModeTag{ .value = 25, .ansi = false }), .value = false };
        let config = ModeConfig {
            mode: modes::ModeTag {
                value: 25,
                ansi: false,
            }
            .to_backing(),
            value: false,
        };
        // try testing.expectEqual(Result.success, set(t, .mode, @ptrCast(&config)));
        assert_eq!(Result::Success, unsafe {
            set(
                t,
                Option_::Mode,
                &config as *const ModeConfig as *const c_void,
            )
        });
        assert_eq!(Result::Success, unsafe {
            get(
                t,
                TerminalData::CursorVisible,
                &mut visible as *mut bool as *mut c_void,
            )
        });
        assert!(!visible);
        unsafe { free(t) };
    });
}

// test "get cursor_at_prompt"
#[test]
fn get_cursor_at_prompt() {
    with_test_allocator(|alloc| {
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);

        // var at_prompt: bool = undefined;
        let mut at_prompt: bool = false;
        // try testing.expectEqual(Result.success, get(t, .cursor_at_prompt, @ptrCast(&at_prompt)));
        assert_eq!(Result::Success, unsafe {
            get(
                t,
                TerminalData::CursorAtPrompt,
                &mut at_prompt as *mut bool as *mut c_void,
            )
        });
        // try testing.expect(!at_prompt);
        assert!(!at_prompt);

        // const prompt_start = "\x1b]133;A\x07";
        let prompt_start = b"\x1b]133;A\x07";
        unsafe { vt_write(t, prompt_start.as_ptr(), prompt_start.len()) };
        assert_eq!(Result::Success, unsafe {
            get(
                t,
                TerminalData::CursorAtPrompt,
                &mut at_prompt as *mut bool as *mut c_void,
            )
        });
        // try testing.expect(at_prompt);
        assert!(at_prompt);

        // const alternate_screen = "\x1b[?1049h";
        let alternate_screen = b"\x1b[?1049h";
        unsafe { vt_write(t, alternate_screen.as_ptr(), alternate_screen.len()) };
        assert_eq!(Result::Success, unsafe {
            get(
                t,
                TerminalData::CursorAtPrompt,
                &mut at_prompt as *mut bool as *mut c_void,
            )
        });
        // try testing.expect(!at_prompt);
        assert!(!at_prompt);
        unsafe { free(t) };
    });
}

// test "get active_screen"
#[test]
fn get_active_screen() {
    with_test_allocator(|alloc| {
        // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);

        let mut screen: TerminalScreen = TerminalScreen::Primary;
        assert_eq!(Result::Success, unsafe {
            get(
                t,
                TerminalData::ActiveScreen,
                &mut screen as *mut TerminalScreen as *mut c_void,
            )
        });
        assert_eq!(TerminalScreen::Primary, screen);
        unsafe { free(t) };
    });
}

// test "get kitty_keyboard_flags"
#[test]
fn get_kitty_keyboard_flags() {
    with_test_allocator(|alloc| {
        // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);

        let mut flags: u8 = 0;
        assert_eq!(Result::Success, unsafe {
            get(
                t,
                TerminalData::KittyKeyboardFlags,
                &mut flags as *mut u8 as *mut c_void,
            )
        });
        assert_eq!(0, flags);

        // Push kitty flags via VT sequence: CSI > 3 u (push disambiguate | report_events)
        unsafe { vt_write(t, b"\x1b[>3u".as_ptr(), 5) };

        assert_eq!(Result::Success, unsafe {
            get(
                t,
                TerminalData::KittyKeyboardFlags,
                &mut flags as *mut u8 as *mut c_void,
            )
        });
        assert_eq!(3, flags);
        unsafe { free(t) };
    });
}

// test "get mouse_tracking"
#[test]
fn get_mouse_tracking() {
    with_test_allocator(|alloc| {
        // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);

        let mut tracking: bool = false;
        let get_tracking = |t: Terminal, tracking: &mut bool| unsafe {
            get(
                t,
                TerminalData::MouseTracking,
                tracking as *mut bool as *mut c_void,
            )
        };
        assert_eq!(Result::Success, get_tracking(t, &mut tracking));
        assert!(!tracking);

        // PORT NOTE (cfc19e805): mode writes go through `set(.mode)` with a
        // `ModeConfig`; this closure is the `set(t, .mode, @ptrCast(&config))`
        // call shared by every step below.
        let set_mode = |t: Terminal, value: u16, on: bool| {
            let config = ModeConfig {
                mode: modes::ModeTag { value, ansi: false }.to_backing(),
                value: on,
            };
            unsafe {
                set(
                    t,
                    Option_::Mode,
                    &config as *const ModeConfig as *const c_void,
                )
            }
        };

        // Enable X10 mouse (DEC mode 9)
        // var config: ModeConfig = .{ .mode = @bitCast(modes.ModeTag{ .value = 9, .ansi = false }), .value = true };
        assert_eq!(Result::Success, set_mode(t, 9, true));
        assert_eq!(Result::Success, get_tracking(t, &mut tracking));
        assert!(tracking);

        // Disable X10, enable normal mouse (DEC mode 1000)
        assert_eq!(Result::Success, set_mode(t, 9, false));
        assert_eq!(Result::Success, set_mode(t, 1000, true));
        assert_eq!(Result::Success, get_tracking(t, &mut tracking));
        assert!(tracking);

        // Disable normal, enable button mouse (DEC mode 1002)
        assert_eq!(Result::Success, set_mode(t, 1000, false));
        assert_eq!(Result::Success, set_mode(t, 1002, true));
        assert_eq!(Result::Success, get_tracking(t, &mut tracking));
        assert!(tracking);

        // Disable button, enable any mouse (DEC mode 1003)
        assert_eq!(Result::Success, set_mode(t, 1002, false));
        assert_eq!(Result::Success, set_mode(t, 1003, true));
        assert_eq!(Result::Success, get_tracking(t, &mut tracking));
        assert!(tracking);

        // Disable all - should be false again
        assert_eq!(Result::Success, set_mode(t, 1003, false));
        assert_eq!(Result::Success, get_tracking(t, &mut tracking));
        assert!(!tracking);
        unsafe { free(t) };
    });
}

// test "get total_rows"
#[test]
fn get_total_rows() {
    with_test_allocator(|alloc| {
        // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);

        let mut total: usize = 0;
        assert_eq!(Result::Success, unsafe {
            get(
                t,
                TerminalData::TotalRows,
                &mut total as *mut usize as *mut c_void,
            )
        });
        assert!(total >= 24);
        unsafe { free(t) };
    });
}

// test "get scrollback_rows"
#[test]
fn get_scrollback_rows() {
    with_test_allocator(|alloc| {
        // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
        let (new_result, t) = new_t(alloc, 80, 3);
        assert_eq!(Result::Success, new_result);

        let mut scrollback: usize = 0;
        assert_eq!(Result::Success, unsafe {
            get(
                t,
                TerminalData::ScrollbackRows,
                &mut scrollback as *mut usize as *mut c_void,
            )
        });
        assert_eq!(0usize, scrollback);

        // Write enough lines to push content into scrollback
        unsafe {
            vt_write(
                t,
                b"line1\r\nline2\r\nline3\r\nline4\r\nline5\r\n".as_ptr(),
                34,
            )
        };

        assert_eq!(Result::Success, unsafe {
            get(
                t,
                TerminalData::ScrollbackRows,
                &mut scrollback as *mut usize as *mut c_void,
            )
        });
        assert_eq!(2usize, scrollback);
        unsafe { free(t) };
    });
}

// test "get invalid"
#[test]
fn get_invalid() {
    with_test_allocator(|alloc| {
        // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);

        assert_eq!(Result::InvalidValue, unsafe {
            get(t, TerminalData::Invalid, std::ptr::null_mut())
        });
        unsafe { free(t) };
    });
}

// test "get_multi null keys returns invalid_value"
#[test]
fn get_multi_null_keys_returns_invalid_value() {
    with_test_allocator(|alloc| {
        // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);

        let mut cols: size::CellCountInt = 0;
        let mut values: [*mut c_void; 1] = [&mut cols as *mut _ as *mut c_void];
        assert_eq!(Result::InvalidValue, unsafe {
            get_multi(
                t,
                1,
                std::ptr::null(),
                values.as_mut_ptr(),
                std::ptr::null_mut(),
            )
        });
        unsafe { free(t) };
    });
}

// test "get mouse_shape"
#[test]
fn get_mouse_shape() {
    with_test_allocator(|alloc| {
        // try testing.expectEqual(Result.success, new(&lib.alloc.test_allocator, &t, 80, 24));
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);

        // var shape: mouse.Shape = undefined;
        let mut shape: rustty_vt::mouse::Shape = rustty_vt::mouse::Shape::Default;
        // try testing.expectEqual(Result.success, get(t, .mouse_shape, @ptrCast(&shape)));
        assert_eq!(Result::Success, unsafe {
            get(
                t,
                TerminalData::MouseShape,
                &mut shape as *mut rustty_vt::mouse::Shape as *mut c_void,
            )
        });
        // try testing.expectEqual(mouse.Shape.text, shape);
        assert_eq!(rustty_vt::mouse::Shape::Text, shape);

        // const cases = .{ ... };
        let cases: &[(&[u8], rustty_vt::mouse::Shape)] = &[
            (b"\x1b]22;pointer\x07", rustty_vt::mouse::Shape::Pointer),
            (
                b"\x1b]22;crosshair\x1b\\",
                rustty_vt::mouse::Shape::Crosshair,
            ),
            // Invalid names leave the last accepted shape unchanged.
            (
                b"\x1b]22;not-a-pointer-shape\x07",
                rustty_vt::mouse::Shape::Crosshair,
            ),
            // Hyperlinks don't override the application's requested shape.
            (
                b"\x1b]8;;https://example.com\x1b\\link\x1b]8;;\x1b\\",
                rustty_vt::mouse::Shape::Crosshair,
            ),
            // .{ "\x1b]22;\x1b\\", mouse.Shape.text },
            (b"\x1b]22;\x1b\\", rustty_vt::mouse::Shape::Text),
            (b"\x1b]22;default\x07", rustty_vt::mouse::Shape::Default),
            (b"\x1b]22;text\x07", rustty_vt::mouse::Shape::Text),
        ];
        // inline for (cases) |case| { ... }
        for (data, want) in cases {
            unsafe { vt_write(t, data.as_ptr(), data.len()) };
            assert_eq!(Result::Success, unsafe {
                get(
                    t,
                    TerminalData::MouseShape,
                    &mut shape as *mut rustty_vt::mouse::Shape as *mut c_void,
                )
            });
            assert_eq!(*want, shape);
        }

        // An incomplete OSC must not update the shape before its terminator.
        let prefix = b"\x1b]22;wait";
        unsafe { vt_write(t, prefix.as_ptr(), prefix.len()) };
        assert_eq!(Result::Success, unsafe {
            get(
                t,
                TerminalData::MouseShape,
                &mut shape as *mut rustty_vt::mouse::Shape as *mut c_void,
            )
        });
        assert_eq!(rustty_vt::mouse::Shape::Text, shape);
        let terminator = b"\x07";
        unsafe { vt_write(t, terminator.as_ptr(), terminator.len()) };
        assert_eq!(Result::Success, unsafe {
            get(
                t,
                TerminalData::MouseShape,
                &mut shape as *mut rustty_vt::mouse::Shape as *mut c_void,
            )
        });
        assert_eq!(rustty_vt::mouse::Shape::Wait, shape);
        unsafe { free(t) };
    });
}

// test "set default cursor style and blink"
#[test]
fn set_default_cursor_style_and_blink() {
    with_test_allocator(|alloc| {
        // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);

        // var default_style: TerminalCursorStyle = .bar;
        let mut default_style = TerminalCursorStyle::Bar;
        // var default_blink = true;
        let mut default_blink = true;
        assert_eq!(Result::Success, unsafe {
            set(
                t,
                Option_::DefaultCursorStyle,
                &mut default_style as *mut _ as *const c_void,
            )
        });
        assert_eq!(Result::Success, unsafe {
            set(
                t,
                Option_::DefaultCursorBlink,
                &mut default_blink as *mut _ as *const c_void,
            )
        });

        // Setting defaults applies them immediately while the cursor is still default.
        assert_eq!(rustty_vt::screen::CursorStyle::Bar, unsafe {
            (*(*(*t).terminal).screens.active).cursor.cursor_style
        });
        assert!(unsafe { (*(*t).terminal).modes.get(modes::Mode::CursorBlinking) });

        // An explicit DECSCUSR style overrides the configured defaults.
        vtw(t, b"\x1b[2 q");
        assert_eq!(rustty_vt::screen::CursorStyle::Block, unsafe {
            (*(*(*t).terminal).screens.active).cursor.cursor_style
        });
        assert!(!unsafe { (*(*t).terminal).modes.get(modes::Mode::CursorBlinking) });

        // Changing defaults does not override an explicit cursor style.
        default_style = TerminalCursorStyle::Underline;
        assert_eq!(Result::Success, unsafe {
            set(
                t,
                Option_::DefaultCursorStyle,
                &mut default_style as *mut _ as *const c_void,
            )
        });
        assert_eq!(rustty_vt::screen::CursorStyle::Block, unsafe {
            (*(*(*t).terminal).screens.active).cursor.cursor_style
        });
        assert!(!unsafe { (*(*t).terminal).modes.get(modes::Mode::CursorBlinking) });

        // DECSCUSR reset restores the configured default style and blink.
        vtw(t, b"\x1b[0 q");
        assert_eq!(rustty_vt::screen::CursorStyle::Underline, unsafe {
            (*(*(*t).terminal).screens.active).cursor.cursor_style
        });
        assert!(unsafe { (*(*t).terminal).modes.get(modes::Mode::CursorBlinking) });

        // RIS also restores cursor defaults from Terminal-owned state.
        vtw(t, b"\x1b[2 q");
        unsafe { reset(t) };
        assert_eq!(rustty_vt::screen::CursorStyle::Underline, unsafe {
            (*(*(*t).terminal).screens.active).cursor.cursor_style
        });
        assert!(unsafe { (*(*t).terminal).modes.get(modes::Mode::CursorBlinking) });
        unsafe { free(t) };
    });
}

// Rust-only regression test (no oracle counterpart): the oracle's
// `TerminalCursorStyle` is a non-exhaustive `enum(c_int)`, so an unknown
// discriminant reaches `toZig`'s `_ => null` arm and yields
// `.invalid_value`. The header exposes
// `RUSTTY_TERMINAL_CURSOR_STYLE_MAX_VALUE` (= INT_MAX), so a conforming
// caller can send one. This pins that the closed Rust enum never sees such
// a value (which would be UB) and that the option is rejected, not applied.
#[test]
fn set_default_cursor_style_rejects_unknown_enum_values() {
    with_test_allocator(|alloc| {
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);

        // Control: a declared enumerator is accepted and applied.
        let mut style = TerminalCursorStyle::Underline;
        assert_eq!(Result::Success, unsafe {
            set(
                t,
                Option_::DefaultCursorStyle,
                &mut style as *mut _ as *const c_void,
            )
        });
        assert_eq!(rustty_vt::screen::CursorStyle::Underline, unsafe {
            (*(*(*t).terminal).screens.active).cursor.cursor_style
        });

        // Out-of-range discriminants: the header sentinel and the first
        // unnamed value. Both are rejected and change nothing.
        for raw in [c_int::MAX, 4, -1] {
            let mut raw = raw;
            assert_eq!(Result::InvalidValue, unsafe {
                set(
                    t,
                    Option_::DefaultCursorStyle,
                    &mut raw as *mut c_int as *const c_void,
                )
            });
            assert_eq!(rustty_vt::screen::CursorStyle::Underline, unsafe {
                (*(*(*t).terminal).screens.active).cursor.cursor_style
            });
        }

        unsafe { free(t) };
    });
}

// test "set resize pull scrollback"
#[test]
fn set_resize_pull_scrollback() {
    with_test_allocator(|alloc| {
        // var t: Terminal = null;
        // try testing.expectEqual(Result.success, new(&lib.alloc.test_allocator, &t, 80, 24));
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);
        // defer free(t);
        // try testing.expect(t.?.terminal.flags.resize_pull_scrollback);
        assert!(unsafe { (*(*t).terminal).flags.resize_pull_scrollback });

        // const disabled = false;
        let disabled: bool = false;
        // try testing.expectEqual(Result.success, set(t, .resize_pull_scrollback, &disabled));
        assert_eq!(Result::Success, unsafe {
            set(
                t,
                Option_::ResizePullScrollback,
                &disabled as *const bool as *const c_void,
            )
        });
        // try testing.expect(!t.?.terminal.flags.resize_pull_scrollback);
        assert!(!unsafe { (*(*t).terminal).flags.resize_pull_scrollback });

        // NULL restores the default.
        // try testing.expectEqual(Result.success, set(t, .resize_pull_scrollback, null));
        assert_eq!(Result::Success, unsafe {
            set(t, Option_::ResizePullScrollback, std::ptr::null())
        });
        // try testing.expect(t.?.terminal.flags.resize_pull_scrollback);
        assert!(unsafe { (*(*t).terminal).flags.resize_pull_scrollback });

        unsafe { free(t) };
    });
}

// test "get_multi success"
#[test]
fn get_multi_success() {
    with_test_allocator(|alloc| {
        // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);

        let mut cols: u16 = 0;
        let mut rows: u16 = 0;
        let mut written: usize = 0;

        let keys = [TerminalData::Cols, TerminalData::Rows];
        let mut values: [*mut c_void; 2] = [
            &mut cols as *mut _ as *mut c_void,
            &mut rows as *mut _ as *mut c_void,
        ];
        assert_eq!(Result::Success, unsafe {
            get_multi(
                t,
                keys.len(),
                // PORT NOTE (W35-B08): the `keys` array crosses the C
                // boundary as raw ints; `get_multi` reads each element as a
                // `c_int` and range-checks it.
                keys.as_ptr().cast(),
                values.as_mut_ptr(),
                &mut written,
            )
        });
        assert_eq!(keys.len(), written);
        assert_eq!(80, cols);
        assert_eq!(24, rows);
        unsafe { free(t) };
    });
}

// test "get_multi error sets out_written"
#[test]
fn get_multi_error_sets_out_written() {
    with_test_allocator(|alloc| {
        // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);

        let mut cols: u16 = 0;
        let mut written: usize = 99;

        let keys = [TerminalData::Cols, TerminalData::Invalid];
        // SB: build the two out-pointers with `addr_of_mut!` (raw), NOT
        // `&mut cols as *mut _`. Both entries point at the same `cols` (the
        // reference test passes `&cols` twice — raw C pointers); a second
        // `&mut cols` reborrow retags the whole variable and pops the first
        // pointer off the borrow stack, so `get_multi`'s write through
        // `values[0]` would be UB (rust-mut-args-strongly-protected).
        let mut values: [*mut c_void; 2] = [
            core::ptr::addr_of_mut!(cols) as *mut c_void,
            core::ptr::addr_of_mut!(cols) as *mut c_void,
        ];
        assert_eq!(Result::InvalidValue, unsafe {
            get_multi(
                t,
                keys.len(),
                // PORT NOTE (W35-B08): the `keys` array crosses the C
                // boundary as raw ints; `get_multi` reads each element as a
                // `c_int` and range-checks it.
                keys.as_ptr().cast(),
                values.as_mut_ptr(),
                &mut written,
            )
        });
        assert_eq!(1, written);
        assert_eq!(80, cols);
        unsafe { free(t) };
    });
}

// W35-B08-01 (Rust-only proving test; the reference's guard is a
// `runtime_safety` block, so it has no unit test for it): a `get_multi`
// KEY ARRAY is C memory. Before the fix the wrapper loaded each element
// straight into a `TerminalData`, which materializes an out-of-range
// discriminant — UB in Rust in EVERY build mode. The reference reaches its
// `std.enums.fromInt` guard per element (its `get_multi` calls `get`), so
// the parity disposition is `.invalid_value` + the failing index.
#[test]
fn get_multi_out_of_range_raw_key_is_rejected_with_the_failing_index() {
    with_test_allocator(|alloc| {
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);

        let mut cols: u16 = 0;
        let mut rows: u16 = 0;
        let mut written: usize = 99;

        // Element 1 is a raw C integer no `TerminalData` tag can hold.
        let keys: [c_int; 3] = [TerminalData::Cols as c_int, 99, TerminalData::Rows as c_int];
        // SB: raw out-pointers (see `get_multi_error_sets_out_written`).
        let mut values: [*mut c_void; 3] = [
            core::ptr::addr_of_mut!(cols) as *mut c_void,
            core::ptr::addr_of_mut!(rows) as *mut c_void,
            core::ptr::addr_of_mut!(rows) as *mut c_void,
        ];
        assert_eq!(Result::InvalidValue, unsafe {
            get_multi(
                t,
                keys.len(),
                keys.as_ptr(),
                values.as_mut_ptr(),
                &mut written,
            )
        });
        // The failing index, exactly as the reference's `get_multi` reports.
        assert_eq!(1, written);
        // Element 0 ran to completion before the guard tripped.
        assert_eq!(80, cols);
        // Element 2 was never reached.
        assert_eq!(0, rows);

        // The same array through the published `extern "C"` entry point.
        written = 99;
        assert_eq!(Result::InvalidValue.to_c_int(), unsafe {
            crate::rustty_terminal_get_multi(
                t,
                keys.len(),
                keys.as_ptr().cast(),
                values.as_mut_ptr(),
                &mut written,
            )
        });
        assert_eq!(1, written);

        unsafe { free(t) };
    });
}

// W35-B03/B06/B08-02 (Rust-only proving test): the invalid-selector
// diagnostics are the reference's `log.warn` on the `.terminal_c` /
// `.selection_c` scopes, which `terminal/c/sys.zig`'s `logFn` routes to the
// installed C log callback and DROPS when none is installed
// (`if (global.log == null) return;`). They must never go to stderr.
mod log_capture {
    use crate::sys::LogLevel;
    use std::ffi::c_void;
    use std::sync::Mutex;

    pub(super) static CAPTURED: Mutex<Vec<(LogLevel, String, String)>> = Mutex::new(Vec::new());

    pub(super) extern "C" fn log_cb(
        _userdata: *mut c_void,
        level: LogLevel,
        scope_ptr: *const u8,
        scope_len: usize,
        message_ptr: *const u8,
        message_len: usize,
    ) {
        // SAFETY: the sink hands us the scope/message byte ranges.
        let scope = unsafe { std::slice::from_raw_parts(scope_ptr, scope_len) };
        // SAFETY: as above.
        let message = unsafe { std::slice::from_raw_parts(message_ptr, message_len) };
        CAPTURED.lock().unwrap().push((
            level,
            String::from_utf8_lossy(scope).into_owned(),
            String::from_utf8_lossy(message).into_owned(),
        ));
    }
}

#[test]
fn terminal_invalid_selector_warns_through_the_scoped_log_sink() {
    // Serialize against the other tests that mutate the process-wide sys
    // GLOBAL (the sys tests and the render-state scoped-log test).
    let _guard = crate::sys::tests::reset_global();
    log_capture::CAPTURED.lock().unwrap().clear();

    // A selector value used by no other test, so the capture can be
    // filtered: these tests run in parallel and other terminal tests also
    // trip invalid-selector warnings while this callback is installed.
    const SEL: c_int = 31338;
    const C_INVALID_VALUE: c_int = -2;

    // No callback installed: silently discarded (the C-ABI contract), and
    // the call still reports `.invalid_value`.
    assert_eq!(C_INVALID_VALUE, unsafe {
        crate::rustty_terminal_set(std::ptr::null_mut(), SEL, std::ptr::null())
    });
    assert_eq!(C_INVALID_VALUE, unsafe {
        crate::rustty_terminal_get(std::ptr::null_mut(), SEL, std::ptr::null_mut())
    });
    assert!(log_capture::CAPTURED.lock().unwrap().is_empty());

    // Install a capturing callback.
    assert_eq!(Result::Success, unsafe {
        crate::sys::set(
            crate::sys::Option_::Log,
            log_capture::log_cb as *const c_void,
        )
    });

    assert_eq!(C_INVALID_VALUE, unsafe {
        crate::rustty_terminal_set(std::ptr::null_mut(), SEL, std::ptr::null())
    });
    assert_eq!(C_INVALID_VALUE, unsafe {
        crate::rustty_terminal_get(std::ptr::null_mut(), SEL, std::ptr::null_mut())
    });
    // The `get_multi` key array reaches the same guard, per element.
    let keys: [c_int; 1] = [SEL];
    let mut values: [*mut c_void; 1] = [std::ptr::null_mut()];
    let mut written: usize = 99;
    assert_eq!(C_INVALID_VALUE, unsafe {
        crate::rustty_terminal_get_multi(
            std::ptr::null_mut(),
            1,
            keys.as_ptr().cast(),
            values.as_mut_ptr(),
            &mut written,
        )
    });
    assert_eq!(0, written);
    // `selection_c`-scoped guards.
    assert_eq!(C_INVALID_VALUE, unsafe {
        crate::rustty_terminal_selection_adjust(std::ptr::null_mut(), std::ptr::null_mut(), SEL)
    });
    assert_eq!(C_INVALID_VALUE, unsafe {
        crate::rustty_terminal_selection_ordered(
            std::ptr::null_mut(),
            std::ptr::null(),
            SEL,
            std::ptr::null_mut(),
        )
    });

    // Clear the callback before asserting, so a failure cannot strand it.
    assert_eq!(Result::Success, unsafe {
        crate::sys::set(crate::sys::Option_::Log, std::ptr::null())
    });

    let all = log_capture::CAPTURED.lock().unwrap().clone();
    // FLAKE FIX (task 15, N28): this loop used to run over EVERY captured
    // entry. `reset_global()` serializes the tests that MUTATE the sys
    // global, but nothing stops a concurrently running test from EMITTING a
    // diagnostic while this callback is installed, so a `debug`-level line
    // from elsewhere in the binary would fail this test for a reason it does
    // not control. Assert over the entries THIS test caused — which is why
    // the unique SEL two lines above exists. Latent here (0 failures in 15
    // runs), live in the two siblings this matches byte for byte:
    // `render::tests::render_invalid_selector_warns_through_the_scoped_log_sink`
    // and `log_root_tests::root_wrapper_invalid_selectors_warn_through_the_c_log_callback`.
    let mine: Vec<&(crate::sys::LogLevel, String, String)> = all
        .iter()
        .filter(|(_, _, m)| m.ends_with("=31338"))
        .collect();
    for (level, _, _) in mine.iter() {
        assert_eq!(crate::sys::LogLevel::Warning, *level);
    }
    let captured: Vec<(&String, &String)> = mine.iter().map(|(_, s, m)| (s, m)).collect();
    let expected = [
        (LOG_SCOPE, "terminal_set invalid option value=31338"),
        (LOG_SCOPE, "terminal_get invalid data value=31338"),
        // `get_multi`'s per-element guard is `get`'s, so is its message.
        (LOG_SCOPE, "terminal_get invalid data value=31338"),
        (
            crate::selection::LOG_SCOPE,
            "terminal_selection_adjust invalid adjustment value=31338",
        ),
        (
            crate::selection::LOG_SCOPE,
            "terminal_selection_ordered invalid desired value=31338",
        ),
    ];
    assert_eq!(expected.len(), captured.len(), "captured: {all:?}");
    for (i, (scope, message)) in captured.iter().enumerate() {
        assert_eq!(expected[i].0, scope.as_str());
        assert_eq!(expected[i].1, message.as_str());
    }
}

// Rust-only (P8-W5 T3, NEW-FINDINGS N550): the C getter's `c_int` guard
// accepts every declared `RusttyTerminalData` value, as the reference's
// `std.enums.fromInt` does, and nothing past the last one. The guard's upper
// bound used to stop at 40, so `RUSTTY_TERMINAL_DATA_MOUSE_SHAPE` (41) was
// rejected with INVALID_VALUE by `rustty_terminal_get`.
#[test]
fn get_checked_accepts_every_declared_data_value() {
    for v in 0..=42 {
        let d = TerminalData::from_c_int(v);
        assert_eq!(Some(v), d.map(|d| d as c_int), "data value {v}");
    }
    for v in [43, -1, c_int::MAX] {
        assert!(TerminalData::from_c_int(v).is_none(), "data value {v}");
    }

    with_test_allocator(|alloc| {
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);
        let mut shape = rustty_vt::mouse::Shape::Default;
        assert_eq!(Result::Success, unsafe {
            get_checked(
                t,
                TerminalData::MouseShape as c_int,
                (&raw mut shape).cast(),
            )
        });
        assert_eq!(rustty_vt::mouse::Shape::Text, shape);
        unsafe { free(t) };
    });
}

// test "get memory_usage"
#[test]
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION (rustty-vt-c, P8-W5 T3): 4,000 lines of history, then a full compression of every page; measured TIMEOUT at the 30-minute miri-run.sh bound; runs natively and under ASan, and get_memory_usage_size_rule plus the memory_usage re-entry test cover the getter under Miri (see MIRI_EXCEPTIONS.md)"
)]
fn get_memory_usage() {
    with_test_allocator(|alloc| {
        // var t: Terminal = null;
        // try testing.expectEqual(Result.success, new(&lib.alloc.test_allocator, &t, 80, 24));
        // defer free(t);
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);
        let get_usage = |out: &mut TerminalMemoryUsage| unsafe {
            get(
                t,
                TerminalData::MemoryUsage,
                (out as *mut TerminalMemoryUsage).cast(),
            )
        };

        // var fresh: TerminalMemoryUsage = .{};
        // try testing.expectEqual(Result.success, get(t, .memory_usage, @ptrCast(&fresh)));
        let mut fresh = TerminalMemoryUsage::default();
        assert_eq!(Result::Success, get_usage(&mut fresh));
        // try testing.expectEqual(@sizeOf(TerminalMemoryUsage), fresh.size);
        assert_eq!(size_of::<TerminalMemoryUsage>(), fresh.size);
        // try testing.expectEqual(terminal_mem.canReclaim(.strict), fresh.compression_supported);
        assert_eq!(
            rustty_vt::mem::can_reclaim(rustty_vt::mem::DecommitMode::Strict),
            fresh.compression_supported
        );
        // try testing.expect(fresh.primary_pages > 0);
        assert!(fresh.primary_pages > 0);
        // try testing.expect(fresh.primary_virtual_bytes > 0);
        assert!(fresh.primary_virtual_bytes > 0);
        // try testing.expect(fresh.primary_resident_bytes <= fresh.primary_virtual_bytes);
        assert!(fresh.primary_resident_bytes <= fresh.primary_virtual_bytes);
        // try testing.expectEqual(@as(u64, 0), fresh.primary_compressed_pages);
        assert_eq!(0u64, fresh.primary_compressed_pages);
        // try testing.expectEqual(@as(u64, 0), fresh.primary_compressed_bytes);
        assert_eq!(0u64, fresh.primary_compressed_bytes);
        // try testing.expectEqual(@as(u64, 0), fresh.primary_image_bytes);
        assert_eq!(0u64, fresh.primary_image_bytes);

        // The alternate screen doesn't exist yet.
        assert_eq!(0u64, fresh.alternate_pages);
        assert_eq!(0u64, fresh.alternate_virtual_bytes);
        assert_eq!(0u64, fresh.alternate_resident_bytes);
        assert_eq!(0u64, fresh.alternate_compressed_pages);
        assert_eq!(0u64, fresh.alternate_compressed_bytes);
        assert_eq!(0u64, fresh.alternate_image_bytes);

        // Write compressible history.
        // const line = "repeated and compressible terminal history\r\n";
        // const repeat = 4_000;
        // ... vt_write(t, input.ptr, input.len);
        let line: &[u8] = b"repeated and compressible terminal history\r\n";
        let repeat = 4_000;
        let input = line.repeat(repeat);
        vtw(t, &input);

        // var history: TerminalMemoryUsage = .{};
        // try testing.expectEqual(Result.success, get(t, .memory_usage, @ptrCast(&history)));
        let mut history = TerminalMemoryUsage::default();
        assert_eq!(Result::Success, get_usage(&mut history));
        // try testing.expect(history.primary_pages > fresh.primary_pages);
        assert!(history.primary_pages > fresh.primary_pages);
        // try testing.expect(history.primary_resident_bytes > fresh.primary_resident_bytes);
        assert!(history.primary_resident_bytes > fresh.primary_resident_bytes);

        // var compression_result: CompressionResult = undefined;
        // try testing.expectEqual(Result.success, compress(t, @intFromEnum(CompressionMode.full), &compression_result));
        let mut compression_result = CompressionResult::Unsupported;
        assert_eq!(Result::Success, unsafe {
            compress(t, CompressionMode::Full as c_int, &mut compression_result)
        });
        // try testing.expectEqual(history.compression_supported, compression_result == .complete);
        assert_eq!(
            history.compression_supported,
            compression_result == CompressionResult::Complete
        );

        // var compressed: TerminalMemoryUsage = .{};
        // try testing.expectEqual(Result.success, get(t, .memory_usage, @ptrCast(&compressed)));
        let mut compressed = TerminalMemoryUsage::default();
        assert_eq!(Result::Success, get_usage(&mut compressed));
        // try testing.expectEqual(history.primary_pages, compressed.primary_pages);
        assert_eq!(history.primary_pages, compressed.primary_pages);
        // try testing.expectEqual(history.primary_virtual_bytes, compressed.primary_virtual_bytes);
        assert_eq!(
            history.primary_virtual_bytes,
            compressed.primary_virtual_bytes
        );
        // if (compressed.compression_supported) { ... } else { ... }
        if compressed.compression_supported {
            assert!(compressed.primary_compressed_pages > 0);
            assert!(compressed.primary_compressed_bytes > 0);
            assert!(compressed.primary_resident_bytes < history.primary_resident_bytes);
        } else {
            assert_eq!(0u64, compressed.primary_compressed_pages);
        }

        // The query never restores a compressed page.
        // var again: TerminalMemoryUsage = .{};
        // try testing.expectEqual(Result.success, get(t, .memory_usage, @ptrCast(&again)));
        // try testing.expectEqual(compressed, again);
        let mut again = TerminalMemoryUsage::default();
        assert_eq!(Result::Success, get_usage(&mut again));
        assert_eq!(compressed, again);

        // Kitty images are counted separately from pages.
        // if (comptime build_options.kitty_graphics) { ... }
        if cfg!(feature = "kitty-graphics") {
            // 1x2 RGB image, 6 bytes of pixel data.
            let transmit = b"\x1b_Ga=t,t=d,f=24,i=1,s=1,v=2;////////\x1b\\";
            vtw(t, transmit);
            let mut images = TerminalMemoryUsage::default();
            assert_eq!(Result::Success, get_usage(&mut images));
            assert!(images.primary_image_bytes > 0);

            let delete = b"\x1b_Ga=d,d=I,i=1\x1b\\";
            vtw(t, delete);
            assert_eq!(Result::Success, get_usage(&mut images));
            assert_eq!(0u64, images.primary_image_bytes);
        }

        // Entering the alternate screen creates it.
        // vt_write(t, "\x1b[?1049h", 8);
        vtw(t, b"\x1b[?1049h");
        let mut alternate = TerminalMemoryUsage::default();
        assert_eq!(Result::Success, get_usage(&mut alternate));
        assert!(alternate.alternate_pages > 0);
        assert!(alternate.alternate_virtual_bytes > 0);
        assert!(alternate.alternate_resident_bytes > 0);
        assert!(alternate.alternate_resident_bytes <= alternate.alternate_virtual_bytes);
        assert_eq!(compressed.primary_pages, alternate.primary_pages);

        unsafe { free(t) };
    });
}

// test "get memory_usage size rule"
#[test]
fn get_memory_usage_size_rule() {
    with_test_allocator(|alloc| {
        // var t: Terminal = null;
        // try testing.expectEqual(Result.success, new(&lib.alloc.test_allocator, &t, 80, 24));
        // defer free(t);
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);

        // A size smaller than the first layout is rejected and nothing is
        // written.
        // var small: TerminalMemoryUsage = .{
        //     .size = @sizeOf(TerminalMemoryUsage) - 1,
        //     .primary_pages = 1234,
        // };
        let mut small = TerminalMemoryUsage {
            size: size_of::<TerminalMemoryUsage>() - 1,
            primary_pages: 1234,
            ..TerminalMemoryUsage::default()
        };
        // try testing.expectEqual(Result.invalid_value, get(t, .memory_usage, @ptrCast(&small)));
        assert_eq!(Result::InvalidValue, unsafe {
            get(t, TerminalData::MemoryUsage, (&raw mut small).cast())
        });
        // try testing.expectEqual(@sizeOf(TerminalMemoryUsage) - 1, small.size);
        assert_eq!(size_of::<TerminalMemoryUsage>() - 1, small.size);
        // try testing.expectEqual(@as(u64, 1234), small.primary_pages);
        assert_eq!(1234u64, small.primary_pages);

        // A size larger than ours, from a caller built against a newer
        // layout, gets the known prefix and the rest is left untouched.
        // const Larger = extern struct {
        //     usage: TerminalMemoryUsage,
        //     extra: u64,
        // };
        #[repr(C)]
        struct Larger {
            usage: TerminalMemoryUsage,
            extra: u64,
        }
        // var large: Larger = .{
        //     .usage = .{ .size = @sizeOf(Larger) },
        //     .extra = 0xDEADBEEF,
        // };
        let mut large = Larger {
            usage: TerminalMemoryUsage {
                size: size_of::<Larger>(),
                ..TerminalMemoryUsage::default()
            },
            extra: 0xDEADBEEF,
        };
        // try testing.expectEqual(Result.success, get(t, .memory_usage, @ptrCast(&large)));
        assert_eq!(Result::Success, unsafe {
            get(t, TerminalData::MemoryUsage, (&raw mut large).cast())
        });
        // try testing.expectEqual(@sizeOf(Larger), large.usage.size);
        assert_eq!(size_of::<Larger>(), large.usage.size);
        // try testing.expect(large.usage.primary_pages > 0);
        assert!(large.usage.primary_pages > 0);
        // try testing.expectEqual(@as(u64, 0xDEADBEEF), large.extra);
        assert_eq!(0xDEADBEEFu64, large.extra);

        unsafe { free(t) };
    });
}

// Rust-only (P8-W5 T3 fix 1; the setter twin of N550): the C setter's
// `c_int` guard accepts every declared `RusttyTerminalOption`, including the
// 42, 43 and 46 this task adds between and after 44/45, and rejects the first
// value past the last one.
#[test]
fn terminal_set_accepts_every_declared_option() {
    with_test_allocator(|alloc| {
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);
        // NULL clears each of these (callbacks, the checksum flag and bits).
        for option in [41, 42, 43, 44, 45, 46] {
            assert_eq!(
                Result::Success.to_c_int(),
                unsafe { crate::rustty_terminal_set(t, option, std::ptr::null()) },
                "option {option}"
            );
        }
        for option in [47, -1, c_int::MAX] {
            assert_eq!(
                Result::InvalidValue.to_c_int(),
                unsafe { crate::rustty_terminal_set(t, option, std::ptr::null()) },
                "option {option}"
            );
        }
        unsafe { free(t) };
    });
}
