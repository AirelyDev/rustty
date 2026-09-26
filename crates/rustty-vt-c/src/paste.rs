//! rustty-vt-c paste
//!
//! PORTS: ghostty src/terminal/c/paste.zig
//!
//! PORT STATUS: done (P5-W6 Bundle P: `terminal_paste` streaming END STATE —
//! 60a1ae2df + da27e6c90's `MimeReader` Request, never the intermediate
//! inline-data form)

// PORT NOTE: the reference imports are
//   const std = @import("std");
//   const lib = @import("../lib.zig");
//   const paste = @import("../../input/paste.zig");
//   const terminal_paste_pkg = @import("../paste.zig");
//   const clipboard = @import("../clipboard.zig");
//   const io = @import("io.zig");
//   const terminal_c = @import("terminal.zig");
//   const Terminal = terminal_c.Terminal;
//   const ClipboardContent = terminal_c.ClipboardContent;
//   const ClipboardRead = terminal_c.ClipboardRead;
//   const ClipboardReadReply = terminal_c.ClipboardReadReply;
//   const Result = @import("result.zig").Result;
// `paste` is rustty-vt's `input::paste` module; `terminal_paste_pkg` is
// rustty-vt's `paste` module; `Result` is the sibling `result` module's
// `Result`.

use crate::io;
use crate::result::Result;
use crate::terminal::Terminal;
use rustty_lib::String as LibString;
use rustty_vt::clipboard;
use rustty_vt::input::paste;
use rustty_vt::paste as terminal_paste_pkg;
use rustty_vt::stream_terminal::PasteError;
use std::ffi::c_void;
use std::os::raw::c_int;

/// Why a paste happened. The flat C form of the Zig tagged union
/// (terminal.paste.Source); the location rides alongside in
/// RusttyPaste and only applies to a clipboard paste.
///
/// C: GhosttyPasteSource -> RusttyPasteSource (R2)
// pub const Source = lib.Enum(lib.target, &.{
//     "clipboard",
//     "text",
// });
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Clipboard = 0,
    Text = 1,
}

// D11 (P3-W6 T2): proves holding the REQUEST field as a raw `c_int` (see
// `Request.source`) loses no width against the reference's `lib.Enum` c_int.
const _: () = assert!(std::mem::size_of::<Source>() == std::mem::size_of::<c_int>());

/// A paste of clipboard contents into the terminal. Sized struct.
///
/// C: GhosttyPaste -> RusttyPaste (R2)
// pub const Request = extern struct {
//     size: usize = @sizeOf(Request),
//     location: clipboard.Location,
//     source: Source,
//     mimes: ?[*]const lib.String,
//     mimes_len: usize,
//     reader: io.MimeReader,
//     allow_unsafe: bool,
// };
//
// D11 (P3-W6 T2): `location`/`source` are held as the raw C integers, NOT as
// the closed Rust enums — the struct arrives from C memory, and materializing
// a `#[repr(C)]` enum from an out-of-range tag is UB before any check could
// run. `terminal_paste` is the narrowing point for `source`, and wraps
// `location` in the open `clipboard::Location` without narrowing it (parity
// S174-F4).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Request {
    pub size: usize,
    pub location: c_int,
    pub source: c_int,
    pub mimes: *const LibString,
    pub mimes_len: usize,
    pub reader: io::MimeReader,
    pub allow_unsafe: bool,
}

/// PORT NOTE: `pub fn terminal_paste(terminal_, req_, out_written)
/// callconv(...) Result`. The C calling convention is the `extern "C"` wrapper
/// in `lib.rs` (`rustty_terminal_paste`).
///
/// # Safety
/// `terminal_`, if non-null, must be a live terminal handle; `req_`, if
/// non-null, must point to a readable `Request` (with `mimes`/`reader`
/// describing valid memory); `out_written`, if non-null, must be a valid,
/// writable pointer to `bool`.
pub unsafe fn terminal_paste(
    terminal_: Terminal,
    req_: *const Request,
    out_written: *mut bool,
) -> Result {
    // const wrapper = terminal_ orelse return .invalid_value;
    if terminal_.is_null() {
        return Result::InvalidValue;
    }
    // const req = req_ orelse return .invalid_value;
    if req_.is_null() {
        return Result::InvalidValue;
    }
    // SAFETY: `req_` is non-null and readable (caller contract).
    let req: &Request = unsafe { &*req_ };

    // Every field is required; a smaller size is a caller from a
    // different ABI version than any this struct has had.
    // if (req.size < @sizeOf(Request)) return .invalid_value;
    if req.size < std::mem::size_of::<Request>() {
        return Result::InvalidValue;
    }

    // The handler always has a write_pty trampoline that no-ops without
    // a C callback, so the "nothing can be written" check is ours.
    // if (wrapper.effects.write_pty == null) return .invalid_value;
    // SAFETY: `terminal_` is a live wrapper (caller contract).
    if unsafe { (*terminal_).effects.write_pty }.is_none() {
        return Result::InvalidValue;
    }

    // const c_mimes: []const lib.String = if (req.mimes) |ptr| ptr[0..req.mimes_len] else &.{};
    //
    // PORT NOTE: kept as (ptr, len) and turned into a `&[LibString]` only where
    // the elements are actually read, below. Zig's `ptr[0..len]` on a many-item
    // pointer imposes no validity requirement, but Rust's
    // `slice::from_raw_parts` requires the whole range to be one live
    // allocation — so forming it here would make the reference's own "the
    // length alone is absurd" case UB instead of an error return.
    let c_mimes_ptr = req.mimes;
    let c_mimes_len = if c_mimes_ptr.is_null() {
        0
    } else {
        req.mimes_len
    };

    // Only a paste with something to read needs a reader.
    // if (c_mimes.len > 0 and !req.reader.valid()) return .invalid_value;
    if c_mimes_len > 0 && !req.reader.valid() {
        return Result::InvalidValue;
    }

    // A paste carries a handful of representations, so keep the common
    // case allocation-free.
    // var sfa = std.heap.stackFallback(256, wrapper.terminal.gpa());
    // const alloc = sfa.get();
    // const mimes = alloc.alloc([]const u8, c_mimes.len) catch return .out_of_memory;
    // defer alloc.free(mimes);
    // (parity tail S7-4: `alloc::StackFallback` over the terminal's `gpa()`;
    // the active screen's allocator is read through the raw wrapper.)
    //
    // PORT NOTE (parity S174-F5): the reference's
    // `alloc.alloc([]const u8, c_mimes.len) catch return .out_of_memory` IS an
    // observable C return, and `mimes_len` is caller-supplied. A `collect()`
    // aborts instead, so the documented `RUSTTY_OUT_OF_MEMORY` was unreachable
    // — and unlike most of the DG5 class this one does not need real memory
    // pressure to reach: an absurd `mimes_len` alone fails the reservation.
    // `try_reserve_exact` is that fallible allocation; it runs BEFORE the
    // elements are read, matching the reference's allocate-then-fill order.
    // SAFETY: `terminal_` is a live wrapper (caller contract); raw reads.
    let gpa = unsafe { (*(*(*terminal_).terminal).screens.active).alloc };
    let sfa = rustty_vt::alloc::StackFallback::<256>::new(gpa);
    let mut mimes: allocator_api2::vec::Vec<&[u8], &rustty_vt::alloc::StackFallback<256>> =
        allocator_api2::vec::Vec::new_in(&sfa);
    if mimes.try_reserve_exact(c_mimes_len).is_err() {
        return Result::OutOfMemory;
    }
    // SAFETY: `mimes`/`mimes_len` describe readable C memory when non-null
    // (caller contract); the reservation above has already rejected a length
    // no allocation could describe.
    let c_mimes: &[LibString] = if c_mimes_ptr.is_null() {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(c_mimes_ptr, c_mimes_len) }
    };
    // for (mimes, c_mimes) |*mime, c_mime| mime.* = c_mime.ptr[0..c_mime.len];
    // SAFETY: each `LibString` names readable C memory for the call duration.
    for c_mime in c_mimes {
        mimes.push(unsafe { std::slice::from_raw_parts(c_mime.ptr, c_mime.len) });
    }

    // .source = switch (req.source) {
    //     .clipboard => .{ .clipboard = req.location },
    //     .text => .text,
    // },
    //
    // PORT NOTE (D11 narrowing; R7 soundness override, recorded): the
    // reference's `Source` is an exhaustive `enum(c_int)` — a safety-checked
    // oracle build TRAPS on an out-of-range source — so the source narrows
    // HERE and an out-of-range value is `.invalid_value` (the crate-wide
    // closed-enum boundary policy, D11). Its `Location` is OPEN (`_`), and
    // `clipboard::Location` is an open newtype that holds any `c_int`, so the
    // location passes through unchecked, as in the reference (paste events
    // report any non-standard location as primary).
    // CORRECTION (parity S174-F4): the location used to narrow to 0..=2 as
    // well, returning `.invalid_value` for any other value.
    let source = match req.source {
        0 => terminal_paste_pkg::Source::Clipboard(clipboard::Location(req.location)),
        1 => terminal_paste_pkg::Source::Text,
        _ => return Result::InvalidValue,
    };

    // const written = wrapper.stream.handler.paste(.{
    //     .source = ...,
    //     .contents = .{ .reader = .{
    //         .mimes = mimes,
    //         .read = .{ .ctx = @constCast(req), .read_fn = &readTrampoline },
    //     } },
    //     .allow_unsafe = req.allow_unsafe,
    // }) catch |err| return switch (err) { ... };
    //
    // PORT NOTE (`@constCast(req)`, hazard §5.3): the `*const Request` is
    // const-cast ONLY to fit the opaque `ctx: *mut c_void` slot; the
    // trampoline reads it back as `*const Request` and never writes through
    // it — no `*mut`-intending-writes is ever minted from the `&`-derived
    // pointer.
    let result = unsafe {
        (*terminal_)
            .stream
            .handler
            .paste(terminal_paste_pkg::Request {
                source,
                contents: terminal_paste_pkg::Contents::Reader(terminal_paste_pkg::Reader {
                    mimes: &mimes,
                    read: clipboard::MimeReader {
                        ctx: req_ as *mut Request as *mut c_void,
                        read_fn: read_trampoline,
                    },
                }),
                allow_unsafe: req.allow_unsafe,
            })
    };
    let written = match result {
        Ok(written) => written,
        Err(err) => {
            return match err {
                PasteError::UnsafePaste => Result::Rejected,
                PasteError::NoWritePty => Result::InvalidValue,
                PasteError::OutOfMemory => Result::OutOfMemory,
                PasteError::ReadFailed | PasteError::EntropyUnavailable | PasteError::Canceled => {
                    Result::IoError
                }
            }
        }
    };
    // if (out_written) |ptr| ptr.* = written;
    if !out_written.is_null() {
        // SAFETY: `out_written` is non-null and writable (caller contract).
        unsafe { *out_written = written };
    }
    // return .success;
    Result::Success
}

/// The sink handed to the C read callback: a RusttyWriter over the
/// Zig sink, remembering whether the sink itself failed so that can be
/// told apart from the callback failing to read.
// const Sink = struct {
//     writer: *std.Io.Writer,
//     write_failed: bool = false,
//     fn write(userdata, data, len) callconv(lib.calling_conv) bool { ... }
// };
//
// PORT NOTE (hazard §5.6): the sink holds only its OWN raw trait-object
// pointer to the paste encoder's writer; the C round-trip erases the
// writer's lifetime (see `read_trampoline`), which is sound because the
// Sink lives strictly within that call.
struct Sink {
    writer: *mut (dyn rustty_vt::snapshot::io::Writer + 'static),
    write_failed: bool,
}

impl Sink {
    // fn write(userdata: ?*anyopaque, data: [*]const u8, len: usize) callconv(...) bool {
    //     const self: *Sink = @ptrCast(@alignCast(userdata.?));
    //     self.writer.writeAll(data[0..len]) catch {
    //         self.write_failed = true;
    //         return false;
    //     };
    //     return true;
    // }
    extern "C" fn write(userdata: *mut c_void, data: *const u8, len: usize) -> bool {
        // SAFETY: `userdata` is the `read_trampoline` frame's `Sink`, live for
        // the duration of the C read callback; `data`/`len` describe readable
        // C memory for this call.
        let sink = userdata as *mut Sink;
        let bytes = unsafe { std::slice::from_raw_parts(data, len) };
        match unsafe { (*(*sink).writer).write_all(bytes) } {
            Ok(()) => true,
            Err(_) => {
                unsafe { (*sink).write_failed = true };
                false
            }
        }
    }
}

// fn readTrampoline(
//     ctx: ?*anyopaque,
//     mime: []const u8,
//     writer: *std.Io.Writer,
// ) clipboard.MimeReader.Error!void { ... }
fn read_trampoline(
    ctx: *mut c_void,
    mime: &[u8],
    writer: &mut dyn rustty_vt::snapshot::io::Writer,
) -> std::result::Result<(), clipboard::MimeReaderError> {
    // const req: *const Request = @ptrCast(@alignCast(ctx.?));
    let req: *const Request = ctx as *const Request;
    // var sink: Sink = .{ .writer = writer };
    //
    // SAFETY (lifetime erasure): the borrowed `writer` outlives this call and
    // the `Sink` never escapes it, so widening the trait-object lifetime for
    // the C round-trip cannot dangle. Fat-pointer layout is unchanged.
    let writer_erased: *mut (dyn rustty_vt::snapshot::io::Writer + 'static) = unsafe {
        std::mem::transmute::<
            *mut (dyn rustty_vt::snapshot::io::Writer + '_),
            *mut (dyn rustty_vt::snapshot::io::Writer + 'static),
        >(writer)
    };
    let mut sink = Sink {
        writer: writer_erased,
        write_failed: false,
    };
    // if (!req.reader.read.?(req.reader.userdata, .init(mime), .{
    //     .write = &Sink.write,
    //     .userdata = &sink,
    // })) {
    //     return if (sink.write_failed) error.WriteFailed else error.ReadFailed;
    // }
    // SAFETY: `req` is the live `terminal_paste` request (`ctx` is borrowed
    // for the paste call duration); `.?` on `read` is the reference's checked
    // unwrap (terminal_paste validated the reader when mimes exist).
    let reader = unsafe { (*req).reader };
    if !(reader.read.unwrap())(
        reader.userdata,
        LibString::init(mime),
        io::Writer {
            write: Some(Sink::write),
            userdata: &mut sink as *mut Sink as *mut c_void,
        },
    ) {
        return Err(if sink.write_failed {
            clipboard::MimeReaderError::WriteFailed
        } else {
            clipboard::MimeReaderError::ReadFailed
        });
    }
    Ok(())
}

/// PORT NOTE: `pub fn is_safe(data, len) callconv(...) bool`. The C calling
/// convention is the `extern "C"` wrapper in `lib.rs` (`rustty_paste_is_safe`).
/// `data` is the optional caller buffer (NULL ==> a zero-length slice).
///
/// # Safety
/// `data`, if non-null, must point to `len` readable bytes.
pub unsafe fn is_safe(data: *const u8, len: usize) -> bool {
    // const slice: []const u8 = if (data) |v| v[0..len] else &.{};
    // SAFETY: `data`/`len` describe a valid readable region when non-null
    // (caller contract).
    let slice: &[u8] = if data.is_null() {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(data, len) }
    };
    // return paste.isSafe(slice);
    paste::is_safe(slice)
}

/// PORT NOTE: `pub fn encode(data, data_len, bracketed, out_, out_len,
/// out_written) callconv(...) Result`. The C calling convention is the
/// `extern "C"` wrapper in `lib.rs` (`rustty_paste_encode`). `data` is mutated
/// in place by the strip pass (`paste.encode` takes `[]u8`), so the rustty
/// source-of-truth analog is `paste::encode_mut`.
///
/// # Safety
/// `data`, if non-null, must point to `data_len` writable bytes; `out_`, if
/// non-null, must point to `out_len` writable bytes; `out_written` must be a
/// valid, writable pointer to `usize`.
pub unsafe fn encode(
    data: *mut u8,
    data_len: usize,
    bracketed: bool,
    out_: *mut u8,
    out_len: usize,
    out_written: *mut usize,
) -> Result {
    // const slice: []u8 = if (data) |v| v[0..data_len] else &.{};
    // SAFETY: `data`/`data_len` describe a valid writable region when non-null
    // (caller contract).
    let mut empty: [u8; 0] = [];
    let slice: &mut [u8] = if data.is_null() {
        &mut empty
    } else {
        unsafe { std::slice::from_raw_parts_mut(data, data_len) }
    };
    // const result = paste.encode(slice, .{ .bracketed = bracketed });
    let result = paste::encode_mut(slice, paste::Options { bracketed });

    // const total = result[0].len + result[1].len + result[2].len;
    let total = result[0].len() + result[1].len() + result[2].len();
    // out_written.* = total;
    // SAFETY: `out_written` is a valid writable pointer (caller contract).
    unsafe { *out_written = total };

    // const out: []u8 = if (out_) |o| o[0..out_len] else &.{};
    // SAFETY: `out_`/`out_len` describe a valid writable region when non-null
    // (caller contract).
    let mut out_empty: [u8; 0] = [];
    let out: &mut [u8] = if out_.is_null() {
        &mut out_empty
    } else {
        unsafe { std::slice::from_raw_parts_mut(out_, out_len) }
    };
    // if (out.len < total) return .out_of_space;
    if out.len() < total {
        return Result::OutOfSpace;
    }

    // var offset: usize = 0;
    let mut offset: usize = 0;
    // for (result) |segment| {
    for segment in result {
        // @memcpy(out[offset..][0..segment.len], segment);
        out[offset..offset + segment.len()].copy_from_slice(segment);
        // offset += segment.len;
        offset += segment.len();
    }

    // return .success;
    Result::Success
}

#[cfg(test)]
mod tests {
    use super::*;

    // PORT NOTE: the reference allocates the input via `testing.allocator.dupe`
    // and frees it (proving the strip pass mutates a heap buffer, not a literal).
    // In Rust an owned `Vec<u8>`/array provides the same writable buffer; the
    // `dupe`/`free` pair has no separate observable behavior to assert.

    #[test]
    fn encode_bracketed() {
        // const input = try testing.allocator.dupe(u8, "hello");
        let mut input = b"hello".to_vec();
        // var buf: [64]u8 = undefined;
        let mut buf = [0u8; 64];
        // var written: usize = 0;
        let mut written: usize = 0;
        // const result = encode(input.ptr, input.len, true, &buf, buf.len, &written);
        let result = unsafe {
            encode(
                input.as_mut_ptr(),
                input.len(),
                true,
                buf.as_mut_ptr(),
                buf.len(),
                &mut written,
            )
        };
        // try testing.expectEqual(.success, result);
        assert_eq!(Result::Success, result);
        // try testing.expectEqualStrings("\x1b[200~hello\x1b[201~", buf[0..written]);
        assert_eq!(b"\x1b[200~hello\x1b[201~", &buf[0..written]);
    }

    #[test]
    fn encode_unbracketed_no_newlines() {
        let mut input = b"hello".to_vec();
        let mut buf = [0u8; 64];
        let mut written: usize = 0;
        // const result = encode(input.ptr, input.len, false, &buf, buf.len, &written);
        let result = unsafe {
            encode(
                input.as_mut_ptr(),
                input.len(),
                false,
                buf.as_mut_ptr(),
                buf.len(),
                &mut written,
            )
        };
        assert_eq!(Result::Success, result);
        // try testing.expectEqualStrings("hello", buf[0..written]);
        assert_eq!(b"hello", &buf[0..written]);
    }

    #[test]
    fn encode_unbracketed_newlines() {
        // const input = try testing.allocator.dupe(u8, "hello\nworld");
        let mut input = b"hello\nworld".to_vec();
        let mut buf = [0u8; 64];
        let mut written: usize = 0;
        let result = unsafe {
            encode(
                input.as_mut_ptr(),
                input.len(),
                false,
                buf.as_mut_ptr(),
                buf.len(),
                &mut written,
            )
        };
        assert_eq!(Result::Success, result);
        // try testing.expectEqualStrings("hello\rworld", buf[0..written]);
        assert_eq!(b"hello\rworld", &buf[0..written]);
    }

    #[test]
    fn encode_strip_unsafe_bytes() {
        // const input = try testing.allocator.dupe(u8, "hel\x1blo\x00world");
        let mut input = b"hel\x1blo\x00world".to_vec();
        let mut buf = [0u8; 64];
        let mut written: usize = 0;
        let result = unsafe {
            encode(
                input.as_mut_ptr(),
                input.len(),
                true,
                buf.as_mut_ptr(),
                buf.len(),
                &mut written,
            )
        };
        assert_eq!(Result::Success, result);
        // try testing.expectEqualStrings("\x1b[200~hel lo world\x1b[201~", buf[0..written]);
        assert_eq!(b"\x1b[200~hel lo world\x1b[201~", &buf[0..written]);
    }

    #[test]
    fn encode_with_insufficient_buffer() {
        let mut input = b"hello".to_vec();
        // var buf: [1]u8 = undefined;
        let mut buf = [0u8; 1];
        let mut written: usize = 0;
        let result = unsafe {
            encode(
                input.as_mut_ptr(),
                input.len(),
                true,
                buf.as_mut_ptr(),
                buf.len(),
                &mut written,
            )
        };
        // try testing.expectEqual(.out_of_space, result);
        assert_eq!(Result::OutOfSpace, result);
        // try testing.expectEqual(17, written);
        assert_eq!(17, written);
    }

    #[test]
    fn encode_with_null_buffer() {
        let mut input = b"hello".to_vec();
        let mut written: usize = 0;
        // const result = encode(input.ptr, input.len, true, null, 0, &written);
        let result = unsafe {
            encode(
                input.as_mut_ptr(),
                input.len(),
                true,
                std::ptr::null_mut(),
                0,
                &mut written,
            )
        };
        assert_eq!(Result::OutOfSpace, result);
        assert_eq!(17, written);
    }

    #[test]
    fn is_safe_with_safe_data() {
        // const safe = "hello world";
        let safe = b"hello world";
        // try testing.expect(is_safe(safe.ptr, safe.len));
        assert!(unsafe { is_safe(safe.as_ptr(), safe.len()) });
    }

    #[test]
    fn is_safe_with_newline() {
        // const unsafe = "hello\nworld";
        let unsafe_ = b"hello\nworld";
        // try testing.expect(!is_safe(unsafe.ptr, unsafe.len));
        assert!(!unsafe { is_safe(unsafe_.as_ptr(), unsafe_.len()) });
    }

    #[test]
    fn is_safe_with_bracketed_paste_end() {
        // const unsafe = "hello\x1b[201~world";
        let unsafe_ = b"hello\x1b[201~world";
        assert!(!unsafe { is_safe(unsafe_.as_ptr(), unsafe_.len()) });
    }

    #[test]
    fn is_safe_with_empty_data() {
        // const empty = "";
        let empty = b"";
        // try testing.expect(is_safe(empty.ptr, 0));
        assert!(unsafe { is_safe(empty.as_ptr(), 0) });
    }

    #[test]
    fn is_safe_with_null_empty_data() {
        // try testing.expect(is_safe(null, 0));
        assert!(unsafe { is_safe(std::ptr::null(), 0) });
    }

    use crate::terminal as terminal_c;
    use crate::terminal::{ClipboardContent, ClipboardRead, ClipboardReadReply};
    use crate::test_util::with_test_allocator;
    use rustty_lib::allocator::Allocator as CAllocator;

    // PORT NOTE (Rust-only glue): the reference's `TerminalPasteCapture`
    // container-level `var`s are shared by all four terminal_paste tests,
    // which Zig runs serially. Rust runs tests in parallel in one process,
    // so every test that touches the shared capture holds this lock for its
    // whole body.
    static PASTE_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Capture state for the terminal_paste tests: every pty write and the
    /// clipboard reads that follow a paste event.
    // const TerminalPasteCapture = struct { ... };
    mod capture {
        use super::*;

        // var written: [1024]u8 = undefined;
        // var written_len: usize = 0;
        // PORT NOTE: the fixed buffer + len become a `Vec` (the crate's
        // test-capture convention).
        pub static mut WRITTEN: Vec<u8> = Vec::new();
        pub static mut WRITE_COUNT: usize = 0;
        pub static mut READ_COUNT: usize = 0;
        pub static mut LAST_READ_GRANTED: bool = false;

        // fn reset() void { ... }
        pub fn reset() {
            unsafe {
                WRITTEN = Vec::new();
                WRITE_COUNT = 0;
                READ_COUNT = 0;
                LAST_READ_GRANTED = false;
            }
        }

        // fn writePty(_: Terminal, _: ?*anyopaque, ptr: [*]const u8, len: usize) ... void
        pub extern "C" fn write_pty(_: Terminal, _: *mut c_void, ptr: *const u8, len: usize) {
            unsafe {
                (*std::ptr::addr_of_mut!(WRITTEN))
                    .extend_from_slice(std::slice::from_raw_parts(ptr, len));
                WRITE_COUNT += 1;
            }
        }

        // fn clipboardRead(_: Terminal, _: ?*anyopaque, request: *const ClipboardRead) ... void
        pub extern "C" fn clipboard_read(
            _: Terminal,
            _: *mut c_void,
            request: *const ClipboardRead,
        ) {
            unsafe {
                READ_COUNT += 1;
                LAST_READ_GRANTED = (*request).granted;
                let contents = [ClipboardContent {
                    mime: LibString::init(b"text/plain"),
                    data: LibString::init(b"Ghostty"),
                }];
                ((*request).reply)(
                    request,
                    &ClipboardReadReply {
                        size: std::mem::size_of::<ClipboardReadReply>(),
                        result: clipboard::ReadStatus::Success,
                        contents: contents.as_ptr(),
                        contents_len: contents.len(),
                        available: std::ptr::null(),
                        available_len: 0,
                        remember: false,
                    },
                );
            }
        }

        // fn writtenSlice() []const u8 { return written[0..written_len]; }
        pub fn written_slice() -> &'static [u8] {
            unsafe { (*std::ptr::addr_of!(WRITTEN)).as_slice() }
        }

        /// The representations a test paste serves: the MIME list for the
        /// request and the data the read callback streams, in pieces.
        // const Contents = struct { mimes, data, len, reads, fail, ... };
        pub struct Contents {
            pub mimes: [LibString; 2],
            pub data: [&'static [u8]; 2],
            pub len: usize,
            pub reads: [usize; 2],
            pub fail: bool,
        }

        impl Contents {
            // fn init(entries: []const struct { []const u8, []const u8 }) Contents { ... }
            pub fn init(entries: &[(&'static [u8], &'static [u8])]) -> Contents {
                let mut this = Contents {
                    mimes: [LibString::init(b""); 2],
                    data: [b"".as_slice(); 2],
                    len: 0,
                    reads: [0; 2],
                    fail: false,
                };
                for entry in entries {
                    this.mimes[this.len] = LibString::init(entry.0);
                    this.data[this.len] = entry.1;
                    this.len += 1;
                }
                this
            }

            // fn request(self: *Contents) Request { ... }
            pub fn request(&mut self) -> Request {
                Request {
                    size: std::mem::size_of::<Request>(),
                    // .location = .standard, .source = .clipboard
                    location: 0,
                    source: 0,
                    mimes: self.mimes.as_ptr(),
                    mimes_len: self.len,
                    reader: io::MimeReader {
                        read: Some(Self::read),
                        userdata: self as *mut Contents as *mut c_void,
                    },
                    allow_unsafe: false,
                }
            }

            // fn read(userdata: ?*anyopaque, mime: lib.String, writer: io.Writer) ... bool { ... }
            pub extern "C" fn read(
                userdata: *mut c_void,
                mime: LibString,
                writer: io::Writer,
            ) -> bool {
                let this = userdata as *mut Contents;
                unsafe {
                    // The mime is the exact string from the request's list, so
                    // identifying it by pointer works.
                    let mut index: Option<usize> = None;
                    let len = (*this).len;
                    for i in 0..len {
                        let m = (*this).mimes[i];
                        if std::ptr::eq(m.ptr, mime.ptr) && m.len == mime.len {
                            index = Some(i);
                            break;
                        }
                    }
                    let Some(index) = index else { return false };
                    (*this).reads[index] += 1;
                    if (*this).fail {
                        return false;
                    }
                    let data = (*this).data[index];
                    let mut offset: usize = 0;
                    while offset < data.len() {
                        // const n = @min(3, data.len - offset);
                        let n = std::cmp::min(3, data.len() - offset);
                        if !(writer.write.unwrap())(writer.userdata, data[offset..].as_ptr(), n) {
                            return false;
                        }
                        offset += n;
                    }
                    true
                }
            }
        }
    }

    // PORT NOTE (Rust-only glue): `std.mem.count(u8, haystack, needle)` — a
    // non-overlapping occurrence count.
    fn mem_count(haystack: &[u8], needle: &[u8]) -> usize {
        let mut count = 0;
        let mut i = 0;
        while i + needle.len() <= haystack.len() {
            if &haystack[i..i + needle.len()] == needle {
                count += 1;
                i += needle.len();
            } else {
                i += 1;
            }
        }
        count
    }

    // PORT NOTE (Rust-only glue): `std.mem.indexOf`/`indexOfPos` over bytes.
    fn mem_index_of_pos(haystack: &[u8], start: usize, needle: &[u8]) -> Option<usize> {
        let mut i = start;
        while i + needle.len() <= haystack.len() {
            if &haystack[i..i + needle.len()] == needle {
                return Some(i);
            }
            i += 1;
        }
        None
    }

    fn vtw(t: Terminal, bytes: &[u8]) {
        unsafe { terminal_c::vt_write(t, bytes.as_ptr(), bytes.len()) };
    }

    // PORT NOTE: no reference test. The reference's mimes array comes from
    // `alloc.alloc([]const u8, c_mimes.len) catch return .out_of_memory`, an
    // observable `RUSTTY_OUT_OF_MEMORY` for a C caller (parity S174-F5; since
    // parity tail S7-4 the list is on `StackFallback(256, gpa)`, as in the
    // reference).
    // `mimes_len` is caller-supplied, so the failure does not need real memory
    // pressure — a length no allocation could ever describe is enough, and that
    // is what this asserts. The pointer is never dereferenced on this path (the
    // slice is formed only after the reservation succeeds), so the absurd
    // length is not a caller-contract violation.
    #[test]
    fn terminal_paste_rejects_an_undescribable_mimes_length() {
        let _guard = PASTE_TEST_LOCK.lock().unwrap();
        use capture as s;
        with_test_allocator(|alloc| {
            let mut t: Terminal = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                terminal_c::new(alloc as *const CAllocator, &mut t, 80, 24)
            });
            assert_eq!(Result::Success, unsafe {
                terminal_c::set(
                    t,
                    terminal_c::Option_::WritePty,
                    s::write_pty as *const c_void,
                )
            });

            let mut contents = s::Contents::init(&[(b"text/plain", b"hello")]);
            let mut written: bool = true;

            // The same request with a sane length succeeds, so the failure
            // below is about the length and not about the fixture.
            let ok_req = contents.request();
            assert_eq!(Result::Success, unsafe {
                terminal_paste(t, &ok_req, &mut written)
            });

            let mut req = contents.request();
            req.mimes_len = usize::MAX / std::mem::size_of::<&[u8]>();
            written = true;
            assert_eq!(Result::OutOfMemory, unsafe {
                terminal_paste(t, &req, &mut written)
            });
            // Nothing was written to the pty on the failure path.
            assert!(written);

            unsafe { terminal_c::free(t) };
        });
    }

    // Parity tail S7-4 (routing evidence, no reference counterpart): the MIME
    // list is on `stackFallback(256, wrapper.terminal.gpa())`. Sixteen
    // entries (256 bytes) fit the stack and reach no allocator; seventeen
    // spill exactly one 272-byte list to the terminal's allocator, which
    // takes it back before `terminal_paste` returns.
    #[test]
    fn terminal_paste_mimes_route_through_the_stack_fallback() {
        let _guard = PASTE_TEST_LOCK.lock().unwrap();
        use capture as s;
        s::reset();
        use rustty_vt::alloc::testing::CountingAllocator;
        let counting = CountingAllocator::over_heap();
        let calloc = counting.allocator();
        let mut t: Terminal = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe {
            terminal_c::new(&calloc as *const CAllocator, &mut t, 80, 24)
        });
        assert_eq!(Result::Success, unsafe {
            terminal_c::set(
                t,
                terminal_c::Option_::WritePty,
                s::write_pty as *const c_void,
            )
        });
        // Image-only lists: nothing is read or written, so the list is the
        // only allocation `terminal_paste` could make.
        let mimes = [LibString::init(b"image/png"); 17];
        extern "C" fn never_read(_: *mut c_void, _: LibString, _: io::Writer) -> bool {
            unreachable!("no text representation is read")
        }
        let paste = |len: usize| {
            let req = Request {
                size: std::mem::size_of::<Request>(),
                location: 0,
                source: 0,
                mimes: mimes.as_ptr(),
                mimes_len: len,
                reader: io::MimeReader {
                    read: Some(never_read),
                    userdata: std::ptr::null_mut(),
                },
                allow_unsafe: false,
            };
            let mut written = true;
            let before = counting.alloc_calls.get();
            let live = counting.live_bytes.get();
            assert_eq!(Result::Success, unsafe {
                terminal_paste(t, &req, &mut written)
            });
            assert!(!written);
            assert_eq!(live, counting.live_bytes.get(), "the list is freed");
            counting.alloc_calls.get() - before
        };
        assert_eq!(16 * std::mem::size_of::<&[u8]>(), 256);
        assert_eq!(0, paste(16), "256 bytes stay on the stack");
        assert_eq!(1, paste(17), "272 bytes spill to the terminal's allocator");
        assert_eq!(
            Some((17 * std::mem::size_of::<&[u8]>(), 3)),
            counting.last_alloc.get()
        );
        unsafe { terminal_c::free(t) };
        assert_eq!(0, counting.live_bytes.get());
    }

    // test "terminal_paste null handling"
    #[test]
    fn terminal_paste_null_handling() {
        let _guard = PASTE_TEST_LOCK.lock().unwrap();
        use capture as s;
        with_test_allocator(|alloc| {
            let mut t: Terminal = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                terminal_c::new(alloc as *const CAllocator, &mut t, 80, 24)
            });
            assert_eq!(Result::Success, unsafe {
                terminal_c::set(
                    t,
                    terminal_c::Option_::WritePty,
                    s::write_pty as *const c_void,
                )
            });

            // var written: bool = true;
            let mut written: bool = true;
            let mut contents = s::Contents::init(&[(b"text/plain", b"hello")]);
            let req = contents.request();
            assert_eq!(Result::InvalidValue, unsafe {
                terminal_paste(std::ptr::null_mut(), &req, &mut written)
            });
            assert_eq!(Result::InvalidValue, unsafe {
                terminal_paste(t, std::ptr::null(), &mut written)
            });
            assert!(written);

            // A size smaller than the struct is rejected.
            let mut small = req;
            small.size = std::mem::size_of::<usize>();
            assert_eq!(Result::InvalidValue, unsafe {
                terminal_paste(t, &small, &mut written)
            });

            // MIME types without a reader are rejected; none at all is fine.
            let mut unreadable = req;
            unreadable.reader.read = None;
            assert_eq!(Result::InvalidValue, unsafe {
                terminal_paste(t, &unreadable, &mut written)
            });
            unreadable.mimes = std::ptr::null();
            unreadable.mimes_len = 0;
            assert_eq!(Result::Success, unsafe {
                terminal_paste(t, &unreadable, &mut written)
            });
            assert!(!written);

            unsafe { terminal_c::free(t) };
        });
    }

    // test "terminal_paste without write_pty is invalid"
    #[test]
    fn terminal_paste_without_write_pty_is_invalid() {
        let _guard = PASTE_TEST_LOCK.lock().unwrap();
        use capture as s;
        s::reset();
        with_test_allocator(|alloc| {
            let mut t: Terminal = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                terminal_c::new(alloc as *const CAllocator, &mut t, 80, 24)
            });

            let mut contents = s::Contents::init(&[(b"text/plain", b"hello")]);
            let req = contents.request();
            assert_eq!(Result::InvalidValue, unsafe {
                terminal_paste(t, &req, std::ptr::null_mut())
            });

            unsafe { terminal_c::free(t) };
        });
    }

    // test "terminal_paste text and unsafe"
    #[test]
    fn terminal_paste_text_and_unsafe() {
        let _guard = PASTE_TEST_LOCK.lock().unwrap();
        use capture as s;
        s::reset();
        with_test_allocator(|alloc| {
            let mut t: Terminal = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                terminal_c::new(alloc as *const CAllocator, &mut t, 80, 24)
            });
            assert_eq!(Result::Success, unsafe {
                terminal_c::set(
                    t,
                    terminal_c::Option_::WritePty,
                    s::write_pty as *const c_void,
                )
            });

            // Plain text, NULL out_written pointer is fine. The text is read
            // once, the image never.
            let mut contents =
                s::Contents::init(&[(b"image/png", b"\x89PNG"), (b"text/plain", b"hel\x1blo")]);
            let req = contents.request();
            assert_eq!(Result::Success, unsafe {
                terminal_paste(t, &req, std::ptr::null_mut())
            });
            assert_eq!(b"hel lo".as_slice(), s::written_slice());
            assert_eq!(1, unsafe { s::WRITE_COUNT });
            assert_eq!(0, contents.reads[0]);
            assert_eq!(1, contents.reads[1]);

            // Unsafe is refused with nothing written, then allowed.
            s::reset();
            let mut written: bool = false;
            let mut unsafe_contents = s::Contents::init(&[(b"text/plain", b"rm -rf /\n")]);
            let mut unsafe_req = unsafe_contents.request();
            assert_eq!(Result::Rejected, unsafe {
                terminal_paste(t, &unsafe_req, &mut written)
            });
            assert_eq!(0, unsafe { s::WRITE_COUNT });
            assert!(!written);
            assert_eq!(1, unsafe_contents.reads[0]);

            unsafe_req.allow_unsafe = true;
            assert_eq!(Result::Success, unsafe {
                terminal_paste(t, &unsafe_req, &mut written)
            });
            assert!(written);
            assert_eq!(b"rm -rf /\r".as_slice(), s::written_slice());
            assert_eq!(2, unsafe_contents.reads[0]);

            // Bracketed paste mode frames the text through the real mode path.
            s::reset();
            vtw(t, b"\x1B[?2004h");
            assert_eq!(Result::Success, unsafe {
                terminal_paste(t, &req, &mut written)
            });
            assert!(written);
            assert_eq!(b"\x1B[200~hel lo\x1B[201~".as_slice(), s::written_slice());

            // No text representation writes nothing and reads nothing.
            s::reset();
            let mut image = s::Contents::init(&[(b"image/png", b"\x89PNG")]);
            let image_req = image.request();
            assert_eq!(Result::Success, unsafe {
                terminal_paste(t, &image_req, &mut written)
            });
            assert!(!written);
            assert_eq!(0, unsafe { s::WRITE_COUNT });
            assert_eq!(0, image.reads[0]);

            // A failing reader is an I/O error.
            s::reset();
            contents.fail = true;
            // SB (test-harness): the direct `contents.fail` write above retags
            // the owner and invalidates the `userdata` raw pointer stored in
            // the earlier `req` (`rust-mut-args-strongly-protected`; Zig's
            // pointers are unprotected so the reference reuses `req`). Re-mint
            // the request so its ctx pointer is derived AFTER the write.
            let req = contents.request();
            assert_eq!(Result::IoError, unsafe {
                terminal_paste(t, &req, &mut written)
            });
            assert_eq!(0, unsafe { s::WRITE_COUNT });

            unsafe { terminal_c::free(t) };
        });
    }

    // Parity S174-F4 / S150-F2 (no reference test): `clipboard.Location` is a
    // non-exhaustive `enum(c_int)`, so the reference forwards an unnamed
    // location to the paste handler, whose event reports any non-standard
    // location as primary. It is not `.invalid_value`.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn terminal_paste_forwards_an_unnamed_location() {
        let _guard = PASTE_TEST_LOCK.lock().unwrap();
        use capture as s;
        s::reset();
        with_test_allocator(|alloc| {
            let mut t: Terminal = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                terminal_c::new(alloc as *const CAllocator, &mut t, 80, 24)
            });
            assert_eq!(Result::Success, unsafe {
                terminal_c::set(
                    t,
                    terminal_c::Option_::WritePty,
                    s::write_pty as *const c_void,
                )
            });
            unsafe {
                (*(*t).terminal)
                    .modes
                    .set(rustty_vt::modes::Mode::KittyPasteEvents, true)
            };
            assert_eq!(Result::Success, unsafe {
                terminal_c::set(
                    t,
                    terminal_c::Option_::ClipboardRead,
                    s::clipboard_read as *const c_void,
                )
            });

            let mut written: bool = false;
            let mut contents = s::Contents::init(&[(b"text/plain", b"secret")]);
            let mut req = contents.request();
            req.location = 7;
            assert_eq!(Result::Success, unsafe {
                terminal_paste(t, &req, &mut written)
            });
            assert!(written);
            assert!(
                s::written_slice().starts_with(b"\x1b]5522;type=read:status=OK:loc=primary:pw=")
            );

            unsafe { terminal_c::free(t) };
        });
    }

    // test "terminal_paste event"
    #[test]
    // Real-OS CSPRNG: the paste event mints a one-time password through the
    // platform entropy source (`arc4random_buf`), which Miri cannot emulate —
    // the `rustty-os` `random_secure` precedent. Covered natively by every
    // `cargo test` lane.
    #[cfg_attr(miri, ignore)]
    fn terminal_paste_event() {
        let _guard = PASTE_TEST_LOCK.lock().unwrap();
        use capture as s;
        s::reset();
        with_test_allocator(|alloc| {
            let mut t: Terminal = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                terminal_c::new(alloc as *const CAllocator, &mut t, 80, 24)
            });
            assert_eq!(Result::Success, unsafe {
                terminal_c::set(
                    t,
                    terminal_c::Option_::WritePty,
                    s::write_pty as *const c_void,
                )
            });
            // t.?.terminal.modes.set(.kitty_paste_events, true);
            unsafe {
                (*(*t).terminal)
                    .modes
                    .set(rustty_vt::modes::Mode::KittyPasteEvents, true)
            };

            // Without a clipboard_read callback the paste stays text.
            let mut written: bool = false;
            let mut contents =
                s::Contents::init(&[(b"text/plain", b"secret"), (b"image/png", b"\x89PNG")]);
            let mut req = contents.request();
            // req.location = .primary;
            req.location = 2;
            assert_eq!(Result::Success, unsafe {
                terminal_paste(t, &req, &mut written)
            });
            assert!(written);
            assert_eq!(b"secret".as_slice(), s::written_slice());

            // With one, an event is sent listing every MIME type and no data
            // is read, let alone written.
            s::reset();
            contents.reads = [0; 2];
            // SB (test-harness): the `contents.reads` write invalidates the
            // ctx pointer inside the earlier `req` — re-mint it (see the
            // text_and_unsafe test's note).
            let mut req = contents.request();
            // req.location = .primary;
            req.location = 2;
            assert_eq!(Result::Success, unsafe {
                terminal_c::set(
                    t,
                    terminal_c::Option_::ClipboardRead,
                    s::clipboard_read as *const c_void,
                )
            });
            assert_eq!(Result::Success, unsafe {
                terminal_paste(t, &req, &mut written)
            });
            assert!(written);
            assert_eq!(1, unsafe { s::WRITE_COUNT });
            assert_eq!(3, mem_count(s::written_slice(), b"\x1b]5522;"));
            assert!(
                s::written_slice().starts_with(b"\x1b]5522;type=read:status=OK:loc=primary:pw=")
            );
            assert!(mem_index_of_pos(s::written_slice(), 0, b"secret").is_none());
            assert!(mem_index_of_pos(
                s::written_slice(),
                0,
                b";dGV4dC9wbGFpbiBpbWFnZS9wbmcK\x1b\\"
            )
            .is_some());
            assert_eq!(0, contents.reads[0]);
            assert_eq!(0, contents.reads[1]);

            // The program's read with the event password is granted once.
            let ok_prefix: &[u8] = b"\x1b]5522;type=read:status=OK:loc=primary:pw=";
            let pw_end = mem_index_of_pos(s::written_slice(), ok_prefix.len(), b"\x1b\\").unwrap();
            // const read = try std.fmt.bufPrint(&read_buf, "\x1b]5522;type=read:pw={s}:name=UGFzdGUgZXZlbnQ=;dGV4dC9wbGFpbg==\x1b\\", .{ ... });
            let pw = s::written_slice()[ok_prefix.len()..pw_end].to_vec();
            let mut read: Vec<u8> = Vec::new();
            read.extend_from_slice(b"\x1b]5522;type=read:pw=");
            read.extend_from_slice(&pw);
            read.extend_from_slice(b":name=UGFzdGUgZXZlbnQ=;dGV4dC9wbGFpbg==\x1b\\");
            s::reset();
            vtw(t, &read);
            assert_eq!(1, unsafe { s::READ_COUNT });
            assert!(unsafe { s::LAST_READ_GRANTED });
            assert!(mem_index_of_pos(s::written_slice(), 0, b";R2hvc3R0eQ==\x1b\\").is_some());

            s::reset();
            vtw(t, &read);
            assert_eq!(1, unsafe { s::READ_COUNT });
            assert!(!unsafe { s::LAST_READ_GRANTED });

            // Text sources never become events.
            s::reset();
            let mut ime = req;
            // ime.source = .text;
            ime.source = 1;
            assert_eq!(Result::Success, unsafe {
                terminal_paste(t, &ime, &mut written)
            });
            assert!(written);
            assert_eq!(b"secret".as_slice(), s::written_slice());

            unsafe { terminal_c::free(t) };
        });
    }
}
