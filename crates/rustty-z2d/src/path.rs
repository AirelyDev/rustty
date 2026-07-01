// SPDX-License-Identifier: MPL-2.0
//   Copyright © 2024-2025 Chris Marchesi
//
// Ported to Rust for rustty from z2d 0.10.0 `src/Path.zig` (R3 vendored dep).
//
// SUBSET NOTE (rustty): `arc` / `arc_negative` (the `internal/arc.zig` spline
// deconstruction) ARE now ported (backed by `internal::arc::arc_in_direction`),
// as ghostty's `branch`/`symbols_for_legacy_computing`/`special` sprites draw
// circles through `Context.arc`. The Zig `ArrayListUnmanaged`-with-external-
// buffer duality is modelled with a `Vec` plus an optional `cap_limit` that
// reproduces the "assume capacity" overflow as a safety-checked panic (faithful
// to Zig's `appendAssumeCapacity`).

//! Path is the "path builder" type, and contains a set of sub-paths used for
//! filling or stroking operations.

use crate::internal::path_nodes::{PathClose, PathCurveTo, PathLineTo, PathMoveTo, PathNode};
use crate::internal::point::Point;
use crate::options;
use crate::transformation::{self, Transformation, IDENTITY};

/// Errors associated with `Path` point plotting operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// A path operation requires a current point, but does not have one.
    NoCurrentPoint,

    /// A relative path helper encountered an error inverting the current
    /// transformation matrix to translate the current point back to user space
    /// before applying the relative point.
    InvalidMatrix,
}

impl From<transformation::Error> for Error {
    fn from(_: transformation::Error) -> Error {
        Error::InvalidMatrix
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Path {
    /// The underlying node set. Do not edit or populate this directly, use the
    /// builder functions (e.g., `move_to`, `line_to`, `curve_to`, `close`).
    pub nodes: Vec<PathNode>,

    /// The start of the current subpath when working with drawing operations.
    pub initial_point: Option<Point>,

    /// The current point when working with drawing operations.
    pub current_point: Option<Point>,

    /// The tolerance setting used when approximating arcs as splines.
    pub tolerance: f64,

    /// The current transformation matrix (CTM) for this path.
    pub transformation: Transformation,

    /// The fixed capacity limit when the path was created via `init_buffer`
    /// (faithful to Zig's static-buffer-backed `ArrayListUnmanaged`). `None`
    /// for a growable (`init_capacity`/`empty`) path.
    cap_limit: Option<usize>,
}

impl Default for Path {
    fn default() -> Path {
        Path::empty()
    }
}

impl Path {
    /// Represents an empty `Path`.
    pub fn empty() -> Path {
        Path {
            nodes: Vec::new(),
            initial_point: None,
            current_point: None,
            tolerance: options::DEFAULT_TOLERANCE,
            transformation: IDENTITY,
            cap_limit: None,
        }
    }

    /// Initializes the path set with an initial capacity of exactly `num`.
    pub fn init_capacity(num: usize) -> Path {
        Path {
            nodes: Vec::with_capacity(num),
            ..Path::empty()
        }
    }

    /// Initializes a `Path` with a fixed static buffer of `len` nodes. If you
    /// use this over `init_capacity`, do not use any method that "takes an
    /// allocator" (here: exceeding `len` is a safety-checked panic).
    pub fn init_buffer(len: usize) -> Path {
        Path {
            nodes: Vec::with_capacity(len),
            cap_limit: Some(len),
            ..Path::empty()
        }
    }

    /// Rests the path set, clearing all nodes and state.
    pub fn reset(&mut self) {
        self.nodes.clear();
        self.initial_point = None;
        self.current_point = None;
    }

    fn push_node(&mut self, node: PathNode) {
        if let Some(cap) = self.cap_limit {
            assert!(
                self.nodes.len() < cap,
                "z2d StaticPath: append past capacity"
            );
        }
        self.nodes.push(node);
    }

    /// Starts a new path, and moves the current point to it.
    pub fn move_to(&mut self, x: f64, y: f64) {
        self.move_to_assume_capacity(x, y);
    }

    /// Like `move_to`, but assumes enough space exists for the point.
    pub fn move_to_assume_capacity(&mut self, x: f64, y: f64) {
        let point = (Point {
            x: clamp_i24(x),
            y: clamp_i24(y),
        })
        .apply_transform(self.transformation);
        // If our last operation is a move_to to this point, this is a no-op.
        // This ensures that there's no duplicates on things like explicit
        // definitions on close_path -> move_to (versus the implicit add in the
        // close operation).
        if let Some(PathNode::MoveTo(move_to)) = self.nodes.last() {
            if move_to.point.equal(point) {
                return;
            }
        }
        self.push_node(PathNode::MoveTo(PathMoveTo { point }));
        self.initial_point = Some(point);
        self.current_point = Some(point);
    }

    /// Begins a new sub-path relative to the current point. It is an error to
    /// call this without a current point.
    pub fn rel_move_to(&mut self, x: f64, y: f64) -> Result<(), Error> {
        if let Some(p) = self.current_point {
            let user_point = p.apply_inverse_transform(self.transformation)?;
            self.move_to(user_point.x + x, user_point.y + y);
            Ok(())
        } else {
            Err(Error::NoCurrentPoint)
        }
    }

    /// Like `rel_move_to`, but assumes enough space exists for the point.
    pub fn rel_move_to_assume_capacity(&mut self, x: f64, y: f64) -> Result<(), Error> {
        if let Some(p) = self.current_point {
            let user_point = p.apply_inverse_transform(self.transformation)?;
            self.move_to_assume_capacity(user_point.x + x, user_point.y + y);
            Ok(())
        } else {
            Err(Error::NoCurrentPoint)
        }
    }

    /// Draws a line from the current point to the specified point and sets it
    /// as the current point. Acts as a `move_to` instead if there is no current
    /// point.
    pub fn line_to(&mut self, x: f64, y: f64) {
        self.line_to_assume_capacity(x, y);
    }

    /// Like `line_to`, but assumes enough space exists for the point.
    pub fn line_to_assume_capacity(&mut self, x: f64, y: f64) {
        if self.current_point.is_none() {
            return self.move_to_assume_capacity(x, y);
        }
        let point = (Point {
            x: clamp_i24(x),
            y: clamp_i24(y),
        })
        .apply_transform(self.transformation);
        self.push_node(PathNode::LineTo(PathLineTo { point }));
        self.current_point = Some(point);
    }

    /// Draws a line relative to the current point. It is an error to call this
    /// without a current point.
    pub fn rel_line_to(&mut self, x: f64, y: f64) -> Result<(), Error> {
        if let Some(p) = self.current_point {
            let user_point = p.apply_inverse_transform(self.transformation)?;
            self.line_to(user_point.x + x, user_point.y + y);
            Ok(())
        } else {
            Err(Error::NoCurrentPoint)
        }
    }

    /// Like `rel_line_to`, but assumes enough space exists for the point.
    pub fn rel_line_to_assume_capacity(&mut self, x: f64, y: f64) -> Result<(), Error> {
        if let Some(p) = self.current_point {
            let user_point = p.apply_inverse_transform(self.transformation)?;
            self.line_to_assume_capacity(user_point.x + x, user_point.y + y);
            Ok(())
        } else {
            Err(Error::NoCurrentPoint)
        }
    }

    /// Draws a cubic bezier with the three supplied control points from the
    /// current point. It is an error to call this without a current point.
    #[allow(clippy::too_many_arguments)]
    pub fn curve_to(
        &mut self,
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        x3: f64,
        y3: f64,
    ) -> Result<(), Error> {
        if self.current_point.is_none() {
            return Err(Error::NoCurrentPoint);
        }
        self.curve_to_inner(x1, y1, x2, y2, x3, y3);
        Ok(())
    }

    /// Like `curve_to`, but assumes enough space exists for the point.
    #[allow(clippy::too_many_arguments)]
    pub fn curve_to_assume_capacity(
        &mut self,
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        x3: f64,
        y3: f64,
    ) -> Result<(), Error> {
        if self.current_point.is_none() {
            return Err(Error::NoCurrentPoint);
        }
        self.curve_to_inner(x1, y1, x2, y2, x3, y3);
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn curve_to_inner(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, x3: f64, y3: f64) {
        let p1 = (Point {
            x: clamp_i24(x1),
            y: clamp_i24(y1),
        })
        .apply_transform(self.transformation);
        let p2 = (Point {
            x: clamp_i24(x2),
            y: clamp_i24(y2),
        })
        .apply_transform(self.transformation);
        let p3 = (Point {
            x: clamp_i24(x3),
            y: clamp_i24(y3),
        })
        .apply_transform(self.transformation);
        self.push_node(PathNode::CurveTo(PathCurveTo { p1, p2, p3 }));
        self.current_point = Some(p3);
    }

    /// Draws a cubic bezier relative to the current point. It is an error to
    /// call this without a current point.
    #[allow(clippy::too_many_arguments)]
    pub fn rel_curve_to(
        &mut self,
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        x3: f64,
        y3: f64,
    ) -> Result<(), Error> {
        if let Some(p) = self.current_point {
            let user_point = p.apply_inverse_transform(self.transformation)?;
            self.curve_to(
                user_point.x + x1,
                user_point.y + y1,
                user_point.x + x2,
                user_point.y + y2,
                user_point.x + x3,
                user_point.y + y3,
            )
        } else {
            Err(Error::NoCurrentPoint)
        }
    }

    /// Like `rel_curve_to`, but assumes enough space exists for the point.
    #[allow(clippy::too_many_arguments)]
    pub fn rel_curve_to_assume_capacity(
        &mut self,
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        x3: f64,
        y3: f64,
    ) -> Result<(), Error> {
        if let Some(p) = self.current_point {
            let user_point = p.apply_inverse_transform(self.transformation)?;
            self.curve_to_assume_capacity(
                user_point.x + x1,
                user_point.y + y1,
                user_point.x + x2,
                user_point.y + y2,
                user_point.x + x3,
                user_point.y + y3,
            )
        } else {
            Err(Error::NoCurrentPoint)
        }
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
    /// behavior is undesired, call `reset` before calling. This will trigger a
    /// `move_to` before the splines are plotted, creating a new subpath.
    ///
    /// After this operation, the current point will be the end of the arc.
    pub fn arc(
        &mut self,
        xc: f64,
        yc: f64,
        radius: f64,
        angle1: f64,
        angle2: f64,
    ) -> Result<(), Error> {
        let mut effective_angle2 = angle2;
        while effective_angle2 < angle1 {
            effective_angle2 += std::f64::consts::PI * 2.0;
        }
        let transformation = self.transformation;
        let tolerance = self.tolerance.max(0.001);
        crate::internal::arc::arc_in_direction(
            self,
            xc,
            yc,
            radius,
            angle1,
            effective_angle2,
            crate::internal::arc::Direction::Forward,
            transformation,
            Some(tolerance),
        )
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
    ) -> Result<(), Error> {
        let mut effective_angle2 = angle2;
        while effective_angle2 > angle1 {
            effective_angle2 -= std::f64::consts::PI * 2.0;
        }
        let transformation = self.transformation;
        let tolerance = self.tolerance.max(0.001);
        crate::internal::arc::arc_in_direction(
            self,
            xc,
            yc,
            radius,
            effective_angle2,
            angle1,
            crate::internal::arc::Direction::Reverse,
            transformation,
            Some(tolerance),
        )
    }

    /// Closes the path by drawing a line from the current point by the starting
    /// point. No effect if there is no current point.
    pub fn close(&mut self) {
        if self.current_point.is_none() {
            return;
        }
        debug_assert!(self.initial_point.is_some());
        self.close_inner();
    }

    /// Like `close`, but assumes enough space exists for the points.
    ///
    /// Note that path closes require two points, one for the `close_path`
    /// entry, and one for the implicit `move_to` entry; ensure your `Path` has
    /// enough space for both.
    pub fn close_assume_capacity(&mut self) {
        if self.current_point.is_none() {
            return;
        }
        debug_assert!(self.initial_point.is_some());
        self.close_inner();
    }

    fn close_inner(&mut self) {
        self.push_node(PathNode::ClosePath(PathClose {}));

        // Add a move_to immediately after the close_path node. This is
        // explicit, to ensure that the state machine for draw operations
        // (fill, stroke) do not get put into an unreachable state.
        let ip = self.initial_point.unwrap();
        self.push_node(PathNode::MoveTo(PathMoveTo {
            point: Point { x: ip.x, y: ip.y },
        }));
    }

    /// Returns `true` if all subpaths in the path set are currently closed.
    pub fn is_closed(&self) -> bool {
        PathNode::is_closed_node_set(&self.nodes)
    }
}

/// i24 min bound, matching Zig's `math.minInt(i24)`.
const I24_MIN: f64 = -8_388_608.0;
/// i24 max bound, matching Zig's `math.maxInt(i24)`.
const I24_MAX: f64 = 8_388_607.0;

fn clamp_i24(x: f64) -> f64 {
    x.clamp(I24_MIN, I24_MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn cv(p1: (f64, f64), p2: (f64, f64), p3: (f64, f64)) -> PathNode {
        PathNode::CurveTo(PathCurveTo {
            p1: Point { x: p1.0, y: p1.1 },
            p2: Point { x: p2.0, y: p2.1 },
            p3: Point { x: p3.0, y: p3.1 },
        })
    }

    #[test]
    fn move_to_clamped() {
        {
            // Normal
            let mut p = Path::init_capacity(0);
            p.move_to(1.0, 2.0);
            assert_eq!(mv(1.0, 2.0), p.nodes[0]);
            assert_eq!(Some(Point { x: 1.0, y: 2.0 }), p.initial_point);
            assert_eq!(Some(Point { x: 1.0, y: 2.0 }), p.current_point);
        }
        {
            // Clamped
            let mut p = Path::init_capacity(0);
            p.move_to(I24_MIN - 1.0, I24_MAX + 1.0);
            assert_eq!(mv(I24_MIN, I24_MAX), p.nodes[0]);
            assert_eq!(
                Some(Point {
                    x: I24_MIN,
                    y: I24_MAX
                }),
                p.initial_point
            );
            assert_eq!(
                Some(Point {
                    x: I24_MIN,
                    y: I24_MAX
                }),
                p.current_point
            );
        }
    }

    #[test]
    fn line_to_clamped() {
        {
            // Normal
            let mut p = Path::init_capacity(0);
            p.move_to(1.0, 1.0);
            p.line_to(1.0, 2.0);
            assert_eq!(ln(1.0, 2.0), p.nodes[1]);
            assert_eq!(Some(Point { x: 1.0, y: 1.0 }), p.initial_point);
            assert_eq!(Some(Point { x: 1.0, y: 2.0 }), p.current_point);
        }
        {
            // Clamped
            let mut p = Path::init_capacity(0);
            p.move_to(1.0, 1.0);
            p.line_to(I24_MIN - 1.0, I24_MAX + 1.0);
            assert_eq!(ln(I24_MIN, I24_MAX), p.nodes[1]);
            assert_eq!(Some(Point { x: 1.0, y: 1.0 }), p.initial_point);
            assert_eq!(
                Some(Point {
                    x: I24_MIN,
                    y: I24_MAX
                }),
                p.current_point
            );
        }
    }

    #[test]
    fn curve_to_clamped() {
        {
            // Normal
            let mut p = Path::init_capacity(0);
            p.move_to(1.0, 1.0);
            p.curve_to(1.0, 2.0, 3.0, 4.0, 5.0, 6.0).unwrap();
            assert_eq!(cv((1.0, 2.0), (3.0, 4.0), (5.0, 6.0)), p.nodes[1]);
            assert_eq!(Some(Point { x: 1.0, y: 1.0 }), p.initial_point);
            assert_eq!(Some(Point { x: 5.0, y: 6.0 }), p.current_point);
        }
        {
            // Clamped
            let mut p = Path::init_capacity(0);
            p.move_to(1.0, 1.0);
            p.curve_to(I24_MIN - 1.0, I24_MAX + 1.0, 3.0, 4.0, 5.0, 6.0)
                .unwrap();
            assert_eq!(cv((I24_MIN, I24_MAX), (3.0, 4.0), (5.0, 6.0)), p.nodes[1]);
            assert_eq!(Some(Point { x: 1.0, y: 1.0 }), p.initial_point);
            assert_eq!(Some(Point { x: 5.0, y: 6.0 }), p.current_point);
            p.curve_to(1.0, 2.0, I24_MIN - 1.0, I24_MAX + 1.0, 5.0, 6.0)
                .unwrap();
            assert_eq!(cv((1.0, 2.0), (I24_MIN, I24_MAX), (5.0, 6.0)), p.nodes[2]);
            assert_eq!(Some(Point { x: 1.0, y: 1.0 }), p.initial_point);
            assert_eq!(Some(Point { x: 5.0, y: 6.0 }), p.current_point);
            p.curve_to(1.0, 2.0, 3.0, 4.0, I24_MIN - 1.0, I24_MAX + 1.0)
                .unwrap();
            assert_eq!(cv((1.0, 2.0), (3.0, 4.0), (I24_MIN, I24_MAX)), p.nodes[3]);
            assert_eq!(Some(Point { x: 1.0, y: 1.0 }), p.initial_point);
            assert_eq!(
                Some(Point {
                    x: I24_MIN,
                    y: I24_MAX
                }),
                p.current_point
            );
        }
    }

    #[test]
    fn is_closed() {
        {
            // Basic (closed)
            let mut p = Path::init_capacity(0);
            p.move_to(1.0, 1.0);
            p.line_to(2.0, 2.0);
            p.line_to(3.0, 3.0);
            p.close();
            assert!(p.is_closed());
        }
        {
            // Multiple subpaths, all closed
            let mut p = Path::init_capacity(0);
            p.move_to(1.0, 1.0);
            p.line_to(2.0, 2.0);
            p.line_to(3.0, 3.0);
            p.close();
            p.move_to(4.0, 4.0);
            p.line_to(5.0, 5.0);
            p.line_to(6.0, 6.0);
            p.close();
            assert!(p.is_closed());
        }
        {
            // Basic (not closed)
            let mut p = Path::init_capacity(0);
            p.move_to(1.0, 1.0);
            p.line_to(2.0, 2.0);
            p.move_to(3.0, 3.0);
            p.line_to(4.0, 4.0);
            p.line_to(5.0, 5.0);
            assert!(!p.is_closed());
        }
        {
            // Closed in the middle
            let mut p = Path::init_capacity(0);
            p.move_to(1.0, 1.0);
            p.line_to(2.0, 2.0);
            p.close();
            p.move_to(3.0, 3.0);
            p.line_to(4.0, 4.0);
            p.line_to(5.0, 5.0);
            assert!(!p.is_closed());
        }
        {
            // Closed at the end (not in the middle)
            let mut p = Path::init_capacity(0);
            p.move_to(1.0, 1.0);
            p.line_to(2.0, 2.0);
            p.move_to(3.0, 3.0);
            p.line_to(4.0, 4.0);
            p.line_to(5.0, 5.0);
            p.close();
            assert!(!p.is_closed());
        }
        {
            // Empty node set
            let p = Path::init_capacity(0);
            assert!(!p.is_closed());
        }
    }

    #[test]
    fn rel_move_to() {
        {
            // Normal
            let mut p = Path::init_capacity(0);
            p.move_to(1.0, 1.0);
            p.rel_move_to(1.0, 1.0).unwrap();
            assert_eq!(mv(1.0, 1.0), p.nodes[0]);
            assert_eq!(mv(2.0, 2.0), p.nodes[1]);
            assert_eq!(Some(Point { x: 2.0, y: 2.0 }), p.initial_point);
            assert_eq!(Some(Point { x: 2.0, y: 2.0 }), p.current_point);
        }
        {
            // Reverse
            let mut p = Path::init_capacity(0);
            p.move_to(1.0, 1.0);
            p.rel_move_to(-10.0, -10.0).unwrap();
            assert_eq!(mv(1.0, 1.0), p.nodes[0]);
            assert_eq!(mv(-9.0, -9.0), p.nodes[1]);
            assert_eq!(Some(Point { x: -9.0, y: -9.0 }), p.initial_point);
            assert_eq!(Some(Point { x: -9.0, y: -9.0 }), p.current_point);
        }
        {
            // No current point
            let mut p = Path::init_capacity(0);
            assert_eq!(Err(Error::NoCurrentPoint), p.rel_move_to(1.0, 1.0));
        }
    }

    #[test]
    fn rel_line_to() {
        {
            // Normal
            let mut p = Path::init_capacity(0);
            p.move_to(1.0, 1.0);
            p.rel_line_to(1.0, 1.0).unwrap();
            assert_eq!(mv(1.0, 1.0), p.nodes[0]);
            assert_eq!(ln(2.0, 2.0), p.nodes[1]);
            assert_eq!(Some(Point { x: 1.0, y: 1.0 }), p.initial_point);
            assert_eq!(Some(Point { x: 2.0, y: 2.0 }), p.current_point);
        }
        {
            // Reverse
            let mut p = Path::init_capacity(0);
            p.move_to(1.0, 1.0);
            p.rel_line_to(-10.0, -10.0).unwrap();
            assert_eq!(mv(1.0, 1.0), p.nodes[0]);
            assert_eq!(ln(-9.0, -9.0), p.nodes[1]);
            assert_eq!(Some(Point { x: 1.0, y: 1.0 }), p.initial_point);
            assert_eq!(Some(Point { x: -9.0, y: -9.0 }), p.current_point);
        }
        {
            // No current point
            let mut p = Path::init_capacity(0);
            assert_eq!(Err(Error::NoCurrentPoint), p.rel_line_to(1.0, 1.0));
        }
    }

    #[test]
    fn rel_curve_to() {
        {
            // Normal
            let mut p = Path::init_capacity(0);
            p.move_to(1.0, 1.0);
            p.rel_curve_to(1.0, 1.0, 2.0, 2.0, 3.0, 3.0).unwrap();
            assert_eq!(mv(1.0, 1.0), p.nodes[0]);
            assert_eq!(cv((2.0, 2.0), (3.0, 3.0), (4.0, 4.0)), p.nodes[1]);
            assert_eq!(Some(Point { x: 1.0, y: 1.0 }), p.initial_point);
            assert_eq!(Some(Point { x: 4.0, y: 4.0 }), p.current_point);
        }
        {
            // Reverse
            let mut p = Path::init_capacity(0);
            p.move_to(1.0, 1.0);
            p.rel_curve_to(-10.0, -10.0, -11.0, -11.0, -12.0, -12.0)
                .unwrap();
            assert_eq!(mv(1.0, 1.0), p.nodes[0]);
            assert_eq!(cv((-9.0, -9.0), (-10.0, -10.0), (-11.0, -11.0)), p.nodes[1]);
            assert_eq!(Some(Point { x: 1.0, y: 1.0 }), p.initial_point);
            assert_eq!(Some(Point { x: -11.0, y: -11.0 }), p.current_point);
        }
        {
            // No current point
            let mut p = Path::init_capacity(0);
            assert_eq!(
                Err(Error::NoCurrentPoint),
                p.rel_curve_to(1.0, 1.0, 2.0, 2.0, 3.0, 3.0)
            );
        }
    }

    #[test]
    fn relative_helpers_with_transformations() {
        let mut p = Path::init_capacity(0);

        p.transformation = p.transformation.translate(100.0, 200.0);
        p.move_to(0.0, 0.0);
        assert_eq!(mv(100.0, 200.0), p.nodes[0]);
        p.rel_move_to(-10.0, -20.0).unwrap();
        assert_eq!(mv(90.0, 180.0), p.nodes[1]);

        p.reset();
        p.move_to(0.0, 0.0);
        p.rel_line_to(-10.0, -20.0).unwrap();
        assert_eq!(ln(90.0, 180.0), p.nodes[1]);

        p.reset();
        p.move_to(0.0, 0.0);
        p.rel_curve_to(-10.0, -20.0, 30.0, 40.0, -50.0, -60.0)
            .unwrap();
        assert_eq!(cv((90.0, 180.0), (130.0, 240.0), (50.0, 140.0)), p.nodes[1]);
    }
}
