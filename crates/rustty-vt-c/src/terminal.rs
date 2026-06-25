//! rustty-vt-c terminal
//!
//! PORTS: ghostty src/terminal/c/terminal.zig
//!
//! PORT STATUS: SKELETON (mutual-atom Stage B v3). Every `#[repr(C)]` extern
//! struct/enum, the hand-authored `ScrollViewport` `[2]u64` tagged union, the
//! `Effects` raw-ptr trampoline state (C fn-ptr typedefs + `CDeviceAttributes`),
//! and every C API function are laid down here. Function bodies are
//! `todo!("vt-c <name>")`; the trampolines, `setTyped`/`getTyped`, the
//! `TerminalWrapper`/`Effects` private helpers, and ALL `test {}` blocks fill in
//! a later sub-batch. This is the root of the irreducible mutual cluster
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
use rustty_vt::device_status;
use rustty_vt::modes;
use rustty_vt::pagelist::grow::ScrollbarC;
use rustty_vt::point::{Coordinate, Tag};
use rustty_vt::screen_set::Key as ScreenSetKey;
use rustty_vt::size;
use rustty_vt::size_report;
use rustty_vt::stream_terminal::Stream;
use rustty_vt::terminal::Terminal as ZigTerminal;
use std::collections::HashSet;
use std::ffi::c_void;
use std::mem::{align_of, size_of};
use std::os::raw::c_int;

// const log = std.log.scoped(.terminal_c);

/// Wrapper around ZigTerminal that tracks additional state for C API usage,
/// such as the persistent VT stream needed to handle escape sequences split
/// across multiple vt_write calls.
// PORT NOTE (internal, NOT C-ABI): `terminal: *ZigTerminal` is the raw owning
// back-pointer to the heap `ZigTerminal`. `stream: Stream` is the persistent VT
// stream whose handler aliases that terminal; in rustty `Stream<'a>` carries a
// lifetime tied to the terminal borrow, so the field is self-referential. The
// `'static` here is a SKELETON placeholder — `new`/`free` are `todo!()` so the
// wrapper is never actually constructed yet; the fill sub-batch resolves the
// self-referential storage (raw-ptr handler aliasing) before any body lands.
// `tracked_grid_refs` is the set of live `*TrackedGridRef` so `free` can detach
// them safely (Zig `AutoArrayHashMapUnmanaged(*TrackedGridRef, void)`).
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

// PORT NOTE: the trampolines are installed on the stream handler by `new` and
// invoked by the VT stream; in this skeleton they are only referenced from the
// (deferred) `new` body, so they read as dead code until the fill sub-batch.
#[allow(dead_code)]
impl Effects {
    // fn writePtyTrampoline(handler: *Handler, data: [:0]const u8) void { ... }
    pub(crate) fn write_pty_trampoline() {
        todo!("vt-c Effects::write_pty_trampoline")
    }

    // fn bellTrampoline(handler: *Handler) void { ... }
    pub(crate) fn bell_trampoline() {
        todo!("vt-c Effects::bell_trampoline")
    }

    // fn colorSchemeTrampoline(handler: *Handler) ?device_status.ColorScheme { ... }
    pub(crate) fn color_scheme_trampoline() {
        todo!("vt-c Effects::color_scheme_trampoline")
    }

    // fn deviceAttributesTrampoline(handler: *Handler) device_attributes.Attributes { ... }
    pub(crate) fn device_attributes_trampoline() {
        todo!("vt-c Effects::device_attributes_trampoline")
    }

    // fn enquiryTrampoline(handler: *Handler) []const u8 { ... }
    pub(crate) fn enquiry_trampoline() {
        todo!("vt-c Effects::enquiry_trampoline")
    }

    // fn xtversionTrampoline(handler: *Handler) []const u8 { ... }
    pub(crate) fn xtversion_trampoline() {
        todo!("vt-c Effects::xtversion_trampoline")
    }

    // fn titleChangedTrampoline(handler: *Handler) void { ... }
    pub(crate) fn title_changed_trampoline() {
        todo!("vt-c Effects::title_changed_trampoline")
    }

    // fn pwdChangedTrampoline(handler: *Handler) void { ... }
    pub(crate) fn pwd_changed_trampoline() {
        todo!("vt-c Effects::pwd_changed_trampoline")
    }

    // fn sizeTrampoline(handler: *Handler) ?size_report.Size { ... }
    pub(crate) fn size_trampoline() {
        todo!("vt-c Effects::size_trampoline")
    }
}

/// C: GhosttyTerminal -> RusttyTerminal (R2)
// pub const Terminal = ?*TerminalWrapper;
// PORT NOTE: nullable opaque handle; raw `*mut TerminalWrapper` (null == none).
pub type Terminal = *mut TerminalWrapper;

// pub fn zigTerminal(terminal_: Terminal) ?*ZigTerminal { ... }
/// # Safety
/// `terminal_`, if non-null, must be a live terminal handle.
pub unsafe fn zig_terminal(_terminal_: Terminal) -> *mut ZigTerminal {
    todo!("vt-c zig_terminal")
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

// pub fn new(alloc_: ?*const CAllocator, result: *Terminal, opts: Options) ...
/// # Safety
/// `alloc_`, if non-null, must point to a valid `CAllocator`; `result` must be a
/// valid writable pointer to a `Terminal`.
pub unsafe fn new(_alloc_: *const CAllocator, _result: *mut Terminal, _opts: Options) -> Result {
    todo!("vt-c new")
}

// pub fn vt_write(terminal_: Terminal, ptr: [*]const u8, len: usize) ...
/// # Safety
/// `terminal_`, if non-null, must be a live terminal handle; `ptr` must point to
/// `len` readable bytes.
pub unsafe fn vt_write(_terminal_: Terminal, _ptr: *const u8, _len: usize) {
    todo!("vt-c vt_write")
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

// pub fn set(terminal_: Terminal, option: Option, value: ?*const anyopaque) ...
/// # Safety
/// `terminal_`, if non-null, must be a live terminal handle; `value`, if
/// non-null, must point to storage of the type required by `option`
/// (`Option_::in_type`).
pub unsafe fn set(_terminal_: Terminal, _option: Option_, _value: *const c_void) -> Result {
    todo!("vt-c set")
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

// pub fn scroll_viewport(terminal_: Terminal, behavior: ScrollViewport) ...
/// # Safety
/// `terminal_`, if non-null, must be a live terminal handle.
pub unsafe fn scroll_viewport(_terminal_: Terminal, _behavior: ScrollViewport) {
    todo!("vt-c scroll_viewport")
}

// pub fn resize(terminal_, cols, rows, cell_width_px, cell_height_px) ...
/// # Safety
/// `terminal_`, if non-null, must be a live terminal handle.
pub unsafe fn resize(
    _terminal_: Terminal,
    _cols: size::CellCountInt,
    _rows: size::CellCountInt,
    _cell_width_px: u32,
    _cell_height_px: u32,
) -> Result {
    todo!("vt-c resize")
}

// pub fn reset(terminal_: Terminal) ...
/// # Safety
/// `terminal_`, if non-null, must be a live terminal handle.
pub unsafe fn reset(_terminal_: Terminal) {
    todo!("vt-c reset")
}

// pub fn mode_get(terminal_, tag: modes.ModeTag.Backing, out_value: *bool) ...
/// # Safety
/// `terminal_`, if non-null, must be a live terminal handle; `out_value` must be
/// a valid writable pointer to a `bool`.
pub unsafe fn mode_get(
    _terminal_: Terminal,
    _tag: modes::ModeTagBacking,
    _out_value: *mut bool,
) -> Result {
    todo!("vt-c mode_get")
}

// pub fn mode_set(terminal_, tag: modes.ModeTag.Backing, value: bool) ...
/// # Safety
/// `terminal_`, if non-null, must be a live terminal handle.
pub unsafe fn mode_set(_terminal_: Terminal, _tag: modes::ModeTagBacking, _value: bool) -> Result {
    todo!("vt-c mode_set")
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

// pub fn get(terminal_: Terminal, data: TerminalData, out: ?*anyopaque) ...
/// # Safety
/// `terminal_`, if non-null, must be a live terminal handle; `out`, if non-null,
/// must point to storage of the type required by `data` (`TerminalData::out_type`).
pub unsafe fn get(_terminal_: Terminal, _data: TerminalData, _out: *mut c_void) -> Result {
    todo!("vt-c get")
}

// pub fn get_multi(terminal_, count, keys: ?[*]const TerminalData, values: ?[*]?*anyopaque, out_written: ?*usize) ...
/// # Safety
/// `terminal_`, if non-null, must be a live terminal handle; `keys`/`values`, if
/// non-null, must point to `count` valid elements; `out_written`, if non-null,
/// must be a valid writable pointer.
pub unsafe fn get_multi(
    _terminal_: Terminal,
    _count: usize,
    _keys: *const TerminalData,
    _values: *mut *mut c_void,
    _out_written: *mut usize,
) -> Result {
    todo!("vt-c get_multi")
}

// pub fn grid_ref(terminal_: Terminal, pt: point.Point.C, out_ref: ?*grid_ref_c.CGridRef) ...
/// # Safety
/// `terminal_`, if non-null, must be a live terminal handle; `out_ref`, if
/// non-null, must be a valid writable pointer to a `CGridRef`.
pub unsafe fn grid_ref(_terminal_: Terminal, _pt: PointC, _out_ref: *mut CGridRef) -> Result {
    todo!("vt-c grid_ref")
}

// pub fn grid_ref_track(terminal_, pt: point.Point.C, out_ref: ?*grid_ref_tracked_c.CTrackedGridRef) ...
/// # Safety
/// `terminal_`, if non-null, must be a live terminal handle; `out_ref`, if
/// non-null, must be a valid writable pointer to a `CTrackedGridRef`.
pub unsafe fn grid_ref_track(
    _terminal_: Terminal,
    _pt: PointC,
    _out_ref: *mut CTrackedGridRef,
) -> Result {
    todo!("vt-c grid_ref_track")
}

// pub fn point_from_grid_ref(terminal_, ref: *const grid_ref_c.CGridRef, tag: point.Tag, out: ?*point.Coordinate) ...
/// # Safety
/// `terminal_`, if non-null, must be a live terminal handle; `ref` must point to
/// a valid `CGridRef`; `out`, if non-null, must be a valid writable pointer to a
/// `Coordinate`.
pub unsafe fn point_from_grid_ref(
    _terminal_: Terminal,
    _ref: *const CGridRef,
    _tag: Tag,
    _out: *mut Coordinate,
) -> Result {
    todo!("vt-c point_from_grid_ref")
}

// pub fn free(terminal_: Terminal) ...
/// # Safety
/// `terminal_`, if non-null, must be a live terminal handle previously returned
/// by `new` and not yet freed.
pub unsafe fn free(_terminal_: Terminal) {
    todo!("vt-c free")
}

// PORT NOTE: `TerminalData` aliases `color.PaletteC` / `color.RGB.C` /
// `style_c.Style` / `selection_c.CSelection` / `KittyGraphics` in its
// `OutType`, and `Option` aliases the same in `InType`. Those projections are
// resolved by the fill sub-batch's `setTyped`/`getTyped` bodies (and
// `color::PaletteC` itself is a DEFERRED vt-c projection per the recon); they
// are intentionally NOT imported in this types+signatures skeleton.
