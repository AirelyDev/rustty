// SPDX-License-Identifier: MPL-2.0
//   Copyright © 2024-2025 Chris Marchesi
//
// Ported to Rust for rustty from z2d 0.10.0
// `src/internal/raster/supersample.zig` (R3 vendored dep).
//
// This is the supersample (`.supersample_4x`) anti-aliased fill/stroke
// rasterizer. Unlike `multisample` (which composites per-scanline), it draws
// the geometry into a scaled-up opaque mask surface, downsamples that mask with
// simple pixel-averaging (producing per-pixel alpha coverage), and then
// composites the pattern — masked by that coverage — onto the target.
//
// PORT NOTE — two faithful, scoped divergences (R5-safe):
//
//   * Mask type: the reference picks the target's own type when it is one of
//     the sub-8-bit packed alpha surfaces (alpha4/2/1), else `Alpha8`. With
//     workstream A those packed surfaces are real, reachable `Surface` variants,
//     so that arm is ported faithfully (it is unreachable via the font-sprite
//     consumer, which always targets `alpha8`, but the type switch is exact).
//
//   * Final composite: the reference calls the multi-step SIMD
//     `compositor.SurfaceCompositor.run(surface, comp_x, comp_y, 2, {dst_in ↦
//     mask, then operator}, precision)`. That vectorized N-step compositor is
//     not part of the rustty subset (only the scalar integer single-pixel core
//     + `shared::composite_opaque`/`composite_opacity` are ported). We reproduce
//     the identical 2-step pipeline per-pixel over the downsampled-mask box: for
//     each mask pixel read its coverage alpha, then `dst = operator(dst, dst_in(
//     pattern_pixel, coverage))` — which is exactly what `composite_opacity`
//     (coverage < full) / `composite_opaque` (coverage == full) compute, the
//     same way `multisample` already reproduces the identical dst_in→operator
//     pipeline per-stride. rustty `Pattern` has only `OpaquePattern`
//     (gradient/dither pattern sources are out of the ghostty subset), so the
//     `pattern.*` switch collapses to the opaque arm. Faithful geometry +
//     scalar-integer composite, no SIMD, no gradient/dither.

use crate::compositor::{Operator, Precision};
use crate::internal::raster::shared::{composite_opacity, composite_opaque};
use crate::internal::tess::polygon::{Polygon, WorkingEdgeSet};
use crate::options::FillRule;
use crate::pattern::Pattern;
use crate::pixel::{Alpha1, Alpha2, Alpha4, Alpha8, Pixel};
use crate::surface::{Surface, SurfaceType};

#[allow(clippy::too_many_arguments)]
pub fn run(
    surface: &mut Surface,
    pattern: &Pattern,
    polygons: &Polygon,
    fill_rule: FillRule,
    scale: f64,
    operator: Operator,
    precision: Precision,
) {
    // Do an initial check to see if our polygon is within the surface, if it
    // isn't, it's a no-op.
    //
    // This also enforces positive and non-zero surface dimensions, and
    // correctly defined polygon extents (e.g., that the end extents are
    // greater than the start extents).
    if !polygons.in_box(scale, surface.get_width(), surface.get_height()) {
        return;
    }

    // This math expects integer scaling.
    debug_assert!(scale.floor() == scale);
    let i_scale: i32 = scale as i32;

    // This is the area on the original image which our polygons may touch (in
    // the event our operator is bounded, otherwise it's the whole of the
    // surface).
    //
    // This range has to accommodate the extents of any possible point in the
    // polygon rectangle, so it needs to be "pushed out"; floored on the
    // top/left, and ceilinged on the bottom/right.
    let bounded = operator.is_bounded();
    let x0: i32 = if bounded {
        (polygons.extent_left / scale).floor() as i32
    } else {
        0
    };
    let y0: i32 = if bounded {
        (polygons.extent_top / scale).floor() as i32
    } else {
        0
    };
    let x1: i32 = if bounded {
        (polygons.extent_right / scale).ceil() as i32
    } else {
        surface.get_width()
    };
    let y1: i32 = if bounded {
        (polygons.extent_bottom / scale).ceil() as i32
    } else {
        surface.get_height()
    };

    let mut mask_sfc = {
        // We calculate a scaled up version of the extents for our supersampled
        // drawing.
        //
        // These dimensions are clamped to the target surface to avoid edge
        // cases and unnecessary work.
        let target_width_scaled: i32 = surface.get_width() * i_scale;
        let target_height_scaled: i32 = surface.get_height() * i_scale;
        let box_x0: i32 = (x0 * i_scale).clamp(0, target_width_scaled - 1);
        let box_y0: i32 = (y0 * i_scale).clamp(0, target_height_scaled - 1);
        let box_x1: i32 = (x1 * i_scale).clamp(box_x0, target_width_scaled - 1);
        let box_y1: i32 = (y1 * i_scale).clamp(box_y0, target_height_scaled - 1);
        let mask_width: i32 = (box_x1 + 1) - box_x0;
        let mask_height: i32 = (box_y1 + 1) - box_y0;

        if mask_width < 1 || mask_height < 1 {
            // This should have been checked earlier, if we hit this, it's a bug.
            panic!("invalid mask dimensions. this is a bug, please report it");
        }

        // Check our surface type. If we are one of our < 8bpp alpha surfaces,
        // we use that type instead.
        let surface_type: SurfaceType = match surface {
            Surface::ImageSurfaceAlpha4(_) => SurfaceType::ImageSurfaceAlpha4,
            Surface::ImageSurfaceAlpha2(_) => SurfaceType::ImageSurfaceAlpha2,
            Surface::ImageSurfaceAlpha1(_) => SurfaceType::ImageSurfaceAlpha1,
            _ => SurfaceType::ImageSurfaceAlpha8,
        };
        let opaque_px: Pixel = match surface_type {
            SurfaceType::ImageSurfaceAlpha4 => Alpha4::OPAQUE.as_pixel(),
            SurfaceType::ImageSurfaceAlpha2 => Alpha2::OPAQUE.as_pixel(),
            SurfaceType::ImageSurfaceAlpha1 => Alpha1::OPAQUE.as_pixel(),
            _ => Alpha8::OPAQUE.as_pixel(),
        };
        // Mask dimensions are validated >= 1 above, so `init` cannot fail.
        let mut mask_sfc_scaled = Surface::init(surface_type, mask_width, mask_height)
            .expect("mask dimensions validated");

        // Our working edge set that survives a particular scanline iteration.
        // This is re-fetched at particular breakpoints, but only incremented on
        // otherwise.
        let mut working_edge_set = WorkingEdgeSet::init(polygons);

        // Fetch our breakpoints
        let y_breakpoints = working_edge_set.breakpoints();
        let mut y_breakpoint_idx: usize = {
            let mut found: Option<usize> = None;
            for (idx, &yv) in y_breakpoints.iter().enumerate() {
                if yv >= box_y0 {
                    found = Some(idx.saturating_sub(1));
                    break;
                }
            }
            match found {
                Some(v) => v,
                // No breakpoints cross y=box_y0, this is a no-op
                None => return,
            }
        };

        for y_u in 0..mask_height.max(0) {
            let y: i32 = y_u;
            let dev_y: i32 = y + box_y0; // Device-space y-coordinate (still supersampled)

            if dev_y >= y_breakpoints[y_breakpoint_idx] {
                // y-breakpoint passed, re-calculate our working edge set.
                working_edge_set.rescan(dev_y);
                if y_breakpoint_idx < y_breakpoints.len() - 1 {
                    y_breakpoint_idx += 1;
                }
            }

            working_edge_set.inc(dev_y);
            working_edge_set.sort();
            let filtered_edge_set: Vec<i32> = working_edge_set.filter(fill_rule).to_vec();

            for edge_pair_idx in 0..filtered_edge_set.len() / 2 {
                // Inverse to the above; pull back our scaled device space
                // co-ordinates to mask space.
                let edge_pair_start = edge_pair_idx * 2;
                let start_x: i32 = (filtered_edge_set[edge_pair_start] - box_x0).max(0);
                if start_x >= mask_width {
                    // We're past the mask draw area and can stop drawing.
                    break;
                }
                let end_x: i32 =
                    (filtered_edge_set[edge_pair_start + 1] - box_x0).clamp(start_x, mask_width);

                let fill_len: i32 = end_x - start_x;
                if fill_len > 0 {
                    mask_sfc_scaled.paint_stride(start_x, y, fill_len.max(0) as usize, opaque_px);
                }
            }
        }

        mask_sfc_scaled.downsample();
        mask_sfc_scaled
    };

    // We only bother clamping here on the low end since we've clipped
    // upper-left overlaps at (0,0). Offsets out of bounds of the surface should
    // have been filtered by the polygon/surface check at the start of the
    // function (and the compositor will ignore out-of-surface offsets too).
    let comp_x: i32 = x0.max(0);
    let comp_y: i32 = y0.max(0);

    // Reproduce `SurfaceCompositor.run(surface, comp_x, comp_y, 2, {dst_in ↦
    // mask, operator}, precision)` per-pixel over the downsampled-mask box: read
    // each mask pixel's coverage alpha (as its `Alpha8`-scaled value, so `full`
    // is 255 regardless of the mask's packing) and composite the (opaque)
    // pattern masked by that coverage via the same scalar dst_in→operator
    // pipeline `multisample` uses. Coverage 0 still routes through
    // `composite_opacity(.., 0)` so that unbounded operators clear the pixel,
    // matching the reference compositor (rather than the `multisample`
    // per-scanline "skip zero" optimization, which relies on a separate
    // pre-clear pass).
    let mask_w: i32 = mask_sfc.get_width();
    let mask_h: i32 = mask_sfc.get_height();
    let full: u8 = 255;
    for my in 0..mask_h.max(0) {
        for mx in 0..mask_w.max(0) {
            let dx = comp_x + mx;
            let dy = comp_y + my;
            // In-bounds by construction (mx,my within the mask dimensions).
            let cov: u8 = match mask_sfc.get_pixel(mx, my) {
                Some(px) => Alpha8::from_pixel(px).a,
                None => 0,
            };
            if cov == full {
                composite_opaque(operator, surface, pattern, dx, dy, 1, precision);
            } else {
                composite_opacity(operator, surface, pattern, dx, dy, 1, precision, cov);
            }
        }
    }

    mask_sfc.deinit();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::internal::tess::fill_plotter;
    use crate::internal::path_nodes::{PathClose, PathLineTo, PathMoveTo, PathNode};
    use crate::internal::point::Point;
    use crate::internal::raster::multisample;
    use crate::pattern::OpaquePattern;
    use crate::pixel::Rgb;
    use crate::surface::SUPERSAMPLE_SCALE;

    fn opaque_white() -> Pattern {
        Pattern::OpaquePattern(OpaquePattern {
            pixel: Pixel::Rgb(Rgb {
                r: 0xFF,
                g: 0xFF,
                b: 0xFF,
            }),
        })
    }

    fn rect_nodes(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<PathNode> {
        vec![
            PathNode::MoveTo(PathMoveTo {
                point: Point { x: x0, y: y0 },
            }),
            PathNode::LineTo(PathLineTo {
                point: Point { x: x1, y: y0 },
            }),
            PathNode::LineTo(PathLineTo {
                point: Point { x: x1, y: y1 },
            }),
            PathNode::LineTo(PathLineTo {
                point: Point { x: x0, y: y1 },
            }),
            PathNode::ClosePath(PathClose {}),
            PathNode::MoveTo(PathMoveTo {
                point: Point { x: x0, y: y0 },
            }),
        ]
    }

    fn alpha_at(sfc: &Surface, x: i32, y: i32) -> u8 {
        match sfc.get_pixel(x, y) {
            Some(px) => Alpha8::from_pixel(px).a,
            None => 0,
        }
    }

    #[test]
    fn supersample_fills_a_solid_rect_fully_opaque() {
        // A rectangle whose edges land exactly on pixel boundaries downsamples
        // to full coverage in its interior: the supersample mask is fully
        // opaque there, so the composited alpha is 0xFF.
        let scale = SUPERSAMPLE_SCALE as f64;
        let nodes = rect_nodes(1.0, 1.0, 5.0, 5.0);
        let polygons = fill_plotter::plot(&nodes, scale, 0.01).unwrap();

        let mut sfc = Surface::init(SurfaceType::ImageSurfaceAlpha8, 8, 8).unwrap();
        run(
            &mut sfc,
            &opaque_white(),
            &polygons,
            FillRule::NonZero,
            scale,
            Operator::SrcOver,
            Precision::Integer,
        );

        // Interior pixels fully covered.
        for y in 2..4 {
            for x in 2..4 {
                assert_eq!(0xFF, alpha_at(&sfc, x, y), "interior ({x},{y})");
            }
        }
        // Well outside the shape: untouched.
        assert_eq!(0x00, alpha_at(&sfc, 7, 7), "far corner");
        sfc.deinit();
    }

    #[test]
    fn supersample_vs_multisample_interior_parity() {
        // Both anti-aliased rasterizers agree on fully-covered interior pixels
        // for an axis-aligned, boundary-aligned rectangle (both must produce
        // 0xFF opaque coverage there).
        let s_scale = SUPERSAMPLE_SCALE as f64;
        let m_scale = multisample::SCALE as f64;
        let nodes = rect_nodes(2.0, 2.0, 10.0, 10.0);

        let ss_poly = fill_plotter::plot(&nodes, s_scale, 0.01).unwrap();
        let mut ss = Surface::init(SurfaceType::ImageSurfaceAlpha8, 16, 16).unwrap();
        run(
            &mut ss,
            &opaque_white(),
            &ss_poly,
            FillRule::NonZero,
            s_scale,
            Operator::SrcOver,
            Precision::Integer,
        );

        let ms_poly = fill_plotter::plot(&nodes, m_scale, 0.01).unwrap();
        let mut ms = Surface::init(SurfaceType::ImageSurfaceAlpha8, 16, 16).unwrap();
        multisample::run(
            &mut ms,
            &opaque_white(),
            &ms_poly,
            FillRule::NonZero,
            Operator::SrcOver,
            Precision::Integer,
        );

        for y in 3..9 {
            for x in 3..9 {
                assert_eq!(
                    alpha_at(&ms, x, y),
                    alpha_at(&ss, x, y),
                    "interior parity at ({x},{y})"
                );
                assert_eq!(0xFF, alpha_at(&ss, x, y), "interior opaque at ({x},{y})");
            }
        }
        ss.deinit();
        ms.deinit();
    }
}
