//! `std.ArrayListUnmanaged(T)` (Zig 0.16 `std/array_list.zig`) for `Copy`
//! elements: a list that does NOT store its allocator; every growing call and
//! `deinit` take it, as the reference's do.
//!
//! PORT STATUS: Rust-only glue (R1 "no-loss, not no-addition"): the ONE
//! unmanaged list of the port. rustty-vt's `alloc::ArrayListUnmanaged` (parity
//! tail S7-3: `Terminal.pwd`/`title`, the stream continuation bytes) is a thin
//! newtype over it with `VtAllocator` signatures, and
//! [`UntouchedPool`](crate::untouched_pool)'s free list (upstream `e01e75bbb`)
//! is this list of item pointers.
//!
//! PORT NOTE (parity tail S7-3): allocator_api2 containers bind their allocator
//! at construction. Where the reference grows a list through an allocator it
//! looks up at call time (`Terminal.pwd`/`title` grow through `self.gpa()`,
//! the ACTIVE screen's allocator, and the reference's own C-API test swaps that
//! allocator to make the growth fail; `UntouchedPool`'s test swaps `pool.gpa`),
//! binding would change which allocator is called. This type keeps the
//! reference's per-call allocator.
//!
//! Growth is Zig 0.16's: `ensureTotalCapacity` rounds up with `growCapacity`
//! (`minimum +| (minimum / 2 + init_capacity)`, `init_capacity =
//! max(1, cache_line / @sizeOf(T))`) and `ensureTotalCapacityPrecise` tries
//! `remap` first, then allocates, copies and frees (the S7 handle's `grow`).

use core::alloc::Layout;
use core::ptr::NonNull;

use allocator_api2::alloc::Allocator;

pub use crate::limited_allocator::OutOfMemory;

/// `std.atomic.cache_line` (Zig's `ArrayList` sizes `init_capacity` with it):
/// the reference's per-target table.
const CACHE_LINE: usize = rustty_zigstd::atomic::CACHE_LINE;

/// See the module docs.
pub struct ArrayListUnmanaged<T: Copy> {
    ptr: NonNull<T>,
    len: usize,
    cap: usize,
}

// SAFETY: the list owns its `Copy` elements outright; the raw pointer is a
// unique owning pointer, as a `Vec`'s is.
unsafe impl<T: Copy + Send> Send for ArrayListUnmanaged<T> {}
// SAFETY: shared access only reads the elements.
unsafe impl<T: Copy + Sync> Sync for ArrayListUnmanaged<T> {}

impl<T: Copy> Default for ArrayListUnmanaged<T> {
    fn default() -> Self {
        Self::EMPTY
    }
}

impl<T: Copy> core::ops::Deref for ArrayListUnmanaged<T> {
    type Target = [T];
    #[inline]
    fn deref(&self) -> &[T] {
        self.items()
    }
}

impl<T: Copy> ArrayListUnmanaged<T> {
    /// `.empty`: allocates nothing.
    pub const EMPTY: ArrayListUnmanaged<T> = ArrayListUnmanaged {
        ptr: NonNull::dangling(),
        len: 0,
        cap: 0,
    };

    /// `items`.
    #[inline]
    pub fn items(&self) -> &[T] {
        // SAFETY: `ptr` is valid for `len` initialized elements (dangling and
        // aligned when `len == 0`).
        unsafe { core::slice::from_raw_parts(self.ptr.as_ptr(), self.len) }
    }

    /// `items` (mutable).
    #[inline]
    pub fn items_mut(&mut self) -> &mut [T] {
        // SAFETY: as `items`, and `&mut self` is exclusive.
        unsafe { core::slice::from_raw_parts_mut(self.ptr.as_ptr(), self.len) }
    }

    /// `items.len`.
    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    /// `items.len == 0`.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// `capacity`.
    #[inline]
    pub fn capacity(&self) -> usize {
        self.cap
    }

    /// The start of the allocated slice, for raw writes.
    #[inline]
    pub fn as_mut_ptr(&mut self) -> *mut T {
        self.ptr.as_ptr()
    }

    /// `items.len = new_len`.
    ///
    /// # Safety
    /// `new_len <= capacity()`, and the first `new_len` elements are
    /// initialized before they are read.
    #[inline]
    pub unsafe fn set_len(&mut self, new_len: usize) {
        debug_assert!(new_len <= self.cap);
        self.len = new_len;
    }

    /// `clearRetainingCapacity`.
    #[inline]
    pub fn clear_retaining_capacity(&mut self) {
        self.len = 0;
    }

    /// `pop()`: remove and return the last element, if any.
    #[inline]
    pub fn pop(&mut self) -> Option<T> {
        if self.len == 0 {
            return None;
        }
        self.len -= 1;
        // SAFETY: index `len` (the old `len - 1`) is an initialized element.
        Some(unsafe { self.ptr.as_ptr().add(self.len).read() })
    }

    /// `appendAssumeCapacity(item)`.
    #[inline]
    pub fn append_assume_capacity(&mut self, item: T) {
        // The reference asserts this in `addOneAssumeCapacity`; here it also
        // guards the write below.
        assert!(self.len < self.cap);
        // SAFETY: `len < cap`, so the slot is inside the allocation.
        unsafe { self.ptr.as_ptr().add(self.len).write(item) };
        self.len += 1;
    }

    fn layout(cap: usize) -> Option<Layout> {
        Layout::array::<T>(cap).ok()
    }

    /// `growCapacity(minimum)`.
    #[inline]
    pub fn grow_capacity(minimum: usize) -> usize {
        let init_capacity = (CACHE_LINE / core::mem::size_of::<T>().max(1)).max(1);
        minimum.saturating_add(minimum / 2 + init_capacity)
    }

    /// `try list.ensureTotalCapacity(gpa, new_capacity)`.
    ///
    /// # Safety
    /// `gpa` is the allocator every earlier growth of this list went through
    /// (any allocator while the list is still `.empty`).
    #[inline]
    pub unsafe fn ensure_total_capacity<G: Allocator>(
        &mut self,
        gpa: &G,
        new_capacity: usize,
    ) -> Result<(), OutOfMemory> {
        if self.cap >= new_capacity {
            return Ok(());
        }
        // SAFETY: the caller's contract.
        unsafe { self.ensure_total_capacity_precise(gpa, Self::grow_capacity(new_capacity)) }
    }

    /// `try list.ensureTotalCapacityPrecise(gpa, new_capacity)`.
    ///
    /// # Safety
    /// As for [`ensure_total_capacity`](Self::ensure_total_capacity).
    pub unsafe fn ensure_total_capacity_precise<G: Allocator>(
        &mut self,
        gpa: &G,
        new_capacity: usize,
    ) -> Result<(), OutOfMemory> {
        if self.cap >= new_capacity {
            return Ok(());
        }
        let new_layout = Self::layout(new_capacity).ok_or(OutOfMemory)?;
        let new_memory = if self.cap == 0 {
            // `remap` of an empty allocation is null: `alignedAlloc`.
            gpa.allocate(new_layout).map_err(|_| OutOfMemory)?
        } else {
            let old_layout = Self::layout(self.cap).expect("a live allocation has a valid layout");
            // SAFETY: `ptr` is the live allocation of `old_layout` made
            // through `gpa` (the caller's contract: one allocator per list,
            // as in the reference), and `new_layout` is larger.
            unsafe { gpa.grow(self.ptr.cast(), old_layout, new_layout) }.map_err(|_| OutOfMemory)?
        };
        self.ptr = new_memory.cast();
        self.cap = new_capacity;
        Ok(())
    }

    /// `list.deinit(gpa)`, leaving the list `.empty` (the port's
    /// `self.* = undefined` is a reusable empty list).
    ///
    /// # Safety
    /// As for [`ensure_total_capacity`](Self::ensure_total_capacity).
    pub unsafe fn deinit<G: Allocator>(&mut self, gpa: &G) {
        if self.cap != 0 {
            let layout = Self::layout(self.cap).expect("a live allocation has a valid layout");
            // SAFETY: the live allocation of `layout`, made through `gpa`
            // (the caller's contract).
            unsafe { gpa.deallocate(self.ptr.cast(), layout) };
        }
        *self = Self::EMPTY;
    }
}
