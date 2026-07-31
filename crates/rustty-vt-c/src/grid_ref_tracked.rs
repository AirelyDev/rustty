//! rustty-vt-c grid_ref_tracked
//!
//! PORTS: ghostty src/terminal/c/grid_ref_tracked.zig
//!
//! PORT STATUS: FILLED (mutual-atom Stage B v4). Types are byte-exact, every C
//! API function body and the `page_list` helper are ported, and all `test {}`
//! blocks are present (assertion-for-assertion). Part of the irreducible mutual
//! cluster (terminal<->grid_ref<->grid_ref_tracked<->selection).

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
    // fn pageList(ref: *const TrackedGridRef) ?*PageList {
    //     const wrapper = ref.terminal orelse return null;
    //     const t = wrapper.terminal;
    //     if (t.screens.generation(ref.screen_key) != ref.screen_generation) return null;
    //     const screen = t.screens.get(ref.screen_key) orelse return null;
    //     return &screen.pages;
    // }
    //
    // PORT NOTE (SOUNDNESS): the reference reaches `wrapper.terminal.screens`
    // through pointers. rustty mirrors that with RAW pointers and never forms a
    // `&mut`/`&` over the whole `ZigTerminal`/`Screen` (the stream handler aliases
    // the terminal via a stored raw ptr; a Unique/Shared retag would conflict —
    // the rust-mut-args-strongly-protected rule). All reads are scalar through the
    // raw ptr.
    pub(crate) fn page_list(&self) -> Option<*mut PageList> {
        // const wrapper = ref.terminal orelse return null;
        if self.terminal.is_null() {
            return None;
        }
        let wrapper = self.terminal;
        // const t = wrapper.terminal;
        // SAFETY: `wrapper` is a live terminal handle (checked non-null); `terminal`
        // is its owned heap `ZigTerminal`.
        let t = unsafe { (*wrapper).terminal };
        // if (t.screens.generation(ref.screen_key) != ref.screen_generation) return null;
        if unsafe { (*t).screens.generation(self.screen_key) } != self.screen_generation {
            return None;
        }
        // const screen = t.screens.get(ref.screen_key) orelse return null;
        let screen = unsafe { (*t).screens.get(self.screen_key) }?;
        // return &screen.pages;
        Some(unsafe { core::ptr::addr_of_mut!((*screen).pages) })
    }
}

// pub fn tracked_grid_ref_free(ref_: CTrackedGridRef) callconv(lib.calling_conv) void {
//     const ref = ref_ orelse return;
//     if (ref.terminal) |wrapper| { _ = wrapper.tracked_grid_refs.swapRemove(ref); }
//     if (ref.pageList()) |list| list.untrackPin(ref.pin);
//     ref.alloc.destroy(ref);
// }
/// # Safety
/// `ref_`, if non-null, must be a pointer returned by `grid_ref_track` that has
/// not yet been freed.
pub unsafe fn tracked_grid_ref_free(ref_: CTrackedGridRef) {
    // const ref = ref_ orelse return;
    if ref_.is_null() {
        return;
    }
    // SAFETY: `ref_` is a live `grid_ref_track`-allocated ref (caller contract).
    // if (ref.terminal) |wrapper| { _ = wrapper.tracked_grid_refs.swapRemove(ref); }
    let wrapper = unsafe { (*ref_).terminal };
    if !wrapper.is_null() {
        // SAFETY: `tracked_grid_refs` is a sibling field of the live wrapper.
        unsafe { (*wrapper).tracked_grid_refs.remove(&ref_) };
    }
    // if (ref.pageList()) |list| list.untrackPin(ref.pin);
    if let Some(list) = unsafe { (*ref_).page_list() } {
        let pin = unsafe { (*ref_).pin };
        unsafe { (*list).untrack_pin(pin) };
    }
    // ref.alloc.destroy(ref);
    // PORT NOTE: `alloc.destroy(ref)` reclaims the heap `TrackedGridRef`; rustty
    // `Box::from_raw` + drop is the analog (the `alloc` field mirrors the
    // reference's owning allocator).
    drop(unsafe { Box::from_raw(ref_) });
}

// pub fn tracked_grid_ref_has_value(ref_: CTrackedGridRef) callconv(lib.calling_conv) bool {
//     const ref = ref_ orelse return false;
//     _ = ref.pageList() orelse return false;
//     return !ref.pin.garbage;
// }
/// # Safety
/// `ref_`, if non-null, must be a live `TrackedGridRef` pointer.
pub unsafe fn tracked_grid_ref_has_value(ref_: CTrackedGridRef) -> bool {
    // const ref = ref_ orelse return false;
    if ref_.is_null() {
        return false;
    }
    // _ = ref.pageList() orelse return false;
    // SAFETY: `ref_` is a live ref (caller contract).
    if unsafe { (*ref_).page_list() }.is_none() {
        return false;
    }
    // return !ref.pin.garbage;
    let pin = unsafe { (*ref_).pin };
    !unsafe { (*pin).garbage }
}

// pub fn tracked_grid_ref_snapshot(ref_: CTrackedGridRef, out_ref: ?*grid_ref_c.CGridRef) ... {
//     const ref = ref_ orelse return .invalid_value;
//     _ = ref.pageList() orelse return .no_value;
//     if (ref.pin.garbage) return .no_value;
//     if (out_ref) |out| out.* = grid_ref_c.CGridRef.fromPin(ref.pin.*);
//     return .success;
// }
/// # Safety
/// `ref_`, if non-null, must be a live `TrackedGridRef` pointer; `out_ref`, if
/// non-null, must be a valid writable pointer to a `CGridRef`.
pub unsafe fn tracked_grid_ref_snapshot(ref_: CTrackedGridRef, out_ref: *mut CGridRef) -> Result {
    // const ref = ref_ orelse return .invalid_value;
    if ref_.is_null() {
        return Result::InvalidValue;
    }
    // _ = ref.pageList() orelse return .no_value;
    // SAFETY: `ref_` is a live ref (caller contract).
    if unsafe { (*ref_).page_list() }.is_none() {
        return Result::NoValue;
    }
    let pin = unsafe { (*ref_).pin };
    // if (ref.pin.garbage) return .no_value;
    if unsafe { (*pin).garbage } {
        return Result::NoValue;
    }
    // if (out_ref) |out| out.* = grid_ref_c.CGridRef.fromPin(ref.pin.*);
    if !out_ref.is_null() {
        let p = unsafe { *pin };
        // SAFETY: `out_ref` is a valid writable `CGridRef`.
        unsafe { *out_ref = CGridRef::from_pin(p) };
    }
    // return .success;
    Result::Success
}

// pub fn tracked_grid_ref_point(ref_: CTrackedGridRef, tag: point.Tag, out: ?*point.Coordinate) ... {
//     const ref = ref_ orelse return .invalid_value;
//     const list = ref.pageList() orelse return .no_value;
//     if (ref.pin.garbage) return .no_value;
//     const pt = list.pointFromPin(tag, ref.pin.*) orelse return .no_value;
//     if (out) |o| o.* = pt.coord();
//     return .success;
// }
/// # Safety
/// `ref_`, if non-null, must be a live `TrackedGridRef` pointer; `out`, if
/// non-null, must be a valid writable pointer to a `Coordinate`.
pub unsafe fn tracked_grid_ref_point(
    ref_: CTrackedGridRef,
    tag: Tag,
    out: *mut Coordinate,
) -> Result {
    // const ref = ref_ orelse return .invalid_value;
    if ref_.is_null() {
        return Result::InvalidValue;
    }
    // const list = ref.pageList() orelse return .no_value;
    // SAFETY: `ref_` is a live ref (caller contract).
    let list = match unsafe { (*ref_).page_list() } {
        Some(l) => l,
        None => return Result::NoValue,
    };
    let pin = unsafe { (*ref_).pin };
    // if (ref.pin.garbage) return .no_value;
    if unsafe { (*pin).garbage } {
        return Result::NoValue;
    }
    // const pt = list.pointFromPin(tag, ref.pin.*) orelse return .no_value;
    let p = unsafe { *pin };
    let pt = match unsafe { (*list).point_from_pin(tag, p) } {
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

// pub fn tracked_grid_ref_set(ref_: CTrackedGridRef, terminal_: terminal_c.Terminal, pt: point.Point.C) ... {
//     const ref = ref_ orelse return .invalid_value;
//     const wrapper = terminal_ orelse return .invalid_value;
//     if (ref.terminal != terminal_) return .invalid_value;
//     const t = wrapper.terminal;
//     const list = &t.screens.active.pages;
//     const p = list.pin(point.Point.fromC(pt)) orelse return .invalid_value;
//     const tracked_pin = list.trackPin(p) catch return .out_of_memory;
//     if (ref.pageList()) |old_list| old_list.untrackPin(ref.pin);
//     ref.screen_key = t.screens.active_key;
//     ref.screen_generation = t.screens.generation(ref.screen_key);
//     ref.pin = tracked_pin;
//     return .success;
// }
/// # Safety
/// `ref_`, if non-null, must be a live `TrackedGridRef` pointer; `terminal_`, if
/// non-null, must be a live terminal handle.
pub unsafe fn tracked_grid_ref_set(
    ref_: CTrackedGridRef,
    terminal_: Terminal,
    pt: PointC,
) -> Result {
    // const ref = ref_ orelse return .invalid_value;
    if ref_.is_null() {
        return Result::InvalidValue;
    }
    // const wrapper = terminal_ orelse return .invalid_value;
    if terminal_.is_null() {
        return Result::InvalidValue;
    }
    let wrapper = terminal_;
    // if (ref.terminal != terminal_) return .invalid_value;
    // SAFETY: `ref_` is a live ref (caller contract).
    if unsafe { (*ref_).terminal } != terminal_ {
        return Result::InvalidValue;
    }

    // const t = wrapper.terminal;
    // SAFETY: `wrapper` is a live terminal handle.
    let t = unsafe { (*wrapper).terminal };
    // const list = &t.screens.active.pages;
    let active = unsafe { (*t).screens.active };
    let list: *mut PageList = unsafe { core::ptr::addr_of_mut!((*active).pages) };
    // const p = list.pin(point.Point.fromC(pt)) orelse return .invalid_value;
    // D11 (P3-W6 T2): `pt.tag` is the raw C integer; `from_c` narrows it and is
    // `None` for an undeclared tag, which takes the same `.invalid_value` exit.
    let zig_pt = match pt.from_c() {
        Some(p) => p,
        None => return Result::InvalidValue,
    };
    let p = match unsafe { (*list).pin(zig_pt) } {
        Some(p) => p,
        None => return Result::InvalidValue,
    };
    // const tracked_pin = list.trackPin(p) catch return .out_of_memory;
    let tracked_pin = match unsafe { (*list).track_pin(p) } {
        Ok(tp) => tp,
        Err(_) => return Result::OutOfMemory,
    };

    // if (ref.pageList()) |old_list| old_list.untrackPin(ref.pin);
    if let Some(old_list) = unsafe { (*ref_).page_list() } {
        let old_pin = unsafe { (*ref_).pin };
        unsafe { (*old_list).untrack_pin(old_pin) };
    }
    // ref.screen_key = t.screens.active_key;
    let active_key = unsafe { (*t).screens.active_key };
    // ref.screen_generation = t.screens.generation(ref.screen_key);
    let generation = unsafe { (*t).screens.generation(active_key) };
    // SAFETY: writes to the live ref's fields through the raw ptr.
    unsafe {
        (*ref_).screen_key = active_key;
        (*ref_).screen_generation = generation;
        // ref.pin = tracked_pin;
        (*ref_).pin = tracked_pin;
    }
    // return .success;
    Result::Success
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid_ref;
    use crate::point::PointC;
    use crate::terminal;
    use crate::test_util::with_test_allocator;
    use rustty_vt::point::{Coordinate, Point};
    use std::mem::MaybeUninit;

    // test "tracked_grid_ref snapshots after terminal scroll"
    #[test]
    fn tracked_grid_ref_snapshots_after_terminal_scroll() {
        with_test_allocator(|alloc| {
            // var terminal: terminal_c.Terminal = null;
            let mut term: terminal::Terminal = std::ptr::null_mut();
            // try testing.expectEqual(Result.success, terminal_c.new(..., 5, 2));
            assert_eq!(Result::Success, unsafe {
                terminal::new(alloc, &mut term, 5, 2)
            });
            // defer terminal_c.free(terminal);

            // terminal_c.vt_write(terminal, "A", 1);
            unsafe { terminal::vt_write(term, b"A".as_ptr(), 1) };

            // var ref: CTrackedGridRef = null;
            let mut ref_: CTrackedGridRef = std::ptr::null_mut();
            // try testing.expectEqual(Result.success, terminal_c.grid_ref_track(terminal, point.Point.cval(.{ .active = .{ .x = 0, .y = 0 } }), &ref));
            assert_eq!(Result::Success, unsafe {
                terminal::grid_ref_track(
                    term,
                    PointC::cval(Point::Active(Coordinate { x: 0, y: 0 })),
                    &mut ref_,
                )
            });
            // defer tracked_grid_ref_free(ref);

            // terminal_c.vt_write(terminal, "\r\nB\r\nC", 6);
            unsafe { terminal::vt_write(term, b"\r\nB\r\nC".as_ptr(), 6) };
            // try testing.expect(tracked_grid_ref_has_value(ref));
            assert!(unsafe { tracked_grid_ref_has_value(ref_) });

            // var snapshot: grid_ref_c.CGridRef = undefined;
            let mut snapshot: MaybeUninit<CGridRef> = MaybeUninit::uninit();
            // try testing.expectEqual(Result.success, tracked_grid_ref_snapshot(ref, &snapshot));
            assert_eq!(Result::Success, unsafe {
                tracked_grid_ref_snapshot(ref_, snapshot.as_mut_ptr())
            });
            let snapshot = unsafe { snapshot.assume_init() };

            // var buf: [1]u32 = undefined;
            let mut buf = [0u32; 1];
            // var len: usize = undefined;
            let mut len: usize = 0;
            // try testing.expectEqual(Result.success, grid_ref_c.grid_ref_graphemes(&snapshot, &buf, buf.len, &len));
            assert_eq!(Result::Success, unsafe {
                grid_ref::grid_ref_graphemes(&snapshot, buf.as_mut_ptr(), buf.len(), &mut len)
            });
            // try testing.expectEqual(@as(usize, 1), len);
            assert_eq!(1usize, len);
            // try testing.expectEqual(@as(u32, 'A'), buf[0]);
            assert_eq!(b'A' as u32, buf[0]);

            unsafe { tracked_grid_ref_free(ref_) };
            unsafe { terminal::free(term) };
        });
    }

    // test "tracked_grid_ref reports no value after reset"
    #[test]
    fn tracked_grid_ref_reports_no_value_after_reset() {
        with_test_allocator(|alloc| {
            let mut term: terminal::Terminal = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                terminal::new(alloc, &mut term, 5, 2)
            });

            // terminal_c.vt_write(terminal, "A", 1);
            unsafe { terminal::vt_write(term, b"A".as_ptr(), 1) };

            let mut ref_: CTrackedGridRef = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                terminal::grid_ref_track(
                    term,
                    PointC::cval(Point::Active(Coordinate { x: 0, y: 0 })),
                    &mut ref_,
                )
            });

            // terminal_c.reset(terminal);
            unsafe { terminal::reset(term) };
            // try testing.expect(!tracked_grid_ref_has_value(ref));
            assert!(!unsafe { tracked_grid_ref_has_value(ref_) });

            // var snapshot: grid_ref_c.CGridRef = undefined;
            let mut snapshot: MaybeUninit<CGridRef> = MaybeUninit::uninit();
            // try testing.expectEqual(Result.no_value, tracked_grid_ref_snapshot(ref, &snapshot));
            assert_eq!(Result::NoValue, unsafe {
                tracked_grid_ref_snapshot(ref_, snapshot.as_mut_ptr())
            });

            unsafe { tracked_grid_ref_free(ref_) };
            unsafe { terminal::free(term) };
        });
    }

    // test "tracked_grid_ref reports no value after alternate screen reset"
    #[test]
    fn tracked_grid_ref_reports_no_value_after_alternate_screen_reset() {
        with_test_allocator(|alloc| {
            let mut term: terminal::Terminal = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                terminal::new(alloc, &mut term, 5, 2)
            });

            // terminal_c.vt_write(terminal, "\x1b[?1049hA", 9);
            unsafe { terminal::vt_write(term, b"\x1b[?1049hA".as_ptr(), 9) };

            let mut ref_: CTrackedGridRef = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                terminal::grid_ref_track(
                    term,
                    PointC::cval(Point::Active(Coordinate { x: 0, y: 0 })),
                    &mut ref_,
                )
            });

            // terminal_c.vt_write(terminal, "\x1bc", 2);
            unsafe { terminal::vt_write(term, b"\x1bc".as_ptr(), 2) };
            // try testing.expect(!tracked_grid_ref_has_value(ref));
            assert!(!unsafe { tracked_grid_ref_has_value(ref_) });

            // var snapshot: grid_ref_c.CGridRef = undefined;
            let mut snapshot: MaybeUninit<CGridRef> = MaybeUninit::uninit();
            // try testing.expectEqual(Result.no_value, tracked_grid_ref_snapshot(ref, &snapshot));
            assert_eq!(Result::NoValue, unsafe {
                tracked_grid_ref_snapshot(ref_, snapshot.as_mut_ptr())
            });

            // var coord: point.Coordinate = undefined;
            let mut coord: MaybeUninit<Coordinate> = MaybeUninit::uninit();
            // try testing.expectEqual(Result.no_value, tracked_grid_ref_point(ref, .active, &coord));
            assert_eq!(Result::NoValue, unsafe {
                tracked_grid_ref_point(ref_, Tag::Active, coord.as_mut_ptr())
            });

            // terminal_c.vt_write(terminal, "\x1b[?1049h", 8);
            unsafe { terminal::vt_write(term, b"\x1b[?1049h".as_ptr(), 8) };
            // try testing.expect(!tracked_grid_ref_has_value(ref));
            assert!(!unsafe { tracked_grid_ref_has_value(ref_) });

            // try testing.expectEqual(Result.success, tracked_grid_ref_set(ref, terminal, point.Point.cval(.{ .active = .{ .x = 0, .y = 0 } })));
            assert_eq!(Result::Success, unsafe {
                tracked_grid_ref_set(
                    ref_,
                    term,
                    PointC::cval(Point::Active(Coordinate { x: 0, y: 0 })),
                )
            });
            // try testing.expect(tracked_grid_ref_has_value(ref));
            assert!(unsafe { tracked_grid_ref_has_value(ref_) });
            // try testing.expectEqual(Result.success, tracked_grid_ref_snapshot(ref, &snapshot));
            assert_eq!(Result::Success, unsafe {
                tracked_grid_ref_snapshot(ref_, snapshot.as_mut_ptr())
            });

            unsafe { tracked_grid_ref_free(ref_) };
            unsafe { terminal::free(term) };
        });
    }

    // test "tracked_grid_ref reports no value after terminal free"
    #[test]
    fn tracked_grid_ref_reports_no_value_after_terminal_free() {
        with_test_allocator(|alloc| {
            let mut term: terminal::Terminal = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                terminal::new(alloc, &mut term, 5, 2)
            });

            // terminal_c.vt_write(terminal, "A", 1);
            unsafe { terminal::vt_write(term, b"A".as_ptr(), 1) };

            let mut ref_: CTrackedGridRef = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                terminal::grid_ref_track(
                    term,
                    PointC::cval(Point::Active(Coordinate { x: 0, y: 0 })),
                    &mut ref_,
                )
            });

            // terminal_c.free(terminal);
            unsafe { terminal::free(term) };
            // try testing.expect(!tracked_grid_ref_has_value(ref));
            assert!(!unsafe { tracked_grid_ref_has_value(ref_) });

            // var snapshot: grid_ref_c.CGridRef = undefined;
            let mut snapshot: MaybeUninit<CGridRef> = MaybeUninit::uninit();
            // try testing.expectEqual(Result.no_value, tracked_grid_ref_snapshot(ref, &snapshot));
            assert_eq!(Result::NoValue, unsafe {
                tracked_grid_ref_snapshot(ref_, snapshot.as_mut_ptr())
            });

            // var coord: point.Coordinate = undefined;
            let mut coord: MaybeUninit<Coordinate> = MaybeUninit::uninit();
            // try testing.expectEqual(Result.no_value, tracked_grid_ref_point(ref, .active, &coord));
            assert_eq!(Result::NoValue, unsafe {
                tracked_grid_ref_point(ref_, Tag::Active, coord.as_mut_ptr())
            });

            // try testing.expectEqual(Result.invalid_value, tracked_grid_ref_set(ref, terminal, point.Point.cval(.{ .active = .{ .x = 0, .y = 0 } })));
            assert_eq!(Result::InvalidValue, unsafe {
                tracked_grid_ref_set(
                    ref_,
                    term,
                    PointC::cval(Point::Active(Coordinate { x: 0, y: 0 })),
                )
            });

            // tracked_grid_ref_free(ref);
            unsafe { tracked_grid_ref_free(ref_) };
        });
    }
}
