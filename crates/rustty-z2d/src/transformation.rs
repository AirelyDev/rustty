// SPDX-License-Identifier: MPL-2.0
//   Copyright © 2024-2025 Chris Marchesi
//
// Ported to Rust for rustty from z2d 0.10.0 `src/Transformation.zig` (R3 vendored dep).

//! A Transformation represents an affine transformation matrix.
//!
//! We use named parameters, as outlined below:
//!
//! ```text
//! [ ax by tx ]
//! [ cx dy ty ]
//! [  0  0  1 ]
//! ```
//!
//! Since all named operations operate on the named fields using the assumption
//! of the affine transformation matrix, more complex operations (such as
//! `inverse`) should be safe, so long as you stick to these methods and don't
//! use `mul`. Checks are still made on functions that are possibly unsafe,
//! however, to ensure that a matrix is valid before execution. Any operation
//! that would fail due to a result of this will fail with an `InvalidMatrix`
//! error.

/// Errors associated with matrix transformation operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// The matrix is invalid for the specific operation.
    InvalidMatrix,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transformation {
    pub ax: f64,
    pub by: f64,
    pub cx: f64,
    pub dy: f64,
    pub tx: f64,
    pub ty: f64,
}

/// Represents the identity matrix; a matrix or point multiplied by this matrix
/// yields the original.
pub const IDENTITY: Transformation = Transformation {
    ax: 1.0,
    by: 0.0,
    cx: 0.0,
    dy: 1.0,
    tx: 0.0,
    ty: 0.0,
};

impl Transformation {
    /// Represents the identity matrix; a matrix or point multiplied by this
    /// matrix yields the original.
    pub const IDENTITY: Transformation = IDENTITY;

    /// Returns `true` if the two matrices are equal.
    pub fn equal(self, b: Transformation) -> bool {
        let a = self;
        a.ax == b.ax && a.by == b.by && a.cx == b.cx && a.dy == b.dy && a.tx == b.tx && a.ty == b.ty
    }

    /// Computes the determinant of the matrix.
    ///
    /// Note that the determinant of an affine transformation matrix reduces to
    /// a simple ad - bc.
    pub fn determinant(self) -> f64 {
        let a = self;
        a.ax * a.dy - a.by * a.cx
    }

    /// Multiplies a transformation matrix with another.
    // Named `mul` to mirror z2d's `Transformation.mul`; this is matrix
    // composition, not the `std::ops::Mul` scalar/elementwise trait.
    #[allow(clippy::should_implement_trait)]
    pub fn mul(self, b: Transformation) -> Transformation {
        let a = self;
        // [ a.ax a.by a.tx ] [ b.ax b.by b.tx ]
        // [ a.cx a.dy a.ty ] [ b.cx b.dy b.ty ] ·
        // [    0    0    1 ] [    0    0    1 ]
        //
        // =
        //
        // [ (a.ax * b.ax + a.by * b.cx) (a.ax * b.by + a.by * b.dy) (a.ax * b.tx + a.by * b.ty + a.tx) ]
        // [ (a.cx * b.ax + a.dy * b.cx) (a.cx * b.by + a.dy * b.dy) (a.cx * b.tx + a.dy * b.ty + a.ty) ]
        // [                          0                           0                                  1  ]
        //
        Transformation {
            ax: a.ax * b.ax + a.by * b.cx,
            by: a.ax * b.by + a.by * b.dy,
            cx: a.cx * b.ax + a.dy * b.cx,
            dy: a.cx * b.by + a.dy * b.dy,
            ty: a.cx * b.tx + a.dy * b.ty + a.ty,
            tx: a.ax * b.tx + a.by * b.ty + a.tx,
        }
    }

    fn mul_scalar(self, x: f64) -> Transformation {
        let a = self;
        Transformation {
            ax: a.ax * x,
            by: a.by * x,
            cx: a.cx * x,
            dy: a.dy * x,
            ty: a.ty * x,
            tx: a.tx * x,
        }
    }

    /// Returns the inverse of this Transformation matrix. `InvalidMatrix` is
    /// returned if the matrix is not invertible.
    pub fn inverse(self) -> Result<Transformation, Error> {
        let a = self;
        // Determine some special cases first (scale + translate only, or
        // translate only).
        if a.by == 0.0 && a.cx == 0.0 {
            if a.ax == 0.0 || a.dy == 0.0 {
                return Err(Error::InvalidMatrix); // We can't invert the determinant
            } else if a.ax != 1.0 || a.dy != 1.0 {
                // Scale + translate, this is the more complex case, but can be
                // ultimately reduced to some simple calculations due to the fact
                // our determinant is only ad (no -bc).
                return Ok(Transformation {
                    ax: 1.0 / a.ax,
                    by: 0.0,
                    cx: 0.0,
                    dy: 1.0 / a.dy,
                    tx: -a.tx / a.ax,
                    ty: -a.ty / a.dy,
                });
            } else {
                // The simpler case where since both a and d are 1, we're only
                // left with negating translations.
                return Ok(Transformation {
                    ax: 1.0,
                    by: 0.0,
                    cx: 0.0,
                    dy: 1.0,
                    tx: -a.tx,
                    ty: -a.ty,
                });
            }
        }

        // Neither of the special cases apply, so we need to do the more complex
        // inverse. However, the inverse of the transformation matrix can be
        // simplified from the general case quite a bit.
        //
        // For example, the determinant reduces to the standard ad - bc, so
        // let's do that first.
        let det = a.determinant();
        if det == 0.0 {
            return Err(Error::InvalidMatrix); // We can't invert the determinant
        }

        // We can just short-circuit to our adjunct below according to what the
        // transpose of the cofactor is ultimately reduced to. Note this holds as
        // the last position in the augmented translate (i.e., 3,3) is ad - bc too,
        // which divided by our determinant is 1, and the rest of the row is 0,
        // which gives us our standard [0, 0, 1] bottom row.
        let adjunct = Transformation {
            ax: a.dy,
            by: -a.by,
            cx: -a.cx,
            dy: a.ax,
            tx: a.by * a.ty - a.dy * a.tx,
            ty: a.cx * a.tx - a.ax * a.ty,
        };

        // Finally, just multiply by 1 / det.
        Ok(adjunct.mul_scalar(1.0 / det))
    }

    /// Apply a co-ordinate offset to the origin.
    pub fn translate(self, tx: f64, ty: f64) -> Transformation {
        let a = self;
        let mut b = IDENTITY;
        b.tx = tx;
        b.ty = ty;
        a.mul(b)
    }

    /// Scale by (`sx`, `sy`). When `sx` and `sy` are not equal, a stretching
    /// effect will be achieved.
    pub fn scale(self, sx: f64, sy: f64) -> Transformation {
        let a = self;
        let mut b = IDENTITY;
        b.ax = sx;
        b.dy = sy;
        a.mul(b)
    }

    /// Rotate around the origin by `angle` (in radians).
    pub fn rotate(self, angle: f64) -> Transformation {
        let a = self;
        let mut b = IDENTITY;
        let s = angle.sin();
        let c = angle.cos();
        b.ax = c;
        b.by = -s;
        b.cx = s;
        b.dy = c;
        a.mul(b)
    }

    /// Applies the transformation matrix to the supplied `x` and `y`, but
    /// ignores translation.
    pub fn user_to_device_distance(self, x: &mut f64, y: &mut f64) {
        let a = self;
        let in_x = *x;
        let in_y = *y;
        *x = a.ax * in_x + a.by * in_y;
        *y = a.cx * in_x + a.dy * in_y;
    }

    /// Applies the transformation matrix to the supplied `x` and `y`.
    pub fn user_to_device(self, x: &mut f64, y: &mut f64) {
        let a = self;
        a.user_to_device_distance(x, y);
        *x += a.tx;
        *y += a.ty;
    }

    /// Applies the inverse of the transformation matrix to the supplied `x` and
    /// `y`, but ignores translation.
    pub fn device_to_user_distance(self, x: &mut f64, y: &mut f64) -> Result<(), Error> {
        let a = self;
        a.inverse()?.user_to_device_distance(x, y);
        Ok(())
    }

    /// Applies the inverse of the transformation matrix to the supplied `x` and
    /// `y`.
    pub fn device_to_user(self, x: &mut f64, y: &mut f64) -> Result<(), Error> {
        let a = self;
        a.inverse()?.user_to_device(x, y);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// For testing only. Rounds each element in the matrix to 16 significant
    /// digits.
    fn roundf64(a: Transformation) -> Transformation {
        // FIXME: Notes on rounding issues (some apply to exactly this helper,
        // some don't)
        //
        // Some transformations and multiplications seem to be yielding some very
        // small fractional discrepancies (e.g., to the tune of e-15 or more) on
        // what you would otherwise expect as whole numbers, or in comparison to
        // say expected results expressed as fractional expressions in tests (the
        // latter situation is why the function in which this comment resides
        // exists). We are rounding some test results to work around this. This
        // could just be the nature of floating point, but I'd like to do another
        // pass after the initial implementation push to double-check, after which
        // we can fix and/or delete this message.
        Transformation {
            ax: roundf64_field(a.ax),
            by: roundf64_field(a.by),
            cx: roundf64_field(a.cx),
            dy: roundf64_field(a.dy),
            tx: roundf64_field(a.tx),
            ty: roundf64_field(a.ty),
        }
    }

    fn roundf64_field(a: f64) -> f64 {
        // https://stackoverflow.com/a/13094362 (CC BY-SA 3.0)
        if a == 0.0 {
            return 0.0;
        }
        let b = 10f64.powf(16.0 - (a.abs().log10()).ceil());
        (a * b).round() / b
    }

    #[test]
    fn equal() {
        let a = Transformation {
            ax: 1.0,
            by: 2.0,
            cx: 3.0,
            dy: 4.0,
            tx: 5.0,
            ty: 6.0,
        };
        let b = Transformation {
            ax: 7.0,
            by: 8.0,
            cx: 9.0,
            dy: 10.0,
            tx: 11.0,
            ty: 12.0,
        };
        assert!(a.equal(a));
        assert!(!a.equal(b));
    }

    #[test]
    fn determinant() {
        let a = Transformation {
            ax: 1.0,
            by: 2.0,
            cx: 3.0,
            dy: 4.0,
            tx: 5.0,
            ty: 6.0,
        };
        assert_eq!(-2.0, a.determinant());
    }

    #[test]
    fn mul() {
        let a = Transformation {
            ax: 1.0,
            by: 2.0,
            cx: 3.0,
            dy: 4.0,
            tx: 5.0,
            ty: 6.0,
        };
        let b = Transformation {
            ax: 7.0,
            by: 8.0,
            cx: 9.0,
            dy: 10.0,
            tx: 11.0,
            ty: 12.0,
        };
        let expected = Transformation {
            ax: 25.0,
            by: 28.0,
            cx: 57.0,
            dy: 64.0,
            tx: 40.0,
            ty: 87.0,
        };
        assert_eq!(expected, a.mul(b));
    }

    #[test]
    fn mul_scalar() {
        let a = Transformation {
            ax: 1.0,
            by: 2.0,
            cx: 3.0,
            dy: 4.0,
            tx: 5.0,
            ty: 6.0,
        };
        let expected = Transformation {
            ax: 3.0,
            by: 6.0,
            cx: 9.0,
            dy: 12.0,
            tx: 15.0,
            ty: 18.0,
        };
        assert_eq!(expected, a.mul_scalar(3.0));
    }

    #[cfg_attr(miri, ignore)]
    // Miri deliberately randomizes sin/cos/log/powf within 1 ULP (float non-determinism, not UB); exact-equality is verified natively.
    #[test]
    fn inverse() {
        {
            // Special case (scale + translate)
            let a = Transformation {
                ax: 2.0,
                by: 0.0,
                cx: 0.0,
                dy: 3.0,
                tx: 10.0,
                ty: 15.0,
            };
            let expected = Transformation {
                ax: 0.5,
                by: 0.0,
                cx: 0.0,
                dy: 1.0 / 3.0,
                tx: -5.0,
                ty: -5.0,
            };
            assert_eq!(Ok(expected), a.inverse());
        }
        {
            // Special case (translate only)
            let a = Transformation {
                ax: 1.0,
                by: 0.0,
                cx: 0.0,
                dy: 1.0,
                tx: 10.0,
                ty: 15.0,
            };
            let expected = Transformation {
                ax: 1.0,
                by: 0.0,
                cx: 0.0,
                dy: 1.0,
                tx: -10.0,
                ty: -15.0,
            };
            assert_eq!(Ok(expected), a.inverse());
        }
        {
            // Full case
            let a = Transformation {
                ax: 2.0,
                by: 7.0,
                cx: 5.0,
                dy: 3.0,
                tx: 10.0,
                ty: 15.0,
            };
            let expected = Transformation {
                ax: -3.0 / 29.0,
                by: 7.0 / 29.0,
                cx: 5.0 / 29.0,
                dy: -2.0 / 29.0,
                tx: -75.0 / 29.0,
                ty: -20.0 / 29.0,
            };
            // FIXME: see roundf64 comments for details on why we need to round
            // test results.
            assert_eq!(roundf64(expected), roundf64(a.inverse().unwrap()));
        }
        {
            // Invalid special-case matrix
            assert_eq!(
                Err(Error::InvalidMatrix),
                Transformation {
                    ax: 0.0,
                    by: 0.0,
                    cx: 0.0,
                    dy: 0.0,
                    tx: 0.0,
                    ty: 0.0
                }
                .inverse()
            );
        }
        {
            // Invalid standard-case matrix
            assert_eq!(
                Err(Error::InvalidMatrix),
                Transformation {
                    ax: 3.0,
                    by: 3.0,
                    cx: 3.0,
                    dy: 3.0,
                    tx: 0.0,
                    ty: 0.0
                }
                .inverse()
            );
        }
    }

    #[test]
    fn translate() {
        assert_eq!(
            Transformation {
                ax: 1.0,
                by: 0.0,
                cx: 0.0,
                dy: 1.0,
                tx: 10.0,
                ty: 15.0
            },
            IDENTITY.translate(10.0, 15.0)
        );
    }

    #[test]
    fn scale() {
        assert_eq!(
            Transformation {
                ax: 2.0,
                by: 0.0,
                cx: 0.0,
                dy: 3.0,
                tx: 0.0,
                ty: 0.0
            },
            IDENTITY.scale(2.0, 3.0)
        );
    }

    #[cfg_attr(miri, ignore)]
    // Miri deliberately randomizes sin/cos/log/powf within 1 ULP (float non-determinism, not UB); exact-equality is verified natively.
    #[test]
    fn rotate() {
        let angle = std::f64::consts::PI / 2.0;
        let s = angle.sin();
        let c = angle.cos();
        assert_eq!(
            Transformation {
                ax: c,
                by: -s,
                cx: s,
                dy: c,
                tx: 0.0,
                ty: 0.0
            },
            IDENTITY.rotate(angle)
        );
    }

    #[cfg_attr(miri, ignore)]
    // Miri deliberately randomizes sin/cos/log/powf within 1 ULP (float non-determinism, not UB); exact-equality is verified natively.
    #[test]
    fn user_to_device() {
        // These tests are much less superficial than the basic
        // translate/rotate/scale tests above as we operate on real
        // co-ordinates.
        {
            // Basic tests
            let orig_x: f64 = 2.0;
            let orig_y: f64 = 3.0;

            // Identity
            let mut got_x = orig_x;
            let mut got_y = orig_y;
            IDENTITY.user_to_device(&mut got_x, &mut got_y);
            assert_eq!(2.0, got_x);
            assert_eq!(3.0, got_y);
            // Translate
            got_x = orig_x;
            got_y = orig_y;
            IDENTITY
                .translate(10.0, 12.0)
                .user_to_device(&mut got_x, &mut got_y);
            assert_eq!(12.0, got_x);
            assert_eq!(15.0, got_y);
            // Scale
            got_x = orig_x;
            got_y = orig_y;
            IDENTITY
                .scale(10.0, 20.0)
                .user_to_device(&mut got_x, &mut got_y);
            assert_eq!(20.0, got_x);
            assert_eq!(60.0, got_y);
        }
        {
            // Rotate
            let mut x: f64 = 9.0;
            let mut y: f64 = 0.0;
            let want_x: f64 = 0.0;
            let want_y: f64 = 9.0;
            IDENTITY
                .rotate(std::f64::consts::PI / 2.0)
                .user_to_device(&mut x, &mut y);
            // FIXME: see roundf64 comments for details on why we need to round
            // test results.
            x = x.round();
            y = y.round();
            assert_eq!(want_x, x);
            assert_eq!(want_y, y);
        }
        {
            // Combined
            let mut x: f64 = 9.0;
            let mut y: f64 = 0.0;
            let want_x: f64 = -200.0;
            let want_y: f64 = 190.0;
            IDENTITY
                .rotate(std::f64::consts::PI / 2.0)
                .scale(10.0, 10.0)
                .translate(10.0, 20.0)
                .user_to_device(&mut x, &mut y);
            assert_eq!(want_x, x);
            assert_eq!(want_y, y);
        }
    }

    #[cfg_attr(miri, ignore)]
    // Miri deliberately randomizes sin/cos/log/powf within 1 ULP (float non-determinism, not UB); exact-equality is verified natively.
    #[test]
    fn device_to_user() {
        let original_x: f64 = 9.0;
        let original_y: f64 = 0.0;
        let want_x: f64 = -200.0;
        let want_y: f64 = 190.0;
        let t = IDENTITY
            .rotate(std::f64::consts::PI / 2.0)
            .scale(10.0, 10.0)
            .translate(10.0, 20.0);
        let mut got_x = original_x;
        let mut got_y = original_y;
        t.user_to_device(&mut got_x, &mut got_y);
        assert_eq!(want_x, got_x);
        assert_eq!(want_y, got_y);
        t.device_to_user(&mut got_x, &mut got_y).unwrap();
        assert_eq!(original_x, got_x);
        assert_eq!(original_y, got_y);
    }

    #[cfg_attr(miri, ignore)]
    // Miri deliberately randomizes sin/cos/log/powf within 1 ULP (float non-determinism, not UB); exact-equality is verified natively.
    #[test]
    fn user_to_device_distance() {
        {
            // Basic tests
            let orig_x: f64 = 2.0;
            let orig_y: f64 = 3.0;

            // Identity
            let mut got_x = orig_x;
            let mut got_y = orig_y;
            IDENTITY.user_to_device_distance(&mut got_x, &mut got_y);
            assert_eq!(2.0, got_x);
            assert_eq!(3.0, got_y);
            // Translate
            got_x = orig_x;
            got_y = orig_y;
            IDENTITY
                .translate(10.0, 12.0)
                .user_to_device_distance(&mut got_x, &mut got_y);
            assert_eq!(2.0, got_x);
            assert_eq!(3.0, got_y);
            // Scale
            got_x = orig_x;
            got_y = orig_y;
            IDENTITY
                .scale(10.0, 20.0)
                .user_to_device_distance(&mut got_x, &mut got_y);
            assert_eq!(20.0, got_x);
            assert_eq!(60.0, got_y);
        }
        {
            // Rotate
            let mut x: f64 = 9.0;
            let mut y: f64 = 0.0;
            let want_x: f64 = 0.0;
            let want_y: f64 = 9.0;
            IDENTITY
                .rotate(std::f64::consts::PI / 2.0)
                .user_to_device_distance(&mut x, &mut y);
            // FIXME: see roundf64 comments for details on why we need to round
            // test results.
            x = x.round();
            y = y.round();
            assert_eq!(want_x, x);
            assert_eq!(want_y, y);
        }
        {
            // Combined
            let mut x: f64 = 9.0;
            let mut y: f64 = 2.0;
            let want_x: f64 = -20.0;
            let want_y: f64 = 90.0;
            IDENTITY
                .rotate(std::f64::consts::PI / 2.0)
                .scale(10.0, 10.0)
                .translate(10.0, 20.0)
                .user_to_device_distance(&mut x, &mut y);
            // FIXME: see roundf64 comments for details on why we need to round
            // test results.
            x = x.round();
            y = y.round();
            assert_eq!(want_x, x);
            assert_eq!(want_y, y);
        }
    }

    #[cfg_attr(miri, ignore)]
    // Miri deliberately randomizes sin/cos/log/powf within 1 ULP (float non-determinism, not UB); exact-equality is verified natively.
    #[test]
    fn device_to_user_distance() {
        let original_x: f64 = 9.0;
        let original_y: f64 = 2.0;
        let want_x: f64 = -20.0;
        let want_y: f64 = 90.0;
        let t = IDENTITY
            .rotate(std::f64::consts::PI / 2.0)
            .scale(10.0, 10.0)
            .translate(10.0, 20.0);
        let mut got_x = original_x;
        let mut got_y = original_y;
        t.user_to_device_distance(&mut got_x, &mut got_y);
        // FIXME: see roundf64 comments for details on why we need to round
        // test results.
        got_x = got_x.round();
        got_y = got_y.round();
        assert_eq!(want_x, got_x);
        assert_eq!(want_y, got_y);
        t.device_to_user_distance(&mut got_x, &mut got_y).unwrap();
        got_x = got_x.round();
        got_y = got_y.round();
        assert_eq!(original_x, got_x);
        assert_eq!(original_y, got_y);
    }
}
