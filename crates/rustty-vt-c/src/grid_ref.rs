//! rustty-vt-c grid_ref
//!
//! PORTS: ghostty src/terminal/c/grid_ref.zig
//!
//! PORT STATUS: FILLED (mutual-atom Stage B v4). Types are byte-exact, the
//! `from_pin`/`to_pin` conversions and every C API function body are ported, and
//! all `test {}` blocks are present (assertion-for-assertion). `grid_ref.zig` is
//! part of the irreducible mutual cluster
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
    // pub fn fromPin(pin: PageList.Pin) CGridRef {
    //     return .{ .node = pin.node, .x = pin.x, .y = pin.y };
    // }
    // PORT NOTE: `.size` and any other field ride the `Default` impl (the
    // reference's `.{ ... }` literal fills `.size = @sizeOf(CGridRef)`).
    pub fn from_pin(pin: Pin) -> CGridRef {
        CGridRef {
            node: pin.node,
            x: pin.x,
            y: pin.y,
            ..CGridRef::default()
        }
    }

    // pub fn toPin(self: CGridRef) ?PageList.Pin {
    //     return .{ .node = self.node orelse return null, .x = self.x, .y = self.y };
    // }
    // PORT NOTE: a null `node` is the reference's `orelse return null`. The
    // resulting `Pin`'s `garbage` flag rides the rustty `Pin` default (`false`),
    // matching the reference's `.{ ... }` literal (no `garbage` field set).
    pub fn to_pin(self) -> Option<Pin> {
        if self.node.is_null() {
            return None;
        }
        Some(Pin {
            node: self.node,
            x: self.x,
            y: self.y,
            garbage: false,
        })
    }
}

// pub fn grid_ref_cell(ref: *const CGridRef, out: ?*cell_c.CCell) ... {
//     const p = ref.toPin() orelse return .invalid_value;
//     if (out) |o| o.* = @bitCast(p.rowAndCell().cell.*);
//     return .success;
// }
/// # Safety
/// `ref_` must point to a valid `CGridRef`; `out`, if non-null, must be a valid
/// writable pointer to a `CCell`.
pub unsafe fn grid_ref_cell(ref_: *const CGridRef, out: *mut CCell) -> Result {
    // const p = ref.toPin() orelse return .invalid_value;
    let p = match unsafe { (*ref_).to_pin() } {
        Some(p) => p,
        None => return Result::InvalidValue,
    };
    // if (out) |o| o.* = @bitCast(p.rowAndCell().cell.*);
    if !out.is_null() {
        let cell = p.row_and_cell().cell;
        // SAFETY: `cell` is a live cell pointer from the pinned page; `out` is a
        // valid writable `CCell` (= u64) per the caller contract. `@bitCast` of
        // the `Cell` is its `u64` bit-pattern.
        unsafe { *out = (*cell).to_bits() };
    }
    // return .success;
    Result::Success
}

// pub fn grid_ref_row(ref: *const CGridRef, out: ?*row_c.CRow) ... {
//     const p = ref.toPin() orelse return .invalid_value;
//     if (out) |o| o.* = @bitCast(p.rowAndCell().row.*);
//     return .success;
// }
/// # Safety
/// `ref_` must point to a valid `CGridRef`; `out`, if non-null, must be a valid
/// writable pointer to a `CRow`.
pub unsafe fn grid_ref_row(ref_: *const CGridRef, out: *mut CRow) -> Result {
    // const p = ref.toPin() orelse return .invalid_value;
    let p = match unsafe { (*ref_).to_pin() } {
        Some(p) => p,
        None => return Result::InvalidValue,
    };
    // if (out) |o| o.* = @bitCast(p.rowAndCell().row.*);
    if !out.is_null() {
        let row = p.row_and_cell().row;
        // SAFETY: `row` is a live row pointer; `out` is a valid writable `CRow`
        // (= u64). `@bitCast` of the `Row` is its `u64` bit-pattern.
        unsafe { *out = (*row).to_bits() };
    }
    // return .success;
    Result::Success
}

// pub fn grid_ref_graphemes(ref: *const CGridRef, out_buf: ?[*]u32, buf_len: usize, out_len: *usize) ... {
//     const p = ref.toPin() orelse return .invalid_value;
//     const cell = p.rowAndCell().cell;
//     if (!cell.hasText()) { out_len.* = 0; return .success; }
//     const cp = cell.codepoint();
//     const extra = if (cell.hasGrapheme()) p.grapheme(cell) else null;
//     const total = 1 + if (extra) |e| e.len else 0;
//     if (out_buf == null or buf_len < total) { out_len.* = total; return .out_of_space; }
//     const buf = out_buf.?[0..buf_len];
//     buf[0] = cp;
//     if (extra) |e| for (e, 1..) |c, i| { buf[i] = c; };
//     out_len.* = total;
//     return .success;
// }
/// # Safety
/// `ref_` must point to a valid `CGridRef`; `out_buf`, if non-null, must point to
/// `buf_len` writable `u32`s; `out_len` must be a valid writable pointer.
pub unsafe fn grid_ref_graphemes(
    ref_: *const CGridRef,
    out_buf: *mut u32,
    buf_len: usize,
    out_len: *mut usize,
) -> Result {
    // const p = ref.toPin() orelse return .invalid_value;
    let p = match unsafe { (*ref_).to_pin() } {
        Some(p) => p,
        None => return Result::InvalidValue,
    };
    // const cell = p.rowAndCell().cell;
    let cell = p.row_and_cell().cell;

    // if (!cell.hasText()) { out_len.* = 0; return .success; }
    // SAFETY: `cell` is a live cell pointer from the pinned page.
    if !unsafe { (*cell).has_text() } {
        unsafe { *out_len = 0 };
        return Result::Success;
    }

    // const cp = cell.codepoint();
    let cp = unsafe { (*cell).codepoint() };
    // const extra = if (cell.hasGrapheme()) p.grapheme(cell) else null;
    let extra: Option<&[u32]> = if unsafe { (*cell).has_grapheme() } {
        p.grapheme(cell)
    } else {
        None
    };
    // const total = 1 + if (extra) |e| e.len else 0;
    let total = 1 + extra.map_or(0, |e| e.len());

    // if (out_buf == null or buf_len < total) { out_len.* = total; return .out_of_space; }
    if out_buf.is_null() || buf_len < total {
        unsafe { *out_len = total };
        return Result::OutOfSpace;
    }

    // const buf = out_buf.?[0..buf_len];
    // SAFETY: `out_buf` is non-null and points to `buf_len` writable u32s.
    let buf = unsafe { core::slice::from_raw_parts_mut(out_buf, buf_len) };
    // buf[0] = cp;
    buf[0] = cp;
    // if (extra) |e| for (e, 1..) |c, i| { buf[i] = c; };
    if let Some(e) = extra {
        for (i, &c) in e.iter().enumerate() {
            buf[i + 1] = c;
        }
    }

    // out_len.* = total;
    unsafe { *out_len = total };
    // return .success;
    Result::Success
}

// pub fn grid_ref_hyperlink_uri(ref: *const CGridRef, out_buf: ?[*]u8, buf_len: usize, out_len: *usize) ... {
//     const p = ref.toPin() orelse return .invalid_value;
//     const rac = p.node.data.getRowAndCell(p.x, p.y);
//     const cell = rac.cell;
//     if (!cell.hyperlink) { out_len.* = 0; return .success; }
//     const link_id = p.node.data.lookupHyperlink(cell) orelse { out_len.* = 0; return .success; };
//     const entry = p.node.data.hyperlink_set.get(p.node.data.memory, link_id);
//     const uri = entry.uri.slice(p.node.data.memory);
//     if (out_buf == null or buf_len < uri.len) { out_len.* = uri.len; return .out_of_space; }
//     @memcpy(out_buf.?[0..uri.len], uri);
//     out_len.* = uri.len;
//     return .success;
// }
/// # Safety
/// `ref_` must point to a valid `CGridRef`; `out_buf`, if non-null, must point to
/// `buf_len` writable bytes; `out_len` must be a valid writable pointer.
pub unsafe fn grid_ref_hyperlink_uri(
    ref_: *const CGridRef,
    out_buf: *mut u8,
    buf_len: usize,
    out_len: *mut usize,
) -> Result {
    // const p = ref.toPin() orelse return .invalid_value;
    let p = match unsafe { (*ref_).to_pin() } {
        Some(p) => p,
        None => return Result::InvalidValue,
    };
    // const rac = p.node.data.getRowAndCell(p.x, p.y);
    // const cell = rac.cell;
    // SAFETY: `p.node` is a live page node (the pin came from a valid ref).
    let rac = unsafe {
        (*rustty_vt::pagelist::Node::page(p.node)).get_row_and_cell(p.x as usize, p.y as usize)
    };
    let cell = rac.cell;

    // if (!cell.hyperlink) { out_len.* = 0; return .success; }
    if !unsafe { (*cell).hyperlink() } {
        unsafe { *out_len = 0 };
        return Result::Success;
    }

    // const link_id = p.node.data.lookupHyperlink(cell) orelse { out_len.* = 0; return .success; };
    let link_id = match unsafe { (*rustty_vt::pagelist::Node::page(p.node)).lookup_hyperlink(cell) }
    {
        Some(id) => id,
        None => {
            unsafe { *out_len = 0 };
            return Result::Success;
        }
    };
    // const entry = p.node.data.hyperlink_set.get(p.node.data.memory, link_id);
    // const uri = entry.uri.slice(p.node.data.memory);
    // SAFETY: `link_id` was just looked up against this page's hyperlink set; the
    // page memory base outlives the borrowed `uri` slice (read immediately).
    let uri: &[u8] = unsafe {
        let base = (*rustty_vt::pagelist::Node::page(p.node)).memory_ptr();
        let entry = (*rustty_vt::pagelist::Node::page(p.node))
            .hyperlink_set
            .get(base, link_id);
        (*entry).uri.slice(base)
    };

    // if (out_buf == null or buf_len < uri.len) { out_len.* = uri.len; return .out_of_space; }
    if out_buf.is_null() || buf_len < uri.len() {
        unsafe { *out_len = uri.len() };
        return Result::OutOfSpace;
    }

    // @memcpy(out_buf.?[0..uri.len], uri);
    // SAFETY: `out_buf` is non-null and `buf_len >= uri.len()`; the source `uri`
    // is a valid slice over the page memory.
    unsafe { core::ptr::copy_nonoverlapping(uri.as_ptr(), out_buf, uri.len()) };
    // out_len.* = uri.len;
    unsafe { *out_len = uri.len() };
    // return .success;
    Result::Success
}

// pub fn grid_ref_style(ref: *const CGridRef, out: ?*style_c.Style) ... {
//     const p = ref.toPin() orelse return .invalid_value;
//     if (out) |o| {
//         const cell = p.rowAndCell().cell;
//         if (cell.style_id == stylepkg.default_id) {
//             o.* = .fromStyle(.{});
//         } else {
//             o.* = .fromStyle(p.node.data.styles.get(p.node.data.memory, cell.style_id).*);
//         }
//     }
//     return .success;
// }
/// # Safety
/// `ref_` must point to a valid `CGridRef`; `out`, if non-null, must be a valid
/// writable pointer to a `Style`.
pub unsafe fn grid_ref_style(ref_: *const CGridRef, out: *mut Style) -> Result {
    // const p = ref.toPin() orelse return .invalid_value;
    let p = match unsafe { (*ref_).to_pin() } {
        Some(p) => p,
        None => return Result::InvalidValue,
    };
    // if (out) |o| { ... }
    if !out.is_null() {
        // const cell = p.rowAndCell().cell;
        let cell = p.row_and_cell().cell;
        // if (cell.style_id == stylepkg.default_id) o.* = .fromStyle(.{});
        if unsafe { (*cell).style_id() } == rustty_vt::style::DEFAULT_ID {
            // SAFETY: `out` is a valid writable `Style`.
            unsafe { *out = Style::from_style(rustty_vt::style::Style::default()) };
        } else {
            // else o.* = .fromStyle(p.node.data.styles.get(p.node.data.memory, cell.style_id).*);
            // SAFETY: `cell.style_id` is a live style id for this page; the page
            // memory base outlives the read; `out` is a valid writable `Style`.
            let style = unsafe {
                let base = (*rustty_vt::pagelist::Node::page(p.node)).memory_ptr();
                *(*rustty_vt::pagelist::Node::page(p.node))
                    .styles
                    .get(base, (*cell).style_id())
            };
            unsafe { *out = Style::from_style(style) };
        }
    }
    // return .success;
    Result::Success
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::point::PointC;
    use crate::terminal;
    use crate::test_util::with_test_allocator;
    use rustty_vt::point::{Coordinate, Point};
    use std::mem::MaybeUninit;

    // test "grid_ref_cell null node"
    #[test]
    fn grid_ref_cell_null_node() {
        // const ref = CGridRef{};
        let ref_ = CGridRef::default();
        // var out: cell_c.CCell = undefined;
        let mut out: MaybeUninit<CCell> = MaybeUninit::uninit();
        // try testing.expectEqual(Result.invalid_value, grid_ref_cell(&ref, &out));
        assert_eq!(Result::InvalidValue, unsafe {
            grid_ref_cell(&ref_, out.as_mut_ptr())
        });
    }

    // test "grid_ref_row null node"
    #[test]
    fn grid_ref_row_null_node() {
        // const ref = CGridRef{};
        let ref_ = CGridRef::default();
        // var out: row_c.CRow = undefined;
        let mut out: MaybeUninit<CRow> = MaybeUninit::uninit();
        // try testing.expectEqual(Result.invalid_value, grid_ref_row(&ref, &out));
        assert_eq!(Result::InvalidValue, unsafe {
            grid_ref_row(&ref_, out.as_mut_ptr())
        });
    }

    // test "grid_ref_cell null out"
    #[test]
    fn grid_ref_cell_null_out() {
        // const ref = CGridRef{};
        let ref_ = CGridRef::default();
        // try testing.expectEqual(Result.invalid_value, grid_ref_cell(&ref, null));
        assert_eq!(Result::InvalidValue, unsafe {
            grid_ref_cell(&ref_, std::ptr::null_mut())
        });
    }

    // test "grid_ref_row null out"
    #[test]
    fn grid_ref_row_null_out() {
        // const ref = CGridRef{};
        let ref_ = CGridRef::default();
        // try testing.expectEqual(Result.invalid_value, grid_ref_row(&ref, null));
        assert_eq!(Result::InvalidValue, unsafe {
            grid_ref_row(&ref_, std::ptr::null_mut())
        });
    }

    // test "grid_ref_graphemes null node"
    #[test]
    fn grid_ref_graphemes_null_node() {
        // const ref = CGridRef{};
        let ref_ = CGridRef::default();
        // var len: usize = undefined;
        let mut len: MaybeUninit<usize> = MaybeUninit::uninit();
        // try testing.expectEqual(Result.invalid_value, grid_ref_graphemes(&ref, null, 0, &len));
        assert_eq!(Result::InvalidValue, unsafe {
            grid_ref_graphemes(&ref_, std::ptr::null_mut(), 0, len.as_mut_ptr())
        });
    }

    // test "grid_ref_graphemes null buf returns out_of_space"
    #[test]
    fn grid_ref_graphemes_null_buf_returns_out_of_space() {
        // const ref = CGridRef{};
        let ref_ = CGridRef::default();
        // var len: usize = undefined;
        let mut len: MaybeUninit<usize> = MaybeUninit::uninit();
        // With null node this returns invalid_value before checking the buffer,
        // so we can only test null node here. Full buffer tests require a real page.
        // try testing.expectEqual(Result.invalid_value, grid_ref_graphemes(&ref, null, 0, &len));
        assert_eq!(Result::InvalidValue, unsafe {
            grid_ref_graphemes(&ref_, std::ptr::null_mut(), 0, len.as_mut_ptr())
        });
    }

    // test "grid_ref_style null node"
    #[test]
    fn grid_ref_style_null_node() {
        // const ref = CGridRef{};
        let ref_ = CGridRef::default();
        // var out: style_c.Style = undefined;
        let mut out: MaybeUninit<Style> = MaybeUninit::uninit();
        // try testing.expectEqual(Result.invalid_value, grid_ref_style(&ref, &out));
        assert_eq!(Result::InvalidValue, unsafe {
            grid_ref_style(&ref_, out.as_mut_ptr())
        });
    }

    // test "grid_ref_style null out"
    #[test]
    fn grid_ref_style_null_out() {
        // const ref = CGridRef{};
        let ref_ = CGridRef::default();
        // try testing.expectEqual(Result.invalid_value, grid_ref_style(&ref, null));
        assert_eq!(Result::InvalidValue, unsafe {
            grid_ref_style(&ref_, std::ptr::null_mut())
        });
    }

    // test "grid_ref_hyperlink_uri null node"
    #[test]
    fn grid_ref_hyperlink_uri_null_node() {
        // const ref = CGridRef{};
        let ref_ = CGridRef::default();
        // var len: usize = undefined;
        let mut len: MaybeUninit<usize> = MaybeUninit::uninit();
        // try testing.expectEqual(Result.invalid_value, grid_ref_hyperlink_uri(&ref, null, 0, &len));
        assert_eq!(Result::InvalidValue, unsafe {
            grid_ref_hyperlink_uri(&ref_, std::ptr::null_mut(), 0, len.as_mut_ptr())
        });
    }

    // test "grid_ref_hyperlink_uri no hyperlink"
    #[test]
    fn grid_ref_hyperlink_uri_no_hyperlink() {
        with_test_allocator(|alloc| {
            // var terminal: terminal_c.Terminal = null;
            let mut term: terminal::Terminal = std::ptr::null_mut();
            // try testing.expectEqual(Result.success, terminal_c.new(..., .{ .cols=80, .rows=24, .max_scrollback=10_000 }));
            assert_eq!(Result::Success, unsafe {
                terminal::new(
                    alloc,
                    &mut term,
                    terminal::Options {
                        cols: 80,
                        rows: 24,
                        max_scrollback: 10_000,
                    },
                )
            });
            // defer terminal_c.free(terminal);

            // terminal_c.vt_write(terminal, "hello", 5);
            let input = b"hello";
            unsafe { terminal::vt_write(term, input.as_ptr(), input.len()) };

            // var ref: CGridRef = undefined;
            let mut ref_: MaybeUninit<CGridRef> = MaybeUninit::uninit();
            // try testing.expectEqual(Result.success, terminal_c.grid_ref(terminal, point.Point.cval(.{ .active = .{ .x = 0, .y = 0 } }), &ref));
            assert_eq!(Result::Success, unsafe {
                terminal::grid_ref(
                    term,
                    PointC::cval(Point::Active(Coordinate { x: 0, y: 0 })),
                    ref_.as_mut_ptr(),
                )
            });
            let ref_ = unsafe { ref_.assume_init() };

            // var len: usize = undefined;
            let mut len: MaybeUninit<usize> = MaybeUninit::uninit();
            // try testing.expectEqual(Result.success, grid_ref_hyperlink_uri(&ref, null, 0, &len));
            assert_eq!(Result::Success, unsafe {
                grid_ref_hyperlink_uri(&ref_, std::ptr::null_mut(), 0, len.as_mut_ptr())
            });
            // try testing.expectEqual(@as(usize, 0), len);
            assert_eq!(0usize, unsafe { len.assume_init() });

            unsafe { terminal::free(term) };
        });
    }

    // test "grid_ref_hyperlink_uri with hyperlink"
    #[test]
    fn grid_ref_hyperlink_uri_with_hyperlink() {
        with_test_allocator(|alloc| {
            // var terminal: terminal_c.Terminal = null;
            let mut term: terminal::Terminal = std::ptr::null_mut();
            // try testing.expectEqual(Result.success, terminal_c.new(..., .{ .cols=80, .rows=24, .max_scrollback=10_000 }));
            assert_eq!(Result::Success, unsafe {
                terminal::new(
                    alloc,
                    &mut term,
                    terminal::Options {
                        cols: 80,
                        rows: 24,
                        max_scrollback: 10_000,
                    },
                )
            });
            // defer terminal_c.free(terminal);

            // Write OSC 8 hyperlink: \e]8;;uri\e\\text\e]8;;\e\\
            // const seq = "\x1b]8;;https://example.com\x1b\\link\x1b]8;;\x1b\\";
            let seq = b"\x1b]8;;https://example.com\x1b\\link\x1b]8;;\x1b\\";
            // terminal_c.vt_write(terminal, seq, seq.len);
            unsafe { terminal::vt_write(term, seq.as_ptr(), seq.len()) };

            // var ref: CGridRef = undefined;
            let mut ref_: MaybeUninit<CGridRef> = MaybeUninit::uninit();
            // try testing.expectEqual(Result.success, terminal_c.grid_ref(terminal, point.Point.cval(.{ .active = .{ .x = 0, .y = 0 } }), &ref));
            assert_eq!(Result::Success, unsafe {
                terminal::grid_ref(
                    term,
                    PointC::cval(Point::Active(Coordinate { x: 0, y: 0 })),
                    ref_.as_mut_ptr(),
                )
            });
            let ref_ = unsafe { ref_.assume_init() };

            // First query length with null buf
            // var len: usize = undefined;
            let mut len: usize = 0;
            // try testing.expectEqual(Result.out_of_space, grid_ref_hyperlink_uri(&ref, null, 0, &len));
            assert_eq!(Result::OutOfSpace, unsafe {
                grid_ref_hyperlink_uri(&ref_, std::ptr::null_mut(), 0, &mut len)
            });
            // try testing.expectEqual(@as(usize, 19), len); // "https://example.com"
            assert_eq!(19usize, len);

            // Now read with a properly sized buffer
            // var buf: [256]u8 = undefined;
            let mut buf = [0u8; 256];
            // try testing.expectEqual(Result.success, grid_ref_hyperlink_uri(&ref, &buf, buf.len, &len));
            assert_eq!(Result::Success, unsafe {
                grid_ref_hyperlink_uri(&ref_, buf.as_mut_ptr(), buf.len(), &mut len)
            });
            // try testing.expectEqualStrings("https://example.com", buf[0..len]);
            assert_eq!(b"https://example.com", &buf[0..len]);

            unsafe { terminal::free(term) };
        });
    }
}
