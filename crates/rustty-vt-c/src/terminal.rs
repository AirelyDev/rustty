//! rustty-vt-c terminal
//!
//! PORTS: ghostty src/terminal/c/terminal.zig
//!
//! PORT STATUS: IN PROGRESS (mutual-atom Stage B v4 — D3 SEAM landed). The D3
//! trampoline seam + `ZigTerminal` lifecycle machinery are filled: `zig_terminal`,
//! `new`/`new_`/`free` (Box-based lifecycle), `vt_write`, `resize`, `reset`,
//! `mode_get`/`mode_set`, `scroll_viewport` (the `ScrollViewport` `[2]u64` union
//! dispatch), `get`/`get_multi`/`get_typed` (the non-deferred-type arms), and the
//! 9 `Effects` trampolines (each recovers the owning `TerminalWrapper` from the
//! stream handler via the exposed-provenance `container_of` and calls the C
//! fn-ptr). STILL DEFERRED to the later get/set + grid_ref sub-batches:
//! `set`/`setTyped`, the deferred-type `get_typed` arms (`color.*`/`cursor_style`/
//! `title`/`pwd`/kitty image+graphics/`selection`), and
//! `grid_ref`/`grid_ref_track`/`point_from_grid_ref` — those remain
//! `todo!("vt-c <name>")`. This is the root of the irreducible mutual cluster
//! (terminal<->grid_ref<->grid_ref_tracked<->selection).

// const std = @import("std");
// const testing = std.testing;
// const build_options = @import("terminal_options");
// const lib = @import("../lib.zig");
// const CAllocator = lib.alloc.Allocator;
// pub const ZigTerminal = @import("../Terminal.zig");
// const Stream = @import("../stream_terminal.zig").Stream;
// const Screen = @import("../Screen.zig");
// const ScreenSet = @import("../ScreenSet.zig");
// const PageList = @import("../PageList.zig");
// const apc = @import("../apc.zig");
// const kitty = @import("../kitty/key.zig");
// const kitty_gfx_c = @import("kitty_graphics.zig");
// const modes = @import("../modes.zig");
// const point = @import("../point.zig");
// const size = @import("../size.zig");
// const device_attributes = @import("../device_attributes.zig");
// const device_status = @import("../device_status.zig");
// const size_report = @import("../size_report.zig");
// const cell_c = @import("cell.zig");
// const row_c = @import("row.zig");
// const grid_ref_c = @import("grid_ref.zig");
// const grid_ref_tracked_c = @import("grid_ref_tracked.zig");
// const selection_c = @import("selection.zig");
// const style_c = @import("style.zig");
// const color = @import("../color.zig");
// const Result = @import("result.zig").Result;
// const Handler = @import("../stream_terminal.zig").Handler;
use crate::grid_ref::CGridRef;
use crate::grid_ref_tracked::{CTrackedGridRef, TrackedGridRef};
use crate::kitty_graphics::KittyGraphics as KittyGraphicsHandle;
use crate::point::PointC;
use crate::result::Result;
use rustty_lib::allocator::Allocator as CAllocator;
use rustty_lib::String as LibString;
use rustty_vt::device_attributes;
use rustty_vt::device_status;
use rustty_vt::modes;
use rustty_vt::pagelist::grow::ScrollbarC;
use rustty_vt::point::{Coordinate, Tag};
use rustty_vt::screen_set::Key as ScreenSetKey;
use rustty_vt::size;
use rustty_vt::size_report;
use rustty_vt::stream_terminal::{Handler, Stream};
use rustty_vt::terminal::{Colors, Terminal as ZigTerminal};
use rustty_vt::terminal::{Options as ZigTerminalOptions, ScrollViewport as ZigScrollViewport};
use std::collections::HashSet;
use std::ffi::c_void;
use std::mem::{align_of, offset_of, size_of};
use std::os::raw::c_int;

// const log = std.log.scoped(.terminal_c);

// PORT NOTE (D3, Rust-only glue): the reference threads the C allocator into the
// OSC parser via `Stream.initAlloc(alloc, ...)`. rustty's OSC parser wants a
// `ZigAllocatorRef`, and the wrapper's `Stream<'static>` requires a `'static`
// backing allocator (the same constraint `Terminal::vt_stream` resolves by
// backing the parser with a process-static global zig allocator). We mirror that
// choice here so dynamic OSC payloads (e.g. OSC 52 clipboard) can grow.
static GLOBAL_ZIG_ALLOCATOR_FOR_OSC: rustty_lib::allocator::GlobalZigAllocator =
    rustty_lib::allocator::GlobalZigAllocator;

/// Wrapper around ZigTerminal that tracks additional state for C API usage,
/// such as the persistent VT stream needed to handle escape sequences split
/// across multiple vt_write calls.
// PORT NOTE (internal, NOT C-ABI): `terminal: *ZigTerminal` is the raw owning
// back-pointer to the heap `ZigTerminal` (`Box::into_raw`'d in `new_`). `stream:
// Stream<'static>` is the persistent VT stream whose handler aliases that
// terminal; the self-referential aliasing is resolved by storing the terminal as
// a raw pointer (the handler holds `*mut Terminal`) and exposing the wrapper's
// provenance so the always-installed effect trampolines can recover it (see
// `wrapper_from_handler`). `tracked_grid_refs` is the set of live
// `*TrackedGridRef` so `free` can detach them safely (Zig
// `AutoArrayHashMapUnmanaged(*TrackedGridRef, void)`).
pub struct TerminalWrapper {
    pub terminal: *mut ZigTerminal,
    pub stream: Stream<'static>,
    pub effects: Effects,
    pub tracked_grid_refs: HashSet<*mut TrackedGridRef>,
}

/// C callback state for terminal effects. Trampolines are always
/// installed on the stream handler; they check these fields and
/// no-op when the corresponding callback is null.
// PORT NOTE (internal, NOT C-ABI): mirrors the reference `Effects` struct field
// for field. The fn-ptr typedefs and `CDeviceAttributes` ARE part of the C ABI
// (named below). `da_features_buf` is the scratch buffer the device-attributes
// trampoline fills (`[64]device_attributes.Primary.Feature`).
pub struct Effects {
    pub userdata: *mut c_void,
    pub write_pty: Option<WritePtyFn>,
    pub bell: Option<BellFn>,
    pub color_scheme: Option<ColorSchemeFn>,
    pub device_attributes_cb: Option<DeviceAttributesFn>,
    pub enquiry: Option<EnquiryFn>,
    pub xtversion: Option<XtversionFn>,
    pub title_changed: Option<TitleChangedFn>,
    pub pwd_changed: Option<PwdChangedFn>,
    pub size_cb: Option<SizeFn>,

    /// Scratch buffer for DA1 feature codes. See the reference doc comment:
    /// the trampoline converts C feature codes into this buffer and returns a
    /// slice pointing into it, kept valid past the trampoline return.
    pub da_features_buf: [rustty_vt::device_attributes::Feature; 64],
}

// PORT NOTE: the reference `Effects` defaults every callback to `null`, the
// `userdata` to `null`, and `da_features_buf` to `undefined` (it is always
// overwritten before being read). `TerminalWrapper { effects: Effects = .{} }`
// relies on those defaults; this is the parity analog (`undefined` -> an
// arbitrary `Feature` placeholder, never observed).
impl Default for Effects {
    fn default() -> Effects {
        Effects {
            userdata: std::ptr::null_mut(),
            write_pty: None,
            bell: None,
            color_scheme: None,
            device_attributes_cb: None,
            enquiry: None,
            xtversion: None,
            title_changed: None,
            pwd_changed: None,
            size_cb: None,
            da_features_buf: [rustty_vt::device_attributes::Feature::AnsiColor; 64],
        }
    }
}

/// C function pointer type for the write_pty callback.
// pub const WritePtyFn = *const fn (Terminal, ?*anyopaque, [*]const u8, usize) callconv(lib.calling_conv) void;
pub type WritePtyFn = extern "C" fn(Terminal, *mut c_void, *const u8, usize);

/// C function pointer type for the bell callback.
// pub const BellFn = *const fn (Terminal, ?*anyopaque) callconv(lib.calling_conv) void;
pub type BellFn = extern "C" fn(Terminal, *mut c_void);

/// C function pointer type for the color_scheme callback.
/// Returns true and fills out_scheme if a color scheme is available,
/// or returns false to silently ignore the query.
// pub const ColorSchemeFn = *const fn (Terminal, ?*anyopaque, *device_status.ColorScheme) callconv(lib.calling_conv) bool;
pub type ColorSchemeFn =
    extern "C" fn(Terminal, *mut c_void, *mut device_status::ColorScheme) -> bool;

/// C function pointer type for the enquiry callback.
/// Returns the response bytes. The memory must remain valid
/// until the callback returns.
// pub const EnquiryFn = *const fn (Terminal, ?*anyopaque) callconv(lib.calling_conv) lib.String;
pub type EnquiryFn = extern "C" fn(Terminal, *mut c_void) -> LibString;

/// C function pointer type for the xtversion callback.
/// Returns the version string (e.g. "ghostty 1.2.3"). The memory
/// must remain valid until the callback returns. An empty string
/// (len=0) causes the default "libghostty" to be reported.
// pub const XtversionFn = *const fn (Terminal, ?*anyopaque) callconv(lib.calling_conv) lib.String;
pub type XtversionFn = extern "C" fn(Terminal, *mut c_void) -> LibString;

/// C function pointer type for the title_changed callback.
// pub const TitleChangedFn = *const fn (Terminal, ?*anyopaque) callconv(lib.calling_conv) void;
pub type TitleChangedFn = extern "C" fn(Terminal, *mut c_void);

/// C function pointer type for the pwd_changed callback.
// pub const PwdChangedFn = *const fn (Terminal, ?*anyopaque) callconv(lib.calling_conv) void;
pub type PwdChangedFn = extern "C" fn(Terminal, *mut c_void);

/// C function pointer type for the size callback.
/// Returns true and fills out_size if size is available,
/// or returns false to silently ignore the query.
// pub const SizeFn = *const fn (Terminal, ?*anyopaque, *size_report.Size) callconv(lib.calling_conv) bool;
pub type SizeFn = extern "C" fn(Terminal, *mut c_void, *mut size_report::Size) -> bool;

/// C function pointer type for the device_attributes callback.
/// Returns true and fills out_attrs if attributes are available,
/// or returns false to silently ignore the query.
// pub const DeviceAttributesFn = *const fn (Terminal, ?*anyopaque, *CDeviceAttributes) callconv(lib.calling_conv) bool;
pub type DeviceAttributesFn = extern "C" fn(Terminal, *mut c_void, *mut CDeviceAttributes) -> bool;

/// C-compatible device attributes struct.
/// C: GhosttyDeviceAttributes
// pub const CDeviceAttributes = extern struct { primary, secondary, tertiary };
#[repr(C)]
#[derive(Clone, Copy)]
pub struct CDeviceAttributes {
    pub primary: CDeviceAttributesPrimary,
    pub secondary: CDeviceAttributesSecondary,
    pub tertiary: CDeviceAttributesTertiary,
}

// PORT NOTE: the reference nests `Primary`/`Secondary`/`Tertiary` inside
// `CDeviceAttributes`. Rust can't nest a type in a struct value namespace, so
// they are hoisted to siblings with the identical `extern` layout.

// pub const Primary = extern struct { conformance_level: u16, features: [64]u16, num_features: usize };
#[repr(C)]
#[derive(Clone, Copy)]
pub struct CDeviceAttributesPrimary {
    pub conformance_level: u16,
    pub features: [u16; 64],
    pub num_features: usize,
}

// pub const Secondary = extern struct { device_type: u16, firmware_version: u16, rom_cartridge: u16 };
#[repr(C)]
#[derive(Clone, Copy)]
pub struct CDeviceAttributesSecondary {
    pub device_type: u16,
    pub firmware_version: u16,
    pub rom_cartridge: u16,
}

// pub const Tertiary = extern struct { unit_id: u32 };
#[repr(C)]
#[derive(Clone, Copy)]
pub struct CDeviceAttributesTertiary {
    pub unit_id: u32,
}

// PORT NOTE (D3 container_of recovery): the reference trampolines recover the
// owning `*TerminalWrapper` from the `*Handler` via two nested
// `@fieldParentPtr`s: `Handler -> Stream` (the handler is `stream.handler`) and
// `Stream -> TerminalWrapper` (the stream is `wrapper.stream`). rustty collapses
// this into a single byte-offset subtraction off the `&mut Handler` address.
// The offset is the SUM of the two field offsets (`TerminalWrapper.stream` and
// `Stream.handler`) — the cross-crate, generic `Stream<'static>` makes a single
// nested `offset_of!(TerminalWrapper, stream.handler)` path fragile, so we sum
// the two leaf offsets (each is a plain in-crate field access).
//
// SB SOUNDNESS: the recovered `*mut TerminalWrapper` is used ONLY to read its
// SIBLING fields (`effects`, `terminal`) through the raw pointer; we never form
// an overlapping `&mut TerminalWrapper` while the caller's `&mut Handler` is
// still live (the `Handler` lives INSIDE the wrapper, so a `&mut TerminalWrapper`
// would alias it). All sibling reads go through raw pointer projection.
//
// PROVENANCE: a pointer derived (by offset subtraction) from the caller's
// `&mut Handler` would, under Stacked Borrows, only carry provenance over the
// Handler's bytes — reading the SIBLING `effects`/`terminal` through it is UB.
// `new_` therefore `expose_provenance()`s the whole `TerminalWrapper` allocation
// once; here we recover the wrapper by computing its ADDRESS (pure integer math
// off the handler address — no pointer provenance involved) and reconstituting a
// pointer over the exposed allocation via `with_exposed_provenance_mut`. This is
// the same exposed-provenance bridge `hyperlink.rs` uses for its stored page
// pointers, and it gives the recovered pointer whole-allocation provenance so the
// sibling reads are SB-sound.
#[inline]
fn wrapper_from_handler(handler: *mut Handler<'static>) -> *mut TerminalWrapper {
    // const stream_ptr: *Stream = @fieldParentPtr("handler", handler);
    // const wrapper: *TerminalWrapper = @fieldParentPtr("stream", stream_ptr);
    let off = offset_of!(TerminalWrapper, stream) + offset_of!(Stream<'static>, handler);
    // Compute the wrapper base ADDRESS from the handler address (integer math
    // only), then reconstitute a pointer over the exposed wrapper allocation.
    let wrapper_addr = (handler as *mut u8).addr() - off;
    std::ptr::with_exposed_provenance_mut::<TerminalWrapper>(wrapper_addr)
}

impl Effects {
    // fn writePtyTrampoline(handler: *Handler, data: [:0]const u8) void {
    //     const stream_ptr: *Stream = @fieldParentPtr("handler", handler);
    //     const wrapper: *TerminalWrapper = @fieldParentPtr("stream", stream_ptr);
    //     const func = wrapper.effects.write_pty orelse return;
    //     func(@ptrCast(wrapper), wrapper.effects.userdata, data.ptr, data.len);
    // }
    pub(crate) fn write_pty_trampoline(handler: &mut Handler<'static>, data: &[u8]) {
        let wrapper = wrapper_from_handler(handler);
        // SAFETY: `wrapper` is the live owning `TerminalWrapper`; we read its
        // `effects` sibling (not the `&mut Handler` we were handed) via raw ptr.
        let func = match unsafe { (*wrapper).effects.write_pty } {
            Some(f) => f,
            None => return,
        };
        let userdata = unsafe { (*wrapper).effects.userdata };
        func(wrapper, userdata, data.as_ptr(), data.len());
    }

    // fn bellTrampoline(handler: *Handler) void {
    //     ... const func = wrapper.effects.bell orelse return;
    //     func(@ptrCast(wrapper), wrapper.effects.userdata);
    // }
    pub(crate) fn bell_trampoline(handler: &mut Handler<'static>) {
        let wrapper = wrapper_from_handler(handler);
        let func = match unsafe { (*wrapper).effects.bell } {
            Some(f) => f,
            None => return,
        };
        let userdata = unsafe { (*wrapper).effects.userdata };
        func(wrapper, userdata);
    }

    // fn colorSchemeTrampoline(handler: *Handler) ?device_status.ColorScheme {
    //     ... const func = wrapper.effects.color_scheme orelse return null;
    //     var scheme: device_status.ColorScheme = undefined;
    //     if (func(@ptrCast(wrapper), wrapper.effects.userdata, &scheme)) return scheme;
    //     return null;
    // }
    pub(crate) fn color_scheme_trampoline(
        handler: &mut Handler<'static>,
    ) -> Option<device_status::ColorScheme> {
        let wrapper = wrapper_from_handler(handler);
        let func = unsafe { (*wrapper).effects.color_scheme }?;
        let userdata = unsafe { (*wrapper).effects.userdata };
        // var scheme: device_status.ColorScheme = undefined;
        let mut scheme = core::mem::MaybeUninit::<device_status::ColorScheme>::uninit();
        if func(wrapper, userdata, scheme.as_mut_ptr()) {
            return Some(unsafe { scheme.assume_init() });
        }
        None
    }

    // fn deviceAttributesTrampoline(handler: *Handler) device_attributes.Attributes {
    //     ... const func = wrapper.effects.device_attributes_cb orelse return .{};
    //     var c_attrs: CDeviceAttributes = undefined;
    //     if (!func(@ptrCast(wrapper), wrapper.effects.userdata, &c_attrs)) return .{};
    //     const n: usize = @min(c_attrs.primary.num_features, 64);
    //     for (0..n) |i| wrapper.effects.da_features_buf[i] = @enumFromInt(c_attrs.primary.features[i]);
    //     return .{ .primary = ..., .secondary = ..., .tertiary = ... };
    // }
    pub(crate) fn device_attributes_trampoline(
        handler: &mut Handler<'static>,
    ) -> device_attributes::Attributes {
        let wrapper = wrapper_from_handler(handler);
        let func = match unsafe { (*wrapper).effects.device_attributes_cb } {
            Some(f) => f,
            None => return device_attributes::Attributes::default(),
        };
        let userdata = unsafe { (*wrapper).effects.userdata };

        // Get our attributes from the callback.
        // var c_attrs: CDeviceAttributes = undefined;
        let mut c_attrs = core::mem::MaybeUninit::<CDeviceAttributes>::uninit();
        if !func(wrapper, userdata, c_attrs.as_mut_ptr()) {
            return device_attributes::Attributes::default();
        }
        let c_attrs = unsafe { c_attrs.assume_init() };

        // Note below we use a lot of enumFromInt but its always safe
        // because all our types are non-exhaustive enums.

        // const n: usize = @min(c_attrs.primary.num_features, 64);
        let n: usize = core::cmp::min(c_attrs.primary.num_features, 64);
        // for (0..n) |i| wrapper.effects.da_features_buf[i] = @enumFromInt(c_attrs.primary.features[i]);
        for i in 0..n {
            // SAFETY: `da_features_buf` is a sibling field of the recovered
            // wrapper; mutate it via the raw ptr (never via the live
            // `&mut Handler`).
            unsafe {
                (*wrapper).effects.da_features_buf[i] =
                    feature_from_int(c_attrs.primary.features[i]);
            }
        }

        // PORT NOTE: the reference returns `features = da_features_buf[0..n]`, a
        // slice into the wrapper's long-lived scratch buffer. The Rust
        // `Primary.features` field is `&'static [Feature]`; the buffer lives as
        // long as the heap `TerminalWrapper`, and `reportDeviceAttributes` reads
        // the slice immediately (before any re-entrant call), so promoting the
        // borrow to `'static` here is sound (matches the reference's reliance on
        // the buffer outliving the trampoline return).
        let features: &'static [device_attributes::Feature] =
            unsafe { core::slice::from_raw_parts((*wrapper).effects.da_features_buf.as_ptr(), n) };

        device_attributes::Attributes {
            primary: device_attributes::Primary {
                conformance_level: conformance_level_from_int(c_attrs.primary.conformance_level),
                features,
            },
            secondary: device_attributes::Secondary {
                device_type: device_type_from_int(c_attrs.secondary.device_type),
                firmware_version: c_attrs.secondary.firmware_version,
                rom_cartridge: c_attrs.secondary.rom_cartridge,
            },
            tertiary: device_attributes::Tertiary {
                unit_id: c_attrs.tertiary.unit_id,
            },
        }
    }

    // fn enquiryTrampoline(handler: *Handler) []const u8 {
    //     ... const func = wrapper.effects.enquiry orelse return "";
    //     const result = func(@ptrCast(wrapper), wrapper.effects.userdata);
    //     if (result.len == 0) return "";
    //     return result.ptr[0..result.len];
    // }
    pub(crate) fn enquiry_trampoline(handler: &mut Handler<'static>) -> &'static [u8] {
        let wrapper = wrapper_from_handler(handler);
        let func = match unsafe { (*wrapper).effects.enquiry } {
            Some(f) => f,
            None => return b"",
        };
        let userdata = unsafe { (*wrapper).effects.userdata };
        let result = func(wrapper, userdata);
        if result.len == 0 {
            return b"";
        }
        // return result.ptr[0..result.len];
        // SAFETY: the callback contract is that `result.ptr[0..result.len]` is
        // valid for the lifetime of the call; the response is read immediately
        // by `reportEnquiry` before any re-entrant call.
        unsafe { core::slice::from_raw_parts(result.ptr, result.len) }
    }

    // fn xtversionTrampoline(handler: *Handler) []const u8 { ... identical shape ... }
    pub(crate) fn xtversion_trampoline(handler: &mut Handler<'static>) -> &'static [u8] {
        let wrapper = wrapper_from_handler(handler);
        let func = match unsafe { (*wrapper).effects.xtversion } {
            Some(f) => f,
            None => return b"",
        };
        let userdata = unsafe { (*wrapper).effects.userdata };
        let result = func(wrapper, userdata);
        if result.len == 0 {
            return b"";
        }
        unsafe { core::slice::from_raw_parts(result.ptr, result.len) }
    }

    // fn titleChangedTrampoline(handler: *Handler) void { ... }
    pub(crate) fn title_changed_trampoline(handler: &mut Handler<'static>) {
        let wrapper = wrapper_from_handler(handler);
        let func = match unsafe { (*wrapper).effects.title_changed } {
            Some(f) => f,
            None => return,
        };
        let userdata = unsafe { (*wrapper).effects.userdata };
        func(wrapper, userdata);
    }

    // fn pwdChangedTrampoline(handler: *Handler) void { ... }
    pub(crate) fn pwd_changed_trampoline(handler: &mut Handler<'static>) {
        let wrapper = wrapper_from_handler(handler);
        let func = match unsafe { (*wrapper).effects.pwd_changed } {
            Some(f) => f,
            None => return,
        };
        let userdata = unsafe { (*wrapper).effects.userdata };
        func(wrapper, userdata);
    }

    // fn sizeTrampoline(handler: *Handler) ?size_report.Size {
    //     ... const func = wrapper.effects.size_cb orelse return null;
    //     var s: size_report.Size = undefined;
    //     if (func(@ptrCast(wrapper), wrapper.effects.userdata, &s)) return s;
    //     return null;
    // }
    pub(crate) fn size_trampoline(handler: &mut Handler<'static>) -> Option<size_report::Size> {
        let wrapper = wrapper_from_handler(handler);
        let func = unsafe { (*wrapper).effects.size_cb }?;
        let userdata = unsafe { (*wrapper).effects.userdata };
        let mut s = core::mem::MaybeUninit::<size_report::Size>::uninit();
        if func(wrapper, userdata, s.as_mut_ptr()) {
            return Some(unsafe { s.assume_init() });
        }
        None
    }
}

// PORT NOTE: the reference uses `@enumFromInt` on the DA enums (always safe
// because they are non-exhaustive). rustty's ported enums carry an explicit
// catch-all variant (`Other(u16)`), so these helpers reproduce `@enumFromInt`:
// known codes map to their named variant, every other value rides the catch-all.
// They are NOT a rustty-vt change (R3) — purely the vt-c-local int->enum bridge.
fn feature_from_int(v: u16) -> device_attributes::Feature {
    use device_attributes::Feature;
    match v {
        1 => Feature::Columns132,
        2 => Feature::Printer,
        3 => Feature::Regis,
        4 => Feature::Sixel,
        6 => Feature::SelectiveErase,
        8 => Feature::UserDefinedKeys,
        9 => Feature::NationalReplacement,
        15 => Feature::TechnicalCharacters,
        16 => Feature::Locator,
        17 => Feature::TerminalState,
        18 => Feature::Windowing,
        21 => Feature::HorizontalScrolling,
        22 => Feature::AnsiColor,
        28 => Feature::RectangularEditing,
        29 => Feature::AnsiTextLocator,
        52 => Feature::Clipboard,
        other => Feature::Other(other),
    }
}

fn conformance_level_from_int(v: u16) -> device_attributes::ConformanceLevel {
    use device_attributes::ConformanceLevel;
    match v {
        1 => ConformanceLevel::VT100,
        4 => ConformanceLevel::VT132,
        6 => ConformanceLevel::VT102,
        7 => ConformanceLevel::VT131,
        12 => ConformanceLevel::VT125,
        62 => ConformanceLevel::Level2,
        63 => ConformanceLevel::Level3,
        64 => ConformanceLevel::Level4,
        65 => ConformanceLevel::Level5,
        other => ConformanceLevel::Other(other),
    }
}

fn device_type_from_int(v: u16) -> device_attributes::DeviceType {
    use device_attributes::DeviceType;
    match v {
        0 => DeviceType::VT100,
        1 => DeviceType::VT220,
        2 => DeviceType::VT240,
        18 => DeviceType::VT330,
        19 => DeviceType::VT340,
        24 => DeviceType::VT320,
        32 => DeviceType::VT382,
        41 => DeviceType::VT420,
        61 => DeviceType::VT510,
        64 => DeviceType::VT520,
        65 => DeviceType::VT525,
        other => DeviceType::Other(other),
    }
}

/// C: GhosttyTerminal -> RusttyTerminal (R2)
// pub const Terminal = ?*TerminalWrapper;
// PORT NOTE: nullable opaque handle; raw `*mut TerminalWrapper` (null == none).
pub type Terminal = *mut TerminalWrapper;

// pub fn zigTerminal(terminal_: Terminal) ?*ZigTerminal {
//     return (terminal_ orelse return null).terminal;
// }
/// # Safety
/// `terminal_`, if non-null, must be a live terminal handle.
pub unsafe fn zig_terminal(terminal_: Terminal) -> *mut ZigTerminal {
    if terminal_.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: `terminal_` is a live `new`-returned wrapper (caller contract).
    unsafe { (*terminal_).terminal }
}

/// C: GhosttyTerminalOptions
// pub const Options = extern struct { cols, rows, max_scrollback };
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Options {
    pub cols: size::CellCountInt,
    pub rows: size::CellCountInt,
    pub max_scrollback: usize,
}

// const NewError = error{ InvalidValue, OutOfMemory };

// PORT NOTE (D3, internal): error analog of the reference `NewError`. The
// reference `new` calls `new_` and maps the error to a `Result`; we inline that
// mapping here (Rust has no `errdefer`, so the cleanup is hand-unwound).
enum NewError {
    InvalidValue,
    OutOfMemory,
}

// pub fn new(alloc_: ?*const CAllocator, result: *Terminal, opts: Options) ... {
//     result.* = new_(alloc_, opts) catch |err| {
//         result.* = null;
//         return switch (err) {
//             error.InvalidValue => .invalid_value,
//             error.OutOfMemory => .out_of_memory,
//         };
//     };
//     return .success;
// }
/// # Safety
/// `alloc_`, if non-null, must point to a valid `CAllocator`; `result` must be a
/// valid writable pointer to a `Terminal`.
pub unsafe fn new(alloc_: *const CAllocator, result: *mut Terminal, opts: Options) -> Result {
    // SAFETY: `alloc_` validity and `result` writability are caller contracts.
    let alloc_ref = unsafe { alloc_.as_ref() };
    match new_(alloc_ref, opts) {
        Ok(wrapper) => {
            unsafe { *result = wrapper };
            Result::Success
        }
        Err(err) => {
            unsafe { *result = std::ptr::null_mut() };
            match err {
                NewError::InvalidValue => Result::InvalidValue,
                NewError::OutOfMemory => Result::OutOfMemory,
            }
        }
    }
}

// fn new_(alloc_: ?*const CAllocator, opts: Options) NewError!*TerminalWrapper {
//     if (opts.cols == 0 or opts.rows == 0) return error.InvalidValue;
//     const alloc = lib.alloc.default(alloc_);
//     const t = alloc.create(ZigTerminal) catch return error.OutOfMemory;
//     errdefer alloc.destroy(t);
//     const wrapper = alloc.create(TerminalWrapper) catch return error.OutOfMemory;
//     errdefer alloc.destroy(wrapper);
//     t.* = try .init(alloc, .{ .cols, .rows, .max_scrollback });
//     errdefer t.deinit(alloc);
//     t.flags.shell_redraws_prompt = .false;
//     var handler: Stream.Handler = t.vtHandler();
//     handler.effects = .{ .write_pty = &Effects.writePtyTrampoline, ... };
//     wrapper.* = .{ .terminal = t, .stream = .initAlloc(alloc, handler) };
//     return wrapper;
// }
//
// PORT NOTE (D3 lifecycle): the reference allocates `t` and `wrapper` via the C
// allocator. rustty's `ZigTerminal` is allocated/freed exclusively through its
// own `Allocator` (`allocator_api2::alloc::Global`; the C `alloc_` only selects
// the rustty-lib resolution and never drives `ZigTerminal`'s internal pools), so
// the wrapper and terminal storage are `Box::into_raw`'d on the Rust global heap
// per the signed-off D3 storage plan. The two `errdefer`s become explicit hand
// unwinds (Box drops + `deinit`) on each fallible step.
fn new_(alloc_: Option<&CAllocator>, opts: Options) -> std::result::Result<Terminal, NewError> {
    // if (opts.cols == 0 or opts.rows == 0) return error.InvalidValue;
    if opts.cols == 0 || opts.rows == 0 {
        return Err(NewError::InvalidValue);
    }

    // const alloc = lib.alloc.default(alloc_);
    // PORT NOTE: resolved for parity with the reference (selects the global
    // default when null); `ZigTerminal` itself uses `Global`, so this is unused
    // beyond validating the C allocator handle.
    let _alloc = rustty_lib::allocator::default(alloc_);
    let gpa = allocator_api2::alloc::Global;

    // const t = alloc.create(ZigTerminal) catch return error.OutOfMemory;
    // errdefer alloc.destroy(t);
    // t.* = try .init(alloc, .{ ... });
    // errdefer t.deinit(alloc);
    let mut terminal = match ZigTerminal::init(
        gpa,
        ZigTerminalOptions {
            cols: opts.cols,
            rows: opts.rows,
            max_scrollback: opts.max_scrollback,
            // PORT NOTE: the reference relies on the `Options` field defaults for
            // the remaining fields. rustty-vt has no `Default` for `Options`; we
            // replicate the (ghostty-artifact) defaults that rustty-vt's source
            // of truth produces.
            colors: Colors::default(),
            default_modes: rustty_vt::modes::ModePacked::default(),
            kitty_image_storage_limit: 320 * 1000 * 1000,
            kitty_image_loading_limits: rustty_vt::kitty::graphics_image::Limits::DIRECT,
        },
    ) {
        Ok(t) => t,
        Err(_oom) => return Err(NewError::OutOfMemory),
    };

    // libghostty-vt embedders don't necessarily install Ghostty's shell
    // integration, so don't assume OSC 133 prompts can be redrawn on resize.
    // Shells can still opt in with OSC 133;A;redraw=1.
    // t.flags.shell_redraws_prompt = .false;
    terminal.flags.shell_redraws_prompt = rustty_vt::osc::semantic_prompt::Redraw::False;

    // const t = ...; — move the terminal to the heap so the wrapper's stream
    // handler can alias a stable address.
    let terminal_ptr: *mut ZigTerminal = Box::into_raw(Box::new(terminal));

    // Setup our stream with trampolines always installed so that
    // setting C callbacks at any time takes effect immediately.
    // var handler: Stream.Handler = t.vtHandler();
    // SAFETY: `terminal_ptr` is a freshly-boxed, uniquely-owned `ZigTerminal`;
    // `vt_handler` only reads `&mut *terminal_ptr` to capture its address into
    // the returned `Handler` (no aliasing handler exists yet).
    let mut handler: Handler<'static> = unsafe { (*terminal_ptr).vt_handler() };

    // handler.effects = .{ .write_pty = &Effects.writePtyTrampoline, ... };
    handler.effects = rustty_vt::stream_terminal::Effects {
        write_pty: Some(Effects::write_pty_trampoline),
        bell: Some(Effects::bell_trampoline),
        color_scheme: Some(Effects::color_scheme_trampoline),
        device_attributes: Some(Effects::device_attributes_trampoline),
        enquiry: Some(Effects::enquiry_trampoline),
        xtversion: Some(Effects::xtversion_trampoline),
        title_changed: Some(Effects::title_changed_trampoline),
        pwd_changed: Some(Effects::pwd_changed_trampoline),
        size: Some(Effects::size_trampoline),
    };

    // wrapper.* = .{ .terminal = t, .stream = .initAlloc(alloc, handler) };
    // PORT NOTE: the reference threads the C allocator into the OSC parser; we
    // back it with the process-static global zig allocator (the same choice
    // `Terminal::vt_stream` makes), which supports dynamic OSC growth.
    let wrapper = TerminalWrapper {
        terminal: terminal_ptr,
        stream: Stream::init_alloc(
            rustty_lib::allocator::ZigAllocatorRef::new(&GLOBAL_ZIG_ALLOCATOR_FOR_OSC),
            handler,
        ),
        effects: Effects::default(),
        tracked_grid_refs: HashSet::new(),
    };

    // return wrapper;
    let wrapper_ptr = Box::into_raw(Box::new(wrapper));
    // PROVENANCE: expose the whole wrapper allocation so the effect trampolines
    // can recover it (siblings of the stream handler) via
    // `with_exposed_provenance_mut` — see `wrapper_from_handler`.
    let _ = (wrapper_ptr as *const u8).expose_provenance();
    Ok(wrapper_ptr)
}

// pub fn vt_write(terminal_: Terminal, ptr: [*]const u8, len: usize) ... {
//     const wrapper = terminal_ orelse return;
//     wrapper.stream.nextSlice(ptr[0..len]);
// }
/// # Safety
/// `terminal_`, if non-null, must be a live terminal handle; `ptr` must point to
/// `len` readable bytes.
pub unsafe fn vt_write(terminal_: Terminal, ptr: *const u8, len: usize) {
    // const wrapper = terminal_ orelse return;
    if terminal_.is_null() {
        return;
    }
    // wrapper.stream.nextSlice(ptr[0..len]);
    // SAFETY: `terminal_` is a live wrapper; `ptr[0..len]` is readable (caller
    // contract). The stream's handler aliases this wrapper's terminal.
    let input = unsafe { core::slice::from_raw_parts(ptr, len) };
    unsafe { (*terminal_).stream.next_slice(input) };
}

/// C: GhosttyTerminalOption
// pub const Option = enum(c_int) { ... };
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Option_ {
    Userdata = 0,
    WritePty = 1,
    Bell = 2,
    Enquiry = 3,
    Xtversion = 4,
    TitleChanged = 5,
    SizeCb = 6,
    ColorScheme = 7,
    DeviceAttributes = 8,
    Title = 9,
    Pwd = 10,
    ColorForeground = 11,
    ColorBackground = 12,
    ColorCursor = 13,
    ColorPalette = 14,
    KittyImageStorageLimit = 15,
    KittyImageMediumFile = 16,
    KittyImageMediumTempFile = 17,
    KittyImageMediumSharedMem = 18,
    ApcMaxBytes = 19,
    ApcMaxBytesKitty = 20,
    Selection = 21,
    DefaultCursorStyle = 22,
    DefaultCursorBlink = 23,
    GlyphProtocol = 24,
    PwdChanged = 25,
}

// pub fn set(terminal_: Terminal, option: Option, value: ?*const anyopaque) ... {
//     if (comptime std.debug.runtime_safety) { _ = intToEnum(Option, ...) catch { ... return .invalid_value; }; }
//     const wrapper = terminal_ orelse return .invalid_value;
//     return switch (option) {
//         inline else => |comptime_option| setTyped(wrapper, comptime_option, @ptrCast(@alignCast(value))),
//     };
// }
//
// PORT NOTE: the `intToEnum(Option, ...)` runtime-safety guard on the raw `c_int`
// crossing the C boundary lives in the lib.rs `extern "C"` wrapper (the cell/key
// pattern); here `option` is already a valid `#[repr(C)] Option_`. The `inline
// else => setTyped` comptime dispatch becomes a direct `set_typed` call.
//
// PARTIAL FILL: only the `.selection` arm of `set_typed` is ported in this
// sub-batch (it is exercised by `selection.zig`'s `selection_format_alloc uses
// active selection` test and depends only on already-filled APIs). Every other
// option arm projects a still-deferred vt-c type or terminal seam and lands in
// the get/set sub-batch; they remain `todo!("vt-c set <option>")`.
/// # Safety
/// `terminal_`, if non-null, must be a live terminal handle; `value`, if
/// non-null, must point to storage of the type required by `option`
/// (`Option_::in_type`).
pub unsafe fn set(terminal_: Terminal, option: Option_, value: *const c_void) -> Result {
    // const wrapper = terminal_ orelse return .invalid_value;
    if terminal_.is_null() {
        return Result::InvalidValue;
    }
    // inline else => |comptime_option| setTyped(wrapper, comptime_option, @ptrCast(@alignCast(value)))
    unsafe { set_typed(terminal_, option, value) }
}

// fn setTyped(wrapper: *TerminalWrapper, comptime option: Option, value: option.InType()) Result {
//     switch (option) { ...; .selection => { ... }; ... }
//     return .success;
// }
//
// # Safety
// `terminal_`, if non-null, is a live wrapper; `value` points at storage of the
// type required by `option`.
unsafe fn set_typed(terminal_: Terminal, option: Option_, value: *const c_void) -> Result {
    match option {
        // .selection => {
        //     if (value) |ptr| {
        //         const sel = ptr.toZig() orelse return .invalid_value;
        //         wrapper.terminal.screens.active.select(sel) catch return .out_of_memory;
        //     } else {
        //         wrapper.terminal.screens.active.clearSelection();
        //     }
        // },
        Option_::Selection => {
            // SAFETY: `terminal_` is a live wrapper; reach the active screen via
            // raw ptrs (no `&mut ZigTerminal` retag that would invalidate the
            // stream handler's stored alias — the strongly-protected discipline).
            let t: *mut ZigTerminal = unsafe { (*terminal_).terminal };
            let active: *mut rustty_vt::screen::Screen = unsafe { (*t).screens.active };
            // if (value) |ptr| { ... } else { ... }
            let ptr = value as *const crate::selection::CSelection;
            if !ptr.is_null() {
                // const sel = ptr.toZig() orelse return .invalid_value;
                let sel = match unsafe { (*ptr).to_zig() } {
                    Some(s) => s,
                    None => return Result::InvalidValue,
                };
                // wrapper.terminal.screens.active.select(sel) catch return .out_of_memory;
                if unsafe { (*active).select(Some(sel)) }.is_err() {
                    return Result::OutOfMemory;
                }
            } else {
                // wrapper.terminal.screens.active.clearSelection();
                unsafe { (*active).clear_selection() };
            }
        }

        // PORT NOTE: every other arm projects a still-deferred vt-c type or
        // terminal seam; they fill in the get/set sub-batch.
        Option_::Userdata => todo!("vt-c set userdata"),
        Option_::WritePty => todo!("vt-c set write_pty"),
        Option_::Bell => todo!("vt-c set bell"),
        Option_::Enquiry => todo!("vt-c set enquiry"),
        Option_::Xtversion => todo!("vt-c set xtversion"),
        Option_::TitleChanged => todo!("vt-c set title_changed"),
        Option_::SizeCb => todo!("vt-c set size_cb"),
        Option_::ColorScheme => todo!("vt-c set color_scheme"),
        Option_::DeviceAttributes => todo!("vt-c set device_attributes"),
        Option_::Title => todo!("vt-c set title"),
        Option_::Pwd => todo!("vt-c set pwd"),
        Option_::ColorForeground => todo!("vt-c set color_foreground"),
        Option_::ColorBackground => todo!("vt-c set color_background"),
        Option_::ColorCursor => todo!("vt-c set color_cursor"),
        Option_::ColorPalette => todo!("vt-c set color_palette"),
        Option_::KittyImageStorageLimit => todo!("vt-c set kitty_image_storage_limit"),
        Option_::KittyImageMediumFile => todo!("vt-c set kitty_image_medium_file"),
        Option_::KittyImageMediumTempFile => todo!("vt-c set kitty_image_medium_temp_file"),
        Option_::KittyImageMediumSharedMem => todo!("vt-c set kitty_image_medium_shared_mem"),
        Option_::ApcMaxBytes => todo!("vt-c set apc_max_bytes"),
        Option_::ApcMaxBytesKitty => todo!("vt-c set apc_max_bytes_kitty"),
        Option_::DefaultCursorStyle => todo!("vt-c set default_cursor_style"),
        Option_::DefaultCursorBlink => todo!("vt-c set default_cursor_blink"),
        Option_::GlyphProtocol => todo!("vt-c set glyph_protocol"),
        Option_::PwdChanged => todo!("vt-c set pwd_changed"),
    }

    // return .success;
    Result::Success
}

/// C: GhosttyTerminalCursorStyle
// pub const TerminalCursorStyle = enum(c_int) { bar = 0, block = 1, underline = 2, block_hollow = 3, _ };
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalCursorStyle {
    Bar = 0,
    Block = 1,
    Underline = 2,
    BlockHollow = 3,
}

/// C: GhosttyDeviceAttributes
// pub const DeviceAttributes = Effects.CDeviceAttributes;
pub type DeviceAttributes = CDeviceAttributes;

/// C: GhosttyTerminalScrollViewport
// pub const ScrollViewport = ZigTerminal.ScrollViewport.C;
//
// PORT NOTE: `ZigTerminal.ScrollViewport` is `union(Tag)` with the C-ABI
// `lib.TaggedUnion(target, @This(), [2]u64)` projection deferred here. We author
// it by hand following the `TaggedUnion` expansion (see point.rs for the same
// pattern): a `#[repr(C)]` struct `{ tag: ScrollViewportTag, value:
// ScrollViewportCValue }` where `ScrollViewportCValue` is an `extern union` with
// one member per tag (`top`/`bottom` carry no payload -> unit `()` is not a
// union member, so they ride the padding; `delta: isize`) plus the PINNED
// `_padding: [u64; 2]`. The `[2]u64` padding is verbatim from the reference
// comment ("largest variant is isize (8 bytes on 64-bit); use [2]u64 (16 bytes)
// for future expansion") and must NEVER change (ABI stability).
pub type ScrollViewport = ScrollViewportC;

/// The C ABI compatible tag for `ScrollViewport`.
// PORT NOTE: `Tag = lib.Enum(lib.target, &.{ "top", "bottom", "delta" })`; for
// the `.c` target a fieldless `enum(c_int)` in key order.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollViewportTag {
    Top = 0,
    Bottom = 1,
    Delta = 2,
}

/// The C ABI compatible union value for `ScrollViewport`.
// PORT NOTE: `lib.TaggedUnion` gives one union field per tag. `top`/`bottom`
// have `void` payload (no addressable member in C); only `delta: isize` carries
// a value. The `_padding` member pins the union to `[2]u64`.
#[repr(C)]
#[derive(Clone, Copy)]
pub union ScrollViewportCValue {
    pub delta: isize,
    /// Padding to the PINNED ABI size. `[2]u64` (16 bytes) per the reference.
    pub _padding: [u64; 2],
}

/// C: GhosttyTerminalScrollViewport — the hand-authored tagged union.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ScrollViewportC {
    pub tag: ScrollViewportTag,
    pub value: ScrollViewportCValue,
}

// BUILD-failure gate (D2): pin the `[2]u64` padding size, the natural alignment,
// and the `c_int`-width tag. Drift here fails the build.
const _: () = assert!(size_of::<ScrollViewportCValue>() == size_of::<[u64; 2]>());
const _: () = assert!(align_of::<ScrollViewportCValue>() == 8);
const _: () = assert!(size_of::<ScrollViewportTag>() == size_of::<c_int>());

// pub fn scroll_viewport(terminal_: Terminal, behavior: ScrollViewport) ... {
//     const t: *ZigTerminal = (terminal_ orelse return).terminal;
//     t.scrollViewport(switch (behavior.tag) {
//         .top => .top,
//         .bottom => .bottom,
//         .delta => .{ .delta = behavior.value.delta },
//     });
// }
/// # Safety
/// `terminal_`, if non-null, must be a live terminal handle.
pub unsafe fn scroll_viewport(terminal_: Terminal, behavior: ScrollViewport) {
    if terminal_.is_null() {
        return;
    }
    // SAFETY: `terminal_` is a live wrapper; `terminal` is its owned heap
    // terminal.
    let t: *mut ZigTerminal = unsafe { (*terminal_).terminal };
    let zig_behavior = match behavior.tag {
        ScrollViewportTag::Top => ZigScrollViewport::Top,
        ScrollViewportTag::Bottom => ZigScrollViewport::Bottom,
        // SAFETY: the `delta` tag selects the `delta` union member (C ABI
        // contract mirrored from the reference `behavior.value.delta`).
        ScrollViewportTag::Delta => ZigScrollViewport::Delta(unsafe { behavior.value.delta }),
    };
    unsafe { (*t).scroll_viewport(zig_behavior) };
}

// pub fn resize(terminal_, cols, rows, cell_width_px, cell_height_px) ... {
//     const wrapper = terminal_ orelse return .invalid_value;
//     const t = wrapper.terminal;
//     if (cols == 0 or rows == 0) return .invalid_value;
//     t.resize(t.gpa(), cols, rows) catch return .out_of_memory;
//     t.width_px = std.math.mul(u32, cols, cell_width_px) catch std.math.maxInt(u32);
//     t.height_px = std.math.mul(u32, rows, cell_height_px) catch std.math.maxInt(u32);
//     t.modes.set(.synchronized_output, false);
//     if (t.modes.get(.in_band_size_reports)) in_band: {
//         const func = wrapper.effects.write_pty orelse break :in_band;
//         var buf: [1024]u8 = undefined;
//         var writer: std.Io.Writer = .fixed(&buf);
//         size_report.encode(&writer, .mode_2048, .{ .rows, .columns, .cell_width, .cell_height }) catch break :in_band;
//         const data = writer.buffered();
//         func(@ptrCast(wrapper), wrapper.effects.userdata, data.ptr, data.len);
//     }
//     return .success;
// }
/// # Safety
/// `terminal_`, if non-null, must be a live terminal handle.
pub unsafe fn resize(
    terminal_: Terminal,
    cols: size::CellCountInt,
    rows: size::CellCountInt,
    cell_width_px: u32,
    cell_height_px: u32,
) -> Result {
    // const wrapper = terminal_ orelse return .invalid_value;
    if terminal_.is_null() {
        return Result::InvalidValue;
    }
    // const t = wrapper.terminal;
    // SAFETY: `terminal_` is a live wrapper.
    let t: *mut ZigTerminal = unsafe { (*terminal_).terminal };

    // if (cols == 0 or rows == 0) return .invalid_value;
    if cols == 0 || rows == 0 {
        return Result::InvalidValue;
    }

    // t.resize(t.gpa(), cols, rows) catch return .out_of_memory;
    // SAFETY: short-lived deref; no overlapping `&mut ZigTerminal` is held.
    let gpa = unsafe { (*t).gpa() };
    if unsafe { (*t).resize(gpa, cols, rows) }.is_err() {
        return Result::OutOfMemory;
    }

    // Update pixel sizes
    // t.width_px = std.math.mul(u32, cols, cell_width_px) catch std.math.maxInt(u32);
    // t.height_px = std.math.mul(u32, rows, cell_height_px) catch std.math.maxInt(u32);
    // PORT NOTE: `std.math.mul(...) catch maxInt(u32)` is saturate-on-overflow.
    unsafe {
        (*t).width_px = (cols as u32).saturating_mul(cell_width_px);
        (*t).height_px = (rows as u32).saturating_mul(cell_height_px);
    }

    // Disable synchronized output mode so that we show changes
    // immediately for a resize. This is allowed by the spec.
    // t.modes.set(.synchronized_output, false);
    unsafe { (*t).modes.set(modes::Mode::SynchronizedOutput, false) };

    // If we have in-band size reporting enabled, send a report.
    // if (t.modes.get(.in_band_size_reports)) in_band: {
    if unsafe { (*t).modes.get(modes::Mode::InBandSizeReports) } {
        // const func = wrapper.effects.write_pty orelse break :in_band;
        // SAFETY: `effects` is a sibling field of the live wrapper.
        if let Some(func) = unsafe { (*terminal_).effects.write_pty } {
            // var buf: [1024]u8 = undefined; var writer = .fixed(&buf);
            // size_report.encode(&writer, .mode_2048, .{...}) catch break :in_band;
            // PORT NOTE: the reference encodes into a fixed `[1024]u8`. rustty's
            // `size_report::encode` writes via `core::fmt::Write`; we accumulate
            // into a `String` (ASCII only) and hand its bytes to the callback,
            // matching `report_device_attributes`. An encode failure is the
            // `catch break :in_band` early-out.
            let mut buf = String::new();
            let encode_ok = size_report::encode(
                &mut buf,
                size_report::Style::Mode2048,
                size_report::Size {
                    rows,
                    columns: cols,
                    cell_width: cell_width_px,
                    cell_height: cell_height_px,
                },
            )
            .is_ok();
            if encode_ok {
                // const data = writer.buffered();
                // func(@ptrCast(wrapper), wrapper.effects.userdata, data.ptr, data.len);
                let data = buf.as_bytes();
                let userdata = unsafe { (*terminal_).effects.userdata };
                func(terminal_, userdata, data.as_ptr(), data.len());
            }
        }
    }

    // return .success;
    Result::Success
}

// pub fn reset(terminal_: Terminal) ... {
//     const t: *ZigTerminal = (terminal_ orelse return).terminal;
//     t.fullReset();
// }
/// # Safety
/// `terminal_`, if non-null, must be a live terminal handle.
pub unsafe fn reset(terminal_: Terminal) {
    if terminal_.is_null() {
        return;
    }
    // SAFETY: `terminal_` is a live wrapper.
    let t: *mut ZigTerminal = unsafe { (*terminal_).terminal };
    unsafe { (*t).full_reset() };
}

// pub fn mode_get(terminal_, tag: modes.ModeTag.Backing, out_value: *bool) ... {
//     const t: *ZigTerminal = (terminal_ orelse return .invalid_value).terminal;
//     const mode_tag: modes.ModeTag = @bitCast(tag);
//     const mode = modes.modeFromInt(mode_tag.value, mode_tag.ansi) orelse return .invalid_value;
//     out_value.* = t.modes.get(mode);
//     return .success;
// }
/// # Safety
/// `terminal_`, if non-null, must be a live terminal handle; `out_value` must be
/// a valid writable pointer to a `bool`.
pub unsafe fn mode_get(
    terminal_: Terminal,
    tag: modes::ModeTagBacking,
    out_value: *mut bool,
) -> Result {
    if terminal_.is_null() {
        return Result::InvalidValue;
    }
    // SAFETY: `terminal_` is a live wrapper.
    let t: *mut ZigTerminal = unsafe { (*terminal_).terminal };
    // const mode_tag: modes.ModeTag = @bitCast(tag);
    let mode_tag = modes::ModeTag::from_backing(tag);
    // const mode = modes.modeFromInt(mode_tag.value, mode_tag.ansi) orelse return .invalid_value;
    let mode = match modes::mode_from_int(mode_tag.value, mode_tag.ansi) {
        Some(m) => m,
        None => return Result::InvalidValue,
    };
    // out_value.* = t.modes.get(mode);
    // SAFETY: `out_value` is a valid writable bool pointer (caller contract).
    unsafe { *out_value = (*t).modes.get(mode) };
    Result::Success
}

// pub fn mode_set(terminal_, tag: modes.ModeTag.Backing, value: bool) ... {
//     const t: *ZigTerminal = (terminal_ orelse return .invalid_value).terminal;
//     const mode_tag: modes.ModeTag = @bitCast(tag);
//     const mode = modes.modeFromInt(mode_tag.value, mode_tag.ansi) orelse return .invalid_value;
//     t.modes.set(mode, value);
//     return .success;
// }
/// # Safety
/// `terminal_`, if non-null, must be a live terminal handle.
pub unsafe fn mode_set(terminal_: Terminal, tag: modes::ModeTagBacking, value: bool) -> Result {
    if terminal_.is_null() {
        return Result::InvalidValue;
    }
    // SAFETY: `terminal_` is a live wrapper.
    let t: *mut ZigTerminal = unsafe { (*terminal_).terminal };
    let mode_tag = modes::ModeTag::from_backing(tag);
    let mode = match modes::mode_from_int(mode_tag.value, mode_tag.ansi) {
        Some(m) => m,
        None => return Result::InvalidValue,
    };
    // t.modes.set(mode, value);
    unsafe { (*t).modes.set(mode, value) };
    Result::Success
}

/// C: GhosttyKittyGraphics
// pub const KittyGraphics = kitty_gfx_c.KittyGraphics;
pub type KittyGraphics = KittyGraphicsHandle;

/// C: GhosttyTerminalScreen
// pub const TerminalScreen = ScreenSet.Key;
pub type TerminalScreen = ScreenSetKey;

/// C: GhosttyTerminalScrollbar
// pub const TerminalScrollbar = PageList.Scrollbar.C;
pub type TerminalScrollbar = ScrollbarC;

/// C: GhosttyTerminalData
// pub const TerminalData = enum(c_int) { ... };
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalData {
    Invalid = 0,
    Cols = 1,
    Rows = 2,
    CursorX = 3,
    CursorY = 4,
    CursorPendingWrap = 5,
    ActiveScreen = 6,
    CursorVisible = 7,
    KittyKeyboardFlags = 8,
    Scrollbar = 9,
    CursorStyle = 10,
    MouseTracking = 11,
    Title = 12,
    Pwd = 13,
    TotalRows = 14,
    ScrollbackRows = 15,
    WidthPx = 16,
    HeightPx = 17,
    ColorForeground = 18,
    ColorBackground = 19,
    ColorCursor = 20,
    ColorPalette = 21,
    ColorForegroundDefault = 22,
    ColorBackgroundDefault = 23,
    ColorCursorDefault = 24,
    ColorPaletteDefault = 25,
    KittyImageStorageLimit = 26,
    KittyImageMediumFile = 27,
    KittyImageMediumTempFile = 28,
    KittyImageMediumSharedMem = 29,
    KittyGraphics = 30,
    Selection = 31,
    ViewportActive = 32,
}

// pub fn get(terminal_: Terminal, data: TerminalData, out: ?*anyopaque) ... {
//     if (comptime std.debug.runtime_safety) {
//         _ = std.meta.intToEnum(TerminalData, @intFromEnum(data)) catch { ... return .invalid_value; };
//     }
//     return switch (data) {
//         .invalid => .invalid_value,
//         inline else => |comptime_data| getTyped(terminal_, comptime_data, @ptrCast(@alignCast(out))),
//     };
// }
//
// PORT NOTE: the reference `intToEnum` runtime-safety guard lives on the raw
// `c_int` crossing the C boundary (the `extern "C"` wrapper in lib.rs); a valid
// `TerminalData` here cannot be out of range. The `inline else => getTyped`
// comptime dispatch becomes a direct `get_typed` call (the typed `out` cast is
// performed per-arm inside `get_typed`).
/// # Safety
/// `terminal_`, if non-null, must be a live terminal handle; `out`, if non-null,
/// must point to storage of the type required by `data` (`TerminalData::out_type`).
pub unsafe fn get(terminal_: Terminal, data: TerminalData, out: *mut c_void) -> Result {
    // .invalid => .invalid_value,
    if matches!(data, TerminalData::Invalid) {
        return Result::InvalidValue;
    }
    // inline else => |comptime_data| getTyped(terminal_, comptime_data, @ptrCast(@alignCast(out)))
    unsafe { get_typed(terminal_, data, out) }
}

// fn getTyped(terminal_: Terminal, comptime data: TerminalData, out: *data.OutType()) Result {
//     const t: *ZigTerminal = (terminal_ orelse return .invalid_value).terminal;
//     switch (data) { ... }
//     return .success;
// }
//
// PORT NOTE: the reference is a `comptime`-monomorphized switch whose `out` is
// typed per arm (`*data.OutType()`). rustty erases `out` to `*mut c_void` and
// casts to the arm's concrete pointer type before the write. Arms whose
// `OutType` projects a still-deferred vt-c type (`color.RGB.C`/`PaletteC`,
// `style_c.Style`, `selection_c.CSelection`, kitty image/graphics) land in the
// later get/set sub-batch; they remain `todo!()` here.
//
// # Safety
// `terminal_`, if non-null, is a live wrapper; `out` points at storage of the
// type required by `data`.
unsafe fn get_typed(terminal_: Terminal, data: TerminalData, out: *mut c_void) -> Result {
    // const t: *ZigTerminal = (terminal_ orelse return .invalid_value).terminal;
    if terminal_.is_null() {
        return Result::InvalidValue;
    }
    // PORT NOTE (SOUNDNESS): the reference reads through `*ZigTerminal` and its
    // `screens.active: *Screen`. We mirror that with RAW pointers and never form
    // a `&mut ZigTerminal`/`&mut Screen`: the stream handler aliases this same
    // terminal via a stored raw pointer (created in `vt_handler`), and a Unique
    // (`&mut`) retag over the terminal would invalidate that aliasing tag and
    // make the next `vt_write` UB (the rust-mut-args-strongly-protected rule).
    // All reads here are shared (`&`/scalar) through the raw ptr; the active
    // screen is reached via the raw `screens.active` pointer.
    let t: *mut ZigTerminal = unsafe { (*terminal_).terminal };
    let active: *mut rustty_vt::screen::Screen = unsafe { (*t).screens.active };

    match data {
        // .invalid => return .invalid_value,
        TerminalData::Invalid => return Result::InvalidValue,
        // .cols => out.* = t.cols,
        TerminalData::Cols => unsafe { *(out as *mut size::CellCountInt) = (*t).cols },
        // .rows => out.* = t.rows,
        TerminalData::Rows => unsafe { *(out as *mut size::CellCountInt) = (*t).rows },
        // .cursor_x => out.* = t.screens.active.cursor.x,
        TerminalData::CursorX => unsafe { *(out as *mut size::CellCountInt) = (*active).cursor.x },
        // .cursor_y => out.* = t.screens.active.cursor.y,
        TerminalData::CursorY => unsafe { *(out as *mut size::CellCountInt) = (*active).cursor.y },
        // .cursor_pending_wrap => out.* = t.screens.active.cursor.pending_wrap,
        TerminalData::CursorPendingWrap => unsafe {
            *(out as *mut bool) = (*active).cursor.pending_wrap
        },
        // .active_screen => out.* = t.screens.active_key,
        TerminalData::ActiveScreen => unsafe {
            *(out as *mut TerminalScreen) = (*t).screens.active_key
        },
        // .cursor_visible => out.* = t.modes.get(.cursor_visible),
        TerminalData::CursorVisible => unsafe {
            *(out as *mut bool) = (*t).modes.get(modes::Mode::CursorVisible)
        },
        // .kitty_keyboard_flags => out.* = @as(u8, t.screens.active.kitty_keyboard.current().int()),
        TerminalData::KittyKeyboardFlags => unsafe {
            *(out as *mut u8) = (*active).kitty_keyboard.current().int()
        },
        // .mouse_tracking => out.* = t.modes.get(.mouse_event_x10) or ... or any,
        TerminalData::MouseTracking => unsafe {
            *(out as *mut bool) = (*t).modes.get(modes::Mode::MouseEventX10)
                || (*t).modes.get(modes::Mode::MouseEventNormal)
                || (*t).modes.get(modes::Mode::MouseEventButton)
                || (*t).modes.get(modes::Mode::MouseEventAny)
        },
        // .total_rows => out.* = t.screens.active.pages.total_rows,
        TerminalData::TotalRows => unsafe { *(out as *mut usize) = (*active).pages.total_rows },
        // .scrollback_rows => out.* = t.screens.active.pages.total_rows - t.rows,
        TerminalData::ScrollbackRows => {
            let total = unsafe { (*active).pages.total_rows };
            let rows = unsafe { (*t).rows } as usize;
            unsafe { *(out as *mut usize) = total - rows };
        }
        // .width_px => out.* = t.width_px,
        TerminalData::WidthPx => unsafe { *(out as *mut u32) = (*t).width_px },
        // .height_px => out.* = t.height_px,
        TerminalData::HeightPx => unsafe { *(out as *mut u32) = (*t).height_px },
        // .viewport_active => out.* = t.screens.active.pages.viewport == .active,
        TerminalData::ViewportActive => unsafe {
            *(out as *mut bool) = matches!(
                (*active).pages.viewport,
                rustty_vt::pagelist::Viewport::Active
            )
        },

        // PORT NOTE: the following arms project still-deferred vt-c types
        // (`scrollbar`/`cursor_style`/`title`/`pwd`/`color_*`/kitty image+graphics/
        // `selection`); they fill in the get/set sub-batch.
        TerminalData::Scrollbar => todo!("vt-c getTyped scrollbar"),
        TerminalData::CursorStyle => todo!("vt-c getTyped cursor_style"),
        TerminalData::Title => todo!("vt-c getTyped title"),
        TerminalData::Pwd => todo!("vt-c getTyped pwd"),
        TerminalData::ColorForeground => todo!("vt-c getTyped color_foreground"),
        TerminalData::ColorBackground => todo!("vt-c getTyped color_background"),
        TerminalData::ColorCursor => todo!("vt-c getTyped color_cursor"),
        TerminalData::ColorPalette => todo!("vt-c getTyped color_palette"),
        TerminalData::ColorForegroundDefault => todo!("vt-c getTyped color_foreground_default"),
        TerminalData::ColorBackgroundDefault => todo!("vt-c getTyped color_background_default"),
        TerminalData::ColorCursorDefault => todo!("vt-c getTyped color_cursor_default"),
        TerminalData::ColorPaletteDefault => todo!("vt-c getTyped color_palette_default"),
        TerminalData::KittyImageStorageLimit => todo!("vt-c getTyped kitty_image_storage_limit"),
        TerminalData::KittyImageMediumFile => todo!("vt-c getTyped kitty_image_medium_file"),
        TerminalData::KittyImageMediumTempFile => {
            todo!("vt-c getTyped kitty_image_medium_temp_file")
        }
        TerminalData::KittyImageMediumSharedMem => {
            todo!("vt-c getTyped kitty_image_medium_shared_mem")
        }
        TerminalData::KittyGraphics => todo!("vt-c getTyped kitty_graphics"),
        TerminalData::Selection => todo!("vt-c getTyped selection"),
    }

    Result::Success
}

// pub fn get_multi(terminal_, count, keys, values, out_written) ... {
//     const k = keys orelse return .invalid_value;
//     const v = values orelse return .invalid_value;
//     for (0..count) |i| {
//         const result = get(terminal_, k[i], v[i]);
//         if (result != .success) { if (out_written) |w| w.* = i; return result; }
//     }
//     if (out_written) |w| w.* = count;
//     return .success;
// }
/// # Safety
/// `terminal_`, if non-null, must be a live terminal handle; `keys`/`values`, if
/// non-null, must point to `count` valid elements; `out_written`, if non-null,
/// must be a valid writable pointer.
pub unsafe fn get_multi(
    terminal_: Terminal,
    count: usize,
    keys: *const TerminalData,
    values: *mut *mut c_void,
    out_written: *mut usize,
) -> Result {
    // const k = keys orelse return .invalid_value;
    if keys.is_null() {
        return Result::InvalidValue;
    }
    // const v = values orelse return .invalid_value;
    if values.is_null() {
        return Result::InvalidValue;
    }

    // for (0..count) |i| {
    for i in 0..count {
        // const result = get(terminal_, k[i], v[i]);
        // SAFETY: `keys`/`values` point at `count` valid elements (caller
        // contract).
        let key = unsafe { *keys.add(i) };
        let value = unsafe { *values.add(i) };
        let result = unsafe { get(terminal_, key, value) };
        // if (result != .success) { if (out_written) |w| w.* = i; return result; }
        if result != Result::Success {
            if !out_written.is_null() {
                unsafe { *out_written = i };
            }
            return result;
        }
    }
    // if (out_written) |w| w.* = count;
    if !out_written.is_null() {
        unsafe { *out_written = count };
    }
    Result::Success
}

// pub fn grid_ref(terminal_: Terminal, pt: point.Point.C, out_ref: ?*grid_ref_c.CGridRef) ... {
//     const t: *ZigTerminal = (terminal_ orelse return .invalid_value).terminal;
//     const zig_pt: point.Point = .fromC(pt);
//     const p = t.screens.active.pages.pin(zig_pt) orelse return .invalid_value;
//     if (out_ref) |out| out.* = grid_ref_c.CGridRef.fromPin(p);
//     return .success;
// }
/// # Safety
/// `terminal_`, if non-null, must be a live terminal handle; `out_ref`, if
/// non-null, must be a valid writable pointer to a `CGridRef`.
pub unsafe fn grid_ref(terminal_: Terminal, pt: PointC, out_ref: *mut CGridRef) -> Result {
    // const t: *ZigTerminal = (terminal_ orelse return .invalid_value).terminal;
    if terminal_.is_null() {
        return Result::InvalidValue;
    }
    // SAFETY: `terminal_` is a live wrapper; reads go through raw ptrs (never a
    // `&mut ZigTerminal`/`&mut Screen` that would invalidate the stream handler's
    // aliasing tag — the rust-mut-args-strongly-protected discipline).
    let t: *mut ZigTerminal = unsafe { (*terminal_).terminal };
    let active: *mut rustty_vt::screen::Screen = unsafe { (*t).screens.active };
    // const zig_pt: point.Point = .fromC(pt);
    let zig_pt = pt.from_c();
    // const p = t.screens.active.pages.pin(zig_pt) orelse return .invalid_value;
    let p = match unsafe { (*active).pages.pin(zig_pt) } {
        Some(p) => p,
        None => return Result::InvalidValue,
    };
    // if (out_ref) |out| out.* = grid_ref_c.CGridRef.fromPin(p);
    if !out_ref.is_null() {
        // SAFETY: `out_ref` is a valid writable `CGridRef`.
        unsafe { *out_ref = CGridRef::from_pin(p) };
    }
    // return .success;
    Result::Success
}

// pub fn grid_ref_track(terminal_, pt: point.Point.C, out_ref: ?*grid_ref_tracked_c.CTrackedGridRef) ... {
//     const wrapper = terminal_ orelse return .invalid_value;
//     const out = out_ref orelse return .invalid_value;
//     out.* = null;
//     const t: *ZigTerminal = wrapper.terminal;
//     const list = &t.screens.active.pages;
//     const p = list.pin(.fromC(pt)) orelse return .invalid_value;
//     const tracked_pin = list.trackPin(p) catch return .out_of_memory;
//     const alloc = t.gpa();
//     const ref = alloc.create(grid_ref_tracked_c.TrackedGridRef) catch { list.untrackPin(tracked_pin); return .out_of_memory; };
//     ref.* = .{ .alloc, .terminal = wrapper, .screen_key = t.screens.active_key, .screen_generation = ..., .pin = tracked_pin };
//     wrapper.tracked_grid_refs.putNoClobber(alloc, ref, {}) catch { list.untrackPin(tracked_pin); alloc.destroy(ref); return .out_of_memory; };
//     out.* = ref;
//     return .success;
// }
/// # Safety
/// `terminal_`, if non-null, must be a live terminal handle; `out_ref`, if
/// non-null, must be a valid writable pointer to a `CTrackedGridRef`.
pub unsafe fn grid_ref_track(
    terminal_: Terminal,
    pt: PointC,
    out_ref: *mut CTrackedGridRef,
) -> Result {
    // const wrapper = terminal_ orelse return .invalid_value;
    if terminal_.is_null() {
        return Result::InvalidValue;
    }
    // const out = out_ref orelse return .invalid_value;
    if out_ref.is_null() {
        return Result::InvalidValue;
    }
    // out.* = null;
    // SAFETY: `out_ref` is a non-null valid writable `CTrackedGridRef`.
    unsafe { *out_ref = std::ptr::null_mut() };

    // const t: *ZigTerminal = wrapper.terminal;
    // SAFETY: `terminal_` is a live wrapper; raw-ptr reads only.
    let t: *mut ZigTerminal = unsafe { (*terminal_).terminal };
    let active: *mut rustty_vt::screen::Screen = unsafe { (*t).screens.active };
    // const list = &t.screens.active.pages;
    let list: *mut rustty_vt::pagelist::PageList =
        unsafe { core::ptr::addr_of_mut!((*active).pages) };
    // const p = list.pin(.fromC(pt)) orelse return .invalid_value;
    let p = match unsafe { (*list).pin(pt.from_c()) } {
        Some(p) => p,
        None => return Result::InvalidValue,
    };
    // const tracked_pin = list.trackPin(p) catch return .out_of_memory;
    let tracked_pin = match unsafe { (*list).track_pin(p) } {
        Ok(tp) => tp,
        Err(_) => return Result::OutOfMemory,
    };

    // const alloc = t.gpa();
    let alloc = unsafe { (*t).gpa() };
    // const ref = alloc.create(grid_ref_tracked_c.TrackedGridRef) catch { list.untrackPin(tracked_pin); return .out_of_memory; };
    // PORT NOTE: `alloc.create` is the per-terminal allocator; rustty heap-boxes
    // the `TrackedGridRef` (the `gpa()` is `Global`). `Box::new` does not return a
    // recoverable OOM (it aborts), so the `catch return .out_of_memory` arm is
    // unreachable — matching the reference's only-on-OOM cleanup path.
    let active_key = unsafe { (*t).screens.active_key };
    let generation = unsafe { (*t).screens.generation(active_key) };
    // ref.* = .{ .alloc, .terminal = wrapper, .screen_key, .screen_generation, .pin = tracked_pin };
    let ref_ptr: *mut TrackedGridRef = Box::into_raw(Box::new(TrackedGridRef {
        alloc,
        terminal: terminal_,
        screen_key: active_key,
        screen_generation: generation,
        pin: tracked_pin,
    }));

    // Store the tracked ref in the terminal so that when we free
    // the terminal the tracked ref can be detached safely.
    // wrapper.tracked_grid_refs.putNoClobber(alloc, ref, {}) catch { ... };
    // PORT NOTE: `putNoClobber` asserts the key is absent (a fresh heap ptr is
    // always unique); the rustty `HashSet::insert` returns `true` for new keys.
    // SAFETY: `tracked_grid_refs` is a sibling field of the live wrapper.
    unsafe { (*terminal_).tracked_grid_refs.insert(ref_ptr) };

    // out.* = ref;
    unsafe { *out_ref = ref_ptr };
    // return .success;
    Result::Success
}

// pub fn point_from_grid_ref(terminal_, ref: *const grid_ref_c.CGridRef, tag: point.Tag, out: ?*point.Coordinate) ... {
//     const t: *ZigTerminal = (terminal_ orelse return .invalid_value).terminal;
//     const p = ref.toPin() orelse return .invalid_value;
//     const pt = t.screens.active.pages.pointFromPin(tag, p) orelse return .no_value;
//     if (out) |o| o.* = pt.coord();
//     return .success;
// }
/// # Safety
/// `terminal_`, if non-null, must be a live terminal handle; `ref` must point to
/// a valid `CGridRef`; `out`, if non-null, must be a valid writable pointer to a
/// `Coordinate`.
pub unsafe fn point_from_grid_ref(
    terminal_: Terminal,
    ref_: *const CGridRef,
    tag: Tag,
    out: *mut Coordinate,
) -> Result {
    // const t: *ZigTerminal = (terminal_ orelse return .invalid_value).terminal;
    if terminal_.is_null() {
        return Result::InvalidValue;
    }
    // SAFETY: `terminal_` is a live wrapper; raw-ptr reads only.
    let t: *mut ZigTerminal = unsafe { (*terminal_).terminal };
    let active: *mut rustty_vt::screen::Screen = unsafe { (*t).screens.active };
    // const p = ref.toPin() orelse return .invalid_value;
    let p = match unsafe { (*ref_).to_pin() } {
        Some(p) => p,
        None => return Result::InvalidValue,
    };
    // const pt = t.screens.active.pages.pointFromPin(tag, p) orelse return .no_value;
    let pt = match unsafe { (*active).pages.point_from_pin(tag, p) } {
        Some(pt) => pt,
        None => return Result::NoValue,
    };
    // if (out) |o| o.* = pt.coord();
    if !out.is_null() {
        // SAFETY: `out` is a valid writable `Coordinate`.
        unsafe { *out = pt.coord() };
    }
    // return .success;
    Result::Success
}

// pub fn free(terminal_: Terminal) ... {
//     const wrapper = terminal_ orelse return;
//     const t = wrapper.terminal;
//     const alloc = t.gpa();
//     for (wrapper.tracked_grid_refs.keys()) |ref| ref.terminal = null;
//     wrapper.tracked_grid_refs.deinit(alloc);
//     wrapper.stream.deinit();
//     t.deinit(alloc);
//     alloc.destroy(t);
//     alloc.destroy(wrapper);
// }
//
// PORT NOTE (D3 lifecycle): the reference frees `t` and `wrapper` via the C
// allocator (`alloc.destroy`). rustty allocated both via `Box::into_raw` on the
// Rust global heap (see `new_`), so we reclaim them with `Box::from_raw`. The
// terminal is freed AFTER the stream (the stream's handler aliases it), matching
// the reference order (`t.deinit` + `destroy(t)` after `stream.deinit`).
/// # Safety
/// `terminal_`, if non-null, must be a live terminal handle previously returned
/// by `new` and not yet freed.
pub unsafe fn free(terminal_: Terminal) {
    // const wrapper = terminal_ orelse return;
    if terminal_.is_null() {
        return;
    }
    // const wrapper = ...; reclaim the heap wrapper.
    // SAFETY: `terminal_` is a `new`-returned wrapper pointer, not yet freed
    // (caller contract).
    let mut wrapper: Box<TerminalWrapper> = unsafe { Box::from_raw(terminal_) };

    // const t = wrapper.terminal;
    let t: *mut ZigTerminal = wrapper.terminal;
    // const alloc = t.gpa();
    // SAFETY: `t` is the wrapper's live, owned terminal.
    let alloc = unsafe { (*t).gpa() };

    // for (wrapper.tracked_grid_refs.keys()) |ref| ref.terminal = null;
    for &ref_ in wrapper.tracked_grid_refs.iter() {
        // SAFETY: each tracked-ref pointer was inserted by `grid_ref_track` and
        // is still live (it back-references this wrapper); detach it so a later
        // tracked-ref free does not dereference the freed terminal.
        unsafe { (*ref_).terminal = std::ptr::null_mut() };
    }
    // wrapper.tracked_grid_refs.deinit(alloc); — the HashSet frees on drop.
    wrapper.tracked_grid_refs.clear();

    // wrapper.stream.deinit();
    wrapper.stream.deinit();

    // t.deinit(alloc);
    unsafe { (*t).deinit(alloc) };
    // alloc.destroy(t); — reclaim the heap terminal AFTER the stream's handler
    // (which aliased it) is torn down.
    // SAFETY: `t` was `Box::into_raw`'d in `new_` and is now uniquely owned.
    drop(unsafe { Box::from_raw(t) });
    // alloc.destroy(wrapper); — `wrapper` (the Box) drops at end of scope.
}

// PORT NOTE: `TerminalData` aliases `color.PaletteC` / `color.RGB.C` /
// `style_c.Style` / `selection_c.CSelection` / `KittyGraphics` in its
// `OutType`, and `Option` aliases the same in `InType`. Those projections are
// resolved by the fill sub-batch's `setTyped`/`getTyped` bodies (and
// `color::PaletteC` itself is a DEFERRED vt-c projection per the recon); they
// are intentionally NOT imported in this types+signatures skeleton.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::with_test_allocator;

    // PORT NOTE: the reference passes `&lib.alloc.test_allocator` to `new`.
    // rustty has no global test allocator (leaks are caught by Miri); the parity
    // analog is `with_test_allocator`, a C `Allocator` backed by the process
    // global. The reference `t.?.terminal.plainString(testing.allocator)` becomes
    // `(*(*t).terminal).plain_string(Global)`; the returned `Vec<u8>` frees on
    // drop (the reference's `testing.allocator.free(str)`).

    fn new_t(
        alloc: &CAllocator,
        cols: size::CellCountInt,
        rows: size::CellCountInt,
        sb: usize,
    ) -> Terminal {
        let mut t: Terminal = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe {
            new(
                alloc as *const CAllocator,
                &mut t,
                Options {
                    cols,
                    rows,
                    max_scrollback: sb,
                },
            )
        });
        t
    }

    fn plain_string(t: Terminal) -> Vec<u8> {
        // SAFETY: `t` is a live terminal from `new_t`.
        let zt = unsafe { &mut *(*t).terminal };
        zt.plain_string(allocator_api2::alloc::Global).unwrap()
    }

    // test "new/free"
    #[test]
    fn new_free() {
        with_test_allocator(|alloc| {
            // var t: Terminal = null;
            let mut t: Terminal = std::ptr::null_mut();
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ .cols=80, .rows=24, .max_scrollback=10_000 }));
            assert_eq!(Result::Success, unsafe {
                new(
                    alloc as *const CAllocator,
                    &mut t,
                    Options {
                        cols: 80,
                        rows: 24,
                        max_scrollback: 10_000,
                    },
                )
            });
            // try testing.expect(t != null);
            assert!(!t.is_null());
            // free(t);
            unsafe { free(t) };
        });
    }

    // test "new invalid value"
    #[test]
    fn new_invalid_value() {
        with_test_allocator(|alloc| {
            let mut t: Terminal = std::ptr::null_mut();
            // cols == 0
            assert_eq!(Result::InvalidValue, unsafe {
                new(
                    alloc as *const CAllocator,
                    &mut t,
                    Options {
                        cols: 0,
                        rows: 24,
                        max_scrollback: 10_000,
                    },
                )
            });
            assert!(t.is_null());

            // rows == 0
            assert_eq!(Result::InvalidValue, unsafe {
                new(
                    alloc as *const CAllocator,
                    &mut t,
                    Options {
                        cols: 80,
                        rows: 0,
                        max_scrollback: 10_000,
                    },
                )
            });
            assert!(t.is_null());
        });
    }

    // test "free null"
    #[test]
    fn free_null() {
        unsafe { free(std::ptr::null_mut()) };
    }

    // test "scroll_viewport"
    #[test]
    fn scroll_viewport_test() {
        with_test_allocator(|alloc| {
            let t = new_t(alloc, 5, 2, 10_000);

            let mut viewport_active: bool = false;
            assert_eq!(Result::Success, unsafe {
                get(
                    t,
                    TerminalData::ViewportActive,
                    &mut viewport_active as *mut bool as *mut c_void,
                )
            });
            assert!(viewport_active);

            // Write "hello" on the first line
            unsafe { vt_write(t, b"hello".as_ptr(), 5) };

            // Push "hello" into scrollback with 3 newlines (index = ESC D)
            unsafe { vt_write(t, b"\x1bD\x1bD\x1bD".as_ptr(), 6) };
            // Viewport should be empty now since hello scrolled off
            assert_eq!(b"", plain_string(t).as_slice());

            // Scroll to top: "hello" should be visible again
            unsafe {
                scroll_viewport(
                    t,
                    ScrollViewportC {
                        tag: ScrollViewportTag::Top,
                        value: ScrollViewportCValue { _padding: [0; 2] },
                    },
                )
            };
            assert_eq!(Result::Success, unsafe {
                get(
                    t,
                    TerminalData::ViewportActive,
                    &mut viewport_active as *mut bool as *mut c_void,
                )
            });
            assert!(!viewport_active);
            assert_eq!(b"hello", plain_string(t).as_slice());

            // Scroll to bottom: viewport should be empty again
            unsafe {
                scroll_viewport(
                    t,
                    ScrollViewportC {
                        tag: ScrollViewportTag::Bottom,
                        value: ScrollViewportCValue { _padding: [0; 2] },
                    },
                )
            };
            assert_eq!(Result::Success, unsafe {
                get(
                    t,
                    TerminalData::ViewportActive,
                    &mut viewport_active as *mut bool as *mut c_void,
                )
            });
            assert!(viewport_active);
            assert_eq!(b"", plain_string(t).as_slice());

            // Scroll up by delta to bring "hello" back into view
            unsafe {
                scroll_viewport(
                    t,
                    ScrollViewportC {
                        tag: ScrollViewportTag::Delta,
                        value: ScrollViewportCValue { delta: -3 },
                    },
                )
            };
            assert_eq!(Result::Success, unsafe {
                get(
                    t,
                    TerminalData::ViewportActive,
                    &mut viewport_active as *mut bool as *mut c_void,
                )
            });
            assert!(!viewport_active);
            assert_eq!(b"hello", plain_string(t).as_slice());

            unsafe { free(t) };
        });
    }

    // test "scroll_viewport null"
    #[test]
    fn scroll_viewport_null() {
        unsafe {
            scroll_viewport(
                std::ptr::null_mut(),
                ScrollViewportC {
                    tag: ScrollViewportTag::Top,
                    value: ScrollViewportCValue { _padding: [0; 2] },
                },
            )
        };
    }

    // test "reset"
    #[test]
    fn reset_test() {
        with_test_allocator(|alloc| {
            let t = new_t(alloc, 80, 24, 10_000);

            unsafe { vt_write(t, b"Hello".as_ptr(), 5) };
            unsafe { reset(t) };

            assert_eq!(b"", plain_string(t).as_slice());
            unsafe { free(t) };
        });
    }

    // test "reset null"
    #[test]
    fn reset_null() {
        unsafe { reset(std::ptr::null_mut()) };
    }

    // test "resize"
    #[test]
    fn resize_test() {
        with_test_allocator(|alloc| {
            let t = new_t(alloc, 80, 24, 10_000);

            assert_eq!(Result::Success, unsafe { resize(t, 40, 12, 9, 18) });
            assert_eq!(40, unsafe { (*(*t).terminal).cols });
            assert_eq!(12, unsafe { (*(*t).terminal).rows });
            unsafe { free(t) };
        });
    }

    // test "resize null"
    #[test]
    fn resize_null() {
        assert_eq!(Result::InvalidValue, unsafe {
            resize(std::ptr::null_mut(), 80, 24, 9, 18)
        });
    }

    // test "resize invalid value"
    #[test]
    fn resize_invalid_value() {
        with_test_allocator(|alloc| {
            let t = new_t(alloc, 80, 24, 10_000);

            assert_eq!(Result::InvalidValue, unsafe { resize(t, 0, 24, 9, 18) });
            assert_eq!(Result::InvalidValue, unsafe { resize(t, 80, 0, 9, 18) });
            unsafe { free(t) };
        });
    }

    // test "resize shrinks both axes with cursor at bottom"
    #[test]
    fn resize_shrinks_both_axes_with_cursor_at_bottom() {
        with_test_allocator(|alloc| {
            let t = new_t(alloc, 80, 24, 0);

            // CSI 24;1H -> park the cursor on the bottom row (1-based).
            let mv = b"\x1b[24;1H";
            unsafe { vt_write(t, mv.as_ptr(), mv.len()) };

            // Shrink both axes; pre-resize cursor.y sits past the new bottom row.
            assert_eq!(Result::Success, unsafe { resize(t, 79, 23, 8, 16) });
            assert_eq!(79, unsafe { (*(*t).terminal).cols });
            assert_eq!(23, unsafe { (*(*t).terminal).rows });
            unsafe { free(t) };
        });
    }

    // test "mode_get and mode_set"
    #[test]
    fn mode_get_and_mode_set() {
        with_test_allocator(|alloc| {
            let t = new_t(alloc, 80, 24, 0);

            let mut value: bool = false;

            // DEC mode 25 (cursor_visible) defaults to true
            let cursor_visible = modes::ModeTag {
                value: 25,
                ansi: false,
            }
            .to_backing();
            assert_eq!(Result::Success, unsafe {
                mode_get(t, cursor_visible, &mut value)
            });
            assert!(value);

            // Set it to false
            assert_eq!(Result::Success, unsafe {
                mode_set(t, cursor_visible, false)
            });
            assert_eq!(Result::Success, unsafe {
                mode_get(t, cursor_visible, &mut value)
            });
            assert!(!value);

            // ANSI mode 4 (insert) defaults to false
            let insert = modes::ModeTag {
                value: 4,
                ansi: true,
            }
            .to_backing();
            assert_eq!(Result::Success, unsafe { mode_get(t, insert, &mut value) });
            assert!(!value);

            assert_eq!(Result::Success, unsafe { mode_set(t, insert, true) });
            assert_eq!(Result::Success, unsafe { mode_get(t, insert, &mut value) });
            assert!(value);
            unsafe { free(t) };
        });
    }

    // test "mode_get null"
    #[test]
    fn mode_get_null() {
        let mut value: bool = false;
        let tag = modes::ModeTag {
            value: 25,
            ansi: false,
        }
        .to_backing();
        assert_eq!(Result::InvalidValue, unsafe {
            mode_get(std::ptr::null_mut(), tag, &mut value)
        });
    }

    // test "mode_set null"
    #[test]
    fn mode_set_null() {
        let tag = modes::ModeTag {
            value: 25,
            ansi: false,
        }
        .to_backing();
        assert_eq!(Result::InvalidValue, unsafe {
            mode_set(std::ptr::null_mut(), tag, true)
        });
    }

    // test "mode_get unknown mode"
    #[test]
    fn mode_get_unknown_mode() {
        with_test_allocator(|alloc| {
            let t = new_t(alloc, 80, 24, 0);

            let mut value: bool = false;
            let unknown = modes::ModeTag {
                value: 9999,
                ansi: false,
            }
            .to_backing();
            assert_eq!(Result::InvalidValue, unsafe {
                mode_get(t, unknown, &mut value)
            });
            unsafe { free(t) };
        });
    }

    // test "mode_set unknown mode"
    #[test]
    fn mode_set_unknown_mode() {
        with_test_allocator(|alloc| {
            let t = new_t(alloc, 80, 24, 0);

            let unknown = modes::ModeTag {
                value: 9999,
                ansi: false,
            }
            .to_backing();
            assert_eq!(Result::InvalidValue, unsafe { mode_set(t, unknown, true) });
            unsafe { free(t) };
        });
    }

    // test "vt_write"
    #[test]
    fn vt_write_test() {
        with_test_allocator(|alloc| {
            let t = new_t(alloc, 80, 24, 10_000);

            unsafe { vt_write(t, b"Hello".as_ptr(), 5) };

            assert_eq!(b"Hello", plain_string(t).as_slice());
            unsafe { free(t) };
        });
    }

    // test "vt_write split escape sequence"
    #[test]
    fn vt_write_split_escape_sequence() {
        with_test_allocator(|alloc| {
            let t = new_t(alloc, 80, 24, 10_000);

            // Split ESC from the rest of the CSI sequence.
            unsafe { vt_write(t, b"Hello \x1b".as_ptr(), 7) };
            unsafe { vt_write(t, b"[1mBold\x1b[0m".as_ptr(), 10) };

            // If the escape sequence leaked, we'd see "[1mBold" as literal text.
            assert_eq!(b"Hello Bold", plain_string(t).as_slice());
            unsafe { free(t) };
        });
    }

    // test "vt_write split combining mark after base at right edge"
    #[test]
    fn vt_write_split_combining_mark_after_base_at_right_edge() {
        with_test_allocator(|alloc| {
            let t = new_t(alloc, 2, 2, 0);

            // Put "å" in the final column, then send its combining low line in a
            // separate write so the mark arrives while the cursor has a pending wrap.
            unsafe { vt_write(t, "xå".as_bytes().as_ptr(), 3) };
            unsafe { vt_write(t, b"\xcc\xb2".as_ptr(), 2) };

            assert_eq!("xå̲".as_bytes(), plain_string(t).as_slice());
            unsafe { free(t) };
        });
    }

    // test "get cols and rows"
    #[test]
    fn get_cols_and_rows() {
        with_test_allocator(|alloc| {
            let t = new_t(alloc, 80, 24, 0);

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
            let t = new_t(alloc, 80, 24, 0);

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
            let t = new_t(alloc, 80, 24, 0);

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
            let cursor_visible_mode = modes::ModeTag {
                value: 25,
                ansi: false,
            }
            .to_backing();
            assert_eq!(Result::Success, unsafe {
                mode_set(t, cursor_visible_mode, false)
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

    // test "get active_screen"
    #[test]
    fn get_active_screen() {
        with_test_allocator(|alloc| {
            let t = new_t(alloc, 80, 24, 0);

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
            let t = new_t(alloc, 80, 24, 0);

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
            let t = new_t(alloc, 80, 24, 0);

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

            // Enable X10 mouse (DEC mode 9)
            let x10_mode = modes::ModeTag {
                value: 9,
                ansi: false,
            }
            .to_backing();
            assert_eq!(Result::Success, unsafe { mode_set(t, x10_mode, true) });
            assert_eq!(Result::Success, get_tracking(t, &mut tracking));
            assert!(tracking);

            // Disable X10, enable normal mouse (DEC mode 1000)
            assert_eq!(Result::Success, unsafe { mode_set(t, x10_mode, false) });
            let normal_mode = modes::ModeTag {
                value: 1000,
                ansi: false,
            }
            .to_backing();
            assert_eq!(Result::Success, unsafe { mode_set(t, normal_mode, true) });
            assert_eq!(Result::Success, get_tracking(t, &mut tracking));
            assert!(tracking);

            // Disable normal, enable button mouse (DEC mode 1002)
            assert_eq!(Result::Success, unsafe { mode_set(t, normal_mode, false) });
            let button_mode = modes::ModeTag {
                value: 1002,
                ansi: false,
            }
            .to_backing();
            assert_eq!(Result::Success, unsafe { mode_set(t, button_mode, true) });
            assert_eq!(Result::Success, get_tracking(t, &mut tracking));
            assert!(tracking);

            // Disable button, enable any mouse (DEC mode 1003)
            assert_eq!(Result::Success, unsafe { mode_set(t, button_mode, false) });
            let any_mode = modes::ModeTag {
                value: 1003,
                ansi: false,
            }
            .to_backing();
            assert_eq!(Result::Success, unsafe { mode_set(t, any_mode, true) });
            assert_eq!(Result::Success, get_tracking(t, &mut tracking));
            assert!(tracking);

            // Disable all - should be false again
            assert_eq!(Result::Success, unsafe { mode_set(t, any_mode, false) });
            assert_eq!(Result::Success, get_tracking(t, &mut tracking));
            assert!(!tracking);
            unsafe { free(t) };
        });
    }

    // test "get total_rows"
    #[test]
    fn get_total_rows() {
        with_test_allocator(|alloc| {
            let t = new_t(alloc, 80, 24, 10_000);

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
            let t = new_t(alloc, 80, 3, 10_000);

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
            let t = new_t(alloc, 80, 24, 0);

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
            let t = new_t(alloc, 80, 24, 0);

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
            let t = new_t(alloc, 80, 24, 0);
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
}
