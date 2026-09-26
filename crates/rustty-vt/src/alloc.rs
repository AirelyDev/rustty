//! The runtime allocator handle: the port's `std.mem.Allocator`.
//!
//! PORT STATUS: Rust-only glue (R1 "no-loss, not no-addition"), parity tail
//! S7-1. Design: `.port/parity-closure/S7-ALLOCATOR-DESIGN.md` option (B).
//!
//! In the reference, `std.mem.Allocator` is a `{ptr, vtable}` RUNTIME value. It
//! is passed by value, stored in objects (`Screen.alloc`, `MemoryPool.alloc`,
//! the C wrappers' `.alloc`) and chosen at run time: libghostty-vt's
//! `lib.alloc.default(c_alloc_)` (`src/lib/allocator.zig:22-44`) returns the
//! caller's allocator for a non-NULL `GhosttyAllocator*` and the process heap
//! for NULL. [`VtAllocator`] keeps it a runtime value:
//!
//! - `None` is the Rust global heap. It is the fast path with no vtable; NULL
//!   and the application use it. It is the port's `std.heap.c_allocator`
//!   (already the mapping of `rustty_lib::allocator::default`).
//! - `Some(p)` is the caller's own `RusttyAllocator` struct. It is exactly
//!   Zig's `std.mem.Allocator.ptr` for a C allocator (`CAllocator.zig()`,
//!   `src/lib/allocator.zig:78-83`); Zig's `.vtable` word is the constant
//!   `&zig_vtable`, so it is elided. Every call re-reads `ctx`/`vtable`
//!   through the caller's pointer, as the reference does, so **the caller's
//!   `RusttyAllocator` must outlive every object built from it** (N260).
//!
//! The handle is `Copy` and one pointer wide. It implements allocator_api2's
//! [`Allocator`](ApiAllocator) (so allocator-aware containers can carry it)
//! and zigstd's [`ResizeInPlace`] (so a zigstd arena can ask its child to
//! resize, as Zig's `ArenaAllocator` does).
//!
//! The module exports `Allocator` and `Global` in BOTH the type and the value
//! namespace, like a unit struct, so every site that spelled
//! `use allocator_api2::alloc::Global as Allocator;` and used `Allocator` (or
//! `Global`) both as a type and as a value keeps compiling after a one-line
//! import swap.
//!
//! Also here: [`CopyVecExt`] (the memcpy/fill specializations allocator_api2's
//! stable `Vec` lacks, N261), the fallible growth helpers, and, for tests,
//! [`testing`] (a port of `std.testing.FailingAllocator` and a counting
//! allocator, both built on the C `RusttyAllocator` ABI).

use allocator_api2::alloc::{AllocError, Allocator as ApiAllocator, Global as ApiGlobal};
use core::alloc::Layout;
use core::ptr::NonNull;
use rustty_lib::allocator::{Alignment, Allocator as CAllocator, VTable};
use rustty_zigstd::arena_allocator::ResizeInPlace;

/// `Allocator.Error` (`error{OutOfMemory}`), as the fallible helpers below
/// return it.
pub use rustty_lib::allocator::OutOfMemory;

/// The runtime allocator handle (`std.mem.Allocator`). See the module docs.
//
// PORT NOTE: `Eq` is pointer identity of the caller's struct, the same
// identity Zig's `Allocator.ptr` has. It backs `debug_assert_same`, which
// checks that an allocator-aware container is always grown and freed through
// the handle it was bound to (Zig's unmanaged containers take the allocator
// per call; allocator_api2 containers bind it at construction).
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct VtAllocator {
    c: Option<NonNull<CAllocator>>,
}

// One pointer wide: the `Option<NonNull>` null niche is the Global arm.
const _: () = assert!(core::mem::size_of::<VtAllocator>() == core::mem::size_of::<usize>());

// SAFETY: the Global arm is the (thread-safe) Rust global heap. For the C arm
// the contract is the reference's, implicit there and documented here: an
// embedder's allocator must be callable from whatever thread drives the
// object built with it. The handle is only ever dereferenced transiently,
// inside one allocator call, and never through `&mut`.
unsafe impl Send for VtAllocator {}
// SAFETY: as for `Send`; the handle itself is an immutable pointer value.
unsafe impl Sync for VtAllocator {}

impl core::fmt::Debug for VtAllocator {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.c {
            None => f.write_str("VtAllocator::Global"),
            Some(p) => write!(f, "VtAllocator::C({:p})", p.as_ptr()),
        }
    }
}

impl VtAllocator {
    /// The Rust global heap (the fast path, no vtable).
    pub const GLOBAL: VtAllocator = VtAllocator { c: None };

    /// A handle over a caller's C allocator (`CAllocator.zig()`).
    ///
    /// # Safety
    /// `c` must point to a valid `RusttyAllocator` whose vtable functions
    /// honour the Zig allocator contract, and it must stay valid (and at the
    /// same address) until the last allocation made through this handle has
    /// been freed.
    #[inline]
    pub unsafe fn from_c(c: NonNull<CAllocator>) -> VtAllocator {
        VtAllocator { c: Some(c) }
    }

    /// Whether this is the Rust global heap arm.
    #[inline]
    pub fn is_global(self) -> bool {
        self.c.is_none()
    }

    /// The caller's C allocator, for the C arm.
    #[inline]
    pub fn c_allocator(self) -> Option<NonNull<CAllocator>> {
        self.c
    }

    /// Debug-assert that `other` is this same allocator. Used where an
    /// allocator-aware container, bound to one handle, is passed another at a
    /// later reference call site that takes the allocator per call.
    #[inline]
    #[track_caller]
    pub fn debug_assert_same(self, other: VtAllocator) {
        debug_assert!(
            self == other,
            "allocator mismatch: {self:?} used with a container bound to {other:?}"
        );
    }
}

/// `lib.alloc.default(c_alloc_)`: the caller's allocator for a non-NULL
/// pointer, else the process default.
///
/// PORT NOTE: the reference's NULL default is `std.heap.c_allocator` when libc
/// is linked; the port maps that to the Rust global heap, as
/// `rustty_lib::allocator::default` does. On a native freestanding target
/// (`target_os = "none"`, not wasm) the reference returns
/// `std.mem.Allocator.failing` (`a5423592c`), mapped to [`failing`].
///
/// # Safety
/// A non-NULL `c_alloc_` must satisfy [`VtAllocator::from_c`]'s contract.
#[inline]
pub unsafe fn default(c_alloc_: Option<&CAllocator>) -> VtAllocator {
    // If we're given an allocator, use it.
    if let Some(c_alloc) = c_alloc_ {
        // SAFETY: forwarded to the caller's contract.
        return unsafe { VtAllocator::from_c(NonNull::from(c_alloc)) };
    }
    #[cfg(all(target_os = "none", not(target_arch = "wasm32")))]
    {
        failing()
    }
    #[cfg(not(all(target_os = "none", not(target_arch = "wasm32"))))]
    {
        VtAllocator::GLOBAL
    }
}

/// `std.mem.Allocator.failing`: every `alloc`/`remap` returns null, `resize`
/// returns false and `free` is a no-op.
pub fn failing() -> VtAllocator {
    // SAFETY: a process static with a total, stateless vtable.
    unsafe { VtAllocator::from_c(NonNull::from(&FAILING.0)) }
}

/// A `RusttyAllocator` in a static: its `ctx` is never dereferenced by the
/// failing vtable, so sharing it across threads is sound.
struct SyncCAllocator(CAllocator);
// SAFETY: see the type's doc comment.
unsafe impl Sync for SyncCAllocator {}

static FAILING: SyncCAllocator = SyncCAllocator(CAllocator {
    ctx: core::ptr::null_mut(),
    vtable: &FAILING_VTABLE,
});

static FAILING_VTABLE: VTable = VTable {
    alloc: failing_alloc,
    resize: failing_resize,
    remap: failing_remap,
    free: failing_free,
};

extern "C" fn failing_alloc(_: *mut core::ffi::c_void, _: usize, _: u8, _: usize) -> *mut u8 {
    core::ptr::null_mut()
}

extern "C" fn failing_resize(
    _: *mut core::ffi::c_void,
    _: *mut u8,
    _: usize,
    _: u8,
    _: usize,
    _: usize,
) -> bool {
    false
}

extern "C" fn failing_remap(
    _: *mut core::ffi::c_void,
    _: *mut u8,
    _: usize,
    _: u8,
    _: usize,
    _: usize,
) -> *mut u8 {
    core::ptr::null_mut()
}

extern "C" fn failing_free(_: *mut core::ffi::c_void, _: *mut u8, _: usize, _: u8, _: usize) {}

/// The inline buffer of a [`StackFallback`], 16-byte aligned so what fits
/// does not depend on where the buffer lands (a request of up to 16-byte
/// alignment starts at offset 0). Rust-only (parity tail S7-4b): Zig's
/// `stackFallback` buffer is a plain `[size]u8`.
#[repr(C, align(16))]
struct StackFallbackBuf<const N: usize>([core::mem::MaybeUninit<u8>; N]);

/// `std.heap.stackFallback(N, fallback)`: an allocator that serves requests
/// from an inline `N`-byte buffer while they fit (bumping through it, as Zig's
/// `FixedBufferAllocator` does) and from `fallback` otherwise. Freeing the
/// most recent in-buffer allocation returns its bytes to the buffer; other
/// in-buffer frees are no-ops, and fallback allocations are freed through
/// `fallback`. Use it through `&StackFallback` (allocator_api2's `Allocator`).
///
/// Rust-only glue for the reference's `var sfa = std.heap.stackFallback(n,
/// alloc); const a = sfa.get();` (parity tail S7-4): a request that fits the
/// buffer reaches no allocator, one that does not reaches `fallback`.
pub struct StackFallback<const N: usize> {
    buf: core::cell::UnsafeCell<StackFallbackBuf<N>>,
    end: core::cell::Cell<usize>,
    fallback: VtAllocator,
}

impl<const N: usize> StackFallback<N> {
    /// `std.heap.stackFallback(N, fallback)` (`.get()` starts empty).
    pub fn new(fallback: VtAllocator) -> StackFallback<N> {
        StackFallback {
            buf: core::cell::UnsafeCell::new(StackFallbackBuf(
                [core::mem::MaybeUninit::uninit(); N],
            )),
            end: core::cell::Cell::new(0),
            fallback,
        }
    }

    fn base(&self) -> *mut u8 {
        self.buf.get() as *mut u8
    }

    fn owns(&self, ptr: NonNull<u8>) -> bool {
        let base = self.base() as usize;
        let p = ptr.as_ptr() as usize;
        p >= base && p < base + N
    }
}

// SAFETY: in-buffer blocks are disjoint ranges of `buf` handed out by bumping
// `end` (never overlapping a live block), valid while `self` lives, which the
// `&StackFallback` borrow guarantees; fallback blocks follow the handle's
// contract. Frees of in-buffer blocks only move `end` back when the block is
// the last one handed out.
unsafe impl<const N: usize> ApiAllocator for &StackFallback<N> {
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        if layout.size() == 0 {
            return Ok(dangling(layout));
        }
        let base = self.base() as usize;
        let start = (base + self.end.get()).next_multiple_of(layout.align()) - base;
        if let Some(new_end) = start.checked_add(layout.size()) {
            if new_end <= N {
                self.end.set(new_end);
                // SAFETY: `start..new_end` lies inside `buf` and is not part of
                // any live block (everything live is below the old `end`).
                let p = unsafe { NonNull::new_unchecked(self.base().add(start)) };
                return Ok(NonNull::slice_from_raw_parts(p, layout.size()));
            }
        }
        self.fallback.allocate(layout)
    }

    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        if layout.size() == 0 {
            return;
        }
        if self.owns(ptr) {
            let offset = ptr.as_ptr() as usize - self.base() as usize;
            if offset + layout.size() == self.end.get() {
                self.end.set(offset);
            }
            return;
        }
        // SAFETY: a fallback block (the caller's contract).
        unsafe { self.fallback.deallocate(ptr, layout) };
    }
}

/// `std.heap.page_allocator` as a handle: every allocation is its own
/// page-granular mapping (`rustty_os::page_allocator`), `resize` succeeds
/// only for the same length and `remap` is refused (Zig's `PageAllocator`
/// may do either in place; refusing is within the allocator contract, and
/// callers fall back to alloc + copy + free).
///
/// PORT NOTE (parity tail S7-4): the handle's C arm over a process static
/// vtable, like [`failing`]. Used where the reference names
/// `std.heap.page_allocator` directly (the inspector's decode buffers). On
/// Darwin the port's page allocator carries the PageList's VM tag, where the
/// reference's generic `std.heap.page_allocator` is untagged.
pub fn page_allocator() -> VtAllocator {
    // SAFETY: a process static whose vtable forwards to the stateless page
    // allocator.
    unsafe { VtAllocator::from_c(NonNull::from(&PAGE.0)) }
}

static PAGE: SyncCAllocator = SyncCAllocator(CAllocator {
    ctx: core::ptr::null_mut(),
    vtable: &PAGE_VTABLE,
});

static PAGE_VTABLE: VTable = VTable {
    alloc: page_alloc,
    resize: page_resize,
    remap: failing_remap,
    free: page_free,
};

fn page_layout(len: usize, alignment: u8) -> Option<Layout> {
    Layout::from_size_align(len, 1usize << alignment).ok()
}

extern "C" fn page_alloc(
    _: *mut core::ffi::c_void,
    len: usize,
    alignment: u8,
    _: usize,
) -> *mut u8 {
    let Some(layout) = page_layout(len, alignment) else {
        return core::ptr::null_mut();
    };
    match ApiAllocator::allocate(&rustty_os::page_allocator::page_allocator(), layout) {
        Ok(p) => p.as_ptr() as *mut u8,
        Err(_) => core::ptr::null_mut(),
    }
}

extern "C" fn page_resize(
    _: *mut core::ffi::c_void,
    _: *mut u8,
    len: usize,
    _: u8,
    new_len: usize,
    _: usize,
) -> bool {
    new_len == len
}

extern "C" fn page_free(
    _: *mut core::ffi::c_void,
    memory: *mut u8,
    len: usize,
    alignment: u8,
    _: usize,
) {
    let (Some(layout), Some(ptr)) = (page_layout(len, alignment), NonNull::new(memory)) else {
        return;
    };
    // SAFETY: `memory` is a live allocation of `layout` from `page_alloc`
    // (the vtable contract: frees get the allocation's own length and
    // alignment).
    unsafe { ApiAllocator::deallocate(&rustty_os::page_allocator::page_allocator(), ptr, layout) };
}

// The type AND value spellings (see the module docs). A const and a type alias
// of the same name live in different namespaces, exactly as a unit struct's
// name does.
#[allow(non_upper_case_globals)]
/// The Rust global heap handle, as a value (`Allocator` used as a unit value).
pub const Allocator: VtAllocator = VtAllocator::GLOBAL;
/// `std.mem.Allocator`.
pub type Allocator = VtAllocator;
#[allow(non_upper_case_globals)]
/// The Rust global heap handle, as a value.
pub const Global: VtAllocator = VtAllocator::GLOBAL;
/// The Rust global heap handle's type (the same runtime handle).
pub type Global = VtAllocator;

impl From<ApiGlobal> for VtAllocator {
    #[inline]
    fn from(_: ApiGlobal) -> VtAllocator {
        VtAllocator::GLOBAL
    }
}

/// A vector whose storage is allocated (and freed) through a [`VtAllocator`]
/// bound at construction: the port's `std.ArrayListUnmanaged(T)` where the
/// reference always passes the same allocator, and its owned slices (`[]T`
/// from `alloc.alloc`/`dupe`/`toOwnedSlice`).
pub type AVec<T> = allocator_api2::vec::Vec<T, VtAllocator>;

/// A single object allocated through a [`VtAllocator`] (`alloc.create(T)`);
/// dropping it is `alloc.destroy`.
pub type ABox<T> = allocator_api2::boxed::Box<T, VtAllocator>;

/// `std.AutoArrayHashMapUnmanaged(K, V)` over a [`VtAllocator`] bound at
/// construction (zigstd's port, S7-3).
pub type AArrayHashMap<K, V> =
    rustty_zigstd::array_hash_map::AutoArrayHashMapUnmanaged<K, V, VtAllocator>;

/// Owned bytes allocated through a [`VtAllocator`] (`[]u8` from
/// `alloc.alloc`/`alloc.dupe`/`toOwnedSlice`).
pub type Bytes = AVec<u8>;

/// `try alloc.dupe(u8, src)`: one exact-sized, fallible allocation through
/// `alloc`. An empty `src` allocates nothing (Zig's `alloc(u8, 0)` returns a
/// zero-length slice without calling the vtable).
pub fn try_dupe(alloc: VtAllocator, src: &[u8]) -> Result<Bytes, OutOfMemory> {
    let mut v = Bytes::new_in(alloc);
    v.try_reserve_exact(src.len()).map_err(|_| OutOfMemory)?;
    v.extend_copy(src);
    Ok(v)
}

/// Bind an unmanaged list to the allocator a reference call site passes.
///
/// Zig's `.empty` unmanaged containers hold no allocator: every growing call
/// passes one, and the contract is that it is always the same. An
/// allocator_api2 vector binds its allocator at construction, so a list that
/// starts `.empty` (a `Default` field) is rebound to `alloc` while it owns no
/// allocation, and afterwards `alloc` is debug-asserted to be the allocator
/// it was bound to (the design's "bind on first growth", S-7 §4 (B) risk 3).
#[inline]
pub fn bind<T>(v: &mut AVec<T>, alloc: VtAllocator) {
    if v.capacity() == 0 {
        if *v.allocator() != alloc {
            *v = AVec::new_in(alloc);
        }
    } else {
        v.allocator().debug_assert_same(alloc);
    }
}

/// A copy of `src` on the Rust heap (the Global arm), aborting on allocation
/// failure like `<[u8]>::to_vec`. Rust-only convenience for code that builds
/// allocator-aware byte fields outside any reference allocation site (tests,
/// the application's Global-arm fixtures).
pub fn heap_bytes(src: &[u8]) -> Bytes {
    let mut v = Bytes::new_in(VtAllocator::GLOBAL);
    v.extend_copy(src);
    v
}

/// A box on the Rust heap (the Global arm), aborting on allocation failure
/// like `Box::new`; the [`heap_bytes`] counterpart for single objects.
pub fn heap_box<T>(value: T) -> ABox<T> {
    ABox::new_in(value, VtAllocator::GLOBAL)
}

/// `try alloc.create(T)` + initialization: one fallible allocation through
/// `alloc`. On failure `value` is dropped.
pub fn try_create<T>(alloc: VtAllocator, value: T) -> Result<ABox<T>, OutOfMemory> {
    ABox::try_new_in(value, alloc).map_err(|_| OutOfMemory)
}

// SAFETY: the Global arm forwards verbatim to allocator_api2's `Global`. The C
// arm forwards to the caller's vtable, which returns memory of at least `len`
// bytes at the requested alignment, or null, and frees exactly what it handed
// out (the Zig allocator contract); zero-sized requests never reach it (Zig's
// typed helpers skip them too), and `grow`/`shrink` are Zig's `realloc`.
//
// PERF(port): `allocate`/`deallocate` are `#[inline]` and their C arm is out of
// line, so the inlined body is a null test plus the `Global` call. With both out
// of line, the OSC capture's fenced config was about 7% slower (T-A3 report,
// the `osc::CaptureAllocator` this handle replaces). `allocate_zeroed`, `grow`,
// `grow_zeroed` and `shrink` are `#[inline(never)]`: growth is amortized, and
// with them `#[inline]` the extra code inlined through the OSC capture into the
// vt `Parser::next` callers cost `terminal-parser/utf8` +17% (paired CPU-time
// median, N=11-21, load 13-31, S7-12); out of line it measured -5.5% against
// the pre-S7 base in the same run, i.e. at or below it.
unsafe impl ApiAllocator for VtAllocator {
    #[inline]
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        match self.c {
            None => ApiGlobal.allocate(layout),
            Some(c) => c_allocate(c, layout),
        }
    }

    #[inline(never)]
    fn allocate_zeroed(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        match self.c {
            None => ApiGlobal.allocate_zeroed(layout),
            Some(c) => c_allocate_zeroed(c, layout),
        }
    }

    #[inline]
    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        match self.c {
            // SAFETY: forwarded under the caller's contract.
            None => unsafe { ApiGlobal.deallocate(ptr, layout) },
            Some(c) => c_deallocate(c, ptr, layout),
        }
    }

    #[inline(never)]
    unsafe fn grow(
        &self,
        ptr: NonNull<u8>,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<NonNull<[u8]>, AllocError> {
        match self.c {
            // SAFETY: forwarded under the caller's contract.
            None => unsafe { ApiGlobal.grow(ptr, old_layout, new_layout) },
            // SAFETY: forwarded under the caller's contract.
            Some(c) => unsafe { c_realloc(c, ptr, old_layout, new_layout) },
        }
    }

    #[inline(never)]
    unsafe fn grow_zeroed(
        &self,
        ptr: NonNull<u8>,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<NonNull<[u8]>, AllocError> {
        match self.c {
            // SAFETY: forwarded under the caller's contract.
            None => unsafe { ApiGlobal.grow_zeroed(ptr, old_layout, new_layout) },
            // SAFETY: forwarded under the caller's contract.
            Some(c) => unsafe { c_realloc_zeroed(c, ptr, old_layout, new_layout) },
        }
    }

    #[inline(never)]
    unsafe fn shrink(
        &self,
        ptr: NonNull<u8>,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<NonNull<[u8]>, AllocError> {
        match self.c {
            // SAFETY: forwarded under the caller's contract.
            None => unsafe { ApiGlobal.shrink(ptr, old_layout, new_layout) },
            // SAFETY: forwarded under the caller's contract.
            Some(c) => unsafe { c_realloc(c, ptr, old_layout, new_layout) },
        }
    }
}

/// The handle behind the raw `std.mem.Allocator` vtable interface of
/// rustty-datastruct (`rawAlloc`/`rawResize`/`rawRemap`/`rawFree`), so a
/// `LimitedAllocator` can wrap it as its child: the kitty PNG decode seam's
/// `LimitedAllocator.init(alloc, max_size)` (parity tail S7-4).
///
/// The C arm forwards each raw call to the caller's vtable unchanged (Zig's
/// `Allocator.rawAlloc` etc. do exactly that). The Global arm is
/// `sys::GlobalRawAllocator` (the Rust heap, the same heap the handle's Global
/// arm uses).
impl rustty_datastruct::limited_allocator::RawAllocator for VtAllocator {
    fn raw_alloc(
        &self,
        len: usize,
        alignment: rustty_datastruct::limited_allocator::Alignment,
        ret_addr: usize,
    ) -> *mut u8 {
        match self.c {
            None => crate::sys::GlobalRawAllocator.raw_alloc(len, alignment, ret_addr),
            Some(c) => {
                // SAFETY: `from_c`'s contract keeps the struct alive and its
                // vtable valid; read by value.
                let CAllocator { ctx, vtable } = unsafe { c.read() };
                // SAFETY: the vtable contract.
                unsafe { ((*vtable).alloc)(ctx, len, alignment.0, ret_addr) }
            }
        }
    }

    fn raw_resize(
        &self,
        memory: rustty_datastruct::limited_allocator::RawSlice,
        alignment: rustty_datastruct::limited_allocator::Alignment,
        new_len: usize,
        ret_addr: usize,
    ) -> bool {
        match self.c {
            None => crate::sys::GlobalRawAllocator.raw_resize(memory, alignment, new_len, ret_addr),
            Some(c) => {
                // SAFETY: as in `raw_alloc`.
                let CAllocator { ctx, vtable } = unsafe { c.read() };
                // SAFETY: the vtable contract; `memory` came from this allocator.
                unsafe {
                    ((*vtable).resize)(ctx, memory.ptr, memory.len, alignment.0, new_len, ret_addr)
                }
            }
        }
    }

    fn raw_remap(
        &self,
        memory: rustty_datastruct::limited_allocator::RawSlice,
        alignment: rustty_datastruct::limited_allocator::Alignment,
        new_len: usize,
        ret_addr: usize,
    ) -> *mut u8 {
        match self.c {
            None => crate::sys::GlobalRawAllocator.raw_remap(memory, alignment, new_len, ret_addr),
            Some(c) => {
                // SAFETY: as in `raw_alloc`.
                let CAllocator { ctx, vtable } = unsafe { c.read() };
                // SAFETY: the vtable contract; `memory` came from this allocator.
                unsafe {
                    ((*vtable).remap)(ctx, memory.ptr, memory.len, alignment.0, new_len, ret_addr)
                }
            }
        }
    }

    fn raw_free(
        &self,
        memory: rustty_datastruct::limited_allocator::RawSlice,
        alignment: rustty_datastruct::limited_allocator::Alignment,
        ret_addr: usize,
    ) {
        match self.c {
            None => crate::sys::GlobalRawAllocator.raw_free(memory, alignment, ret_addr),
            Some(c) => {
                // SAFETY: as in `raw_alloc`.
                let CAllocator { ctx, vtable } = unsafe { c.read() };
                // SAFETY: the vtable contract; `memory` came from this allocator.
                unsafe { ((*vtable).free)(ctx, memory.ptr, memory.len, alignment.0, ret_addr) }
            }
        }
    }
}

/// `@intFromEnum(Alignment.fromByteUnits(n))`.
#[inline]
fn align_log2(layout: Layout) -> u8 {
    Alignment::from_byte_units(layout.align()).0
}

/// The zero-length result of Zig's typed helpers: a dangling, aligned pointer
/// that is never passed to the raw vtable.
#[inline]
fn dangling(layout: Layout) -> NonNull<[u8]> {
    let p = core::ptr::without_provenance_mut::<u8>(layout.align());
    // SAFETY: an alignment is never zero.
    let p = unsafe { NonNull::new_unchecked(p) };
    NonNull::slice_from_raw_parts(p, 0)
}

/// `rawAlloc` (skipped for zero bytes, as `Allocator.alloc` skips it).
///
/// PORT NOTE (W36-B06): every C-arm call passes `ret_addr = 0` where Zig passes
/// `@returnAddress()` through `rawAlloc`/`rawResize`/`rawRemap`/`rawFree` to the
/// C vtable's `ret_addr`. Stable Rust has no return-address intrinsic, and the
/// argument is only a hint in the Zig allocator contract.
#[inline(never)]
fn c_allocate(c: NonNull<CAllocator>, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
    if layout.size() == 0 {
        return Ok(dangling(layout));
    }
    // SAFETY: `from_c`'s contract keeps the struct alive; read it by value
    // (the handle never holds a reference to it).
    let CAllocator { ctx, vtable } = unsafe { c.read() };
    // SAFETY: `from_c`'s contract: a valid vtable.
    let ptr = unsafe { ((*vtable).alloc)(ctx, layout.size(), align_log2(layout), 0) };
    match NonNull::new(ptr) {
        Some(p) => Ok(NonNull::slice_from_raw_parts(p, layout.size())),
        None => Err(AllocError),
    }
}

#[inline(never)]
fn c_allocate_zeroed(c: NonNull<CAllocator>, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
    let p = c_allocate(c, layout)?;
    // SAFETY: freshly allocated, `layout.size()` writable bytes.
    unsafe { core::ptr::write_bytes(p.as_ptr() as *mut u8, 0, layout.size()) };
    Ok(p)
}

/// `rawFree` (skipped for zero bytes, as `Allocator.free` skips it).
#[inline(never)]
fn c_deallocate(c: NonNull<CAllocator>, ptr: NonNull<u8>, layout: Layout) {
    if layout.size() == 0 {
        return;
    }
    // SAFETY: as in `c_allocate`.
    let CAllocator { ctx, vtable } = unsafe { c.read() };
    // SAFETY: the vtable contract; `ptr`/`layout` came from this allocator.
    unsafe { ((*vtable).free)(ctx, ptr.as_ptr(), layout.size(), align_log2(layout), 0) };
}

/// `Allocator.reallocAdvanced` (Zig 0.16 `std/mem/Allocator.zig:403-438`):
/// a zero-length old block is a fresh allocation, a zero-length new block is a
/// free, otherwise `rawRemap` is tried first and the fallback is
/// `rawAlloc` + copy + `rawFree`.
///
/// PORT NOTE: allocator_api2 lets `grow`/`shrink` change the alignment, which
/// Zig's `realloc` never does; `rawRemap` cannot express that, so a changed
/// alignment always takes the alloc + copy + free path.
///
/// # Safety
/// allocator_api2's `grow`/`shrink` contract: `ptr` is a live block of this
/// allocator with `old`.
#[inline(never)]
unsafe fn c_realloc(
    c: NonNull<CAllocator>,
    ptr: NonNull<u8>,
    old: Layout,
    new: Layout,
) -> Result<NonNull<[u8]>, AllocError> {
    // if (old_mem.len == 0) return self.allocAdvancedWithRetAddr(...);
    if old.size() == 0 {
        return c_allocate(c, new);
    }
    // if (new_n == 0) { self.free(old_mem); return <zero-length>; }
    if new.size() == 0 {
        c_deallocate(c, ptr, old);
        return Ok(dangling(new));
    }
    if old.align() == new.align() {
        // SAFETY: as in `c_allocate`.
        let CAllocator { ctx, vtable } = unsafe { c.read() };
        // SAFETY: the vtable contract; `ptr`/`old` came from this allocator.
        let p = unsafe {
            ((*vtable).remap)(
                ctx,
                ptr.as_ptr(),
                old.size(),
                align_log2(old),
                new.size(),
                0,
            )
        };
        if let Some(p) = NonNull::new(p) {
            return Ok(NonNull::slice_from_raw_parts(p, new.size()));
        }
    }
    let fresh = c_allocate(c, new)?;
    let n = old.size().min(new.size());
    // SAFETY: both blocks are live, at least `n` bytes, and distinct.
    unsafe { core::ptr::copy_nonoverlapping(ptr.as_ptr(), fresh.as_ptr() as *mut u8, n) };
    c_deallocate(c, ptr, old);
    Ok(fresh)
}

/// # Safety
/// As `c_realloc`, with `new.size() >= old.size()`.
#[inline(never)]
unsafe fn c_realloc_zeroed(
    c: NonNull<CAllocator>,
    ptr: NonNull<u8>,
    old: Layout,
    new: Layout,
) -> Result<NonNull<[u8]>, AllocError> {
    // SAFETY: forwarded.
    let p = unsafe { c_realloc(c, ptr, old, new)? };
    // SAFETY: the block is `new.size()` bytes; zero the grown tail.
    unsafe {
        core::ptr::write_bytes(
            (p.as_ptr() as *mut u8).add(old.size()),
            0,
            new.size() - old.size(),
        )
    };
    Ok(p)
}

// `rawResize`: the Global arm refuses (as `impl ResizeInPlace for Global`,
// the Rust heap cannot resize in place through the stable API); a C allocator
// is asked through its `resize` callback, as Zig's arena asks its child.
impl ResizeInPlace for VtAllocator {
    #[inline]
    unsafe fn resize_in_place(&self, ptr: NonNull<u8>, old_layout: Layout, new_len: usize) -> bool {
        match self.c {
            None => false,
            // SAFETY: forwarded under the caller's contract.
            Some(c) => unsafe { c_resize(c, ptr, old_layout, new_len) },
        }
    }
}

/// # Safety
/// `ResizeInPlace::resize_in_place`'s contract.
#[inline(never)]
unsafe fn c_resize(c: NonNull<CAllocator>, ptr: NonNull<u8>, old: Layout, new_len: usize) -> bool {
    // SAFETY: as in `c_allocate`.
    let CAllocator { ctx, vtable } = unsafe { c.read() };
    // SAFETY: the vtable contract; `ptr`/`old` came from this allocator.
    unsafe { ((*vtable).resize)(ctx, ptr.as_ptr(), old.size(), align_log2(old), new_len, 0) }
}

/// The std `Vec` specializations allocator_api2's stable `Vec` lacks (N261).
///
/// allocator_api2 0.2's `Vec::extend_from_slice` is `extend(iter.cloned())`, a
/// per-element, capacity-checked loop; for `Copy` elements std lowers it to one
/// `memcpy`. Its `resize` is a per-element `ptr::write` loop behind a
/// `SetLenOnDrop`; a plain slice fill vectorizes. Swapping RenderState's std
/// `Vec`s for allocator_api2's naively cost +81% on `screen-clone.render`; these
/// helpers remove all of it (S-7a). The slow methods are banned in the vt
/// crates by clippy `disallowed-methods` (`crates/rustty-vt/clippy.toml`, gate
/// G-VTALLOC). allocator_api2's `Extend` impls (`v.extend(slice)`,
/// `v.extend(slice.iter().copied())`) are the same per-element loop but cannot
/// be banned by path (a trait-impl method); use `extend_copy` for them too.
pub trait CopyVecExt<T: Copy> {
    /// `extend_from_slice` as one `memcpy`. Aborts on allocation failure,
    /// like the std method (use [`try_extend_copy`](Self::try_extend_copy) at a
    /// reference `try` site).
    fn extend_copy(&mut self, src: &[T]);
    /// `resize(new_len, value)` as a slice fill.
    fn resize_copy(&mut self, new_len: usize, value: T);
    /// `appendSlice`: [`extend_copy`](Self::extend_copy), failing with
    /// `error.OutOfMemory` instead of aborting.
    fn try_extend_copy(&mut self, src: &[T]) -> Result<(), OutOfMemory>;
    /// `resize` (`appendNTimes`), failing with `error.OutOfMemory` instead of
    /// aborting.
    fn try_resize_copy(&mut self, new_len: usize, value: T) -> Result<(), OutOfMemory>;
}

/// The append after capacity is ensured.
///
/// # Safety
/// `v` has room for `src.len()` more elements.
#[inline]
unsafe fn append_copy_unchecked<T: Copy, A: ApiAllocator>(
    v: &mut allocator_api2::vec::Vec<T, A>,
    src: &[T],
) {
    let len = v.len();
    // SAFETY: the caller reserved room for `src.len()` more elements, `src`
    // cannot alias the spare capacity, and `T: Copy` needs no drop.
    unsafe {
        core::ptr::copy_nonoverlapping(src.as_ptr(), v.as_mut_ptr().add(len), src.len());
        v.set_len(len + src.len());
    }
}

/// The fill after capacity is ensured (`new_len > v.len()`).
#[inline]
fn fill_copy<T: Copy, A: ApiAllocator>(
    v: &mut allocator_api2::vec::Vec<T, A>,
    new_len: usize,
    value: T,
) {
    let len = v.len();
    for slot in &mut v.spare_capacity_mut()[..new_len - len] {
        slot.write(value);
    }
    // SAFETY: the first `new_len - len` spare slots were just initialized.
    unsafe { v.set_len(new_len) };
}

impl<T: Copy, A: ApiAllocator> CopyVecExt<T> for allocator_api2::vec::Vec<T, A> {
    #[inline]
    fn extend_copy(&mut self, src: &[T]) {
        self.reserve(src.len());
        // SAFETY: `reserve` made room.
        unsafe { append_copy_unchecked(self, src) };
    }

    #[inline]
    fn resize_copy(&mut self, new_len: usize, value: T) {
        if new_len <= self.len() {
            self.truncate(new_len);
            return;
        }
        self.reserve(new_len - self.len());
        fill_copy(self, new_len, value);
    }

    #[inline]
    fn try_extend_copy(&mut self, src: &[T]) -> Result<(), OutOfMemory> {
        self.try_reserve(src.len()).map_err(|_| OutOfMemory)?;
        // SAFETY: `try_reserve` made room.
        unsafe { append_copy_unchecked(self, src) };
        Ok(())
    }

    #[inline]
    fn try_resize_copy(&mut self, new_len: usize, value: T) -> Result<(), OutOfMemory> {
        if new_len <= self.len() {
            self.truncate(new_len);
            return Ok(());
        }
        self.try_reserve(new_len - self.len())
            .map_err(|_| OutOfMemory)?;
        fill_copy(self, new_len, value);
        Ok(())
    }
}

/// Fallible single-element growth (`try list.append(alloc, x)`).
pub trait TryPushExt<T> {
    /// `append`, failing with `error.OutOfMemory` instead of aborting. On
    /// failure `value` is dropped and the vector is unchanged.
    fn try_push(&mut self, value: T) -> Result<(), OutOfMemory>;
}

impl<T, A: ApiAllocator> TryPushExt<T> for allocator_api2::vec::Vec<T, A> {
    #[inline]
    fn try_push(&mut self, value: T) -> Result<(), OutOfMemory> {
        if self.len() == self.capacity() {
            self.try_reserve(1).map_err(|_| OutOfMemory)?;
        }
        self.push(value);
        Ok(())
    }
}

/// `std.ArrayListUnmanaged(T)` for `Copy` elements: a list that does NOT store
/// its allocator; every growing call and `deinit` take it, as the reference's
/// do.
///
/// PORT NOTE (parity tail S7-3): allocator_api2 containers bind their allocator
/// at construction. Where the reference grows a list through an allocator it
/// looks up at call time (`Terminal.pwd`/`title` grow through `self.gpa()`,
/// the ACTIVE screen's allocator, and the reference's own C-API test swaps that
/// allocator to make the growth fail), binding would change which allocator is
/// called. This type keeps the reference's per-call allocator.
///
/// Growth is Zig 0.16's: `ensureTotalCapacity` rounds up with `growCapacity`
/// (`minimum +| (minimum / 2 + init_capacity)`, `init_capacity =
/// max(1, cache_line / @sizeOf(T))`) and `ensureTotalCapacityPrecise` tries
/// `remap` first, then allocates, copies and frees (the handle's `grow`).
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

/// `std.atomic.cache_line` for the targets the port builds.
const CACHE_LINE: usize = if cfg!(any(
    target_arch = "x86_64",
    target_arch = "aarch64",
    target_arch = "powerpc64"
)) {
    128
} else {
    64
};

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

    /// `items` as a slice (the `Vec::as_slice` spelling).
    #[inline]
    pub fn as_slice(&self) -> &[T] {
        self.items()
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

    fn layout(cap: usize) -> Option<Layout> {
        Layout::array::<T>(cap).ok()
    }

    /// `growCapacity(minimum)`.
    pub fn grow_capacity(minimum: usize) -> usize {
        let init_capacity = (CACHE_LINE / core::mem::size_of::<T>().max(1)).max(1);
        minimum.saturating_add(minimum / 2 + init_capacity)
    }

    /// `try list.ensureTotalCapacity(gpa, new_capacity)`.
    pub fn ensure_total_capacity(
        &mut self,
        gpa: VtAllocator,
        new_capacity: usize,
    ) -> Result<(), OutOfMemory> {
        if self.cap >= new_capacity {
            return Ok(());
        }
        self.ensure_total_capacity_precise(gpa, Self::grow_capacity(new_capacity))
    }

    /// `try list.ensureTotalCapacityPrecise(gpa, new_capacity)`.
    pub fn ensure_total_capacity_precise(
        &mut self,
        gpa: VtAllocator,
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
    pub fn deinit(&mut self, gpa: VtAllocator) {
        if self.cap != 0 {
            let layout = Self::layout(self.cap).expect("a live allocation has a valid layout");
            // SAFETY: the live allocation of `layout`, made through `gpa`.
            unsafe { gpa.deallocate(self.ptr.cast(), layout) };
        }
        *self = Self::EMPTY;
    }
}

/// Test allocators on the C `RusttyAllocator` ABI, so the handle's C arm (and
/// the C API) see them exactly as an embedder's allocator.
///
/// Compiled for this crate's tests and, through the `test-allocators` feature,
/// for dependents' tests (rustty-vt-c enables it as a dev-dependency). Never
/// in a production build.
///
/// SINGLE-THREAD ONLY: [`FailingAllocator`] and [`CountingAllocator`] keep
/// their state in `Cell`s behind the C `ctx` pointer, so they are not `Sync`.
/// A `VtAllocator` over one of them is nevertheless `Send + Sync` (the handle
/// cannot see what it points at), so a test must not move an object built over
/// a test allocator to another thread, or use it from two threads at once.
#[cfg(any(test, feature = "test-allocators"))]
pub mod testing {
    use super::VtAllocator;
    use core::cell::{Cell, Ref, RefCell};
    use core::ffi::c_void;
    use core::ptr::NonNull;
    use rustty_lib::allocator::{Allocator as CAllocator, VTable};
    use std::backtrace::Backtrace;

    /// A C allocator over the Rust global heap, the port's `std.testing.allocator`
    /// for the wrappers below (leaks are Miri's job, R6).
    ///
    /// PORT NOTE: like `rustty_lib::allocator::c_allocator()` (N270), this one
    /// resizes in place only to the same length: `std::alloc::dealloc` must be
    /// given the layout the block was allocated with, so a block cannot be
    /// recorded as shorter without moving it. `remap` is `realloc` (it may
    /// move).
    pub fn heap() -> CAllocator {
        CAllocator {
            ctx: core::ptr::null_mut(),
            vtable: &HEAP_VTABLE,
        }
    }

    static HEAP_VTABLE: VTable = VTable {
        alloc: heap_alloc,
        resize: heap_resize,
        remap: heap_remap,
        free: heap_free,
    };

    fn layout(len: usize, alignment: u8) -> Option<std::alloc::Layout> {
        std::alloc::Layout::from_size_align(len, 1usize.checked_shl(alignment.into())?).ok()
    }

    extern "C" fn heap_alloc(_: *mut c_void, len: usize, alignment: u8, _: usize) -> *mut u8 {
        match layout(len, alignment) {
            // SAFETY: the raw vtable is never called with `len == 0`.
            Some(l) if len != 0 => unsafe { std::alloc::alloc(l) },
            _ => core::ptr::null_mut(),
        }
    }

    extern "C" fn heap_resize(
        _: *mut c_void,
        _: *mut u8,
        len: usize,
        _: u8,
        new_len: usize,
        _: usize,
    ) -> bool {
        new_len == len
    }

    extern "C" fn heap_remap(
        _: *mut c_void,
        memory: *mut u8,
        len: usize,
        alignment: u8,
        new_len: usize,
        _: usize,
    ) -> *mut u8 {
        match (layout(len, alignment), layout(new_len, alignment)) {
            // SAFETY: `memory` is a live block of `len` bytes from this heap.
            (Some(old), Some(_)) if new_len != 0 => unsafe {
                std::alloc::realloc(memory, old, new_len)
            },
            _ => core::ptr::null_mut(),
        }
    }

    extern "C" fn heap_free(_: *mut c_void, memory: *mut u8, len: usize, alignment: u8, _: usize) {
        if let Some(l) = layout(len, alignment) {
            // SAFETY: `memory` is a live block of `len` bytes from this heap.
            unsafe { std::alloc::dealloc(memory, l) }
        }
    }

    /// Call the wrapped allocator's vtable.
    macro_rules! internal {
        ($s:expr, $f:ident($($arg:expr),*)) => {{
            let c = &$s.internal_allocator;
            // SAFETY: the internal allocator is valid for the wrapper's life
            // (the constructor's contract).
            unsafe { ((*c.vtable).$f)(c.ctx, $($arg),*) }
        }};
    }

    /// `std.testing.FailingAllocator` (Zig 0.16
    /// `std/testing/FailingAllocator.zig`): an allocator that fails after N
    /// allocations (or N resizes), useful for making sure out of memory
    /// conditions are handled correctly.
    ///
    /// PORT NOTE: the fields are `Cell`s because the C vtable reaches the state
    /// through a shared `ctx` pointer. That includes `fail_index` and
    /// `resize_fail_index`, which the reference's tests move mid-test
    /// (`failing.fail_index = failing.alloc_index;`) while objects built over
    /// the allocator are live: a write through `&mut FailingAllocator` there
    /// would invalidate the `ctx` pointer those objects hold. Zig's `stack_addresses:
    /// [num_stack_frames]usize` (the current stack captured at the first
    /// induced failure, starting at the failing call's `return_address`) is
    /// `stack_trace`, a `std::backtrace::Backtrace` force-captured at the same
    /// moment; `getStackTrace` is `get_stack_trace`. The capture starts in this
    /// callback rather than at the caller's return address (the C ABI carries
    /// `ret_addr = 0`, W36-B06), so it shows a few more frames at the top. Zig's
    /// fixed 16-frame buffer has no counterpart: the backtrace is unbounded.
    pub struct FailingAllocator {
        pub alloc_index: Cell<usize>,
        pub resize_index: Cell<usize>,
        internal_allocator: CAllocator,
        pub allocated_bytes: Cell<usize>,
        pub freed_bytes: Cell<usize>,
        pub allocations: Cell<usize>,
        pub deallocations: Cell<usize>,
        pub stack_trace: RefCell<Option<Backtrace>>,
        pub has_induced_failure: Cell<bool>,
        pub fail_index: Cell<usize>,
        pub resize_fail_index: Cell<usize>,
    }

    /// `FailingAllocator.Config`.
    #[derive(Clone, Copy, Debug)]
    pub struct Config {
        /// The number of successful allocations you can expect from this allocator.
        /// The next allocation will fail.
        pub fail_index: usize,

        /// Number of successful resizes to expect from this allocator. The next resize will fail.
        pub resize_fail_index: usize,
    }

    impl Default for Config {
        fn default() -> Config {
            Config {
                fail_index: usize::MAX,
                resize_fail_index: usize::MAX,
            }
        }
    }

    impl FailingAllocator {
        /// `FailingAllocator.init(internal_allocator, config)`.
        ///
        /// # Safety
        /// `internal_allocator` must be a valid C allocator for as long as this
        /// wrapper is used.
        pub unsafe fn init(internal_allocator: CAllocator, config: Config) -> FailingAllocator {
            FailingAllocator {
                internal_allocator,
                alloc_index: Cell::new(0),
                resize_index: Cell::new(0),
                allocated_bytes: Cell::new(0),
                freed_bytes: Cell::new(0),
                allocations: Cell::new(0),
                deallocations: Cell::new(0),
                // .stack_addresses = undefined,
                stack_trace: RefCell::new(None),
                has_induced_failure: Cell::new(false),
                fail_index: Cell::new(config.fail_index),
                resize_fail_index: Cell::new(config.resize_fail_index),
            }
        }

        /// `init` over [`heap`].
        pub fn over_heap(config: Config) -> FailingAllocator {
            // SAFETY: `heap()` is a static vtable over the global heap.
            unsafe { FailingAllocator::init(heap(), config) }
        }

        /// Only valid once `has_induced_failure == true`
        ///
        /// `getStackTrace`: the stack captured at the first induced failure.
        pub fn get_stack_trace(&self) -> Ref<'_, Backtrace> {
            // std.debug.assert(self.has_induced_failure);
            debug_assert!(self.has_induced_failure.get());
            Ref::map(self.stack_trace.borrow(), |t| {
                t.as_ref()
                    .expect("get_stack_trace before any induced failure")
            })
        }

        /// `allocator()`: the C allocator over this state. `self` must stay at
        /// the same address while the result (or a handle over it) is in use.
        pub fn allocator(&self) -> CAllocator {
            CAllocator {
                ctx: self as *const FailingAllocator as *mut c_void,
                vtable: &FAILING_ALLOCATOR_VTABLE,
            }
        }
    }

    static FAILING_ALLOCATOR_VTABLE: VTable = VTable {
        alloc: fa_alloc,
        resize: fa_resize,
        remap: fa_remap,
        free: fa_free,
    };

    /// # Safety
    /// `ctx` is the `FailingAllocator` that `allocator()` was called on.
    unsafe fn fa<'a>(ctx: *mut c_void) -> &'a FailingAllocator {
        // SAFETY: the caller's contract.
        unsafe { &*(ctx as *const FailingAllocator) }
    }

    extern "C" fn fa_alloc(ctx: *mut c_void, len: usize, alignment: u8, ra: usize) -> *mut u8 {
        // const self: *FailingAllocator = @ptrCast(@alignCast(ctx));
        // SAFETY: `ctx` came from `allocator()`.
        let s = unsafe { fa(ctx) };
        if s.alloc_index.get() == s.fail_index.get() {
            if !s.has_induced_failure.get() {
                // const st = std.debug.captureCurrentStackTrace(.{ .first_address = return_address }, &self.stack_addresses);
                // @memset(self.stack_addresses[@min(st.return_addresses.len, self.stack_addresses.len)..], 0);
                *s.stack_trace.borrow_mut() = Some(Backtrace::force_capture());
                s.has_induced_failure.set(true);
            }
            return core::ptr::null_mut();
        }
        let result = internal!(s, alloc(len, alignment, ra));
        if result.is_null() {
            return core::ptr::null_mut();
        }
        s.allocated_bytes.set(s.allocated_bytes.get() + len);
        s.allocations.set(s.allocations.get() + 1);
        s.alloc_index.set(s.alloc_index.get() + 1);
        result
    }

    extern "C" fn fa_resize(
        ctx: *mut c_void,
        memory: *mut u8,
        len: usize,
        alignment: u8,
        new_len: usize,
        ra: usize,
    ) -> bool {
        // SAFETY: `ctx` came from `allocator()`.
        let s = unsafe { fa(ctx) };
        if s.resize_index.get() == s.resize_fail_index.get() {
            return false;
        }
        if !internal!(s, resize(memory, len, alignment, new_len, ra)) {
            return false;
        }
        if new_len < len {
            s.freed_bytes.set(s.freed_bytes.get() + (len - new_len));
        } else {
            s.allocated_bytes
                .set(s.allocated_bytes.get() + (new_len - len));
        }
        s.resize_index.set(s.resize_index.get() + 1);
        true
    }

    extern "C" fn fa_remap(
        ctx: *mut c_void,
        memory: *mut u8,
        len: usize,
        alignment: u8,
        new_len: usize,
        ra: usize,
    ) -> *mut u8 {
        // SAFETY: `ctx` came from `allocator()`.
        let s = unsafe { fa(ctx) };
        if s.resize_index.get() == s.resize_fail_index.get() {
            return core::ptr::null_mut();
        }
        let new_ptr = internal!(s, remap(memory, len, alignment, new_len, ra));
        if new_ptr.is_null() {
            return core::ptr::null_mut();
        }
        if new_len < len {
            s.freed_bytes.set(s.freed_bytes.get() + (len - new_len));
        } else {
            s.allocated_bytes
                .set(s.allocated_bytes.get() + (new_len - len));
        }
        s.resize_index.set(s.resize_index.get() + 1);
        new_ptr
    }

    extern "C" fn fa_free(ctx: *mut c_void, memory: *mut u8, len: usize, alignment: u8, ra: usize) {
        // SAFETY: `ctx` came from `allocator()`.
        let s = unsafe { fa(ctx) };
        internal!(s, free(memory, len, alignment, ra));
        s.deallocations.set(s.deallocations.get() + 1);
        s.freed_bytes.set(s.freed_bytes.get() + len);
    }

    /// A counting C allocator: every raw callback is counted, refused or not,
    /// with the outstanding and peak byte counts, so a test can prove which
    /// allocator a family's internals reach and that nothing is left live.
    ///
    /// Rust-only test glue (the routing evidence of the S-7 design).
    pub struct CountingAllocator {
        internal_allocator: CAllocator,
        pub alloc_calls: Cell<usize>,
        pub resize_calls: Cell<usize>,
        pub remap_calls: Cell<usize>,
        pub free_calls: Cell<usize>,
        /// Bytes handed out and not yet freed.
        pub live_bytes: Cell<usize>,
        /// The largest `live_bytes` seen.
        pub peak_bytes: Cell<usize>,
        /// The last raw `alloc` length and alignment (log2) requested.
        pub last_alloc: Cell<Option<(usize, u8)>>,
        /// The last raw `free` length and alignment (log2) passed.
        pub last_free: Cell<Option<(usize, u8)>>,
    }

    impl CountingAllocator {
        /// # Safety
        /// `internal_allocator` must be a valid C allocator for as long as this
        /// wrapper is used.
        pub unsafe fn init(internal_allocator: CAllocator) -> CountingAllocator {
            CountingAllocator {
                internal_allocator,
                alloc_calls: Cell::new(0),
                resize_calls: Cell::new(0),
                remap_calls: Cell::new(0),
                free_calls: Cell::new(0),
                live_bytes: Cell::new(0),
                peak_bytes: Cell::new(0),
                last_alloc: Cell::new(None),
                last_free: Cell::new(None),
            }
        }

        /// `init` over [`heap`].
        pub fn over_heap() -> CountingAllocator {
            // SAFETY: `heap()` is a static vtable over the global heap.
            unsafe { CountingAllocator::init(heap()) }
        }

        /// The C allocator over this state. `self` must stay at the same
        /// address while the result (or a handle over it) is in use.
        pub fn allocator(&self) -> CAllocator {
            CAllocator {
                ctx: self as *const CountingAllocator as *mut c_void,
                vtable: &COUNTING_ALLOCATOR_VTABLE,
            }
        }

        fn add_live(&self, n: usize) {
            let live = self.live_bytes.get() + n;
            self.live_bytes.set(live);
            self.peak_bytes.set(self.peak_bytes.get().max(live));
        }
    }

    static COUNTING_ALLOCATOR_VTABLE: VTable = VTable {
        alloc: ca_alloc,
        resize: ca_resize,
        remap: ca_remap,
        free: ca_free,
    };

    /// # Safety
    /// `ctx` is the `CountingAllocator` that `allocator()` was called on.
    unsafe fn ca<'a>(ctx: *mut c_void) -> &'a CountingAllocator {
        // SAFETY: the caller's contract.
        unsafe { &*(ctx as *const CountingAllocator) }
    }

    extern "C" fn ca_alloc(ctx: *mut c_void, len: usize, alignment: u8, ra: usize) -> *mut u8 {
        // SAFETY: `ctx` came from `allocator()`.
        let s = unsafe { ca(ctx) };
        s.alloc_calls.set(s.alloc_calls.get() + 1);
        s.last_alloc.set(Some((len, alignment)));
        let p = internal!(s, alloc(len, alignment, ra));
        if !p.is_null() {
            s.add_live(len);
        }
        p
    }

    extern "C" fn ca_resize(
        ctx: *mut c_void,
        memory: *mut u8,
        len: usize,
        alignment: u8,
        new_len: usize,
        ra: usize,
    ) -> bool {
        // SAFETY: `ctx` came from `allocator()`.
        let s = unsafe { ca(ctx) };
        s.resize_calls.set(s.resize_calls.get() + 1);
        let ok = internal!(s, resize(memory, len, alignment, new_len, ra));
        if ok {
            s.live_bytes.set(s.live_bytes.get() - len);
            s.add_live(new_len);
        }
        ok
    }

    extern "C" fn ca_remap(
        ctx: *mut c_void,
        memory: *mut u8,
        len: usize,
        alignment: u8,
        new_len: usize,
        ra: usize,
    ) -> *mut u8 {
        // SAFETY: `ctx` came from `allocator()`.
        let s = unsafe { ca(ctx) };
        s.remap_calls.set(s.remap_calls.get() + 1);
        let p = internal!(s, remap(memory, len, alignment, new_len, ra));
        if !p.is_null() {
            s.live_bytes.set(s.live_bytes.get() - len);
            s.add_live(new_len);
        }
        p
    }

    extern "C" fn ca_free(ctx: *mut c_void, memory: *mut u8, len: usize, alignment: u8, ra: usize) {
        // SAFETY: `ctx` came from `allocator()`.
        let s = unsafe { ca(ctx) };
        s.free_calls.set(s.free_calls.get() + 1);
        s.last_free.set(Some((len, alignment)));
        s.live_bytes.set(s.live_bytes.get() - len);
        internal!(s, free(memory, len, alignment, ra));
    }

    /// Run `f` with a handle over `c` (which outlives the call).
    pub fn with_handle<R>(c: &CAllocator, f: impl FnOnce(VtAllocator) -> R) -> R {
        // SAFETY: `c` is borrowed for the whole call; a handle that escapes `f`
        // is the caller's bug, as with any `from_c`.
        f(unsafe { VtAllocator::from_c(NonNull::from(c)) })
    }
}

#[cfg(test)]
mod tests {
    use super::testing::{with_handle, Config, CountingAllocator, FailingAllocator};
    use super::*;
    use allocator_api2::vec::Vec as AVec;

    fn l(size: usize, align: usize) -> Layout {
        Layout::from_size_align(size, align).unwrap()
    }

    #[test]
    fn handle_is_one_pointer_and_global_by_default() {
        assert_eq!(
            core::mem::size_of::<VtAllocator>(),
            core::mem::size_of::<usize>()
        );
        assert!(VtAllocator::default().is_global());
        assert!(Allocator.is_global());
        assert!(Global.is_global());
        assert_eq!(VtAllocator::from(ApiGlobal), VtAllocator::GLOBAL);
        assert_eq!(VtAllocator::GLOBAL.c_allocator(), None);
        // SAFETY: NULL is the process default.
        assert!(unsafe { default(None) }.is_global());
    }

    #[test]
    fn default_non_null_is_the_callers_own_struct() {
        let counting = CountingAllocator::over_heap();
        let c = counting.allocator();
        // SAFETY: `c` outlives the handle.
        let a = unsafe { default(Some(&c)) };
        assert!(!a.is_global());
        // The caller's pointer itself, not a copy (N260).
        assert_eq!(
            a.c_allocator().unwrap().as_ptr() as *const CAllocator,
            &c as *const _
        );
    }

    #[test]
    fn equality_is_pointer_identity() {
        let c1 = testing::heap();
        let c2 = testing::heap();
        with_handle(&c1, |a1| {
            with_handle(&c2, |a2| {
                assert_eq!(a1, a1);
                assert_ne!(
                    a1, a2,
                    "same contents, different struct: not the same allocator"
                );
                assert_ne!(a1, VtAllocator::GLOBAL);
                a1.debug_assert_same(a1);
            })
        });
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "allocator mismatch")]
    fn debug_assert_same_fires_on_a_different_allocator() {
        let c = testing::heap();
        with_handle(&c, |a| VtAllocator::GLOBAL.debug_assert_same(a));
    }

    #[test]
    fn dispatch_follows_the_handle() {
        // The same workload through each arm. Through the C arm every block
        // comes from, and goes back to, the counting allocator; through the
        // Global arm the handle carries no C allocator at all, so the same
        // counter stays at 0 while the vector still grows. (Review S7-12 F5:
        // the earlier Global-only version could not fail.)
        fn workload(h: VtAllocator) -> usize {
            let mut v: AVec<u8, VtAllocator> = AVec::new_in(h);
            v.extend_copy(&[1, 2, 3]);
            v.resize_copy(1000, 7);
            v.shrink_to_fit();
            assert_eq!(
                *v.allocator(),
                h,
                "the vector is bound to the handle it was given"
            );
            assert_eq!((v[0], v[999], v.len()), (1, 7, 1000));
            v.capacity()
        }
        let counting = CountingAllocator::over_heap();
        let c = counting.allocator();
        with_handle(&c, |h| {
            assert!(workload(h) >= 1000);
        });
        let c_calls = counting.alloc_calls.get();
        assert!(
            c_calls >= 1,
            "the C arm allocated through the caller's vtable"
        );
        assert_eq!(
            counting.free_calls.get(),
            c_calls,
            "and freed every block there"
        );
        assert_eq!(counting.live_bytes.get(), 0);

        assert_eq!(VtAllocator::GLOBAL.c_allocator(), None);
        assert!(workload(VtAllocator::GLOBAL) >= 1000);
        assert_eq!(
            counting.alloc_calls.get(),
            c_calls,
            "the Global arm made no C call"
        );
    }

    #[test]
    fn c_arm_alloc_and_free_carry_length_and_alignment() {
        let counting = CountingAllocator::over_heap();
        let c = counting.allocator();
        with_handle(&c, |a| {
            let p = a.allocate(l(24, 8)).unwrap();
            assert_eq!(p.len(), 24);
            assert_eq!(p.as_ptr() as *mut u8 as usize % 8, 0);
            assert_eq!(counting.alloc_calls.get(), 1);
            assert_eq!(counting.last_alloc.get(), Some((24, 3)));
            assert_eq!(counting.live_bytes.get(), 24);
            // SAFETY: allocated above with this layout.
            unsafe { a.deallocate(p.cast(), l(24, 8)) };
            assert_eq!(counting.free_calls.get(), 1);
            assert_eq!(counting.last_free.get(), Some((24, 3)));
            assert_eq!(counting.live_bytes.get(), 0);

            // A larger alignment reaches the vtable as its log2.
            let p = a.allocate(l(64, 64)).unwrap();
            assert_eq!(counting.last_alloc.get(), Some((64, 6)));
            assert_eq!(p.as_ptr() as *mut u8 as usize % 64, 0);
            // SAFETY: allocated above with this layout.
            unsafe { a.deallocate(p.cast(), l(64, 64)) };
            assert_eq!(counting.last_free.get(), Some((64, 6)));
        });
    }

    #[test]
    fn c_arm_zero_size_never_reaches_the_vtable() {
        let counting = CountingAllocator::over_heap();
        let c = counting.allocator();
        with_handle(&c, |a| {
            let p = a.allocate(l(0, 16)).unwrap();
            assert_eq!(p.len(), 0);
            assert_eq!(p.as_ptr() as *mut u8 as usize % 16, 0);
            // SAFETY: the zero-size block from above.
            unsafe { a.deallocate(p.cast(), l(0, 16)) };
            let z = a.allocate_zeroed(l(0, 1)).unwrap();
            assert_eq!(z.len(), 0);
        });
        assert_eq!(counting.alloc_calls.get(), 0);
        assert_eq!(counting.free_calls.get(), 0);
    }

    #[test]
    fn c_arm_allocate_zeroed_zeroes() {
        let counting = CountingAllocator::over_heap();
        let c = counting.allocator();
        with_handle(&c, |a| {
            let p = a.allocate_zeroed(l(33, 1)).unwrap();
            // SAFETY: 33 initialized (zeroed) bytes.
            let s = unsafe { core::slice::from_raw_parts(p.as_ptr() as *const u8, 33) };
            assert!(s.iter().all(|&b| b == 0));
            // SAFETY: allocated above with this layout.
            unsafe { a.deallocate(p.cast(), l(33, 1)) };
        });
        assert_eq!(counting.live_bytes.get(), 0);
    }

    #[test]
    fn c_arm_grow_tries_remap_first_then_falls_back() {
        // remap hit: the heap's remap (realloc) succeeds, no fresh alloc.
        let counting = CountingAllocator::over_heap();
        let c = counting.allocator();
        with_handle(&c, |a| {
            let p = a.allocate(l(8, 1)).unwrap();
            // SAFETY: fill the 8 live bytes.
            unsafe { core::ptr::write_bytes(p.as_ptr() as *mut u8, 0xAB, 8) };
            // SAFETY: `p` is live with the old layout.
            let g = unsafe { a.grow(p.cast(), l(8, 1), l(100, 1)) }.unwrap();
            assert_eq!(g.len(), 100);
            assert_eq!(counting.remap_calls.get(), 1);
            assert_eq!(
                counting.alloc_calls.get(),
                1,
                "remap hit: no fresh allocation"
            );
            // SAFETY: the first 8 bytes were preserved.
            let s = unsafe { core::slice::from_raw_parts(g.as_ptr() as *const u8, 8) };
            assert_eq!(s, &[0xAB; 8]);
            // SAFETY: live with the new layout.
            unsafe { a.deallocate(g.cast(), l(100, 1)) };
        });
        assert_eq!(counting.live_bytes.get(), 0);

        // remap miss: a failing-on-resize wrapper refuses remap, so the grow
        // is alloc + copy + free, as Zig's `realloc` falls back.
        let inner = CountingAllocator::over_heap();
        // SAFETY: `inner` outlives `failing`.
        let failing = unsafe {
            FailingAllocator::init(
                inner.allocator(),
                Config {
                    resize_fail_index: 0,
                    ..Config::default()
                },
            )
        };
        let c = failing.allocator();
        with_handle(&c, |a| {
            let p = a.allocate(l(8, 1)).unwrap();
            // SAFETY: fill the 8 live bytes.
            unsafe { core::ptr::write_bytes(p.as_ptr() as *mut u8, 0xCD, 8) };
            // SAFETY: `p` is live with the old layout.
            let g = unsafe { a.grow_zeroed(p.cast(), l(8, 1), l(16, 1)) }.unwrap();
            // SAFETY: 16 initialized bytes (8 copied + 8 zeroed).
            let s = unsafe { core::slice::from_raw_parts(g.as_ptr() as *const u8, 16) };
            assert_eq!(&s[..8], &[0xCD; 8]);
            assert_eq!(&s[8..], &[0; 8]);
            assert_eq!(
                inner.remap_calls.get(),
                0,
                "refused before reaching the inner"
            );
            assert_eq!(failing.allocations.get(), 2, "the fallback allocated fresh");
            assert_eq!(failing.deallocations.get(), 1, "and freed the old block");
            assert_eq!(inner.last_free.get(), Some((8, 0)));
            // SAFETY: live with the new layout.
            unsafe { a.deallocate(g.cast(), l(16, 1)) };
        });
        assert_eq!(inner.live_bytes.get(), 0);
    }

    #[test]
    fn c_arm_grow_with_new_alignment_does_not_remap() {
        let counting = CountingAllocator::over_heap();
        let c = counting.allocator();
        with_handle(&c, |a| {
            let p = a.allocate(l(8, 1)).unwrap();
            // SAFETY: `p` is live with the old layout.
            let g = unsafe { a.grow(p.cast(), l(8, 1), l(64, 32)) }.unwrap();
            assert_eq!(g.as_ptr() as *mut u8 as usize % 32, 0);
            assert_eq!(counting.remap_calls.get(), 0);
            assert_eq!(counting.alloc_calls.get(), 2);
            assert_eq!(counting.free_calls.get(), 1);
            // SAFETY: live with the new layout.
            unsafe { a.deallocate(g.cast(), l(64, 32)) };
        });
        assert_eq!(counting.live_bytes.get(), 0);
    }

    #[test]
    fn c_arm_shrink_and_shrink_to_zero() {
        let counting = CountingAllocator::over_heap();
        let c = counting.allocator();
        with_handle(&c, |a| {
            let p = a.allocate(l(64, 8)).unwrap();
            // SAFETY: `p` is live with the old layout.
            let s = unsafe { a.shrink(p.cast(), l(64, 8), l(16, 8)) }.unwrap();
            assert_eq!(s.len(), 16);
            assert_eq!(counting.remap_calls.get(), 1);
            assert_eq!(counting.live_bytes.get(), 16);
            // Shrinking to zero is Zig's `realloc(.., 0)`: a free, no remap.
            // SAFETY: live with the 16-byte layout.
            let z = unsafe { a.shrink(s.cast(), l(16, 8), l(0, 8)) }.unwrap();
            assert_eq!(z.len(), 0);
            assert_eq!(counting.remap_calls.get(), 1);
            assert_eq!(counting.free_calls.get(), 1);
            assert_eq!(counting.last_free.get(), Some((16, 3)));
        });
        assert_eq!(counting.live_bytes.get(), 0);
    }

    #[test]
    fn c_arm_resize_in_place_asks_the_c_resize() {
        let counting = CountingAllocator::over_heap();
        let c = counting.allocator();
        with_handle(&c, |a| {
            let p = a.allocate(l(32, 1)).unwrap();
            // The heap refuses a length change in place...
            // SAFETY: `p` is live with this layout; `new_len` non-zero.
            assert!(!unsafe { a.resize_in_place(p.cast(), l(32, 1), 64) });
            // ...and accepts the same length.
            // SAFETY: as above.
            assert!(unsafe { a.resize_in_place(p.cast(), l(32, 1), 32) });
            assert_eq!(counting.resize_calls.get(), 2);
            // SAFETY: live with this layout.
            unsafe { a.deallocate(p.cast(), l(32, 1)) };
        });
        // The Global arm refuses without any call (as `impl for Global`).
        let p = VtAllocator::GLOBAL.allocate(l(32, 1)).unwrap();
        // SAFETY: `p` is live with this layout.
        assert!(!unsafe { VtAllocator::GLOBAL.resize_in_place(p.cast(), l(32, 1), 16) });
        // SAFETY: live with this layout.
        unsafe { VtAllocator::GLOBAL.deallocate(p.cast(), l(32, 1)) };
        assert_eq!(counting.resize_calls.get(), 2);
    }

    // Parity tail S7-4: the `std.heap.page_allocator` handle hands out
    // page-aligned memory, grows by alloc + copy + free (its `remap` is
    // refused) and frees with the allocation's own layout.
    // `stackFallback`: requests that fit the buffer reach no allocator,
    // larger ones (and the overflow of a growing list) reach the fallback,
    // and every fallback byte is returned.
    #[test]
    fn stack_fallback_serves_the_buffer_then_the_fallback() {
        let counting = CountingAllocator::over_heap();
        let c = counting.allocator();
        with_handle(&c, |alloc| {
            let sfa = StackFallback::<64>::new(alloc);
            let mut small: AVec<u64, &StackFallback<64>> = AVec::new_in(&sfa);
            small.try_reserve_exact(4).unwrap();
            small.extend([1, 2, 3, 4]);
            assert_eq!(0, counting.alloc_calls.get(), "32 bytes fit the buffer");
            let mut more: AVec<u8, &StackFallback<64>> = AVec::new_in(&sfa);
            more.try_reserve_exact(32).unwrap();
            assert_eq!(0, counting.alloc_calls.get(), "64 bytes still fit");
            let mut big: AVec<u8, &StackFallback<64>> = AVec::new_in(&sfa);
            big.try_reserve_exact(1).unwrap();
            assert_eq!(1, counting.alloc_calls.get(), "the buffer is full");
            assert_eq!(Some((1, 0)), counting.last_alloc.get());
            drop(big);
            drop(more);
            // The last in-buffer block was returned; a grow past the buffer
            // moves the list to the fallback.
            small.try_reserve_exact(100).unwrap();
            assert_eq!(2, counting.alloc_calls.get());
            assert_eq!(&[1, 2, 3, 4], small.as_slice());
            drop(small);
            assert_eq!(
                0,
                counting.live_bytes.get(),
                "every fallback byte is returned"
            );

            // A failing fallback surfaces as the allocation error.
            let sfa = StackFallback::<8>::new(failing());
            let mut v: AVec<u8, &StackFallback<8>> = AVec::new_in(&sfa);
            assert!(v.try_reserve_exact(8).is_ok());
            assert!(v.try_reserve_exact(9).is_err());
        });
    }

    #[test]
    fn page_allocator_handle_allocates_grows_and_frees() {
        let h = page_allocator();
        assert!(!h.is_global());
        let page = l(4096, 4096);
        let p = h.allocate(page).unwrap();
        assert_eq!(0, p.as_ptr() as *mut u8 as usize % 4096);
        // SAFETY: a fresh 4096-byte block.
        unsafe { core::ptr::write_bytes(p.as_ptr() as *mut u8, 7, 4096) };
        // SAFETY: the live block of `page`, grown to two pages.
        let g = unsafe { h.grow(p.cast(), page, l(8192, 4096)) }.unwrap();
        // SAFETY: the grown block keeps the first page's bytes.
        assert_eq!(7, unsafe { *(g.as_ptr() as *mut u8).add(4095) });
        // SAFETY: the live grown block.
        unsafe { h.deallocate(g.cast(), l(8192, 4096)) };
    }

    #[test]
    fn c_arm_null_is_alloc_error() {
        let failing = FailingAllocator::over_heap(Config {
            fail_index: 0,
            ..Config::default()
        });
        let c = failing.allocator();
        with_handle(&c, |a| {
            assert!(a.allocate(l(8, 1)).is_err());
            assert!(a.allocate_zeroed(l(8, 1)).is_err());
            let mut v: AVec<u8, VtAllocator> = AVec::new_in(a);
            assert_eq!(v.try_push(1), Err(OutOfMemory));
            assert_eq!(v.try_extend_copy(&[1, 2]), Err(OutOfMemory));
            assert_eq!(v.try_resize_copy(4, 0), Err(OutOfMemory));
            assert!(v.is_empty(), "a failed growth leaves the vector unchanged");
        });
        assert!(failing.has_induced_failure.get());
        assert_eq!(failing.allocations.get(), 0);
        // `failing()` is `std.mem.Allocator.failing`.
        let f = failing_handle_checks();
        assert!(f);
    }

    fn failing_handle_checks() -> bool {
        let a = failing();
        assert!(!a.is_global());
        assert!(a.allocate(l(1, 1)).is_err());
        let p = VtAllocator::GLOBAL.allocate(l(4, 1)).unwrap();
        // SAFETY: `p` is a live Global block; the failing resize never touches it.
        assert!(!unsafe { a.resize_in_place(p.cast(), l(4, 1), 4) });
        // SAFETY: live with this layout.
        unsafe { VtAllocator::GLOBAL.deallocate(p.cast(), l(4, 1)) };
        true
    }

    #[test]
    fn fallible_helpers_succeed_and_match_the_std_methods() {
        let counting = CountingAllocator::over_heap();
        let c = counting.allocator();
        with_handle(&c, |a| {
            let mut v: AVec<u32, VtAllocator> = AVec::new_in(a);
            v.try_push(1).unwrap();
            v.try_extend_copy(&[2, 3, 4]).unwrap();
            v.try_resize_copy(6, 9).unwrap();
            assert_eq!(&v[..], &[1, 2, 3, 4, 9, 9]);
            v.try_resize_copy(2, 0).unwrap();
            assert_eq!(&v[..], &[1, 2]);
            v.extend_copy(&[5]);
            v.resize_copy(5, 8);
            assert_eq!(&v[..], &[1, 2, 5, 8, 8]);
            v.resize_copy(0, 0);
            assert!(v.is_empty());
            let mut std_v: Vec<u32> = vec![1];
            std_v.extend_from_slice(&[2, 3]);
            std_v.resize(5, 7);
            let mut av: AVec<u32, VtAllocator> = AVec::new_in(a);
            av.extend_copy(&[1]);
            av.extend_copy(&[2, 3]);
            av.resize_copy(5, 7);
            assert_eq!(&av[..], &std_v[..]);
            assert!(
                counting.alloc_calls.get() >= 2,
                "the vectors grew through the C arm"
            );
        });
        assert_eq!(
            counting.live_bytes.get(),
            0,
            "every vector freed through the C arm"
        );
    }

    #[test]
    fn fallible_helpers_fail_after_n() {
        // One successful allocation, then failure: the second growth fails
        // with OutOfMemory (not an abort) and keeps the first contents.
        let failing = FailingAllocator::over_heap(Config {
            fail_index: 1,
            resize_fail_index: 0,
        });
        let c = failing.allocator();
        with_handle(&c, |a| {
            let mut v: AVec<u8, VtAllocator> = AVec::new_in(a);
            v.try_extend_copy(&[1, 2, 3]).unwrap();
            let cap = v.capacity();
            let big = [0u8; 4096];
            assert_eq!(v.try_extend_copy(&big[..cap + 1]), Err(OutOfMemory));
            assert_eq!(&v[..], &[1, 2, 3]);
        });
        assert!(failing.has_induced_failure.get());
        assert_eq!(failing.allocations.get(), 1);
        assert_eq!(failing.deallocations.get(), 1);
        assert_eq!(failing.allocated_bytes.get(), failing.freed_bytes.get());
    }

    // Zig 0.16 `std/testing/FailingAllocator.zig` `test FailingAllocator`,
    // over the C ABI and the handle (Rust-only glue: the Zig std tests are not
    // in the port's test contract).
    #[test]
    fn failing_allocator() {
        // Fail on allocation
        {
            let failing_allocator_state = FailingAllocator::over_heap(Config {
                fail_index: 2,
                ..Config::default()
            });
            let c = failing_allocator_state.allocator();
            with_handle(&c, |failing_alloc| {
                let a = failing_alloc.allocate(Layout::new::<i32>()).unwrap();
                let b = failing_alloc.allocate(Layout::new::<i32>()).unwrap();
                assert!(failing_alloc.allocate(Layout::new::<i32>()).is_err());
                // SAFETY: both are live with this layout.
                unsafe {
                    failing_alloc.deallocate(b.cast(), Layout::new::<i32>());
                    failing_alloc.deallocate(a.cast(), Layout::new::<i32>());
                }
            });
        }
        // Fail on resize
        {
            let failing_allocator_state = FailingAllocator::over_heap(Config {
                resize_fail_index: 1,
                ..Config::default()
            });
            let c = failing_allocator_state.allocator();
            let s = &failing_allocator_state;
            let ctx = c.ctx;
            let vt = c.vtable;
            // SAFETY: `c` is the live C allocator over `s`.
            let slice = unsafe { ((*vt).alloc)(ctx, 8, 0, 0) };
            assert!(!slice.is_null());
            // SAFETY: remap of the live 8-byte block to 6.
            let resized = unsafe { ((*vt).remap)(ctx, slice, 8, 0, 6, 0) };
            assert!(!resized.is_null(), "UnexpectedRemapFailure");
            // Remap and resize should fail from here on out
            // SAFETY: the live 6-byte block.
            assert!(unsafe { ((*vt).remap)(ctx, resized, 6, 0, 4, 0) }.is_null());
            // SAFETY: as above.
            assert!(!unsafe { ((*vt).resize)(ctx, resized, 6, 0, 4, 0) });
            // Note: realloc could succeed because it falls back to free+alloc
            // SAFETY: free the live 6-byte block.
            unsafe { ((*vt).free)(ctx, resized, 6, 0, 0) };
            assert_eq!(s.resize_index.get(), 1);
            assert_eq!(s.allocations.get(), 1);
            assert_eq!(s.deallocations.get(), 1);
            assert_eq!(s.allocated_bytes.get(), 8);
            assert_eq!(
                s.freed_bytes.get(),
                8,
                "2 by the remap shrink + 6 by the free"
            );
            // Through the handle, the refused remap falls back to alloc + copy
            // + free, so the realloc still succeeds.
            with_handle(&c, |h| {
                let p = h.allocate(l(8, 1)).unwrap();
                // SAFETY: live with the old layout.
                let g = unsafe { h.grow(p.cast(), l(8, 1), l(32, 1)) }.unwrap();
                // SAFETY: live with the new layout.
                unsafe { h.deallocate(g.cast(), l(32, 1)) };
            });
            assert_eq!(s.allocations.get(), 3);
            assert_eq!(s.deallocations.get(), 3);
        }
    }

    #[test]
    fn failing_allocator_captures_the_stack_at_the_first_induced_failure() {
        let f = FailingAllocator::over_heap(Config {
            fail_index: 1,
            ..Config::default()
        });
        let c = f.allocator();
        with_handle(&c, |a| {
            let p = a.allocate(l(8, 1)).unwrap();
            assert!(!f.has_induced_failure.get());
            assert!(
                f.stack_trace.borrow().is_none(),
                "nothing captured before a failure"
            );
            assert!(a.allocate(l(8, 1)).is_err());
            assert!(f.has_induced_failure.get());
            let first = format!("{:?}", *f.get_stack_trace());
            assert!(!first.is_empty());
            // Only the FIRST induced failure captures (Zig: `if (!has_induced_failure)`).
            assert!(a.allocate(l(8, 1)).is_err());
            assert_eq!(format!("{:?}", *f.get_stack_trace()), first);
            // SAFETY: live with this layout.
            unsafe { a.deallocate(p.cast(), l(8, 1)) };
        });
    }

    #[test]
    fn failing_allocator_counts_only_successes_and_is_exact_at_n() {
        for n in 0..4 {
            let f = FailingAllocator::over_heap(Config {
                fail_index: n,
                ..Config::default()
            });
            let c = f.allocator();
            let mut got = 0;
            with_handle(&c, |a| {
                let mut live = alloc_vec_of_blocks();
                for _ in 0..6 {
                    match a.allocate(l(4, 4)) {
                        Ok(p) => {
                            got += 1;
                            live.push(p);
                        }
                        Err(_) => break,
                    }
                }
                for p in live {
                    // SAFETY: each is live with this layout.
                    unsafe { a.deallocate(p.cast(), l(4, 4)) };
                }
            });
            assert_eq!(got, n, "exactly n allocations succeed");
            assert_eq!(f.alloc_index.get(), n);
            assert!(f.has_induced_failure.get());
        }
    }

    fn alloc_vec_of_blocks() -> Vec<NonNull<[u8]>> {
        Vec::new()
    }

    #[test]
    fn arena_over_the_c_arm_routes_and_frees() {
        // zigstd's arena over the handle: its blocks come from the C
        // allocator and all go back on drop.
        let counting = CountingAllocator::over_heap();
        let c = counting.allocator();
        with_handle(&c, |a| {
            let mut arena = rustty_zigstd::arena_allocator::ArenaAllocator::with_child(a);
            assert!(arena.alloc_bytes(100, 8).is_some());
            assert!(counting.alloc_calls.get() >= 1);
            assert!(counting.live_bytes.get() >= 100);
            drop(arena);
        });
        assert_eq!(counting.live_bytes.get(), 0);
    }
}
