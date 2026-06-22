//! Port of `std.array_hash_map` (Zig 0.15.2) — ADDITIVE INFRA GLUE.
//!
//! PORT STATUS: infra stub (skeleton)
//!
//! Faithful target: `AutoArrayHashMap` semantics as the port uses them —
//! INSERTION-ORDER iteration plus `getOrPut`/`put`/`swapRemove`/`orderedRemove`/
//! `count`/`keys`/`values`. Backing memory is the Rust system allocator
//! (`std::alloc`). Implementation lands in a follow-up; this is the module
//! skeleton so the crate compiles and the workspace stays acyclic.
