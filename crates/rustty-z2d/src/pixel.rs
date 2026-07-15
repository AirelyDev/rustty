// SPDX-License-Identifier: MPL-2.0
//   Copyright © 2024-2025 Chris Marchesi
//
// Ported to Rust for rustty from z2d 0.10.0 `src/pixel.zig` (R3 vendored dep).
//
// SUBSET NOTE (rustty): the ghostty sprite/glyf rasterizers only ever operate
// on `.alpha8` (and, for scratch surfaces, `.rgb`/`.rgba`) surfaces. This port
// carries the full `Format`/`Pixel` shape for parity, implements the RGB(A)
// family and `Alpha8` faithfully, and provides thin `Alpha4`/`Alpha2`/`Alpha1`
// values so the union is complete. The sub-8-bit *packed surfaces* and the
// `color`-module-backed `fromColor` verb are NOT part of this subset (see the
// crate-level blockers doc).

//! Pixel types represented by the library.

/// Format descriptors for the pixel formats supported by the library:
///
/// ### Little-endian ARGB/XRGB formats
///
/// The following formats are BGRA-ordered as a `[4]u8`. As little endian, this
/// will look like ARGB.
///
/// * `.argb` is 24-bit truecolor as an 8-bit depth RGB, *with* alpha channel.
/// * `.xrgb` is 24-bit truecolor as an 8-bit depth RGB, *without* alpha channel.
///
/// ### RGBA-array formats
///
/// The following formats are RGBA-ordered as a `[4]u8`. As little endian, this
/// will look like ABGR.
///
/// * `.rgba` is 24-bit truecolor as an 8-bit depth RGB, *with* alpha channel.
/// * `.rgb` is 24-bit truecolor as an 8-bit depth RGB, *without* alpha channel.
///
/// ### Alpha-only channels
///
/// All alpha channels below 8 bits will be packed into memory when used as the
/// pixel type of a `Surface`.
///
/// * `.alpha8` is an 8-bit alpha channel.
/// * `.alpha4` is an 4-bit alpha channel.
/// * `.alpha2` is an 2-bit alpha channel.
/// * `.alpha1` is a 1-bit alpha channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Argb,
    Xrgb,
    Rgb,
    Rgba,
    Alpha8,
    Alpha4,
    Alpha2,
    Alpha1,
}

/// Describes a 32-bit little-endian xRGB format (can also be thought of as
/// ARGB with a disabled alpha channel).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Xrgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

/// Describes a 32-bit RGBx format (or xBGR when described as little endian);
/// can also be thought of as RGBA with disabled alpha channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

/// Describes a 32-bit little-endian ARGB format.
///
/// Note that all compositing operations in z2d expect a pre-multiplied alpha.
/// You can convert between pre-multiplied and straight alpha using `multiply`
/// and `demultiply`. Additionally, `from_clamped` takes straight alpha.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Argb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

/// Describes a 32-bit RGBA format (or ABGR when described a little-endian).
///
/// Note that all compositing operations in z2d expect a pre-multiplied alpha.
/// You can convert between pre-multiplied and straight alpha using `multiply`
/// and `demultiply`. Additionally, `from_clamped` takes straight alpha.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

/// Describes an 8-bit alpha channel-only format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Alpha8 {
    pub a: u8,
}

/// Describes a 4-bit alpha channel-only format (value stored in the low nibble).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Alpha4 {
    pub a: u8,
}

/// Describes a 2-bit alpha channel-only format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Alpha2 {
    pub a: u8,
}

/// Describes a 1-bit alpha channel-only format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Alpha1 {
    pub a: u8,
}

/// Represents an interface as a union of the pixel formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pixel {
    Argb(Argb),
    Xrgb(Xrgb),
    Rgb(Rgb),
    Rgba(Rgba),
    Alpha8(Alpha8),
    Alpha4(Alpha4),
    Alpha2(Alpha2),
    Alpha1(Alpha1),
}

impl Pixel {
    /// Returns `true` if the supplied pixels are equal.
    pub fn equal(self, other: Pixel) -> bool {
        match self {
            Pixel::Argb(px) => px.equal(other),
            Pixel::Xrgb(px) => px.equal(other),
            Pixel::Rgb(px) => px.equal(other),
            Pixel::Rgba(px) => px.equal(other),
            Pixel::Alpha8(px) => px.equal(other),
            Pixel::Alpha4(px) => px.equal(other),
            Pixel::Alpha2(px) => px.equal(other),
            Pixel::Alpha1(px) => px.equal(other),
        }
    }

    /// Returns the `Format` descriptor of this pixel.
    pub fn format(self) -> Format {
        match self {
            Pixel::Argb(_) => Format::Argb,
            Pixel::Xrgb(_) => Format::Xrgb,
            Pixel::Rgb(_) => Format::Rgb,
            Pixel::Rgba(_) => Format::Rgba,
            Pixel::Alpha8(_) => Format::Alpha8,
            Pixel::Alpha4(_) => Format::Alpha4,
            Pixel::Alpha2(_) => Format::Alpha2,
            Pixel::Alpha1(_) => Format::Alpha1,
        }
    }

    /// Convenience method that returns true if the pixel is full-opacity.
    ///
    /// Note that:
    ///
    ///  * Any pixel type lacking an alpha channel (e.g., `RGB`) is always
    ///    considered opaque.
    ///  * Alpha types (e.g., `Alpha8` or the sub-8-bit versions) follow the
    ///    same opacity rules as RGBA types (full opacity is max alpha channel
    ///    value). Keep this in mind when using alpha-only pixel types to
    ///    represent grayscale.
    pub fn is_opaque(self) -> bool {
        match self {
            Pixel::Xrgb(_) | Pixel::Rgb(_) => true,
            Pixel::Argb(px) => px.a == 255,
            Pixel::Rgba(px) => px.a == 255,
            Pixel::Alpha8(px) => px.a == 255,
            Pixel::Alpha4(px) => px.a == 15,
            Pixel::Alpha2(px) => px.a == 3,
            Pixel::Alpha1(px) => px.a == 1,
        }
    }
}

impl Xrgb {
    pub const FORMAT: Format = Format::Xrgb;

    /// Returns a pixel from a clamped 0-1 value.
    pub fn from_clamped(r: f64, g: f64, b: f64) -> Xrgb {
        let rgba = Rgba::from_clamped(r, g, b, 1.0);
        Xrgb {
            r: rgba.r,
            g: rgba.g,
            b: rgba.b,
        }
    }

    /// Returns the pixel translated to XRGB.
    pub fn from_pixel(p: Pixel) -> Xrgb {
        let (r, g, b) = rgb_from_pixel(p);
        Xrgb { r, g, b }
    }

    /// Returns this pixel as an interface.
    pub fn as_pixel(self) -> Pixel {
        Pixel::Xrgb(self)
    }

    /// Returns `true` if the supplied pixels are equal.
    pub fn equal(self, other: Pixel) -> bool {
        matches!(other, Pixel::Xrgb(o) if self == o)
    }
}

impl Rgb {
    pub const FORMAT: Format = Format::Rgb;

    /// Returns a pixel from a clamped 0-1 value.
    pub fn from_clamped(r: f64, g: f64, b: f64) -> Rgb {
        let rgba = Rgba::from_clamped(r, g, b, 1.0);
        Rgb {
            r: rgba.r,
            g: rgba.g,
            b: rgba.b,
        }
    }

    /// Returns the pixel translated to RGB.
    pub fn from_pixel(p: Pixel) -> Rgb {
        let (r, g, b) = rgb_from_pixel(p);
        Rgb { r, g, b }
    }

    /// Returns this pixel as an interface.
    pub fn as_pixel(self) -> Pixel {
        Pixel::Rgb(self)
    }

    /// Returns `true` if the supplied pixels are equal.
    pub fn equal(self, other: Pixel) -> bool {
        matches!(other, Pixel::Rgb(o) if self == o)
    }
}

impl Argb {
    pub const FORMAT: Format = Format::Argb;

    /// Returns a pixel from a clamped 0-1 value. The helper expects the values
    /// as straight alpha and will pre-multiply the values for you.
    pub fn from_clamped(r: f64, g: f64, b: f64, a: f64) -> Argb {
        let rgba = Rgba::from_clamped(r, g, b, a);
        Argb {
            r: rgba.r,
            g: rgba.g,
            b: rgba.b,
            a: rgba.a,
        }
    }

    /// Returns the pixel translated to ARGB.
    pub fn from_pixel(p: Pixel) -> Argb {
        let (r, g, b, a) = rgba_from_pixel(p);
        Argb { r, g, b, a }
    }

    /// Returns this pixel as an interface.
    pub fn as_pixel(self) -> Pixel {
        Pixel::Argb(self)
    }

    /// Returns `true` if the supplied pixels are equal.
    pub fn equal(self, other: Pixel) -> bool {
        matches!(other, Pixel::Argb(o) if self == o)
    }

    /// Returns a new ARGB value, with the color values multiplied by the alpha
    /// (as alpha / 255).
    pub fn multiply(self) -> Argb {
        let r = Rgba {
            r: self.r,
            g: self.g,
            b: self.b,
            a: self.a,
        }
        .multiply();
        Argb {
            r: r.r,
            g: r.g,
            b: r.b,
            a: r.a,
        }
    }

    /// Returns a new ARGB value, with the color values de-multiplied by the
    /// alpha.
    pub fn demultiply(self) -> Argb {
        let r = Rgba {
            r: self.r,
            g: self.g,
            b: self.b,
            a: self.a,
        }
        .demultiply();
        Argb {
            r: r.r,
            g: r.g,
            b: r.b,
            a: r.a,
        }
    }
}

impl Rgba {
    pub const FORMAT: Format = Format::Rgba;

    /// Returns a pixel from a clamped 0-1 value. The helper expects the values
    /// as straight alpha and will pre-multiply the values for you.
    pub fn from_clamped(r: f64, g: f64, b: f64, a: f64) -> Rgba {
        let rc = r.clamp(0.0, 1.0);
        let gc = g.clamp(0.0, 1.0);
        let bc = b.clamp(0.0, 1.0);
        let ac = a.clamp(0.0, 1.0);
        let result = Rgba {
            r: (255.0 * rc).round() as u8,
            g: (255.0 * gc).round() as u8,
            b: (255.0 * bc).round() as u8,
            a: (255.0 * ac).round() as u8,
        };
        result.multiply()
    }

    /// Returns the pixel translated to RGBA.
    pub fn from_pixel(p: Pixel) -> Rgba {
        let (r, g, b, a) = rgba_from_pixel(p);
        Rgba { r, g, b, a }
    }

    /// Returns this pixel as an interface.
    pub fn as_pixel(self) -> Pixel {
        Pixel::Rgba(self)
    }

    /// Returns `true` if the supplied pixels are equal.
    pub fn equal(self, other: Pixel) -> bool {
        matches!(other, Pixel::Rgba(o) if self == o)
    }

    /// Returns a new RGBA value, with the color values multiplied by the alpha
    /// (as alpha / 255).
    pub fn multiply(self) -> Rgba {
        Rgba {
            r: (self.r as u16 * self.a as u16 / 255) as u8,
            g: (self.g as u16 * self.a as u16 / 255) as u8,
            b: (self.b as u16 * self.a as u16 / 255) as u8,
            a: self.a,
        }
    }

    /// Returns a new RGBA value, with the color values de-multiplied by the
    /// alpha.
    ///
    /// As a special case, a zero alpha de-multiplies into transparent black.
    pub fn demultiply(self) -> Rgba {
        if self.a == 0 {
            return Rgba {
                r: 0,
                g: 0,
                b: 0,
                a: 0,
            };
        }
        Rgba {
            r: (self.r as u16 * 255 / self.a as u16) as u8,
            g: (self.g as u16 * 255 / self.a as u16) as u8,
            b: (self.b as u16 * 255 / self.a as u16) as u8,
            a: self.a,
        }
    }
}

/// Shared RGB extraction (no alpha) for the non-alpha RGB(A) formats, mirroring
/// the `RGBA_T.fromPixel` branch where `is_rgba == false`.
fn rgb_from_pixel(p: Pixel) -> (u8, u8, u8) {
    match p {
        Pixel::Xrgb(q) => (q.r, q.g, q.b),
        Pixel::Rgb(q) => (q.r, q.g, q.b),
        Pixel::Argb(q) => (q.r, q.g, q.b),
        Pixel::Rgba(q) => (q.r, q.g, q.b),
        Pixel::Alpha8(_) | Pixel::Alpha4(_) | Pixel::Alpha2(_) | Pixel::Alpha1(_) => (0, 0, 0),
    }
}

/// Shared RGBA extraction for the alpha-carrying RGB(A) formats, mirroring the
/// `RGBA_T.fromPixel` branch where `is_rgba == true`.
fn rgba_from_pixel(p: Pixel) -> (u8, u8, u8, u8) {
    match p {
        Pixel::Xrgb(q) => (q.r, q.g, q.b, 255),
        Pixel::Rgb(q) => (q.r, q.g, q.b, 255),
        Pixel::Argb(q) => (q.r, q.g, q.b, q.a),
        Pixel::Rgba(q) => (q.r, q.g, q.b, q.a),
        Pixel::Alpha8(_) | Pixel::Alpha4(_) | Pixel::Alpha2(_) | Pixel::Alpha1(_) => {
            (0, 0, 0, Alpha8::from_pixel(p).a)
        }
    }
}

impl Alpha8 {
    pub const FORMAT: Format = Format::Alpha8;
    const MAX_INT: u8 = 255;

    /// Shorthand for a fully-opaque pixel in this format.
    pub const OPAQUE: Alpha8 = Alpha8 { a: 255 };

    /// Returns the pixel translated to the alpha format.
    pub fn from_pixel(p: Pixel) -> Alpha8 {
        match p {
            // Special case: we assume that RGB pixels are always opaque
            Pixel::Xrgb(_) | Pixel::Rgb(_) => Alpha8 { a: Self::MAX_INT },
            Pixel::Argb(q) => Alpha8 {
                a: Self::scale_into_int_alpha8(q.a, 8),
            },
            Pixel::Rgba(q) => Alpha8 {
                a: Self::scale_into_int_alpha8(q.a, 8),
            },
            Pixel::Alpha8(q) => Alpha8 {
                a: Self::scale_into_int_alpha8(q.a, 8),
            },
            Pixel::Alpha4(q) => Alpha8 {
                a: Self::scale_into_int_alpha8(q.a, 4),
            },
            Pixel::Alpha2(q) => Alpha8 {
                a: Self::scale_into_int_alpha8(q.a, 2),
            },
            Pixel::Alpha1(q) => Alpha8 {
                a: Self::scale_into_int_alpha8(q.a, 1),
            },
        }
    }

    /// Low-level primitive used to scale up/down alpha bytes, uses
    /// repeating-bit padding when scaling up. `from_bits` is the source bit
    /// width. Not recommended for external use (use `from_pixel` instead).
    pub fn scale_into_int_alpha8(val: u8, from_bits: usize) -> u8 {
        if from_bits == 1 {
            return val * Self::MAX_INT;
        }
        Self::shlr_u8(val, from_bits)
    }

    fn shlr_u8(val: u8, from_bits: usize) -> u8 {
        if val == 0 {
            return val;
        }
        let to_bits: usize = 8;
        if to_bits == from_bits {
            return val;
        }
        // Only up-scaling into 8 bits is reachable here (from_bits < 8).
        // For scaling up (left shift), we use a repeating-bit padding scheme to
        // make sure that there is as little error as possible. As our cases are
        // very simple, we can reduce this to a lookup table of the difference
        // between our source and target bits:
        //
        // 2, 4 should accommodate u2 to u4 and u4 to u8 (2x bit-difference)
        // 6 should accommodate u2 to u8 (3x bit-difference)
        let _val: u16 = val as u16;
        let out: u16 = match to_bits - from_bits {
            2 | 4 => (_val << (to_bits - from_bits)) + _val,
            6 => {
                (_val << (to_bits - from_bits))
                    | (_val << (to_bits - 2 * from_bits))
                    | (_val << (to_bits - 3 * from_bits))
                    | _val
            }
            _ => panic!("invalid bit difference in lookup table"),
        };
        out as u8
    }

    /// Returns an average of the pixels in the supplied slice. The average of a
    /// zero-length slice is transparent.
    pub fn average(ps: &[Alpha8]) -> Alpha8 {
        debug_assert!(ps.len() <= 256);
        if ps.is_empty() {
            return Alpha8 { a: 0 };
        }
        let mut a: u16 = 0;
        for p in ps {
            a += p.a as u16;
        }
        Alpha8 {
            a: (a / ps.len() as u16) as u8,
        }
    }

    /// Returns this pixel as an interface.
    pub fn as_pixel(self) -> Pixel {
        Pixel::Alpha8(self)
    }

    /// Returns `true` if the supplied pixels are equal.
    pub fn equal(self, other: Pixel) -> bool {
        matches!(other, Pixel::Alpha8(o) if self.a == o.a)
    }
}

/// Low-level primitive used to scale up/down alpha bytes, uses repeating-bit
/// padding when scaling up. Faithful port of the generic `Alpha(fmt).shlr`
/// (`pixel.zig`), with the source/target bit widths passed as runtime values
/// (Zig carries them as the integer `@TypeOf`). `from_bits`/`to_bits` are the
/// source and target bit widths.
fn shlr(val: u8, from_bits: usize, to_bits: usize) -> u8 {
    if val == 0 {
        return val;
    }
    if to_bits == from_bits {
        return val;
    }
    if from_bits > to_bits {
        return val >> (from_bits - to_bits);
    }

    // For scaling up (left shift), we use a repeating-bit padding scheme to
    // make sure that there is as little error as possible. As our cases are
    // very simple, we can reduce this to a lookup table of the difference
    // between our source and target bits:
    //
    // 2, 4 should accommodate u2 to u4 and u4 to u8 (2x bit-difference)
    // 6 should accommodate u2 to u8 (3x bit-difference)
    let _val: u16 = val as u16;
    let out: u16 = match to_bits - from_bits {
        2 | 4 => (_val << (to_bits - from_bits)) + _val,
        6 => {
            (_val << (to_bits - from_bits))
                | (_val << (to_bits - 2 * from_bits))
                | (_val << (to_bits - 3 * from_bits))
                | _val
        }
        _ => panic!("invalid bit difference in lookup table"),
    };
    out as u8
}

/// Faithful port of the generic `Alpha(fmt).scaleIntoIntAlpha`: scale a source
/// alpha value of `from_bits` width into a target of `to_bits` width, with
/// `max_int` the target's maximum value. Single-bit sources multiply into the
/// target's max (the on/off convention).
fn scale_into_int_alpha(val: u8, from_bits: usize, to_bits: usize, max_int: u8) -> u8 {
    if from_bits == 1 {
        return val * max_int;
    }
    shlr(val, from_bits, to_bits)
}

/// Shared `Alpha(fmt).fromPixel` for the sub-8-bit packed alpha types: RGB is
/// opaque, every other source carries its alpha scaled into `to_bits`.
fn alpha_from_pixel(p: Pixel, to_bits: usize, max_int: u8) -> u8 {
    match p {
        // Special case: we assume that RGB pixels are always opaque
        Pixel::Xrgb(_) | Pixel::Rgb(_) => max_int,
        Pixel::Argb(q) => scale_into_int_alpha(q.a, 8, to_bits, max_int),
        Pixel::Rgba(q) => scale_into_int_alpha(q.a, 8, to_bits, max_int),
        Pixel::Alpha8(q) => scale_into_int_alpha(q.a, 8, to_bits, max_int),
        Pixel::Alpha4(q) => scale_into_int_alpha(q.a, 4, to_bits, max_int),
        Pixel::Alpha2(q) => scale_into_int_alpha(q.a, 2, to_bits, max_int),
        Pixel::Alpha1(q) => scale_into_int_alpha(q.a, 1, to_bits, max_int),
    }
}

/// Shared `Alpha(fmt).average` for the packed alpha types: integer mean of the
/// raw channel values (a zero-length slice averages to transparent). Limited to
/// 256 entries, as in the reference.
fn alpha_average(ps: &[u8]) -> u8 {
    debug_assert!(ps.len() <= 256);
    if ps.is_empty() {
        return 0;
    }
    let mut a: u16 = 0;
    for &p in ps {
        a += p as u16;
    }
    (a / ps.len() as u16) as u8
}

impl Alpha4 {
    pub const FORMAT: Format = Format::Alpha4;
    /// Number of significant bits in the packed channel.
    pub const NUM_BITS: usize = 4;
    /// Maximum (fully-opaque) channel value: `(1 << NUM_BITS) - 1`.
    pub const MAX_INT: u8 = (1 << Self::NUM_BITS) - 1;
    /// Shorthand for a fully-opaque pixel in this format.
    pub const OPAQUE: Alpha4 = Alpha4 { a: Self::MAX_INT };

    /// Returns the pixel translated to the alpha format.
    pub fn from_pixel(p: Pixel) -> Alpha4 {
        Alpha4 {
            a: alpha_from_pixel(p, Self::NUM_BITS, Self::MAX_INT),
        }
    }

    /// Returns an average of the pixels in the supplied slice. The average of a
    /// zero-length slice is transparent.
    pub fn average(ps: &[Alpha4]) -> Alpha4 {
        let vals: Vec<u8> = ps.iter().map(|p| p.a).collect();
        Alpha4 {
            a: alpha_average(&vals),
        }
    }

    pub fn as_pixel(self) -> Pixel {
        Pixel::Alpha4(self)
    }
    pub fn equal(self, other: Pixel) -> bool {
        matches!(other, Pixel::Alpha4(o) if self.a == o.a)
    }
}

impl Alpha2 {
    pub const FORMAT: Format = Format::Alpha2;
    /// Number of significant bits in the packed channel.
    pub const NUM_BITS: usize = 2;
    /// Maximum (fully-opaque) channel value: `(1 << NUM_BITS) - 1`.
    pub const MAX_INT: u8 = (1 << Self::NUM_BITS) - 1;
    /// Shorthand for a fully-opaque pixel in this format.
    pub const OPAQUE: Alpha2 = Alpha2 { a: Self::MAX_INT };

    /// Returns the pixel translated to the alpha format.
    pub fn from_pixel(p: Pixel) -> Alpha2 {
        Alpha2 {
            a: alpha_from_pixel(p, Self::NUM_BITS, Self::MAX_INT),
        }
    }

    /// Returns an average of the pixels in the supplied slice. The average of a
    /// zero-length slice is transparent.
    pub fn average(ps: &[Alpha2]) -> Alpha2 {
        let vals: Vec<u8> = ps.iter().map(|p| p.a).collect();
        Alpha2 {
            a: alpha_average(&vals),
        }
    }

    pub fn as_pixel(self) -> Pixel {
        Pixel::Alpha2(self)
    }
    pub fn equal(self, other: Pixel) -> bool {
        matches!(other, Pixel::Alpha2(o) if self.a == o.a)
    }
}

impl Alpha1 {
    pub const FORMAT: Format = Format::Alpha1;
    /// Number of significant bits in the packed channel.
    pub const NUM_BITS: usize = 1;
    /// Maximum (fully-opaque) channel value: `(1 << NUM_BITS) - 1`.
    pub const MAX_INT: u8 = (1 << Self::NUM_BITS) - 1;
    /// Shorthand for a fully-opaque pixel in this format.
    pub const OPAQUE: Alpha1 = Alpha1 { a: Self::MAX_INT };

    /// Returns the pixel translated to the alpha format.
    pub fn from_pixel(p: Pixel) -> Alpha1 {
        Alpha1 {
            a: alpha_from_pixel(p, Self::NUM_BITS, Self::MAX_INT),
        }
    }

    /// Returns an average of the pixels in the supplied slice. The average of a
    /// zero-length slice is transparent.
    pub fn average(ps: &[Alpha1]) -> Alpha1 {
        let vals: Vec<u8> = ps.iter().map(|p| p.a).collect();
        Alpha1 {
            a: alpha_average(&vals),
        }
    }

    pub fn as_pixel(self) -> Pixel {
        Pixel::Alpha1(self)
    }
    pub fn equal(self, other: Pixel) -> bool {
        matches!(other, Pixel::Alpha1(o) if self.a == o.a)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn as_pixel_and_equal() {
        let argb = Argb {
            r: 0xAA,
            g: 0xBB,
            b: 0xCC,
            a: 0xDD,
        };
        let xrgb = Xrgb {
            r: 0xAA,
            g: 0xBB,
            b: 0xCC,
        };
        let rgb = Rgb {
            r: 0xAA,
            g: 0xBB,
            b: 0xCC,
        };
        let rgba = Rgba {
            r: 0xAA,
            g: 0xBB,
            b: 0xCC,
            a: 0xDD,
        };
        let alpha8 = Alpha8 { a: 0xDD };

        assert_eq!(Pixel::Argb(argb), argb.as_pixel());
        assert_eq!(Pixel::Xrgb(xrgb), xrgb.as_pixel());
        assert_eq!(Pixel::Rgb(rgb), rgb.as_pixel());
        assert_eq!(Pixel::Rgba(rgba), rgba.as_pixel());
        assert_eq!(Pixel::Alpha8(alpha8), alpha8.as_pixel());

        assert!(argb.equal(argb.as_pixel()));
        assert!(!argb.equal(rgba.as_pixel()));
        assert!(alpha8.equal(Pixel::Alpha8(Alpha8 { a: 0xDD })));
    }

    #[test]
    fn is_opaque() {
        assert!(Pixel::Rgb(Rgb { r: 1, g: 2, b: 3 }).is_opaque());
        assert!(Pixel::Xrgb(Xrgb { r: 1, g: 2, b: 3 }).is_opaque());
        assert!(Pixel::Alpha8(Alpha8 { a: 255 }).is_opaque());
        assert!(!Pixel::Alpha8(Alpha8 { a: 254 }).is_opaque());
        assert!(Pixel::Rgba(Rgba {
            r: 0,
            g: 0,
            b: 0,
            a: 255
        })
        .is_opaque());
        assert!(!Pixel::Rgba(Rgba {
            r: 0,
            g: 0,
            b: 0,
            a: 0
        })
        .is_opaque());
    }

    #[test]
    fn average_alpha() {
        // Port of the alpha arms of `pixel.zig` `test "average"` — integer
        // mean of the raw packed channel values.
        let alpha8 = [
            Alpha8 { a: 13 },
            Alpha8 { a: 14 },
            Alpha8 { a: 15 },
            Alpha8 { a: 16 },
        ];
        let alpha8_expected = Alpha8 { a: 14 };

        let alpha4 = [
            Alpha4 { a: 3 },
            Alpha4 { a: 4 },
            Alpha4 { a: 5 },
            Alpha4 { a: 6 },
        ];
        let alpha4_expected = Alpha4 { a: 4 };

        let alpha2 = [
            Alpha2 { a: 0 },
            Alpha2 { a: 1 },
            Alpha2 { a: 2 },
            Alpha2 { a: 3 },
        ];
        let alpha2_expected = Alpha2 { a: 1 };

        let alpha1_full = [
            Alpha1 { a: 1 },
            Alpha1 { a: 1 },
            Alpha1 { a: 1 },
            Alpha1 { a: 1 },
        ];
        let alpha1_full_expected = Alpha1 { a: 1 };

        let alpha1_partial = [
            Alpha1 { a: 1 },
            Alpha1 { a: 0 },
            Alpha1 { a: 1 },
            Alpha1 { a: 1 },
        ];
        let alpha1_partial_expected = Alpha1 { a: 0 };

        assert_eq!(alpha8_expected, Alpha8::average(&alpha8));
        assert_eq!(alpha4_expected, Alpha4::average(&alpha4));
        assert_eq!(alpha2_expected, Alpha2::average(&alpha2));
        assert_eq!(alpha1_full_expected, Alpha1::average(&alpha1_full));
        assert_eq!(alpha1_partial_expected, Alpha1::average(&alpha1_partial));
    }

    #[test]
    fn from_pixel_alpha() {
        // RGB is treated as opaque; RGBA carries its alpha into Alpha8.
        assert_eq!(
            Alpha8 { a: 255 },
            Alpha8::from_pixel(Pixel::Rgb(Rgb { r: 1, g: 2, b: 3 }))
        );
        assert_eq!(
            Alpha8 { a: 0x7F },
            Alpha8::from_pixel(Pixel::Rgba(Rgba {
                r: 0,
                g: 0,
                b: 0,
                a: 0x7F
            }))
        );
        // Any alpha-only pixel maps into a zero-RGB, alpha-carrying RGBA.
        assert_eq!(
            Rgba {
                r: 0,
                g: 0,
                b: 0,
                a: 0x40
            },
            Rgba::from_pixel(Pixel::Alpha8(Alpha8 { a: 0x40 }))
        );
    }
}
