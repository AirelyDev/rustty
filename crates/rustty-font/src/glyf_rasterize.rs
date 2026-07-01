//! Rasterization for OpenType glyf outlines.
//!
//! This module intentionally lives in `font` rather than `font/opentype`
//! because I wanted to keep `font/opentype` dependency free on the font
//! package.
// PORT: ghostty src/font/glyf_rasterize.zig. The Zig file imports `std`,
// `z2d`, `Glyph = @import("Glyph.zig")`, and `glyf = @import("opentype/glyf.zig")`.
// Here `z2d` is the `rustty_z2d` crate, `Glyph`/`DesignMetrics`/`RenderOptions`
// are `crate::glyph::*`, and `glyf` is `crate::opentype::glyf`.

use rustty_z2d as z2d;
use z2d::painter::{self, FillOptions};
use z2d::pattern::{OpaquePattern, Pattern};
use z2d::pixel::{Alpha8, Pixel};
use z2d::surface::{Surface, SurfaceType};
use z2d::Path;

use crate::glyph::{self, DesignMetrics, RenderOptions, Size};
use crate::opentype::glyf::{Outline, Point as GlyfPoint};

/// An owned, tightly packed alpha8 bitmap.
// PORT NOTE: the Zig `data: []u8` is an allocator-owned slice freed by `deinit`.
// Here it is an owned `Vec<u8>` freed on drop, so there is no `deinit`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bitmap {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

impl Bitmap {
    // An empty 0x0 bitmap.
    pub fn empty() -> Bitmap {
        Bitmap {
            width: 0,
            height: 0,
            data: Vec::new(),
        }
    }

    pub fn init_empty(width: u32, height: u32) -> Bitmap {
        let data = vec![0u8; (width as usize) * (height as usize)];
        Bitmap {
            width,
            height,
            data,
        }
    }
}

/// Errors that can arise during rasterization.
// PORT NOTE: the Zig `Error = Allocator.Error || z2d.Path.Error ||
// z2d.painter.FillError`. Rust allocation is infallible, so the allocator arm
// disappears; the `z2d.Surface.init` error (part of `FillError` in Zig via its
// surface error) is folded in explicitly as `Surface`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Path(z2d::path::Error),
    Fill(painter::FillError),
    Surface(z2d::surface::Error),
}

impl From<z2d::path::Error> for Error {
    fn from(e: z2d::path::Error) -> Error {
        Error::Path(e)
    }
}

impl From<painter::FillError> for Error {
    fn from(e: painter::FillError) -> Error {
        Error::Fill(e)
    }
}

impl From<z2d::surface::Error> for Error {
    fn from(e: z2d::surface::Error) -> Error {
        Error::Surface(e)
    }
}

/// Rasterize a decoded glyf outline to a full-cell alpha bitmap.
///
/// The returned bitmap is always `grid_metrics.cell_width * cell_width` by
/// `grid_metrics.cell_height`. `opts.constraint` is applied using the same
/// `RenderOptions.Constraint` machinery used by the platform font
/// backends.
///
/// The caller owns the returned bitmap.
pub fn rasterize(
    outline: &Outline,
    design: DesignMetrics,
    opts: RenderOptions,
) -> Result<Bitmap, Error> {
    assert!(design.units_per_em > 0);
    assert!(design.advance_width > 0);
    assert!(design.line_height > 0);

    // Calculate our final width/height.
    // PORT NOTE: Zig's `std.math.mul(u32, ...) catch std.math.maxInt(u32)`
    // saturates to the max on overflow, which is exactly `u32::saturating_mul`.
    let width: u32 = opts
        .grid_metrics
        .cell_width
        .saturating_mul(opts.cell_width.unwrap_or(1) as u32);
    let height = opts.grid_metrics.cell_height;
    assert!(width > 0 && height > 0);

    // If we have no contours or points then we have no drawable shape, but the
    // caller still asked for a cell-sized bitmap. Return that full bitmap with
    // zero coverage so downstream atlas/upload code doesn't need a separate
    // size contract for empty glyphs.
    if outline.contours.is_empty() || outline.points.is_empty() {
        return Ok(Bitmap::init_empty(width, height));
    }

    // Glyf entries have a header bounding box, but this rasterizer operates on
    // the decoded Outline only. Recompute bounds from the decoded coordinate
    // data so placement and scaling follow the geometry we actually draw. If a
    // source glyf header disagrees with its points, the point data is the safer
    // source of truth for rasterization; the header belongs in decode-time
    // validation/metadata, not this font-level drawing API.
    let bounds: Bounds = {
        let mut bounds: Bounds = Bounds {
            x_min: outline.points[0].x as f64,
            y_min: outline.points[0].y as f64,
            x_max: outline.points[0].x as f64,
            y_max: outline.points[0].y as f64,
        };
        for p in &outline.points[1..] {
            let x: f64 = p.x as f64;
            let y: f64 = p.y as f64;
            bounds.x_min = bounds.x_min.min(x);
            bounds.y_min = bounds.y_min.min(y);
            bounds.x_max = bounds.x_max.max(x);
            bounds.y_max = bounds.y_max.max(y);
        }
        bounds
    };

    // Degenerate point bounds can't produce filled area and would make the
    // point-to-bitmap transform divide by zero, so return a full transparent
    // bitmap just like an empty outline.
    if bounds.width() == 0.0 || bounds.height() == 0.0 {
        return Ok(Bitmap::init_empty(width, height));
    }

    // Build the surface we'll draw on. This is a simple alpha8 drawing.
    let mut sfc: Surface =
        Surface::init(SurfaceType::ImageSurfaceAlpha8, width as i32, height as i32)?;

    let mut path: Path = Path::empty();

    let placement: Placement = Placement::init(bounds, design, opts);
    for i in 0..outline.contours.len() {
        append_contour_path(&mut path, outline.contour(i), bounds, placement)?;
    }

    painter::fill(
        &mut sfc,
        &Pattern::OpaquePattern(OpaquePattern {
            pixel: Pixel::Alpha8(Alpha8 { a: 255 }),
        }),
        &path.nodes,
        FillOptions::default(),
    )?;

    let data: Vec<u8> = sfc.alpha8_buf().iter().map(|p| p.a).collect();

    Ok(Bitmap {
        width,
        height,
        data,
    })
}

#[derive(Clone, Copy, Debug)]
struct Bounds {
    x_min: f64,
    y_min: f64,
    x_max: f64,
    y_max: f64,
}

impl Bounds {
    fn width(self) -> f64 {
        self.x_max - self.x_min
    }

    fn height(self) -> f64 {
        self.y_max - self.y_min
    }
}

/// Cell-relative pixel rectangle where the decoded outline bounds should be
/// rasterized within the output bitmap.
///
/// This is deliberately the placement of the outline's computed point bounds,
/// not the full declared advance/line-height box. `advance_width` and
/// `line_height` describe the design-space layout box the outline was drawn
/// within: they include intentional bearings and whitespace around the visible
/// points. We use that declared box when applying `RenderOptions.Constraint` so
/// sizing and alignment preserve those bearings consistently with other font
/// backends; once that is resolved, we rasterize only the actual outline bounds
/// into this rectangle.
///
/// ```text
/// output bitmap / terminal cell
/// ╭────────────────────────────────────────────────────────────────────────╮ top
/// │                                                                        │
/// │   declared advance/line-height box                                     │
/// │   (outer layout box used for constraints)                              │
/// │   ╭────────────────────────────────────────────────────────────────╮   │
/// │   │                                                                │   │
/// │◀──────── x ────────▶╭────────── width ──────────╮                  │   │
/// │   │                 │ Placement                 │ ▲ height         │   │
/// │   │                 │ outline point bounds      │ │                │   │
/// │   │                 │ pixels to draw            │ │                │   │
/// │   │                 │                           │ │                │   │
/// │   │                 ╰───────────────────────────╯ ▼                │   │
/// │   ╰─────────────────▲──────────────────────────────────────────────╯   │
/// │                     │ y                                                │
/// ╰────────────────────────────────────────────────────────────────────────╯ bottom
///              x is measured from the bitmap left to the Placement left.
///              y is measured from the bitmap bottom to the Placement bottom.
///              bitmap_height is the full top-to-bottom bitmap height.
/// ```
///
/// Constraints are applied to the outer box so the whitespace remains part of
/// alignment decisions. `Placement` is the inner rectangle after that outer box
/// has been constrained.
#[derive(Clone, Copy, Debug)]
struct Placement {
    /// Left edge of the rasterized outline bounds in bitmap pixels, measured
    /// from the bitmap's left edge.
    x: f64,

    /// Bottom edge of the rasterized outline bounds in bitmap pixels, measured
    /// from the bitmap's bottom edge. This matches the cell-relative y axis
    /// used by font.Glyph.Size and is converted to z2d's y-down axis when
    /// points are transformed.
    y: f64,

    /// Width of the rasterized outline bounds in bitmap pixels after applying
    /// font.Glyph.RenderOptions.Constraint.
    width: f64,

    /// Height of the rasterized outline bounds in bitmap pixels after applying
    /// font.Glyph.RenderOptions.Constraint.
    height: f64,

    /// Full bitmap height in pixels, used to convert cell-relative y-up-ish
    /// placement into the y-down coordinate system used by z2d surfaces.
    bitmap_height: f64,
}

impl Placement {
    /// Calculate where the decoded point bounds should land in the output
    /// bitmap.
    ///
    /// The glyf protocol supplies declared metrics (`units_per_em`,
    /// `advance_width`, and `line_height`) in design units, while Ghostty's
    /// font constraint code works in cell-relative pixels. We first map the em
    /// square to one cell height, matching the linked glyph rasterizer's
    /// baseline model where design-space `y=0` is the bottom/baseline of the em
    /// and `y=units_per_em` is its top. Then we describe the actual outline
    /// bounds as a relative sub-rectangle of the declared advance/line-height
    /// box. That declared box includes any intentional side bearings or
    /// vertical whitespace around the outline; constraints should apply to that
    /// layout box rather than to the tight point bounds alone. This returns the
    /// final pixel rectangle for only the outline bounds that we will rasterize.
    fn init(bounds: Bounds, design: DesignMetrics, opts: RenderOptions) -> Placement {
        // Start with protocol-like design units mapped so that the em square
        // occupies one cell. This makes units_per_em the scale reference and
        // preserves the linked rasterizer's y=0 baseline/bottom behavior.
        // Callers can then use RenderOptions.Constraint to fit/cover/stretch/
        // align the declared advance/line-height box using existing font logic.
        let scale = (opts.grid_metrics.cell_height as f64) / (design.units_per_em as f64);

        // Convert the decoded point bounds into the same pixel coordinate space
        // expected by RenderOptions.Constraint. This rectangle is the visible
        // outline bounds, not the full advance/line-height layout box.
        let glyph: Size = Size {
            width: bounds.width() * scale,
            height: bounds.height() * scale,
            x: bounds.x_min * scale,
            y: bounds.y_min * scale,
        };

        // Convert the declared layout box to pixels. This is the box that
        // carries intentional bearings/whitespace and should be constrained.
        let group_width = (design.advance_width as f64) * scale;
        let group_height = (design.line_height as f64) * scale;

        // Apply the same fit/cover/stretch/alignment/padding rules used by
        // normal font rendering. The result is still the outline bounds, but
        // placed as if its containing advance/line-height box was constrained.
        let constraint: glyph::Constraint = {
            let mut constraint = opts.constraint;
            if group_width > 0.0 && group_height > 0.0 {
                // Tell Constraint that `glyph` is a sub-rectangle of the
                // declared layout box. Constraint will size/align the outer box
                // and then return the corresponding transformed inner box.
                constraint.relative_width = glyph.width / group_width;
                constraint.relative_height = glyph.height / group_height;
                constraint.relative_x = glyph.x / group_width;
                constraint.relative_y = glyph.y / group_height;
            }
            constraint
        };
        let constrained = constraint.constrain(glyph, opts.grid_metrics, opts.constraint_width);

        // Store the final outline placement plus the full bitmap height needed
        // later to flip from cell-relative y to z2d's y-down surface space.
        Placement {
            x: constrained.x,
            y: constrained.y,
            width: constrained.width,
            height: constrained.height,
            bitmap_height: opts.grid_metrics.cell_height as f64,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Point {
    x: f64,
    y: f64,
}

/// Append one contour to a z2d path.
///
/// Glyf contours are quadratic outlines with explicit on-curve points and
/// off-curve control points. Consecutive off-curve points imply an on-curve
/// point halfway between them, and a contour may begin with an off-curve point.
/// This normalizes those cases while walking the closed contour and emits z2d
/// line/cubic-curve operations in bitmap coordinates.
fn append_contour_path(
    path: &mut Path,
    contour: &[GlyfPoint],
    bounds: Bounds,
    placement: Placement,
) -> Result<(), Error> {
    if contour.is_empty() {
        return Ok(());
    }

    let first = contour[0];
    let last = contour[contour.len() - 1];

    let mut current: Point;
    let mut i: usize = 0;

    // Choose the starting on-curve point for this closed contour. If the first
    // point is off-curve then the contour logically starts either at the final
    // on-curve point, or at the implied midpoint between the final and first
    // off-curve points.
    if first.on_curve {
        i = 1;
        current = transform_point(first, bounds, placement);
    } else if last.on_curve {
        current = transform_point(last, bounds, placement);
    } else {
        current = midpoint(
            transform_point(last, bounds, placement),
            transform_point(first, bounds, placement),
        );
    }

    // Move to the beginning
    path.move_to(current.x, current.y);

    // Go through the points and connect em!
    while i < contour.len() {
        let p = contour[i];

        // On-curve points connect to the current point with a straight line.
        if p.on_curve {
            current = transform_point(p, bounds, placement);
            path.line_to(current.x, current.y);
            i += 1;
            continue;
        }

        // Off-curve points are quadratic control points. The following point is
        // either the curve endpoint or, if it is also off-curve, contributes an
        // implied on-curve endpoint halfway between the two controls.
        let control = transform_point(p, bounds, placement);
        let next = contour[(i + 1) % contour.len()];
        let end = if next.on_curve {
            transform_point(next, bounds, placement)
        } else {
            midpoint(control, transform_point(next, bounds, placement))
        };

        // z2d paths only expose cubic curves, so convert the TrueType
        // quadratic segment to an equivalent cubic segment before appending it.
        let c1 = Point {
            x: current.x + ((2.0 / 3.0) * (control.x - current.x)),
            y: current.y + ((2.0 / 3.0) * (control.y - current.y)),
        };
        let c2 = Point {
            x: end.x + ((2.0 / 3.0) * (control.x - end.x)),
            y: end.y + ((2.0 / 3.0) * (control.y - end.y)),
        };
        path.curve_to(c1.x, c1.y, c2.x, c2.y, end.x, end.y)?;

        current = end;

        // If we consumed an explicit on-curve endpoint then skip it; otherwise
        // the next off-curve point still needs to be used as the control point
        // for the following quadratic segment.
        i += if next.on_curve { 2 } else { 1 };
    }

    path.close();

    Ok(())
}

/// Convert a decoded glyf point from design-space coordinates to z2d bitmap
/// coordinates.
///
/// `bounds` describes the decoded outline's point/control bounds in glyf
/// design units. `placement` describes where those bounds should land in the
/// output bitmap after constraints are applied. Glyf coordinates are y-up; z2d
/// surfaces are y-down, so this also flips the y axis using
/// `placement.bitmap_height`.
fn transform_point(p: GlyfPoint, bounds: Bounds, placement: Placement) -> Point {
    let scale_x = placement.width / bounds.width();
    let scale_y = placement.height / bounds.height();
    let x_design: f64 = p.x as f64;
    let y_design: f64 = p.y as f64;
    Point {
        x: placement.x + ((x_design - bounds.x_min) * scale_x),
        y: placement.bitmap_height - placement.y - ((y_design - bounds.y_min) * scale_y),
    }
}

/// Return the implied on-curve point between two off-curve TrueType control
/// points.
fn midpoint(a: Point, b: Point) -> Point {
    Point {
        x: (a.x + b.x) / 2.0,
        y: (a.y + b.y) / 2.0,
    }
}

/// Return deterministic font metrics for the PNG reference test.
///
/// These are intentionally minimal: the rasterizer only needs cell geometry,
/// face geometry, and icon heights for constraint calculations.
// PORT NOTE: glyf_rasterize.zig and glyf_rasterize_png_test.zig each declare an
// identical `testMetrics` helper; this single shared `test_metrics` serves both
// the unit tests below and the PNG known-answer test in the submodule.
#[cfg(test)]
fn test_metrics(width: u32, height: u32) -> crate::metrics::Metrics {
    crate::metrics::Metrics {
        cell_width: width,
        cell_height: height,
        cell_baseline: 0,
        underline_position: height,
        underline_thickness: 1,
        strikethrough_position: height / 2,
        strikethrough_thickness: 1,
        overline_position: 0,
        overline_thickness: 1,
        box_thickness: 1,
        cursor_thickness: 1,
        cursor_height: height,
        icon_height: height as f64,
        icon_height_single: height as f64,
        face_width: width as f64,
        face_height: height as f64,
        face_y: 0.0,
    }
}

#[cfg(test)]
mod glyf_rasterize_png_test;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::opentype::glyf::Point as GP;

    fn render_options(metrics: crate::metrics::Metrics) -> RenderOptions {
        RenderOptions {
            grid_metrics: metrics,
            ..RenderOptions::default()
        }
    }

    // PORT: ghostty's leading `test { _ = @import("glyf_rasterize_png_test.zig"); }`
    // — the reference-import test block that pulls the PNG known-answer test file
    // into the test binary. In Rust that inclusion is the
    // `#[cfg(test)] mod glyf_rasterize_png_test;` declaration in the parent
    // module; this mirrors the reference `test {}` block so the ported test count
    // matches the reference 1:1 (R1).
    #[test]
    fn reference_import_glyf_rasterize_png_test() {
        // The submodule is linked via `mod glyf_rasterize_png_test;`; its
        // `bubbletea_glyph_protocol_examples_match_reference_image` known-answer
        // test runs as part of this binary.
    }

    #[test]
    fn empty_outline_returns_empty_bitmap() {
        let bm = rasterize(
            &Outline {
                points: vec![],
                contours: vec![],
            },
            DesignMetrics {
                units_per_em: 1000,
                advance_width: 1000,
                line_height: 1000,
            },
            render_options(test_metrics(20, 20)),
        )
        .unwrap();

        assert_eq!(20u32, bm.width);
        assert_eq!(20u32, bm.height);
        assert_eq!(20usize * 20, bm.data.len());
        for v in &bm.data {
            assert_eq!(0u8, *v);
        }
    }

    #[test]
    fn square_fills_bitmap_center() {
        let outline = Outline {
            points: vec![
                GP {
                    x: 0,
                    y: 0,
                    on_curve: true,
                },
                GP {
                    x: 1000,
                    y: 0,
                    on_curve: true,
                },
                GP {
                    x: 1000,
                    y: 1000,
                    on_curve: true,
                },
                GP {
                    x: 0,
                    y: 1000,
                    on_curve: true,
                },
            ],
            contours: vec![3],
        };

        let bm = rasterize(
            &outline,
            DesignMetrics {
                units_per_em: 1000,
                advance_width: 1000,
                line_height: 1000,
            },
            render_options(test_metrics(20, 20)),
        )
        .unwrap();

        assert!(bm.data[10 * bm.width as usize + 10] > 200);
    }

    #[test]
    fn quadratic_contour_renders() {
        let outline = Outline {
            points: vec![
                GP {
                    x: 0,
                    y: 0,
                    on_curve: true,
                },
                GP {
                    x: 500,
                    y: 1000,
                    on_curve: false,
                },
                GP {
                    x: 1000,
                    y: 0,
                    on_curve: true,
                },
            ],
            contours: vec![2],
        };

        let bm = rasterize(
            &outline,
            DesignMetrics {
                units_per_em: 1000,
                advance_width: 1000,
                line_height: 1000,
            },
            render_options(test_metrics(20, 20)),
        )
        .unwrap();

        let mut nonzero = false;
        for v in &bm.data {
            nonzero = nonzero || *v != 0;
        }
        assert!(nonzero);
    }

    #[test]
    fn consecutive_off_curve_points_render() {
        let outline = Outline {
            points: vec![
                GP {
                    x: 0,
                    y: 0,
                    on_curve: true,
                },
                GP {
                    x: 250,
                    y: 1000,
                    on_curve: false,
                },
                GP {
                    x: 750,
                    y: 1000,
                    on_curve: false,
                },
                GP {
                    x: 1000,
                    y: 0,
                    on_curve: true,
                },
            ],
            contours: vec![3],
        };

        let bm = rasterize(
            &outline,
            DesignMetrics {
                units_per_em: 1000,
                advance_width: 1000,
                line_height: 1000,
            },
            render_options(test_metrics(20, 20)),
        )
        .unwrap();

        let mut nonzero = false;
        for v in &bm.data {
            nonzero = nonzero || *v != 0;
        }
        assert!(nonzero);
    }

    #[test]
    fn units_per_em_controls_baseline_scale() {
        let outline = Outline {
            points: vec![
                GP {
                    x: 0,
                    y: 0,
                    on_curve: true,
                },
                GP {
                    x: 1000,
                    y: 0,
                    on_curve: true,
                },
                GP {
                    x: 1000,
                    y: 1000,
                    on_curve: true,
                },
                GP {
                    x: 0,
                    y: 1000,
                    on_curve: true,
                },
            ],
            contours: vec![3],
        };

        let bm = rasterize(
            &outline,
            DesignMetrics {
                units_per_em: 2000,
                advance_width: 1000,
                line_height: 1000,
            },
            render_options(test_metrics(20, 20)),
        )
        .unwrap();

        // With a 2000-unit em in a 20px cell, this 1000-unit square occupies the
        // bottom half of the cell. This matches the linked rasterizer's y=0
        // baseline/bottom behavior and proves units_per_em is the scale reference.
        assert!(bm.data[15 * bm.width as usize + 5] > 200);
        assert_eq!(0u8, bm.data[5 * bm.width as usize + 5]);
    }

    #[test]
    fn degenerate_outline_returns_full_empty_bitmap() {
        let outline = Outline {
            points: vec![
                GP {
                    x: 0,
                    y: 0,
                    on_curve: true,
                },
                GP {
                    x: 1000,
                    y: 0,
                    on_curve: true,
                },
                GP {
                    x: 500,
                    y: 0,
                    on_curve: true,
                },
            ],
            contours: vec![2],
        };

        let bm = rasterize(
            &outline,
            DesignMetrics {
                units_per_em: 1000,
                advance_width: 1000,
                line_height: 1000,
            },
            render_options(test_metrics(20, 20)),
        )
        .unwrap();

        assert_eq!(20u32, bm.width);
        assert_eq!(20u32, bm.height);
        assert_eq!(20usize * 20, bm.data.len());
        for v in &bm.data {
            assert_eq!(0u8, *v);
        }
    }

    #[test]
    fn contour_can_start_off_curve_with_final_on_curve_point() {
        let outline = Outline {
            points: vec![
                GP {
                    x: 500,
                    y: 1000,
                    on_curve: false,
                },
                GP {
                    x: 1000,
                    y: 0,
                    on_curve: true,
                },
                GP {
                    x: 0,
                    y: 0,
                    on_curve: true,
                },
            ],
            contours: vec![2],
        };

        let bm = rasterize(
            &outline,
            DesignMetrics {
                units_per_em: 1000,
                advance_width: 1000,
                line_height: 1000,
            },
            render_options(test_metrics(20, 20)),
        )
        .unwrap();

        let mut nonzero = false;
        for v in &bm.data {
            nonzero = nonzero || *v != 0;
        }
        assert!(nonzero);
    }

    #[test]
    fn contour_can_start_with_implied_midpoint() {
        let outline = Outline {
            points: vec![
                GP {
                    x: 250,
                    y: 1000,
                    on_curve: false,
                },
                GP {
                    x: 1000,
                    y: 0,
                    on_curve: true,
                },
                GP {
                    x: 750,
                    y: 1000,
                    on_curve: false,
                },
                GP {
                    x: 0,
                    y: 0,
                    on_curve: true,
                },
            ],
            contours: vec![3],
        };

        let bm = rasterize(
            &outline,
            DesignMetrics {
                units_per_em: 1000,
                advance_width: 1000,
                line_height: 1000,
            },
            render_options(test_metrics(20, 20)),
        )
        .unwrap();

        let mut nonzero = false;
        for v in &bm.data {
            nonzero = nonzero || *v != 0;
        }
        assert!(nonzero);
    }

    #[test]
    fn multiple_contours_render_independently() {
        let outline = Outline {
            points: vec![
                GP {
                    x: 0,
                    y: 0,
                    on_curve: true,
                },
                GP {
                    x: 400,
                    y: 0,
                    on_curve: true,
                },
                GP {
                    x: 400,
                    y: 400,
                    on_curve: true,
                },
                GP {
                    x: 0,
                    y: 400,
                    on_curve: true,
                },
                GP {
                    x: 600,
                    y: 600,
                    on_curve: true,
                },
                GP {
                    x: 1000,
                    y: 600,
                    on_curve: true,
                },
                GP {
                    x: 1000,
                    y: 1000,
                    on_curve: true,
                },
                GP {
                    x: 600,
                    y: 1000,
                    on_curve: true,
                },
            ],
            contours: vec![3, 7],
        };

        let bm = rasterize(
            &outline,
            DesignMetrics {
                units_per_em: 1000,
                advance_width: 1000,
                line_height: 1000,
            },
            render_options(test_metrics(20, 20)),
        )
        .unwrap();

        assert!(bm.data[16 * bm.width as usize + 4] > 200);
        assert!(bm.data[4 * bm.width as usize + 16] > 200);
        assert_eq!(0u8, bm.data[10 * bm.width as usize + 10]);
    }

    #[test]
    fn non_zero_bearings_preserve_declared_whitespace() {
        let outline = Outline {
            points: vec![
                GP {
                    x: 250,
                    y: 0,
                    on_curve: true,
                },
                GP {
                    x: 750,
                    y: 0,
                    on_curve: true,
                },
                GP {
                    x: 750,
                    y: 1000,
                    on_curve: true,
                },
                GP {
                    x: 250,
                    y: 1000,
                    on_curve: true,
                },
            ],
            contours: vec![3],
        };

        let bm = rasterize(
            &outline,
            DesignMetrics {
                units_per_em: 1000,
                advance_width: 1000,
                line_height: 1000,
            },
            render_options(test_metrics(20, 20)),
        )
        .unwrap();

        assert_eq!(0u8, bm.data[10 * bm.width as usize + 2]);
        assert!(bm.data[10 * bm.width as usize + 10] > 200);
        assert_eq!(0u8, bm.data[10 * bm.width as usize + 17]);
    }

    #[test]
    fn negative_y_coordinates_descend_below_baseline() {
        let outline = Outline {
            points: vec![
                GP {
                    x: 0,
                    y: -250,
                    on_curve: true,
                },
                GP {
                    x: 1000,
                    y: -250,
                    on_curve: true,
                },
                GP {
                    x: 1000,
                    y: 750,
                    on_curve: true,
                },
                GP {
                    x: 0,
                    y: 750,
                    on_curve: true,
                },
            ],
            contours: vec![3],
        };

        let bm = rasterize(
            &outline,
            DesignMetrics {
                units_per_em: 1000,
                advance_width: 1000,
                line_height: 1000,
            },
            render_options(test_metrics(20, 20)),
        )
        .unwrap();

        assert_eq!(0u8, bm.data[2 * bm.width as usize + 10]);
        assert!(bm.data[10 * bm.width as usize + 10] > 200);
        assert!(bm.data[18 * bm.width as usize + 10] > 200);
    }

    #[test]
    fn two_cell_bitmap_and_constraint_render_within_width() {
        let outline = Outline {
            points: vec![
                GP {
                    x: 0,
                    y: 0,
                    on_curve: true,
                },
                GP {
                    x: 1000,
                    y: 0,
                    on_curve: true,
                },
                GP {
                    x: 1000,
                    y: 1000,
                    on_curve: true,
                },
                GP {
                    x: 0,
                    y: 1000,
                    on_curve: true,
                },
            ],
            contours: vec![3],
        };

        let bm = rasterize(
            &outline,
            DesignMetrics {
                units_per_em: 1000,
                advance_width: 1000,
                line_height: 1000,
            },
            RenderOptions {
                grid_metrics: test_metrics(20, 20),
                cell_width: Some(2),
                constraint_width: 2,
                constraint: glyph::Constraint {
                    size: glyph::ConstraintSize::Cover,
                    align_horizontal: glyph::Align::Center,
                    align_vertical: glyph::Align::Center,
                    ..glyph::Constraint::none
                },
                ..RenderOptions::default()
            },
        )
        .unwrap();

        assert_eq!(40u32, bm.width);
        assert_eq!(20u32, bm.height);
        assert_eq!(0u8, bm.data[10 * bm.width as usize + 2]);
        assert!(bm.data[10 * bm.width as usize + 20] > 200);
        assert_eq!(0u8, bm.data[10 * bm.width as usize + 37]);
    }

    #[test]
    fn line_height_does_not_change_unconstrained_em_scale() {
        let outline = Outline {
            points: vec![
                GP {
                    x: 0,
                    y: 0,
                    on_curve: true,
                },
                GP {
                    x: 1000,
                    y: 0,
                    on_curve: true,
                },
                GP {
                    x: 1000,
                    y: 1000,
                    on_curve: true,
                },
                GP {
                    x: 0,
                    y: 1000,
                    on_curve: true,
                },
            ],
            contours: vec![3],
        };

        let bm = rasterize(
            &outline,
            DesignMetrics {
                units_per_em: 1000,
                advance_width: 1000,
                line_height: 2000,
            },
            render_options(test_metrics(20, 20)),
        )
        .unwrap();

        assert!(bm.data[10 * bm.width as usize + 10] > 200);
        assert!(bm.data[2 * bm.width as usize + 10] > 200);
        assert!(bm.data[17 * bm.width as usize + 10] > 200);
    }
}
