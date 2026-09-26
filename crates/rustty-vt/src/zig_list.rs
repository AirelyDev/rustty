//! Zig 0.16 container growth over allocator-bound vectors.
//!
//! PORT STATUS: Rust-only glue (R1 "no-loss, not no-addition"), parity tail
//! S7-6. The search subsystem's containers (and the highlight and formatter
//! lists it shares) are `allocator_api2` vectors bound to the runtime allocator
//! handle ([`AVec`]). allocator_api2's own growth is amortized doubling, which
//! is not what the caller's allocator sees in the reference. These helpers
//! reproduce the reference's calls instead:
//!
//! - `std.ArrayList` (Zig 0.16 `std/array_list.zig`): `ensureTotalCapacity`
//!   rounds up with `growCapacity` (`minimum +| (minimum / 2 +
//!   init_capacity)`, `init_capacity = max(1, cache_line / @sizeOf(T))`), and
//!   `ensureTotalCapacityPrecise` tries `remap` first, then allocates, copies
//!   and frees. The handle's `grow` is exactly that sequence, and
//!   `try_reserve_exact` calls it with the exact new capacity.
//! - `std.MultiArrayList` (`std/multi_array_list.zig`): `setCapacity` always
//!   allocates, copies and frees (never `remap`); `clone` is
//!   `ensureTotalCapacity(len)` on an empty list.
//! - `std.Io.Writer.Allocating` (`std/Io/Writer.zig:2502-2725`): the buffer IS
//!   the allocation; a write that does not fit drains through
//!   `ensureUnusedCapacity(bytes.len + splat_len + 1)` (`ArrayList(u8)`'s
//!   `growCapacity`, `remap` first).
//!
//! PORT NOTE (T3): `init_capacity` is computed from the REFERENCE's element size
//! (`zig_size`), not Rust's, so the element counts the caller's allocator sees
//! are the reference's. Where the Rust element is larger than the Zig one (a
//! `Flattened` is 40 bytes against Zig's 32; a `highlight.Chunk` is 24 bytes in
//! an array of structs against 20 in Zig's struct of arrays), the byte sizes of
//! those calls differ by that ratio. Recorded under N325.

use crate::alloc::{AVec, OutOfMemory};
use allocator_api2::alloc::Allocator as ApiAllocator;
use core::alloc::Layout;
use core::ptr::NonNull;

/// `std.atomic.cache_line` for the targets the port builds (the value
/// `alloc::ArrayListUnmanaged` uses).
const CACHE_LINE: usize = if cfg!(any(
    target_arch = "x86_64",
    target_arch = "aarch64",
    target_arch = "powerpc64"
)) {
    128
} else {
    64
};

/// `init_capacity` of `ArrayList(T)` / `MultiArrayList(T)` for a Zig element
/// (or, for a `MultiArrayList`, largest field) of `zig_size` bytes.
pub(crate) const fn init_capacity(zig_size: usize) -> usize {
    let n = CACHE_LINE / if zig_size == 0 { 1 } else { zig_size };
    if n == 0 {
        1
    } else {
        n
    }
}

/// `growCapacity(minimum)`.
#[inline]
pub(crate) const fn grow_capacity(minimum: usize, init_capacity: usize) -> usize {
    minimum.saturating_add(minimum / 2 + init_capacity)
}

/// `try list.ensureTotalCapacityPrecise(gpa, n)`: `remap` first, then
/// alloc + copy + free (the handle's `grow`), to exactly `n`.
#[inline]
pub(crate) fn ensure_total_capacity_precise<T>(
    v: &mut AVec<T>,
    n: usize,
) -> Result<(), OutOfMemory> {
    if v.capacity() >= n {
        return Ok(());
    }
    v.try_reserve_exact(n - v.len()).map_err(|_| OutOfMemory)
}

/// `try list.ensureTotalCapacity(gpa, n)`.
#[inline]
pub(crate) fn ensure_total_capacity<T>(
    v: &mut AVec<T>,
    n: usize,
    init_capacity: usize,
) -> Result<(), OutOfMemory> {
    if v.capacity() >= n {
        return Ok(());
    }
    ensure_total_capacity_precise(v, grow_capacity(n, init_capacity))
}

/// `try list.ensureUnusedCapacity(gpa, n)`.
#[inline]
pub(crate) fn ensure_unused_capacity<T>(
    v: &mut AVec<T>,
    n: usize,
    init_capacity: usize,
) -> Result<(), OutOfMemory> {
    let total = v.len().checked_add(n).ok_or(OutOfMemory)?;
    ensure_total_capacity(v, total, init_capacity)
}

/// `try list.append(gpa, value)`. On failure `value` is dropped and the list is
/// unchanged.
#[inline]
pub(crate) fn append<T>(
    v: &mut AVec<T>,
    value: T,
    init_capacity: usize,
) -> Result<(), OutOfMemory> {
    ensure_unused_capacity(v, 1, init_capacity)?;
    // Capacity was ensured above, so this cannot reallocate.
    v.push(value);
    Ok(())
}

/// `try list.appendNTimes(gpa, value, n)`.
#[inline]
pub(crate) fn append_n_times<T: Copy>(
    v: &mut AVec<T>,
    value: T,
    n: usize,
    init_capacity: usize,
) -> Result<(), OutOfMemory> {
    ensure_unused_capacity(v, n, init_capacity)?;
    let len = v.len();
    for slot in &mut v.spare_capacity_mut()[..n] {
        slot.write(value);
    }
    // SAFETY: the first `n` spare slots were just initialized.
    unsafe { v.set_len(len + n) };
    Ok(())
}

/// `list.shrinkAndFree(gpa, new_len)`: the dropped elements are dropped, then
/// the allocation shrinks to exactly `new_len` (`remap` first, then alloc +
/// copy + free; a `new_len` of 0 frees it). If the allocation cannot shrink,
/// the capacity stays ("No problem, capacity is still correct then").
pub(crate) fn shrink_and_free<T>(v: &mut AVec<T>, new_len: usize) {
    debug_assert!(new_len <= v.len());
    v.truncate(new_len);
    if core::mem::size_of::<T>() == 0 || v.capacity() == new_len {
        return;
    }
    let alloc = *v.allocator();
    let taken = core::mem::replace(v, AVec::new_in(alloc));
    let (ptr, len, cap, alloc) = taken.into_raw_parts_with_alloc();
    let old = Layout::array::<T>(cap).expect("a live allocation has a valid layout");
    let new = Layout::array::<T>(new_len).expect("smaller than a live allocation");
    // SAFETY: `ptr` is the live allocation of `old` made through `alloc` (the
    // vector's own), and `new` is no larger.
    let ptr_u8 = unsafe { NonNull::new_unchecked(ptr as *mut u8) };
    // SAFETY: as above.
    match unsafe { ApiAllocator::shrink(&alloc, ptr_u8, old, new) } {
        Ok(p) => {
            // SAFETY: `p` holds `new_len` elements' worth of the old block's
            // prefix (the handle's shrink copies it), all `len` of them live.
            *v = unsafe { AVec::from_raw_parts_in(p.as_ptr() as *mut T, len, new_len, alloc) };
        }
        Err(_) => {
            // SAFETY: the shrink failed, so the old block is untouched.
            *v = unsafe { AVec::from_raw_parts_in(ptr, len, cap, alloc) };
        }
    }
}

/// `try list.setCapacity(gpa, n)` for a `MultiArrayList`: allocate the new
/// block, copy the elements, free the old block. Never `remap`.
pub(crate) fn set_capacity_alloc_copy_free<T: Copy>(
    v: &mut AVec<T>,
    n: usize,
) -> Result<(), OutOfMemory> {
    debug_assert!(n >= v.len());
    let mut fresh: AVec<T> = AVec::new_in(*v.allocator());
    fresh.try_reserve_exact(n).map_err(|_| OutOfMemory)?;
    // SAFETY: `n >= v.len()` slots were reserved; the two blocks are distinct.
    unsafe {
        core::ptr::copy_nonoverlapping(v.as_ptr(), fresh.as_mut_ptr(), v.len());
        fresh.set_len(v.len());
    }
    // Dropping the old list frees its block (after the new one is allocated,
    // the reference's order).
    *v = fresh;
    Ok(())
}

/// `try list.ensureTotalCapacity(gpa, n)` for a `MultiArrayList`
/// (`growCapacity`, then `setCapacity`).
#[inline]
pub(crate) fn multi_ensure_total_capacity<T: Copy>(
    v: &mut AVec<T>,
    n: usize,
    init_capacity: usize,
) -> Result<(), OutOfMemory> {
    if v.capacity() >= n {
        return Ok(());
    }
    set_capacity_alloc_copy_free(v, grow_capacity(n, init_capacity))
}

/// `try list.clone(gpa)` for a `MultiArrayList` of `Copy` elements:
/// `ensureTotalCapacity(len)` on an empty list, then copy. An empty list
/// allocates nothing.
pub(crate) fn multi_clone<T: Copy>(
    src: &[T],
    alloc: crate::alloc::VtAllocator,
    init_capacity: usize,
) -> Result<AVec<T>, OutOfMemory> {
    let mut out: AVec<T> = AVec::new_in(alloc);
    multi_ensure_total_capacity(&mut out, src.len(), init_capacity)?;
    // SAFETY: capacity for `src.len()` elements was ensured above.
    unsafe {
        core::ptr::copy_nonoverlapping(src.as_ptr(), out.as_mut_ptr(), src.len());
        out.set_len(src.len());
    }
    Ok(out)
}

/// `std.Io.Writer.Allocating` over the handle: the written bytes are
/// `buf[..len]` and the buffer's capacity is `writer.buffer.len`.
///
/// Implements `std::io::Write` with Zig's `Writer.write` semantics: bytes that
/// fit are copied into the buffer; otherwise `drain` grows it by
/// `ensureUnusedCapacity(bytes.len + splat_len + 1)` (with `splat == 1` the
/// splat pattern is the bytes themselves). A failed growth is
/// `error.WriteFailed` (an `io::ErrorKind::OutOfMemory` error here).
pub(crate) struct AllocatingWriter {
    pub(crate) buf: AVec<u8>,
}

/// `ArrayList(u8)`'s `init_capacity` (`cache_line / 1`).
const U8_INIT_CAPACITY: usize = init_capacity(1);

impl AllocatingWriter {
    /// `Allocating.init(alloc)`: no allocation.
    pub(crate) fn init(alloc: crate::alloc::VtAllocator) -> AllocatingWriter {
        AllocatingWriter {
            buf: AVec::new_in(alloc),
        }
    }

    /// `written()`.
    pub(crate) fn written(&self) -> &[u8] {
        &self.buf
    }

    /// `written()` (mutable, for the in-place reverse).
    pub(crate) fn written_mut(&mut self) -> &mut [u8] {
        &mut self.buf
    }

    /// `writeByte`: grow through `rebase(0, 1)` only when the buffer is full.
    pub(crate) fn write_byte(&mut self, byte: u8) -> Result<(), OutOfMemory> {
        if self.buf.capacity() == self.buf.len() {
            // growingRebase(0, 1): ensureTotalCapacity(1), then
            // ensureUnusedCapacity(1).
            ensure_total_capacity(&mut self.buf, 1, U8_INIT_CAPACITY)?;
            ensure_unused_capacity(&mut self.buf, 1, U8_INIT_CAPACITY)?;
        }
        self.buf.push(byte);
        Ok(())
    }
}

impl std::io::Write for AllocatingWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > self.buf.capacity() - self.buf.len() {
            // drain(&.{bytes}, 1): ensureUnusedCapacity(bytes.len + splat_len + 1)
            let extra = bytes
                .len()
                .checked_mul(2)
                .and_then(|n| n.checked_add(1))
                .ok_or(std::io::ErrorKind::OutOfMemory)?;
            ensure_unused_capacity(&mut self.buf, extra, U8_INIT_CAPACITY)
                .map_err(|_| std::io::Error::from(std::io::ErrorKind::OutOfMemory))?;
        }
        let len = self.buf.len();
        // SAFETY: room for `bytes.len()` more bytes is ensured above (or was
        // already there), and `bytes` cannot alias the spare capacity.
        unsafe {
            core::ptr::copy_nonoverlapping(
                bytes.as_ptr(),
                self.buf.as_mut_ptr().add(len),
                bytes.len(),
            );
            self.buf.set_len(len + bytes.len());
        }
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alloc::testing::{with_handle, Config, CountingAllocator, FailingAllocator};
    use std::io::Write;

    #[test]
    fn grow_capacity_matches_zig() {
        // ArrayList(u8) on a 128-byte cache line: init_capacity 128.
        assert_eq!(128, init_capacity(1));
        assert_eq!(1 + 0 + 128, grow_capacity(1, 128));
        // ArrayList(Coordinate) (8 bytes): 16.
        assert_eq!(16, init_capacity(8));
        assert_eq!(10 + 5 + 16, grow_capacity(10, 16));
        // Saturating, as `+|`.
        assert_eq!(usize::MAX, grow_capacity(usize::MAX, 16));
    }

    #[test]
    fn append_grows_by_grow_capacity_through_remap() {
        let counting = CountingAllocator::over_heap();
        let c = counting.allocator();
        with_handle(&c, |alloc| {
            let mut v: AVec<u64> = AVec::new_in(alloc);
            append(&mut v, 1, 16).unwrap();
            // First growth: one alloc of growCapacity(1) = 1 + 0 + 16 elements.
            assert_eq!(
                (1, 0, 0),
                (
                    counting.alloc_calls.get(),
                    counting.remap_calls.get(),
                    counting.free_calls.get()
                )
            );
            assert_eq!(17, v.capacity());
            for i in 0..16 {
                append(&mut v, i, 16).unwrap();
            }
            // len 17 -> 18 needs growCapacity(18) = 18 + 9 + 16 = 43, by remap.
            append(&mut v, 99, 16).unwrap();
            assert_eq!(43, v.capacity());
            assert_eq!(
                (1, 1),
                (counting.alloc_calls.get(), counting.remap_calls.get())
            );
            drop(v);
            assert_eq!(0, counting.live_bytes.get());
        });
    }

    #[test]
    fn multi_growth_never_remaps() {
        let counting = CountingAllocator::over_heap();
        let c = counting.allocator();
        with_handle(&c, |alloc| {
            let src = [1u32, 2, 3];
            let mut v = multi_clone(&src, alloc, 16).unwrap();
            assert_eq!(3 + 1 + 16, v.capacity());
            assert_eq!(&src[..], &v[..]);
            multi_ensure_total_capacity(&mut v, 40, 16).unwrap();
            assert_eq!(40 + 20 + 16, v.capacity());
            assert_eq!(&src[..], &v[..]);
            assert_eq!(
                (2, 0, 1),
                (
                    counting.alloc_calls.get(),
                    counting.remap_calls.get(),
                    counting.free_calls.get()
                )
            );
            // An empty clone allocates nothing.
            let empty = multi_clone::<u32>(&[], alloc, 16).unwrap();
            assert_eq!(0, empty.capacity());
            assert_eq!(2, counting.alloc_calls.get());
            drop(v);
            assert_eq!(0, counting.live_bytes.get());
        });
    }

    #[test]
    fn shrink_and_free_remaps_and_keeps_capacity_on_failure() {
        let failing = FailingAllocator::over_heap(Config::default());
        let c = failing.allocator();
        with_handle(&c, |alloc| {
            let mut v: AVec<u16> = AVec::new_in(alloc);
            for x in [1, 2, 3, 4, 5] {
                append(&mut v, x, 64).unwrap();
            }
            let cap = v.capacity();
            // A refused remap and a refused alloc: capacity stays, length shrinks.
            failing.resize_fail_index.set(failing.resize_index.get());
            failing.fail_index.set(failing.alloc_index.get());
            shrink_and_free(&mut v, 2);
            assert_eq!((2, cap), (v.len(), v.capacity()));
            assert_eq!(&[1, 2][..], &v[..]);
            // Allowed: exactly two elements remain allocated.
            failing.resize_fail_index.set(usize::MAX);
            failing.fail_index.set(usize::MAX);
            shrink_and_free(&mut v, 2);
            assert_eq!((2, 2), (v.len(), v.capacity()));
            assert_eq!(&[1, 2][..], &v[..]);
            shrink_and_free(&mut v, 0);
            assert_eq!(0, v.capacity());
            drop(v);
            assert_eq!(failing.allocated_bytes.get(), failing.freed_bytes.get());
        });
    }

    #[test]
    fn allocating_writer_drains_like_zig() {
        let counting = CountingAllocator::over_heap();
        let c = counting.allocator();
        with_handle(&c, |alloc| {
            let mut w = AllocatingWriter::init(alloc);
            // The first write drains: ensureUnusedCapacity(3 + 3 + 1 = 7),
            // growCapacity(7) = 7 + 3 + 128 = 138.
            w.write_all(b"abc").unwrap();
            assert_eq!(138, w.buf.capacity());
            assert_eq!(1, counting.alloc_calls.get());
            // Writes that fit do not grow.
            w.write_all(&[b'x'; 135]).unwrap();
            assert_eq!(1, counting.alloc_calls.get() + counting.remap_calls.get());
            // The buffer is full: writeByte rebases to growCapacity(139).
            w.write_byte(b'\n').unwrap();
            assert_eq!(139 + 69 + 128, w.buf.capacity());
            assert_eq!(1, counting.remap_calls.get());
            assert_eq!(139, w.written().len());
            drop(w);
            assert_eq!(0, counting.live_bytes.get());
        });
    }

    #[test]
    fn allocating_writer_reports_a_failed_drain() {
        let failing = FailingAllocator::over_heap(Config {
            fail_index: 0,
            resize_fail_index: usize::MAX,
        });
        let c = failing.allocator();
        with_handle(&c, |alloc| {
            let mut w = AllocatingWriter::init(alloc);
            let err = w.write_all(b"abc").unwrap_err();
            assert_eq!(std::io::ErrorKind::OutOfMemory, err.kind());
            assert_eq!(Err(OutOfMemory), w.write_byte(b'\n'));
            assert!(w.written().is_empty());
            assert!(failing.has_induced_failure.get());
        });
    }
}
