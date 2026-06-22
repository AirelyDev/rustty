//! Port of `std.heap.arena_allocator` (Zig 0.15.2) — ADDITIVE INFRA GLUE.
//!
//! PORT STATUS: infra stub (skeleton)
//!
//! Faithful target: `ArenaAllocator` semantics as the port uses them, including
//! the `reset` modes `.free_all` / `.retain_capacity` / `.retain_with_limit(n)`.
//! Backing memory is the Rust system allocator (`std::alloc`) — the faithful map
//! of Zig's child/page allocator. Implementation lands in a follow-up; this is
//! the module skeleton so the crate compiles and the workspace stays acyclic.
