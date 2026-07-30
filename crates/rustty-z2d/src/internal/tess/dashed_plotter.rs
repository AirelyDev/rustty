// SPDX-License-Identifier: MPL-2.0
//   Copyright © 2024-2025 Chris Marchesi
//
// Ported to Rust for rustty from z2d 0.10.0
// `src/internal/tess/dashed_plotter.zig` (R3 vendored dep).
//
// The dashed variant of the stroke plotter. It walks each segment in dash-sized
// steps (via `Dasher`), plotting a fresh sub-polygon per "on" dash and, at the
// end, joining/capping the trailing dash back onto the very first dash of the
// sub-path (the "initial polygon") so a closed dashed stroke meets cleanly.
//
// SB-SAFE DESIGN (see MEMORY "long-lived raw alias into a field"): the Zig
// `InitialPolygon` embeds `result: *Polygon` aliasing the plotter's own
// `result`, and `joinAndCapInitial` performs live polygon surgery
// (`self.outer.concat(&initial.outer)`, `initial.outer = self.outer`,
// `plotOpenJoined(InitialPolygon, &initial, ...)`) interleaved with the
// plotter's `result` — a raw `*mut Polygon` stored in `InitialPolygon` and held
// live across `&mut self.result` would be SB/TB UB. Instead, `InitialPolygon`
// does NOT store `result`; when the generic stroke helpers need to run against
// it, we build a short-lived `InitialView` that stitches the plotter's `result`
// (borrowed disjointly) to the initial polygon's own fields and implements
// `StrokePlotter`. rustty's `Contour` is an index arena, so its `concat`/`plot`/
// `plot_reverse` moves carry no raw aliasing; the hazard was solely the
// self-referential `result`, which this design removes. Proven SB/TB-clean on
// the isolated `.port/phase-2/mirihar-z2d-dashed` Miri harness.

use crate::internal::error::InternalError;
use crate::internal::path_nodes::{PathCurveTo, PathLineTo, PathMoveTo, PathNode};
use crate::internal::point::Point;
use crate::internal::tess::dasher::Dasher;
use crate::internal::tess::face::Face;
use crate::internal::tess::pen::Pen;
use crate::internal::tess::point_buffer::PointBuffer;
use crate::internal::tess::polygon::{Contour, Polygon};
use crate::internal::tess::slope::Slope;
use crate::internal::tess::spline::Spline;
use crate::internal::tess::stroke_plotter::{
    join, plot_open_joined, plot_single, PlotterOptions, StrokePlotter, StrokeState,
};
use crate::options::{CapMode, JoinMode};

pub fn plot(nodes: &[PathNode], opts: PlotterOptions) -> Result<Polygon, InternalError> {
    // NOTE: `opts.dashes` needs to be validated separately before calling this
    // - this is done from `stroke_plotter.plot` in normal operation. Be
    // cognizant of this if you plan on calling this directly!
    let pen = if opts.join_mode == JoinMode::Round || opts.cap_mode == CapMode::Round {
        Some(Pen::init(opts.thickness, opts.tolerance, opts.ctm))
    } else {
        None
    };

    let dasher = Dasher::init(&opts.dashes, opts.dash_offset);

    let mut plotter = Plotter {
        nodes,
        opts: &opts,

        pen,

        points: PointBuffer::default(),
        clockwise_: None,

        result: Polygon::new(1.0),
        outer: Contour::new(opts.scale),
        inner: Contour::new(opts.scale),

        dasher,
        // normalized current device slope (see `_run_line_to`); "undefined"
        // until first assigned, mirroring the Zig `current_slope: Slope =
        // undefined`.
        current_slope: Slope { dx: 0.0, dy: 0.0 },
        initial_polygon: InitialPolygonState::None,
    };

    plotter.run()?;

    Ok(plotter.result)
}

/// Port of the Zig `Plotter.InitialPolygon` — the saved first dash of a
/// sub-path, used for final joining/capping. Notably it does NOT hold a pointer
/// to the plotter's `result` (see the SB-SAFE DESIGN note); the `result` is
/// threaded in via `InitialView` when the generic helpers run against it.
struct InitialPolygon {
    // Duplicated from the plotter so the generic helpers can run against the
    // initial polygon. `pen` is captured by clone (the Zig aliases the shared
    // vertices buffer, which is read-only during plotting, so a clone is
    // behaviorally identical; if the pen is lazy-initialized only later in a
    // curve_to, this stays `None`, which is fine — it would only be needed for
    // round-joining decomposed curve lines, never for the initial polygon's
    // caps/joins).
    pen: Option<Pen>,
    points: PointBuffer<2, 5>,
    clockwise_: Option<bool>,
    outer: Contour,
    inner: Contour,
    current_slope: Slope,
}

// WS19 (audit row N11): `clippy::large_enum_variant` fires here because `On`'s
// `InitialPolygon` (two `Contour`s + a `Slope`) dwarfs `Off(Point)`. Boxing `On` is the
// lint's suggested fix and is REFUSED: this enum mirrors the reference's tagged union
// field-for-field, the value is a short-lived plotter-local held on the stack for the
// duration of one dash traversal (never stored in a collection, so the size difference
// costs nothing per-element), and introducing a heap indirection here would diverge
// from the reference's allocation behaviour on a hot tessellation path — an R1 loss
// bought for no measurable gain. Allowed at the type, with the reason, rather than
// crate-wide.
#[allow(clippy::large_enum_variant)]
enum InitialPolygonState {
    None,
    Off(Point),
    On(InitialPolygon),
}

#[derive(Clone, Copy)]
enum IpKind {
    None,
    Off,
    On,
}

/// A short-lived view stitching the plotter's `result` to an `InitialPolygon`'s
/// own fields, implementing `StrokePlotter` so the generic `join` /
/// `plot_open_joined` helpers can run against the initial polygon and append to
/// the shared result — without the initial polygon ever storing a `result`
/// pointer (the SB hazard). Constructed from disjoint field borrows of the
/// plotter.
struct InitialView<'v> {
    opts: &'v PlotterOptions,
    result: &'v mut Polygon,
    ip: &'v mut InitialPolygon,
}

impl StrokePlotter for InitialView<'_> {
    fn state(&mut self) -> StrokeState<'_> {
        StrokeState {
            opts: self.opts,
            pen: self.ip.pen.as_ref(),
            clockwise_: &mut self.ip.clockwise_,
            result: self.result,
            outer: &mut self.ip.outer,
            inner: &mut self.ip.inner,
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

    dasher: Dasher<'a>,
    current_slope: Slope, // normalized current device slope (see _run_line_to)
    initial_polygon: InitialPolygonState,
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

impl Plotter<'_> {
    fn ip_kind(&self) -> IpKind {
        match &self.initial_polygon {
            InitialPolygonState::None => IpKind::None,
            InitialPolygonState::Off(_) => IpKind::Off,
            InitialPolygonState::On(_) => IpKind::On,
        }
    }

    fn run(&mut self) -> Result<(), InternalError> {
        for idx in 0..self.nodes.len() {
            let node = self.nodes[idx];
            match node {
                PathNode::MoveTo(n) => self.run_move_to(n)?,
                PathNode::LineTo(n) => self.run_line_to(n)?,
                PathNode::CurveTo(n) => self.run_curve_to(n)?,
                PathNode::ClosePath(_) => self.run_close_path()?,
            }
        }

        self.finish()
    }

    fn run_move_to(&mut self, node: PathMoveTo) -> Result<(), InternalError> {
        self.finish()?;
        self.dasher.reset();
        self.points.reset();
        self.points.add(node.point);
        Ok(())
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
        let first_dash_point = current_point;
        let mut slope = Slope::init(first_dash_point, node.point);
        self.current_slope = slope;
        self.current_slope.normalize();
        // `deviceToUserDistance` here is `catch unreachable` in the reference —
        // the matrix invertibility was validated in `painter::stroke`.
        self.opts
            .ctm
            .device_to_user_distance(&mut slope.dx, &mut slope.dy)
            .expect("ctm invertible (validated in painter::stroke)");
        let total_len = slope.normalize();
        let mut remaining_len = total_len;
        let mut step_len = self.dasher.remain.min(remaining_len);
        while remaining_len > 0.0 {
            remaining_len -= step_len;
            let mut x_offset = slope.dx * (total_len - remaining_len);
            let mut y_offset = slope.dy * (total_len - remaining_len);
            self.opts
                .ctm
                .user_to_device_distance(&mut x_offset, &mut y_offset);
            let current_dash_point = Point {
                x: first_dash_point.x + x_offset,
                y: first_dash_point.y + y_offset,
            };
            if !current_dash_point.equal(self.points.last().unwrap()) {
                // Only add this point if it's different than the point before
                // it, this allows us to plot dots (and squares also), i.e.,
                // zero-length dash stops.
                self.points.add(current_dash_point);
            }
            if self.dasher.on && self.points.len > 2 {
                let t3 = self.points.tail(3).unwrap();
                let t2 = self.points.tail(2).unwrap();
                let t1 = self.points.tail(1).unwrap();
                join(self, join_mode, t3, t2, t1, None);
            }
            if self.dasher.step(step_len) {
                self.next_segment(current_dash_point);
            }
            step_len = self.dasher.remain.min(remaining_len);
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
        // `_runLineTo(.round, ...)`; we collect the (state-independent) spline
        // line_to points first, then replay them through `_run_line_to` — the
        // dasher advances identically since replay is in order.
        //
        // PORT NOTE (faithful-but-safe divergence, error path only): the oracle's
        // `CurveToCtx.line_to` records a `_runLineTo` error into `err_` and lets
        // `spline.decompose` run to completion, surfacing it afterward; here the
        // replay loop propagates the FIRST `_run_line_to` error immediately via
        // `?`. The success path is identical (spline decomposition is
        // state-independent); only the post-error continuation differs, and that
        // is unobservable — the first error aborts the stroke in both.
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

    fn run_close_path(&mut self) -> Result<(), InternalError> {
        if self.points.len == 0 {
            return Err(InternalError::InvalidState);
        }
        // Unlike non-dashed close_path, we need to dash to our initial point to
        // ensure that any dashes in-between are taken care of.
        let target: Point = match &self.initial_polygon {
            InitialPolygonState::On(poly) => {
                poly.points.first().ok_or(InternalError::InvalidState)?
            }
            InitialPolygonState::Off(initial_point) => *initial_point,
            // Already validated above.
            InitialPolygonState::None => self.points.first().unwrap(),
        };
        let join_mode = self.opts.join_mode;
        self._run_line_to(join_mode, PathLineTo { point: target })?;

        // How we close now depends on whether or not we actually dashed. Note
        // that `_run_line_to` may have transitioned `initial_polygon` (via
        // `next_segment` -> `save_initial`) if it was `None`, so we re-read it.
        match self.ip_kind() {
            IpKind::On => {
                let (poly_len, ph0, ph1) = match &self.initial_polygon {
                    InitialPolygonState::On(poly) => {
                        (poly.points.len, poly.points.head(0), poly.points.head(1))
                    }
                    _ => unreachable!(),
                };
                // Check the current dasher state.
                if self.dasher.on && self.points.len > 1 {
                    // We're on, so we need to join our last segment to the
                    // initial polygon.
                    debug_assert!(poly_len > 0);
                    if poly_len == 1 {
                        // Our initial polygon was a dot (zero-length dash).
                        // Since we actually have already plotted back to our
                        // original point, we can just treat this as a
                        // last-segment dash off our current state.
                        let h0 = self.points.head(0).unwrap();
                        let h1 = self.points.head(1).unwrap();
                        let t2 = self.points.tail(2).unwrap();
                        let t1 = self.points.tail(1).unwrap();
                        plot_open_joined(self, h0, h1, t2, t1);
                        // Reset the initial state since we're not invoking a
                        // helper that does it.
                        self.initial_polygon = InitialPolygonState::None;
                    } else {
                        self.join_and_cap_initial()?;
                    }
                } else {
                    // We're off, or we just transitioned to an on segment at
                    // exactly the original point, so cap off the initial using
                    // the original initial points.
                    match poly_len {
                        0 => return Err(InternalError::InvalidState),
                        // Zero-length dash, plot a dot.
                        1 => self.finish_initial_dotted()?,
                        _ => self.finish_initial(ph0.unwrap(), ph1.unwrap())?,
                    }
                }
            }
            IpKind::Off => {
                // Nothing - we've already drawn back to the initial point. Just
                // reset the initial polygon state.
                self.initial_polygon = InitialPolygonState::None;
            }
            IpKind::None => {
                // We never actually transitioned off the initial dash segment.
                // This almost acts like an undashed closed path, but since
                // we've already advanced to our end point in the above line_to
                // call, we need to act accordingly.
                match self.points.len {
                    0 => unreachable!(),
                    1 => {
                        let p = self.points.first().unwrap();
                        self.plot_dotted(p, self.current_slope);
                    }
                    2 => {
                        let h0 = self.points.head(0).unwrap();
                        let h1 = self.points.head(1).unwrap();
                        plot_single(self, h0, h1);
                    }
                    _ => {
                        // We only need to plot the final join here, since we've
                        // already plotted the first.
                        //
                        // Join around the initial point.
                        let t2 = self.points.tail(2).unwrap();
                        let h0 = self.points.head(0).unwrap();
                        let h1 = self.points.head(1).unwrap();
                        join(self, join_mode, t2, h0, h1, None);

                        // Done.
                        let st = self.state();
                        st.result.add_edges_from_contour(&*st.outer);
                        st.result.add_edges_from_contour(&*st.inner);

                        // De-allocate corners along with resetting state.
                        *st.outer = Contour::new(st.opts.scale);
                        *st.inner = Contour::new(st.opts.scale);
                        *st.clockwise_ = None;
                    }
                }
            }
        }
        self.points.reset();
        Ok(())
    }

    fn next_segment(&mut self, point: Point) {
        if matches!(self.initial_polygon, InitialPolygonState::None) {
            self.save_initial();
        } else if !self.dasher.on {
            match self.points.len {
                0 => {} //Nothing
                1 => {
                    let p = self.points.first().unwrap();
                    self.plot_dotted(p, self.current_slope);
                }
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
        self.points.reset();
        self.points.add(point);
    }

    fn finish(&mut self) -> Result<(), InternalError> {
        match self.ip_kind() {
            IpKind::On => {
                let (poly_len, ph0, ph1) = match &self.initial_polygon {
                    InitialPolygonState::On(poly) => {
                        (poly.points.len, poly.points.head(0), poly.points.head(1))
                    }
                    _ => unreachable!(),
                };
                match poly_len {
                    0 => return Err(InternalError::InvalidState),
                    1 => self.finish_initial_dotted()?,
                    _ => self.finish_initial(ph0.unwrap(), ph1.unwrap())?,
                }
            }
            IpKind::Off => self.initial_polygon = InitialPolygonState::None,
            IpKind::None => {}
        }
        if self.dasher.on {
            match self.points.len {
                0 => {} //Nothing
                1 => {
                    let p = self.points.first().unwrap();
                    self.plot_dotted(p, self.current_slope);
                }
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
        Ok(())
    }

    fn plot_dotted(&mut self, point: Point, current_slope: Slope) {
        // Closed degenerate line of a length == 0. This is handled in special
        // cases:
        //
        // * When the cap style is round, we draw a circle around the point (as
        // if both ends were round-capped).
        //
        // * When the cap style is square, and we are in an "on" segment in a
        // dashed stroke, we draw a square, oriented in the direction of the
        // stroke.
        //
        // All other zero-length strokes draw nothing.
        debug_assert!(self.inner.len == 0); // should have not been used
        match self.opts.cap_mode {
            CapMode::Round => {
                // Just plot off all of the pen's vertices, no need to determine
                // a subset as we're doing a 360-degree plot.
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

                // Done
                self.result.add_edges_from_contour(&self.outer);
            }
            CapMode::Square => {
                // "cap" a single point with the last slope we've logged in a
                // line_to or close_path. We just take a subset of capSquare in
                // Face, while still computing the offsets using some of the init
                // functionality.
                let face =
                    Face::init_single(point, current_slope, self.opts.thickness, self.opts.ctm);
                let mut offset_x = face.user_slope.dx * face.half_width;
                let mut offset_y = face.user_slope.dy * face.half_width;
                self.opts
                    .ctm
                    .user_to_device_distance(&mut offset_x, &mut offset_y);
                self.outer.plot(
                    Point {
                        x: face.p1_cw.x - offset_x,
                        y: face.p1_cw.y - offset_y,
                    },
                    None,
                );
                self.outer.plot(
                    Point {
                        x: face.p1_cw.x + offset_x,
                        y: face.p1_cw.y + offset_y,
                    },
                    None,
                );
                self.outer.plot(
                    Point {
                        x: face.p1_ccw.x + offset_x,
                        y: face.p1_ccw.y + offset_y,
                    },
                    None,
                );
                self.outer.plot(
                    Point {
                        x: face.p1_ccw.x - offset_x,
                        y: face.p1_ccw.y - offset_y,
                    },
                    None,
                );
                // Done
                self.result.add_edges_from_contour(&self.outer);
            }
            CapMode::Butt => {}
        }
        // Reset outer, de-allocating our contour that has been recorded as
        // edges and resetting state.
        self.outer = Contour::new(self.opts.scale);
        self.clockwise_ = None;
    }

    fn save_initial(&mut self) {
        // This prepares the initial polygon for later capping or joining,
        // depending on the final state of the stroke. The thing is that we
        // don't necessarily know if we're dealing with a close_path or not, and
        // if the stroker state will be on or not in that close. So this tracks
        // as much state as we need to make a decision at that point.
        if !self.dasher.on {
            // This is intended to be used on dash step transitions, so we save
            // if the dasher state was off. The plotter's contours are MOVED
            // into the initial polygon (self.outer/inner are reset below), so
            // there is no aliasing and no de-init needed. See the SB-SAFE
            // DESIGN note for why `result` is threaded via `InitialView` rather
            // than stored here.
            let outer = std::mem::replace(&mut self.outer, Contour::new(self.opts.scale));
            let inner = std::mem::replace(&mut self.inner, Contour::new(self.opts.scale));
            self.initial_polygon = InitialPolygonState::On(InitialPolygon {
                pen: self.pen.clone(),
                points: self.points,
                clockwise_: self.clockwise_,
                outer,
                inner,
                current_slope: self.current_slope,
            });
        } else {
            // We record that the initial dash state was off and discard any
            // data in the off segment, minus the initial point, which we need
            // for a possible close_path in the sub-path we're currently in.
            // This can happen when dash offsets have pushed/pulled the state
            // into an off segment at the start of the stroke.
            debug_assert!(self.points.len > 0);
            let first = self.points.first().unwrap();
            self.initial_polygon = InitialPolygonState::Off(first);
        }

        // Reset the contours, and clockwise state. As our corner data has been
        // off-loaded to the initial polygon, no deinit is necessary on either
        // contour.
        self.outer = Contour::new(self.opts.scale);
        self.inner = Contour::new(self.opts.scale);
        self.clockwise_ = None;
    }

    fn finish_initial_dotted(&mut self) -> Result<(), InternalError> {
        let (first, slope) = match &self.initial_polygon {
            InitialPolygonState::On(poly) => (
                poly.points.first().ok_or(InternalError::InvalidState)?,
                poly.current_slope,
            ),
            _ => return Err(InternalError::InvalidState),
        };
        self.plot_dotted(first, slope);
        self.initial_polygon = InitialPolygonState::None;
        Ok(())
    }

    fn finish_initial(
        &mut self,
        last_point: Point,
        second_to_last_point: Point,
    ) -> Result<(), InternalError> {
        let (t2, t1) = match &self.initial_polygon {
            InitialPolygonState::On(poly) => (
                poly.points.tail(2).ok_or(InternalError::InvalidState)?,
                poly.points.tail(1).ok_or(InternalError::InvalidState)?,
            ),
            _ => return Err(InternalError::InvalidState),
        };
        {
            let opts = self.opts;
            let result = &mut self.result;
            if let InitialPolygonState::On(ip) = &mut self.initial_polygon {
                let mut view = InitialView { opts, result, ip };
                plot_open_joined(&mut view, last_point, second_to_last_point, t2, t1);
            } else {
                return Err(InternalError::InvalidState);
            }
        }
        // Note that our generic `plot_open_joined` will properly reset the
        // contour state within our initial polygon, so all we need to do here
        // is change the initial polygon state back to `None`.
        self.initial_polygon = InitialPolygonState::None;
        Ok(())
    }

    fn join_and_cap_initial(&mut self) -> Result<(), InternalError> {
        // This adds a join at the beginning of the initial polygon before
        // capping.
        if !matches!(self.initial_polygon, InitialPolygonState::On(_)) {
            return Err(InternalError::InvalidState);
        }
        let join_mode = self.opts.join_mode;
        if self.points.len > 2 {
            // We need to actually do some polygon surgery here because we have
            // some outstanding joins.
            let t2 = self.points.tail(2).unwrap();
            let ph0 = self.points.head(0).unwrap();
            let ph1 = self.points.head(1).unwrap();
            let (iph0, iph1, ipt2, ipt1) = match &self.initial_polygon {
                InitialPolygonState::On(poly) => (
                    poly.points.head(0).ok_or(InternalError::InvalidState)?,
                    poly.points.head(1).ok_or(InternalError::InvalidState)?,
                    poly.points.tail(2).ok_or(InternalError::InvalidState)?,
                    poly.points.tail(1).ok_or(InternalError::InvalidState)?,
                ),
                _ => return Err(InternalError::InvalidState),
            };

            // Do the final join on the existing polygon.
            join(self, join_mode, t2, iph0, iph1, None);

            // Concat the initial polygon to the main outer, and inner to
            // initial, i.e., the other way around. Then replace the initial
            // polygon's outer with the (now-combined) main outer due to the
            // direction the concat happened in. Finally, run the caps against
            // the initial polygon (via the view) off of the plotter state.
            {
                let opts = self.opts;
                let result = &mut self.result;
                let self_outer = &mut self.outer;
                let self_inner = &mut self.inner;
                if let InitialPolygonState::On(ip) = &mut self.initial_polygon {
                    self_outer.concat(&mut ip.outer);
                    ip.inner.concat(self_inner);
                    ip.outer = std::mem::replace(self_outer, Contour::new(opts.scale));
                    let mut view = InitialView { opts, result, ip };
                    // Our first cap points are based entirely off of the plotter
                    // state (not the initial state).
                    plot_open_joined(&mut view, ph0, ph1, ipt2, ipt1);
                } else {
                    return Err(InternalError::InvalidState);
                }
            }
        } else {
            // Don't need to do any concats and our cap points are last corner
            // -> start of initial polygon.
            let t2 = self.points.tail(2).unwrap();
            let pfirst = self.points.first().unwrap();
            let (iph0, iph1, ipfirst, ipt2, ipt1, before) = match &self.initial_polygon {
                InitialPolygonState::On(poly) => (
                    poly.points.head(0).ok_or(InternalError::InvalidState)?,
                    poly.points.head(1).ok_or(InternalError::InvalidState)?,
                    poly.points.first().ok_or(InternalError::InvalidState)?,
                    poly.points.tail(2).ok_or(InternalError::InvalidState)?,
                    poly.points.tail(1).ok_or(InternalError::InvalidState)?,
                    poly.outer.corners_first(),
                ),
                _ => return Err(InternalError::InvalidState),
            };
            let opts = self.opts;
            let result = &mut self.result;
            if let InitialPolygonState::On(ip) = &mut self.initial_polygon {
                let mut view = InitialView { opts, result, ip };
                // Do the join
                join(&mut view, join_mode, t2, iph0, iph1, before);
                plot_open_joined(&mut view, pfirst, ipfirst, ipt2, ipt1);
            } else {
                return Err(InternalError::InvalidState);
            }
        }

        self.initial_polygon = InitialPolygonState::None;
        // Reset the main polygon state as we don't do that above (the initial
        // gets cleared instead). Note that we don't need to de-init any corners
        // here, as either they are all in the initial polygon (in the simple
        // case where there were no outstanding joins), or were moved there (in
        // the more complex case where there were).
        self.outer = Contour::new(self.opts.scale);
        self.inner = Contour::new(self.opts.scale);
        self.clockwise_ = None;
        Ok(())
    }
}
