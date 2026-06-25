//! rustty-vt-c allocator
//!
//! PORTS: ghostty src/terminal/c/allocator.zig
//!
//! PORT STATUS: done

// PORT NOTE: the reference imports are
//   const std = @import("std");
//   const testing = std.testing;
//   const lib = @import("../lib.zig");
//   const CAllocator = lib.alloc.Allocator;
// `lib.alloc.Allocator` is the C allocator ABI struct ported in rustty-lib as
// `rustty_lib::allocator::Allocator`; `lib.alloc.default(...)` is
// `rustty_lib::allocator::default(...)`.

use rustty_lib::allocator::{default, Alignment, Allocator as CAllocator, ZigSlice};

/// Allocate a buffer of `len` bytes using the given allocator
/// (or the default allocator if NULL).
///
/// Returns a pointer to the allocated buffer, or NULL if the
/// allocation failed.
///
/// # Safety
/// `alloc_`, if non-null, must point to a valid `CAllocator` whose vtable
/// outlives the call. The returned pointer (if non-null) owns `len` bytes that
/// must be released with `free` using the same allocator.
pub unsafe fn alloc(alloc_: Option<&CAllocator>, len: usize) -> *mut u8 {
    // const allocator = lib.alloc.default(alloc_);
    let allocator = default(alloc_);
    // const buf = allocator.alloc(u8, len) catch return null;
    // PORT NOTE: `allocator.alloc(u8, len)` allocates `len` bytes at byte (1)
    // alignment. The rustty allocator returns a null `*mut u8` on failure (Zig
    // returns `error.OutOfMemory`), so the null check IS the `catch return null`.
    let buf = allocator.alloc(len, Alignment(0), 0);
    if buf.is_null() {
        return std::ptr::null_mut();
    }
    // return buf.ptr;
    buf
}

/// Free memory that was allocated by a librustty-vt function.
///
/// This must be used to free buffers returned by functions like
/// `format_alloc`. Pass the same allocator (or NULL for the default)
/// that was used for the allocation.
///
/// # Safety
/// `ptr`, if non-null, must be a buffer of `len` bytes previously returned by
/// `alloc` with the same `alloc_`. `alloc_`, if non-null, must point to a valid
/// `CAllocator`.
pub unsafe fn free(alloc_: Option<&CAllocator>, ptr: *mut u8, len: usize) {
    // const mem = ptr orelse return;
    if ptr.is_null() {
        return;
    }
    let mem = ptr;
    // const allocator = lib.alloc.default(alloc_);
    let allocator = default(alloc_);
    // allocator.free(mem[0..len]);
    allocator.free(ZigSlice::new(mem, len), Alignment(0), 0);
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustty_lib::allocator::{GlobalZigAllocator, ZigAllocatorRef};

    // PORT NOTE: the reference passes `&lib.alloc.test_allocator` — Zig's
    // leak-detecting testing allocator. rustty has no such global; the parity
    // analog is a C `Allocator` backed by the process global allocator (leaks
    // are caught by Miri per R6). `with_test_allocator` builds that C allocator
    // and runs `f` with it; the `ZigAllocatorRef` must outlive the `Allocator`,
    // so it is held on the stack for the duration of the closure.
    fn with_test_allocator<R>(f: impl FnOnce(&CAllocator) -> R) -> R {
        let g = GlobalZigAllocator;
        let zref = ZigAllocatorRef::new(&g);
        let calloc = CAllocator::from_zig(&zref);
        f(&calloc)
    }

    #[test]
    fn alloc_returns_non_null() {
        with_test_allocator(|ta| {
            // const ptr = alloc(&lib.alloc.test_allocator, 16);
            let ptr = unsafe { alloc(Some(ta), 16) };
            // try testing.expect(ptr != null);
            assert!(!ptr.is_null());
            // free(&lib.alloc.test_allocator, ptr, 16);
            unsafe { free(Some(ta), ptr, 16) };
        });
    }

    #[test]
    fn alloc_with_null_allocator() {
        // const ptr = alloc(null, 8);
        let ptr = unsafe { alloc(None, 8) };
        // try testing.expect(ptr != null);
        assert!(!ptr.is_null());
        // free(null, ptr, 8);
        unsafe { free(None, ptr, 8) };
    }

    #[test]
    fn alloc_zero_length() {
        with_test_allocator(|ta| {
            // const ptr = alloc(&lib.alloc.test_allocator, 0);
            let ptr = unsafe { alloc(Some(ta), 0) };
            // defer free(&lib.alloc.test_allocator, ptr, 0);
            unsafe { free(Some(ta), ptr, 0) };
        });
    }

    #[test]
    fn free_null_pointer() {
        with_test_allocator(|ta| {
            // free(&lib.alloc.test_allocator, null, 0);
            unsafe { free(Some(ta), std::ptr::null_mut(), 0) };
        });
    }

    #[test]
    fn free_allocated_memory() {
        with_test_allocator(|ta| {
            // const allocator = lib.alloc.default(&lib.alloc.test_allocator);
            let allocator = default(Some(ta));
            // const mem = try allocator.alloc(u8, 16);
            let mem = allocator.alloc(16, Alignment(0), 0);
            assert!(!mem.is_null());
            // free(&lib.alloc.test_allocator, mem.ptr, mem.len);
            unsafe { free(Some(ta), mem, 16) };
        });
    }

    #[test]
    fn free_with_null_allocator() {
        // null allocator falls back to the default (test allocator in tests)
        // const allocator = lib.alloc.default(null);
        let allocator = default(None);
        // const mem = try allocator.alloc(u8, 8);
        let mem = allocator.alloc(8, Alignment(0), 0);
        assert!(!mem.is_null());
        // free(null, mem.ptr, mem.len);
        unsafe { free(None, mem, 8) };
    }
}
