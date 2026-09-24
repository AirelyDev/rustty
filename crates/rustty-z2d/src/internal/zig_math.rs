// SPDX-License-Identifier: MPL-2.0
//   Copyright © 2024-2025 Chris Marchesi
//   Copyright © 2025 rustty contributors (Rust port)

//! The Zig standard-library math z2d calls, ported so the port computes the
//! same bits as the reference.
//!
//! z2d normalizes slopes and measures lengths with Zig's `std.math.hypot`
//! (`internal/tess/Slope.zig:206`, `internal/arc.zig:261`,
//! `internal/path_fx/InputSet.zig:479`). The port called Rust's `f64::hypot`,
//! the platform libm's, which is a different algorithm and can differ in the
//! last bit: for `(30.678190110391604, 57.9476924307397)`, the slope of the
//! offset hypotenuse of the sprite glyph U+25FA at 9x17, Zig gives
//! `0x4050645096e433c0` and macOS libm `...c1`. That one bit moved a miter
//! join point and flipped one 4x4 multisample of one pixel, the
//! `U+2500...U+25FF-9x17+1` golden's one-pixel difference (S97-F1).

/// PORTS: Zig 0.16.0 `std.math.hypot` for `f64` (`lib/std/math/hypot.zig`):
/// `sqrt(x * x + y * y)`, avoiding unnecessary overflow and underflow.
///
/// | x | y | hypot |
/// |---|---|---|
/// | +-inf | any | +inf |
/// | any | +-inf | +inf |
/// | nan | fin | nan |
/// | fin | nan | nan |
pub fn hypot(x: f64, y: f64) -> f64 {
    // const lower = @sqrt(floatMin(T));
    let lower = f64::MIN_POSITIVE.sqrt();
    // const upper = @sqrt(floatMax(T) / 2);
    let upper = (f64::MAX / 2.0).sqrt();
    // const scale = floatTrueMin(T) * upper;
    let scale = f64::from_bits(1) * upper;
    // const hypfn = if (emulateFma(T)) hypotUnfused else hypotFused;
    // (`emulateFma` is false for f64.)
    let mut major = x;
    let mut minor = y;
    if major.is_infinite() || minor.is_infinite() {
        return f64::INFINITY;
    }
    if major.is_nan() || minor.is_nan() {
        return f64::NAN;
    }
    major = major.abs();
    minor = minor.abs();
    if minor > major {
        core::mem::swap(&mut major, &mut minor);
    }
    if minor == 0.0 {
        return major;
    }
    if major - minor == major {
        return major;
    }
    if major > upper {
        return hypot_fused(major * scale, minor * scale) / scale;
    }
    if minor < lower {
        return hypot_fused(major / scale, minor / scale) * scale;
    }
    hypot_fused(major, minor)
}

/// `hypotFused`: a correctly rounded `sqrt(x² + y²)` refined by one
/// Newton-style correction computed with fused multiply-adds (`@mulAdd` is
/// `f64::mul_add`, a single rounding).
fn hypot_fused(x: f64, y: f64) -> f64 {
    // const r = @sqrt(@mulAdd(F, x, x, y * y));
    let r = x.mul_add(x, y * y).sqrt();
    // const rr = r * r;
    let rr = r * r;
    // const xx = x * x;
    let xx = x * x;
    // const z = @mulAdd(F, -y, y, rr - xx) + @mulAdd(F, r, r, -rr) - @mulAdd(F, x, x, -xx);
    let z = (-y).mul_add(y, rr - xx) + r.mul_add(r, -rr) - x.mul_add(x, -xx);
    // return r - z / (2 * r);
    r - z / (2.0 * r)
}

#[cfg(test)]
// The expected values are Zig's `hypot_test_cases`, verbatim.
#[allow(clippy::excessive_precision)]
mod tests {
    use super::hypot;

    /// `std.math.approxEqRel`.
    fn approx_eq_rel(x: f64, y: f64, tolerance: f64) -> bool {
        if x == y {
            return true;
        }
        if x.is_nan() || y.is_nan() {
            return false;
        }
        (x - y).abs() <= x.abs().max(y.abs()) * tolerance
    }

    // const hypot_test_cases = .{ ... };
    const HYPOT_TEST_CASES: [(f64, f64, f64); 7] = [
        (0.0, -1.2, 1.2),
        (
            0.2,
            -0.34,
            0.3944616584663203993612799816649560759946493601889826495362,
        ),
        (
            0.8923,
            2.636890,
            2.7837722899152509525110650481670176852603253522923737962880,
        ),
        (
            1.5,
            5.25,
            5.4600824169603887033229768686452745953332522619323580787836,
        ),
        (
            37.45,
            159.835,
            164.16372840856167640478217141034363907565754072954443805164,
        ),
        (
            89.123,
            382.028905,
            392.28687638576315875933966414927490685367196874260165618371,
        ),
        (
            123123.234375,
            529428.707813,
            543556.88524707706887251269205923830745438413088753096759371,
        ),
    ];

    // test hypot {
    #[test]
    fn test_hypot() {
        assert!(hypot(0.3, 0.4) == 0.5);
    }

    // test "hypot.correct" (the f64 instantiation; f16/f32/f128 are not ported)
    #[test]
    fn hypot_correct() {
        for (a, b, c) in HYPOT_TEST_CASES {
            assert!(approx_eq_rel(hypot(a, b), c, f64::EPSILON.sqrt()));
        }
    }

    // test "hypot.precise" (the f64 instantiation)
    #[test]
    fn hypot_precise() {
        for (a, b, c) in HYPOT_TEST_CASES {
            assert!(approx_eq_rel(hypot(a, b), c, f64::EPSILON));
        }
    }

    // test "hypot.special" (the f64 instantiation)
    #[test]
    fn hypot_special() {
        let inf = f64::INFINITY;
        let nan = f64::NAN;
        assert!(hypot(nan, 0.0).is_nan());
        assert!(hypot(0.0, nan).is_nan());

        assert!(hypot(inf, 0.0) == inf);
        assert!(hypot(0.0, inf) == inf);
        assert!(hypot(inf, nan) == inf);
        assert!(hypot(nan, inf) == inf);

        assert!(hypot(-inf, 0.0) == inf);
        assert!(hypot(0.0, -inf) == inf);
        assert!(hypot(-inf, nan) == inf);
        assert!(hypot(nan, -inf) == inf);
    }

    /// S97-F1: bit-for-bit Zig's result where the platform libm differs
    /// (measured: Zig 0.16.0 `std.math.hypot` gives `0x4050645096e433c0`,
    /// macOS libm `f64::hypot` gives `0x4050645096e433c1`).
    #[test]
    fn hypot_matches_zig_where_libm_does_not() {
        let dx = f64::from_bits(0x403ead9dddf89a42);
        let dy = f64::from_bits(0x404cf94dfc4e58cd);
        assert_eq!(hypot(dx, dy).to_bits(), 0x4050645096e433c0);
        assert_eq!(hypot(-dx, -dy).to_bits(), 0x4050645096e433c0);
    }
}
