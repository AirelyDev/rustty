//! Wasm allocation conveniences for caller-owned storage and opaque out slots.
//!
//! The primary use case for this is Wasm builds. Rustty relies a lot on
//! pointers to various types for ABI compatibility and creating those pointers
//! in Wasm is tedious. This file contains the small set of functions exposed by
//! the Wasm module without changing the API from the C library.
//!
//! Given these are convenience methods, they always use the default allocator.
//! If a caller is using a custom allocator, they have the expertise to
//! allocate these types manually using their custom allocator.

// PARITY: These `pub extern "C"` functions are the WASM/C allocator-export FFI
// boundary. They null-check then forward a caller-provided raw pointer to the
// global allocator (the moral equivalent of the reference's `alloc.rawFree`).
// The C/Wasm caller owns pointer validity; marking these `unsafe` would not
// help a C caller (it cannot observe Rust's `unsafe` qualifier) and would only
// add ABI-neutral noise, so we silence `not_unsafe_ptr_arg_deref` at module
// scope.
#![allow(clippy::not_unsafe_ptr_arg_deref)]

// const c_abi = @import("../c_abi.zig");
use crate::c_abi;

// Get our default allocator at comptime since it is known.
//
// PORT NOTE: The reference binds `alloc` at comptime via `default(null)`. In
// rustty the default allocator is the global Rust allocator; each function
// below uses it directly (`std::alloc`) with a `Layout` aligned to
// `wasm_alignment`, which is the same backend `default(None)` returns.
use std::alloc::{alloc as raw_alloc, dealloc, Layout};
use std::ffi::c_void;

// const wasm_alignment: std.mem.Alignment = .fromByteUnits(c_abi.max_alignment);
//
// PORT NOTE: `std.mem.Alignment` stores the log2; the Rust `Layout` takes the
// byte units directly, so the constant is carried in byte units here.
const WASM_ALIGNMENT: usize = c_abi::MAX_ALIGNMENT as usize;

/// A nullable opaque C handle stored in a constructor out-parameter slot.
// pub const Opaque = ?*anyopaque;
pub type Opaque = *mut c_void;

/// The `Layout` for a caller-owned Wasm ABI allocation of `len` bytes: the
/// Rust spelling of `alloc.rawAlloc(len, wasm_alignment, ...)`'s size/alignment
/// pair. `len` is nonzero at every call site (both entry points return early on
/// zero), so the layout is always valid.
#[inline]
fn bytes_layout(len: usize) -> Layout {
    Layout::from_size_align(len, WASM_ALIGNMENT)
        .expect("wasm ABI storage layout is valid (len > 0, align = max_alignment)")
}

/// Allocate `len` bytes of uninitialized, caller-owned Wasm ABI storage.
///
/// The returned pointer is aligned for any fundamental C ABI type and must be
/// released with `free_bytes` using the same `len`. Returns null when `len` is
/// zero or allocation fails.
// pub fn allocBytes(len: usize) callconv(.c) ?[*]u8 {
pub extern "C" fn alloc_bytes(len: usize) -> *mut u8 {
    // if (len == 0) return null;
    if len == 0 {
        return std::ptr::null_mut();
    }
    // return alloc.rawAlloc(len, wasm_alignment, @returnAddress());
    unsafe { raw_alloc(bytes_layout(len)) }
}

/// Release storage returned by `alloc_bytes`.
///
/// `len` must exactly match the allocation length. A null pointer is ignored.
// pub fn freeBytes(ptr: ?[*]u8, len: usize) callconv(.c) void {
pub extern "C" fn free_bytes(ptr: *mut u8, len: usize) {
    // const p = ptr orelse return;
    if ptr.is_null() {
        return;
    }
    // if (len == 0) return;
    if len == 0 {
        return;
    }
    // alloc.rawFree(p[0..len], wasm_alignment, @returnAddress());
    unsafe { dealloc(ptr, bytes_layout(len)) };
}

/// Allocate a null-initialized slot for an opaque constructor out-parameter.
///
/// The slot may be reused after each value is removed with `take_opaque`. It
/// must eventually be released with `free_opaque`.
// pub fn allocOpaque() callconv(.c) ?*Opaque {
pub extern "C" fn alloc_opaque() -> *mut Opaque {
    // const ptr = allocBytes(@sizeOf(Opaque)) orelse return null;
    let ptr = alloc_bytes(core::mem::size_of::<Opaque>());
    if ptr.is_null() {
        return std::ptr::null_mut();
    }
    // const result: *Opaque = @ptrCast(@alignCast(ptr));
    let result = ptr as *mut Opaque;
    // result.* = null;
    unsafe { result.write(std::ptr::null_mut()) };
    result
}

/// Release a slot returned by `alloc_opaque` without freeing its stored handle.
///
/// Call the handle's type-specific destructor before freeing a populated slot.
/// A null slot pointer is ignored.
// pub fn freeOpaque(ptr: ?*Opaque) callconv(.c) void {
pub extern "C" fn free_opaque(ptr: *mut Opaque) {
    // freeBytes(@ptrCast(ptr), @sizeOf(Opaque));
    free_bytes(ptr as *mut u8, core::mem::size_of::<Opaque>());
}

/// Remove and return the handle in an opaque out-parameter slot.
///
/// The slot is reset to null so it can be safely reused for another
/// constructor. This does not free either the handle or the slot. Returns null
/// when the slot pointer is null or the slot is empty.
// pub fn takeOpaque(ptr: ?*Opaque) callconv(.c) Opaque {
pub extern "C" fn take_opaque(ptr: *mut Opaque) -> Opaque {
    // const p = ptr orelse return null;
    if ptr.is_null() {
        return std::ptr::null_mut();
    }
    // const result = p.*;
    let result = unsafe { ptr.read() };
    // p.* = null;
    unsafe { ptr.write(std::ptr::null_mut()) };
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    // test "Wasm allocation" {
    #[test]
    fn wasm_allocation() {
        // const ptr = allocBytes(1) orelse return error.OutOfMemory;
        let ptr = alloc_bytes(1);
        assert!(!ptr.is_null(), "OutOfMemory");
        // defer freeBytes(ptr, 1);  (freed at the end; Rust has no defer)

        // try std.testing.expectEqual(
        //     @as(usize, 0),
        //     @intFromPtr(ptr) % c_abi.max_alignment,
        // );
        assert_eq!(0usize, (ptr as usize) % (c_abi::MAX_ALIGNMENT as usize));
        // ptr[0] = 42;
        unsafe { ptr.write(42) };
        // try std.testing.expectEqual(@as(u8, 42), ptr[0]);
        assert_eq!(42u8, unsafe { ptr.read() });

        // try std.testing.expect(allocBytes(0) == null);
        assert!(alloc_bytes(0).is_null());
        // freeBytes(null, 0);
        free_bytes(std::ptr::null_mut(), 0);

        // defer freeBytes(ptr, 1);
        free_bytes(ptr, 1);
    }

    // test "opaque slots are initialized and reusable" {
    #[test]
    fn opaque_slots_are_initialized_and_reusable() {
        // const slot = allocOpaque() orelse return error.OutOfMemory;
        let slot = alloc_opaque();
        assert!(!slot.is_null(), "OutOfMemory");
        // defer freeOpaque(slot);  (freed at the end; Rust has no defer)

        // try std.testing.expect(takeOpaque(slot) == null);
        assert!(take_opaque(slot).is_null());

        // var value: u8 = 42;
        let mut value: u8 = 42;
        // slot.* = &value;
        unsafe { slot.write(&mut value as *mut u8 as Opaque) };
        // try std.testing.expectEqual(@as(?*anyopaque, &value), takeOpaque(slot));
        assert_eq!(&mut value as *mut u8 as Opaque, take_opaque(slot));
        // try std.testing.expect(takeOpaque(slot) == null);
        assert!(take_opaque(slot).is_null());

        // defer freeOpaque(slot);
        free_opaque(slot);
    }
}
