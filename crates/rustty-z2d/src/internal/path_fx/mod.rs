// SPDX-License-Identifier: MPL-2.0
//   Copyright © 2024-2026 Chris Marchesi
//
// Ported to Rust for rustty from z2d 0.12.1 `src/internal/path_fx/` (R3
// vendored dep).
//
// SUBSET NOTE (rustty, P3-W8 T3): the rest of `crates/rustty-z2d` targets z2d
// **0.10.0**; this package is the one 0.12.x feature back-ported into it,
// because ghostty `d0e72a3ab` rewrites `src/font/sprite/canvas.zig`'s
// `innerStrokePath` in terms of `Path.offset`, which does not exist in 0.10.0.
//
// Deliberately NOT ported from `path_fx/`:
//
//   * `simplify.zig` (and with it `InputSet.Contour.computeIntersections`,
//     `Segment.intersectionFor`, `Segment.insertIntersection`,
//     `Segment.unshiftIntersection` and the `Intersection` record). Nothing in
//     ghostty calls `Path.simplify`; porting it would land ~400 lines that no
//     rustty caller reaches, which `-D warnings` would then flag as dead code.
//     `offset.zig`'s own doc comment tells callers to run `simplify` first on
//     self-intersecting input; rustty's only caller (`innerStrokePath`) feeds
//     it simple polygons, exactly as ghostty's does.
//   * `shared.zig`'s `dumpNodes` (upstream marks it "For testing only. TODO:
//     remove eventually.").

pub mod input_set;
pub mod offset;
pub mod output_set;
pub mod shared;
