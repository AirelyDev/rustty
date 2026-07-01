// SPDX-License-Identifier: MPL-2.0
//   Copyright © 2024-2025 Chris Marchesi
//   Copyright © 2002 University of Southern California
//
// Portions of the code in this file have been derived and adapted from the
// Cairo project (https://www.cairographics.org/), notably cairo-spline.c.
//
// Ported to Rust for rustty from z2d 0.10.0 `src/internal/tess/Spline.zig`
// (R3 vendored dep).

//! Given a set of four points representing a bezier curve ("spline"), subdivide
//! the curve into a series of line_to nodes.
//!
//! The supplied tolerance is in pixels (fractions are supported). Higher
//! tolerance will give better performance, while a smaller tolerance will give
//! better quality.

use crate::internal::error::InternalError;
use crate::internal::path_nodes::PathLineTo;
use crate::internal::point::Point;

/// Plotter callback trait: receives each emitted `line_to`.
pub trait Plotter {
    fn line_to(&mut self, node: PathLineTo) -> Result<(), InternalError>;
}

impl<F: FnMut(PathLineTo) -> Result<(), InternalError>> Plotter for F {
    fn line_to(&mut self, node: PathLineTo) -> Result<(), InternalError> {
        self(node)
    }
}

pub struct Spline<'a, P: Plotter> {
    // Initial points.
    pub a: Point,
    pub b: Point,
    pub c: Point,
    pub d: Point,

    // Error tolerance
    pub tolerance: f64,

    // Plotter implementation.
    pub plotter_impl: &'a mut P,
}

impl<P: Plotter> Spline<'_, P> {
    /// Run decomposition on the spline to break it down to its individual lines,
    /// plotting each line along the way.
    pub fn decompose(&mut self) -> Result<(), InternalError> {
        // Both tangents being zero means that this is just a straight line.
        if self.a.equal(self.b) && self.c.equal(self.d) {
            self.plotter_impl.line_to(PathLineTo { point: self.d })?;
            return Ok(());
        }

        // Our initial knot set
        let mut s1 = Knots {
            a: self.a,
            b: self.b,
            c: self.c,
            d: self.d,
        };

        // Decompose the curve into its individual points, plotting them along
        // the way.
        let start = self.a;
        let tol = self.tolerance * self.tolerance;
        Self::decompose_into(self.plotter_impl, &mut s1, start, tol)?;

        // Plot our last point in the curve before finishing
        self.plotter_impl.line_to(PathLineTo { point: self.d })?;
        Ok(())
    }

    /// Inner and recursive decomposition into the specified knot set.
    fn decompose_into(
        plotter: &mut P,
        s1: &mut Knots,
        start: Point,
        tolerance: f64,
    ) -> Result<(), InternalError> {
        if s1.error_sq() < tolerance {
            if !s1.a.equal(start) {
                plotter.line_to(PathLineTo { point: s1.a })?;
            }
            return Ok(());
        }

        // Split our spline
        let mut s2 = s1.de_casteljau();

        // Recurse into each half
        Self::decompose_into(plotter, s1, start, tolerance)?;
        Self::decompose_into(plotter, &mut s2, start, tolerance)?;
        Ok(())
    }
}

/// Represents knots on a spline.
struct Knots {
    a: Point,
    b: Point,
    c: Point,
    d: Point,
}

impl Knots {
    /// Returns an upper bound on the error (squared) that could result from
    /// approximating the spline as a line segment connecting the two endpoints.
    fn error_sq(&self) -> f64 {
        // Compute the distance between the control points (A -> B, A -> C). We
        // eventually select the larger of these two as our error.
        let mut b_x_delta = self.b.x - self.a.x;
        let mut b_y_delta = self.b.y - self.a.y;
        let mut c_x_delta = self.c.x - self.a.x;
        let mut c_y_delta = self.c.y - self.a.y;

        if self.a.x != self.d.x || self.a.y != self.d.y {
            let d_x_delta = self.d.x - self.a.x;
            let d_y_delta = self.d.y - self.a.y;
            let d_dot_sq = dot_sq(d_x_delta, d_y_delta);

            let b_d_dot = b_x_delta * d_x_delta + b_y_delta * d_y_delta;
            if b_d_dot >= d_dot_sq {
                b_x_delta -= d_x_delta;
                b_y_delta -= d_y_delta;
            } else {
                b_x_delta -= b_d_dot / d_dot_sq * d_x_delta;
                b_y_delta -= b_d_dot / d_dot_sq * d_y_delta;
            }

            let c_d_dot = c_x_delta * d_x_delta + c_y_delta * d_y_delta;
            if c_d_dot >= d_dot_sq {
                c_x_delta -= d_x_delta;
                c_y_delta -= d_y_delta;
            } else {
                c_x_delta -= c_d_dot / d_dot_sq * d_x_delta;
                c_y_delta -= c_d_dot / d_dot_sq * d_y_delta;
            }
        }

        let b_err = dot_sq(b_x_delta, b_y_delta);
        let c_err = dot_sq(c_x_delta, c_y_delta);

        if b_err > c_err {
            b_err
        } else {
            c_err
        }
    }

    /// Subdivides/linearly interpolates (lerps) the current knot using De
    /// Casteljau labeling. Sets the current knot to the first half (A -> middle
    /// of BC), and returns the second half (middle of BC -> D).
    fn de_casteljau(&mut self) -> Knots {
        let ab = lerp_half(self.a, self.b);
        let bc = lerp_half(self.b, self.c);
        let cd = lerp_half(self.c, self.d);
        let abbc = lerp_half(ab, bc);
        let bccd = lerp_half(bc, cd);
        let final_pt = lerp_half(abbc, bccd);

        // Build the result first since we're swapping stuff in the original
        let result = Knots {
            a: final_pt,
            b: bccd,
            c: cd,
            d: self.d,
        };

        // Update the original
        self.b = ab;
        self.c = abbc;
        self.d = final_pt;

        // Done
        result
    }
}

fn dot_sq(x: f64, y: f64) -> f64 {
    x * x + y * y
}

fn lerp_half(a: Point, b: Point) -> Point {
    // The lerp is basically a + ((b - a) / 2), aka the middle of AB is half of
    // the difference between A and B.
    Point {
        x: a.x + ((b.x - a.x) / 2.0),
        y: a.y + ((b.y - a.y) / 2.0),
    }
}
