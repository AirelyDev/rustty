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

/// Records every live block's requested size in a fixed side table keyed by
/// address, and counts deallocations whose layout size differs from it. There
/// is no header: the pointer handed out is the one `System` returned, and it is
/// the only pointer `dealloc` ever uses (a header in front of the block made
/// Miri report the header read, and then the whole-block free, as Stacked
/// Borrows violations against the caller's pointer).
struct SizeChecking;

static MISMATCHES: AtomicUsize = AtomicUsize::new(0);

/// Open-addressed table of live blocks: `ADDRS[i]` is a block address (0 =
/// empty, `TOMB` = freed), `SIZES[i]` its requested size.
const SLOTS: usize = 1 << 14;
const TOMB: usize = 1;
static ADDRS: [AtomicUsize; SLOTS] = [const { AtomicUsize::new(0) }; SLOTS];
static SIZES: [AtomicUsize; SLOTS] = [const { AtomicUsize::new(0) }; SLOTS];

fn home(addr: usize) -> usize {
    (addr.wrapping_mul(0x9E37_79B9_7F4A_7C15_u64 as usize) >> (usize::BITS - 14)) % SLOTS
}

fn record(addr: usize, size: usize) {
    let h = home(addr);
    for i in 0..SLOTS {
        let s = (h + i) % SLOTS;
        let cur = ADDRS[s].load(Ordering::Acquire);
        if (cur == 0 || cur == TOMB)
            && ADDRS[s]
                .compare_exchange(cur, addr, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
        {
            SIZES[s].store(size, Ordering::Release);
            return;
        }
    }
    // Table full: the block is simply not tracked (its free is not checked).
}

/// Removes `addr` and returns its recorded size, or `None` if untracked.
fn take(addr: usize) -> Option<usize> {
    let h = home(addr);
    for i in 0..SLOTS {
        let s = (h + i) % SLOTS;
        match ADDRS[s].load(Ordering::Acquire) {
            0 => return None,
            a if a == addr => {
                let size = SIZES[s].load(Ordering::Acquire);
                ADDRS[s].store(TOMB, Ordering::Release);
                return Some(size);
            }
            _ => {}
        }
    }
    None
}

unsafe impl GlobalAlloc for SizeChecking {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: forwarded unchanged to the system allocator.
        let p = unsafe { System.alloc(layout) };
        if !p.is_null() {
            record(p.addr(), layout.size());
        }
        p
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let size = match take(ptr.addr()) {
            Some(recorded) => {
                if recorded != layout.size() {
                    MISMATCHES.fetch_add(1, Ordering::Relaxed);
                }
                recorded
            }
            None => layout.size(),
        };
        // Free with the RECORDED size so the system heap itself stays sound.
        // SAFETY: `ptr` came from `System.alloc` with this size and alignment.
        unsafe { System.dealloc(ptr, Layout::from_size_align_unchecked(size, layout.align())) }
    }
}

// D6-ALLOW(size-checking shim, not an allocator substitution): this lives in an
// integration test target (`tests/`), so it exists only in this one test binary
// and can never enter a shipped build. Every block is still a plain
// `malloc`/`free` from `std::alloc::System`, which the ASan/LSan interceptors
// see; the shim only records each block's size in a side table and counts
// mismatched frees.
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
