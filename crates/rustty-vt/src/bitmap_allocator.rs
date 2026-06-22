//! rustty-vt bitmap_allocator
//!
//! PORTS: ghostty src/terminal/bitmap_allocator.zig
//!
//! PORT STATUS: done

// PORT NOTE: the reference imports are
//   const std = @import("std");
//   const assert = @import("../quirks.zig").inlineAssert;
//   const Allocator = std.mem.Allocator;
//   const size = @import("size.zig");
//   const getOffset = size.getOffset;
//   const Offset = size.Offset;
//   const OffsetBuf = size.OffsetBuf;
//   const alignForward = std.mem.alignForward;
// `getOffset` is unused in the runtime port (the reference also only mentions it
// in a doc comment), the rest map to the hoisted size types in
// rustty-render-types (A1) plus the rustty-util `inlineAssert` quirk.
//
// PORT NOTE: the reference is `pub fn BitmapAllocator(comptime chunk_size:
// comptime_int) type` — a generic over an integer chunk size. In Rust this is a
// zero-sized struct parameterized by a `const CHUNK_SIZE: usize` const generic.
// All `comptime`/`@This()`-relative declarations become associated consts/fns on
// `BitmapAllocator<CHUNK_SIZE>`.
//
// PORT NOTE: the reference `base: anytype` parameters are the offset "true base"
// pointer (an `IntFromBase`). We thread a `*mut u8` base through, matching the
// render-types `Offset::ptr` contract. The methods that dereference offset
// pointers are `unsafe` because the reference relies on the same raw-pointer
// soundness invariants (valid, properly-aligned backing buffer).

// PARITY: the reference uses `x % alignment == 0` modulo checks, `for (slice,
// 0..) |*el, idx|` indexed loops, and `idx == null` comparisons verbatim. These
// are kept char-for-char rather than rewritten to `is_multiple_of` /
// iterator-with-index / `.is_none()` so the ported expressions match the
// reference; the corresponding clippy lints are allowed module-wide.
#![allow(clippy::manual_is_multiple_of)]
#![allow(clippy::needless_range_loop)]
#![allow(clippy::partialeq_to_none)]

use rustty_render_types::term_size::{IntFromBase, Offset, OffsetBuf};
use rustty_util::quirks::inlineAssert as assert;

/// A relatively naive bitmap allocator that uses memory offsets against
/// a fixed backing buffer so that the backing buffer can be easily moved
/// without having to update pointers.
///
/// The chunk size determines the size of each chunk in bytes. This is the
/// minimum distributed unit of memory. For example, if you request a
/// 1-byte allocation, you'll use a chunk of chunk_size bytes. Likewise,
/// if your chunk size is 4, and you request a 5-byte allocation, you'll
/// use 2 chunks.
///
/// The allocator is susceptible to fragmentation. If you allocate and free
/// memory in a way that leaves small holes in the memory, you may not be
/// able to allocate large chunks of memory even if there is enough free
/// memory in aggregate. To avoid fragmentation, use a chunk size that is
/// large enough to cover most of your allocations.
///
// Notes for contributors: this is highly contributor friendly part of
// the code. If you can improve this, add tests, show benchmarks, then
// please do so!
// PORT NOTE: the reference `BitmapAllocator` is a plain struct of two `Offset`s
// plus a `usize`, so `var x = self.*` copies it trivially. We mirror that with
// `Clone`/`Copy` (all fields are `Copy`); needed by `Page` clone, which copies
// the whole struct (`var result = self.*`).
#[derive(Clone, Copy)]
pub struct BitmapAllocator<const CHUNK_SIZE: usize> {
    /// The bitmap of available chunks. Each bit represents a chunk. A
    /// 1 means the chunk is free and a 0 means it's used. We use 1
    /// for free since it makes it very slightly faster to find free
    /// chunks.
    pub bitmap: Offset<u64>,
    pub bitmap_count: usize,

    /// The contiguous buffer of chunks.
    pub chunks: Offset<u8>,
}

/// `OutOfMemory` error returned by `alloc`.
// PORT NOTE: the reference returns `Allocator.Error![]T`, whose only error
// variant relevant here is `error.OutOfMemory`. We model that with a dedicated
// zero-field error type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OutOfMemory;

impl<const CHUNK_SIZE: usize> BitmapAllocator<CHUNK_SIZE> {
    // comptime {
    //     assert(std.math.isPowerOfTwo(chunk_size));
    // }
    // PORT NOTE: the comptime power-of-two assertion is reproduced as a const
    // assertion that is evaluated when the type is instantiated.
    const _ASSERT_POW2: () = assert!(CHUNK_SIZE.is_power_of_two());

    /// `pub const base_align: std.mem.Alignment = .fromByteUnits(@alignOf(u64));`
    pub const BASE_ALIGN: usize = core::mem::align_of::<u64>();
    /// `pub const bitmap_bit_size = @bitSizeOf(u64);`
    pub const BITMAP_BIT_SIZE: usize = u64::BITS as usize;

    /// Initialize the allocator map with a given buf and memory layout.
    ///
    /// # Safety
    ///
    /// `buf` must point at a valid, `base_align`-aligned allocation of at least
    /// `l.total_size` bytes; the bitmap region must be writable.
    pub unsafe fn init(buf: OffsetBuf, l: Layout) -> Self {
        // Force the comptime power-of-two assertion for this instantiation.
        let () = Self::_ASSERT_POW2;
        // assert(base_align.check(@intFromPtr(buf.start())));
        assert(unsafe { buf.start() } as usize % Self::BASE_ALIGN == 0);

        // Initialize our bitmaps to all 1s to note that all chunks are free.
        let bitmap: Offset<u64> = buf.member(l.bitmap_start);
        let bitmap_ptr = unsafe { bitmap.ptr(buf) };
        // @memset(bitmap_ptr[0..l.bitmap_count], std.math.maxInt(u64));
        unsafe {
            core::slice::from_raw_parts_mut(bitmap_ptr, l.bitmap_count).fill(u64::MAX);
        }

        BitmapAllocator {
            bitmap,
            bitmap_count: l.bitmap_count,
            chunks: buf.member(l.chunks_start),
        }
    }

    /// Returns the number of bytes required to allocate n elements of
    /// type T. This accounts for the chunk size alignment used by the
    /// bitmap allocator.
    pub fn bytes_required<T>(n: usize) -> usize {
        let byte_count = core::mem::size_of::<T>() * n;
        align_forward(byte_count, CHUNK_SIZE)
    }

    /// Allocate n elements of type T. This will return error.OutOfMemory
    /// if there isn't enough space in the backing buffer.
    ///
    /// Use (size.zig).getOffset to get the base offset from the backing
    /// memory for portable storage.
    ///
    /// # Safety
    ///
    /// `base` must be the true base pointer of the backing buffer this
    /// allocator was initialized against. The returned slice borrows from that
    /// buffer for the caller-chosen lifetime.
    #[allow(clippy::missing_safety_doc)]
    pub unsafe fn alloc<'a, T, B: IntFromBase + Copy>(
        &mut self,
        base: B,
        n: usize,
    ) -> Result<&'a mut [T], OutOfMemory> {
        // note: we don't handle alignment yet, we just require that all
        // types are properly aligned. This is a limitation that should be
        // fixed but we haven't needed it. Contributor friendly: add tests
        // and fix this.
        assert(CHUNK_SIZE % core::mem::align_of::<T>() == 0);
        assert(n > 0);

        // const byte_count = std.math.mul(usize, @sizeOf(T), n) catch
        //     return error.OutOfMemory;
        let byte_count = match core::mem::size_of::<T>().checked_mul(n) {
            Some(v) => v,
            None => return Err(OutOfMemory),
        };
        // const chunk_count = std.math.divCeil(usize, byte_count, chunk_size) catch
        //     return error.OutOfMemory;
        let chunk_count = byte_count.div_ceil(CHUNK_SIZE);

        // Find the index of the free chunk. This also marks it as used.
        let bitmaps =
            unsafe { core::slice::from_raw_parts_mut(self.bitmap.ptr(base), self.bitmap_count) };
        let idx = match find_free_chunks(bitmaps, chunk_count) {
            Some(idx) => idx,
            None => return Err(OutOfMemory),
        };

        let chunks = unsafe { self.chunks.ptr(base) };
        // const ptr: [*]T = @ptrCast(@alignCast(&chunks[idx * chunk_size]));
        let ptr = unsafe { chunks.add(idx * CHUNK_SIZE) } as *mut T;
        // return ptr[0..n];
        Ok(unsafe { core::slice::from_raw_parts_mut(ptr, n) })
    }

    /// # Safety
    ///
    /// `base` must be the true base pointer of the backing buffer, and `slice`
    /// must be a slice previously returned by `alloc` from this allocator.
    pub unsafe fn free<T, B: IntFromBase + Copy>(&mut self, base: B, slice: &[T]) {
        // Convert the slice of whatever type to a slice of bytes. We
        // can then use the byte len and chunk size to determine the
        // number of chunks that were allocated.
        // const bytes = std.mem.sliceAsBytes(slice);
        let bytes_len = core::mem::size_of_val(slice);
        let aligned_len = align_forward(bytes_len, CHUNK_SIZE);
        // const chunk_count = @divExact(aligned_len, chunk_size);
        let chunk_count = aligned_len / CHUNK_SIZE;

        // From the pointer, we can calculate the exact index.
        let chunks = unsafe { self.chunks.ptr(base) };
        // const chunk_idx = @divExact(@intFromPtr(slice.ptr) - @intFromPtr(chunks), chunk_size);
        let chunk_idx = ((slice.as_ptr() as usize) - (chunks as usize)) / CHUNK_SIZE;

        let bitmaps =
            unsafe { core::slice::from_raw_parts_mut(self.bitmap.ptr(base), self.bitmap_count) };

        // Current bitmap index.
        let mut i: usize = chunk_idx / 64;
        // Number of chunks we still have to mark as free.
        let mut rem: usize = chunk_count;

        // Mark any bits in the starting bitmap that need to be marked.
        {
            // Bit index.
            let bit = chunk_idx % 64;
            // Number of bits we need to mark in this bitmap.
            let bits = core::cmp::min(rem, 64 - bit);

            // bitmaps[i] |= ~@as(u64, 0) >> @intCast(64 - bits) << @intCast(bit);
            bitmaps[i] |= ((!0u64) >> (64 - bits)) << bit;
            rem -= bits;
        }

        // Mark any full bitmaps worth of bits that need to be marked.
        i += 1;
        while rem > 64 {
            bitmaps[i] = u64::MAX;
            rem -= 64;
            i += 1;
        }

        // Mark any bits at the start of this last bitmap if it needs it.
        if rem > 0 {
            // bitmaps[i] |= ~@as(u64, 0) >> @intCast(64 - rem);
            bitmaps[i] |= (!0u64) >> (64 - rem);
        }
    }

    /// Returns the total capacity in bytes.
    pub fn capacity_bytes(&self) -> usize {
        self.bitmap_count * Self::BITMAP_BIT_SIZE * CHUNK_SIZE
    }

    /// Returns the number of bytes currently in use.
    ///
    /// # Safety
    ///
    /// `base` must be the true base pointer of the backing buffer.
    pub unsafe fn used_bytes<B: IntFromBase + Copy>(&self, base: B) -> usize {
        let bitmaps =
            unsafe { core::slice::from_raw_parts(self.bitmap.ptr(base), self.bitmap_count) };
        let mut free_chunks: usize = 0;
        for bitmap in bitmaps {
            free_chunks += bitmap.count_ones() as usize;
        }
        let total_chunks = self.bitmap_count * Self::BITMAP_BIT_SIZE;
        (total_chunks - free_chunks) * CHUNK_SIZE
    }

    /// For testing only.
    ///
    /// # Safety
    ///
    /// `base` must be the true base pointer of the backing buffer, and `slice`
    /// a slice previously returned by `alloc`.
    #[cfg(test)]
    unsafe fn is_allocated<T, B: IntFromBase + Copy>(&mut self, base: B, slice: &[T]) -> bool {
        // comptime assert(@import("builtin").is_test);

        let bytes_len = core::mem::size_of_val(slice);
        let aligned_len = align_forward(bytes_len, CHUNK_SIZE);
        let chunk_count = aligned_len / CHUNK_SIZE;

        let chunks = unsafe { self.chunks.ptr(base) };
        let chunk_idx = ((slice.as_ptr() as usize) - (chunks as usize)) / CHUNK_SIZE;

        let bitmaps =
            unsafe { core::slice::from_raw_parts(self.bitmap.ptr(base), self.bitmap_count) };

        for i in chunk_idx..chunk_idx + chunk_count {
            let bitmap = i / Self::BITMAP_BIT_SIZE;
            let bit = i % Self::BITMAP_BIT_SIZE;
            if bitmaps[bitmap] & (1u64 << bit) != 0 {
                return false;
            }
        }

        true
    }

    // PORT NOTE: `dumpBitmaps` is a debug-only logging helper (`std.log.warn`);
    // it is not exercised by any test and is omitted to avoid pulling in a
    // logging dependency. It is preserved here as the original commented body so
    // no content is lost:
    //
    // /// For debugging
    // fn dumpBitmaps(self: *Self, base: anytype) void {
    //     const bitmaps = self.bitmap.ptr(base);
    //     for (bitmaps[0..self.bitmap_count], 0..) |bitmap, idx| {
    //         std.log.warn("bm={b} idx={}", .{ bitmap, idx });
    //     }
    // }

    /// Get the layout for the given capacity. The capacity is in
    /// number of bytes, not chunks. The capacity will likely be
    /// rounded up to the nearest chunk size and bitmap size so
    /// everything is perfectly divisible.
    pub fn layout(cap: usize) -> Layout {
        // Force the comptime power-of-two assertion for this instantiation.
        let () = Self::_ASSERT_POW2;
        // Align the cap forward to our chunk size so we always have
        // a full chunk at the end.
        let aligned_cap = align_forward(cap, CHUNK_SIZE);

        // Calculate the number of bitmaps. We need 1 bitmap per 64 chunks.
        // We align the chunk count forward so our bitmaps are full so we
        // don't have to handle the case where we have a partial bitmap.
        // const chunk_count = @divExact(aligned_cap, chunk_size);
        let chunk_count = aligned_cap / CHUNK_SIZE;
        let aligned_chunk_count = align_forward(chunk_count, 64);
        // const bitmap_count = @divExact(aligned_chunk_count, 64);
        let bitmap_count = aligned_chunk_count / 64;

        let bitmap_start = 0;
        let bitmap_end = core::mem::size_of::<u64>() * bitmap_count;
        // const chunks_start = alignForward(usize, bitmap_end, @alignOf(u8));
        let chunks_start = align_forward(bitmap_end, core::mem::align_of::<u8>());
        let chunks_end = chunks_start + (aligned_cap * CHUNK_SIZE);
        let total_size = chunks_end;

        Layout {
            total_size,
            bitmap_count,
            bitmap_start,
            chunks_start,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layout {
    pub total_size: usize,
    pub bitmap_count: usize,
    pub bitmap_start: usize,
    pub chunks_start: usize,
}

// PORT NOTE: `std.mem.alignForward(usize, value, alignment)` rounds `value` up
// to the next multiple of `alignment` (which is a power of two here).
#[inline]
fn align_forward(value: usize, alignment: usize) -> usize {
    value.next_multiple_of(alignment)
}

/// Find `n` sequential free chunks in the given bitmaps and return the index
/// of the first chunk. If no chunks are found, return `null`. This also updates
/// the bitmap to mark the chunks as used.
fn find_free_chunks(bitmaps: &mut [u64], n: usize) -> Option<usize> {
    // NOTE: This is a naive implementation that just iterates through the
    // bitmaps. There is very likely a more efficient way to do this but
    // I'm not a bit twiddling expert. Perhaps even SIMD could be used here
    // but unsure. Contributor friendly: let's benchmark and improve this!

    // Large chunks require special handling.
    if n > u64::BITS as usize {
        let mut i: usize = 0;
        // search: while (i < bitmaps.len) {
        'search: while i < bitmaps.len() {
            // Number of chunks available at the end of this bitmap.
            // const prefix = @clz(~bitmaps[i]);
            let prefix = (!bitmaps[i]).leading_zeros() as usize;

            // If there are no chunks available at the end of this bitmap
            // then we can't start in it, so we'll try the next one.
            if prefix == 0 {
                i += 1;
                continue;
            }

            // Starting position if we manage to find the span we need here.
            let start_bitmap = i;
            let start_bit = 64 - prefix;

            // The remaining number of sequential free chunks we need to find.
            let mut rem: usize = n - prefix;

            i += 1;
            while rem > 64 {
                // We ran out of bitmaps, there's no sufficiently large gap.
                if i >= bitmaps.len() {
                    return None;
                }

                // There's more than 64 remaining chunks and this bitmap has
                // content in it, so we try starting again with this bitmap.
                if bitmaps[i] != u64::MAX {
                    continue 'search;
                }

                // This bitmap is completely free, we can subtract 64 from
                // our remaining number.
                rem -= 64;
                i += 1;
            }

            // If the number of available chunks at the start of this bitmap
            // is less than the remaining required, we have to try again.
            // if (@ctz(~bitmaps[i]) < rem) continue;
            if ((!bitmaps[i]).trailing_zeros() as usize) < rem {
                continue;
            }

            let suffix = (n - prefix) % 64;

            // Found! Mark everything between our start and end as full.
            // bitmaps[start_bitmap] ^= ~@as(u64, 0) >> @intCast(start_bit) << @intCast(start_bit);
            bitmaps[start_bitmap] ^= ((!0u64) >> start_bit) << start_bit;
            // const full_bitmaps = @divFloor(n - prefix - suffix, 64);
            let full_bitmaps = (n - prefix - suffix) / 64;
            // for (bitmaps[start_bitmap + 1 ..][0..full_bitmaps]) |*bitmap| {
            //     bitmap.* = 0;
            // }
            for bitmap in &mut bitmaps[start_bitmap + 1..][0..full_bitmaps] {
                *bitmap = 0;
            }
            // if (suffix > 0) bitmaps[i] ^= ~@as(u64, 0) >> @intCast(64 - suffix);
            if suffix > 0 {
                bitmaps[i] ^= (!0u64) >> (64 - suffix);
            }

            return Some(start_bitmap * 64 + start_bit);
        }

        return None;
    }

    assert(n <= u64::BITS as usize);
    // for (bitmaps, 0..) |*bitmap, idx| {
    for idx in 0..bitmaps.len() {
        // Shift the bitmap to find `n` sequential free chunks.
        // EXAMPLE:
        // n = 4
        // shifted = 001111001011110010
        //         & 000111100101111001
        //         & 000011110010111100
        //         & 000001111001011110
        //         = 000001000000010000
        //                ^       ^
        // In this example there are 2 places with at least 4 sequential 1s.
        let mut shifted: u64 = bitmaps[idx];
        // for (1..n) |i| shifted &= bitmap.* >> @intCast(i);
        for i in 1..n {
            shifted &= bitmaps[idx] >> i;
        }

        // If we have zero then we have no matches
        if shifted == 0 {
            continue;
        }

        // Trailing zeroes gets us the index of the first bit index with at
        // least `n` sequential 1s. In the example above, that would be `4`.
        let bit = shifted.trailing_zeros() as usize;

        // Calculate the mask so we can mark it as used
        // const mask = (@as(u64, std.math.maxInt(u64)) >> @intCast(64 - n)) << @intCast(bit);
        let mask = (u64::MAX >> (64 - n)) << bit;
        // bitmap.* ^= mask;
        bitmaps[idx] ^= mask;

        // return (idx * 64) + bit;
        return Some((idx * 64) + bit);
    }

    None
}

#[cfg(test)]
#[allow(non_snake_case)]
mod tests {
    use super::*;
    use std::alloc::{alloc, dealloc, Layout as StdLayout};

    // PORT NOTE: a test-only RAII helper for the reference's
    // `alloc.alignedAlloc(u8, base_align, total_size)` /
    // `defer alloc.free(buf)`. It owns a heap allocation of `total_size` bytes
    // aligned to `base_align` and frees it on drop. `as_offset_buf`/`base_ptr`
    // expose the buffer the way the offset-based structures expect.
    struct AlignedBuf {
        ptr: *mut u8,
        layout: StdLayout,
    }

    impl AlignedBuf {
        fn new(total_size: usize, base_align: usize) -> AlignedBuf {
            let layout = StdLayout::from_size_align(total_size, base_align).unwrap();
            let ptr = unsafe { alloc(layout) };
            assert!(!ptr.is_null());
            AlignedBuf { ptr, layout }
        }

        fn base(&self) -> *mut u8 {
            self.ptr
        }

        fn offset_buf(&self) -> OffsetBuf {
            OffsetBuf::init(self.ptr as *const u8)
        }
    }

    impl Drop for AlignedBuf {
        fn drop(&mut self) {
            unsafe { dealloc(self.ptr, self.layout) };
        }
    }

    #[test]
    fn findFreeChunks_single_found() {
        let mut bitmaps: [u64; 1] =
            [0b10000000_00000000_00000000_00000000_00000000_00000000_00001110_00000000];
        let idx = find_free_chunks(&mut bitmaps, 2).unwrap();
        assert_eq!(9usize, idx);
        assert_eq!(
            0b10000000_00000000_00000000_00000000_00000000_00000000_00001000_00000000,
            bitmaps[0],
        );
    }

    #[test]
    fn findFreeChunks_single_not_found() {
        let mut bitmaps: [u64; 1] =
            [0b10000111_00000000_00000000_00000000_00000000_00000000_00000000_00000000];
        let idx = find_free_chunks(&mut bitmaps, 4);
        assert!(idx == None);
    }

    #[test]
    fn findFreeChunks_multiple_found() {
        let mut bitmaps: [u64; 2] = [
            0b10000111_00000000_00000000_00000000_00000000_00000000_00000000_01110000,
            0b10000000_00111110_00000000_00000000_00000000_00000000_00111110_00000000,
        ];
        let idx = find_free_chunks(&mut bitmaps, 4).unwrap();
        assert_eq!(73usize, idx);
        assert_eq!(
            0b10000000_00111110_00000000_00000000_00000000_00000000_00100000_00000000,
            bitmaps[1],
        );
    }

    #[test]
    fn findFreeChunks_exactly_64_chunks() {
        let mut bitmaps: [u64; 1] =
            [0b11111111_11111111_11111111_11111111_11111111_11111111_11111111_11111111];
        let idx = find_free_chunks(&mut bitmaps, 64).unwrap();
        assert_eq!(
            0b00000000_00000000_00000000_00000000_00000000_00000000_00000000_00000000,
            bitmaps[0],
        );
        assert_eq!(0usize, idx);
    }

    #[test]
    fn findFreeChunks_larger_than_64_chunks() {
        let mut bitmaps: [u64; 2] = [
            0b11111111_11111111_11111111_11111111_11111111_11111111_11111111_11111111,
            0b11111111_11111111_11111111_11111111_11111111_11111111_11111111_11111111,
        ];
        let idx = find_free_chunks(&mut bitmaps, 65).unwrap();
        assert_eq!(
            0b00000000_00000000_00000000_00000000_00000000_00000000_00000000_00000000,
            bitmaps[0],
        );
        assert_eq!(
            0b11111111_11111111_11111111_11111111_11111111_11111111_11111111_11111110,
            bitmaps[1],
        );
        assert_eq!(0usize, idx);
    }

    #[test]
    fn findFreeChunks_larger_than_64_chunks_not_at_beginning() {
        let mut bitmaps: [u64; 3] = [
            0b11111111_00000000_00000000_00000000_00000000_00000000_00000000_00000000,
            0b11111111_11111111_11111111_11111111_11111111_11111111_11111111_11111111,
            0b11111111_11111111_11111111_11111111_11111111_11111111_11111111_11111111,
        ];
        let idx = find_free_chunks(&mut bitmaps, 65).unwrap();
        assert_eq!(
            0b00000000_00000000_00000000_00000000_00000000_00000000_00000000_00000000,
            bitmaps[0],
        );
        assert_eq!(
            0b11111110_00000000_00000000_00000000_00000000_00000000_00000000_00000000,
            bitmaps[1],
        );
        assert_eq!(
            0b11111111_11111111_11111111_11111111_11111111_11111111_11111111_11111111,
            bitmaps[2],
        );
        assert_eq!(56usize, idx);
    }

    #[test]
    fn findFreeChunks_larger_than_64_chunks_exact() {
        let mut bitmaps: [u64; 2] = [
            0b11111111_11111111_11111111_11111111_11111111_11111111_11111111_11111111,
            0b11111111_11111111_11111111_11111111_11111111_11111111_11111111_11111111,
        ];
        let idx = find_free_chunks(&mut bitmaps, 128).unwrap();
        assert_eq!(
            0b00000000_00000000_00000000_00000000_00000000_00000000_00000000_00000000,
            bitmaps[0],
        );
        assert_eq!(
            0b00000000_00000000_00000000_00000000_00000000_00000000_00000000_00000000,
            bitmaps[1],
        );
        assert_eq!(0usize, idx);
    }

    #[test]
    fn BitmapAllocator_layout() {
        type Alloc = BitmapAllocator<4>;
        let cap = 64 * 4;

        let layout = Alloc::layout(cap);

        // We expect to use one bitmap since the cap is bytes.
        assert_eq!(1usize, layout.bitmap_count);
    }

    #[test]
    fn BitmapAllocator_alloc_sequentially() {
        type Alloc = BitmapAllocator<4>;
        let cap = 64;

        let layout = Alloc::layout(cap);
        let buf = AlignedBuf::new(layout.total_size, Alloc::BASE_ALIGN);

        let mut bm = unsafe { Alloc::init(buf.offset_buf(), layout) };
        let ptr = unsafe { bm.alloc::<u8, _>(buf.base(), 1) }.unwrap();
        ptr[0] = b'A';
        let ptr_addr = ptr.as_ptr() as usize;

        let ptr2 = unsafe { bm.alloc::<u8, _>(buf.base(), 1) }.unwrap();
        let ptr2_addr = ptr2.as_ptr() as usize;
        assert!(ptr_addr != ptr2_addr);

        // Should grab the next chunk
        assert_eq!(ptr_addr + 4, ptr2_addr);

        // Free ptr and next allocation should be back
        let ptr_slice: &[u8] = unsafe { core::slice::from_raw_parts(ptr_addr as *const u8, 1) };
        unsafe { bm.free(buf.base(), ptr_slice) };
        let ptr3 = unsafe { bm.alloc::<u8, _>(buf.base(), 1) }.unwrap();
        assert_eq!(ptr_addr, ptr3.as_ptr() as usize);
    }

    #[test]
    fn BitmapAllocator_alloc_non_byte() {
        type Alloc = BitmapAllocator<4>;
        let cap = 128;

        let layout = Alloc::layout(cap);
        let buf = AlignedBuf::new(layout.total_size, Alloc::BASE_ALIGN);

        let mut bm = unsafe { Alloc::init(buf.offset_buf(), layout) };
        // PORT NOTE: the reference uses `u21` (a 3-byte-value, 4-byte-aligned Zig
        // integer). Rust has no u21; `u32` has the same 4-byte size/alignment that
        // every assertion in this test depends on (chunk arithmetic), so it is the
        // faithful stand-in.
        let ptr = unsafe { bm.alloc::<u32, _>(buf.base(), 1) }.unwrap();
        ptr[0] = b'A' as u32;
        let ptr_addr = ptr.as_ptr() as usize;

        let ptr2 = unsafe { bm.alloc::<u32, _>(buf.base(), 1) }.unwrap();
        let ptr2_addr = ptr2.as_ptr() as usize;
        assert!(ptr_addr != ptr2_addr);
        assert_eq!(ptr_addr + 4, ptr2_addr);

        // Free ptr and next allocation should be back
        let ptr_slice: &[u32] = unsafe { core::slice::from_raw_parts(ptr_addr as *const u32, 1) };
        unsafe { bm.free(buf.base(), ptr_slice) };
        let ptr3 = unsafe { bm.alloc::<u32, _>(buf.base(), 1) }.unwrap();
        assert_eq!(ptr_addr, ptr3.as_ptr() as usize);
    }

    #[test]
    fn BitmapAllocator_alloc_non_byte_multi_chunk() {
        // const Alloc = BitmapAllocator(4 * @sizeOf(u21));
        // PORT NOTE: `@sizeOf(u21) == 4`, so the chunk size is `4 * 4 == 16`. We
        // use `u32` (size 4) as the u21 stand-in per the note above; the chunk
        // arithmetic (`@sizeOf(u21) * 4 * 2`) is reproduced with `size_of::<u32>()`.
        type Alloc = BitmapAllocator<{ 4 * core::mem::size_of::<u32>() }>;
        let cap = 128;

        let layout = Alloc::layout(cap);
        let buf = AlignedBuf::new(layout.total_size, Alloc::BASE_ALIGN);

        let mut bm = unsafe { Alloc::init(buf.offset_buf(), layout) };
        let ptr = unsafe { bm.alloc::<u32, _>(buf.base(), 6) }.unwrap();
        assert_eq!(6usize, ptr.len());
        for v in ptr.iter_mut() {
            *v = b'A' as u32;
        }
        let ptr_addr = ptr.as_ptr() as usize;

        let ptr2 = unsafe { bm.alloc::<u32, _>(buf.base(), 1) }.unwrap();
        let ptr2_addr = ptr2.as_ptr() as usize;
        assert!(ptr_addr != ptr2_addr);
        assert_eq!(ptr_addr + (core::mem::size_of::<u32>() * 4 * 2), ptr2_addr);

        // Free ptr and next allocation should be back
        let ptr_slice: &[u32] = unsafe { core::slice::from_raw_parts(ptr_addr as *const u32, 6) };
        unsafe { bm.free(buf.base(), ptr_slice) };
        let ptr3 = unsafe { bm.alloc::<u32, _>(buf.base(), 1) }.unwrap();
        assert_eq!(ptr_addr, ptr3.as_ptr() as usize);
    }

    #[test]
    fn BitmapAllocator_alloc_large() {
        type Alloc = BitmapAllocator<2>;
        let cap = 256;

        let layout = Alloc::layout(cap);
        let buf = AlignedBuf::new(layout.total_size, Alloc::BASE_ALIGN);

        let mut bm = unsafe { Alloc::init(buf.offset_buf(), layout) };
        let ptr = unsafe { bm.alloc::<u8, _>(buf.base(), 129) }.unwrap();
        ptr[0] = b'A';
        let ptr_addr = ptr.as_ptr() as usize;
        let ptr_slice: &[u8] = unsafe { core::slice::from_raw_parts(ptr_addr as *const u8, 129) };
        unsafe { bm.free(buf.base(), ptr_slice) };
    }

    #[test]
    fn BitmapAllocator_alloc_and_free_one_bitmap() {
        type Alloc = BitmapAllocator<1>;
        // Capacity such that we'll have 3 bitmaps.
        let cap = Alloc::BITMAP_BIT_SIZE * 3;

        let layout = Alloc::layout(cap);
        let buf = AlignedBuf::new(layout.total_size, Alloc::BASE_ALIGN);

        let mut bm = unsafe { Alloc::init(buf.offset_buf(), layout) };

        // Allocate exactly one bitmap worth of bytes.
        let slice = unsafe { bm.alloc::<u8, _>(buf.base(), Alloc::BITMAP_BIT_SIZE) }.unwrap();
        assert_eq!(Alloc::BITMAP_BIT_SIZE, slice.len());
        let slice_addr = slice.as_ptr() as usize;
        let slice_len = slice.len();

        slice.fill(0x11);
        assert_eq!(&[0x11u8; 64][..], slice);

        // Free it
        let slice_ref: &[u8] =
            unsafe { core::slice::from_raw_parts(slice_addr as *const u8, slice_len) };
        assert!(unsafe { bm.is_allocated(buf.base(), slice_ref) });
        unsafe { bm.free(buf.base(), slice_ref) };
        assert!(!unsafe { bm.is_allocated(buf.base(), slice_ref) });

        // All of our bitmaps should be free.
        let bitmaps: &[u64] = unsafe { core::slice::from_raw_parts(bm.bitmap.ptr(buf.base()), 3) };
        assert_eq!(&[!0u64; 3][..], bitmaps);
    }

    #[test]
    fn BitmapAllocator_alloc_and_free_half_bitmap() {
        type Alloc = BitmapAllocator<1>;
        // Capacity such that we'll have 3 bitmaps.
        let cap = Alloc::BITMAP_BIT_SIZE * 3;

        let layout = Alloc::layout(cap);
        let buf = AlignedBuf::new(layout.total_size, Alloc::BASE_ALIGN);

        let mut bm = unsafe { Alloc::init(buf.offset_buf(), layout) };

        // Allocate exactly half a bitmap worth of bytes.
        let slice = unsafe { bm.alloc::<u8, _>(buf.base(), Alloc::BITMAP_BIT_SIZE / 2) }.unwrap();
        assert_eq!(Alloc::BITMAP_BIT_SIZE / 2, slice.len());
        let slice_addr = slice.as_ptr() as usize;
        let slice_len = slice.len();

        slice.fill(0x11);
        assert_eq!(&[0x11u8; 32][..], slice);

        // Free it
        let slice_ref: &[u8] =
            unsafe { core::slice::from_raw_parts(slice_addr as *const u8, slice_len) };
        assert!(unsafe { bm.is_allocated(buf.base(), slice_ref) });
        unsafe { bm.free(buf.base(), slice_ref) };
        assert!(!unsafe { bm.is_allocated(buf.base(), slice_ref) });

        // All of our bitmaps should be free.
        let bitmaps: &[u64] = unsafe { core::slice::from_raw_parts(bm.bitmap.ptr(buf.base()), 3) };
        assert_eq!(&[!0u64; 3][..], bitmaps);
    }

    #[test]
    fn BitmapAllocator_alloc_and_free_two_half_bitmaps() {
        type Alloc = BitmapAllocator<1>;
        // Capacity such that we'll have 3 bitmaps.
        let cap = Alloc::BITMAP_BIT_SIZE * 3;

        let layout = Alloc::layout(cap);
        let buf = AlignedBuf::new(layout.total_size, Alloc::BASE_ALIGN);

        let mut bm = unsafe { Alloc::init(buf.offset_buf(), layout) };

        // Allocate exactly one bitmap worth of bytes across two allocations.
        let slice = unsafe { bm.alloc::<u8, _>(buf.base(), Alloc::BITMAP_BIT_SIZE / 2) }.unwrap();
        assert_eq!(Alloc::BITMAP_BIT_SIZE / 2, slice.len());
        let slice_addr = slice.as_ptr() as usize;
        let slice_len = slice.len();

        slice.fill(0x11);
        assert_eq!(&[0x11u8; 32][..], slice);

        let slice2 = unsafe { bm.alloc::<u8, _>(buf.base(), Alloc::BITMAP_BIT_SIZE / 2) }.unwrap();
        assert_eq!(Alloc::BITMAP_BIT_SIZE / 2, slice2.len());
        let slice2_addr = slice2.as_ptr() as usize;
        let slice2_len = slice2.len();

        slice2.fill(0x22);
        assert_eq!(&[0x22u8; 32][..], slice2);
        let slice_ref: &[u8] =
            unsafe { core::slice::from_raw_parts(slice_addr as *const u8, slice_len) };
        assert_eq!(&[0x11u8; 32][..], slice_ref);

        // Free them
        let slice2_ref: &[u8] =
            unsafe { core::slice::from_raw_parts(slice2_addr as *const u8, slice2_len) };
        assert!(unsafe { bm.is_allocated(buf.base(), slice2_ref) });
        unsafe { bm.free(buf.base(), slice2_ref) };
        assert!(!unsafe { bm.is_allocated(buf.base(), slice2_ref) });
        assert!(unsafe { bm.is_allocated(buf.base(), slice_ref) });
        unsafe { bm.free(buf.base(), slice_ref) };
        assert!(!unsafe { bm.is_allocated(buf.base(), slice_ref) });

        // All of our bitmaps should be free.
        let bitmaps: &[u64] = unsafe { core::slice::from_raw_parts(bm.bitmap.ptr(buf.base()), 3) };
        assert_eq!(&[!0u64; 3][..], bitmaps);
    }

    #[test]
    fn BitmapAllocator_alloc_and_free_1_5_bitmaps() {
        type Alloc = BitmapAllocator<1>;
        // Capacity such that we'll have 3 bitmaps.
        let cap = Alloc::BITMAP_BIT_SIZE * 3;

        let layout = Alloc::layout(cap);
        let buf = AlignedBuf::new(layout.total_size, Alloc::BASE_ALIGN);

        let mut bm = unsafe { Alloc::init(buf.offset_buf(), layout) };

        // Allocate exactly 1.5 bitmaps worth of bytes.
        let slice =
            unsafe { bm.alloc::<u8, _>(buf.base(), 3 * Alloc::BITMAP_BIT_SIZE / 2) }.unwrap();
        assert_eq!(3 * Alloc::BITMAP_BIT_SIZE / 2, slice.len());
        let slice_addr = slice.as_ptr() as usize;
        let slice_len = slice.len();

        slice.fill(0x11);
        assert_eq!(&[0x11u8; 96][..], slice);

        // Free them
        let slice_ref: &[u8] =
            unsafe { core::slice::from_raw_parts(slice_addr as *const u8, slice_len) };
        assert!(unsafe { bm.is_allocated(buf.base(), slice_ref) });
        unsafe { bm.free(buf.base(), slice_ref) };
        assert!(!unsafe { bm.is_allocated(buf.base(), slice_ref) });

        // All of our bitmaps should be free.
        let bitmaps: &[u64] = unsafe { core::slice::from_raw_parts(bm.bitmap.ptr(buf.base()), 3) };
        assert_eq!(&[!0u64; 3][..], bitmaps);
    }

    #[test]
    fn BitmapAllocator_alloc_and_free_two_1_5_bitmaps() {
        type Alloc = BitmapAllocator<1>;
        // Capacity such that we'll have 3 bitmaps.
        let cap = Alloc::BITMAP_BIT_SIZE * 3;

        let layout = Alloc::layout(cap);
        let buf = AlignedBuf::new(layout.total_size, Alloc::BASE_ALIGN);

        let mut bm = unsafe { Alloc::init(buf.offset_buf(), layout) };

        // Allocate exactly 3 bitmaps worth of bytes across two allocations.
        let slice =
            unsafe { bm.alloc::<u8, _>(buf.base(), 3 * Alloc::BITMAP_BIT_SIZE / 2) }.unwrap();
        assert_eq!(3 * Alloc::BITMAP_BIT_SIZE / 2, slice.len());
        let slice_addr = slice.as_ptr() as usize;
        let slice_len = slice.len();

        slice.fill(0x11);
        assert_eq!(&[0x11u8; 96][..], slice);

        let slice2 =
            unsafe { bm.alloc::<u8, _>(buf.base(), 3 * Alloc::BITMAP_BIT_SIZE / 2) }.unwrap();
        assert_eq!(3 * Alloc::BITMAP_BIT_SIZE / 2, slice2.len());
        let slice2_addr = slice2.as_ptr() as usize;
        let slice2_len = slice2.len();

        slice2.fill(0x22);
        assert_eq!(&[0x22u8; 96][..], slice2);
        let slice_ref: &[u8] =
            unsafe { core::slice::from_raw_parts(slice_addr as *const u8, slice_len) };
        assert_eq!(&[0x11u8; 96][..], slice_ref);

        // Free them
        let slice2_ref: &[u8] =
            unsafe { core::slice::from_raw_parts(slice2_addr as *const u8, slice2_len) };
        assert!(unsafe { bm.is_allocated(buf.base(), slice2_ref) });
        unsafe { bm.free(buf.base(), slice2_ref) };
        assert!(!unsafe { bm.is_allocated(buf.base(), slice2_ref) });
        assert!(unsafe { bm.is_allocated(buf.base(), slice_ref) });
        unsafe { bm.free(buf.base(), slice_ref) };
        assert!(!unsafe { bm.is_allocated(buf.base(), slice_ref) });

        // All of our bitmaps should be free.
        let bitmaps: &[u64] = unsafe { core::slice::from_raw_parts(bm.bitmap.ptr(buf.base()), 3) };
        assert_eq!(&[!0u64; 3][..], bitmaps);
    }

    #[test]
    fn BitmapAllocator_alloc_and_free_1_5_bitmaps_offset_by_0_75() {
        type Alloc = BitmapAllocator<1>;
        // Capacity such that we'll have 3 bitmaps.
        let cap = Alloc::BITMAP_BIT_SIZE * 3;

        let layout = Alloc::layout(cap);
        let buf = AlignedBuf::new(layout.total_size, Alloc::BASE_ALIGN);

        let mut bm = unsafe { Alloc::init(buf.offset_buf(), layout) };

        // Allocate three quarters of a bitmap first.
        let slice =
            unsafe { bm.alloc::<u8, _>(buf.base(), 3 * Alloc::BITMAP_BIT_SIZE / 4) }.unwrap();
        assert_eq!(3 * Alloc::BITMAP_BIT_SIZE / 4, slice.len());
        let slice_addr = slice.as_ptr() as usize;
        let slice_len = slice.len();

        slice.fill(0x11);
        assert_eq!(&[0x11u8; 48][..], slice);

        // Then a 1.5 bitmap sized allocation, so that it spans
        // from 0.75 to 2.25, occupying bits in 3 different bitmaps.
        let slice2 =
            unsafe { bm.alloc::<u8, _>(buf.base(), 3 * Alloc::BITMAP_BIT_SIZE / 2) }.unwrap();
        assert_eq!(3 * Alloc::BITMAP_BIT_SIZE / 2, slice2.len());
        let slice2_addr = slice2.as_ptr() as usize;
        let slice2_len = slice2.len();

        slice2.fill(0x22);
        assert_eq!(&[0x22u8; 96][..], slice2);
        let slice_ref: &[u8] =
            unsafe { core::slice::from_raw_parts(slice_addr as *const u8, slice_len) };
        assert_eq!(&[0x11u8; 48][..], slice_ref);

        // Free them
        let slice2_ref: &[u8] =
            unsafe { core::slice::from_raw_parts(slice2_addr as *const u8, slice2_len) };
        assert!(unsafe { bm.is_allocated(buf.base(), slice2_ref) });
        unsafe { bm.free(buf.base(), slice2_ref) };
        assert!(!unsafe { bm.is_allocated(buf.base(), slice2_ref) });
        assert!(unsafe { bm.is_allocated(buf.base(), slice_ref) });
        unsafe { bm.free(buf.base(), slice_ref) };
        assert!(!unsafe { bm.is_allocated(buf.base(), slice_ref) });

        // All of our bitmaps should be free.
        let bitmaps: &[u64] = unsafe { core::slice::from_raw_parts(bm.bitmap.ptr(buf.base()), 3) };
        assert_eq!(&[!0u64; 3][..], bitmaps);
    }

    #[test]
    fn BitmapAllocator_alloc_and_free_three_0_75_bitmaps() {
        type Alloc = BitmapAllocator<1>;
        // Capacity such that we'll have 3 bitmaps.
        let cap = Alloc::BITMAP_BIT_SIZE * 3;

        let layout = Alloc::layout(cap);
        let buf = AlignedBuf::new(layout.total_size, Alloc::BASE_ALIGN);

        let mut bm = unsafe { Alloc::init(buf.offset_buf(), layout) };

        // Allocate three quarters of a bitmap three times.
        let slice =
            unsafe { bm.alloc::<u8, _>(buf.base(), 3 * Alloc::BITMAP_BIT_SIZE / 4) }.unwrap();
        assert_eq!(3 * Alloc::BITMAP_BIT_SIZE / 4, slice.len());
        let slice_addr = slice.as_ptr() as usize;
        let slice_len = slice.len();

        slice.fill(0x11);
        assert_eq!(&[0x11u8; 48][..], slice);

        let slice2 =
            unsafe { bm.alloc::<u8, _>(buf.base(), 3 * Alloc::BITMAP_BIT_SIZE / 4) }.unwrap();
        assert_eq!(3 * Alloc::BITMAP_BIT_SIZE / 4, slice2.len());
        let slice2_addr = slice2.as_ptr() as usize;
        let slice2_len = slice2.len();

        slice2.fill(0x22);
        assert_eq!(&[0x22u8; 48][..], slice2);
        let slice_ref: &[u8] =
            unsafe { core::slice::from_raw_parts(slice_addr as *const u8, slice_len) };
        assert_eq!(&[0x11u8; 48][..], slice_ref);

        let slice3 =
            unsafe { bm.alloc::<u8, _>(buf.base(), 3 * Alloc::BITMAP_BIT_SIZE / 4) }.unwrap();
        assert_eq!(3 * Alloc::BITMAP_BIT_SIZE / 4, slice3.len());
        let slice3_addr = slice3.as_ptr() as usize;
        let slice3_len = slice3.len();

        slice3.fill(0x33);
        assert_eq!(&[0x33u8; 48][..], slice3);
        let slice2_ref: &[u8] =
            unsafe { core::slice::from_raw_parts(slice2_addr as *const u8, slice2_len) };
        assert_eq!(&[0x22u8; 48][..], slice2_ref);
        assert_eq!(&[0x11u8; 48][..], slice_ref);

        // Free them
        assert!(unsafe { bm.is_allocated(buf.base(), slice2_ref) });
        unsafe { bm.free(buf.base(), slice2_ref) };
        assert!(!unsafe { bm.is_allocated(buf.base(), slice2_ref) });
        assert!(unsafe { bm.is_allocated(buf.base(), slice_ref) });
        unsafe { bm.free(buf.base(), slice_ref) };
        assert!(!unsafe { bm.is_allocated(buf.base(), slice_ref) });
        let slice3_ref: &[u8] =
            unsafe { core::slice::from_raw_parts(slice3_addr as *const u8, slice3_len) };
        assert!(unsafe { bm.is_allocated(buf.base(), slice3_ref) });
        unsafe { bm.free(buf.base(), slice3_ref) };
        assert!(!unsafe { bm.is_allocated(buf.base(), slice3_ref) });

        // All of our bitmaps should be free.
        let bitmaps: &[u64] = unsafe { core::slice::from_raw_parts(bm.bitmap.ptr(buf.base()), 3) };
        assert_eq!(&[!0u64; 3][..], bitmaps);
    }

    #[test]
    fn BitmapAllocator_alloc_and_free_two_1_5_bitmaps_offset_0_75() {
        type Alloc = BitmapAllocator<1>;
        // Capacity such that we'll have 4 bitmaps.
        let cap = Alloc::BITMAP_BIT_SIZE * 4;

        let layout = Alloc::layout(cap);
        let buf = AlignedBuf::new(layout.total_size, Alloc::BASE_ALIGN);

        let mut bm = unsafe { Alloc::init(buf.offset_buf(), layout) };

        // First allocate a 0.75 bitmap
        let slice =
            unsafe { bm.alloc::<u8, _>(buf.base(), 3 * Alloc::BITMAP_BIT_SIZE / 4) }.unwrap();
        assert_eq!(3 * Alloc::BITMAP_BIT_SIZE / 4, slice.len());
        let slice_addr = slice.as_ptr() as usize;
        let slice_len = slice.len();

        slice.fill(0x11);
        assert_eq!(&[0x11u8; 48][..], slice);

        // Then two 1.5 bitmaps
        let slice2 =
            unsafe { bm.alloc::<u8, _>(buf.base(), 3 * Alloc::BITMAP_BIT_SIZE / 2) }.unwrap();
        assert_eq!(3 * Alloc::BITMAP_BIT_SIZE / 2, slice2.len());
        let slice2_addr = slice2.as_ptr() as usize;
        let slice2_len = slice2.len();

        slice2.fill(0x22);
        assert_eq!(&[0x22u8; 96][..], slice2);
        let slice_ref: &[u8] =
            unsafe { core::slice::from_raw_parts(slice_addr as *const u8, slice_len) };
        assert_eq!(&[0x11u8; 48][..], slice_ref);

        let slice3 =
            unsafe { bm.alloc::<u8, _>(buf.base(), 3 * Alloc::BITMAP_BIT_SIZE / 2) }.unwrap();
        assert_eq!(3 * Alloc::BITMAP_BIT_SIZE / 2, slice3.len());
        let slice3_addr = slice3.as_ptr() as usize;
        let slice3_len = slice3.len();

        slice3.fill(0x33);
        assert_eq!(&[0x33u8; 96][..], slice3);
        let slice2_ref: &[u8] =
            unsafe { core::slice::from_raw_parts(slice2_addr as *const u8, slice2_len) };
        assert_eq!(&[0x22u8; 96][..], slice2_ref);
        assert_eq!(&[0x11u8; 48][..], slice_ref);

        // Free them
        assert!(unsafe { bm.is_allocated(buf.base(), slice2_ref) });
        unsafe { bm.free(buf.base(), slice2_ref) };
        assert!(!unsafe { bm.is_allocated(buf.base(), slice2_ref) });
        assert!(unsafe { bm.is_allocated(buf.base(), slice_ref) });
        unsafe { bm.free(buf.base(), slice_ref) };
        assert!(!unsafe { bm.is_allocated(buf.base(), slice_ref) });
        let slice3_ref: &[u8] =
            unsafe { core::slice::from_raw_parts(slice3_addr as *const u8, slice3_len) };
        assert!(unsafe { bm.is_allocated(buf.base(), slice3_ref) });
        unsafe { bm.free(buf.base(), slice3_ref) };
        assert!(!unsafe { bm.is_allocated(buf.base(), slice3_ref) });

        // All of our bitmaps should be free.
        let bitmaps: &[u64] = unsafe { core::slice::from_raw_parts(bm.bitmap.ptr(buf.base()), 4) };
        assert_eq!(&[!0u64; 4][..], bitmaps);
    }

    #[test]
    fn BitmapAllocator_bytesRequired() {
        // Chunk size of 16 bytes (like grapheme_chunk in page.zig)
        {
            type Alloc = BitmapAllocator<16>;

            // Single byte rounds up to chunk size
            assert_eq!(16, Alloc::bytes_required::<u8>(1));
            assert_eq!(16, Alloc::bytes_required::<u8>(16));
            assert_eq!(32, Alloc::bytes_required::<u8>(17));

            // u21 (4 bytes each)
            // PORT NOTE: u21 stand-in is u32 (4 bytes), see notes above.
            assert_eq!(16, Alloc::bytes_required::<u32>(1)); // 4 bytes -> 16
            assert_eq!(16, Alloc::bytes_required::<u32>(4)); // 16 bytes -> 16
            assert_eq!(32, Alloc::bytes_required::<u32>(5)); // 20 bytes -> 32
            assert_eq!(32, Alloc::bytes_required::<u32>(6)); // 24 bytes -> 32
        }

        // Chunk size of 4 bytes
        {
            type Alloc = BitmapAllocator<4>;

            assert_eq!(4, Alloc::bytes_required::<u8>(1));
            assert_eq!(4, Alloc::bytes_required::<u8>(4));
            assert_eq!(8, Alloc::bytes_required::<u8>(5));

            // u32 (4 bytes each) - exactly one chunk per element
            assert_eq!(4, Alloc::bytes_required::<u32>(1));
            assert_eq!(8, Alloc::bytes_required::<u32>(2));
        }

        // Chunk size of 32 bytes (like string_chunk in page.zig)
        {
            type Alloc = BitmapAllocator<32>;

            assert_eq!(32, Alloc::bytes_required::<u8>(1));
            assert_eq!(32, Alloc::bytes_required::<u8>(32));
            assert_eq!(64, Alloc::bytes_required::<u8>(33));
        }
    }
}
