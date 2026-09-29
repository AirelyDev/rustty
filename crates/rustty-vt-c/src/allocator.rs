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

use rustty_lib::allocator::{default, Allocator as CAllocator, ZigSlice};

/// Hand an owned byte buffer (the result of `aw.toOwnedSlice()`) across the C
/// ABI as `(ptr, len)`: `out_ptr.* = if (buf.len == 0) null else buf.ptr;
/// out_len.* = buf.len;`.
///
/// PORT NOTE (parity tail S7-8): the buffer was allocated, and exactly sized,
/// through the caller's allocator, so the caller releases it with
/// `rustty_free` and that same allocator, as in the reference.
///
/// PORT NOTE (`dd2edd760`, DECISIONS B1): empty output is NULL. Empty slices
/// may contain sentinel pointers that foreign runtimes reject even when the
/// length is zero (for example Go's stack scanner); an empty Rust buffer's
/// pointer is the dangling address, the same hazard. `rustty_free(alloc,
/// NULL, 0)` is the no-op the reference's is. This reverses the non-null empty
/// sentinel N331 recorded.
pub(crate) fn into_c_bytes(buf: rustty_vt::alloc::AVec<u8>) -> (*mut u8, usize) {
    let (ptr, len, cap, _alloc) = buf.into_raw_parts_with_alloc();
    debug_assert_eq!(len, cap, "an owned slice is exactly its length");
    if len == 0 {
        (std::ptr::null_mut(), 0)
    } else {
        (ptr, len)
    }
}

/// Allocate a buffer of `len` bytes using the given allocator
/// (or the default allocator if NULL).
///
/// Returns a pointer to the allocated buffer, or NULL if the
/// allocation failed or `len` is zero.
///
/// # Safety
/// `alloc_`, if non-null, must point to a valid `CAllocator` whose vtable
/// outlives the call. The returned pointer (if non-null) owns `len` bytes that
/// must be released with `free` using the same allocator.
pub unsafe fn alloc(alloc_: Option<&CAllocator>, len: usize) -> *mut u8 {
    // Zig's empty allocation pointer is not safe to expose to foreign runtimes.
    // if (len == 0) return null;
    if len == 0 {
        return std::ptr::null_mut();
    }
    // const allocator = lib.alloc.default(alloc_);
    let allocator = default(alloc_);
    // const buf = allocator.alloc(u8, len) catch return null;
    // PORT NOTE: `allocator.alloc(u8, len)` allocates `len` bytes at byte (1)
    // alignment. The rustty allocator returns a null `*mut u8` on failure (Zig
    // returns `error.OutOfMemory`), so the null check IS the `catch return null`.
    // PORT NOTE (W36-B06): `alloc_bytes` is the Zig TYPED helper
    // (`std.mem.Allocator.alloc`). Since `dd2edd760` a zero-length request
    // returns NULL above, before this call, so custom C allocators still
    // never observe zero-length alloc callbacks.
    let buf = allocator.alloc_bytes(len);
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
    // PORT NOTE (W36-B06): `free_bytes` is the Zig TYPED helper — a
    // zero-length slice returns BEFORE the raw free callback (Zig 0.16
    // `std.mem.Allocator.free`), so custom C allocators never observe
    // zero-length free callbacks.
    allocator.free_bytes(ZigSlice::new(mem, len));
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustty_lib::allocator::{Alignment, GlobalZigAllocator, ZigAllocatorRef};

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
            struct FreeOnDrop<'a>(&'a CAllocator, *mut u8);
            impl Drop for FreeOnDrop<'_> {
                fn drop(&mut self) {
                    unsafe { free(Some(self.0), self.1, 0) };
                }
            }
            let _free = FreeOnDrop(ta, ptr);
            // try testing.expectEqual(null, ptr);
            assert!(ptr.is_null());
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

    // PORT NOTE (W36-B06 proving test, Rust-only — the reference has no unit
    // test here): a CUSTOM counting C allocator must never observe the
    // zero-length paths (Ghostty short-circuits them), and the nonzero paths
    // must reach its callbacks with the exact lengths. Since `dd2edd760`
    // (DECISIONS B2) the zero-length `alloc` answer is NULL, never a sentinel
    // address; the "never calls the C allocator for zero bytes" half holds.
    #[test]
    fn custom_allocator_zero_length_short_circuit() {
        let stats = crate::test_util::CountingCAllocator::new();
        let ta = stats.c_allocator();

        // alloc(custom, 0): Ghostty returns NULL WITHOUT calling the raw
        // allocator callback.
        let ptr = unsafe { alloc(Some(&ta), 0) };
        assert!(ptr.is_null());
        assert_eq!(0, stats.alloc_calls.get());

        // free(custom, NULL, 0): returns before the raw free callback.
        unsafe { free(Some(&ta), ptr, 0) };
        assert_eq!(0, stats.free_calls.get());

        // free(custom, non-NULL, 0): a zero-length slice also returns before
        // the raw free callback (the typed `free`).
        let mut byte = 0u8;
        unsafe { free(Some(&ta), &mut byte, 0) };
        assert_eq!(0, stats.free_calls.get());

        // Nonzero paths still hit the callbacks with the correct lengths.
        let p = unsafe { alloc(Some(&ta), 16) };
        assert!(!p.is_null());
        assert_eq!(1, stats.alloc_calls.get());
        assert_eq!(Some(16), stats.last_alloc_len.get());
        unsafe { free(Some(&ta), p, 16) };
        assert_eq!(1, stats.free_calls.get());
        assert_eq!(Some(16), stats.last_free_len.get());
        assert_eq!(0, stats.live_bytes.get());
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
