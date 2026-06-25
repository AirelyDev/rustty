//! rustty-vt-c grid_ref
//!
//! PORTS: ghostty src/terminal/c/grid_ref.zig
//!
//! PORT STATUS: SKELETON (mutual-atom Stage B v3). Types are byte-exact and every
//! C API function is laid down as a SIGNATURE with a `todo!("vt-c <name>")` body.
//! The function bodies and the `test {}` blocks land in a later fill sub-batch.
//! `grid_ref.zig` is part of the irreducible mutual cluster
//! (terminal<->grid_ref<->grid_ref_tracked<->selection), so all five files are
//! laid down together so the mutual type graph compiles.

// const std = @import("std");
// const testing = std.testing;
// const lib = @import("../lib.zig");
// const page = @import("../page.zig");
// const PageList = @import("../PageList.zig");
// const point = @import("../point.zig");
// const size = @import("../size.zig");
// const stylepkg = @import("../style.zig");
// const cell_c = @import("cell.zig");
// const row_c = @import("row.zig");
// const style_c = @import("style.zig");
// const terminal_c = @import("terminal.zig");
// const Result = @import("result.zig").Result;
use crate::cell::CCell;
use crate::result::Result;
use crate::row::CRow;
use crate::style::Style;
use rustty_vt::pagelist::{Node, Pin};
use rustty_vt::size;
use std::mem::size_of;

/// C: GhosttyGridRef -> RusttyGridRef (R2)
///
/// A sized struct that holds a reference to a position in the terminal grid.
/// The ref points to a specific cell position within the terminal's
/// internal page structure.
// PORT NOTE: the reference is `extern struct` with field defaults
//   size: usize = @sizeOf(CGridRef),
//   node: ?*PageList.List.Node = null,
//   x: size.CellCountInt = 0,
//   y: size.CellCountInt = 0,
// `?*PageList.List.Node` is the nullable back-pointer to a PageList node; ported
// as a raw `*mut Node` (null == none) per the raw-ptr discipline across the C
// boundary. The defaults ride on the `Default` impl below (the reference's
// `.{}` literal).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct CGridRef {
    pub size: usize,
    pub node: *mut Node,
    pub x: size::CellCountInt,
    pub y: size::CellCountInt,
}

impl Default for CGridRef {
    fn default() -> Self {
        CGridRef {
            size: size_of::<CGridRef>(),
            node: std::ptr::null_mut(),
            x: 0,
            y: 0,
        }
    }
}

impl CGridRef {
    // pub fn fromPin(pin: PageList.Pin) CGridRef { ... }
    pub fn from_pin(_pin: Pin) -> CGridRef {
        todo!("vt-c grid_ref CGridRef::from_pin")
    }

    // pub fn toPin(self: CGridRef) ?PageList.Pin { ... }
    pub fn to_pin(self) -> Option<Pin> {
        todo!("vt-c grid_ref CGridRef::to_pin")
    }
}

// pub fn grid_ref_cell(ref: *const CGridRef, out: ?*cell_c.CCell) ...
/// # Safety
/// `ref_` must point to a valid `CGridRef`; `out`, if non-null, must be a valid
/// writable pointer to a `CCell`.
pub unsafe fn grid_ref_cell(_ref_: *const CGridRef, _out: *mut CCell) -> Result {
    todo!("vt-c grid_ref_cell")
}

// pub fn grid_ref_row(ref: *const CGridRef, out: ?*row_c.CRow) ...
/// # Safety
/// `ref_` must point to a valid `CGridRef`; `out`, if non-null, must be a valid
/// writable pointer to a `CRow`.
pub unsafe fn grid_ref_row(_ref_: *const CGridRef, _out: *mut CRow) -> Result {
    todo!("vt-c grid_ref_row")
}

// pub fn grid_ref_graphemes(ref: *const CGridRef, out_buf: ?[*]u32, buf_len: usize, out_len: *usize) ...
/// # Safety
/// `ref_` must point to a valid `CGridRef`; `out_buf`, if non-null, must point to
/// `buf_len` writable `u32`s; `out_len` must be a valid writable pointer.
pub unsafe fn grid_ref_graphemes(
    _ref_: *const CGridRef,
    _out_buf: *mut u32,
    _buf_len: usize,
    _out_len: *mut usize,
) -> Result {
    todo!("vt-c grid_ref_graphemes")
}

// pub fn grid_ref_hyperlink_uri(ref: *const CGridRef, out_buf: ?[*]u8, buf_len: usize, out_len: *usize) ...
/// # Safety
/// `ref_` must point to a valid `CGridRef`; `out_buf`, if non-null, must point to
/// `buf_len` writable bytes; `out_len` must be a valid writable pointer.
pub unsafe fn grid_ref_hyperlink_uri(
    _ref_: *const CGridRef,
    _out_buf: *mut u8,
    _buf_len: usize,
    _out_len: *mut usize,
) -> Result {
    todo!("vt-c grid_ref_hyperlink_uri")
}

// pub fn grid_ref_style(ref: *const CGridRef, out: ?*style_c.Style) ...
/// # Safety
/// `ref_` must point to a valid `CGridRef`; `out`, if non-null, must be a valid
/// writable pointer to a `Style`.
pub unsafe fn grid_ref_style(_ref_: *const CGridRef, _out: *mut Style) -> Result {
    todo!("vt-c grid_ref_style")
}
