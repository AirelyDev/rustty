//! rustty-vt-c mouse_event
//!
//! PORTS: ghostty src/terminal/c/mouse_event.zig
//!
//! PORT STATUS: done

// PORT NOTE: the reference imports are
//   const std = @import("std");
//   const Allocator = std.mem.Allocator;
//   const testing = std.testing;
//   const lib = @import("../lib.zig");
//   const CAllocator = lib.alloc.Allocator;
//   const key = @import("../../input/key.zig");
//   const mouse = @import("../../input/mouse.zig");
//   const mouse_encode = @import("../../input/mouse_encode.zig");
//   const Result = @import("result.zig").Result;
// These map to rustty-vt's `input::{key, mouse, mouse_encode}` modules and the
// sibling `result` module; `CAllocator` is `rustty_lib::allocator::Allocator`.

use crate::cwrap::{create, destroy};
use crate::result::Result;
use rustty_lib::allocator::Allocator as CAllocator;
use rustty_vt::input::{key, mouse, mouse_encode};

// const log = std.log.scoped(.mouse_event);

/// Wrapper around mouse event that tracks the allocator for C API usage.
//
// PORT NOTE: the reference stores the resolved `std.mem.Allocator`; rustty stores
// the original (possibly null) C `Allocator` from which `default(...)` resolves
// the identical allocator in `free` (see `cwrap`). `event: mouse_encode.Event`
// defaults via `mouse_encode.Event = .{}`.
// PORT NOTE: the reference `MouseEventWrapper` is file-private. In Rust the
// opaque handle `Event = *mut MouseEventWrapper` is `pub`, so the struct must be
// `pub` too (a private pointee in a public alias is rejected); its fields stay
// private to this module, preserving the reference's encapsulation.
pub struct MouseEventWrapper {
    event: mouse_encode::Event,
    alloc: Option<CAllocator>,
}

/// C: RusttyMouseEvent (R2)
pub type Event = *mut MouseEventWrapper;

/// C: RusttyMouseAction (R2)
// PORT NOTE: `pub const Action = mouse.Action;`
pub use mouse::Action;

/// C: RusttyMouseButton (R2)
// PORT NOTE: `pub const Button = mouse.Button;`
pub use mouse::Button;

/// C: RusttyMousePosition (R2)
// PORT NOTE: `pub const Position = mouse_encode.Event.Pos;`
pub use mouse_encode::Pos as Position;

/// C: RusttyMods (R2)
// PORT NOTE: `pub const Mods = key.Mods;`
pub use key::Mods;

/// PORT NOTE: `pub fn new(alloc_, result) callconv(...) Result`.
///
/// # Safety
/// `alloc_`, if non-null, must point to a valid `CAllocator`; `result` must be
/// a valid, writable pointer to `Event`.
pub unsafe fn new(alloc_: Option<&CAllocator>, result: *mut Event) -> Result {
    // const ptr = alloc.create(MouseEventWrapper) catch return .out_of_memory;
    // ptr.* = .{ .alloc = alloc };
    let ptr = unsafe {
        create(
            alloc_,
            MouseEventWrapper {
                event: mouse_encode::Event::default(),
                alloc: alloc_.copied(),
            },
        )
    };
    if ptr.is_null() {
        return Result::OutOfMemory;
    }
    // result.* = ptr;
    // SAFETY: `result` is a valid writable pointer (caller contract).
    unsafe { *result = ptr };
    Result::Success
}

/// PORT NOTE: `pub fn free(event_) callconv(...) void`.
///
/// # Safety
/// `event_`, if non-null, must be a pointer returned by `new`, not yet freed.
pub unsafe fn free(event_: Event) {
    // const wrapper = event_ orelse return;
    if event_.is_null() {
        return;
    }
    // const alloc = wrapper.alloc;
    // SAFETY: `event_` is a valid `new`-returned pointer (caller contract).
    let alloc = unsafe { (*event_).alloc };
    // alloc.destroy(wrapper);
    unsafe { destroy(alloc.as_ref(), event_) };
}

/// PORT NOTE: the `intToEnum` runtime-safety guard lives on the raw `c_int`
/// crossing the C boundary (the `extern "C"` wrapper in lib.rs).
///
/// # Safety
/// `event_` must be a valid `new`-returned pointer.
pub unsafe fn set_action(event_: Event, action: Action) {
    // event_.?.event.action = action;
    unsafe { (*event_).event.action = action };
}

/// # Safety
/// `event_` must be a valid `new`-returned pointer.
pub unsafe fn get_action(event_: Event) -> Action {
    // return event_.?.event.action;
    unsafe { (*event_).event.action }
}

/// PORT NOTE: the `intToEnum` guard lives on the raw `c_int` in lib.rs.
///
/// # Safety
/// `event_` must be a valid `new`-returned pointer.
pub unsafe fn set_button(event_: Event, button: Button) {
    // event_.?.event.button = button;
    unsafe { (*event_).event.button = Some(button) };
}

/// # Safety
/// `event_` must be a valid `new`-returned pointer.
pub unsafe fn clear_button(event_: Event) {
    // event_.?.event.button = null;
    unsafe { (*event_).event.button = None };
}

/// PORT NOTE: `pub fn get_button(event_, out) callconv(...) bool`. Writes the
/// button through `out` (when non-null) and returns whether a button is set.
///
/// # Safety
/// `event_` must be a valid `new`-returned pointer; `out`, if non-null, must be
/// a valid, writable pointer to `Button`.
pub unsafe fn get_button(event_: Event, out: *mut Button) -> bool {
    // if (event_.?.event.button) |button| {
    if let Some(button) = unsafe { (*event_).event.button } {
        // if (out) |ptr| ptr.* = button;
        if !out.is_null() {
            // SAFETY: `out` is a valid writable pointer (caller contract).
            unsafe { *out = button };
        }
        // return true;
        return true;
    }
    // return false;
    false
}

/// # Safety
/// `event_` must be a valid `new`-returned pointer.
pub unsafe fn set_mods(event_: Event, mods: Mods) {
    // event_.?.event.mods = mods;
    unsafe { (*event_).event.mods = mods };
}

/// # Safety
/// `event_` must be a valid `new`-returned pointer.
pub unsafe fn get_mods(event_: Event) -> Mods {
    // return event_.?.event.mods;
    unsafe { (*event_).event.mods }
}

/// # Safety
/// `event_` must be a valid `new`-returned pointer.
pub unsafe fn set_position(event_: Event, pos: Position) {
    // event_.?.event.pos = pos;
    unsafe { (*event_).event.pos = pos };
}

/// # Safety
/// `event_` must be a valid `new`-returned pointer.
pub unsafe fn get_position(event_: Event) -> Position {
    // return event_.?.event.pos;
    unsafe { (*event_).event.pos }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::with_test_allocator;

    #[test]
    fn alloc() {
        with_test_allocator(|ta| {
            // var e: Event = undefined;
            let mut e: Event = std::ptr::null_mut();
            // try testing.expectEqual(Result.success, new(&lib.alloc.test_allocator, &e));
            assert_eq!(Result::Success, unsafe { new(Some(ta), &mut e) });
            // free(e);
            unsafe { free(e) };
        });
    }

    #[test]
    fn free_null() {
        // free(null);
        unsafe { free(std::ptr::null_mut()) };
    }

    #[test]
    fn set_get() {
        with_test_allocator(|ta| {
            let mut e: Event = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe { new(Some(ta), &mut e) });
            // defer free(e);

            // Action
            // set_action(e, .motion);
            unsafe { set_action(e, Action::Motion) };
            // try testing.expectEqual(Action.motion, get_action(e));
            assert_eq!(Action::Motion, unsafe { get_action(e) });

            // Button
            // set_button(e, .left);
            unsafe { set_button(e, Button::Left) };
            // var button: Button = .unknown;
            let mut button: Button = Button::Unknown;
            // try testing.expect(get_button(e, &button));
            assert!(unsafe { get_button(e, &mut button) });
            // try testing.expectEqual(Button.left, button);
            assert_eq!(Button::Left, button);
            // try testing.expect(get_button(e, null));
            assert!(unsafe { get_button(e, std::ptr::null_mut()) });

            // clear_button(e);
            unsafe { clear_button(e) };
            // try testing.expect(!get_button(e, &button));
            assert!(!unsafe { get_button(e, &mut button) });

            // Mods
            // const mods: Mods = .{ .shift = true, .ctrl = true };
            let mods = Mods {
                shift: true,
                ctrl: true,
                ..Default::default()
            };
            // set_mods(e, mods);
            unsafe { set_mods(e, mods) };
            // const got_mods = get_mods(e);
            let got_mods = unsafe { get_mods(e) };
            // try testing.expect(got_mods.shift);
            assert!(got_mods.shift);
            // try testing.expect(got_mods.ctrl);
            assert!(got_mods.ctrl);
            // try testing.expect(!got_mods.alt);
            assert!(!got_mods.alt);

            // Position
            // set_position(e, .{ .x = 12.5, .y = -4.0 });
            unsafe { set_position(e, Position { x: 12.5, y: -4.0 }) };
            // const pos = get_position(e);
            let pos = unsafe { get_position(e) };
            // try testing.expectEqual(@as(f32, 12.5), pos.x);
            assert_eq!(12.5f32, pos.x);
            // try testing.expectEqual(@as(f32, -4.0), pos.y);
            assert_eq!(-4.0f32, pos.y);

            unsafe { free(e) };
        });
    }
}
