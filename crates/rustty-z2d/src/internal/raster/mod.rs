// SPDX-License-Identifier: MPL-2.0
//   Copyright © 2024-2025 Chris Marchesi
//
// Rasterizer internals for the rustty z2d subset port. Only the non-AA
// (`direct`) scanline rasterizer is ported in this subset; the anti-aliased
// (`supersample`/`multisample`) and `hairline` rasterizers are blockers.

pub mod direct;
pub mod shared;
