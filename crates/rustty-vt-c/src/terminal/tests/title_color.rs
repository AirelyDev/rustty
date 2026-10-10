//! PORTS: ghostty src/terminal/c/terminal.zig — tests: title / pwd,
//! resize pixel dimensions and in-band size reports, colors, and the
//! glyph protocol option.

use super::*;

// test "set and get title"
#[test]
fn set_and_get_title() {
    with_test_allocator(|alloc| {
        // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);

        // No title set yet — should return empty string
        let mut title = LibString {
            ptr: std::ptr::null(),
            len: 0,
        };
        assert_eq!(Result::Success, unsafe {
            get(t, TerminalData::Title, &mut title as *mut _ as *mut c_void)
        });
        assert_eq!(0usize, title.len);

        // Set title via option
        let hello = LibString {
            ptr: b"Hello".as_ptr(),
            len: 5,
        };
        assert_eq!(Result::Success, unsafe {
            set(t, Option_::Title, &hello as *const _ as *const c_void)
        });

        assert_eq!(Result::Success, unsafe {
            get(t, TerminalData::Title, &mut title as *mut _ as *mut c_void)
        });
        assert_eq!(b"Hello".as_slice(), unsafe {
            core::slice::from_raw_parts(title.ptr, title.len)
        });

        // Overwrite title
        let world = LibString {
            ptr: b"World".as_ptr(),
            len: 5,
        };
        assert_eq!(Result::Success, unsafe {
            set(t, Option_::Title, &world as *const _ as *const c_void)
        });

        assert_eq!(Result::Success, unsafe {
            get(t, TerminalData::Title, &mut title as *mut _ as *mut c_void)
        });
        assert_eq!(b"World".as_slice(), unsafe {
            core::slice::from_raw_parts(title.ptr, title.len)
        });

        // Clear title with NULL
        assert_eq!(Result::Success, unsafe {
            set(t, Option_::Title, std::ptr::null())
        });

        assert_eq!(Result::Success, unsafe {
            get(t, TerminalData::Title, &mut title as *mut _ as *mut c_void)
        });
        assert_eq!(0usize, title.len);
        unsafe { free(t) };
    });
}

// test "set and get pwd"
#[test]
fn set_and_get_pwd() {
    with_test_allocator(|alloc| {
        // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);

        // No pwd set yet — should return empty string
        let mut pwd = LibString {
            ptr: std::ptr::null(),
            len: 0,
        };
        assert_eq!(Result::Success, unsafe {
            get(t, TerminalData::Pwd, &mut pwd as *mut _ as *mut c_void)
        });
        assert_eq!(0usize, pwd.len);

        // Set pwd via option
        let home = LibString {
            ptr: b"/home/user".as_ptr(),
            len: 10,
        };
        assert_eq!(Result::Success, unsafe {
            set(t, Option_::Pwd, &home as *const _ as *const c_void)
        });

        assert_eq!(Result::Success, unsafe {
            get(t, TerminalData::Pwd, &mut pwd as *mut _ as *mut c_void)
        });
        assert_eq!(b"/home/user".as_slice(), unsafe {
            core::slice::from_raw_parts(pwd.ptr, pwd.len)
        });

        // Clear pwd with NULL
        assert_eq!(Result::Success, unsafe {
            set(t, Option_::Pwd, std::ptr::null())
        });

        assert_eq!(Result::Success, unsafe {
            get(t, TerminalData::Pwd, &mut pwd as *mut _ as *mut c_void)
        });
        assert_eq!(0usize, pwd.len);
        unsafe { free(t) };
    });
}

// test "get title set via vt_write"
#[test]
fn get_title_set_via_vt_write() {
    with_test_allocator(|alloc| {
        // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);

        // Set title via OSC 2
        vtw(t, b"\x1B]2;VT Title\x1B\\");

        let mut title = LibString {
            ptr: std::ptr::null(),
            len: 0,
        };
        assert_eq!(Result::Success, unsafe {
            get(t, TerminalData::Title, &mut title as *mut _ as *mut c_void)
        });
        assert_eq!(b"VT Title".as_slice(), unsafe {
            core::slice::from_raw_parts(title.ptr, title.len)
        });
        unsafe { free(t) };
    });
}

// test "title report requires explicit opt in"
#[test]
fn title_report_requires_explicit_opt_in() {
    static mut LAST_DATA: Option<Vec<u8>> = None;
    fn deinit() {
        // if (last_data) |data| testing.allocator.free(data);
        unsafe { LAST_DATA = None };
    }
    fn last_data() -> Option<&'static Vec<u8>> {
        unsafe { (*std::ptr::addr_of!(LAST_DATA)).as_ref() }
    }
    extern "C" fn write_pty(_t: Terminal, _ud: *mut c_void, ptr: *const u8, len: usize) {
        let data = unsafe { core::slice::from_raw_parts(ptr, len) }.to_vec();
        unsafe { LAST_DATA = Some(data) };
    }

    with_test_allocator(|alloc| {
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);
        // S.last_data = null;
        unsafe { LAST_DATA = None };
        // defer S.deinit();

        assert_eq!(Result::Success, unsafe {
            set(t, Option_::WritePty, write_pty as *const c_void)
        });

        // const set_title = "\x1B]2;echo vulnerable\x1B\\";
        let set_title = b"\x1B]2;echo vulnerable\x1B\\";
        // const query_title = "\x1B[21t";
        let query_title = b"\x1B[21t";
        vtw(t, set_title);

        // WRITE_PTY alone must not enable the security-sensitive response.
        vtw(t, query_title);
        // try testing.expect(S.last_data == null);
        assert!(last_data().is_none());

        // const enabled = true;
        let enabled: bool = true;
        assert_eq!(Result::Success, unsafe {
            set(
                t,
                Option_::TitleReport,
                &enabled as *const bool as *const c_void,
            )
        });
        vtw(t, query_title);
        // try testing.expectEqualStrings("\x1b]lecho vulnerable\x1b\\", S.last_data.?);
        assert_eq!(
            b"\x1b]lecho vulnerable\x1b\\".as_slice(),
            last_data().unwrap().as_slice()
        );

        // NULL restores the secure default.
        deinit();
        assert_eq!(Result::Success, unsafe {
            set(t, Option_::TitleReport, std::ptr::null())
        });
        vtw(t, query_title);
        assert!(last_data().is_none());

        unsafe { free(t) };
    });
}

// test "checksum report requires explicit opt in"
#[test]
fn checksum_report_requires_explicit_opt_in() {
    static mut LAST_DATA: Option<Vec<u8>> = None;
    fn deinit() {
        // if (last_data) |data| testing.allocator.free(data);
        unsafe { LAST_DATA = None };
    }
    fn last_data() -> Option<&'static Vec<u8>> {
        unsafe { (*std::ptr::addr_of!(LAST_DATA)).as_ref() }
    }
    extern "C" fn write_pty(_t: Terminal, _ud: *mut c_void, ptr: *const u8, len: usize) {
        let data = unsafe { core::slice::from_raw_parts(ptr, len) }.to_vec();
        unsafe { LAST_DATA = Some(data) };
    }

    with_test_allocator(|alloc| {
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);
        // S.last_data = null;
        unsafe { LAST_DATA = None };
        // defer S.deinit();

        assert_eq!(Result::Success, unsafe {
            set(t, Option_::WritePty, write_pty as *const c_void)
        });

        // const text = "hello";
        let text = b"hello";
        // const query = "\x1B[1;1;1;1;1;5*y";
        let query = b"\x1B[1;1;1;1;1;5*y";
        vtw(t, text);

        // WRITE_PTY alone must not enable the security-sensitive response.
        vtw(t, query);
        // try testing.expect(S.last_data == null);
        assert!(last_data().is_none());

        // const enabled = true;
        let enabled: bool = true;
        assert_eq!(Result::Success, unsafe {
            set(
                t,
                Option_::XtChecksumReport,
                &enabled as *const bool as *const c_void,
            )
        });
        vtw(t, query);
        // try testing.expectEqualStrings("\x1bP1!~FDEC\x1b\\", S.last_data.?);
        assert_eq!(
            b"\x1bP1!~FDEC\x1b\\".as_slice(),
            last_data().unwrap().as_slice()
        );

        // NULL restores the secure default.
        deinit();
        assert_eq!(Result::Success, unsafe {
            set(t, Option_::XtChecksumReport, std::ptr::null())
        });
        vtw(t, query);
        assert!(last_data().is_none());

        unsafe { free(t) };
    });
}

// test "checksum extension survives resets"
#[test]
fn checksum_extension_survives_resets() {
    static mut LAST_DATA: Option<Vec<u8>> = None;
    fn last_data() -> Option<&'static Vec<u8>> {
        unsafe { (*std::ptr::addr_of!(LAST_DATA)).as_ref() }
    }
    extern "C" fn write_pty(_t: Terminal, _ud: *mut c_void, ptr: *const u8, len: usize) {
        let data = unsafe { core::slice::from_raw_parts(ptr, len) }.to_vec();
        unsafe { LAST_DATA = Some(data) };
    }

    with_test_allocator(|alloc| {
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);
        // S.last_data = null;
        unsafe { LAST_DATA = None };

        // const enabled = true;
        let enabled: bool = true;
        assert_eq!(Result::Success, unsafe {
            set(t, Option_::WritePty, write_pty as *const c_void)
        });
        assert_eq!(Result::Success, unsafe {
            set(
                t,
                Option_::XtChecksumReport,
                &enabled as *const bool as *const c_void,
            )
        });

        // Don't negate the result.
        // const positive: u8 = 1;
        let positive: u8 = 1;
        assert_eq!(Result::Success, unsafe {
            set(
                t,
                Option_::XtChecksumExtension,
                &positive as *const u8 as *const c_void,
            )
        });

        let text = b"hello";
        let query = b"\x1B[1;1;1;1;1;5*y";
        vtw(t, text);
        vtw(t, query);
        assert_eq!(
            b"\x1bP1!~0214\x1b\\".as_slice(),
            last_data().unwrap().as_slice()
        );

        // A reset restores the configured calculation, not the DEC one.
        let negate = b"\x1B[0#y";
        let ris = b"\x1Bc";
        vtw(t, negate);
        vtw(t, ris);
        vtw(t, text);
        vtw(t, query);
        assert_eq!(
            b"\x1bP1!~0214\x1b\\".as_slice(),
            last_data().unwrap().as_slice()
        );

        // NULL restores the DEC calculation.
        assert_eq!(Result::Success, unsafe {
            set(t, Option_::XtChecksumExtension, std::ptr::null())
        });
        vtw(t, query);
        assert_eq!(
            b"\x1bP1!~FDEC\x1b\\".as_slice(),
            last_data().unwrap().as_slice()
        );

        // const invalid: u8 = 32;
        let invalid: u8 = 32;
        assert_eq!(Result::InvalidValue, unsafe {
            set(
                t,
                Option_::XtChecksumExtension,
                &invalid as *const u8 as *const c_void,
            )
        });

        unsafe { free(t) };
    });
}

// test "resize updates pixel dimensions"
#[test]
fn resize_updates_pixel_dimensions() {
    with_test_allocator(|alloc| {
        // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);

        // Pixel geometry must still be applied when the cell dimensions match.
        assert_eq!(Result::Success, unsafe { resize(t, 80, 24, 9, 18) });

        let zt = unsafe { (*t).terminal };
        assert_eq!(80u32 * 9, unsafe { (*zt).width_px });
        assert_eq!(24u32 * 18, unsafe { (*zt).height_px });
        unsafe { free(t) };
    });
}

// test "resize pixel overflow saturates"
#[test]
fn resize_pixel_overflow_saturates() {
    with_test_allocator(|alloc| {
        // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);

        assert_eq!(Result::Success, unsafe {
            resize(t, 100, 40, u32::MAX, u32::MAX)
        });

        let zt = unsafe { (*t).terminal };
        assert_eq!(u32::MAX, unsafe { (*zt).width_px });
        assert_eq!(u32::MAX, unsafe { (*zt).height_px });
        unsafe { free(t) };
    });
}

// test "resize disables synchronized output"
#[test]
fn resize_disables_synchronized_output() {
    with_test_allocator(|alloc| {
        // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);

        let zt = unsafe { (*t).terminal };
        unsafe { (*zt).modes.set(modes::Mode::SynchronizedOutput, true) };

        // NEGATIVE-CONTROL ASSERTION (P3-W4 T2 Step 5): the clear must
        // happen EXACTLY ONCE. `89b103dd5` moves the clear out of this
        // wrapper and into `Terminal::resize`; a port that keeps both
        // double-clears (== 2, behaviourally invisible) and a port that
        // deletes both without the terminal-side move loses it entirely
        // (== 0). Only the count separates the three cases — the
        // `!get(SynchronizedOutput)` assertion below passes for both 1 and
        // 2. See the `sync_output_clear_count` PORT NOTE in vt/modes.rs.
        // The COUNTER is `cfg(debug_assertions)` in rustty-vt, so it must be
        // referenced under the same cfg here — an unconditional call does not
        // compile under `--release`. The behavioural assertions below stay in
        // BOTH profiles; only the count, which is what distinguishes 0/1/2
        // clears, is debug-only.
        #[cfg(debug_assertions)]
        rustty_vt::modes::sync_output_clear_reset();

        // The terminal-level reset must run even if grid work is unnecessary.
        assert_eq!(Result::Success, unsafe { resize(t, 80, 24, 9, 18) });
        assert!(!unsafe { (*zt).modes.get(modes::Mode::SynchronizedOutput) });
        #[cfg(debug_assertions)]
        assert_eq!(1, rustty_vt::modes::sync_output_clear_count());
        unsafe { free(t) };
    });
}

// test "resize sends in-band size report"
#[test]
fn resize_sends_in_band_size_report() {
    static mut LAST_DATA: Option<Vec<u8>> = None;
    extern "C" fn write_pty(_t: Terminal, _ud: *mut c_void, ptr: *const u8, len: usize) {
        let data = unsafe { core::slice::from_raw_parts(ptr, len) }.to_vec();
        unsafe { LAST_DATA = Some(data) };
    }

    with_test_allocator(|alloc| {
        // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);
        unsafe { LAST_DATA = None };

        assert_eq!(Result::Success, unsafe {
            set(t, Option_::WritePty, write_pty as *const c_void)
        });

        // Enable in-band size reports (mode 2048)
        unsafe {
            (*(*t).terminal)
                .modes
                .set(modes::Mode::InBandSizeReports, true)
        };

        assert_eq!(Result::Success, unsafe { resize(t, 100, 40, 9, 18) });

        // Expected: \x1B[48;rows;cols;height_px;width_pxt
        // height_px = 40*18 = 720, width_px = 100*9 = 900
        assert!(unsafe { (*std::ptr::addr_of!(LAST_DATA)).as_ref() }.is_some());
        assert_eq!(
            b"\x1B[48;40;100;720;900t".as_slice(),
            unsafe { (*std::ptr::addr_of!(LAST_DATA)).as_ref() }
                .unwrap()
                .as_slice()
        );
        unsafe { free(t) };
    });
}

// test "resize no size report without mode 2048"
#[test]
fn resize_no_size_report_without_mode_2048() {
    static mut CALLED: bool = false;
    extern "C" fn write_pty(_t: Terminal, _ud: *mut c_void, _ptr: *const u8, _len: usize) {
        unsafe { CALLED = true };
    }

    with_test_allocator(|alloc| {
        // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);
        unsafe { CALLED = false };

        assert_eq!(Result::Success, unsafe {
            set(t, Option_::WritePty, write_pty as *const c_void)
        });

        // in_band_size_reports is off by default
        assert_eq!(Result::Success, unsafe { resize(t, 100, 40, 9, 18) });
        assert!(!unsafe { CALLED });
        unsafe { free(t) };
    });
}

// test "resize in-band report without write_pty callback"
#[test]
fn resize_in_band_report_without_write_pty_callback() {
    with_test_allocator(|alloc| {
        // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);

        // Enable mode 2048 but don't set a write_pty callback — should not crash
        unsafe {
            (*(*t).terminal)
                .modes
                .set(modes::Mode::InBandSizeReports, true)
        };
        assert_eq!(Result::Success, unsafe { resize(t, 100, 40, 9, 18) });
        unsafe { free(t) };
    });
}

// test "resize null terminal"
#[test]
fn resize_null_terminal() {
    assert_eq!(Result::InvalidValue, unsafe {
        resize(std::ptr::null_mut(), 100, 40, 9, 18)
    });
}

// test "resize zero cols"
#[test]
fn resize_zero_cols() {
    with_test_allocator(|alloc| {
        // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);
        assert_eq!(Result::InvalidValue, unsafe { resize(t, 0, 40, 9, 18) });
        unsafe { free(t) };
    });
}

// test "resize zero rows"
#[test]
fn resize_zero_rows() {
    with_test_allocator(|alloc| {
        // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);
        assert_eq!(Result::InvalidValue, unsafe { resize(t, 100, 0, 9, 18) });
        unsafe { free(t) };
    });
}

// test "grid_ref out of bounds"
#[test]
fn grid_ref_out_of_bounds() {
    with_test_allocator(|alloc| {
        // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);

        let mut out_ref = CGridRef::default();
        assert_eq!(Result::InvalidValue, unsafe {
            grid_ref(t, pt_active(100, 0), &mut out_ref)
        });
        unsafe { free(t) };
    });
}

// test "set and get color_foreground"
#[test]
fn set_and_get_color_foreground() {
    with_test_allocator(|alloc| {
        // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);

        // Initially unset
        let mut rgb = crate::color::RgbC { r: 0, g: 0, b: 0 };
        assert_eq!(Result::NoValue, unsafe {
            get(
                t,
                TerminalData::ColorForeground,
                &mut rgb as *mut _ as *mut c_void,
            )
        });

        // Set a value
        let fg = crate::color::RgbC {
            r: 0xAA,
            g: 0xBB,
            b: 0xCC,
        };
        assert_eq!(Result::Success, unsafe {
            set(
                t,
                Option_::ColorForeground,
                &fg as *const _ as *const c_void,
            )
        });
        assert_eq!(Result::Success, unsafe {
            get(
                t,
                TerminalData::ColorForeground,
                &mut rgb as *mut _ as *mut c_void,
            )
        });
        assert_eq!(fg, rgb);

        // Clear with null
        assert_eq!(Result::Success, unsafe {
            set(t, Option_::ColorForeground, std::ptr::null())
        });
        assert_eq!(Result::NoValue, unsafe {
            get(
                t,
                TerminalData::ColorForeground,
                &mut rgb as *mut _ as *mut c_void,
            )
        });
        unsafe { free(t) };
    });
}

// test "set and get color_background"
#[test]
fn set_and_get_color_background() {
    with_test_allocator(|alloc| {
        // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);

        let mut rgb = crate::color::RgbC { r: 0, g: 0, b: 0 };
        assert_eq!(Result::NoValue, unsafe {
            get(
                t,
                TerminalData::ColorBackground,
                &mut rgb as *mut _ as *mut c_void,
            )
        });

        let bg = crate::color::RgbC {
            r: 0x11,
            g: 0x22,
            b: 0x33,
        };
        assert_eq!(Result::Success, unsafe {
            set(
                t,
                Option_::ColorBackground,
                &bg as *const _ as *const c_void,
            )
        });
        assert_eq!(Result::Success, unsafe {
            get(
                t,
                TerminalData::ColorBackground,
                &mut rgb as *mut _ as *mut c_void,
            )
        });
        assert_eq!(bg, rgb);

        assert_eq!(Result::Success, unsafe {
            set(t, Option_::ColorBackground, std::ptr::null())
        });
        assert_eq!(Result::NoValue, unsafe {
            get(
                t,
                TerminalData::ColorBackground,
                &mut rgb as *mut _ as *mut c_void,
            )
        });
        unsafe { free(t) };
    });
}

// test "set and get color_cursor"
#[test]
fn set_and_get_color_cursor() {
    with_test_allocator(|alloc| {
        // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);

        let mut rgb = crate::color::RgbC { r: 0, g: 0, b: 0 };
        assert_eq!(Result::NoValue, unsafe {
            get(
                t,
                TerminalData::ColorCursor,
                &mut rgb as *mut _ as *mut c_void,
            )
        });

        let cur = crate::color::RgbC {
            r: 0xFF,
            g: 0x00,
            b: 0x88,
        };
        assert_eq!(Result::Success, unsafe {
            set(t, Option_::ColorCursor, &cur as *const _ as *const c_void)
        });
        assert_eq!(Result::Success, unsafe {
            get(
                t,
                TerminalData::ColorCursor,
                &mut rgb as *mut _ as *mut c_void,
            )
        });
        assert_eq!(cur, rgb);

        assert_eq!(Result::Success, unsafe {
            set(t, Option_::ColorCursor, std::ptr::null())
        });
        assert_eq!(Result::NoValue, unsafe {
            get(
                t,
                TerminalData::ColorCursor,
                &mut rgb as *mut _ as *mut c_void,
            )
        });
        unsafe { free(t) };
    });
}

// test "set and get color_palette"
#[test]
fn set_and_get_color_palette() {
    with_test_allocator(|alloc| {
        // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);

        // Get default palette
        let mut palette: crate::color::PaletteC = [crate::color::RgbC { r: 0, g: 0, b: 0 }; 256];
        assert_eq!(Result::Success, unsafe {
            get(
                t,
                TerminalData::ColorPalette,
                palette.as_mut_ptr() as *mut c_void,
            )
        });
        assert_eq!(rustty_vt::color::DEFAULT[0], palette[0]);

        // Set custom palette
        let mut custom: crate::color::PaletteC =
            unsafe { crate::color::palette_cval(&rustty_vt::color::DEFAULT) };
        custom[0] = crate::color::RgbC {
            r: 0x12,
            g: 0x34,
            b: 0x56,
        };
        assert_eq!(Result::Success, unsafe {
            set(t, Option_::ColorPalette, custom.as_ptr() as *const c_void)
        });
        assert_eq!(Result::Success, unsafe {
            get(
                t,
                TerminalData::ColorPalette,
                palette.as_mut_ptr() as *mut c_void,
            )
        });
        assert_eq!(custom[0], palette[0]);

        // Reset with null restores default
        assert_eq!(Result::Success, unsafe {
            set(t, Option_::ColorPalette, std::ptr::null())
        });
        assert_eq!(Result::Success, unsafe {
            get(
                t,
                TerminalData::ColorPalette,
                palette.as_mut_ptr() as *mut c_void,
            )
        });
        assert_eq!(rustty_vt::color::DEFAULT[0], palette[0]);
        unsafe { free(t) };
    });
}

// PORT NOTE: Rust-only (d1cd56a56, P7-W2 T7). A non-default palette is
// copied into memory from the terminal's allocator, so setting one can
// fail with `OUT_OF_MEMORY`, and then nothing changes. The copy is freed
// on replacement by the built-in default and in `free`.
#[test]
fn set_color_palette_out_of_memory() {
    use rustty_vt::alloc::testing::{Config, FailingAllocator};

    let failing = FailingAllocator::over_heap(Config::default());
    let alloc = failing.allocator();
    let (new_result, t) = new_t(&alloc, 80, 24);
    assert_eq!(Result::Success, new_result);

    let mut custom: crate::color::PaletteC =
        unsafe { crate::color::palette_cval(&rustty_vt::color::DEFAULT) };
    custom[0] = crate::color::RgbC {
        r: 0x12,
        g: 0x34,
        b: 0x56,
    };
    let mut palette: crate::color::PaletteC = [crate::color::RgbC { r: 0, g: 0, b: 0 }; 256];

    // The copy's allocation fails: out of memory, and nothing changes.
    unsafe { (*zig_terminal(t)).flags.dirty.palette = false };
    failing.fail_index.set(failing.alloc_index.get());
    assert_eq!(Result::OutOfMemory, unsafe {
        set(t, Option_::ColorPalette, custom.as_ptr() as *const c_void)
    });
    assert!(failing.has_induced_failure.get());
    assert!(!unsafe { (*zig_terminal(t)).flags.dirty.palette });
    for data in [
        TerminalData::ColorPalette,
        TerminalData::ColorPaletteDefault,
    ] {
        assert_eq!(Result::Success, unsafe {
            get(t, data, palette.as_mut_ptr() as *mut c_void)
        });
        assert_eq!(rustty_vt::color::DEFAULT[0], palette[0]);
    }

    // The built-in default never allocates, even while failing.
    assert_eq!(Result::Success, unsafe {
        set(t, Option_::ColorPalette, std::ptr::null())
    });

    // With memory, the custom palette is set; the built-in default then
    // releases the copy, and a second custom palette is freed by `free`.
    failing.fail_index.set(usize::MAX);
    assert_eq!(Result::Success, unsafe {
        set(t, Option_::ColorPalette, custom.as_ptr() as *const c_void)
    });
    assert_eq!(Result::Success, unsafe {
        get(
            t,
            TerminalData::ColorPaletteDefault,
            palette.as_mut_ptr() as *mut c_void,
        )
    });
    assert_eq!(custom[0], palette[0]);
    let live = failing.allocated_bytes.get() - failing.freed_bytes.get();
    assert_eq!(Result::Success, unsafe {
        set(t, Option_::ColorPalette, std::ptr::null())
    });
    assert_eq!(
        live - core::mem::size_of::<rustty_vt::color::Palette>(),
        failing.allocated_bytes.get() - failing.freed_bytes.get()
    );
    assert_eq!(Result::Success, unsafe {
        set(t, Option_::ColorPalette, custom.as_ptr() as *const c_void)
    });
    unsafe { free(t) };
    assert_eq!(failing.allocated_bytes.get(), failing.freed_bytes.get());
    assert_eq!(failing.allocations.get(), failing.deallocations.get());
}

// test "get color default vs effective with override"
#[test]
fn get_color_default_vs_effective_with_override() {
    with_test_allocator(|alloc| {
        // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);

        let zt = unsafe { (*t).terminal };
        let mut rgb = crate::color::RgbC { r: 0, g: 0, b: 0 };

        // Set defaults
        let fg = crate::color::RgbC {
            r: 0xAA,
            g: 0xBB,
            b: 0xCC,
        };
        let bg = crate::color::RgbC {
            r: 0x11,
            g: 0x22,
            b: 0x33,
        };
        let cur = crate::color::RgbC {
            r: 0xFF,
            g: 0x00,
            b: 0x88,
        };
        assert_eq!(Result::Success, unsafe {
            set(
                t,
                Option_::ColorForeground,
                &fg as *const _ as *const c_void,
            )
        });
        assert_eq!(Result::Success, unsafe {
            set(
                t,
                Option_::ColorBackground,
                &bg as *const _ as *const c_void,
            )
        });
        assert_eq!(Result::Success, unsafe {
            set(t, Option_::ColorCursor, &cur as *const _ as *const c_void)
        });

        // Simulate OSC overrides
        let override_ = crate::color::RgbC {
            r: 0x00,
            g: 0x00,
            b: 0x00,
        };
        unsafe {
            (*zt).colors.foreground.override_ = Some(override_);
            (*zt).colors.background.override_ = Some(override_);
            (*zt).colors.cursor.override_ = Some(override_);
        }

        // Effective returns override
        assert_eq!(Result::Success, unsafe {
            get(
                t,
                TerminalData::ColorForeground,
                &mut rgb as *mut _ as *mut c_void,
            )
        });
        assert_eq!(override_, rgb);
        assert_eq!(Result::Success, unsafe {
            get(
                t,
                TerminalData::ColorBackground,
                &mut rgb as *mut _ as *mut c_void,
            )
        });
        assert_eq!(override_, rgb);
        assert_eq!(Result::Success, unsafe {
            get(
                t,
                TerminalData::ColorCursor,
                &mut rgb as *mut _ as *mut c_void,
            )
        });
        assert_eq!(override_, rgb);

        // Default returns original
        assert_eq!(Result::Success, unsafe {
            get(
                t,
                TerminalData::ColorForegroundDefault,
                &mut rgb as *mut _ as *mut c_void,
            )
        });
        assert_eq!(fg, rgb);
        assert_eq!(Result::Success, unsafe {
            get(
                t,
                TerminalData::ColorBackgroundDefault,
                &mut rgb as *mut _ as *mut c_void,
            )
        });
        assert_eq!(bg, rgb);
        assert_eq!(Result::Success, unsafe {
            get(
                t,
                TerminalData::ColorCursorDefault,
                &mut rgb as *mut _ as *mut c_void,
            )
        });
        assert_eq!(cur, rgb);
        unsafe { free(t) };
    });
}

// test "get color default returns no_value when unset"
#[test]
fn get_color_default_returns_no_value_when_unset() {
    with_test_allocator(|alloc| {
        // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);

        let mut rgb = crate::color::RgbC { r: 0, g: 0, b: 0 };
        assert_eq!(Result::NoValue, unsafe {
            get(
                t,
                TerminalData::ColorForegroundDefault,
                &mut rgb as *mut _ as *mut c_void,
            )
        });
        assert_eq!(Result::NoValue, unsafe {
            get(
                t,
                TerminalData::ColorBackgroundDefault,
                &mut rgb as *mut _ as *mut c_void,
            )
        });
        assert_eq!(Result::NoValue, unsafe {
            get(
                t,
                TerminalData::ColorCursorDefault,
                &mut rgb as *mut _ as *mut c_void,
            )
        });
        unsafe { free(t) };
    });
}

// test "get color_palette_default vs current"
#[test]
fn get_color_palette_default_vs_current() {
    with_test_allocator(|alloc| {
        // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);

        let zt = unsafe { (*t).terminal };

        // Set a custom default palette
        let mut custom: crate::color::PaletteC =
            unsafe { crate::color::palette_cval(&rustty_vt::color::DEFAULT) };
        custom[0] = crate::color::RgbC {
            r: 0x12,
            g: 0x34,
            b: 0x56,
        };
        assert_eq!(Result::Success, unsafe {
            set(t, Option_::ColorPalette, custom.as_ptr() as *const c_void)
        });

        // Simulate OSC override on index 0
        unsafe {
            (*zt).colors.palette.set(
                0,
                crate::color::RgbC {
                    r: 0xFF,
                    g: 0xFF,
                    b: 0xFF,
                },
            )
        };

        // Current palette returns the override
        let mut palette: crate::color::PaletteC = [crate::color::RgbC { r: 0, g: 0, b: 0 }; 256];
        assert_eq!(Result::Success, unsafe {
            get(
                t,
                TerminalData::ColorPalette,
                palette.as_mut_ptr() as *mut c_void,
            )
        });
        assert_eq!(
            crate::color::RgbC {
                r: 0xFF,
                g: 0xFF,
                b: 0xFF
            },
            palette[0]
        );

        // Default palette returns the original
        assert_eq!(Result::Success, unsafe {
            get(
                t,
                TerminalData::ColorPaletteDefault,
                palette.as_mut_ptr() as *mut c_void,
            )
        });
        assert_eq!(custom[0], palette[0]);
        unsafe { free(t) };
    });
}

// test "set color sets dirty flag"
#[test]
fn set_color_sets_dirty_flag() {
    with_test_allocator(|alloc| {
        // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);

        let zt = unsafe { (*t).terminal };
        unsafe { (*zt).flags.dirty.palette = false };

        let fg = crate::color::RgbC {
            r: 0xFF,
            g: 0xFF,
            b: 0xFF,
        };
        assert_eq!(Result::Success, unsafe {
            set(
                t,
                Option_::ColorForeground,
                &fg as *const _ as *const c_void,
            )
        });
        assert!(unsafe { (*zt).flags.dirty.palette });
        unsafe { free(t) };
    });
}

// test "set glyph protocol disables APC handling and clears glossary"
#[test]
#[cfg_attr(
    not(feature = "glyph-protocol"),
    ignore = "SkipZigTest: needs the glyph-protocol vt feature (R14)"
)]
fn set_glyph_protocol_disables_apc_handling_and_clears_glossary() {
    // N541: this test reaches `sys::log_fn`; hold the sys guard so its
    // warnings cannot land in another test's log sink.
    let _guard = crate::sys::tests::reset_global();
    // if (comptime !build_options.glyph_protocol) return error.SkipZigTest;
    with_test_allocator(|alloc| {
        // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);

        let register = b"\x1B_25a1;r;cp=e0a0;AAAAAAAAAAAAAA==\x1B\\";
        vtw(t, register);
        assert!(unsafe { (*(*t).terminal).glyph_glossary.contains(0xE0A0) });

        let disabled = false;
        assert_eq!(Result::Success, unsafe {
            set(
                t,
                Option_::GlyphProtocol,
                &disabled as *const _ as *const c_void,
            )
        });
        assert!(
            !unsafe { (*t).stream.handler.apc_handler.enabled }
                [rustty_vt::apc::Protocol::Glyph as usize]
        );
        assert!(!unsafe { (*(*t).terminal).glyph_glossary.contains(0xE0A0) });

        vtw(t, register);
        assert!(!unsafe { (*(*t).terminal).glyph_glossary.contains(0xE0A0) });

        let enabled = true;
        assert_eq!(Result::Success, unsafe {
            set(
                t,
                Option_::GlyphProtocol,
                &enabled as *const _ as *const c_void,
            )
        });
        vtw(t, register);
        assert!(unsafe { (*(*t).terminal).glyph_glossary.contains(0xE0A0) });
        unsafe { free(t) };
    });
}
