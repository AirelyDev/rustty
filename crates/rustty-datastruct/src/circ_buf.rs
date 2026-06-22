//! Returns a circular buffer containing type T.
//
// PORT NOTE: The Zig reference is `fn CircBuf(T, default) type` where `default`
// is a comptime default value used to fill new/cleared storage. We model `T`
// as a generic and `default` as a value supplied at `init` time (the reference
// only ever uses a single comptime default per instantiation; supplying it at
// `init` is observationally identical and required since Rust const generics
// can't be arbitrary `T`). `getPtrSlice` returns `[2][]T` in Zig; in Rust we
// return two mutable subslices of `storage` (the second is empty when the data
// fits in one slice, and the two are disjoint when the data wraps — exactly the
// reference's invariant), via `split_at_mut`.

use rustty_util::fastmem;
use rustty_util::quirks::inlineAssert as assert;

/// Error type for allocation/capacity failures (mirror of `Allocator.Error`
/// / `error.OutOfMemory`).
#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    OutOfMemory,
}

/// Returns a circular buffer containing type T.
pub struct CircBuf<T: Copy> {
    // Implementation note: there's a lot of unsafe addition of usize
    // here in this implementation that can technically overflow. If someone
    // wants to fix this and make it overflow safe (use subtractions for
    // checks prior to additions) then I welcome it. In reality, we'd
    // have to be a really, really large terminal screen to even worry
    // about this so I'm punting it.
    pub storage: Vec<T>,
    pub head: usize,
    pub tail: usize,

    // We could remove this and just use math with head/tail to figure
    // it out, but our usage of circular buffers stores so much data that
    // this minor overhead is not worth optimizing out.
    pub full: bool,

    // The default value used to fill storage (the Zig comptime `default`).
    default: T,
}

/// Direction of iteration.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Forward,
    Reverse,
}

impl<T: Copy> CircBuf<T> {
    /// Initialize a new circular buffer that can store size elements.
    pub fn init(size: usize, default: T) -> Result<Self, Error> {
        let buf = vec![default; size];

        Ok(CircBuf {
            storage: buf,
            head: 0,
            tail: 0,
            full: size == 0,
            default,
        })
    }

    /// Append a single value to the buffer. If the buffer is full,
    /// an error will be returned.
    pub fn append(&mut self, v: T) -> Result<(), Error> {
        if self.full {
            return Err(Error::OutOfMemory);
        }
        self.storage[self.head] = v;
        self.head += 1;
        if self.head >= self.storage.len() {
            self.head = 0;
        }
        self.full = self.head == self.tail;
        Ok(())
    }

    /// Append a single value to the buffer, assuming there is capacity.
    pub fn append_assume_capacity(&mut self, v: T) {
        assert(!self.full);
        self.storage[self.head] = v;
        self.head += 1;
        if self.head >= self.storage.len() {
            self.head = 0;
        }
        self.full = self.head == self.tail;
    }

    /// Append a slice to the buffer.
    pub fn append_slice_assume_capacity(&mut self, slice: &[T]) {
        let storage = self.get_ptr_slice(self.len(), slice.len());
        // SAFETY: `get_ptr_slice` returns two disjoint, valid `*mut [T]`
        // subslices of `self.storage`; we hold no other borrow of `self` while
        // dereferencing them (mirrors the Zig `[2][]T` raw-pointer slices).
        unsafe {
            let s0 = &mut *storage.0;
            let s1 = &mut *storage.1;
            let s0_len = s0.len();
            fastmem::copy(s0, &slice[0..s0_len]);
            fastmem::copy(s1, &slice[s0_len..]);
        }
    }

    /// Clear the buffer.
    pub fn clear(&mut self) {
        self.head = 0;
        self.tail = 0;
        self.full = false;
    }

    /// Iterate over the circular buffer.
    pub fn iterator(&self, direction: Direction) -> Iterator<'_, T> {
        Iterator {
            buf: self,
            idx: 0,
            direction,
        }
    }

    /// Get the first (oldest) value in the buffer.
    pub fn first(&self) -> Option<&T> {
        // Note: this can be more efficient by not using the
        // iterator, but this was an easy way to implement it.
        let mut it = self.iterator(Direction::Forward);
        it.next()
    }

    /// Get the last (newest) value in the buffer.
    pub fn last(&self) -> Option<&T> {
        // Note: this can be more efficient by not using the
        // iterator, but this was an easy way to implement it.
        let mut it = self.iterator(Direction::Reverse);
        it.next()
    }

    /// Ensures that there is enough capacity to store amount more
    /// items via append.
    pub fn ensure_unused_capacity(&mut self, amount: usize) -> Result<(), Error> {
        let new_cap = self.len() + amount;
        if new_cap <= self.capacity() {
            return Ok(());
        }
        self.resize(new_cap)
    }

    /// Resize the buffer to the given size (larger or smaller).
    /// If larger, new values will be set to the default value.
    pub fn resize(&mut self, size: usize) -> Result<(), Error> {
        // Rotate to zero so it is aligned.
        self.rotate_to_zero()?;

        // Reallocate, this adds to the end so we're ready to go.
        let prev_len = self.len();
        let prev_cap = self.storage.len();
        self.storage.resize(size, self.default);

        // If we grew, we need to set our new defaults. We can add it
        // at the end since we rotated to start.
        if size > prev_cap {
            // (Vec::resize already filled [prev_cap..] with default.)
            for v in self.storage[prev_cap..].iter_mut() {
                *v = self.default;
            }

            // Fix up our head/tail
            if self.full {
                self.head = prev_len;
                self.full = false;
            }
        }
        Ok(())
    }

    /// Rotate the data so that it is zero-aligned.
    pub fn rotate_to_zero(&mut self) -> Result<(), Error> {
        // If we're already at zero then do nothing.
        if self.tail == 0 {
            return Ok(());
        }

        // We use std.mem.rotate to rotate our storage in-place.
        // std.mem.rotate(T, items, n) rotates left by n; Rust's
        // slice::rotate_left has the same semantics.
        self.storage.rotate_left(self.tail);

        // Then fix up our head and tail.
        self.head = self.len() % self.storage.len();
        self.tail = 0;
        Ok(())
    }

    /// Returns if the buffer is currently empty. To check if its
    /// full, just check the "full" attribute.
    pub fn empty(&self) -> bool {
        !self.full && self.head == self.tail
    }

    // PARITY: the reference exposes `empty()` (mirrored above). This is an
    // additive Rust convenience that satisfies clippy's `len_without_is_empty`
    // for `len()` below by delegating to the reference `empty()`; it adds no
    // new behavior and removes nothing.
    pub fn is_empty(&self) -> bool {
        self.empty()
    }

    /// Returns the total capacity allocated for this buffer.
    pub fn capacity(&self) -> usize {
        self.storage.len()
    }

    /// Returns the length in elements that are used.
    pub fn len(&self) -> usize {
        if self.full {
            return self.storage.len();
        }
        if self.head >= self.tail {
            return self.head - self.tail;
        }
        self.storage.len() - (self.tail - self.head)
    }

    /// Delete the oldest n values from the buffer. If there are less
    /// than n values in the buffer, it'll delete everything.
    pub fn delete_oldest(&mut self, n: usize) {
        assert(n <= self.storage.len());

        // Special case n == 0 otherwise we will accidentally break
        // our circular buffer.
        if n == 0 {
            // @branchHint(.cold);
            return;
        }

        // Clear the values back to default
        let default = self.default;
        let (s0, s1) = self.get_ptr_slice(0, n);
        // inline for (slices) |slice| @memset(slice, default);
        // SAFETY: the two returned `*mut [T]` are disjoint, valid subslices of
        // `self.storage` and no other borrow of `self` is live here.
        unsafe {
            for v in (*s0).iter_mut() {
                *v = default;
            }
            for v in (*s1).iter_mut() {
                *v = default;
            }
        }

        // If we're not full, we can just advance the tail. We know
        // it'll be less than the length because otherwise we'd be full.
        self.tail += core::cmp::min(self.len(), n);
        if self.tail >= self.storage.len() {
            self.tail -= self.storage.len();
        }
        self.full = false;
    }

    /// Returns a pointer to the value at offset with the given length,
    /// and considers this full amount of data "written" if it is beyond
    /// the end of our buffer. This never "rotates" the buffer because
    /// the offset can only be within the size of the buffer.
    pub fn get_ptr_slice(&mut self, offset: usize, slice_len: usize) -> (*mut [T], *mut [T]) {
        // Special case the empty slice fast-path.
        if slice_len == 0 {
            // @branchHint(.cold);
            // return .{ &.{}, &.{} };
            let empty: &mut [T] = &mut [];
            return (empty as *mut [T], &mut [] as *mut [T]);
        }

        // Note: this assertion is very important, it hints the compiler
        // which generates ~10% faster code than without it.
        assert(offset + slice_len <= self.capacity());

        // End offset is the last offset (exclusive) for our slice.
        // We use exclusive because it makes the math easier and it
        // matches Zigs slicing parameterization.
        let end_offset = offset + slice_len;

        // If our slice can't fit it in our length, then we need to advance.
        if end_offset > self.len() {
            self.advance(end_offset - self.len());
        }

        // Our start and end indexes into the storage buffer
        let start_idx = self.storage_offset(offset);
        let end_idx = self.storage_offset(end_offset - 1);
        // std.log.warn("A={} B={}", .{ start_idx, end_idx });

        // Optimistically, our data fits in one slice
        if end_idx >= start_idx {
            // self.storage[start_idx .. end_idx + 1], self.storage[0..0]
            // We derive the two disjoint subslices from a single `split_at_mut`
            // so they have separate provenance (Stacked-Borrows-clean under
            // Miri) — mirroring the Zig `[2][]T` raw slices.
            let (left, right) = self.storage.split_at_mut(end_idx + 1);
            let single: *mut [T] = &mut left[start_idx..];
            let empty: *mut [T] = &mut right[0..0]; // So there is an empty slice
            return (single, empty);
        }

        // Wrapping case: self.storage[start_idx..], self.storage[0 .. end_idx + 1]
        // These two regions are disjoint because end_idx < start_idx, so
        // `split_at_mut(start_idx)` yields independent halves we slice into.
        let (head, tail) = self.storage.split_at_mut(start_idx);
        let first: *mut [T] = tail; // [start_idx..]
        let second: *mut [T] = &mut head[0..end_idx + 1]; // [0 .. end_idx + 1]
        (first, second)
    }

    /// Advances the head/tail so that we can store amount.
    fn advance(&mut self, amount: usize) {
        assert(amount <= self.storage.len() - self.len());

        // Optimistically add our amount
        self.head += amount;

        // If we exceeded the length of the buffer, wrap around.
        if self.head >= self.storage.len() {
            self.head -= self.storage.len();
        }

        // If we're full, we have to keep tail lined up.
        if self.full {
            self.tail = self.head;
        }

        // We're full if the head reached the tail. The head can never
        // pass the tail because advance asserts amount is only in
        // available space left
        self.full = self.head == self.tail;
    }

    /// For a given offset from zero, this returns the offset in the
    /// storage buffer where this data can be found.
    fn storage_offset(&self, offset: usize) -> usize {
        assert(offset < self.storage.len());

        // This should be subtraction ideally to avoid overflows but
        // it would take a really, really, huge buffer to overflow.
        let fits_offset = self.tail + offset;
        if fits_offset < self.storage.len() {
            return fits_offset;
        }
        fits_offset - self.storage.len()
    }
}

/// Iterator over a circular buffer.
pub struct Iterator<'a, T: Copy> {
    buf: &'a CircBuf<T>,
    idx: usize,
    direction: Direction,
}

impl<'a, T: Copy> Iterator<'a, T> {
    // PARITY: the reference `circ_buf.zig:34` exposes an inherent,
    // iterator-like `next()` on its `Iterator` type. We keep that public API
    // shape rather than reshaping it into `std::iter::Iterator`, so we silence
    // `should_implement_trait`.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Option<&'a T> {
        if self.idx >= self.buf.len() {
            return None;
        }

        // Get our index from the tail
        let tail_idx = match self.direction {
            Direction::Forward => self.idx,
            Direction::Reverse => self.buf.len() - self.idx - 1,
        };

        // Translate the tail index to a storage index
        let storage_idx = (self.buf.tail + tail_idx) % self.buf.capacity();
        self.idx += 1;
        Some(&self.buf.storage[storage_idx])
    }

    /// Seek the iterator by a given amount. This will clamp
    /// the values to the bounds of the buffer so overflows are
    /// not possible.
    pub fn seek_by(&mut self, amount: isize) {
        if amount > 0 {
            self.idx = self.idx.saturating_add(amount as usize);
        } else {
            self.idx = self.idx.saturating_sub((amount).unsigned_abs());
        }
    }

    /// Reset the iterator back to the first value.
    pub fn reset(&mut self) {
        self.idx = 0;
    }
}

#[cfg(test)]
#[allow(non_snake_case)]
mod tests {
    use super::*;

    #[test]
    fn test_anon_0() {
        type Buf = CircBuf<u8>;
        let buf = Buf::init(12, 0).unwrap();

        assert!(buf.empty());
        assert_eq!(0usize, buf.len());
    }

    #[test]
    fn CircBuf_append() {
        type Buf = CircBuf<u8>;
        let mut buf = Buf::init(3, 0).unwrap();

        buf.append(1).unwrap();
        buf.append(2).unwrap();
        buf.append(3).unwrap();
        assert!(matches!(buf.append(4), Err(Error::OutOfMemory)));
        buf.delete_oldest(1);
        buf.append(4).unwrap();
        assert!(matches!(buf.append(5), Err(Error::OutOfMemory)));
    }

    #[test]
    fn CircBuf_forward_iterator() {
        type Buf = CircBuf<u8>;
        let mut buf = Buf::init(3, 0).unwrap();

        // Empty
        {
            let mut it = buf.iterator(Direction::Forward);
            assert!(it.next().is_none());
        }

        // Partially full
        buf.append(1).unwrap();
        buf.append(2).unwrap();
        {
            let mut it = buf.iterator(Direction::Forward);
            assert!(*it.next().unwrap() == 1);
            assert!(*it.next().unwrap() == 2);
            assert!(it.next().is_none());
        }

        // Full
        buf.append(3).unwrap();
        {
            let mut it = buf.iterator(Direction::Forward);
            assert!(*it.next().unwrap() == 1);
            assert!(*it.next().unwrap() == 2);
            assert!(*it.next().unwrap() == 3);
            assert!(it.next().is_none());
        }

        // Delete and add
        buf.delete_oldest(1);
        buf.append(4).unwrap();
        {
            let mut it = buf.iterator(Direction::Forward);
            assert!(*it.next().unwrap() == 2);
            assert!(*it.next().unwrap() == 3);
            assert!(*it.next().unwrap() == 4);
            assert!(it.next().is_none());
        }
    }

    #[test]
    fn CircBuf_reverse_iterator() {
        type Buf = CircBuf<u8>;
        let mut buf = Buf::init(3, 0).unwrap();

        // Empty
        {
            let mut it = buf.iterator(Direction::Reverse);
            assert!(it.next().is_none());
        }

        // Partially full
        buf.append(1).unwrap();
        buf.append(2).unwrap();
        {
            let mut it = buf.iterator(Direction::Reverse);
            assert!(*it.next().unwrap() == 2);
            assert!(*it.next().unwrap() == 1);
            assert!(it.next().is_none());
        }

        // Full
        buf.append(3).unwrap();
        {
            let mut it = buf.iterator(Direction::Reverse);
            assert!(*it.next().unwrap() == 3);
            assert!(*it.next().unwrap() == 2);
            assert!(*it.next().unwrap() == 1);
            assert!(it.next().is_none());
        }

        // Delete and add
        buf.delete_oldest(1);
        buf.append(4).unwrap();
        {
            let mut it = buf.iterator(Direction::Reverse);
            assert!(*it.next().unwrap() == 4);
            assert!(*it.next().unwrap() == 3);
            assert!(*it.next().unwrap() == 2);
            assert!(it.next().is_none());
        }
    }

    #[test]
    fn CircBuf_first_last() {
        type Buf = CircBuf<u8>;
        let mut buf = Buf::init(3, 0).unwrap();

        buf.append(1).unwrap();
        buf.append(2).unwrap();
        buf.append(3).unwrap();
        assert_eq!(3, *buf.last().unwrap());
        assert_eq!(1, *buf.first().unwrap());
    }

    #[test]
    fn CircBuf_first_last_empty() {
        type Buf = CircBuf<u8>;
        let buf = Buf::init(0, 0).unwrap();

        assert!(buf.first().is_none());
        assert!(buf.last().is_none());
    }

    #[test]
    fn CircBuf_first_last_empty_with_cap() {
        type Buf = CircBuf<u8>;
        let buf = Buf::init(3, 0).unwrap();

        assert!(buf.first().is_none());
        assert!(buf.last().is_none());
    }

    #[test]
    fn CircBuf_append_slice() {
        type Buf = CircBuf<u8>;
        let mut buf = Buf::init(5, 0).unwrap();

        buf.append_slice_assume_capacity(b"hello");
        {
            let mut it = buf.iterator(Direction::Forward);
            assert!(*it.next().unwrap() == b'h');
            assert!(*it.next().unwrap() == b'e');
            assert!(*it.next().unwrap() == b'l');
            assert!(*it.next().unwrap() == b'l');
            assert!(*it.next().unwrap() == b'o');
            assert!(it.next().is_none());
        }
    }

    #[test]
    fn CircBuf_append_slice_with_wrap() {
        type Buf = CircBuf<u8>;
        let mut buf = Buf::init(4, 0).unwrap();

        // Fill the buffer
        let _ = buf.get_ptr_slice(0, buf.capacity());
        assert!(buf.full);
        assert_eq!(4usize, buf.len());

        // Delete
        buf.delete_oldest(2);
        assert!(!buf.full);
        assert_eq!(2usize, buf.len());

        buf.append_slice_assume_capacity(b"AB");
        {
            let mut it = buf.iterator(Direction::Forward);
            assert!(*it.next().unwrap() == 0);
            assert!(*it.next().unwrap() == 0);
            assert!(*it.next().unwrap() == b'A');
            assert!(*it.next().unwrap() == b'B');
            assert!(it.next().is_none());
        }
    }

    #[test]
    fn CircBuf_getPtrSlice_fits() {
        type Buf = CircBuf<u8>;
        let mut buf = Buf::init(12, 0).unwrap();

        let slices = buf.get_ptr_slice(0, 11);
        assert_eq!(11usize, slices.0.len());
        assert_eq!(0usize, slices.1.len());
        assert_eq!(11usize, buf.len());
    }

    #[test]
    fn CircBuf_getPtrSlice_wraps() {
        type Buf = CircBuf<u8>;
        let mut buf = Buf::init(4, 0).unwrap();

        // Fill the buffer
        let _ = buf.get_ptr_slice(0, buf.capacity());
        assert!(buf.full);
        assert_eq!(4usize, buf.len());

        // Delete
        buf.delete_oldest(2);
        assert!(!buf.full);
        assert_eq!(2usize, buf.len());

        // Get a slice that doesn't grow
        {
            let slices = buf.get_ptr_slice(0, 2);
            assert_eq!(2usize, slices.0.len());
            assert_eq!(0usize, slices.1.len());
            assert_eq!(2usize, buf.len());
            unsafe {
                (&mut *slices.0)[0] = 1;
            }
            unsafe {
                (&mut *slices.0)[1] = 2;
            }
        }

        // Get a slice that does grow, and forces wrap
        {
            let slices = buf.get_ptr_slice(2, 2);
            assert_eq!(2usize, slices.0.len());
            assert_eq!(0usize, slices.1.len());
            assert_eq!(4usize, buf.len());

            // should be empty
            assert_eq!(0u8, unsafe { (&*slices.0)[0] });
            assert_eq!(0u8, unsafe { (&*slices.0)[1] });
            unsafe {
                (&mut *slices.0)[0] = 3;
            }
            unsafe {
                (&mut *slices.0)[1] = 4;
            }
        }

        // Get a slice across boundaries
        {
            let slices = buf.get_ptr_slice(0, 4);
            assert_eq!(2usize, slices.0.len());
            assert_eq!(2usize, slices.1.len());
            assert_eq!(4usize, buf.len());

            assert_eq!(1u8, unsafe { (&*slices.0)[0] });
            assert_eq!(2u8, unsafe { (&*slices.0)[1] });
            assert_eq!(3u8, unsafe { (&*slices.1)[0] });
            assert_eq!(4u8, unsafe { (&*slices.1)[1] });
        }
    }

    #[test]
    fn CircBuf_rotateToZero() {
        type Buf = CircBuf<u8>;
        let mut buf = Buf::init(12, 0).unwrap();

        let _ = buf.get_ptr_slice(0, 11);
        buf.rotate_to_zero().unwrap();
    }

    #[test]
    fn CircBuf_rotateToZero_offset() {
        type Buf = CircBuf<u8>;
        let mut buf = Buf::init(4, 0).unwrap();

        // Fill the buffer
        let _ = buf.get_ptr_slice(0, 3);
        assert_eq!(3usize, buf.len());

        // Delete
        buf.delete_oldest(2);
        assert!(!buf.full);
        assert_eq!(1usize, buf.len());
        assert!(buf.tail > 0 && buf.head >= buf.tail);

        // Rotate to zero
        buf.rotate_to_zero().unwrap();
        assert_eq!(0usize, buf.tail);
        assert_eq!(1usize, buf.head);
    }

    #[test]
    fn CircBuf_rotateToZero_wraps() {
        type Buf = CircBuf<u8>;
        let mut buf = Buf::init(4, 0).unwrap();

        // Fill the buffer
        let _ = buf.get_ptr_slice(0, 3);
        assert_eq!(3usize, buf.len());
        assert!(buf.tail == 0 && buf.head == 3);

        // Delete all
        buf.delete_oldest(3);
        assert_eq!(0usize, buf.len());
        assert!(buf.tail == 3 && buf.head == 3);

        // Refill to force a wrap
        {
            let slices = buf.get_ptr_slice(0, 3);
            unsafe {
                (&mut *slices.0)[0] = 1;
            }
            unsafe {
                (&mut *slices.1)[0] = 2;
            }
            unsafe {
                (&mut *slices.1)[1] = 3;
            }
            assert_eq!(3usize, buf.len());
            assert!(buf.tail == 3 && buf.head == 2);
        }

        // Rotate to zero
        buf.rotate_to_zero().unwrap();
        assert_eq!(0usize, buf.tail);
        assert_eq!(3usize, buf.head);
        {
            let slices = buf.get_ptr_slice(0, 3);
            assert_eq!(1u8, unsafe { (&*slices.0)[0] });
            assert_eq!(2u8, unsafe { (&*slices.0)[1] });
            assert_eq!(3u8, unsafe { (&*slices.0)[2] });
        }
    }

    #[test]
    fn CircBuf_rotateToZero_full_no_wrap() {
        type Buf = CircBuf<u8>;
        let mut buf = Buf::init(4, 0).unwrap();

        // Fill the buffer
        let _ = buf.get_ptr_slice(0, 3);

        // Delete all
        buf.delete_oldest(3);

        // Refill to force a wrap
        {
            let slices = buf.get_ptr_slice(0, 4);
            assert!(buf.full);
            unsafe {
                (&mut *slices.0)[0] = 1;
            }
            unsafe {
                (&mut *slices.1)[0] = 2;
            }
            unsafe {
                (&mut *slices.1)[1] = 3;
            }
            unsafe {
                (&mut *slices.1)[2] = 4;
            }
        }

        // Rotate to zero
        buf.rotate_to_zero().unwrap();
        assert!(buf.full);
        assert_eq!(0usize, buf.tail);
        assert_eq!(0usize, buf.head);
        {
            let slices = buf.get_ptr_slice(0, 4);
            assert_eq!(1u8, unsafe { (&*slices.0)[0] });
            assert_eq!(2u8, unsafe { (&*slices.0)[1] });
            assert_eq!(3u8, unsafe { (&*slices.0)[2] });
            assert_eq!(4u8, unsafe { (&*slices.0)[3] });
        }
    }

    #[test]
    fn CircBuf_resize_grow_from_zero() {
        type Buf = CircBuf<u8>;
        let mut buf = Buf::init(0, 0).unwrap();
        assert!(buf.full);

        // Resize
        buf.resize(2).unwrap();
        assert!(!buf.full);
        assert_eq!(0usize, buf.len());
        assert_eq!(2usize, buf.capacity());

        buf.append(1).unwrap();
        buf.append(2).unwrap();

        {
            let slices = buf.get_ptr_slice(0, 2);
            assert_eq!(1u8, unsafe { (&*slices.0)[0] });
            assert_eq!(2u8, unsafe { (&*slices.0)[1] });
        }
    }

    #[test]
    fn CircBuf_resize_grow() {
        type Buf = CircBuf<u8>;
        let mut buf = Buf::init(4, 0).unwrap();

        // Fill and write
        {
            let slices = buf.get_ptr_slice(0, 4);
            assert!(buf.full);
            unsafe {
                (&mut *slices.0)[0] = 1;
            }
            unsafe {
                (&mut *slices.0)[1] = 2;
            }
            unsafe {
                (&mut *slices.0)[2] = 3;
            }
            unsafe {
                (&mut *slices.0)[3] = 4;
            }
        }

        // Resize
        buf.resize(6).unwrap();
        assert!(!buf.full);
        assert_eq!(4usize, buf.len());
        assert_eq!(6usize, buf.capacity());

        {
            let slices = buf.get_ptr_slice(0, 4);
            assert_eq!(1u8, unsafe { (&*slices.0)[0] });
            assert_eq!(2u8, unsafe { (&*slices.0)[1] });
            assert_eq!(3u8, unsafe { (&*slices.0)[2] });
            assert_eq!(4u8, unsafe { (&*slices.0)[3] });
        }
    }

    #[test]
    fn CircBuf_resize_shrink() {
        type Buf = CircBuf<u8>;
        let mut buf = Buf::init(4, 0).unwrap();

        // Fill and write
        {
            let slices = buf.get_ptr_slice(0, 4);
            assert!(buf.full);
            unsafe {
                (&mut *slices.0)[0] = 1;
            }
            unsafe {
                (&mut *slices.0)[1] = 2;
            }
            unsafe {
                (&mut *slices.0)[2] = 3;
            }
            unsafe {
                (&mut *slices.0)[3] = 4;
            }
        }

        // Resize
        buf.resize(3).unwrap();
        assert!(buf.full);
        assert_eq!(3usize, buf.len());
        assert_eq!(3usize, buf.capacity());

        {
            let slices = buf.get_ptr_slice(0, 3);
            assert_eq!(1u8, unsafe { (&*slices.0)[0] });
            assert_eq!(2u8, unsafe { (&*slices.0)[1] });
            assert_eq!(3u8, unsafe { (&*slices.0)[2] });
        }
    }

    #[test]
    fn CircBuf_append_empty_slice() {
        type Buf = CircBuf<u8>;
        let mut buf = Buf::init(5, 0).unwrap();

        // Appending an empty slice to empty buffer should be a no-op
        buf.append_slice_assume_capacity(b"");
        assert_eq!(0usize, buf.len());
        assert!(!buf.full);

        // Buffer should still work normally after appending empty slice
        buf.append_slice_assume_capacity(b"hi");
        assert_eq!(2usize, buf.len());

        // Appending an empty slice to non-empty buffer should also be a no-op
        buf.append_slice_assume_capacity(b"");
        assert_eq!(2usize, buf.len());
    }

    #[test]
    fn CircBuf_getPtrSlice_zero_length() {
        type Buf = CircBuf<u8>;
        let mut buf = Buf::init(5, 0).unwrap();

        // getPtrSlice with zero length on empty buffer should return empty slices
        let slices = buf.get_ptr_slice(0, 0);
        assert_eq!(0usize, slices.0.len());
        assert_eq!(0usize, slices.1.len());
        assert_eq!(0usize, buf.len());

        // Fill buffer partially
        buf.append_slice_assume_capacity(b"abc");
        assert_eq!(3usize, buf.len());

        // getPtrSlice with zero length on non-empty buffer should also work
        let slices2 = buf.get_ptr_slice(0, 0);
        assert_eq!(0usize, slices2.0.len());
        assert_eq!(0usize, slices2.1.len());
        assert_eq!(3usize, buf.len());
    }

    #[test]
    fn CircBuf_deleteOldest_zero() {
        type Buf = CircBuf<u8>;
        let mut buf = Buf::init(5, 0).unwrap();

        // deleteOldest(0) on empty buffer should be a no-op
        buf.delete_oldest(0);
        assert_eq!(0usize, buf.len());

        // Fill buffer
        buf.append_slice_assume_capacity(b"hello");
        assert_eq!(5usize, buf.len());

        // deleteOldest(0) on non-empty buffer should be a no-op
        buf.delete_oldest(0);
        assert_eq!(5usize, buf.len());

        // Verify data is unchanged
        let mut it = buf.iterator(Direction::Forward);
        assert_eq!(b'h', *it.next().unwrap());
    }
}
