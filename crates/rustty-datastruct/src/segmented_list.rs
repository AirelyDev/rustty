//! Code taken from 0.15.2 `std.segmented_list` (MIT license). See
//! lib/compat/README.md for license and details.
//!
//! PORTS: ghostty src/datastruct/segmented_list.zig
//!
//! PORT STATUS: ported (parity tail S7-7). `.port/phase-3/NON_PORTS.md` A1
//! recorded this file as "do NOT port as a file" because its one use in
//! libghostty-vt, the OSC color request `List`, could be a plain `Vec`. That
//! stopped being true once the list went through the caller's allocator
//! (S174-F3): the preallocated inline segment is what makes an OSC with at most
//! two color requests allocate NOTHING, and the shelves are what the caller's
//! allocator sees for more. A `Vec` allocates on the first push and grows by
//! doubling, so the caller would see calls the reference never makes.
//!
//! PORT NOTE (allocator binding): the reference is an UNMANAGED container:
//! every growing call and `deinit` take the allocator. Here the allocator is
//! bound at construction ([`SegmentedList::new_in`]) and dropping the list
//! frees it, the convention the S7 allocator-aware containers follow
//! (allocator_api2 binds at construction). The OSC color list lives in the OSC
//! parser (`color_requests`) and is dropped at the parser's `reset`, which must
//! free the shelves through the allocator the list grew with, whatever the
//! parser's `alloc` is by then. The calls the allocator sees, and their sizes
//! and order, are the reference's.
//!
//! PORT NOTE (element bound): `T: Copy`. The reference copies elements freely
//! (`append`, `pop`, `writeToSlice`) and never runs element destructors; its
//! only libghostty-vt instantiation is the OSC color `Request`, a plain value.

// const std = @import("std");
// const assert = @import("../quirks.zig").inlineAssert;
// const testing = std.testing;
// const mem = std.mem;
// const Allocator = std.mem.Allocator;
use allocator_api2::alloc::Allocator;
use core::alloc::Layout;
use core::mem::MaybeUninit;
use core::ptr::NonNull;

/// `Allocator.Error` (`error.OutOfMemory`).
pub use crate::limited_allocator::OutOfMemory;

// Imagine that `fn at(self: *Self, index: usize) &T` is a customer asking for a box
// from a warehouse, based on a flat array, boxes ordered from 0 to N - 1.
// But the warehouse actually stores boxes in shelves of increasing powers of 2 sizes.
// So when the customer requests a box index, we have to translate it to shelf index
// and box index within that shelf. Illustration:
//
// customer indexes:
// shelf 0:  0
// shelf 1:  1  2
// shelf 2:  3  4  5  6
// shelf 3:  7  8  9 10 11 12 13 14
// shelf 4: 15 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30
// shelf 5: 31 32 33 34 35 36 37 38 39 40 41 42 43 44 45 46 47 48 49 50 51 52 53 54 55 56 57 58 59 60 61 62
// ...
//
// warehouse indexes:
// shelf 0:  0
// shelf 1:  0  1
// shelf 2:  0  1  2  3
// shelf 3:  0  1  2  3  4  5  6  7
// shelf 4:  0  1  2  3  4  5  6  7  8  9 10 11 12 13 14 15
// shelf 5:  0  1  2  3  4  5  6  7  8  9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 31
// ...
//
// With this arrangement, here are the equations to get the shelf index and
// box index based on customer box index:
//
// shelf_index = floor(log2(customer_index + 1))
// shelf_count = ceil(log2(box_count + 1))
// box_index = customer_index + 1 - 2 ** shelf
// shelf_size = 2 ** shelf_index
//
// Now we complicate it a little bit further by adding a preallocated shelf, which must be
// a power of 2:
// prealloc=4
//
// customer indexes:
// prealloc:  0  1  2  3
//  shelf 0:  4  5  6  7  8  9 10 11
//  shelf 1: 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26 27
//  shelf 2: 28 29 30 31 32 33 34 35 36 37 38 39 40 41 42 43 44 45 46 47 48 49 50 51 52 53 54 55 56 57 58 59
// ...
//
// warehouse indexes:
// prealloc:  0  1  2  3
//  shelf 0:  0  1  2  3  4  5  6  7
//  shelf 1:  0  1  2  3  4  5  6  7  8  9 10 11 12 13 14 15
//  shelf 2:  0  1  2  3  4  5  6  7  8  9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 31
// ...
//
// Now the equations are:
//
// shelf_index = floor(log2(customer_index + prealloc)) - log2(prealloc) - 1
// shelf_count = ceil(log2(box_count + prealloc)) - log2(prealloc) - 1
// box_index = customer_index + prealloc - 2 ** (log2(prealloc) + 1 + shelf)
// shelf_size = prealloc * 2 ** (shelf_index + 1)

/// This is a stack data structure where pointers to indexes have the same lifetime as the data structure
/// itself, unlike ArrayList where append() invalidates all existing element pointers.
/// The tradeoff is that elements are not guaranteed to be contiguous. For that, use ArrayList.
/// Note however that most elements are contiguous, making this data structure cache-friendly.
///
/// Because it never has to copy elements from an old location to a new location, it does not require
/// its elements to be copyable, and it avoids wasting memory when backed by an ArenaAllocator.
/// Note that the append() and pop() convenience methods perform a copy, but you can instead use
/// addOne(), at(), setCapacity(), and shrinkCapacity() to avoid copying items.
///
/// This data structure has O(1) append and O(1) pop.
///
/// It supports preallocated elements, making it especially well suited when the expected maximum
/// size is small. `prealloc_item_count` must be 0, or a power of 2.
// pub fn SegmentedList(comptime T: type, comptime prealloc_item_count: usize) type {
pub struct SegmentedList<T: Copy, const PREALLOC: usize, A: Allocator> {
    // prealloc_segment: [prealloc_item_count]T = undefined,
    prealloc_segment: [MaybeUninit<T>; PREALLOC],
    // dynamic_segments: [][*]T = &[_][*]T{},
    // PORT NOTE: the `[][*]T` slice as a raw (ptr, len) pair; an empty slice
    // is a dangling pointer with length 0 and owns no allocation.
    dynamic_segments_ptr: NonNull<NonNull<T>>,
    dynamic_segments_len: usize,
    // len: usize = 0,
    pub len: usize,
    /// The bound allocator (see the module PORT NOTE).
    alloc: A,
}

// SAFETY: the list owns its elements and its shelves outright, as a `Vec`
// does; the raw pointers are unique owning pointers.
unsafe impl<T: Copy + Send, const P: usize, A: Allocator + Send> Send for SegmentedList<T, P, A> {}
// SAFETY: shared access only reads.
unsafe impl<T: Copy + Sync, const P: usize, A: Allocator + Sync> Sync for SegmentedList<T, P, A> {}

/// `std.math.log2_int(usize, x)`: floor(log2(x)), `x != 0`.
#[inline]
fn log2_int(x: usize) -> usize {
    assert!(x != 0);
    (usize::BITS - 1 - x.leading_zeros()) as usize
}

/// TODO look into why this std.math function was changed in
/// fc9430f56798a53f9393a697f4ccd6bf9981b970.
// fn log2_int_ceil(comptime T: type, x: T) std.math.Log2Int(T) {
fn log2_int_ceil(x: usize) -> usize {
    // assert(x != 0);
    assert!(x != 0);
    // const log2_val = std.math.log2_int(T, x);
    let log2_val = log2_int(x);
    // if (@as(T, 1) << log2_val == x)
    //     return log2_val;
    if 1usize << log2_val == x {
        return log2_val;
    }
    // return log2_val + 1;
    log2_val + 1
}

impl<T: Copy, const PREALLOC: usize, A: Allocator> SegmentedList<T, PREALLOC, A> {
    // const prealloc_exp: ShelfIndex = blk: { ... };
    const PREALLOC_EXP: usize = {
        // we don't use the prealloc_exp constant when prealloc_item_count is 0
        // but lazy-init may still be triggered by other code so supply a value
        if PREALLOC == 0 {
            0
        } else {
            assert!(PREALLOC.is_power_of_two());
            (usize::BITS - 1 - PREALLOC.leading_zeros()) as usize
        }
    };

    // pub const prealloc_count = prealloc_item_count;
    pub const PREALLOC_COUNT: usize = PREALLOC;

    /// `.{}`: an empty list bound to `alloc`. Allocates nothing.
    pub fn new_in(alloc: A) -> Self {
        // Force the power-of-two check for this instantiation.
        let _ = Self::PREALLOC_EXP;
        SegmentedList {
            prealloc_segment: [const { MaybeUninit::uninit() }; PREALLOC],
            dynamic_segments_ptr: NonNull::dangling(),
            dynamic_segments_len: 0,
            len: 0,
            alloc,
        }
    }

    /// The bound allocator.
    pub fn allocator(&self) -> &A {
        &self.alloc
    }

    /// The number of dynamic shelves (`dynamic_segments.len`).
    pub fn dynamic_segment_count(&self) -> usize {
        self.dynamic_segments_len
    }

    #[inline]
    fn segment(&self, shelf_index: usize) -> NonNull<T> {
        debug_assert!(shelf_index < self.dynamic_segments_len);
        // SAFETY: `shelf_index < dynamic_segments_len`, and the first
        // `dynamic_segments_len` entries of the segment array are initialized
        // shelf pointers.
        unsafe { *self.dynamic_segments_ptr.as_ptr().add(shelf_index) }
    }

    fn segments_layout(n: usize) -> Layout {
        Layout::array::<NonNull<T>>(n).expect("shelf array layout")
    }

    fn shelf_layout(shelf_index: usize) -> Layout {
        Layout::array::<T>(Self::shelf_size(shelf_index)).expect("shelf layout")
    }

    /// `allocator.free(self.dynamic_segments)`. Zig's `free` of an empty slice
    /// is a no-op.
    fn free_segments_array(&self, ptr: NonNull<NonNull<T>>, len: usize) {
        if len == 0 {
            return;
        }
        // SAFETY: a non-empty segment array was allocated through `alloc`
        // with exactly this layout.
        unsafe {
            self.alloc
                .deallocate(ptr.cast(), Self::segments_layout(len))
        };
    }

    // pub fn deinit(self: *Self, allocator: Allocator) void {
    /// Frees every shelf and the shelf array. The list is empty afterwards
    /// (the reference's `self.* = undefined`); dropping it again is a no-op.
    pub fn deinit(&mut self) {
        // self.freeShelves(allocator, @as(ShelfIndex, @intCast(self.dynamic_segments.len)), 0);
        self.free_shelves(self.dynamic_segments_len, 0);
        // allocator.free(self.dynamic_segments);
        self.free_segments_array(self.dynamic_segments_ptr, self.dynamic_segments_len);
        // self.* = undefined;
        self.dynamic_segments_ptr = NonNull::dangling();
        self.dynamic_segments_len = 0;
        self.len = 0;
    }

    // pub fn at(self: anytype, i: usize) AtType(@TypeOf(self)) {
    pub fn at(&self, i: usize) -> &T {
        // assert(i < self.len);
        assert!(i < self.len);
        // return self.uncheckedAt(i);
        // SAFETY: `i < len`, so the element is initialized.
        unsafe { &*self.unchecked_at(i) }
    }

    /// `at` through a mutable list.
    pub fn at_mut(&mut self, i: usize) -> &mut T {
        assert!(i < self.len);
        // SAFETY: `i < len`, so the element is initialized; `&mut self` is
        // exclusive.
        unsafe { &mut *self.unchecked_at_mut(i) }
    }

    pub fn count(&self) -> usize {
        self.len
    }

    /// `count()`, in the `Vec::len` spelling.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Whether the list is empty.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    // pub fn append(self: *Self, allocator: Allocator, item: T) Allocator.Error!void {
    pub fn append(&mut self, item: T) -> Result<(), OutOfMemory> {
        // const new_item_ptr = try self.addOne(allocator);
        let new_item_ptr = self.add_one()?;
        // new_item_ptr.* = item;
        new_item_ptr.write(item);
        Ok(())
    }

    // pub fn appendSlice(self: *Self, allocator: Allocator, items: []const T) Allocator.Error!void {
    pub fn append_slice(&mut self, items: &[T]) -> Result<(), OutOfMemory> {
        for item in items {
            self.append(*item)?;
        }
        Ok(())
    }

    pub fn pop(&mut self) -> Option<T> {
        if self.len == 0 {
            return None;
        }

        let index = self.len - 1;
        // SAFETY: `index < len`, so the element is initialized.
        let result = unsafe { *self.unchecked_at(index) };
        self.len = index;
        Some(result)
    }

    // pub fn addOne(self: *Self, allocator: Allocator) Allocator.Error!*T {
    /// PORT NOTE: returns the new slot uninitialized (Zig's `undefined`); the
    /// caller writes it. `len` already counts it, as in the reference.
    pub fn add_one(&mut self) -> Result<&mut MaybeUninit<T>, OutOfMemory> {
        // const new_length = self.len + 1;
        let new_length = self.len + 1;
        // try self.growCapacity(allocator, new_length);
        self.grow_capacity(new_length)?;
        // const result = uncheckedAt(self, self.len);
        let result = self.unchecked_at_mut(self.len);
        // self.len = new_length;
        self.len = new_length;
        // return result;
        // SAFETY: the slot lies inside the capacity just ensured, and
        // `&mut self` is exclusive for the returned borrow.
        Ok(unsafe { &mut *result.cast::<MaybeUninit<T>>() })
    }

    /// Reduce length to `new_len`.
    /// Invalidates pointers for the elements at index new_len and beyond.
    pub fn shrink_retaining_capacity(&mut self, new_len: usize) {
        assert!(new_len <= self.len);
        self.len = new_len;
    }

    /// Invalidates all element pointers.
    pub fn clear_retaining_capacity(&mut self) {
        self.len = 0;
    }

    /// Invalidates all element pointers.
    pub fn clear_and_free(&mut self) {
        // self.setCapacity(allocator, 0) catch unreachable;
        self.set_capacity(0)
            .expect("setCapacity(0) never allocates");
        self.len = 0;
    }

    /// Grows or shrinks capacity to match usage.
    /// TODO update this and related methods to match the conventions set by ArrayList
    pub fn set_capacity(&mut self, new_capacity: usize) -> Result<(), OutOfMemory> {
        if PREALLOC != 0
            && new_capacity <= 1usize << (Self::PREALLOC_EXP + self.dynamic_segments_len)
        {
            self.shrink_capacity(new_capacity);
            return Ok(());
        }
        self.grow_capacity(new_capacity)
    }

    /// Only grows capacity, or retains current capacity.
    pub fn grow_capacity(&mut self, new_capacity: usize) -> Result<(), OutOfMemory> {
        // const new_cap_shelf_count = shelfCount(new_capacity);
        let new_cap_shelf_count = Self::shelf_count(new_capacity);
        // const old_shelf_count = @as(ShelfIndex, @intCast(self.dynamic_segments.len));
        let old_shelf_count = self.dynamic_segments_len;
        // if (new_cap_shelf_count <= old_shelf_count) return;
        if new_cap_shelf_count <= old_shelf_count {
            return Ok(());
        }

        // const new_dynamic_segments = try allocator.alloc([*]T, new_cap_shelf_count);
        let new_dynamic_segments: NonNull<NonNull<T>> = self
            .alloc
            .allocate(Self::segments_layout(new_cap_shelf_count))
            .map_err(|_| OutOfMemory)?
            .cast();
        // errdefer allocator.free(new_dynamic_segments);

        // var i: ShelfIndex = 0;
        // while (i < old_shelf_count) : (i += 1) {
        //     new_dynamic_segments[i] = self.dynamic_segments[i];
        // }
        let mut i = 0;
        while i < old_shelf_count {
            // SAFETY: both arrays hold at least `old_shelf_count` entries; the
            // old entries are initialized.
            unsafe {
                new_dynamic_segments
                    .as_ptr()
                    .add(i)
                    .write(*self.dynamic_segments_ptr.as_ptr().add(i));
            }
            i += 1;
        }
        // errdefer while (i > old_shelf_count) : (i -= 1) {
        //     allocator.free(new_dynamic_segments[i][0..shelfSize(i)]);
        // };
        // PORT NOTE (UD311, N322): the reference's cleanup starts at the shelf whose
        // allocation FAILED (index `i`, never written) and stops before
        // `old_shelf_count`, so it frees an undefined pointer and leaks the
        // first new shelf. It only runs when one call adds two or more shelves
        // and a later one fails; `addOne`, libghostty-vt's only caller, adds at
        // most one. The port frees exactly the shelves this call allocated.
        while i < new_cap_shelf_count {
            // new_dynamic_segments[i] = (try allocator.alloc(T, shelfSize(i))).ptr;
            match self.alloc.allocate(Self::shelf_layout(i)) {
                Ok(shelf) => {
                    // SAFETY: `i < new_cap_shelf_count`, inside the new array.
                    unsafe { new_dynamic_segments.as_ptr().add(i).write(shelf.cast()) };
                }
                Err(_) => {
                    let mut j = i;
                    while j > old_shelf_count {
                        j -= 1;
                        // SAFETY: shelves `old_shelf_count..i` were allocated
                        // above with their own layouts.
                        unsafe {
                            self.alloc.deallocate(
                                (*new_dynamic_segments.as_ptr().add(j)).cast(),
                                Self::shelf_layout(j),
                            )
                        };
                    }
                    self.free_segments_array(new_dynamic_segments, new_cap_shelf_count);
                    return Err(OutOfMemory);
                }
            }
            i += 1;
        }

        // allocator.free(self.dynamic_segments);
        self.free_segments_array(self.dynamic_segments_ptr, self.dynamic_segments_len);
        // self.dynamic_segments = new_dynamic_segments;
        self.dynamic_segments_ptr = new_dynamic_segments;
        self.dynamic_segments_len = new_cap_shelf_count;
        Ok(())
    }

    /// Only shrinks capacity or retains current capacity.
    /// It may fail to reduce the capacity in which case the capacity will remain unchanged.
    pub fn shrink_capacity(&mut self, new_capacity: usize) {
        if new_capacity <= PREALLOC {
            // const len = @as(ShelfIndex, @intCast(self.dynamic_segments.len));
            let len = self.dynamic_segments_len;
            // self.freeShelves(allocator, len, 0);
            self.free_shelves(len, 0);
            // allocator.free(self.dynamic_segments);
            self.free_segments_array(self.dynamic_segments_ptr, len);
            // self.dynamic_segments = &[_][*]T{};
            self.dynamic_segments_ptr = NonNull::dangling();
            self.dynamic_segments_len = 0;
            return;
        }

        let new_cap_shelf_count = Self::shelf_count(new_capacity);
        let old_shelf_count = self.dynamic_segments_len;
        // assert(new_cap_shelf_count <= old_shelf_count);
        assert!(new_cap_shelf_count <= old_shelf_count);
        if new_cap_shelf_count == old_shelf_count {
            return;
        }

        // freeShelves() must be called before resizing the dynamic
        // segments, but we don't know if resizing the dynamic segments
        // will work until we try it. So we must allocate a fresh memory
        // buffer in order to reduce capacity.
        // const new_dynamic_segments = allocator.alloc([*]T, new_cap_shelf_count) catch return;
        let new_dynamic_segments: NonNull<NonNull<T>> = match self
            .alloc
            .allocate(Self::segments_layout(new_cap_shelf_count))
        {
            Ok(p) => p.cast(),
            Err(_) => return,
        };
        // self.freeShelves(allocator, old_shelf_count, new_cap_shelf_count);
        self.free_shelves(old_shelf_count, new_cap_shelf_count);
        // if (allocator.resize(self.dynamic_segments, new_cap_shelf_count)) { ... } else {
        // PORT NOTE (T3): allocator_api2 has no in-place `resize`, so this
        // always takes the reference's "resize refused" arm. The Rust global
        // heap's zig adapter refuses every length change anyway (N273).
        // Good thing we allocated that new memory slice.
        // @memcpy(new_dynamic_segments, self.dynamic_segments[0..new_cap_shelf_count]);
        // SAFETY: both arrays hold at least `new_cap_shelf_count` entries, and
        // they are distinct allocations.
        unsafe {
            core::ptr::copy_nonoverlapping(
                self.dynamic_segments_ptr.as_ptr(),
                new_dynamic_segments.as_ptr(),
                new_cap_shelf_count,
            );
        }
        // allocator.free(self.dynamic_segments);
        self.free_segments_array(self.dynamic_segments_ptr, old_shelf_count);
        // self.dynamic_segments = new_dynamic_segments;
        self.dynamic_segments_ptr = new_dynamic_segments;
        self.dynamic_segments_len = new_cap_shelf_count;
    }

    pub fn shrink(&mut self, new_len: usize) {
        assert!(new_len <= self.len);
        // TODO take advantage of the new realloc semantics
        self.len = new_len;
    }

    pub fn write_to_slice(&self, dest: &mut [T], start: usize) {
        let end = start + dest.len();
        assert!(end <= self.len);

        let mut i = start;
        if end <= PREALLOC {
            // const src = self.prealloc_segment[i..end];
            // @memcpy(dest[i - start ..][0..src.len], src);
            for (d, s) in dest[i - start..][..end - i]
                .iter_mut()
                .zip(&self.prealloc_segment[i..end])
            {
                // SAFETY: indices below `len` are initialized.
                *d = unsafe { s.assume_init() };
            }
            return;
        } else if i < PREALLOC {
            // const src = self.prealloc_segment[i..];
            // @memcpy(dest[i - start ..][0..src.len], src);
            for (d, s) in dest[i - start..][..PREALLOC - i]
                .iter_mut()
                .zip(&self.prealloc_segment[i..])
            {
                // SAFETY: indices below `len` are initialized.
                *d = unsafe { s.assume_init() };
            }
            i = PREALLOC;
        }

        while i < end {
            let shelf_index = Self::shelf_index(i);
            let copy_start = Self::box_index(i, shelf_index);
            let copy_end = Self::shelf_size(shelf_index).min(copy_start + end - i);
            // const src = self.dynamic_segments[shelf_index][copy_start..copy_end];
            // @memcpy(dest[i - start ..][0..src.len], src);
            let n = copy_end - copy_start;
            // SAFETY: `[copy_start, copy_end)` lies in the shelf, below `len`
            // (so initialized); `dest` is a distinct borrow.
            unsafe {
                core::ptr::copy_nonoverlapping(
                    self.segment(shelf_index).as_ptr().add(copy_start),
                    dest[i - start..][..n].as_mut_ptr(),
                    n,
                );
            }
            i += copy_end - copy_start;
        }
    }

    /// `uncheckedAt`: the slot's address, READ-ONLY (it is derived from
    /// `&self`). Reading it is sound only below `len`; the slot must lie within
    /// the capacity. Writes go through [`SegmentedList::unchecked_at_mut`].
    ///
    /// PORT NOTE (parity tail S7-7b, review F1): the reference's `uncheckedAt`
    /// returns `*T` or `*const T` by the constness of `self` (`AtType`). A
    /// pointer projected from `&self` must not be written through (Stacked
    /// and Tree Borrows: the preallocated slots live inside the list), so
    /// the two constnesses are two functions.
    pub fn unchecked_at(&self, index: usize) -> *const T {
        if index < PREALLOC {
            return self.prealloc_segment[index].as_ptr();
        }
        let shelf_index = Self::shelf_index(index);
        let box_index = Self::box_index(index, shelf_index);
        // SAFETY: the caller keeps `index` within the capacity, so the box
        // lies inside that shelf's allocation.
        unsafe { self.segment(shelf_index).as_ptr().add(box_index) }
    }

    /// `uncheckedAt` through `*Self`: the slot's address, writable, projected
    /// from `&mut self` (see `unchecked_at`).
    pub fn unchecked_at_mut(&mut self, index: usize) -> *mut T {
        if index < PREALLOC {
            return self.prealloc_segment[index].as_mut_ptr();
        }
        let shelf_index = Self::shelf_index(index);
        let box_index = Self::box_index(index, shelf_index);
        // SAFETY: as `unchecked_at`; a shelf pointer comes from `allocate`
        // and is writable.
        unsafe { self.segment(shelf_index).as_ptr().add(box_index) }
    }

    /// A cursor slot's address for a const walk (read-only).
    fn slot_ptr_const(&self, slot: Slot) -> *const T {
        match slot {
            Slot::Prealloc(i) => self.prealloc_segment[i].as_ptr(),
            // SAFETY: the cursor only reaches boxes below `len`, inside
            // their shelf.
            Slot::Shelf(s, b) => unsafe { self.segment(s).as_ptr().add(b) },
        }
    }

    /// A cursor slot's address for a mutable walk, projected from `&mut self`.
    fn slot_ptr_mut(&mut self, slot: Slot) -> *mut T {
        match slot {
            Slot::Prealloc(i) => self.prealloc_segment[i].as_mut_ptr(),
            // SAFETY: as `slot_ptr_const`; shelf pointers are writable.
            Slot::Shelf(s, b) => unsafe { self.segment(s).as_ptr().add(b) },
        }
    }

    fn shelf_count(box_count: usize) -> usize {
        if PREALLOC == 0 {
            return log2_int_ceil(box_count + 1);
        }
        log2_int_ceil(box_count + PREALLOC) - Self::PREALLOC_EXP - 1
    }

    fn shelf_size(shelf_index: usize) -> usize {
        if PREALLOC == 0 {
            return 1usize << shelf_index;
        }
        1usize << (shelf_index + (Self::PREALLOC_EXP + 1))
    }

    fn shelf_index(list_index: usize) -> usize {
        if PREALLOC == 0 {
            return log2_int(list_index + 1);
        }
        log2_int(list_index + PREALLOC) - Self::PREALLOC_EXP - 1
    }

    fn box_index(list_index: usize, shelf_index: usize) -> usize {
        if PREALLOC == 0 {
            return (list_index + 1) - (1usize << shelf_index);
        }
        list_index + PREALLOC - (1usize << ((Self::PREALLOC_EXP + 1) + shelf_index))
    }

    fn free_shelves(&mut self, from_count: usize, to_count: usize) {
        let mut i = from_count;
        while i != to_count {
            i -= 1;
            // allocator.free(self.dynamic_segments[i][0..shelfSize(i)]);
            // SAFETY: shelf `i` was allocated through `alloc` with this layout.
            unsafe {
                self.alloc
                    .deallocate(self.segment(i).cast(), Self::shelf_layout(i))
            };
        }
    }

    // pub fn iterator(self: *Self, start_index: usize) Iterator {
    pub fn iterator(&mut self, start_index: usize) -> Iterator<'_, T, PREALLOC, A> {
        let mut it = Iterator {
            list: self,
            cursor: Cursor::default(),
        };
        it.set(start_index);
        it
    }

    // pub fn constIterator(self: *const Self, start_index: usize) ConstIterator {
    pub fn const_iterator(&self, start_index: usize) -> ConstIterator<'_, T, PREALLOC, A> {
        let mut it = ConstIterator {
            list: self,
            cursor: Cursor::default(),
        };
        it.set(start_index);
        it
    }

    /// A Rust iterator over the elements in order (the `constIterator(0)`
    /// walk).
    pub fn iter(&self) -> impl core::iter::Iterator<Item = &T> + '_ {
        let mut it = self.const_iterator(0);
        core::iter::from_fn(move || it.next())
    }
}

/// `list.at(i).*`, in the `Vec` indexing spelling.
impl<T: Copy, const PREALLOC: usize, A: Allocator> core::ops::Index<usize>
    for SegmentedList<T, PREALLOC, A>
{
    type Output = T;
    fn index(&self, i: usize) -> &T {
        self.at(i)
    }
}

impl<T: Copy, const PREALLOC: usize, A: Allocator> Drop for SegmentedList<T, PREALLOC, A> {
    fn drop(&mut self) {
        self.deinit();
    }
}

impl<T: Copy + core::fmt::Debug, const PREALLOC: usize, A: Allocator> core::fmt::Debug
    for SegmentedList<T, PREALLOC, A>
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

/// Element-wise equality (the list's allocator and shelf layout are not
/// compared).
impl<T: Copy + PartialEq, const PREALLOC: usize, A: Allocator> PartialEq
    for SegmentedList<T, PREALLOC, A>
{
    fn eq(&self, other: &Self) -> bool {
        self.len == other.len && self.iter().eq(other.iter())
    }
}

/// The iterator position shared by `Iterator` and `ConstIterator`
/// (`BaseIterator`'s fields other than `list`).
#[derive(Default, Clone, Copy)]
struct Cursor {
    index: usize,
    box_index: usize,
    shelf_index: usize,
    shelf_size: usize,
}

/// Where a cursor step landed: a preallocated slot or a shelf box.
enum Slot {
    Prealloc(usize),
    Shelf(usize, usize),
}

impl Cursor {
    fn next<const PREALLOC: usize>(&mut self, len: usize) -> Option<Slot> {
        if self.index >= len {
            return None;
        }
        if self.index < PREALLOC {
            let slot = Slot::Prealloc(self.index);
            self.index += 1;
            if self.index == PREALLOC {
                self.box_index = 0;
                self.shelf_index = 0;
                self.shelf_size = PREALLOC * 2;
            }
            return Some(slot);
        }

        let slot = Slot::Shelf(self.shelf_index, self.box_index);
        self.index += 1;
        self.box_index += 1;
        if self.box_index == self.shelf_size {
            self.shelf_index += 1;
            self.box_index = 0;
            self.shelf_size *= 2;
        }
        Some(slot)
    }

    fn prev<const PREALLOC: usize>(&mut self) -> Option<Slot> {
        if self.index == 0 {
            return None;
        }

        self.index -= 1;
        if self.index < PREALLOC {
            return Some(Slot::Prealloc(self.index));
        }

        if self.box_index == 0 {
            self.shelf_index -= 1;
            self.shelf_size /= 2;
            self.box_index = self.shelf_size - 1;
        } else {
            self.box_index -= 1;
        }

        Some(Slot::Shelf(self.shelf_index, self.box_index))
    }

    fn peek<const PREALLOC: usize>(&self, len: usize) -> Option<Slot> {
        if self.index >= len {
            return None;
        }
        if self.index < PREALLOC {
            return Some(Slot::Prealloc(self.index));
        }
        Some(Slot::Shelf(self.shelf_index, self.box_index))
    }
}

macro_rules! base_iterator {
    ($name:ident, $list:ty, $elem:ty, $deref:ident, $ptr:ty, $slot_ptr:ident, ($($m:tt)*)) => {
        // pub const Iterator = BaseIterator(*Self, *T);
        // pub const ConstIterator = BaseIterator(*const Self, *const T);
        pub struct $name<'a, T: Copy, const PREALLOC: usize, A: Allocator> {
            list: $list,
            cursor: Cursor,
        }

        impl<'a, T: Copy, const PREALLOC: usize, A: Allocator> $name<'a, T, PREALLOC, A> {
            // PORT NOTE (parity tail S7-7b, review F1): the mutable iterator
            // projects its slots through `&mut *self.list`, the const one
            // through `&*self.list`.
            fn slot_ptr(&$($m)* self, slot: Slot) -> $ptr {
                self.list.$slot_ptr(slot)
            }

            // `BaseIterator.next`; `Iterator` lends its elements (see below),
            // so it cannot be `core::iter::Iterator`.
            #[allow(clippy::should_implement_trait)]
            pub fn next(&mut self) -> Option<$elem> {
                let slot = self.cursor.next::<PREALLOC>(self.list.len)?;
                let p = self.slot_ptr(slot);
                // SAFETY: the slot is below `len`, so initialized.
                Some(unsafe { $deref(p) })
            }

            pub fn prev(&mut self) -> Option<$elem> {
                let slot = self.cursor.prev::<PREALLOC>()?;
                let p = self.slot_ptr(slot);
                // SAFETY: as `next`.
                Some(unsafe { $deref(p) })
            }

            pub fn peek(&mut self) -> Option<$elem> {
                let slot = self.cursor.peek::<PREALLOC>(self.list.len)?;
                let p = self.slot_ptr(slot);
                // SAFETY: as `next`.
                Some(unsafe { $deref(p) })
            }

            pub fn set(&mut self, index: usize) {
                self.cursor.index = index;
                if index < PREALLOC {
                    return;
                }
                self.cursor.shelf_index = SegmentedList::<T, PREALLOC, A>::shelf_index(index);
                self.cursor.box_index =
                    SegmentedList::<T, PREALLOC, A>::box_index(index, self.cursor.shelf_index);
                self.cursor.shelf_size =
                    SegmentedList::<T, PREALLOC, A>::shelf_size(self.cursor.shelf_index);
            }
        }
    };
}

/// # Safety
/// `p` points to an initialized `T` that outlives `'b` and is not mutated
/// while the returned borrow lives.
unsafe fn deref_const<'b, T>(p: *const T) -> &'b T {
    // SAFETY: forwarded to the caller.
    unsafe { &*p }
}

/// # Safety
/// `p` points to an initialized `T` that outlives `'b`, with no other live
/// borrow of it.
unsafe fn deref_mut<'b, T>(p: *mut T) -> &'b mut T {
    // SAFETY: forwarded to the caller.
    unsafe { &mut *p }
}

// PORT NOTE: `Iterator.next` returns `*T`; here the element borrow is tied to
// the `&mut` call (a lending iterator), so two elements cannot be held
// mutably at once. `ConstIterator` hands out `&'a T` for the list's borrow.
base_iterator!(
    Iterator,
    &'a mut SegmentedList<T, PREALLOC, A>,
    &mut T,
    deref_mut,
    *mut T,
    slot_ptr_mut,
    (mut)
);
base_iterator!(
    ConstIterator,
    &'a SegmentedList<T, PREALLOC, A>,
    &'a T,
    deref_const,
    *const T,
    slot_ptr_const,
    ()
);

#[cfg(test)]
mod tests {
    use super::*;
    use allocator_api2::alloc::Global;

    // test "basic usage" {
    #[test]
    fn basic_usage() {
        test_segmented_list::<0>();
        test_segmented_list::<1>();
        test_segmented_list::<2>();
        test_segmented_list::<4>();
        test_segmented_list::<8>();
        test_segmented_list::<16>();
    }

    // fn testSegmentedList(comptime prealloc: usize) !void {
    fn test_segmented_list<const PREALLOC: usize>() {
        // var list = SegmentedList(i32, prealloc){};
        // defer list.deinit(testing.allocator);
        let mut list = SegmentedList::<i32, PREALLOC, Global>::new_in(Global);

        {
            let mut i: usize = 0;
            while i < 100 {
                list.append(i32::try_from(i + 1).unwrap()).unwrap();
                assert!(list.len == i + 1);
                i += 1;
            }
        }

        {
            let mut i: usize = 0;
            while i < 100 {
                assert!(*list.at(i) == i32::try_from(i + 1).unwrap());
                i += 1;
            }
        }

        {
            let mut it = list.iterator(0);
            let mut x: i32 = 0;
            while let Some(item) = it.next() {
                x += 1;
                assert!(*item == x);
            }
            assert!(x == 100);
            while let Some(item) = it.prev() {
                assert!(*item == x);
                x -= 1;
            }
            assert!(x == 0);
        }

        {
            let mut it = list.const_iterator(0);
            let mut x: i32 = 0;
            while let Some(item) = it.next() {
                x += 1;
                assert!(*item == x);
            }
            assert!(x == 100);
            while let Some(item) = it.prev() {
                assert!(*item == x);
                x -= 1;
            }
            assert!(x == 0);
        }

        assert!(list.pop().unwrap() == 100);
        assert!(list.len == 99);

        list.append_slice(&[1, 2, 3]).unwrap();
        assert!(list.len == 102);
        assert!(list.pop().unwrap() == 3);
        assert!(list.pop().unwrap() == 2);
        assert!(list.pop().unwrap() == 1);
        assert!(list.len == 99);

        list.append_slice(&[]).unwrap();
        assert!(list.len == 99);

        {
            let mut i: i32 = 99;
            while let Some(item) = list.pop() {
                assert!(item == i);
                let len = list.len;
                list.shrink_capacity(len);
                i -= 1;
            }
        }

        {
            let mut control = [0i32; 100];
            let mut dest = [0i32; 100];

            let mut i: i32 = 0;
            while i < 100 {
                list.append(i + 1).unwrap();
                control[usize::try_from(i).unwrap()] = i + 1;
                i += 1;
            }

            dest.fill(0);
            list.write_to_slice(&mut dest[..], 0);
            assert!(control[..] == dest[..]);

            dest.fill(0);
            list.write_to_slice(&mut dest[50..], 50);
            assert!(control[50..] == dest[50..]);
        }

        list.set_capacity(0).unwrap();
    }

    // Parity tail S7-7b (review F1; Rust-only, the Miri target): every WRITE
    // path, through both the preallocated slots and a shelf. `add_one` and
    // `append` write new slots, `at_mut` and the mutable `Iterator` overwrite
    // them, and the const walks read them back. Isolated and FFI-free, so
    // `cargo +nightly miri test -p rustty-datastruct --lib --
    // segmented_list::tests::mutating_paths_through_prealloc_and_shelves`
    // checks the pointer projections under Stacked and Tree Borrows.
    #[test]
    fn mutating_paths_through_prealloc_and_shelves() {
        let mut list = SegmentedList::<u32, 2, Global>::new_in(Global);
        for v in 0..7u32 {
            list.add_one().unwrap().write(v);
        }
        list.append(7).unwrap();
        // Slots 0-1 are the preallocated segment, 2-5 shelf 0, 6-7 shelf 1.
        for i in 0..list.count() {
            *list.at_mut(i) += 100;
        }
        {
            let mut it = list.iterator(0);
            while let Some(v) = it.next() {
                *v *= 2;
            }
            while let Some(v) = it.prev() {
                *v += 1;
            }
            it.set(1);
            *it.peek().unwrap() += 1000;
        }
        let expected: Vec<u32> = (0..8u32)
            .map(|v| (v + 100) * 2 + 1 + if v == 1 { 1000 } else { 0 })
            .collect();
        assert_eq!(expected, list.iter().copied().collect::<Vec<_>>());
        let mut it = list.const_iterator(0);
        assert_eq!(Some(&expected[0]), it.next());
        assert_eq!(Some(expected[7]), list.pop());
        // SAFETY: index 0 is below `len`.
        assert_eq!(expected[0], unsafe { *list.unchecked_at(0) });
        let mut out = [0u32; 7];
        list.write_to_slice(&mut out, 0);
        assert_eq!(expected[..7], out[..]);
    }

    // test "clearRetainingCapacity" {
    #[test]
    fn clear_retaining_capacity() {
        let mut list = SegmentedList::<i32, 1, Global>::new_in(Global);

        list.append_slice(&[4, 5]).unwrap();
        list.clear_retaining_capacity();
        list.append(6).unwrap();
        assert!(*list.at(0) == 6);
        assert!(list.len == 1);
        list.clear_retaining_capacity();
        assert!(list.len == 0);
    }

    /// A counting allocator over the global heap: every call and its layout,
    /// with an optional refusal of the n-th `allocate` (0-based).
    struct Counting {
        log: core::cell::RefCell<Vec<(&'static str, usize, usize)>>,
        live: core::cell::Cell<usize>,
        calls: core::cell::Cell<usize>,
        fail_at: Option<usize>,
    }

    impl Counting {
        fn new(fail_at: Option<usize>) -> Counting {
            Counting {
                log: Default::default(),
                live: Default::default(),
                calls: Default::default(),
                fail_at,
            }
        }
    }

    // SAFETY: forwards to `Global`, recording each call.
    unsafe impl Allocator for &Counting {
        fn allocate(
            &self,
            layout: Layout,
        ) -> Result<NonNull<[u8]>, allocator_api2::alloc::AllocError> {
            let n = self.calls.get();
            self.calls.set(n + 1);
            if self.fail_at == Some(n) {
                self.log
                    .borrow_mut()
                    .push(("refused", layout.size(), layout.align()));
                return Err(allocator_api2::alloc::AllocError);
            }
            self.log
                .borrow_mut()
                .push(("alloc", layout.size(), layout.align()));
            self.live.set(self.live.get() + layout.size());
            Global.allocate(layout)
        }

        unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
            self.log
                .borrow_mut()
                .push(("free", layout.size(), layout.align()));
            self.live.set(self.live.get() - layout.size());
            // SAFETY: forwarded from the caller.
            unsafe { Global.deallocate(ptr, layout) }
        }
    }

    // Parity tail S7-7 (no reference test): the calls an allocator sees are the
    // reference's. Two elements fit the preallocated segment and allocate
    // nothing; the third allocates the one-entry shelf array and shelf 0
    // (`prealloc * 2` elements); the seventh allocates a two-entry array and
    // shelf 1 (8 elements) and frees the old array; `deinit` frees the shelves
    // from the last, then the array.
    #[test]
    fn allocator_calls_follow_the_reference_shelf_growth() {
        let counting = Counting::new(None);
        let p = core::mem::size_of::<usize>();
        {
            let mut list = SegmentedList::<u64, 2, &Counting>::new_in(&counting);
            list.append(1).unwrap();
            list.append(2).unwrap();
            assert!(counting.log.borrow().is_empty(), "two fit the prealloc");
            list.append(3).unwrap();
            assert_eq!(
                vec![("alloc", p, p), ("alloc", 4 * 8, 8)],
                *counting.log.borrow()
            );
            for v in 4..=6 {
                list.append(v).unwrap();
            }
            assert_eq!(2, counting.log.borrow().len(), "shelf 0 holds 3..=6");
            list.append(7).unwrap();
            assert_eq!(
                vec![
                    ("alloc", p, p),
                    ("alloc", 4 * 8, 8),
                    ("alloc", 2 * p, p),
                    ("alloc", 8 * 8, 8),
                    ("free", p, p),
                ],
                *counting.log.borrow()
            );
            assert_eq!(
                (1..=7).collect::<Vec<u64>>(),
                list.iter().copied().collect::<Vec<_>>()
            );
        }
        assert_eq!(
            vec![("free", 8 * 8, 8), ("free", 4 * 8, 8), ("free", 2 * p, p)],
            counting.log.borrow()[5..].to_vec()
        );
        assert_eq!(0, counting.live.get());
    }

    // Parity tail S7-7 (no reference test): a refused shelf allocation fails the
    // append with `error.OutOfMemory`, frees the new shelf array, and leaves the
    // list as it was; nothing leaks.
    #[test]
    fn a_refused_shelf_leaves_the_list_unchanged() {
        // Call 0 is the shelf array, call 1 is shelf 0.
        let counting = Counting::new(Some(1));
        {
            let mut list = SegmentedList::<u64, 2, &Counting>::new_in(&counting);
            list.append(1).unwrap();
            list.append(2).unwrap();
            assert_eq!(Err(OutOfMemory), list.append(3));
            assert_eq!(2, list.count());
            assert_eq!(0, list.dynamic_segment_count());
            assert_eq!(0, counting.live.get(), "the new shelf array was freed");
            // A retry allocates again.
            list.append(3).unwrap();
            assert_eq!(3, list.count());
            assert_eq!(3, *list.at(2));
        }
        assert_eq!(0, counting.live.get());
    }

    // Parity tail S7-7, N322: when one `growCapacity` adds several shelves and a
    // later one is refused, the shelves this call already allocated are freed
    // (the reference's cleanup frees the refused, never-written slot instead
    // and leaks the first new shelf; see the note in `grow_capacity`).
    #[test]
    fn a_refused_shelf_in_a_multi_shelf_growth_frees_the_new_shelves() {
        // Call 0 is the 3-entry shelf array, calls 1 and 2 are shelves 0 and 1,
        // call 3 (shelf 2) is refused.
        let counting = Counting::new(Some(3));
        {
            let mut list = SegmentedList::<u64, 2, &Counting>::new_in(&counting);
            assert_eq!(Err(OutOfMemory), list.grow_capacity(2 + 4 + 8 + 16));
            assert_eq!(0, list.dynamic_segment_count());
            assert_eq!(
                vec![
                    (
                        "alloc",
                        3 * core::mem::size_of::<usize>(),
                        core::mem::size_of::<usize>()
                    ),
                    ("alloc", 4 * 8, 8),
                    ("alloc", 8 * 8, 8),
                    ("refused", 16 * 8, 8),
                    ("free", 8 * 8, 8),
                    ("free", 4 * 8, 8),
                    (
                        "free",
                        3 * core::mem::size_of::<usize>(),
                        core::mem::size_of::<usize>()
                    ),
                ],
                *counting.log.borrow()
            );
            assert_eq!(
                0,
                counting.live.get(),
                "shelves 0 and 1 and the array freed"
            );
        }
        assert_eq!(0, counting.live.get());
    }
}
