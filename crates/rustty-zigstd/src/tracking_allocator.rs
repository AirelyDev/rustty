//! `TrackingAllocator` — TEST-ONLY leak-counting allocator wrapper.
//!
//! PORT STATUS: test infra (scoped faithful equivalent of Zig's
//! `std.testing.allocator`; not in the 3234 contract).
//!
//! Zig's test runner installs `std.testing.allocator`, a leak-detecting
//! general-purpose allocator: every allocation it serves is tracked, and at the
//! end of a test the runner FAILS the test if any allocation was not freed (and
//! also catches double-frees / mismatched frees). `PageList.pageAllocator()`
//! returns exactly this allocator under `builtin.is_test`, so the reference's
//! PageList tests get page-leak detection for free.
//!
//! Rust's primary UB/leak gate for pure-Rust crates is Miri (R6), which is
//! strictly stronger. But Miri is interpreted and TIMES OUT on the heavy PageList
//! paths (resize/reflow, clone over many pages). This wrapper is the scoped,
//! NATIVE-SPEED faithful equivalent of the testing allocator's leak check for
//! exactly those paths: it wraps a child allocator and atomically counts
//! outstanding bytes and allocations, so a test can assert ZERO outstanding after
//! `deinit`/`reset` — catching a leaked page where Miri cannot run.
//!
//! It is a FAITHFUL counting wrapper: every [`allocate`](Allocator::allocate)
//! increments the counters by the requested layout, every
//! [`deallocate`](Allocator::deallocate) / [`shrink`/`grow`] decrements by the
//! SAME layout, and the underlying allocation behavior is delegated verbatim to
//! the child — it changes NOTHING observable about the memory itself, it only
//! observes. Gated behind `cfg(any(test, feature = "tracking"))` so it never
//! enters a production build.

use core::alloc::Layout;
use core::ptr::NonNull;
use core::sync::atomic::{AtomicUsize, Ordering};

use allocator_api2::alloc::{AllocError, Allocator};

/// The shared counter state behind a [`TrackingAllocator`].
///
/// Split out from the wrapper so it can be `&'static`-installed where a wrapper
/// value cannot be threaded by type (e.g. a process-wide page allocator that is
/// selected, not passed) — the faithful map of Zig's testing allocator being a
/// single global the runner inspects. Counts are atomic so the same tracker can
/// be consulted from any thread, matching `AtomicUsize` accounting.
#[derive(Debug, Default)]
pub struct TrackerState {
    /// Sum of `layout.size()` over every live (allocated-but-not-freed) block.
    outstanding_bytes: AtomicUsize,
    /// Number of live (allocated-but-not-freed) blocks.
    outstanding_allocs: AtomicUsize,
    /// Total allocations ever served (monotonic; for diagnostics).
    total_allocs: AtomicUsize,
    /// Total deallocations ever served (monotonic; for diagnostics).
    total_frees: AtomicUsize,
}

impl TrackerState {
    /// A fresh, zeroed tracker.
    pub const fn new() -> TrackerState {
        TrackerState {
            outstanding_bytes: AtomicUsize::new(0),
            outstanding_allocs: AtomicUsize::new(0),
            total_allocs: AtomicUsize::new(0),
            total_frees: AtomicUsize::new(0),
        }
    }

    /// Record an allocation of `layout` (one block, `layout.size()` bytes).
    #[inline]
    pub fn on_alloc(&self, layout: Layout) {
        self.outstanding_bytes
            .fetch_add(layout.size(), Ordering::Relaxed);
        self.outstanding_allocs.fetch_add(1, Ordering::Relaxed);
        self.total_allocs.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a deallocation of `layout` — must be the SAME layout the matching
    /// [`on_alloc`](Self::on_alloc) recorded (faithful: a free decrements by the
    /// exact block it allocated).
    #[inline]
    pub fn on_free(&self, layout: Layout) {
        self.outstanding_bytes
            .fetch_sub(layout.size(), Ordering::Relaxed);
        self.outstanding_allocs.fetch_sub(1, Ordering::Relaxed);
        self.total_frees.fetch_add(1, Ordering::Relaxed);
    }

    /// Bytes that have been allocated but not yet freed.
    #[inline]
    pub fn outstanding_bytes(&self) -> usize {
        self.outstanding_bytes.load(Ordering::Relaxed)
    }

    /// Blocks that have been allocated but not yet freed.
    #[inline]
    pub fn outstanding_allocs(&self) -> usize {
        self.outstanding_allocs.load(Ordering::Relaxed)
    }

    /// Total allocations ever served (for diagnostics).
    #[inline]
    pub fn total_allocs(&self) -> usize {
        self.total_allocs.load(Ordering::Relaxed)
    }

    /// Total deallocations ever served (for diagnostics).
    #[inline]
    pub fn total_frees(&self) -> usize {
        self.total_frees.load(Ordering::Relaxed)
    }

    /// Reset every counter to zero. Use to scope a tracker to one test phase.
    pub fn reset(&self) {
        self.outstanding_bytes.store(0, Ordering::Relaxed);
        self.outstanding_allocs.store(0, Ordering::Relaxed);
        self.total_allocs.store(0, Ordering::Relaxed);
        self.total_frees.store(0, Ordering::Relaxed);
    }

    /// Panic with a clear message if ANY allocation is still outstanding — the
    /// faithful equivalent of `std.testing.allocator` failing a test on leak.
    #[track_caller]
    pub fn assert_no_leak(&self) {
        let bytes = self.outstanding_bytes();
        let allocs = self.outstanding_allocs();
        assert!(
            bytes == 0 && allocs == 0,
            "memory leak detected: {allocs} allocation(s) / {bytes} byte(s) still \
             outstanding (served {} alloc, {} free)",
            self.total_allocs(),
            self.total_frees(),
        );
    }
}

/// A faithful counting wrapper around a child [`Allocator`] `A`.
///
/// Delegates every operation verbatim to the child and, on success, updates the
/// shared [`TrackerState`] by the EXACT layout involved. It is itself an
/// `Allocator`, so it can be dropped in anywhere a child allocator is accepted
/// (e.g. as the child of an [`ArenaAllocator`](crate::arena_allocator) or a
/// [`MemoryPool`](crate::memory_pool)).
///
/// The tracker may be OWNED (each wrapper has its own [`TrackerState`]) via
/// [`new`](Self::new), or SHARED via a borrowed `&'a TrackerState`
/// ([`with_state`](Self::with_state)) so the live counts survive the wrapper —
/// the shape needed to install one over a process-wide allocator.
pub struct TrackingAllocator<'a, A: Allocator> {
    child: A,
    state: TrackerRef<'a>,
}

/// Either an owned tracker (boxed so the `&TrackerState` it hands out is stable
/// for the wrapper's lifetime) or a borrowed shared one.
enum TrackerRef<'a> {
    Owned(std::boxed::Box<TrackerState>),
    Borrowed(&'a TrackerState),
}

impl TrackerRef<'_> {
    #[inline]
    fn get(&self) -> &TrackerState {
        match self {
            TrackerRef::Owned(b) => b,
            TrackerRef::Borrowed(s) => s,
        }
    }
}

impl<A: Allocator> TrackingAllocator<'static, A> {
    /// Wrap `child` with a fresh, owned tracker.
    pub fn new(child: A) -> TrackingAllocator<'static, A> {
        TrackingAllocator {
            child,
            state: TrackerRef::Owned(std::boxed::Box::new(TrackerState::new())),
        }
    }
}

impl<'a, A: Allocator> TrackingAllocator<'a, A> {
    /// Wrap `child`, recording into the SHARED `state`. The state outlives the
    /// wrapper, so counts persist after the wrapper is dropped — used to assert
    /// "everything this wrapper allocated was freed" even after the pool that
    /// owned the wrapper is gone.
    pub fn with_state(child: A, state: &'a TrackerState) -> TrackingAllocator<'a, A> {
        TrackingAllocator {
            child,
            state: TrackerRef::Borrowed(state),
        }
    }

    /// Borrow the shared counter state.
    #[inline]
    pub fn state(&self) -> &TrackerState {
        self.state.get()
    }

    /// Bytes allocated but not yet freed through this wrapper's tracker.
    #[inline]
    pub fn outstanding_bytes(&self) -> usize {
        self.state.get().outstanding_bytes()
    }

    /// Blocks allocated but not yet freed through this wrapper's tracker.
    #[inline]
    pub fn outstanding_allocs(&self) -> usize {
        self.state.get().outstanding_allocs()
    }

    /// Panic if anything is still outstanding (see [`TrackerState::assert_no_leak`]).
    #[track_caller]
    pub fn assert_no_leak(&self) {
        self.state.get().assert_no_leak();
    }
}

// SAFETY: every method delegates to the child `Allocator` (which upholds the
// `Allocator` contract) and only additionally reads/writes atomic counters. The
// returned blocks, their validity, alignment and lifetime are exactly the
// child's; tracking is observation-only and does not alter memory behavior.
unsafe impl<A: Allocator> Allocator for TrackingAllocator<'_, A> {
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        let p = self.child.allocate(layout)?;
        self.state.get().on_alloc(layout);
        Ok(p)
    }

    fn allocate_zeroed(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        let p = self.child.allocate_zeroed(layout)?;
        self.state.get().on_alloc(layout);
        Ok(p)
    }

    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        // SAFETY: forwarded verbatim; the caller upholds `deallocate`'s contract
        // (ptr came from this allocator with `layout`). We decrement by the SAME
        // layout the matching `allocate` incremented.
        unsafe { self.child.deallocate(ptr, layout) };
        self.state.get().on_free(layout);
    }

    unsafe fn grow(
        &self,
        ptr: NonNull<u8>,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<NonNull<[u8]>, AllocError> {
        // SAFETY: forwarded verbatim under the same preconditions. A grow frees
        // the old block and allocates the new one; mirror that on the counters.
        let p = unsafe { self.child.grow(ptr, old_layout, new_layout)? };
        let s = self.state.get();
        s.on_free(old_layout);
        s.on_alloc(new_layout);
        Ok(p)
    }

    unsafe fn grow_zeroed(
        &self,
        ptr: NonNull<u8>,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<NonNull<[u8]>, AllocError> {
        // SAFETY: forwarded verbatim under the same preconditions.
        let p = unsafe { self.child.grow_zeroed(ptr, old_layout, new_layout)? };
        let s = self.state.get();
        s.on_free(old_layout);
        s.on_alloc(new_layout);
        Ok(p)
    }

    unsafe fn shrink(
        &self,
        ptr: NonNull<u8>,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<NonNull<[u8]>, AllocError> {
        // SAFETY: forwarded verbatim under the same preconditions.
        let p = unsafe { self.child.shrink(ptr, old_layout, new_layout)? };
        let s = self.state.get();
        s.on_free(old_layout);
        s.on_alloc(new_layout);
        Ok(p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use allocator_api2::alloc::Global;

    /// A single alloc/free round-trips the counters to zero.
    #[test]
    fn alloc_then_free_is_balanced() {
        let t = TrackingAllocator::new(Global);
        let layout = Layout::from_size_align(64, 8).unwrap();
        let p = t.allocate(layout).unwrap();
        assert_eq!(t.outstanding_allocs(), 1);
        assert_eq!(t.outstanding_bytes(), 64);
        unsafe { t.deallocate(p.cast::<u8>(), layout) };
        assert_eq!(t.outstanding_allocs(), 0);
        assert_eq!(t.outstanding_bytes(), 0);
        t.assert_no_leak();
    }

    /// Multiple distinct blocks accumulate and drain exactly.
    #[test]
    fn many_blocks_balance() {
        let t = TrackingAllocator::new(Global);
        let mut live = std::vec::Vec::new();
        let mut expect_bytes = 0usize;
        for i in 1..=16usize {
            let layout = Layout::from_size_align(i * 8, 8).unwrap();
            let p = t.allocate(layout).unwrap();
            expect_bytes += layout.size();
            live.push((p, layout));
        }
        assert_eq!(t.outstanding_allocs(), 16);
        assert_eq!(t.outstanding_bytes(), expect_bytes);
        for (p, layout) in live {
            unsafe { t.deallocate(p.cast::<u8>(), layout) };
        }
        t.assert_no_leak();
    }

    /// A leaked block makes `assert_no_leak` panic. We catch the panic so we can
    /// still free the block afterwards (otherwise Miri/ASan would flag the
    /// deliberate leak at process exit), then re-assert that the panic happened.
    #[test]
    fn leak_is_detected() {
        let t = TrackingAllocator::new(Global);
        let layout = Layout::from_size_align(128, 16).unwrap();
        let p = t.allocate(layout).unwrap();
        // Intentionally outstanding: `assert_no_leak` must panic.
        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            t.assert_no_leak();
        }))
        .is_err();
        // Clean up the deliberate leak BEFORE asserting, so a failure path also
        // frees it and neither Miri nor ASan reports a leak at exit.
        unsafe { t.deallocate(p.cast::<u8>(), layout) };
        assert!(
            panicked,
            "assert_no_leak must panic when a block is outstanding"
        );
        t.assert_no_leak();
    }

    /// `grow`/`shrink` keep the counters consistent (old freed, new allocated).
    #[test]
    fn grow_and_shrink_track() {
        let t = TrackingAllocator::new(Global);
        let small = Layout::from_size_align(32, 8).unwrap();
        let big = Layout::from_size_align(256, 8).unwrap();
        let p = t.allocate(small).unwrap();
        assert_eq!(t.outstanding_bytes(), 32);
        let p = unsafe { t.grow(p.cast::<u8>(), small, big).unwrap() };
        assert_eq!(t.outstanding_allocs(), 1);
        assert_eq!(t.outstanding_bytes(), 256);
        let p = unsafe { t.shrink(p.cast::<u8>(), big, small).unwrap() };
        assert_eq!(t.outstanding_bytes(), 32);
        unsafe { t.deallocate(p.cast::<u8>(), small) };
        t.assert_no_leak();
    }

    /// A shared `TrackerState` survives the wrapper: counts persist after the
    /// `TrackingAllocator` that produced them is dropped — the install-globally
    /// shape used for the page pool.
    #[test]
    fn shared_state_outlives_wrapper() {
        let state = TrackerState::new();
        let layout = Layout::from_size_align(64, 8).unwrap();
        let p = {
            let t = TrackingAllocator::with_state(Global, &state);
            t.allocate(layout).unwrap()
        };
        // Wrapper dropped; the shared state still reports the live block.
        assert_eq!(state.outstanding_allocs(), 1);
        assert_eq!(state.outstanding_bytes(), 64);
        unsafe { Global.deallocate(p.cast::<u8>(), layout) };
        // We freed through the child directly, so decrement the tracker by hand
        // to model the matching free (the wrapper is gone).
        state.on_free(layout);
        state.assert_no_leak();
    }
}
