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
//! fn-ptr). The get/set + grid_ref sub-batches have LANDED (VALIDATION_REPORT
//! M3): `set`/`setTyped`, every `get_typed` arm, and
//! `grid_ref`/`grid_ref_track`/`point_from_grid_ref` are live — zero
//! `todo!()` remains in this crate. The formerly irreducible mutual cluster
//! (terminal<->grid_ref<->grid_ref_tracked<->selection) is fully resolved.

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
use rustty_vt::clipboard;
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
    pub clipboard_write: Option<ClipboardWriteFn>,

    /// Scratch buffer for DA1 feature codes. The device attributes
    /// trampoline converts C feature codes into this buffer and returns
    /// a slice pointing into it. Storing it here ensures the slice
    /// remains valid after the trampoline returns, since the caller
    /// (`reportDeviceAttributes`) reads it before any re-entrant call.
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
            clipboard_write: None,
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

/// A single MIME representation in a clipboard write.
///
/// C: GhosttyClipboardContent -> RusttyClipboardContent (R2)
// pub const ClipboardContent = extern struct { mime: lib.String, data: lib.String };
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ClipboardContent {
    pub mime: LibString,
    pub data: LibString,
}

/// A protocol-neutral request to replace or clear clipboard contents.
///
/// C: GhosttyClipboardWrite -> RusttyClipboardWrite (R2)
// pub const ClipboardWrite = extern struct { size, location, contents, contents_len };
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ClipboardWrite {
    pub size: usize,
    pub location: clipboard::Location,
    /// Null when `contents_len` is 0 (an empty write clears the destination).
    pub contents: *const ClipboardContent,
    pub contents_len: usize,
}

/// C function pointer type for the clipboard_write callback. The request
/// and its contents are borrowed and only valid for the callback duration.
// pub const ClipboardWriteFn = *const fn (Terminal, ?*anyopaque, *const ClipboardWrite) callconv(lib.calling_conv) clipboard.WriteResult;
pub type ClipboardWriteFn =
    extern "C" fn(Terminal, *mut c_void, *const ClipboardWrite) -> clipboard::WriteResult;

/// C-compatible device attributes struct.
/// C: GhosttyDeviceAttributes -> RusttyDeviceAttributes (R2)
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

    // fn clipboardWriteTrampoline(handler: *Handler, write: clipboard.Write) clipboard.WriteResult {
    //     const stream_ptr: *Stream = @fieldParentPtr("handler", handler);
    //     const wrapper: *TerminalWrapper = @fieldParentPtr("stream", stream_ptr);
    //     const func = wrapper.effects.clipboard_write orelse return .unsupported;
    //     ...stack-first (<=4) content conversion, gpa fallback...
    //     return func(@ptrCast(wrapper), wrapper.effects.userdata, &request);
    // }
    pub(crate) fn clipboard_write_trampoline(
        handler: &mut Handler<'static>,
        write: clipboard::Write<'_>,
    ) -> clipboard::WriteResult {
        use clipboard::WriteResult;
        let wrapper = wrapper_from_handler(handler);
        let func = match unsafe { (*wrapper).effects.clipboard_write } {
            Some(f) => f,
            None => return WriteResult::Unsupported,
        };
        let userdata = unsafe { (*wrapper).effects.userdata };

        // Most protocols currently produce one representation, so keep that
        // path allocation-free while supporting arbitrary multi-MIME writes.
        // PORT NOTE: the reference's `gpa().alloc(...) catch return .io_error`
        // OOM arm has no rustty analog — a `Vec` aborts on OOM (DG5 abort
        // policy), so the past-4 path is an infallible `Vec`.
        let to_c = |content: &clipboard::Content<'_>| ClipboardContent {
            mime: LibString { ptr: content.mime.as_ptr(), len: content.mime.len() },
            data: LibString { ptr: content.data.as_ptr(), len: content.data.len() },
        };

        let mut stack_contents: [ClipboardContent; 4] = [ClipboardContent {
            mime: LibString { ptr: std::ptr::null(), len: 0 },
            data: LibString { ptr: std::ptr::null(), len: 0 },
        }; 4];
        let heap_contents: Vec<ClipboardContent>;
        let contents: &[ClipboardContent] = if write.contents.len() <= stack_contents.len() {
            for (dst, content) in stack_contents.iter_mut().zip(write.contents) {
                *dst = to_c(content);
            }
            &stack_contents[..write.contents.len()]
        } else {
            heap_contents = write.contents.iter().map(to_c).collect();
            &heap_contents
        };

        let request = ClipboardWrite {
            size: std::mem::size_of::<ClipboardWrite>(),
            location: write.location,
            contents: if contents.is_empty() { std::ptr::null() } else { contents.as_ptr() },
            contents_len: contents.len(),
        };
        func(wrapper, userdata, &request)
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

/// C: GhosttyTerminalOptions -> RusttyTerminalOptions (R2)
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
    // SB (SOUNDNESS): build the Handler DIRECTLY from the heap box's base raw
    // pointer. Going through `(*terminal_ptr).vt_handler()` would autoref a
    // temporary `&mut ZigTerminal` and the handler would store a child of that
    // temporary's tag — the FIRST later `&mut`-method call on the terminal
    // from any export (e.g. `resize`'s `(*t).gpa()`) would pop it and make
    // every subsequent `vt_write` Stacked-Borrows UB (Miri-caught via the
    // kitty `image_get_multi_success` KAT). With the base tag, transient
    // `&mut` children created by exports die at the next handler use instead
    // of killing it — the rust-mut-args-strongly-protected rule; the
    // reference's `t.vtHandler()` is a plain unprotected Zig pointer copy.
    let mut handler: Handler<'static> = Handler::init(terminal_ptr);

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
        clipboard_write: Some(Effects::clipboard_write_trampoline),
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

/// C: GhosttyTerminalOption -> RusttyTerminalOption (R2)
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
    ClipboardWrite = 26,
    // /// Input type expected for setting the option.
    // pub fn InType(comptime self: Option) type {
    //     return switch (self) {
    //         .userdata => ?*const anyopaque,
    //         .write_pty => ?Effects.WritePtyFn,
    //         .bell => ?Effects.BellFn,
    //         .color_scheme => ?Effects.ColorSchemeFn,
    //         .device_attributes => ?Effects.DeviceAttributesFn,
    //         .enquiry => ?Effects.EnquiryFn,
    //         .xtversion => ?Effects.XtversionFn,
    //         .title_changed => ?Effects.TitleChangedFn,
    //         .pwd_changed => ?Effects.PwdChangedFn,
    //         .size_cb => ?Effects.SizeFn,
    //         .title, .pwd => ?*const lib.String,
    //         .color_foreground, .color_background, .color_cursor => ?*const color.RGB.C,
    //         .color_palette => ?*const color.PaletteC,
    //         .kitty_image_storage_limit => ?*const u64,
    //         .kitty_image_medium_file,
    //         .kitty_image_medium_temp_file,
    //         .kitty_image_medium_shared_mem,
    //         .glyph_protocol,
    //         => ?*const bool,
    //         .apc_max_bytes, .apc_max_bytes_kitty => ?*const usize,
    //         .selection => ?*const selection_c.CSelection,
    //         .default_cursor_style => ?*const TerminalCursorStyle,
    //         .default_cursor_blink => ?*const bool,
    //     };
    // }
    //
    // PORT NOTE: `InType` is a comptime type-returning fn (the input type expected
    // for setting each option). rustty erases `value` to `*const c_void` at the FFI
    // boundary and reads it back as the per-arm type inside `set_typed`, so this
    // comptime projection has no Rust analog; it is preserved verbatim as the
    // reference contract for each arm's expected pointee type.
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
// PORT NOTE (formerly PARTIAL FILL): the `.selection` arm landed first (it is
// exercised by `selection.zig`'s `selection_format_alloc uses active
// selection` test); the get/set sub-batch has since landed every other option
// arm — no `todo!("vt-c set <option>")` remains (VALIDATION_REPORT M3).
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
    // PORT NOTE (SOUNDNESS): the reference is `setTyped(wrapper, ...)`, mutating
    // the public fields `wrapper.effects`, `wrapper.terminal.*`, and
    // `wrapper.stream.handler.*`. rustty reaches each through RAW pointers off
    // `terminal_` (the `*mut TerminalWrapper`) and forms a `&mut` only to the
    // disjoint sub-field it writes — never an overlapping `&mut TerminalWrapper`
    // or `&mut ZigTerminal` that would invalidate the stream handler's stored
    // alias of the terminal (the rust-mut-args-strongly-protected discipline).
    // `value`, for callback-fn arms, IS the C function pointer (the reference's
    // `?Effects.WritePtyFn` etc.); for data arms it points at the typed value.
    let t: *mut ZigTerminal = unsafe { (*terminal_).terminal };

    match option {
        // .userdata => wrapper.effects.userdata = @constCast(value),
        Option_::Userdata => unsafe {
            (*terminal_).effects.userdata = value as *mut c_void;
        },
        // .write_pty => wrapper.effects.write_pty = value,
        Option_::WritePty => unsafe {
            (*terminal_).effects.write_pty =
                std::mem::transmute::<*const c_void, Option<WritePtyFn>>(value);
        },
        // .bell => wrapper.effects.bell = value,
        Option_::Bell => unsafe {
            (*terminal_).effects.bell = std::mem::transmute::<*const c_void, Option<BellFn>>(value);
        },
        // .color_scheme => wrapper.effects.color_scheme = value,
        Option_::ColorScheme => unsafe {
            (*terminal_).effects.color_scheme =
                std::mem::transmute::<*const c_void, Option<ColorSchemeFn>>(value);
        },
        // .device_attributes => wrapper.effects.device_attributes_cb = value,
        Option_::DeviceAttributes => unsafe {
            (*terminal_).effects.device_attributes_cb =
                std::mem::transmute::<*const c_void, Option<DeviceAttributesFn>>(value);
        },
        // .enquiry => wrapper.effects.enquiry = value,
        Option_::Enquiry => unsafe {
            (*terminal_).effects.enquiry =
                std::mem::transmute::<*const c_void, Option<EnquiryFn>>(value);
        },
        // .xtversion => wrapper.effects.xtversion = value,
        Option_::Xtversion => unsafe {
            (*terminal_).effects.xtversion =
                std::mem::transmute::<*const c_void, Option<XtversionFn>>(value);
        },
        // .title_changed => wrapper.effects.title_changed = value,
        Option_::TitleChanged => unsafe {
            (*terminal_).effects.title_changed =
                std::mem::transmute::<*const c_void, Option<TitleChangedFn>>(value);
        },
        // .pwd_changed => wrapper.effects.pwd_changed = value,
        Option_::PwdChanged => unsafe {
            (*terminal_).effects.pwd_changed =
                std::mem::transmute::<*const c_void, Option<PwdChangedFn>>(value);
        },
        // .size_cb => wrapper.effects.size_cb = value,
        Option_::SizeCb => unsafe {
            (*terminal_).effects.size_cb =
                std::mem::transmute::<*const c_void, Option<SizeFn>>(value);
        },
        // .clipboard_write => wrapper.effects.clipboard_write = value,
        Option_::ClipboardWrite => unsafe {
            (*terminal_).effects.clipboard_write =
                std::mem::transmute::<*const c_void, Option<ClipboardWriteFn>>(value);
        },

        // .title => {
        //     const str = if (value) |v| v.ptr[0..v.len] else "";
        //     wrapper.terminal.setTitle(str) catch return .out_of_memory;
        // },
        Option_::Title => {
            // const str = if (value) |v| v.ptr[0..v.len] else "";
            let ptr = value as *const LibString;
            let str_: &[u8] = if !ptr.is_null() {
                let s = unsafe { *ptr };
                unsafe { core::slice::from_raw_parts(s.ptr, s.len) }
            } else {
                b""
            };
            // wrapper.terminal.setTitle(str) catch return .out_of_memory;
            // SAFETY: write the `title` sub-field of the terminal via the raw ptr;
            // `set_title` takes `&mut Terminal` over a region disjoint from the
            // stream handler's alias usage (no concurrent `vt_write` is live).
            if unsafe { (*t).set_title(str_) }.is_err() {
                return Result::OutOfMemory;
            }
        }
        // .pwd => {
        //     const str = if (value) |v| v.ptr[0..v.len] else "";
        //     wrapper.terminal.setPwd(str) catch return .out_of_memory;
        // },
        Option_::Pwd => {
            let ptr = value as *const LibString;
            let str_: &[u8] = if !ptr.is_null() {
                let s = unsafe { *ptr };
                unsafe { core::slice::from_raw_parts(s.ptr, s.len) }
            } else {
                b""
            };
            if unsafe { (*t).set_pwd(str_) }.is_err() {
                return Result::OutOfMemory;
            }
        }

        // .color_foreground => {
        //     wrapper.terminal.colors.foreground.default = if (value) |v| .fromC(v.*) else null;
        //     wrapper.terminal.flags.dirty.palette = true;
        // },
        Option_::ColorForeground => unsafe {
            let ptr = value as *const crate::color::RgbC;
            (*t).colors.foreground.default = if !ptr.is_null() { Some(*ptr) } else { None };
            (*t).flags.dirty.palette = true;
        },
        // .color_background => { ... },
        Option_::ColorBackground => unsafe {
            let ptr = value as *const crate::color::RgbC;
            (*t).colors.background.default = if !ptr.is_null() { Some(*ptr) } else { None };
            (*t).flags.dirty.palette = true;
        },
        // .color_cursor => { ... },
        Option_::ColorCursor => unsafe {
            let ptr = value as *const crate::color::RgbC;
            (*t).colors.cursor.default = if !ptr.is_null() { Some(*ptr) } else { None };
            (*t).flags.dirty.palette = true;
        },
        // .color_palette => {
        //     wrapper.terminal.colors.palette.changeDefault(
        //         if (value) |v| color.paletteZval(v) else color.default,
        //     );
        //     wrapper.terminal.flags.dirty.palette = true;
        // },
        Option_::ColorPalette => unsafe {
            let ptr = value as *const crate::color::PaletteC;
            let def = if !ptr.is_null() {
                crate::color::palette_zval(ptr)
            } else {
                rustty_vt::color::DEFAULT
            };
            (*t).colors.palette.change_default(def);
            (*t).flags.dirty.palette = true;
        },

        // .kitty_image_storage_limit => {
        //     if (comptime !build_options.kitty_graphics) return .success;
        //     const limit: usize = if (value) |v| @intCast(v.*) else 0;
        //     var it = wrapper.terminal.screens.all.iterator();
        //     while (it.next()) |entry| {
        //         const screen = entry.value.*;
        //         screen.kitty_images.setLimit(screen.alloc, screen, limit) catch return .out_of_memory;
        //     }
        // },
        Option_::KittyImageStorageLimit => {
            // if (comptime !build_options.kitty_graphics) return .success;
            if !rustty_vt::options::KITTY_GRAPHICS {
                return Result::Success;
            }
            // const limit: usize = if (value) |v| @intCast(v.*) else 0;
            let ptr = value as *const u64;
            let limit: usize = if !ptr.is_null() {
                unsafe { *ptr as usize }
            } else {
                0
            };
            // var it = wrapper.terminal.screens.all.iterator();
            // while (it.next()) |entry| { const screen = entry.value.*; ... }
            for slot in unsafe { (*t).screens.all.iter() } {
                if let Some(screen) = *slot {
                    // PORT NOTE: `setLimit(&mut self, alloc, s: &mut Screen, limit)`
                    // aliases `screen.kitty_images` (the receiver) with `screen`
                    // (the `s` arg). Mirror `ScreenSet::init`: MOVE `kitty_images`
                    // out, run `set_limit` against the now-disjoint `&mut Screen`,
                    // then move it back.
                    let alloc = unsafe { (*screen).alloc };
                    let mut images = core::mem::take(unsafe { &mut (*screen).kitty_images });
                    let res = images.set_limit(alloc, unsafe { &mut *screen }, limit);
                    unsafe { (*screen).kitty_images = images };
                    if res.is_err() {
                        return Result::OutOfMemory;
                    }
                }
            }
        }
        // .kitty_image_medium_file,
        // .kitty_image_medium_temp_file,
        // .kitty_image_medium_shared_mem,
        // => {
        //     if (comptime !build_options.kitty_graphics) return .success;
        //     const val = (value orelse return .success).*;
        //     var it = wrapper.terminal.screens.all.iterator();
        //     while (it.next()) |entry| {
        //         const screen = entry.value.*;
        //         switch (option) {
        //             .kitty_image_medium_file => screen.kitty_images.image_limits.file = val,
        //             .kitty_image_medium_temp_file => screen.kitty_images.image_limits.temporary_file = val,
        //             .kitty_image_medium_shared_mem => screen.kitty_images.image_limits.shared_memory = val,
        //             else => unreachable,
        //         }
        //     }
        // },
        Option_::KittyImageMediumFile
        | Option_::KittyImageMediumTempFile
        | Option_::KittyImageMediumSharedMem => {
            if !rustty_vt::options::KITTY_GRAPHICS {
                return Result::Success;
            }
            // const val = (value orelse return .success).*;
            let ptr = value as *const bool;
            if ptr.is_null() {
                return Result::Success;
            }
            let val = unsafe { *ptr };
            for slot in unsafe { (*t).screens.all.iter() } {
                if let Some(screen) = *slot {
                    let limits = unsafe { &mut (*screen).kitty_images.image_limits };
                    match option {
                        Option_::KittyImageMediumFile => limits.file = val,
                        Option_::KittyImageMediumTempFile => limits.temporary_file = val,
                        Option_::KittyImageMediumSharedMem => limits.shared_memory = val,
                        _ => unreachable!(),
                    }
                }
            }
        }

        // .apc_max_bytes => {
        //     wrapper.stream.handler.apc_handler.max_bytes = if (value) |ptr|
        //         .initFull(ptr.*)
        //     else
        //         .{};
        // },
        Option_::ApcMaxBytes => {
            // PORT NOTE: the reference `EnumMap` ports to the always-full
            // `[usize; Protocol::COUNT]` array (apc.rs). `.initFull(ptr.*)` sets
            // every protocol to the value; the `else .{}` branch (an EnumMap with
            // no entries) is, under the always-full port, the field's struct
            // default — `default_max_bytes` per protocol.
            let handler = unsafe { &mut (*terminal_).stream.handler };
            let ptr = value as *const usize;
            if !ptr.is_null() {
                let v = unsafe { *ptr };
                handler.apc_handler.max_bytes = [v; rustty_vt::apc::Protocol::COUNT];
            } else {
                handler.apc_handler.max_bytes = [
                    rustty_vt::apc::Protocol::default_max_bytes(rustty_vt::apc::Protocol::Kitty),
                    rustty_vt::apc::Protocol::default_max_bytes(rustty_vt::apc::Protocol::Glyph),
                ];
            }
        }
        // .apc_max_bytes_kitty => {
        //     if (value) |ptr| {
        //         wrapper.stream.handler.apc_handler.max_bytes.put(.kitty, ptr.*);
        //     } else {
        //         wrapper.stream.handler.apc_handler.max_bytes.remove(.kitty);
        //     }
        // },
        Option_::ApcMaxBytesKitty => {
            // PORT NOTE: `.put(.kitty, v)` sets the kitty slot. `.remove(.kitty)`
            // removes the entry so the later `.get(.kitty) orelse
            // defaultMaxBytes(.kitty)` falls back to the default; under the
            // always-full array port that is exactly resetting the slot to
            // `defaultMaxBytes(.kitty)`.
            let handler = unsafe { &mut (*terminal_).stream.handler };
            let ptr = value as *const usize;
            let kitty = rustty_vt::apc::Protocol::Kitty as usize;
            if !ptr.is_null() {
                handler.apc_handler.max_bytes[kitty] = unsafe { *ptr };
            } else {
                handler.apc_handler.max_bytes[kitty] =
                    rustty_vt::apc::Protocol::default_max_bytes(rustty_vt::apc::Protocol::Kitty);
            }
        }

        // .glyph_protocol => {
        //     const enabled = (value orelse return .success).*;
        //     wrapper.stream.handler.apc_handler.enable(.glyph, enabled);
        //     if (!enabled) wrapper.terminal.glyph_glossary.clearAndFree(wrapper.terminal.gpa());
        // },
        Option_::GlyphProtocol => {
            // const enabled = (value orelse return .success).*;
            let ptr = value as *const bool;
            if ptr.is_null() {
                return Result::Success;
            }
            let enabled = unsafe { *ptr };
            // wrapper.stream.handler.apc_handler.enable(.glyph, enabled);
            unsafe {
                (*terminal_)
                    .stream
                    .handler
                    .apc_handler
                    .enable(rustty_vt::apc::Protocol::Glyph, enabled)
            };
            // if (!enabled) wrapper.terminal.glyph_glossary.clearAndFree(wrapper.terminal.gpa());
            if !enabled {
                // PORT NOTE: rustty's `Glossary::clear_and_free` carries its own
                // allocator (the `gpa()` thread is internal); the reference passes
                // `wrapper.terminal.gpa()` explicitly.
                unsafe { (*t).glyph_glossary.clear_and_free() };
            }
        }

        // .selection => {
        //     if (value) |ptr| {
        //         const sel = ptr.toZig() orelse return .invalid_value;
        //         wrapper.terminal.screens.active.select(sel) catch return .out_of_memory;
        //     } else {
        //         wrapper.terminal.screens.active.clearSelection();
        //     }
        // },
        Option_::Selection => {
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

        // .default_cursor_style => {
        //     const style = (if (value) |ptr| ptr.* else TerminalCursorStyle.block).toZig() orelse return .invalid_value;
        //     wrapper.stream.handler.default_cursor_style = style;
        //     if (wrapper.stream.handler.default_cursor) {
        //         wrapper.terminal.screens.active.cursor.cursor_style = style;
        //     }
        // },
        Option_::DefaultCursorStyle => {
            // const style = (if (value) |ptr| ptr.* else .block).toZig() orelse return .invalid_value;
            let ptr = value as *const TerminalCursorStyle;
            let c_style = if !ptr.is_null() {
                unsafe { *ptr }
            } else {
                TerminalCursorStyle::Block
            };
            let style = match c_style.to_zig() {
                Some(s) => s,
                None => return Result::InvalidValue,
            };
            // wrapper.stream.handler.default_cursor_style = style;
            unsafe { (*terminal_).stream.handler.default_cursor_style = style };
            // if (wrapper.stream.handler.default_cursor) { ... }
            if unsafe { (*terminal_).stream.handler.default_cursor } {
                let active: *mut rustty_vt::screen::Screen = unsafe { (*t).screens.active };
                unsafe { (*active).cursor.cursor_style = style };
            }
        }
        // .default_cursor_blink => {
        //     const blink = if (value) |ptr| ptr.* else false;
        //     wrapper.stream.handler.default_cursor_blink = blink;
        //     if (wrapper.stream.handler.default_cursor) {
        //         wrapper.terminal.modes.set(.cursor_blinking, blink);
        //     }
        // },
        Option_::DefaultCursorBlink => {
            // const blink = if (value) |ptr| ptr.* else false;
            let ptr = value as *const bool;
            let blink = if !ptr.is_null() {
                unsafe { *ptr }
            } else {
                false
            };
            // wrapper.stream.handler.default_cursor_blink = blink;
            unsafe { (*terminal_).stream.handler.default_cursor_blink = blink };
            // if (wrapper.stream.handler.default_cursor) { ... }
            if unsafe { (*terminal_).stream.handler.default_cursor } {
                unsafe { (*t).modes.set(modes::Mode::CursorBlinking, blink) };
            }
        }
    }

    // return .success;
    Result::Success
}

/// C: GhosttyTerminalCursorStyle -> RusttyTerminalCursorStyle (R2)
// pub const TerminalCursorStyle = enum(c_int) { bar = 0, block = 1, underline = 2, block_hollow = 3, _ };
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalCursorStyle {
    Bar = 0,
    Block = 1,
    Underline = 2,
    BlockHollow = 3,
}

impl TerminalCursorStyle {
    // fn toZig(self: TerminalCursorStyle) ?Screen.CursorStyle {
    //     return switch (self) {
    //         .bar => .bar,
    //         .block => .block,
    //         .underline => .underline,
    //         .block_hollow => .block_hollow,
    //         _ => null,
    //     };
    // }
    // PORT NOTE: the reference enum is non-exhaustive (`_`), mapping any unknown
    // discriminant to `null`. The Rust `#[repr(C)]` enum carries only the four
    // named variants; an out-of-range C value is already rejected at the
    // `*ptr` read (it would be an invalid enum value), so the four-arm match is
    // exhaustive and the reference `_ => null` arm is structurally unreachable.
    fn to_zig(self) -> Option<rustty_vt::screen::CursorStyle> {
        match self {
            // .bar => .bar,
            TerminalCursorStyle::Bar => Some(rustty_vt::screen::CursorStyle::Bar),
            // .block => .block,
            TerminalCursorStyle::Block => Some(rustty_vt::screen::CursorStyle::Block),
            // .underline => .underline,
            TerminalCursorStyle::Underline => Some(rustty_vt::screen::CursorStyle::Underline),
            // .block_hollow => .block_hollow,
            TerminalCursorStyle::BlockHollow => Some(rustty_vt::screen::CursorStyle::BlockHollow),
        }
    }
}

/// C: GhosttyDeviceAttributes -> RusttyDeviceAttributes (R2)
// pub const DeviceAttributes = Effects.CDeviceAttributes;
pub type DeviceAttributes = CDeviceAttributes;

/// C: GhosttyTerminalScrollViewport -> RusttyTerminalScrollViewport (R2)
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
// PORT NOTE: `Tag = lib.Enum(lib.target, &.{ "top", "bottom", "delta", "row" })`;
// for the `.c` target a fieldless `enum(c_int)` in key order. `Row` is appended
// (oracle 3a2e28329) — the union already reserves `[2]u64` so this is ABI stable.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollViewportTag {
    Top = 0,
    Bottom = 1,
    Delta = 2,
    Row = 3,
}

/// The C ABI compatible union value for `ScrollViewport`.
// PORT NOTE: `lib.TaggedUnion` gives one union field per tag. `top`/`bottom`
// have `void` payload (no addressable member in C); only `delta: isize` carries
// a value. The `_padding` member pins the union to `[2]u64`.
#[repr(C)]
#[derive(Clone, Copy)]
pub union ScrollViewportCValue {
    pub delta: isize,
    /// Absolute row offset for the `Row` tag (oracle 3a2e28329).
    pub row: usize,
    /// Padding to the PINNED ABI size. `[2]u64` (16 bytes) per the reference.
    pub _padding: [u64; 2],
}

/// C: GhosttyTerminalScrollViewport -> RusttyTerminalScrollViewport (R2) — the hand-authored tagged union.
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
//         .row => .{ .row = behavior.value.row },
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
        // SAFETY: the `row` tag selects the `row` union member.
        ScrollViewportTag::Row => ZigScrollViewport::Row(unsafe { behavior.value.row }),
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

/// C: GhosttyKittyGraphics -> RusttyKittyGraphics (R2)
// pub const KittyGraphics = kitty_gfx_c.KittyGraphics;
pub type KittyGraphics = KittyGraphicsHandle;

/// C: GhosttyTerminalScreen -> RusttyTerminalScreen (R2)
// pub const TerminalScreen = ScreenSet.Key;
pub type TerminalScreen = ScreenSetKey;

/// C: GhosttyTerminalScrollbar -> RusttyTerminalScrollbar (R2)
// pub const TerminalScrollbar = PageList.Scrollbar.C;
pub type TerminalScrollbar = ScrollbarC;

/// C: GhosttyTerminalData -> RusttyTerminalData (R2)
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
    // /// Output type expected for querying the data of the given kind.
    // pub fn OutType(comptime self: TerminalData) type {
    //     return switch (self) {
    //         .invalid => void,
    //         .cols, .rows, .cursor_x, .cursor_y => size.CellCountInt,
    //         .cursor_pending_wrap, .cursor_visible, .mouse_tracking, .viewport_active => bool,
    //         .active_screen => TerminalScreen,
    //         .kitty_keyboard_flags => u8,
    //         .scrollbar => TerminalScrollbar,
    //         .cursor_style => style_c.Style,
    //         .title, .pwd => lib.String,
    //         .total_rows, .scrollback_rows => usize,
    //         .width_px, .height_px => u32,
    //         .color_foreground,
    //         .color_background,
    //         .color_cursor,
    //         .color_foreground_default,
    //         .color_background_default,
    //         .color_cursor_default,
    //         => color.RGB.C,
    //         .color_palette, .color_palette_default => color.PaletteC,
    //         .kitty_image_storage_limit => u64,
    //         .kitty_image_medium_file,
    //         .kitty_image_medium_temp_file,
    //         .kitty_image_medium_shared_mem,
    //         => bool,
    //         .kitty_graphics => KittyGraphics,
    //         .selection => selection_c.CSelection,
    //     };
    // }
    //
    // PORT NOTE: `OutType` is a comptime type-returning fn (the output type expected
    // for querying each data kind). rustty erases `out` to `*mut c_void` at the FFI
    // boundary and writes it back as the per-arm type inside `get_typed`, so this
    // comptime projection has no Rust analog; it is preserved verbatim as the
    // reference contract for each arm's expected pointee type.
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
// `OutType` projects a vt-c type (`color.RGB.C`/`PaletteC`, `style_c.Style`,
// `selection_c.CSelection`, kitty image/graphics) landed with the get/set
// sub-batch — every arm below is live (VALIDATION_REPORT M3).
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

        // .scrollbar => out.* = t.screens.active.pages.scrollbar().cval(),
        TerminalData::Scrollbar => unsafe {
            *(out as *mut TerminalScrollbar) = (*active).pages.scrollbar().cval()
        },
        // .cursor_style => out.* = .fromStyle(t.screens.active.cursor.style),
        TerminalData::CursorStyle => unsafe {
            *(out as *mut crate::style::Style) =
                crate::style::Style::from_style((*active).cursor.style)
        },
        // .title => {
        //     const title = t.getTitle() orelse "";
        //     out.* = .{ .ptr = title.ptr, .len = title.len };
        // },
        TerminalData::Title => {
            // SAFETY: shared read of the terminal's title via the raw ptr (no
            // `&mut` retag of the handler-aliased terminal).
            let title = unsafe { (*t).get_title() }.unwrap_or(b"");
            unsafe { *(out as *mut LibString) = LibString::init(title) };
        }
        // .pwd => {
        //     const pwd = t.getPwd() orelse "";
        //     out.* = .{ .ptr = pwd.ptr, .len = pwd.len };
        // },
        TerminalData::Pwd => {
            let pwd = unsafe { (*t).get_pwd() }.unwrap_or(b"");
            unsafe { *(out as *mut LibString) = LibString::init(pwd) };
        }
        // .color_foreground => out.* = (t.colors.foreground.get() orelse return .no_value).cval(),
        TerminalData::ColorForeground => {
            let rgb = match unsafe { (*t).colors.foreground.get() } {
                Some(c) => c,
                None => return Result::NoValue,
            };
            unsafe { *(out as *mut crate::color::RgbC) = rgb };
        }
        // .color_background => out.* = (t.colors.background.get() orelse return .no_value).cval(),
        TerminalData::ColorBackground => {
            let rgb = match unsafe { (*t).colors.background.get() } {
                Some(c) => c,
                None => return Result::NoValue,
            };
            unsafe { *(out as *mut crate::color::RgbC) = rgb };
        }
        // .color_cursor => out.* = (t.colors.cursor.get() orelse return .no_value).cval(),
        TerminalData::ColorCursor => {
            let rgb = match unsafe { (*t).colors.cursor.get() } {
                Some(c) => c,
                None => return Result::NoValue,
            };
            unsafe { *(out as *mut crate::color::RgbC) = rgb };
        }
        // .color_foreground_default => out.* = (t.colors.foreground.default orelse return .no_value).cval(),
        TerminalData::ColorForegroundDefault => {
            let rgb = match unsafe { (*t).colors.foreground.default } {
                Some(c) => c,
                None => return Result::NoValue,
            };
            unsafe { *(out as *mut crate::color::RgbC) = rgb };
        }
        // .color_background_default => out.* = (t.colors.background.default orelse return .no_value).cval(),
        TerminalData::ColorBackgroundDefault => {
            let rgb = match unsafe { (*t).colors.background.default } {
                Some(c) => c,
                None => return Result::NoValue,
            };
            unsafe { *(out as *mut crate::color::RgbC) = rgb };
        }
        // .color_cursor_default => out.* = (t.colors.cursor.default orelse return .no_value).cval(),
        TerminalData::ColorCursorDefault => {
            let rgb = match unsafe { (*t).colors.cursor.default } {
                Some(c) => c,
                None => return Result::NoValue,
            };
            unsafe { *(out as *mut crate::color::RgbC) = rgb };
        }
        // .color_palette => out.* = color.paletteCval(&t.colors.palette.current),
        TerminalData::ColorPalette => unsafe {
            *(out as *mut crate::color::PaletteC) =
                crate::color::palette_cval(&(*t).colors.palette.current)
        },
        // .color_palette_default => out.* = color.paletteCval(&t.colors.palette.original),
        TerminalData::ColorPaletteDefault => unsafe {
            *(out as *mut crate::color::PaletteC) =
                crate::color::palette_cval(&(*t).colors.palette.original)
        },
        // .kitty_image_storage_limit => {
        //     if (comptime !build_options.kitty_graphics) return .no_value;
        //     out.* = @intCast(t.screens.active.kitty_images.total_limit);
        // },
        TerminalData::KittyImageStorageLimit => {
            if !rustty_vt::options::KITTY_GRAPHICS {
                return Result::NoValue;
            }
            unsafe { *(out as *mut u64) = (*active).kitty_images.total_limit as u64 };
        }
        // .kitty_image_medium_file => {
        //     if (comptime !build_options.kitty_graphics) return .no_value;
        //     out.* = t.screens.active.kitty_images.image_limits.file;
        // },
        TerminalData::KittyImageMediumFile => {
            if !rustty_vt::options::KITTY_GRAPHICS {
                return Result::NoValue;
            }
            unsafe { *(out as *mut bool) = (*active).kitty_images.image_limits.file };
        }
        // .kitty_image_medium_temp_file => {
        //     if (comptime !build_options.kitty_graphics) return .no_value;
        //     out.* = t.screens.active.kitty_images.image_limits.temporary_file;
        // },
        TerminalData::KittyImageMediumTempFile => {
            if !rustty_vt::options::KITTY_GRAPHICS {
                return Result::NoValue;
            }
            unsafe { *(out as *mut bool) = (*active).kitty_images.image_limits.temporary_file };
        }
        // .kitty_image_medium_shared_mem => {
        //     if (comptime !build_options.kitty_graphics) return .no_value;
        //     out.* = t.screens.active.kitty_images.image_limits.shared_memory;
        // },
        TerminalData::KittyImageMediumSharedMem => {
            if !rustty_vt::options::KITTY_GRAPHICS {
                return Result::NoValue;
            }
            unsafe { *(out as *mut bool) = (*active).kitty_images.image_limits.shared_memory };
        }
        // .kitty_graphics => {
        //     if (comptime !build_options.kitty_graphics) return .no_value;
        //     out.* = &t.screens.active.kitty_images;
        // },
        TerminalData::KittyGraphics => {
            if !rustty_vt::options::KITTY_GRAPHICS {
                return Result::NoValue;
            }
            // out.* = &t.screens.active.kitty_images;
            // PORT NOTE: `KittyGraphics` is the opaque `*mut c_void` handle (see
            // kitty_graphics.rs); hand out the address of the active screen's
            // `kitty_images` storage.
            unsafe {
                *(out as *mut crate::kitty_graphics::KittyGraphics) =
                    core::ptr::addr_of_mut!((*active).kitty_images) as *mut c_void
            };
        }
        // .selection => out.* = selection_c.CSelection.fromZig(
        //     t.screens.active.selection orelse return .no_value,
        // ),
        TerminalData::Selection => {
            let sel = match unsafe { (*active).selection } {
                Some(s) => s,
                None => return Result::NoValue,
            };
            unsafe {
                *(out as *mut crate::selection::CSelection) =
                    crate::selection::CSelection::from_zig(sel)
            };
        }
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
    // PORT NOTE (Miri SB+TB): `t.gpa()` is `fn gpa(&mut self)` and would form a
    // `&Terminal`/`&mut Terminal` reborrow over `*t`, freezing/aliasing the
    // `tracked_pin` tag created above through the `&mut Terminal` path and causing
    // a Tree-Borrows protector violation when the pin is later written during a
    // terminal scroll. `gpa()` merely reads `self.screens.active.alloc` (a `Global`
    // ZST copy), so we read that field through the existing `active: *mut Screen`
    // raw pointer, keeping the access window to a single field read and never
    // forming a reference to `*t`.
    let alloc = unsafe { (*active).alloc };
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

    // Shared scaffold for the reference's repeated
    //   var t: Terminal = null;
    //   try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
    // preamble. It returns the `new` Result UNASSERTED so the reference's
    // `expectEqual(Result.success, ...)` ASSERTION lives in each test body
    // (assertion-for-assertion; G2 counts source-level asserts per test).
    fn new_t(
        alloc: &CAllocator,
        cols: size::CellCountInt,
        rows: size::CellCountInt,
        sb: usize,
    ) -> (Result, Terminal) {
        let mut t: Terminal = std::ptr::null_mut();
        let r = unsafe {
            new(
                alloc as *const CAllocator,
                &mut t,
                Options {
                    cols,
                    rows,
                    max_scrollback: sb,
                },
            )
        };
        (r, t)
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
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 5, 2, 10_000);
            assert_eq!(Result::Success, new_result);

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

    // test "scroll_viewport row" (oracle 3a2e28329)
    #[test]
    fn scroll_viewport_row_test() {
        with_test_allocator(|alloc| {
            let (new_result, t) = new_t(alloc, 5, 2, 10_000);
            assert_eq!(Result::Success, new_result);

            // Write 4 rows so rows "1" and "2" are pushed into scrollback:
            // total rows is 4, viewport length is 2.
            unsafe { vt_write(t, b"1\r\n2\r\n3\r\n4".as_ptr(), 10) };

            let mut viewport_active: bool = false;
            assert_eq!(Result::Success, unsafe {
                get(t, TerminalData::ViewportActive, &mut viewport_active as *mut bool as *mut c_void)
            });
            assert!(viewport_active);

            // Row 0 is the top of the scrollback.
            unsafe {
                scroll_viewport(
                    t,
                    ScrollViewportC { tag: ScrollViewportTag::Row, value: ScrollViewportCValue { row: 0 } },
                )
            };
            assert_eq!(Result::Success, unsafe {
                get(t, TerminalData::ViewportActive, &mut viewport_active as *mut bool as *mut c_void)
            });
            assert!(!viewport_active);
            assert_eq!(b"1\n2", plain_string(t).as_slice());

            // An absolute row within the scrollback becomes the first visible
            // row and round-trips through the scrollbar offset.
            unsafe {
                scroll_viewport(
                    t,
                    ScrollViewportC { tag: ScrollViewportTag::Row, value: ScrollViewportCValue { row: 1 } },
                )
            };
            assert_eq!(b"2\n3", plain_string(t).as_slice());

            let mut scrollbar_data: TerminalScrollbar = TerminalScrollbar { total: 0, offset: 0, len: 0 };
            assert_eq!(Result::Success, unsafe {
                get(t, TerminalData::Scrollbar, &mut scrollbar_data as *mut TerminalScrollbar as *mut c_void)
            });
            assert_eq!(4, scrollbar_data.total);
            assert_eq!(1, scrollbar_data.offset);
            assert_eq!(2, scrollbar_data.len);

            // A row past the end clamps to the active area.
            unsafe {
                scroll_viewport(
                    t,
                    ScrollViewportC { tag: ScrollViewportTag::Row, value: ScrollViewportCValue { row: 9999 } },
                )
            };
            assert_eq!(Result::Success, unsafe {
                get(t, TerminalData::ViewportActive, &mut viewport_active as *mut bool as *mut c_void)
            });
            assert!(viewport_active);
            assert_eq!(b"3\n4", plain_string(t).as_slice());
            assert_eq!(Result::Success, unsafe {
                get(t, TerminalData::Scrollbar, &mut scrollbar_data as *mut TerminalScrollbar as *mut c_void)
            });
            assert_eq!(2, scrollbar_data.offset);

            unsafe { free(t) };
        });
    }

    // test "scroll_viewport row alt screen" (oracle 3a2e28329)
    #[test]
    fn scroll_viewport_row_alt_screen_test() {
        with_test_allocator(|alloc| {
            let (new_result, t) = new_t(alloc, 5, 2, 10_000);
            assert_eq!(Result::Success, new_result);

            // Enter the alternate screen, which has no scrollback.
            unsafe { vt_write(t, b"\x1b[?1049h".as_ptr(), 8) };
            let mut screen: TerminalScreen = TerminalScreen::Primary;
            assert_eq!(Result::Success, unsafe {
                get(t, TerminalData::ActiveScreen, &mut screen as *mut TerminalScreen as *mut c_void)
            });
            assert_eq!(TerminalScreen::Alternate, screen);

            // Scrolling to any row keeps the viewport on the active area.
            let mut viewport_active: bool = false;
            unsafe {
                scroll_viewport(
                    t,
                    ScrollViewportC { tag: ScrollViewportTag::Row, value: ScrollViewportCValue { row: 0 } },
                )
            };
            assert_eq!(Result::Success, unsafe {
                get(t, TerminalData::ViewportActive, &mut viewport_active as *mut bool as *mut c_void)
            });
            assert!(viewport_active);
            unsafe {
                scroll_viewport(
                    t,
                    ScrollViewportC { tag: ScrollViewportTag::Row, value: ScrollViewportCValue { row: 9999 } },
                )
            };
            assert_eq!(Result::Success, unsafe {
                get(t, TerminalData::ViewportActive, &mut viewport_active as *mut bool as *mut c_void)
            });
            assert!(viewport_active);

            unsafe { free(t) };
        });
    }

    // test "reset"
    #[test]
    fn reset_test() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24, 10_000);
            assert_eq!(Result::Success, new_result);

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
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24, 10_000);
            assert_eq!(Result::Success, new_result);

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
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24, 10_000);
            assert_eq!(Result::Success, new_result);

            assert_eq!(Result::InvalidValue, unsafe { resize(t, 0, 24, 9, 18) });
            assert_eq!(Result::InvalidValue, unsafe { resize(t, 80, 0, 9, 18) });
            unsafe { free(t) };
        });
    }

    // test "resize shrinks both axes with cursor at bottom"
    #[test]
    fn resize_shrinks_both_axes_with_cursor_at_bottom() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24, 0);
            assert_eq!(Result::Success, new_result);

            // CSI 24;1H -> park the cursor on the bottom row (1-based).
            let mv = b"\x1b[24;1H";
            unsafe { vt_write(t, mv.as_ptr(), mv.len()) };

            // Shrink both axes; pre-resize cursor.y sits past the new bottom row.
            // Previously this underflowed in PageList.resizeCols.
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
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24, 0);
            assert_eq!(Result::Success, new_result);

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
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24, 0);
            assert_eq!(Result::Success, new_result);

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
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24, 0);
            assert_eq!(Result::Success, new_result);

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
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24, 10_000);
            assert_eq!(Result::Success, new_result);

            unsafe { vt_write(t, b"Hello".as_ptr(), 5) };

            assert_eq!(b"Hello", plain_string(t).as_slice());
            unsafe { free(t) };
        });
    }

    // test "vt_write split escape sequence"
    #[test]
    fn vt_write_split_escape_sequence() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24, 10_000);
            assert_eq!(Result::Success, new_result);

            // Write "Hello" in bold by splitting the CSI bold sequence across two writes.
            // ESC [ 1 m  = bold on, ESC [ 0 m = reset
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
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 2, 2, 0);
            assert_eq!(Result::Success, new_result);

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
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24, 0);
            assert_eq!(Result::Success, new_result);

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
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24, 0);
            assert_eq!(Result::Success, new_result);

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
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24, 0);
            assert_eq!(Result::Success, new_result);

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
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24, 0);
            assert_eq!(Result::Success, new_result);

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
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24, 0);
            assert_eq!(Result::Success, new_result);

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
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24, 0);
            assert_eq!(Result::Success, new_result);

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
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24, 10_000);
            assert_eq!(Result::Success, new_result);

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
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 3, 10_000);
            assert_eq!(Result::Success, new_result);

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
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24, 0);
            assert_eq!(Result::Success, new_result);

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
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24, 0);
            assert_eq!(Result::Success, new_result);

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
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24, 0);
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

    // PORT NOTE (test helpers): the reference builds `point.Point.C` literals
    // inline (`.{ .tag = .active, .value = .{ .active = .{ .x, .y } } }`). rustty's
    // `PointC` is a hand-authored tagged union; these helpers construct it for the
    // active/viewport/screen tags used by the ported tests.
    fn pt_active(x: size::CellCountInt, y: u32) -> PointC {
        PointC {
            tag: crate::point::TagC::Active,
            value: crate::point::PointCValue {
                active: Coordinate { x, y },
            },
        }
    }
    fn pt_viewport(x: size::CellCountInt, y: u32) -> PointC {
        PointC {
            tag: crate::point::TagC::Viewport,
            value: crate::point::PointCValue {
                viewport: Coordinate { x, y },
            },
        }
    }
    fn pt_screen(x: size::CellCountInt, y: u32) -> PointC {
        PointC {
            tag: crate::point::TagC::Screen,
            value: crate::point::PointCValue {
                screen: Coordinate { x, y },
            },
        }
    }

    // Convenience: write a byte slice through `vt_write`.
    fn vtw(t: Terminal, bytes: &[u8]) {
        unsafe { vt_write(t, bytes.as_ptr(), bytes.len()) };
    }

    // test "set default cursor style and blink"
    #[test]
    fn set_default_cursor_style_and_blink() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24, 0);
            assert_eq!(Result::Success, new_result);

            // var default_style: TerminalCursorStyle = .bar;
            let mut default_style = TerminalCursorStyle::Bar;
            // var default_blink = true;
            let mut default_blink = true;
            assert_eq!(Result::Success, unsafe {
                set(
                    t,
                    Option_::DefaultCursorStyle,
                    &mut default_style as *mut _ as *const c_void,
                )
            });
            assert_eq!(Result::Success, unsafe {
                set(
                    t,
                    Option_::DefaultCursorBlink,
                    &mut default_blink as *mut _ as *const c_void,
                )
            });

            // Setting defaults applies them immediately while the cursor is still default.
            assert_eq!(rustty_vt::screen::CursorStyle::Bar, unsafe {
                (*(*(*t).terminal).screens.active).cursor.cursor_style
            });
            assert!(unsafe { (*(*t).terminal).modes.get(modes::Mode::CursorBlinking) });

            // An explicit DECSCUSR style overrides the configured defaults.
            vtw(t, b"\x1b[2 q");
            assert_eq!(rustty_vt::screen::CursorStyle::Block, unsafe {
                (*(*(*t).terminal).screens.active).cursor.cursor_style
            });
            assert!(!unsafe { (*(*t).terminal).modes.get(modes::Mode::CursorBlinking) });

            // Changing defaults does not override an explicit cursor style.
            default_style = TerminalCursorStyle::Underline;
            assert_eq!(Result::Success, unsafe {
                set(
                    t,
                    Option_::DefaultCursorStyle,
                    &mut default_style as *mut _ as *const c_void,
                )
            });
            assert_eq!(rustty_vt::screen::CursorStyle::Block, unsafe {
                (*(*(*t).terminal).screens.active).cursor.cursor_style
            });
            assert!(!unsafe { (*(*t).terminal).modes.get(modes::Mode::CursorBlinking) });

            // DECSCUSR reset restores the configured default style and blink.
            vtw(t, b"\x1b[0 q");
            assert_eq!(rustty_vt::screen::CursorStyle::Underline, unsafe {
                (*(*(*t).terminal).screens.active).cursor.cursor_style
            });
            assert!(unsafe { (*(*t).terminal).modes.get(modes::Mode::CursorBlinking) });
            unsafe { free(t) };
        });
    }

    // test "set and get selection"
    #[test]
    fn set_and_get_selection() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24, 0);
            assert_eq!(Result::Success, new_result);

            vtw(t, b"Hello");

            // var start_ref: grid_ref_c.CGridRef = .{};
            let mut start_ref = CGridRef::default();
            assert_eq!(Result::Success, unsafe {
                grid_ref(t, pt_active(0, 0), &mut start_ref)
            });

            let mut end_ref = CGridRef::default();
            assert_eq!(Result::Success, unsafe {
                grid_ref(t, pt_active(4, 0), &mut end_ref)
            });

            let mut out = crate::selection::CSelection::default();
            assert_eq!(Result::NoValue, unsafe {
                get(
                    t,
                    TerminalData::Selection,
                    &mut out as *mut _ as *mut c_void,
                )
            });

            let sel = crate::selection::CSelection {
                start: start_ref,
                end: end_ref,
                rectangle: true,
                ..crate::selection::CSelection::default()
            };
            assert_eq!(Result::Success, unsafe {
                set(t, Option_::Selection, &sel as *const _ as *const c_void)
            });
            assert!(unsafe {
                (*(*(*t).terminal).screens.active)
                    .selection
                    .unwrap()
                    .tracked()
            });

            assert_eq!(Result::Success, unsafe {
                get(
                    t,
                    TerminalData::Selection,
                    &mut out as *mut _ as *mut c_void,
                )
            });
            assert!(out.start.to_pin().unwrap().eql(start_ref.to_pin().unwrap()));
            assert!(out.end.to_pin().unwrap().eql(end_ref.to_pin().unwrap()));
            assert!(out.rectangle);

            // set(t, .selection, null)
            assert_eq!(Result::Success, unsafe {
                set(t, Option_::Selection, std::ptr::null())
            });
            assert!(unsafe { (*(*(*t).terminal).screens.active).selection.is_none() });
            assert_eq!(Result::NoValue, unsafe {
                get(
                    t,
                    TerminalData::Selection,
                    &mut out as *mut _ as *mut c_void,
                )
            });
            unsafe { free(t) };
        });
    }

    // test "selection derivation helpers"
    #[test]
    fn selection_derivation_helpers() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24, 0);
            assert_eq!(Result::Success, new_result);

            vtw(t, b"  Hello  \r\nWorld");

            let mut out = crate::selection::CSelection::default();

            let mut word_ref = CGridRef::default();
            assert_eq!(Result::Success, unsafe {
                grid_ref(t, pt_active(3, 0), &mut word_ref)
            });

            let mut empty_ref = CGridRef::default();
            assert_eq!(Result::Success, unsafe {
                grid_ref(t, pt_active(20, 0), &mut empty_ref)
            });

            let mut line_ref = CGridRef::default();
            assert_eq!(Result::Success, unsafe {
                grid_ref(t, pt_active(0, 0), &mut line_ref)
            });

            let mut word_opts = crate::selection::SelectWordOptions {
                size: size_of::<crate::selection::SelectWordOptions>(),
                ref_: word_ref,
                boundary_codepoints: std::ptr::null(),
                boundary_codepoints_len: 0,
            };
            assert_eq!(Result::Success, unsafe {
                crate::selection::word(t, &word_opts, &mut out)
            });
            assert_eq!(2u16, out.start.to_pin().unwrap().x);
            assert_eq!(6u16, out.end.to_pin().unwrap().x);

            word_opts.ref_ = empty_ref;
            assert_eq!(Result::NoValue, unsafe {
                crate::selection::word(t, &word_opts, &mut out)
            });

            let mut between_start_ref = CGridRef::default();
            assert_eq!(Result::Success, unsafe {
                grid_ref(t, pt_active(20, 1), &mut between_start_ref)
            });

            let mut between_end_ref = CGridRef::default();
            assert_eq!(Result::Success, unsafe {
                grid_ref(t, pt_active(0, 1), &mut between_end_ref)
            });

            let word_between_opts = crate::selection::SelectWordBetweenOptions {
                size: size_of::<crate::selection::SelectWordBetweenOptions>(),
                start: between_start_ref,
                end: between_end_ref,
                boundary_codepoints: std::ptr::null(),
                boundary_codepoints_len: 0,
            };
            assert_eq!(Result::Success, unsafe {
                crate::selection::word_between(t, &word_between_opts, &mut out)
            });
            assert_eq!(0u16, out.start.to_pin().unwrap().x);
            assert_eq!(1u16, out.start.to_pin().unwrap().y);
            assert_eq!(4u16, out.end.to_pin().unwrap().x);
            assert_eq!(1u16, out.end.to_pin().unwrap().y);

            let mut line_opts = crate::selection::SelectLineOptions {
                size: size_of::<crate::selection::SelectLineOptions>(),
                ref_: line_ref,
                whitespace: std::ptr::null(),
                whitespace_len: 0,
                semantic_prompt_boundary: false,
            };
            assert_eq!(Result::Success, unsafe {
                crate::selection::line(t, &line_opts, &mut out)
            });
            assert_eq!(2u16, out.start.to_pin().unwrap().x);
            assert_eq!(6u16, out.end.to_pin().unwrap().x);

            assert_eq!(Result::Success, unsafe {
                crate::selection::all(t, &mut out)
            });
            assert_eq!(2u16, out.start.to_pin().unwrap().x);
            assert_eq!(0u16, out.start.to_pin().unwrap().y);
            assert_eq!(4u16, out.end.to_pin().unwrap().x);
            assert_eq!(1u16, out.end.to_pin().unwrap().y);

            assert_eq!(Result::NoValue, unsafe {
                crate::selection::output(t, line_ref, &mut out)
            });

            line_opts.size = size_of::<usize>() - 1;
            assert_eq!(Result::InvalidValue, unsafe {
                crate::selection::line(t, &line_opts, &mut out)
            });
            assert_eq!(Result::InvalidValue, unsafe {
                crate::selection::word(t, std::ptr::null(), &mut out)
            });
            assert_eq!(Result::InvalidValue, unsafe {
                crate::selection::word(t, &word_opts, std::ptr::null_mut())
            });
            assert_eq!(Result::InvalidValue, unsafe {
                crate::selection::word_between(t, std::ptr::null(), &mut out)
            });
            assert_eq!(Result::InvalidValue, unsafe {
                crate::selection::word_between(t, &word_between_opts, std::ptr::null_mut())
            });
            unsafe { free(t) };
        });
    }

    // test "selection_adjust mutates snapshot end"
    #[test]
    fn selection_adjust_mutates_snapshot_end() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24, 0);
            assert_eq!(Result::Success, new_result);

            vtw(t, b"Hello");

            let mut start_ref = CGridRef::default();
            assert_eq!(Result::Success, unsafe {
                grid_ref(t, pt_active(0, 0), &mut start_ref)
            });

            let mut end_ref = CGridRef::default();
            assert_eq!(Result::Success, unsafe {
                grid_ref(t, pt_active(1, 0), &mut end_ref)
            });

            let mut sel = crate::selection::CSelection {
                start: start_ref,
                end: end_ref,
                ..crate::selection::CSelection::default()
            };
            assert_eq!(Result::Success, unsafe {
                crate::selection::adjust(t, &mut sel, crate::selection::Adjustment::Right)
            });
            assert_eq!(0u16, sel.start.to_pin().unwrap().x);
            assert_eq!(2u16, sel.end.to_pin().unwrap().x);

            assert_eq!(Result::Success, unsafe {
                crate::selection::adjust(t, &mut sel, crate::selection::Adjustment::Left)
            });
            assert_eq!(0u16, sel.start.to_pin().unwrap().x);
            assert_eq!(1u16, sel.end.to_pin().unwrap().x);

            sel = crate::selection::CSelection {
                start: end_ref,
                end: start_ref,
                ..crate::selection::CSelection::default()
            };
            assert_eq!(Result::Success, unsafe {
                crate::selection::adjust(t, &mut sel, crate::selection::Adjustment::Right)
            });
            assert_eq!(1u16, sel.start.to_pin().unwrap().x);
            assert_eq!(1u16, sel.end.to_pin().unwrap().x);
            unsafe { free(t) };
        });
    }

    // test "selection_order and selection_ordered"
    #[test]
    fn selection_order_and_selection_ordered() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24, 0);
            assert_eq!(Result::Success, new_result);

            vtw(t, b"Hello\r\nWorld");

            let mut start_ref = CGridRef::default();
            assert_eq!(Result::Success, unsafe {
                grid_ref(t, pt_active(3, 0), &mut start_ref)
            });

            let mut end_ref = CGridRef::default();
            assert_eq!(Result::Success, unsafe {
                grid_ref(t, pt_active(1, 1), &mut end_ref)
            });

            let sel = crate::selection::CSelection {
                start: start_ref,
                end: end_ref,
                rectangle: true,
                ..crate::selection::CSelection::default()
            };

            let mut order = crate::selection::Order::Forward;
            assert_eq!(Result::Success, unsafe {
                crate::selection::order(t, &sel, &mut order)
            });
            assert_eq!(crate::selection::Order::MirroredForward, order);

            let mut out = crate::selection::CSelection::default();
            assert_eq!(Result::Success, unsafe {
                crate::selection::ordered(t, &sel, crate::selection::Order::Forward, &mut out)
            });
            assert_eq!(1u16, out.start.to_pin().unwrap().x);
            assert_eq!(0u16, out.start.to_pin().unwrap().y);
            assert_eq!(3u16, out.end.to_pin().unwrap().x);
            assert_eq!(1u16, out.end.to_pin().unwrap().y);
            assert!(out.rectangle);

            assert_eq!(Result::Success, unsafe {
                crate::selection::ordered(t, &sel, crate::selection::Order::Reverse, &mut out)
            });
            assert_eq!(3u16, out.start.to_pin().unwrap().x);
            assert_eq!(1u16, out.start.to_pin().unwrap().y);
            assert_eq!(1u16, out.end.to_pin().unwrap().x);
            assert_eq!(0u16, out.end.to_pin().unwrap().y);
            assert!(out.rectangle);
            unsafe { free(t) };
        });
    }

    // test "selection_contains"
    #[test]
    fn selection_contains_test() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24, 0);
            assert_eq!(Result::Success, new_result);

            vtw(t, b"Hello\r\nWorld");

            let mut start_ref = CGridRef::default();
            assert_eq!(Result::Success, unsafe {
                grid_ref(t, pt_active(3, 0), &mut start_ref)
            });

            let mut end_ref = CGridRef::default();
            assert_eq!(Result::Success, unsafe {
                grid_ref(t, pt_active(1, 1), &mut end_ref)
            });

            let linear = crate::selection::CSelection {
                start: start_ref,
                end: end_ref,
                ..crate::selection::CSelection::default()
            };

            let mut contains = false;
            assert_eq!(Result::Success, unsafe {
                crate::selection::contains(t, &linear, pt_active(4, 0), &mut contains)
            });
            assert!(contains);

            assert_eq!(Result::Success, unsafe {
                crate::selection::contains(t, &linear, pt_active(2, 0), &mut contains)
            });
            assert!(!contains);

            let rectangle = crate::selection::CSelection {
                start: start_ref,
                end: end_ref,
                rectangle: true,
                ..crate::selection::CSelection::default()
            };

            assert_eq!(Result::Success, unsafe {
                crate::selection::contains(t, &rectangle, pt_active(2, 0), &mut contains)
            });
            assert!(contains);
            unsafe { free(t) };
        });
    }

    // test "selection_equal"
    #[test]
    fn selection_equal_test() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24, 0);
            assert_eq!(Result::Success, new_result);
            // try testing.expectEqual(Result.success, new(&test_allocator, &other_t, .{ ... }));
            let (new_result, other_t) = new_t(alloc, 80, 24, 0);
            assert_eq!(Result::Success, new_result);

            vtw(t, b"Hello");
            vtw(other_t, b"Hello");

            let mut start_ref = CGridRef::default();
            assert_eq!(Result::Success, unsafe {
                grid_ref(t, pt_active(0, 0), &mut start_ref)
            });

            let mut end_ref = CGridRef::default();
            assert_eq!(Result::Success, unsafe {
                grid_ref(t, pt_active(1, 0), &mut end_ref)
            });

            let mut other_end_ref = CGridRef::default();
            assert_eq!(Result::Success, unsafe {
                grid_ref(t, pt_active(2, 0), &mut other_end_ref)
            });

            let mut cross_terminal_ref = CGridRef::default();
            assert_eq!(Result::Success, unsafe {
                grid_ref(other_t, pt_active(1, 0), &mut cross_terminal_ref)
            });

            let sel = crate::selection::CSelection {
                start: start_ref,
                end: end_ref,
                ..crate::selection::CSelection::default()
            };
            let equal_sel = crate::selection::CSelection {
                start: start_ref,
                end: end_ref,
                ..crate::selection::CSelection::default()
            };
            let different_endpoint = crate::selection::CSelection {
                start: start_ref,
                end: other_end_ref,
                ..crate::selection::CSelection::default()
            };
            let different_rectangle = crate::selection::CSelection {
                start: start_ref,
                end: end_ref,
                rectangle: true,
                ..crate::selection::CSelection::default()
            };
            let cross_terminal = crate::selection::CSelection {
                start: start_ref,
                end: cross_terminal_ref,
                ..crate::selection::CSelection::default()
            };

            let mut equal = false;
            assert_eq!(Result::Success, unsafe {
                crate::selection::equal(t, &sel, &equal_sel, &mut equal)
            });
            assert!(equal);

            assert_eq!(Result::Success, unsafe {
                crate::selection::equal(t, &sel, &different_endpoint, &mut equal)
            });
            assert!(!equal);

            assert_eq!(Result::Success, unsafe {
                crate::selection::equal(t, &sel, &different_rectangle, &mut equal)
            });
            assert!(!equal);

            assert_eq!(Result::Success, unsafe {
                crate::selection::equal(t, &sel, &cross_terminal, &mut equal)
            });
            assert!(!equal);
            assert_eq!(Result::InvalidValue, unsafe {
                crate::selection::equal(t, &sel, &equal_sel, std::ptr::null_mut())
            });
            unsafe { free(t) };
            unsafe { free(other_t) };
        });
    }

    // test "selection_order invalid values"
    #[test]
    fn selection_order_invalid_values() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24, 0);
            assert_eq!(Result::Success, new_result);

            let mut order = crate::selection::Order::Forward;
            assert_eq!(Result::InvalidValue, unsafe {
                crate::selection::order(std::ptr::null_mut(), std::ptr::null(), &mut order)
            });
            assert_eq!(Result::InvalidValue, unsafe {
                crate::selection::order(t, std::ptr::null(), &mut order)
            });
            unsafe { free(t) };
        });
    }

    // test "grid_ref"
    #[test]
    fn grid_ref_test() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24, 0);
            assert_eq!(Result::Success, new_result);

            vtw(t, b"Hello");

            let mut out_ref = CGridRef::default();
            assert_eq!(Result::Success, unsafe {
                grid_ref(t, pt_active(0, 0), &mut out_ref)
            });

            // Extract cell from grid ref and verify it contains 'H'
            let mut out_cell: crate::cell::CCell = 0;
            assert_eq!(Result::Success, unsafe {
                crate::grid_ref::grid_ref_cell(&out_ref, &mut out_cell)
            });

            let mut cp: u32 = 0;
            assert_eq!(Result::Success, unsafe {
                crate::cell::get(
                    out_cell,
                    crate::cell::CellData::Codepoint,
                    &mut cp as *mut _ as *mut c_void,
                )
            });
            assert_eq!(b'H' as u32, cp);
            unsafe { free(t) };
        });
    }

    // test "grid_ref null terminal"
    #[test]
    fn grid_ref_null_terminal() {
        let mut out_ref = CGridRef::default();
        assert_eq!(Result::InvalidValue, unsafe {
            grid_ref(std::ptr::null_mut(), pt_active(0, 0), &mut out_ref)
        });
    }

    // test "point_from_grid_ref roundtrip active"
    #[test]
    fn point_from_grid_ref_roundtrip_active() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24, 10_000);
            assert_eq!(Result::Success, new_result);

            vtw(t, b"Hello");

            // Get a grid ref at (2, 0) in active coords
            let mut ref_ = CGridRef::default();
            assert_eq!(Result::Success, unsafe {
                grid_ref(t, pt_active(2, 0), &mut ref_)
            });

            // Convert back to active coords
            let mut coord = Coordinate { x: 0, y: 0 };
            assert_eq!(Result::Success, unsafe {
                point_from_grid_ref(t, &ref_, Tag::Active, &mut coord)
            });
            assert_eq!(2 as size::CellCountInt, coord.x);
            assert_eq!(0u32, coord.y);
            unsafe { free(t) };
        });
    }

    // test "point_from_grid_ref roundtrip viewport"
    #[test]
    fn point_from_grid_ref_roundtrip_viewport() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24, 10_000);
            assert_eq!(Result::Success, new_result);

            vtw(t, b"Hello");

            let mut ref_ = CGridRef::default();
            assert_eq!(Result::Success, unsafe {
                grid_ref(t, pt_viewport(0, 0), &mut ref_)
            });

            let mut coord = Coordinate { x: 0, y: 0 };
            assert_eq!(Result::Success, unsafe {
                point_from_grid_ref(t, &ref_, Tag::Viewport, &mut coord)
            });
            assert_eq!(0 as size::CellCountInt, coord.x);
            assert_eq!(0u32, coord.y);
            unsafe { free(t) };
        });
    }

    // test "point_from_grid_ref history ref to active returns no_value"
    #[test]
    fn point_from_grid_ref_history_ref_to_active_returns_no_value() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 4, 10_000);
            assert_eq!(Result::Success, new_result);

            // Write enough lines to push content into scrollback
            for _ in 0..10 {
                vtw(t, b"line\n");
            }

            // Get a ref to the first line (now in scrollback)
            let mut ref_ = CGridRef::default();
            assert_eq!(Result::Success, unsafe {
                grid_ref(t, pt_screen(0, 0), &mut ref_)
            });

            // Should succeed for screen coords
            let mut coord = Coordinate { x: 0, y: 0 };
            assert_eq!(Result::Success, unsafe {
                point_from_grid_ref(t, &ref_, Tag::Screen, &mut coord)
            });
            assert_eq!(0u32, coord.y);

            // Should fail for active coords (it's in scrollback)
            assert_eq!(Result::NoValue, unsafe {
                point_from_grid_ref(t, &ref_, Tag::Active, &mut coord)
            });
            unsafe { free(t) };
        });
    }

    // test "point_from_grid_ref null terminal"
    #[test]
    fn point_from_grid_ref_null_terminal() {
        let ref_ = CGridRef::default();
        assert_eq!(Result::InvalidValue, unsafe {
            point_from_grid_ref(
                std::ptr::null_mut(),
                &ref_,
                Tag::Active,
                std::ptr::null_mut(),
            )
        });
    }

    // test "point_from_grid_ref null node"
    #[test]
    fn point_from_grid_ref_null_node() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24, 0);
            assert_eq!(Result::Success, new_result);

            let ref_ = CGridRef::default();
            assert_eq!(Result::InvalidValue, unsafe {
                point_from_grid_ref(t, &ref_, Tag::Active, std::ptr::null_mut())
            });
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

    // test "set write_pty callback"
    #[test]
    fn set_write_pty_callback() {
        static mut LAST_DATA: Option<Vec<u8>> = None;
        static mut LAST_USERDATA: *mut c_void = std::ptr::null_mut();
        extern "C" fn write_pty(_t: Terminal, ud: *mut c_void, ptr: *const u8, len: usize) {
            let data = unsafe { core::slice::from_raw_parts(ptr, len) }.to_vec();
            unsafe {
                LAST_DATA = Some(data);
                LAST_USERDATA = ud;
            }
        }

        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24, 0);
            assert_eq!(Result::Success, new_result);
            unsafe {
                LAST_DATA = None;
                LAST_USERDATA = std::ptr::null_mut();
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
            let (new_result, t) = new_t(alloc, 80, 24, 0);
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
            let (new_result, t) = new_t(alloc, 80, 24, 0);
            assert_eq!(Result::Success, new_result);
            // Without setting a callback, DECRQM should be silently ignored (no crash)
            vtw(t, b"\x1B[?7$p");
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
            let (new_result, t) = new_t(alloc, 80, 24, 0);
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
            let (new_result, t) = new_t(alloc, 80, 24, 0);
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
            let (new_result, t) = new_t(alloc, 80, 24, 0);
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
            let (new_result, t) = new_t(alloc, 80, 24, 0);
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
            let (new_result, t) = new_t(alloc, 80, 24, 0);
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
            let (new_result, t) = new_t(alloc, 80, 24, 0);
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
            let (new_result, t) = new_t(alloc, 80, 24, 0);
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
            let (new_result, t) = new_t(alloc, 80, 24, 0);
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

    // test "title_changed without callback is silent"
    #[test]
    fn title_changed_without_callback_is_silent() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24, 0);
            assert_eq!(Result::Success, new_result);
            // OSC 2 without a callback should not crash
            vtw(t, b"\x1B]2;Hello\x1B\\");
            unsafe { free(t) };
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
            let (new_result, t) = new_t(alloc, 80, 24, 0);
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

    // test "set clipboard_write callback" (oracle 634ef7198)
    #[test]
    fn set_clipboard_write_callback() {
        static mut COUNT: usize = 0;
        static mut LAST_TERMINAL: Terminal = std::ptr::null_mut();
        static mut LAST_USERDATA: *mut c_void = std::ptr::null_mut();
        static mut LAST_SIZE: usize = 0;
        static mut LAST_LOCATION: clipboard::Location = clipboard::Location::Standard;
        static mut LAST_CONTENTS_NULL: bool = false;
        static mut LAST_CONTENTS_LEN: usize = 0;
        static mut LAST_MIMES: Vec<Vec<u8>> = Vec::new();
        static mut LAST_DATA: Vec<Vec<u8>> = Vec::new();
        static mut NEXT_RESULT: clipboard::WriteResult = clipboard::WriteResult::Success;

        extern "C" fn clipboard_write(
            terminal_: Terminal,
            ud: *mut c_void,
            request: *const ClipboardWrite,
        ) -> clipboard::WriteResult {
            unsafe {
                COUNT += 1;
                LAST_TERMINAL = terminal_;
                LAST_USERDATA = ud;
                LAST_SIZE = (*request).size;
                LAST_LOCATION = (*request).location;
                LAST_CONTENTS_NULL = (*request).contents.is_null();
                LAST_CONTENTS_LEN = (*request).contents_len;
                let mut mimes: Vec<Vec<u8>> = Vec::new();
                let mut data: Vec<Vec<u8>> = Vec::new();
                if !(*request).contents.is_null() {
                    let contents =
                        std::slice::from_raw_parts((*request).contents, (*request).contents_len);
                    for c in contents {
                        mimes.push(std::slice::from_raw_parts(c.mime.ptr, c.mime.len).to_vec());
                        data.push(std::slice::from_raw_parts(c.data.ptr, c.data.len).to_vec());
                    }
                }
                LAST_MIMES = mimes;
                LAST_DATA = data;
                NEXT_RESULT
            }
        }

        with_test_allocator(|alloc| {
            let (new_result, t) = new_t(alloc, 80, 24, 0);
            assert_eq!(Result::Success, new_result);
            unsafe {
                COUNT = 0;
                LAST_TERMINAL = std::ptr::null_mut();
                LAST_USERDATA = std::ptr::null_mut();
                NEXT_RESULT = clipboard::WriteResult::Denied;
            }

            let mut sentinel: u8 = 88;
            assert_eq!(Result::Success, unsafe {
                set(t, Option_::Userdata, &mut sentinel as *mut u8 as *const c_void)
            });
            assert_eq!(Result::Success, unsafe {
                set(t, Option_::ClipboardWrite, clipboard_write as *const c_void)
            });

            // Split OSC 52 write whose decoded payload contains an embedded NUL.
            vtw(t, b"\x1B]52;c;aGVs");
            vtw(t, b"bG8Ad29ybGQ=\x1B\\");
            assert_eq!(1, unsafe { COUNT });
            assert_eq!(t, unsafe { LAST_TERMINAL });
            assert_eq!(&mut sentinel as *mut u8 as *mut c_void, unsafe { LAST_USERDATA });
            assert_eq!(std::mem::size_of::<ClipboardWrite>(), unsafe { LAST_SIZE });
            assert_eq!(clipboard::Location::Standard, unsafe { LAST_LOCATION });
            assert!(!unsafe { LAST_CONTENTS_NULL });
            assert_eq!(1, unsafe { LAST_CONTENTS_LEN });
            assert_eq!(b"text/plain".as_slice(), unsafe { (&*std::ptr::addr_of!(LAST_MIMES))[0].as_slice() });
            assert_eq!(b"hello\x00world".as_slice(), unsafe { (&*std::ptr::addr_of!(LAST_DATA))[0].as_slice() });

            // OSC 52 destinations are normalized rather than exposed as wire bytes.
            for (selector, expected) in [
                (b's', clipboard::Location::Selection),
                (b'p', clipboard::Location::Primary),
                (b'q', clipboard::Location::Standard),
            ] {
                let seq = [
                    0x1B, b']', b'5', b'2', b';', selector, b';', b'e', b'A', b'=', b'=', 0x1B, b'\\',
                ];
                vtw(t, &seq);
                assert_eq!(expected, unsafe { LAST_LOCATION });
            }
            assert_eq!(4, unsafe { COUNT });

            // An empty content list is a clear and retains a null descriptor pointer.
            vtw(t, b"\x1B]52;s;\x1B\\");
            assert_eq!(5, unsafe { COUNT });
            assert_eq!(clipboard::Location::Selection, unsafe { LAST_LOCATION });
            assert!(unsafe { LAST_CONTENTS_NULL });
            assert_eq!(0, unsafe { LAST_CONTENTS_LEN });

            // Read requests and malformed base64 must never reach the callback.
            vtw(t, b"\x1B]52;c;?\x1B\\");
            vtw(t, b"\x1B]52;c;%%%\x1B\\");
            assert_eq!(5, unsafe { COUNT });

            // iTerm2 Copy reaches the same normalized callback.
            vtw(t, b"\x1B]1337;Copy=:aVRlcm0=\x1B\\");
            assert_eq!(6, unsafe { COUNT });
            assert_eq!(clipboard::Location::Standard, unsafe { LAST_LOCATION });
            assert_eq!(b"text/plain".as_slice(), unsafe { (&*std::ptr::addr_of!(LAST_MIMES))[0].as_slice() });
            assert_eq!(b"iTerm".as_slice(), unsafe { (&*std::ptr::addr_of!(LAST_DATA))[0].as_slice() });

            // Every representation is converted, and callback results propagate
            // through the C trampoline for protocols that can acknowledge writes.
            // (Direct invoke of the installed trampoline, mirroring the oracle.)
            let internal_contents = [
                clipboard::Content { mime: b"text/plain", data: b"plain" },
                clipboard::Content { mime: b"application/octet-stream", data: b"a\x00b" },
                clipboard::Content { mime: b"text/html", data: b"<b>plain</b>" },
                clipboard::Content { mime: b"text/rtf", data: b"{\\rtf1 plain}" },
                clipboard::Content { mime: b"image/png", data: b"\x89PNG" },
            ];
            unsafe { NEXT_RESULT = clipboard::WriteResult::Busy };
            let write_result = Effects::clipboard_write_trampoline(
                unsafe { &mut (*t).stream.handler },
                clipboard::Write {
                    location: clipboard::Location::Primary,
                    contents: &internal_contents,
                },
            );
            assert_eq!(clipboard::WriteResult::Busy, write_result);
            assert_eq!(7, unsafe { COUNT });
            assert_eq!(5, unsafe { LAST_CONTENTS_LEN });
            assert_eq!(b"application/octet-stream".as_slice(), unsafe { (&*std::ptr::addr_of!(LAST_MIMES))[1].as_slice() });
            assert_eq!(b"a\x00b".as_slice(), unsafe { (&*std::ptr::addr_of!(LAST_DATA))[1].as_slice() });
            assert_eq!(b"image/png".as_slice(), unsafe { (&*std::ptr::addr_of!(LAST_MIMES))[4].as_slice() });
            assert_eq!(b"\x89PNG".as_slice(), unsafe { (&*std::ptr::addr_of!(LAST_DATA))[4].as_slice() });

            unsafe {
                LAST_MIMES = Vec::new();
                LAST_DATA = Vec::new();
                free(t);
            }
        });
    }

    // test "pwd_changed without callback is silent"
    #[test]
    fn pwd_changed_without_callback_is_silent() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24, 0);
            assert_eq!(Result::Success, new_result);
            // OSC 7 without a callback should not crash, but should still set the pwd
            vtw(t, b"\x1B]7;file:///tmp\x1B\\");
            assert_eq!(b"file:///tmp".as_slice(), unsafe {
                (*zig_terminal(t)).get_pwd().unwrap()
            });
            unsafe { free(t) };
        });
    }

    // test "set size callback"
    #[test]
    fn set_size_callback() {
        static mut LAST_DATA: Option<Vec<u8>> = None;
        extern "C" fn write_pty(_t: Terminal, _ud: *mut c_void, ptr: *const u8, len: usize) {
            let data = unsafe { core::slice::from_raw_parts(ptr, len) }.to_vec();
            unsafe { LAST_DATA = Some(data) };
        }
        extern "C" fn size_cb(
            _t: Terminal,
            _ud: *mut c_void,
            out_size: *mut size_report::Size,
        ) -> bool {
            unsafe {
                *out_size = size_report::Size {
                    rows: 24,
                    columns: 80,
                    cell_width: 8,
                    cell_height: 16,
                };
            }
            true
        }

        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24, 0);
            assert_eq!(Result::Success, new_result);
            unsafe { LAST_DATA = None };

            assert_eq!(Result::Success, unsafe {
                set(t, Option_::WritePty, write_pty as *const c_void)
            });
            assert_eq!(Result::Success, unsafe {
                set(t, Option_::SizeCb, size_cb as *const c_void)
            });

            // CSI 18 t — report text area size in characters
            vtw(t, b"\x1B[18t");
            assert!(unsafe { (*std::ptr::addr_of!(LAST_DATA)).as_ref() }.is_some());
            assert_eq!(
                b"\x1b[8;24;80t".as_slice(),
                unsafe { (*std::ptr::addr_of!(LAST_DATA)).as_ref() }
                    .unwrap()
                    .as_slice()
            );
            unsafe { free(t) };
        });
    }

    // test "size without callback is silent"
    #[test]
    fn size_without_callback_is_silent() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24, 0);
            assert_eq!(Result::Success, new_result);
            // CSI 18 t without a size callback should not crash
            vtw(t, b"\x1B[18t");
            unsafe { free(t) };
        });
    }

    // test "set device_attributes callback primary"
    #[test]
    fn set_device_attributes_callback_primary() {
        static mut LAST_DATA: Option<Vec<u8>> = None;
        extern "C" fn write_pty(_t: Terminal, _ud: *mut c_void, ptr: *const u8, len: usize) {
            let data = unsafe { core::slice::from_raw_parts(ptr, len) }.to_vec();
            unsafe { LAST_DATA = Some(data) };
        }
        extern "C" fn da(_t: Terminal, _ud: *mut c_void, out: *mut CDeviceAttributes) -> bool {
            let mut features = [0u16; 64];
            features[0] = 22;
            features[1] = 52;
            unsafe {
                *out = CDeviceAttributes {
                    primary: CDeviceAttributesPrimary {
                        conformance_level: 64,
                        features,
                        num_features: 2,
                    },
                    secondary: CDeviceAttributesSecondary {
                        device_type: 1,
                        firmware_version: 10,
                        rom_cartridge: 0,
                    },
                    tertiary: CDeviceAttributesTertiary { unit_id: 0 },
                };
            }
            true
        }

        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24, 0);
            assert_eq!(Result::Success, new_result);
            unsafe { LAST_DATA = None };

            assert_eq!(Result::Success, unsafe {
                set(t, Option_::WritePty, write_pty as *const c_void)
            });
            assert_eq!(Result::Success, unsafe {
                set(t, Option_::DeviceAttributes, da as *const c_void)
            });

            // CSI c — primary DA
            vtw(t, b"\x1B[c");
            assert!(unsafe { (*std::ptr::addr_of!(LAST_DATA)).as_ref() }.is_some());
            assert_eq!(
                b"\x1b[?64;22;52c".as_slice(),
                unsafe { (*std::ptr::addr_of!(LAST_DATA)).as_ref() }
                    .unwrap()
                    .as_slice()
            );
            unsafe { free(t) };
        });
    }

    // test "set device_attributes callback secondary"
    #[test]
    fn set_device_attributes_callback_secondary() {
        static mut LAST_DATA: Option<Vec<u8>> = None;
        extern "C" fn write_pty(_t: Terminal, _ud: *mut c_void, ptr: *const u8, len: usize) {
            let data = unsafe { core::slice::from_raw_parts(ptr, len) }.to_vec();
            unsafe { LAST_DATA = Some(data) };
        }
        extern "C" fn da(_t: Terminal, _ud: *mut c_void, out: *mut CDeviceAttributes) -> bool {
            let mut features = [0u16; 64];
            features[0] = 22;
            unsafe {
                *out = CDeviceAttributes {
                    primary: CDeviceAttributesPrimary {
                        conformance_level: 62,
                        features,
                        num_features: 1,
                    },
                    secondary: CDeviceAttributesSecondary {
                        device_type: 1,
                        firmware_version: 10,
                        rom_cartridge: 0,
                    },
                    tertiary: CDeviceAttributesTertiary { unit_id: 0 },
                };
            }
            true
        }

        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24, 0);
            assert_eq!(Result::Success, new_result);
            unsafe { LAST_DATA = None };

            assert_eq!(Result::Success, unsafe {
                set(t, Option_::WritePty, write_pty as *const c_void)
            });
            assert_eq!(Result::Success, unsafe {
                set(t, Option_::DeviceAttributes, da as *const c_void)
            });

            // CSI > c — secondary DA
            vtw(t, b"\x1B[>c");
            assert!(unsafe { (*std::ptr::addr_of!(LAST_DATA)).as_ref() }.is_some());
            assert_eq!(
                b"\x1b[>1;10;0c".as_slice(),
                unsafe { (*std::ptr::addr_of!(LAST_DATA)).as_ref() }
                    .unwrap()
                    .as_slice()
            );
            unsafe { free(t) };
        });
    }

    // test "set device_attributes callback tertiary"
    #[test]
    fn set_device_attributes_callback_tertiary() {
        static mut LAST_DATA: Option<Vec<u8>> = None;
        extern "C" fn write_pty(_t: Terminal, _ud: *mut c_void, ptr: *const u8, len: usize) {
            let data = unsafe { core::slice::from_raw_parts(ptr, len) }.to_vec();
            unsafe { LAST_DATA = Some(data) };
        }
        extern "C" fn da(_t: Terminal, _ud: *mut c_void, out: *mut CDeviceAttributes) -> bool {
            unsafe {
                *out = CDeviceAttributes {
                    primary: CDeviceAttributesPrimary {
                        conformance_level: 62,
                        features: [0u16; 64],
                        num_features: 0,
                    },
                    secondary: CDeviceAttributesSecondary {
                        device_type: 1,
                        firmware_version: 0,
                        rom_cartridge: 0,
                    },
                    tertiary: CDeviceAttributesTertiary {
                        unit_id: 0xAABBCCDD,
                    },
                };
            }
            true
        }

        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24, 0);
            assert_eq!(Result::Success, new_result);
            unsafe { LAST_DATA = None };

            assert_eq!(Result::Success, unsafe {
                set(t, Option_::WritePty, write_pty as *const c_void)
            });
            assert_eq!(Result::Success, unsafe {
                set(t, Option_::DeviceAttributes, da as *const c_void)
            });

            // CSI = c — tertiary DA
            vtw(t, b"\x1B[=c");
            assert!(unsafe { (*std::ptr::addr_of!(LAST_DATA)).as_ref() }.is_some());
            assert_eq!(
                b"\x1bP!|AABBCCDD\x1b\\".as_slice(),
                unsafe { (*std::ptr::addr_of!(LAST_DATA)).as_ref() }
                    .unwrap()
                    .as_slice()
            );
            unsafe { free(t) };
        });
    }

    // test "device_attributes without callback uses default"
    #[test]
    fn device_attributes_without_callback_uses_default() {
        static mut LAST_DATA: Option<Vec<u8>> = None;
        extern "C" fn write_pty(_t: Terminal, _ud: *mut c_void, ptr: *const u8, len: usize) {
            let data = unsafe { core::slice::from_raw_parts(ptr, len) }.to_vec();
            unsafe { LAST_DATA = Some(data) };
        }

        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24, 0);
            assert_eq!(Result::Success, new_result);
            unsafe { LAST_DATA = None };

            assert_eq!(Result::Success, unsafe {
                set(t, Option_::WritePty, write_pty as *const c_void)
            });

            // Without setting a device_attributes callback, DA1 should return the default
            vtw(t, b"\x1B[c");
            assert!(unsafe { (*std::ptr::addr_of!(LAST_DATA)).as_ref() }.is_some());
            assert_eq!(
                b"\x1b[?62;22c".as_slice(),
                unsafe { (*std::ptr::addr_of!(LAST_DATA)).as_ref() }
                    .unwrap()
                    .as_slice()
            );
            unsafe { free(t) };
        });
    }

    // test "device_attributes callback returns false uses default"
    #[test]
    fn device_attributes_callback_returns_false_uses_default() {
        static mut LAST_DATA: Option<Vec<u8>> = None;
        extern "C" fn write_pty(_t: Terminal, _ud: *mut c_void, ptr: *const u8, len: usize) {
            let data = unsafe { core::slice::from_raw_parts(ptr, len) }.to_vec();
            unsafe { LAST_DATA = Some(data) };
        }
        extern "C" fn da(_t: Terminal, _ud: *mut c_void, _out: *mut CDeviceAttributes) -> bool {
            false
        }

        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24, 0);
            assert_eq!(Result::Success, new_result);
            unsafe { LAST_DATA = None };

            assert_eq!(Result::Success, unsafe {
                set(t, Option_::WritePty, write_pty as *const c_void)
            });
            assert_eq!(Result::Success, unsafe {
                set(t, Option_::DeviceAttributes, da as *const c_void)
            });

            // Callback returns false, should use default response
            vtw(t, b"\x1B[c");
            assert!(unsafe { (*std::ptr::addr_of!(LAST_DATA)).as_ref() }.is_some());
            assert_eq!(
                b"\x1b[?62;22c".as_slice(),
                unsafe { (*std::ptr::addr_of!(LAST_DATA)).as_ref() }
                    .unwrap()
                    .as_slice()
            );
            unsafe { free(t) };
        });
    }

    // test "set and get title"
    #[test]
    fn set_and_get_title() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24, 0);
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
            let (new_result, t) = new_t(alloc, 80, 24, 0);
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
            let (new_result, t) = new_t(alloc, 80, 24, 0);
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

    // test "resize updates pixel dimensions"
    #[test]
    fn resize_updates_pixel_dimensions() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24, 0);
            assert_eq!(Result::Success, new_result);

            assert_eq!(Result::Success, unsafe { resize(t, 100, 40, 9, 18) });

            let zt = unsafe { (*t).terminal };
            assert_eq!(100u32 * 9, unsafe { (*zt).width_px });
            assert_eq!(40u32 * 18, unsafe { (*zt).height_px });
            unsafe { free(t) };
        });
    }

    // test "resize pixel overflow saturates"
    #[test]
    fn resize_pixel_overflow_saturates() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24, 0);
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
            let (new_result, t) = new_t(alloc, 80, 24, 0);
            assert_eq!(Result::Success, new_result);

            let zt = unsafe { (*t).terminal };
            unsafe { (*zt).modes.set(modes::Mode::SynchronizedOutput, true) };

            assert_eq!(Result::Success, unsafe { resize(t, 100, 40, 9, 18) });
            assert!(!unsafe { (*zt).modes.get(modes::Mode::SynchronizedOutput) });
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
            let (new_result, t) = new_t(alloc, 80, 24, 0);
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
            let (new_result, t) = new_t(alloc, 80, 24, 0);
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
            let (new_result, t) = new_t(alloc, 80, 24, 0);
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
            let (new_result, t) = new_t(alloc, 80, 24, 0);
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
            let (new_result, t) = new_t(alloc, 80, 24, 0);
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
            let (new_result, t) = new_t(alloc, 80, 24, 0);
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
            let (new_result, t) = new_t(alloc, 80, 24, 0);
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
            let (new_result, t) = new_t(alloc, 80, 24, 0);
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
            let (new_result, t) = new_t(alloc, 80, 24, 0);
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
            let (new_result, t) = new_t(alloc, 80, 24, 0);
            assert_eq!(Result::Success, new_result);

            // Get default palette
            let mut palette: crate::color::PaletteC =
                [crate::color::RgbC { r: 0, g: 0, b: 0 }; 256];
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

    // test "get color default vs effective with override"
    #[test]
    fn get_color_default_vs_effective_with_override() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24, 0);
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
            let (new_result, t) = new_t(alloc, 80, 24, 0);
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
            let (new_result, t) = new_t(alloc, 80, 24, 0);
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
            let mut palette: crate::color::PaletteC =
                [crate::color::RgbC { r: 0, g: 0, b: 0 }; 256];
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
            let (new_result, t) = new_t(alloc, 80, 24, 0);
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
    fn set_glyph_protocol_disables_apc_handling_and_clears_glossary() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24, 0);
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

    // test "get_multi success"
    #[test]
    fn get_multi_success() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24, 0);
            assert_eq!(Result::Success, new_result);

            let mut cols: u16 = 0;
            let mut rows: u16 = 0;
            let mut written: usize = 0;

            let keys = [TerminalData::Cols, TerminalData::Rows];
            let mut values: [*mut c_void; 2] = [
                &mut cols as *mut _ as *mut c_void,
                &mut rows as *mut _ as *mut c_void,
            ];
            assert_eq!(Result::Success, unsafe {
                get_multi(
                    t,
                    keys.len(),
                    keys.as_ptr(),
                    values.as_mut_ptr(),
                    &mut written,
                )
            });
            assert_eq!(keys.len(), written);
            assert_eq!(80, cols);
            assert_eq!(24, rows);
            unsafe { free(t) };
        });
    }

    // test "get_multi error sets out_written"
    #[test]
    fn get_multi_error_sets_out_written() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24, 0);
            assert_eq!(Result::Success, new_result);

            let mut cols: u16 = 0;
            let mut written: usize = 99;

            let keys = [TerminalData::Cols, TerminalData::Invalid];
            // SB: build the two out-pointers with `addr_of_mut!` (raw), NOT
            // `&mut cols as *mut _`. Both entries point at the same `cols` (the
            // reference test passes `&cols` twice — raw C pointers); a second
            // `&mut cols` reborrow retags the whole variable and pops the first
            // pointer off the borrow stack, so `get_multi`'s write through
            // `values[0]` would be UB (rust-mut-args-strongly-protected).
            let mut values: [*mut c_void; 2] = [
                core::ptr::addr_of_mut!(cols) as *mut c_void,
                core::ptr::addr_of_mut!(cols) as *mut c_void,
            ];
            assert_eq!(Result::InvalidValue, unsafe {
                get_multi(
                    t,
                    keys.len(),
                    keys.as_ptr(),
                    values.as_mut_ptr(),
                    &mut written,
                )
            });
            assert_eq!(1, written);
            assert_eq!(80, cols);
            unsafe { free(t) };
        });
    }
}
