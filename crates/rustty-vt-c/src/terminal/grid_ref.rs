//! PORTS: ghostty src/terminal/c/terminal.zig — the `grid_ref`,
//! `grid_ref_track`, and `point_from_grid_ref` cluster.

use super::*;

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
    // D11 (P3-W6 T2): `pt.tag` is the raw C integer; `from_c` narrows it.
    let zig_pt = match pt.from_c() {
        Some(p) => p,
        None => return Result::InvalidValue,
    };
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
    // D11 (P3-W6 T2): `pt.tag` is the raw C integer; `from_c` narrows it.
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

    // const alloc = t.gpa();
    // PORT NOTE (parity S179-F3, S7-3): the reference's `t.gpa()` is the
    // caller's allocator (`t.init(alloc, ..)`), and since S7-3 so is the
    // port's (the tracked pin above came from the terminal's pin pool on it).
    // The ref takes the wrapper's copy of that C allocator
    // (`TerminalWrapper.c_alloc`), read through the raw wrapper pointer (no
    // reference to `*t` is formed; Miri SB+TB), and keeps its own copy, so it
    // can be freed after the terminal.
    let alloc: Option<CAllocator> = unsafe { (*terminal_).c_alloc };
    let active_key = unsafe { (*t).screens.active_key };
    let generation = unsafe { (*t).screens.generation(active_key) };
    // const ref = alloc.create(grid_ref_tracked_c.TrackedGridRef) catch { list.untrackPin(tracked_pin); return .out_of_memory; };
    // ref.* = .{ .alloc, .terminal = wrapper, .screen_key, .screen_generation, .pin = tracked_pin };
    // SAFETY: `alloc`, if set, is the caller's allocator (C contract: valid
    // for as long as objects created with it live).
    let ref_ptr: *mut TrackedGridRef = unsafe {
        crate::cwrap::create(
            alloc.as_ref(),
            TrackedGridRef {
                alloc,
                terminal: terminal_,
                screen_key: active_key,
                screen_generation: generation,
                pin: tracked_pin,
            },
        )
    };
    if ref_ptr.is_null() {
        // SAFETY: `list` is the live active PageList that tracked the pin.
        unsafe { (*list).untrack_pin(tracked_pin) };
        return Result::OutOfMemory;
    }

    // Store the tracked ref in the terminal so that when we free
    // the terminal the tracked ref can be detached safely.
    // wrapper.tracked_grid_refs.putNoClobber(alloc, ref, {}) catch {
    //     list.untrackPin(tracked_pin);
    //     alloc.destroy(ref);
    //     return .out_of_memory;
    // };
    // PORT NOTE (parity S179-F3, parity tail S7-4): `try_put_no_clobber` is
    // the fallible `putNoClobber` (the key is a fresh heap pointer, so it is
    // absent), so the reference's `catch` arm is reachable, as in
    // `search::new` (S4-F3). The registry grows through the terminal's
    // allocator, bound in `wrap`.
    #[cfg(test)]
    let reserved = track_tw_check().and_then(|()| unsafe {
        (*terminal_)
            .tracked_grid_refs
            .try_put_no_clobber(ref_ptr, ())
            .map_err(|_| TrackRegistryFull)
    });
    #[cfg(not(test))]
    let reserved = unsafe {
        (*terminal_)
            .tracked_grid_refs
            .try_put_no_clobber(ref_ptr, ())
    };
    if reserved.is_err() {
        // SAFETY: as above; `ref_ptr` was just created with `alloc` and has
        // not been handed out.
        unsafe { (*list).untrack_pin(tracked_pin) };
        unsafe { crate::cwrap::destroy(alloc.as_ref(), ref_ptr) };
        return Result::OutOfMemory;
    }

    // out.* = ref;
    unsafe { *out_ref = ref_ptr };
    // return .success;
    Result::Success
}

// PORT NOTE: tripwire fail-point for the tracked-ref registry's growth in
// `grid_ref_track` (`wrapper.tracked_grid_refs.putNoClobber(alloc, ref, {})`),
// standing in for `std.testing.FailingAllocator`, as `search.rs`'s
// `REGISTER_TW` did for S4-F3 until parity tail S7-6 drove that arm with a
// `FailingAllocator` under the terminal (parity S179-F3).
#[cfg(test)]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) enum TrackFp {
    Reserve,
}

/// The tripwire's error value (Zig `Allocator.Error`).
#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TrackRegistryFull;

#[cfg(test)]
thread_local! {
    pub(crate) static TRACK_TW: core::cell::RefCell<
        rustty_util::tripwire::Module<TrackFp, TrackRegistryFull>,
    > = core::cell::RefCell::new(rustty_util::tripwire::module());
}

#[cfg(test)]
#[inline]
fn track_tw_check() -> core::result::Result<(), TrackRegistryFull> {
    TRACK_TW.with(|tw| tw.borrow_mut().check(TrackFp::Reserve))
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
