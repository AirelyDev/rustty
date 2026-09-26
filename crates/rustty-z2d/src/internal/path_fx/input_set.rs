// SPDX-License-Identifier: MPL-2.0
//   Copyright © 2024-2026 Chris Marchesi
//
// Ported to Rust for rustty from z2d 0.12.1
// `src/internal/path_fx/InputSet.zig` (R3 vendored dep). See
// `super`'s SUBSET NOTE for what is deliberately omitted.

//! Input set for path effects. Includes helpers for calculation of
//! intersections, re-ordering points, and other functionality that is useful
//! to all path effect functionality that should happen at input time.

use crate::internal::error::InternalError;
use crate::internal::path_nodes::{PathLineTo, PathNode};
use crate::internal::point::Point;
use crate::internal::tess::point_buffer::PointBuffer;
use crate::internal::tess::spline::{Plotter, Spline};

#[derive(Debug, Clone, Default)]
pub struct InputSet {
    pub contours: Vec<Contour>,
}

impl InputSet {
    /// Loads an input set from path nodes.
    ///
    /// The following rules currently apply:
    ///
    /// * Co-linear nodes are consolidated.
    /// * Single-node paths (move_to -> close_path, or just move_to) are
    ///   currently ignored.
    pub fn from_nodes(nodes: &[PathNode], tolerance: f64) -> Result<InputSet, InternalError> {
        // NOTE: the state machine has some duplicated logic in comparison to
        // other plotters, namely due to how we specifically flag closed
        // contours, this means that we have to append and clear out the state
        // on both close and move_to, but in the latter case only when we have
        // nodes already (for open paths). This could use a bit of a refactor to
        // possibly remove these duplicate code paths, but for now it should be
        // fine (and is well tested).

        let mut result = InputSet::default();
        let mut current_contour = Contour::default();
        let mut points: PointBuffer<1, 2> = PointBuffer::default();

        for (i, node) in nodes.iter().enumerate() {
            match node {
                PathNode::MoveTo(n) => {
                    if !current_contour.segments.is_empty() {
                        result.contours.push(current_contour);
                        current_contour = Contour::default();
                        points.reset();
                    }

                    // Check if this is the last node, and no-op if it is, as
                    // this is the auto-added move_to node that is given after
                    // close_path.
                    if i == nodes.len() - 1 {
                        break;
                    }
                    points.add(n.point);
                }
                PathNode::LineTo(n) => {
                    let Some(last_point) = points.last() else {
                        return Err(InternalError::InvalidState);
                    };
                    if !last_point.equal(n.point) {
                        if current_contour.segments.is_empty() {
                            current_contour = Contour::init(last_point, n.point);
                        } else {
                            current_contour.plot(n.point);
                        }

                        points.add(n.point);
                    }
                }
                PathNode::CurveTo(n) => {
                    if points.len == 0 {
                        return Err(InternalError::InvalidState);
                    }
                    let mut ctx = SplinePlotterCtx {
                        contour: &mut current_contour,
                        points: &mut points,
                    };
                    let a = ctx.points.last().expect("points.len checked above");
                    let mut spline = Spline {
                        a,
                        b: n.p1,
                        c: n.p2,
                        d: n.p3,
                        tolerance,
                        plotter_impl: &mut ctx,
                    };
                    spline.decompose()?;
                }
                PathNode::ClosePath(_) => {
                    // Note that unlike our fill plotter, there's not much in
                    // the way of special handling here, however we skip if all
                    // we have in the buffer is a move_to, mainly because that
                    // would only be a single point anyway.
                    //
                    // Our limit is 2 currently but this guards in the case we
                    // change it.
                    if points.len >= 2 {
                        // Close and add contour
                        current_contour.close();
                        result.contours.push(current_contour);
                        current_contour = Contour::default();

                        // Reset points (a well-formed path always has a move_to
                        // after close_point)
                        points.reset();
                    }
                }
            }
        }

        if !current_contour.segments.is_empty() {
            result.contours.push(current_contour);
        }

        Ok(result)
    }
}

/// PORT NOTE (rustty): upstream's `SplinePlotterCtx` threads its error out
/// through an `err_: *?PlotterVTable.Error` out-parameter because Zig's plotter
/// vtable cannot return an error. rustty's `Spline` plotter trait returns
/// `Result`, so the out-parameter dance (and upstream's own "FIXME: This is
/// cursed" labelled-break) collapses into a plain `?`.
struct SplinePlotterCtx<'a> {
    contour: &'a mut Contour,
    points: &'a mut PointBuffer<1, 2>,
}

impl Plotter for SplinePlotterCtx<'_> {
    fn line_to(&mut self, node: PathLineTo) -> Result<(), InternalError> {
        let Some(last_point) = self.points.last() else {
            return Err(InternalError::InvalidState);
        };
        if !last_point.equal(node.point) {
            if self.contour.segments.is_empty() {
                *self.contour = Contour::init(last_point, node.point);
            } else {
                self.contour.plot(node.point);
            }
            self.points.add(node.point);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default)]
pub struct Contour {
    pub segments: Vec<Segment>,
    pub closed: bool,
}

impl Contour {
    /// Asserts that the two points are not equal.
    fn init(p0: Point, p1: Point) -> Contour {
        Contour {
            segments: vec![Segment::init(p0, p1)],
            closed: false,
        }
    }

    /// Asserts that there is at least one segment in the contour, and that the
    /// contour is not closed. Will combine co-linear consecutive segments into
    /// single segments.
    fn plot(&mut self, point: Point) {
        assert!(!self.segments.is_empty());
        assert!(!self.closed);
        let prev = self.segments[self.segments.len() - 1];
        let mut current = Segment::init(prev.p1, point);
        current.orientation = match prev.slope.compare(current.slope) {
            -1 => Orientation::Cw,
            1 => Orientation::Ccw,
            _ => {
                let last = self.segments.len() - 1;
                self.segments[last].p1 = point;
                return;
            }
        };

        if self.segments.len() == 1 {
            // Note that we set this on close as well to what its correct value
            // would be. Setting it on the second plot here to *something*
            // ensures our special case for orientation on the first segment
            // for open segments gets handled correctly (we assume there is a
            // fictional previous point going in the same direction as the
            // previous segment to this one).
            self.segments[0].orientation = current.orientation;
        }

        self.segments.push(current);
    }

    /// Asserts that there is at least two segments in the contour, and that the
    /// contour is not closed.
    fn close(&mut self) {
        assert!(self.segments.len() >= 2);
        assert!(!self.closed);

        // PORT NOTE (rustty): the reference wraps this in a `plot_last:`
        // labelled block with two `break :plot_last` exits; a private helper
        // with two `return`s is the direct Rust equivalent.
        self.close_plot_last();

        {
            self.closed = true;
            let prev = self.segments[self.segments.len() - 1];
            let current = &mut self.segments[0];
            current.orientation = if prev.slope.compare(current.slope) < 0 {
                Orientation::Cw
            } else {
                Orientation::Ccw
            };
        }
    }

    /// The reference's `plot_last:` block inside `close`.
    fn close_plot_last(&mut self) {
        let prev = self.segments[self.segments.len() - 1];
        if prev.p1.equal(self.segments[0].p0) {
            // Skip adding a segment as both the ending point and initial
            // point are the same
            return;
        }

        let mut current = Segment::init(prev.p1, self.segments[0].p0);
        current.orientation = match prev.slope.compare(current.slope) {
            -1 => Orientation::Cw,
            1 => Orientation::Ccw,
            _ => {
                let last = self.segments.len() - 1;
                self.segments[last].p1 = self.segments[0].p0;
                return;
            }
        };

        self.segments.push(current);
    }

    /// Aligns the segments in the contour so that the first left-most vertex
    /// encountered (defined as p0 in the Segment) is the first point. Order of
    /// the polyline is preserved, under the assumption that the polyline
    /// represents a closed contour.
    ///
    /// Asserts that the contour is closed.
    pub fn align_segments(&mut self) {
        assert!(self.closed);
        let mut start_idx: usize = 0;
        for (new_idx, seg) in self.segments.iter().enumerate() {
            if seg.p0.x < self.segments[start_idx].p0.x {
                start_idx = new_idx;
            }
        }

        // This can be optimized if need be.
        self.segments.rotate_left(start_idx);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Orientation {
    Invalid,
    Cw,
    Ccw,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Segment {
    pub p0: Point,
    pub p1: Point,
    pub orientation: Orientation,
    pub slope: Slope,
}

impl Segment {
    /// Asserts the two points are not equal.
    fn init(p0: Point, p1: Point) -> Segment {
        assert!(!p0.equal(p1));
        Segment {
            p0,
            p1,
            orientation: Orientation::Invalid,
            slope: Slope::init(p0, p1),
        }
    }
}

/// Slope for the segment, always stored in normalized form. Reduced
/// functionality of our slope bits used for tessellation/stroking.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Slope {
    pub dx: f64,
    pub dy: f64,
}

impl Slope {
    /// Asserts that a != b.
    fn init(a: Point, b: Point) -> Slope {
        assert!(!a.equal(b));
        let dx_real = b.x - a.x;
        let dy_real = b.y - a.y;

        if dx_real == 0.0 {
            return if dy_real > 0.0 {
                Slope { dx: 0.0, dy: 1.0 }
            } else {
                Slope { dx: 0.0, dy: -1.0 }
            };
        } else if dy_real == 0.0 {
            return if dx_real > 0.0 {
                Slope { dx: 1.0, dy: 0.0 }
            } else {
                Slope { dx: -1.0, dy: 0.0 }
            };
        }

        // const mag = std.math.hypot(dx_real, dy_real); (Zig's, not libm's: S97-F1)
        let mag = crate::internal::zig_math::hypot(dx_real, dy_real);
        Slope {
            dx: dx_real / mag,
            dy: dy_real / mag,
        }
    }

    /// PORT NOTE (rustty): the reference is
    /// `@as(i2, std.math.sign(a_dy * b_dx - b_dy * a_dx))`, i.e. the sign of the
    /// cross product, in -1/0/1. NaN (only reachable from a non-finite input
    /// point) maps to 0, matching `std.math.sign`'s "neither > nor <" case as
    /// consumed by the reference's `else =>` arms.
    fn compare(self, b: Slope) -> i8 {
        let a_dx = self.dx;
        let a_dy = self.dy;
        let b_dx = Self::snap_eps_to(self.dx, b.dx);
        let b_dy = Self::snap_eps_to(self.dy, b.dy);
        // NOTE: We don't do tie breakers here since a lot of them should not
        // apply to normalized slopes. If we need them we can add them back.
        let v = a_dy * b_dx - b_dy * a_dx;
        if v < 0.0 {
            -1
        } else if v > 0.0 {
            1
        } else {
            0
        }
    }

    fn snap_eps_to(a: f64, b: f64) -> f64 {
        if (b - a).abs() > f64::EPSILON {
            b
        } else {
            a
        }
    }
}
