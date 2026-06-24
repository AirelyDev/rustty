//! Port of `std.heap.memory_pool` (Zig 0.15.2) — ADDITIVE INFRA GLUE.
//!
//! PORT STATUS: infra (faithful-semantics port of Zig stdlib; not in the 3234
//! contract)
//!
//! A memory pool that can allocate objects of a single type very quickly. Use
//! this when you need to allocate a lot of objects of the same type, because it
//! outperforms general purpose allocators.
//!
//! This is a faithful port of `std.heap.MemoryPool` / `MemoryPoolAligned` /
//! `MemoryPoolExtra`. The observable semantics the port relies on:
//!
//!   * `create()` reuses the most-recently-`destroy()`ed item (LIFO free list);
//!     otherwise it allocates a fresh slot from the backing arena (when
//!     `growable`).
//!   * `destroy(ptr)` pushes the slot onto the free list.
//!   * `deinit()` frees the arena (all slots at once).
//!   * `reset(mode)` resets the arena and clears the free list.
//!   * `item_size = max(size_of(Node), size_of(Item))` and
//!     `item_alignment = max(align_of(*Node), alignment)` — so each slot can hold
//!     either an `Item` or a free-list `Node`, exactly like Zig.
//!   * `MemoryPoolAligned` over-aligns items (e.g. page-aligned pages in
//!     `PageList`).
//!
//! Faithful to Zig, the pool OWNS an [`ArenaAllocator`] and `init(child)` always
//! takes that arena's child allocator (Zig: `MemoryPool(T).init(allocator)`
//! forwards to `ArenaAllocator.init(allocator)`). `init_preheated(child, n)`
//! mirrors `initPreheated(allocator, n)`. The child is the `allocator-api2`
//! `Allocator` trait — the stable polyfill of `core::alloc::Allocator` and the
//! faithful map of Zig's `std.mem.Allocator`. Every existing caller passes
//! [`allocator_api2::alloc::Global`], which is exactly the prior `std::alloc`
//! path, so observable behavior is unchanged.
//!
//! NOTE on `undefined`: Zig writes `ptr.* = undefined` on create/destroy to poison
//! contents. In Rust we hand back raw, uninitialized-typed storage; the slot
//! memory is real and writable but its `T` value is logically uninitialized, so
//! callers must initialize before reading (same contract as Zig's `undefined`).

use core::marker::PhantomData;
use core::mem::{align_of, size_of};
use core::ptr::NonNull;

use allocator_api2::alloc::{Allocator, Global};

use crate::arena_allocator::{ArenaAllocator, ResetMode};

/// The free-list node. Zig: `struct { next: ?*align(item_alignment) Node }`.
/// A single pointer; its alignment requirement is `align_of::<*Node>()`, which
/// is folded into `item_alignment`.
#[repr(C)]
struct Node {
    next: Option<NonNull<Node>>,
}

/// Faithful port of `MemoryPoolExtra(Item, .{ .alignment, .growable })`.
///
/// `ALIGN` is the requested item alignment (Zig's `pool_options.alignment`);
/// pass `0` for "natural alignment" (`align_of::<T>()`), matching Zig's
/// `?Alignment = null`. The effective slot alignment is
/// `max(align_of::<*Node>(), max(ALIGN_or_natural, align_of::<T>()))`.
///
/// `GROWABLE` mirrors `Options.growable`: when `false`, `create()` past the
/// preheated capacity returns `Err(OutOfMemory)`.
///
/// `A` is the child allocator type of the owned [`ArenaAllocator`]
/// (`allocator-api2`'s `Allocator` trait, the faithful map of Zig's
/// `std.mem.Allocator`). It defaults to [`Global`] — the prior `std::alloc`
/// path — so existing call sites that do not care about the child stay terse
/// and behavior-identical.
pub struct MemoryPoolExtra<T, const ALIGN: usize, const GROWABLE: bool, A: Allocator = Global> {
    arena: ArenaAllocator<A>,
    free_list: Option<NonNull<Node>>,
    _marker: PhantomData<T>,
}

/// Error returned by pool operations. Faithful port of `MemoryPoolError`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutOfMemory;

impl<T, const ALIGN: usize, const GROWABLE: bool, A: Allocator>
    MemoryPoolExtra<T, ALIGN, GROWABLE, A>
{
    /// Size of the memory pool items. This is not necessarily the same as
    /// `size_of::<T>()` as the pool also uses the items for internal means
    /// (the free-list `Node`). Faithful: `max(size_of(Node), size_of(Item))`.
    pub const ITEM_SIZE: usize = max_usize(size_of::<Node>(), size_of::<T>());

    /// Alignment of the memory pool items. Not necessarily `align_of::<T>()` as
    /// the pool also uses the items for internal means. Faithful:
    /// `node_alignment.max(alignment orelse align_of(Item))` where
    /// `node_alignment = align_of(*Node)`.
    pub const ITEM_ALIGNMENT: usize = max_usize(
        align_of::<Option<NonNull<Node>>>(),
        max_usize(
            if ALIGN == 0 { align_of::<T>() } else { ALIGN },
            align_of::<T>(),
        ),
    );

    /// Faithful port of `MemoryPool(T).init(allocator)`: creates a new memory
    /// pool whose owned arena allocates its backing nodes through `child`.
    pub fn init(child: A) -> Self {
        MemoryPoolExtra {
            arena: ArenaAllocator::with_child(child),
            free_list: None,
            _marker: PhantomData,
        }
    }

    /// Faithful port of `MemoryPool(T).initPreheated(allocator, initial_size)`:
    /// creates a new memory pool over `child` and pre-allocates `initial_size`
    /// items. This allows up to `initial_size` active allocations before an
    /// `OutOfMemory` error happens when calling `create()`.
    pub fn init_preheated(child: A, initial_size: usize) -> Result<Self, OutOfMemory> {
        let mut pool = Self::init(child);
        // errdefer pool.deinit(): on failure, dropping `pool` frees the arena.
        pool.preheat(initial_size)?;
        Ok(pool)
    }

    /// Preheats the memory pool by pre-allocating `size` items. This allows up
    /// to `size` active allocations before an `OutOfMemory` error might happen
    /// when calling `create()`. The preheated nodes form a LIFO free list.
    pub fn preheat(&mut self, size: usize) -> Result<(), OutOfMemory> {
        let mut i: usize = 0;
        while i < size {
            let raw_mem = self.alloc_new()?;
            let free_node = raw_mem.cast::<Node>();
            // free_node.* = Node{ .next = pool.free_list };
            // SAFETY: `free_node` points to a fresh, suitably-aligned slot of at
            // least ITEM_SIZE >= size_of::<Node>() bytes.
            unsafe {
                free_node.as_ptr().write(Node {
                    next: self.free_list,
                });
            }
            self.free_list = Some(free_node);
            i += 1;
        }
        Ok(())
    }

    /// Resets the memory pool and destroys all allocated items. This can be used
    /// to batch-destroy all objects without invalidating the memory pool.
    ///
    /// Returns whether the reset was successful (mirrors arena `reset`); if the
    /// reallocation failed, `false` is returned but the pool stays functional.
    /// If `mode` is `FreeAll`, this always returns `true`.
    pub fn reset(&mut self, mode: ResetMode) -> bool {
        // TODO (from Zig): Potentially store all allocated objects in a list as
        // well, allowing to just move them into the free list instead of
        // actually releasing the memory.
        let reset_successful = self.arena.reset(mode);
        self.free_list = None;
        reset_successful
    }

    /// Creates a new item and adds it to the memory pool.
    ///
    /// Reuses the most-recently-destroyed item (LIFO) if the free list is
    /// non-empty; otherwise allocates a fresh slot from the arena (if growable).
    /// The returned slot is logically uninitialized (`undefined` in Zig): the
    /// caller must initialize the `T` before reading it.
    pub fn create(&mut self) -> Result<NonNull<T>, OutOfMemory> {
        let node = if let Some(item) = self.free_list {
            // pool.free_list = item.next;
            // SAFETY: `item` is a live free-list node we own.
            self.free_list = unsafe { item.as_ref().next };
            item
        } else if GROWABLE {
            self.alloc_new()?.cast::<Node>()
        } else {
            return Err(OutOfMemory);
        };

        let ptr = node.cast::<T>();
        // ptr.* = undefined; — we leave the storage logically uninitialized.
        Ok(ptr)
    }

    /// Destroys a previously created item. Only pass items to `ptr` that were
    /// previously created with `create()` of the same memory pool!
    ///
    /// Pushes the slot onto the LIFO free list so the next `create()` reuses it.
    ///
    /// # Safety
    /// `ptr` must have been returned by `create()` on this pool and not already
    /// destroyed. After this call the `T` it pointed to is logically dead; the
    /// slot is reclaimed for reuse.
    pub unsafe fn destroy(&mut self, ptr: NonNull<T>) {
        // ptr.* = undefined; — caller's T value is now logically dead. (If T has
        // a Drop, the caller is responsible for running it before destroy, just
        // as a Zig caller would deinit the value; the pool only manages storage.)
        let node = ptr.cast::<Node>();
        // node.* = Node{ .next = pool.free_list };
        // SAFETY: the slot is at least ITEM_SIZE >= size_of::<Node>() bytes and
        // suitably aligned for Node (ITEM_ALIGNMENT >= align_of::<*Node>()).
        unsafe {
            node.as_ptr().write(Node {
                next: self.free_list,
            });
        }
        self.free_list = Some(node);
    }

    /// Faithful port of `allocNew`: allocate one `item_size`-byte,
    /// `item_alignment`-aligned slot from the backing arena.
    fn alloc_new(&mut self) -> Result<NonNull<u8>, OutOfMemory> {
        self.arena
            .alloc_bytes(Self::ITEM_SIZE, Self::ITEM_ALIGNMENT)
            .ok_or(OutOfMemory)
    }
}

impl<T, const ALIGN: usize, const GROWABLE: bool, A: Allocator> Drop
    for MemoryPoolExtra<T, ALIGN, GROWABLE, A>
{
    fn drop(&mut self) {
        // Faithful port of `deinit`: free the arena (all slots at once). The
        // ArenaAllocator's own Drop releases every backing node.
    }
}

/// `MemoryPool(Item)` — natural alignment, growable. Faithful alias for
/// `MemoryPoolExtra(Item, .{})`.
pub type MemoryPool<T, A = Global> = MemoryPoolExtra<T, 0, true, A>;

/// `MemoryPoolAligned(Item, alignment)` — over-aligned items (e.g. page-aligned),
/// growable. Faithful alias; pass the desired byte alignment as `ALIGN`.
pub type MemoryPoolAligned<T, const ALIGN: usize, A = Global> = MemoryPoolExtra<T, ALIGN, true, A>;

/// `const fn max` for computing `ITEM_SIZE`/`ITEM_ALIGNMENT` in const context.
const fn max_usize(a: usize, b: usize) -> usize {
    if a > b {
        a
    } else {
        b
    }
}

/// A faithful port of `std.heap.MemoryPoolAligned` whose item SIZE is supplied
/// at construction instead of via a `[N]u8` type parameter.
///
/// PORT NOTE: in `PageList.zig` the page pool is
/// `MemoryPoolAligned([std_size]u8, .fromByteUnits(page_size_min))`. In the Rust
/// port `std_size` is a RUNTIME value (`Page.layout(std_capacity).total_size`,
/// which depends on `cfg!(test)` for grapheme bytes), so it cannot be a
/// const-generic array length. This type is the closest faithful mapping: it has
/// the SAME implementation as [`MemoryPoolExtra`] (an owned [`ArenaAllocator`]
/// over the child allocator plus a LIFO free-list), but `item_size` /
/// `item_alignment` are dynamic. The pool dispenses raw, page-aligned,
/// `item_size`-byte slots whose contents are whatever the backing allocator
/// hands back (for the page allocator: lazily OS-zeroed mmap memory on fresh
/// slots; reused free-list slots keep their previous contents until the caller
/// zeroes them — exactly the reference semantics).
///
/// It additionally exposes [`child_allocator`](Self::child_allocator) and
/// [`zero_retained_arena`](Self::zero_retained_arena) so `PageList` can reproduce
/// the reference's `pool.pages.arena.child_allocator` (non-standard pages) and
/// the post-reset arena-zeroing block.
pub struct DynamicMemoryPoolAligned<A: Allocator = Global> {
    arena: ArenaAllocator<A>,
    free_list: Option<NonNull<Node>>,
    /// `item_size = max(size_of(Node), requested_item_size)`.
    item_size: usize,
    /// `item_alignment = max(align_of(*Node), requested_alignment)`.
    item_alignment: usize,
}

impl<A: Allocator> DynamicMemoryPoolAligned<A> {
    /// Faithful port of `MemoryPoolAligned(.., alignment).init(child)` with a
    /// runtime item size. `requested_item_size` is the byte size of one item
    /// (`@sizeOf([std_size]u8) == std_size`); `requested_alignment` is the
    /// requested item alignment (`page_size_min`).
    pub fn init(child: A, requested_item_size: usize, requested_alignment: usize) -> Self {
        DynamicMemoryPoolAligned {
            arena: ArenaAllocator::with_child(child),
            free_list: None,
            item_size: max_usize(size_of::<Node>(), requested_item_size),
            item_alignment: max_usize(align_of::<Option<NonNull<Node>>>(), requested_alignment),
        }
    }

    /// Faithful port of `initPreheated(child, n)`: preheat `initial_size` items
    /// onto the LIFO free-list.
    pub fn init_preheated(
        child: A,
        requested_item_size: usize,
        requested_alignment: usize,
        initial_size: usize,
    ) -> Result<Self, OutOfMemory> {
        let mut pool = Self::init(child, requested_item_size, requested_alignment);
        // errdefer pool.deinit(): on failure, dropping `pool` frees the arena.
        pool.preheat(initial_size)?;
        Ok(pool)
    }

    /// `PagePool.item_size`: the byte size of one item slot.
    #[inline]
    pub fn item_size(&self) -> usize {
        self.item_size
    }

    /// Preheat the pool by pre-allocating `size` items onto the LIFO free-list.
    pub fn preheat(&mut self, size: usize) -> Result<(), OutOfMemory> {
        let mut i: usize = 0;
        while i < size {
            let raw_mem = self.alloc_new()?;
            let free_node = raw_mem.cast::<Node>();
            // SAFETY: `free_node` points to a fresh, suitably-aligned slot of at
            // least item_size >= size_of::<Node>() bytes.
            unsafe {
                free_node.as_ptr().write(Node {
                    next: self.free_list,
                });
            }
            self.free_list = Some(free_node);
            i += 1;
        }
        Ok(())
    }

    /// Faithful port of `reset(mode)`: reset the arena and clear the free-list.
    pub fn reset(&mut self, mode: ResetMode) -> bool {
        let reset_successful = self.arena.reset(mode);
        self.free_list = None;
        reset_successful
    }

    /// Faithful port of `create()`: hand back an `item_size`-byte, page-aligned
    /// slot, reusing the most-recently-`destroy`ed one (LIFO) if available,
    /// otherwise bumping a fresh slot from the arena. The returned slot's
    /// contents are NOT zeroed by the pool: a fresh slot is whatever the backing
    /// allocator returned (lazily OS-zeroed mmap for the page allocator), and a
    /// reused slot retains its previous contents.
    pub fn create(&mut self) -> Result<NonNull<u8>, OutOfMemory> {
        if let Some(item) = self.free_list {
            // SAFETY: `item` is a live free-list node we own.
            self.free_list = unsafe { item.as_ref().next };
            return Ok(item.cast::<u8>());
        }
        self.alloc_new()
    }

    /// Faithful port of `destroy(ptr)`: push the slot onto the LIFO free-list so
    /// the next `create()` reuses it.
    ///
    /// # Safety
    /// `ptr` must have been returned by `create()`/`preheat` on this pool and not
    /// already destroyed.
    pub unsafe fn destroy(&mut self, ptr: NonNull<u8>) {
        let node = ptr.cast::<Node>();
        // SAFETY: the slot is at least item_size >= size_of::<Node>() bytes and
        // suitably aligned for Node (item_alignment >= align_of::<*Node>()).
        unsafe {
            node.as_ptr().write(Node {
                next: self.free_list,
            });
        }
        self.free_list = Some(node);
    }

    /// Borrow of the child ("backing") allocator — the reference's
    /// `pool.pages.arena.child_allocator`, used for NON-STANDARD page buffers
    /// that bypass the pool.
    #[inline]
    pub fn child_allocator(&self) -> &A {
        self.arena.child_allocator()
    }

    /// Zero the usable region of every retained arena backing node. Faithful map
    /// of `PageList.reset`'s post-reset arena-zeroing block (retained arena
    /// memory is not re-`mmap`ed so it is not implicitly OS-zeroed).
    pub fn zero_retained_arena(&mut self) {
        self.arena.zero_retained();
    }

    /// Allocate one `item_size`-byte, `item_alignment`-aligned slot from the
    /// backing arena.
    fn alloc_new(&mut self) -> Result<NonNull<u8>, OutOfMemory> {
        self.arena
            .alloc_bytes(self.item_size, self.item_alignment)
            .ok_or(OutOfMemory)
    }
}

impl<A: Allocator> Drop for DynamicMemoryPoolAligned<A> {
    fn drop(&mut self) {
        // Faithful port of `deinit`: the owned ArenaAllocator's Drop frees every
        // backing node (all slots at once).
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Port of the Zig test "basic": create three distinct items, destroy the
    /// middle, and assert the next create reuses that exact slot (LIFO reuse).
    #[test]
    fn basic_create_destroy_reuse() {
        let mut pool: MemoryPool<u32> = MemoryPool::init(Global);

        let p1 = pool.create().unwrap();
        let p2 = pool.create().unwrap();
        let p3 = pool.create().unwrap();

        // Assert uniqueness.
        assert_ne!(p1.as_ptr(), p2.as_ptr());
        assert_ne!(p1.as_ptr(), p3.as_ptr());
        assert_ne!(p2.as_ptr(), p3.as_ptr());

        // Initialize so Miri sees defined memory, then destroy.
        unsafe {
            p1.as_ptr().write(1);
            p2.as_ptr().write(2);
            p3.as_ptr().write(3);
        }

        unsafe { pool.destroy(p2) };
        let p4 = pool.create().unwrap();

        // Assert memory reuse.
        assert_eq!(p2.as_ptr(), p4.as_ptr());
        unsafe { p4.as_ptr().write(4) };

        // Cleanup remaining live items (arena frees storage on drop).
        unsafe {
            pool.destroy(p1);
            pool.destroy(p3);
            pool.destroy(p4);
        }
    }

    /// LIFO ordering across multiple destroys: the most-recently-destroyed item
    /// is the first one create() returns.
    #[test]
    fn lifo_ordering() {
        let mut pool: MemoryPool<u64> = MemoryPool::init(Global);
        let a = pool.create().unwrap();
        let b = pool.create().unwrap();
        let c = pool.create().unwrap();
        unsafe {
            a.as_ptr().write(0);
            b.as_ptr().write(0);
            c.as_ptr().write(0);
        }
        // Destroy in order a, b, c -> free list head is c.
        unsafe {
            pool.destroy(a);
            pool.destroy(b);
            pool.destroy(c);
        }
        // create() returns c, then b, then a (LIFO).
        let r1 = pool.create().unwrap();
        let r2 = pool.create().unwrap();
        let r3 = pool.create().unwrap();
        assert_eq!(r1.as_ptr(), c.as_ptr());
        assert_eq!(r2.as_ptr(), b.as_ptr());
        assert_eq!(r3.as_ptr(), a.as_ptr());
        unsafe {
            pool.destroy(r1);
            pool.destroy(r2);
            pool.destroy(r3);
        }
    }

    /// Port of the Zig test "preheating (success)".
    #[test]
    fn preheating_success() {
        let mut pool: MemoryPool<u32> = MemoryPool::init_preheated(Global, 4).unwrap();
        let a = pool.create().unwrap();
        let b = pool.create().unwrap();
        let c = pool.create().unwrap();
        unsafe {
            a.as_ptr().write(0);
            b.as_ptr().write(0);
            c.as_ptr().write(0);
            pool.destroy(a);
            pool.destroy(b);
            pool.destroy(c);
        }
    }

    /// Port of the Zig test "growable": a non-growable pool preheated with 4
    /// allows exactly 4 creates, then errors OutOfMemory.
    #[test]
    fn growable_false_caps_at_preheat() {
        let mut pool: MemoryPoolExtra<u32, 0, false> =
            MemoryPoolExtra::<u32, 0, false, Global>::init_preheated(Global, 4).unwrap();
        let a = pool.create().unwrap();
        let b = pool.create().unwrap();
        let c = pool.create().unwrap();
        let d = pool.create().unwrap();
        unsafe {
            a.as_ptr().write(0);
            b.as_ptr().write(0);
            c.as_ptr().write(0);
            d.as_ptr().write(0);
        }
        assert_eq!(pool.create().err(), Some(OutOfMemory));
        unsafe {
            pool.destroy(a);
            pool.destroy(b);
            pool.destroy(c);
            pool.destroy(d);
        }
    }

    /// Port of the Zig test "greater than pointer default alignment": a type
    /// with align(16) is handed back correctly aligned.
    #[test]
    fn greater_than_pointer_default_alignment() {
        #[repr(C, align(16))]
        struct Foo {
            data: u64,
        }
        let mut pool: MemoryPool<Foo> = MemoryPool::init(Global);
        let foo = pool.create().unwrap();
        assert_eq!(foo.as_ptr() as usize % 16, 0);
        unsafe { foo.as_ptr().write(Foo { data: 0 }) };
        unsafe { pool.destroy(foo) };
    }

    /// Port of the Zig test "greater than pointer manual alignment":
    /// MemoryPoolAligned(Foo, 16) over a naturally-8-aligned type.
    #[test]
    fn greater_than_pointer_manual_alignment() {
        #[repr(C)]
        struct Foo {
            data: u64,
        }
        let mut pool: MemoryPoolAligned<Foo, 16> = MemoryPoolAligned::<Foo, 16>::init(Global);
        let foo = pool.create().unwrap();
        assert_eq!(foo.as_ptr() as usize % 16, 0);
        unsafe { foo.as_ptr().write(Foo { data: 0 }) };
        unsafe { pool.destroy(foo) };
    }

    /// Page-aligned items as `PageList` uses them: large item, big alignment.
    #[test]
    fn page_aligned_items() {
        const PAGE: usize = 4096;
        // Stand-in for `[std_size]u8` with page alignment.
        let mut pool: MemoryPoolAligned<[u8; 256], PAGE> =
            MemoryPoolAligned::<[u8; 256], PAGE>::init_preheated(Global, 2).unwrap();
        let a = pool.create().unwrap();
        let b = pool.create().unwrap();
        assert_eq!(a.as_ptr() as usize % PAGE, 0);
        assert_eq!(b.as_ptr() as usize % PAGE, 0);
        assert_ne!(a.as_ptr(), b.as_ptr());
        unsafe {
            a.as_ptr().write([0u8; 256]);
            b.as_ptr().write([0u8; 256]);
            pool.destroy(a);
            pool.destroy(b);
        }
    }

    /// Growth: many creates beyond preheat succeed and stay distinct.
    #[test]
    fn growth_beyond_preheat() {
        let mut pool: MemoryPool<u32> = MemoryPool::init_preheated(Global, 2).unwrap();
        let mut ptrs = Vec::new();
        for i in 0..100u32 {
            let p = pool.create().unwrap();
            unsafe { p.as_ptr().write(i) };
            ptrs.push(p);
        }
        for i in 0..ptrs.len() {
            for j in (i + 1)..ptrs.len() {
                assert_ne!(ptrs[i].as_ptr(), ptrs[j].as_ptr());
            }
        }
        for p in ptrs {
            unsafe { pool.destroy(p) };
        }
    }

    /// reset() clears the free list and resets the arena; the pool keeps working.
    #[test]
    fn reset_clears_and_reuses() {
        let mut pool: MemoryPool<u64> = MemoryPool::init(Global);
        let a = pool.create().unwrap();
        let b = pool.create().unwrap();
        unsafe {
            a.as_ptr().write(1);
            b.as_ptr().write(2);
        }
        assert!(pool.reset(ResetMode::RetainCapacity));
        // After reset the free list is empty and the arena was retained; new
        // creates draw from the retained arena capacity.
        let c = pool.create().unwrap();
        unsafe { c.as_ptr().write(3) };
        unsafe { pool.destroy(c) };
    }

    /// `DynamicMemoryPoolAligned`: runtime item size + page alignment, LIFO
    /// reuse, and that fresh arena slots come back zeroed (Global zeroes on
    /// first-touch via its backing; we only assert reuse-after-zero semantics).
    #[test]
    fn dynamic_aligned_runtime_size_align_and_reuse() {
        const PAGE: usize = 4096;
        let item_size = 1024usize; // stand-in for a runtime `std_size`.
        let mut pool: DynamicMemoryPoolAligned<Global> =
            DynamicMemoryPoolAligned::init_preheated(Global, item_size, PAGE, 2).unwrap();
        assert_eq!(pool.item_size(), max_usize(size_of::<Node>(), item_size));

        let a = pool.create().unwrap();
        let b = pool.create().unwrap();
        assert_eq!(a.as_ptr() as usize % PAGE, 0);
        assert_eq!(b.as_ptr() as usize % PAGE, 0);
        assert_ne!(a.as_ptr(), b.as_ptr());

        // Scribble, destroy, and assert LIFO reuse hands the exact slot back.
        unsafe {
            core::ptr::write_bytes(a.as_ptr(), 0xAB, item_size);
            core::ptr::write_bytes(b.as_ptr(), 0xCD, item_size);
            pool.destroy(b);
        }
        let c = pool.create().unwrap();
        assert_eq!(c.as_ptr(), b.as_ptr());
        unsafe {
            pool.destroy(a);
            pool.destroy(c);
        }
    }

    /// `zero_retained_arena` zeroes the retained arena memory after a reset, so a
    /// slot reused from the retained free list reads zero even though it was not
    /// freshly allocated (the `PageList.reset` contract). Mirrors the realistic
    /// PageList geometry: item_size is a multiple of the alignment and the retain
    /// limit covers several items (so the page-aligned bump never spills to a
    /// fresh, un-zeroed node).
    #[test]
    fn dynamic_aligned_zero_retained_arena() {
        const ALIGN: usize = 64;
        let item_size = 256usize; // multiple of ALIGN, like std_size vs page_size_min.
        let mut pool: DynamicMemoryPoolAligned<Global> =
            DynamicMemoryPoolAligned::init_preheated(Global, item_size, ALIGN, 4).unwrap();
        // Hand out and scribble several slots.
        let mut handed = Vec::new();
        for _ in 0..4 {
            let p = pool.create().unwrap();
            unsafe { core::ptr::write_bytes(p.as_ptr(), 0xFF, item_size) };
            handed.push(p);
        }
        // Retain plenty of capacity, then zero the retained arena.
        pool.reset(ResetMode::RetainWithLimit(item_size * 8));
        pool.zero_retained_arena();
        // Every slot drawn from the retained (now-zeroed) arena reads zero.
        for _ in 0..4 {
            let q = pool.create().unwrap();
            let bytes = unsafe { core::slice::from_raw_parts(q.as_ptr(), item_size) };
            assert!(
                bytes.iter().all(|&b| b == 0),
                "retained slot must be zeroed"
            );
            unsafe { pool.destroy(q) };
        }
    }

    /// `child_allocator` borrows the backing allocator (faithful map of
    /// `pool.pages.arena.child_allocator`).
    #[test]
    fn dynamic_aligned_child_allocator_accessible() {
        let pool: DynamicMemoryPoolAligned<Global> =
            DynamicMemoryPoolAligned::init(Global, 256, 64);
        // Just ensure the accessor is callable and yields the child type.
        let _child: &Global = pool.child_allocator();
    }

    /// ITEM_SIZE/ITEM_ALIGNMENT match the Zig formulas for a small type.
    #[test]
    fn item_size_and_alignment_formulas() {
        // u8: size_of(Node)=size_of(*Node) (pointer) > 1, so ITEM_SIZE = ptr size.
        assert_eq!(
            MemoryPool::<u8>::ITEM_SIZE,
            core::mem::size_of::<Option<NonNull<Node>>>()
        );
        // Natural alignment of u8 is 1, but node alignment (pointer) dominates.
        assert_eq!(
            MemoryPool::<u8>::ITEM_ALIGNMENT,
            core::mem::align_of::<Option<NonNull<Node>>>()
        );
        // Over-aligned: ITEM_ALIGNMENT = max(ptr_align, 64).
        assert_eq!(MemoryPoolAligned::<u8, 64>::ITEM_ALIGNMENT, 64);
    }
}
