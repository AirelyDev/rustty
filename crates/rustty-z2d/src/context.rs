// SPDX-License-Identifier: MPL-2.0
//   Copyright © 2024-2025 Chris Marchesi
//
// Ported to Rust for rustty from z2d 0.10.0 `src/Context.zig` (R3 vendored dep).
//
// SUBSET NOTE (rustty): ghostty's sprite canvas acquires a `Context` both to set
// the padding transformation AND (in `branch`/`symbols_for_legacy_computing`/
// `special`) to drive the managed drawing API — `set_source`, `set_line_width`,
// `line_cap_mode`, `move_to`/`line_to`/`curve_to`/`arc`/`close_path`, `fill`,
// `stroke`. Those are ported here, delegating fill/stroke to the unmanaged
// `painter::fill` / `painter::stroke` (the same pixel-producing core the sprite
// canvas already uses directly).
//
// The `font`/`showText` frontend (needs `Font`/`text`), dithering (needs
// `Dither`), and gradient sources (needs `gradient.zig`) are NOT part of this
// subset — those types are not ported — so the corresponding fields/methods are
// omitted. The `pattern` field therefore only ever holds the opaque source the
// sprite canvas sets, and `wrapDither` (which would wrap the pattern for
// dithering) is unnecessary: `fill`/`stroke` pass `self.pattern` directly.

//! Context patterns and state for drawing.

use crate::compositor::{Operator, Precision};
use crate::options::{self, AntiAliasMode, CapMode, FillRule, JoinMode};
use crate::painter::{self, FillOptions, StrokeOptions};
use crate::path::{self, Path};
use crate::pattern::{OpaquePattern, Pattern};
use crate::pixel::{Pixel, Rgba};
use crate::surface::Surface;
use crate::transformation::{Transformation, IDENTITY};

/// A managed drawing context bound to a mutable `Surface`.
///
/// Every field is controllable via setters or other frontend methods. It is
/// recommended you do not manipulate these fields directly. If you do wish
/// further control over the process than what `Context` provides, you can use
/// each underlying component in an unmanaged fashion by following the patterns
/// used here as an example. (The exception observed by ghostty is
/// `line_cap_mode`, which it sets directly.)
pub struct Context<'a> {
    /// The surface this context draws to.
    pub surface: &'a mut Surface,
    /// The managed path builder.
    pub path: Path,
    /// The source pattern; fill and stroke operations draw with this source.
    /// The default is an RGBA opaque black pixel source.
    pub pattern: Pattern,

    /// The anti-aliasing mode used for fill and stroke operations.
    pub anti_aliasing_mode: AntiAliasMode,
    /// The dash array, if dashed lines are desired.
    pub dashes: Vec<f64>,
    /// The dash offset for dashed lines.
    pub dash_offset: f64,
    /// How edges are counted during fill operations.
    pub fill_rule: FillRule,
    /// Whether hairline stroking is enabled.
    pub hairline: bool,
    /// How the ends of lines are drawn during stroke operations ("capping").
    pub line_cap_mode: CapMode,
    /// How lines are joined during stroke operations.
    pub line_join_mode: JoinMode,
    /// The line width for stroking operations, in pixels.
    pub line_width: f64,
    /// The limit when the line join mode is `.miter`.
    pub miter_limit: f64,
    /// The compositing operator used for drawing operations.
    pub operator: Operator,
    /// The compositing precision used for fill and stroke.
    pub precision: Precision,
    /// The maximum error tolerance used for approximating curves and arcs.
    pub tolerance: f64,
    /// The current transformation matrix (CTM).
    pub transformation: Transformation,
}

impl<'a> Context<'a> {
    /// Initializes a `Context` with the passed in surface. Call `deinit` to
    /// release any resources managed solely by the context, such as the managed
    /// `Path`.
    pub fn init(surface: &'a mut Surface) -> Context<'a> {
        Context {
            surface,
            path: Path::empty(),
            pattern: Pattern::OpaquePattern(OpaquePattern {
                pixel: Pixel::Rgba(Rgba {
                    r: 0x00,
                    g: 0x00,
                    b: 0x00,
                    a: 0xFF,
                }),
            }),
            anti_aliasing_mode: AntiAliasMode::Default,
            dashes: Vec::new(),
            dash_offset: 0.0,
            fill_rule: FillRule::NonZero,
            hairline: false,
            line_cap_mode: CapMode::Butt,
            line_join_mode: JoinMode::Miter,
            line_width: 2.0,
            miter_limit: 10.0,
            operator: Operator::SrcOver,
            precision: Precision::Integer,
            tolerance: options::DEFAULT_TOLERANCE,
            transformation: IDENTITY,
        }
    }

    /// Releases the context's managed path (Rust: dropped). The surface is not
    /// owned by the context and is left intact.
    pub fn deinit(&mut self) {
        self.path.reset();
    }

    /// Returns the current source pattern for the context.
    pub fn get_source(&self) -> Pattern {
        self.pattern
    }

    /// Sets the context's pattern to the pattern supplied; fill and stroke
    /// operations will draw with this source when they are called.
    ///
    /// The default pattern is an RGBA opaque black pixel source.
    pub fn set_source(&mut self, source: Pattern) {
        self.pattern = source;
    }

    /// Sets the context's pattern to the supplied `Pixel`. Fill and stroke
    /// operations will draw with this pixel when they are called.
    pub fn set_source_to_pixel(&mut self, px: Pixel) {
        self.pattern = Pattern::OpaquePattern(OpaquePattern { pixel: px });
    }

    /// Returns the current fill rule for the context.
    pub fn get_fill_rule(&self) -> FillRule {
        self.fill_rule
    }

    /// Sets how edges are counted during fill operations. The default mode is
    /// `.non_zero`.
    pub fn set_fill_rule(&mut self, fill_rule: FillRule) {
        self.fill_rule = fill_rule;
    }

    /// Returns the current line cap mode for the context.
    pub fn get_line_cap_mode(&self) -> CapMode {
        self.line_cap_mode
    }

    /// Sets how the ends of lines are drawn during stroke operations, a process
    /// known as "capping". The default cap mode is `.butt`.
    pub fn set_line_cap_mode(&mut self, line_cap_mode: CapMode) {
        self.line_cap_mode = line_cap_mode;
    }

    /// Returns the current line width for the context.
    pub fn get_line_width(&self) -> f64 {
        self.line_width
    }

    /// Sets the line width for stroking operations, in pixels. This value is
    /// taken at call time of `stroke`, and has no effect during path
    /// construction.
    pub fn set_line_width(&mut self, line_width: f64) {
        self.line_width = line_width;
    }

    /// Returns the current transformation matrix (CTM) for the context.
    pub fn get_transformation(&self) -> Transformation {
        self.transformation
    }

    /// Modifies the current transformation matrix (CTM) by setting it equal to
    /// the supplied matrix, also applying it to the managed path builder
    /// (mirroring z2d's `setTransformation`).
    pub fn set_transformation(&mut self, transformation: Transformation) {
        self.transformation = transformation;
        self.path.transformation = transformation;
    }

    /// Rests the path set, clearing all nodes and state.
    pub fn reset_path(&mut self) {
        self.path.reset();
    }

    /// Starts a new path, and moves the current point to it. If a path already
    /// has been started, a new sub-path is started. Note that this does not
    /// clear any existing paths or nodes, use `reset_path` to accomplish this.
    pub fn move_to(&mut self, x: f64, y: f64) {
        self.path.move_to(x, y);
    }

    /// Draws a line from the current point to the specified point and sets it as
    /// the current point. Acts as a `move_to` instead if there is no current
    /// point.
    pub fn line_to(&mut self, x: f64, y: f64) {
        self.path.line_to(x, y);
    }

    /// Draws a cubic bezier with the three supplied control points from the
    /// current point. The new current point is set to (`x3`, `y3`). It is an
    /// error to call this without a current point.
    #[allow(clippy::too_many_arguments)]
    pub fn curve_to(
        &mut self,
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        x3: f64,
        y3: f64,
    ) -> Result<(), path::Error> {
        self.path.curve_to(x1, y1, x2, y2, x3, y3)
    }

    /// Adds a circular arc of the given radius to the current path. The arc is
    /// centered at (`xc`, `yc`), begins at `angle1` and proceeds in the
    /// direction of increasing angles (i.e., counterclockwise direction) to end
    /// at `angle2`.
    ///
    /// If `angle2` is less than `angle1`, it will be increased by 2 * Π until
    /// it's greater than `angle1`.
    ///
    /// Angles are measured at radians (to convert from degrees, multiply by Π /
    /// 180).
    ///
    /// If there's a current point, an initial line segment will be added to the
    /// path to connect the current point to the beginning of the arc. If this
    /// behavior is undesired, call `reset_path` before calling. This will
    /// trigger a `move_to` before the splines are plotted, creating a new
    /// subpath.
    ///
    /// After this operation, the current point will be the end of the arc.
    pub fn arc(
        &mut self,
        xc: f64,
        yc: f64,
        radius: f64,
        angle1: f64,
        angle2: f64,
    ) -> Result<(), path::Error> {
        self.path.arc(xc, yc, radius, angle1, angle2)
    }

    /// Like `arc`, but draws in the reverse direction, i.e., begins at `angle1`,
    /// and moves in decreasing angles (i.e., counterclockwise direction) to end
    /// at `angle2`. If `angle2` is greater than `angle1`, it will be decreased
    /// by 2 * Π until it's less than `angle1`.
    pub fn arc_negative(
        &mut self,
        xc: f64,
        yc: f64,
        radius: f64,
        angle1: f64,
        angle2: f64,
    ) -> Result<(), path::Error> {
        self.path.arc_negative(xc, yc, radius, angle1, angle2)
    }

    /// Closes the path by drawing a line from the current point by the starting
    /// point. No effect if there is no current point.
    pub fn close_path(&mut self) {
        self.path.close();
    }

    /// Returns `true` if all subpaths in the context's path set are currently
    /// closed.
    pub fn is_path_closed(&self) -> bool {
        self.path.is_closed()
    }

    /// Runs a fill operation for the current path and any subpaths. All paths in
    /// the set must be closed. This is a no-op if there are no nodes.
    pub fn fill(&mut self) -> Result<(), painter::FillError> {
        let opts = FillOptions {
            anti_aliasing_mode: self.anti_aliasing_mode,
            fill_rule: self.fill_rule,
            operator: self.operator,
            precision: self.precision,
            tolerance: self.tolerance,
        };
        painter::fill(&mut *self.surface, &self.pattern, &self.path.nodes, opts)
    }

    /// Strokes a line for the current path and any subpaths.
    ///
    /// The behavior of open and closed paths are different for stroking. For
    /// open paths (not explicitly closed with `close_path`), the start and the
    /// end of the line are capped using the style set with `set_line_cap_mode`.
    /// For closed paths (ones that *are* explicitly closed with `close_path`),
    /// the intersection joint of the start and end are instead joined, as with
    /// all other joints along the way, with the style set in
    /// `set_line_join_mode`.
    ///
    /// This is a no-op if there are no nodes.
    pub fn stroke(&mut self) -> Result<(), painter::StrokeError> {
        let opts = StrokeOptions {
            anti_aliasing_mode: self.anti_aliasing_mode,
            dashes: self.dashes.clone(),
            dash_offset: self.dash_offset,
            line_cap_mode: self.line_cap_mode,
            line_join_mode: self.line_join_mode,
            line_width: self.line_width,
            miter_limit: self.miter_limit,
            operator: self.operator,
            precision: self.precision,
            tolerance: self.tolerance,
            transformation: self.transformation,
            hairline: self.hairline,
        };
        painter::stroke(&mut *self.surface, &self.pattern, &self.path.nodes, opts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pixel::Alpha8;
    use crate::surface::SurfaceType;

    // Returns the alpha value at (x, y) for an alpha8 surface.
    fn alpha_at(sfc: &Surface, x: i32, y: i32) -> u8 {
        match sfc.get_pixel(x, y).unwrap() {
            Pixel::Alpha8(a) => a.a,
            other => panic!("expected alpha8, got {other:?}"),
        }
    }

    #[test]
    fn init_defaults() {
        let mut sfc = Surface::init(SurfaceType::ImageSurfaceAlpha8, 4, 4).unwrap();
        let ctx = Context::init(&mut sfc);
        assert_eq!(
            Pattern::OpaquePattern(OpaquePattern {
                pixel: Pixel::Rgba(Rgba {
                    r: 0,
                    g: 0,
                    b: 0,
                    a: 0xFF
                })
            }),
            ctx.get_source()
        );
        assert_eq!(FillRule::NonZero, ctx.get_fill_rule());
        assert_eq!(CapMode::Butt, ctx.get_line_cap_mode());
        assert_eq!(2.0, ctx.get_line_width());
        assert_eq!(IDENTITY, ctx.get_transformation());
        assert!(ctx.path.nodes.is_empty());
    }

    #[test]
    fn build_and_reset_path() {
        let mut sfc = Surface::init(SurfaceType::ImageSurfaceAlpha8, 4, 4).unwrap();
        let mut ctx = Context::init(&mut sfc);
        ctx.move_to(1.0, 1.0);
        ctx.line_to(3.0, 1.0);
        ctx.line_to(3.0, 3.0);
        assert_eq!(3, ctx.path.nodes.len());
        assert!(!ctx.is_path_closed());
        ctx.close_path();
        assert!(ctx.is_path_closed());
        ctx.reset_path();
        assert!(ctx.path.nodes.is_empty());
    }

    #[test]
    fn set_source_and_fill_moved_lined_closed_path() {
        // Fill a solid square with a white opaque source on an alpha8 surface;
        // the interior must be fully covered (alpha 0xFF) and a point well
        // outside must remain untouched (alpha 0).
        let mut sfc = Surface::init(SurfaceType::ImageSurfaceAlpha8, 10, 10).unwrap();
        let mut ctx = Context::init(&mut sfc);
        ctx.set_source(Pattern::OpaquePattern(OpaquePattern {
            pixel: Pixel::Alpha8(Alpha8 { a: 0xFF }),
        }));
        // Use the non-anti-aliased rasterizer for an exact interior.
        ctx.anti_aliasing_mode = AntiAliasMode::None;
        ctx.move_to(2.0, 2.0);
        ctx.line_to(8.0, 2.0);
        ctx.line_to(8.0, 8.0);
        ctx.line_to(2.0, 8.0);
        ctx.close_path();
        ctx.fill().unwrap();
        assert_eq!(0xFF, alpha_at(ctx.surface, 5, 5));
        assert_eq!(0x00, alpha_at(ctx.surface, 0, 0));
    }

    #[test]
    fn stroke_a_curve() {
        // Stroke an open cubic bezier; the operation must succeed and lay down
        // some coverage somewhere on the surface.
        let mut sfc = Surface::init(SurfaceType::ImageSurfaceAlpha8, 32, 32).unwrap();
        let mut ctx = Context::init(&mut sfc);
        ctx.set_source(Pattern::OpaquePattern(OpaquePattern {
            pixel: Pixel::Alpha8(Alpha8 { a: 0xFF }),
        }));
        ctx.set_line_width(2.0);
        ctx.move_to(4.0, 16.0);
        ctx.curve_to(10.0, 2.0, 22.0, 30.0, 28.0, 16.0).unwrap();
        ctx.stroke().unwrap();
        let mut any = false;
        for y in 0..32 {
            for x in 0..32 {
                if alpha_at(ctx.surface, x, y) != 0 {
                    any = true;
                }
            }
        }
        assert!(any, "stroke laid down no coverage");
    }

    #[test]
    fn arc_a_circle_fills() {
        // Arc a full circle, close it, and fill it. The center must be covered.
        let mut sfc = Surface::init(SurfaceType::ImageSurfaceAlpha8, 32, 32).unwrap();
        let mut ctx = Context::init(&mut sfc);
        ctx.set_source(Pattern::OpaquePattern(OpaquePattern {
            pixel: Pixel::Alpha8(Alpha8 { a: 0xFF }),
        }));
        ctx.anti_aliasing_mode = AntiAliasMode::None;
        ctx.arc(16.0, 16.0, 10.0, 0.0, std::f64::consts::PI * 2.0)
            .unwrap();
        ctx.close_path();
        // The arc must have produced spline nodes.
        assert!(ctx.path.nodes.len() > 2);
        ctx.fill().unwrap();
        assert_eq!(0xFF, alpha_at(ctx.surface, 16, 16));
        // A corner well outside the circle must remain empty.
        assert_eq!(0x00, alpha_at(ctx.surface, 0, 0));
    }

    #[test]
    fn arc_negative_reverse_direction() {
        // arc and arc_negative over the same angular span produce the same set
        // of endpoints, but the negative variant sweeps the other way. Both
        // must build a non-trivial spline path.
        let mut sfc = Surface::init(SurfaceType::ImageSurfaceAlpha8, 32, 32).unwrap();
        let mut ctx = Context::init(&mut sfc);
        ctx.arc_negative(16.0, 16.0, 10.0, std::f64::consts::PI * 2.0, 0.0)
            .unwrap();
        assert!(ctx.path.nodes.len() > 2);
    }
}
