//! rustty-zigstd
//!
//! PORTS: Zig stdlib types AS THE PORT USES THEM — std.heap.ArenaAllocator,
//! std.heap.MemoryPool / MemoryPoolAligned, std.array_hash_map.AutoArrayHashMap.
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
//!
//! Backing memory is the Rust system allocator (`std::alloc`), the faithful map
//! of Zig's `gpa`/`page_allocator`. This crate is unsafe-heavy; its UB/leak gate
//! is Miri (R6) — `check_leaks.sh --crate rustty-zigstd`.
//!
//! This crate is a leaf over rustty-util (for `quirks::inlineAssert`). It has NO
//! dependency on the ghostty tree (R3).

pub mod arena_allocator;
pub mod array_hash_map;
pub mod memory_pool;
