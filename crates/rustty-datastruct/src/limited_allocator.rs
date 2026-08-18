//! PORT NOTE: Ports ghostty `src/datastruct/limited_allocator.zig`.
//!
//! An allocator that rejects any single allocation or resize larger than a
//! configured byte limit. The limit applies to each request independently,
//! not to the total amount of memory currently allocated.
//!
//! PORT NOTE (allocator interface): the Zig reference wraps a runtime
//! `std.mem.Allocator` (`child: Allocator`) and re-exposes the same interface
//! via `allocator()`. rustty-lib models `std.mem.Allocator` as its `ZigAllocator`
//! trait, but `rustty-datastruct` is a leaf below `rustty-lib` (R4) and cannot
//! depend on it. We therefore define a small self-contained equivalent here —
//! the `RawAllocator` trait plus `Alignment`/`RawSlice`/`OutOfMemory` — with the
//! SAME semantics as `std.mem.Allocator`'s raw vtable (`rawAlloc`/`rawResize`/
//! `rawRemap`/`rawFree`) and typed helpers. `LimitedAllocator<A>` is generic over
//! the child (the Rust analog of the erased `child: Allocator` value); its
//! `allocator()` returns `&Self`, i.e. itself viewed as the wrapped interface,
//! exactly like `LimitedAllocator.allocator()` returns a vtable over `self`.
//!
//! `limit_exceeded` is set from inside a raw callback while `allocator()`'s
//! borrow of `self` is live and the test also reads/writes the field, so it is a
//! `Cell<bool>` (the raw callbacks take `&self`, mirroring the erased mutable
//! `ctx` in Zig via interior mutability).

use std::cell::Cell;

/// PORT NOTE: `std.mem.Alignment` — an enum whose integer value is the log2 of
/// the alignment in bytes. Modeled as a transparent `u8` (`@intFromEnum` value).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Alignment(pub u8);

impl Alignment {
    pub fn from_byte_units(n: usize) -> Alignment {
        debug_assert!(n.is_power_of_two());
        Alignment(n.trailing_zeros() as u8)
    }

    pub fn to_byte_units(self) -> usize {
        1usize << self.0
    }
}

/// PORT NOTE: the Rust embodiment of a Zig `[]u8` — a `(ptr, len)` pair with NO
/// Rust aliasing/borrow guarantees (exactly like Zig's `memory: []u8`). Used
/// instead of `&mut [u8]` so that freeing/realloc'ing the memory does not
/// deallocate behind a live, strongly-protected Rust reference (which is UB
/// under Stacked Borrows).
#[derive(Debug, Clone, Copy)]
pub struct RawSlice {
    pub ptr: *mut u8,
    pub len: usize,
}

impl RawSlice {
    pub fn new(ptr: *mut u8, len: usize) -> RawSlice {
        RawSlice { ptr, len }
    }
}

/// PORT NOTE: Zig `error.OutOfMemory` (`Allocator.Error`), as returned by the
/// typed `alloc` helper when the (raw) allocator rejects the request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutOfMemory;

/// PORT NOTE: the raw allocator interface — the rustty-datastruct analog of
/// `std.mem.Allocator`'s vtable. The four raw methods mirror
/// `rawAlloc`/`rawResize`/`rawRemap`/`rawFree`; a null `*mut u8` models the Zig
/// `?[*]u8` `null` (failure). `ret_addr` is the return-address hint (unused by
/// these allocators), preserved for interface fidelity. The typed helpers below
/// mirror the subset of `std.mem.Allocator`'s typed API exercised by the tests
/// (all `u8`, i.e. element size 1 / alignment 1).
pub trait RawAllocator {
    /// `rawAlloc(len, alignment, ret_addr) ?[*]u8`
    fn raw_alloc(&self, len: usize, alignment: Alignment, ret_addr: usize) -> *mut u8;
    /// `rawResize(memory, alignment, new_len, ret_addr) bool`
    fn raw_resize(
        &self,
        memory: RawSlice,
        alignment: Alignment,
        new_len: usize,
        ret_addr: usize,
    ) -> bool;
    /// `rawRemap(memory, alignment, new_len, ret_addr) ?[*]u8`
    fn raw_remap(
        &self,
        memory: RawSlice,
        alignment: Alignment,
        new_len: usize,
        ret_addr: usize,
    ) -> *mut u8;
    /// `rawFree(memory, alignment, ret_addr) void`
    fn raw_free(&self, memory: RawSlice, alignment: Alignment, ret_addr: usize);

    /// PORT NOTE: Zig typed helper `allocator.alloc(u8, n)` — allocate `n` bytes
    /// at byte (1) alignment. Returns `error.OutOfMemory` (`Err(OutOfMemory)`)
    /// when the raw allocator rejects the request (`null`).
    fn alloc_bytes(&self, n: usize) -> Result<RawSlice, OutOfMemory> {
        let ptr = self.raw_alloc(n, Alignment(0), 0);
        if ptr.is_null() {
            return Err(OutOfMemory);
        }
        Ok(RawSlice::new(ptr, n))
    }

    /// PORT NOTE: Zig typed helper `allocator.free(memory)` for a byte-aligned
    /// `[]u8`.
    fn free_bytes(&self, memory: RawSlice) {
        self.raw_free(memory, Alignment(0), 0);
    }

    /// PORT NOTE: Zig typed helper `allocator.resize(memory, new_n)` for a
    /// byte-aligned `[]u8` — returns whether the in-place resize succeeded.
    fn resize_bytes(&self, memory: RawSlice, new_n: usize) -> bool {
        self.raw_resize(memory, Alignment(0), new_n, 0)
    }

    /// PORT NOTE: Zig typed helper `allocator.remap(memory, new_n)` for a
    /// byte-aligned `[]u8` — returns `None` (Zig `null`) on failure.
    fn remap_bytes(&self, memory: RawSlice, new_n: usize) -> Option<RawSlice> {
        let ptr = self.raw_remap(memory, Alignment(0), new_n, 0);
        if ptr.is_null() {
            None
        } else {
            Some(RawSlice::new(ptr, new_n))
        }
    }
}

/// An allocator that rejects any single allocation or resize larger than a
/// configured byte limit. The limit applies to each request independently,
/// not to the total amount of memory currently allocated.
pub struct LimitedAllocator<A: RawAllocator> {
    pub child: A,
    pub limit: usize,

    /// Set after an allocation, resize, or remap is rejected for exceeding
    /// the limit. This remains set until the caller clears it.
    ///
    /// PORT NOTE: `Cell` because the raw callbacks (`&self`) set it while
    /// `allocator()`'s borrow of `self` is live (Zig mutates it through the
    /// erased `ctx` pointer).
    pub limit_exceeded: Cell<bool>,
}

impl<A: RawAllocator> LimitedAllocator<A> {
    pub fn init(child: A, limit: usize) -> LimitedAllocator<A> {
        LimitedAllocator {
            child,
            limit,
            limit_exceeded: Cell::new(false),
        }
    }

    /// PORT NOTE: `allocator()` in Zig returns an `Allocator` vtable over
    /// `self`; the Rust analog is `&Self` viewed as the wrapped `RawAllocator`
    /// interface (`Self: RawAllocator`).
    pub fn allocator(&self) -> &LimitedAllocator<A> {
        self
    }

    fn check(&self, size: usize) -> bool {
        if size <= self.limit {
            return true;
        }
        self.limit_exceeded.set(true);
        false
    }
}

impl<A: RawAllocator> RawAllocator for LimitedAllocator<A> {
    fn raw_alloc(&self, len: usize, alignment: Alignment, ret_addr: usize) -> *mut u8 {
        if !self.check(len) {
            return std::ptr::null_mut();
        }
        self.child.raw_alloc(len, alignment, ret_addr)
    }

    fn raw_resize(
        &self,
        memory: RawSlice,
        alignment: Alignment,
        new_len: usize,
        ret_addr: usize,
    ) -> bool {
        if !self.check(new_len) {
            return false;
        }
        self.child.raw_resize(memory, alignment, new_len, ret_addr)
    }

    fn raw_remap(
        &self,
        memory: RawSlice,
        alignment: Alignment,
        new_len: usize,
        ret_addr: usize,
    ) -> *mut u8 {
        if !self.check(new_len) {
            return std::ptr::null_mut();
        }
        self.child.raw_remap(memory, alignment, new_len, ret_addr)
    }

    fn raw_free(&self, memory: RawSlice, alignment: Alignment, ret_addr: usize) {
        self.child.raw_free(memory, alignment, ret_addr);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// PORT NOTE: the rustty analog of `std.testing.allocator` — a `RawAllocator`
    /// backed by Rust's global allocator. Under R6 leak/UB checking is performed
    /// by Miri (which replaces `testing.allocator`'s tracking role).
    struct GlobalAllocator;

    impl RawAllocator for GlobalAllocator {
        fn raw_alloc(&self, len: usize, alignment: Alignment, _ra: usize) -> *mut u8 {
            if len == 0 {
                // Zig: a zero-length alloc returns a valid, aligned,
                // dereference-free pointer; model it with a non-null aligned
                // sentinel so the global allocator never sees a 0-size layout.
                return alignment.to_byte_units() as *mut u8;
            }
            let layout =
                std::alloc::Layout::from_size_align(len, alignment.to_byte_units()).unwrap();
            // SAFETY(std.mem.Allocator.rawAlloc/GlobalAllocator): `layout` has a
            // nonzero size (guarded above) and a power-of-two alignment
            // (`Alignment::to_byte_units`), the contract for `std::alloc::alloc`.
            unsafe { std::alloc::alloc(layout) }
        }

        fn raw_resize(
            &self,
            memory: RawSlice,
            _alignment: Alignment,
            new_len: usize,
            _ra: usize,
        ) -> bool {
            // Conservative in-place resize: only succeed when shrinking-or-equal.
            new_len <= memory.len
        }

        fn raw_remap(
            &self,
            memory: RawSlice,
            alignment: Alignment,
            new_len: usize,
            _ra: usize,
        ) -> *mut u8 {
            if memory.len == 0 {
                return self.raw_alloc(new_len, alignment, 0);
            }
            let old_layout =
                std::alloc::Layout::from_size_align(memory.len, alignment.to_byte_units()).unwrap();
            // SAFETY(std.mem.Allocator.rawRemap/GlobalAllocator): `memory.ptr`
            // came from `raw_alloc` with `old_layout` (nonzero len guarded
            // above), the contract for `std::alloc::realloc`.
            unsafe { std::alloc::realloc(memory.ptr, old_layout, new_len) }
        }

        fn raw_free(&self, memory: RawSlice, alignment: Alignment, _ra: usize) {
            if memory.len == 0 {
                return;
            }
            let layout =
                std::alloc::Layout::from_size_align(memory.len, alignment.to_byte_units()).unwrap();
            // SAFETY(std.mem.Allocator.rawFree/GlobalAllocator): `memory.ptr`
            // came from `raw_alloc`/`raw_remap` with this exact `layout` (nonzero
            // len guarded above), the contract for `std::alloc::dealloc`.
            unsafe { std::alloc::dealloc(memory.ptr, layout) }
        }
    }

    /// PORT NOTE: the rustty analog of `std.testing.FailingAllocator` restricted
    /// to what the test observes — a `fail_index`, an allocation counter, and
    /// `has_induced_failure`. When `alloc` is reached with the running index at
    /// or past `fail_index` it induces a failure (returns `null`) and records it;
    /// otherwise it delegates to the global allocator. The test only checks that
    /// NO failure was induced (the request never reaches the child because the
    /// limit rejects it first), so resize/remap/free simply delegate.
    struct FailingAllocator {
        fail_index: usize,
        index: Cell<usize>,
        has_induced_failure: Cell<bool>,
    }

    impl FailingAllocator {
        fn init(fail_index: usize) -> FailingAllocator {
            FailingAllocator {
                fail_index,
                index: Cell::new(0),
                has_induced_failure: Cell::new(false),
            }
        }
    }

    impl RawAllocator for FailingAllocator {
        fn raw_alloc(&self, len: usize, alignment: Alignment, ra: usize) -> *mut u8 {
            if self.index.get() >= self.fail_index {
                self.has_induced_failure.set(true);
                return std::ptr::null_mut();
            }
            self.index.set(self.index.get() + 1);
            GlobalAllocator.raw_alloc(len, alignment, ra)
        }

        fn raw_resize(
            &self,
            memory: RawSlice,
            alignment: Alignment,
            new_len: usize,
            ra: usize,
        ) -> bool {
            GlobalAllocator.raw_resize(memory, alignment, new_len, ra)
        }

        fn raw_remap(
            &self,
            memory: RawSlice,
            alignment: Alignment,
            new_len: usize,
            ra: usize,
        ) -> *mut u8 {
            GlobalAllocator.raw_remap(memory, alignment, new_len, ra)
        }

        fn raw_free(&self, memory: RawSlice, alignment: Alignment, ra: usize) {
            GlobalAllocator.raw_free(memory, alignment, ra);
        }
    }

    #[test]
    fn limited_allocator_allows_allocations_through_the_limit() {
        let limited = LimitedAllocator::init(GlobalAllocator, 8);
        let alloc = limited.allocator();
        let data = alloc.alloc_bytes(8).unwrap();
        // defer alloc.free(data);
        alloc.free_bytes(data);

        assert!(!limited.limit_exceeded.get());
    }

    #[test]
    fn limited_allocator_rejects_allocations_before_the_child() {
        let failing = FailingAllocator::init(0);
        let limited = LimitedAllocator::init(failing, 8);

        assert_eq!(
            Err(OutOfMemory),
            limited.allocator().alloc_bytes(9).map(|_| ())
        );
        assert!(limited.limit_exceeded.get());
        assert!(!limited.child.has_induced_failure.get());
    }

    #[test]
    fn limited_allocator_rejects_oversized_resize_and_remap() {
        let limited = LimitedAllocator::init(GlobalAllocator, 8);
        let alloc = limited.allocator();
        let data = alloc.alloc_bytes(8).unwrap();
        // defer alloc.free(data);

        assert!(!alloc.resize_bytes(data, 9));
        assert!(limited.limit_exceeded.get());

        limited.limit_exceeded.set(false);
        assert!(alloc.remap_bytes(data, 9).is_none());
        assert!(limited.limit_exceeded.get());

        alloc.free_bytes(data);
    }
}
