//! rustty-os::mach
//!
//! PORT STATUS: ported
//!
//! Ports the reference's `src/os/mach.zig` — every line, comment, and `test {}`
//! is ported here (R1). Retargeted only for product identity (R2).

// PORT NOTE: the Zig reference imports `std`, `assert = quirks.inlineAssert`,
// `mem = std.mem`, and `Allocator = std.mem.Allocator`. In Rust we use
// `rustty_util::quirks::inlineAssert` for the assert seam (the reference imports
// it from `../quirks.zig`; that file is ported in rustty-util per R4) and the
// platform `libc` mmap bindings (R3) to back the tagged page allocator.
#![allow(non_snake_case)]

use rustty_util::quirks::inlineAssert as assert;

/// macOS virtual memory tags for use with mach_vm_map/mach_vm_allocate.
/// These identify memory regions in tools like vmmap and Instruments.
//
// PORT NOTE: `enum(u8)` with explicit application_specific_1..16 values plus the
// non-exhaustive `_` tag. Modeled as a `u8`-valued wrapper so that the
// non-exhaustive ("We ignore the rest...") tag is representable, matching Zig's
// open enum.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct VMTag(u8);

#[allow(non_upper_case_globals)]
impl VMTag {
    pub const application_specific_1: VMTag = VMTag(240);
    pub const application_specific_2: VMTag = VMTag(241);
    pub const application_specific_3: VMTag = VMTag(242);
    pub const application_specific_4: VMTag = VMTag(243);
    pub const application_specific_5: VMTag = VMTag(244);
    pub const application_specific_6: VMTag = VMTag(245);
    pub const application_specific_7: VMTag = VMTag(246);
    pub const application_specific_8: VMTag = VMTag(247);
    pub const application_specific_9: VMTag = VMTag(248);
    pub const application_specific_10: VMTag = VMTag(249);
    pub const application_specific_11: VMTag = VMTag(250);
    pub const application_specific_12: VMTag = VMTag(251);
    pub const application_specific_13: VMTag = VMTag(252);
    pub const application_specific_14: VMTag = VMTag(253);
    pub const application_specific_15: VMTag = VMTag(254);
    pub const application_specific_16: VMTag = VMTag(255);

    // We ignore the rest because we never realistic set them.
    // PORT NOTE: Zig's open-enum `_` member. Any other `u8` value is representable via
    // `VMTag(n)`; `intFromEnum`/`enumFromInt` are the inner `u8`.
    pub const fn from_int(n: u8) -> VMTag {
        VMTag(n)
    }
    pub const fn int_from(self) -> u8 {
        self.0
    }

    /// Converts the tag to the format expected by mach_vm_map/mach_vm_allocate.
    /// Equivalent to C macro: VM_MAKE_TAG(tag)
    pub fn make(self) -> i32 {
        ((self.0 as u32) << 24) as i32
    }
}

/// Creates a page allocator that tags all allocated memory with the given
/// VMTag.
//
// PORT NOTE: the Zig reference returns a `std.mem.Allocator` whose `.ptr` smuggles
// the tag in as the context pointer and whose `.vtable` is
// `TaggedPageAllocator.vtable`. Rust's stable allocator surface differs, so we
// return a concrete `TaggedPageAllocator` value carrying the tag; its methods
// (`alloc`/`resize`/`remap`/`free`) reproduce the reference vtable.
pub fn taggedPageAllocator(tag: VMTag) -> TaggedPageAllocator {
    // We smuggle the tag in as the context pointer.
    TaggedPageAllocator { tag }
}

/// This is based heavily on the Zig 0.15.2 PageAllocator implementation,
/// with only the posix implementation. Zig 0.15.2 is MIT licensed.
//
// PORT NOTE: the reference is a `struct` with a static `vtable` and free functions
// taking an opaque `context`. The context (`*anyopaque`) carries the `VMTag` as
// an integer (see `taggedPageAllocator`). We carry the tag as a field instead;
// `mem.Alignment` is modeled as a byte count (`usize`), matching its sole use as
// `alignment.toByteUnits()`.
pub struct TaggedPageAllocator {
    tag: VMTag,
}

impl TaggedPageAllocator {
    // PORT NOTE: `pub const vtable: Allocator.VTable = .{ .alloc, .resize, .remap,
    // .free }`. The four entries are reproduced as the methods below.

    pub fn alloc(&self, n: usize, alignment: usize, ra: usize) -> Option<*mut u8> {
        let _ = ra;
        assert(n > 0);
        let tag: VMTag = self.tag;
        Self::map(n, alignment, tag)
    }

    pub fn resize(
        &self,
        memory: &mut [u8],
        alignment: usize,
        new_len: usize,
        return_address: usize,
    ) -> bool {
        let _ = alignment;
        let _ = return_address;
        Self::realloc(memory, new_len, false).is_some()
    }

    pub fn remap(
        &self,
        memory: &mut [u8],
        alignment: usize,
        new_len: usize,
        return_address: usize,
    ) -> Option<*mut u8> {
        let _ = alignment;
        let _ = return_address;
        Self::realloc(memory, new_len, true)
    }

    pub fn free(&self, memory: &mut [u8], alignment: usize, return_address: usize) {
        let _ = alignment;
        let _ = return_address;
        Self::unmap(memory)
    }

    pub fn map(n: usize, alignment: usize, tag: VMTag) -> Option<*mut u8> {
        let page_size = page_size();
        if n >= usize::MAX - page_size {
            return None;
        }
        let alignment_bytes = alignment;

        let aligned_len = mem_align_forward(n, page_size);
        let max_drop_len = alignment_bytes - alignment_bytes.min(page_size);
        let overalloc_len = if max_drop_len <= aligned_len - n {
            aligned_len
        } else {
            mem_align_forward(aligned_len + max_drop_len, page_size)
        };
        // PORT NOTE: `next_mmap_addr_hint` is a Zig std internal mmap-placement hint
        // with no Rust std equivalent. `e8525c0fd` moved it into the allocator
        // itself —
        //   // Static local next_mmap_addr_hint since this has been removed from stdlib now
        //   var next_mmap_addr_hint: ?[*]align(std.heap.page_size_min) u8 = null;
        // — which is a Zig-stdlib-removal accommodation, N/A here
        // (`.port/phase-3/NON_PORTS.md` §E24: there is no global to replace).
        // The reference seeds the kernel hint and
        // updates it post-map purely as an optimization; passing a null hint (let
        // the kernel choose) is behavior-equivalent for allocation correctness.
        let hint: *mut std::ffi::c_void = std::ptr::null_mut();
        let slice_ptr = {
            let p = unsafe {
                libc::mmap(
                    hint,
                    overalloc_len,
                    libc::PROT_READ | libc::PROT_WRITE,
                    libc::MAP_PRIVATE | libc::MAP_ANON,
                    tag.make(),
                    0,
                )
            };
            if p == libc::MAP_FAILED {
                return None;
            }
            p.cast::<u8>()
        };
        // mem.alignPointer(slice.ptr, alignment_bytes) orelse return null
        let result_ptr = align_pointer(slice_ptr, alignment_bytes)?;
        // Unmap the extra bytes that were only requested in order to guarantee
        // that the range of memory we were provided had a proper alignment in it
        // somewhere. The extra bytes could be at the beginning, or end, or both.
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
        // PORT NOTE: post-map update of `next_mmap_addr_hint` (a hint-only Zig std
        // internal) is omitted; see the hint note above.
        Some(result_ptr)
    }

    pub fn unmap(memory: &mut [u8]) {
        let page_aligned_len = mem_align_forward(memory.len(), page_size());
        unsafe { libc::munmap(memory.as_mut_ptr().cast(), page_aligned_len) };
    }

    pub fn realloc(uncasted_memory: &mut [u8], new_len: usize, may_move: bool) -> Option<*mut u8> {
        let memory = uncasted_memory;
        let page_size = page_size();
        let new_size_aligned = mem_align_forward(new_len, page_size);

        let page_aligned_len = mem_align_forward(memory.len(), page_size);
        if new_size_aligned == page_aligned_len {
            return Some(memory.as_mut_ptr());
        }

        // PORT NOTE: `std.posix.MREMAP` is `void` on platforms without `mremap`
        // (notably Darwin/macOS, this module's target). The reference guards the
        // `mremap` fast path on `MREMAP != void`; on Darwin that branch is dead,
        // so it is reproduced here as a `cfg(target_os = "linux")` path (Linux
        // exposes `mremap`).
        #[cfg(target_os = "linux")]
        {
            // TODO: if the next_mmap_addr_hint is within the remapped range, update it
            let new_memory = unsafe {
                libc::mremap(
                    memory.as_mut_ptr().cast(),
                    memory.len(),
                    new_len,
                    if may_move { libc::MREMAP_MAYMOVE } else { 0 },
                )
            };
            if new_memory == libc::MAP_FAILED {
                return None;
            }
            // PORT NOTE: the tail expression of the block, not a `return` — on
            // Linux the `cfg(not(linux))` block below is stripped, so this block
            // IS the function's tail.
            Some(new_memory.cast())
        }

        #[cfg(not(target_os = "linux"))]
        {
            let _ = may_move;
            if new_size_aligned < page_aligned_len {
                let ptr = unsafe { memory.as_mut_ptr().add(new_size_aligned) };
                // TODO: if the next_mmap_addr_hint is within the unmapped range, update it
                unsafe { libc::munmap(ptr.cast(), page_aligned_len - new_size_aligned) };
                return Some(memory.as_mut_ptr());
            }

            None
        }
    }
}

// PORT NOTE: helpers reproducing the `std.heap`/`std.mem` primitives the reference
// uses: `std.heap.pageSize()`, `mem.alignForward`, and `mem.alignPointer`.
fn page_size() -> usize {
    // std.heap.pageSize()
    unsafe { libc::sysconf(libc::_SC_PAGESIZE) as usize }
}

fn mem_align_forward(addr: usize, alignment: usize) -> usize {
    // mem.alignForward(usize, addr, alignment)
    (addr + alignment - 1) & !(alignment - 1)
}

fn align_pointer(ptr: *mut u8, align: usize) -> Option<*mut u8> {
    // mem.alignPointer(ptr, align)
    let addr = ptr as usize;
    let aligned = mem_align_forward(addr, align);
    // alignPointer returns null on overflow.
    if aligned < addr {
        return None;
    }
    Some(aligned as *mut u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn VMTag_make() {
        // PORT NOTE: the reference computes the expected value via
        // `@as(i32, @bitCast(@as(u32, 240) << 24))` — i.e. VM_MAKE_TAG(240).
        // To keep this an INDEPENDENT oracle (not re-deriving it from `make`),
        // the expected value is the literal result of that bit math, computed by
        // an independent Zig probe: 240u32 << 24 = 0xF000_0000 = 4026531840,
        // bit-cast to i32 = -268435456.
        assert_eq!(-268435456_i32, VMTag::application_specific_1.make());
    }
}
