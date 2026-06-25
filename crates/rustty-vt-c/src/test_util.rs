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

use rustty_lib::allocator::{Allocator as CAllocator, GlobalZigAllocator, ZigAllocatorRef};

pub fn with_test_allocator<R>(f: impl FnOnce(&CAllocator) -> R) -> R {
    let g = GlobalZigAllocator;
    let zref = ZigAllocatorRef::new(&g);
    let calloc = CAllocator::from_zig(&zref);
    f(&calloc)
}
