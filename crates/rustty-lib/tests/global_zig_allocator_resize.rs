//! global_zig_allocator_resize.rs — N270: `GlobalZigAllocator::resize` must not
//! record a block as shorter than it was allocated.
//!
//! The reference `std.heap.c_allocator.resize` succeeds for any shrink, which
//! is legal over libc (`free` takes no size). The port backs `c_allocator()`
//! with the Rust global heap, where `GlobalAlloc::dealloc` must receive the
//! layout the block was allocated with. The old port answered `true` for any
//! shrink, so the caller's later `free(new_len)` reached `dealloc` with a
//! smaller layout than `alloc` used (review S7-12 F4, reproduced by a probe:
//! alloc 64, resize 64 -> 16 answered `true`, free 16 = one mismatch).
//!
//! WHY ITS OWN INTEGRATION BINARY: it installs a size-checking
//! `#[global_allocator]`, which must not be imposed on the crate's other tests.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

use rustty_lib::allocator::c_allocator;

/// Records every block's requested size in a header just before the block and
/// counts deallocations whose layout size differs from it.
struct SizeChecking;

static MISMATCHES: AtomicUsize = AtomicUsize::new(0);

/// The header offset for `align`: at least 16 bytes (room for the `usize` size
/// record) and a multiple of the block's alignment.
fn header(align: usize) -> usize {
    align.max(16)
}

/// The real block for a user layout of `size` bytes at `align`.
fn outer(size: usize, align: usize) -> Layout {
    let off = header(align);
    Layout::from_size_align(size + off, off).expect("test sizes are small")
}

unsafe impl GlobalAlloc for SizeChecking {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let off = header(layout.align());
        // SAFETY: `outer` is a valid non-zero layout.
        let base = unsafe { System.alloc(outer(layout.size(), layout.align())) };
        if base.is_null() {
            return base;
        }
        // SAFETY: `base + off` is inside the block, and the 8 bytes before it
        // are inside the header.
        unsafe {
            let user = base.add(off);
            (user.sub(core::mem::size_of::<usize>()) as *mut usize).write_unaligned(layout.size());
            user
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let off = header(layout.align());
        // SAFETY: `ptr` came from `alloc` above, so its header holds the size.
        let recorded =
            unsafe { (ptr.sub(core::mem::size_of::<usize>()) as *const usize).read_unaligned() };
        if recorded != layout.size() {
            MISMATCHES.fetch_add(1, Ordering::Relaxed);
        }
        // Free with the RECORDED size so the system heap itself stays sound.
        // SAFETY: the block was allocated with exactly this outer layout.
        unsafe { System.dealloc(ptr.sub(off), outer(recorded, layout.align())) }
    }
}

// D6-ALLOW(size-checking shim, not an allocator substitution): this lives in an
// integration test target (`tests/`), so it exists only in this one test binary
// and can never enter a shipped build. Every block is still a plain
// `malloc`/`free` from `std::alloc::System`, which the ASan/LSan interceptors
// see; the shim only prefixes a size record and counts mismatched frees.
#[global_allocator]
static ALLOCATOR: SizeChecking = SizeChecking;

#[test]
fn c_allocator_shrink_resize_never_frees_with_a_smaller_layout() {
    let c_alloc = c_allocator();
    // SAFETY: `c_allocator()` is a process static with a valid vtable.
    let vtable = unsafe { &*c_alloc.vtable };
    const ALIGN_LOG2: u8 = 3;

    let before = MISMATCHES.load(Ordering::Relaxed);

    // The Zig caller contract: after `resize` answers `true`, the block's
    // length IS `new_len`, and it is freed with that length.
    let ptr = (vtable.alloc)(c_alloc.ctx, 64, ALIGN_LOG2, 0);
    assert!(!ptr.is_null());
    let shrunk = (vtable.resize)(c_alloc.ctx, ptr, 64, ALIGN_LOG2, 16, 0);
    let len = if shrunk { 16 } else { 64 };
    (vtable.free)(c_alloc.ctx, ptr, len, ALIGN_LOG2, 0);

    // The fixed path: `dealloc` saw the layout `alloc` used.
    assert_eq!(
        before,
        MISMATCHES.load(Ordering::Relaxed),
        "a shrinking resize reported success without moving the block, so \
         `dealloc` was given a smaller layout than `alloc`",
    );
    // And it got there because the shrink was refused, not by luck.
    assert!(!shrunk, "the Rust heap cannot shrink a block in place");

    // Same-length resize is the one in-place answer the Rust heap can give,
    // and a growth is refused (no usable-size query).
    let ptr = (vtable.alloc)(c_alloc.ctx, 64, ALIGN_LOG2, 0);
    assert!(!ptr.is_null());
    assert!((vtable.resize)(c_alloc.ctx, ptr, 64, ALIGN_LOG2, 64, 0));
    assert!(!(vtable.resize)(c_alloc.ctx, ptr, 64, ALIGN_LOG2, 128, 0));
    // `remap` still shrinks, by moving (`realloc`), and the new length frees
    // cleanly.
    let moved = (vtable.remap)(c_alloc.ctx, ptr, 64, ALIGN_LOG2, 16, 0);
    assert!(!moved.is_null());
    (vtable.free)(c_alloc.ctx, moved, 16, ALIGN_LOG2, 0);
    assert_eq!(before, MISMATCHES.load(Ordering::Relaxed));
}
