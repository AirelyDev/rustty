//! rustty-vt-c selection
//!
//! PORTS: ghostty src/terminal/c/selection.zig
//!
//! PORT STATUS: FILLED (mutual-atom Stage B v4). Every `#[repr(C)]` extern
//! struct is byte-exact; every C API function body, the private
//! `formatSelection`/`selectionFormatter`/`codepointSlice` helpers, and all
//! `test {}` blocks are ported (assertion-for-assertion). Part of the irreducible
//! mutual cluster (terminal<->grid_ref<->grid_ref_tracked<->selection).

// const std = @import("std");
// const testing = std.testing;
// const lib = @import("../lib.zig");
// const CAllocator = lib.alloc.Allocator;
// const formatterpkg = @import("../formatter.zig");
// const grid_ref = @import("grid_ref.zig");
// const point = @import("../point.zig");
// const selection_codepoints = @import("../selection_codepoints.zig");
// const Selection = @import("../Selection.zig");
// const Result = @import("result.zig").Result;
// const terminal_c = @import("terminal.zig");
use crate::grid_ref::CGridRef;
use crate::point::PointC;
use crate::result::Result;
use crate::terminal::Terminal;
use rustty_lib::allocator::Allocator as CAllocator;
use rustty_vt::terminal::Terminal as ZigTerminal;
use std::mem::{align_of, size_of};

// const log = std.log.scoped(.selection_c);

// pub const Adjustment = Selection.Adjustment;
// pub const Order = Selection.Order;
// pub const Format = formatterpkg.Format;
//
// PORT NOTE: in the reference these are aliases of the pure-VT
// `Selection.Adjustment`/`Selection.Order`/`formatter.Format`, which for the
// `.c` target are `lib.Enum`-generated `enum(c_int)`s. The rustty-vt enums are
// the pure-VT (`.zig`-target) plain enums, so the C ABI surface re-projects them
// here as `#[repr(C)]` fieldless `c_int` enums (a faithful re-projection per the
// recon, NOT a fabrication). The `to_zig`/`from_zig` bridges keep the
// discriminants pinned to the pure-VT key order.

/// C: GhosttySelectionAdjustment -> RusttySelectionAdjustment (R2) — the `enum(c_int)` projection of
/// `Selection.Adjustment`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Adjustment {
    Left = 0,
    Right = 1,
    Up = 2,
    Down = 3,
    Home = 4,
    End = 5,
    PageUp = 6,
    PageDown = 7,
    BeginningOfLine = 8,
    EndOfLine = 9,
}

impl Adjustment {
    // PORT NOTE: bridge to the pure-VT `selection::Adjustment`; discriminants are
    // identical by construction (same key order).
    fn to_zig(self) -> rustty_vt::selection::Adjustment {
        use rustty_vt::selection::Adjustment as A;
        match self {
            Adjustment::Left => A::Left,
            Adjustment::Right => A::Right,
            Adjustment::Up => A::Up,
            Adjustment::Down => A::Down,
            Adjustment::Home => A::Home,
            Adjustment::End => A::End,
            Adjustment::PageUp => A::PageUp,
            Adjustment::PageDown => A::PageDown,
            Adjustment::BeginningOfLine => A::BeginningOfLine,
            Adjustment::EndOfLine => A::EndOfLine,
        }
    }
}

/// C: GhosttySelectionOrder -> RusttySelectionOrder (R2) — the `enum(c_int)` projection of `Selection.Order`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Order {
    Forward = 0,
    Reverse = 1,
    MirroredForward = 2,
    MirroredReverse = 3,
}

impl Order {
    // PORT NOTE: bridges to/from the pure-VT `selection::Order`; discriminants are
    // identical by construction (same key order).
    fn to_zig(self) -> rustty_vt::selection::Order {
        use rustty_vt::selection::Order as O;
        match self {
            Order::Forward => O::Forward,
            Order::Reverse => O::Reverse,
            Order::MirroredForward => O::MirroredForward,
            Order::MirroredReverse => O::MirroredReverse,
        }
    }

    fn from_zig(o: rustty_vt::selection::Order) -> Order {
        use rustty_vt::selection::Order as O;
        match o {
            O::Forward => Order::Forward,
            O::Reverse => Order::Reverse,
            O::MirroredForward => Order::MirroredForward,
            O::MirroredReverse => Order::MirroredReverse,
        }
    }
}

/// C: GhosttySelectionFormat -> RusttySelectionFormat (R2) — the `enum(c_int)` projection of `formatter.Format`.
// PORT NOTE: `formatter.Format = lib.Enum(lib.target, &.{ "plain", "vt", "html" })`;
// for the `.c` target a fieldless `enum(c_int)` with the keys in order (pinned
// here against `rustty_vt::formatter::Format`). The `to_zig` bridge keeps the
// discriminants matched to the pure-VT key order.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Plain = 0,
    Vt = 1,
    Html = 2,
}

impl Format {
    // PORT NOTE: bridge to the pure-VT `formatter::Format`. The discriminants are
    // identical by construction (same key order plain/vt/html).
    pub fn to_zig(self) -> rustty_vt::formatter::Format {
        match self {
            Format::Plain => rustty_vt::formatter::Format::Plain,
            Format::Vt => rustty_vt::formatter::Format::Vt,
            Format::Html => rustty_vt::formatter::Format::Html,
        }
    }
}

/// C: GhosttySelection -> RusttySelection (R2)
// pub const CSelection = extern struct {
//     size: usize = @sizeOf(CSelection),
//     start: grid_ref.CGridRef,
//     end: grid_ref.CGridRef,
//     rectangle: bool = false,
// };
#[repr(C)]
#[derive(Clone, Copy)]
pub struct CSelection {
    pub size: usize,
    pub start: CGridRef,
    pub end: CGridRef,
    pub rectangle: bool,
}

impl Default for CSelection {
    fn default() -> Self {
        CSelection {
            size: size_of::<CSelection>(),
            start: CGridRef::default(),
            end: CGridRef::default(),
            rectangle: false,
        }
    }
}

impl CSelection {
    // pub fn toZig(self: CSelection) ?Selection {
    //     const start_pin = self.start.toPin() orelse return null;
    //     const end_pin = self.end.toPin() orelse return null;
    //     return Selection.init(start_pin, end_pin, self.rectangle);
    // }
    pub fn to_zig(self) -> Option<rustty_vt::selection::Selection> {
        // const start_pin = self.start.toPin() orelse return null;
        let start_pin = self.start.to_pin()?;
        // const end_pin = self.end.toPin() orelse return null;
        let end_pin = self.end.to_pin()?;
        // return Selection.init(start_pin, end_pin, self.rectangle);
        Some(rustty_vt::selection::Selection::init(
            start_pin,
            end_pin,
            self.rectangle,
        ))
    }

    // pub fn fromZig(sel: Selection) CSelection {
    //     return .{ .start = .fromPin(sel.start()), .end = .fromPin(sel.end()), .rectangle = sel.rectangle };
    // }
    // PORT NOTE: `.size` rides the `Default` impl (the reference fills
    // `.size = @sizeOf(CSelection)` via the field default).
    pub fn from_zig(sel: rustty_vt::selection::Selection) -> CSelection {
        CSelection {
            start: CGridRef::from_pin(sel.start()),
            end: CGridRef::from_pin(sel.end()),
            rectangle: sel.rectangle,
            ..CSelection::default()
        }
    }
}

/// C: GhosttyTerminalSelectWordOptions -> RusttyTerminalSelectWordOptions (R2)
// pub const SelectWordOptions = extern struct {
//     size: usize = @sizeOf(SelectWordOptions),
//     ref: grid_ref.CGridRef,
//     boundary_codepoints: ?[*]const u32 = null,
//     boundary_codepoints_len: usize = 0,
// };
#[repr(C)]
#[derive(Clone, Copy)]
pub struct SelectWordOptions {
    pub size: usize,
    pub ref_: CGridRef,
    pub boundary_codepoints: *const u32,
    pub boundary_codepoints_len: usize,
}

/// C: GhosttyTerminalSelectWordBetweenOptions -> RusttyTerminalSelectWordBetweenOptions (R2)
// pub const SelectWordBetweenOptions = extern struct {
//     size: usize = @sizeOf(SelectWordBetweenOptions),
//     start: grid_ref.CGridRef,
//     end: grid_ref.CGridRef,
//     boundary_codepoints: ?[*]const u32 = null,
//     boundary_codepoints_len: usize = 0,
// };
#[repr(C)]
#[derive(Clone, Copy)]
pub struct SelectWordBetweenOptions {
    pub size: usize,
    pub start: CGridRef,
    pub end: CGridRef,
    pub boundary_codepoints: *const u32,
    pub boundary_codepoints_len: usize,
}

/// C: GhosttyTerminalSelectLineOptions -> RusttyTerminalSelectLineOptions (R2)
// pub const SelectLineOptions = extern struct {
//     size: usize = @sizeOf(SelectLineOptions),
//     ref: grid_ref.CGridRef,
//     whitespace: ?[*]const u32 = null,
//     whitespace_len: usize = 0,
//     semantic_prompt_boundary: bool = false,
// };
#[repr(C)]
#[derive(Clone, Copy)]
pub struct SelectLineOptions {
    pub size: usize,
    pub ref_: CGridRef,
    pub whitespace: *const u32,
    pub whitespace_len: usize,
    pub semantic_prompt_boundary: bool,
}

/// C: GhosttyTerminalSelectionFormatOptions -> RusttyTerminalSelectionFormatOptions (R2)
// pub const FormatOptions = extern struct {
//     size: usize = @sizeOf(FormatOptions),
//     emit: Format,
//     unwrap: bool,
//     trim: bool,
//     selection: ?*const CSelection = null,
// };
#[repr(C)]
#[derive(Clone, Copy)]
pub struct FormatOptions {
    pub size: usize,
    pub emit: Format,
    pub unwrap: bool,
    pub trim: bool,
    pub selection: *const CSelection,
}

// pub fn word(terminal, options: ?*const SelectWordOptions, out_selection: ?*CSelection) ... {
//     const t = terminal_c.zigTerminal(terminal) orelse return .invalid_value;
//     const opts = options orelse return .invalid_value;
//     if (opts.size < @sizeOf(SelectWordOptions)) return .invalid_value;
//     const out = out_selection orelse return .invalid_value;
//     const boundary_codepoints = codepointSlice(opts.boundary_codepoints, opts.boundary_codepoints_len) catch return .invalid_value;
//     const screen = t.screens.active;
//     const pin = opts.ref.toPin() orelse return .invalid_value;
//     out.* = .fromZig(screen.selectWord(pin, boundary_codepoints orelse &selection_codepoints.default_word_boundaries) orelse return .no_value);
//     return .success;
// }
/// # Safety
/// `terminal`, if non-null, must be a live terminal handle; `options`/
/// `out_selection`, if non-null, must point to valid storage of their types.
pub unsafe fn word(
    terminal: Terminal,
    options: *const SelectWordOptions,
    out_selection: *mut CSelection,
) -> Result {
    // const t = terminal_c.zigTerminal(terminal) orelse return .invalid_value;
    let t = unsafe { crate::terminal::zig_terminal(terminal) };
    if t.is_null() {
        return Result::InvalidValue;
    }
    // const opts = options orelse return .invalid_value;
    if options.is_null() {
        return Result::InvalidValue;
    }
    // if (opts.size < @sizeOf(SelectWordOptions)) return .invalid_value;
    if unsafe { (*options).size } < size_of::<SelectWordOptions>() {
        return Result::InvalidValue;
    }
    // const out = out_selection orelse return .invalid_value;
    if out_selection.is_null() {
        return Result::InvalidValue;
    }

    // const boundary_codepoints = codepointSlice(opts.boundary_codepoints, opts.boundary_codepoints_len) catch return .invalid_value;
    let boundary_codepoints = match unsafe {
        codepoint_slice(
            (*options).boundary_codepoints,
            (*options).boundary_codepoints_len,
        )
    } {
        Ok(b) => b,
        Err(_) => return Result::InvalidValue,
    };

    // const screen = t.screens.active;
    let screen = unsafe { (*t).screens.active };
    // const pin = opts.ref.toPin() orelse return .invalid_value;
    let pin = match unsafe { (*options).ref_ }.to_pin() {
        Some(p) => p,
        None => return Result::InvalidValue,
    };
    // out.* = .fromZig(screen.selectWord(pin, boundary_codepoints orelse &default_word_boundaries) orelse return .no_value);
    let sel = match unsafe {
        (*screen).select_word(
            pin,
            boundary_codepoints
                .unwrap_or(&rustty_vt::selection_codepoints::DEFAULT_WORD_BOUNDARIES),
        )
    } {
        Some(s) => s,
        None => return Result::NoValue,
    };
    unsafe { *out_selection = CSelection::from_zig(sel) };
    // return .success;
    Result::Success
}

// pub fn word_between(terminal, options: ?*const SelectWordBetweenOptions, out_selection: ?*CSelection) ... {
//     ... const screen = t.screens.active;
//     const start = opts.start.toPin() orelse return .invalid_value;
//     const end = opts.end.toPin() orelse return .invalid_value;
//     out.* = .fromZig(screen.selectWordBetween(start, end, boundary_codepoints orelse &default_word_boundaries) orelse return .no_value);
//     return .success;
// }
/// # Safety
/// See `word`.
pub unsafe fn word_between(
    terminal: Terminal,
    options: *const SelectWordBetweenOptions,
    out_selection: *mut CSelection,
) -> Result {
    // const t = terminal_c.zigTerminal(terminal) orelse return .invalid_value;
    let t = unsafe { crate::terminal::zig_terminal(terminal) };
    if t.is_null() {
        return Result::InvalidValue;
    }
    // const opts = options orelse return .invalid_value;
    if options.is_null() {
        return Result::InvalidValue;
    }
    // if (opts.size < @sizeOf(SelectWordBetweenOptions)) return .invalid_value;
    if unsafe { (*options).size } < size_of::<SelectWordBetweenOptions>() {
        return Result::InvalidValue;
    }
    // const out = out_selection orelse return .invalid_value;
    if out_selection.is_null() {
        return Result::InvalidValue;
    }

    // const boundary_codepoints = codepointSlice(...) catch return .invalid_value;
    let boundary_codepoints = match unsafe {
        codepoint_slice(
            (*options).boundary_codepoints,
            (*options).boundary_codepoints_len,
        )
    } {
        Ok(b) => b,
        Err(_) => return Result::InvalidValue,
    };

    // const screen = t.screens.active;
    let screen = unsafe { (*t).screens.active };
    // const start = opts.start.toPin() orelse return .invalid_value;
    let start = match unsafe { (*options).start }.to_pin() {
        Some(p) => p,
        None => return Result::InvalidValue,
    };
    // const end = opts.end.toPin() orelse return .invalid_value;
    let end = match unsafe { (*options).end }.to_pin() {
        Some(p) => p,
        None => return Result::InvalidValue,
    };
    // out.* = .fromZig(screen.selectWordBetween(start, end, boundary_codepoints orelse &default_word_boundaries) orelse return .no_value);
    let sel = match unsafe {
        (*screen).select_word_between(
            start,
            end,
            boundary_codepoints
                .unwrap_or(&rustty_vt::selection_codepoints::DEFAULT_WORD_BOUNDARIES),
        )
    } {
        Some(s) => s,
        None => return Result::NoValue,
    };
    unsafe { *out_selection = CSelection::from_zig(sel) };
    // return .success;
    Result::Success
}

// pub fn line(terminal, options: ?*const SelectLineOptions, out_selection: ?*CSelection) ... {
//     ... const whitespace = codepointSlice(opts.whitespace, opts.whitespace_len) catch return .invalid_value;
//     const screen = t.screens.active;
//     const pin = opts.ref.toPin() orelse return .invalid_value;
//     out.* = .fromZig(screen.selectLine(.{ .pin = pin, .whitespace = whitespace orelse &default_line_whitespace, .semantic_prompt_boundary = opts.semantic_prompt_boundary }) orelse return .no_value);
//     return .success;
// }
/// # Safety
/// See `word`.
pub unsafe fn line(
    terminal: Terminal,
    options: *const SelectLineOptions,
    out_selection: *mut CSelection,
) -> Result {
    // const t = terminal_c.zigTerminal(terminal) orelse return .invalid_value;
    let t = unsafe { crate::terminal::zig_terminal(terminal) };
    if t.is_null() {
        return Result::InvalidValue;
    }
    // const opts = options orelse return .invalid_value;
    if options.is_null() {
        return Result::InvalidValue;
    }
    // if (opts.size < @sizeOf(SelectLineOptions)) return .invalid_value;
    if unsafe { (*options).size } < size_of::<SelectLineOptions>() {
        return Result::InvalidValue;
    }
    // const out = out_selection orelse return .invalid_value;
    if out_selection.is_null() {
        return Result::InvalidValue;
    }

    // const whitespace = codepointSlice(opts.whitespace, opts.whitespace_len) catch return .invalid_value;
    let whitespace =
        match unsafe { codepoint_slice((*options).whitespace, (*options).whitespace_len) } {
            Ok(w) => w,
            Err(_) => return Result::InvalidValue,
        };

    // const screen = t.screens.active;
    let screen = unsafe { (*t).screens.active };
    // const pin = opts.ref.toPin() orelse return .invalid_value;
    let pin = match unsafe { (*options).ref_ }.to_pin() {
        Some(p) => p,
        None => return Result::InvalidValue,
    };
    // out.* = .fromZig(screen.selectLine(.{ .pin, .whitespace = whitespace orelse &default_line_whitespace, .semantic_prompt_boundary = opts.semantic_prompt_boundary }) orelse return .no_value);
    // PORT NOTE: the reference substitutes `&default_line_whitespace` for a null
    // codepoint set; rustty `SelectLine.whitespace` is `Option<&[u32]>`, so we
    // pass `Some(whitespace orelse &DEFAULT_LINE_WHITESPACE)` to preserve the
    // "explicitly empty set != use default" distinction the reference encodes.
    let ws: &[u32] =
        whitespace.unwrap_or(&rustty_vt::selection_codepoints::DEFAULT_LINE_WHITESPACE);
    let sel = match unsafe {
        (*screen).select_line(rustty_vt::screen::SelectLine {
            pin,
            whitespace: Some(ws),
            semantic_prompt_boundary: (*options).semantic_prompt_boundary,
        })
    } {
        Some(s) => s,
        None => return Result::NoValue,
    };
    unsafe { *out_selection = CSelection::from_zig(sel) };
    // return .success;
    Result::Success
}

// pub fn all(terminal, out_selection: ?*CSelection) ... {
//     const t = terminal_c.zigTerminal(terminal) orelse return .invalid_value;
//     const out = out_selection orelse return .invalid_value;
//     out.* = .fromZig(t.screens.active.selectAll() orelse return .no_value);
//     return .success;
// }
/// # Safety
/// `terminal`, if non-null, must be a live terminal handle; `out_selection`, if
/// non-null, must be a valid writable pointer to a `CSelection`.
pub unsafe fn all(terminal: Terminal, out_selection: *mut CSelection) -> Result {
    // const t = terminal_c.zigTerminal(terminal) orelse return .invalid_value;
    let t = unsafe { crate::terminal::zig_terminal(terminal) };
    if t.is_null() {
        return Result::InvalidValue;
    }
    // const out = out_selection orelse return .invalid_value;
    if out_selection.is_null() {
        return Result::InvalidValue;
    }
    // out.* = .fromZig(t.screens.active.selectAll() orelse return .no_value);
    let screen = unsafe { (*t).screens.active };
    let sel = match unsafe { (*screen).select_all() } {
        Some(s) => s,
        None => return Result::NoValue,
    };
    unsafe { *out_selection = CSelection::from_zig(sel) };
    // return .success;
    Result::Success
}

// pub fn output(terminal, ref: grid_ref.CGridRef, out_selection: ?*CSelection) ... {
//     const t = terminal_c.zigTerminal(terminal) orelse return .invalid_value;
//     const out = out_selection orelse return .invalid_value;
//     const screen = t.screens.active;
//     const pin = ref.toPin() orelse return .invalid_value;
//     out.* = .fromZig(screen.selectOutput(pin) orelse return .no_value);
//     return .success;
// }
/// # Safety
/// `terminal`, if non-null, must be a live terminal handle; `out_selection`, if
/// non-null, must be a valid writable pointer to a `CSelection`.
pub unsafe fn output(terminal: Terminal, ref_: CGridRef, out_selection: *mut CSelection) -> Result {
    // const t = terminal_c.zigTerminal(terminal) orelse return .invalid_value;
    let t = unsafe { crate::terminal::zig_terminal(terminal) };
    if t.is_null() {
        return Result::InvalidValue;
    }
    // const out = out_selection orelse return .invalid_value;
    if out_selection.is_null() {
        return Result::InvalidValue;
    }
    // const screen = t.screens.active;
    let screen = unsafe { (*t).screens.active };
    // const pin = ref.toPin() orelse return .invalid_value;
    let pin = match ref_.to_pin() {
        Some(p) => p,
        None => return Result::InvalidValue,
    };
    // out.* = .fromZig(screen.selectOutput(pin) orelse return .no_value);
    let sel = match unsafe { (*screen).select_output(pin) } {
        Some(s) => s,
        None => return Result::NoValue,
    };
    unsafe { *out_selection = CSelection::from_zig(sel) };
    // return .success;
    Result::Success
}

// PORT NOTE (Rust-only glue): the reference uses `std.Io.Writer` variants:
// `Discarding` (counts written bytes, never fails), `.fixed` (writes into a
// caller buffer, fails with `WriteFailed` when full), and `.Allocating` (grows a
// heap buffer). rustty's `formatter.format` takes `&mut dyn io::Write`. These
// adapters reproduce the three writer behaviors. `WriteFailed` is signaled as an
// `io::Error` of kind `WriteZero` and recovered/classified by the callers exactly
// as the reference's error switch does.

/// Counts bytes written without storing them (the reference `Discarding` writer).
struct CountingWriter {
    count: usize,
}

impl std::io::Write for CountingWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.count += buf.len();
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Writes into a fixed caller buffer; fails when the buffer is exhausted (the
/// reference `std.Io.Writer.fixed`). `end` tracks how many bytes were written
/// (mirrors `writer.end`).
struct FixedWriter<'a> {
    buf: &'a mut [u8],
    end: usize,
}

impl std::io::Write for FixedWriter<'_> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let remaining = self.buf.len() - self.end;
        if remaining == 0 && !buf.is_empty() {
            // error.WriteFailed
            return Err(std::io::Error::from(std::io::ErrorKind::WriteZero));
        }
        let n = core::cmp::min(remaining, buf.len());
        self.buf[self.end..self.end + n].copy_from_slice(&buf[..n]);
        self.end += n;
        if n < buf.len() {
            // error.WriteFailed — partial write means the buffer is full.
            return Err(std::io::Error::from(std::io::ErrorKind::WriteZero));
        }
        Ok(n)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

// PORT NOTE: the reference `format*` functions classify `formatSelection`'s
// errors (below). `error.WriteFailed` is `unreachable` for the Discarding writer
// (it never fails) and is the buffer-full signal for the fixed writer.
#[derive(Debug)]
enum FormatError {
    InvalidValue,
    NoValue,
    WriteFailed,
}

// pub fn format_buf(terminal, opts: FormatOptions, out_: ?[*]u8, out_len: usize, out_written: *usize) ... {
//     const t = terminal_c.zigTerminal(terminal) orelse return .invalid_value;
//     if (out_ == null) {
//         var discarding: std.Io.Writer.Discarding = .init(&.{});
//         formatSelection(t, opts, &discarding.writer) catch |err| return switch (err) { ... };
//         out_written.* = @intCast(discarding.count);
//         return .out_of_space;
//     }
//     var writer: std.Io.Writer = .fixed(out_.?[0..out_len]);
//     formatSelection(t, opts, &writer) catch |err| switch (err) {
//         error.InvalidValue => return .invalid_value,
//         error.NoValue => return .no_value,
//         error.WriteFailed => {
//             var discarding ...; formatSelection(...) catch unreachable; out_written.* = discarding.count; return .out_of_space;
//         },
//     };
//     out_written.* = writer.end;
//     return .success;
// }
/// # Safety
/// `terminal`, if non-null, must be a live terminal handle; `out_`, if non-null,
/// must point to `out_len` writable bytes; `out_written` must be a valid
/// writable pointer.
pub unsafe fn format_buf(
    terminal: Terminal,
    opts: FormatOptions,
    out_: *mut u8,
    out_len: usize,
    out_written: *mut usize,
) -> Result {
    // const t = terminal_c.zigTerminal(terminal) orelse return .invalid_value;
    let t = unsafe { crate::terminal::zig_terminal(terminal) };
    if t.is_null() {
        return Result::InvalidValue;
    }

    // if (out_ == null) { ... }
    if out_.is_null() {
        // var discarding: std.Io.Writer.Discarding = .init(&.{});
        let mut discarding = CountingWriter { count: 0 };
        // formatSelection(t, opts, &discarding.writer) catch |err| return switch (err) { ... };
        if let Err(err) = format_selection(t, opts, &mut discarding) {
            return match err {
                FormatError::InvalidValue => Result::InvalidValue,
                FormatError::NoValue => Result::NoValue,
                // error.WriteFailed => unreachable,
                FormatError::WriteFailed => unreachable!(),
            };
        }
        // out_written.* = @intCast(discarding.count);
        unsafe { *out_written = discarding.count };
        // return .out_of_space;
        return Result::OutOfSpace;
    }

    // var writer: std.Io.Writer = .fixed(out_.?[0..out_len]);
    // SAFETY: `out_` is non-null and points to `out_len` writable bytes.
    let buf = unsafe { core::slice::from_raw_parts_mut(out_, out_len) };
    let mut writer = FixedWriter { buf, end: 0 };
    // formatSelection(t, opts, &writer) catch |err| switch (err) { ... };
    if let Err(err) = format_selection(t, opts, &mut writer) {
        match err {
            FormatError::InvalidValue => return Result::InvalidValue,
            FormatError::NoValue => return Result::NoValue,
            FormatError::WriteFailed => {
                // var discarding ...; formatSelection(...) catch unreachable; out_written.* = ...; return .out_of_space;
                let mut discarding = CountingWriter { count: 0 };
                format_selection(t, opts, &mut discarding).unwrap_or_else(|_| unreachable!());
                unsafe { *out_written = discarding.count };
                return Result::OutOfSpace;
            }
        }
    }
    // out_written.* = writer.end;
    unsafe { *out_written = writer.end };
    // return .success;
    Result::Success
}

// pub fn format_alloc(terminal, alloc_: ?*const CAllocator, opts: FormatOptions, out_ptr: *?[*]u8, out_len: *usize) ... {
//     out_ptr.* = null; out_len.* = 0;
//     const t = terminal_c.zigTerminal(terminal) orelse return .invalid_value;
//     const alloc = lib.alloc.default(alloc_);
//     var aw: std.Io.Writer.Allocating = .init(alloc);
//     defer aw.deinit();
//     formatSelection(t, opts, &aw.writer) catch |err| return switch (err) {
//         error.InvalidValue => .invalid_value, error.NoValue => .no_value, error.WriteFailed => .out_of_memory,
//     };
//     const buf = aw.toOwnedSlice() catch return .out_of_memory;
//     out_ptr.* = buf.ptr; out_len.* = buf.len;
//     return .success;
// }
/// # Safety
/// `terminal`, if non-null, must be a live terminal handle; `alloc_`, if
/// non-null, must point to a valid `CAllocator`; `out_ptr`/`out_len` must be
/// valid writable pointers.
pub unsafe fn format_alloc(
    terminal: Terminal,
    alloc_: *const CAllocator,
    opts: FormatOptions,
    out_ptr: *mut *mut u8,
    out_len: *mut usize,
) -> Result {
    // out_ptr.* = null; out_len.* = 0;
    unsafe {
        *out_ptr = std::ptr::null_mut();
        *out_len = 0;
    }

    // const t = terminal_c.zigTerminal(terminal) orelse return .invalid_value;
    let t = unsafe { crate::terminal::zig_terminal(terminal) };
    if t.is_null() {
        return Result::InvalidValue;
    }

    // const alloc = lib.alloc.default(alloc_);
    let alloc = rustty_lib::allocator::default(unsafe { alloc_.as_ref() });

    // var aw: std.Io.Writer.Allocating = .init(alloc);
    // PORT NOTE: rustty accumulates into an in-process `Vec<u8>`, then copies into
    // a buffer allocated through the C allocator (`alloc`) so the returned pointer
    // is owned by — and freeable through — that allocator (mirroring the
    // reference's `Allocating` writer backed by the C allocator + `toOwnedSlice`).
    let mut aw: Vec<u8> = Vec::new();
    // formatSelection(t, opts, &aw.writer) catch |err| return switch (err) { ... };
    if let Err(err) = format_selection(t, opts, &mut aw) {
        return match err {
            FormatError::InvalidValue => Result::InvalidValue,
            FormatError::NoValue => Result::NoValue,
            FormatError::WriteFailed => Result::OutOfMemory,
        };
    }

    // const buf = aw.toOwnedSlice() catch return .out_of_memory;
    // out_ptr.* = buf.ptr; out_len.* = buf.len;
    let len = aw.len();
    // PORT NOTE: an empty selection yields a zero-length buffer; the reference
    // `toOwnedSlice` returns an empty (`ptr`, 0) slice. We allocate `len` bytes
    // (alignment 1, matching `[]u8`); a 0-length alloc returns whatever the
    // allocator gives, and `out_len` is 0.
    let ptr = alloc.alloc(len, rustty_lib::allocator::Alignment::from_byte_units(1), 0);
    if ptr.is_null() && len != 0 {
        return Result::OutOfMemory;
    }
    // For an empty selection there is nothing to copy, and `copy_nonoverlapping`
    // requires a NON-NULL dst even for count 0 — so a pathological caller
    // allocator returning NULL for a 0-byte request would make the copy UB and
    // hand back a NULL `out_ptr` (the reference `toOwnedSlice` always yields a
    // non-null empty sentinel). Skip the copy and substitute a non-null aligned
    // sentinel in that case, matching Zig. (VALIDATION_REPORT_2 apprt-c audit, LOW.)
    let ptr = if len != 0 {
        // SAFETY: `ptr` is a fresh `len`-byte allocation; `aw` holds `len` bytes.
        unsafe { core::ptr::copy_nonoverlapping(aw.as_ptr(), ptr, len) };
        ptr
    } else if ptr.is_null() {
        core::ptr::NonNull::<u8>::dangling().as_ptr()
    } else {
        ptr
    };
    unsafe {
        *out_ptr = ptr;
        *out_len = len;
    }
    // return .success;
    Result::Success
}

// fn formatSelection(t, opts, writer) error{ InvalidValue, NoValue, WriteFailed }!void {
//     var formatter = selectionFormatter(t, opts) catch |err| return err;
//     try formatter.format(writer);
// }
//
// PORT NOTE (SOUNDNESS): `t` is the active terminal's owning `*mut ZigTerminal`.
// The formatter reads it through a `*const Terminal`; we pass the raw pointer
// (never a `&mut`/`&` over the whole terminal that would conflict with the stream
// handler's stored alias — the rust-mut-args-strongly-protected discipline).
fn format_selection(
    t: *mut ZigTerminal,
    opts: FormatOptions,
    writer: &mut dyn std::io::Write,
) -> std::result::Result<(), FormatError> {
    // var formatter = selectionFormatter(t, opts) catch |err| return err;
    let formatter = selection_formatter(t, opts)?;
    // try formatter.format(writer);
    // PORT NOTE: `formatter.format` returns `io::Result`; a writer error is the
    // reference's `error.WriteFailed`.
    formatter
        .format(writer)
        .map_err(|_| FormatError::WriteFailed)
}

// fn selectionFormatter(t, opts) error{ InvalidValue, NoValue }!formatterpkg.TerminalFormatter {
//     if (opts.size < @sizeOf(FormatOptions)) return error.InvalidValue;
//     _ = std.meta.intToEnum(Format, @intFromEnum(opts.emit)) catch return error.InvalidValue;
//     const sel = if (opts.selection) |sel| sel.toZig() orelse return error.InvalidValue
//                 else t.screens.active.selection orelse return error.NoValue;
//     var formatter: formatterpkg.TerminalFormatter = .init(t, .{ .emit = opts.emit, .unwrap = opts.unwrap, .trim = opts.trim });
//     formatter.content = .{ .selection = sel };
//     return formatter;
// }
//
// PORT NOTE: the `intToEnum(Format, @intFromEnum(opts.emit))` guard rejects an
// out-of-range `emit`. In rustty `opts.emit` is already a valid `#[repr(C)] Format`
// (the raw-`c_int` range check lives in the lib.rs `extern "C"` wrapper, per the
// cell/key pattern), so the guard is tautological here and folds into `emit.to_zig()`.
fn selection_formatter<'a>(
    t: *mut ZigTerminal,
    opts: FormatOptions,
) -> std::result::Result<rustty_vt::formatter::TerminalFormatter<'a>, FormatError> {
    // if (opts.size < @sizeOf(FormatOptions)) return error.InvalidValue;
    if opts.size < size_of::<FormatOptions>() {
        return Err(FormatError::InvalidValue);
    }
    // _ = std.meta.intToEnum(Format, @intFromEnum(opts.emit)) catch return error.InvalidValue;
    let emit = opts.emit.to_zig();

    // const sel = if (opts.selection) |sel| sel.toZig() orelse return error.InvalidValue
    //             else t.screens.active.selection orelse return error.NoValue;
    let sel = if !opts.selection.is_null() {
        // SAFETY: `opts.selection` is non-null and a valid `CSelection` (caller).
        match unsafe { (*opts.selection).to_zig() } {
            Some(s) => s,
            None => return Err(FormatError::InvalidValue),
        }
    } else {
        // SAFETY: `t` is the live active terminal; read the active screen's
        // selection through raw ptrs.
        let screen = unsafe { (*t).screens.active };
        match unsafe { (*screen).selection } {
            Some(s) => s,
            None => return Err(FormatError::NoValue),
        }
    };

    // var formatter: formatterpkg.TerminalFormatter = .init(t, .{ .emit, .unwrap, .trim });
    let mut formatter = rustty_vt::formatter::TerminalFormatter::init(
        t as *const ZigTerminal,
        rustty_vt::formatter::Options {
            emit,
            unwrap: opts.unwrap,
            trim: opts.trim,
            // PORT NOTE: the reference struct-literal sets only emit/unwrap/trim;
            // the remaining `Options` fields ride their defaults (the `init`'s
            // per-format base). rustty's `Options` has no `Default`, so we mirror
            // the reference field defaults explicitly.
            codepoint_map: None,
            background: None,
            foreground: None,
            palette: None,
        },
    );
    // formatter.content = .{ .selection = sel };
    formatter.content = rustty_vt::formatter::Content::Selection(Some(sel));
    // return formatter;
    Ok(formatter)
}

/// Return the borrowed C array of `uint32_t` codepoints as a `[]const u21`.
///
/// `NULL + len 0` returns null, which callers treat as "use the API default
/// set." A non-null pointer with `len 0` returns an empty slice, meaning "use an
/// explicitly empty set." A non-zero length requires a non-null pointer.
///
/// This is intentionally zero-copy. In the C ABI, codepoints are `uint32_t`,
/// but selection internals use Zig's `u21` to represent valid Unicode scalar
/// values. Zig currently stores `u21` in the same size and alignment as `u32`,
/// so we assert that layout relationship and reinterpret the borrowed slice.
/// If Zig ever changes that representation, these comptime assertions fail
/// loudly rather than silently making this cast wrong.
//
// PORT NOTE: rustty's `u21` IS `u32` (`page::u21 = u32`), so the codepoint
// arrays the selection APIs take are already `&[u32]`; the reinterpret cast is
// the identity and the comptime size/align asserts hold trivially. We keep the
// "null vs explicitly-empty vs non-empty" contract verbatim.
//
// # Safety
// `ptr`, if non-null with `len > 0`, must point to `len` readable `u32`s.
unsafe fn codepoint_slice(
    ptr: *const u32,
    len: usize,
) -> std::result::Result<Option<&'static [u32]>, ()> {
    // comptime { assert(@sizeOf(u21) == @sizeOf(u32)); assert(@alignOf(u21) == @alignOf(u32)); }
    const _: () = assert!(size_of::<rustty_vt::page::u21>() == size_of::<u32>());
    const _: () = assert!(align_of::<rustty_vt::page::u21>() == align_of::<u32>());

    // if (len == 0) { const p = ptr orelse return null; _ = p; return &.{}; }
    if len == 0 {
        if ptr.is_null() {
            return Ok(None);
        }
        return Ok(Some(&[]));
    }

    // const p = ptr orelse return error.InvalidValue;
    if ptr.is_null() {
        return Err(());
    }
    // const cps: [*]const u21 = @ptrCast(p); return cps[0..len];
    // SAFETY: `ptr` is non-null and points to `len` readable u32s (caller). The
    // slice borrows the caller's array, valid for the call (the reference's
    // borrowed slice). The `'static` is the C-borrow analog — only read inside
    // the same call, before any re-entrant mutation.
    Ok(Some(unsafe { core::slice::from_raw_parts(ptr, len) }))
}

// pub fn adjust(terminal, selection: ?*CSelection, adjustment: Selection.Adjustment) ... {
//     if (comptime std.debug.runtime_safety) { _ = intToEnum(...) catch { log.warn(...); return .invalid_value; }; }
//     const t = terminal_c.zigTerminal(terminal) orelse return .invalid_value;
//     const sel_ptr = selection orelse return .invalid_value;
//     var sel = sel_ptr.toZig() orelse return .invalid_value;
//     sel.adjust(t.screens.active, adjustment);
//     sel_ptr.* = .fromZig(sel);
//     return .success;
// }
//
// PORT NOTE: the `intToEnum(Selection.Adjustment, ...)` runtime-safety guard
// rejects an out-of-range `adjustment`. In rustty the raw-`c_int` range check
// lives in the lib.rs `extern "C"` wrapper (the cell/key pattern); here
// `adjustment` is already a valid `#[repr(C)] Adjustment`, so the guard folds
// into `adjustment.to_zig()`.
/// # Safety
/// `terminal`, if non-null, must be a live terminal handle; `selection`, if
/// non-null, must be a valid writable pointer to a `CSelection`.
pub unsafe fn adjust(
    terminal: Terminal,
    selection: *mut CSelection,
    adjustment: Adjustment,
) -> Result {
    // const t = terminal_c.zigTerminal(terminal) orelse return .invalid_value;
    let t = unsafe { crate::terminal::zig_terminal(terminal) };
    if t.is_null() {
        return Result::InvalidValue;
    }
    // const sel_ptr = selection orelse return .invalid_value;
    if selection.is_null() {
        return Result::InvalidValue;
    }
    // var sel = sel_ptr.toZig() orelse return .invalid_value;
    let mut sel = match unsafe { (*selection).to_zig() } {
        Some(s) => s,
        None => return Result::InvalidValue,
    };
    // sel.adjust(t.screens.active, adjustment);
    // SAFETY: read-only borrow of the active screen for the adjustment (no live
    // `&mut` alias exists during this synchronous call).
    let screen = unsafe { (*t).screens.active };
    sel.adjust(unsafe { &*screen }, adjustment.to_zig());
    // sel_ptr.* = .fromZig(sel);
    unsafe { *selection = CSelection::from_zig(sel) };
    // return .success;
    Result::Success
}

// pub fn order(terminal, selection: ?*const CSelection, out_order: ?*Selection.Order) ... {
//     const t = terminal_c.zigTerminal(terminal) orelse return .invalid_value;
//     const sel = (selection orelse return .invalid_value).toZig() orelse return .invalid_value;
//     const out = out_order orelse return .invalid_value;
//     out.* = sel.order(t.screens.active);
//     return .success;
// }
/// # Safety
/// `terminal`, if non-null, must be a live terminal handle; `selection`/
/// `out_order`, if non-null, must point to valid storage of their types.
pub unsafe fn order(
    terminal: Terminal,
    selection: *const CSelection,
    out_order: *mut Order,
) -> Result {
    // const t = terminal_c.zigTerminal(terminal) orelse return .invalid_value;
    let t = unsafe { crate::terminal::zig_terminal(terminal) };
    if t.is_null() {
        return Result::InvalidValue;
    }
    // const sel = (selection orelse return .invalid_value).toZig() orelse return .invalid_value;
    if selection.is_null() {
        return Result::InvalidValue;
    }
    let sel = match unsafe { (*selection).to_zig() } {
        Some(s) => s,
        None => return Result::InvalidValue,
    };
    // const out = out_order orelse return .invalid_value;
    if out_order.is_null() {
        return Result::InvalidValue;
    }
    // out.* = sel.order(t.screens.active);
    let screen = unsafe { (*t).screens.active };
    let ord = sel.order(unsafe { &*screen });
    unsafe { *out_order = Order::from_zig(ord) };
    // return .success;
    Result::Success
}

// pub fn ordered(terminal, selection: ?*const CSelection, desired: Selection.Order, out_selection: ?*CSelection) ... {
//     if (comptime std.debug.runtime_safety) { _ = intToEnum(...) catch { log.warn(...); return .invalid_value; }; }
//     const t = terminal_c.zigTerminal(terminal) orelse return .invalid_value;
//     const sel = (selection orelse return .invalid_value).toZig() orelse return .invalid_value;
//     const out = out_selection orelse return .invalid_value;
//     out.* = .fromZig(sel.ordered(t.screens.active, desired));
//     return .success;
// }
//
// PORT NOTE: the `intToEnum(Selection.Order, ...)` guard lives in lib.rs (the
// cell/key pattern); `desired` is already a valid `#[repr(C)] Order` here.
/// # Safety
/// See `order`.
pub unsafe fn ordered(
    terminal: Terminal,
    selection: *const CSelection,
    desired: Order,
    out_selection: *mut CSelection,
) -> Result {
    // const t = terminal_c.zigTerminal(terminal) orelse return .invalid_value;
    let t = unsafe { crate::terminal::zig_terminal(terminal) };
    if t.is_null() {
        return Result::InvalidValue;
    }
    // const sel = (selection orelse return .invalid_value).toZig() orelse return .invalid_value;
    if selection.is_null() {
        return Result::InvalidValue;
    }
    let sel = match unsafe { (*selection).to_zig() } {
        Some(s) => s,
        None => return Result::InvalidValue,
    };
    // const out = out_selection orelse return .invalid_value;
    if out_selection.is_null() {
        return Result::InvalidValue;
    }
    // out.* = .fromZig(sel.ordered(t.screens.active, desired));
    let screen = unsafe { (*t).screens.active };
    let ordered_sel = sel.ordered(unsafe { &*screen }, desired.to_zig());
    unsafe { *out_selection = CSelection::from_zig(ordered_sel) };
    // return .success;
    Result::Success
}

// pub fn contains(terminal, selection: ?*const CSelection, pt: point.Point.C, out_contains: ?*bool) ... {
//     const t = terminal_c.zigTerminal(terminal) orelse return .invalid_value;
//     const sel = (selection orelse return .invalid_value).toZig() orelse return .invalid_value;
//     const out = out_contains orelse return .invalid_value;
//     const screen = t.screens.active;
//     const pin = screen.pages.pin(.fromC(pt)) orelse return .invalid_value;
//     out.* = sel.contains(screen, pin);
//     return .success;
// }
/// # Safety
/// `terminal`, if non-null, must be a live terminal handle; `selection`/
/// `out_contains`, if non-null, must point to valid storage of their types.
pub unsafe fn contains(
    terminal: Terminal,
    selection: *const CSelection,
    pt: PointC,
    out_contains: *mut bool,
) -> Result {
    // const t = terminal_c.zigTerminal(terminal) orelse return .invalid_value;
    let t = unsafe { crate::terminal::zig_terminal(terminal) };
    if t.is_null() {
        return Result::InvalidValue;
    }
    // const sel = (selection orelse return .invalid_value).toZig() orelse return .invalid_value;
    if selection.is_null() {
        return Result::InvalidValue;
    }
    let sel = match unsafe { (*selection).to_zig() } {
        Some(s) => s,
        None => return Result::InvalidValue,
    };
    // const out = out_contains orelse return .invalid_value;
    if out_contains.is_null() {
        return Result::InvalidValue;
    }
    // const screen = t.screens.active;
    let screen = unsafe { (*t).screens.active };
    // const pin = screen.pages.pin(.fromC(pt)) orelse return .invalid_value;
    let pin = match unsafe { (*screen).pages.pin(pt.from_c()) } {
        Some(p) => p,
        None => return Result::InvalidValue,
    };
    // out.* = sel.contains(screen, pin);
    unsafe { *out_contains = sel.contains(&*screen, pin) };
    // return .success;
    Result::Success
}

// pub fn equal(terminal, a: ?*const CSelection, b: ?*const CSelection, out_equal: ?*bool) ... {
//     _ = terminal_c.zigTerminal(terminal) orelse return .invalid_value;
//     const sel_a = (a orelse return .invalid_value).toZig() orelse return .invalid_value;
//     const sel_b = (b orelse return .invalid_value).toZig() orelse return .invalid_value;
//     const out = out_equal orelse return .invalid_value;
//     out.* = sel_a.eql(sel_b);
//     return .success;
// }
/// # Safety
/// `terminal`, if non-null, must be a live terminal handle; `a`/`b`/`out_equal`,
/// if non-null, must point to valid storage of their types.
pub unsafe fn equal(
    terminal: Terminal,
    a: *const CSelection,
    b: *const CSelection,
    out_equal: *mut bool,
) -> Result {
    // _ = terminal_c.zigTerminal(terminal) orelse return .invalid_value;
    let t = unsafe { crate::terminal::zig_terminal(terminal) };
    if t.is_null() {
        return Result::InvalidValue;
    }
    // const sel_a = (a orelse return .invalid_value).toZig() orelse return .invalid_value;
    if a.is_null() {
        return Result::InvalidValue;
    }
    let sel_a = match unsafe { (*a).to_zig() } {
        Some(s) => s,
        None => return Result::InvalidValue,
    };
    // const sel_b = (b orelse return .invalid_value).toZig() orelse return .invalid_value;
    if b.is_null() {
        return Result::InvalidValue;
    }
    let sel_b = match unsafe { (*b).to_zig() } {
        Some(s) => s,
        None => return Result::InvalidValue,
    };
    // const out = out_equal orelse return .invalid_value;
    if out_equal.is_null() {
        return Result::InvalidValue;
    }
    // out.* = sel_a.eql(sel_b);
    unsafe { *out_equal = sel_a.eql(sel_b) };
    // return .success;
    Result::Success
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid_ref::CGridRef;
    use crate::point::{PointC, PointCValue, TagC};
    use crate::terminal;
    use crate::test_util::with_test_allocator;
    use rustty_vt::point::Coordinate;

    // PORT NOTE: `point.Point.C` literals in the reference tests are built directly
    // as `.{ .tag = .active, .value = .{ .active = .{ .x, .y } } }`; the rustty
    // analog constructs `PointC { tag, value }` with the active union member.
    fn active_point(x: u16, y: u16) -> PointC {
        PointC {
            tag: TagC::Active,
            value: PointCValue {
                active: Coordinate { x, y: y as u32 },
            },
        }
    }

    // test "selection_format_alloc uses active selection"
    #[test]
    fn selection_format_alloc_uses_active_selection() {
        with_test_allocator(|alloc| {
            // var t: terminal_c.Terminal = null;
            let mut t: terminal::Terminal = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                terminal::new(
                    alloc,
                    &mut t,
                    terminal::Options {
                        cols: 80,
                        rows: 24,
                        max_scrollback: 10_000,
                    },
                )
            });
            // defer terminal_c.free(t);

            // terminal_c.vt_write(t, "Hello World", 11);
            let input = b"Hello World";
            unsafe { terminal::vt_write(t, input.as_ptr(), input.len()) };

            // var start_ref: grid_ref.CGridRef = .{};
            let mut start_ref = CGridRef::default();
            // try testing.expectEqual(Result.success, terminal_c.grid_ref(t, .{ .tag = .active, .value = .{ .active = .{ .x = 6, .y = 0 } } }, &start_ref));
            assert_eq!(Result::Success, unsafe {
                terminal::grid_ref(t, active_point(6, 0), &mut start_ref)
            });

            // var end_ref: grid_ref.CGridRef = .{};
            let mut end_ref = CGridRef::default();
            // try testing.expectEqual(Result.success, terminal_c.grid_ref(t, .{ ... x = 10, y = 0 ... }, &end_ref));
            assert_eq!(Result::Success, unsafe {
                terminal::grid_ref(t, active_point(10, 0), &mut end_ref)
            });

            // const sel: CSelection = .{ .start = start_ref, .end = end_ref };
            let sel = CSelection {
                start: start_ref,
                end: end_ref,
                ..CSelection::default()
            };
            // try testing.expectEqual(Result.success, terminal_c.set(t, .selection, @ptrCast(&sel)));
            assert_eq!(Result::Success, unsafe {
                terminal::set(
                    t,
                    terminal::Option_::Selection,
                    &sel as *const CSelection as *const std::ffi::c_void,
                )
            });

            // const opts: FormatOptions = .{ .emit = .plain, .unwrap = true, .trim = true };
            let opts = FormatOptions {
                emit: Format::Plain,
                unwrap: true,
                trim: true,
                ..format_options_default()
            };

            // var required: usize = 0;
            let mut required: usize = 0;
            // try testing.expectEqual(Result.out_of_space, format_buf(t, opts, null, 0, &required));
            assert_eq!(Result::OutOfSpace, unsafe {
                format_buf(t, opts, std::ptr::null_mut(), 0, &mut required)
            });
            // try testing.expectEqual(@as(usize, 5), required);
            assert_eq!(5usize, required);

            // var out_ptr: ?[*]u8 = null; var out_len: usize = 0;
            let mut out_ptr: *mut u8 = std::ptr::null_mut();
            let mut out_len: usize = 0;
            // try testing.expectEqual(Result.success, format_alloc(t, &test_allocator, opts, &out_ptr, &out_len));
            assert_eq!(Result::Success, unsafe {
                format_alloc(
                    t,
                    alloc as *const CAllocator,
                    opts,
                    &mut out_ptr,
                    &mut out_len,
                )
            });
            // const ptr = out_ptr orelse return error.TestExpectedEqual;
            assert!(!out_ptr.is_null());
            // defer test_allocator.free(ptr[0..out_len]);

            // try testing.expectEqualStrings("World", ptr[0..out_len]);
            let got = unsafe { core::slice::from_raw_parts(out_ptr, out_len) };
            assert_eq!(b"World", got);

            // Free the C-allocated output buffer through the same allocator.
            free_c_buf(alloc, out_ptr, out_len);
            unsafe { terminal::free(t) };
        });
    }

    // test "selection_format_buf uses provided selection"
    #[test]
    fn selection_format_buf_uses_provided_selection() {
        with_test_allocator(|alloc| {
            let mut t: terminal::Terminal = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                terminal::new(
                    alloc,
                    &mut t,
                    terminal::Options {
                        cols: 80,
                        rows: 24,
                        max_scrollback: 10_000,
                    },
                )
            });

            // terminal_c.vt_write(t, "Hello World", 11);
            let input = b"Hello World";
            unsafe { terminal::vt_write(t, input.as_ptr(), input.len()) };

            // var start_ref: grid_ref.CGridRef = .{};
            let mut start_ref = CGridRef::default();
            assert_eq!(Result::Success, unsafe {
                terminal::grid_ref(t, active_point(0, 0), &mut start_ref)
            });

            // var end_ref: grid_ref.CGridRef = .{};
            let mut end_ref = CGridRef::default();
            assert_eq!(Result::Success, unsafe {
                terminal::grid_ref(t, active_point(4, 0), &mut end_ref)
            });

            // const sel: CSelection = .{ .start = start_ref, .end = end_ref };
            let sel = CSelection {
                start: start_ref,
                end: end_ref,
                ..CSelection::default()
            };
            // const opts: FormatOptions = .{ .emit = .plain, .unwrap = true, .trim = true, .selection = &sel };
            let opts = FormatOptions {
                emit: Format::Plain,
                unwrap: true,
                trim: true,
                selection: &sel as *const CSelection,
                ..format_options_default()
            };

            // var small: [2]u8 = undefined; var written: usize = 0;
            let mut small = [0u8; 2];
            let mut written: usize = 0;
            // try testing.expectEqual(Result.out_of_space, format_buf(t, opts, &small, small.len, &written));
            assert_eq!(Result::OutOfSpace, unsafe {
                format_buf(t, opts, small.as_mut_ptr(), small.len(), &mut written)
            });
            // try testing.expectEqual(@as(usize, 5), written);
            assert_eq!(5usize, written);

            // var buf: [32]u8 = undefined;
            let mut buf = [0u8; 32];
            // try testing.expectEqual(Result.success, format_buf(t, opts, &buf, buf.len, &written));
            assert_eq!(Result::Success, unsafe {
                format_buf(t, opts, buf.as_mut_ptr(), buf.len(), &mut written)
            });
            // try testing.expectEqualStrings("Hello", buf[0..written]);
            assert_eq!(b"Hello", &buf[0..written]);

            unsafe { terminal::free(t) };
        });
    }

    // test "selection_format_alloc returns no_value without active selection"
    #[test]
    fn selection_format_alloc_returns_no_value_without_active_selection() {
        with_test_allocator(|alloc| {
            let mut t: terminal::Terminal = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                terminal::new(
                    alloc,
                    &mut t,
                    terminal::Options {
                        cols: 80,
                        rows: 24,
                        max_scrollback: 10_000,
                    },
                )
            });

            // var out_ptr: ?[*]u8 = @ptrFromInt(1); var out_len: usize = 123;
            // PORT NOTE: `@ptrFromInt(1)` is a deliberately non-null sentinel the
            // test asserts `format_alloc` overwrites to null; it is never
            // dereferenced. `with_exposed_provenance_mut` builds it without the
            // dangling-pointer lint firing.
            let mut out_ptr: *mut u8 = std::ptr::with_exposed_provenance_mut(1);
            let mut out_len: usize = 123;
            // try testing.expectEqual(Result.no_value, format_alloc(t, &test_allocator, .{ .emit = .plain, .unwrap = true, .trim = true }, &out_ptr, &out_len));
            let opts = FormatOptions {
                emit: Format::Plain,
                unwrap: true,
                trim: true,
                ..format_options_default()
            };
            assert_eq!(Result::NoValue, unsafe {
                format_alloc(
                    t,
                    alloc as *const CAllocator,
                    opts,
                    &mut out_ptr,
                    &mut out_len,
                )
            });
            // try testing.expect(out_ptr == null);
            assert!(out_ptr.is_null());
            // try testing.expectEqual(@as(usize, 0), out_len);
            assert_eq!(0usize, out_len);

            unsafe { terminal::free(t) };
        });
    }

    // PORT NOTE (Rust-only glue): the reference `FormatOptions` literals rely on
    // the `.size = @sizeOf(FormatOptions)` and `.selection = null` field defaults.
    // rustty's `#[repr(C)]` struct has no `Default`, so this helper supplies them.
    fn format_options_default() -> FormatOptions {
        FormatOptions {
            size: size_of::<FormatOptions>(),
            emit: Format::Plain,
            unwrap: false,
            trim: false,
            selection: std::ptr::null(),
        }
    }

    // PORT NOTE (Rust-only glue): the reference frees the `format_alloc` output via
    // `lib.alloc.default(&test_allocator).free(ptr[0..out_len])`. The rustty analog
    // frees the C-allocated buffer through the same resolved allocator.
    fn free_c_buf(alloc: &CAllocator, ptr: *mut u8, len: usize) {
        let a = rustty_lib::allocator::default(Some(alloc));
        a.free(
            rustty_lib::allocator::ZigSlice::new(ptr, len),
            rustty_lib::allocator::Alignment::from_byte_units(1),
            0,
        );
    }
}
