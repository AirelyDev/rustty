// SPDX-License-Identifier: MPL-2.0
//   Copyright © 2024-2025 Chris Marchesi
//
// Ported to Rust for rustty from z2d 0.10.0 `src/internal/raster/direct.zig`
// (R3 vendored dep). This is the non-anti-aliased (`.none`) scanline fill
// rasterizer.

use crate::compositor::{Operator, Precision};
use crate::internal::raster::shared::composite_opaque;
use crate::internal::tess::polygon::{Polygon, WorkingEdgeSet};
use crate::options::FillRule;
use crate::pattern::Pattern;
use crate::surface::Surface;

pub fn run(
    surface: &mut Surface,
    pattern: &Pattern,
    polygons: &Polygon,
    fill_rule: FillRule,
    operator: Operator,
    precision: Precision,
) {
    let sfc_width: i32 = surface.get_width();
    let sfc_height: i32 = surface.get_height();
    let precision_eff = if operator.requires_float() {
        Precision::Float
    } else {
        precision
    };
    let bounded = operator.is_bounded();

    // Do an initial check to see if our polygon is within the surface, if it
    // isn't, it's a no-op.
    if !polygons.in_box(1.0, sfc_width, sfc_height) {
        return;
    }

    // This is the scanline range on the original image which our polygons may
    // touch (in the event our operator is bounded, otherwise it's the whole of
    // the surface).
    let poly_start_y: i32 = if bounded {
        polygons.extent_top.floor() as i32
    } else {
        0
    };
    let poly_end_y: i32 = if bounded {
        polygons.extent_bottom.ceil() as i32
    } else {
        sfc_height - 1
    };
    // Clamp the scanlines to the surface
    let start_scanline: i32 = poly_start_y.clamp(0, sfc_height - 1);
    let end_scanline: i32 = poly_end_y.clamp(start_scanline, sfc_height - 1);

    // Our working edge set that survives a particular scanline iteration.
    let mut working_edge_set = WorkingEdgeSet::init(polygons);

    // Fetch our breakpoints
    let y_breakpoints = working_edge_set.breakpoints();
    let mut y_breakpoint_idx: usize = {
        // Seek to our starting breakpoint, given a possible clamping of y=0
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

    // Note that we have to add 1 to the end scanline here as our start -> end
    // boundaries above only account for+clamp to the last line to be scanned, so
    // our len is end + 1.
    for y in start_scanline.max(0)..(end_scanline.max(0) + 1) {
        if y >= y_breakpoints[y_breakpoint_idx] {
            // y-breakpoint passed, re-calculate our working edge set.
            working_edge_set.rescan(y);
            if y_breakpoint_idx < y_breakpoints.len() - 1 {
                y_breakpoint_idx += 1;
            }
        }

        working_edge_set.inc(y);
        working_edge_set.sort();
        let filtered_edge_set: Vec<i32> = working_edge_set.filter(fill_rule).to_vec();

        if !bounded && filtered_edge_set.is_empty() {
            // Empty line but we're not bounded, so we clear the whole line.
            surface.clear_stride(0, y, sfc_width.max(0) as usize);
            continue;
        }

        for edge_pair_idx in 0..filtered_edge_set.len() / 2 {
            let edge_pair_start = edge_pair_idx * 2;
            let start_x: i32 = filtered_edge_set[edge_pair_start].max(0);
            if start_x >= sfc_width {
                // We're past the end of the draw area and can stop drawing.
                break;
            }
            let end_x: i32 = filtered_edge_set[edge_pair_start + 1].clamp(start_x, sfc_width);
            let fill_len: i32 = end_x - start_x;
            let end_clear_len: i32 = sfc_width - end_x;

            if !bounded && start_x > 0 {
                // Clear up to the start
                surface.clear_stride(0, y, start_x.max(0) as usize);
            }

            if fill_len > 0 {
                composite_opaque(
                    operator,
                    surface,
                    pattern,
                    start_x,
                    y,
                    fill_len.max(0) as usize,
                    precision_eff,
                );
            }

            if !bounded && end_clear_len > 0 {
                // Clear to the end
                surface.clear_stride(end_x, y, end_clear_len.max(0) as usize);
            }
        }
    }
}
