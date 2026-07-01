// SPDX-License-Identifier: MPL-2.0
//   Copyright © 2024-2025 Chris Marchesi
//
// Ported to Rust for rustty from z2d 0.10.0 `src/internal/Point.zig` (R3 vendored dep).

//! Represents a point in 2D space.

use crate::transformation::{self, Transformation};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    /// Checks to see if a point is equal to another point.
    pub fn equal(self, other: Point) -> bool {
        self.x == other.x && self.y == other.y
    }

    /// Apply the supplied transformation matrix to a point.
    pub fn apply_transform(self, t: Transformation) -> Point {
        let mut x = self.x;
        let mut y = self.y;
        t.user_to_device(&mut x, &mut y);
        Point { x, y }
    }

    /// Apply the supplied transformation matrix to a point, ignoring translate.
    pub fn apply_transform_distance(self, t: Transformation) -> Point {
        let mut x = self.x;
        let mut y = self.y;
        t.user_to_device_distance(&mut x, &mut y);
        Point { x, y }
    }

    /// Apply the inverse of the supplied transformation matrix to a point.
    pub fn apply_inverse_transform(
        self,
        t: Transformation,
    ) -> Result<Point, transformation::Error> {
        let mut x = self.x;
        let mut y = self.y;
        t.device_to_user(&mut x, &mut y)?;
        Ok(Point { x, y })
    }

    /// Apply the inverse of the supplied transformation matrix to a point,
    /// ignoring translate.
    pub fn apply_inverse_transform_distance(
        self,
        t: Transformation,
    ) -> Result<Point, transformation::Error> {
        let mut x = self.x;
        let mut y = self.y;
        t.device_to_user_distance(&mut x, &mut y)?;
        Ok(Point { x, y })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transformation::IDENTITY;
    use std::f64::consts::PI;

    #[cfg_attr(miri, ignore)]
    // Miri deliberately randomizes sin/cos within 1 ULP (float non-determinism, not UB); verified natively.
    #[test]
    fn apply_transform() {
        {
            // Basic tests
            let p = Point { x: 2.0, y: 3.0 };

            // Identity
            assert_eq!(Point { x: 2.0, y: 3.0 }, p.apply_transform(IDENTITY));
            // Translate
            assert_eq!(
                Point { x: 12.0, y: 15.0 },
                p.apply_transform(IDENTITY.translate(10.0, 12.0))
            );
            // Scale
            assert_eq!(
                Point { x: 20.0, y: 60.0 },
                p.apply_transform(IDENTITY.scale(10.0, 20.0))
            );
        }
        {
            // Rotate
            let want = Point { x: 0.0, y: 9.0 };
            let mut got = (Point { x: 9.0, y: 0.0 }).apply_transform(IDENTITY.rotate(PI / 2.0));
            // FIXME: See some of the comments in Transformation.zig for why we
            // need to round (currently).
            got.x = got.x.round();
            got.y = got.y.round();
            assert_eq!(want, got);
        }
        {
            // Combined
            let want = Point {
                x: -200.0,
                y: 190.0,
            };
            let t = IDENTITY
                .rotate(PI / 2.0)
                .scale(10.0, 10.0)
                .translate(10.0, 20.0);
            let got = (Point { x: 9.0, y: 0.0 }).apply_transform(t);
            assert_eq!(want, got);
        }
    }

    #[cfg_attr(miri, ignore)]
    // Miri deliberately randomizes sin/cos within 1 ULP (float non-determinism, not UB); verified natively.
    #[test]
    fn apply_transform_distance() {
        {
            // Basic tests
            let p = Point { x: 2.0, y: 3.0 };

            // Identity
            assert_eq!(
                Point { x: 2.0, y: 3.0 },
                p.apply_transform_distance(IDENTITY)
            );
            // Translate
            assert_eq!(
                Point { x: 2.0, y: 3.0 },
                p.apply_transform_distance(IDENTITY.translate(10.0, 12.0))
            );
            // Scale
            assert_eq!(
                Point { x: 20.0, y: 60.0 },
                p.apply_transform_distance(IDENTITY.scale(10.0, 20.0))
            );
        }
        {
            // Rotate
            let want = Point { x: 0.0, y: 9.0 };
            let mut got =
                (Point { x: 9.0, y: 0.0 }).apply_transform_distance(IDENTITY.rotate(PI / 2.0));
            // FIXME: See some of the comments in Transformation.zig for why we
            // need to round (currently).
            got.x = got.x.round();
            got.y = got.y.round();
            assert_eq!(want, got);
        }
        {
            // Combined
            let want = Point { x: -20.0, y: 90.0 };
            let t = IDENTITY
                .rotate(PI / 2.0)
                .scale(10.0, 10.0)
                .translate(10.0, 20.0);
            let mut got = (Point { x: 9.0, y: 2.0 }).apply_transform_distance(t);
            // FIXME: See some of the comments in Transformation.zig for why we
            // need to round (currently).
            got.x = got.x.round();
            got.y = got.y.round();
            assert_eq!(want, got);
        }
    }

    #[cfg_attr(miri, ignore)]
    // Miri deliberately randomizes sin/cos within 1 ULP (float non-determinism, not UB); verified natively.
    #[test]
    fn apply_inverse_transform() {
        let original = Point { x: 9.0, y: 0.0 };
        let want = Point {
            x: -200.0,
            y: 190.0,
        };
        let t = IDENTITY
            .rotate(PI / 2.0)
            .scale(10.0, 10.0)
            .translate(10.0, 20.0);
        let mut got = original.apply_transform(t);
        assert_eq!(want, got);
        got = got.apply_inverse_transform(t).unwrap();
        assert_eq!(original, got);
    }

    #[cfg_attr(miri, ignore)]
    // Miri deliberately randomizes sin/cos within 1 ULP (float non-determinism, not UB); verified natively.
    #[test]
    fn apply_inverse_transform_distance() {
        let original = Point { x: 9.0, y: 2.0 };
        let want = Point { x: -20.0, y: 90.0 };
        let t = IDENTITY
            .rotate(PI / 2.0)
            .scale(10.0, 10.0)
            .translate(10.0, 20.0);
        let mut got = original.apply_transform_distance(t);
        // FIXME: See some of the comments in Transformation.zig for why we
        // need to round (currently).
        got.x = got.x.round();
        got.y = got.y.round();
        assert_eq!(want, got);
        got = got.apply_inverse_transform_distance(t).unwrap();
        got.x = got.x.round();
        got.y = got.y.round();
        assert_eq!(original, got);
    }
}
