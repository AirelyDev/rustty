//! global_raw_allocator_resize.rs — N273 (the sibling of N270):
//! `sys::GlobalRawAllocator::raw_resize` must not record a block as shorter
//! than it was allocated.
//!
//! `GlobalRawAllocator` is the Rust-heap `RawAllocator` behind the kitty PNG
//! decode seam (`LimitedAllocator::init(GlobalRawAllocator, ...)`), which an
//! embedder's decoder allocates through. It answered `true` for any shrink
//! without moving the block, so a decoder that shrank a buffer and then freed
//! it with the new length reached `dealloc` with a smaller layout than `alloc`
//! used (review S7-3 F1).
//!
//! WHY ITS OWN INTEGRATION BINARY: it installs a size-checking
//! `#[global_allocator]`, which must not be imposed on the crate's other tests.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

use rustty_datastruct::limited_allocator::LimitedAllocator;
use rustty_vt::sys::{Alignment, GlobalRawAllocator, RawAllocator, RawSlice};

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

/// The Zig caller contract: after `resize` answers `true`, the block's length
/// IS `new_len`, and it is freed with that length.
fn shrink_then_free(alloc: &dyn RawAllocator) -> bool {
    let a = Alignment(3);
    let ptr = alloc.raw_alloc(64, a, 0);
    assert!(!ptr.is_null());
    let shrunk = alloc.raw_resize(RawSlice { ptr, len: 64 }, a, 16, 0);
    let len = if shrunk { 16 } else { 64 };
    alloc.raw_free(RawSlice { ptr, len }, a, 0);
    shrunk
}

#[test]
fn global_raw_allocator_shrink_resize_never_frees_with_a_smaller_layout() {
    let before = MISMATCHES.load(Ordering::Relaxed);

    // Directly, and through the decode seam's `LimitedAllocator` wrapper.
    let direct = shrink_then_free(&GlobalRawAllocator);
    let limited = LimitedAllocator::init(GlobalRawAllocator, 1 << 20);
    let wrapped = shrink_then_free(&limited);

    // The fixed path: `dealloc` saw the layout `alloc` used.
    assert_eq!(
        before,
        MISMATCHES.load(Ordering::Relaxed),
        "a shrinking resize reported success without moving the block, so \
         `dealloc` was given a smaller layout than `alloc`",
    );
    // And it got there because the shrink was refused, not by luck.
    assert!(!direct && !wrapped, "the Rust heap cannot shrink a block in place");

    // Same-length resize is the one in-place answer; a growth is refused;
    // `remap` still shrinks by moving (`realloc`) and frees cleanly.
    let a = Alignment(3);
    let ptr = GlobalRawAllocator.raw_alloc(64, a, 0);
    assert!(GlobalRawAllocator.raw_resize(RawSlice { ptr, len: 64 }, a, 64, 0));
    assert!(!GlobalRawAllocator.raw_resize(RawSlice { ptr, len: 64 }, a, 128, 0));
    let moved = GlobalRawAllocator.raw_remap(RawSlice { ptr, len: 64 }, a, 16, 0);
    assert!(!moved.is_null());
    GlobalRawAllocator.raw_free(RawSlice { ptr: moved, len: 16 }, a, 0);
    assert_eq!(before, MISMATCHES.load(Ordering::Relaxed));
}
