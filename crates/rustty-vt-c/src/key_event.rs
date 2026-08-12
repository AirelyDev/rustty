//! rustty-vt-c key_event
//!
//! PORTS: ghostty src/terminal/c/key_event.zig
//!
//! PORT STATUS: done

// PORT NOTE: the reference imports are
//   const std = @import("std");
//   const Allocator = std.mem.Allocator;
//   const lib = @import("../lib.zig");
//   const CAllocator = lib.alloc.Allocator;
//   const key = @import("../../input/key.zig");
//   const Result = @import("result.zig").Result;
// `key` is rustty-vt's `input::key` module; `Result` is the sibling `result`
// module's `Result`; `CAllocator` is `rustty_lib::allocator::Allocator`.

use crate::cwrap::{create, destroy};
use crate::result::Result;
use rustty_lib::allocator::Allocator as CAllocator;
use rustty_vt::input::key;

// const log = std.log.scoped(.key_event);

/// Wrapper around KeyEvent that tracks the allocator for C API usage.
/// The UTF-8 text is not owned by this wrapper - the caller is responsible
/// for ensuring the lifetime of any UTF-8 text set via set_utf8.
//
// PORT NOTE: the reference stores the resolved `std.mem.Allocator`. rustty stores
// the original (possibly null) C `Allocator` from which `default(...)` resolves
// the identical allocator in `free` (see `cwrap`). The `event: key.KeyEvent`
// default mirrors `key.KeyEvent = .{}`.
//
// PORT NOTE (utf8 ownership, W80-B02-01): the reference KeyEvent's `utf8` is a
// NON-owning `[]const u8` borrow of the caller's bytes — `set_utf8` stores
// `ptr[0..len]` verbatim and `get_utf8` hands that very pointer back, so a
// caller sees its OWN pointer and any later mutation of its buffer is observed
// by both `get_utf8` and the encoder. That borrow contract is the published one
// (`include/rustty/vt/key/event.h`: "does NOT take ownership ... the caller
// must ensure the string remains valid").
//
// rustty-vt's `KeyEvent` (the R3 source of truth) owns `utf8: Vec<u8>`, so this
// wrapper — not rustty-vt — carries the reference's borrow: `utf8_ptr`/
// `utf8_len` ARE the reference's slice, and the owned vec is refreshed from
// them at every use (`sync_utf8`) so the encoder reads the caller's live bytes.
// An earlier revision instead copied into the vec at `set_utf8` time and
// claimed that was "observationally identical"; it is not — the C surface
// exposes both pointer identity and post-`set` mutation, and paired C probes
// against the reference measured `same_pointer=0`/`reflects_source_mutation=0`
// where the reference gives 1/1.
// PORT NOTE: the reference `KeyEventWrapper` is file-private. In Rust the opaque
// handle `Event = *mut KeyEventWrapper` is `pub`, so the struct must be `pub`
// too (a private pointee in a public alias is rejected); its fields stay private
// to this module, preserving the reference's encapsulation.
pub struct KeyEventWrapper {
    event: key::KeyEvent,
    alloc: Option<CAllocator>,
    // PORT NOTE (W80-B02-01): the reference's `event.utf8` slice itself — the
    // caller's pointer and length, borrowed, never copied. `event.utf8` (the
    // owned rustty-vt vec) is a derived cache refreshed by `sync_utf8`.
    utf8_ptr: *const u8,
    utf8_len: usize,
}

/// C: RusttyKeyEvent (R2)
// PORT NOTE: `pub const Event = ?*KeyEventWrapper;` — a nullable raw pointer to
// the heap wrapper. The C ABI passes this as an opaque `*mut KeyEventWrapper`.
pub type Event = *mut KeyEventWrapper;

// PORT NOTE (sibling-module access): `key_encode.zig` reads `event_.?.event`
// directly (same package). rustty keeps `KeyEventWrapper.event` private to this
// module, so a crate-internal borrow helper mirrors the reference's field
// access for `key_encode`.
//
/// # Safety
/// `event_` must be a valid `new`-returned pointer (the reference's `event_.?`).
pub(crate) unsafe fn event_ref<'a>(event_: Event) -> &'a key::KeyEvent {
    // PORT NOTE (W80-B02-01): the reference reads `event_.?.event.utf8`, which
    // IS the caller's live slice. rustty-vt's `KeyEvent` owns its `utf8`, so
    // refresh that owned copy from the borrowed ptr/len first — otherwise the
    // encoder would see the bytes as they were at `set_utf8` time instead of
    // the caller's current buffer contents.
    // SAFETY: `event_` validity is the caller's contract.
    unsafe { sync_utf8(event_) };
    // const event: *key.KeyEvent = &event_.?.event;
    // SAFETY: `event_` validity is the caller's contract.
    unsafe { &(*event_).event }
}

/// Refresh the owned `event.utf8` vec from the borrowed `utf8_ptr`/`utf8_len`.
///
/// # Safety
/// `event_` must be a valid `new`-returned pointer, and its stored
/// `utf8_ptr`/`utf8_len` must still describe a readable region (the caller's
/// borrow contract, exactly as in the reference).
unsafe fn sync_utf8(event_: Event) {
    // SAFETY: `event_` validity is the caller's contract.
    let ptr = unsafe { (*event_).utf8_ptr };
    let len = unsafe { (*event_).utf8_len };
    let event = unsafe { std::ptr::addr_of_mut!((*event_).event) };
    if ptr.is_null() || len == 0 {
        // SAFETY: `event` points at the wrapper's live `KeyEvent`.
        unsafe { (*event).utf8.clear() };
        return;
    }
    // SAFETY: `ptr`/`len` describe a readable region (borrow contract).
    let slice = unsafe { std::slice::from_raw_parts(ptr, len) };
    // SAFETY: `event` points at the wrapper's live `KeyEvent`.
    unsafe {
        (*event).utf8.clear();
        (*event).utf8.extend_from_slice(slice);
    }
}

/// PORT NOTE: `pub fn new(alloc_, result) callconv(...) Result`.
///
/// # Safety
/// `alloc_`, if non-null, must point to a valid `CAllocator`; `result` must be
/// a valid, writable pointer to `Event`.
pub unsafe fn new(alloc_: Option<&CAllocator>, result: *mut Event) -> Result {
    // const ptr = alloc.create(KeyEventWrapper) catch return .out_of_memory;
    // ptr.* = .{ .alloc = alloc };
    let ptr = unsafe {
        create(
            alloc_,
            KeyEventWrapper {
                event: key::KeyEvent::default(),
                alloc: alloc_.copied(),
                // `key.KeyEvent = .{}` leaves `utf8` the empty slice.
                utf8_ptr: std::ptr::null(),
                utf8_len: 0,
            },
        )
    };
    if ptr.is_null() {
        return Result::OutOfMemory;
    }
    // result.* = ptr;
    // SAFETY: `result` is a valid writable pointer (caller contract).
    unsafe { *result = ptr };
    // return .success;
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

/// PORT NOTE: `pub fn set_action(event_, action) callconv(...) void`. The
/// `intToEnum` runtime-safety guard lives on the raw `c_int` crossing the C
/// boundary (the `extern "C"` wrapper in lib.rs); a valid `key::Action` here
/// cannot be out of range.
///
/// # Safety
/// `event_` must be a valid `new`-returned pointer.
pub unsafe fn set_action(event_: Event, action: key::Action) {
    // const event: *key.KeyEvent = &event_.?.event;
    // event.action = action;
    // SAFETY: `event_` validity is the caller's contract.
    unsafe { (*event_).event.action = action };
}

/// # Safety
/// `event_` must be a valid `new`-returned pointer.
pub unsafe fn get_action(event_: Event) -> key::Action {
    // const event: *key.KeyEvent = &event_.?.event;
    // return event.action;
    unsafe { (*event_).event.action }
}

/// PORT NOTE: the `intToEnum` guard lives on the raw `c_int` in lib.rs.
///
/// # Safety
/// `event_` must be a valid `new`-returned pointer.
pub unsafe fn set_key(event_: Event, k: key::Key) {
    // event.key = k;
    unsafe { (*event_).event.key = k };
}

/// # Safety
/// `event_` must be a valid `new`-returned pointer.
pub unsafe fn get_key(event_: Event) -> key::Key {
    unsafe { (*event_).event.key }
}

/// # Safety
/// `event_` must be a valid `new`-returned pointer.
pub unsafe fn set_mods(event_: Event, mods: key::Mods) {
    // event.mods = mods;
    unsafe { (*event_).event.mods = mods };
}

/// # Safety
/// `event_` must be a valid `new`-returned pointer.
pub unsafe fn get_mods(event_: Event) -> key::Mods {
    unsafe { (*event_).event.mods }
}

/// # Safety
/// `event_` must be a valid `new`-returned pointer.
pub unsafe fn set_consumed_mods(event_: Event, consumed_mods: key::Mods) {
    // event.consumed_mods = consumed_mods;
    unsafe { (*event_).event.consumed_mods = consumed_mods };
}

/// # Safety
/// `event_` must be a valid `new`-returned pointer.
pub unsafe fn get_consumed_mods(event_: Event) -> key::Mods {
    unsafe { (*event_).event.consumed_mods }
}

/// # Safety
/// `event_` must be a valid `new`-returned pointer.
pub unsafe fn set_composing(event_: Event, composing: bool) {
    // event.composing = composing;
    unsafe { (*event_).event.composing = composing };
}

/// # Safety
/// `event_` must be a valid `new`-returned pointer.
pub unsafe fn get_composing(event_: Event) -> bool {
    unsafe { (*event_).event.composing }
}

/// PORT NOTE: `event.utf8 = if (utf8) |ptr| ptr[0..len] else "";` — see the
/// utf8-ownership note on `KeyEventWrapper`: the caller's pointer/length are
/// BORROWED (null collapses to the empty slice, as `else ""` does), and the
/// owned rustty-vt vec is refreshed from that borrow.
///
/// # Safety
/// `event_` must be a valid `new`-returned pointer; `utf8`, if non-null, must
/// point to `len` readable bytes, and — per the published header contract —
/// must remain valid for as long as the event may be read or encoded.
pub unsafe fn set_utf8(event_: Event, utf8: *const u8, len: usize) {
    // event.utf8 = if (utf8) |ptr| ptr[0..len] else "";
    unsafe {
        if utf8.is_null() {
            (*event_).utf8_ptr = std::ptr::null();
            (*event_).utf8_len = 0;
        } else {
            (*event_).utf8_ptr = utf8;
            (*event_).utf8_len = len;
        }
    }
    // Keep the owned rustty-vt vec in step with the borrow.
    // SAFETY: the region just stored is readable (caller contract).
    unsafe { sync_utf8(event_) };
}

/// PORT NOTE: `pub fn get_utf8(event_, len) callconv(...) ?[*]const u8`. Writes
/// the byte length through `len` (when non-null) and returns the bytes pointer,
/// or null when empty.
///
/// # Safety
/// `event_` must be a valid `new`-returned pointer; `len`, if non-null, must be
/// a valid, writable pointer to `usize`. The returned pointer is the CALLER's
/// own `set_utf8` pointer (the reference's borrow), valid for exactly as long
/// as the caller keeps that buffer alive.
pub unsafe fn get_utf8(event_: Event, len: *mut usize) -> *const u8 {
    // const event: *key.KeyEvent = &event_.?.event;
    // PORT NOTE (W80-B02-01): the reference's `event.utf8` IS the caller's
    // borrowed slice, so both the reported length and the returned pointer come
    // from the borrow (`utf8_ptr`/`utf8_len`), never from the derived owned vec.
    // A caller therefore gets ITS OWN pointer back and observes later mutation
    // of its buffer — the copy-based getter returned a different pointer frozen
    // at `set_utf8` time.
    // SAFETY: `event_` validity is the caller's contract.
    let ptr = unsafe { (*event_).utf8_ptr };
    // SAFETY: `event_` validity is the caller's contract.
    let utf8_len = unsafe { (*event_).utf8_len };
    // if (len) |l| l.* = event.utf8.len;
    if !len.is_null() {
        // SAFETY: `len` is a valid writable pointer (caller contract).
        unsafe { *len = utf8_len };
    }
    // return if (event.utf8.len == 0) null else event.utf8.ptr;
    if utf8_len == 0 {
        std::ptr::null()
    } else {
        ptr
    }
}

/// PORT NOTE: `event.unshifted_codepoint = @truncate(codepoint);` — truncated to
/// the `u21` storage (modeled as `u32`, masked to 21 bits).
///
/// # Safety
/// `event_` must be a valid `new`-returned pointer.
pub unsafe fn set_unshifted_codepoint(event_: Event, codepoint: u32) {
    // event.unshifted_codepoint = @truncate(codepoint);
    // PORT NOTE: `key::u21` is a `u32` modeling a 21-bit value; `@truncate` to
    // `u21` keeps the low 21 bits.
    unsafe { (*event_).event.unshifted_codepoint = codepoint & 0x1F_FFFF };
}

/// # Safety
/// `event_` must be a valid `new`-returned pointer.
pub unsafe fn get_unshifted_codepoint(event_: Event) -> u32 {
    // return event.unshifted_codepoint;
    unsafe { (*event_).event.unshifted_codepoint }
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
    fn set() {
        with_test_allocator(|ta| {
            let mut e: Event = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe { new(Some(ta), &mut e) });
            // defer free(e);

            // Test action
            // set_action(e, .press);
            unsafe { set_action(e, key::Action::Press) };
            // try testing.expectEqual(key.Action.press, e.?.event.action);
            assert_eq!(key::Action::Press, unsafe { (*e).event.action });

            // Test key
            // set_key(e, .key_a);
            unsafe { set_key(e, key::Key::KeyA) };
            // try testing.expectEqual(key.Key.key_a, e.?.event.key);
            assert_eq!(key::Key::KeyA, unsafe { (*e).event.key });

            // Test mods
            // const mods: key.Mods = .{ .shift = true, .ctrl = true };
            let mods = key::Mods {
                shift: true,
                ctrl: true,
                ..Default::default()
            };
            // set_mods(e, mods);
            unsafe { set_mods(e, mods) };
            // try testing.expect(e.?.event.mods.shift);
            assert!(unsafe { (*e).event.mods.shift });
            // try testing.expect(e.?.event.mods.ctrl);
            assert!(unsafe { (*e).event.mods.ctrl });

            // Test consumed mods
            // const consumed: key.Mods = .{ .shift = true };
            let consumed = key::Mods {
                shift: true,
                ..Default::default()
            };
            // set_consumed_mods(e, consumed);
            unsafe { set_consumed_mods(e, consumed) };
            // try testing.expect(e.?.event.consumed_mods.shift);
            assert!(unsafe { (*e).event.consumed_mods.shift });
            // try testing.expect(!e.?.event.consumed_mods.ctrl);
            assert!(!unsafe { (*e).event.consumed_mods.ctrl });

            // Test composing
            // set_composing(e, true);
            unsafe { set_composing(e, true) };
            // try testing.expect(e.?.event.composing);
            assert!(unsafe { (*e).event.composing });

            // Test UTF-8
            // const text = "hello";
            let text = b"hello";
            // set_utf8(e, text.ptr, text.len);
            unsafe { set_utf8(e, text.as_ptr(), text.len()) };
            // try testing.expectEqualStrings(text, e.?.event.utf8);
            assert_eq!(&text[..], unsafe { (*e).event.utf8.as_slice() });

            // Test UTF-8 null
            // set_utf8(e, null, 0);
            unsafe { set_utf8(e, std::ptr::null(), 0) };
            // try testing.expectEqualStrings("", e.?.event.utf8);
            assert_eq!(b"", unsafe { (*e).event.utf8.as_slice() });

            // Test unshifted codepoint
            // set_unshifted_codepoint(e, 'a');
            unsafe { set_unshifted_codepoint(e, b'a' as u32) };
            // try testing.expectEqual(@as(u21, 'a'), e.?.event.unshifted_codepoint);
            assert_eq!(b'a' as u32, unsafe { (*e).event.unshifted_codepoint });

            unsafe { free(e) };
        });
    }

    #[test]
    fn get() {
        with_test_allocator(|ta| {
            let mut e: Event = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe { new(Some(ta), &mut e) });
            // defer free(e);

            // Set some values
            // set_action(e, .repeat);
            unsafe { set_action(e, key::Action::Repeat) };
            // set_key(e, .key_z);
            unsafe { set_key(e, key::Key::KeyZ) };

            // const mods: key.Mods = .{ .alt = true, .super = true };
            let mods = key::Mods {
                alt: true,
                super_: true,
                ..Default::default()
            };
            // set_mods(e, mods);
            unsafe { set_mods(e, mods) };

            // const consumed: key.Mods = .{ .alt = true };
            let consumed = key::Mods {
                alt: true,
                ..Default::default()
            };
            // set_consumed_mods(e, consumed);
            unsafe { set_consumed_mods(e, consumed) };

            // set_composing(e, true);
            unsafe { set_composing(e, true) };

            // const text = "test";
            let text = b"test";
            // set_utf8(e, text.ptr, text.len);
            unsafe { set_utf8(e, text.as_ptr(), text.len()) };

            // set_unshifted_codepoint(e, 'z');
            unsafe { set_unshifted_codepoint(e, b'z' as u32) };

            // Get them back
            // try testing.expectEqual(key.Action.repeat, get_action(e));
            assert_eq!(key::Action::Repeat, unsafe { get_action(e) });
            // try testing.expectEqual(key.Key.key_z, get_key(e));
            assert_eq!(key::Key::KeyZ, unsafe { get_key(e) });

            // const got_mods = get_mods(e);
            let got_mods = unsafe { get_mods(e) };
            // try testing.expect(got_mods.alt);
            assert!(got_mods.alt);
            // try testing.expect(got_mods.super);
            assert!(got_mods.super_);

            // const got_consumed = get_consumed_mods(e);
            let got_consumed = unsafe { get_consumed_mods(e) };
            // try testing.expect(got_consumed.alt);
            assert!(got_consumed.alt);
            // try testing.expect(!got_consumed.super);
            assert!(!got_consumed.super_);

            // try testing.expect(get_composing(e));
            assert!(unsafe { get_composing(e) });

            // var utf8_len: usize = undefined;
            let mut utf8_len: usize = 0;
            // const got_utf8 = get_utf8(e, &utf8_len);
            let got_utf8 = unsafe { get_utf8(e, &mut utf8_len) };
            // try testing.expect(got_utf8 != null);
            assert!(!got_utf8.is_null());
            // try testing.expectEqual(@as(usize, 4), utf8_len);
            assert_eq!(4usize, utf8_len);
            // try testing.expectEqualStrings("test", got_utf8.?[0..utf8_len]);
            assert_eq!(b"test", unsafe {
                std::slice::from_raw_parts(got_utf8, utf8_len)
            });

            // try testing.expectEqual(@as(u32, 'z'), get_unshifted_codepoint(e));
            assert_eq!(b'z' as u32, unsafe { get_unshifted_codepoint(e) });

            unsafe { free(e) };
        });
    }

    #[test]
    fn complete_key_event() {
        with_test_allocator(|ta| {
            let mut e: Event = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe { new(Some(ta), &mut e) });
            // defer free(e);

            // Build a complete key event for shift+a
            // set_action(e, .press);
            unsafe { set_action(e, key::Action::Press) };
            // set_key(e, .key_a);
            unsafe { set_key(e, key::Key::KeyA) };

            // const mods: key.Mods = .{ .shift = true };
            let mods = key::Mods {
                shift: true,
                ..Default::default()
            };
            // set_mods(e, mods);
            unsafe { set_mods(e, mods) };

            // const consumed: key.Mods = .{ .shift = true };
            let consumed = key::Mods {
                shift: true,
                ..Default::default()
            };
            // set_consumed_mods(e, consumed);
            unsafe { set_consumed_mods(e, consumed) };

            // const text = "A";
            let text = b"A";
            // set_utf8(e, text.ptr, text.len);
            unsafe { set_utf8(e, text.as_ptr(), text.len()) };

            // set_unshifted_codepoint(e, 'a');
            unsafe { set_unshifted_codepoint(e, b'a' as u32) };

            // Verify all fields
            // try testing.expectEqual(key.Action.press, e.?.event.action);
            assert_eq!(key::Action::Press, unsafe { (*e).event.action });
            // try testing.expectEqual(key.Key.key_a, e.?.event.key);
            assert_eq!(key::Key::KeyA, unsafe { (*e).event.key });
            // try testing.expect(e.?.event.mods.shift);
            assert!(unsafe { (*e).event.mods.shift });
            // try testing.expect(e.?.event.consumed_mods.shift);
            assert!(unsafe { (*e).event.consumed_mods.shift });
            // try testing.expectEqualStrings("A", e.?.event.utf8);
            assert_eq!(b"A", unsafe { (*e).event.utf8.as_slice() });
            // try testing.expectEqual(@as(u21, 'a'), e.?.event.unshifted_codepoint);
            assert_eq!(b'a' as u32, unsafe { (*e).event.unshifted_codepoint });

            // Also test the getter
            // var utf8_len: usize = undefined;
            let mut utf8_len: usize = 0;
            // const got_utf8 = get_utf8(e, &utf8_len);
            let got_utf8 = unsafe { get_utf8(e, &mut utf8_len) };
            // try testing.expect(got_utf8 != null);
            assert!(!got_utf8.is_null());
            // try testing.expectEqual(@as(usize, 1), utf8_len);
            assert_eq!(1usize, utf8_len);
            // try testing.expectEqualStrings("A", got_utf8.?[0..utf8_len]);
            assert_eq!(b"A", unsafe {
                std::slice::from_raw_parts(got_utf8, utf8_len)
            });

            unsafe { free(e) };
        });
    }

    // W80-B02-01 (Rust-only proving test; the reference has none — the borrow
    // is simply what `event.utf8 = ptr[0..len]` means). `set_utf8` BORROWS the
    // caller's bytes and `get_utf8` returns that very pointer, so a caller sees
    // pointer identity AND observes any later mutation of its own buffer. The
    // earlier copy-into-`Vec<u8>` implementation returned a different pointer
    // frozen at `set_utf8` time; the audit's paired C probes measured
    // `same_pointer=0`/`reflects_source_mutation=0` against the reference's
    // `1`/`1`.
    //
    // ALIASING NOTE (Stacked Borrows): the wrapper KEEPS the pointer this test
    // hands it and reads through it after the mutation — exactly the C
    // contract. So the caller-side buffer must be reached through a SINGLE
    // provenance root: take one raw `*mut u8` from the local and do every read
    // AND the mutation through that same pointer. Naming the local again
    // (`text[0] = ...`, `text.as_ptr()`) creates a fresh borrow whose write
    // invalidates the tag the wrapper stored, and the wrapper's later read is
    // then UB under Miri (the G6 lane runs `cargo +nightly miri test
    // -p rustty-vt-c --lib`). Same doctrine as the raw-ptr `&mut`-free seams
    // elsewhere in the port: mutate through the alias's own pointer.
    #[test]
    fn get_utf8_returns_the_borrowed_caller_pointer_and_observes_mutation() {
        with_test_allocator(|ta| {
            let mut e: Event = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe { new(Some(ta), &mut e) });

            let mut text = *b"hi";
            let text_len = text.len();
            // The one provenance root for the caller's buffer.
            let base: *mut u8 = text.as_mut_ptr();
            unsafe { set_utf8(e, base.cast_const(), text_len) };

            // Pointer identity: the caller's own buffer comes back.
            let mut utf8_len: usize = 0;
            let got = unsafe { get_utf8(e, &mut utf8_len) };
            assert_eq!(base.cast_const(), got);
            assert_eq!(2usize, utf8_len);

            // Live: mutating the caller buffer is observed through the getter
            // and through the internal `event_ref` view the encoder reads.
            unsafe { *base = b'H' };
            let got = unsafe { get_utf8(e, &mut utf8_len) };
            assert_eq!(base.cast_const(), got);
            assert_eq!(b"Hi", unsafe { std::slice::from_raw_parts(got, utf8_len) });
            assert_eq!(b"Hi", unsafe { event_ref(e).utf8.as_slice() });

            // A zero-length borrow reports null, exactly as `event.utf8.len == 0`
            // does in the reference — even when the stored pointer is non-null.
            unsafe { set_utf8(e, base.cast_const(), 0) };
            utf8_len = 99;
            assert!(unsafe { get_utf8(e, &mut utf8_len) }.is_null());
            assert_eq!(0usize, utf8_len);

            unsafe { free(e) };
        });
    }
}
