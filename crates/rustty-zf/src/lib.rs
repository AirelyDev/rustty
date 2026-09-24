//! rustty-zf — pure-Rust port of the vendored `zf` 0.11.0 fuzzy finder (R3).
//!
//! PORTS: natecraddock/zf 0.11.0 (0.10.3 until parity tail S-10b, NON_PORTS.md §G2) `src/zf/{zf,filter,clib}.zig` — the fuzzy-match
//! ranking `cli/list_themes.zig` sorts the theme picker with (`zf.rank`). A
//! DEPENDENCY package of ghostty, NOT ghostty source; the vendored-dep-port
//! pattern of `rustty-z2d` / `rustty-wuffs`: a PURE-RUST LEAF, no `rustty-*` deps.
//!
//! Source of truth (READ-ONLY oracle): the zig-cache dep dir
//!   `…/zf-0.11.0-OIRy8X-RAAAwaRXHMYpj2uvBnuGTZWEE_3V7acqHQNtW/src/zf/{zf,filter,clib}.zig`.
//!
//! R1 fidelity: every line / comment / `test {}` ported assertion-for-assertion.
//! The rank algorithm is BIT-FAITHFUL — `list_themes` sorts on the exact `f64`
//! and the exact match / no-match (`?f64 == null`) decision, so `rank()`
//! reproduces zf's values. Zig byte-slices (`[]const u8`) map to `&[u8]`, NEVER
//! lossy `String` (zf matches over raw bytes; `std.ascii.toLower` is ASCII-only).
//!
//! PORT STATUS: done — zf.zig/filter.zig/clib.zig every line/comment/test{} (R1);
//! all 9 named `test {}` ported assertion-for-assertion (exact rank match/no-match
//! decisions + the pinned highlight index arrays); the anonymous refAllDecls
//! `test {}` is the `ref_all_decls` compile-touch below.

// The Zig import graph is `zf → {filter, clib}`, `clib → filter`, so `mod` order
// is filter (leaf), then clib, then zf. `filter` / `clib` stay crate-private; the
// public API is the `zf::*` surface (list_themes only needs `rank` / `highlight`).
mod clib;
mod filter;
pub mod zf;

// Mirrors zf.zig's anonymous `test { _ = @import("clib.zig"); _ = @import("filter.zig"); }`
// refAllDecls compile-touch (skipped by the count gate; keeps rust ≥ zig).
#[cfg(test)]
#[test]
fn ref_all_decls() {
    // A compile-touch: the crate builds `filter` / `clib` / `zf` as part of the
    // test binary, which is exactly what the Zig `_ = @import(...)` achieves.
}
