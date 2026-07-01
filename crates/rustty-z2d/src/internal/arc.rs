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
// SUBSET NOTE (rustty): only `transformed_circle_major_axis` — the sole part of
// this module the stroke `Pen` uses to size its vertex fan — is ported. The
// arc-decomposition path builders (`arc`/`arc_in_direction` etc.) are excluded,
// as ghostty's sprite/glyf paths use `curve_to`, never `arc`.

use crate::transformation::Transformation;

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

    radius * (f + g.hypot(h)).sqrt()

    //
    // we don't need the minor axis length, which is
    // double min = radius * sqrt (f - sqrt (g*g+h*h));
    //
}
