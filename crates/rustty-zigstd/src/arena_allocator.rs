//! Port of `std.heap.arena_allocator` (Zig 0.15.2) — ADDITIVE INFRA GLUE.
//!
//! PORT STATUS: infra (faithful-semantics port of Zig stdlib; not in the 3234
//! contract)
//!
//! This allocator takes an existing ("child") allocator, wraps it, and provides
//! an interface where you can allocate and then free it all together. Calls to
//! free an individual item only free the item if it was the most recent
//! allocation, otherwise calls to free do nothing.
//!
//! This is a faithful port of `std.heap.ArenaAllocator`: it reproduces the exact
//! observable semantics the port relies on — a growing `BufNode` linked list, a
//! bump pointer (`end_index`), per-node geometric growth, "free the most recent"
//! `free`/`resize`, and the three `reset` modes (`FreeAll`, `RetainCapacity`,
//! `RetainWithLimit(n)`).
//!
//! Faithful to Zig, the arena stores a `child_allocator` and `init(child)`
//! always takes one (Zig: `ArenaAllocator.init(child_allocator)`). The backing
//! nodes are allocated/freed through that child via the `allocator-api2`
//! `Allocator` trait — the stable polyfill of `core::alloc::Allocator` and the
//! faithful map of Zig's `std.mem.Allocator` (`allocate`/`deallocate` map to
//! `rawAlloc`/`rawFree`). Every existing caller passes
//! [`allocator_api2::alloc::Global`], which is exactly the prior `std::alloc`
//! path, so observable behavior is unchanged.
//!
//! The Zig original stores a `BufNode` *header* at the front of each backing
//! allocation, followed by the usable bytes:
//!
//! ```text
//!   [ BufNode header | ...usable bytes... ]
//!   ^ node ptr        ^ node ptr + size_of(BufNode)
//! ```
//!
//! `BufNode.data` is the *total* size of the backing allocation (header +
//! usable). We reproduce this layout byte-for-byte so the geometric-growth and
//! reset arithmetic match Zig exactly.

use allocator_api2::alloc::{Allocator, Global, Layout};
use core::ptr::NonNull;

/// Header stored at the front of every backing allocation, exactly like Zig's
/// `BufNode`. `data` is the total size (in bytes) of the backing allocation,
/// i.e. `size_of::<BufNode>() + usable_len`. `next` is the LIFO singly-linked
/// list pointer (Zig: `node.next`).
///
/// In Zig `BufNode` is `struct { data: usize, node: SinglyLinkedList.Node }`
/// where the node is just a `?*Node`. We fold the link directly into the header.
#[repr(C)]
struct BufNode {
    data: usize,
    next: Option<NonNull<BufNode>>,
}

/// The alignment Zig uses for every backing allocation: `Alignment.of(BufNode)`.
/// We use the alignment of our `BufNode` which mirrors Zig's `BufNode` (two
/// pointer-sized fields, so pointer alignment).
#[inline]
fn buf_node_alignment() -> usize {
    core::mem::align_of::<BufNode>()
}

#[inline]
fn buf_node_size() -> usize {
    core::mem::size_of::<BufNode>()
}

/// `mem.alignForward(usize, addr, ptr_align)` — round `addr` up to a multiple of
/// `align` (a power of two). Faithful to Zig's `alignForward`.
#[inline]
fn align_forward(addr: usize, align: usize) -> usize {
    debug_assert!(align.is_power_of_two());
    (addr + (align - 1)) & !(align - 1)
}

/// How `reset` should handle the currently allocated memory. Faithful port of
/// `std.heap.ArenaAllocator.ResetMode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResetMode {
    /// Releases all allocated memory in the arena.
    FreeAll,
    /// Pre-heat the arena for future allocations by allocating a large enough
    /// buffer for all previously done allocations. Preheating speeds up future
    /// allocation by invoking the backing allocator less often.
    RetainCapacity,
    /// Same as `RetainCapacity`, but the memory will be shrunk to this value if
    /// it exceeds the limit.
    RetainWithLimit(usize),
}

/// Faithful port of `std.heap.ArenaAllocator`.
///
/// Inner state mirrors Zig's `State`: a `buffer_list` (singly linked, LIFO
/// prepend) and an `end_index` bump pointer into the *first* node. Like Zig,
/// the arena also stores its `child_allocator` (`child`) — backing nodes are
/// allocated and freed through it.
///
/// `A` is the child allocator type (`allocator-api2`'s `Allocator` trait, the
/// faithful map of Zig's `std.mem.Allocator`). It defaults to
/// [`Global`] — the prior `std::alloc` path — so existing call sites that do
/// not care about the child stay terse and behavior-identical.
pub struct ArenaAllocator<A: Allocator = Global> {
    /// The backing ("child") allocator. Faithful to Zig's `child_allocator`:
    /// every backing node is allocated via `child.allocate` and freed via
    /// `child.deallocate`.
    child: A,
    /// LIFO list of backing nodes. `first` is the most-recently-created node and
    /// is the one we bump into.
    first: Option<NonNull<BufNode>>,
    /// Bump offset into the usable region of `first`.
    end_index: usize,
}

impl ArenaAllocator<Global> {
    /// Creates a new arena over the global (`std::alloc`) child allocator.
    ///
    /// Convenience for `ArenaAllocator::with_child(Global)`; equivalent to Zig's
    /// `ArenaAllocator.init(std.heap.page_allocator/gpa)` with the system
    /// allocator as the child.
    pub fn init() -> ArenaAllocator<Global> {
        ArenaAllocator::with_child(Global)
    }
}

impl<A: Allocator> ArenaAllocator<A> {
    /// Faithful port of `ArenaAllocator.init(child_allocator)`: creates a new
    /// arena that allocates its backing nodes through `child`.
    pub fn with_child(child: A) -> ArenaAllocator<A> {
        ArenaAllocator {
            child,
            first: None,
            end_index: 0,
        }
    }

    /// Layout for a backing allocation of `total` bytes with `BufNode`
    /// alignment. `total` always includes the `BufNode` header.
    #[inline]
    fn node_layout(total: usize) -> Layout {
        // SAFETY-NOTE (parity): alignment is a power of two (align_of) and
        // `total` is a finite size computed from the geometric growth formula;
        // Layout::from_size_align validates both.
        Layout::from_size_align(total, buf_node_alignment())
            .expect("arena node layout (size/align) must be valid")
    }

    /// `child_allocator.rawFree` of an entire backing node allocation.
    ///
    /// # Safety
    /// `node` must point to a live backing allocation previously produced by
    /// `raw_alloc_node`/`create_node` *of this arena*, whose `data` field is its
    /// true total size. After this call the node memory is freed and must not be
    /// used.
    unsafe fn raw_free_node(&self, node: NonNull<BufNode>) {
        let total = unsafe { node.as_ref().data };
        let layout = Self::node_layout(total);
        // SAFETY: `node` was allocated from `self.child` with this exact layout
        // (same align, same `total` recorded in `data`). Maps to Zig
        // `child_allocator.rawFree`.
        unsafe {
            self.child.deallocate(node.cast::<u8>(), layout);
        }
    }

    /// Faithful port of `deinit`: free every node in the buffer list.
    fn free_all_nodes(&mut self) {
        let mut it = self.first;
        while let Some(node) = it {
            // This has to occur before the free because the free frees node.
            let next_it = unsafe { node.as_ref().next };
            unsafe { self.raw_free_node(node) };
            it = next_it;
        }
        self.first = None;
        self.end_index = 0;
    }

    /// Faithful port of `queryCapacity`: total usable bytes across all nodes
    /// (excludes the `BufNode` header storage), i.e. `data - size_of(BufNode)`
    /// summed over nodes.
    pub fn query_capacity(&self) -> usize {
        let mut size: usize = 0;
        let mut it = self.first;
        while let Some(node) = it {
            let data = unsafe { node.as_ref().data };
            size += data - buf_node_size();
            it = unsafe { node.as_ref().next };
        }
        size
    }

    /// Allocate a fresh backing node of `total` bytes, writing the `BufNode`
    /// header. Returns `None` on allocation failure (faithful to Zig's
    /// `rawAlloc orelse return null`). The node's `next` is left as the caller
    /// wants; here we initialize it to `None` and the caller prepends.
    fn raw_alloc_node(&self, total: usize) -> Option<NonNull<BufNode>> {
        let layout = Self::node_layout(total);
        // Maps to Zig `child_allocator.rawAlloc(...) orelse return null`.
        // `allocate` returns `NonNull<[u8]>`; we only need the data pointer.
        let raw = self.child.allocate(layout).ok()?;
        let ptr = raw.cast::<BufNode>();
        // SAFETY: `ptr` is freshly allocated by the child, suitably aligned and
        // sized for a `BufNode` (layout uses BufNode alignment, total >=
        // size_of::<BufNode>()); we initialize all fields before any read.
        unsafe {
            ptr.as_ptr().write(BufNode {
                data: total,
                next: None,
            });
        }
        Some(ptr)
    }

    /// Faithful port of `createNode`: geometric growth, prepend, reset bump.
    ///
    /// `actual_min_size = minimum_size + (size_of(BufNode) + 16)`
    /// `big_enough_len = prev_len + actual_min_size`
    /// `len = big_enough_len + big_enough_len / 2`  (i.e. *1.5)
    fn create_node(&mut self, prev_len: usize, minimum_size: usize) -> Option<NonNull<BufNode>> {
        let actual_min_size = minimum_size + (buf_node_size() + 16);
        let big_enough_len = prev_len + actual_min_size;
        let len = big_enough_len + big_enough_len / 2;
        let mut buf_node = self.raw_alloc_node(len)?;
        // prepend(&buf_node.node): new node becomes the head.
        unsafe { buf_node.as_mut().next = self.first };
        self.first = Some(buf_node);
        self.end_index = 0;
        Some(buf_node)
    }

    /// Pointer to the usable region of `node` and its usable length.
    /// Usable region begins `size_of(BufNode)` bytes past the node header and
    /// runs to `data`.
    #[inline]
    unsafe fn usable(node: NonNull<BufNode>) -> (*mut u8, usize) {
        let data = unsafe { node.as_ref().data };
        let base = node.as_ptr().cast::<u8>();
        let usable_ptr = unsafe { base.add(buf_node_size()) };
        (usable_ptr, data - buf_node_size())
    }

    /// Faithful port of `alloc` from the `Allocator` vtable. Returns a pointer
    /// to `n` bytes aligned to `align` (a power of two), or `None` on OOM.
    ///
    /// The returned memory is valid until the arena is reset or dropped (or, for
    /// the most-recent allocation only, freed via `free`).
    pub fn alloc_bytes(&mut self, n: usize, align: usize) -> Option<NonNull<u8>> {
        debug_assert!(align.is_power_of_two());
        let ptr_align = align;

        let mut cur_node = match self.first {
            Some(first) => first,
            None => self.create_node(0, n + ptr_align)?,
        };
        loop {
            let (cur_buf_ptr, cur_buf_len) = unsafe { Self::usable(cur_node) };
            let addr = cur_buf_ptr as usize + self.end_index;
            let adjusted_addr = align_forward(addr, ptr_align);
            let adjusted_index = self.end_index + (adjusted_addr - addr);
            let new_end_index = adjusted_index + n;

            if new_end_index <= cur_buf_len {
                // SAFETY: adjusted_index..new_end_index is within the usable
                // region (new_end_index <= cur_buf_len).
                let result = unsafe { cur_buf_ptr.add(adjusted_index) };
                self.end_index = new_end_index;
                return NonNull::new(result);
            }

            // Doesn't fit. Zig tries rawResize of the current node to grow it.
            // The Rust system allocator's `realloc` may move memory, which would
            // invalidate already-handed-out pointers in this node — so we must
            // NOT use realloc here. Zig's child allocators (page/gpa) only report
            // a resize as successful when it happens *in place*. The faithful,
            // sound behavior is therefore: in-place grow is not available, so we
            // allocate a new (bigger) node — exactly Zig's fallback branch.
            cur_node = self.create_node(cur_buf_len, n + ptr_align)?;
        }
    }

    /// Faithful port of `resize` from the vtable, used by `free`-adjacent logic
    /// and by callers that grow/shrink the most-recent allocation. `buf` is the
    /// pointer+len of a previously handed-out allocation. Returns whether the
    /// resize succeeded; on success the bump pointer is adjusted.
    ///
    /// # Safety
    /// `buf_ptr`/`buf_len` must describe a slice previously returned by
    /// `alloc_bytes` from this arena that is still live.
    pub unsafe fn resize(&mut self, buf_ptr: NonNull<u8>, buf_len: usize, new_len: usize) -> bool {
        let cur_node = match self.first {
            Some(n) => n,
            None => return false,
        };
        let (cur_buf_ptr, cur_buf_len) = unsafe { Self::usable(cur_node) };
        if cur_buf_ptr as usize + self.end_index != buf_ptr.as_ptr() as usize + buf_len {
            // It's not the most recent allocation, so it cannot be expanded,
            // but it's fine if they want to make it smaller.
            return new_len <= buf_len;
        }

        if buf_len >= new_len {
            self.end_index -= buf_len - new_len;
            true
        } else if cur_buf_len - self.end_index >= new_len - buf_len {
            self.end_index += new_len - buf_len;
            true
        } else {
            false
        }
    }

    /// Faithful port of `free` from the vtable: only frees (rolls back the bump
    /// pointer) if `buf` was the most recent allocation; otherwise a no-op.
    ///
    /// # Safety
    /// `buf_ptr`/`buf_len` must describe a slice previously returned by
    /// `alloc_bytes` from this arena.
    pub unsafe fn free(&mut self, buf_ptr: NonNull<u8>, buf_len: usize) {
        let cur_node = match self.first {
            Some(n) => n,
            None => return,
        };
        let (cur_buf_ptr, _) = unsafe { Self::usable(cur_node) };
        if cur_buf_ptr as usize + self.end_index == buf_ptr.as_ptr() as usize + buf_len {
            self.end_index -= buf_len;
        }
    }

    /// Faithful port of `reset(mode)`.
    ///
    /// Returns whether the reset operation was successful. If a reallocation
    /// failed, `false` is returned; the arena is still fully functional (all
    /// memory released), future allocations might just be slower. If `mode` is
    /// `FreeAll`, this always returns `true`.
    pub fn reset(&mut self, mode: ResetMode) -> bool {
        let requested_capacity = match mode {
            ResetMode::RetainCapacity => self.query_capacity(),
            ResetMode::RetainWithLimit(limit) => limit.min(self.query_capacity()),
            ResetMode::FreeAll => 0,
        };
        if requested_capacity == 0 {
            // Just reset when we don't have anything to reallocate.
            self.free_all_nodes();
            return true;
        }
        let total_size = requested_capacity + buf_node_size();

        // Free all nodes except for the last one (the originally-first-allocated
        // node, which sits at the tail of the LIFO list).
        let mut it = self.first;
        let mut maybe_first_node: Option<NonNull<BufNode>> = None;
        while let Some(node) = it {
            // This has to occur before the free because the free frees node.
            let next_it = unsafe { node.as_ref().next };
            if next_it.is_none() {
                maybe_first_node = Some(node);
                break;
            }
            unsafe { self.raw_free_node(node) };
            it = next_it;
        }
        debug_assert!(
            maybe_first_node.is_none()
                || unsafe { maybe_first_node.unwrap().as_ref().next.is_none() }
        );
        // Reset the state before resizing buffers so the arena is definitely 0.
        self.end_index = 0;

        if let Some(mut first_node) = maybe_first_node {
            // Sever its link (it is now the only node).
            unsafe { first_node.as_mut().next = None };
            self.first = Some(first_node);
            let cur_total = unsafe { first_node.as_ref().data };
            if cur_total == total_size {
                // Perfect, no need to invoke the child allocator.
                return true;
            }
            // Zig tries `rawResize` first and, if that fails (the general case
            // for an arbitrary child allocator), does a "manual realloc":
            // `rawAlloc(total_size) orelse return false`, then `rawFree(old)`.
            // The `allocator-api2` `Allocator::grow` is not guaranteed in-place
            // and may move the block; since the retained node has NO live
            // allocations carved from it (we reset `end_index` to 0 and, by the
            // arena contract, outstanding references die at reset), the faithful
            // map is exactly Zig's manual-realloc fallback: allocate a fresh
            // node from the child and free the old one.
            match self.raw_alloc_node(total_size) {
                Some(new_node) => {
                    // SAFETY: `first_node` is the live retained node, allocated
                    // from `self.child` with `cur_total` recorded in `data`.
                    unsafe { self.raw_free_node(first_node) };
                    // `raw_alloc_node` already wrote `{ data: total_size, next:
                    // None }` into `new_node`; install it as the sole node.
                    self.first = Some(new_node);
                    true
                }
                None => {
                    // We failed to preheat the arena properly. The original
                    // `first_node` is still live and installed as `self.first`,
                    // so the arena stays fully functional. Signal failure to the
                    // user, matching Zig.
                    false
                }
            }
        } else {
            true
        }
    }

    /// Number of backing nodes currently in the buffer list. Test/inspection
    /// helper (mirrors walking `state.buffer_list`).
    pub fn node_count(&self) -> usize {
        let mut n = 0;
        let mut it = self.first;
        while let Some(node) = it {
            n += 1;
            it = unsafe { node.as_ref().next };
        }
        n
    }

    /// The current bump offset into the first node. Test/inspection helper.
    pub fn end_index(&self) -> usize {
        self.end_index
    }
}

impl<A: Allocator> Drop for ArenaAllocator<A> {
    fn drop(&mut self) {
        // Faithful port of `deinit`.
        self.free_all_nodes();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Faithful to Zig `ArenaAllocator.init(child_allocator)`: the child is a
    /// real, threaded parameter. `with_child(Global)` is the explicit form of
    /// `init()` and must behave identically (Global == the `std::alloc` path).
    #[test]
    fn with_child_global_threads_explicit_allocator() {
        let mut arena = ArenaAllocator::with_child(Global);
        let p = arena.alloc_bytes(128, 16).unwrap();
        assert_eq!(p.as_ptr() as usize % 16, 0);
        unsafe { core::ptr::write_bytes(p.as_ptr(), 0x5A, 128) };
        assert!(arena.node_count() >= 1);
        assert!(arena.reset(ResetMode::RetainCapacity));
        let q = arena.alloc_bytes(64, 8).unwrap();
        unsafe { core::ptr::write_bytes(q.as_ptr(), 0, 64) };
    }

    /// Faithful analog of basic alloc patterns: many allocations, all live,
    /// distinct, and correctly sized/aligned.
    #[test]
    fn alloc_basic_distinct_and_aligned() {
        // Explicit child threading (faithful to Zig init(child)); Global is the
        // prior std::alloc path so behavior is unchanged.
        let mut arena = ArenaAllocator::with_child(Global);
        let mut ptrs = Vec::new();
        for i in 0..64usize {
            let n = 16 + i;
            let p = arena.alloc_bytes(n, 8).unwrap();
            assert_eq!(p.as_ptr() as usize % 8, 0, "alignment");
            // Write to prove the memory is usable (Miri checks bounds/init).
            unsafe {
                core::ptr::write_bytes(p.as_ptr(), 0xAB, n);
            }
            ptrs.push(p);
        }
        // All distinct.
        for i in 0..ptrs.len() {
            for j in (i + 1)..ptrs.len() {
                assert_ne!(ptrs[i].as_ptr(), ptrs[j].as_ptr());
            }
        }
    }

    /// Various alignments are honored.
    #[test]
    fn alloc_alignment() {
        let mut arena = ArenaAllocator::init();
        for &align in &[1usize, 2, 4, 8, 16, 32, 64, 128] {
            let p = arena.alloc_bytes(40, align).unwrap();
            assert_eq!(p.as_ptr() as usize % align, 0, "align {align}");
            unsafe { core::ptr::write_bytes(p.as_ptr(), 1, 40) };
        }
    }

    /// Port of the Zig test "reset with preheating": many rounds of
    /// reset(.retain_capacity) + allocs, alignment 32, sizes returned exactly.
    #[test]
    fn reset_with_preheating() {
        let mut arena = ArenaAllocator::init();
        // Deterministic pseudo-random (no std rng dep): simple LCG.
        let mut s: u64 = 0x9E3779B97F4A7C15;
        let mut next = |lo: usize, hi: usize| -> usize {
            s = s
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            lo + (s >> 33) as usize % (hi - lo + 1)
        };
        for _ in 0..25 {
            assert!(arena.reset(ResetMode::RetainCapacity));
            let mut alloced: usize = 0;
            let total_size = next(256, 16384);
            while alloced < total_size {
                let size = next(16, 256);
                let p = arena.alloc_bytes(size, 32).unwrap();
                assert_eq!(p.as_ptr() as usize % 32, 0);
                unsafe { core::ptr::write_bytes(p.as_ptr(), 7, size) };
                alloced += size;
            }
        }
    }

    /// Port of the Zig test "reset while retaining a buffer".
    #[test]
    fn reset_while_retaining_a_buffer() {
        let mut arena = ArenaAllocator::init();
        // Create two internal buffers.
        let _ = arena.alloc_bytes(1, 1).unwrap();
        let _ = arena.alloc_bytes(1000, 1).unwrap();
        // Check that we have at least two buffers.
        assert!(arena.node_count() >= 2);
        // This retains the first allocated buffer.
        assert!(arena.reset(ResetMode::RetainWithLimit(1)));
    }

    /// FreeAll empties everything and the arena keeps working.
    #[test]
    fn reset_free_all() {
        let mut arena = ArenaAllocator::init();
        let _ = arena.alloc_bytes(100, 8).unwrap();
        let _ = arena.alloc_bytes(5000, 8).unwrap();
        assert!(arena.node_count() >= 1);
        assert!(arena.reset(ResetMode::FreeAll));
        assert_eq!(arena.node_count(), 0);
        assert_eq!(arena.query_capacity(), 0);
        // Still usable.
        let p = arena.alloc_bytes(10, 8).unwrap();
        unsafe { core::ptr::write_bytes(p.as_ptr(), 0, 10) };
    }

    /// RetainCapacity keeps total capacity >= what was used, in one node.
    #[test]
    fn reset_retain_capacity_collapses_to_one_node() {
        let mut arena = ArenaAllocator::init();
        // Force several nodes by allocating larger and larger blocks.
        for _ in 0..5 {
            let _ = arena.alloc_bytes(4096, 16).unwrap();
        }
        let cap_before = arena.query_capacity();
        assert!(cap_before >= 5 * 4096);
        assert!(arena.reset(ResetMode::RetainCapacity));
        // Collapses to a single node holding (at least) the requested capacity.
        assert_eq!(arena.node_count(), 1);
        assert_eq!(arena.end_index(), 0);
        assert!(arena.query_capacity() >= cap_before);
        // And it is reusable without hitting the backing allocator for small
        // allocations within the retained capacity.
        let nodes_before = arena.node_count();
        let _ = arena.alloc_bytes(100, 8).unwrap();
        assert_eq!(arena.node_count(), nodes_before);
    }

    /// RetainWithLimit shrinks capacity down to <= limit and keeps one node.
    #[test]
    fn reset_retain_with_limit_shrinks() {
        let mut arena = ArenaAllocator::init();
        for _ in 0..5 {
            let _ = arena.alloc_bytes(4096, 16).unwrap();
        }
        assert!(arena.query_capacity() > 8192);
        let limit = 8192usize;
        assert!(arena.reset(ResetMode::RetainWithLimit(limit)));
        assert_eq!(arena.node_count(), 1);
        // Retained usable capacity equals the limit (min(limit, prev cap)).
        assert_eq!(arena.query_capacity(), limit);
        // Reuse within the retained capacity does not allocate a new node.
        let nodes_before = arena.node_count();
        let p = arena.alloc_bytes(limit, 1).unwrap();
        unsafe { core::ptr::write_bytes(p.as_ptr(), 0, limit) };
        assert_eq!(arena.node_count(), nodes_before);
    }

    /// RetainWithLimit(huge) behaves like RetainCapacity (limit clamps to cap).
    #[test]
    fn reset_retain_with_limit_above_capacity() {
        let mut arena = ArenaAllocator::init();
        let _ = arena.alloc_bytes(1000, 8).unwrap();
        let cap = arena.query_capacity();
        assert!(arena.reset(ResetMode::RetainWithLimit(usize::MAX)));
        assert_eq!(arena.node_count(), 1);
        // min(MAX, cap) == cap retained.
        assert_eq!(arena.query_capacity(), cap);
    }

    /// free() rolls back only the most-recent allocation (bump pointer).
    #[test]
    fn free_only_most_recent() {
        let mut arena = ArenaAllocator::init();
        let a = arena.alloc_bytes(64, 8).unwrap();
        let end_after_a = arena.end_index();
        let b = arena.alloc_bytes(64, 8).unwrap();
        // Freeing the older `a` is a no-op.
        unsafe { arena.free(a, 64) };
        assert!(arena.end_index() > end_after_a);
        // Freeing the most-recent `b` rolls back.
        unsafe { arena.free(b, 64) };
        assert_eq!(arena.end_index(), end_after_a);
        // The rolled-back space is reused by the next allocation.
        let c = arena.alloc_bytes(64, 8).unwrap();
        assert_eq!(b.as_ptr(), c.as_ptr());
    }

    /// resize() grows/shrinks the most-recent allocation in place.
    #[test]
    fn resize_most_recent() {
        let mut arena = ArenaAllocator::init();
        let _pad = arena.alloc_bytes(16, 8).unwrap();
        let p = arena.alloc_bytes(64, 8).unwrap();
        // Shrink.
        assert!(unsafe { arena.resize(p, 64, 32) });
        // Grow back (space is available within the node).
        assert!(unsafe { arena.resize(p, 32, 64) });
        // Shrinking a non-most-recent allocation succeeds; growing fails.
        let q = arena.alloc_bytes(8, 8).unwrap();
        assert!(unsafe { arena.resize(p, 64, 10) }); // shrink ok even if not most-recent
        assert!(!unsafe { arena.resize(p, 64, 128) }); // can't grow non-most-recent
        let _ = q;
    }
}
