//! rustty-vt-c osc
//!
//! PORTS: ghostty src/terminal/c/osc.zig (the OSC parser C ABI layer).
//!
//! The rustty-vt `osc::Parser`/`osc::Command` are the source of truth (R3);
//! this module is the FFI projection. The `rustty_osc_*` `extern "C"` wrappers
//! live in `lib.rs` and forward here.

// const std = @import("std");
// const lib = @import("../lib.zig");
// const CAllocator = lib.alloc.Allocator;
// const osc = @import("../osc.zig");
// const Result = @import("result.zig").Result;
//
// const log = std.log.scoped(.osc);
//
// S174-F2: the commandData warning below used to bypass the C-ABI log sink
// with a raw `eprintln!`, which an embedder that registered a log callback
// via `rustty_sys_set(RUSTTY_SYS_OPT_LOG, ...)` never saw. Routed through
// `sys::log_fn` under this scope constant, the `@tagName` of the reference's
// `std.log.scoped(.osc)`.
pub(crate) const LOG_SCOPE: &str = "osc";
use crate::result::Result;
use rustty_lib::allocator::Allocator as CAllocator;
use rustty_vt::osc;
use std::ffi::c_void;
use std::os::raw::c_int;

use crate::cwrap::{create, destroy};

// PORT NOTE (lifetime/allocator): the reference `osc.Parser` carries its own
// `std.mem.Allocator` (`parser.alloc`, `.init(alloc)` with the resolved
// `lib.alloc.default(alloc_)`), and so does the port's: `osc::Parser.alloc` is
// the runtime allocator handle, over the caller's own `RusttyAllocator` (or
// the Rust heap for NULL). The parser's capture and the command lists it
// builds go through it (parity S174-F3). The wrapper also keeps the original
// (possibly null) C `alloc_` (a `Copy` C struct; see `cwrap` PORT NOTE) so
// `free` releases the wrapper through the SAME allocator the reference's
// `parser.alloc.?` would select.
// CORRECTION (parity tail S7-7): the wrapper used to own a boxed
// `ZigAllocator` for the parser to borrow, plus a separate capture handle; the
// handle in `parser.alloc` replaces both, and the Rust-heap box is gone.

/// Wrapper around the parser that tracks the allocator for C API usage.
// PORT NOTE: the reference stores `parser.alloc` inside `osc.Parser` itself
// (`.init(alloc)`), and `end` returns a pointer into `parser.command`.
pub struct OscParserWrapper {
    /// The command the last `end` returned, which `rustty_osc_end`'s pointer
    /// points at.
    // PORT NOTE (parity tail S7-7, N263): the reference's `parser.command`.
    // The port's parser returns the command by value, borrowing the parser's
    // buffer or capture (`osc::Command<'_>`); the wrapper keeps it here, with
    // that borrow's lifetime erased, for the C caller. It is valid exactly as
    // long as the header says the command is ("until the next call to any
    // `rustty_osc_*` function with the same parser instance", except the
    // command introspection functions): every other entry point below clears
    // it FIRST, before it touches the parser. The wrapper lives in its own C
    // allocation and never moves, so the borrowed bytes stay where the
    // command points.
    command: Option<osc::Command<'static>>,
    parser: osc::Parser,
    alloc: Option<CAllocator>,
}

impl OscParserWrapper {
    /// Ends the borrow of the last command (the reference's "only valid until
    /// the next call to the parser"). The command owns nothing: the lists it
    /// borrows are the parser's, freed at `reset`/`free` as in the reference.
    #[inline]
    fn clear_command(&mut self) {
        self.command = None;
    }
}

/// C: RusttyOscParser (R2)
// pub const Parser = ?*osc.Parser;
pub type Parser = *mut OscParserWrapper;

/// C: RusttyOscCommand (R2)
// pub const Command = ?*osc.Command;
pub type Command = *mut osc::Command<'static>;

// pub fn new(alloc_: ?*const CAllocator, result: *Parser) ... Result {
//     const alloc = lib.alloc.default(alloc_);
//     const ptr = alloc.create(osc.Parser) catch return .out_of_memory;
//     ptr.* = .init(alloc);
//     result.* = ptr;
//     return .success;
// }
//
/// # Safety
/// `alloc_`, if non-null, must point to a valid `CAllocator`; `result` must be
/// writable storage for one `Parser`.
pub unsafe fn new(alloc_: Option<&CAllocator>, result: *mut Parser) -> Result {
    // const alloc = lib.alloc.default(alloc_);
    // SAFETY: a non-NULL `alloc_` is valid for the parser's life (the C API's
    // contract, N260).
    let alloc = unsafe { rustty_vt::alloc::default(alloc_) };
    // const ptr = alloc.create(osc.Parser) catch return .out_of_memory;
    // ptr.* = .init(alloc);
    // PORT NOTE: the original C `alloc_` is stored for `free`.
    // SAFETY: `alloc_` is valid per the caller contract.
    let ptr = unsafe {
        create(
            alloc_,
            OscParserWrapper {
                command: None,
                parser: osc::Parser::init(Some(alloc)),
                alloc: alloc_.copied(),
            },
        )
    };
    if ptr.is_null() {
        return Result::OutOfMemory;
    }
    // result.* = ptr;
    // SAFETY: `result` is writable per the caller contract.
    unsafe { *result = ptr };
    // return .success;
    Result::Success
}

// pub fn free(parser_: Parser) ... void {
//     // C-built parsers always have an associated allocator.
//     const parser = parser_ orelse return;
//     const alloc = parser.alloc.?;
//     parser.deinit();
//     alloc.destroy(parser);
// }
//
/// # Safety
/// `parser_`, if non-null, must be a handle returned by `new`, not yet freed.
pub unsafe fn free(parser_: Parser) {
    // C-built parsers always have an associated allocator.
    // const parser = parser_ orelse return;
    if parser_.is_null() {
        return;
    }
    // const alloc = parser.alloc.?;
    // SAFETY: `parser_` is a live wrapper handle (caller contract).
    let alloc = unsafe { (*parser_).alloc };
    // SAFETY: as above; the last command's borrow ends before the parser goes.
    unsafe { (*parser_).clear_command() };
    // parser.deinit();
    // SAFETY: `parser_` is a live wrapper handle (caller contract).
    unsafe { (*parser_).parser.deinit() };
    // alloc.destroy(parser);
    // SAFETY: `parser_` was produced by `create` with the same `alloc_`.
    unsafe { destroy(alloc.as_ref(), parser_) };
}

// pub fn reset(parser_: Parser) ... void {
//     parser_.?.reset();
// }
//
/// # Safety
/// `parser_` must be a non-null handle returned by `new`.
pub unsafe fn reset(parser_: Parser) {
    // parser_.?.reset();
    // SAFETY: `parser_` is non-null per the caller contract (`.?`); the last
    // command's borrow ends first.
    unsafe {
        (*parser_).clear_command();
        (*parser_).parser.reset();
    }
}

// pub fn next(parser_: Parser, byte: u8) ... void {
//     parser_.?.next(byte);
// }
//
/// # Safety
/// `parser_` must be a non-null handle returned by `new`.
pub unsafe fn next(parser_: Parser, byte: u8) {
    // parser_.?.next(byte);
    // SAFETY: `parser_` is non-null per the caller contract (`.?`); the last
    // command's borrow ends first.
    unsafe {
        (*parser_).clear_command();
        (*parser_).parser.next(byte);
    }
}

// pub fn end(parser_: Parser, terminator: u8) ... Command {
//     return parser_.?.end(terminator);
// }
//
/// # Safety
/// `parser_` must be a non-null handle returned by `new`.
pub unsafe fn end(parser_: Parser, terminator: u8) -> Command {
    // return parser_.?.end(terminator);
    // PORT NOTE: `osc::Parser::end` takes `Option<u8>` (the reference passes the
    // raw `u8` terminator; rustty wraps it in `Some`). It returns the command
    // by value, borrowing the parser; the wrapper keeps it (the reference's
    // `parser.command`) and C gets a pointer to it (null for `None`).
    // SAFETY: `parser_` is non-null per the caller contract (`.?`).
    let wrapper = unsafe { &mut *parser_ };
    // The previous command's borrow ends before the parser is ended again.
    wrapper.clear_command();
    match wrapper.parser.end(Some(terminator)) {
        Some(cmd) => {
            // SAFETY: lifetime erasure only (same type, same layout). The
            // command borrows `wrapper.parser`'s buffer or capture. The wrapper
            // does not move (it lives in its C allocation until `free`), and
            // every entry point that could touch those bytes (`next`, `reset`,
            // `end`, `free`) clears `command` before it does, so the erased
            // borrow is never used past the bytes' validity; this is the
            // header's lifetime for the returned command.
            let cmd: osc::Command<'static> =
                unsafe { core::mem::transmute::<osc::Command<'_>, osc::Command<'static>>(cmd) };
            let slot = wrapper.command.insert(cmd);
            slot as *mut osc::Command<'static>
        }
        None => std::ptr::null_mut(),
    }
}

// pub fn commandType(command_: Command) ... osc.Command.Key {
//     const command = command_ orelse return .invalid;
//     return command.*;
// }
//
/// # Safety
/// `command_`, if non-null, must be a live command pointer returned by `end`.
pub unsafe fn command_type(command_: Command) -> osc::CommandKey {
    // const command = command_ orelse return .invalid;
    if command_.is_null() {
        return osc::CommandKey::Invalid;
    }
    // return command.*;
    // PORT NOTE: `command.*` coerces the `union(Key)` to its tag; ported as the
    // `Command::key()` projection.
    // SAFETY: `command_` is a live command pointer (caller contract).
    unsafe { (*command_).key() }
}

/// C: RusttyOscCommandData (R2)
// pub const CommandData = enum(c_int) {
//     invalid = 0,
//     change_window_title_str = 1,
//     ...
// };
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandData {
    Invalid = 0,
    ChangeWindowTitleStr = 1,
    // /// Output type expected for querying the data of the given kind.
    // pub fn OutType(comptime self: CommandData) type {
    //     return switch (self) {
    //         .invalid => void,
    //         .change_window_title_str => [*:0]const u8,
    //     };
    // }
    // PORT NOTE: `OutType` is a comptime type map used by `commandDataTyped`;
    // the typed dispatch is inlined into `command_data` below (Rust has no
    // comptime type-returning function).
}

// pub fn commandData(command_: Command, data: CommandData, out: ?*anyopaque) ... bool {
//     if (comptime std.debug.runtime_safety) {
//         _ = std.meta.intToEnum(CommandData, @intFromEnum(data)) catch {
//             log.warn("commandData invalid data value={d}", .{@intFromEnum(data)});
//             return false;
//         };
//     }
//
//     return switch (data) {
//         .invalid => false,
//         inline else => |comptime_data| commandDataTyped(
//             command_,
//             comptime_data,
//             @ptrCast(@alignCast(out)),
//         ),
//     };
// }
//
/// # Safety
/// `command_`, if non-null, must be a live command pointer; `out`, for a valid
/// `data` kind, must point to writable storage of that kind's out-type.
pub unsafe fn command_data(command_: Command, data: c_int, out: *mut c_void) -> bool {
    // if (comptime std.debug.runtime_safety) {
    //     _ = std.meta.intToEnum(CommandData, ...) catch { log.warn(...); return false; };
    // }
    // PORT NOTE: the integer-range guard runs on the raw `c_int` crossing the
    // boundary; an out-of-range `data` logs and returns false (no `out` deref).
    let data = match data {
        0 => CommandData::Invalid,
        1 => CommandData::ChangeWindowTitleStr,
        _ => {
            // log.warn("commandData invalid data value={d}", .{@intFromEnum(data)});
            crate::sys::log_fn(
                crate::sys::StdLevel::Warn,
                LOG_SCOPE,
                format_args!("commandData invalid data value={data}"),
            );
            return false;
        }
    };

    // return switch (data) { .invalid => false, inline else => commandDataTyped(...) };
    match data {
        // .invalid => false,
        CommandData::Invalid => false,
        // .change_window_title_str => commandDataTyped(...)
        CommandData::ChangeWindowTitleStr => {
            // SAFETY: `command_`/`out` validity is the caller's contract.
            unsafe { command_data_change_window_title_str(command_, out as *mut *const u8) }
        }
    }
}

// fn commandDataTyped(command_: Command, comptime data: CommandData, out: *data.OutType()) bool {
//     const command = command_.?;
//     switch (data) {
//         .invalid => return false,
//         .change_window_title_str => switch (command.*) {
//             .change_window_title => |v| out.* = v.ptr,
//             else => return false,
//         },
//     }
//
//     return true;
// }
//
/// PORT NOTE: the `.change_window_title_str` arm of `commandDataTyped`.
///
/// # Safety
/// `command_`, if non-null, must be a live command pointer; `out` must point
/// to writable `*const u8` storage.
unsafe fn command_data_change_window_title_str(command_: Command, out: *mut *const u8) -> bool {
    // const command = command_.?;
    // PORT NOTE: reference defect (R13), N323. The header documents
    // `command` as "may be NULL", and `rustty_osc_end` returns NULL for every
    // invalid or unfinished OSC, but the reference unwraps it with `.?`: a
    // panic in ReleaseSafe and undefined behaviour in ReleaseFast when a C
    // caller asks a NULL command for its title. `commandType` answers NULL
    // with `.invalid`; this answers it with `false` ("the data is not
    // available"), the function's own failure value, and never dereferences.
    if command_.is_null() {
        return false;
    }
    // SAFETY: non-null, so a live command pointer (caller contract).
    let command = unsafe { &*command_ };
    // switch (command.*) { .change_window_title => |v| out.* = v.ptr, else => return false }
    match command {
        osc::Command::ChangeWindowTitle(v) => {
            // out.* = v.ptr;
            // CORRECTION (parity tail S7-7): the title used to be an owned
            // `Vec<u8>` without its sentinel, and this pushed a NUL onto it
            // before exposing the pointer. It is the reference's `[:0]const u8`
            // now, a `ZStr` borrowing the parser's buffer WITH its sentinel,
            // so `.ptr` is already a C string, owned by the parser, as the
            // header says.
            // SAFETY: `out` is writable `*const u8` storage (caller contract).
            unsafe { *out = v.as_ptr() };
            // return true;
            true
        }
        // else => return false,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::with_test_allocator;

    // test "alloc" {
    #[test]
    fn alloc() {
        with_test_allocator(|calloc| {
            // var p: Parser = undefined;
            let mut p: Parser = std::ptr::null_mut();
            // try testing.expectEqual(Result.success, new(&lib.alloc.test_allocator, &p));
            assert_eq!(Result::Success, unsafe { new(Some(calloc), &mut p) });
            // free(p);
            unsafe { free(p) };
        });
    }

    // Parity S174-F3 (no reference test): a C-created parser allocates its
    // OSC capture through the caller's allocator (`osc.Parser.init(alloc)`,
    // `Capture.allocating(.., alloc, ..)`), not the Rust heap. An OSC 52
    // longer than the fixed buffer must grow through it, and everything it
    // handed out must come back by `free`.
    #[test]
    fn c_parser_captures_through_the_caller_allocator() {
        let counting = crate::test_util::CountingCAllocator::new();
        let calloc = counting.c_allocator();
        let mut p: Parser = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe { new(Some(&calloc), &mut p) });
        let after_new = counting.alloc_calls.get();
        assert_eq!(1, after_new, "only the parser wrapper itself");

        for &b in b"52;c;" {
            unsafe { next(p, b) };
        }
        for _ in 0..(osc::MAX_BUF * 2) {
            unsafe { next(p, b'A') };
        }
        // The initial capture (MAX_BUF) and at least one growth past it.
        assert!(
            counting.alloc_calls.get() >= after_new + 2,
            "capture allocations went elsewhere: {} calls",
            counting.alloc_calls.get()
        );
        let cmd = unsafe { end(p, 0) };
        assert_eq!(osc::CommandKey::ClipboardContents, unsafe {
            command_type(cmd)
        });

        unsafe { free(p) };
        assert_eq!(0, counting.live_bytes.get());
        assert_eq!(counting.alloc_calls.get(), counting.free_calls.get());
    }

    // Parity S174-F3: when the caller's allocator refuses the capture, the
    // parser falls back to the fixed buffer, as the reference's
    // `Capture.allocating(..) catch { self.captureTrailing(.fixed); }` does:
    // a short OSC 52 still parses, a longer one is invalid, and nothing aborts.
    // A refused growth of an allocating capture makes the OSC invalid.
    #[test]
    fn c_parser_capture_falls_back_to_fixed_when_the_caller_allocator_fails() {
        let counting = crate::test_util::CountingCAllocator::new();
        let calloc = counting.c_allocator();
        let mut p: Parser = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe { new(Some(&calloc), &mut p) });
        counting.reject.set(true);

        for &b in b"52;c;aGk=" {
            unsafe { next(p, b) };
        }
        assert_eq!(2, counting.alloc_calls.get(), "the refused capture");
        let cmd = unsafe { end(p, 0) };
        assert_eq!(osc::CommandKey::ClipboardContents, unsafe {
            command_type(cmd)
        });

        unsafe { reset(p) };
        for &b in b"52;c;" {
            unsafe { next(p, b) };
        }
        for _ in 0..(osc::MAX_BUF * 2) {
            unsafe { next(p, b'A') };
        }
        let cmd = unsafe { end(p, 0) };
        assert_eq!(osc::CommandKey::Invalid, unsafe { command_type(cmd) });

        // A refused GROWTH (`realloc(..) catch return error.WriteFailed`)
        // makes the OSC invalid.
        counting.reject.set(false);
        unsafe { reset(p) };
        for &b in b"52;c;" {
            unsafe { next(p, b) };
        }
        let before = counting.alloc_calls.get();
        counting.reject.set(true);
        for _ in 0..(osc::MAX_BUF * 2) {
            unsafe { next(p, b'A') };
        }
        assert!(counting.alloc_calls.get() > before, "the refused growth");
        let cmd = unsafe { end(p, 0) };
        assert_eq!(osc::CommandKey::Invalid, unsafe { command_type(cmd) });

        counting.reject.set(false);
        unsafe { free(p) };
        assert_eq!(0, counting.live_bytes.get());
    }

    // Parity tail S7-1 (no reference test): the capture's allocator is the
    // runtime handle over the caller's own `RusttyAllocator`, and its growth
    // is Zig's `realloc` (`w.allocator.realloc(..)`, osc.zig): `remap` first,
    // alloc + copy + free only when the remap is refused. The counting
    // allocator's heap remaps, so the growth is remaps with no extra alloc.
    // Negative control: the fallback shape (a `FailingAllocator` that refuses
    // every remap) must show the alloc + copy + free instead.
    #[test]
    fn c_parser_capture_grows_by_remap_like_zig_realloc() {
        use rustty_vt::alloc::testing::{Config, CountingAllocator, FailingAllocator};

        let feed_long_osc52 = |p: Parser| {
            for &b in b"52;c;" {
                unsafe { next(p, b) };
            }
            for _ in 0..(osc::MAX_BUF * 4) {
                unsafe { next(p, b'A') };
            }
            let cmd = unsafe { end(p, 0) };
            assert_eq!(osc::CommandKey::ClipboardContents, unsafe {
                command_type(cmd)
            });
        };

        let counting = CountingAllocator::over_heap();
        let calloc = counting.allocator();
        let mut p: Parser = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe { new(Some(&calloc), &mut p) });
        assert_eq!(
            1,
            counting.alloc_calls.get(),
            "only the parser wrapper itself"
        );
        feed_long_osc52(p);
        assert_eq!(
            2,
            counting.alloc_calls.get(),
            "wrapper + the initial capture"
        );
        assert!(counting.remap_calls.get() >= 2, "the capture grew by remap");
        unsafe { free(p) };
        assert_eq!(0, counting.live_bytes.get());
        assert_eq!(counting.alloc_calls.get(), counting.free_calls.get());

        // Negative control: remaps refused, so every growth allocates fresh.
        let failing = FailingAllocator::over_heap(Config {
            resize_fail_index: 0,
            ..Config::default()
        });
        let calloc = failing.allocator();
        let mut p: Parser = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe { new(Some(&calloc), &mut p) });
        feed_long_osc52(p);
        assert!(
            failing.allocations.get() >= 4,
            "wrapper + capture + >= 2 growths"
        );
        assert_eq!(failing.resize_index.get(), 0, "no remap succeeded");
        unsafe { free(p) };
        assert_eq!(failing.allocations.get(), failing.deallocations.get());
        assert_eq!(failing.allocated_bytes.get(), failing.freed_bytes.get());

        // NULL: the Rust heap (`lib.alloc.default(null)`), same result.
        let mut p: Parser = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe { new(None, &mut p) });
        feed_long_osc52(p);
        unsafe { free(p) };
    }

    // test "command type null" {
    #[test]
    fn command_type_null() {
        // try testing.expectEqual(.invalid, commandType(null));
        assert_eq!(osc::CommandKey::Invalid, unsafe {
            command_type(std::ptr::null_mut())
        });
    }

    // Parity tail S7-7, N323 (R13; no reference test): `commandData` on a NULL
    // command (what `end` returns for an invalid OSC; the header says the
    // command "may be NULL") answers `false` and leaves `out` alone, where
    // the reference's `command_.?` traps.
    #[test]
    fn command_data_on_a_null_command_is_false() {
        let mut p: Parser = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe { new(None, &mut p) });
        unsafe { next(p, b'2') };
        let cmd = unsafe { end(p, 0x07) };
        assert!(cmd.is_null(), "an unfinished OSC has no command");
        let sentinel: *const u8 = std::ptr::dangling();
        let mut title: *const u8 = sentinel;
        assert!(!unsafe {
            command_data(
                cmd,
                CommandData::ChangeWindowTitleStr as c_int,
                &mut title as *mut *const u8 as *mut c_void,
            )
        });
        assert_eq!(sentinel, title, "out is untouched");
        unsafe { free(p) };
    }

    // test "change window title" {
    #[test]
    fn change_window_title() {
        with_test_allocator(|calloc| {
            // var p: Parser = undefined;
            let mut p: Parser = std::ptr::null_mut();
            // try testing.expectEqual(Result.success, new(&lib.alloc.test_allocator, &p));
            assert_eq!(Result::Success, unsafe { new(Some(calloc), &mut p) });
            // defer free(p);

            // Parse it
            // next(p, '0'); next(p, ';'); next(p, 'a');
            unsafe { next(p, b'0') };
            unsafe { next(p, b';') };
            unsafe { next(p, b'a') };
            // const cmd = end(p, 0);
            let cmd = unsafe { end(p, 0) };
            // try testing.expectEqual(.change_window_title, commandType(cmd));
            assert_eq!(osc::CommandKey::ChangeWindowTitle, unsafe {
                command_type(cmd)
            });

            // Extract the title
            // var title: [*:0]const u8 = undefined;
            let mut title: *const u8 = std::ptr::null();
            // try testing.expect(commandData(cmd, .change_window_title_str, @ptrCast(&title)));
            assert!(unsafe {
                command_data(
                    cmd,
                    CommandData::ChangeWindowTitleStr as c_int,
                    &mut title as *mut *const u8 as *mut c_void,
                )
            });
            // try testing.expectEqualStrings("a", std.mem.span(title));
            let span = unsafe { std::ffi::CStr::from_ptr(title as *const std::os::raw::c_char) };
            assert_eq!(b"a", span.to_bytes());

            // defer free(p);
            unsafe { free(p) };
        });
    }

    // task-14a (rustty-only proving test, NOT a reference test — S174-F2):
    // the commandData invalid-data-value warning used to bypass `sys::log_fn`
    // with a raw `eprintln!`, unreachable by an embedder's installed log
    // callback. Proves the message now reaches the sink under the `osc` scope.
    #[test]
    fn command_data_invalid_value_warns_through_the_scoped_log_sink() {
        let _guard = crate::sys::tests::reset_global();

        static CAPTURED: std::sync::Mutex<Vec<(crate::sys::LogLevel, String, String)>> =
            std::sync::Mutex::new(Vec::new());
        extern "C" fn log_cb(
            _userdata: *mut c_void,
            level: crate::sys::LogLevel,
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
        CAPTURED.lock().unwrap().clear();

        assert_eq!(Result::Success, unsafe {
            crate::sys::set(crate::sys::Option_::Log, log_cb as *const c_void)
        });

        // An out-of-range `CommandData` discriminant on a valid command.
        assert!(!unsafe { command_data(std::ptr::null_mut(), 999, std::ptr::null_mut()) });

        assert_eq!(Result::Success, unsafe {
            crate::sys::set(crate::sys::Option_::Log, std::ptr::null())
        });

        // FLAKE FIX (task 15, N28): compare only the entries on THIS test's own
        // log scope. The sys log callback is process-global, so a concurrently
        // running test's diagnostic lands in whichever buffer is installed; a
        // whole-buffer equality would then fail for a reason this test does not
        // control. Nothing else in the binary emits on this scope, so no
        // coverage is lost.
        let captured: Vec<_> = CAPTURED
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, s, _)| s == super::LOG_SCOPE)
            .cloned()
            .collect();
        assert_eq!(
            captured,
            vec![(
                crate::sys::LogLevel::Warning,
                "osc".to_string(),
                "commandData invalid data value=999".to_string(),
            )],
            "the S174-F2 warning was not routed through sys::log_fn"
        );
    }

    // Parity tail S7-7 (Rust-only, no reference test): a C-created parser
    // grows the color request list through the caller's allocator
    // (`parseColor(parser.alloc, ..)`): none for two requests (the
    // preallocated slots), the shelf array and shelf 0 for the third. The
    // parser owns the list (as the reference's `parser.command` does) and
    // frees it at `reset`, and `free` returns everything. `next` ends the
    // command's borrow of the parser's buffer (the header's lifetime) but
    // frees nothing, as in the reference.
    #[test]
    fn c_parser_color_requests_route_through_the_caller_allocator() {
        use rustty_vt::alloc::testing::CountingAllocator;

        let counting = CountingAllocator::over_heap();
        let calloc = counting.allocator();
        let mut p: Parser = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe { new(Some(&calloc), &mut p) });
        assert_eq!(1, counting.alloc_calls.get(), "the parser itself");

        for &b in b"104;1;2" {
            unsafe { next(p, b) };
        }
        let cmd = unsafe { end(p, 0x07) };
        assert_eq!(osc::CommandKey::ColorOperation, unsafe {
            command_type(cmd)
        });
        assert_eq!(
            1,
            counting.alloc_calls.get(),
            "two requests fit the prealloc"
        );

        unsafe { reset(p) };
        for &b in b"104;1;2;3" {
            unsafe { next(p, b) };
        }
        let cmd = unsafe { end(p, 0x07) };
        assert_eq!(osc::CommandKey::ColorOperation, unsafe {
            command_type(cmd)
        });
        // SAFETY: `cmd` is the live command (no call on the parser since).
        match unsafe { &*cmd } {
            osc::Command::ColorOperation { requests, .. } => assert_eq!(3, requests.count()),
            other => panic!("expected color_operation, got {other:?}"),
        }
        assert_eq!(
            3,
            counting.alloc_calls.get(),
            "+ the shelf array and shelf 0"
        );
        assert_eq!(0, counting.free_calls.get(), "owned by the parser");

        unsafe { reset(p) };
        assert_eq!(2, counting.free_calls.get(), "`reset` frees the list");

        // `next` is a call on the parser too: the command's borrow of the
        // buffer ends before the byte lands (the wrapper drops it), but the
        // list stays with the parser until `reset`.
        for &b in b"104;1;2;3" {
            unsafe { next(p, b) };
        }
        let cmd = unsafe { end(p, 0x07) };
        assert!(!cmd.is_null());
        assert_eq!(5, counting.alloc_calls.get());
        unsafe { next(p, b';') };
        // SAFETY: `p` is live; the wrapper's stored command is inspected.
        assert!(unsafe { (*p).command.is_none() }, "next ended the command");
        assert_eq!(
            2,
            counting.free_calls.get(),
            "the parser still owns the list"
        );
        unsafe { reset(p) };
        assert_eq!(4, counting.free_calls.get());
        unsafe { free(p) };
        assert_eq!(0, counting.live_bytes.get());
        assert_eq!(counting.alloc_calls.get(), counting.free_calls.get());
    }

    // Parity tail S7-7, N263: a title is the parser's own sentinel-terminated
    // bytes (the reference's `v.ptr` into the capture), not a copy: parsing it
    // allocates nothing beyond the parser, and the string reads back through
    // `commandData` until the next call on the parser.
    #[test]
    fn c_parser_title_is_borrowed_not_copied() {
        use rustty_vt::alloc::testing::CountingAllocator;

        let counting = CountingAllocator::over_heap();
        let calloc = counting.allocator();
        let mut p: Parser = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe { new(Some(&calloc), &mut p) });
        for &b in b"2;a borrowed title" {
            unsafe { next(p, b) };
        }
        let cmd = unsafe { end(p, 0x07) };
        let mut title: *const u8 = std::ptr::null();
        assert!(unsafe {
            command_data(
                cmd,
                CommandData::ChangeWindowTitleStr as c_int,
                &mut title as *mut *const u8 as *mut c_void,
            )
        });
        // SAFETY: `p` is live; the buffer is inside the parser's storage.
        let buffer = unsafe { (*p).parser.buffer.as_ptr_range() };
        assert!(buffer.contains(&title), "the title is the parser's buffer");
        // SAFETY: valid until the next call on `p`.
        let span = unsafe { std::ffi::CStr::from_ptr(title as *const std::os::raw::c_char) };
        assert_eq!(b"a borrowed title", span.to_bytes());
        // Introspection is not "a call on the parser": the command stays.
        assert_eq!(osc::CommandKey::ChangeWindowTitle, unsafe {
            command_type(cmd)
        });
        assert_eq!(1, counting.alloc_calls.get(), "only the parser itself");
        unsafe { free(p) };
        assert_eq!(0, counting.live_bytes.get());
    }

    /// One pass of the OSC C-API scenario over `failing`: every command kind
    /// that allocates through the parser (the allocating capture and its
    /// growth, the color list past its preallocation, the kitty color list),
    /// with `end`, introspection and `reset` after each. Returns whether `new`
    /// failed.
    fn osc_sweep_once(failing: &rustty_vt::alloc::testing::FailingAllocator, what: &str) -> bool {
        let calloc = failing.allocator();
        let mut p: Parser = std::ptr::null_mut();
        match unsafe { new(Some(&calloc), &mut p) } {
            Result::Success => {}
            Result::OutOfMemory => {
                assert!(p.is_null(), "{what}: no parser on OUT_OF_MEMORY");
                assert_eq!(failing.allocated_bytes.get(), failing.freed_bytes.get());
                return true;
            }
            other => panic!("{what}: unexpected result {other:?}"),
        }
        let long = if cfg!(miri) { 64 } else { osc::MAX_BUF * 3 };
        let mut corpus: Vec<Vec<u8>> = vec![
            b"0;a title".to_vec(),
            b"4;1;?;2;?;3;?;4;?;5;?;6;?;7;?;8;?".to_vec(),
            b"10;?;?;?;?;?;?;?".to_vec(),
            b"104;1;2;3;4;5".to_vec(),
            b"21;foreground=?;background=#ffffff;cursor=;2=?;3=rgb:10/20/30".to_vec(),
            b"52;c;aGVsbG8=".to_vec(),
            b"8;id=x;https://example.com".to_vec(),
        ];
        let mut osc52 = b"52;c;".to_vec();
        osc52.extend(std::iter::repeat_n(b'A', long));
        corpus.push(osc52);
        if cfg!(miri) {
            corpus.truncate(5);
            corpus.push(b"52;c;aGk=".to_vec());
        }
        for input in &corpus {
            for &b in input {
                unsafe { next(p, b) };
            }
            let cmd = unsafe { end(p, 0x07) };
            let kind = unsafe { command_type(cmd) };
            let mut title: *const u8 = std::ptr::null();
            if unsafe {
                command_data(
                    cmd,
                    CommandData::ChangeWindowTitleStr as c_int,
                    &mut title as *mut *const u8 as *mut c_void,
                )
            } {
                assert_eq!(osc::CommandKey::ChangeWindowTitle, kind);
                // SAFETY: valid until the next call on `p`.
                let span =
                    unsafe { std::ffi::CStr::from_ptr(title as *const std::os::raw::c_char) };
                assert_eq!(b"a title", span.to_bytes(), "{what}");
            }
            unsafe { reset(p) };
        }
        unsafe { free(p) };
        assert_eq!(
            failing.allocated_bytes.get(),
            failing.freed_bytes.get(),
            "{what}: every byte returned"
        );
        assert_eq!(
            failing.allocations.get(),
            failing.deallocations.get(),
            "{what}: every allocation freed"
        );
        false
    }

    // Parity tail S7-7 (Rust-only; the reference has no C-API failure test): a
    // `FailingAllocator` that refuses the n-th allocation, for every n the
    // scenario reaches, and then every n-th remap. The process never aborts,
    // `new` answers OUT_OF_MEMORY or SUCCESS, every refusal takes the
    // reference's path (capture fixed fallback or invalid, color catch with an
    // empty list, kitty warn-and-continue), and every byte comes back.
    #[test]
    fn osc_allocation_failure_sweep_through_the_c_api() {
        use rustty_vt::alloc::testing::{Config, FailingAllocator};

        let mut n = 0usize;
        let mut new_failures = 0usize;
        loop {
            let failing = FailingAllocator::over_heap(Config {
                fail_index: n,
                ..Config::default()
            });
            new_failures += usize::from(osc_sweep_once(&failing, &format!("alloc n={n}")));
            if !failing.has_induced_failure.get() {
                break;
            }
            n += 1;
            assert!(n < 10_000, "the sweep must terminate");
        }
        // The parser, the captures and their growth, the color shelves and the
        // kitty lists: well past the parser's own allocation.
        let min_swept = if cfg!(miri) { 4 } else { 12 };
        assert!(n > min_swept, "only {n} allocations were swept");
        assert_eq!(1, new_failures, "only n = 0 fails `new`");

        let clean = FailingAllocator::over_heap(Config::default());
        assert!(!osc_sweep_once(&clean, "remap clean run"));
        assert!(!clean.has_induced_failure.get());
        let remaps = clean.resize_index.get();
        assert!(remaps > 0, "the corpus makes no remap/resize call");
        for n in 0..=remaps {
            let failing = FailingAllocator::over_heap(Config {
                resize_fail_index: n,
                ..Config::default()
            });
            assert!(
                !osc_sweep_once(&failing, &format!("remap n={n}")),
                "remap n={n}: new never remaps"
            );
            assert!(
                failing.resize_index.get() <= n,
                "remap n={n}: no remap/resize succeeded past the refusal"
            );
        }
    }
}
