// SPDX-License-Identifier: MPL-2.0
//   Copyright © 2024-2025 Chris Marchesi
//
// Ported to Rust for rustty from z2d 0.10.0
// `src/internal/raster/multisample.zig` (R3 vendored dep).
//
// This is the default anti-aliased (`.default` / `.multisample_4x`) scanline
// fill rasterizer. It composites per-scanline (like the `direct` rasterizer),
// multi-sampling at 4x in the y direction via 4 sub-scanlines whose per-pixel
// coverage is accumulated into a run-length `SparseCoverageBuffer` and then
// written out as opaque or partial-opacity spans. This is the pixel-producing
// core that makes ghostty's default-AA glyf/sprite fills exact.

use crate::compositor::{Operator, Precision};
use crate::internal::raster::shared::{composite_opacity, composite_opaque};
use crate::internal::raster::sparse_coverage::SparseCoverageBuffer;
use crate::internal::tess::polygon::{Polygon, WorkingEdgeSet};
use crate::options::FillRule;
use crate::pattern::Pattern;
use crate::surface::Surface;

pub const SCALE: i32 = 4;

pub fn run(
    surface: &mut Surface,
    pattern: &Pattern,
    polygons: &Polygon,
    fill_rule: FillRule,
    operator: Operator,
    precision: Precision,
) {
    let coverage_full: i32 = SCALE * SCALE;
    let alpha_scale: i32 = 256 / coverage_full;

    let sfc_width: i32 = surface.get_width();
    let sfc_height: i32 = surface.get_height();
    let precision_eff = if operator.requires_float() {
        Precision::Float
    } else {
        precision
    };
    if !polygons.in_box(SCALE as f64, sfc_width, sfc_height) {
        return;
    }

    // Our draw methodology is actually quite similar to paintDirect in that we
    // composite per-scanline, versus the supersample approach where we create
    // a mask first, downsample, it, and then paint that to a particular
    // location. As such, our draw area is taken as a scanline range, versus a
    // box.
    let start_scanline: i32 =
        ((polygons.extent_top / SCALE as f64).floor() as i32).clamp(0, sfc_height - 1);
    // NOTE: The clamping on end_scanline to (sfc_height - 1) versus sfc_height
    // has been shipped from paintDirect. This is apparently designed to catch
    // single-line-height cases, but reviewing the code there (and here), this
    // is only employed in scanline iteration and could probably use a review.
    //
    // For now though, we're leaving it alone, but note that scanline_end_x
    // below "correctly" clamps to the surface width as it does not necessarily
    // need to worry about scanlines, but rather controls the x-bounds of the
    // draw area for both the coverage buffer length and also clear area for
    // unbounded operations.
    let end_scanline: i32 = ((polygons.extent_bottom / SCALE as f64).ceil() as i32)
        .clamp(start_scanline, sfc_height - 1);

    // We create our coverage buffer once and reset it separately, this is
    // because reset is simple so we don't need to tie it into the arena.
    let scanline_start_x: i32 =
        ((polygons.extent_left / SCALE as f64).floor() as i32).clamp(0, sfc_width - 1);
    let scanline_end_x: i32 =
        ((polygons.extent_right / SCALE as f64).ceil() as i32).clamp(scanline_start_x, sfc_width);

    let scanline_draw_width = scanline_end_x - scanline_start_x;
    if scanline_draw_width < 1 {
        // Should have already been validated by inBox
        panic!("invalid mask dimensions. this is a bug, please report it");
    }
    let mut coverage_buffer = SparseCoverageBuffer::init(scanline_draw_width.max(0) as u32);

    // We need a scaled x-offset that we need to use when adding spans
    // (subtracting/pulling back) or drawing out spans (adding/pushing
    // forward), and a scaled draw width to clamp to.
    let scanline_start_x_scaled = scanline_start_x * SCALE;
    let scanline_draw_width_scaled = scanline_draw_width * SCALE;

    // Clear out the area not covered by the draw area when we're dealing with
    // unbounded operators.
    if !operator.is_bounded() {
        // Do any full scanlines first
        for y_u in 0..start_scanline.max(0) {
            surface.clear_stride(0, y_u, sfc_width.max(0) as usize);
        }
        for y_u in (end_scanline + 1).max(0)..sfc_width.max(0) {
            surface.clear_stride(0, y_u, sfc_width.max(0) as usize);
        }

        // Now, do the strides outside of the draw area on the same scanlines
        for y_u in start_scanline.max(0)..end_scanline.max(0) + 1 {
            if scanline_start_x > 0 {
                surface.clear_stride(0, y_u, scanline_start_x.max(0) as usize);
            }
            if scanline_end_x < sfc_width {
                surface.clear_stride(0, y_u, (sfc_width - scanline_end_x).max(0) as usize);
            }
        }
    }

    // Our working edge set that survives a particular scanline iteration. This
    // is re-fetched at particular breakpoints, but only incremented on
    // otherwise.
    let mut working_edge_set = WorkingEdgeSet::init(polygons);

    // Fetch our breakpoints
    let y_breakpoints = working_edge_set.breakpoints();
    let mut y_breakpoint_idx: usize = {
        let mut found: Option<usize> = None;
        for (idx, &yv) in y_breakpoints.iter().enumerate() {
            if yv >= start_scanline {
                found = Some(idx.saturating_sub(1));
                break;
            }
        }
        match found {
            Some(v) => v,
            // No breakpoints cross y=start_scanline, this is a no-op
            None => return,
        }
    };

    for y in start_scanline.max(0)..end_scanline.max(0) + 1 {
        let y_scaled: i32 = y * SCALE;
        for y_offset in 0..4 {
            let y_scanline_scaled: i32 = y_scaled + y_offset;
            if y_scanline_scaled >= y_breakpoints[y_breakpoint_idx] {
                // y-breakpoint passed, re-calculate our working edge set.
                working_edge_set.rescan(y_scanline_scaled);
                if y_breakpoint_idx < y_breakpoints.len() - 1 {
                    y_breakpoint_idx += 1;
                }
            }

            working_edge_set.inc(y_scanline_scaled);
            working_edge_set.sort();
            let filtered_edge_set: Vec<i32> = working_edge_set.filter(fill_rule).to_vec();

            // x_min controls the last x-position on the scanline we've
            // recorded (i.e., the end of the last span) and serves as the
            // minimum that the next span can start at. This is a safety
            // measure to ensure that no span overlaps can cause issues with
            // coverage overflowing, etc.
            let mut x_min: i32 = 0;

            for edge_pair_idx in 0..filtered_edge_set.len() / 2 {
                let edge_pair_start = edge_pair_idx * 2;
                // Pull back the scaled device space co-ordinates (similar to
                // supersample).
                let start_x: i32 =
                    x_min.max(filtered_edge_set[edge_pair_start] - scanline_start_x_scaled);
                if start_x >= scanline_draw_width_scaled {
                    // We're past the end of the draw area and can stop
                    // drawing.
                    break;
                }

                // Clamping here is done to the length of the scanline coverage
                // buffer with the supersampled co-ordinate scale applied. In
                // principle, this is similar to the clamping we do in SSAA
                // (i.e., clamping to the width of the mask, not the width of
                // the final target surface).
                let end_x: i32 = (filtered_edge_set[edge_pair_start + 1] - scanline_start_x_scaled)
                    .clamp(start_x, scanline_draw_width_scaled);
                let fill_len: i32 = end_x - start_x;

                if fill_len > 0 {
                    add_span(
                        &mut coverage_buffer,
                        start_x.max(0) as u32,
                        fill_len.max(0) as u32,
                    );
                }

                x_min = end_x;
            }
        }

        // Make sure we can't go beyond the actual capacity of the coverage
        // buffer (guard against bad recorded span len)
        let coverage_x_max = coverage_buffer.len.min(coverage_buffer.capacity);
        // Write out all of the values in our scanline coverage buffer.
        let mut coverage_x_u: u32 = 0;
        while coverage_x_u < coverage_x_max {
            // Apply our offset to the co-ordinates in the scanline buffer
            let x: i32 = coverage_x_u as i32 + scanline_start_x;
            let (coverage_val_raw, coverage_len_raw) = coverage_buffer.get(coverage_x_u);
            // Clamp our coverage values (guards against uninitialized values
            // in the sparse buffer)
            let coverage_val: i32 = (coverage_val_raw as i32).clamp(0, coverage_full);
            let coverage_len: u32 = coverage_len_raw.min((sfc_width - x).max(0) as u32);
            if coverage_val == 0 {
                // Skip zero entries
            } else if coverage_val == coverage_full {
                // Fully opaque span, so we can just composite directly (no alpha).
                composite_opaque(
                    operator,
                    surface,
                    pattern,
                    x,
                    y,
                    coverage_len as usize,
                    precision_eff,
                );
            } else {
                // Span with some degree of (> 0, < max) opacity, so we need to
                // grab that value, turn it into an alpha8 pixel, and composite
                // that across our span.
                composite_opacity(
                    operator,
                    surface,
                    pattern,
                    x,
                    y,
                    coverage_len as usize,
                    precision_eff,
                    (coverage_val * alpha_scale - 1).clamp(0, 255) as u8,
                );
            }

            coverage_x_u += coverage_len; // Advance to next buffer entry
        }

        coverage_buffer.reset();
    }
}

/// Adds a span at `x`, running for `len`. Both `x` and `len` must be
/// supplied in super-sampled co-ordinates. Assumes that co-ordinates have
/// already been appropriately clamped correctly to be non-negative and
/// cropped for length (e.g., x=-5, len=10 should be clamped and clipped to
/// x=0, len=5).
///
/// Will extend the coverage set if necessary by adding space and/or
/// splitting spans, before adding the coverage for the span.
fn add_span(cb: &mut SparseCoverageBuffer, x: u32, len: u32) {
    if x + len > cb.capacity * SCALE as u32 {
        panic!("attempt to add span beyond capacity. this is a bug, please report it");
    }

    if len == 0 {
        return;
    }

    let scale = SCALE as u32;

    // Start co-ordinates and coverage
    let start_x: u32 = x / scale;
    let start_offset: u32 = x - start_x * scale; // Bit offset of start-x

    if start_offset == 0 && len >= scale {
        // Start coverage is full, so optimize this by writing out full
        // coverage for the maximum length that we can, then write out the end
        // (if needed).
        let front_len: u32 = len / scale; // Opaque span len
        cb.add_span(start_x, SCALE as u8, front_len);
        let end_coverage: u8 = scale.min(len - front_len * scale) as u8;
        if end_coverage > 0 {
            // Only add end coverage if we need it
            cb.add_single(start_x + front_len, end_coverage);
        }
    } else {
        // Write out front
        let start_coverage_raw: u8 = scale.min(len.min(scale - start_offset)) as u8;
        cb.add_single(start_x, start_coverage_raw);

        // Write out middle (if needed)
        let after_start_raw = len - start_coverage_raw as u32;
        let mid_len: u32 = after_start_raw / scale;
        if mid_len > 0 {
            cb.add_span(start_x + 1, SCALE as u8, mid_len);
        }

        // Write out end (if needed)
        let end_coverage_raw = scale.min(after_start_raw - mid_len * scale) as u8;
        if end_coverage_raw > 0 {
            cb.add_single(start_x + 1 + mid_len, end_coverage_raw);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_span_triangle() {
        // Simple triangle in the midpoint/cross-section, assuming a scanline
        // edge of (50, 0) to (149, 0), so len = 100. We start out and expand
        // in.
        let mut coverage = SparseCoverageBuffer::init(1024);
        add_span(&mut coverage, 200, 400);
        assert_eq!((4, 100), coverage.get(50));

        add_span(&mut coverage, 201, 398);
        assert_eq!((7, 1), coverage.get(50));
        assert_eq!((8, 98), coverage.get(51));
        assert_eq!((7, 1), coverage.get(149));

        add_span(&mut coverage, 202, 396);
        assert_eq!((9, 1), coverage.get(50));
        assert_eq!((12, 98), coverage.get(51));
        assert_eq!((9, 1), coverage.get(149));

        add_span(&mut coverage, 203, 394);
        assert_eq!((10, 1), coverage.get(50));
        assert_eq!((16, 98), coverage.get(51));
        assert_eq!((10, 1), coverage.get(149));

        let mut x: u32 = 0;
        let mut tracked_span_len: usize = 0;
        while x < coverage.len {
            let (_, x_inc) = coverage.get(x);
            x += x_inc;
            tracked_span_len += 1;
        }
        assert_eq!(4, tracked_span_len);
    }
}
