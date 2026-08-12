//! rustty-vt-c test helpers
//!
//! PORT NOTE (Rust-only glue, R1 "no-loss, not no-addition"): the reference
//! tests pass `&lib.alloc.test_allocator` — Zig's leak-detecting testing
//! allocator. rustty has no such global (`test_allocator` is intentionally
//! omitted from rustty-lib; see its allocator.rs PORT NOTE); the parity analog
//! is a C `Allocator` backed by the process global allocator, with leaks caught
//! by Miri per R6. `with_test_allocator` builds that C allocator and runs `f`
//! with it; the `ZigAllocatorRef` must outlive the `Allocator`, so it is held on
//! the stack for the duration of the closure. This mirrors the helper already
//! used in `allocator.rs`'s tests.

use rustty_lib::allocator::{Allocator as CAllocator, GlobalZigAllocator, VTable, ZigAllocatorRef};
use std::cell::Cell;
use std::ffi::c_void;

pub fn with_test_allocator<R>(f: impl FnOnce(&CAllocator) -> R) -> R {
    let g = GlobalZigAllocator;
    let zref = ZigAllocatorRef::new(&g);
    let calloc = CAllocator::from_zig(&zref);
    f(&calloc)
}

/// PORT NOTE (F-008/W12-01/W36-B06 proving tests, Rust-only glue): a counting
/// — optionally rejecting — CUSTOM C allocator, built directly on the C
/// `VTable` ABI exactly like an embedder would. It records every raw callback
/// (count + last length + outstanding bytes) so tests can prove:
///   * constructor/free lifecycle storage flows through the CALLER allocator
///     (F-008/W12-01: Ghostty `alloc.create`/`alloc.destroy`);
///   * a rejecting caller allocator yields recoverable `OUT_OF_MEMORY`
///     instead of silently succeeding on the Rust global heap;
///   * the zero-length typed-helper paths never reach the raw callbacks
///     (W36-B06).
///
/// The backing memory is the Rust global allocator; the vtable rebuilds the
/// `Layout` from the `(len, alignment)` the C ABI carries. `resize`/`remap`
/// are conservatively "cannot move/grow" so the byte accounting stays exact.
pub struct CountingCAllocator {
    pub alloc_calls: Cell<usize>,
    pub free_calls: Cell<usize>,
    pub last_alloc_len: Cell<Option<usize>>,
    pub last_free_len: Cell<Option<usize>>,
    /// Outstanding bytes handed out and not yet freed.
    pub live_bytes: Cell<usize>,
    /// When true, every alloc callback rejects (returns null).
    pub reject: Cell<bool>,
}

impl CountingCAllocator {
    pub fn new() -> CountingCAllocator {
        CountingCAllocator {
            alloc_calls: Cell::new(0),
            free_calls: Cell::new(0),
            last_alloc_len: Cell::new(None),
            last_free_len: Cell::new(None),
            live_bytes: Cell::new(0),
            reject: Cell::new(false),
        }
    }

    /// Build the C `Allocator` handle over this state. The returned value (and
    /// `self`) must stay pointer-stable for as long as any object created with
    /// it is alive — the same contract the C ABI imposes on embedders.
    pub fn c_allocator(&self) -> CAllocator {
        CAllocator {
            ctx: self as *const CountingCAllocator as *mut c_void,
            vtable: &COUNTING_VTABLE,
        }
    }
}

static COUNTING_VTABLE: VTable = VTable {
    alloc: counting_alloc,
    resize: counting_resize,
    remap: counting_remap,
    free: counting_free,
};

fn counting_layout(len: usize, alignment: u8) -> std::alloc::Layout {
    std::alloc::Layout::from_size_align(len, 1usize << alignment).unwrap()
}

extern "C" fn counting_alloc(ctx: *mut c_void, len: usize, alignment: u8, _ra: usize) -> *mut u8 {
    // SAFETY: `ctx` is the `CountingCAllocator` the test built the handle over.
    let s = unsafe { &*(ctx as *const CountingCAllocator) };
    s.alloc_calls.set(s.alloc_calls.get() + 1);
    s.last_alloc_len.set(Some(len));
    if s.reject.get() || len == 0 {
        // len == 0 is a raw-contract violation tripwire (the typed helpers
        // must short-circuit it, W36-B06): report failure so it is loud.
        return std::ptr::null_mut();
    }
    // SAFETY: `len > 0`, layout is valid.
    let ptr = unsafe { std::alloc::alloc(counting_layout(len, alignment)) };
    if !ptr.is_null() {
        s.live_bytes.set(s.live_bytes.get() + len);
    }
    ptr
}

extern "C" fn counting_resize(
    _ctx: *mut c_void,
    _memory: *mut u8,
    memory_len: usize,
    _alignment: u8,
    new_len: usize,
    _ra: usize,
) -> bool {
    // Only trivially succeed in place; keeps the byte accounting exact.
    new_len == memory_len
}

extern "C" fn counting_remap(
    _ctx: *mut c_void,
    _memory: *mut u8,
    _memory_len: usize,
    _alignment: u8,
    _new_len: usize,
    _ra: usize,
) -> *mut u8 {
    // Cannot move; callers fall back to alloc + copy + free (all counted).
    std::ptr::null_mut()
}

extern "C" fn counting_free(
    ctx: *mut c_void,
    memory: *mut u8,
    memory_len: usize,
    alignment: u8,
    _ra: usize,
) {
    // SAFETY: `ctx` is the `CountingCAllocator` the test built the handle over.
    let s = unsafe { &*(ctx as *const CountingCAllocator) };
    s.free_calls.set(s.free_calls.get() + 1);
    s.last_free_len.set(Some(memory_len));
    if memory_len == 0 {
        // Zero-length raw free: tripwire only (see counting_alloc).
        return;
    }
    s.live_bytes.set(s.live_bytes.get() - memory_len);
    // SAFETY: `(memory, memory_len, alignment)` round-trips the counting_alloc
    // layout per the Zig allocator contract.
    unsafe { std::alloc::dealloc(memory, counting_layout(memory_len, alignment)) };
}
