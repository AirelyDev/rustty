//! rustty-vt-c mouse_encode
//!
//! PORTS: ghostty src/terminal/c/mouse_encode.zig (the mouse-encoder C ABI
//! layer).
//!
//! The rustty-vt `input::mouse_encode::Options`/`encode` and `mouse::Event`/
//! `mouse::Format` are the source of truth (R3); this module is the FFI
//! projection. The `rustty_mouse_encoder_*` `extern "C"` wrappers live in
//! `lib.rs` and forward here.

// const std = @import("std");
// const Allocator = std.mem.Allocator;
// const testing = std.testing;
// const lib = @import("../lib.zig");
// const CAllocator = lib.alloc.Allocator;
// const input_mouse_encode = @import("../../input/mouse_encode.zig");
// const renderer_size = @import("../../renderer/size.zig");
// const point = @import("../point.zig");
// const terminal_mouse = @import("../mouse.zig");
// const mouse_event = @import("mouse_event.zig");
// const Result = @import("result.zig").Result;
// const Event = mouse_event.Event;
// const Terminal = @import("terminal.zig").Terminal;
// const ZigTerminal = @import("../Terminal.zig");
//
// const log = std.log.scoped(.mouse_encode);
//
// S174-F1: the setopt/setopt_typed warnings below used to bypass the C-ABI
// log sink with a raw `eprintln!`, which an embedder that registered a log
// callback via `rustty_sys_set(RUSTTY_SYS_OPT_LOG, ...)` never saw (they went
// to the process's stderr instead). Routed through `sys::log_fn` — the sink
// these warnings were always meant to reach — under this scope constant, the
// `@tagName` of the reference's `std.log.scoped(.mouse_encode)`.
pub(crate) const LOG_SCOPE: &str = "mouse_encode";
use crate::mouse_event::{self, Event};
use crate::result::Result;
use crate::terminal as terminal_c;
use crate::writer::{CountingWriter, FixedWriter};
use rustty_lib::allocator::Allocator as CAllocator;
use rustty_render_types as renderer_size;
use rustty_vt::input::mouse_encode as input_mouse_encode;
use rustty_vt::mouse as terminal_mouse;
use rustty_vt::point;
use std::ffi::c_void;
use std::os::raw::c_int;

use crate::cwrap::{create, destroy};

/// Wrapper around mouse encoding options that tracks the allocator for C API usage.
// PORT NOTE: the reference `MouseEncoderWrapper` stores `opts:
// input_mouse_encode.Options` plus `track_last_cell`, `last_cell`, and its own
// `Allocator`. rustty's `Options<'a>.last_cell` is a borrow (`Option<&mut
// Option<Coordinate>>`) that cannot be stored long-term, so the wrapper instead
// stores the scalar option fields (`event`, `format`, `size`,
// `any_button_pressed`) and rebuilds an `Options` (wiring `last_cell` into the
// wrapper's own cell) at each `encode` call. `alloc` is the original (possibly
// null) C `alloc_` (see `cwrap`).
pub struct MouseEncoderWrapper {
    // opts: input_mouse_encode.Options (scalar fields; `last_cell` is rebuilt)
    event: terminal_mouse::Event,
    format: terminal_mouse::Format,
    size: renderer_size::Size,
    any_button_pressed: bool,

    // track_last_cell: bool = false,
    track_last_cell: bool,
    // last_cell: ?point.Coordinate = null,
    last_cell: Option<point::Coordinate>,
    alloc: Option<CAllocator>,
}

/// C: RusttyMouseEncoder (R2)
// pub const Encoder = ?*MouseEncoderWrapper;
pub type Encoder = *mut MouseEncoderWrapper;

/// C: RusttyMouseTrackingMode (R2)
// pub const TrackingMode = terminal_mouse.Event;
// PORT NOTE: `terminal_mouse.Event = lib.Enum(...)`; for the `.c` target a
// fieldless `enum(c_int)`. rustty's source-of-truth `mouse::Event` is a plain
// (repr-Rust) enum, so the C ABI projection is a `#[repr(C)]` mirror with a
// `to_zig`/`from_zig` bridge (the discriminants follow the same key order:
// none, x10, normal, button, any).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrackingMode {
    None = 0,
    X10 = 1,
    Normal = 2,
    Button = 3,
    Any = 4,
}

impl TrackingMode {
    fn from_int(v: c_int) -> Option<TrackingMode> {
        match v {
            0 => Some(TrackingMode::None),
            1 => Some(TrackingMode::X10),
            2 => Some(TrackingMode::Normal),
            3 => Some(TrackingMode::Button),
            4 => Some(TrackingMode::Any),
            _ => None,
        }
    }

    fn to_zig(self) -> terminal_mouse::Event {
        match self {
            TrackingMode::None => terminal_mouse::Event::None,
            TrackingMode::X10 => terminal_mouse::Event::X10,
            TrackingMode::Normal => terminal_mouse::Event::Normal,
            TrackingMode::Button => terminal_mouse::Event::Button,
            TrackingMode::Any => terminal_mouse::Event::Any,
        }
    }
}

/// C: RusttyMouseFormat (R2)
// pub const Format = terminal_mouse.Format;
// PORT NOTE: the `enum(c_int)` projection of `mouse::Format` (key order:
// x10, utf8, sgr, urxvt, sgr_pixels), with a `to_zig`/`from_zig` bridge.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseFormat {
    X10 = 0,
    Utf8 = 1,
    Sgr = 2,
    Urxvt = 3,
    SgrPixels = 4,
}

impl MouseFormat {
    fn from_int(v: c_int) -> Option<MouseFormat> {
        match v {
            0 => Some(MouseFormat::X10),
            1 => Some(MouseFormat::Utf8),
            2 => Some(MouseFormat::Sgr),
            3 => Some(MouseFormat::Urxvt),
            4 => Some(MouseFormat::SgrPixels),
            _ => None,
        }
    }

    fn to_zig(self) -> terminal_mouse::Format {
        match self {
            MouseFormat::X10 => terminal_mouse::Format::X10,
            MouseFormat::Utf8 => terminal_mouse::Format::Utf8,
            MouseFormat::Sgr => terminal_mouse::Format::Sgr,
            MouseFormat::Urxvt => terminal_mouse::Format::Urxvt,
            MouseFormat::SgrPixels => terminal_mouse::Format::SgrPixels,
        }
    }
}

/// C: RusttyMouseEncoderSize (R2)
// pub const Size = extern struct {
//     size: usize = @sizeOf(Size),
//     screen_width, screen_height, cell_width, cell_height,
//     padding_top, padding_bottom, padding_right, padding_left: u32,
//     fn toRenderer(self: Size) ?renderer_size.Size { ... }
// };
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Size {
    pub size: usize,
    pub screen_width: u32,
    pub screen_height: u32,
    pub cell_width: u32,
    pub cell_height: u32,
    pub padding_top: u32,
    pub padding_bottom: u32,
    pub padding_right: u32,
    pub padding_left: u32,
}

impl Default for Size {
    // size: usize = @sizeOf(Size),
    fn default() -> Self {
        Size {
            size: std::mem::size_of::<Size>(),
            screen_width: 0,
            screen_height: 0,
            cell_width: 0,
            cell_height: 0,
            padding_top: 0,
            padding_bottom: 0,
            padding_right: 0,
            padding_left: 0,
        }
    }
}

impl Size {
    // fn toRenderer(self: Size) ?renderer_size.Size {
    //     if (self.cell_width == 0 or self.cell_height == 0) return null;
    //     return .{ .screen = .{ ... }, .cell = .{ ... }, .padding = .{ ... } };
    // }
    fn to_renderer(self) -> Option<renderer_size::Size> {
        // if (self.cell_width == 0 or self.cell_height == 0) return null;
        if self.cell_width == 0 || self.cell_height == 0 {
            return None;
        }
        Some(renderer_size::Size {
            screen: renderer_size::ScreenSize {
                width: self.screen_width,
                height: self.screen_height,
            },
            cell: renderer_size::CellSize {
                width: self.cell_width,
                height: self.cell_height,
            },
            padding: renderer_size::Padding {
                top: self.padding_top,
                bottom: self.padding_bottom,
                right: self.padding_right,
                left: self.padding_left,
            },
        })
    }
}

/// C: RusttyMouseEncoderOption (R2)
// pub const Option = enum(c_int) {
//     event = 0, format = 1, size = 2, any_button_pressed = 3, track_last_cell = 4,
//     pub fn InType(comptime self: Option) type { ... }
// };
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Option_ {
    Event = 0,
    Format = 1,
    Size = 2,
    AnyButtonPressed = 3,
    TrackLastCell = 4,
    // PORT NOTE: `InType` is the comptime type map; the per-option typed reads
    // are inlined into `setopt` below.
}

// pub fn new(alloc_: ?*const CAllocator, result: *Encoder) ... Result {
//     const alloc = lib.alloc.default(alloc_);
//     const ptr = alloc.create(MouseEncoderWrapper) catch return .out_of_memory;
//     ptr.* = .{ .opts = .{ .size = defaultSize() }, .alloc = alloc };
//     result.* = ptr;
//     return .success;
// }
//
/// # Safety
/// `alloc_`, if non-null, must point to a valid `CAllocator`; `result` must be
/// writable storage for one `Encoder`.
pub unsafe fn new(alloc_: Option<&CAllocator>, result: *mut Encoder) -> Result {
    // const ptr = alloc.create(MouseEncoderWrapper) catch return .out_of_memory;
    // ptr.* = .{ .opts = .{ .size = defaultSize() }, .alloc = alloc };
    let ptr = unsafe {
        create(
            alloc_,
            MouseEncoderWrapper {
                // .opts = .{ .size = defaultSize() } — the remaining `Options`
                // fields ride their defaults (event=.none, format=.x10,
                // any_button_pressed=false, last_cell=null).
                event: terminal_mouse::Event::None,
                format: terminal_mouse::Format::X10,
                size: default_size(),
                any_button_pressed: false,
                track_last_cell: false,
                last_cell: None,
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

// pub fn free(encoder_: Encoder) ... void {
//     const wrapper = encoder_ orelse return;
//     const alloc = wrapper.alloc;
//     alloc.destroy(wrapper);
// }
//
/// # Safety
/// `encoder_`, if non-null, must be a handle returned by `new`, not yet freed.
pub unsafe fn free(encoder_: Encoder) {
    // const wrapper = encoder_ orelse return;
    if encoder_.is_null() {
        return;
    }
    // const alloc = wrapper.alloc;
    // SAFETY: `encoder_` is a live handle (caller contract).
    let alloc = unsafe { (*encoder_).alloc };
    // alloc.destroy(wrapper);
    // SAFETY: `encoder_` was produced by `create` with the same `alloc_`.
    unsafe { destroy(alloc.as_ref(), encoder_) };
}

// pub fn setopt(encoder_: Encoder, option: Option, value: ?*const anyopaque) ... void {
//     if (comptime std.debug.runtime_safety) {
//         _ = std.meta.intToEnum(Option, @intFromEnum(option)) catch {
//             log.warn("setopt invalid option value={d}", .{@intFromEnum(option)});
//             return;
//         };
//     }
//     return switch (option) {
//         inline else => |comptime_option| setoptTyped(
//             encoder_, comptime_option, @ptrCast(@alignCast(value orelse return)),
//         ),
//     };
// }
//
/// # Safety
/// `encoder_`, if non-null, must be a `new` handle; `value`, if non-null, must
/// point to a readable value of the option's in-type.
pub unsafe fn setopt(encoder_: Encoder, option: c_int, value: *const c_void) {
    // if (comptime std.debug.runtime_safety) {
    //     _ = std.meta.intToEnum(Option, ...) catch { log.warn(...); return; };
    // }
    let option = match option {
        0 => Option_::Event,
        1 => Option_::Format,
        2 => Option_::Size,
        3 => Option_::AnyButtonPressed,
        4 => Option_::TrackLastCell,
        _ => {
            // log.warn("setopt invalid option value={d}", .{@intFromEnum(option)});
            crate::sys::log_fn(
                crate::sys::StdLevel::Warn,
                LOG_SCOPE,
                format_args!("setopt invalid option value={option}"),
            );
            return;
        }
    };

    // inline else => setoptTyped(encoder_, comptime_option, value orelse return)
    if value.is_null() {
        return;
    }
    // SAFETY: `encoder_`/`value` validity is the caller's contract.
    unsafe { setopt_typed(encoder_, option, value) };
}

// fn setoptTyped(encoder_: Encoder, comptime option: Option, value: *const option.InType()) void {
//     const wrapper = encoder_.?;
//     switch (option) { ... }
// }
//
/// # Safety
/// `encoder_` must be non-null (`encoder_.?`); `value` must point to a readable
/// value of `option`'s in-type.
unsafe fn setopt_typed(encoder_: Encoder, option: Option_, value: *const c_void) {
    // const wrapper = encoder_.?;
    // SAFETY: `encoder_` is non-null per the caller contract (`.?`).
    let wrapper = unsafe { &mut *encoder_ };
    match option {
        // .event => { ...guard...; if (wrapper.opts.event != value.*) wrapper.last_cell = null;
        //             wrapper.opts.event = value.*; },
        Option_::Event => {
            // _ = std.meta.intToEnum(TrackingMode, @intFromEnum(value.*)) catch {
            //     log.warn("setopt invalid TrackingMode value={d}", ...); return; };
            let raw: c_int = unsafe { *(value as *const c_int) };
            let mode = match TrackingMode::from_int(raw) {
                Some(m) => m.to_zig(),
                None => {
                    crate::sys::log_fn(
                        crate::sys::StdLevel::Warn,
                        LOG_SCOPE,
                        format_args!("setopt invalid TrackingMode value={raw}"),
                    );
                    return;
                }
            };
            // if (wrapper.opts.event != value.*) wrapper.last_cell = null;
            if wrapper.event != mode {
                wrapper.last_cell = None;
            }
            // wrapper.opts.event = value.*;
            wrapper.event = mode;
        }

        // .format => { ...guard...; if (wrapper.opts.format != value.*) wrapper.last_cell = null;
        //              wrapper.opts.format = value.*; },
        Option_::Format => {
            let raw: c_int = unsafe { *(value as *const c_int) };
            let fmt = match MouseFormat::from_int(raw) {
                Some(f) => f.to_zig(),
                None => {
                    crate::sys::log_fn(
                        crate::sys::StdLevel::Warn,
                        LOG_SCOPE,
                        format_args!("setopt invalid Format value={raw}"),
                    );
                    return;
                }
            };
            // if (wrapper.opts.format != value.*) wrapper.last_cell = null;
            if wrapper.format != fmt {
                wrapper.last_cell = None;
            }
            // wrapper.opts.format = value.*;
            wrapper.format = fmt;
        }

        // .size => {
        //     if (value.size < @sizeOf(Size)) { log.warn(...); return; }
        //     wrapper.opts.size = value.toRenderer() orelse { log.warn(...); return; };
        //     wrapper.last_cell = null;
        // },
        Option_::Size => {
            let value: &Size = unsafe { &*(value as *const Size) };
            // if (value.size < @sizeOf(Size)) { ...; return; }
            if value.size < std::mem::size_of::<Size>() {
                crate::sys::log_fn(
                    crate::sys::StdLevel::Warn,
                    LOG_SCOPE,
                    format_args!(
                        "setopt size struct too small size={} expected>={}",
                        value.size,
                        std::mem::size_of::<Size>()
                    ),
                );
                return;
            }
            // wrapper.opts.size = value.toRenderer() orelse { ...; return; };
            match value.to_renderer() {
                Some(s) => wrapper.size = s,
                None => {
                    crate::sys::log_fn(
                        crate::sys::StdLevel::Warn,
                        LOG_SCOPE,
                        format_args!(
                            "setopt invalid size values (cell width and height must be non-zero)"
                        ),
                    );
                    return;
                }
            }
            // wrapper.last_cell = null;
            wrapper.last_cell = None;
        }

        // .any_button_pressed => wrapper.opts.any_button_pressed = value.*,
        Option_::AnyButtonPressed => {
            wrapper.any_button_pressed = unsafe { *(value as *const bool) };
        }

        // .track_last_cell => {
        //     wrapper.track_last_cell = value.*;
        //     if (!value.*) wrapper.last_cell = null;
        // },
        Option_::TrackLastCell => {
            let v: bool = unsafe { *(value as *const bool) };
            wrapper.track_last_cell = v;
            if !v {
                wrapper.last_cell = None;
            }
        }
    }
}

// pub fn setopt_from_terminal(encoder_: Encoder, terminal_: Terminal) ... void {
//     const wrapper = encoder_ orelse return;
//     const t: *ZigTerminal = (terminal_ orelse return).terminal;
//     wrapper.opts.event = t.flags.mouse_event;
//     wrapper.opts.format = t.flags.mouse_format;
//     wrapper.last_cell = null;
// }
//
/// # Safety
/// `encoder_`/`terminal_`, if non-null, must be live handles.
pub unsafe fn setopt_from_terminal(encoder_: Encoder, terminal_: terminal_c::Terminal) {
    // const wrapper = encoder_ orelse return;
    if encoder_.is_null() {
        return;
    }
    // const t: *ZigTerminal = (terminal_ orelse return).terminal;
    if terminal_.is_null() {
        return;
    }
    // SAFETY: `terminal_` is a live handle (caller contract; non-null checked).
    let t = unsafe { &*terminal_c::zig_terminal(terminal_) };
    // SAFETY: `encoder_` is a live handle (caller contract; non-null checked).
    let wrapper = unsafe { &mut *encoder_ };
    // wrapper.opts.event = t.flags.mouse_event;
    wrapper.event = t.flags.mouse_event;
    // wrapper.opts.format = t.flags.mouse_format;
    wrapper.format = t.flags.mouse_format;
    // wrapper.last_cell = null;
    wrapper.last_cell = None;
}

// pub fn reset(encoder_: Encoder) ... void {
//     const wrapper = encoder_ orelse return;
//     wrapper.last_cell = null;
// }
//
/// # Safety
/// `encoder_`, if non-null, must be a `new` handle.
pub unsafe fn reset(encoder_: Encoder) {
    // const wrapper = encoder_ orelse return;
    if encoder_.is_null() {
        return;
    }
    // wrapper.last_cell = null;
    // SAFETY: `encoder_` is a live handle (caller contract).
    unsafe { (*encoder_).last_cell = None };
}

// pub fn encode(encoder_, event_, out_, out_len, out_written) ... Result {
//     const wrapper = encoder_ orelse return .invalid_value;
//     const event = event_ orelse return .invalid_value;
//     const prev_last_cell = wrapper.last_cell;
//     var opts = wrapper.opts;
//     opts.last_cell = if (wrapper.track_last_cell) &wrapper.last_cell else null;
//     var writer: std.Io.Writer = .fixed(if (out_) |out| out[0..out_len] else &.{});
//     input_mouse_encode.encode(&writer, event.event, opts) catch |err| switch (err) {
//         error.WriteFailed => {
//             wrapper.last_cell = prev_last_cell;
//             var count_last_cell = prev_last_cell;
//             var count_opts = wrapper.opts;
//             count_opts.last_cell = if (wrapper.track_last_cell) &count_last_cell else null;
//             var discarding: std.Io.Writer.Discarding = .init(&.{});
//             input_mouse_encode.encode(&discarding.writer, event.event, count_opts) catch unreachable;
//             out_written.* = @intCast(discarding.count);
//             return .out_of_space;
//         },
//     };
//     out_written.* = writer.end;
//     return .success;
// }
//
/// # Safety
/// `encoder_`/`event_` must be `new` handles; `out_` (if non-null) must point to
/// `out_len` writable bytes; `out_written` must be writable.
pub unsafe fn encode(
    encoder_: Encoder,
    event_: Event,
    out_: *mut u8,
    out_len: usize,
    out_written: *mut usize,
) -> Result {
    // const wrapper = encoder_ orelse return .invalid_value;
    if encoder_.is_null() {
        return Result::InvalidValue;
    }
    // const event = event_ orelse return .invalid_value;
    if event_.is_null() {
        return Result::InvalidValue;
    }
    // SAFETY: `encoder_` is a live handle (caller contract; non-null checked).
    let wrapper = unsafe { &mut *encoder_ };
    // SAFETY: `event_` is a live handle (caller contract; non-null checked).
    let event = unsafe { mouse_event::event_value(event_) };

    // const prev_last_cell = wrapper.last_cell;
    let prev_last_cell = wrapper.last_cell;

    // var opts = wrapper.opts;
    // opts.last_cell = if (wrapper.track_last_cell) &wrapper.last_cell else null;
    // PORT NOTE: rebuild `Options`, wiring `last_cell` into the wrapper's own
    // cell only when tracking is on (the reference's `&wrapper.last_cell`). The
    // borrow is constructed here (it cannot be stored long-term in the wrapper).
    let event_mode = wrapper.event;
    let format = wrapper.format;
    let size = wrapper.size;
    let any_button_pressed = wrapper.any_button_pressed;
    let track_last_cell = wrapper.track_last_cell;

    // var writer: std.Io.Writer = .fixed(if (out_) |out| out[0..out_len] else &.{});
    let mut empty: [u8; 0] = [];
    let slice: &mut [u8] = if out_.is_null() {
        &mut empty
    } else {
        // SAFETY: `out_` points to `out_len` writable bytes (caller contract).
        unsafe { std::slice::from_raw_parts_mut(out_, out_len) }
    };
    let mut writer = FixedWriter::new(slice);

    // input_mouse_encode.encode(&writer, event.event, opts) catch |err| switch (err) {
    let write_result = {
        let opts = input_mouse_encode::Options {
            event: event_mode,
            format,
            size,
            any_button_pressed,
            last_cell: if track_last_cell {
                Some(&mut wrapper.last_cell)
            } else {
                None
            },
        };
        input_mouse_encode::encode(&mut writer, event, opts)
    };

    if write_result.is_err() {
        // error.WriteFailed:
        // Failed writes should not mutate motion dedupe state because no
        // complete sequence was produced.
        // wrapper.last_cell = prev_last_cell;
        wrapper.last_cell = prev_last_cell;

        // Use a discarding writer to count how much space we would have needed.
        // var count_last_cell = prev_last_cell;
        let mut count_last_cell = prev_last_cell;
        // var count_opts = wrapper.opts;
        // count_opts.last_cell = if (wrapper.track_last_cell) &count_last_cell else null;
        // var discarding: std.Io.Writer.Discarding = .init(&.{});
        let mut discarding = CountingWriter::new();
        // input_mouse_encode.encode(&discarding.writer, event.event, count_opts) catch unreachable;
        let count_opts = input_mouse_encode::Options {
            event: event_mode,
            format,
            size,
            any_button_pressed,
            last_cell: if track_last_cell {
                Some(&mut count_last_cell)
            } else {
                None
            },
        };
        input_mouse_encode::encode(&mut discarding, event, count_opts).unwrap();

        // Discarding always uses a u64. If we're on 32-bit systems
        // we cast down. We should make this safer in the future.
        // out_written.* = @intCast(discarding.count);
        // SAFETY: `out_written` is writable (caller contract).
        unsafe { *out_written = discarding.count };
        // return .out_of_space;
        return Result::OutOfSpace;
    }

    // out_written.* = writer.end;
    // SAFETY: `out_written` is writable (caller contract).
    unsafe { *out_written = writer.end };
    // return .success;
    Result::Success
}

// fn defaultSize() renderer_size.Size {
//     return .{ .screen = .{ .width = 1, .height = 1 }, .cell = .{ .width = 1, .height = 1 },
//               .padding = .{} };
// }
fn default_size() -> renderer_size::Size {
    renderer_size::Size {
        screen: renderer_size::ScreenSize {
            width: 1,
            height: 1,
        },
        cell: renderer_size::CellSize {
            width: 1,
            height: 1,
        },
        // .padding = .{}  (all zero)
        padding: renderer_size::Padding {
            top: 0,
            bottom: 0,
            right: 0,
            left: 0,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::with_test_allocator;

    // fn testSize() Size {
    //     return .{ .size = @sizeOf(Size), .screen_width = 1_000, .screen_height = 1_000,
    //               .cell_width = 1, .cell_height = 1, .padding_* = 0 };
    // }
    fn test_size() -> Size {
        Size {
            screen_width: 1_000,
            screen_height: 1_000,
            cell_width: 1,
            cell_height: 1,
            ..Size::default()
        }
    }

    // PORT NOTE: setopt takes the option as the raw `c_int` discriminant (the
    // lib.rs `extern "C"` wrapper guards the range); the tests pass it directly.
    unsafe fn setopt_v<T>(e: Encoder, option: Option_, v: &T) {
        unsafe { setopt(e, option as c_int, v as *const T as *const c_void) };
    }

    // test "alloc" {
    #[test]
    fn alloc() {
        with_test_allocator(|calloc| {
            // var e: Encoder = undefined;
            let mut e: Encoder = std::ptr::null_mut();
            // try testing.expectEqual(Result.success, new(&lib.alloc.test_allocator, &e));
            assert_eq!(Result::Success, unsafe { new(Some(calloc), &mut e) });
            // free(e);
            unsafe { free(e) };
        });
    }

    // test "setopt" {
    #[test]
    fn setopt_test() {
        with_test_allocator(|calloc| {
            let mut e: Encoder = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe { new(Some(calloc), &mut e) });

            // const event_mode: TrackingMode = .any;
            let event_mode = TrackingMode::Any;
            // setopt(e, .event, &event_mode);
            unsafe { setopt_v(e, Option_::Event, &event_mode) };
            // try testing.expectEqual(TrackingMode.any, e.?.opts.event);
            // PORT NOTE: the reference's `TrackingMode.any` IS `mouse.Event.any`;
            // the rustty assertion compares the source-of-truth `mouse::Event`.
            assert_eq!(terminal_mouse::Event::Any, unsafe { (*e).event });

            // const format_mode: Format = .sgr;
            let format_mode = MouseFormat::Sgr;
            // setopt(e, .format, &format_mode);
            unsafe { setopt_v(e, Option_::Format, &format_mode) };
            // try testing.expectEqual(Format.sgr, e.?.opts.format);
            assert_eq!(terminal_mouse::Format::Sgr, unsafe { (*e).format });

            // const size = testSize();
            let size = test_size();
            // setopt(e, .size, &size);
            unsafe { setopt_v(e, Option_::Size, &size) };
            // try testing.expectEqual(size.screen_width, e.?.opts.size.screen.width);
            assert_eq!(size.screen_width, unsafe { (*e).size.screen.width });
            // try testing.expectEqual(size.screen_height, e.?.opts.size.screen.height);
            assert_eq!(size.screen_height, unsafe { (*e).size.screen.height });

            // const any_button_pressed = true;
            let any_button_pressed = true;
            // setopt(e, .any_button_pressed, &any_button_pressed);
            unsafe { setopt_v(e, Option_::AnyButtonPressed, &any_button_pressed) };
            // try testing.expect(e.?.opts.any_button_pressed);
            assert!(unsafe { (*e).any_button_pressed });

            // const track_last_cell = true;
            let track_last_cell = true;
            // setopt(e, .track_last_cell, &track_last_cell);
            unsafe { setopt_v(e, Option_::TrackLastCell, &track_last_cell) };
            // try testing.expect(e.?.track_last_cell);
            assert!(unsafe { (*e).track_last_cell });

            unsafe { free(e) };
        });
    }

    // test "setopt_from_terminal" {
    #[test]
    fn setopt_from_terminal_test() {
        with_test_allocator(|calloc| {
            let mut e: Encoder = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe { new(Some(calloc), &mut e) });

            let mut t: terminal_c::Terminal = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                terminal_c::new(calloc, &mut t, 80, 24)
            });

            // const event_mode: TrackingMode = .any;
            let event_mode = TrackingMode::Any;
            // setopt(e, .event, &event_mode);
            unsafe { setopt_v(e, Option_::Event, &event_mode) };
            // const format_mode: Format = .sgr;
            let format_mode = MouseFormat::Sgr;
            // setopt(e, .format, &format_mode);
            unsafe { setopt_v(e, Option_::Format, &format_mode) };

            // setopt_from_terminal(e, t);
            unsafe { setopt_from_terminal(e, t) };
            // try testing.expectEqual(TrackingMode.none, e.?.opts.event);
            assert_eq!(terminal_mouse::Event::None, unsafe { (*e).event });
            // try testing.expectEqual(Format.x10, e.?.opts.format);
            assert_eq!(terminal_mouse::Format::X10, unsafe { (*e).format });

            unsafe { terminal_c::free(t) };
            unsafe { free(e) };
        });
    }

    // test "setopt_from_terminal null" {
    #[test]
    fn setopt_from_terminal_null() {
        // setopt_from_terminal(null, null);
        unsafe { setopt_from_terminal(std::ptr::null_mut(), std::ptr::null_mut()) };

        with_test_allocator(|calloc| {
            let mut t: terminal_c::Terminal = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                terminal_c::new(calloc, &mut t, 80, 24)
            });
            // setopt_from_terminal(null, t);
            unsafe { setopt_from_terminal(std::ptr::null_mut(), t) };

            let mut e: Encoder = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe { new(Some(calloc), &mut e) });
            // setopt_from_terminal(e, null);
            unsafe { setopt_from_terminal(e, std::ptr::null_mut()) };

            unsafe { free(e) };
            unsafe { terminal_c::free(t) };
        });
    }

    // PORT NOTE: shared encoder setup for the encode tests — sgr/any with the
    // test size, returning the configured encoder WITH its unasserted `new`
    // Result. Mirrors the repeated setopt(event/format/size) preamble; the
    // preamble's `Result.success` expectEqual ASSERTION lives in each test body
    // (assertion-for-assertion; G2 counts source-level asserts per test).
    fn make_sgr_any_encoder(calloc: &CAllocator) -> (Result, Encoder) {
        let mut encoder: Encoder = std::ptr::null_mut();
        let new_result = unsafe { new(Some(calloc), &mut encoder) };
        let event_mode = TrackingMode::Any;
        unsafe { setopt_v(encoder, Option_::Event, &event_mode) };
        let format_mode = MouseFormat::Sgr;
        unsafe { setopt_v(encoder, Option_::Format, &format_mode) };
        let size = test_size();
        unsafe { setopt_v(encoder, Option_::Size, &size) };
        (new_result, encoder)
    }

    // rustty-only (R13, UPSTREAM-DEFECTS.md UD60): a UTF-8 (mode 1005) mouse
    // coordinate whose `+ 33` is a surrogate cannot be encoded. The reference
    // reaches `catch unreachable`; the port does not report the event, and writes
    // nothing. Driven through the C ABI with a 60000-column grid.
    #[test]
    fn encode_utf8_surrogate_coordinate_is_not_reported() {
        with_test_allocator(|calloc| {
            let mut encoder: Encoder = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe { new(Some(calloc), &mut encoder) });
            let event_mode = TrackingMode::Any;
            unsafe { setopt_v(encoder, Option_::Event, &event_mode) };
            let size = Size {
                screen_width: 60_000,
                screen_height: 60_000,
                cell_width: 1,
                cell_height: 1,
                ..Size::default()
            };
            unsafe { setopt_v(encoder, Option_::Size, &size) };

            let mut event: Event = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                mouse_event::new(Some(calloc), &mut event)
            });
            unsafe { mouse_event::set_action(event, mouse_event::Action::Press) };
            unsafe { mouse_event::set_button(event, mouse_event::Button::Left) };

            let encode_at = |format: MouseFormat, x: f64, y: f64| -> (Result, Vec<u8>) {
                unsafe { setopt_v(encoder, Option_::Format, &format) };
                unsafe {
                    mouse_event::set_position(
                        event,
                        mouse_event::Position {
                            x: x as _,
                            y: y as _,
                        },
                    )
                };
                let mut buf = [0u8; 32];
                let mut written: usize = 0;
                let r =
                    unsafe { encode(encoder, event, buf.as_mut_ptr(), buf.len(), &mut written) };
                (r, buf[..written].to_vec())
            };

            // Positive reach: SGR reports the very cells the UTF-8 cases use, so
            // the grid is wide and tall enough and the press is reported.
            assert_eq!(
                (Result::Success, b"\x1b[<0;55264;1M".to_vec()),
                encode_at(MouseFormat::Sgr, 55_263.0, 0.0)
            );
            assert_eq!(
                (Result::Success, b"\x1b[<0;1;57311M".to_vec()),
                encode_at(MouseFormat::Sgr, 0.0, 57_310.0)
            );

            // The last and first encodable code points around the surrogates.
            // x = 55262 -> U+D7FF; x = 57311 -> U+E000.
            assert_eq!(
                (Result::Success, b"\x1b[M \xed\x9f\xbf!".to_vec()),
                encode_at(MouseFormat::Utf8, 55_262.0, 0.0)
            );
            assert_eq!(
                (Result::Success, b"\x1b[M \xee\x80\x80!".to_vec()),
                encode_at(MouseFormat::Utf8, 57_311.0, 0.0)
            );

            // Surrogates, in each coordinate and at both ends of the range: not
            // reported, nothing written, no partial `ESC [ M`.
            for (x, y) in [
                (55_263.0, 0.0),
                (57_310.0, 0.0),
                (0.0, 55_263.0),
                (0.0, 57_310.0),
            ] {
                assert_eq!(
                    (Result::Success, Vec::new()),
                    encode_at(MouseFormat::Utf8, x, y),
                    "UTF-8 mouse report at cell ({x}, {y})"
                );
            }

            unsafe { mouse_event::free(event) };
            unsafe { free(encoder) };
        });
    }

    // test "encode: sgr press left" {
    #[test]
    fn encode_sgr_press_left() {
        with_test_allocator(|calloc| {
            let (encoder_new_result, encoder) = make_sgr_any_encoder(calloc);
            // try testing.expectEqual(Result.success, new(&test_allocator, &encoder));
            assert_eq!(Result::Success, encoder_new_result);

            let mut event: Event = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                mouse_event::new(Some(calloc), &mut event)
            });

            // mouse_event.set_action(event, .press);
            unsafe { mouse_event::set_action(event, mouse_event::Action::Press) };
            // mouse_event.set_button(event, .left);
            unsafe { mouse_event::set_button(event, mouse_event::Button::Left) };
            // mouse_event.set_position(event, .{ .x = 0, .y = 0 });
            unsafe { mouse_event::set_position(event, mouse_event::Position { x: 0.0, y: 0.0 }) };

            // var required: usize = 0;
            let mut required: usize = 0;
            // try testing.expectEqual(Result.out_of_space, encode(encoder, event, null, 0, &required));
            assert_eq!(Result::OutOfSpace, unsafe {
                encode(encoder, event, std::ptr::null_mut(), 0, &mut required)
            });
            // try testing.expectEqual(@as(usize, 9), required);
            assert_eq!(9usize, required);

            // var buf: [32]u8 = undefined; var written: usize = 0;
            let mut buf = [0u8; 32];
            let mut written: usize = 0;
            assert_eq!(Result::Success, unsafe {
                encode(encoder, event, buf.as_mut_ptr(), buf.len(), &mut written)
            });
            // try testing.expectEqual(required, written);
            assert_eq!(required, written);
            // try testing.expectEqualStrings("\x1b[<0;1;1M", buf[0..written]);
            assert_eq!(b"\x1b[<0;1;1M", &buf[0..written]);

            unsafe { mouse_event::free(event) };
            unsafe { free(encoder) };
        });
    }

    // W81-B01-01 (Rust-only proving test; the reference has none — it inherits
    // the behavior from `std.Io.Writer.fixed`, which makes the CALLER buffer
    // its own internal buffer and therefore leaves the fitting PREFIX behind
    // before reporting `error.WriteFailed`). The shared `writer::FixedWriter`
    // used to reject an overflowing write whole, so `out_of_space` left the
    // caller buffer untouched here too (W82-B04/W82-B05 dedupe to the same
    // root).
    #[test]
    fn encode_out_of_space_leaves_the_fitting_prefix_in_the_caller_buffer() {
        with_test_allocator(|calloc| {
            let (encoder_new_result, encoder) = make_sgr_any_encoder(calloc);
            assert_eq!(Result::Success, encoder_new_result);

            let mut event: Event = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                mouse_event::new(Some(calloc), &mut event)
            });
            unsafe { mouse_event::set_action(event, mouse_event::Action::Press) };
            unsafe { mouse_event::set_button(event, mouse_event::Button::Left) };
            unsafe { mouse_event::set_position(event, mouse_event::Position { x: 0.0, y: 0.0 }) };

            // Full encoding is "\x1b[<0;1;1M" (9 bytes); the CSI introducer is
            // written as one chunk, so a 2-byte buffer overflows on the very
            // first write — the shape the whole-write rejection hid.
            let mut buf = *b"XY";
            let mut written: usize = 0;
            assert_eq!(Result::OutOfSpace, unsafe {
                encode(encoder, event, buf.as_mut_ptr(), buf.len(), &mut written)
            });
            // The required length is still reported in full.
            assert_eq!(9usize, written);
            // ...and the prefix that fit is in the caller buffer.
            assert_eq!(b"\x1b[", &buf);

            unsafe { mouse_event::free(event) };
            unsafe { free(encoder) };
        });
    }

    // test "encode: motion dedupe and reset" {
    #[test]
    fn encode_motion_dedupe_and_reset() {
        with_test_allocator(|calloc| {
            let (encoder_new_result, encoder) = make_sgr_any_encoder(calloc);
            // try testing.expectEqual(Result.success, new(&test_allocator, &encoder));
            assert_eq!(Result::Success, encoder_new_result);
            // const track_last_cell = true;
            let track_last_cell = true;
            // setopt(encoder, .track_last_cell, &track_last_cell);
            unsafe { setopt_v(encoder, Option_::TrackLastCell, &track_last_cell) };

            let mut event: Event = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                mouse_event::new(Some(calloc), &mut event)
            });

            // mouse_event.set_action(event, .motion);
            unsafe { mouse_event::set_action(event, mouse_event::Action::Motion) };
            // mouse_event.set_button(event, .left);
            unsafe { mouse_event::set_button(event, mouse_event::Button::Left) };
            // mouse_event.set_position(event, .{ .x = 5, .y = 6 });
            unsafe { mouse_event::set_position(event, mouse_event::Position { x: 5.0, y: 6.0 }) };

            {
                let mut buf = [0u8; 32];
                let mut written: usize = 0;
                assert_eq!(Result::Success, unsafe {
                    encode(encoder, event, buf.as_mut_ptr(), buf.len(), &mut written)
                });
                // try testing.expect(written > 0);
                assert!(written > 0);
            }

            {
                let mut buf = [0u8; 32];
                let mut written: usize = 0;
                assert_eq!(Result::Success, unsafe {
                    encode(encoder, event, buf.as_mut_ptr(), buf.len(), &mut written)
                });
                // try testing.expectEqual(@as(usize, 0), written);
                assert_eq!(0usize, written);
            }

            // reset(encoder);
            unsafe { reset(encoder) };

            {
                let mut buf = [0u8; 32];
                let mut written: usize = 0;
                assert_eq!(Result::Success, unsafe {
                    encode(encoder, event, buf.as_mut_ptr(), buf.len(), &mut written)
                });
                // try testing.expect(written > 0);
                assert!(written > 0);
            }

            unsafe { mouse_event::free(event) };
            unsafe { free(encoder) };
        });
    }

    // test "encode: querying required size doesn't update dedupe state" {
    #[test]
    fn encode_querying_required_size_doesnt_update_dedupe_state() {
        with_test_allocator(|calloc| {
            let (encoder_new_result, encoder) = make_sgr_any_encoder(calloc);
            // try testing.expectEqual(Result.success, new(&test_allocator, &encoder));
            assert_eq!(Result::Success, encoder_new_result);
            let track_last_cell = true;
            unsafe { setopt_v(encoder, Option_::TrackLastCell, &track_last_cell) };

            let mut event: Event = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                mouse_event::new(Some(calloc), &mut event)
            });

            unsafe { mouse_event::set_action(event, mouse_event::Action::Motion) };
            unsafe { mouse_event::set_button(event, mouse_event::Button::Left) };
            unsafe { mouse_event::set_position(event, mouse_event::Position { x: 5.0, y: 6.0 }) };

            // var required: usize = 0;
            let mut required: usize = 0;
            // try testing.expectEqual(Result.out_of_space, encode(encoder, event, null, 0, &required));
            assert_eq!(Result::OutOfSpace, unsafe {
                encode(encoder, event, std::ptr::null_mut(), 0, &mut required)
            });
            // try testing.expect(required > 0);
            assert!(required > 0);

            // var buf: [32]u8 = undefined; var written: usize = 0;
            let mut buf = [0u8; 32];
            let mut written: usize = 0;
            assert_eq!(Result::Success, unsafe {
                encode(encoder, event, buf.as_mut_ptr(), buf.len(), &mut written)
            });
            // try testing.expect(written > 0);
            assert!(written > 0);

            unsafe { mouse_event::free(event) };
            unsafe { free(encoder) };
        });
    }

    // task-14a (rustty-only proving test, NOT a reference test — S174-F1):
    // the five setopt/setopt_typed invalid-value diagnostics used to bypass
    // `sys::log_fn` with a raw `eprintln!` to the process's stderr, which an
    // embedder that installed a log callback via
    // `rustty_sys_set(RUSTTY_SYS_OPT_LOG, ...)` never received. Installs a
    // capturing callback (the established pattern from
    // `terminal.rs::terminal_invalid_selector_warns_through_the_scoped_log_sink`)
    // and drives every one of the five sites, asserting each message actually
    // reaches the sink under the `mouse_encode` scope.
    #[test]
    fn setopt_invalid_value_warnings_reach_the_scoped_log_sink() {
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

        with_test_allocator(|calloc| {
            let mut e: Encoder = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe { new(Some(calloc), &mut e) });

            assert_eq!(Result::Success, unsafe {
                crate::sys::set(crate::sys::Option_::Log, log_cb as *const c_void)
            });

            // setopt: invalid option discriminant.
            unsafe { setopt(e, 999, std::ptr::null()) };
            // setopt_typed via setopt: invalid TrackingMode.
            let bad: c_int = 999;
            unsafe {
                setopt(
                    e,
                    Option_::Event as c_int,
                    &bad as *const c_int as *const c_void,
                )
            };
            // invalid Format.
            unsafe {
                setopt(
                    e,
                    Option_::Format as c_int,
                    &bad as *const c_int as *const c_void,
                )
            };
            // Size struct too small (a real, aligned `Size` whose `size` field
            // under-reports itself).
            let mut tiny_size = test_size();
            tiny_size.size = 1;
            unsafe {
                setopt(
                    e,
                    Option_::Size as c_int,
                    &tiny_size as *const Size as *const c_void,
                )
            };
            // Size with a zero cell dimension (valid struct size, invalid values).
            let mut zero_size = test_size();
            zero_size.cell_width = 0;
            unsafe {
                setopt(
                    e,
                    Option_::Size as c_int,
                    &zero_size as *const Size as *const c_void,
                )
            };

            assert_eq!(Result::Success, unsafe {
                crate::sys::set(crate::sys::Option_::Log, std::ptr::null())
            });

            unsafe { free(e) };
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
            vec![
                (
                    crate::sys::LogLevel::Warning,
                    "mouse_encode".to_string(),
                    "setopt invalid option value=999".to_string(),
                ),
                (
                    crate::sys::LogLevel::Warning,
                    "mouse_encode".to_string(),
                    "setopt invalid TrackingMode value=999".to_string(),
                ),
                (
                    crate::sys::LogLevel::Warning,
                    "mouse_encode".to_string(),
                    "setopt invalid Format value=999".to_string(),
                ),
                (
                    crate::sys::LogLevel::Warning,
                    "mouse_encode".to_string(),
                    format!(
                        "setopt size struct too small size=1 expected>={}",
                        std::mem::size_of::<Size>()
                    ),
                ),
                (
                    crate::sys::LogLevel::Warning,
                    "mouse_encode".to_string(),
                    "setopt invalid size values (cell width and height must be non-zero)"
                        .to_string(),
                ),
            ],
            "the five S174-F1 warnings were not routed through sys::log_fn"
        );
    }
}
