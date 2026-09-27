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
    assert!(
        !direct && !wrapped,
        "the Rust heap cannot shrink a block in place"
    );

    // Same-length resize is the one in-place answer; a growth is refused;
    // `remap` still shrinks by moving (`realloc`) and frees cleanly.
    let a = Alignment(3);
    let ptr = GlobalRawAllocator.raw_alloc(64, a, 0);
    assert!(GlobalRawAllocator.raw_resize(RawSlice { ptr, len: 64 }, a, 64, 0));
    assert!(!GlobalRawAllocator.raw_resize(RawSlice { ptr, len: 64 }, a, 128, 0));
    let moved = GlobalRawAllocator.raw_remap(RawSlice { ptr, len: 64 }, a, 16, 0);
    assert!(!moved.is_null());
    GlobalRawAllocator.raw_free(
        RawSlice {
            ptr: moved,
            len: 16,
        },
        a,
        0,
    );
    assert_eq!(before, MISMATCHES.load(Ordering::Relaxed));
}
