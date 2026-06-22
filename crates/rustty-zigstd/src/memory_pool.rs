//! Port of `std.heap.memory_pool` (Zig 0.15.2) — ADDITIVE INFRA GLUE.
//!
//! PORT STATUS: infra stub (skeleton)
//!
//! Faithful target: `MemoryPool` semantics (LIFO free-list reuse via
//! `create`/`destroy`) and `MemoryPoolAligned` (page-aligned items) as the port
//! uses them. Backing memory is the Rust system allocator (`std::alloc`) — the
//! faithful map of Zig's child allocator. Implementation lands in a follow-up;
//! this is the module skeleton so the crate compiles and stays acyclic.
