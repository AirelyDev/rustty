//! PORT NOTE: Ports ghostty `src/datastruct/untouched_pool.zig` (upstream
//! `e01e75bbb`).
//!
//! A fixed-size item pool whose bookkeeping never touches item memory.
//!
//! std.heap.MemoryPool keeps its free list inside the items intrusively,
//! meaning it has to touch allocated item memory. For memory that is
//! demand-paged, that write forces the OS to create a physical page
//! for nothing (to mark it free!).
//!
//! This pool keeps the free list in a separate array allocated from a
//! general-purpose allocator and allocates every item individually from
//! the item allocator with the requested alignment. The pool never reads
//! or writes an item until it is needed.
//!
//! Contracts:
//!
//!   - The pool never zeroes. Callers that need zeroed items must use an
//!     item allocator that returns zeroed memory and must zero (or
//!     decommit) an item before destroy().
//!   - destroy() never allocates and never fails: create() reserves a
//!     free-list slot for every live item before allocating a new one.
//!   - The pool tracks free items only. Every item must be returned with
//!     destroy() or release() before reset() or deinit(), or it leaks.
//!     This is asserted in safe builds.
//!
//! The tradeoff is that this isn't as fast as std.heap.MemoryPool, its
//! not as cache friendly. But benchmarks show that the cost is minimal
//! and if the tradeoff of not touching the memory is important, then
//! this pays off.
//
// PORT NOTE (shapes): the reference's `UntouchedPool(Item, alignment)` returns
// a type whose item size is comptime. Rustty needs two shapes of it, one
// implementation ([`UntouchedPoolBase`]) over an [`ItemShape`]:
//
//   - [`UntouchedPool<T, A, G, ALIGN>`](UntouchedPool): `@sizeOf(Item)` and
//     the alignment are constants (PageList's node pool,
//     `UntouchedPool(List.Node, .of(List.Node))`). `ALIGN = 0` is
//     `.of(Item)`, the natural alignment.
//   - [`DynamicUntouchedPool<A, G>`](DynamicUntouchedPool): the item size and
//     alignment are runtime values (PageList's page pool,
//     `UntouchedPool([std_size]u8, .fromByteUnits(page_size_min))`). Rustty's
//     `std_size()` is runtime (it depends on `cfg!(test)` grapheme bytes), the
//     deviation `rustty_zigstd::memory_pool::DynamicMemoryPoolAligned` already
//     records; items are raw `NonNull<u8>` buffers.
//
// PORT NOTE (allocators): Zig's `std.mem.Allocator` is a runtime `{ptr,
// vtable}` value; here both allocators are generic allocator_api2 handles
// stored by value. `A` is the item allocator (`allocator`), `G` the
// general-purpose allocator for the free list (`gpa`, defaulting to `A`).
// PageList passes the page backing allocator and the S7 `VtAllocator`
// (P7-W1 T5; DECISIONS R13 makes the page allocator a generic parameter
// there, defaulting to `PageBackingAlloc`).
//
// PORT NOTE (field access): upstream's fields are public and its test swaps
// `pool.gpa`. Here the fields are private behind getters: from safe code, a
// swapped allocator would grow or free memory another allocator made, and a
// pointer pushed onto `free` would be freed as an item. The gpa swap is the
// `unsafe` [`set_gpa`](UntouchedPoolBase::set_gpa).
//
// PORT NOTE (free list): `std.ArrayList(ItemPtr)` is the port's one unmanaged
// list, [`crate::array_list::ArrayListUnmanaged`] (the S7 T3 call shape that
// rustty-vt's `alloc::ArrayListUnmanaged` also wraps), grown and freed through
// `gpa` per call.
//
// PORT NOTE (Stacked Borrows, LIFETIMES.tsv P7-W1 T4 rows): the free list
// holds raw `NonNull` item pointers and the pool never forms a `&T`/`&mut T`
// to an item, never reads one and never writes one. Items are allocated and
// deallocated through the item allocator only, so a caller's raw aliases into
// a live item stay valid until it hands the item back.

use core::alloc::Layout;
use core::marker::PhantomData;
use core::mem::{align_of, size_of};
use core::ptr::NonNull;

use allocator_api2::alloc::Allocator;

use crate::array_list::ArrayListUnmanaged;
pub use crate::array_list::OutOfMemory;
/// `pub const ResetMode = std.heap.ArenaAllocator.ResetMode;`
pub use rustty_zigstd::arena_allocator::ResetMode;

/// The item size and alignment of an [`UntouchedPoolBase`]: comptime
/// ([`Fixed`]) or runtime ([`Dynamic`]).
pub trait ItemShape {
    /// The pointee of `ItemPtr`.
    type Item;

    /// `item_size` and `item_alignment`, as one layout.
    fn layout(&self) -> Layout;
}

/// The comptime shape: `Item` with `alignment.max(.of(Item))`, where
/// `ALIGN = 0` is `.of(Item)`.
pub struct Fixed<T, const ALIGN: usize>(PhantomData<fn() -> T>);

impl<T, const ALIGN: usize> Fixed<T, ALIGN> {
    /// `pub const item_size = @sizeOf(Item);`
    pub const ITEM_SIZE: usize = size_of::<T>();

    /// `pub const item_alignment: Alignment = alignment.max(.of(Item));`
    pub const ITEM_ALIGNMENT: usize = if ALIGN > align_of::<T>() {
        ALIGN
    } else {
        align_of::<T>()
    };

    /// `alignment` is a Zig `Alignment`: a power of two (`0` here meaning
    /// `.of(Item)`). A compile error otherwise, even when `.of(Item)` is larger.
    const ALIGN_IS_VALID: () = assert!(
        ALIGN == 0 || ALIGN.is_power_of_two(),
        "UntouchedPool: ALIGN must be 0 or a power of two"
    );

    /// Both, as one layout.
    pub const LAYOUT: Layout = {
        #[allow(clippy::let_unit_value)]
        let () = Self::ALIGN_IS_VALID;
        match Layout::from_size_align(Self::ITEM_SIZE, Self::ITEM_ALIGNMENT) {
            Ok(layout) => layout,
            Err(_) => panic!("UntouchedPool: the alignment must be a power of two"),
        }
    };
}

impl<T, const ALIGN: usize> ItemShape for Fixed<T, ALIGN> {
    type Item = T;

    #[inline(always)]
    fn layout(&self) -> Layout {
        Self::LAYOUT
    }
}

/// The runtime shape: `[item_size]u8` buffers with a runtime alignment.
pub struct Dynamic {
    layout: Layout,
}

impl ItemShape for Dynamic {
    type Item = u8;

    #[inline(always)]
    fn layout(&self) -> Layout {
        self.layout
    }
}

/// `UntouchedPool(Item, alignment)` with a comptime item size.
pub type UntouchedPool<T, A, G = A, const ALIGN: usize = 0> =
    UntouchedPoolBase<Fixed<T, ALIGN>, A, G>;

/// `UntouchedPool([item_size]u8, alignment)` with a runtime item size (see the
/// module's PORT NOTE on shapes). Items are `NonNull<u8>` buffers.
pub type DynamicUntouchedPool<A, G = A> = UntouchedPoolBase<Dynamic, A, G>;

/// The pool (see the module docs); use it through [`UntouchedPool`] or
/// [`DynamicUntouchedPool`].
pub struct UntouchedPoolBase<S: ItemShape, A: Allocator, G: Allocator> {
    /// The allocator items are allocated from.
    allocator: A,

    /// The general-purpose allocator for the free list.
    gpa: G,

    /// Free items. The most recently destroyed item is handed out first.
    free: ArrayListUnmanaged<NonNull<S::Item>>,

    /// Number of items currently allocated from the item allocator,
    /// free or in use. `free.capacity >= live` always holds so that
    /// destroy() never has to grow the free list.
    live: usize,

    shape: S,
}

impl<T, A: Allocator, G: Allocator, const ALIGN: usize> UntouchedPoolBase<Fixed<T, ALIGN>, A, G> {
    /// `pub const item_size = @sizeOf(Item);`
    pub const ITEM_SIZE: usize = Fixed::<T, ALIGN>::ITEM_SIZE;

    /// `pub const item_alignment: Alignment = alignment.max(.of(Item));`
    pub const ITEM_ALIGNMENT: usize = Fixed::<T, ALIGN>::ITEM_ALIGNMENT;

    /// Create a pool with `preheat` items already allocated.
    pub fn init_capacity(gpa: G, item_alloc: A, preheat: usize) -> Result<Self, OutOfMemory> {
        #[allow(clippy::let_unit_value)]
        let () = Fixed::<T, ALIGN>::ALIGN_IS_VALID;
        Self::init_capacity_shape(gpa, item_alloc, Fixed(PhantomData), preheat)
    }
}

impl<A: Allocator, G: Allocator> UntouchedPoolBase<Dynamic, A, G> {
    /// Create a pool of `item_size`-byte items aligned to `alignment` (a power
    /// of two; `item_alignment` is `alignment.max(.of(u8))`, i.e. `alignment`),
    /// with `preheat` items already allocated.
    pub fn init_capacity(
        gpa: G,
        item_alloc: A,
        item_size: usize,
        alignment: usize,
        preheat: usize,
    ) -> Result<Self, OutOfMemory> {
        // PORT NOTE: `[item_size]u8` is never zero-sized for a pool upstream
        // (a page); a zero size would divide by zero in `reset`.
        assert!(item_size > 0);
        let layout = Layout::from_size_align(item_size, alignment)
            .expect("UntouchedPool: the alignment must be a power of two");
        Self::init_capacity_shape(gpa, item_alloc, Dynamic { layout }, preheat)
    }

    /// `item_size` (the runtime value).
    #[inline]
    pub fn item_size(&self) -> usize {
        self.shape.layout.size()
    }

    /// `item_alignment` (the runtime value).
    #[inline]
    pub fn item_alignment(&self) -> usize {
        self.shape.layout.align()
    }
}

impl<S: ItemShape, A: Allocator, G: Allocator> UntouchedPoolBase<S, A, G> {
    /// `allocator`: the allocator items are allocated from.
    #[inline]
    pub fn allocator(&self) -> &A {
        &self.allocator
    }

    /// `gpa`: the general-purpose allocator for the free list.
    #[inline]
    pub fn gpa(&self) -> &G {
        &self.gpa
    }

    /// `pool.gpa = gpa`, returning the previous one.
    ///
    /// # Safety
    /// Until the previous gpa is restored, the pool must not grow or free its
    /// free list: no `create()` that finds the free list empty, no `deinit()`,
    /// no `Drop`. (`destroy()`, `release()`, `reset()` and a `create()` that
    /// pops a free item never call the gpa.) The one exception: the free list
    /// has never allocated (capacity 0).
    #[inline]
    pub unsafe fn set_gpa(&mut self, gpa: G) -> G {
        core::mem::replace(&mut self.gpa, gpa)
    }

    /// `free`: the free items, most recently destroyed last.
    #[inline]
    pub fn free(&self) -> &ArrayListUnmanaged<NonNull<S::Item>> {
        &self.free
    }

    /// `live`: the number of items allocated from the item allocator, free or
    /// in use.
    #[inline]
    pub fn live(&self) -> usize {
        self.live
    }

    /// `initCapacity(gpa, item_alloc, preheat)` for either shape.
    fn init_capacity_shape(
        gpa: G,
        item_alloc: A,
        shape: S,
        preheat: usize,
    ) -> Result<Self, OutOfMemory> {
        let mut pool = UntouchedPoolBase {
            allocator: item_alloc,
            gpa,
            free: ArrayListUnmanaged::EMPTY,
            live: 0,
            shape,
        };
        // errdefer self.deinit(); (`Drop`: every item allocated so far is on
        // the free list, so `live == free.len` holds on every failure path.)

        // SAFETY: the list is empty; `pool.gpa` is the list's gpa from now on.
        unsafe { pool.free.ensure_total_capacity_precise(&pool.gpa, preheat) }?;
        for _ in 0..preheat {
            let item = pool.alloc_item()?;
            pool.free.append_assume_capacity(item);
        }

        Ok(pool)
    }

    /// Free all items and the free list. Every item must have been
    /// returned with destroy() or release().
    //
    // PORT NOTE: the reference ends with `self.* = undefined`; here the pool is
    // left empty, so the `Drop` that follows is a no-op.
    pub fn deinit(&mut self) {
        assert!(self.live == self.free.len());
        let layout = self.shape.layout();
        for &item in self.free.items() {
            // SAFETY: every free item was allocated through `allocator` with
            // `layout` and is owned by the pool; it is freed exactly once here.
            unsafe { self.allocator.deallocate(item.cast(), layout) };
        }
        // SAFETY: `gpa` grew the list (`set_gpa`'s contract).
        unsafe { self.free.deinit(&self.gpa) };
        self.live = 0;
    }

    /// Free items so that the retained free items fit `mode`. Every
    /// item must have been returned with destroy() or release().
    ///
    /// This mirrors std.heap.MemoryPool.reset; it always succeeds
    /// (returns true) because nothing is reallocated.
    pub fn reset(&mut self, mode: ResetMode) -> bool {
        assert!(self.live == self.free.len());
        let retain_bytes: usize = match mode {
            ResetMode::FreeAll => 0,
            ResetMode::RetainCapacity => return true,
            ResetMode::RetainWithLimit(limit) => limit,
        };

        let layout = self.shape.layout();
        let retain_items = retain_bytes / layout.size();
        while self.free.len() > retain_items {
            let item = self
                .free
                .pop()
                .expect("the free list is longer than retain_items");
            // SAFETY: a free item allocated through `allocator` with `layout`,
            // just removed from the free list, so it is freed exactly once.
            unsafe { self.allocator.deallocate(item.cast(), layout) };
            self.live -= 1;
        }

        true
    }

    /// Get an item. This pops a free item without touching it or,
    /// when none is free, allocates a new one from the item
    /// allocator.
    pub fn create(&mut self) -> Result<NonNull<S::Item>, OutOfMemory> {
        if let Some(item) = self.free.pop() {
            return Ok(item);
        }

        // Reserve the free-list slot for the new item before
        // allocating it so that destroy() can never fail.
        let min = self.live.checked_add(1).expect("live item count overflow");
        // SAFETY: `gpa` grew the list (`set_gpa`'s contract).
        unsafe { self.free.ensure_total_capacity(&self.gpa, min) }?;
        self.alloc_item()
    }

    /// Return an item to the free list for reuse. The item is not
    /// modified or zeroed so it is up to the caller.
    ///
    /// # Safety
    /// `item` was returned by this pool's `create()` and has not been handed
    /// back since (with `destroy` or `release`).
    #[inline]
    pub unsafe fn destroy(&mut self, item: NonNull<S::Item>) {
        assert!(self.free.len() < self.live);
        self.free.append_assume_capacity(item);
    }

    /// Return an item straight to the item allocator instead of the
    /// free list. This is for teardown paths that would otherwise
    /// have to zero an item only for it to be freed moments later.
    ///
    /// # Safety
    /// As for [`destroy`](Self::destroy); the item is not used afterward.
    pub unsafe fn release(&mut self, item: NonNull<S::Item>) {
        assert!(self.free.len() < self.live);
        // SAFETY: the caller's contract: `item` is a live item this pool
        // allocated through `allocator` with this layout.
        unsafe { self.allocator.deallocate(item.cast(), self.shape.layout()) };
        self.live -= 1;
    }

    fn alloc_item(&mut self) -> Result<NonNull<S::Item>, OutOfMemory> {
        assert!(self.free.capacity() > self.live);
        let memory = self
            .allocator
            .allocate(self.shape.layout())
            .map_err(|_| OutOfMemory)?;
        self.live += 1;
        Ok(memory.cast())
    }
}

impl<S: ItemShape, A: Allocator, G: Allocator> Drop for UntouchedPoolBase<S, A, G> {
    /// `deinit` for a pool that was not deinitialized explicitly, and the
    /// reference's `errdefer self.deinit()` in `initCapacity`.
    //
    // PORT NOTE: while a panic is unwinding, a failing `live == free.len`
    // assert is skipped (the pool leaks instead of aborting the process with
    // a second panic, so the first panic is the one reported).
    fn drop(&mut self) {
        if std::thread::panicking() && self.live != self.free.len() {
            return;
        }
        self.deinit();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use allocator_api2::alloc::{AllocError, Allocator, Global};
    use core::alloc::Layout;
    use core::cell::Cell;
    use core::ptr::NonNull;
    use rustty_zigstd::heap::PAGE_SIZE_MIN;

    /// Test item: one minimum OS page, page-aligned, like a terminal page.
    type TestPool<'a> =
        UntouchedPool<[u8; PAGE_SIZE_MIN], &'a dyn Allocator, &'a dyn Allocator, PAGE_SIZE_MIN>;

    /// `TestPool.item_size`.
    const ITEM_SIZE: usize = <TestPool<'static>>::ITEM_SIZE;

    /// `std.testing.allocator`. Leak detection is Miri's job (R6).
    fn testing_allocator() -> &'static dyn Allocator {
        &Global
    }

    /// `std.mem.allEqual(u8, bytes, value)` over an item.
    fn item_all_equal(item: NonNull<[u8; PAGE_SIZE_MIN]>, value: u8) -> bool {
        // SAFETY: `item` is a live, fully initialized item handed out by the
        // test's pool; the shared view ends before the item is next used.
        let bytes = unsafe { &*item.as_ptr() };
        bytes.iter().all(|&b| b == value)
    }

    /// PORT NOTE: `std.heap.FixedBufferAllocator` over a raw backing buffer, as
    /// much of it as the test observes (`end_index`, the bump allocation, and
    /// `free` reclaiming only the last allocation). Every pointer it hands out
    /// is derived from the one raw `buffer` pointer, so the test's writes
    /// through that same pointer never invalidate an item (Stacked Borrows).
    struct FixedBufferAllocator {
        buffer: *mut u8,
        len: usize,
        end_index: Cell<usize>,
    }

    impl FixedBufferAllocator {
        fn init(buffer: *mut u8, len: usize) -> FixedBufferAllocator {
            FixedBufferAllocator {
                buffer,
                len,
                end_index: Cell::new(0),
            }
        }
    }

    // SAFETY: every block returned lies inside `buffer`, is disjoint from every
    // other live block (the index only moves past handed-out bytes, and moves
    // back only over the last one when it is freed), and is aligned as asked.
    unsafe impl Allocator for FixedBufferAllocator {
        fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
            let end = self.end_index.get();
            let addr = (self.buffer as usize).checked_add(end).ok_or(AllocError)?;
            let adjust_off = addr.next_multiple_of(layout.align()) - addr;
            let adjusted_index = end + adjust_off;
            let new_end_index = adjusted_index
                .checked_add(layout.size())
                .ok_or(AllocError)?;
            if new_end_index > self.len {
                return Err(AllocError);
            }
            self.end_index.set(new_end_index);
            // SAFETY: `adjusted_index + size <= len`, inside the buffer.
            let ptr = unsafe { NonNull::new_unchecked(self.buffer.add(adjusted_index)) };
            Ok(NonNull::slice_from_raw_parts(ptr, layout.size()))
        }

        unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
            // `if (!self.isLastAllocation(buf)) return;`
            let end = self.end_index.get();
            if ptr.as_ptr() as usize + layout.size() != self.buffer as usize + end {
                return;
            }
            self.end_index.set(end - layout.size());
        }
    }

    /// PORT NOTE: `std.testing.FailingAllocator` over the global heap, as much
    /// of it as the test uses: every allocation at or past `fail_index` fails
    /// and is recorded (`has_induced_failure`).
    struct FailingAllocator {
        fail_index: usize,
        alloc_index: Cell<usize>,
        has_induced_failure: Cell<bool>,
    }

    impl FailingAllocator {
        fn init(fail_index: usize) -> FailingAllocator {
            FailingAllocator {
                fail_index,
                alloc_index: Cell::new(0),
                has_induced_failure: Cell::new(false),
            }
        }
    }

    // SAFETY: every block comes from, and returns to, the global heap.
    unsafe impl Allocator for FailingAllocator {
        fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
            if self.alloc_index.get() == self.fail_index {
                self.has_induced_failure.set(true);
                return Err(AllocError);
            }
            self.alloc_index.set(self.alloc_index.get() + 1);
            Global.allocate(layout)
        }

        unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
            // SAFETY: the caller's contract; the block came from `Global`.
            unsafe { Global.deallocate(ptr, layout) }
        }
    }

    #[test]
    fn untouched_pool_create_destroy_reuse() {
        // test "UntouchedPool: create, destroy, reuse"
        let mut pool =
            TestPool::init_capacity(testing_allocator(), testing_allocator(), 0).unwrap();
        // defer pool.deinit(); (Drop)

        let a = pool.create().unwrap();
        let b = pool.create().unwrap();
        let c = pool.create().unwrap();
        assert!(a != b);
        assert!(a != c);
        assert!(b != c);

        // Freed items are recycled, most recent first.
        // SAFETY: each item is live and came from this pool.
        unsafe {
            pool.destroy(a);
            pool.destroy(b);
        }
        assert_eq!(b, pool.create().unwrap());
        assert_eq!(a, pool.create().unwrap());

        // SAFETY: as above.
        unsafe {
            pool.destroy(a);
            pool.destroy(b);
            pool.destroy(c);
        }
        pool.deinit();
    }

    #[test]
    fn untouched_pool_create_and_destroy_never_touch_items() {
        // test "UntouchedPool: create and destroy never touch items"
        const PREHEAT: usize = 4;

        // Back the item allocator with memory we can inspect.
        let backing_layout = Layout::from_size_align(PREHEAT * ITEM_SIZE, PAGE_SIZE_MIN).unwrap();
        // SAFETY: the layout is non-zero-sized.
        let backing = unsafe { std::alloc::alloc(backing_layout) };
        assert!(!backing.is_null());
        // defer testing.allocator.free(backing); (at the end, after the pool)
        let fba = FixedBufferAllocator::init(backing, PREHEAT * ITEM_SIZE);

        let mut pool = TestPool::init_capacity(testing_allocator(), &fba, PREHEAT).unwrap();
        assert_eq!(PREHEAT * ITEM_SIZE, fba.end_index.get());

        // Lay the sentinel down after preheat: allocation itself may write
        // (the Allocator interface fills fresh memory with undefined in
        // safe builds, which valgrind also tracks). The sentinel must differ
        // from Zig's 0xAA undefined pattern so that any write is visible.
        const SENTINEL: u8 = 0x5A;
        // SAFETY: `backing` is valid for `PREHEAT * ITEM_SIZE` bytes, and the
        // write goes through the same raw pointer every item derives from.
        unsafe { core::ptr::write_bytes(backing, SENTINEL, PREHEAT * ITEM_SIZE) };
        let backing_all_equal = |value: u8| {
            // SAFETY: as above; the bytes were initialized by the write.
            let bytes = unsafe { core::slice::from_raw_parts(backing, PREHEAT * ITEM_SIZE) };
            bytes.iter().all(|&b| b == value)
        };

        // Every preheated item comes out untouched and without allocating.
        let mut items: [NonNull<[u8; PAGE_SIZE_MIN]>; PREHEAT] = [NonNull::dangling(); PREHEAT];
        for item in &mut items {
            *item = pool.create().unwrap();
            assert_eq!(PREHEAT * ITEM_SIZE, fba.end_index.get());
            assert!(item_all_equal(*item, SENTINEL));
        }

        // Destroying and re-creating doesn't touch them either.
        for item in items {
            // SAFETY: each item is live and came from this pool.
            unsafe { pool.destroy(item) };
        }
        assert!(backing_all_equal(SENTINEL));
        for item in &mut items {
            *item = pool.create().unwrap();
            assert!(item_all_equal(*item, SENTINEL));
        }
        for item in items {
            // SAFETY: as above.
            unsafe { pool.destroy(item) };
        }

        pool.deinit();
        drop(pool);
        // SAFETY: allocated above with `backing_layout`; the pool, the only
        // user of the fixed buffer allocator, is gone.
        unsafe { std::alloc::dealloc(backing, backing_layout) };
    }

    #[test]
    fn untouched_pool_destroy_never_allocates_from_the_general_allocator() {
        // test "UntouchedPool: destroy never allocates from the general allocator"

        // A general allocator that fails every allocation: after create has
        // reserved the free-list slot, destroy must not need it.
        let failing = FailingAllocator::init(0);

        let mut pool =
            TestPool::init_capacity(testing_allocator(), testing_allocator(), 0).unwrap();

        // The pool grows the free list through its own gpa, so swap in the
        // failing one only around destroy.
        let mut items: [NonNull<[u8; PAGE_SIZE_MIN]>; 64] = [NonNull::dangling(); 64];
        for item in &mut items {
            *item = pool.create().unwrap();
        }
        // SAFETY: only `destroy` runs before the gpa is restored.
        let gpa = unsafe { pool.set_gpa(&failing) };
        for item in items {
            // SAFETY: each item is live and came from this pool.
            unsafe { pool.destroy(item) };
        }
        // SAFETY: restores the gpa that grew the free list.
        unsafe { pool.set_gpa(gpa) };
        assert_eq!(items.len(), pool.free().items().len());
        // PORT NOTE: an added check (R1 "no-loss, not no-addition"): destroy
        // returns nothing, so upstream would only see a failed growth as a
        // crash; here the failing allocator also proves it was never called.
        assert!(!failing.has_induced_failure.get());

        pool.deinit();
    }

    #[test]
    fn untouched_pool_reset_retains_at_most_the_limit() {
        // test "UntouchedPool: reset retains at most the limit"
        let mut pool =
            TestPool::init_capacity(testing_allocator(), testing_allocator(), 4).unwrap();

        // Free everything above the limit
        assert!(pool.reset(ResetMode::RetainWithLimit(2 * ITEM_SIZE)));
        assert_eq!(2, pool.free().items().len());
        assert_eq!(2, pool.live());

        // retain_capacity keeps everything
        assert!(pool.reset(ResetMode::RetainCapacity));
        assert_eq!(2, pool.free().items().len());

        // Retained items are handed out without allocating; new items
        // beyond them are allocated as needed.
        let a = pool.create().unwrap();
        let b = pool.create().unwrap();
        assert_eq!(2, pool.live());
        let c = pool.create().unwrap();
        assert_eq!(3, pool.live());
        // SAFETY: each item is live and came from this pool.
        unsafe {
            pool.destroy(a);
            pool.destroy(b);
            pool.destroy(c);
        }

        assert!(pool.reset(ResetMode::FreeAll));
        assert_eq!(0, pool.free().items().len());
        assert_eq!(0, pool.live());

        pool.deinit();
    }

    #[test]
    fn untouched_pool_release_frees_immediately() {
        // test "UntouchedPool: release frees immediately"
        let mut pool =
            TestPool::init_capacity(testing_allocator(), testing_allocator(), 1).unwrap();

        let a = pool.create().unwrap();
        let b = pool.create().unwrap();
        assert_eq!(2, pool.live());
        // SAFETY: `a` is live and came from this pool.
        unsafe { pool.release(a) };
        assert_eq!(1, pool.live());
        assert_eq!(0, pool.free().items().len());
        // SAFETY: `b` is live and came from this pool.
        unsafe { pool.destroy(b) };
        assert_eq!(1, pool.free().items().len());

        pool.deinit();
    }

    /// PORT NOTE: port-only. The runtime-size shape PageList's page pool uses
    /// (`std_size()` is runtime in Rustty): items have the runtime size and
    /// alignment, are writable end to end, and recycle like the typed pool's.
    #[test]
    fn dynamic_untouched_pool_runtime_item_size() {
        let item_size = 3 * PAGE_SIZE_MIN;
        let mut pool: DynamicUntouchedPool<&dyn Allocator> = DynamicUntouchedPool::init_capacity(
            testing_allocator(),
            testing_allocator(),
            item_size,
            PAGE_SIZE_MIN,
            1,
        )
        .unwrap();
        assert_eq!(item_size, pool.item_size());
        assert_eq!(PAGE_SIZE_MIN, pool.item_alignment());
        assert_eq!((1, 1), (pool.live(), pool.free().len()));

        let a = pool.create().unwrap();
        let b = pool.create().unwrap();
        assert_eq!((2, 0), (pool.live(), pool.free().len()));
        for item in [a, b] {
            assert_eq!(0, item.as_ptr() as usize % PAGE_SIZE_MIN);
            // SAFETY: a live item of `item_size` bytes from this pool.
            unsafe { core::ptr::write_bytes(item.as_ptr(), 0x5A, item_size) };
        }

        // SAFETY: `b` is live and came from this pool.
        unsafe { pool.destroy(b) };
        assert_eq!(b, pool.create().unwrap());

        // SAFETY: both are live and came from this pool.
        unsafe {
            pool.release(a);
            pool.destroy(b);
        }
        assert_eq!((1, 1), (pool.live(), pool.free().len()));
        assert!(pool.reset(ResetMode::FreeAll));
        assert_eq!((0, 0), (pool.live(), pool.free().len()));
    }
}
