// SPDX-License-Identifier: MPL-2.0
//   Copyright © 2024-2025 Chris Marchesi
//
// Ported to Rust for rustty from z2d 0.10.0 `src/compositor.zig` (R3 vendored dep).
//
// SUBSET NOTE (rustty): z2d's compositor is a large SIMD-vectorized pixel
// pipeline (`StrideCompositor` / `SurfaceCompositor` / `runPixelT`, backed by
// `color.zig` + `color_vector.zig`). Ghostty's sprite/glyf FILL path only ever
// uses the fast-path where the operator reduces to a direct source write (see
// `internal/raster/shared.rs`), so the full compositor is NOT part of this
// subset; only the `Operator` / `Precision` descriptors and their `requires_float`
// / `is_bounded` predicates (consumed by the rasterizers) are ported here.

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
