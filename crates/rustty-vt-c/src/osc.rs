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
use rustty_lib::allocator::{Allocator as CAllocator, GlobalZigAllocator, ZigAllocatorRef};
use rustty_vt::osc;
use std::ffi::c_void;
use std::os::raw::c_int;

use crate::cwrap::{create, destroy};

// PORT NOTE (lifetime/allocator): the reference `osc.Parser` carries its own
// `std.mem.Allocator` (`parser.alloc`) and `free` reconstructs it from the
// stored field. rustty's `osc::Parser<'a>` is parameterized by the allocator
// lifetime; following the same choice `terminal::new_` makes for its embedded
// OSC parser, the C-built parser is backed by the process-static global zig
// allocator, which supports the dynamic OSC growth the parser needs. The
// `OscParserWrapper` mirrors the sgr `ParserWrapper`: it stores a
// `osc::Parser<'static>` plus the original (possibly null) C `alloc_` (a `Copy`
// C struct; see `cwrap` PORT NOTE) so `free` can release the wrapper through the
// SAME allocator the reference's `parser.alloc.?` would select.
static GLOBAL_ZIG_ALLOCATOR: GlobalZigAllocator = GlobalZigAllocator;

/// Wrapper around the parser that tracks the allocator for C API usage.
// PORT NOTE: the reference stores `parser.alloc` inside `osc.Parser` itself
// (`.init(alloc)`). rustty's `osc::Parser<'static>` is backed by the static
// global allocator, so the wrapper additionally carries the original C `alloc_`
// used to allocate the wrapper struct, matching `free`'s `parser.alloc.?`.
pub struct OscParserWrapper {
    parser: osc::Parser<'static>,
    alloc: Option<CAllocator>,
}

/// C: RusttyOscParser (R2)
// pub const Parser = ?*osc.Parser;
pub type Parser = *mut OscParserWrapper;

/// C: RusttyOscCommand (R2)
// pub const Command = ?*osc.Command;
pub type Command = *mut osc::Command;

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
    // const ptr = alloc.create(osc.Parser) catch return .out_of_memory;
    // ptr.* = .init(alloc);
    // PORT NOTE: `.init(alloc)` is the parser's own embedded allocator. We back
    // it with the process-static global zig allocator (see module PORT NOTE) and
    // store the original C `alloc_` for `free`.
    let ptr = unsafe {
        create(
            alloc_,
            OscParserWrapper {
                parser: osc::Parser::init(Some(ZigAllocatorRef::new(&GLOBAL_ZIG_ALLOCATOR))),
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
    // SAFETY: `parser_` is non-null per the caller contract (`.?`).
    unsafe { (*parser_).parser.reset() };
}

// pub fn next(parser_: Parser, byte: u8) ... void {
//     parser_.?.next(byte);
// }
//
/// # Safety
/// `parser_` must be a non-null handle returned by `new`.
pub unsafe fn next(parser_: Parser, byte: u8) {
    // parser_.?.next(byte);
    // SAFETY: `parser_` is non-null per the caller contract (`.?`).
    unsafe { (*parser_).parser.next(byte) };
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
    // raw `u8` terminator; rustty wraps it in `Some`). It returns
    // `Option<&mut Command>`; project back to a raw `*mut Command` (null for
    // `None`).
    // SAFETY: `parser_` is non-null per the caller contract (`.?`).
    match unsafe { (*parser_).parser.end(Some(terminator)) } {
        Some(cmd) => cmd as *mut osc::Command,
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
/// `command_` must be non-null (`command_.?`); `out` must point to writable
/// `*const u8` storage.
unsafe fn command_data_change_window_title_str(command_: Command, out: *mut *const u8) -> bool {
    // const command = command_.?;
    // SAFETY: `command_` is non-null per the caller contract (`.?`).
    let command = unsafe { &mut *command_ };
    // switch (command.*) { .change_window_title => |v| out.* = v.ptr, else => return false }
    match command {
        osc::Command::ChangeWindowTitle(v) => {
            // out.* = v.ptr;
            // PORT NOTE (fidelity): the reference field is `[:0]const u8` — its
            // `.ptr` is a NUL-terminated C string. rustty's source-of-truth
            // `osc::Command::ChangeWindowTitle(Vec<u8>)` stores the title WITHOUT
            // the trailing NUL (`change_window_title.rs` excludes the sentinel),
            // so we restore the sentinel in place before exposing the pointer to
            // C consumers. C consumers reach this command only through this FFI
            // surface, so appending a single NUL byte is behavior-equivalent to
            // the reference's `[:0]const u8` and keeps the returned pointer a
            // valid C string. This mutates the owned `Vec` (the command outlives
            // the call via the parser), never reallocating away the live bytes.
            if v.last() != Some(&0) {
                v.push(0);
            }
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

    // test "command type null" {
    #[test]
    fn command_type_null() {
        // try testing.expectEqual(.invalid, commandType(null));
        assert_eq!(osc::CommandKey::Invalid, unsafe {
            command_type(std::ptr::null_mut())
        });
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
}
