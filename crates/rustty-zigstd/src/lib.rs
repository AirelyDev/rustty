//! rustty-zigstd
//!
//! PORTS: Zig stdlib types AS THE PORT USES THEM — std.heap.ArenaAllocator,
//! std.heap.MemoryPool / MemoryPoolAligned, std.array_hash_map.AutoArrayHashMap,
//! std.Thread.Mutex, std.hash.Wyhash, std.fs.path.dirnamePosix.
//!
//! PORT STATUS: done (infra; Zig stdlib equivalents; not in the 3234 contract)
//!
//! These types are NOT in ghostty src/, so they are NOT part of the 3234-test
//! parity contract; they are ADDITIVE INFRASTRUCTURE GLUE (R1 allows necessary
//! additions). The faithfulness target is to reproduce the EXACT observable
//! SEMANTICS of the Zig stdlib types as ghostty uses them (e.g. PageList.zig):
//!
//!   * arena_allocator — `ArenaAllocator` with reset modes `.free_all` /
//!     `.retain_capacity` / `.retain_with_limit(n)`.
//!   * memory_pool — `MemoryPool` (LIFO free-list reuse, `create`/`destroy`)
//!     and `MemoryPoolAligned` (page-aligned items).
//!   * array_hash_map — `AutoArrayHashMap` with INSERTION-ORDER iteration and
//!     `getOrPut`/`put`/`swapRemove`/`orderedRemove`/`count`/`keys`/`values`.
//!   * thread — `std.Thread.Mutex`: the GUARD-LESS manual lock/try_lock/unlock
//!     mutex (no RAII guard, no poisoning) the core-tier mutex model requires
//!     (CORE_BLUEPRINT §3; renderer `State.mutex`); and `std.Thread.Semaphore`,
//!     the counting semaphore `generic.zig`'s `SwapChain` uses for triple-buffer
//!     GPU-frame backpressure.
//!   * wyhash — `std.hash.Wyhash`, the hash `src/terminal/hash_map.zig`'s
//!     `AutoContext` uses (`Wyhash.hash(0, asBytes(&key))`). Bit-exact, because
//!     it decides the physical slot/fingerprint layout inside a page's backing
//!     memory and therefore the compressed page bytes.
//!
//! Backing memory is the Rust system allocator (`std::alloc`), the faithful map
//! of Zig's `gpa`/`page_allocator`. This crate is unsafe-heavy; its UB/leak gate
//! is Miri (R6) — `check_leaks.sh --crate rustty-zigstd`.
//!
//! This crate is a leaf over rustty-util (for `quirks::inlineAssert`). It has NO
//! dependency on the ghostty tree (R3).

pub mod arena_allocator;
pub mod array_hash_map;
pub mod fmt;
pub mod fs_path;
pub mod memory_pool;
pub mod thread;
pub mod wyhash;

// TEST-ONLY leak-counting allocator (scoped faithful equivalent of Zig's
// `std.testing.allocator`). Compiled under `cfg(test)` for this crate's own unit
// tests, and under the `tracking` feature so downstream crates (rustty-vt's
// PageList tests) can wire it onto the page pool where Miri times out (R6).
#[cfg(any(test, feature = "tracking"))]
pub mod tracking_allocator;
