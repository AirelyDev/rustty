// SPDX-License-Identifier: MPL-2.0
//   Copyright © 2024-2025 Chris Marchesi
//   Copyright © 2002 University of Southern California
//
// Portions of the code in this file have been derived and adapted from the
// Cairo project (https://www.cairographics.org/), notably cairo-arc.c and
// cairo-matrix.c.
//
// Ported to Rust for rustty from z2d 0.10.0 `src/internal/arc.zig` (R3 vendored
// dep).
//
// SUBSET NOTE (rustty): the full arc decomposition is ported —
// `transformed_circle_major_axis` (used by the stroke `Pen` to size its vertex
// fan) plus `arc_in_direction` and its helpers (`arc_error_normalized`,
// `arc_max_angle_for_tolerance_normalized`, `arc_segments_needed`,
// `arc_segment`), which decompose a circular arc into bezier splines. Ghostty's
// `branch.zig`/`symbols_for_legacy_computing.zig`/`special.zig` sprites draw
// circles via `Context.arc`, so the arc path is now exercised.
//
// The Zig original threads the spline emission through a `PathVTable` indirection
// (so `arc_in_direction` can drive any path-like target); the only consumer is
// `Path` itself, so this port operates directly on `&mut Path`, inlining the
// `arc_line_to` no-op (skip a `lineTo` whose destination equals the current
// point) that the Zig vtable wrapper applied.

use crate::options;
use crate::path::{Error, Path};
use crate::transformation::Transformation;

use std::f64::consts::PI;

const MAX_FULL_CIRCLES: u32 = 65536;

// arc_error_normalized and arc_max_angle_for_tolerance_normalized are used for
// determining the largest angle that can be used in our arc segment splines
// within our set error tolerance.
//
// Spline deviation from the circle in radius would be given by:
//
//      error = sqrt (x**2 + y**2) - 1
//
// A simpler error function to work with is:
//
//      e = x**2 + y**2 - 1
//
// From "Good approximation of circles by curvature-continuous Bezier
// curves", Tor Dokken and Morten Daehlen, Computer Aided Geometric
// Design 8 (1990) 22-41, we learn:
//
//      abs (max(e)) = 4/27 * sin**6(angle/4) / cos**2(angle/4)
//
// and
//      abs (error) =~ 1/2 * e

fn arc_error_normalized(angle: f64) -> f64 {
    2.0 / 27.0 * (angle / 4.0).sin().powf(6.0) / (angle / 4.0).cos().powf(2.0)
}

// The tolerance/error table below is transcribed verbatim from the Zig source;
// several entries carry more digits than an `f64` can hold, which is fidelity,
// not a bug.
#[allow(clippy::excessive_precision)]
fn arc_max_angle_for_tolerance_normalized(tolerance: f64) -> f64 {
    // Use table lookup to reduce search time in most cases.
    struct Entry {
        angle: f64,
        err: f64,
    }
    let table = [
        Entry {
            angle: PI / 1.0,
            err: 0.0185185185185185036127,
        },
        Entry {
            angle: PI / 2.0,
            err: 0.000272567143730179811158,
        },
        Entry {
            angle: PI / 3.0,
            err: 2.38647043651461047433e-05,
        },
        Entry {
            angle: PI / 4.0,
            err: 4.2455377443222443279e-06,
        },
        Entry {
            angle: PI / 5.0,
            err: 1.11281001494389081528e-06,
        },
        Entry {
            angle: PI / 6.0,
            err: 3.72662000942734705475e-07,
        },
        Entry {
            angle: PI / 7.0,
            err: 1.47783685574284411325e-07,
        },
        Entry {
            angle: PI / 8.0,
            err: 6.63240432022601149057e-08,
        },
        Entry {
            angle: PI / 9.0,
            err: 3.2715520137536980553e-08,
        },
        Entry {
            angle: PI / 10.0,
            err: 1.73863223499021216974e-08,
        },
        Entry {
            angle: PI / 11.0,
            err: 9.81410988043554039085e-09,
        },
    ];

    // this value is chosen arbitrarily. this gives an error of about 1.74909e-20
    let max_segments = 1000;

    for entry in table.iter() {
        if entry.err < tolerance {
            return entry.angle;
        }
    }

    let mut angle: f64 = 0.0;
    for i in table.len()..max_segments {
        angle = PI / (i as f64);
        let err = arc_error_normalized(angle);
        if err <= tolerance {
            break;
        }
    }

    angle
}

fn arc_segments_needed(angle: f64, radius: f64, ctm: Transformation, tolerance: f64) -> i32 {
    // the error is amplified by at most the length of the circle.
    let major_axis = transformed_circle_major_axis(ctm, radius);
    let max_angle = arc_max_angle_for_tolerance_normalized(tolerance / major_axis);

    (angle.abs() / max_angle).ceil() as i32
}

/// Returns the length of the major axis of the ellipse that a circle of the
/// given `radius` in user space maps to under `matrix` in device space.
pub fn transformed_circle_major_axis(matrix: Transformation, radius: f64) -> f64 {
    // This lengthy explanation was taken from the comments above the
    // implementation of Cairo's _cairo_matrix_transformed_circle_major_axis in
    // cairo-matrix.c. I've preserved it in its entirety just to have it close to
    // the code as it's an important piece to understanding how we use the CTM to
    // calculate the major axis.
    //
    // A circle in user space is transformed into an ellipse in device space.
    //
    // The following is a derivation of a formula to calculate the length of the
    // major axis for this ellipse; this is useful for error bounds calculations.
    //
    // Thanks to Walter Brisken <wbrisken@aoc.nrao.edu> for this derivation:
    //
    // 1.  First some notation:
    //
    // All capital letters represent vectors in two dimensions.  A prime '
    // represents a transformed coordinate.  Matrices are written in underlined
    // form, ie _R_.  Lowercase letters represent scalar real values.
    //
    // 2.  The question has been posed:  What is the maximum expansion factor
    // achieved by the linear transformation
    //
    // X' = X _R_
    //
    // where _R_ is a real-valued 2x2 matrix with entries:
    //
    // _R_ = [a b]
    //       [c d]  .
    //
    // In other words, what is the maximum radius, MAX[ |X'| ], reached for any
    // X on the unit circle ( |X| = 1 ) ?
    //
    // 3.  Some useful formulae
    //
    // (A) through (C) below are standard double-angle formulae.  (D) is a lesser
    // known result and is derived below:
    //
    // (A)  sin²(θ) = (1 - cos(2*θ))/2
    // (B)  cos²(θ) = (1 + cos(2*θ))/2
    // (C)  sin(θ)*cos(θ) = sin(2*θ)/2
    // (D)  MAX[a*cos(θ) + b*sin(θ)] = sqrt(a² + b²)
    //
    // Proof of (D):
    //
    // find the maximum of the function by setting the derivative to zero:
    //
    //      -a*sin(θ)+b*cos(θ) = 0
    //
    // From this it follows that
    //
    //      tan(θ) = b/a
    //
    // and hence
    //
    //      sin(θ) = b/sqrt(a² + b²)
    //
    // and
    //
    //      cos(θ) = a/sqrt(a² + b²)
    //
    // Thus the maximum value is
    //
    //      MAX[a*cos(θ) + b*sin(θ)] = (a² + b²)/sqrt(a² + b²)
    //                                  = sqrt(a² + b²)
    //
    // 4.  Derivation of maximum expansion
    //
    // To find MAX[ |X'| ] we search brute force method using calculus.  The unit
    // circle on which X is constrained is to be parameterized by t:
    //
    //      X(θ) = (cos(θ), sin(θ))
    //
    // Thus
    //
    //      X'(θ) = X(θ) * _R_ = (cos(θ), sin(θ)) * [a b]
    //                                              [c d]
    //            = (a*cos(θ) + c*sin(θ), b*cos(θ) + d*sin(θ)).
    //
    // Define
    //
    //      r(θ) = |X'(θ)|
    //
    // Thus
    //
    //      r²(θ) = (a*cos(θ) + c*sin(θ))² + (b*cos(θ) + d*sin(θ))²
    //            = (a² + b²)*cos²(θ) + (c² + d²)*sin²(θ)
    //                + 2*(a*c + b*d)*cos(θ)*sin(θ)
    //
    // Now apply the double angle formulae (A) to (C) from above:
    //
    //      r²(θ) = (a² + b² + c² + d²)/2
    //         + (a² + b² - c² - d²)*cos(2*θ)/2
    //      + (a*c + b*d)*sin(2*θ)
    //            = f + g*cos(φ) + h*sin(φ)
    //
    // Where
    //
    //      f = (a² + b² + c² + d²)/2
    //      g = (a² + b² - c² - d²)/2
    //      h = (a*c + d*d)
    //      φ = 2*θ
    //
    // It is clear that MAX[ |X'| ] = sqrt(MAX[ r² ]).  Here we determine MAX[ r² ]
    // using (D) from above:
    //
    //      MAX[ r² ] = f + sqrt(g² + h²)
    //
    // And finally
    //
    //      MAX[ |X'| ] = sqrt( f + sqrt(g² + h²) )
    //
    // Which is the solution to this problem.
    //
    // Walter Brisken
    // 2004/10/08
    //
    // (Note that the minor axis length is at the minimum of the above solution,
    // which is just sqrt ( f - sqrt(g² + h²) ) given the symmetry of (D)).
    //
    //
    // For another derivation of the same result, using Singular Value Decomposition,
    // see doc/tutorial/src/singular.c.
    let a: f64 = matrix.ax;
    let b: f64 = matrix.by;
    let c: f64 = matrix.cx;
    let d: f64 = matrix.dy;

    // Unrolled and abridged _cairo_matrix_has_unity_scale here. Checks if the
    // matrix is only 90 degree rotations or flips.
    let has_unity_scale: bool = {
        // Possible FIXME if things break here. This is derived from expanding
        // Cairo's SCALING_EPSILON manually since we don't use fixed point;
        // it's possible that "close to zero" in Cairo means "zero" for us due
        // to this.
        //
        // This is ultimately the minimum value within Cairo's 24.8 fixed-point
        // notation (1/256).
        let scaling_epsilon: f64 = 0.00390625;
        // check that the determinant is near +/-1
        let det: f64 = matrix.ax * matrix.dy - matrix.by * matrix.cx;
        if (det * det - 1.0).abs() < scaling_epsilon {
            // check that one axis is close to zero
            if matrix.by.abs() < scaling_epsilon && matrix.cx.abs() < scaling_epsilon {
                true
            } else {
                matrix.ax.abs() < scaling_epsilon && matrix.dy.abs() < scaling_epsilon
            }
            // If rotations are allowed then it must instead test for
            // orthogonality. This is xx*xy+yx*yy ~= 0.
        } else {
            false
        }
    };

    if has_unity_scale {
        return radius;
    }

    let i: f64 = a * a + b * b;
    let j: f64 = c * c + d * d;

    let f: f64 = 0.5 * (i + j);
    let g: f64 = 0.5 * (i - j);
    let h: f64 = a * c + b * d;

    // return radius * (f + math.hypot(g, h)); (Zig's hypot, not libm's: S97-F1)
    radius * (f + crate::internal::zig_math::hypot(g, h)).sqrt()

    //
    // we don't need the minor axis length, which is
    // double min = radius * sqrt (f - sqrt (g*g+h*h));
    //
}

// We want to draw a single spline approximating a circular arc radius
// R from angle A to angle B. Since we want a symmetric spline that
// matches the endpoints of the arc in position and slope, we know
// that the spline control points must be:
//
//      (R * cos(A), R * sin(A))
//      (R * cos(A) - h * sin(A), R * sin(A) + h * cos (A))
//      (R * cos(B) + h * sin(B), R * sin(B) - h * cos (B))
//      (R * cos(B), R * sin(B))
//
// for some value of h.
//
// "Approximation of circular arcs by cubic polynomials", Michael
// Goldapp, Computer Aided Geometric Design 8 (1991) 227-238, provides
// various values of h along with error analysis for each.
//
// From that paper, a very practical value of h is:
//
//      h = 4/3 * R * tan(angle/4)
//
// This value does not give the spline with minimal error, but it does
// provide a very good approximation, (6th-order convergence), and the
// error expression is quite simple, (see the comment for
// arc_error_normalized).

#[allow(non_snake_case)]
fn arc_segment(
    path: &mut Path,
    xc: f64,
    yc: f64,
    radius: f64,
    angle_A: f64,
    angle_B: f64,
) -> Result<(), Error> {
    let r_sin_A = radius * angle_A.sin();
    let r_cos_A = radius * angle_A.cos();
    let r_sin_B = radius * angle_B.sin();
    let r_cos_B = radius * angle_B.cos();

    let h = 4.0 / 3.0 * ((angle_B - angle_A) / 4.0).tan();

    // arc_curve_to in the Zig original simply forwards to `curveTo`.
    path.curve_to(
        xc + r_cos_A - h * r_sin_A,
        yc + r_sin_A + h * r_cos_A,
        xc + r_cos_B + h * r_sin_B,
        yc + r_sin_B - h * r_cos_B,
        xc + r_cos_B,
        yc + r_sin_B,
    )
}

/// The direction an arc is drawn in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Forward,
    Reverse,
}

/// Checks for NaNs/Infs by squaring the number (will detect overflow in
/// addition to NaN/Inf propagation).
fn is_finite(x: f64) -> bool {
    x * x >= 0.0
}

/// The `arc_line_to` wrapper from the Zig original: a `lineTo` that is a no-op
/// if the current point already equals the destination. This is used to avoid
/// drawing artifacts for now on arcs, but if it's needed generally, we can add
/// it to lineTo proper itself.
fn arc_line_to(path: &mut Path, x: f64, y: f64) {
    if let Some(p) = path.current_point {
        if p.x == x && p.y == y {
            return;
        }
    }
    path.line_to(x, y);
}

#[allow(clippy::too_many_arguments)]
pub fn arc_in_direction(
    path: &mut Path,
    xc: f64,
    yc: f64,
    radius: f64,
    angle_min: f64,
    angle_max: f64,
    dir: Direction,
    ctm: Transformation,
    tolerance_: Option<f64>,
) -> Result<(), Error> {
    let mut amin = angle_min;
    let mut amax = angle_max;
    let tolerance = tolerance_.unwrap_or(options::DEFAULT_TOLERANCE);

    if !is_finite(amax) || !is_finite(amin) {
        return Ok(());
    }

    debug_assert!(amax >= amin);

    if amax - amin > 2.0 * PI * (MAX_FULL_CIRCLES as f64) {
        // Zig `@mod` is floored modulo; for the positive divisor `2 * PI` this
        // is `rem_euclid`.
        amax = (amax - amin).rem_euclid(2.0 * PI);
        amin = amin.rem_euclid(2.0 * PI);
        amax += amin + 2.0 * PI * (MAX_FULL_CIRCLES as f64);
    }

    // Recurse if drawing arc larger than pi
    if amax - amin > PI {
        let amid = amin + (amax - amin) / 2.0;
        if dir == Direction::Forward {
            arc_in_direction(path, xc, yc, radius, amin, amid, dir, ctm, Some(tolerance))?;
            arc_in_direction(path, xc, yc, radius, amid, amax, dir, ctm, Some(tolerance))?;
        } else {
            arc_in_direction(path, xc, yc, radius, amid, amax, dir, ctm, Some(tolerance))?;
            arc_in_direction(path, xc, yc, radius, amin, amid, dir, ctm, Some(tolerance))?;
        }
    } else if amax != amin {
        let mut segments = arc_segments_needed(amax - amin, radius, ctm, tolerance);
        let mut step = (amax - amin) / (segments as f64);
        segments -= 1;

        if dir == Direction::Reverse {
            // Faithful to the Zig original's temp-swap of amin/amax.
            #[allow(clippy::manual_swap)]
            {
                let t = amin;
                amin = amax;
                amax = t;
            }

            step = -step;
        }

        arc_line_to(path, xc + radius * amin.cos(), yc + radius * amin.sin());

        for _ in 0..segments.max(0) {
            arc_segment(path, xc, yc, radius, amin, amin + step)?;
            amin += step;
        }

        arc_segment(path, xc, yc, radius, amin, amax)?;
    } else {
        arc_line_to(path, xc + radius * amin.cos(), yc + radius * amin.sin());
    }

    Ok(())
}
