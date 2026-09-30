//! PORT NOTE: Ports ghostty `src/datastruct/wasm_page_pool.zig`.
//!
//! A memory pool of wasm-page-multiple-sized items backed directly by
//! wasm linear memory, for wasm targets only.
//!
//! This is necessary for two reasons: the std.heap.MemoryPool grows 1.5x
//! at each growth point. The backing allocator for that is usually a GPA
//! which is the BrkAllocator for wasm. This grows by power-of-two
//! big-allocation slots. If you pair these together you get a massive
//! permanent linear memory growth. Native (non-wasm) targets don't care
//! because unused virtual mappings are effectively free, but this isn't
//! exactly true for Wasm runtimes.
//!
//! This pool instead grows exactly @sizeOf(Item) bytes of fresh linear
//! memory per item with memory.grow (which also guarantees wasm-page
//! alignment and zeroing) and recycles freed items through a free list
//! that is container-level, i.e. shared by every pool of the same Item
//! type in the module instance. Every freed item is immediately
//! reusable by every pool, and each item costs exactly @sizeOf(Item)
//! reserved bytes, ever.
//!
//! The "shared by every pool of the same Item type" is really important:
//! the normal Rustty memory pool is per-terminal. This one is per-module.
//! The Rustty wasm modules are not multi-threaded so this doesn't require
//! any synchronization. But this means that all terminals share one pool
//! so memory doesn't balloon like crazy that way either.
//!
//! Requirements on Item:
//!
//!   - @sizeOf(Item) must be a nonzero multiple of the wasm page size
//!     (64KiB), since memory.grow allocates in whole pages. This is
//!     also what makes the pool exact-fit: any other size would strand
//!     the remainder of the last page.
//!   - Natural alignment must be at most the wasm page size.
//!
//! Free-list items are dirty except that their first pointer-size bytes
//! hold the intrusive free-list node, exactly like the std MemoryPool.
//! Callers that need zeroed items must zero them (fresh items from
//! memory.grow are zero by the wasm spec; recycled items retain
//! whatever the caller left, so zero either on destroy or on create).
//!
//! The API mirrors the native page pool (datastruct.UntouchedPool:
//! initCapacity/deinit/reset/create/destroy plus the allocator field),
//! so PageList can swap them at comptime.
//
// PORT NOTE (structure): the Zig `WasmPagePool(comptime Item: type)` returns a
// struct type whose `var free_list` static is per-monomorphization, i.e. one
// free list per Item type. Rust has no per-type statics inside a generic impl
// (a `static` inside a generic fn is shared across ALL instantiations), so the
// per-Item-type free list is carried by the `WasmPageItem` trait: each Item type
// implements `free_list()` to return its own `&'static FreeList`. This preserves
// the "shared by every pool of the same Item type" property exactly. The Zig
// `allocator: Allocator` field (carried only for interface compatibility — pool
// items never come from it) is modeled by the generic `A` type parameter.
//
// PORT NOTE (LIFETIMES): `create` hands out a raw `ItemPtr` (`*mut Item`) whose
// backing linear memory is OWNED by the module instance, not the pool: freed
// items live on the container-level `free_list` and are reused by other pools of
// the same Item type. So an `ItemPtr` OUTLIVES the `WasmPagePool` it came from
// (deinit/reset intentionally retain items). The consumer wiring — feeding these
// raw-ptr pages into rustty-vt's `PageList` — is `DynamicWasmPagePool` below
// (the pool does not own the pages; callers must `destroy` every live item
// before dropping the pool or they leak, mirroring the Zig deinit doc-comment).

#[cfg(target_arch = "wasm32")]
use core::arch::wasm32;
use core::cell::Cell;
use core::marker::PhantomData;
use core::ptr::NonNull;

/// PORT NOTE: `std.heap.page_size_min` for wasm — the wasm page size (64KiB).
/// `memory.grow` allocates in whole pages of this size.
pub const PAGE_SIZE_MIN: usize = 64 * 1024;

/// PORT NOTE: `error.OutOfMemory` (`Allocator.Error`) — returned by
/// `init_capacity`/`create` on failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutOfMemory;

/// PORT NOTE: `std.heap.ArenaAllocator.ResetMode` — the argument to `reset`.
/// Unused by this pool (`reset` is a no-op returning `true`), but carried for
/// interface compatibility with the std managed memory pools.
#[derive(Debug, Clone, Copy)]
pub enum ResetMode {
    FreeAll,
    RetainCapacity,
    RetainWithLimit(usize),
}

/// PORT NOTE: the intrusive free-list node — `std.SinglyLinkedList.Node`. Stored
/// in the first pointer-size bytes of a freed item, exactly like the std
/// MemoryPool.
#[repr(C)]
pub struct Node {
    pub next: Option<NonNull<Node>>,
}

/// PORT NOTE: the head of a container-level free list — `std.SinglyLinkedList`.
/// Wasm is single-threaded (asserted below), so no synchronization is needed and
/// the head lives in a `static` per Item type (see `WasmPageItem::free_list`).
pub struct FreeList {
    first: Cell<Option<NonNull<Node>>>,
}

// SAFETY(std.SinglyLinkedList/FreeList): the free list is only ever touched on
// wasm targets, which are single-threaded (the pool asserts
// `builtin.single_threaded`); it is never shared across threads, so exposing it
// through a `static` is sound.
unsafe impl Sync for FreeList {}

impl FreeList {
    pub const fn new() -> FreeList {
        FreeList {
            first: Cell::new(None),
        }
    }

    /// `std.SinglyLinkedList.popFirst`.
    ///
    /// PORT NOTE: only reached from `create` (the wasm growth/reuse path), so it
    /// is wasm-gated to stay dead-code-clean on native targets.
    #[cfg(target_arch = "wasm32")]
    fn pop_first(&self) -> Option<NonNull<Node>> {
        let node = self.first.get()?;
        // SAFETY(std.SinglyLinkedList.popFirst/FreeList): `node` was placed on
        // this list by `prepend`, whose `next` field we read to advance the head.
        let next = unsafe { node.as_ref().next };
        self.first.set(next);
        Some(node)
    }

    /// `std.SinglyLinkedList.prepend`.
    fn prepend(&self, mut node: NonNull<Node>) {
        // SAFETY(std.SinglyLinkedList.prepend/FreeList): `node` points at freshly
        // reinterpreted item storage (see `destroy`); we write only its `next`
        // field before publishing it as the new head.
        unsafe { node.as_mut().next = self.first.get() };
        self.first.set(Some(node));
    }
}

impl Default for FreeList {
    fn default() -> FreeList {
        FreeList::new()
    }
}

/// PORT NOTE: carries the per-Item-type shared free list (the Rust analog of the
/// Zig per-monomorphization `var free_list`). Each Item type implements this to
/// return its own `&'static FreeList`, so all pools of the same Item type share
/// one list, exactly like the Zig container-level static.
pub trait WasmPageItem: Sized {
    fn free_list() -> &'static FreeList;
}

/// A memory pool of wasm-page-multiple-sized items backed directly by wasm
/// linear memory. See the module doc comment.
pub struct WasmPagePool<Item: WasmPageItem, A> {
    /// The allocator this managed pool was initialized with,
    /// carried for interface compatibility with the std managed
    /// memory pools. Pool items never come from it (they come
    /// from memory.grow), but callers may use it for related
    /// allocations that don't fit the pool.
    pub allocator: A,

    _item: PhantomData<fn() -> Item>,
}

impl<Item: WasmPageItem, A> WasmPagePool<Item, A> {
    pub const ITEM_SIZE: usize = core::mem::size_of::<Item>();

    pub fn init_capacity<G>(
        gpa: G,
        allocator: A,
        preheat: usize,
    ) -> Result<WasmPagePool<Item, A>, OutOfMemory> {
        // The free list is intrusive so we never allocate from the
        // general purpose allocator.
        let _ = gpa;

        // Preheating is intentionally a no-op: item creation is a
        // cheap memory.grow or free-list pop. And we want to avoid
        // the preallocation mem cost.
        let _ = preheat;
        Ok(WasmPagePool {
            allocator,
            _item: PhantomData,
        })
    }

    pub fn deinit(&mut self) {
        // Items are global to the instance and outlive the pool so
        // other pools can reuse them. Callers must destroy() any
        // items they still hold before deinit or they leak.
        //
        // PORT NOTE: the Zig `self.* = undefined` poisons the pool storage; in
        // Rust there is no `undefined` and dropping the `allocator`/`PhantomData`
        // is a no-op, so this is intentionally empty (the free list, being
        // container-level, is deliberately left intact).
    }

    pub fn reset(&mut self, mode: ResetMode) -> bool {
        // There is nothing to trim: linear memory cannot shrink,
        // so retaining every freed item for reuse is always the
        // right policy on wasm. As with deinit, live items must be
        // destroyed by the caller first.
        let _ = mode;
        true
    }

    /// PORT NOTE: the Zig comptime block (`comptime { if isWasm() { ... } }`)
    /// evaluated per-monomorphization on wasm. Expressed as a per-monomorph
    /// `const` block so the same invariants are checked at compile time when the
    /// pool is instantiated on wasm. (`builtin.single_threaded` is a target
    /// property, not const-checkable here, so it is documented rather than
    /// asserted — wasm rustty modules are single-threaded.)
    #[cfg(target_arch = "wasm32")]
    const COMPTIME_CHECKS: () = {
        // The shared free list (and wasm's single linear
        // memory) requires a single-threaded target.
        // (Documented, not asserted; see above.)
        // Items are whole wasm pages
        assert!(Self::ITEM_SIZE > 0);
        assert!(Self::ITEM_SIZE % PAGE_SIZE_MIN == 0);
        assert!(core::mem::align_of::<Item>() <= PAGE_SIZE_MIN);
        // Free items store the intrusive node in their bytes.
        assert!(Self::ITEM_SIZE >= core::mem::size_of::<Node>());
    };

    #[cfg(target_arch = "wasm32")]
    pub fn create(&self) -> Result<NonNull<Item>, OutOfMemory> {
        // Force the per-monomorph comptime invariant checks.
        let () = Self::COMPTIME_CHECKS;

        // If we have a free item, use it.
        if let Some(node) = Item::free_list().pop_first() {
            return Ok(node.cast::<Item>());
        }

        // Grow linear memory by exactly one item.
        let pages = Self::ITEM_SIZE / PAGE_SIZE_MIN;
        // `memory.grow` on the sole linear memory (index 0) by `pages` whole wasm
        // pages; it returns the previous size in pages, or `usize::MAX` on failure
        // (the analog of Zig's `-1`). The returned region is page-aligned and
        // zeroed by the wasm spec. `core::arch::wasm32::memory_grow` is a safe
        // intrinsic (no `unsafe` needed).
        let page_index = wasm32::memory_grow(0, pages);
        if page_index == usize::MAX {
            return Err(OutOfMemory);
        }
        let addr = page_index * PAGE_SIZE_MIN;
        // SAFETY(@ptrFromInt/WasmPagePool.create): `addr` is the base of the
        // freshly grown, page-aligned, nonzero linear-memory region just
        // reserved for one `Item`; it is non-null (page 0's base is only returned
        // when memory previously had size 0, which cannot happen for a running
        // module whose first page is the data/stack region).
        Ok(unsafe { NonNull::new_unchecked(addr as *mut Item) })
    }

    pub fn destroy(&self, ptr: NonNull<Item>) {
        let mut node: NonNull<Node> = ptr.cast::<Node>();
        // SAFETY(std.SinglyLinkedList.Node write/WasmPagePool.destroy): `ptr` is a
        // live item at least `@sizeOf(Node)` bytes (enforced by the comptime
        // checks on wasm); reinterpreting its first bytes as a `Node` and zeroing
        // the link matches the Zig `node.* = .{}` before prepending it.
        unsafe { node.as_mut().next = None };
        Item::free_list().prepend(node);
    }
}

/// The runtime-item-size instantiation of [`WasmPagePool`], for consumers whose
/// item size is a runtime value.
///
/// PORT NOTE: the Zig `WasmPagePool(comptime Item: type)` is consumed by
/// `PageList` as `WasmPagePool([std_size]u8)`, where `std_size =
/// Page.layout(std_capacity).total_size` is comptime. The Rust port's
/// `std_size` is a RUNTIME value (it cannot be a const-generic `[std_size]u8`
/// array length), so — exactly like `rustty_zigstd`'s
/// `DynamicMemoryPoolAligned` is the runtime-size instantiation of Zig's
/// `MemoryPoolAligned` — this type carries `item_size` at runtime and the
/// per-Item-type shared free list as a caller-supplied `&'static FreeList`.
/// Every pool sharing one `FreeList` MUST be constructed with the same
/// `item_size` (the Rust spelling of "the same Item type"); `init_capacity`
/// asserts the same invariants the Zig comptime block checks.
///
/// All other behavior is line-for-line `WasmPagePool`: `create` pops the shared
/// free list or grows linear memory by exactly one item, `destroy` prepends the
/// item to the shared free list, `deinit`/`reset` intentionally retain every
/// freed item for the module instance.
pub struct DynamicWasmPagePool<A> {
    /// The allocator this managed pool was initialized with,
    /// carried for interface compatibility with the std managed
    /// memory pools. Pool items never come from it (they come
    /// from memory.grow), but callers may use it for related
    /// allocations that don't fit the pool.
    pub allocator: A,

    /// The shared free list — the runtime stand-in for the Zig
    /// per-monomorphization `var free_list` (see `WasmPageItem`).
    free_list: &'static FreeList,

    /// `@sizeOf(Item)` — the runtime item size, identical for every pool
    /// sharing `free_list`.
    item_size: usize,
}

impl<A> DynamicWasmPagePool<A> {
    /// `WasmPagePool.initCapacity`, plus the runtime `free_list`/`item_size`
    /// pair that the comptime instantiation carries in its type.
    pub fn init_capacity<G>(
        gpa: G,
        allocator: A,
        free_list: &'static FreeList,
        item_size: usize,
        preheat: usize,
    ) -> Result<DynamicWasmPagePool<A>, OutOfMemory> {
        // PORT NOTE: the runtime spelling of the Zig `comptime { ... }` block
        // (`WasmPagePool::COMPTIME_CHECKS`); `@alignOf(Item) <=
        // page_size_min` has no runtime analog here because a byte buffer's
        // natural alignment is 1 — `create` returns wasm-page-aligned items.
        // Items are whole wasm pages
        assert!(item_size > 0);
        assert!(item_size.is_multiple_of(PAGE_SIZE_MIN));
        // Free items store the intrusive node in their bytes.
        assert!(item_size >= core::mem::size_of::<Node>());

        // The free list is intrusive so we never allocate from the
        // general purpose allocator.
        let _ = gpa;

        // Preheating is intentionally a no-op: item creation is a
        // cheap memory.grow or free-list pop. And we want to avoid
        // the preallocation mem cost.
        let _ = preheat;
        Ok(DynamicWasmPagePool {
            allocator,
            free_list,
            item_size,
        })
    }

    /// `WasmPagePool.item_size` (the runtime value).
    pub fn item_size(&self) -> usize {
        self.item_size
    }

    /// `allocator`, spelled as the native pool's getter
    /// (`UntouchedPoolBase::allocator`) so PageList reads either pool alike.
    pub fn allocator(&self) -> &A {
        &self.allocator
    }

    pub fn deinit(&mut self) {
        // Items are global to the instance and outlive the pool so
        // other pools can reuse them. Callers must destroy() any
        // items they still hold before deinit or they leak.
    }

    pub fn reset(&mut self, mode: ResetMode) -> bool {
        // There is nothing to trim: linear memory cannot shrink,
        // so retaining every freed item for reuse is always the
        // right policy on wasm. As with deinit, live items must be
        // destroyed by the caller first.
        let _ = mode;
        true
    }

    /// `WasmPagePool.create`: pop a recycled item off the shared free list, or
    /// grow linear memory by exactly one item. The returned pointer addresses
    /// `item_size` bytes, wasm-page-aligned; fresh items are zero (wasm spec),
    /// recycled items are dirty (see the module doc comment).
    #[cfg(target_arch = "wasm32")]
    pub fn create(&mut self) -> Result<NonNull<u8>, OutOfMemory> {
        // If we have a free item, use it.
        if let Some(node) = self.free_list.pop_first() {
            return Ok(node.cast::<u8>());
        }

        // Grow linear memory by exactly one item.
        let pages = self.item_size / PAGE_SIZE_MIN;
        let page_index = wasm32::memory_grow(0, pages);
        if page_index == usize::MAX {
            return Err(OutOfMemory);
        }
        let addr = page_index * PAGE_SIZE_MIN;
        // SAFETY(@ptrFromInt/DynamicWasmPagePool.create): as in
        // `WasmPagePool::create` — `addr` is the base of the freshly grown,
        // page-aligned, non-null linear-memory region just reserved for one
        // item.
        Ok(unsafe { NonNull::new_unchecked(addr as *mut u8) })
    }

    /// `WasmPagePool.destroy`.
    ///
    /// # Safety
    /// `ptr` must be a live item handed out by a pool sharing this pool's
    /// `free_list` (i.e. of this pool's `item_size`), not yet destroyed.
    /// (`unsafe` — unlike the typed `WasmPagePool::destroy` — because the
    /// runtime pool cannot type-check that `ptr` addresses `item_size` bytes.)
    pub unsafe fn destroy(&mut self, ptr: NonNull<u8>) {
        let mut node: NonNull<Node> = ptr.cast::<Node>();
        // SAFETY(std.SinglyLinkedList.Node write/DynamicWasmPagePool.destroy):
        // `ptr` is a live item at least `@sizeOf(Node)` bytes (enforced by the
        // `init_capacity` asserts); reinterpreting its first bytes as a `Node`
        // and zeroing the link matches the Zig `node.* = .{}` before prepending.
        unsafe { node.as_mut().next = None };
        self.free_list.prepend(node);
    }
}

#[cfg(test)]
mod tests {
    #[cfg(target_arch = "wasm32")]
    use super::*;

    #[test]
    #[cfg_attr(
        not(target_arch = "wasm32"),
        ignore = "SkipZigTest: wasm only (memory.grow) (R14)"
    )]
    fn wasm_page_pool() {
        // if (!comptime builtin.target.cpu.arch.isWasm()) return error.SkipZigTest;
        //
        // Skip on non-wasm targets (the reference returns error.SkipZigTest): the
        // `cfg_attr(.., ignore)` above reports it `ignored` there (R14). The pool's
        // `create`/growth path uses `memory.grow`, which only exists on wasm;
        // nothing below is instantiated on other targets.

        #[cfg(target_arch = "wasm32")]
        {
            /// The test Item: one 64KiB wasm page.
            #[allow(dead_code)]
            struct Page([u8; 64 * 1024]);

            static PAGE_FREE_LIST: FreeList = FreeList::new();
            impl WasmPageItem for Page {
                fn free_list() -> &'static FreeList {
                    &PAGE_FREE_LIST
                }
            }

            type Pool = WasmPagePool<Page, ()>;
            let mut pool: Pool = Pool::init_capacity((), (), 0).unwrap();
            // defer pool.deinit();

            let a = pool.create().unwrap();
            let b = pool.create().unwrap();
            assert!(a != b);

            // Freed items are recycled, most recent first.
            pool.destroy(b);
            assert_eq!(b, pool.create().unwrap());

            // The free list is shared across pools of the same Item type.
            let mut pool2: Pool = Pool::init_capacity((), (), 0).unwrap();
            pool.destroy(a);
            assert_eq!(a, pool2.create().unwrap());

            pool.destroy(a);
            pool.destroy(b);

            pool2.deinit();
            pool.deinit();
        }
    }
}
