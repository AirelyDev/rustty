//! Software IEEE 754 half-precision (`f16`) arithmetic.
//!
//! PORT/GLUE: The Zig reference (`split_tree.zig`) uses the native `f16` type
//! for split ratios and spatial layout math. Rust's `f16` is still an unstable
//! nightly-only primitive and the rustty toolchain is pinned to stable
//! (1.93.1), so we cannot use it directly in this Miri-able leaf crate. To
//! preserve byte-/rounding-identical behavior with the reference (the
//! split-tree ASCII diagram tests depend on exact `f16` rounding), this module
//! emulates `f16` deterministically: each value is stored as the `f32`
//! widening of an IEEE half, and every arithmetic operation rounds its result
//! back through the IEEE half-precision representation (round-to-nearest-even),
//! exactly matching hardware/software `f16` semantics. This adds nothing to the
//! reference logic (R1: no-loss, only glue) — it is the minimal mechanism
//! required to reproduce `f16` arithmetic on a stable toolchain.

/// A software IEEE 754 binary16 ("half") float. Stored as the exact `f32`
/// widening of a representable half value; arithmetic rounds back to half.
#[derive(Clone, Copy, PartialEq, PartialOrd, Default)]
pub struct F16(f32);

/// Round a finite-or-not `f32` to the nearest IEEE 754 half, returning the
/// `f32` widening of that half. Implements round-to-nearest, ties-to-even,
/// matching what `@as(f16, x)` produces in Zig. Based on the standard
/// f32->f16 bit-twiddling conversion.
fn round_to_half_f32(value: f32) -> f32 {
    half_to_f32(f32_to_half_bits(value))
}

/// Convert an `f32` to IEEE 754 half-precision bits (round-to-nearest-even).
fn f32_to_half_bits(value: f32) -> u16 {
    let x = value.to_bits();
    let sign = ((x >> 16) & 0x8000) as u16;
    // Exponent and mantissa of the f32.
    let mut mantissa = x & 0x007f_ffff;
    let exp = ((x >> 23) & 0xff) as i32;

    // NaN / Inf
    if exp == 0xff {
        if mantissa != 0 {
            // NaN: keep it a NaN (set a mantissa bit), preserve sign.
            return sign | 0x7e00;
        }
        // Infinity
        return sign | 0x7c00;
    }

    // Unbiased exponent for half = exp - 127 + 15.
    let mut new_exp = exp - 127 + 15;

    if new_exp >= 0x1f {
        // Overflow to infinity.
        return sign | 0x7c00;
    }

    if new_exp <= 0 {
        // Subnormal or zero in half.
        if 14 - new_exp > 24 {
            // Too small even for subnormal: rounds to zero.
            return sign;
        }
        // Add the implicit leading 1 to the mantissa.
        mantissa |= 0x0080_0000;
        let shift = (14 - new_exp) as u32;
        let half_mant = mantissa >> shift;
        // Round-to-nearest-even on the discarded bits.
        let remainder = mantissa & ((1u32 << shift) - 1);
        let halfway = 1u32 << (shift - 1);
        let mut result = half_mant as u16;
        if remainder > halfway || (remainder == halfway && (half_mant & 1) == 1) {
            result += 1;
        }
        return sign | result;
    }

    // Normal number. Round the 23-bit mantissa down to 10 bits.
    let half_mant = mantissa >> 13;
    let remainder = mantissa & 0x1fff;
    let halfway = 0x1000;
    let mut result = ((new_exp as u16) << 10) | (half_mant as u16);
    if remainder > halfway || (remainder == halfway && (half_mant & 1) == 1) {
        // Rounding may carry into the exponent; the bit pattern handles it
        // naturally because mantissa overflow increments the exponent field.
        result += 1;
        // If we overflowed to inf via rounding, that's the correct IEEE result.
        let _ = &mut new_exp;
    }
    sign | result
}

/// Convert IEEE 754 half-precision bits to `f32`.
fn half_to_f32(bits: u16) -> f32 {
    let sign = ((bits & 0x8000) as u32) << 16;
    let exp = ((bits >> 10) & 0x1f) as u32;
    let mantissa = (bits & 0x03ff) as u32;

    if exp == 0 {
        if mantissa == 0 {
            // Zero (signed).
            return f32::from_bits(sign);
        }
        // Subnormal: normalize.
        let mut m = mantissa;
        let mut e: i32 = -1;
        while (m & 0x0400) == 0 {
            m <<= 1;
            e -= 1;
        }
        m &= 0x03ff;
        let f32_exp = ((e + 1 - 15 + 127) as u32) << 23;
        let f32_mant = m << 13;
        return f32::from_bits(sign | f32_exp | f32_mant);
    }

    if exp == 0x1f {
        // Inf / NaN
        if mantissa == 0 {
            return f32::from_bits(sign | 0x7f80_0000);
        }
        return f32::from_bits(sign | 0x7fc0_0000);
    }

    // Normal.
    let f32_exp = (exp + 127 - 15) << 23;
    let f32_mant = mantissa << 13;
    f32::from_bits(sign | f32_exp | f32_mant)
}

impl F16 {
    /// Construct an `F16` from an `f32`, rounding to the nearest half.
    #[inline]
    pub fn from_f32(value: f32) -> Self {
        F16(round_to_half_f32(value))
    }

    /// Construct an `F16` from an integer (matches `@floatFromInt`).
    #[inline]
    pub fn from_int(value: i64) -> Self {
        F16::from_f32(value as f32)
    }

    /// Widen to `f32` (lossless; the stored value is already a representable
    /// half widened to `f32`).
    #[inline]
    pub fn to_f32(self) -> f32 {
        self.0
    }

    /// Truncate to integer (matches Zig `@intFromFloat`).
    #[inline]
    pub fn to_int_trunc(self) -> i64 {
        self.0 as i64
    }

    /// `@ceil`
    #[inline]
    pub fn ceil(self) -> Self {
        F16::from_f32(self.0.ceil())
    }

    /// `@floor`
    #[inline]
    pub fn floor(self) -> Self {
        F16::from_f32(self.0.floor())
    }

    /// `@sqrt`
    #[inline]
    pub fn sqrt(self) -> Self {
        F16::from_f32(self.0.sqrt())
    }

    /// `@min`
    #[inline]
    pub fn min(self, other: Self) -> Self {
        // Zig's @min: result is a representable half already.
        F16(self.0.min(other.0))
    }

    /// `@max`
    #[inline]
    pub fn max(self, other: Self) -> Self {
        F16(self.0.max(other.0))
    }
}

impl core::ops::Add for F16 {
    type Output = F16;
    #[inline]
    fn add(self, rhs: F16) -> F16 {
        F16::from_f32(self.0 + rhs.0)
    }
}

impl core::ops::Sub for F16 {
    type Output = F16;
    #[inline]
    fn sub(self, rhs: F16) -> F16 {
        F16::from_f32(self.0 - rhs.0)
    }
}

impl core::ops::Mul for F16 {
    type Output = F16;
    #[inline]
    fn mul(self, rhs: F16) -> F16 {
        F16::from_f32(self.0 * rhs.0)
    }
}

impl core::ops::Div for F16 {
    type Output = F16;
    #[inline]
    fn div(self, rhs: F16) -> F16 {
        F16::from_f32(self.0 / rhs.0)
    }
}

impl core::ops::Neg for F16 {
    type Output = F16;
    #[inline]
    fn neg(self) -> F16 {
        F16(-self.0)
    }
}

impl core::fmt::Debug for F16 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}
