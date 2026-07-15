//! rustty-os::page_allocator
//!
//! PORT STATUS: ported (reference oracle: `src/terminal/PageList.zig`
//! `pageAllocator()` ~line 322 + the Zig 0.15.2 `std.heap.PageAllocator`).
//!
//! The reference `PageList.pageAllocator()` is the page allocator used for all
//! of PageList's underlying VM page allocations:
//!
//! ```zig
//! inline fn pageAllocator() Allocator {
//!     // In tests we use our testing allocator so we can detect leaks.
//!     if (builtin.is_test) return std.testing.allocator;
//!     // On non-macOS we use our standard Zig page allocator.
//!     if (!builtin.target.os.tag.isDarwin()) return std.heap.page_allocator;
//!     // On macOS we want to tag our memory so we can assign it to our
//!     // core terminal usage.
//!     const mach = @import("../os/mach.zig");
//!     return mach.taggedPageAllocator(.application_specific_1);
//! }
//! ```
//!
//! It returns a `std.mem.Allocator`. The faithful Rust map of `std.mem.Allocator`
//! is `allocator_api2::alloc::Allocator` (`allocate(Layout) -> NonNull<[u8]>` /
//! `deallocate` mirror Zig `rawAlloc`/`rawFree`). This module exposes
//! [`PageAllocator`] — a concrete type implementing that trait — and the
//! [`page_allocator`] selector that reproduces the reference's darwin/non-darwin
//! branch.
//!
//! WHY THIS EXISTS (the point of the port): pages are allocated PAGE-ALIGNED via
//! `mmap(MAP_ANON | MAP_PRIVATE)` (unix), so their backing physical memory is
//! provided LAZILY and OS-zeroed on first touch — not eagerly `memset`. This is
//! the behavior the reference relies on and is what fixes the audit's eager-zero
//! RSS regression. Memory is released with `munmap`.
//
// PORT NOTE: the reference's test-only branch (`if (builtin.is_test) return
// std.testing.allocator;`) swaps in Zig's leak-detecting testing allocator so
// PageList's own tests catch leaks. That is a property of the Zig test runner,
// not of the page allocator's observable allocation behavior; the leak-detection
// concern is handled in Rust by Miri/the test harness. The page-allocation path
// (darwin tagged vs generic posix `mmap`) is what this module ports.

#![allow(non_snake_case)]

use core::alloc::Layout;
use core::ptr::NonNull;

use allocator_api2::alloc::{AllocError, Allocator};

#[cfg(any(target_os = "macos", target_os = "ios"))]
use crate::mach::VMTag;
// `TaggedPageAllocator` drives the mmap path, which is excluded under Miri.
#[cfg(all(not(miri), any(target_os = "macos", target_os = "ios")))]
use crate::mach::TaggedPageAllocator;

/// The page allocator PageList uses for all underlying VM page allocations.
///
/// PORT NOTE: the reference `pageAllocator()` returns a `std.mem.Allocator`. We return
/// a concrete value implementing the explicit-allocator trait; the
/// darwin/non-darwin selection is reproduced in [`page_allocator`].
//
// On Darwin the variant carries the macOS VM tag (smuggled into the reference's
// allocator context pointer); on other unix it is the untagged generic posix
// page allocator (Zig's `std.heap.page_allocator`).
#[derive(Clone, Copy)]
pub struct PageAllocator {
    // The Darwin VM tag this allocator stamps onto every mapping (smuggled into
    // the reference's allocator context pointer via `taggedPageAllocator`).
    // Unused under Miri, which serves allocations from the std heap (no mmap).
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    #[cfg_attr(miri, allow(dead_code))]
    tag: VMTag,
}

/// Creates the page allocator for PageList's VM page allocations.
///
/// PORT NOTE: faithful map of `PageList.pageAllocator()`. On Darwin it tags memory as
/// `application_specific_1` (so it shows up under our core terminal usage in
/// vmmap/Instruments); on every other OS it is the standard generic page
/// allocator (`std.heap.page_allocator`).
pub fn page_allocator() -> PageAllocator {
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    {
        // On macOS we want to tag our memory so we can assign it to our core
        // terminal usage. PORT NOTE: `mach.taggedPageAllocator(.application_specific_1)`.
        PageAllocator {
            tag: VMTag::application_specific_1,
        }
    }

    #[cfg(not(any(target_os = "macos", target_os = "ios")))]
    {
        // On non-macOS we use our standard (generic) page allocator.
        PageAllocator {}
    }
}

// PORT NOTE: `std.mem.Allocator`'s `rawAlloc`/`rawFree` map to the trait's
// `allocate`/`deallocate`. The reference's `alloc`/`free` vtable entries assert
// `n > 0` and forward to `map`/`unmap`; the trait guarantees `layout.size() > 0`
// is NOT enforced by the contract, so we treat a zero-size request the same way
// the reference's `map` does for a page-rounded zero (it returns a valid page).
unsafe impl Allocator for PageAllocator {
    // Under Miri there is no `mmap` (Miri cannot execute the syscall / map
    // file-backed memory). The `cfg(miri)` impl below mirrors the reference's
    // `if (builtin.is_test) return std.testing.allocator;` branch: under the test
    // interpreter we serve page-aligned, zeroed allocations from the std heap.
    // The observable contract callers rely on (page-aligned, zeroed-on-handout
    // memory) is identical to the mmap path; only the backing differs.
    #[cfg(not(any(miri, target_arch = "wasm32")))]
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        // PORT NOTE: `map(n, alignment)` — `n` is the byte count, `alignment` the
        // requested byte alignment. The mmap'd region is lazily OS-zeroed.
        let n = layout.size();
        let align = layout.align();

        // The reference's vtable `alloc` asserts `n > 0`. The trait permits
        // zero-size layouts; mmap of 0 bytes is invalid, so we round a zero
        // request up to a single page's worth of accounting by allocating the
        // smallest non-empty mapping (one byte rounded to a page). This keeps the
        // returned pointer page-aligned and dereferenceable for the (empty) slice.
        let map_n = if n == 0 { 1 } else { n };

        let ptr = self.map_pages(map_n, align).ok_or(AllocError)?;

        // The returned slice length is the requested size (Allocator contract:
        // the slice must be at least `layout.size()` and have `layout.align()`).
        // The backing mapping is page-rounded, but we report exactly the request.
        let slice = unsafe { core::slice::from_raw_parts_mut(ptr.as_ptr(), n) };
        Ok(NonNull::from(slice))
    }

    // PORT NOTE (wasm32): wasm has no `mmap` syscall, so — exactly like the Miri
    // branch, and faithful to the reference's `if (builtin.is_test) return
    // std.testing.allocator;` fallback — we serve page-aligned, zeroed
    // allocations from the Rust global heap. The observable contract (page
    // alignment + zeroed-on-handout) is identical to the mmap path.
    #[cfg(any(miri, target_arch = "wasm32"))]
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        miri_alloc::allocate(layout)
    }

    #[cfg(not(any(miri, target_arch = "wasm32")))]
    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        // PORT NOTE: `free(memory, ...)` -> `unmap(memory)`. `unmap` page-rounds the
        // length and `munmap`s it. A zero-size allocation mapped one page (see
        // `allocate`); free at least one byte's worth so the same page is unmapped.
        let len = if layout.size() == 0 { 1 } else { layout.size() };
        let slice = unsafe { core::slice::from_raw_parts_mut(ptr.as_ptr(), len) };
        self.unmap_pages(slice);
    }

    #[cfg(any(miri, target_arch = "wasm32"))]
    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        unsafe { miri_alloc::deallocate(ptr, layout) };
    }
}

// Miri-only std-heap backing for `PageAllocator`, faithful to the reference's
// `if (builtin.is_test) return std.testing.allocator;` branch. Page-aligned and
// zeroed on handout (matching the mmap path's observable zero contract). We
// record the exact `Layout` we allocated with by over-allocating a header so the
// matching `dealloc` uses the same size/align Miri requires.
#[cfg(any(miri, target_arch = "wasm32"))]
mod miri_alloc {
    use core::alloc::Layout;
    use core::ptr::NonNull;

    use allocator_api2::alloc::{AllocError, Allocator, Global};

    /// Page-align every allocation (matches the mmap path). 4096 on all targets
    /// we care about; the requested alignment is folded in via `layout.align()`.
    /// The size is rounded up to the alignment so `dealloc` reconstructs the same
    /// `Layout` deterministically from `(ptr, requested_layout)`.
    fn aligned_layout(layout: Layout) -> Layout {
        let align = layout.align().max(4096);
        let size = layout.size().max(1);
        let rounded = (size + align - 1) & !(align - 1);
        Layout::from_size_align(rounded, align).expect("valid page-aligned layout")
    }

    pub fn allocate(layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        let n = layout.size();
        let al = aligned_layout(layout);
        // Delegate to `Global` so the returned pointer carries whole-allocation
        // provenance (Stacked Borrows clean under Miri, unlike a manually
        // narrowed `from_raw_parts_mut`). `allocate_zeroed` matches the mmap
        // path's zeroed-on-handout contract.
        let slice = Global.allocate_zeroed(al)?;
        // Report exactly the requested size while preserving the full-allocation
        // provenance from `Global`. Use `NonNull::slice_from_raw_parts` (raw slice
        // metadata, NO intermediate `&mut [u8]` retag) so Stacked Borrows stays
        // clean under Miri — a `from_raw_parts_mut` would narrow the borrow stack.
        let base = slice.cast::<u8>();
        Ok(NonNull::slice_from_raw_parts(base, n))
    }

    /// # Safety
    /// `ptr` must have been returned by [`allocate`] with `layout`.
    pub unsafe fn deallocate(ptr: NonNull<u8>, layout: Layout) {
        let al = aligned_layout(layout);
        unsafe { Global.deallocate(ptr, al) };
    }
}

// PORT NOTE: dispatch the actual mmap/munmap to the platform path. On Darwin this is
// `mach::TaggedPageAllocator` (the tagged posix `mmap`); on other unix it is the
// identical untagged posix path (Zig's generic `std.heap.PageAllocator.map`).
//
// These methods exist so the trait impl above is OS-agnostic and the
// darwin/non-darwin split lives in exactly one place.

// `cfg(not(miri))`: under Miri these mmap-backed helpers are unused (the
// `cfg(miri)` trait impl serves from the std heap), so we exclude them entirely
// to avoid dead-code warnings while keeping the real OS path intact.
#[cfg(all(not(miri), any(target_os = "macos", target_os = "ios")))]
impl PageAllocator {
    fn map_pages(&self, n: usize, align: usize) -> Option<NonNull<u8>> {
        // The Darwin path tags the mapping with this allocator's VM tag.
        let p = TaggedPageAllocator::map(n, align, self.tag)?;
        NonNull::new(p)
    }

    fn unmap_pages(&self, memory: &mut [u8]) {
        TaggedPageAllocator::unmap(memory);
    }
}

// Generic (non-Darwin) unix path: faithful port of the posix branch of Zig's
// `std.heap.PageAllocator.map`/`unmap` — `mmap(MAP_ANON | MAP_PRIVATE)` /
// `munmap`, lazily OS-zeroed. This is the untagged equivalent of the Darwin path
// in `mach.rs` (fd `-1`, no VM tag).
#[cfg(all(not(miri), unix, not(any(target_os = "macos", target_os = "ios"))))]
impl PageAllocator {
    fn map_pages(&self, n: usize, align: usize) -> Option<NonNull<u8>> {
        let page_size = page_size();
        if n >= usize::MAX - page_size {
            return None;
        }
        let alignment_bytes = align;

        let aligned_len = mem_align_forward(n, page_size);
        let max_drop_len = alignment_bytes - alignment_bytes.min(page_size);
        let overalloc_len = if max_drop_len <= aligned_len - n {
            aligned_len
        } else {
            mem_align_forward(aligned_len + max_drop_len, page_size)
        };

        let slice_ptr = {
            let p = unsafe {
                libc::mmap(
                    core::ptr::null_mut(),
                    overalloc_len,
                    libc::PROT_READ | libc::PROT_WRITE,
                    libc::MAP_PRIVATE | libc::MAP_ANON,
                    -1,
                    0,
                )
            };
            if p == libc::MAP_FAILED {
                return None;
            }
            p.cast::<u8>()
        };

        let result_ptr = align_pointer(slice_ptr, alignment_bytes)?;
        let drop_len = (result_ptr as usize) - (slice_ptr as usize);
        if drop_len != 0 {
            unsafe { libc::munmap(slice_ptr.cast(), drop_len) };
        }
        let remaining_len = overalloc_len - drop_len;
        if remaining_len > aligned_len {
            unsafe {
                libc::munmap(
                    result_ptr.add(aligned_len).cast(),
                    remaining_len - aligned_len,
                )
            };
        }
        NonNull::new(result_ptr)
    }

    fn unmap_pages(&self, memory: &mut [u8]) {
        let page_aligned_len = mem_align_forward(memory.len(), page_size());
        unsafe { libc::munmap(memory.as_mut_ptr().cast(), page_aligned_len) };
    }
}

#[cfg(all(unix, not(any(target_os = "macos", target_os = "ios"))))]
fn page_size() -> usize {
    unsafe { libc::sysconf(libc::_SC_PAGESIZE) as usize }
}

#[cfg(all(unix, not(any(target_os = "macos", target_os = "ios"))))]
fn mem_align_forward(addr: usize, alignment: usize) -> usize {
    (addr + alignment - 1) & !(alignment - 1)
}

#[cfg(all(unix, not(any(target_os = "macos", target_os = "ios"))))]
fn align_pointer(ptr: *mut u8, align: usize) -> Option<*mut u8> {
    let addr = ptr as usize;
    let aligned = mem_align_forward(addr, align);
    if aligned < addr {
        return None;
    }
    Some(aligned as *mut u8)
}

// PORT NOTE: the reference's non-darwin, non-unix path is Zig's windows
// `std.heap.PageAllocator` (ntdll NtAllocateVirtualMemory == VirtualAlloc). This
// crate establishes no windows VM bindings yet; like `mach.rs`'s deferred
// `mremap` branch, the windows mapping path is left unported until a windows
// target lands. Compiling for windows is a deliberate error until then so the
// gap is explicit rather than silently wrong.
// wasm32 is excluded here: it never reaches `map_pages`/`unmap_pages` (its trait
// impl above serves from the std heap, like Miri), so no mapping path is needed.
#[cfg(all(not(unix), not(target_arch = "wasm32")))]
impl PageAllocator {
    fn map_pages(&self, _n: usize, _align: usize) -> Option<NonNull<u8>> {
        compile_error!("rustty-os::page_allocator: windows VM mapping path is not yet ported");
    }

    fn unmap_pages(&self, _memory: &mut [u8]) {
        compile_error!("rustty-os::page_allocator: windows VM mapping path is not yet ported");
    }
}

// PORT NOTE: these tests exercise the property the reference relies on — `mmap`'d
// pages come back page-aligned and lazily OS-zeroed. Miri cannot model the
// `mmap`/`munmap` syscalls, so the entire test module is gated out under Miri
// (the rest of the crate's Miri run stays green; this module simply does not run
// there). Under a normal (non-Miri) test run both tests execute.
#[cfg(all(test, not(miri)))]
mod tests {
    use super::*;

    /// A page-sized region is page-aligned, readable, and zero-initialized; and
    /// freeing it works.
    #[test]
    fn page_sized_region_is_aligned_zeroed_and_freeable() {
        let alloc = page_allocator();

        // sysconf page size (Darwin uses 16K on aarch64; any page works here).
        let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) as usize };
        let layout = Layout::from_size_align(page, page).unwrap();

        let region = alloc.allocate(layout).expect("mmap page failed");
        let ptr = region.cast::<u8>().as_ptr();

        // Page-aligned.
        assert_eq!(
            (ptr as usize) % page,
            0,
            "mmap'd region must be page-aligned"
        );

        // Readable and zero-initialized (lazily OS-zeroed, NOT eagerly memset).
        let bytes = unsafe { core::slice::from_raw_parts(ptr, page) };
        assert!(
            bytes.iter().all(|&b| b == 0),
            "fresh mmap'd page must read as all zeroes"
        );

        // Writable.
        let bytes_mut = unsafe { core::slice::from_raw_parts_mut(ptr, page) };
        bytes_mut[0] = 0xAB;
        bytes_mut[page - 1] = 0xCD;
        assert_eq!(bytes_mut[0], 0xAB);
        assert_eq!(bytes_mut[page - 1], 0xCD);

        // Freeing works (munmap). No leak; nothing should fault.
        unsafe { alloc.deallocate(region.cast::<u8>(), layout) };
    }

    /// An over-aligned request (alignment greater than the page size) still comes
    /// back correctly aligned, and frees cleanly.
    #[test]
    fn over_aligned_request_is_aligned_and_freeable() {
        let alloc = page_allocator();

        let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) as usize };
        let align = page * 4;
        let size = page * 2;
        let layout = Layout::from_size_align(size, align).unwrap();

        let region = alloc.allocate(layout).expect("over-aligned mmap failed");
        let ptr = region.cast::<u8>().as_ptr();
        assert_eq!(
            (ptr as usize) % align,
            0,
            "over-aligned region must meet the requested alignment"
        );

        // Zero-initialized across the whole request.
        let bytes = unsafe { core::slice::from_raw_parts(ptr, size) };
        assert!(bytes.iter().all(|&b| b == 0));

        unsafe { alloc.deallocate(region.cast::<u8>(), layout) };
    }
}
