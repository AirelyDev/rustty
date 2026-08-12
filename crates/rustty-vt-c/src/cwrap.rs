//! rustty-vt-c C-wrapper allocation glue
//!
//! PORT NOTE (Rust-only glue, R1 "no-loss, not no-addition"): several C API
//! objects (`key_event`, `mouse_event`, `key_encode`, `mouse_encode`, `osc`)
//! are heap-allocated wrapper structs created in `new()` and released in
//! `free()`:
//!
//!   const ptr = alloc.create(Wrapper) catch return .out_of_memory;
//!   ptr.* = .{ ... .alloc = alloc };   // store the resolved allocator
//!   ...
//!   alloc.destroy(wrapper);            // free via the stored allocator
//!
//! In Zig the wrapper stores the resolved `std.mem.Allocator` so it can free
//! itself. In rustty `lib.alloc.default(alloc_)` resolves the SAME allocator
//! the C `Allocator` (or the global default when null) selects, but it returns
//! a `Box<dyn ZigAllocator>` rather than a `Copy` value. The resolution is a
//! pure function of the original (possibly null) C `Allocator`, so each wrapper
//! stores that original `Option<Allocator>` (a `Copy` C struct) and `free`
//! reconstructs the identical allocator via `default(stored.as_ref())` — the
//! exact moral equivalent of carrying `alloc: Allocator` in the Zig struct.
//!
//! `create`/`destroy` below model `alloc.create(T)` / `alloc.destroy(p)`
//! against that resolved allocator with the matching `Layout` (size+alignment).
//! No reference logic/behavior changes; this is the minimum glue to drive the
//! C allocator across the FFI boundary soundly.

use rustty_lib::allocator::{default, Alignment, Allocator as CAllocator, ZigSlice};
use std::alloc::Layout;

/// `alloc.create(T)`: allocate storage for one `T` via the allocator the
/// (possibly null) C `Allocator` resolves to, write `value` into it, and return
/// a raw owning pointer. Returns null on allocation failure (the reference's
/// `catch return .out_of_memory`).
///
/// # Safety
/// `alloc_`, if non-null, must point to a valid `CAllocator` whose vtable
/// outlives the returned pointer. The returned pointer (if non-null) must be
/// released with `destroy` using the SAME `alloc_`.
pub unsafe fn create<T>(alloc_: Option<&CAllocator>, value: T) -> *mut T {
    // alloc.create(T)
    let ptr = unsafe { alloc_uninit::<T>(alloc_) };
    if ptr.is_null() {
        return std::ptr::null_mut();
    }
    // ptr.* = value;
    // SAFETY: `ptr` points at freshly allocated, correctly aligned storage for
    // exactly one `T`; `write` initializes it without dropping the (garbage)
    // prior contents.
    unsafe { ptr.write(value) };
    ptr
}

/// The ALLOCATION half of `create`: `alloc.create(T)` WITHOUT initializing the
/// storage. This is the shape the reference uses whenever fallible work sits
/// BETWEEN the allocation and the initialization:
///
///   const ptr = alloc.create(T) catch return error.OutOfMemory;
///   errdefer alloc.destroy(ptr);
///   ... fallible work ...
///   ptr.* = .{ ... };
///
/// so a caller-supplied C allocator observes the SAME callback sequence as the
/// reference (one alloc, plus one free on every error arm) rather than a
/// silently-skipped allocation. Returns null on allocation failure (the
/// reference's `catch return .out_of_memory`).
///
/// # Safety
/// `alloc_`, if non-null, must point to a valid `CAllocator` whose vtable
/// outlives the returned pointer. The returned pointer (if non-null) holds
/// UNINITIALIZED storage: it must be initialized with `ptr.write(..)` before
/// any read, and released with `free_uninit` (while still uninitialized) or
/// with `destroy` (once initialized), using the SAME `alloc_`.
pub unsafe fn alloc_uninit<T>(alloc_: Option<&CAllocator>) -> *mut T {
    let allocator = default(alloc_);
    let layout = Layout::new::<T>();
    // alloc.create(T) — `Layout::new::<T>` carries T's size+alignment.
    allocator.alloc(layout.size(), Alignment::from_byte_units(layout.align()), 0) as *mut T
}

/// `alloc.destroy(p)` for storage that is still UNINITIALIZED — the `errdefer
/// alloc.destroy(ptr)` arm of the shape above. Frees the backing bytes WITHOUT
/// running `T`'s destructor (there is no `T` there yet). Zig's `destroy` is
/// exactly this free (Zig has no destructors), so the reference's `errdefer`
/// and its normal `free` path are the same call; in Rust the two must be
/// distinguished.
///
/// # Safety
/// `ptr` must be a non-null pointer returned by `alloc_uninit::<T>` with the
/// same `alloc_`, whose storage was never initialized, and not yet freed.
pub unsafe fn free_uninit<T>(alloc_: Option<&CAllocator>, ptr: *mut T) {
    let allocator = default(alloc_);
    let layout = Layout::new::<T>();
    allocator.free(
        ZigSlice::new(ptr as *mut u8, layout.size()),
        Alignment::from_byte_units(layout.align()),
        0,
    );
}

/// `alloc.destroy(p)`: drop the `T` and free its storage via the allocator the
/// (possibly null) C `Allocator` resolves to. The `alloc_` MUST be the same one
/// passed to the matching `create`.
///
/// # Safety
/// `ptr` must be a pointer previously returned by `create` with the same
/// `alloc_`, not yet destroyed.
pub unsafe fn destroy<T>(alloc_: Option<&CAllocator>, ptr: *mut T) {
    let allocator = default(alloc_);
    let layout = Layout::new::<T>();
    // SAFETY: `ptr` owns an initialized `T` (caller contract); run its
    // destructor before releasing the backing storage.
    unsafe { std::ptr::drop_in_place(ptr) };
    allocator.free(
        ZigSlice::new(ptr as *mut u8, layout.size()),
        Alignment::from_byte_units(layout.align()),
        0,
    );
}
