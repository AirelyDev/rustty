// SPDX-License-Identifier: MPL-2.0
//   Copyright © 2024-2025 Chris Marchesi
//
// Ported to Rust for rustty from z2d 0.10.0 `src/internal/raster/shared.zig`
// (R3 vendored dep).
//
// SUBSET NOTE (rustty): the non-fast-path branches (where the operator does not
// reduce to a direct source/opacity write) are reached by ghostty's *shaded*
// sprite fills, which fill with a TRANSLUCENT `alpha8` source (e.g. the corner
// triangle / block shades) and so cannot fast-path through `paint_stride`. The
// reference routes those through the full SIMD `StrideCompositor`; the scalar
// equivalent — `Surface::composite_stride` (RGBA `dst_in` premultiply by the
// span opacity, then a per-pixel `run_pixel_t(dst, src, operator)` composite) —
// already reproduces that pipeline exactly, so the non-fast-path branches
// delegate to it. The `getStride`/`pxLen() == 0` guard is subsumed by
// `composite_stride`'s own bounds check. Only `OpaquePattern` (pixel) sources
// exist in this subset; gradient/dither pattern sources are not ported.

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
            // Non-fast-path: `dst = operator(dst, src_pixel)` per pixel. The
            // reference runs the single-op `StrideCompositor` (`{operator, src:
            // pixel}`); the scalar equivalent is `composite_stride` at full
            // opacity (255) — no premultiply, then `run_pixel_t(dst, src,
            // operator)` across the span.
            surface.composite_stride(x, y, len, op.pixel, operator, 255);
        }
    }
}

/// Composites pixels with opacity, fast-pathing if the operator reduces to the
/// source pixel.
//
// PORT NOTE: faithful port of `compositeOpacity`. In the ghostty sprite/glyf
// fill subset the pattern is always an opaque `alpha8` source and the operator
// is `src_over`, which reduces to the source, so this always takes the
// `composite_stride` fast path. The non-fast-path (full `StrideCompositor`
// dst_in/operator two-op pipeline) is not part of this subset.
#[allow(clippy::too_many_arguments)]
pub fn composite_opacity(
    operator: Operator,
    surface: &mut Surface,
    pattern: &Pattern,
    x: i32,
    y: i32,
    len: usize,
    _precision: Precision,
    opacity: u8,
) {
    if operator == Operator::Clear {
        surface.clear_stride(x, y, len);
    } else {
        let Pattern::OpaquePattern(op) = pattern;
        if fill_reduces_to_source(operator, op.pixel) {
            surface.composite_stride(x, y, len, op.pixel, operator, opacity);
        } else {
            // Non-fast-path: premultiply the source by `opacity` (`dst_in`
            // against an alpha8 mask) then composite with `operator`. The
            // reference runs the two-op `StrideCompositor` pipeline;
            // `composite_stride` performs exactly that scalar sequence (DstIn
            // premultiply when opacity < 255, then the operator composite).
            surface.composite_stride(x, y, len, op.pixel, operator, opacity);
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
