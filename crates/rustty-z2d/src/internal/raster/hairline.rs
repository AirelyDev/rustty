// SPDX-License-Identifier: MPL-2.0
//   Copyright © 2024-2025 Chris Marchesi
//
// Ported to Rust for rustty from z2d 0.10.0
// `src/internal/raster/hairline.zig` (R3 vendored dep).

//! Various hairline stroking functions. These functions draw at the minimum
//! technical stroke width (1 pixel). Note that it is possible to get widths
//! smaller than this with the standard polygon stroker, but sub-1-pixel line
//! widths will usually yield artifacts, and as such a small width is only
//! really useful with a transform that can widen the width after.
//!
//! These functions ensure a hairline is properly drawn. However, they don't
//! support things like caps or joins.
//!
//! Note that these technically don't "raster", but we've put them in this
//! context for a lack of a better name.

use crate::compositor::{Operator, Precision};
use crate::internal::point::Point;
use crate::internal::raster::shared::{composite_opacity, composite_opaque};
use crate::internal::tess::polygon::ContourList;
use crate::options::AntiAliasMode;
use crate::pattern::Pattern;
use crate::surface::Surface;

pub fn run(
    surface: &mut Surface,
    pattern: &Pattern,
    contours: &ContourList,
    operator: Operator,
    precision: Precision,
    aa_mode: AntiAliasMode,
) {
    let effective_precision = if operator.requires_float() {
        Precision::Float
    } else {
        precision
    };
    for contour in &contours.list {
        match contour.len {
            0 => {}
            1 => {
                let point = contour.corner_point(contour.corners_first().unwrap());
                // NOTE: OOB co-ordinates are handled by the surface, see
                // documentation for Surface.paintStride, Surface.getStride,
                // and Surface.compositeStride for details).
                composite_opaque(
                    operator,
                    surface,
                    pattern,
                    point.x.round() as i32,
                    point.y.round() as i32,
                    1,
                    effective_precision,
                );
            }
            _ => {
                let mut points: [Point; 2] = [Point { x: 0.0, y: 0.0 }; 2];
                points[0] = contour.corner_point(contour.corners_first().unwrap());
                let mut idx: usize = 1;
                let mut node_ = contour.corner_next(contour.corners_first().unwrap());
                while let Some(node) = node_ {
                    points[idx] = contour.corner_point(node);
                    draw_line(
                        DrawOptions {
                            surface: &mut *surface,
                            pattern,
                            operator,
                            precision: effective_precision,
                            aa_mode,
                        },
                        points[0].x.round() as i32,
                        points[0].y.round() as i32,
                        points[1].x.round() as i32,
                        points[1].y.round() as i32,
                    );
                    idx ^= 1;
                    node_ = contour.corner_next(node);
                }
            }
        }
    }
}

struct DrawOptions<'a> {
    surface: &'a mut Surface,
    pattern: &'a Pattern,
    operator: Operator,
    precision: Precision,
    aa_mode: AntiAliasMode,
}

fn draw_line(opts: DrawOptions, x0: i32, y0: i32, x1: i32, y1: i32) {
    // We do a full box check here, and clipping in the fast-path functions.
    // Note, however, that due to how clipping complicates the Bresenham and Wu
    // algorithms, we rely on the surface implementation to do bounds checking
    // (since we only draw 1 pixel at a time, we can rely on the start x and y
    // co-ordinate check in the underlying surface; see documentation for
    // Surface.paintStride, Surface.getStride, and Surface.compositeStride for
    // details).
    if !in_box(opts.surface, x0, y0, x1, y1) {
        return;
    }

    let dx: u32 = (x1 - x0).unsigned_abs();
    let dy: u32 = (y1 - y0).unsigned_abs();
    if dx == 0 {
        draw_vertical(opts, x0, y0, y1);
    } else if dy == 0 {
        draw_horizontal(opts, x0, x1, y0);
    } else if dx < dy {
        match opts.aa_mode {
            AntiAliasMode::None => draw_bres_vertical(opts, x0, y0, x1, y1),
            _ => draw_wu_vertical(opts, x0, y0, x1, y1),
        }
    } else {
        match opts.aa_mode {
            AntiAliasMode::None => draw_bres_horizontal(opts, x0, y0, x1, y1),
            _ => draw_wu_horizontal(opts, x0, y0, x1, y1),
        }
    }
}

fn draw_horizontal(opts: DrawOptions, x0: i32, x1: i32, y: i32) {
    let start_x: i32 = x0.min(x1).clamp(0, opts.surface.get_width() - 1);
    let end_x: i32 = x0.max(x1).clamp(0, opts.surface.get_width() - 1);
    let len: i32 = end_x - start_x + 1;
    if len > 0 {
        composite_opaque(
            opts.operator,
            opts.surface,
            opts.pattern,
            start_x,
            y,
            len as usize,
            opts.precision,
        );
    }
}

fn draw_vertical(opts: DrawOptions, x: i32, y0: i32, y1: i32) {
    let start_y: u32 = y0.min(y1).clamp(0, opts.surface.get_height() - 1) as u32;
    let end_y: u32 = y0.max(y1).clamp(0, opts.surface.get_height() - 1) as u32;

    for y_u in start_y..=end_y {
        let y_i: i32 = y_u as i32;
        composite_opaque(
            opts.operator,
            opts.surface,
            opts.pattern,
            x,
            y_i,
            1,
            opts.precision,
        );
    }
}

fn draw_bres_horizontal(opts: DrawOptions, x0: i32, y0: i32, x1: i32, y1: i32) {
    if x0 > x1 {
        // Reverse our lines as the major axis is out-of-order
        return draw_bres_horizontal(opts, x1, y1, x0, y0);
    }

    let dx = x1 - x0;
    let (dy, sy) = delta_step(y0, y1);

    let mut x: i32 = x0;
    let mut y: i32 = y0;
    let mut d: i32 = 2 * dy - dx; // Decision parameter
    let d_inc = 2 * dy; // Decision increment
    let d_dec = 2 * dx; // Decision decrement

    while x <= x1 {
        composite_opaque(
            opts.operator,
            opts.surface,
            opts.pattern,
            x,
            y,
            1,
            opts.precision,
        );
        if d > 0 {
            y += sy;
            d -= d_dec;
        }
        d += d_inc;
        x += 1;
    }
}

// See drawBresHorizontal for details.
fn draw_bres_vertical(opts: DrawOptions, x0: i32, y0: i32, x1: i32, y1: i32) {
    if y0 > y1 {
        return draw_bres_vertical(opts, x1, y1, x0, y0);
    }

    let dy = y1 - y0;
    let (dx, sx) = delta_step(x0, x1);

    let mut x: i32 = x0;
    let mut y: i32 = y0;
    let mut d: i32 = 2 * dx - dy;
    let d_inc = 2 * dx;
    let d_dec = 2 * dy;

    while y <= y1 {
        composite_opaque(
            opts.operator,
            opts.surface,
            opts.pattern,
            x,
            y,
            1,
            opts.precision,
        );
        if d > 0 {
            x += sx;
            d -= d_dec;
        }
        d += d_inc;
        y += 1;
    }
}

fn draw_wu_horizontal(opts: DrawOptions, x0: i32, y0: i32, x1: i32, y1: i32) {
    // NOTE: We're using static stuff here: our error is u16, opacity is u8.
    if x0 > x1 {
        // Reverse our lines as the major axis is out-of-order
        return draw_wu_horizontal(opts, x1, y1, x0, y0);
    }

    let dx = x1 - x0;
    let (dy, sy) = delta_step(y0, y1);

    let mut x: i32 = x0;
    let mut y: i32 = y0;

    // Decision parameter and increment, see the errInc helper for more
    // details.
    let mut err: u16 = 0;
    let err_inc: u16 = err_inc(dy, dx);

    // First pixel is full intensity
    composite_opaque(
        opts.operator,
        opts.surface,
        opts.pattern,
        x,
        y,
        1,
        opts.precision,
    );
    x += 1;

    while x < x1 {
        let (new_err, overflow) = err.overflowing_add(err_inc);
        err = new_err;
        if overflow {
            // Error rolled over, so increment y
            y += sy;
        }

        // Shift down the error. This gives us the *complement* to our opacity,
        // not the opacity itself. This is because error dictates how much in
        // the center we are, so no error means fully in the middle, and hence
        // full opacity.
        let opacity_complement: u8 = (err >> 8) as u8;
        composite_opacity(
            opts.operator,
            opts.surface,
            opts.pattern,
            x,
            y,
            1,
            opts.precision,
            opacity_complement ^ 0xFF,
        );
        composite_opacity(
            opts.operator,
            opts.surface,
            opts.pattern,
            x,
            y + sy,
            1,
            opts.precision,
            opacity_complement,
        );
        x += 1;
    }

    // Last pixel is full intensity
    composite_opaque(
        opts.operator,
        opts.surface,
        opts.pattern,
        x1,
        y1,
        1,
        opts.precision,
    );
}

// See drawWuHorizontal for details.
fn draw_wu_vertical(opts: DrawOptions, x0: i32, y0: i32, x1: i32, y1: i32) {
    if y0 > y1 {
        return draw_wu_vertical(opts, x1, y1, x0, y0);
    }

    let dy = y1 - y0;
    let (dx, sx) = delta_step(x0, x1);

    let mut x: i32 = x0;
    let mut y: i32 = y0;

    let mut err: u16 = 0;
    let err_inc: u16 = err_inc(dx, dy);

    composite_opaque(
        opts.operator,
        opts.surface,
        opts.pattern,
        x,
        y,
        1,
        opts.precision,
    );
    y += 1;

    while y < y1 {
        let (new_err, overflow) = err.overflowing_add(err_inc);
        err = new_err;
        if overflow {
            x += sx;
        }

        let opacity_complement: u8 = (err >> 8) as u8;
        composite_opacity(
            opts.operator,
            opts.surface,
            opts.pattern,
            x,
            y,
            1,
            opts.precision,
            opacity_complement ^ 0xFF,
        );
        composite_opacity(
            opts.operator,
            opts.surface,
            opts.pattern,
            x + sx,
            y,
            1,
            opts.precision,
            opacity_complement,
        );
        y += 1;
    }

    composite_opaque(
        opts.operator,
        opts.surface,
        opts.pattern,
        x1,
        y1,
        1,
        opts.precision,
    );
}

/// Calculates a delta and step value for the minor axis in both Bresenham and
/// Xiaolin Wu algorithms. This enables a single version of each of these
/// algorithms to operate on both directions on the minor axis.
//
// PORT NOTE (rustty): the Zig second element is an `i2` step (∈ {-1, +1}); the
// port widens it to `i32`. The value is only ever ±1 and is only added to a
// coordinate, so the widening is behavior-identical (see design R2).
fn delta_step(a: i32, b: i32) -> (i32, i32) {
    let c = b - a;
    if c < 0 {
        return (-c, -1);
    }
    (c, 1)
}

/// Calculates an error increment as ((a << 16) / b) for use in Xiaolin Wu line
/// algorithms.
///
/// In the Wu algorithm, the error variable is stored as an unsigned value that
/// is representative of the slope (a fully fractional value less than one);
/// contrast this with Bresenham where the decision parameter is stored as a
/// signed reduction of the slope, which fully avoids division. As a completely
/// fractional value would, rollover dictates when to advance, versus the
/// decision parameter being greater than zero.
//
// PORT NOTE (rustty): the Zig `@intCast(c / d)` to `u16` asserts the quotient
// fits (guaranteed `< 1 << 16` since this branch has `a < b` at both call
// sites); the port keeps the same truncation semantics via `as u16` (see design
// R3).
fn err_inc(a: i32, b: i32) -> u16 {
    if a == b {
        // Guards against dx == dy. Note that this case should always be
        // fast-pathed outside of the Wu algorithm (or even Bresenham, for that
        // matter).
        return u16::MAX;
    }
    let c: u32 = (a as u32) << 16;
    let d: u32 = b as u32;
    (c / d) as u16
}

fn in_box(surface: &Surface, x0: i32, y0: i32, x1: i32, y1: i32) -> bool {
    if (x0 < 0 || x0 >= surface.get_width()) && (x1 < 0 || x1 >= surface.get_width()) {
        return false;
    }

    if (y0 < 0 || y0 >= surface.get_height()) && (y1 < 0 || y1 >= surface.get_height()) {
        return false;
    }

    true
}

#[cfg(test)]
mod tests {
    // The reference `hairline.zig` carries NO `test{}` blocks (`grep '^test'` →
    // 0). These are additive, host-only known-answer tests: build a `Contour`
    // by hand, wrap in a `ContourList`, `run()`, then assert the deterministic
    // `AntiAliasMode::None` coverage pixels exactly (single-pixel
    // `composite_opaque`), and the Wu path via robust partial-coverage
    // invariants (design §5).
    use super::*;
    use crate::context::Context;
    use crate::internal::tess::polygon::Contour;
    use crate::pattern::OpaquePattern;
    use crate::pixel::{Alpha8, Pixel};
    use crate::surface::SurfaceType;

    fn opaque_alpha8() -> Pattern {
        Pattern::OpaquePattern(OpaquePattern {
            pixel: Pixel::Alpha8(Alpha8 { a: 0xFF }),
        })
    }

    fn alpha_at(sfc: &Surface, x: i32, y: i32) -> u8 {
        match sfc.get_pixel(x, y).unwrap() {
            Pixel::Alpha8(a) => a.a,
            other => panic!("expected alpha8, got {other:?}"),
        }
    }

    // Build a single-contour `ContourList` from the given (x, y) points.
    fn contour_list(points: &[(f64, f64)]) -> ContourList {
        let mut c = Contour::new(1.0);
        for &(x, y) in points {
            c.plot(Point { x, y }, None);
        }
        let mut list = ContourList::empty();
        list.append(c);
        list
    }

    #[test]
    fn hairline_horizontal_line_none() {
        // 8x8 surface, contour (1,2)->(6,2), aa=None, SrcOver opaque.
        // Exercises draw_horizontal (dy == 0) + clamp.
        let mut sfc = Surface::init(SurfaceType::ImageSurfaceAlpha8, 8, 8).unwrap();
        let pattern = opaque_alpha8();
        let contours = contour_list(&[(1.0, 2.0), (6.0, 2.0)]);
        run(
            &mut sfc,
            &pattern,
            &contours,
            Operator::SrcOver,
            Precision::Integer,
            AntiAliasMode::None,
        );
        for x in 1..=6 {
            assert_eq!(0xFF, alpha_at(&sfc, x, 2), "drawn pixel ({x},2)");
        }
        assert_eq!(0x00, alpha_at(&sfc, 0, 2), "left of line");
        assert_eq!(0x00, alpha_at(&sfc, 7, 2), "right of line");
        for x in 0..8 {
            assert_eq!(0x00, alpha_at(&sfc, x, 1), "row above ({x},1)");
            assert_eq!(0x00, alpha_at(&sfc, x, 3), "row below ({x},3)");
        }
        sfc.deinit();
    }

    #[test]
    fn hairline_vertical_line_none() {
        // contour (3,1)->(3,6); drawn via draw_vertical (dx == 0).
        let mut sfc = Surface::init(SurfaceType::ImageSurfaceAlpha8, 8, 8).unwrap();
        let pattern = opaque_alpha8();
        let contours = contour_list(&[(3.0, 1.0), (3.0, 6.0)]);
        run(
            &mut sfc,
            &pattern,
            &contours,
            Operator::SrcOver,
            Precision::Integer,
            AntiAliasMode::None,
        );
        for y in 1..=6 {
            assert_eq!(0xFF, alpha_at(&sfc, 3, y), "drawn pixel (3,{y})");
        }
        assert_eq!(0x00, alpha_at(&sfc, 3, 0), "above line");
        assert_eq!(0x00, alpha_at(&sfc, 3, 7), "below line");
        for y in 0..8 {
            assert_eq!(0x00, alpha_at(&sfc, 2, y), "col left (2,{y})");
            assert_eq!(0x00, alpha_at(&sfc, 4, y), "col right (4,{y})");
        }
        sfc.deinit();
    }

    #[test]
    fn hairline_diagonal_bresenham_none() {
        // contour (0,0)->(5,3), aa=None -> draw_bres_horizontal (dx > dy). The
        // exact Bresenham staircase (decision-parameter walk) is:
        //   (0,0),(1,1),(2,1),(3,2),(4,2),(5,3)
        let mut sfc = Surface::init(SurfaceType::ImageSurfaceAlpha8, 8, 8).unwrap();
        let pattern = opaque_alpha8();
        let contours = contour_list(&[(0.0, 0.0), (5.0, 3.0)]);
        run(
            &mut sfc,
            &pattern,
            &contours,
            Operator::SrcOver,
            Precision::Integer,
            AntiAliasMode::None,
        );
        let expected = [(0, 0), (1, 1), (2, 1), (3, 2), (4, 2), (5, 3)];
        for y in 0..8 {
            for x in 0..8 {
                let want = if expected.contains(&(x, y)) {
                    0xFF
                } else {
                    0x00
                };
                assert_eq!(want, alpha_at(&sfc, x, y), "pixel ({x},{y})");
            }
        }
        sfc.deinit();
    }

    #[test]
    fn hairline_diagonal_wu_default() {
        // Same endpoints, aa=Default -> draw_wu_horizontal. Robust invariants:
        // (1) both endpoints are full-intensity (composite_opaque), and (2) at
        // least one interior column carries a partial-coverage pixel
        // (0 < a < 0xFF), proving the Wu error accumulator ran.
        let mut sfc = Surface::init(SurfaceType::ImageSurfaceAlpha8, 8, 8).unwrap();
        let pattern = opaque_alpha8();
        let contours = contour_list(&[(0.0, 0.0), (5.0, 3.0)]);
        run(
            &mut sfc,
            &pattern,
            &contours,
            Operator::SrcOver,
            Precision::Integer,
            AntiAliasMode::Default,
        );
        // Endpoints full.
        assert_eq!(0xFF, alpha_at(&sfc, 0, 0), "start endpoint full");
        assert_eq!(0xFF, alpha_at(&sfc, 5, 3), "end endpoint full");
        // Interior partials (x=1: opacity_complement=153 -> (1,0)=102, (1,1)=153).
        let p10 = alpha_at(&sfc, 1, 0);
        let p11 = alpha_at(&sfc, 1, 1);
        assert!(p10 > 0 && p10 < 0xFF, "interior partial (1,0)={p10}");
        assert!(p11 > 0 && p11 < 0xFF, "interior partial (1,1)={p11}");
        sfc.deinit();
    }

    #[test]
    fn hairline_rect_via_painter_stroke() {
        // The overlay's real shape: 4 line_to's + close_path through the managed
        // Context (`ctx.hairline = true; ctx.stroke()`), aa=None. Assert the 1px
        // border pixels are set and the interior center is untouched (stroke
        // only, no fill). This is the end-to-end no-longer-panics assertion.
        let mut sfc = Surface::init(SurfaceType::ImageSurfaceAlpha8, 8, 8).unwrap();
        {
            let mut ctx = Context::init(&mut sfc);
            ctx.set_source(opaque_alpha8());
            ctx.anti_aliasing_mode = AntiAliasMode::None;
            ctx.hairline = true;
            ctx.move_to(1.0, 1.0);
            ctx.line_to(5.0, 1.0);
            ctx.line_to(5.0, 5.0);
            ctx.line_to(1.0, 5.0);
            ctx.close_path();
            ctx.stroke().unwrap();
            ctx.deinit();
        }
        // Borders drawn.
        assert_eq!(0xFF, alpha_at(&sfc, 3, 1), "top border");
        assert_eq!(0xFF, alpha_at(&sfc, 3, 5), "bottom border");
        assert_eq!(0xFF, alpha_at(&sfc, 1, 3), "left border");
        assert_eq!(0xFF, alpha_at(&sfc, 5, 3), "right border");
        // Interior untouched (stroke only).
        assert_eq!(0x00, alpha_at(&sfc, 3, 3), "interior center");
        sfc.deinit();
    }

    #[test]
    fn hairline_dashed_is_blocker() {
        // A valid dash array with hairline stroking must hit the
        // polyline_plotter Dasher.validate `unimplemented!()` (subset-guard
        // contract; parity with stroke_plotter's dashed blocker).
        use crate::internal::path_nodes::{PathLineTo, PathMoveTo, PathNode};
        use crate::painter::{self, StrokeOptions};

        let result = std::panic::catch_unwind(|| {
            let mut sfc = Surface::init(SurfaceType::ImageSurfaceAlpha8, 8, 8).unwrap();
            let pattern = opaque_alpha8();
            let nodes = [
                PathNode::MoveTo(PathMoveTo {
                    point: Point { x: 1.0, y: 1.0 },
                }),
                PathNode::LineTo(PathLineTo {
                    point: Point { x: 5.0, y: 1.0 },
                }),
            ];
            let opts = StrokeOptions {
                hairline: true,
                dashes: vec![2.0, 2.0],
                ..StrokeOptions::default()
            };
            let _ = painter::stroke(&mut sfc, &pattern, &nodes, opts);
        });
        assert!(result.is_err(), "expected dashed-hairline blocker panic");
    }
}
