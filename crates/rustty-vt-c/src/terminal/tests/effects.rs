//! PORTS: ghostty src/terminal/c/terminal.zig — tests: effect callbacks
//! (write_pty, bell, enquiry, xtversion, terminfo, title, render hold,
//! re-entrant callbacks, pwd, desktop notification, progress report,
//! unknown sequence).

use super::*;

// PORT NOTE (D3 SEAM soundness probe, Rust-only addition): the reference
// exercises the effect trampolines through the C `set` API (a later
// sub-batch). To validate the `container_of` recovery (`wrapper_from_handler`)
// in THIS sub-batch, we install a C `write_pty` callback by writing the vt-c
// `effects.write_pty` field directly (the same field `setTyped(.write_pty)`
// will write), then drive a VT query (DSR operating status `CSI 5 n`) that
// round-trips through the always-installed trampoline and unconditionally
// writes `"\x1b[0n"` back. This is the Miri soundness gate for the raw-ptr
// sibling recovery off the `&mut Handler`.
static mut WRITE_PTY_CAPTURE: Vec<u8> = Vec::new();

extern "C" fn capture_write_pty(_t: Terminal, _ud: *mut c_void, ptr: *const u8, len: usize) {
    let data = unsafe { core::slice::from_raw_parts(ptr, len) };
    unsafe {
        #[allow(static_mut_refs)]
        WRITE_PTY_CAPTURE.extend_from_slice(data);
    }
}

#[test]
fn write_pty_trampoline_round_trip() {
    with_test_allocator(|alloc| {
        // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);
        unsafe {
            #[allow(static_mut_refs)]
            WRITE_PTY_CAPTURE.clear();
        }

        // Install the C callback directly on the vt-c effects (the field the
        // C `set(.write_pty)` will populate).
        unsafe { (*t).effects.write_pty = Some(capture_write_pty) };

        // DSR operating status "CSI 5 n" writes "\x1b[0n" via write_pty.
        unsafe { vt_write(t, b"\x1b[5n".as_ptr(), 4) };

        let captured = unsafe {
            #[allow(static_mut_refs)]
            WRITE_PTY_CAPTURE.clone()
        };
        assert_eq!(b"\x1b[0n", captured.as_slice());
        unsafe { free(t) };
    });
}

// PORT NOTE (callback test scaffolding): the reference declares a per-test
// `const S = struct { var last_data: ?[]u8 = ...; fn writePty(...) {...} }`
// capturing into file-local `var`s with `testing.allocator.dupe`. The rustty
// analog is a per-test module-scope `static mut` plus an `extern "C" fn`; the
// capture buffer is an owned `Vec<u8>` (freed on test exit). `#[allow(
// static_mut_refs)]` mirrors the existing trampoline probe above. Each test's
// statics are isolated to its own `mod`-like block so the parallel test runner
// never aliases them.

// test "set write_pty callback DECRQM" (renamed by oracle 9dc0d974e)
#[test]
fn set_write_pty_callback_decrqm() {
    static mut LAST_DATA: Option<Vec<u8>> = None;
    static mut LAST_USERDATA: *mut c_void = std::ptr::null_mut();
    static mut CALLS: usize = 0;
    extern "C" fn write_pty(_t: Terminal, ud: *mut c_void, ptr: *const u8, len: usize) {
        let data = unsafe { core::slice::from_raw_parts(ptr, len) }.to_vec();
        unsafe {
            LAST_DATA = Some(data);
            LAST_USERDATA = ud;
            CALLS += 1;
        }
    }

    with_test_allocator(|alloc| {
        // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);
        unsafe {
            LAST_DATA = None;
            LAST_USERDATA = std::ptr::null_mut();
            CALLS = 0;
        }

        // Set userdata and write_pty callback
        let mut sentinel: u8 = 42;
        assert_eq!(Result::Success, unsafe {
            set(
                t,
                Option_::Userdata,
                &mut sentinel as *mut u8 as *const c_void,
            )
        });
        assert_eq!(Result::Success, unsafe {
            set(t, Option_::WritePty, write_pty as *const c_void)
        });

        // DECRQM for wraparound mode (mode 7, set by default) should trigger write_pty
        vtw(t, b"\x1B[?7$p");
        assert!(unsafe { (*std::ptr::addr_of!(LAST_DATA)).as_ref() }.is_some());
        assert_eq!(
            b"\x1B[?7;1$y".as_slice(),
            unsafe { (*std::ptr::addr_of!(LAST_DATA)).as_ref() }
                .unwrap()
                .as_slice()
        );
        assert_eq!(&mut sentinel as *mut u8 as *mut c_void, unsafe {
            LAST_USERDATA
        });
        assert_eq!(1, unsafe { CALLS });

        let query = b"\x1B[4h\x1B[4$p";
        vtw(t, query);
        assert_eq!(2, unsafe { CALLS });
        assert_eq!(
            b"\x1B[4;1$y".as_slice(),
            unsafe { (*std::ptr::addr_of!(LAST_DATA)).as_ref() }
                .unwrap()
                .as_slice()
        );
        assert_eq!(&mut sentinel as *mut u8 as *mut c_void, unsafe {
            LAST_USERDATA
        });
        unsafe { free(t) };
    });
}

// test "write_pty receives DECRQSS response" (oracle 39ae85f04)
//
// Area C §6.5 recorded a BOUNDED UNCERTAINTY: nobody had traced whether the
// C `vt_write` path delivers a DECRQSS reply through the `write_pty`
// trampoline. This test answers it definitively — and it does, so no
// additional C-side plumbing was needed beyond the `dcs_handler` field.
#[test]
fn write_pty_receives_decrqss_response() {
    static mut LAST_DATA: Option<Vec<u8>> = None;
    extern "C" fn write_pty(_t: Terminal, _ud: *mut c_void, ptr: *const u8, len: usize) {
        let data = unsafe { core::slice::from_raw_parts(ptr, len) }.to_vec();
        unsafe { LAST_DATA = Some(data) };
    }

    with_test_allocator(|alloc| {
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);
        unsafe { LAST_DATA = None };

        assert_eq!(Result::Success, unsafe {
            set(t, Option_::WritePty, write_pty as *const c_void)
        });

        vtw(t, b"\x1B[1m\x1BP$qm\x1B\\");
        assert_eq!(
            b"\x1BP1$r0;1m\x1B\\".as_slice(),
            unsafe { (*std::ptr::addr_of!(LAST_DATA)).as_ref() }
                .unwrap()
                .as_slice()
        );
        unsafe { free(t) };
    });
}

// test "write_pty receives OSC color query response" (oracle 14c829883)
#[test]
fn write_pty_receives_osc_color_query_response() {
    static mut LAST_DATA: Option<Vec<u8>> = None;
    extern "C" fn write_pty(_t: Terminal, _ud: *mut c_void, ptr: *const u8, len: usize) {
        let data = unsafe { core::slice::from_raw_parts(ptr, len) }.to_vec();
        unsafe {
            LAST_DATA = Some(data);
        }
    }

    with_test_allocator(|alloc| {
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);
        unsafe {
            LAST_DATA = None;
        }

        assert_eq!(Result::Success, unsafe {
            set(t, Option_::WritePty, write_pty as *const c_void)
        });

        let set_fg = b"\x1B]10;rgb:01/02/03\x1B\\";
        vtw(t, set_fg);
        assert!(unsafe { (*std::ptr::addr_of!(LAST_DATA)).as_ref() }.is_none());

        let query_fg = b"\x1B]10;?\x1B\\";
        vtw(t, query_fg);
        assert!(unsafe { (*std::ptr::addr_of!(LAST_DATA)).as_ref() }.is_some());
        assert_eq!(
            b"\x1B]10;rgb:0101/0202/0303\x1B\\".as_slice(),
            unsafe { (*std::ptr::addr_of!(LAST_DATA)).as_ref() }
                .unwrap()
                .as_slice()
        );

        unsafe { free(t) };
    });
}

// test "set write_pty without callback ignores queries"
#[test]
fn set_write_pty_without_callback_ignores_queries() {
    with_test_allocator(|alloc| {
        // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);
        // Without setting a callback, DECRQM should be silently ignored (no crash)
        vtw(t, b"\x1B[?7$p");
        vtw(t, b"\x1B[4$p");
        unsafe { free(t) };
    });
}

// test "set write_pty null clears callback"
#[test]
fn set_write_pty_null_clears_callback() {
    static mut CALLED: bool = false;
    extern "C" fn write_pty(_t: Terminal, _ud: *mut c_void, _ptr: *const u8, _len: usize) {
        unsafe { CALLED = true };
    }

    with_test_allocator(|alloc| {
        // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);
        unsafe { CALLED = false };

        // Set then clear the callback
        assert_eq!(Result::Success, unsafe {
            set(t, Option_::WritePty, write_pty as *const c_void)
        });
        assert_eq!(Result::Success, unsafe {
            set(t, Option_::WritePty, std::ptr::null())
        });

        vtw(t, b"\x1B[?7$p");
        assert!(!unsafe { CALLED });
        unsafe { free(t) };
    });
}

// test "set bell callback"
#[test]
fn set_bell_callback() {
    static mut BELL_COUNT: usize = 0;
    static mut LAST_USERDATA: *mut c_void = std::ptr::null_mut();
    extern "C" fn bell(_t: Terminal, ud: *mut c_void) {
        unsafe {
            BELL_COUNT += 1;
            LAST_USERDATA = ud;
        }
    }

    with_test_allocator(|alloc| {
        // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);
        unsafe {
            BELL_COUNT = 0;
            LAST_USERDATA = std::ptr::null_mut();
        }

        // Set userdata and bell callback
        let mut sentinel: u8 = 99;
        assert_eq!(Result::Success, unsafe {
            set(
                t,
                Option_::Userdata,
                &mut sentinel as *mut u8 as *const c_void,
            )
        });
        assert_eq!(Result::Success, unsafe {
            set(t, Option_::Bell, bell as *const c_void)
        });

        // Single BEL
        vtw(t, b"\x07");
        assert_eq!(1usize, unsafe { BELL_COUNT });
        assert_eq!(&mut sentinel as *mut u8 as *mut c_void, unsafe {
            LAST_USERDATA
        });

        // Multiple BELs
        vtw(t, b"\x07\x07");
        assert_eq!(3usize, unsafe { BELL_COUNT });
        unsafe { free(t) };
    });
}

// test "bell without callback is silent"
#[test]
fn bell_without_callback_is_silent() {
    with_test_allocator(|alloc| {
        // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);
        // BEL without a callback should not crash
        vtw(t, b"\x07");
        unsafe { free(t) };
    });
}

// test "set enquiry callback"
#[test]
fn set_enquiry_callback() {
    static mut LAST_DATA: Option<Vec<u8>> = None;
    extern "C" fn write_pty(_t: Terminal, _ud: *mut c_void, ptr: *const u8, len: usize) {
        let data = unsafe { core::slice::from_raw_parts(ptr, len) }.to_vec();
        unsafe { LAST_DATA = Some(data) };
    }
    const RESPONSE: &[u8] = b"OK";
    extern "C" fn enquiry(_t: Terminal, _ud: *mut c_void) -> LibString {
        LibString {
            ptr: RESPONSE.as_ptr(),
            len: RESPONSE.len(),
        }
    }

    with_test_allocator(|alloc| {
        // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);
        unsafe { LAST_DATA = None };

        assert_eq!(Result::Success, unsafe {
            set(t, Option_::WritePty, write_pty as *const c_void)
        });
        assert_eq!(Result::Success, unsafe {
            set(t, Option_::Enquiry, enquiry as *const c_void)
        });

        // ENQ (0x05) should trigger the enquiry callback and write response via write_pty
        vtw(t, b"\x05");
        assert!(unsafe { (*std::ptr::addr_of!(LAST_DATA)).as_ref() }.is_some());
        assert_eq!(
            b"OK".as_slice(),
            unsafe { (*std::ptr::addr_of!(LAST_DATA)).as_ref() }
                .unwrap()
                .as_slice()
        );
        unsafe { free(t) };
    });
}

// test "enquiry without callback is silent"
#[test]
fn enquiry_without_callback_is_silent() {
    with_test_allocator(|alloc| {
        // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);
        // ENQ without a callback should not crash
        vtw(t, b"\x05");
        unsafe { free(t) };
    });
}

// test "set xtversion callback"
#[test]
fn set_xtversion_callback() {
    static mut LAST_DATA: Option<Vec<u8>> = None;
    extern "C" fn write_pty(_t: Terminal, _ud: *mut c_void, ptr: *const u8, len: usize) {
        let data = unsafe { core::slice::from_raw_parts(ptr, len) }.to_vec();
        unsafe { LAST_DATA = Some(data) };
    }
    const VERSION: &[u8] = b"myterm 1.0";
    extern "C" fn xtversion(_t: Terminal, _ud: *mut c_void) -> LibString {
        LibString {
            ptr: VERSION.as_ptr(),
            len: VERSION.len(),
        }
    }

    with_test_allocator(|alloc| {
        // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);
        unsafe { LAST_DATA = None };

        assert_eq!(Result::Success, unsafe {
            set(t, Option_::WritePty, write_pty as *const c_void)
        });
        assert_eq!(Result::Success, unsafe {
            set(t, Option_::Xtversion, xtversion as *const c_void)
        });

        // XTVERSION: CSI > q
        vtw(t, b"\x1B[>q");
        assert!(unsafe { (*std::ptr::addr_of!(LAST_DATA)).as_ref() }.is_some());
        // Response should be DCS >| version ST
        assert_eq!(
            b"\x1BP>|myterm 1.0\x1B\\".as_slice(),
            unsafe { (*std::ptr::addr_of!(LAST_DATA)).as_ref() }
                .unwrap()
                .as_slice()
        );
        unsafe { free(t) };
    });
}

// test "xtversion without callback reports default"
#[test]
fn xtversion_without_callback_reports_default() {
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

        // Set write_pty but not xtversion — should get default "libghostty"
        assert_eq!(Result::Success, unsafe {
            set(t, Option_::WritePty, write_pty as *const c_void)
        });

        vtw(t, b"\x1B[>q");
        assert!(unsafe { (*std::ptr::addr_of!(LAST_DATA)).as_ref() }.is_some());
        // PORT NOTE: the reference default version string is "libghostty";
        // rustty retargets it to "librustty" (the rename rule, see
        // `report_xtversion`), so the INDEPENDENT expected value is "librustty".
        assert_eq!(
            b"\x1BP>|librustty\x1B\\".as_slice(),
            unsafe { (*std::ptr::addr_of!(LAST_DATA)).as_ref() }
                .unwrap()
                .as_slice()
        );
        unsafe { free(t) };
    });
}

// test "set terminfo_name option"
#[test]
fn set_terminfo_name_option() {
    static mut LAST_DATA: Option<Vec<u8>> = None;
    fn deinit() {
        // if (last_data) |d| testing.allocator.free(d);
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
        unsafe { LAST_DATA = None };
        // defer S.deinit();

        assert_eq!(Result::Success, unsafe {
            set(t, Option_::WritePty, write_pty as *const c_void)
        });

        // While no name is set the query goes unanswered; other capabilities
        // are still served from the static map.
        // const query = "\x1BP+q" ++ std.fmt.bytesToHex("TN", .upper) ++ "\x1B\\";
        let query = b"\x1BP+q544E\x1B\\";
        vtw(t, query);
        // try testing.expect(S.last_data == null);
        assert!(last_data().is_none());
        // const co_query = "\x1BP+q" ++ std.fmt.bytesToHex("Co", .upper) ++ "\x1B\\";
        let co_query = b"\x1BP+q436F\x1B\\";
        vtw(t, co_query);
        // try testing.expectEqualStrings("\x1BP1+r" ++ hex("Co") ++ "=" ++ hex("256") ++ "\x1B\\", S.last_data.?);
        assert_eq!(
            b"\x1BP1+r436F=323536\x1B\\".as_slice(),
            last_data().unwrap().as_slice()
        );
        deinit();

        // The name is copied, so the caller's buffer can go away afterwards.
        // var name: [14]u8 = "xterm-256color".*;
        let mut name: [u8; 14] = *b"xterm-256color";
        let value = LibString {
            ptr: name.as_ptr(),
            len: name.len(),
        };
        assert_eq!(Result::Success, unsafe {
            set(
                t,
                Option_::TerminfoName,
                &value as *const LibString as *const c_void,
            )
        });
        // @memset(&name, 'z');
        name.fill(b'z');

        vtw(t, query);
        // try testing.expect(S.last_data != null);
        assert!(last_data().is_some());
        // try testing.expectEqualStrings("\x1BP1+r" ++ hex("TN") ++ "=" ++ hex("xterm-256color") ++ "\x1B\\", S.last_data.?);
        assert_eq!(
            b"\x1BP1+r544E=787465726D2D323536636F6C6F72\x1B\\".as_slice(),
            last_data().unwrap().as_slice()
        );

        // Clearing with NULL leaves the query unanswered again.
        deinit();
        assert_eq!(Result::Success, unsafe {
            set(t, Option_::TerminfoName, std::ptr::null())
        });
        vtw(t, query);
        assert!(last_data().is_none());

        // Names beyond the maximum are rejected rather than truncated.
        // const long: [Handler.max_terminfo_name_bytes + 1]u8 = @splat('a');
        let long = [b'a'; rustty_vt::stream_terminal::MAX_TERMINFO_NAME_BYTES + 1];
        let long_value = LibString {
            ptr: long.as_ptr(),
            len: long.len(),
        };
        assert_eq!(Result::InvalidValue, unsafe {
            set(
                t,
                Option_::TerminfoName,
                &long_value as *const LibString as *const c_void,
            )
        });
        unsafe { free(t) };
    });
}

// test "set title_changed callback"
#[test]
fn set_title_changed_callback() {
    static mut TITLE_COUNT: usize = 0;
    static mut LAST_USERDATA: *mut c_void = std::ptr::null_mut();
    extern "C" fn title_changed(_t: Terminal, ud: *mut c_void) {
        unsafe {
            TITLE_COUNT += 1;
            LAST_USERDATA = ud;
        }
    }

    with_test_allocator(|alloc| {
        // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);
        unsafe {
            TITLE_COUNT = 0;
            LAST_USERDATA = std::ptr::null_mut();
        }

        let mut sentinel: u8 = 77;
        assert_eq!(Result::Success, unsafe {
            set(
                t,
                Option_::Userdata,
                &mut sentinel as *mut u8 as *const c_void,
            )
        });
        assert_eq!(Result::Success, unsafe {
            set(t, Option_::TitleChanged, title_changed as *const c_void)
        });

        // OSC 2 ; title ST — set window title
        vtw(t, b"\x1B]2;Hello\x1B\\");
        assert_eq!(1usize, unsafe { TITLE_COUNT });
        assert_eq!(&mut sentinel as *mut u8 as *mut c_void, unsafe {
            LAST_USERDATA
        });

        // Another title change
        vtw(t, b"\x1B]2;World\x1B\\");
        assert_eq!(2usize, unsafe { TITLE_COUNT });
        unsafe { free(t) };
    });
}

// test "set render_hold callback"
#[test]
fn set_render_hold_callback() {
    // const S = struct {
    //     var events: [8]bool = undefined;
    //     var len: usize = 0;
    //     var last_userdata: ?*anyopaque = null;
    //     fn hold(_: Terminal, ud: ?*anyopaque, held: bool) callconv(lib.calling_conv) void { ... }
    // };
    //
    // PORT NOTE: `S.events[0..S.len]` is the prefix of the fixed array,
    // compared with `&EVENTS[..LEN]`.
    static mut EVENTS: [bool; 8] = [false; 8];
    static mut LEN: usize = 0;
    static mut LAST_USERDATA: *mut c_void = std::ptr::null_mut();
    extern "C" fn hold(_t: Terminal, ud: *mut c_void, held: bool) {
        unsafe {
            EVENTS[LEN] = held;
            LEN += 1;
            LAST_USERDATA = ud;
        }
    }
    fn events() -> Vec<bool> {
        // SAFETY: single-threaded test-local statics.
        unsafe { (&*std::ptr::addr_of!(EVENTS))[..LEN].to_vec() }
    }

    with_test_allocator(|alloc| {
        // var t: Terminal = null;
        // try testing.expectEqual(Result.success, new(&lib.alloc.test_allocator, &t, 80, 24));
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);
        // defer free(t);

        // S.len = 0;
        // S.last_userdata = null;
        unsafe {
            LEN = 0;
            LAST_USERDATA = std::ptr::null_mut();
        }

        // var sentinel: u8 = 0;
        let mut sentinel: u8 = 0;
        // try testing.expectEqual(Result.success, set(t, .userdata, @ptrCast(&sentinel)));
        assert_eq!(Result::Success, unsafe {
            set(
                t,
                Option_::Userdata,
                &mut sentinel as *mut u8 as *const c_void,
            )
        });
        // try testing.expectEqual(Result.success, set(t, .render_hold, @ptrCast(&S.hold)));
        assert_eq!(Result::Success, unsafe {
            set(t, Option_::RenderHold, hold as *const c_void)
        });

        // A set during a hold and a reset without a hold are ignored.
        let frame = b"\x1b[?2026h\x1b[?2026hA\x1b[?2026l\x1b[?2026l";
        vtw(t, frame);
        assert_eq!(vec![true, false], events());
        assert_eq!(&mut sentinel as *mut u8 as *mut c_void, unsafe {
            LAST_USERDATA
        });

        // Resize and reset turn the mode off and end the hold.
        let begin = b"\x1b[?2026h";
        unsafe { LEN = 0 };
        vtw(t, begin);
        assert_eq!(Result::Success, unsafe { resize(t, 80, 24, 9, 18) });
        vtw(t, begin);
        unsafe { reset(t) };
        unsafe { reset(t) };
        assert_eq!(vec![true, false, true, false], events());

        // Changing the mode ourselves (e.g. for a timeout) isn't reported.
        unsafe { LEN = 0 };
        vtw(t, begin);
        // const off: ModeConfig = .{ .mode = 2026, .value = false };
        let off = ModeConfig {
            mode: 2026,
            value: false,
        };
        assert_eq!(Result::Success, unsafe {
            set(t, Option_::Mode, &off as *const ModeConfig as *const c_void)
        });
        // try testing.expect(!t.?.terminal.modes.get(.synchronized_output));
        assert!(!unsafe { (*(*t).terminal).modes.get(modes::Mode::SynchronizedOutput) });
        assert_eq!(vec![true], events());

        // Clearing the callback silences it.
        unsafe { LEN = 0 };
        assert_eq!(Result::Success, unsafe {
            set(t, Option_::RenderHold, std::ptr::null())
        });
        vtw(t, frame);
        assert_eq!(0, unsafe { LEN });

        unsafe { free(t) };
    });
}

// Rust-only (P7-W2 T6; delta review R1 #20 SB hazard; Miri target in the
// `vtc` group). The render_hold callback is documented to re-enter the
// terminal with `rustty_render_state_update` while the terminal is still
// processing the sequence (or the resize, or the C reset) that ended or
// began the hold. This drives that re-entry through every call site that
// fires the effect — `vt_write` (set, reset, restore, RIS), the C
// `resize` (through `Handler::resize`) and the C `reset` — and checks the
// captured frame. Under Miri (SB and TB) it proves that no `&mut
// Terminal`/Screen/Page, and no `&mut Handler` formed by C `reset`, is live
// across the callback.
#[test]
fn render_hold_callback_reenters_render_state_update() {
    struct Renderer {
        render_state: crate::render::RenderState,
        snapshots: usize,
        held: bool,
    }
    extern "C" fn on_render_hold(terminal: Terminal, userdata: *mut c_void, held: bool) {
        // SAFETY: `userdata` is the test's live `Renderer`.
        let r = userdata as *mut Renderer;
        // Update on every event (on `true` to capture the frame the
        // program wants left on screen; on `false` as an embedder that
        // resumes updating does), so every call site is re-entered.
        let rs = unsafe { (*r).render_state };
        assert_eq!(Result::Success, unsafe {
            crate::render::update(rs, terminal)
        });
        unsafe {
            (*r).snapshots += 1;
            (*r).held = held;
        }
    }
    fn char(r: &Renderer, y: usize, x: usize) -> u32 {
        // SAFETY: `render_state` is live for the test.
        unsafe {
            (&(*r.render_state).state.row_data)[y]
                .cells
                .get(x)
                .raw
                .codepoint()
        }
    }

    with_test_allocator(|alloc| {
        let (new_result, t) = new_t(alloc, 10, 3);
        assert_eq!(Result::Success, new_result);

        let mut render_state: crate::render::RenderState = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe {
            crate::render::new(Some(alloc), &mut render_state)
        });
        let mut r = Renderer {
            render_state,
            snapshots: 0,
            held: false,
        };
        let rp: *mut Renderer = &mut r;

        assert_eq!(Result::Success, unsafe {
            set(t, Option_::Userdata, rp as *const c_void)
        });
        assert_eq!(Result::Success, unsafe {
            set(t, Option_::RenderHold, on_render_hold as *const c_void)
        });

        // vt_write: the hold begins mid-write; the frame before it is
        // captured and nothing after it.
        vtw(t, b"AB\x1b[?2026h\x1b[HXY");
        unsafe {
            assert_eq!(1, (*rp).snapshots);
            assert!((*rp).held);
            assert_eq!('A' as u32, char(&*rp, 0, 0));
            assert_eq!('B' as u32, char(&*rp, 0, 1));
        }

        // vt_write: the hold ends and the next begins in one write; the
        // finished frame is captured.
        vtw(t, b"Z\x1b[?2026l\x1b[?2026h\x1b[H123");
        unsafe {
            assert_eq!(3, (*rp).snapshots);
            assert!((*rp).held);
            assert_eq!('X' as u32, char(&*rp, 0, 0));
            assert_eq!('Y' as u32, char(&*rp, 0, 1));
            assert_eq!('Z' as u32, char(&*rp, 0, 2));
        }

        // vt_write: restore (XTRESTORE) ends the hold.
        vtw(t, b"\x1b[?2026l\x1b[?2026s\x1b[?2026h\x1b[?2026r");
        unsafe {
            assert_eq!(6, (*rp).snapshots);
            assert!(!(*rp).held);
            assert_eq!('1' as u32, char(&*rp, 0, 0));
        }

        // vt_write: RIS ends the hold.
        vtw(t, b"\x1b[?2026h\x1bc");
        unsafe {
            assert_eq!(8, (*rp).snapshots);
            assert!(!(*rp).held);
            assert_eq!(0, char(&*rp, 0, 0));
        }

        // C resize (through `Handler::resize`) ends the hold.
        vtw(t, b"Q\x1b[?2026h");
        assert_eq!(Result::Success, unsafe { resize(t, 12, 4, 9, 18) });
        unsafe {
            assert_eq!(10, (*rp).snapshots);
            assert!(!(*rp).held);
            let rows = &(*(*rp).render_state).state.row_data;
            assert_eq!(4, rows.len());
            assert_eq!('Q' as u32, char(&*rp, 0, 0));
        }

        // C reset ends the hold.
        vtw(t, b"\x1b[?2026h");
        unsafe { reset(t) };
        unsafe {
            assert_eq!(12, (*rp).snapshots);
            assert!(!(*rp).held);
            assert_eq!(0, char(&*rp, 0, 0));
        }

        unsafe { crate::render::free(render_state) };
        unsafe { free(t) };
    });
}

// ---------------------------------------------------------------------
// Rust-only (P7-W2 T9, N452; Miri targets in the `vtc` group). Effect
// callbacks run synchronously inside `vt_write`, and the reference forbids
// only `vt_write`/`vt_write_until_ground` on the same terminal from one.
// Each test below re-enters one other API class from a callback fired
// while the parser is mid-sequence (a BEL inside a CSI executes as C0 in
// csi_param), and the sequence then completes. Under Miri (SB and TB)
// they prove the raw-self stream drivers and the trampolines'
// `live_handler` leave no protected `&mut Stream`/`&mut Handler` for the
// re-entry to invalidate (`.port/phase-2/mirihar-n452`).
// ---------------------------------------------------------------------

/// The re-entry tests' shared userdata.
#[derive(Default)]
struct Reentry {
    bells: usize,
    pty: Vec<u8>,
    grounds: Vec<bool>,
    semantic_failures: Vec<bool>,
    clipboard_write_max: Vec<usize>,
    continuation_max: Vec<usize>,
    continuation_results: Vec<(Result, Vec<u8>)>,
    /// `reentry_write_pty` also re-enters `set(TITLE_REPORT, true)`
    /// (P7-W2 T10: nested `live_handler` publication on real code).
    pty_sets_title_report: bool,
}

extern "C" fn reentry_write_pty(t: Terminal, ud: *mut c_void, ptr: *const u8, len: usize) {
    // SAFETY: `ud` is the test's live `Reentry`; `ptr[0..len]` is the
    // borrowed response.
    unsafe {
        let data = std::slice::from_raw_parts(ptr, len);
        (*(ud as *mut Reentry)).pty.extend_from_slice(data);
        if (*(ud as *mut Reentry)).pty_sets_title_report {
            let yes = true;
            assert_eq!(
                Result::Success,
                set(t, Option_::TitleReport, (&raw const yes).cast())
            );
        }
    }
}

extern "C" fn reentry_clipboard_write(
    _t: Terminal,
    _ud: *mut c_void,
    _request: *const ClipboardWrite,
) {
}

/// A new 10x3 terminal with `r` as userdata, `write_pty` recording, and
/// `bell` set to `on_bell`.
fn reentry_terminal(alloc: &CAllocator, r: *mut Reentry, on_bell: BellFn) -> Terminal {
    let (new_result, t) = new_t(alloc, 10, 3);
    assert_eq!(Result::Success, new_result);
    assert_eq!(Result::Success, unsafe {
        set(t, Option_::Userdata, r as *const c_void)
    });
    assert_eq!(Result::Success, unsafe {
        set(t, Option_::WritePty, reentry_write_pty as *const c_void)
    });
    assert_eq!(Result::Success, unsafe {
        set(t, Option_::Bell, on_bell as *const c_void)
    });
    t
}

#[test]
fn reentrant_callback_resize() {
    extern "C" fn on_bell(t: Terminal, ud: *mut c_void) {
        // SAFETY: `ud` is the test's live `Reentry`.
        unsafe { (*(ud as *mut Reentry)).bells += 1 };
        // With mode 2048 on, the resize reports the new size in band
        // through `write_pty`: a nested effect from inside the callback.
        assert_eq!(Result::Success, unsafe { resize(t, 20, 6, 9, 18) });
    }
    with_test_allocator(|alloc| {
        let mut r = Reentry::default();
        let rp: *mut Reentry = &mut r;
        let t = reentry_terminal(alloc, rp, on_bell);

        vtw(t, b"\x1b[?2048h");
        unsafe {
            (*rp).pty.clear();
            // The nested `write_pty` re-enters too (P7-W2 T10): a
            // handler-option `set` under two published trampolines.
            (*rp).pty_sets_title_report = true;
            assert!(!(*t).stream.handler.title_report);
        }
        vtw(t, b"\x1b[2\x07;3H");
        unsafe { assert!((*t).stream.handler.title_report) };

        let mut cols: size::CellCountInt = 0;
        let mut rows: size::CellCountInt = 0;
        let mut x: size::CellCountInt = 0;
        let mut y: size::CellCountInt = 0;
        unsafe {
            assert_eq!(1, (*rp).bells);
            assert_eq!(b"\x1b[48;6;20;108;180t".as_slice(), (*rp).pty.as_slice());
            assert_eq!(
                Result::Success,
                get(t, TerminalData::Cols, (&raw mut cols).cast())
            );
            assert_eq!(
                Result::Success,
                get(t, TerminalData::Rows, (&raw mut rows).cast())
            );
            assert_eq!(
                Result::Success,
                get(t, TerminalData::CursorX, (&raw mut x).cast())
            );
            assert_eq!(
                Result::Success,
                get(t, TerminalData::CursorY, (&raw mut y).cast())
            );
        }
        assert_eq!((20, 6), (cols, rows));
        // The CSI resumed after the callback: CUP 2;3 on the new grid.
        assert_eq!((2, 1), (x, y));

        unsafe { free(t) };
    });
}

#[test]
fn reentrant_callback_set_handler_option() {
    extern "C" fn on_bell(t: Terminal, ud: *mut c_void) {
        // SAFETY: `ud` is the test's live `Reentry`.
        unsafe { (*(ud as *mut Reentry)).bells += 1 };
        let yes = true;
        let unknown_max: usize = 5;
        let clipboard_max: usize = 7;
        let name = LibString::init(b"xterm-n452");
        unsafe {
            assert_eq!(
                Result::Success,
                set(t, Option_::TitleReport, (&raw const yes).cast())
            );
            assert_eq!(
                Result::Success,
                set(t, Option_::UnknownMaxBytes, (&raw const unknown_max).cast())
            );
            assert_eq!(
                Result::Success,
                set(
                    t,
                    Option_::ClipboardWriteMaxBytes,
                    (&raw const clipboard_max).cast()
                )
            );
            assert_eq!(
                Result::Success,
                set(t, Option_::TerminfoName, (&raw const name).cast())
            );
            assert_eq!(
                Result::Success,
                set(
                    t,
                    Option_::ClipboardWrite,
                    reentry_clipboard_write as *const c_void
                )
            );
        }
    }
    with_test_allocator(|alloc| {
        let mut r = Reentry::default();
        let rp: *mut Reentry = &mut r;
        let t = reentry_terminal(alloc, rp, on_bell);

        // The bell turns title reports on mid-write; the report later in
        // the same write sees it.
        vtw(t, b"\x1b[1\x07m\x1b]2;hi\x1b\\\x1b[21t");

        let mut clipboard_max: usize = 0;
        unsafe {
            assert_eq!(1, (*rp).bells);
            assert_eq!(b"\x1b]lhi\x1b\\".as_slice(), (*rp).pty.as_slice());
            assert_eq!(
                Result::Success,
                get(
                    t,
                    TerminalData::ClipboardWriteMaxBytes,
                    (&raw mut clipboard_max).cast()
                )
            );
            assert!((*t).stream.handler.title_report);
            assert_eq!(5, (*t).stream.handler.apc_handler.unknown_max_bytes);
            assert_eq!(
                Some(b"xterm-n452".as_slice()),
                (*t).stream.handler.terminfo_name
            );
            assert!((*t).stream.handler.effects.clipboard_write.is_some());
        }
        assert_eq!(7, clipboard_max);

        unsafe { free(t) };
    });
}

#[test]
fn reentrant_callback_get_vt_ground() {
    extern "C" fn on_bell(t: Terminal, ud: *mut c_void) {
        let r = ud as *mut Reentry;
        let mut ground = false;
        let mut failure = true;
        let mut clipboard_max: usize = 0;
        // SAFETY: `ud` is the test's live `Reentry`; `out` pointers are
        // locals of the right types.
        unsafe {
            (*r).bells += 1;
            assert_eq!(
                Result::Success,
                get(t, TerminalData::VtGround, (&raw mut ground).cast())
            );
            assert_eq!(
                Result::Success,
                get(
                    t,
                    TerminalData::VtProcessingError,
                    (&raw mut failure).cast()
                )
            );
            assert_eq!(
                Result::Success,
                get(
                    t,
                    TerminalData::ClipboardWriteMaxBytes,
                    (&raw mut clipboard_max).cast()
                )
            );
            (*r).grounds.push(ground);
            (*r).semantic_failures.push(failure);
            (*r).clipboard_write_max.push(clipboard_max);
        }
    }
    with_test_allocator(|alloc| {
        let mut r = Reentry::default();
        let rp: *mut Reentry = &mut r;
        let t = reentry_terminal(alloc, rp, on_bell);

        // Mid-CSI, then at ground.
        vtw(t, b"\x1b[1\x07mA\x07");

        unsafe {
            assert_eq!(2, (*rp).bells);
            assert_eq!(vec![false, true], (*rp).grounds);
            assert_eq!(vec![false, false], (*rp).semantic_failures);
            assert_eq!(
                vec![rustty_vt::kitty::clipboard::max_write_size; 2],
                (*rp).clipboard_write_max
            );
        }

        unsafe { free(t) };
    });
}

#[test]
fn reentrant_callback_continuation() {
    extern "C" fn on_bell(t: Terminal, ud: *mut c_void) {
        let r = ud as *mut Reentry;
        let mut max: usize = 0;
        let mut buf = [0u8; 64];
        let mut written: usize = 0;
        // SAFETY: `ud` is the test's live `Reentry`; `out` pointers are
        // locals of the right types.
        unsafe {
            (*r).bells += 1;
            assert_eq!(
                Result::Success,
                get(t, TerminalData::ContinuationMaxBytes, (&raw mut max).cast())
            );
            (*r).continuation_max.push(max);
            let result = continuation_buf(t, buf.as_mut_ptr(), buf.len(), &raw mut written);
            (*r).continuation_results
                .push((result, buf[..written].to_vec()));
            // Change the tracking policy mid-feed: shrink the cap on the
            // first bell, disable on the second, re-enable on the third.
            let next: usize = match (*r).bells {
                1 => 32,
                2 => 0,
                _ => 16,
            };
            assert_eq!(
                Result::Success,
                set(t, Option_::ContinuationMaxBytes, (&raw const next).cast())
            );
        }
    }
    with_test_allocator(|alloc| {
        let mut r = Reentry::default();
        let rp: *mut Reentry = &mut r;
        let t = reentry_terminal(alloc, rp, on_bell);
        let max: usize = 64;
        assert_eq!(Result::Success, unsafe {
            set(t, Option_::ContinuationMaxBytes, (&raw const max).cast())
        });

        // A feed that ends mid-sequence leaves a continuation; the next
        // feed's bells run with the parser mid-CSI.
        vtw(t, b"\x1b[1");
        vtw(t, b"\x07;2\x07\x07");

        let mut after: usize = 0;
        let mut buf = [0u8; 64];
        let mut written: usize = 0;
        let result = unsafe {
            assert_eq!(
                Result::Success,
                get(
                    t,
                    TerminalData::ContinuationMaxBytes,
                    (&raw mut after).cast()
                )
            );
            continuation_buf(t, buf.as_mut_ptr(), buf.len(), &raw mut written)
        };
        unsafe {
            assert_eq!(3, (*rp).bells);
            assert_eq!(vec![64, 32, 0], (*rp).continuation_max);
            let seen: Vec<Result> = (*rp).continuation_results.iter().map(|c| c.0).collect();
            assert_eq!(
                vec![Result::Success, Result::Success, Result::InvalidValue],
                seen
            );
            let results = &(*rp).continuation_results;
            assert_eq!(b"\x1b[1".as_slice(), results[0].1.as_slice());
            assert_eq!(b"\x1b[1".as_slice(), results[1].1.as_slice());
        }
        // Re-enabled mid-sequence by the last bell: the prefix is lost,
        // so the continuation is unavailable (`InvalidValue`).
        assert_eq!(16, after);
        assert_eq!(Result::InvalidValue, result);

        unsafe { free(t) };
    });
}

/// The paste reader's userdata (N454).
struct PasteReentry {
    t: Terminal,
    step: u32,
    reads: usize,
}

extern "C" fn reentry_clipboard_read(
    _t: Terminal,
    _ud: *mut c_void,
    _request: *const ClipboardRead,
) {
}

/// A `RusttyMimeReader` that re-enters the terminal before it streams
/// the data.
extern "C" fn reentry_paste_read(
    ud: *mut c_void,
    _mime: LibString,
    writer: crate::io::Writer,
) -> bool {
    let p = ud as *mut PasteReentry;
    // SAFETY: `ud` is the test's live `PasteReentry`; `t` its live
    // terminal; `writer` is valid for this call.
    unsafe {
        (*p).reads += 1;
        let t = (*p).t;
        let data: &[u8] = match (*p).step {
            1 => {
                let yes = true;
                assert_eq!(
                    Result::Success,
                    set(t, Option_::TitleReport, (&raw const yes).cast())
                );
                // Mode 2048: a nested `write_pty` from inside the read.
                assert_eq!(Result::Success, resize(t, 20, 6, 9, 18));
                // A nested feed: the title report sees the `set`, and the
                // BEL fires an effect.
                vtw(t, b"\x1b]2;pt\x1b\\\x1b[21t\x07");
                b"hi"
            }
            2 => {
                // RIS resets the handler (its Kitty clipboard grants
                // too), then the C reset.
                vtw(t, b"\x1bc");
                reset(t);
                b"ok"
            }
            _ => b"ok",
        };
        (writer.write.unwrap())(writer.userdata, data.as_ptr(), data.len())
    }
}

// Rust-only (P7-W2 T10, N454; a Miri target in the `vtc` group). The C
// paste reader runs synchronously inside `terminal_paste`, and the
// reference's `io.h`/`paste.h` do not forbid it from re-entering the
// terminal. Under Miri (SB and TB) this proves the raw-self
// `Handler::paste_raw`, the raw paste `Context` and the paste's published
// `live_handler` leave no protected borrow for the re-entry to
// invalidate (`.port/phase-2/mirihar-n452`, the `n454_*` candidates).
#[test]
fn reentrant_callback_paste_reader() {
    extern "C" fn on_bell(_t: Terminal, ud: *mut c_void) {
        // SAFETY: `ud` is the test's live `Reentry`.
        unsafe { (*(ud as *mut Reentry)).bells += 1 };
    }
    with_test_allocator(|alloc| {
        let mut r = Reentry::default();
        let rp: *mut Reentry = &mut r;
        let t = reentry_terminal(alloc, rp, on_bell);
        // With a clipboard_read callback the paste carries the Kitty
        // clipboard grants across the read.
        assert_eq!(Result::Success, unsafe {
            set(
                t,
                Option_::ClipboardRead,
                reentry_clipboard_read as *const c_void,
            )
        });
        vtw(t, b"\x1b[?2004h\x1b[?2048h");
        unsafe { (*rp).pty.clear() };

        let mut pr = PasteReentry {
            t,
            step: 1,
            reads: 0,
        };
        let pp: *mut PasteReentry = &mut pr;
        let mimes = [LibString::init(b"text/plain")];
        let req = crate::paste::Request {
            size: std::mem::size_of::<crate::paste::Request>(),
            location: 0,
            source: 0,
            mimes: mimes.as_ptr(),
            mimes_len: mimes.len(),
            reader: crate::io::MimeReader {
                read: Some(reentry_paste_read),
                userdata: pp.cast(),
            },
            allow_unsafe: false,
        };

        // 1. The reader sets a handler option, resizes (a nested
        // `write_pty`), and feeds a title report and a BEL.
        let mut written = false;
        assert_eq!(Result::Success, unsafe {
            crate::paste::terminal_paste(t, &req, &mut written)
        });
        assert!(written);
        let mut cols: size::CellCountInt = 0;
        let mut rows: size::CellCountInt = 0;
        unsafe {
            assert_eq!(1, (*pp).reads);
            assert_eq!(1, (*rp).bells);
            assert!((*t).live_handler.is_null());
            assert!((*t).stream.handler.title_report);
            assert_eq!(
                b"\x1b[48;6;20;108;180t\x1b]lpt\x1b\\\x1b[200~hi\x1b[201~".as_slice(),
                (*rp).pty.as_slice()
            );
            assert_eq!(
                Result::Success,
                get(t, TerminalData::Cols, (&raw mut cols).cast())
            );
            assert_eq!(
                Result::Success,
                get(t, TerminalData::Rows, (&raw mut rows).cast())
            );
        }
        assert_eq!((20, 6), (cols, rows));

        // 2. The reader resets the terminal (RIS through a nested feed,
        // which clears the grants, then the C reset). The framing was
        // decided from mode 2004 before the read, as in the reference.
        unsafe {
            let gpa = (*(*(*t).terminal).screens.active).alloc;
            assert!((*t)
                .stream
                .handler
                .kitty_clipboard_grants
                .grant(
                    gpa,
                    b"pw",
                    rustty_vt::kitty::clipboard::Direction::Read,
                    false
                )
                .is_ok());
            assert_eq!(1, (*t).stream.handler.kitty_clipboard_grants.len());
            (*rp).pty.clear();
            (*pp).step = 2;
        }
        assert_eq!(Result::Success, unsafe {
            crate::paste::terminal_paste(t, &req, &mut written)
        });
        unsafe {
            assert_eq!(2, (*pp).reads);
            assert_eq!(0, (*t).stream.handler.kitty_clipboard_grants.len());
            assert_eq!(b"\x1b[200~ok\x1b[201~".as_slice(), (*rp).pty.as_slice());
            (*rp).pty.clear();
            (*pp).step = 0;
        }

        // 3. The reset took effect: no bracketing now.
        assert_eq!(Result::Success, unsafe {
            crate::paste::terminal_paste(t, &req, &mut written)
        });
        unsafe {
            assert_eq!(3, (*pp).reads);
            assert_eq!(b"ok".as_slice(), (*rp).pty.as_slice());
        }

        unsafe { free(t) };
    });
}

// ---------------------------------------------------------------------
// Rust-only (P8-W5 T3; R2 §2.9; Miri targets, filter `callback`). The four
// effects this task adds hand the callback text that borrows the OSC
// parser's buffer or capture inside `wrapper.stream.parser` (unknown OSC
// content, program-status id and app, the semantic-prompt error), or this
// frame's buffers (program-status title and message, the decoded command
// line). Each callback below, fired from `vt_write`, re-enters the same
// terminal while it holds that text: it reads the title and the memory
// usage (`rustty_terminal_get`) and sets UNKNOWN_MAX_BYTES, which writes
// the OSC parser's own limit. Only then does it read the borrowed strings.
// Under Miri (SB and TB) this proves the trampolines' `live_handler` and the
// raw place write of the parser limit leave the borrowed bytes valid.
// ---------------------------------------------------------------------

/// What the four re-entry callbacks recorded.
#[derive(Default)]
struct OscReentry {
    calls: usize,
    titles: Vec<Vec<u8>>,
    primary_pages: Vec<u64>,
    strings: Vec<Vec<u8>>,
}

/// Re-enter `t` the way the callbacks above do: read the title and the
/// memory usage, then set the shared unknown-sequence limit to 3.
///
/// # Safety
/// `t` is the live terminal the callback was given; `r` is the test's live
/// `OscReentry`.
unsafe fn osc_reenter(t: Terminal, r: *mut OscReentry) {
    let mut title = LibString::init(b"");
    assert_eq!(Result::Success, unsafe {
        get(t, TerminalData::Title, (&raw mut title).cast())
    });
    let title = unsafe { core::slice::from_raw_parts(title.ptr, title.len) }.to_vec();
    let mut usage = TerminalMemoryUsage::default();
    assert_eq!(Result::Success, unsafe {
        get(t, TerminalData::MemoryUsage, (&raw mut usage).cast())
    });
    let max_bytes: usize = 3;
    assert_eq!(Result::Success, unsafe {
        set(t, Option_::UnknownMaxBytes, (&raw const max_bytes).cast())
    });
    unsafe {
        (*r).calls += 1;
        (*r).titles.push(title);
        (*r).primary_pages.push(usage.primary_pages);
    }
}

/// # Safety
/// `s` is a borrowed string valid for the callback.
unsafe fn osc_reentry_copy(r: *mut OscReentry, s: LibString) {
    let bytes = if s.len == 0 {
        Vec::new()
    } else {
        unsafe { core::slice::from_raw_parts(s.ptr, s.len) }.to_vec()
    };
    unsafe { (*r).strings.push(bytes) };
}

/// A new 10x3 terminal with `r` as userdata and the title "t".
fn osc_reentry_terminal(alloc: &CAllocator, r: *mut OscReentry) -> Terminal {
    let (new_result, t) = new_t(alloc, 10, 3);
    assert_eq!(Result::Success, new_result);
    assert_eq!(Result::Success, unsafe {
        set(t, Option_::Userdata, r as *const c_void)
    });
    vtw(t, b"\x1b]2;t\x07");
    t
}

#[test]
fn reentrant_callback_unknown_osc() {
    extern "C" fn on_unknown(t: Terminal, ud: *mut c_void, sequence: *const UnknownSequence) {
        let r = ud as *mut OscReentry;
        unsafe {
            osc_reenter(t, r);
            assert_eq!(UnknownSequenceTag::Osc, (*sequence).tag);
            osc_reentry_copy(r, (*sequence).value.osc.content);
        }
    }
    with_test_allocator(|alloc| {
        let mut r = OscReentry::default();
        let rp: *mut OscReentry = &mut r;
        let t = osc_reentry_terminal(alloc, rp);
        let max_bytes: usize = 8;
        assert_eq!(Result::Success, unsafe {
            set(t, Option_::UnknownMaxBytes, (&raw const max_bytes).cast())
        });
        assert_eq!(Result::Success, unsafe {
            set(t, Option_::UnknownSequence, on_unknown as *const c_void)
        });

        // The callback lowers the limit while it holds the content; the
        // content it then reads is the sequence it was given, and the next
        // sequence is captured at the new limit.
        vtw(t, b"\x1b]7400;abcdef\x07\x1b]7400;abcdef\x1b\\");
        unsafe {
            assert_eq!(2, (*rp).calls);
            assert_eq!(vec![b"t".to_vec(), b"t".to_vec()], (*rp).titles);
            assert!((*rp).primary_pages.iter().all(|&p| p > 0));
            assert_eq!(vec![b"7400;abc".to_vec(), b"740".to_vec()], (*rp).strings);
            assert_eq!(3, (*t).stream.parser.osc_parser.unknown_max_bytes);
            assert_eq!(3, (*t).stream.handler.apc_handler.unknown_max_bytes);
        }
        unsafe { free(t) };
    });
}

#[test]
fn reentrant_callback_program_status() {
    extern "C" fn on_program_status(t: Terminal, ud: *mut c_void, report: *const ProgramStatus) {
        let r = ud as *mut OscReentry;
        unsafe {
            osc_reenter(t, r);
            osc_reentry_copy(r, (*report).id);
            osc_reentry_copy(r, (*report).app);
            osc_reentry_copy(r, (*report).title);
            osc_reentry_copy(r, (*report).message);
        }
    }
    with_test_allocator(|alloc| {
        let mut r = OscReentry::default();
        let rp: *mut OscReentry = &mut r;
        let t = osc_reentry_terminal(alloc, rp);
        assert_eq!(Result::Success, unsafe {
            set(
                t,
                Option_::ProgramStatus,
                on_program_status as *const c_void,
            )
        });

        vtw(
            t,
            b"\x1b]7501;state=blocked:id=a/b:app=terraform:title=UGxhbg==:msg=QXBwbHk/\x07",
        );
        unsafe {
            assert_eq!(1, (*rp).calls);
            assert_eq!(vec![b"t".to_vec()], (*rp).titles);
            assert_eq!(
                vec![
                    b"a/b".to_vec(),
                    b"terraform".to_vec(),
                    b"Plan".to_vec(),
                    b"Apply?".to_vec(),
                ],
                (*rp).strings
            );
            assert_eq!(3, (*t).stream.parser.osc_parser.unknown_max_bytes);
        }
        unsafe { free(t) };
    });
}

#[test]
fn reentrant_callback_semantic_prompt() {
    extern "C" fn on_semantic_prompt(t: Terminal, ud: *mut c_void, event: *const SemanticPrompt) {
        let r = ud as *mut OscReentry;
        unsafe {
            osc_reenter(t, r);
            osc_reentry_copy(r, (*event).command);
            osc_reentry_copy(r, (*event).error);
        }
    }
    with_test_allocator(|alloc| {
        let mut r = OscReentry::default();
        let rp: *mut OscReentry = &mut r;
        let t = osc_reentry_terminal(alloc, rp);
        assert_eq!(Result::Success, unsafe {
            set(
                t,
                Option_::SemanticPrompt,
                on_semantic_prompt as *const c_void,
            )
        });

        vtw(
            t,
            b"\x1b]133;C;cmdline_url=ls%20-la\x07\x1b]133;D;1;err=boom\x1b\\",
        );
        unsafe {
            assert_eq!(2, (*rp).calls);
            assert_eq!(
                vec![b"ls -la".to_vec(), Vec::new(), Vec::new(), b"boom".to_vec(),],
                (*rp).strings
            );
        }
        unsafe { free(t) };
    });
}

#[test]
fn reentrant_callback_reset() {
    extern "C" fn on_reset(t: Terminal, ud: *mut c_void) {
        unsafe { osc_reenter(t, ud as *mut OscReentry) };
    }
    with_test_allocator(|alloc| {
        let mut r = OscReentry::default();
        let rp: *mut OscReentry = &mut r;
        let t = osc_reentry_terminal(alloc, rp);
        assert_eq!(Result::Success, unsafe {
            set(t, Option_::Reset, on_reset as *const c_void)
        });

        // RIS clears the title before the callback; text after it in the
        // same write still lands.
        vtw(t, b"\x1bcAB");
        unsafe {
            assert_eq!(1, (*rp).calls);
            assert_eq!(vec![Vec::<u8>::new()], (*rp).titles);
            assert_eq!(3, (*t).stream.parser.osc_parser.unknown_max_bytes);
        }
        let mut x: size::CellCountInt = 0;
        assert_eq!(Result::Success, unsafe {
            get(t, TerminalData::CursorX, (&raw mut x).cast())
        });
        assert_eq!(2, x);
        unsafe { free(t) };
    });
}

// test "title_changed without callback is silent"
#[test]
fn title_changed_without_callback_is_silent() {
    with_test_allocator(|alloc| {
        // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);
        // OSC 2 without a callback should not crash
        vtw(t, b"\x1B]2;Hello\x1B\\");
        unsafe { free(t) };
    });
}

// test "kitty_image_medium_temp_file empty output"
#[test]
#[cfg_attr(
    not(feature = "kitty-graphics"),
    ignore = "SkipZigTest: needs the kitty-graphics vt feature (R14)"
)]
fn kitty_image_medium_temp_file_empty_output() {
    // if (comptime !build_options.kitty_graphics) return error.SkipZigTest;
    // PORT NOTE (R14): the skip is the `cfg_attr(ignore)` above.
    with_test_allocator(|alloc| {
        // var t: Terminal = null;
        // try testing.expectEqual(Result.success, new(&lib.alloc.test_allocator, &t, 80, 24));
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);
        // defer free(t);
        struct FreeTerminal(Terminal);
        impl Drop for FreeTerminal {
            fn drop(&mut self) {
                unsafe { free(self.0) };
            }
        }
        let _free_t = FreeTerminal(t);

        // const empty: lib.String = .{ .ptr = "", .len = 0 };
        let empty = LibString {
            ptr: rustty_lib::EMPTY.as_ptr(),
            len: 0,
        };
        // try testing.expectEqual(Result.success, set(t, .kitty_image_medium_temp_file, &empty));
        assert_eq!(Result::Success, unsafe {
            set(
                t,
                Option_::KittyImageMediumTempFile,
                core::ptr::addr_of!(empty) as *const c_void,
            )
        });
        // var result: lib.String = undefined;
        let mut result = LibString {
            ptr: core::ptr::null(),
            len: usize::MAX,
        };
        // try testing.expectEqual(Result.success, get(t, .kitty_image_medium_temp_file, &result));
        assert_eq!(Result::Success, unsafe {
            get(
                t,
                TerminalData::KittyImageMediumTempFile,
                core::ptr::addr_of_mut!(result) as *mut c_void,
            )
        });
        // try testing.expectEqual(@as(usize, 0), result.len);
        assert_eq!(0usize, result.len);
        // try testing.expectEqual(@as([*]const u8, ""), result.ptr);
        // PORT NOTE (DECISIONS B1): Zig's `""` is `rustty_lib::EMPTY`.
        assert_eq!(rustty_lib::EMPTY.as_ptr(), result.ptr);
    });
}

// test "set pwd_changed callback"
#[test]
fn set_pwd_changed_callback() {
    static mut PWD_COUNT: usize = 0;
    static mut LAST_USERDATA: *mut c_void = std::ptr::null_mut();
    extern "C" fn pwd_changed(_t: Terminal, ud: *mut c_void) {
        unsafe {
            PWD_COUNT += 1;
            LAST_USERDATA = ud;
        }
    }

    with_test_allocator(|alloc| {
        // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);
        unsafe {
            PWD_COUNT = 0;
            LAST_USERDATA = std::ptr::null_mut();
        }

        let mut sentinel: u8 = 88;
        assert_eq!(Result::Success, unsafe {
            set(
                t,
                Option_::Userdata,
                &mut sentinel as *mut u8 as *const c_void,
            )
        });
        assert_eq!(Result::Success, unsafe {
            set(t, Option_::PwdChanged, pwd_changed as *const c_void)
        });

        // OSC 7 ; file:///tmp ST — report pwd
        vtw(t, b"\x1B]7;file:///tmp\x1B\\");
        assert_eq!(1usize, unsafe { PWD_COUNT });
        assert_eq!(&mut sentinel as *mut u8 as *mut c_void, unsafe {
            LAST_USERDATA
        });
        assert_eq!(b"file:///tmp".as_slice(), unsafe {
            (*zig_terminal(t)).get_pwd().unwrap()
        });

        // Another pwd change
        vtw(t, b"\x1B]7;file:///home/user\x1B\\");
        assert_eq!(2usize, unsafe { PWD_COUNT });
        assert_eq!(b"file:///home/user".as_slice(), unsafe {
            (*zig_terminal(t)).get_pwd().unwrap()
        });
        unsafe { free(t) };
    });
}

// test "set desktop_notification callback" (oracle c3655ba25)
#[test]
fn set_desktop_notification_callback() {
    static mut COUNT: usize = 0;
    static mut LAST_TERMINAL: Terminal = std::ptr::null_mut();
    static mut LAST_USERDATA: *mut c_void = std::ptr::null_mut();
    static mut LAST_SIZE: usize = 0;
    // PER THE BYTE-SLICE RULE: `Vec<u8>`, never `String`. The oracle keeps
    // `[64]u8` + a length; the borrowed slices are raw pty bytes.
    static mut LAST_TITLE: Vec<u8> = Vec::new();
    static mut LAST_BODY: Vec<u8> = Vec::new();

    extern "C" fn desktop_notification(
        terminal_: Terminal,
        ud: *mut c_void,
        n: *const DesktopNotification,
    ) {
        unsafe {
            COUNT += 1;
            LAST_TERMINAL = terminal_;
            LAST_USERDATA = ud;
            LAST_SIZE = (*n).size;
            LAST_TITLE = std::slice::from_raw_parts((*n).title.ptr, (*n).title.len).to_vec();
            LAST_BODY = std::slice::from_raw_parts((*n).body.ptr, (*n).body.len).to_vec();
        }
    }

    with_test_allocator(|alloc| {
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);
        unsafe {
            COUNT = 0;
            LAST_TERMINAL = std::ptr::null_mut();
            LAST_USERDATA = std::ptr::null_mut();
        }

        let mut sentinel: u8 = 88;
        assert_eq!(Result::Success, unsafe {
            set(
                t,
                Option_::Userdata,
                &mut sentinel as *mut u8 as *const c_void,
            )
        });
        assert_eq!(Result::Success, unsafe {
            set(
                t,
                Option_::DesktopNotification,
                desktop_notification as *const c_void,
            )
        });

        // Split OSC 777 across writes to exercise the persistent VT parser.
        vtw(t, b"\x1B]777;notify;Codex;");
        assert_eq!(0usize, unsafe { COUNT });
        vtw(t, b"Needs attention\x1B\\");
        assert_eq!(1usize, unsafe { COUNT });
        assert_eq!(t, unsafe { LAST_TERMINAL });
        assert_eq!(&mut sentinel as *mut u8 as *mut c_void, unsafe {
            LAST_USERDATA
        });
        // The sized-struct contract: `size` is `size_of` the struct rustty
        // built, so a header/library ABI drift shows up here.
        assert_eq!(std::mem::size_of::<DesktopNotification>(), unsafe {
            LAST_SIZE
        });
        assert_eq!(b"Codex".as_slice(), unsafe {
            (&*std::ptr::addr_of!(LAST_TITLE)).as_slice()
        });
        assert_eq!(b"Needs attention".as_slice(), unsafe {
            (&*std::ptr::addr_of!(LAST_BODY)).as_slice()
        });

        // OSC 9 has no title and preserves its body.
        vtw(t, b"\x1B]9;Build complete\x07");
        assert_eq!(2usize, unsafe { COUNT });
        assert_eq!(b"".as_slice(), unsafe {
            (&*std::ptr::addr_of!(LAST_TITLE)).as_slice()
        });
        assert_eq!(b"Build complete".as_slice(), unsafe {
            (&*std::ptr::addr_of!(LAST_BODY)).as_slice()
        });

        // Removing the callback takes effect immediately.
        assert_eq!(Result::Success, unsafe {
            set(t, Option_::DesktopNotification, std::ptr::null())
        });
        vtw(t, b"\x1B]9;Build complete\x07");
        assert_eq!(2usize, unsafe { COUNT });

        unsafe { free(t) };
    });
}

// test "set progress_report callback" (oracle 47d602c42)
#[test]
fn set_progress_report_callback() {
    static mut COUNT: usize = 0;
    static mut LAST_TERMINAL: Terminal = std::ptr::null_mut();
    static mut LAST_USERDATA: *mut c_void = std::ptr::null_mut();
    static mut LAST_SIZE: usize = 0;
    static mut LAST_STATE: ProgressState = ProgressState::Remove;
    static mut LAST_PROGRESS: i8 = 0;

    extern "C" fn progress_report(terminal_: Terminal, ud: *mut c_void, r: *const ProgressReport) {
        unsafe {
            COUNT += 1;
            LAST_TERMINAL = terminal_;
            LAST_USERDATA = ud;
            LAST_SIZE = (*r).size;
            LAST_STATE = (*r).state;
            LAST_PROGRESS = (*r).progress;
        }
    }

    with_test_allocator(|alloc| {
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);
        // S.count = 0; S.last_userdata = null; S.last_size = 0;
        // S.last_state = .remove; S.last_progress = -1;
        unsafe {
            COUNT = 0;
            LAST_TERMINAL = std::ptr::null_mut();
            LAST_USERDATA = std::ptr::null_mut();
            LAST_SIZE = 0;
            LAST_STATE = ProgressState::Remove;
            LAST_PROGRESS = -1;
        }

        let mut sentinel: u8 = 88;
        assert_eq!(Result::Success, unsafe {
            set(
                t,
                Option_::Userdata,
                &mut sentinel as *mut u8 as *const c_void,
            )
        });
        assert_eq!(Result::Success, unsafe {
            set(t, Option_::ProgressReport, progress_report as *const c_void)
        });

        // const cases = [_]struct { sequence, state, progress }{
        //     .{ "\x1B]9;4;0;\x1B\\",   .remove,        -1 },
        //     .{ "\x1B]9;4;1;42\x07",   .set,           42 },
        //     .{ "\x1B]9;4;2;7\x1B\\",  .@"error",       7 },
        //     .{ "\x1B]9;4;3\x1B\\",    .indeterminate, -1 },
        //     .{ "\x1B]9;4;4;75\x1B\\", .pause,         75 },
        // };
        // Every state the ABI can carry, including the BEL-terminated form.
        let cases: [(&[u8], ProgressState, i8); 5] = [
            (b"\x1B]9;4;0;\x1B\\", ProgressState::Remove, -1),
            (b"\x1B]9;4;1;42\x07", ProgressState::Set, 42),
            (b"\x1B]9;4;2;7\x1B\\", ProgressState::Error, 7),
            (b"\x1B]9;4;3\x1B\\", ProgressState::Indeterminate, -1),
            (b"\x1B]9;4;4;75\x1B\\", ProgressState::Pause, 75),
        ];

        // for (cases, 1..) |case, expected_count| { ... }
        for (i, (sequence, state, progress)) in cases.iter().enumerate() {
            let expected_count = i + 1;
            // Split at the midpoint to exercise the persistent VT parser.
            let midpoint = sequence.len() / 2;
            vtw(t, &sequence[..midpoint]);
            assert_eq!(expected_count - 1, unsafe { COUNT });
            vtw(t, &sequence[midpoint..]);
            assert_eq!(expected_count, unsafe { COUNT });
            assert_eq!(t, unsafe { LAST_TERMINAL });
            assert_eq!(&mut sentinel as *mut u8 as *mut c_void, unsafe {
                LAST_USERDATA
            });
            // The sized-struct contract: a header/library ABI drift shows here.
            assert_eq!(std::mem::size_of::<ProgressReport>(), unsafe { LAST_SIZE });
            assert_eq!(*state, unsafe { LAST_STATE });
            // An omitted percentage crosses the ABI as -1, not as 0.
            assert_eq!(*progress, unsafe { LAST_PROGRESS });
        }

        // Removing the callback takes effect immediately.
        assert_eq!(Result::Success, unsafe {
            set(t, Option_::ProgressReport, std::ptr::null())
        });
        vtw(t, b"\x1B]9;4;1;90\x1B\\");
        assert_eq!(cases.len(), unsafe { COUNT });

        unsafe { free(t) };
    });
}

// test "set program_status callback"
#[test]
fn set_program_status_callback() {
    // const S = struct { var count ... };
    static mut COUNT: usize = 0;
    static mut LAST_USERDATA: *mut c_void = std::ptr::null_mut();
    static mut LAST_SIZE: usize = 0;
    static mut LAST_STATE: ProgramStatusState = ProgramStatusState::Idle;
    static mut LAST_KIND: ProgramStatusKind = ProgramStatusKind::None;
    static mut LAST_PROGRESS: i8 = -1;
    static mut LAST_ID: Vec<u8> = Vec::new();
    static mut LAST_APP: Vec<u8> = Vec::new();
    static mut LAST_TITLE: Vec<u8> = Vec::new();
    static mut LAST_MESSAGE: Vec<u8> = Vec::new();
    static mut WRITTEN: Vec<u8> = Vec::new();

    // fn copy(dst: []u8, src: lib.String) usize { ... }
    // PORT NOTE: the fixed `[64]u8` buffers + lengths become `Vec`s (the
    // crate's test-capture convention).
    unsafe fn copy(dst: *mut Vec<u8>, src: LibString) {
        unsafe {
            (*dst).clear();
            if src.len > 0 {
                (*dst).extend_from_slice(core::slice::from_raw_parts(src.ptr, src.len));
            }
        }
    }

    extern "C" fn program_status(_: Terminal, ud: *mut c_void, report: *const ProgramStatus) {
        unsafe {
            COUNT += 1;
            LAST_USERDATA = ud;
            LAST_SIZE = (*report).size;
            LAST_STATE = (*report).state;
            LAST_KIND = (*report).kind;
            LAST_PROGRESS = (*report).progress;
            copy(&raw mut LAST_ID, (*report).id);
            copy(&raw mut LAST_APP, (*report).app);
            copy(&raw mut LAST_TITLE, (*report).title);
            copy(&raw mut LAST_MESSAGE, (*report).message);
        }
    }

    extern "C" fn write_pty(_: Terminal, _: *mut c_void, ptr: *const u8, len: usize) {
        unsafe {
            let written: *mut Vec<u8> = &raw mut WRITTEN;
            (*written).extend_from_slice(core::slice::from_raw_parts(ptr, len));
        }
    }
    fn get(v: *const Vec<u8>) -> Vec<u8> {
        unsafe { (*v).clone() }
    }

    with_test_allocator(|alloc| {
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);
        // S.count = 0;
        // S.written_len = 0;
        unsafe {
            COUNT = 0;
            let written: *mut Vec<u8> = &raw mut WRITTEN;
            (*written).clear();
        }

        // var sentinel: u8 = 100;
        let mut sentinel: u8 = 100;
        assert_eq!(Result::Success, unsafe {
            set(t, Option_::Userdata, (&raw mut sentinel).cast())
        });
        assert_eq!(Result::Success, unsafe {
            set(t, Option_::WritePty, write_pty as *const c_void)
        });

        // Without the callback, the support query is not answered.
        let query = b"\x1B]7501;?\x1B\\";
        vtw(t, query);
        assert_eq!(0, get(&raw const WRITTEN).len());

        assert_eq!(Result::Success, unsafe {
            set(t, Option_::ProgramStatus, program_status as *const c_void)
        });

        // With the callback, it is.
        vtw(t, query);
        assert_eq!(query.as_slice(), get(&raw const WRITTEN).as_slice());
        assert_eq!(0, unsafe { COUNT });

        // "Plan" and "Apply?"
        let report: &[u8] = b"\x1B]7501;state=blocked:kind=permission:progress=40:id=a/b\
:app=terraform:title=UGxhbg==:msg=QXBwbHk/\x07";
        vtw(t, report);
        assert_eq!(1, unsafe { COUNT });
        assert_eq!((&raw mut sentinel).cast::<c_void>(), unsafe {
            LAST_USERDATA
        });
        assert_eq!(size_of::<ProgramStatus>(), unsafe { LAST_SIZE });
        assert_eq!(ProgramStatusState::Blocked, unsafe { LAST_STATE });
        assert_eq!(ProgramStatusKind::Permission, unsafe { LAST_KIND });
        assert_eq!(40i8, unsafe { LAST_PROGRESS });
        assert_eq!(b"a/b".as_slice(), get(&raw const LAST_ID).as_slice());
        assert_eq!(b"terraform".as_slice(), get(&raw const LAST_APP).as_slice());
        assert_eq!(b"Plan".as_slice(), get(&raw const LAST_TITLE).as_slice());
        assert_eq!(
            b"Apply?".as_slice(),
            get(&raw const LAST_MESSAGE).as_slice()
        );

        // Absent values are empty, none, or -1.
        let minimal = b"\x1B]7501;state=done\x1B\\";
        vtw(t, minimal);
        assert_eq!(2, unsafe { COUNT });
        assert_eq!(ProgramStatusState::Done, unsafe { LAST_STATE });
        assert_eq!(ProgramStatusKind::None, unsafe { LAST_KIND });
        assert_eq!(-1i8, unsafe { LAST_PROGRESS });
        assert_eq!(0, get(&raw const LAST_ID).len());
        assert_eq!(0, get(&raw const LAST_APP).len());
        assert_eq!(0, get(&raw const LAST_TITLE).len());
        assert_eq!(0, get(&raw const LAST_MESSAGE).len());

        // Unsetting the callback ignores reports and the query again.
        assert_eq!(Result::Success, unsafe {
            set(t, Option_::ProgramStatus, std::ptr::null())
        });
        unsafe {
            let written: *mut Vec<u8> = &raw mut WRITTEN;
            (*written).clear();
        }
        vtw(t, query);
        vtw(t, minimal);
        assert_eq!(0, get(&raw const WRITTEN).len());
        assert_eq!(2, unsafe { COUNT });

        unsafe { free(t) };
    });
}

// test "set semantic_prompt callback"
#[test]
fn set_semantic_prompt_callback() {
    static mut COUNT: usize = 0;
    static mut LAST_USERDATA: *mut c_void = std::ptr::null_mut();
    static mut LAST_SIZE: usize = 0;
    static mut LAST_KIND: SemanticPromptKind = SemanticPromptKind::Invalid;
    static mut LAST_PROMPT_KIND: SemanticPromptPromptKind = SemanticPromptPromptKind::Primary;
    static mut LAST_HAS_EXIT_CODE: bool = false;
    static mut LAST_EXIT_CODE: i32 = 0;
    static mut COMMAND: Vec<u8> = Vec::new();
    static mut ERROR: Vec<u8> = Vec::new();

    extern "C" fn semantic_prompt(_: Terminal, ud: *mut c_void, event: *const SemanticPrompt) {
        unsafe {
            COUNT += 1;
            LAST_USERDATA = ud;
            LAST_SIZE = (*event).size;
            LAST_KIND = (*event).kind;
            LAST_PROMPT_KIND = (*event).prompt_kind;
            LAST_HAS_EXIT_CODE = (*event).has_exit_code;
            LAST_EXIT_CODE = (*event).exit_code;
            let command = (*event).command;
            let command_out: *mut Vec<u8> = &raw mut COMMAND;
            *command_out = core::slice::from_raw_parts(command.ptr, command.len).to_vec();
            let error = (*event).error;
            let error_out: *mut Vec<u8> = &raw mut ERROR;
            *error_out = core::slice::from_raw_parts(error.ptr, error.len).to_vec();
        }
    }
    fn get(v: *const Vec<u8>) -> Vec<u8> {
        unsafe { (*v).clone() }
    }

    with_test_allocator(|alloc| {
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);
        // S.count = 0; S.last_userdata = null; S.last_size = 0;
        unsafe {
            COUNT = 0;
            LAST_USERDATA = std::ptr::null_mut();
            LAST_SIZE = 0;
        }

        let mut sentinel: u8 = 100;
        assert_eq!(Result::Success, unsafe {
            set(t, Option_::Userdata, (&raw mut sentinel).cast())
        });
        assert_eq!(Result::Success, unsafe {
            set(t, Option_::SemanticPrompt, semantic_prompt as *const c_void)
        });

        let prompt = b"\x1B]133;P;k=r\x07";
        vtw(t, prompt);
        assert_eq!(1, unsafe { COUNT });
        assert_eq!((&raw mut sentinel).cast::<c_void>(), unsafe {
            LAST_USERDATA
        });
        assert_eq!(size_of::<SemanticPrompt>(), unsafe { LAST_SIZE });
        assert_eq!(SemanticPromptKind::PromptStart, unsafe { LAST_KIND });
        assert_eq!(SemanticPromptPromptKind::Right, unsafe { LAST_PROMPT_KIND });
        assert!(!unsafe { LAST_HAS_EXIT_CODE });

        let input = b"\x1B]133;B\x07";
        vtw(t, input);
        assert_eq!(2, unsafe { COUNT });
        assert_eq!(SemanticPromptKind::InputStart, unsafe { LAST_KIND });
        assert_eq!(SemanticPromptPromptKind::Primary, unsafe {
            LAST_PROMPT_KIND
        });

        let output = b"\x1B]133;C;cmdline_url=ls%20-la\x07";
        vtw(t, output);
        assert_eq!(3, unsafe { COUNT });
        assert_eq!(SemanticPromptKind::OutputStart, unsafe { LAST_KIND });
        assert_eq!(b"ls -la".as_slice(), get(&raw const COMMAND).as_slice());

        let end = b"\x1B]133;D;-1;err=boom\x07";
        vtw(t, end);
        assert_eq!(4, unsafe { COUNT });
        assert_eq!(SemanticPromptKind::CommandEnd, unsafe { LAST_KIND });
        assert!(unsafe { LAST_HAS_EXIT_CODE });
        assert_eq!(-1i32, unsafe { LAST_EXIT_CODE });
        assert_eq!(b"boom".as_slice(), get(&raw const ERROR).as_slice());
        assert_eq!(0, get(&raw const COMMAND).len());

        let end_no_code = b"\x1B]133;D\x07";
        vtw(t, end_no_code);
        assert_eq!(5, unsafe { COUNT });
        assert!(!unsafe { LAST_HAS_EXIT_CODE });
        assert_eq!(0i32, unsafe { LAST_EXIT_CODE });
        assert_eq!(0, get(&raw const ERROR).len());

        assert_eq!(Result::Success, unsafe {
            set(t, Option_::SemanticPrompt, std::ptr::null())
        });
        vtw(t, prompt);
        assert_eq!(5, unsafe { COUNT });

        unsafe { free(t) };
    });
}

// test "set reset callback"
#[test]
fn set_reset_callback() {
    // N541: this test reaches `sys::log_fn`; hold the sys guard so its
    // warnings cannot land in another test's log sink.
    let _guard = crate::sys::tests::reset_global();
    static mut COUNT: usize = 0;
    static mut LAST_USERDATA: *mut c_void = std::ptr::null_mut();

    extern "C" fn reset_cb(_: Terminal, ud: *mut c_void) {
        unsafe {
            COUNT += 1;
            LAST_USERDATA = ud;
        }
    }

    with_test_allocator(|alloc| {
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);
        unsafe {
            COUNT = 0;
            LAST_USERDATA = std::ptr::null_mut();
        }

        let mut sentinel: u8 = 100;
        assert_eq!(Result::Success, unsafe {
            set(t, Option_::Userdata, (&raw mut sentinel).cast())
        });
        assert_eq!(Result::Success, unsafe {
            set(t, Option_::Reset, reset_cb as *const c_void)
        });

        // A soft reset (DECSTR) doesn't report a reset.
        let soft = b"\x1B[!p";
        vtw(t, soft);
        assert_eq!(0, unsafe { COUNT });

        let full = b"\x1Bc";
        vtw(t, full);
        assert_eq!(1, unsafe { COUNT });
        assert_eq!((&raw mut sentinel).cast::<c_void>(), unsafe {
            LAST_USERDATA
        });

        assert_eq!(Result::Success, unsafe {
            set(t, Option_::Reset, std::ptr::null())
        });
        vtw(t, full);
        assert_eq!(1, unsafe { COUNT });

        unsafe { free(t) };
    });
}

// test "set unknown_sequence callback"
#[test]
fn set_unknown_sequence_callback() {
    // N541: this test reaches `sys::log_fn`; hold the sys guard so its
    // warnings cannot land in another test's log sink.
    let _guard = crate::sys::tests::reset_global();
    static mut COUNT: usize = 0;
    static mut LAST_TERMINAL: Terminal = std::ptr::null_mut();
    static mut LAST_USERDATA: *mut c_void = std::ptr::null_mut();
    static mut LAST_TAG: UnknownSequenceTag = UnknownSequenceTag::Apc;
    static mut LAST_TRUNCATED: bool = false;
    static mut LAST_TERMINATOR: crate::osc::Terminator = crate::osc::Terminator::St;
    static mut CONTENT: [u8; 64] = [0; 64];
    static mut CONTENT_LEN: usize = 0;

    extern "C" fn unknown_sequence(
        terminal_: Terminal,
        ud: *mut c_void,
        sequence: *const UnknownSequence,
    ) {
        unsafe {
            COUNT += 1;
            LAST_TERMINAL = terminal_;
            LAST_USERDATA = ud;
            LAST_TAG = (*sequence).tag;
            // const str: lib.String = switch (sequence.tag) {
            //     .apc => str: {
            //         const apc_value = sequence.value.apc;
            //         last_truncated = apc_value.truncated;
            //         break :str apc_value.content;
            //     },
            //     .osc => str: {
            //         const osc_value = sequence.value.osc;
            //         last_truncated = osc_value.truncated;
            //         last_terminator = osc_value.terminator;
            //         break :str osc_value.content;
            //     },
            // };
            let str = match (*sequence).tag {
                UnknownSequenceTag::Apc => {
                    let apc_value = (*sequence).value.apc;
                    LAST_TRUNCATED = apc_value.truncated;
                    apc_value.content
                }
                UnknownSequenceTag::Osc => {
                    let osc_value = (*sequence).value.osc;
                    LAST_TRUNCATED = osc_value.truncated;
                    LAST_TERMINATOR = osc_value.terminator;
                    osc_value.content
                }
            };
            // content_len = @min(str.len, content.len);
            CONTENT_LEN = core::cmp::min(str.len, 64);
            // @memcpy(content[0..content_len], str.ptr[0..content_len]);
            core::ptr::copy_nonoverlapping(
                str.ptr,
                core::ptr::addr_of_mut!(CONTENT) as *mut u8,
                CONTENT_LEN,
            );
        }
    }
    fn content() -> &'static [u8] {
        unsafe {
            core::slice::from_raw_parts(core::ptr::addr_of!(CONTENT) as *const u8, CONTENT_LEN)
        }
    }

    with_test_allocator(|alloc| {
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);
        // S.count = 0; ... (the reference resets its statics)
        unsafe {
            COUNT = 0;
            LAST_TERMINAL = std::ptr::null_mut();
            LAST_USERDATA = std::ptr::null_mut();
            LAST_TAG = UnknownSequenceTag::Apc;
            LAST_TRUNCATED = false;
            CONTENT_LEN = 0;
        }

        // var sentinel: u8 = 101;
        let mut sentinel: u8 = 101;
        assert_eq!(Result::Success, unsafe {
            set(
                t,
                Option_::Userdata,
                &mut sentinel as *mut u8 as *const c_void,
            )
        });

        // const max_bytes: usize = 8;
        let max_bytes: usize = 8;
        assert_eq!(Result::Success, unsafe {
            set(
                t,
                Option_::UnknownMaxBytes,
                &max_bytes as *const usize as *const c_void,
            )
        });
        // try testing.expectEqual(max_bytes, t.?.stream.handler.apc_handler.unknown_max_bytes);
        assert_eq!(max_bytes, unsafe {
            (*t).stream.handler.apc_handler.unknown_max_bytes
        });
        // try testing.expectEqual(max_bytes, t.?.stream.parser.osc_parser.unknown_max_bytes);
        assert_eq!(max_bytes, unsafe {
            (*t).stream.parser.osc_parser.unknown_max_bytes
        });

        // A byte limit without a callback performs no external effect.
        // const before_callback = "\x1B_abc;xy\x1B\\";
        let before_callback = b"\x1B_abc;xy\x1B\\";
        vtw(t, before_callback);
        assert_eq!(0, unsafe { COUNT });
        // try testing.expect(t.?.stream.handler.unknown_sequence == null);
        assert!(unsafe { (*t).stream.handler.unknown_sequence }.is_none());

        assert_eq!(Result::Success, unsafe {
            set(
                t,
                Option_::UnknownSequence,
                unknown_sequence as *const c_void,
            )
        });
        // try testing.expect(t.?.stream.handler.unknown_sequence != null);
        assert!(unsafe { (*t).stream.handler.unknown_sequence }.is_some());

        // Split a complete APC across writes to exercise persistent parser state.
        // const seq_a = "\x1B_abc;"; const seq_b = "xy\x1B\\";
        let seq_a = b"\x1B_abc;";
        let seq_b = b"xy\x1B\\";
        vtw(t, seq_a);
        assert_eq!(0, unsafe { COUNT });
        vtw(t, seq_b);
        assert_eq!(1, unsafe { COUNT });
        // try testing.expectEqual(t, S.last_terminal);
        assert_eq!(t, unsafe { LAST_TERMINAL });
        // try testing.expectEqual(@as(?*anyopaque, @ptrCast(&sentinel)), S.last_userdata);
        assert_eq!(&mut sentinel as *mut u8 as *mut c_void, unsafe {
            LAST_USERDATA
        });
        // try testing.expectEqual(UnknownSequence.Tag.apc, S.last_tag);
        assert_eq!(UnknownSequenceTag::Apc, unsafe { LAST_TAG });
        // try testing.expect(!S.last_truncated);
        assert!(!unsafe { LAST_TRUNCATED });
        // try testing.expectEqualStrings("abc;xy", S.content[0..S.content_len]);
        assert_eq!(b"abc;xy".as_slice(), content());

        // Content beyond the generic limit is omitted and marked truncated.
        // const truncated = "\x1B_abcdefghijkl\x1B\\";
        let truncated = b"\x1B_abcdefghijkl\x1B\\";
        vtw(t, truncated);
        assert_eq!(2, unsafe { COUNT });
        assert!(unsafe { LAST_TRUNCATED });
        // try testing.expectEqualStrings("abcdefgh", S.content[0..S.content_len]);
        assert_eq!(b"abcdefgh".as_slice(), content());

        // CAN aborts the APC and must not invoke the callback.
        // const aborted = "\x1B_abcdef\x18";
        let aborted = b"\x1B_abcdef\x18";
        vtw(t, aborted);
        assert_eq!(2, unsafe { COUNT });

        // Unknown OSCs share the callback and the byte limit.
        // const osc_st = "\x1B]7400;x\x1B\\";
        let osc_st = b"\x1B]7400;x\x1B\\";
        vtw(t, osc_st);
        assert_eq!(3, unsafe { COUNT });
        assert_eq!(UnknownSequenceTag::Osc, unsafe { LAST_TAG });
        assert!(!unsafe { LAST_TRUNCATED });
        assert_eq!(crate::osc::Terminator::St, unsafe { LAST_TERMINATOR });
        assert_eq!(b"7400;x".as_slice(), content());

        // const osc_bel = "\x1B]7400;abcdef\x07";
        let osc_bel = b"\x1B]7400;abcdef\x07";
        vtw(t, osc_bel);
        assert_eq!(4, unsafe { COUNT });
        assert_eq!(UnknownSequenceTag::Osc, unsafe { LAST_TAG });
        assert!(unsafe { LAST_TRUNCATED });
        assert_eq!(crate::osc::Terminator::Bel, unsafe { LAST_TERMINATOR });
        assert_eq!(b"7400;abc".as_slice(), content());

        // Aborted unknown OSCs and supported OSCs are not reported.
        // const osc_aborted = "\x1B]7400;x\x18\x1B]2;title\x07";
        let osc_aborted = b"\x1B]7400;x\x18\x1B]2;title\x07";
        vtw(t, osc_aborted);
        assert_eq!(4, unsafe { COUNT });

        // Clearing the callback restores the null fast path immediately.
        assert_eq!(Result::Success, unsafe {
            set(t, Option_::UnknownSequence, std::ptr::null())
        });
        assert!(unsafe { (*t).stream.handler.unknown_sequence }.is_none());
        vtw(t, before_callback);
        vtw(t, osc_st);
        assert_eq!(4, unsafe { COUNT });

        // A NULL limit disables capture even after reinstalling the callback.
        assert_eq!(Result::Success, unsafe {
            set(
                t,
                Option_::UnknownSequence,
                unknown_sequence as *const c_void,
            )
        });
        assert_eq!(Result::Success, unsafe {
            set(t, Option_::UnknownMaxBytes, std::ptr::null())
        });
        // try testing.expectEqual(@as(usize, 0), t.?.stream.handler.apc_handler.unknown_max_bytes);
        assert_eq!(0, unsafe {
            (*t).stream.handler.apc_handler.unknown_max_bytes
        });
        // try testing.expectEqual(@as(usize, 0), t.?.stream.parser.osc_parser.unknown_max_bytes);
        assert_eq!(0, unsafe {
            (*t).stream.parser.osc_parser.unknown_max_bytes
        });
        vtw(t, before_callback);
        vtw(t, osc_st);
        assert_eq!(4, unsafe { COUNT });

        unsafe { free(t) };
    });
}
