//! rustty-vt-c grid_ref_tracked
//!
//! PORTS: ghostty src/terminal/c/grid_ref_tracked.zig
//!
//! PORT STATUS: SKELETON (mutual-atom Stage B v3). Types are byte-exact and every
//! C API function is a SIGNATURE with a `todo!("vt-c <name>")` body. Bodies and
//! the `test {}` blocks land in a later fill sub-batch. Part of the irreducible
//! mutual cluster (terminal<->grid_ref<->grid_ref_tracked<->selection).

// const std = @import("std");
// const testing = std.testing;
// const lib = @import("../lib.zig");
// const PageList = @import("../PageList.zig");
// const point = @import("../point.zig");
// const grid_ref_c = @import("grid_ref.zig");
// const terminal_c = @import("terminal.zig");
// const Result = @import("result.zig").Result;
use crate::grid_ref::CGridRef;
use crate::point::PointC;
use crate::result::Result;
use crate::terminal::{Terminal, TerminalScreen};
use allocator_api2::alloc::Global as Allocator;
use rustty_vt::pagelist::{PageList, Pin};
use rustty_vt::point::{Coordinate, Tag};

/// C: GhosttyTrackedGridRef -> RusttyTrackedGridRef (R2)
///
/// An owned tracked reference to a position in the terminal grid. The
/// underlying PageList pin is automatically updated as the PageList changes.
// pub const CTrackedGridRef = ?*TrackedGridRef;
// PORT NOTE: the C-ABI handle is a nullable opaque pointer to the owned
// `TrackedGridRef`; ported as a raw `*mut TrackedGridRef` (null == none).
pub type CTrackedGridRef = *mut TrackedGridRef;

// pub const TrackedGridRef = struct { ... }
// PORT NOTE: `alloc: std.mem.Allocator` is the allocator that owns this ref's
// heap allocation (resolved from `t.gpa()` at `grid_ref_track` time); ported as
// the rustty-vt terminal `Allocator`. `terminal: terminal_c.Terminal` is the
// nullable wrapper back-pointer; `pin: *PageList.Pin` is the tracked PageList
// pin (raw ptr, owned by the PageList's tracked-pin set).
pub struct TrackedGridRef {
    pub alloc: Allocator,
    pub terminal: Terminal,
    pub screen_key: TerminalScreen,
    pub screen_generation: usize,
    pub pin: *mut Pin,
}

impl TrackedGridRef {
    /// Return the PageList that owns this tracked ref's pin, or null if the
    /// owning screen has been removed/reinitialized since the ref was created.
    // fn pageList(ref: *const TrackedGridRef) ?*PageList { ... }
    // PORT NOTE: only referenced from the (deferred) C API bodies; reads as dead
    // code until the fill sub-batch.
    #[allow(dead_code)]
    pub(crate) fn page_list(&self) -> Option<*mut PageList> {
        todo!("vt-c grid_ref_tracked TrackedGridRef::page_list")
    }
}

// pub fn tracked_grid_ref_free(ref_: CTrackedGridRef) callconv(lib.calling_conv) void
/// # Safety
/// `ref_`, if non-null, must be a pointer returned by `grid_ref_track` that has
/// not yet been freed.
pub unsafe fn tracked_grid_ref_free(_ref_: CTrackedGridRef) {
    todo!("vt-c tracked_grid_ref_free")
}

// pub fn tracked_grid_ref_has_value(ref_: CTrackedGridRef) callconv(lib.calling_conv) bool
/// # Safety
/// `ref_`, if non-null, must be a live `TrackedGridRef` pointer.
pub unsafe fn tracked_grid_ref_has_value(_ref_: CTrackedGridRef) -> bool {
    todo!("vt-c tracked_grid_ref_has_value")
}

// pub fn tracked_grid_ref_snapshot(ref_: CTrackedGridRef, out_ref: ?*grid_ref_c.CGridRef) ...
/// # Safety
/// `ref_`, if non-null, must be a live `TrackedGridRef` pointer; `out_ref`, if
/// non-null, must be a valid writable pointer to a `CGridRef`.
pub unsafe fn tracked_grid_ref_snapshot(_ref_: CTrackedGridRef, _out_ref: *mut CGridRef) -> Result {
    todo!("vt-c tracked_grid_ref_snapshot")
}

// pub fn tracked_grid_ref_point(ref_: CTrackedGridRef, tag: point.Tag, out: ?*point.Coordinate) ...
/// # Safety
/// `ref_`, if non-null, must be a live `TrackedGridRef` pointer; `out`, if
/// non-null, must be a valid writable pointer to a `Coordinate`.
pub unsafe fn tracked_grid_ref_point(
    _ref_: CTrackedGridRef,
    _tag: Tag,
    _out: *mut Coordinate,
) -> Result {
    todo!("vt-c tracked_grid_ref_point")
}

// pub fn tracked_grid_ref_set(ref_: CTrackedGridRef, terminal_: terminal_c.Terminal, pt: point.Point.C) ...
/// # Safety
/// `ref_`, if non-null, must be a live `TrackedGridRef` pointer; `terminal_`, if
/// non-null, must be a live terminal handle.
pub unsafe fn tracked_grid_ref_set(
    _ref_: CTrackedGridRef,
    _terminal_: Terminal,
    _pt: PointC,
) -> Result {
    todo!("vt-c tracked_grid_ref_set")
}
