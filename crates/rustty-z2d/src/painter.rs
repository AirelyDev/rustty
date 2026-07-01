// SPDX-License-Identifier: MPL-2.0
//   Copyright © 2024-2025 Chris Marchesi
//
// Ported to Rust for rustty from z2d 0.10.0 `src/painter.zig` (R3 vendored dep).
//
// SUBSET NOTE (rustty): the FILL operation is ported for both the non-anti-
// aliased (`.none`) rasterizer (`internal/raster/direct.rs`) and the default
// anti-aliased (`.default` / `.multisample_4x`) rasterizer
// (`internal/raster/multisample.rs` + `sparse_coverage.rs` + the scalar integer
// compositor), which together are the pixel-producing core ghostty's glyf/sprite
// fills use. The STROKE operation is ported for the (non-hairline, non-dashed)
// path the sprite canvas exercises — `internal/tess/stroke_plotter.rs` +
// `Pen`/`Face`/`Slope` + `Contour`, fed into the same `.none`/`.default`
// rasterizers as fill (butt/round caps, miter/bevel/round joins). The
// `.supersample_4x` rasterizer, hairline stroking (`polyline_plotter` +
// `hairline` — never reached by the sprite canvas, which leaves `hairline`
// false), and dashed stroking (`Dasher`/`dashed_plotter`) are NOT part of this
// subset; those code paths are `unimplemented!` (and reported as blockers).

//! Contains unmanaged painter functions for filling and stroking.

use crate::compositor::{Operator, Precision};
use crate::internal::error::InternalError;
use crate::internal::path_nodes::PathNode;
use crate::internal::raster::{direct, multisample};
use crate::internal::tess::{fill_plotter, stroke_plotter};
use crate::options::{self, AntiAliasMode, CapMode, FillRule, JoinMode};
use crate::pattern::Pattern;
use crate::surface::{self, Surface, SUPERSAMPLE_SCALE};
use crate::transformation::{self, Transformation, IDENTITY};

/// The multisample rasterizer works at 4x scale.
const MULTISAMPLE_SCALE: f64 = 4.0;

/// Options for the `fill` operation.
#[derive(Debug, Clone, Copy)]
pub struct FillOptions {
    /// The anti-aliasing mode to use with the fill operation.
    pub anti_aliasing_mode: AntiAliasMode,
    /// The fill rule to use during the fill operation.
    pub fill_rule: FillRule,
    /// The operator to use for compositing.
    pub operator: Operator,
    /// The precision to use when compositing.
    pub precision: Precision,
    /// The maximum error tolerance used for approximating curves and arcs.
    pub tolerance: f64,
}

impl Default for FillOptions {
    fn default() -> FillOptions {
        FillOptions {
            anti_aliasing_mode: AntiAliasMode::Default,
            fill_rule: FillRule::NonZero,
            operator: Operator::SrcOver,
            precision: Precision::Integer,
            tolerance: options::DEFAULT_TOLERANCE,
        }
    }
}

/// Errors related to the `fill` operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FillError {
    /// The supplied path (and any sub-paths) have not been explicitly closed,
    /// which is required by the fill operation.
    PathNotClosed,
    Surface(surface::Error),
    Internal(InternalError),
}

impl From<InternalError> for FillError {
    fn from(e: InternalError) -> FillError {
        FillError::Internal(e)
    }
}

/// Runs a fill operation on the path set represented by `nodes`.
pub fn fill(
    surface: &mut Surface,
    pattern: &Pattern,
    nodes: &[PathNode],
    opts: FillOptions,
) -> Result<(), FillError> {
    if nodes.is_empty() {
        return Ok(());
    }
    if !PathNode::is_closed_node_set(nodes) {
        return Err(FillError::PathNotClosed);
    }

    // NOTE (subset): the Zig original forces AA to `.none` on 1-bit alpha
    // surfaces; this subset has no packed alpha1 surface, so the option is used
    // as-is.
    let aa_mode = opts.anti_aliasing_mode;

    let scale: f64 = match aa_mode {
        AntiAliasMode::None => 1.0,
        AntiAliasMode::Supersample4x => SUPERSAMPLE_SCALE as f64,
        AntiAliasMode::Default | AntiAliasMode::Multisample4x => MULTISAMPLE_SCALE,
    };

    let polygons = fill_plotter::plot(nodes, scale, opts.tolerance.max(0.001))?;

    match aa_mode {
        AntiAliasMode::None => {
            direct::run(
                surface,
                pattern,
                &polygons,
                opts.fill_rule,
                opts.operator,
                opts.precision,
            );
        }
        AntiAliasMode::Default | AntiAliasMode::Multisample4x => {
            multisample::run(
                surface,
                pattern,
                &polygons,
                opts.fill_rule,
                opts.operator,
                opts.precision,
            );
        }
        AntiAliasMode::Supersample4x => {
            unimplemented!("anti-aliased fill (supersample) is not part of the initial z2d subset");
        }
    }

    Ok(())
}

/// Options for the `stroke` operation.
#[derive(Debug, Clone)]
pub struct StrokeOptions {
    /// The anti-aliasing mode to use with the stroke operation.
    pub anti_aliasing_mode: AntiAliasMode,
    /// The dash array, if dashed lines are desired.
    pub dashes: Vec<f64>,
    /// The dash offset when doing dashed lines.
    pub dash_offset: f64,
    /// The line cap rule for the stroke operation.
    pub line_cap_mode: CapMode,
    /// The line join style for the stroke operation.
    pub line_join_mode: JoinMode,
    /// The line width for the stroke operation.
    pub line_width: f64,
    /// The maximum allowed ratio for miter joins.
    pub miter_limit: f64,
    /// The operator to use for compositing.
    pub operator: Operator,
    /// The precision to use when compositing.
    pub precision: Precision,
    /// The maximum error tolerance used for approximating curves and arcs.
    pub tolerance: f64,
    /// The transformation matrix to use for the stroke operation.
    pub transformation: Transformation,
    /// Sets hairline mode.
    pub hairline: bool,
}

impl Default for StrokeOptions {
    fn default() -> StrokeOptions {
        StrokeOptions {
            anti_aliasing_mode: AntiAliasMode::Default,
            dashes: Vec::new(),
            dash_offset: 0.0,
            line_cap_mode: CapMode::Butt,
            line_join_mode: JoinMode::Miter,
            line_width: 2.0,
            miter_limit: 10.0,
            operator: Operator::SrcOver,
            precision: Precision::Integer,
            tolerance: options::DEFAULT_TOLERANCE,
            transformation: IDENTITY,
            hairline: false,
        }
    }
}

/// Errors related to the `stroke` operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrokeError {
    InvalidMatrix,
    Surface(surface::Error),
    Internal(InternalError),
}

impl From<transformation::Error> for StrokeError {
    fn from(_: transformation::Error) -> StrokeError {
        StrokeError::InvalidMatrix
    }
}

impl From<InternalError> for StrokeError {
    fn from(e: InternalError) -> StrokeError {
        StrokeError::Internal(e)
    }
}

/// Runs a stroke operation on the path set represented by `nodes`. The path(s)
/// is/are transformed to one or more polygon(s) representing the line(s),
/// which are then filled.
pub fn stroke(
    surface: &mut Surface,
    pattern: &Pattern,
    nodes: &[PathNode],
    opts: StrokeOptions,
) -> Result<(), StrokeError> {
    // Attempt to inverse the matrix supplied to ensure it can be inverted
    // farther down.
    opts.transformation.inverse()?;

    // Return if called with zero nodes.
    if nodes.is_empty() {
        return Ok(());
    }

    // NOTE (subset): the Zig original forces AA to `.none` on 1-bit alpha
    // surfaces; this subset has no packed alpha1 surface, so the option is used
    // as-is.
    let aa_mode = opts.anti_aliasing_mode;

    // We can fast-path here if we're using hairline stroking.
    if opts.hairline {
        unimplemented!("hairline stroking is not part of the z2d sprite subset");
    }

    let scale: f64 = match aa_mode {
        AntiAliasMode::None => 1.0,
        AntiAliasMode::Supersample4x => SUPERSAMPLE_SCALE as f64,
        AntiAliasMode::Default | AntiAliasMode::Multisample4x => MULTISAMPLE_SCALE,
    };

    // NOTE: for now, we set a minimum thickness for the following options:
    // join_mode, miter_limit, and cap_mode. Any thickness lower than 2 will
    // cause these options to revert to the defaults of join_mode = .miter,
    // miter_limit = 10.0, cap_mode = .butt.
    //
    // This is a stop-gap to prevent artifacts with very thin lines (not
    // necessarily hairline, but close to being the single-pixel width that are
    // used to represent hairlines). As our path builder gets better for
    // stroking, I'm expecting that some of these restrictions will be lifted
    // and/or moved to specific places where they can be used to address the
    // artifacts related to particular edge cases.
    //
    // Not a stop gap more than likely: minimum line width. This is value is
    // sort of arbitrarily chosen as Cairo's minimum 24.8 fixed-point value (so
    // 1/256).
    let minimum_line_width: f64 = 0.00390625;
    let polygons = stroke_plotter::plot(
        nodes,
        stroke_plotter::PlotterOptions {
            cap_mode: if opts.line_width >= 2.0 {
                opts.line_cap_mode
            } else {
                CapMode::Butt
            },
            ctm: opts.transformation,
            dashes: opts.dashes.clone(),
            dash_offset: opts.dash_offset,
            join_mode: if opts.line_width >= 2.0 {
                opts.line_join_mode
            } else {
                JoinMode::Miter
            },
            miter_limit: if opts.line_width >= 2.0 {
                opts.miter_limit
            } else {
                10.0
            },
            scale,
            thickness: if opts.line_width >= minimum_line_width {
                opts.line_width
            } else {
                minimum_line_width
            },
            tolerance: opts.tolerance.max(0.001),
        },
    )?;

    match aa_mode {
        AntiAliasMode::None => {
            direct::run(
                surface,
                pattern,
                &polygons,
                FillRule::NonZero,
                opts.operator,
                opts.precision,
            );
        }
        AntiAliasMode::Default | AntiAliasMode::Multisample4x => {
            multisample::run(
                surface,
                pattern,
                &polygons,
                FillRule::NonZero,
                opts.operator,
                opts.precision,
            );
        }
        AntiAliasMode::Supersample4x => {
            unimplemented!(
                "anti-aliased stroke (supersample) is not part of the initial z2d subset"
            );
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pattern::OpaquePattern;
    use crate::pixel::{Pixel, Rgb};

    #[test]
    fn stroke_uninvertible_matrix_error() {
        let mut sfc = Surface::init(surface::SurfaceType::ImageSurfaceRgb, 1, 1).unwrap();
        let pattern = Pattern::OpaquePattern(OpaquePattern {
            pixel: Pixel::Rgb(Rgb {
                r: 0xFF,
                g: 0xFF,
                b: 0xFF,
            }), // White on black
        });
        let opts = StrokeOptions {
            transformation: Transformation {
                ax: 1.0,
                by: 1.0,
                cx: 2.0,
                dy: 2.0,
                tx: 5.0,
                ty: 6.0,
            },
            ..StrokeOptions::default()
        };
        assert_eq!(
            Err(StrokeError::InvalidMatrix),
            stroke(&mut sfc, &pattern, &[], opts)
        );
        sfc.deinit();
    }
}
