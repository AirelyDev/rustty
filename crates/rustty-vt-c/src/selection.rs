//! rustty-vt-c selection
//!
//! PORTS: ghostty src/terminal/c/selection.zig
//!
//! PORT STATUS: SKELETON (mutual-atom Stage B v3). Every `#[repr(C)]` extern
//! struct is byte-exact; every C API function is a SIGNATURE with a
//! `todo!("vt-c <name>")` body. Bodies, the private `formatSelection`/
//! `selectionFormatter`/`codepointSlice` helpers, and the `test {}` blocks land
//! in a later fill sub-batch. Part of the irreducible mutual cluster
//! (terminal<->grid_ref<->grid_ref_tracked<->selection).

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
use std::mem::size_of;

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

/// C: GhosttySelectionAdjustment — the `enum(c_int)` projection of
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

/// C: GhosttySelectionOrder — the `enum(c_int)` projection of `Selection.Order`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Order {
    Forward = 0,
    Reverse = 1,
    MirroredForward = 2,
    MirroredReverse = 3,
}

/// C: GhosttySelectionFormat — the `enum(c_int)` projection of `formatter.Format`.
// PORT NOTE: the `formatter.Format` key order is `plain`, `vt`. The fill
// sub-batch pins the full discriminant set against `rustty_vt::formatter::Format`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Plain = 0,
    Vt = 1,
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
    // pub fn toZig(self: CSelection) ?Selection { ... }
    pub fn to_zig(self) -> Option<rustty_vt::selection::Selection> {
        todo!("vt-c selection CSelection::to_zig")
    }

    // pub fn fromZig(sel: Selection) CSelection { ... }
    pub fn from_zig(_sel: rustty_vt::selection::Selection) -> CSelection {
        todo!("vt-c selection CSelection::from_zig")
    }
}

/// C: GhosttyTerminalSelectWordOptions
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

/// C: GhosttyTerminalSelectWordBetweenOptions
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

/// C: GhosttyTerminalSelectLineOptions
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

/// C: GhosttyTerminalSelectionFormatOptions
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

// pub fn word(terminal, options: ?*const SelectWordOptions, out_selection: ?*CSelection) ...
/// # Safety
/// `terminal`, if non-null, must be a live terminal handle; `options`/
/// `out_selection`, if non-null, must point to valid storage of their types.
pub unsafe fn word(
    _terminal: Terminal,
    _options: *const SelectWordOptions,
    _out_selection: *mut CSelection,
) -> Result {
    todo!("vt-c selection word")
}

// pub fn word_between(terminal, options: ?*const SelectWordBetweenOptions, out_selection: ?*CSelection) ...
/// # Safety
/// See `word`.
pub unsafe fn word_between(
    _terminal: Terminal,
    _options: *const SelectWordBetweenOptions,
    _out_selection: *mut CSelection,
) -> Result {
    todo!("vt-c selection word_between")
}

// pub fn line(terminal, options: ?*const SelectLineOptions, out_selection: ?*CSelection) ...
/// # Safety
/// See `word`.
pub unsafe fn line(
    _terminal: Terminal,
    _options: *const SelectLineOptions,
    _out_selection: *mut CSelection,
) -> Result {
    todo!("vt-c selection line")
}

// pub fn all(terminal, out_selection: ?*CSelection) ...
/// # Safety
/// `terminal`, if non-null, must be a live terminal handle; `out_selection`, if
/// non-null, must be a valid writable pointer to a `CSelection`.
pub unsafe fn all(_terminal: Terminal, _out_selection: *mut CSelection) -> Result {
    todo!("vt-c selection all")
}

// pub fn output(terminal, ref: grid_ref.CGridRef, out_selection: ?*CSelection) ...
/// # Safety
/// `terminal`, if non-null, must be a live terminal handle; `out_selection`, if
/// non-null, must be a valid writable pointer to a `CSelection`.
pub unsafe fn output(
    _terminal: Terminal,
    _ref: CGridRef,
    _out_selection: *mut CSelection,
) -> Result {
    todo!("vt-c selection output")
}

// pub fn format_buf(terminal, opts: FormatOptions, out_: ?[*]u8, out_len: usize, out_written: *usize) ...
/// # Safety
/// `terminal`, if non-null, must be a live terminal handle; `out_`, if non-null,
/// must point to `out_len` writable bytes; `out_written` must be a valid
/// writable pointer.
pub unsafe fn format_buf(
    _terminal: Terminal,
    _opts: FormatOptions,
    _out_: *mut u8,
    _out_len: usize,
    _out_written: *mut usize,
) -> Result {
    todo!("vt-c selection format_buf")
}

// pub fn format_alloc(terminal, alloc_: ?*const CAllocator, opts: FormatOptions, out_ptr: *?[*]u8, out_len: *usize) ...
/// # Safety
/// `terminal`, if non-null, must be a live terminal handle; `alloc_`, if
/// non-null, must point to a valid `CAllocator`; `out_ptr`/`out_len` must be
/// valid writable pointers.
pub unsafe fn format_alloc(
    _terminal: Terminal,
    _alloc_: *const CAllocator,
    _opts: FormatOptions,
    _out_ptr: *mut *mut u8,
    _out_len: *mut usize,
) -> Result {
    todo!("vt-c selection format_alloc")
}

// pub fn adjust(terminal, selection: ?*CSelection, adjustment: Selection.Adjustment) ...
/// # Safety
/// `terminal`, if non-null, must be a live terminal handle; `selection`, if
/// non-null, must be a valid writable pointer to a `CSelection`.
pub unsafe fn adjust(
    _terminal: Terminal,
    _selection: *mut CSelection,
    _adjustment: Adjustment,
) -> Result {
    todo!("vt-c selection adjust")
}

// pub fn order(terminal, selection: ?*const CSelection, out_order: ?*Selection.Order) ...
/// # Safety
/// `terminal`, if non-null, must be a live terminal handle; `selection`/
/// `out_order`, if non-null, must point to valid storage of their types.
pub unsafe fn order(
    _terminal: Terminal,
    _selection: *const CSelection,
    _out_order: *mut Order,
) -> Result {
    todo!("vt-c selection order")
}

// pub fn ordered(terminal, selection: ?*const CSelection, desired: Selection.Order, out_selection: ?*CSelection) ...
/// # Safety
/// See `order`.
pub unsafe fn ordered(
    _terminal: Terminal,
    _selection: *const CSelection,
    _desired: Order,
    _out_selection: *mut CSelection,
) -> Result {
    todo!("vt-c selection ordered")
}

// pub fn contains(terminal, selection: ?*const CSelection, pt: point.Point.C, out_contains: ?*bool) ...
/// # Safety
/// `terminal`, if non-null, must be a live terminal handle; `selection`/
/// `out_contains`, if non-null, must point to valid storage of their types.
pub unsafe fn contains(
    _terminal: Terminal,
    _selection: *const CSelection,
    _pt: PointC,
    _out_contains: *mut bool,
) -> Result {
    todo!("vt-c selection contains")
}

// pub fn equal(terminal, a: ?*const CSelection, b: ?*const CSelection, out_equal: ?*bool) ...
/// # Safety
/// `terminal`, if non-null, must be a live terminal handle; `a`/`b`/`out_equal`,
/// if non-null, must point to valid storage of their types.
pub unsafe fn equal(
    _terminal: Terminal,
    _a: *const CSelection,
    _b: *const CSelection,
    _out_equal: *mut bool,
) -> Result {
    todo!("vt-c selection equal")
}
