//! A data structure where you can get stable (never copied) pointers to
//! a type that automatically grows if necessary. The values can be "put back"
//! but are expected to be put back IN ORDER.
//!
//! This is implemented specifically for libuv write requests, since the
//! write requests must have a stable pointer and are guaranteed to be processed
//! in order for a single stream.
//!
//! This is NOT thread safe.
//
// PORT NOTE: The Zig reference is built on `std.SegmentedList(T, prealloc)`,
// which provides stable element pointers that survive growth (growth never
// moves existing elements). We reproduce that property by storing each
// "segment" as an independently-heap-allocated `Box<[T]>`: existing segments
// are never reallocated when we grow, so `&mut T` references into them remain
// stable. `at(i)` indexes into the flattened logical sequence; `len` tracks the
// total element count exactly like `SegmentedList.len`.

use rustty_util::quirks::inlineAssert as assert;

/// Error returned when there are no available values left.
#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    OutOfValues,
}

/// A data structure where you can get stable (never copied) pointers to
/// a type that automatically grows if necessary. The values can be "put back"
/// but are expected to be put back IN ORDER.
///
/// This is implemented specifically for libuv write requests, since the
/// write requests must have a stable pointer and are guaranteed to be processed
/// in order for a single stream.
///
/// This is NOT thread safe.
pub struct SegmentedPool<T: Default> {
    pub i: usize,
    pub available: usize,
    // The backing storage: a list of heap-allocated segments. Existing
    // segments are never moved/reallocated on growth, giving stable pointers.
    // The total logical length is tracked separately to mirror
    // `std.SegmentedList.len`.
    segments: Vec<Box<[T]>>,
    list_len: usize,
}

impl<T: Default> SegmentedPool<T> {
    /// Create a new pool with `prealloc` preallocated values (matches the Zig
    /// default value `.{ .len = prealloc }` with `available = prealloc`).
    pub fn new(prealloc: usize) -> Self {
        let mut pool = SegmentedPool {
            i: 0,
            available: prealloc,
            segments: Vec::new(),
            list_len: prealloc,
        };
        // Preallocate the initial segment of `prealloc` default values.
        if prealloc > 0 {
            let seg: Vec<T> = (0..prealloc).map(|_| T::default()).collect();
            pool.segments.push(seg.into_boxed_slice());
        }
        pool
    }

    // Internal: total preallocated/grown length (mirror of `self.list.len`).
    #[inline]
    fn list_len(&self) -> usize {
        self.list_len
    }

    // Internal: stable mutable pointer to logical element `i`.
    fn at(&mut self, i: usize) -> *mut T {
        let mut remaining = i;
        for seg in self.segments.iter_mut() {
            if remaining < seg.len() {
                return &mut seg[remaining] as *mut T;
            }
            remaining -= seg.len();
        }
        // Should never be reached for valid indices.
        core::ptr::null_mut()
    }

    /// Get the next available value out of the list. This will not
    /// grow the list.
    ///
    /// Returns a stable pointer to the value.
    ///
    /// # Safety
    /// The returned pointer is stable for the lifetime of the pool but the
    /// caller must not alias it with another live `get` of the same slot.
    pub fn get(&mut self) -> Result<*mut T, Error> {
        // Error to not have any
        if self.available == 0 {
            return Err(Error::OutOfValues);
        }

        // The index we grab is just i % len, so we wrap around to the front.
        let i = self.i % self.list_len();
        self.i = self.i.wrapping_add(1); // Wrapping addition to swe go back to 0
        self.available -= 1;
        Ok(self.at(i))
    }

    /// Get the next available value out of the list and grow the list
    /// if necessary.
    pub fn get_grow(&mut self) -> Result<*mut T, Error> {
        if self.available == 0 {
            self.grow();
        }
        self.get()
    }

    fn grow(&mut self) {
        // Grow capacity to `len * 2`, appending a fresh segment of `len`
        // default values (existing segments stay put -> stable pointers).
        let old_len = self.list_len;
        let seg: Vec<T> = (0..old_len).map(|_| T::default()).collect();
        self.segments.push(seg.into_boxed_slice());
        self.i = self.list_len;
        self.available = self.list_len;
        self.list_len *= 2;
    }

    /// Put a value back. The value put back is expected to be the
    /// in order of get.
    pub fn put(&mut self) {
        self.available += 1;
        assert(self.available <= self.list_len());
    }
}

#[cfg(test)]
#[allow(non_snake_case)]
mod tests {
    use super::*;

    #[test]
    fn SegmentedPool() {
        let mut list: SegmentedPool<u8> = SegmentedPool::new(2);
        assert_eq!(2usize, list.available);

        unsafe {
            // Get to capacity
            let v1 = list.get().unwrap();
            let v2 = list.get().unwrap();
            assert!(v1 != v2);
            assert!(matches!(list.get(), Err(Error::OutOfValues)));

            // Test writing for later
            *v1 = 42;

            // Put a value back
            list.put();
            let temp = list.get().unwrap();
            assert!(v1 == temp);
            assert!(*temp == 42);
            assert!(matches!(list.get(), Err(Error::OutOfValues)));

            // Grow
            let v3 = list.get_grow().unwrap();
            assert!(v1 != v3 && v2 != v3);
            let _ = list.get().unwrap();
            assert!(matches!(list.get(), Err(Error::OutOfValues)));

            // Put a value back
            list.put();
            assert!(v1 == list.get().unwrap());
            assert!(matches!(list.get(), Err(Error::OutOfValues)));
        }
    }
}
