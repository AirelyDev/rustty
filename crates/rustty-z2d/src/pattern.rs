// SPDX-License-Identifier: MPL-2.0
//   Copyright © 2024-2025 Chris Marchesi
//
// Ported to Rust for rustty from z2d 0.10.0 `src/pattern.zig` (R3 vendored dep).
//
// SUBSET NOTE (rustty): z2d's `Pattern` union additionally carries `.gradient`
// (backed by `gradient.zig`) and `.dither` (backed by `Dither.zig`). Ghostty's
// sprite/glyf rasterizers only ever construct `.opaque_pattern` sources, so
// only that variant is part of this subset.

//! Pattern types and interfaces.

use crate::pixel::Pixel;

/// A simple opaque color pattern that writes the set color to every pixel.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OpaquePattern {
    /// The underlying pixel for this pattern.
    pub pixel: Pixel,
}

/// Represents sources of pixel data. In this subset, only the "opaque"
/// single-pixel pattern is supported (see the module note).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Pattern {
    OpaquePattern(OpaquePattern),
}

impl Pattern {
    /// Gets the pixel data at the co-ordinates specified.
    pub fn get_pixel(self, _x: i32, _y: i32) -> Pixel {
        match self {
            Pattern::OpaquePattern(s) => s.pixel,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pixel::Rgb;

    #[test]
    fn opaque_pattern_as_interface() {
        let px: Pixel = Pixel::Rgb(Rgb {
            r: 0xAA,
            g: 0xBB,
            b: 0xCC,
        });
        let pt: Pattern = Pattern::OpaquePattern(OpaquePattern { pixel: px });
        assert_eq!(px, pt.get_pixel(1, 1));
    }
}
