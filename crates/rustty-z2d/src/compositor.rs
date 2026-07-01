// SPDX-License-Identifier: MPL-2.0
//   Copyright © 2024-2025 Chris Marchesi
//
// Ported to Rust for rustty from z2d 0.10.0 `src/compositor.zig` (R3 vendored dep).
//
// SUBSET NOTE (rustty): z2d's compositor is a large SIMD-vectorized pixel
// pipeline (`StrideCompositor` / `SurfaceCompositor` / `runPixelT`, backed by
// `color.zig` + `color_vector.zig`). Ghostty's sprite/glyf FILL path composites
// alpha8 either through the fast-path where the operator reduces to a direct
// source write (see `internal/raster/shared.rs`) OR — for the anti-aliased
// (`.default`/`.multisample_4x`) rasterizer — through the scalar single-pixel
// integer pipeline `runPixelT` (used by `Surface.composite_stride`). This crate
// ports the `Operator` / `Precision` descriptors, and the scalar `RGBA16`
// integer operator core (`run_pixel_t` + `IntegerOps`) that the multisample
// rasterizer needs to be pixel-exact. The SIMD `StrideCompositor` /
// `RGBA16Vec` / `RGBAFloat` vectorized paths are NOT part of this subset.

use crate::pixel::{Alpha8, Pixel, Rgba};

/// List of the supported compositor operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Operator {
    /// Ignores source and destination, returning a completely blank output.
    Clear,
    /// Ignores the destination and copies the source only.
    Src,
    /// Ignores source and leaves the destination (effectively a no-op).
    Dst,
    /// The source is composited over the destination. This is the default
    /// operator.
    #[default]
    SrcOver,
    /// The destination is composited over the source, replacing the destination.
    DstOver,
    /// The part of the source inside the destination replaces the destination.
    /// This operation is unbounded.
    SrcIn,
    /// The part of the destination inside the source replaces the destination.
    /// This operation is unbounded.
    DstIn,
    /// The part of the source outside the destination replaces the destination.
    /// This operation is unbounded.
    SrcOut,
    /// The part of the destination outside the source replaces the destination.
    DstOut,
    /// The part of the source inside the destination is composited onto the
    /// destination.
    SrcAtop,
    /// The part of the destination inside of the source is composited over the
    /// source; the destination is replaced with the result. Unbounded.
    DstAtop,
    /// The part of the source outside of the destination is combined with the
    /// part of the destination outside of the source.
    Xor,
    /// The source is added to the destination and replaces the destination.
    Plus,
    /// The source color is multiplied by the destination color.
    Multiply,
    /// Complements the source and destination colors.
    Screen,
    /// Multiplies or screens depending on the destination color.
    Overlay,
    /// Returns the darker of the source or destination colors.
    Darken,
    /// Returns the lighter of the source or destination colors.
    Lighten,
    /// The destination is brightened to reflect the source.
    ColorDodge,
    /// The destination is darkened to reflect the source.
    ColorBurn,
    /// Multiplies or screens depending on the destination color.
    HardLight,
    /// Darkens or lightens depending on the destination color.
    SoftLight,
    /// Returns the absolute difference between the source and destination.
    Difference,
    /// Similar to difference, with a lower contrast on the result.
    Exclusion,
    /// Combines the hue of the source with the saturation and luminosity of the
    /// destination.
    Hue,
    /// Combines the saturation of the source with the hue and luminosity of the
    /// destination.
    Saturation,
    /// Combines the hue and saturation of the source with the luminosity of the
    /// destination.
    Color,
    /// Combines the luminosity of the source with the hue and saturation of the
    /// destination.
    Luminosity,
}

impl Operator {
    /// Returns true if the operator requires floating-point precision.
    pub fn requires_float(self) -> bool {
        matches!(
            self,
            Operator::ColorDodge
                | Operator::ColorBurn
                | Operator::SoftLight
                | Operator::Hue
                | Operator::Saturation
                | Operator::Color
                | Operator::Luminosity
        )
    }

    /// Returns true if the operator is bounded, meaning that drawing is limited
    /// to the bounding box of the source polygon. This is honored during fill
    /// and stroke operations, but ignored when using the compositor directly.
    ///
    /// Most operators are bounded, or the result of the operator is defined as
    /// being equal for both bounded and unbounded sources, so this function
    /// rather lists operators that are explicitly unbounded.
    pub fn is_bounded(self) -> bool {
        !matches!(
            self,
            Operator::SrcIn | Operator::DstIn | Operator::SrcOut | Operator::DstAtop
        )
    }
}

/// List of the supported compositor precision types.
///
/// * `Integer` represents integer precision, using 16bpc unsigned-integer RGBA.
/// * `Float` represents floating-point precision, using 32bpc single-precision
///   floating point linear RGBA color.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Precision {
    #[default]
    Integer,
    Float,
}

/// Like `runPixel`, but operates on underlying pixel types directly.
///
/// This function currently only supports integer operators.
//
// PORT NOTE: the Zig `runPixelT` is `comptime`-generic over the concrete `dst_T`
// / `src_T` pixel structs and returns `dst_T` via `RGBA16.toPixelT`. In this
// subset every reachable call routes both operands through the `Pixel` union
// (whose `RGBA16.fromPixel` == `fromPixelT` for the alpha8/rgb(a) types used by
// the sprite/glyf fill), so this port takes `Pixel` operands and returns a
// `Pixel::Rgba`; the caller narrows it to its surface pixel via `from_pixel`,
// which is byte-for-byte equivalent to `toPixelT` for those formats.
pub fn run_pixel_t(dst: Pixel, src: Pixel, operator: Operator) -> Pixel {
    let mut d = Rgba16::from_pixel(dst);
    let s = Rgba16::from_pixel(src);
    d = d.run_operator(s, operator);
    d.to_pixel()
}

/// Represents an RGBA value as 16bpc. Note that this is only for intermediary
/// calculations, no channel should be bigger than an u8 after any particular
/// compositor step.
//
// PORT NOTE: the Zig struct uses `u16` fields; this port widens to `i32` to hold
// the transient wide products in the `overlay`/`hard_light` operators (Zig does
// the same via `widenType` -> `i32` for those). All final channel values remain
// in the 0..=255 range, matching Zig's `@intCast` back to `u8`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Rgba16 {
    r: i32,
    g: i32,
    b: i32,
    a: i32,
}

impl Rgba16 {
    fn from_pixel(src: Pixel) -> Rgba16 {
        let p = Rgba::from_pixel(src);
        Rgba16 {
            r: p.r as i32,
            g: p.g as i32,
            b: p.b as i32,
            a: p.a as i32,
        }
    }

    fn to_pixel(self) -> Pixel {
        Pixel::Rgba(Rgba {
            r: self.r as u8,
            g: self.g as u8,
            b: self.b as u8,
            a: self.a as u8,
        })
    }

    fn run_operator(self, src: Rgba16, op: Operator) -> Rgba16 {
        IntegerOps::run(op, self, src)
    }
}

/// Convenience so callers with a concrete `Alpha8` mask can build the source
/// pixel for `run_pixel_t` without importing `Pixel` shape details.
#[allow(dead_code)]
pub(crate) fn alpha8_pixel(a: u8) -> Pixel {
    Pixel::Alpha8(Alpha8 { a })
}

/// Integer-precision operator implementations.
//
// PORT NOTE: this is the scalar arm of z2d's `IntegerOps`. The `requires_float`
// operators (`color_dodge`/`color_burn`/`soft_light`/`hue`/`saturation`/
// `color`/`luminosity`) fall through to `clear` in the integer pipeline exactly
// as they do in the Zig original.
struct IntegerOps;

impl IntegerOps {
    fn run(op: Operator, dst: Rgba16, src: Rgba16) -> Rgba16 {
        match op {
            Operator::Clear => Self::clear(dst, src),
            Operator::Src => Self::src_op(dst, src),
            Operator::Dst => Self::dst_op(dst, src),
            Operator::SrcOver => Self::src_over(dst, src),
            Operator::DstOver => Self::dst_over(dst, src),
            Operator::SrcIn => Self::src_in(dst, src),
            Operator::DstIn => Self::dst_in(dst, src),
            Operator::SrcOut => Self::src_out(dst, src),
            Operator::DstOut => Self::dst_out(dst, src),
            Operator::SrcAtop => Self::src_atop(dst, src),
            Operator::DstAtop => Self::dst_atop(dst, src),
            Operator::Xor => Self::xor(dst, src),
            Operator::Plus => Self::plus(dst, src),
            Operator::Multiply => Self::multiply(dst, src),
            Operator::Screen => Self::screen(dst, src),
            Operator::Overlay => Self::overlay(dst, src),
            Operator::Darken => Self::darken(dst, src),
            Operator::Lighten => Self::lighten(dst, src),
            Operator::ColorDodge => Self::clear(dst, src),
            Operator::ColorBurn => Self::clear(dst, src),
            Operator::HardLight => Self::hard_light(dst, src),
            Operator::SoftLight => Self::clear(dst, src),
            Operator::Difference => Self::difference(dst, src),
            Operator::Exclusion => Self::exclusion(dst, src),
            Operator::Hue => Self::clear(dst, src),
            Operator::Saturation => Self::clear(dst, src),
            Operator::Color => Self::clear(dst, src),
            Operator::Luminosity => Self::clear(dst, src),
        }
    }

    fn clear(_dst: Rgba16, _src: Rgba16) -> Rgba16 {
        Rgba16 {
            r: 0,
            g: 0,
            b: 0,
            a: 0,
        }
    }

    fn src_op(_dst: Rgba16, src: Rgba16) -> Rgba16 {
        Rgba16 {
            r: src.r,
            g: src.g,
            b: src.b,
            a: src.a,
        }
    }

    fn dst_op(dst: Rgba16, _src: Rgba16) -> Rgba16 {
        Rgba16 {
            r: dst.r,
            g: dst.g,
            b: dst.b,
            a: dst.a,
        }
    }

    fn src_over(dst: Rgba16, src: Rgba16) -> Rgba16 {
        Rgba16 {
            r: src.r + inv_mul(dst.r, src.a),
            g: src.g + inv_mul(dst.g, src.a),
            b: src.b + inv_mul(dst.b, src.a),
            a: src.a + dst.a - mul(src.a, dst.a),
        }
    }

    fn dst_over(dst: Rgba16, src: Rgba16) -> Rgba16 {
        Rgba16 {
            r: dst.r + inv_mul(src.r, dst.a),
            g: dst.g + inv_mul(src.g, dst.a),
            b: dst.b + inv_mul(src.b, dst.a),
            a: src.a + dst.a - mul(src.a, dst.a),
        }
    }

    fn src_in(dst: Rgba16, src: Rgba16) -> Rgba16 {
        Rgba16 {
            r: mul(src.r, dst.a),
            g: mul(src.g, dst.a),
            b: mul(src.b, dst.a),
            a: mul(src.a, dst.a),
        }
    }

    fn dst_in(dst: Rgba16, src: Rgba16) -> Rgba16 {
        Rgba16 {
            r: mul(dst.r, src.a),
            g: mul(dst.g, src.a),
            b: mul(dst.b, src.a),
            a: mul(dst.a, src.a),
        }
    }

    fn src_out(dst: Rgba16, src: Rgba16) -> Rgba16 {
        Rgba16 {
            r: inv_mul(src.r, dst.a),
            g: inv_mul(src.g, dst.a),
            b: inv_mul(src.b, dst.a),
            a: inv_mul(src.a, dst.a),
        }
    }

    fn dst_out(dst: Rgba16, src: Rgba16) -> Rgba16 {
        Rgba16 {
            r: inv_mul(dst.r, src.a),
            g: inv_mul(dst.g, src.a),
            b: inv_mul(dst.b, src.a),
            a: inv_mul(dst.a, src.a),
        }
    }

    fn src_atop(dst: Rgba16, src: Rgba16) -> Rgba16 {
        Rgba16 {
            r: mul(src.r, dst.a) + inv_mul(dst.r, src.a),
            g: mul(src.g, dst.a) + inv_mul(dst.g, src.a),
            b: mul(src.b, dst.a) + inv_mul(dst.b, src.a),
            a: dst.a,
        }
    }

    fn dst_atop(dst: Rgba16, src: Rgba16) -> Rgba16 {
        Rgba16 {
            r: mul(dst.r, src.a) + inv_mul(src.r, dst.a),
            g: mul(dst.g, src.a) + inv_mul(src.g, dst.a),
            b: mul(dst.b, src.a) + inv_mul(src.b, dst.a),
            a: src.a,
        }
    }

    fn xor(dst: Rgba16, src: Rgba16) -> Rgba16 {
        Rgba16 {
            r: inv_mul(src.r, dst.a) + inv_mul(dst.r, src.a),
            g: inv_mul(src.g, dst.a) + inv_mul(dst.g, src.a),
            b: inv_mul(src.b, dst.a) + inv_mul(dst.b, src.a),
            a: inv_mul(src.a, dst.a) + inv_mul(dst.a, src.a),
        }
    }

    fn plus(dst: Rgba16, src: Rgba16) -> Rgba16 {
        Rgba16 {
            r: limit_u8(src.r + dst.r),
            g: limit_u8(src.g + dst.g),
            b: limit_u8(src.b + dst.b),
            a: limit_u8(src.a + dst.a),
        }
    }

    fn multiply(dst: Rgba16, src: Rgba16) -> Rgba16 {
        Rgba16 {
            r: mul(src.r, dst.r) + inv_mul(src.r, dst.a) + inv_mul(dst.r, src.a),
            g: mul(src.g, dst.g) + inv_mul(src.g, dst.a) + inv_mul(dst.g, src.a),
            b: mul(src.b, dst.b) + inv_mul(src.b, dst.a) + inv_mul(dst.b, src.a),
            a: src.a + dst.a - mul(src.a, dst.a),
        }
    }

    fn screen(dst: Rgba16, src: Rgba16) -> Rgba16 {
        Rgba16 {
            r: src.r + dst.r - mul(src.r, dst.r),
            g: src.g + dst.g - mul(src.g, dst.g),
            b: src.b + dst.b - mul(src.b, dst.b),
            a: src.a + dst.a - mul(src.a, dst.a),
        }
    }

    fn overlay(dst: Rgba16, src: Rgba16) -> Rgba16 {
        // sca=src, dca=dst per-channel; sa=src.a, da=dst.a
        fn p0(dca: i32, da: i32) -> bool {
            mul_scalar(dca, 2) <= da
        }
        fn a(sca: i32, dca: i32, sa: i32, da: i32) -> i32 {
            mul(mul_scalar(sca, 2), dca) + inv_mul(sca, da) + inv_mul(dca, sa)
        }
        fn b(sca: i32, dca: i32, sa: i32, da: i32) -> i32 {
            r_inv_mul(sca, da) + r_inv_mul(dca, sa) - mul(mul_scalar(dca, 2), sca) - mul(da, sa)
        }
        fn run_scalar(sca: i32, dca: i32, sa: i32, da: i32) -> i32 {
            if p0(dca, da) {
                a(sca, dca, sa, da)
            } else {
                b(sca, dca, sa, da)
            }
        }
        Rgba16 {
            r: run_scalar(src.r, dst.r, src.a, dst.a),
            g: run_scalar(src.g, dst.g, src.a, dst.a),
            b: run_scalar(src.b, dst.b, src.a, dst.a),
            a: src.a + dst.a - mul(src.a, dst.a),
        }
    }

    fn darken(dst: Rgba16, src: Rgba16) -> Rgba16 {
        Rgba16 {
            r: mul(src.r, dst.a).min(mul(dst.r, src.a))
                + inv_mul(src.r, dst.a)
                + inv_mul(dst.r, src.a),
            g: mul(src.g, dst.a).min(mul(dst.g, src.a))
                + inv_mul(src.g, dst.a)
                + inv_mul(dst.g, src.a),
            b: mul(src.b, dst.a).min(mul(dst.b, src.a))
                + inv_mul(src.b, dst.a)
                + inv_mul(dst.b, src.a),
            a: src.a + dst.a - mul(src.a, dst.a),
        }
    }

    fn lighten(dst: Rgba16, src: Rgba16) -> Rgba16 {
        Rgba16 {
            r: mul(src.r, dst.a).max(mul(dst.r, src.a))
                + inv_mul(src.r, dst.a)
                + inv_mul(dst.r, src.a),
            g: mul(src.g, dst.a).max(mul(dst.g, src.a))
                + inv_mul(src.g, dst.a)
                + inv_mul(dst.g, src.a),
            b: mul(src.b, dst.a).max(mul(dst.b, src.a))
                + inv_mul(src.b, dst.a)
                + inv_mul(dst.b, src.a),
            a: src.a + dst.a - mul(src.a, dst.a),
        }
    }

    fn hard_light(dst: Rgba16, src: Rgba16) -> Rgba16 {
        fn p0(sca: i32, sa: i32) -> bool {
            mul_scalar(sca, 2) <= sa
        }
        fn a(sca: i32, dca: i32, sa: i32, da: i32) -> i32 {
            mul(mul_scalar(sca, 2), dca) + inv_mul(sca, da) + inv_mul(dca, sa)
        }
        fn b(sca: i32, dca: i32, sa: i32, da: i32) -> i32 {
            r_inv_mul(sca, da) + r_inv_mul(dca, sa) - mul(sa, da) - mul(mul_scalar(sca, 2), dca)
        }
        fn run_scalar(sca: i32, dca: i32, sa: i32, da: i32) -> i32 {
            if p0(sca, sa) {
                a(sca, dca, sa, da)
            } else {
                b(sca, dca, sa, da)
            }
        }
        Rgba16 {
            r: run_scalar(src.r, dst.r, src.a, dst.a),
            g: run_scalar(src.g, dst.g, src.a, dst.a),
            b: run_scalar(src.b, dst.b, src.a, dst.a),
            a: src.a + dst.a - mul(src.a, dst.a),
        }
    }

    fn difference(dst: Rgba16, src: Rgba16) -> Rgba16 {
        Rgba16 {
            r: src.r + dst.r - mul_scalar(mul(src.r, dst.a).min(mul(dst.r, src.a)), 2),
            g: src.g + dst.g - mul_scalar(mul(src.g, dst.a).min(mul(dst.g, src.a)), 2),
            b: src.b + dst.b - mul_scalar(mul(src.b, dst.a).min(mul(dst.b, src.a)), 2),
            a: src.a + dst.a - mul(src.a, dst.a),
        }
    }

    fn exclusion(dst: Rgba16, src: Rgba16) -> Rgba16 {
        Rgba16 {
            r: (mul(src.r, dst.a) + mul(dst.r, src.a) - mul_scalar(mul(src.r, dst.r), 2))
                + inv_mul(src.r, dst.a)
                + inv_mul(dst.r, src.a),
            g: (mul(src.g, dst.a) + mul(dst.g, src.a) - mul_scalar(mul(src.g, dst.g), 2))
                + inv_mul(src.g, dst.a)
                + inv_mul(dst.g, src.a),
            b: (mul(src.b, dst.a) + mul(dst.b, src.a) - mul_scalar(mul(src.b, dst.b), 2))
                + inv_mul(src.b, dst.a)
                + inv_mul(dst.b, src.a),
            a: src.a + dst.a - mul(src.a, dst.a),
        }
    }
}

/// Max u8 value, used as the integer-fixed-point "one".
const MAX_U8: i32 = 255;

/// Clamps the value to the max u8 value.
fn limit_u8(x: i32) -> i32 {
    MAX_U8.min(x)
}

/// Integer-equivalent multiplication for colors, downscaling by the max u8
/// value after multiplication (equivalent to `a * b` for 0-1 normalized colors).
fn mul(a: i32, b: i32) -> i32 {
    (a * b) / MAX_U8
}

/// Integer-equivalent multiplication of `x` by the scalar value `y`.
fn mul_scalar(x: i32, y: i32) -> i32 {
    let mut z = 0;
    for _ in 0..y {
        z += x;
    }
    z
}

/// Integer-equivalent of `1 - a` for 0-1 normalized colors ("inverting").
fn inv(a: i32) -> i32 {
    MAX_U8 - a
}

/// Integer-equivalent of `1 + a` for 0-1 normalized colors ("reverse" inversion).
fn r_inv(a: i32) -> i32 {
    MAX_U8 + a
}

/// Integer-equivalent of `a * (1 - b)` for 0-1 normalized colors.
fn inv_mul(a: i32, b: i32) -> i32 {
    mul(a, inv(b))
}

/// Integer-equivalent of `a * (1 + b)` for 0-1 normalized colors.
fn r_inv_mul(a: i32, b: i32) -> i32 {
    mul(a, r_inv(b))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pixel::Rgb;

    #[test]
    fn src_over_alpha8_matches_reference() {
        // out = src + dst*(1-src.a) on the alpha channel: opaque black over
        // a mid-gray alpha8 destination.
        let out = run_pixel_t(
            Pixel::Alpha8(Alpha8 { a: 100 }),
            Pixel::Rgba(Rgba {
                r: 0,
                g: 0,
                b: 0,
                a: 200,
            }),
            Operator::SrcOver,
        );
        // a = 200 + 100 - (200*100/255) = 200 + 100 - 78 = 222
        assert_eq!(
            out,
            Pixel::Rgba(Rgba {
                r: 0,
                g: 0,
                b: 0,
                a: 222,
            })
        );
    }

    #[test]
    fn dst_in_premultiplies_opacity() {
        // dst_in of an opaque black source with an alpha8 mask yields a source
        // whose alpha is the mask alpha (mul(255, opacity)).
        let out = run_pixel_t(
            Pixel::Rgba(Rgba {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            }),
            Pixel::Alpha8(Alpha8 { a: 128 }),
            Operator::DstIn,
        );
        assert_eq!(
            out,
            Pixel::Rgba(Rgba {
                r: 0,
                g: 0,
                b: 0,
                a: 128,
            })
        );
    }

    #[test]
    fn src_opaque_rgb_replaces() {
        let out = run_pixel_t(
            Pixel::Rgb(Rgb { r: 1, g: 2, b: 3 }),
            Pixel::Rgba(Rgba {
                r: 10,
                g: 20,
                b: 30,
                a: 255,
            }),
            Operator::Src,
        );
        assert_eq!(
            out,
            Pixel::Rgba(Rgba {
                r: 10,
                g: 20,
                b: 30,
                a: 255,
            })
        );
    }
}
