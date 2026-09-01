// SPDX-License-Identifier: MPL-2.0
//   Copyright © 2024-2025 Chris Marchesi
//
// Ported to Rust for rustty from z2d 0.10.0 `src/internal/tess/stroke_plotter.zig`
// (R3 vendored dep).
//
// PORT NOTE (rustty): `plot` routes to `dashed_plotter::plot` when the dash
// array validates, exactly as the reference does. The four helpers the Zig
// declares as generic over the plotter type `T` — `plotSingle` /
// `plotOpenJoined` / `plotClosedJoined` / `join` — are ported as free functions
// generic over a `StrokePlotter` trait, so the dashed plotter can reuse them
// against BOTH its `Plotter` and its nested `InitialPolygon` (via a view), which
// is the whole reason the Zig makes them `fn(T: type, self: *T, ...)`. The trait
// exposes the mutable stroke state (opts/pen/clockwise/result/outer/inner) in
// one shot via `StrokeState` so the helpers can hold the disjoint field borrows
// the polygon surgery needs (concat outer+inner, add-edges from both). The Zig
// `PlotterVTable` (type-erased `line_to` callbacks for spline decomposition and
// cap plotting) and the `Joiner` vtable (outer/inner target swap) are expressed
// as `&mut dyn FnMut(...)` sinks / `join_plot_outer`/`join_plot_inner` helpers —
// equivalent behavior, no raw pointers, which keeps the Miri gate clean.

use crate::internal::error::InternalError;
use crate::internal::path_nodes::{PathCurveTo, PathLineTo, PathMoveTo, PathNode};
use crate::internal::point::Point;
use crate::internal::tess::dashed_plotter;
use crate::internal::tess::dasher;
use crate::internal::tess::face::Face;
use crate::internal::tess::pen::Pen;
use crate::internal::tess::point_buffer::PointBuffer;
use crate::internal::tess::polygon::{Contour, Polygon};
use crate::internal::tess::slope::Slope;
use crate::internal::tess::spline::Spline;
use crate::options::{CapMode, JoinMode};
use crate::transformation::Transformation;

pub struct PlotterOptions {
    pub cap_mode: CapMode,
    pub ctm: Transformation,
    pub dashes: Vec<f64>,
    pub dash_offset: f64,
    pub join_mode: JoinMode,
    pub miter_limit: f64,
    pub scale: f64,
    pub thickness: f64,
    pub tolerance: f64,
}

pub fn plot(nodes: &[PathNode], opts: PlotterOptions) -> Result<Polygon, InternalError> {
    if dasher::validate(&opts.dashes) {
        return dashed_plotter::plot(nodes, opts);
    }

    let pen = if opts.join_mode == JoinMode::Round || opts.cap_mode == CapMode::Round {
        Some(Pen::init(opts.thickness, opts.tolerance, opts.ctm))
    } else {
        None
    };

    let mut plotter = Plotter {
        nodes,
        opts: &opts,

        pen,

        points: PointBuffer::default(),
        clockwise_: None,

        result: Polygon::new(1.0),
        outer: Contour::new(opts.scale),
        inner: Contour::new(opts.scale),
    };

    plotter.run()?;

    Ok(plotter.result)
}

/// A bundle of disjoint mutable references to a stroke plotter's mutable state,
/// as required by the generic `plot_single`/`plot_open_joined`/
/// `plot_closed_joined`/`join` helpers. This is what lets the helpers be generic
/// over both `stroke_plotter::Plotter` and `dashed_plotter`'s `InitialPolygon`
/// (via a view that stitches the initial polygon's fields to the plotter's
/// `result`): each implementor hands back disjoint borrows of its own fields in
/// a single call, so the borrow checker permits the simultaneous outer+inner and
/// result+outer accesses the polygon surgery performs.
pub struct StrokeState<'s> {
    pub opts: &'s PlotterOptions,
    pub pen: Option<&'s Pen>,
    pub clockwise_: &'s mut Option<bool>,
    pub result: &'s mut Polygon,
    pub outer: &'s mut Contour,
    pub inner: &'s mut Contour,
}

/// Implemented by anything the generic stroke helpers can plot into. Faithful to
/// the Zig `fn(T: type, self: *T, ...)` shape, but exposing the touched fields
/// (`opts`/`pen`/`clockwise_`/`result`/`outer`/`inner`) in one borrow.
pub trait StrokePlotter {
    fn state(&mut self) -> StrokeState<'_>;
}

impl StrokePlotter for Plotter<'_> {
    fn state(&mut self) -> StrokeState<'_> {
        StrokeState {
            opts: self.opts,
            pen: self.pen.as_ref(),
            clockwise_: &mut self.clockwise_,
            result: &mut self.result,
            outer: &mut self.outer,
            inner: &mut self.inner,
        }
    }
}

struct Plotter<'a> {
    nodes: &'a [PathNode],
    opts: &'a PlotterOptions,

    pen: Option<Pen>, // pen (lazy-initialized)

    points: PointBuffer<2, 5>, // point buffer (initial and join points)
    clockwise_: Option<bool>,  // clockwise state

    result: Polygon, // Result polygon
    outer: Contour,  // Current polygon (outer)
    inner: Contour,  // Current polygon (inner)
}

impl Plotter<'_> {
    fn run(&mut self) -> Result<(), InternalError> {
        for idx in 0..self.nodes.len() {
            let node = self.nodes[idx];
            match node {
                PathNode::MoveTo(n) => self.run_move_to(n),
                PathNode::LineTo(n) => self.run_line_to(n)?,
                PathNode::CurveTo(n) => self.run_curve_to(n)?,
                PathNode::ClosePath(_) => self.run_close_path(),
            }
        }

        self.finish();
        Ok(())
    }

    fn run_move_to(&mut self, node: PathMoveTo) {
        if self.points.len > 0 {
            self.finish();
        }
        self.points.reset();
        self.points.add(node.point);
    }

    fn run_line_to(&mut self, node: PathLineTo) -> Result<(), InternalError> {
        let join_mode = self.opts.join_mode;
        self._run_line_to(join_mode, node)
    }

    fn _run_line_to(&mut self, join_mode: JoinMode, node: PathLineTo) -> Result<(), InternalError> {
        let current_point = self.points.last().ok_or(InternalError::InvalidState)?;
        if node.point.equal(current_point) {
            // consume degenerate nodes
            return Ok(());
        }
        self.points.add(node.point);
        if self.points.len > 2 {
            let t3 = self.points.tail(3).unwrap();
            let t2 = self.points.tail(2).unwrap();
            let t1 = self.points.tail(1).unwrap();
            join(self, join_mode, t3, t2, t1, None);
        }
        Ok(())
    }

    fn run_curve_to(&mut self, node: PathCurveTo) -> Result<(), InternalError> {
        let current_point = self.points.last().ok_or(InternalError::InvalidState)?;
        // Lazy-init the pen if it has not been initialized. It
        // does not need to be de-initialized here (nor should it),
        // deinit on the plotter will take care of it.
        if self.pen.is_none() {
            self.pen = Some(Pen::init(
                self.opts.thickness,
                self.opts.tolerance,
                self.opts.ctm,
            ));
        }
        // The Zig `CurveToCtx` feeds each decomposed `line_to` straight into
        // `_runLineTo(.round, ...)`. Here we collect the spline's line_to points
        // first (the decomposition is independent of plotter state and cannot
        // error), then feed them through `_run_line_to` — behavior-equivalent,
        // and it sidesteps a self-borrow cycle.
        let mut pts: Vec<PathLineTo> = Vec::new();
        {
            let mut collector = |n: PathLineTo| -> Result<(), InternalError> {
                pts.push(n);
                Ok(())
            };
            let mut spline = Spline {
                a: current_point,
                b: node.p1,
                c: node.p2,
                d: node.p3,
                tolerance: self.opts.tolerance,
                plotter_impl: &mut collector,
            };
            spline.decompose()?;
        }
        for n in pts {
            self._run_line_to(JoinMode::Round, n)?;
        }
        Ok(())
    }

    fn run_close_path(&mut self) {
        match self.points.len {
            0 => {} //Nothing
            1 => self.plot_dotted(self.points.first().unwrap()),
            2 => {
                let h0 = self.points.head(0).unwrap();
                let h1 = self.points.head(1).unwrap();
                plot_single(self, h0, h1);
            }
            _ => {
                let h0 = self.points.head(0).unwrap();
                let h1 = self.points.head(1).unwrap();
                let t2 = self.points.tail(2).unwrap();
                let t1 = self.points.tail(1).unwrap();
                plot_closed_joined(self, h0, h1, t2, t1);
            }
        }
        self.points.reset();
    }

    fn finish(&mut self) {
        match self.points.len {
            0 | 1 => {} // Nothing
            2 => {
                let h0 = self.points.head(0).unwrap();
                let h1 = self.points.head(1).unwrap();
                plot_single(self, h0, h1);
            }
            _ => {
                let h0 = self.points.head(0).unwrap();
                let h1 = self.points.head(1).unwrap();
                let t2 = self.points.tail(2).unwrap();
                let t1 = self.points.tail(1).unwrap();
                plot_open_joined(self, h0, h1, t2, t1);
            }
        }
    }

    fn plot_dotted(&mut self, point: Point) {
        // Closed degenerate line of a length == 0. When the cap style is
        // round, we draw a circle around the point (as if both ends were
        // round-capped).
        //
        // All other zero-length strokes draw nothing.
        //
        // Note that we draw rectangles/squares for dashed lines, see this
        // function in the dashed plotter for more details.
        debug_assert!(self.inner.len == 0); // should have not been used
        if self.opts.cap_mode == CapMode::Round {
            // Just plot off all of the pen's vertices, no need to
            // determine a subset as we're doing a 360-degree plot.
            debug_assert!(self.pen.is_some());
            {
                let outer = &mut self.outer;
                for v in &self.pen.as_ref().unwrap().vertices {
                    outer.plot(
                        Point {
                            x: point.x + v.point.x,
                            y: point.y + v.point.y,
                        },
                        None,
                    );
                }
            }

            // Convert our contour to edges
            self.result.add_edges_from_contour(&self.outer);

            // Done, de-allocate our contour corners now that they have been
            // converted to edges, and reset our states (only need to do outer,
            // as inner was not touched).
            self.outer = Contour::new(self.opts.scale);
            self.clockwise_ = None;
        }
    }
}

pub fn plot_single(sp: &mut impl StrokePlotter, start: Point, end: Point) {
    // Single-segment line. This can be drawn off of
    // our start line caps.
    let st = sp.state();
    debug_assert!(st.inner.len == 0); // should have not been used
    let cap_points = Face::init(start, end, st.opts.thickness, st.opts.ctm);
    let cap_mode = st.opts.cap_mode;
    {
        let outer = &mut *st.outer;
        let pen = st.pen;
        let mut plot = |p: Point| outer.plot(p, None);
        cap_points.cap_p0(&mut plot, cap_mode, true, pen);
        cap_points.cap_p1(&mut plot, cap_mode, true, pen);
    }

    // Convert our contour to edges
    st.result.add_edges_from_contour(&*st.outer);

    // Done, de-allocate our contour corners now that they have been converted
    // to edges, and reset our states (only need to do outer, as inner was not
    // touched).
    *st.outer = Contour::new(st.opts.scale);
    *st.clockwise_ = None;
}

pub fn plot_open_joined(
    sp: &mut impl StrokePlotter,
    start0: Point,
    end0: Point,
    start1: Point,
    end1: Point,
) {
    // Open path, plot our cap ends and concatenate
    // outer and inner.
    let st = sp.state();
    let thickness = st.opts.thickness;
    let ctm = st.opts.ctm;
    let cap_mode = st.opts.cap_mode;
    let cap_points_start = Face::init(start0, end0, thickness, ctm);
    let cap_points_end = Face::init(start1, end1, thickness, ctm);

    // Check our direction so we know how to plot our cap points
    let clockwise = (*st.clockwise_).unwrap_or(true);

    // Start point
    {
        let before = st.outer.corners_first();
        let outer = &mut *st.outer;
        let pen = st.pen;
        let mut plot = |p: Point| outer.plot(p, before);
        cap_points_start.cap_p0(&mut plot, cap_mode, clockwise, pen);
    }

    // End point
    {
        let outer = &mut *st.outer;
        let pen = st.pen;
        let mut plot = |p: Point| outer.plot(p, None);
        cap_points_end.cap_p1(&mut plot, cap_mode, clockwise, pen);
    }

    // Now, concat the end of the inner contour to the end of the outer to give
    // a single contour (polyline) representing the whole open stroke.
    {
        let (outer, inner) = (&mut *st.outer, &mut *st.inner);
        outer.concat(inner);
    }

    // Convert our completed contour to edges
    st.result.add_edges_from_contour(&*st.outer);

    // Deinit our contour now that it's converted, and reset the state. Note
    // that we don't need to deinit the inner contour as all its corners were
    // moved to the outer.
    *st.outer = Contour::new(st.opts.scale);
    *st.inner = Contour::new(st.opts.scale);
    *st.clockwise_ = None;
}

pub fn plot_closed_joined(
    sp: &mut impl StrokePlotter,
    initial0: Point,
    initial1: Point,
    p1: Point,
    p2: Point,
) {
    // Fully closed path, record the final join, and then
    // insert a join at the start of each the already plotted
    // inner and outer polygons. Append both.
    let join_mode = sp.state().opts.join_mode;
    if !p2.equal(initial0) {
        // Normal case - current point does not equal the initial point.
        //
        // Do the final join (close_path acts as line_to to initial point), and
        // then join around the initial point.
        join(sp, join_mode, p1, p2, initial0, None);
        join(sp, join_mode, p2, initial0, initial1, None);
    } else {
        // Degenerate case - the current point is equal to the initial point.
        //
        // This will happen when a line_to or equivalent has already been
        // processed previously to the initial point. In this case, the final
        // join (p1 -> p2 -> initial0 as per above in the normal case) will
        // have already been done, and our points will have essentially shifted
        // up so that effectively p1 takes the place of p2. This means that we
        // only need to join p1 -> initial0 -> initial1 as our closing join.
        join(sp, join_mode, p1, initial0, initial1, None);
    }

    // Convert both of our outer and inner contours to edges, as they both
    // represent two different closed polygons.
    let st = sp.state();
    st.result.add_edges_from_contour(&*st.outer);
    st.result.add_edges_from_contour(&*st.inner);

    // Done, de-allocate our contour corners now that they have been converted
    // to edges, and reset our states.
    *st.outer = Contour::new(st.opts.scale);
    *st.inner = Contour::new(st.opts.scale);
    *st.clockwise_ = None;
}

/// Plots `point` on the outer side of the join. The Zig original uses a
/// vtable-swapped `Joiner` so that, when the join direction differs from the
/// larger polygon's clockwise direction, the "outer" and "inner" targets are
/// swapped to preserve edge order and prevent twisting; here that swap is
/// `direction_switched`. Note `before` only applies to outer (append/insert)
/// plotting — the inner side always plots in reverse and ignores it.
fn join_plot_outer(
    st: &mut StrokeState,
    direction_switched: bool,
    point: Point,
    before: Option<usize>,
) {
    if direction_switched {
        st.inner.plot_reverse(point);
    } else {
        st.outer.plot(point, before);
    }
}

/// Plots `point` on the inner side of the join (see `join_plot_outer`).
fn join_plot_inner(
    st: &mut StrokeState,
    direction_switched: bool,
    point: Point,
    before: Option<usize>,
) {
    if direction_switched {
        st.outer.plot(point, before);
    } else {
        st.inner.plot_reverse(point);
    }
}

pub fn join(
    sp: &mut impl StrokePlotter,
    join_mode: JoinMode,
    p0: Point,
    p1: Point,
    p2: Point,
    before_outer: Option<usize>,
) {
    let mut st = sp.state();

    // Guard against no-op joins - if one of our segments is degenerate, just return.
    if p0.equal(p1) || p1.equal(p2) {
        if st.clockwise_.is_none() {
            *st.clockwise_ = Some(false);
        }
        return;
    }

    let in_ = Face::init(p0, p1, st.opts.thickness, st.opts.ctm);
    let out = Face::init(p1, p2, st.opts.thickness, st.opts.ctm);
    let join_clockwise = in_.dev_slope.compare(out.dev_slope) < 0;

    // Calculate if the join direction is different from the larger
    // polygon's clockwise direction. If it is, we need to plot respective
    // points on the opposite sides of what you would normally expect to
    // preserve correct edge order and prevent twisting. We use vtables to
    // avoid the constant need to branch while plotting.
    let poly_clockwise = (*st.clockwise_).unwrap_or(join_clockwise);
    let direction_switched: bool = join_clockwise != poly_clockwise;

    // If our slopes are equal (co-linear), only plot the end of the
    // inbound face, regardless of join mode.
    if in_.dev_slope.compare(out.dev_slope) == 0 {
        join_plot_outer(
            &mut st,
            direction_switched,
            if join_clockwise {
                in_.p1_ccw
            } else {
                in_.p1_cw
            },
            before_outer,
        );
        join_plot_inner(
            &mut st,
            direction_switched,
            if join_clockwise {
                in_.p1_cw
            } else {
                in_.p1_ccw
            },
            before_outer,
        );
        if st.clockwise_.is_none() {
            *st.clockwise_ = Some(poly_clockwise);
        }
        return;
    }

    match join_mode {
        JoinMode::Miter | JoinMode::Bevel => {
            if join_mode == JoinMode::Miter
                && Slope::compare_for_miter_limit(in_.dev_slope, out.dev_slope, st.opts.miter_limit)
            {
                let pt = Face::intersect(in_, out, join_clockwise);
                join_plot_outer(&mut st, direction_switched, pt, before_outer);
            } else {
                join_plot_outer(
                    &mut st,
                    direction_switched,
                    if join_clockwise {
                        in_.p1_ccw
                    } else {
                        in_.p1_cw
                    },
                    before_outer,
                );
                join_plot_outer(
                    &mut st,
                    direction_switched,
                    if join_clockwise {
                        out.p0_ccw
                    } else {
                        out.p0_cw
                    },
                    before_outer,
                );
            }
        }

        JoinMode::Round => {
            debug_assert!(st.pen.is_some());
            join_plot_outer(
                &mut st,
                direction_switched,
                if join_clockwise {
                    in_.p1_ccw
                } else {
                    in_.p1_cw
                },
                before_outer,
            );
            // Collect the pen's fan points (immutable borrow of the pen)
            // before plotting them (which needs `&mut` on the contours).
            let fan: Vec<Point> = {
                let pen = st.pen.unwrap();
                let vit = pen.vertex_iterator_for(in_.dev_slope, out.dev_slope, join_clockwise);
                vit.map(|v| Point {
                    x: p1.x + v.point.x,
                    y: p1.y + v.point.y,
                })
                .collect()
            };
            for pt in fan {
                join_plot_outer(&mut st, direction_switched, pt, before_outer);
            }
            join_plot_outer(
                &mut st,
                direction_switched,
                if join_clockwise {
                    out.p0_ccw
                } else {
                    out.p0_cw
                },
                before_outer,
            );
        }
    }

    // Inner join. We plot our ends depending on direction, going through
    // the midpoint.
    join_plot_inner(
        &mut st,
        direction_switched,
        if join_clockwise {
            in_.p1_cw
        } else {
            in_.p1_ccw
        },
        before_outer,
    );
    join_plot_inner(&mut st, direction_switched, p1, before_outer);
    join_plot_inner(
        &mut st,
        direction_switched,
        if join_clockwise {
            out.p0_cw
        } else {
            out.p0_ccw
        },
        before_outer,
    );

    if st.clockwise_.is_none() {
        *st.clockwise_ = Some(poly_clockwise);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transformation::IDENTITY;

    fn base_opts() -> PlotterOptions {
        PlotterOptions {
            cap_mode: CapMode::Butt,
            ctm: IDENTITY,
            dashes: Vec::new(),
            dash_offset: 0.0,
            join_mode: JoinMode::Miter,
            miter_limit: 10.0,
            scale: 1.0,
            thickness: 2.0,
            tolerance: 0.01,
        }
    }

    fn mv(x: f64, y: f64) -> PathNode {
        PathNode::MoveTo(PathMoveTo {
            point: Point { x, y },
        })
    }
    fn ln(x: f64, y: f64) -> PathNode {
        PathNode::LineTo(PathLineTo {
            point: Point { x, y },
        })
    }

    #[test]
    fn assert_ok_degenerate_moveto_lineto_then_good_lineto() {
        {
            // p0 -> p1 is equal
            let nodes = vec![mv(10.0, 10.0), ln(10.0, 10.0), ln(20.0, 20.0)];
            let result = plot(&nodes, base_opts()).unwrap();
            assert_eq!(4, result.edges.len());
        }

        {
            // p1 -> p2 is equal
            let nodes = vec![mv(10.0, 10.0), ln(20.0, 20.0), ln(20.0, 20.0)];
            let result = plot(&nodes, base_opts()).unwrap();
            assert_eq!(4, result.edges.len());
        }
    }

    #[test]
    // Same root cause as `path_fx::offset`'s two e2e tests: Miri's libm is not
    // bit-faithful for `hypot`, which `Slope::normalize` (`slope.rs:243`) uses.
    // Measured chain, not inferred — a probe on `Face::intersect` printed the
    // exact operands under Miri:
    //
    //     det=0e0  in=(bfeda7d91c1e082a, 3fd80b8d70be677c)
    //              out=(bfeda7d91c1e082c, 3fd80b8d70be677e)
    //
    // The two normalized slopes differ by 2 ULP per component, so the miter
    // determinant `in.dx*out.dy - out.dx*in.dy` rounds to EXACTLY zero and the
    // division yields 0/0 = NaN, which trips `polygon.rs`'s finite-point
    // `debug_assert`. Natively the same probe never fires: the host `hypot`
    // gives normalized components whose determinant stays non-zero. Runs green
    // in ci.sh's HARD `cargo test --workspace` step.
    //
    // The latent fragility this exposes — `Face::intersect` guards on
    // `Slope::compare(..) != 0`, which is not the same predicate as "the
    // determinant is non-zero" — is recorded in `.port/phase-3/miri/FINDINGS.md`
    // rather than fixed here: the miter math is z2d 0.12.1's (itself Cairo's
    // `cairo-path-stroke-polygon.c`), and changing its numerics would be a
    // deliberate divergence from the vendored reference (R1).
    // See .port/phase-2/MIRI_EXCEPTIONS.md, "rustty-z2d path_fx offset e2e".
    #[cfg_attr(
        miri,
        ignore = "Miri libm: hypot ULPs collapse the miter determinant to 0 (0/0 NaN)"
    )]
    fn slope_difference_below_epsilon_does_not_produce_nan() {
        {
            // p1 -> p2 is equal
            let nodes = vec![
                mv(2.864_101_562_5e2, 1.431_542_968_75e1),
                ln(2.822_548_828_125e2, 1.6e1),
                ln(2.809_902_343_75e2, 1.651_269_531_25e1),
            ];

            let result = plot(&nodes, base_opts()).unwrap();
            assert_eq!(6, result.edges.len());
            for (idx, edge) in result.edges.iter().enumerate() {
                for value in [edge.y0, edge.y1, edge.x_start, edge.x_inc] {
                    if !value.is_finite() {
                        panic!("Non-finite value found at index {idx}, data: {edge:?}");
                    }
                }
            }
        }
    }

    // Behavioral test: a dashed stroke of a straight line produces multiple
    // disjoint filled dashes (replaces the old dashed-panic blocker contract). A
    // plain (undashed) open single-segment horizontal stroke is one butt-capped
    // rectangle whose two horizontal sides contribute no scanline edges, so 2
    // edges; the `[2,2]`-dashed version tessellates several on-dash
    // sub-polygons, so it must yield strictly more edges than the undashed one.
    #[test]
    fn dashed_stroke_produces_multiple_dashes() {
        let nodes = vec![mv(0.0, 0.0), ln(10.0, 0.0)];
        let undashed = plot(&nodes, base_opts()).unwrap();

        let opts = PlotterOptions {
            dashes: vec![2.0, 2.0],
            ..base_opts()
        };
        let result = plot(&nodes, opts).unwrap();
        assert!(
            result.edges.len() > undashed.edges.len(),
            "dashed stroke did not tessellate multiple dashes: {} edges (undashed {})",
            result.edges.len(),
            undashed.edges.len()
        );
    }
}
