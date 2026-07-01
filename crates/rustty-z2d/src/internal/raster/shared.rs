// SPDX-License-Identifier: MPL-2.0
//   Copyright © 2024-2025 Chris Marchesi
//
// Ported to Rust for rustty from z2d 0.10.0 `src/internal/raster/shared.zig`
// (R3 vendored dep).
//
// SUBSET NOTE (rustty): the non-fast-path branches (where the operator does not
// reduce to a direct source/opacity write and the full `StrideCompositor` is
// invoked) are NOT part of the ghostty fill subset — ghostty fills/strokes with
// opaque `alpha8`, which always fast-paths through `paint_stride`. Those
// branches are `unimplemented!` and never reached in the subset.

use crate::compositor::{Operator, Precision};
use crate::pattern::Pattern;
use crate::pixel::Pixel;
use crate::surface::Surface;

/// Composites opaque pixels, fast-pathing if the operator reduces to the source
/// pixel.
#[allow(clippy::too_many_arguments)]
pub fn composite_opaque(
    operator: Operator,
    surface: &mut Surface,
    pattern: &Pattern,
    x: i32,
    y: i32,
    len: usize,
    _precision: Precision,
) {
    if operator == Operator::Clear {
        surface.clear_stride(x, y, len);
    } else {
        let Pattern::OpaquePattern(op) = pattern;
        if fill_reduces_to_source(operator, op.pixel) {
            surface.paint_stride(x, y, len, op.pixel);
        } else {
            unimplemented!(
                "composite_opaque non-fast-path requires the full StrideCompositor (not in the z2d subset)"
            );
        }
    }
}

/// Returns true if the operator can be fast-pathed on the source by writing the
/// source pixel directly to the surface.
///
/// Note that all operators that can be fast-pathed are also integer pipeline
/// operations.
fn fill_reduces_to_source(op: Operator, px: Pixel) -> bool {
    match op {
        Operator::Src => true,
        Operator::SrcOver => px.is_opaque(),
        _ => false,
    }
}
