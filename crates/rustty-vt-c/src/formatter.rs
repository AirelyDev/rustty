//! rustty-vt-c formatter
//!
//! PORTS: ghostty src/terminal/c/formatter.zig (the screen/terminal formatter
//! C ABI layer).
//!
//! The rustty-vt `formatter::TerminalFormatter`/`Format`/`Options`/extras are
//! the source of truth (R3); this module is the FFI projection. The
//! `rustty_formatter_*` `extern "C"` wrappers live in `lib.rs` and forward here.

// const std = @import("std");
// const testing = std.testing;
// const lib = @import("../lib.zig");
// const CAllocator = lib.alloc.Allocator;
// const terminal_c = @import("terminal.zig");
// const grid_ref = @import("grid_ref.zig");
// const selection_c = @import("selection.zig");
// const ZigTerminal = @import("../Terminal.zig");
// const formatterpkg = @import("../formatter.zig");
// const Result = @import("result.zig").Result;
use crate::result::Result;
use crate::selection::{CSelection, Format};
use crate::terminal as terminal_c;
use crate::writer::{CountingWriter, FixedWriter};
use rustty_lib::allocator::{Alignment, Allocator as CAllocator};
use rustty_vt::formatter as formatterpkg;
use rustty_vt::terminal::Terminal as ZigTerminal;
use std::mem::size_of;

use crate::cwrap::{create, destroy};

// PORT NOTE (rebuild-on-format): the reference `FormatterWrapper` stores a live
// `formatterpkg.TerminalFormatter` and re-invokes `t.format(&writer)` for each
// `format_buf`/`format_alloc` call (the formatter borrows). rustty's
// `TerminalFormatter::format(self)` CONSUMES the formatter, so the wrapper
// instead stores the construction inputs (terminal pointer + the resolved
// `emit/unwrap/trim`, `extra`, and `content`) and rebuilds a fresh
// `TerminalFormatter` per format call. This is behavior-equivalent: the
// reference formatter is stateless across `format` calls (it re-reads the live
// terminal each time, hence "format reflects terminal changes").
/// Wrapper around formatter that tracks the allocator for C API usage.
pub struct FormatterWrapper {
    kind: Kind,
    alloc: Option<CAllocator>,
}

// const Kind = union(enum) { terminal: formatterpkg.TerminalFormatter };
enum Kind {
    Terminal(TerminalInputs),
}

// PORT NOTE: the rebuild inputs for a `TerminalFormatter` (see wrapper PORT
// NOTE). `terminal` is the `*const ZigTerminal`; the rest mirror the fields the
// reference sets in `terminal_new_`.
struct TerminalInputs {
    terminal: *const ZigTerminal,
    emit: formatterpkg::Format,
    unwrap: bool,
    trim: bool,
    extra: formatterpkg::TerminalExtra,
    content: formatterpkg::Content,
}

impl TerminalInputs {
    // PORT NOTE: rebuilds `formatterpkg.TerminalFormatter.init(t, .{ .emit, .unwrap,
    // .trim }); formatter.extra = ...; formatter.content = ...;` per format call.
    fn build(&self) -> formatterpkg::TerminalFormatter<'_> {
        // PORT NOTE: the reference builds `.{ .emit, .unwrap, .trim }` (the rest
        // of `Options` rides its field defaults). rustty exposes the same
        // defaults via the per-format constructors `plain/vt/html`; we pick by
        // `emit`, then override `unwrap`/`trim`.
        let mut opts = match self.emit {
            formatterpkg::Format::Plain => formatterpkg::Options::plain(),
            formatterpkg::Format::Vt => formatterpkg::Options::vt(),
            formatterpkg::Format::Html => formatterpkg::Options::html(),
        };
        opts.unwrap = self.unwrap;
        opts.trim = self.trim;
        let mut formatter = formatterpkg::TerminalFormatter::init(self.terminal, opts);
        formatter.extra = self.extra;
        formatter.content = copy_content(&self.content);
        formatter
    }
}

// PORT NOTE: `formatterpkg.Content` is not `Clone`; the two variants the C API
// produces (`None`, `Selection(Option<Selection>)`) are copied by hand (the
// inner `Selection` is `Copy`).
fn copy_content(c: &formatterpkg::Content) -> formatterpkg::Content {
    match c {
        formatterpkg::Content::None => formatterpkg::Content::None,
        formatterpkg::Content::Selection(sel) => formatterpkg::Content::Selection(*sel),
    }
}

/// C: RusttyFormatter (R2)
// pub const Formatter = ?*FormatterWrapper;
pub type Formatter = *mut FormatterWrapper;

// PORT NOTE: `pub const Format = formatterpkg.Format;` — the C ABI projection
// lives in `selection.rs` (`selection::Format`, the `enum(c_int)` of
// `formatter.Format`); re-exported here under the reference's name.
pub use crate::selection::Format as FormatterFormat;

// const CSelection = selection_c.CSelection;

/// C: RusttyFormatterScreenOptions (R2)
// pub const ScreenOptions = extern struct {
//     pub const Extra = extern struct { size, cursor, style, hyperlink, protection,
//         kitty_keyboard, charsets };
// };
//
// PORT NOTE: `ScreenOptions` is an empty namespace struct in the reference; only
// its nested `Extra` is a real C struct. Ported as a standalone
// `ScreenOptionsExtra`.
/// C: RusttyFormatterScreenExtra (R2)
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ScreenOptionsExtra {
    pub size: usize,
    pub cursor: bool,
    pub style: bool,
    pub hyperlink: bool,
    pub protection: bool,
    pub kitty_keyboard: bool,
    pub charsets: bool,
}

impl Default for ScreenOptionsExtra {
    // size: usize = @sizeOf(Extra),
    fn default() -> Self {
        ScreenOptionsExtra {
            size: size_of::<ScreenOptionsExtra>(),
            cursor: false,
            style: false,
            hyperlink: false,
            protection: false,
            kitty_keyboard: false,
            charsets: false,
        }
    }
}

impl ScreenOptionsExtra {
    // comptime {
    //     for (std.meta.fieldNames(formatterpkg.ScreenFormatter.Extra)) |name| {
    //         if (!@hasField(Extra, name))
    //             @compileError("ScreenOptions.Extra missing field: " ++ name);
    //     }
    // }
    // PORT NOTE: the comptime field-presence guard is enforced by `to_zig` below
    // (it names every `ScreenExtra` field; a missing one is a compile error).

    // fn toZig(self: Extra) formatterpkg.ScreenFormatter.Extra {
    fn to_zig(self) -> formatterpkg::ScreenExtra {
        formatterpkg::ScreenExtra {
            cursor: self.cursor,
            style: self.style,
            hyperlink: self.hyperlink,
            protection: self.protection,
            kitty_keyboard: self.kitty_keyboard,
            charsets: self.charsets,
        }
    }
}

/// C: RusttyFormatterTerminalOptions (R2)
// pub const TerminalOptions = extern struct {
//     size: usize = @sizeOf(TerminalOptions),
//     emit: Format,
//     unwrap: bool,
//     trim: bool,
//     extra: Extra,
//     selection: ?*const CSelection = null,
//     pub const Extra = extern struct { ... };
// };
#[repr(C)]
#[derive(Clone, Copy)]
pub struct TerminalOptions {
    pub size: usize,
    pub emit: Format,
    pub unwrap: bool,
    pub trim: bool,
    pub extra: TerminalOptionsExtra,

    /// Optional selection to restrict output to a range.
    /// If null, the entire screen is formatted.
    pub selection: *const CSelection,
}

/// C: RusttyFormatterTerminalExtra (R2)
// pub const Extra = extern struct {
//     size: usize = @sizeOf(Extra),
//     palette, modes, scrolling_region, tabstops, pwd, keyboard,
//     screen: ScreenOptions.Extra,
// };
#[repr(C)]
#[derive(Clone, Copy)]
pub struct TerminalOptionsExtra {
    pub size: usize,
    pub palette: bool,
    pub modes: bool,
    pub scrolling_region: bool,
    pub tabstops: bool,
    pub pwd: bool,
    pub keyboard: bool,
    pub screen: ScreenOptionsExtra,
}

impl Default for TerminalOptionsExtra {
    // size: usize = @sizeOf(Extra),
    fn default() -> Self {
        TerminalOptionsExtra {
            size: size_of::<TerminalOptionsExtra>(),
            palette: false,
            modes: false,
            scrolling_region: false,
            tabstops: false,
            pwd: false,
            keyboard: false,
            screen: ScreenOptionsExtra::default(),
        }
    }
}

impl TerminalOptionsExtra {
    // comptime {
    //     for (std.meta.fieldNames(formatterpkg.TerminalFormatter.Extra)) |name| {
    //         if (!@hasField(Extra, name))
    //             @compileError("TerminalOptions.Extra missing field: " ++ name);
    //     }
    // }
    // PORT NOTE: the comptime field-presence guard is enforced by `to_zig` below.

    // fn toZig(self: Extra) formatterpkg.TerminalFormatter.Extra {
    fn to_zig(self) -> formatterpkg::TerminalExtra {
        formatterpkg::TerminalExtra {
            palette: self.palette,
            modes: self.modes,
            scrolling_region: self.scrolling_region,
            tabstops: self.tabstops,
            pwd: self.pwd,
            keyboard: self.keyboard,
            screen: self.screen.to_zig(),
        }
    }
}

// PORT NOTE: the reference relies on `extern struct` field defaults
// (`.size = @sizeOf(TerminalOptions)`, `.selection = null`). Tests construct
// `TerminalOptions` literals omitting `size`/`selection`; Rust has no
// struct-literal defaults, so callers/tests build via `Default` + field
// overrides. `Default` carries the same field defaults.
impl Default for TerminalOptions {
    fn default() -> Self {
        TerminalOptions {
            size: size_of::<TerminalOptions>(),
            emit: Format::Plain,
            unwrap: false,
            trim: true,
            extra: TerminalOptionsExtra::default(),
            selection: std::ptr::null(),
        }
    }
}

// PORT NOTE: the fallible inner constructor's error set.
// error{ InvalidValue, OutOfMemory }
enum NewError {
    InvalidValue,
    OutOfMemory,
}

// pub fn terminal_new(alloc_, result, terminal_, opts) ... Result {
//     result.* = terminal_new_(alloc_, terminal_, opts) catch |err| {
//         result.* = null;
//         return switch (err) {
//             error.InvalidValue => .invalid_value,
//             error.OutOfMemory => .out_of_memory,
//         };
//     };
//     return .success;
// }
//
/// # Safety
/// `alloc_`, if non-null, must point to a valid `CAllocator`; `result` must be
/// writable storage for a `Formatter`; `terminal_`, if non-null, a live handle.
pub unsafe fn terminal_new(
    alloc_: Option<&CAllocator>,
    result: *mut Formatter,
    terminal_: terminal_c::Terminal,
    opts: TerminalOptions,
) -> Result {
    // result.* = terminal_new_(alloc_, terminal_, opts) catch |err| { ... };
    match unsafe { terminal_new_(alloc_, terminal_, opts) } {
        Ok(ptr) => {
            // result.* = <ptr>;
            // SAFETY: `result` is writable per the caller contract.
            unsafe { *result = ptr };
            // return .success;
            Result::Success
        }
        Err(err) => {
            // result.* = null;
            // SAFETY: `result` is writable per the caller contract.
            unsafe { *result = std::ptr::null_mut() };
            // return switch (err) { ... };
            match err {
                NewError::InvalidValue => Result::InvalidValue,
                NewError::OutOfMemory => Result::OutOfMemory,
            }
        }
    }
}

// fn terminal_new_(alloc_, terminal_, opts) error{InvalidValue,OutOfMemory}!*FormatterWrapper {
//     const t: *ZigTerminal = (terminal_ orelse return error.InvalidValue).terminal;
//     const alloc = lib.alloc.default(alloc_);
//     const ptr = alloc.create(FormatterWrapper) catch return error.OutOfMemory;
//     errdefer alloc.destroy(ptr);
//     var formatter: formatterpkg.TerminalFormatter = .init(t, .{ .emit, .unwrap, .trim });
//     formatter.extra = opts.extra.toZig();
//     if (opts.selection) |sel| formatter.content = .{
//         .selection = sel.toZig() orelse return error.InvalidValue,
//     };
//     ptr.* = .{ .kind = .{ .terminal = formatter }, .alloc = alloc };
//     return ptr;
// }
//
/// # Safety
/// `alloc_`/`terminal_`/`opts.selection` validity is the caller's contract.
unsafe fn terminal_new_(
    alloc_: Option<&CAllocator>,
    terminal_: terminal_c::Terminal,
    opts: TerminalOptions,
) -> std::result::Result<Formatter, NewError> {
    // const t: *ZigTerminal = (terminal_ orelse return error.InvalidValue).terminal;
    if terminal_.is_null() {
        return Err(NewError::InvalidValue);
    }
    // SAFETY: `terminal_` is a live handle (caller contract; non-null checked).
    let t: *const ZigTerminal = unsafe { terminal_c::zig_terminal(terminal_) };

    // var formatter: formatterpkg.TerminalFormatter = .init(t, .{ .emit, .unwrap, .trim });
    // formatter.extra = opts.extra.toZig();
    let mut content = formatterpkg::Content::Selection(None);

    // Setup the content that we're formatting
    // if (opts.selection) |sel| formatter.content = .{ .selection = sel.toZig() orelse ... };
    if !opts.selection.is_null() {
        // SAFETY: `opts.selection`, if non-null, points to a valid `CSelection`
        // (caller contract).
        let sel = unsafe { *opts.selection };
        match sel.to_zig() {
            Some(selection) => content = formatterpkg::Content::Selection(Some(selection)),
            // orelse return error.InvalidValue,
            None => return Err(NewError::InvalidValue),
        }
    }

    // PORT NOTE: the rebuild inputs are stored (see wrapper PORT NOTE) rather than
    // a live formatter. The `emit/unwrap/trim/extra/content` are exactly what the
    // reference's `formatter` carries after `init` + the field assignments above.
    let inputs = TerminalInputs {
        terminal: t,
        emit: opts.emit.to_zig(),
        unwrap: opts.unwrap,
        trim: opts.trim,
        extra: opts.extra.to_zig(),
        content,
    };

    // const ptr = alloc.create(FormatterWrapper) catch return error.OutOfMemory;
    // ptr.* = .{ .kind = .{ .terminal = formatter }, .alloc = alloc };
    let ptr = unsafe {
        create(
            alloc_,
            FormatterWrapper {
                kind: Kind::Terminal(inputs),
                alloc: alloc_.copied(),
            },
        )
    };
    if ptr.is_null() {
        return Err(NewError::OutOfMemory);
    }
    // return ptr;
    Ok(ptr)
}

// pub fn format_buf(formatter_, out_, out_len, out_written) ... Result {
//     const wrapper = formatter_ orelse return .invalid_value;
//     var writer: std.Io.Writer = .fixed(if (out_) |out| out[0..out_len] else &.{});
//     switch (wrapper.kind) {
//         .terminal => |*t| t.format(&writer) catch |err| switch (err) {
//             error.WriteFailed => {
//                 var discarding: std.Io.Writer.Discarding = .init(&.{});
//                 t.format(&discarding.writer) catch unreachable;
//                 out_written.* = @intCast(discarding.count);
//                 return .out_of_space;
//             },
//         },
//     }
//     out_written.* = writer.end;
//     return .success;
// }
//
/// # Safety
/// `formatter_`, if non-null, must be a handle returned by `terminal_new`; `out_`
/// (if non-null) must point to `out_len` writable bytes; `out_written` must be
/// writable.
pub unsafe fn format_buf(
    formatter_: Formatter,
    out_: *mut u8,
    out_len: usize,
    out_written: *mut usize,
) -> Result {
    // const wrapper = formatter_ orelse return .invalid_value;
    if formatter_.is_null() {
        return Result::InvalidValue;
    }
    // SAFETY: `formatter_` is a live handle (caller contract).
    let wrapper = unsafe { &*formatter_ };

    // var writer: std.Io.Writer = .fixed(if (out_) |out| out[0..out_len] else &.{});
    let mut empty: [u8; 0] = [];
    let slice: &mut [u8] = if out_.is_null() {
        &mut empty
    } else {
        // SAFETY: `out_` points to `out_len` writable bytes (caller contract).
        unsafe { std::slice::from_raw_parts_mut(out_, out_len) }
    };
    let mut writer = FixedWriter::new(slice);

    // switch (wrapper.kind) { .terminal => |*t| t.format(&writer) catch ... };
    match &wrapper.kind {
        Kind::Terminal(inputs) => {
            if inputs.build().format(&mut writer).is_err() {
                // error.WriteFailed:
                // On write failed we always report how much space we actually needed.
                // var discarding: std.Io.Writer.Discarding = .init(&.{});
                let mut discarding = CountingWriter::new();
                // t.format(&discarding.writer) catch unreachable;
                inputs.build().format(&mut discarding).unwrap();
                // out_written.* = @intCast(discarding.count);
                // SAFETY: `out_written` is writable (caller contract).
                unsafe { *out_written = discarding.count };
                // return .out_of_space;
                return Result::OutOfSpace;
            }
        }
    }

    // out_written.* = writer.end;
    // SAFETY: `out_written` is writable (caller contract).
    unsafe { *out_written = writer.end };
    // return .success;
    Result::Success
}

// pub fn format_alloc(formatter_, alloc_, out_ptr, out_len) ... Result {
//     const wrapper = formatter_ orelse return .invalid_value;
//     const alloc = lib.alloc.default(alloc_);
//     var aw: std.Io.Writer.Allocating = .init(alloc);
//     defer aw.deinit();
//     switch (wrapper.kind) {
//         .terminal => |*t| t.format(&aw.writer) catch return .out_of_memory,
//     }
//     const buf = aw.toOwnedSlice() catch return .out_of_memory;
//     out_ptr.* = buf.ptr;
//     out_len.* = buf.len;
//     return .success;
// }
//
/// # Safety
/// `formatter_`, if non-null, must be a `terminal_new` handle; `alloc_`, if
/// non-null, a valid `CAllocator`; `out_ptr`/`out_len` must be writable.
pub unsafe fn format_alloc(
    formatter_: Formatter,
    alloc_: Option<&CAllocator>,
    out_ptr: *mut *mut u8,
    out_len: *mut usize,
) -> Result {
    // const wrapper = formatter_ orelse return .invalid_value;
    if formatter_.is_null() {
        return Result::InvalidValue;
    }
    // SAFETY: `formatter_` is a live handle (caller contract).
    let wrapper = unsafe { &*formatter_ };

    // const alloc = lib.alloc.default(alloc_);
    let allocator = rustty_lib::allocator::default(alloc_);

    // var aw: std.Io.Writer.Allocating = .init(alloc);
    // switch (wrapper.kind) { .terminal => |*t| t.format(&aw.writer) catch return .out_of_memory };
    // PORT NOTE: `std.Io.Writer.Allocating` grows a buffer; rustty formats into a
    // `Vec<u8>` (infallible growth), then `toOwnedSlice` is reproduced by copying
    // the bytes into a buffer allocated through the SAME resolved C allocator so
    // the C caller frees it with the matching allocator.
    let mut buf_vec: Vec<u8> = Vec::new();
    match &wrapper.kind {
        Kind::Terminal(inputs) => {
            if inputs.build().format(&mut buf_vec).is_err() {
                return Result::OutOfMemory;
            }
        }
    }

    // const buf = aw.toOwnedSlice() catch return .out_of_memory;
    // out_ptr.* = buf.ptr; out_len.* = buf.len;
    let len = buf_vec.len();
    let dst = allocator.alloc(len, Alignment::from_byte_units(1), 0);
    if dst.is_null() {
        return Result::OutOfMemory;
    }
    // SAFETY: `dst` is `len` freshly-allocated bytes; `buf_vec` holds `len`
    // initialized bytes.
    unsafe { std::ptr::copy_nonoverlapping(buf_vec.as_ptr(), dst, len) };
    // SAFETY: `out_ptr`/`out_len` are writable (caller contract).
    unsafe {
        *out_ptr = dst;
        *out_len = len;
    }
    // return .success;
    Result::Success
}

// pub fn free(formatter_: Formatter) ... void {
//     const wrapper = formatter_ orelse return;
//     const alloc = wrapper.alloc;
//     alloc.destroy(wrapper);
// }
//
/// # Safety
/// `formatter_`, if non-null, must be a handle returned by `terminal_new`, not
/// yet freed.
pub unsafe fn free(formatter_: Formatter) {
    // const wrapper = formatter_ orelse return;
    if formatter_.is_null() {
        return;
    }
    // const alloc = wrapper.alloc;
    // SAFETY: `formatter_` is a live handle (caller contract).
    let alloc = unsafe { (*formatter_).alloc };
    // alloc.destroy(wrapper);
    // SAFETY: `formatter_` was produced by `create` with the same `alloc_`.
    unsafe { destroy(alloc.as_ref(), formatter_) };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid_ref::CGridRef;
    use crate::point::{PointC, PointCValue, TagC};
    use crate::test_util::with_test_allocator;
    use rustty_vt::point::Coordinate;

    // PORT NOTE: the reference test literal
    // `.{ .emit = .plain, .unwrap = false, .trim = true, .extra = .{ ... } }`
    // omits `size`/`selection` (extern-struct field defaults). The helpers below
    // build the equivalent `TerminalOptions`/extras via `Default` + overrides.
    fn make_opts(
        emit: Format,
        unwrap: bool,
        trim: bool,
        extra: TerminalOptionsExtra,
        selection: *const CSelection,
    ) -> TerminalOptions {
        TerminalOptions {
            emit,
            unwrap,
            trim,
            extra,
            selection,
            ..TerminalOptions::default()
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn make_extra(
        palette: bool,
        modes: bool,
        scrolling_region: bool,
        tabstops: bool,
        pwd: bool,
        keyboard: bool,
        screen: ScreenOptionsExtra,
    ) -> TerminalOptionsExtra {
        TerminalOptionsExtra {
            palette,
            modes,
            scrolling_region,
            tabstops,
            pwd,
            keyboard,
            screen,
            ..TerminalOptionsExtra::default()
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn make_screen(
        cursor: bool,
        style: bool,
        hyperlink: bool,
        protection: bool,
        kitty_keyboard: bool,
        charsets: bool,
    ) -> ScreenOptionsExtra {
        ScreenOptionsExtra {
            cursor,
            style,
            hyperlink,
            protection,
            kitty_keyboard,
            charsets,
            ..ScreenOptionsExtra::default()
        }
    }

    // test "terminal_new/free" {
    #[test]
    fn terminal_new_free() {
        with_test_allocator(|calloc| {
            // var t: terminal_c.Terminal = null;
            let mut t: terminal_c::Terminal = std::ptr::null_mut();
            // try testing.expectEqual(Result.success, terminal_c.new(...));
            assert_eq!(Result::Success, unsafe {
                terminal_c::new(calloc, &mut t, 80, 24)
            });
            // defer terminal_c.free(t);

            // var f: Formatter = null;
            let mut f: Formatter = std::ptr::null_mut();
            let extra = make_extra(
                false,
                false,
                false,
                false,
                false,
                false,
                make_screen(false, false, false, false, false, false),
            );
            // try testing.expectEqual(Result.success, terminal_new(...));
            assert_eq!(Result::Success, unsafe {
                terminal_new(
                    Some(calloc),
                    &mut f,
                    t,
                    make_opts(Format::Plain, false, true, extra, std::ptr::null()),
                )
            });
            // try testing.expect(f != null);
            assert!(!f.is_null());
            // free(f);
            unsafe { free(f) };

            unsafe { terminal_c::free(t) };
        });
    }

    // test "terminal_new invalid_value on null terminal" {
    #[test]
    fn terminal_new_invalid_value_on_null_terminal() {
        with_test_allocator(|calloc| {
            // var f: Formatter = null;
            let mut f: Formatter = std::ptr::null_mut();
            let extra = make_extra(
                false,
                false,
                false,
                false,
                false,
                false,
                make_screen(false, false, false, false, false, false),
            );
            // try testing.expectEqual(Result.invalid_value, terminal_new(... null ...));
            assert_eq!(Result::InvalidValue, unsafe {
                terminal_new(
                    Some(calloc),
                    &mut f,
                    std::ptr::null_mut(),
                    make_opts(Format::Plain, false, true, extra, std::ptr::null()),
                )
            });
            // try testing.expect(f == null);
            assert!(f.is_null());
        });
    }

    // test "free null" {
    #[test]
    fn free_null() {
        // free(null);
        unsafe { free(std::ptr::null_mut()) };
    }

    // PORT NOTE: shared setup for the format tests — build a terminal, write
    // `bytes`, build a plain formatter, returning the handles WITH their
    // unasserted `new` Results. Mirrors the repeated `terminal_c.new` +
    // `vt_write` + `terminal_new` preamble; the two `Result.success`
    // expectEqual ASSERTIONS of that preamble live in each test body
    // (assertion-for-assertion; G2 counts source-level asserts per test).
    fn setup_plain(
        calloc: &CAllocator,
        bytes: &[u8],
        extra: TerminalOptionsExtra,
        emit: Format,
        selection: *const CSelection,
    ) -> (Result, terminal_c::Terminal, Result, Formatter) {
        let mut t: terminal_c::Terminal = std::ptr::null_mut();
        let t_new_result = unsafe { terminal_c::new(calloc, &mut t, 80, 24) };
        unsafe { terminal_c::vt_write(t, bytes.as_ptr(), bytes.len()) };
        let mut f: Formatter = std::ptr::null_mut();
        let f_new_result = unsafe {
            terminal_new(
                Some(calloc),
                &mut f,
                t,
                make_opts(emit, false, true, extra, selection),
            )
        };
        (t_new_result, t, f_new_result, f)
    }

    fn plain_extra() -> TerminalOptionsExtra {
        make_extra(
            false,
            false,
            false,
            false,
            false,
            false,
            make_screen(false, false, false, false, false, false),
        )
    }

    // test "format plain" {
    #[test]
    fn format_plain() {
        with_test_allocator(|calloc| {
            // terminal_c.vt_write(t, "Hello", 5);
            let (t_new_result, t, f_new_result, f) = setup_plain(
                calloc,
                b"Hello",
                plain_extra(),
                Format::Plain,
                std::ptr::null(),
            );
            // try testing.expectEqual(Result.success, terminal_c.new(&test_allocator, &t, .{ ... }));
            assert_eq!(Result::Success, t_new_result);
            // try testing.expectEqual(Result.success, terminal_new(&test_allocator, &f, t, opts));
            assert_eq!(Result::Success, f_new_result);

            // var buf: [1024]u8 = undefined; var written: usize = 0;
            let mut buf = [0u8; 1024];
            let mut written: usize = 0;
            // try testing.expectEqual(Result.success, format_buf(f, &buf, buf.len, &written));
            assert_eq!(Result::Success, unsafe {
                format_buf(f, buf.as_mut_ptr(), buf.len(), &mut written)
            });
            // try testing.expectEqualStrings("Hello", buf[0..written]);
            assert_eq!(b"Hello", &buf[0..written]);

            unsafe { free(f) };
            unsafe { terminal_c::free(t) };
        });
    }

    // test "format reflects terminal changes" {
    #[test]
    fn format_reflects_terminal_changes() {
        with_test_allocator(|calloc| {
            let (t_new_result, t, f_new_result, f) = setup_plain(
                calloc,
                b"Hello",
                plain_extra(),
                Format::Plain,
                std::ptr::null(),
            );
            // try testing.expectEqual(Result.success, terminal_c.new(&test_allocator, &t, .{ ... }));
            assert_eq!(Result::Success, t_new_result);
            // try testing.expectEqual(Result.success, terminal_new(&test_allocator, &f, t, opts));
            assert_eq!(Result::Success, f_new_result);

            let mut buf = [0u8; 1024];
            let mut written: usize = 0;
            assert_eq!(Result::Success, unsafe {
                format_buf(f, buf.as_mut_ptr(), buf.len(), &mut written)
            });
            assert_eq!(b"Hello", &buf[0..written]);

            // Write more data and re-format
            // terminal_c.vt_write(t, "\r\nWorld", 7);
            let more = b"\r\nWorld";
            unsafe { terminal_c::vt_write(t, more.as_ptr(), more.len()) };

            assert_eq!(Result::Success, unsafe {
                format_buf(f, buf.as_mut_ptr(), buf.len(), &mut written)
            });
            // try testing.expectEqualStrings("Hello\nWorld", buf[0..written]);
            assert_eq!(b"Hello\nWorld", &buf[0..written]);

            unsafe { free(f) };
            unsafe { terminal_c::free(t) };
        });
    }

    // test "format null returns required size" {
    #[test]
    fn format_null_returns_required_size() {
        with_test_allocator(|calloc| {
            let (t_new_result, t, f_new_result, f) = setup_plain(
                calloc,
                b"Hello",
                plain_extra(),
                Format::Plain,
                std::ptr::null(),
            );
            // try testing.expectEqual(Result.success, terminal_c.new(&test_allocator, &t, .{ ... }));
            assert_eq!(Result::Success, t_new_result);
            // try testing.expectEqual(Result.success, terminal_new(&test_allocator, &f, t, opts));
            assert_eq!(Result::Success, f_new_result);

            // Pass null buffer to query required size
            // var required: usize = 0;
            let mut required: usize = 0;
            // try testing.expectEqual(Result.out_of_space, format_buf(f, null, 0, &required));
            assert_eq!(Result::OutOfSpace, unsafe {
                format_buf(f, std::ptr::null_mut(), 0, &mut required)
            });
            // try testing.expect(required > 0);
            assert!(required > 0);

            // Now allocate and format
            let mut buf = [0u8; 1024];
            let mut written: usize = 0;
            assert_eq!(Result::Success, unsafe {
                format_buf(f, buf.as_mut_ptr(), buf.len(), &mut written)
            });
            // try testing.expectEqual(required, written);
            assert_eq!(required, written);

            unsafe { free(f) };
            unsafe { terminal_c::free(t) };
        });
    }

    // test "format buffer too small" {
    #[test]
    fn format_buffer_too_small() {
        with_test_allocator(|calloc| {
            let (t_new_result, t, f_new_result, f) = setup_plain(
                calloc,
                b"Hello",
                plain_extra(),
                Format::Plain,
                std::ptr::null(),
            );
            // try testing.expectEqual(Result.success, terminal_c.new(&test_allocator, &t, .{ ... }));
            assert_eq!(Result::Success, t_new_result);
            // try testing.expectEqual(Result.success, terminal_new(&test_allocator, &f, t, opts));
            assert_eq!(Result::Success, f_new_result);

            // Buffer too small
            // var buf: [2]u8 = undefined; var written: usize = 0;
            let mut buf = [0u8; 2];
            let mut written: usize = 0;
            // try testing.expectEqual(Result.out_of_space, format_buf(f, &buf, buf.len, &written));
            assert_eq!(Result::OutOfSpace, unsafe {
                format_buf(f, buf.as_mut_ptr(), buf.len(), &mut written)
            });
            // written contains the required size
            // try testing.expectEqual(@as(usize, 5), written);
            assert_eq!(5usize, written);

            unsafe { free(f) };
            unsafe { terminal_c::free(t) };
        });
    }

    // test "format null formatter" {
    #[test]
    fn format_null_formatter() {
        // var written: usize = 0;
        let mut written: usize = 0;
        // try testing.expectEqual(Result.invalid_value, format_buf(null, null, 0, &written));
        assert_eq!(Result::InvalidValue, unsafe {
            format_buf(std::ptr::null_mut(), std::ptr::null_mut(), 0, &mut written)
        });
    }

    // test "format vt" {
    #[test]
    fn format_vt() {
        with_test_allocator(|calloc| {
            // terminal_c.vt_write(t, "Test", 4);
            // .extra = .{ .palette = true, ... .screen = .{ .style = true, .hyperlink = true, ... } }
            let extra = make_extra(
                true,
                false,
                false,
                false,
                false,
                false,
                make_screen(false, true, true, false, false, false),
            );
            let (t_new_result, t, f_new_result, f) =
                setup_plain(calloc, b"Test", extra, Format::Vt, std::ptr::null());
            // try testing.expectEqual(Result.success, terminal_c.new(&test_allocator, &t, .{ ... }));
            assert_eq!(Result::Success, t_new_result);
            // try testing.expectEqual(Result.success, terminal_new(&test_allocator, &f, t, opts));
            assert_eq!(Result::Success, f_new_result);

            // var buf: [65536]u8 = undefined; var written: usize = 0;
            let mut buf = vec![0u8; 65536];
            let mut written: usize = 0;
            assert_eq!(Result::Success, unsafe {
                format_buf(f, buf.as_mut_ptr(), buf.len(), &mut written)
            });
            // try testing.expect(written > 0);
            assert!(written > 0);
            // try testing.expect(std.mem.indexOf(u8, buf[0..written], "Test") != null);
            assert!(buf[0..written].windows(4).any(|w| w == b"Test"));

            unsafe { free(f) };
            unsafe { terminal_c::free(t) };
        });
    }

    // test "format plain with selection" {
    #[test]
    fn format_plain_with_selection() {
        with_test_allocator(|calloc| {
            // terminal_c.vt_write(t, "Hello World", 11);
            let mut t: terminal_c::Terminal = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                terminal_c::new(calloc, &mut t, 80, 24)
            });
            let bytes = b"Hello World";
            unsafe { terminal_c::vt_write(t, bytes.as_ptr(), bytes.len()) };

            // Get grid refs for "World" (columns 6..10 on row 0)
            // var start_ref: grid_ref.CGridRef = .{};
            let mut start_ref = CGridRef::default();
            // try testing.expectEqual(Result.success, terminal_c.grid_ref(t,
            //     .{ .tag = .active, .value = .{ .active = .{ .x = 6, .y = 0 } } }, &start_ref));
            assert_eq!(Result::Success, unsafe {
                terminal_c::grid_ref(
                    t,
                    PointC {
                        tag: TagC::Active,
                        value: PointCValue {
                            active: Coordinate { x: 6, y: 0 },
                        },
                    },
                    &mut start_ref,
                )
            });

            // var end_ref: grid_ref.CGridRef = .{};
            let mut end_ref = CGridRef::default();
            assert_eq!(Result::Success, unsafe {
                terminal_c::grid_ref(
                    t,
                    PointC {
                        tag: TagC::Active,
                        value: PointCValue {
                            active: Coordinate { x: 10, y: 0 },
                        },
                    },
                    &mut end_ref,
                )
            });

            // const sel: selection_c.CSelection = .{ .start = start_ref, .end = end_ref };
            let sel = CSelection {
                start: start_ref,
                end: end_ref,
                ..CSelection::default()
            };

            // .selection = &sel
            let mut f: Formatter = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                terminal_new(
                    Some(calloc),
                    &mut f,
                    t,
                    make_opts(Format::Plain, false, true, plain_extra(), &sel),
                )
            });

            let mut buf = [0u8; 1024];
            let mut written: usize = 0;
            assert_eq!(Result::Success, unsafe {
                format_buf(f, buf.as_mut_ptr(), buf.len(), &mut written)
            });
            // try testing.expectEqualStrings("World", buf[0..written]);
            assert_eq!(b"World", &buf[0..written]);

            unsafe { free(f) };
            unsafe { terminal_c::free(t) };
        });
    }

    // test "format html" {
    #[test]
    fn format_html() {
        with_test_allocator(|calloc| {
            // terminal_c.vt_write(t, "Html", 4);
            let (t_new_result, t, f_new_result, f) = setup_plain(
                calloc,
                b"Html",
                plain_extra(),
                Format::Html,
                std::ptr::null(),
            );
            // try testing.expectEqual(Result.success, terminal_c.new(&test_allocator, &t, .{ ... }));
            assert_eq!(Result::Success, t_new_result);
            // try testing.expectEqual(Result.success, terminal_new(&test_allocator, &f, t, opts));
            assert_eq!(Result::Success, f_new_result);

            // var buf: [65536]u8 = undefined; var written: usize = 0;
            let mut buf = vec![0u8; 65536];
            let mut written: usize = 0;
            assert_eq!(Result::Success, unsafe {
                format_buf(f, buf.as_mut_ptr(), buf.len(), &mut written)
            });
            // try testing.expect(written > 0);
            assert!(written > 0);
            // try testing.expect(std.mem.indexOf(u8, buf[0..written], "Html") != null);
            assert!(buf[0..written].windows(4).any(|w| w == b"Html"));

            unsafe { free(f) };
            unsafe { terminal_c::free(t) };
        });
    }
}
