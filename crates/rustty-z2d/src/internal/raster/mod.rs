// SPDX-License-Identifier: MPL-2.0
//   Copyright © 2024-2025 Chris Marchesi
//
// Rasterizer internals for the rustty z2d subset port. The non-AA (`direct`)
// scanline rasterizer and the anti-aliased multi-sample (`multisample`, backed
// by the run-length `sparse_coverage` buffer) rasterizer are ported — the
// latter is what ghostty's default-AA glyf/sprite fills use. The `supersample`
// and `hairline` rasterizers are blockers.

pub mod direct;
pub mod multisample;
pub mod shared;
pub mod sparse_coverage;
