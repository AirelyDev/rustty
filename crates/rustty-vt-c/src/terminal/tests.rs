use super::*;
use crate::test_util::with_test_allocator;

mod alloc;
mod effects;
mod effects_clipboard;
mod get;
mod modes_write;
mod selection_grid;
mod title_color;
mod viewport_resize;

// PORT NOTE: the reference passes `&lib.alloc.test_allocator` to `new`.
// rustty has no global test allocator (leaks are caught by Miri); the parity
// analog is `with_test_allocator`, a C `Allocator` backed by the process
// global. The reference `t.?.terminal.plainString(testing.allocator)` becomes
// `(*(*t).terminal).plain_string(Global)`; the returned `Vec<u8>` frees on
// drop (the reference's `testing.allocator.free(str)`).

// Shared scaffold for the reference's repeated
//   var t: Terminal = null;
//   try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
// preamble. It returns the `new` Result UNASSERTED so the reference's
// `expectEqual(Result.success, ...)` ASSERTION lives in each test body
// (assertion-for-assertion; G2 counts source-level asserts per test).
pub(super) fn new_t(
    alloc: &CAllocator,
    cols: size::CellCountInt,
    rows: size::CellCountInt,
) -> (Result, Terminal) {
    let mut t: Terminal = std::ptr::null_mut();
    let r = unsafe { new(alloc as *const CAllocator, &mut t, cols, rows) };
    (r, t)
}

pub(super) fn plain_string(t: Terminal) -> Vec<u8> {
    // SAFETY: `t` is a live terminal from `new_t`.
    let zt = unsafe { &mut *(*t).terminal };
    zt.plain_string(rustty_vt::alloc::Global).unwrap()
}

// test "new/free"
#[test]
fn new_free() {
    with_test_allocator(|alloc| {
        // var t: Terminal = null;
        let mut t: Terminal = std::ptr::null_mut();
        // try testing.expectEqual(Result.success, new(&test_allocator, &t, 80, 24));
        assert_eq!(Result::Success, unsafe {
            new(alloc as *const CAllocator, &mut t, 80, 24)
        });
        // try testing.expect(t != null);
        assert!(!t.is_null());
        // free(t);
        unsafe { free(t) };
    });
}

// PORT NOTE (F-008 proving tests, Rust-only — the reference has no unit
// test for this; the contract is `new_`'s `alloc.create(...) catch return
// error.OutOfMemory` + `free`'s `alloc.destroy`): the caller-supplied C
// allocator must OWN the wrapper-level lifecycle storage.
#[test]
fn new_free_uses_caller_allocator() {
    let stats = crate::test_util::CountingCAllocator::new();
    let calloc = stats.c_allocator();
    let mut t: Terminal = std::ptr::null_mut();
    assert_eq!(Result::Success, unsafe {
        new(&calloc as *const CAllocator, &mut t, 80, 24)
    });
    assert!(!t.is_null());
    // At least the ZigTerminal + TerminalWrapper storage (Ghostty:
    // `alloc.create(ZigTerminal)` + `alloc.create(TerminalWrapper)`).
    assert!(
        stats.alloc_calls.get() >= 2,
        "terminal/wrapper storage must come from the caller allocator (got {} allocs)",
        stats.alloc_calls.get()
    );
    unsafe { free(t) };
    // free must return every caller-allocator byte (`alloc.destroy` x2).
    assert!(stats.free_calls.get() >= 2);
    assert_eq!(
        0,
        stats.live_bytes.get(),
        "free must balance the caller allocator"
    );
}

// Parity S174-F3 (no reference test): the stream's OSC capture allocates
// through the terminal's caller allocator (`.initAlloc(alloc, handler)`),
// so a long OSC 52 grows through it and `free` returns every byte.
#[test]
fn stream_osc_capture_uses_the_caller_allocator() {
    let stats = crate::test_util::CountingCAllocator::new();
    let calloc = stats.c_allocator();
    let mut t: Terminal = std::ptr::null_mut();
    assert_eq!(Result::Success, unsafe {
        new(&calloc as *const CAllocator, &mut t, 80, 24)
    });
    let before = stats.alloc_calls.get();
    let mut osc = b"\x1B]52;c;".to_vec();
    osc.extend(std::iter::repeat_n(b'A', rustty_vt::osc::MAX_BUF * 2));
    osc.extend_from_slice(b"\x1B\\");
    vtw(t, &osc);
    // The capture's initial MAX_BUF and at least one growth.
    assert!(
        stats.alloc_calls.get() >= before + 2,
        "the OSC capture went elsewhere ({} -> {} allocs)",
        before,
        stats.alloc_calls.get()
    );
    unsafe { free(t) };
    assert_eq!(0, stats.live_bytes.get());
}

#[test]
fn new_rejecting_allocator_returns_out_of_memory() {
    let stats = crate::test_util::CountingCAllocator::new();
    stats.reject.set(true);
    let calloc = stats.c_allocator();
    let mut t: Terminal = std::ptr::null_mut();
    // Ghostty: a rejecting caller allocator makes `alloc.create` fail and
    // `new` return `.out_of_memory` with `result.* = null` — NOT a silent
    // success on some other heap.
    assert_eq!(Result::OutOfMemory, unsafe {
        new(&calloc as *const CAllocator, &mut t, 80, 24)
    });
    assert!(t.is_null());
    assert!(stats.alloc_calls.get() >= 1);
    assert_eq!(0, stats.live_bytes.get());
}

// test "new invalid value"
#[test]
fn new_invalid_value() {
    with_test_allocator(|alloc| {
        let mut t: Terminal = std::ptr::null_mut();
        // cols == 0
        assert_eq!(Result::InvalidValue, unsafe {
            new(alloc as *const CAllocator, &mut t, 0, 24)
        });
        assert!(t.is_null());

        // rows == 0
        assert_eq!(Result::InvalidValue, unsafe {
            new(alloc as *const CAllocator, &mut t, 80, 0)
        });
        assert!(t.is_null());
    });
}

// fn testEnableContinuation(terminal: Terminal) !void {
pub(super) fn test_enable_continuation(terminal: Terminal) {
    let limit: usize = 1024;
    assert_eq!(Result::Success, unsafe {
        set(
            terminal,
            Option_::ContinuationMaxBytes,
            &limit as *const usize as *const c_void,
        )
    });
}

// test "continuation option and data"
#[test]
fn continuation_option_and_data() {
    with_test_allocator(|alloc| {
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);

        let mut value: usize = 0;
        assert_eq!(Result::Success, unsafe {
            get(
                t,
                TerminalData::ContinuationMaxBytes,
                &mut value as *mut usize as *mut c_void,
            )
        });
        assert_eq!(DEFAULT_CONTINUATION_MAX_BYTES, value);

        let custom: usize = 1234;
        assert_eq!(Result::Success, unsafe {
            set(
                t,
                Option_::ContinuationMaxBytes,
                &custom as *const usize as *const c_void,
            )
        });
        assert_eq!(Result::Success, unsafe {
            get(
                t,
                TerminalData::ContinuationMaxBytes,
                &mut value as *mut usize as *mut c_void,
            )
        });
        assert_eq!(custom, value);

        assert_eq!(Result::Success, unsafe {
            set(t, Option_::ContinuationMaxBytes, std::ptr::null())
        });
        assert_eq!(Result::Success, unsafe {
            get(
                t,
                TerminalData::ContinuationMaxBytes,
                &mut value as *mut usize as *mut c_void,
            )
        });
        assert_eq!(DEFAULT_CONTINUATION_MAX_BYTES, value);

        let disabled: usize = 0;
        assert_eq!(Result::Success, unsafe {
            set(
                t,
                Option_::ContinuationMaxBytes,
                &disabled as *const usize as *const c_void,
            )
        });
        assert_eq!(Result::Success, unsafe {
            get(
                t,
                TerminalData::ContinuationMaxBytes,
                &mut value as *mut usize as *mut c_void,
            )
        });
        assert_eq!(0usize, value);

        let mut written: usize = 99;
        assert_eq!(Result::InvalidValue, unsafe {
            continuation_buf(t, std::ptr::null_mut(), 0, &mut written)
        });
        assert_eq!(0usize, written);
        unsafe { free(t) };
    });
}

// test "continuation buffer and allocator export exact suffix"
#[test]
fn continuation_buffer_and_allocator_export_exact_suffix() {
    with_test_allocator(|alloc| {
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);
        test_enable_continuation(t);

        vtw(t, b"A\x1b[31");

        let mut required: usize = 0;
        assert_eq!(Result::OutOfSpace, unsafe {
            continuation_buf(t, std::ptr::null_mut(), 0, &mut required)
        });
        assert_eq!(4usize, required);

        let mut short = [0u8; 2];
        assert_eq!(Result::OutOfSpace, unsafe {
            continuation_buf(t, short.as_mut_ptr(), short.len(), &mut required)
        });
        assert_eq!(4usize, required);

        let mut buf = [0u8; 8];
        let mut written: usize = 0;
        assert_eq!(Result::Success, unsafe {
            continuation_buf(t, buf.as_mut_ptr(), buf.len(), &mut written)
        });
        assert_eq!(b"\x1b[31", &buf[..written]);

        let mut out_ptr: *mut u8 = std::ptr::null_mut();
        let mut out_len: usize = 0;
        assert_eq!(Result::Success, unsafe {
            continuation_alloc(t, Some(alloc), &mut out_ptr, &mut out_len)
        });
        // const allocated = (out_ptr orelse return error.TestExpectedEqual)[0..out_len];
        assert!(!out_ptr.is_null());
        // SAFETY: `out_ptr[0..out_len]` is the allocation returned above.
        let allocated = unsafe { core::slice::from_raw_parts(out_ptr, out_len) };
        // try testing.expectEqualStrings("\x1b[31", allocated);
        assert_eq!(b"\x1b[31", allocated);
        // defer lib.alloc.default(&test_allocator).free(allocated);
        rustty_lib::allocator::default(Some(alloc)).free(
            rustty_lib::allocator::ZigSlice::new(out_ptr, out_len),
            rustty_lib::allocator::Alignment::from_byte_units(1),
            0,
        );

        vtw(t, b"m");
        assert_eq!(Result::OutOfSpace, unsafe {
            continuation_buf(t, std::ptr::null_mut(), 0, &mut required)
        });
        assert_eq!(0usize, required);

        // Completing the sequence must return an empty, allocation-free result.
        // const failing: CAllocator = .fromZig(&std.mem.Allocator.failing);
        let failing_zig = rustty_lib::allocator::FailingZigAllocator;
        let failing_ref = rustty_lib::allocator::ZigAllocatorRef::new(&failing_zig);
        let failing = rustty_lib::allocator::Allocator::from_zig(&failing_ref);
        // try testing.expectEqual(Result.success, continuation_alloc(t, &failing, &out_ptr, &out_len));
        assert_eq!(Result::Success, unsafe {
            continuation_alloc(t, Some(&failing), &mut out_ptr, &mut out_len)
        });
        // defer @import("allocator.zig").free(&failing, out_ptr, out_len);
        // PORT NOTE: the deferred free runs after the assertions below.
        // try testing.expectEqual(null, out_ptr);
        assert!(out_ptr.is_null());
        // try testing.expectEqual(@as(usize, 0), out_len);
        assert_eq!(0usize, out_len);
        unsafe { crate::allocator::free(Some(&failing), out_ptr, out_len) };
        unsafe { free(t) };
    });
}

// test "continuation export tracks split UTF-8"
#[test]
fn continuation_export_tracks_split_utf8() {
    with_test_allocator(|alloc| {
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);
        test_enable_continuation(t);

        let prefix = [0xF0u8, 0x9F];
        vtw(t, &prefix);

        let mut buf = [0u8; 4];
        let mut written: usize = 0;
        assert_eq!(Result::Success, unsafe {
            continuation_buf(t, buf.as_mut_ptr(), buf.len(), &mut written)
        });
        assert_eq!(&prefix[..], &buf[..written]);

        let suffix = [0x98u8, 0x80];
        vtw(t, &suffix);
        assert_eq!(Result::OutOfSpace, unsafe {
            continuation_buf(t, std::ptr::null_mut(), 0, &mut written)
        });
        assert_eq!(0usize, written);
        unsafe { free(t) };
    });
}

// test "continuation callback writer reports success and failure"
#[test]
fn continuation_callback_writer_reports_success_and_failure() {
    struct Sink {
        bytes: [u8; 8],
        len: usize,
    }
    extern "C" fn sink_write(userdata: *mut c_void, data: *const u8, len: usize) -> bool {
        let sink = unsafe { &mut *(userdata as *mut Sink) };
        if len > sink.bytes.len() - sink.len {
            return false;
        }
        unsafe { std::ptr::copy_nonoverlapping(data, sink.bytes[sink.len..].as_mut_ptr(), len) };
        sink.len += len;
        true
    }
    extern "C" fn sink_fail(_: *mut c_void, _: *const u8, _: usize) -> bool {
        false
    }

    with_test_allocator(|alloc| {
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);
        test_enable_continuation(t);
        vtw(t, b"\x1b[31");

        let mut sink = Sink {
            bytes: [0; 8],
            len: 0,
        };
        assert_eq!(Result::Success, unsafe {
            continuation_write(
                t,
                crate::io::Writer {
                    write: Some(sink_write),
                    userdata: &mut sink as *mut Sink as *mut c_void,
                },
            )
        });
        assert_eq!(b"\x1b[31", &sink.bytes[..sink.len]);

        assert_eq!(Result::IoError, unsafe {
            continuation_write(
                t,
                crate::io::Writer {
                    write: Some(sink_fail),
                    userdata: std::ptr::null_mut(),
                },
            )
        });
        assert_eq!(Result::InvalidValue, unsafe {
            continuation_write(t, crate::io::Writer::default())
        });
        assert_eq!(Result::InvalidValue, unsafe {
            continuation_write(
                std::ptr::null_mut(),
                crate::io::Writer {
                    write: Some(sink_fail),
                    userdata: std::ptr::null_mut(),
                },
            )
        });
        unsafe { free(t) };
    });
}

// test "continuation runtime reconfiguration recovers safely"
#[test]
fn continuation_runtime_reconfiguration_recovers_safely() {
    with_test_allocator(|alloc| {
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);

        let enough: usize = 8;
        assert_eq!(Result::Success, unsafe {
            set(
                t,
                Option_::ContinuationMaxBytes,
                &enough as *const usize as *const c_void,
            )
        });
        vtw(t, b"\x1b[123");
        let too_small: usize = 2;
        assert_eq!(Result::Success, unsafe {
            set(
                t,
                Option_::ContinuationMaxBytes,
                &too_small as *const usize as *const c_void,
            )
        });

        let mut written: usize = 99;
        assert_eq!(Result::InvalidValue, unsafe {
            continuation_buf(t, std::ptr::null_mut(), 0, &mut written)
        });
        assert_eq!(0usize, written);

        // Raising the limit cannot reconstruct discarded bytes, but a new ESC is
        // a complete replay start and repairs tracking without first grounding.
        assert_eq!(Result::Success, unsafe {
            set(
                t,
                Option_::ContinuationMaxBytes,
                &enough as *const usize as *const c_void,
            )
        });
        vtw(t, b"\x1b[");
        let mut buf = [0u8; 8];
        assert_eq!(Result::Success, unsafe {
            continuation_buf(t, buf.as_mut_ptr(), buf.len(), &mut written)
        });
        assert_eq!(b"\x1b[", &buf[..written]);

        let disabled: usize = 0;
        assert_eq!(Result::Success, unsafe {
            set(
                t,
                Option_::ContinuationMaxBytes,
                &disabled as *const usize as *const c_void,
            )
        });
        assert_eq!(Result::Success, unsafe {
            set(
                t,
                Option_::ContinuationMaxBytes,
                &enough as *const usize as *const c_void,
            )
        });
        assert_eq!(Result::InvalidValue, unsafe {
            continuation_buf(t, std::ptr::null_mut(), 0, &mut written)
        });

        // Completing the sequence reaches ground and resets the broken marker.
        vtw(t, b"m");
        assert_eq!(Result::OutOfSpace, unsafe {
            continuation_buf(t, std::ptr::null_mut(), 0, &mut written)
        });
        assert_eq!(0usize, written);
        unsafe { free(t) };
    });
}

// test "continuation internal allocation and restoration"
#[test]
fn continuation_internal_allocation_and_restoration() {
    with_test_allocator(|alloc| {
        let (new_result, source) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);
        test_enable_continuation(source);
        vtw(source, b"\x1b[31");

        // const alloc = lib.alloc.default(&lib.alloc.test_allocator);
        // const bytes = try continuationAllocIo(source, alloc, false);
        let default_alloc = unsafe { rustty_vt::alloc::default(Some(alloc)) };
        let bytes = unsafe { continuation_alloc_io(source, default_alloc, false) }.unwrap();
        assert_eq!(b"\x1b[31", bytes.as_slice());

        let (new_result, restored) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);
        test_enable_continuation(restored);
        unsafe { restore_continuation(restored, &bytes) }.unwrap();

        let reexported = unsafe { continuation_alloc_io(restored, default_alloc, false) }.unwrap();
        assert_eq!(bytes, reexported);

        let (new_result, invalid) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);
        test_enable_continuation(invalid);
        assert_eq!(Err(RestoreContinuationError::InvalidContinuation), unsafe {
            restore_continuation(invalid, b"A")
        });
        unsafe { free(invalid) };
        unsafe { free(restored) };
        unsafe { free(source) };
    });
}

// test "set scrollback limits"
#[test]
fn set_scrollback_limits() {
    with_test_allocator(|alloc| {
        let (new_result, t) = new_t(alloc, 80, 3);
        assert_eq!(Result::Success, new_result);

        // const primary = t.?.terminal.screens.get(.primary).?;
        let primary: *mut rustty_vt::screen::Screen = unsafe {
            (*(*t).terminal)
                .screens
                .get(rustty_vt::screen_set::Key::Primary)
                .unwrap()
        };

        let max_bytes: usize = 0;
        assert_eq!(Result::Success, unsafe {
            set(
                t,
                Option_::ScrollbackMaxBytes,
                &max_bytes as *const usize as *const c_void,
            )
        });
        assert_eq!(0usize, unsafe { (*primary).pages.limits.bytes.explicit });
        assert!(unsafe { (*primary).no_scrollback });

        assert_eq!(Result::Success, unsafe {
            set(t, Option_::ScrollbackMaxBytes, std::ptr::null())
        });
        assert_eq!(usize::MAX, unsafe {
            (*primary).pages.limits.bytes.explicit
        });
        assert!(!unsafe { (*primary).no_scrollback });

        let max_lines: usize = 12;
        assert_eq!(Result::Success, unsafe {
            set(
                t,
                Option_::ScrollbackMaxLines,
                &max_lines as *const usize as *const c_void,
            )
        });
        assert_eq!(max_lines, unsafe { (*primary).pages.limits.lines.explicit });

        assert_eq!(Result::Success, unsafe {
            set(t, Option_::ScrollbackMaxLines, std::ptr::null())
        });
        assert_eq!(usize::MAX, unsafe {
            (*primary).pages.limits.lines.explicit
        });

        unsafe { free(t) };
    });
}

// test "get configured scrollback limits"
#[test]
fn get_configured_scrollback_limits() {
    with_test_allocator(|alloc| {
        let (new_result, t) = new_t(alloc, 80, 3);
        assert_eq!(Result::Success, new_result);

        let mut value: usize = 0;
        assert_eq!(Result::Success, unsafe {
            get(
                t,
                TerminalData::ScrollbackMaxBytes,
                &mut value as *mut usize as *mut c_void,
            )
        });
        assert_eq!(10_000usize, value);
        assert_eq!(Result::NoValue, unsafe {
            get(
                t,
                TerminalData::ScrollbackMaxLines,
                &mut value as *mut usize as *mut c_void,
            )
        });

        let max_bytes: usize = 0;
        let max_lines: usize = 12;
        assert_eq!(Result::Success, unsafe {
            set(
                t,
                Option_::ScrollbackMaxBytes,
                &max_bytes as *const usize as *const c_void,
            )
        });
        assert_eq!(Result::Success, unsafe {
            set(
                t,
                Option_::ScrollbackMaxLines,
                &max_lines as *const usize as *const c_void,
            )
        });
        assert_eq!(Result::Success, unsafe {
            get(
                t,
                TerminalData::ScrollbackMaxBytes,
                &mut value as *mut usize as *mut c_void,
            )
        });
        assert_eq!(max_bytes, value);
        assert_eq!(Result::Success, unsafe {
            get(
                t,
                TerminalData::ScrollbackMaxLines,
                &mut value as *mut usize as *mut c_void,
            )
        });
        assert_eq!(max_lines, value);

        // The configured limits belong to the primary screen and remain
        // readable while an alternate screen is active.
        unsafe { vt_write(t, b"\x1b[?1049h".as_ptr(), 8) };
        assert_eq!(Result::Success, unsafe {
            get(
                t,
                TerminalData::ScrollbackMaxBytes,
                &mut value as *mut usize as *mut c_void,
            )
        });
        assert_eq!(max_bytes, value);
        assert_eq!(Result::Success, unsafe {
            get(
                t,
                TerminalData::ScrollbackMaxLines,
                &mut value as *mut usize as *mut c_void,
            )
        });
        assert_eq!(max_lines, value);

        assert_eq!(Result::Success, unsafe {
            set(t, Option_::ScrollbackMaxBytes, std::ptr::null())
        });
        assert_eq!(Result::Success, unsafe {
            set(t, Option_::ScrollbackMaxLines, std::ptr::null())
        });
        assert_eq!(Result::NoValue, unsafe {
            get(
                t,
                TerminalData::ScrollbackMaxBytes,
                &mut value as *mut usize as *mut c_void,
            )
        });
        assert_eq!(Result::NoValue, unsafe {
            get(
                t,
                TerminalData::ScrollbackMaxLines,
                &mut value as *mut usize as *mut c_void,
            )
        });

        unsafe { free(t) };
    });
}

// PORT NOTE (test helpers): the reference builds `point.Point.C` literals
// inline (`.{ .tag = .active, .value = .{ .active = .{ .x, .y } } }`). rustty's
// `PointC` is a hand-authored tagged union; these helpers construct it for the
// active/viewport/screen tags used by the ported tests.
pub(super) fn pt_active(x: size::CellCountInt, y: u32) -> PointC {
    PointC {
        tag: crate::point::TagC::Active as c_int,
        value: crate::point::PointCValue {
            active: Coordinate { x, y },
        },
    }
}
pub(super) fn pt_viewport(x: size::CellCountInt, y: u32) -> PointC {
    PointC {
        tag: crate::point::TagC::Viewport as c_int,
        value: crate::point::PointCValue {
            viewport: Coordinate { x, y },
        },
    }
}
pub(super) fn pt_screen(x: size::CellCountInt, y: u32) -> PointC {
    PointC {
        tag: crate::point::TagC::Screen as c_int,
        value: crate::point::PointCValue {
            screen: Coordinate { x, y },
        },
    }
}

// Convenience: write a byte slice through `vt_write`.
pub(super) fn vtw(t: Terminal, bytes: &[u8]) {
    unsafe { vt_write(t, bytes.as_ptr(), bytes.len()) };
}
