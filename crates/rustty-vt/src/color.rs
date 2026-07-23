//! rustty-vt color
//!
//! PORTS: ghostty src/terminal/color.zig
//!
//! PORT STATUS: done

// PARITY: the CIELAB conversion constants are copied verbatim from the reference
// (f32 literals that exceed f32 precision, e.g. 0.1191920); clippy would round
// them, which would diverge from the reference values (R1). Keep them as-is.
#![allow(clippy::excessive_precision)]
// PARITY: `LAB` is the reference type name (`const LAB = struct {...}`); keep it.
#![allow(clippy::upper_case_acronyms)]
// PARITY: `Default` impls mirror the reference's EXPLICIT field defaults (e.g.
// `r/g/b = 0`); keeping the explicit form documents the reference defaults.
#![allow(clippy::derivable_impls)]

// PORT NOTE: the reference imports are
//   const colorpkg = @This();
//   const std = @import("std");
//   const assert = @import("../quirks.zig").inlineAssert;
//   const x11_color = @import("x11_color.zig");
// `colorpkg` (a self-import used for `colorpkg.default`) maps to `crate::color`
// in Rust; `assert` is the `inlineAssert` quirk in rustty-util.

use crate::x11_color;

// PORT NOTE: the reference `assert` is `quirks.inlineAssert`, but several call
// sites here are `const fn` (the reference `default` is a comptime block) and the
// quirk is not a `const` fn. Inside const contexts we use the builtin `assert!`
// macro, which is const-callable and carries identical semantics.

/// The default palette.
// PORT NOTE: the reference builds this at comptime. We reproduce the identical
// construction with a `const fn` (`build_default`) so `DEFAULT` is a compile-time
// constant matching the reference's `default` value.
pub static DEFAULT: Palette = build_default();

const fn build_default() -> Palette {
    let mut result: Palette = [RGB { r: 0, g: 0, b: 0 }; 256];

    // Named values
    let mut i: u8 = 0;
    while i < 16 {
        // PORT NOTE: `Name.default(@enumFromInt(i)) catch unreachable`. For
        // 0..16 every index is a named color with a default, so the error path
        // is unreachable; `name_default_const` returns the value directly.
        result[i as usize] = name_default_const(i);
        i += 1;
    }

    // Cube
    assert!(i == 16);
    let mut r: u8 = 0;
    while r < 6 {
        let mut g: u8 = 0;
        while g < 6 {
            let mut b: u8 = 0;
            while b < 6 {
                result[i as usize] = RGB {
                    r: if r == 0 { 0 } else { r * 40 + 55 },
                    g: if g == 0 { 0 } else { g * 40 + 55 },
                    b: if b == 0 { 0 } else { b * 40 + 55 },
                };

                i += 1;
                b += 1;
            }
            g += 1;
        }
        r += 1;
    }

    // Gray ramp
    assert!(i == 232);
    // assert(@TypeOf(i) == u8);  -- i is declared `u8` above.
    // PORT NOTE: the reference loops `while (i > 0) : (i +%= 1)` using wrapping
    // add so that after index 255 `i` wraps to 0 and the loop terminates. We
    // reproduce the wrapping increment and the `i > 0` guard exactly.
    while i > 0 {
        let value = ((i - 232) * 10) + 8;
        result[i as usize] = RGB {
            r: value,
            g: value,
            b: value,
        };
        i = i.wrapping_add(1);
    }

    result
}

/// Palette is the 256 color palette.
pub type Palette = [RGB; 256];

/// A parsed palette entry from Rustty's config "N=COLOR" syntax.
// pub const PaletteEntry = struct { index: u8, color: RGB };
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PaletteEntry {
    pub index: u8,
    pub color: RGB,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParsePaletteEntryError {
    InvalidFormat,
    Overflow,
}

/// Parse a palette entry in Rustty config syntax: "N=COLOR" where N is
/// a palette index 0-255 (decimal, or 0x/0o/0b-prefixed per Zig's
/// parseInt base-0 rules) and COLOR is anything RGB::parse accepts.
/// Whitespace (spaces/tabs) around N and COLOR is ignored.
// pub fn parsePaletteEntry(value: []const u8) error{ InvalidFormat, Overflow }!PaletteEntry
pub fn parse_palette_entry(value: &[u8]) -> Result<PaletteEntry, ParsePaletteEntryError> {
    // const eql_idx = std.mem.indexOfScalar(u8, value, '=') orelse return error.InvalidFormat;
    let eql_idx = value
        .iter()
        .position(|&b| b == b'=')
        .ok_or(ParsePaletteEntryError::InvalidFormat)?;
    // const index = std.fmt.parseInt(u8, std.mem.trim(u8, value[0..eql_idx], " \t"), 0) catch ...
    let index_str = trim_space_tab(&value[..eql_idx]);
    let index = parse_int_u8_base0(index_str)?;
    // const rgb = try RGB.parse(value[eql_idx + 1 ..]);
    let rest = &value[eql_idx + 1..];
    let rest_str = std::str::from_utf8(rest).map_err(|_| ParsePaletteEntryError::InvalidFormat)?;
    let rgb = RGB::parse(rest_str).map_err(|_| ParsePaletteEntryError::InvalidFormat)?;
    Ok(PaletteEntry { index, color: rgb })
}

/// Zig `std.fmt.parseInt(u8, s, 0)`: base-0 = auto-detect 0x/0o/0b prefix,
/// else decimal. `Overflow` (value > 255) is distinguished from
/// `InvalidCharacter` (mapped to InvalidFormat), matching the oracle.
fn parse_int_u8_base0(s: &[u8]) -> Result<u8, ParsePaletteEntryError> {
    let (digits, radix): (&[u8], u32) = if s.len() > 2 && s[0] == b'0' {
        match s[1] {
            b'x' | b'X' => (&s[2..], 16),
            b'o' | b'O' => (&s[2..], 8),
            b'b' | b'B' => (&s[2..], 2),
            _ => (s, 10),
        }
    } else {
        (s, 10)
    };
    if digits.is_empty() {
        return Err(ParsePaletteEntryError::InvalidFormat);
    }
    let mut acc: u32 = 0;
    for &b in digits {
        // Zig parseInt allows '_' separators; preserve that.
        if b == b'_' {
            continue;
        }
        let d = (b as char)
            .to_digit(radix)
            .ok_or(ParsePaletteEntryError::InvalidFormat)?;
        acc = acc * radix + d;
        if acc > 255 {
            return Err(ParsePaletteEntryError::Overflow);
        }
    }
    Ok(acc as u8)
}

fn trim_space_tab(s: &[u8]) -> &[u8] {
    let start = s.iter().position(|&b| b != b' ' && b != b'\t').unwrap_or(s.len());
    let end = s.iter().rposition(|&b| b != b' ' && b != b'\t').map_or(start, |i| i + 1);
    &s[start..end]
}

// PORT NOTE: `pub const PaletteC = [256]RGB.C;` and `paletteCval`/`paletteZval`
// convert between the native and C-ABI palette. The C ABI surface (`RGB.C`) is
// ported in the rustty-vt-c crate; since `RGB` and `RGB.C` carry identical r/g/b
// data, the conversions are deferred to that crate with the C type. They are
// reproduced here as comments to preserve the reference surface.
//
// /// C-compatible palette type using the extern RGB struct.
// pub const PaletteC = [256]RGB.C;
//
// /// Convert a Palette to a PaletteC.
// pub fn paletteCval(palette: *const Palette) PaletteC { ... }
//
// /// Convert a PaletteC to a Palette.
// pub fn paletteZval(palette: *const PaletteC) Palette { ... }

/// Mask that can be used to set which palette indexes were set.
// PORT NOTE: `pub const PaletteMask = std.StaticBitSet(@typeInfo(Palette).array.len);`
// (256 bits). Rust has no std StaticBitSet, so a compact fixed-size bitset is
// defined below with the operations the reference uses (initEmpty/set/unset/
// isSet/count/iterator).
pub type PaletteMask = StaticBitSet256;

/// A 256-bit fixed-size bitset matching `std.StaticBitSet(256)`.
// PORT NOTE: minimal Rust-only glue (R1) standing in for `std.StaticBitSet`. It
// adds no reference logic; it only provides the masked-set operations used by
// the palette code and tests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StaticBitSet256 {
    words: [u64; 4],
}

impl StaticBitSet256 {
    /// `initEmpty`: all bits unset.
    pub const fn init_empty() -> Self {
        StaticBitSet256 { words: [0; 4] }
    }

    /// `set`: set the bit at `idx`.
    pub fn set(&mut self, idx: usize) {
        self.words[idx / 64] |= 1u64 << (idx % 64);
    }

    /// `unset`: clear the bit at `idx`.
    pub fn unset(&mut self, idx: usize) {
        self.words[idx / 64] &= !(1u64 << (idx % 64));
    }

    /// `isSet`: true if the bit at `idx` is set.
    pub fn is_set(&self, idx: usize) -> bool {
        (self.words[idx / 64] >> (idx % 64)) & 1 == 1
    }

    /// `count`: number of set bits.
    pub fn count(&self) -> usize {
        self.words.iter().map(|w| w.count_ones() as usize).sum()
    }

    /// `iterator(.{})`: iterate set bit indices in ascending order.
    pub fn iter_set(&self) -> impl Iterator<Item = usize> + '_ {
        (0..256).filter(move |&i| self.is_set(i))
    }
}

impl Default for StaticBitSet256 {
    fn default() -> Self {
        Self::init_empty()
    }
}

/// Generate the 256-color palette from the user's base16 theme colors,
/// terminal background, and terminal foreground.
///
/// Motivation: The default 256-color palette uses fixed, fully-saturated
/// colors that clash with custom base16 themes, have poor readability in
/// dark shades (the first non-black shade jumps to 37% intensity instead
/// of the expected 20%), and exhibit inconsistent perceived brightness
/// across hues of the same shade (e.g., blue appears darker than green).
/// By generating the extended palette from the user's chosen colors,
/// programs can use the richer 256-color range without requiring their
/// own theme configuration, and light/dark switching works automatically.
///
/// The 216-color cube (indices 16–231) is built via trilinear
/// interpolation in CIELAB space over the 8 base colors. The base16
/// palette maps to the 8 corners of a 6×6×6 RGB cube as follows:
///
///   R=0 edge: bg      → base[1] (red)
///   R=5 edge: base[6] → fg
///   G=0 edge: bg/base[6] (via R) → base[2]/base[4] (green/blue via R)
///   G=5 edge: base[1]/fg (via R) → base[3]/base[5] (yellow/magenta via R)
///
/// For each R slice, four corner colors (c0–c3) are interpolated along
/// the R axis, then for each G row two edge colors (c4–c5) are
/// interpolated along G, and finally each B cell is interpolated along B
/// to produce the final color. CIELAB interpolation ensures perceptually
/// uniform brightness transitions across different hues.
///
/// The 24-step grayscale ramp (indices 232–255) is a simple linear
/// interpolation in CIELAB from the background to the foreground,
/// excluding pure black and white (available in the cube at (0,0,0)
/// and (5,5,5)). The interpolation parameter runs from 1/25 to 24/25.
///
/// Fill `skip` with user-defined color indexes to avoid replacing them.
///
/// Reference: https://gist.github.com/jake-stewart/0a8ea46159a7da2c808e5be2177e1783
pub fn generate_256_color(
    base: Palette,
    skip: PaletteMask,
    bg: RGB,
    fg: RGB,
    harmonious: bool,
) -> Palette {
    // Convert the background, foreground, and 8 base theme colors into
    // CIELAB space so that all interpolation is perceptually uniform.
    let base8_lab: [LAB; 8] = {
        let mut base8: [LAB; 8] = [
            LAB::from_rgb(bg),
            LAB::from_rgb(base[1]),
            LAB::from_rgb(base[2]),
            LAB::from_rgb(base[3]),
            LAB::from_rgb(base[4]),
            LAB::from_rgb(base[5]),
            LAB::from_rgb(base[6]),
            LAB::from_rgb(fg),
        ];

        // For light themes (where the foreground is darker than the
        // background), the cube's dark-to-light orientation is inverted
        // relative to the base color mapping. When `harmonious` is false,
        // swap bg and fg so the cube still runs from black (16) to
        // white (231).
        let is_light_theme = base8[7].l < base8[0].l;
        let invert = is_light_theme && !harmonious;
        if invert {
            base8.swap(0, 7);
        }

        base8
    };

    // Start from the base palette so indices 0–15 are preserved as-is.
    let mut result = base;

    // Build the 216-color cube (indices 16–231) via trilinear interpolation
    // in CIELAB. The three nested loops correspond to the R, G, and B axes
    // of a 6×6×6 cube. For each R slice, four corner colors (c0–c3) are
    // interpolated along R from the 8 base colors, mapping the cube corners
    // to theme-aware anchors (see doc comment for the mapping). Then for
    // each G row, two edge colors (c4–c5) blend along G, and finally each
    // B cell interpolates along B to produce the final color.
    let mut idx: usize = 16;
    for ri in 0..6 {
        // R-axis corners: blend base colors along the red dimension.
        let tr = ri as f32 / 5.0;
        let c0: LAB = LAB::lerp(tr, base8_lab[0], base8_lab[1]);
        let c1: LAB = LAB::lerp(tr, base8_lab[2], base8_lab[3]);
        let c2: LAB = LAB::lerp(tr, base8_lab[4], base8_lab[5]);
        let c3: LAB = LAB::lerp(tr, base8_lab[6], base8_lab[7]);
        for gi in 0..6 {
            // G-axis edges: blend the R-interpolated corners along green.
            let tg = gi as f32 / 5.0;
            let c4: LAB = LAB::lerp(tg, c0, c1);
            let c5: LAB = LAB::lerp(tg, c2, c3);
            for bi in 0..6 {
                // B-axis: final interpolation along blue, then convert back to RGB.
                if !skip.is_set(idx) {
                    let c6: LAB = LAB::lerp(bi as f32 / 5.0, c4, c5);
                    result[idx] = c6.to_rgb();
                }

                idx += 1;
            }
        }
    }

    // Build the 24-step grayscale ramp (indices 232–255) by linearly
    // interpolating in CIELAB from background to foreground. The parameter
    // runs from 1/25 to 24/25, excluding the endpoints which are already
    // available in the cube at (0,0,0) and (5,5,5).
    for i in 0..24 {
        let t = (i + 1) as f32 / 25.0;
        if !skip.is_set(idx) {
            let c: LAB = LAB::lerp(t, base8_lab[0], base8_lab[7]);
            result[idx] = c.to_rgb();
        }
        idx += 1;
    }

    result
}

/// A palette that can have its colors changed and reset. Purposely built
/// for terminal color operations.
#[derive(Clone, Copy, Debug)]
pub struct DynamicPalette {
    /// The current palette including any user modifications.
    pub current: Palette,

    /// The original/default palette values.
    pub original: Palette,

    /// A bitset where each bit represents whether the corresponding
    /// palette index has been modified from its default value.
    pub mask: PaletteMask,
}

impl DynamicPalette {
    // pub const default: DynamicPalette = .init(colorpkg.default);
    pub fn default_palette() -> DynamicPalette {
        DynamicPalette::init(DEFAULT)
    }

    /// Initialize a dynamic palette with a default palette.
    pub fn init(def: Palette) -> DynamicPalette {
        DynamicPalette {
            current: def,
            original: def,
            mask: PaletteMask::init_empty(),
        }
    }

    /// Set a custom color at the given palette index.
    pub fn set(&mut self, idx: u8, color: RGB) {
        self.current[idx as usize] = color;
        self.mask.set(idx as usize);
    }

    /// Reset the color at the given palette index to its original value.
    pub fn reset(&mut self, idx: u8) {
        self.current[idx as usize] = self.original[idx as usize];
        self.mask.unset(idx as usize);
    }

    /// Reset all colors to their original values.
    pub fn reset_all(&mut self) {
        *self = DynamicPalette::init(self.original);
    }

    /// Change the default palette, but preserve the changed values.
    pub fn change_default(&mut self, def: Palette) {
        self.original = def;

        // Fast path, the palette is usually not changed.
        if self.mask.count() == 0 {
            self.current = self.original;
            return;
        }

        // There are usually less set than unset, so iterate over the changed
        // values and override them.
        let mut current = def;
        for idx in self.mask.iter_set() {
            current[idx] = self.current[idx];
        }
        self.current = current;
    }
}

/// RGB value that can be changed and reset. This can also be totally unset
/// in every way, in which case the caller can determine their own ultimate
/// default.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DynamicRGB {
    pub override_: Option<RGB>,
    pub default: Option<RGB>,
}

impl DynamicRGB {
    // pub const unset: DynamicRGB = .{ .override = null, .default = null };
    pub const UNSET: DynamicRGB = DynamicRGB {
        override_: None,
        default: None,
    };

    pub fn init(def: RGB) -> DynamicRGB {
        DynamicRGB {
            override_: None,
            default: Some(def),
        }
    }

    pub fn get(&self) -> Option<RGB> {
        self.override_.or(self.default)
    }

    pub fn set(&mut self, color: RGB) {
        self.override_ = Some(color);
    }

    pub fn reset(&mut self) {
        self.override_ = self.default;
    }
}

/// Color names in the standard 8 or 16 color palette.
// PORT NOTE: `enum(u8)` with a `_` (non-exhaustive) tag. The remainder values
// (16..256) are valid unnamed palette entries; the `Other(u8)` arm preserves
// the non-exhaustive semantics noted by the reference below.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Name {
    Black,         // = 0
    Red,           // = 1
    Green,         // = 2
    Yellow,        // = 3
    Blue,          // = 4
    Magenta,       // = 5
    Cyan,          // = 6
    White,         // = 7
    BrightBlack,   // = 8
    BrightRed,     // = 9
    BrightGreen,   // = 10
    BrightYellow,  // = 11
    BrightBlue,    // = 12
    BrightMagenta, // = 13
    BrightCyan,    // = 14
    BrightWhite,   // = 15

    // Remainders are valid unnamed values in the 256 color palette.
    Other(u8),
}

impl Name {
    // pub const C = u8;
    // (The C type is `u8`; the value is the enum's integer below.)

    /// Construct a Name from its u8 value (`@enumFromInt`).
    pub fn from_int(v: u8) -> Name {
        match v {
            0 => Name::Black,
            1 => Name::Red,
            2 => Name::Green,
            3 => Name::Yellow,
            4 => Name::Blue,
            5 => Name::Magenta,
            6 => Name::Cyan,
            7 => Name::White,
            8 => Name::BrightBlack,
            9 => Name::BrightRed,
            10 => Name::BrightGreen,
            11 => Name::BrightYellow,
            12 => Name::BrightBlue,
            13 => Name::BrightMagenta,
            14 => Name::BrightCyan,
            15 => Name::BrightWhite,
            _ => Name::Other(v),
        }
    }

    /// The integer value of this name (`@intFromEnum`).
    pub fn cval(self) -> u8 {
        match self {
            Name::Black => 0,
            Name::Red => 1,
            Name::Green => 2,
            Name::Yellow => 3,
            Name::Blue => 4,
            Name::Magenta => 5,
            Name::Cyan => 6,
            Name::White => 7,
            Name::BrightBlack => 8,
            Name::BrightRed => 9,
            Name::BrightGreen => 10,
            Name::BrightYellow => 11,
            Name::BrightBlue => 12,
            Name::BrightMagenta => 13,
            Name::BrightCyan => 14,
            Name::BrightWhite => 15,
            Name::Other(v) => v,
        }
    }

    /// Default colors for tagged values.
    // PORT NOTE: the reference returns `error{NoDefaultValue}!RGB`; ported as
    // `Result<RGB, NoDefaultValue>`.
    pub fn default(self) -> Result<RGB, NoDefaultValue> {
        Ok(match self {
            Name::Black => RGB {
                r: 0x1D,
                g: 0x1F,
                b: 0x21,
            },
            Name::Red => RGB {
                r: 0xCC,
                g: 0x66,
                b: 0x66,
            },
            Name::Green => RGB {
                r: 0xB5,
                g: 0xBD,
                b: 0x68,
            },
            Name::Yellow => RGB {
                r: 0xF0,
                g: 0xC6,
                b: 0x74,
            },
            Name::Blue => RGB {
                r: 0x81,
                g: 0xA2,
                b: 0xBE,
            },
            Name::Magenta => RGB {
                r: 0xB2,
                g: 0x94,
                b: 0xBB,
            },
            Name::Cyan => RGB {
                r: 0x8A,
                g: 0xBE,
                b: 0xB7,
            },
            Name::White => RGB {
                r: 0xC5,
                g: 0xC8,
                b: 0xC6,
            },
            Name::BrightBlack => RGB {
                r: 0x66,
                g: 0x66,
                b: 0x66,
            },
            Name::BrightRed => RGB {
                r: 0xD5,
                g: 0x4E,
                b: 0x53,
            },
            Name::BrightGreen => RGB {
                r: 0xB9,
                g: 0xCA,
                b: 0x4A,
            },
            Name::BrightYellow => RGB {
                r: 0xE7,
                g: 0xC5,
                b: 0x47,
            },
            Name::BrightBlue => RGB {
                r: 0x7A,
                g: 0xA6,
                b: 0xDA,
            },
            Name::BrightMagenta => RGB {
                r: 0xC3,
                g: 0x97,
                b: 0xD8,
            },
            Name::BrightCyan => RGB {
                r: 0x70,
                g: 0xC0,
                b: 0xB1,
            },
            Name::BrightWhite => RGB {
                r: 0xEA,
                g: 0xEA,
                b: 0xEA,
            },

            _ => return Err(NoDefaultValue),
        })
    }
}

/// The error returned by `Name::default` for values without a default.
// PORT NOTE: the reference `error{NoDefaultValue}`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NoDefaultValue;

// PORT NOTE: `name_default_const` is the `const fn` form of `Name.default` used
// to build `DEFAULT` at compile time. It mirrors the same value table; for
// 0..16 (the only inputs used by `build_default`) it returns the named color.
const fn name_default_const(v: u8) -> RGB {
    match v {
        0 => RGB {
            r: 0x1D,
            g: 0x1F,
            b: 0x21,
        },
        1 => RGB {
            r: 0xCC,
            g: 0x66,
            b: 0x66,
        },
        2 => RGB {
            r: 0xB5,
            g: 0xBD,
            b: 0x68,
        },
        3 => RGB {
            r: 0xF0,
            g: 0xC6,
            b: 0x74,
        },
        4 => RGB {
            r: 0x81,
            g: 0xA2,
            b: 0xBE,
        },
        5 => RGB {
            r: 0xB2,
            g: 0x94,
            b: 0xBB,
        },
        6 => RGB {
            r: 0x8A,
            g: 0xBE,
            b: 0xB7,
        },
        7 => RGB {
            r: 0xC5,
            g: 0xC8,
            b: 0xC6,
        },
        8 => RGB {
            r: 0x66,
            g: 0x66,
            b: 0x66,
        },
        9 => RGB {
            r: 0xD5,
            g: 0x4E,
            b: 0x53,
        },
        10 => RGB {
            r: 0xB9,
            g: 0xCA,
            b: 0x4A,
        },
        11 => RGB {
            r: 0xE7,
            g: 0xC5,
            b: 0x47,
        },
        12 => RGB {
            r: 0x7A,
            g: 0xA6,
            b: 0xDA,
        },
        13 => RGB {
            r: 0xC3,
            g: 0x97,
            b: 0xD8,
        },
        14 => RGB {
            r: 0x70,
            g: 0xC0,
            b: 0xB1,
        },
        15 => RGB {
            r: 0xEA,
            g: 0xEA,
            b: 0xEA,
        },
        _ => unreachable!(),
    }
}

/// The "special colors" as denoted by xterm. These can be set via
/// OSC 5 or via OSC 4 by adding the palette length to it.
///
/// https://invisible-island.net/xterm/ctlseqs/ctlseqs.html
// PORT NOTE: `enum(u3)`; ported with explicit `int()` since Rust has no u3.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Special {
    Bold,      // = 0
    Underline, // = 1
    Blink,     // = 2
    Reverse,   // = 3
    Italic,    // = 4
}

impl Special {
    fn int(self) -> u16 {
        match self {
            Special::Bold => 0,
            Special::Underline => 1,
            Special::Blink => 2,
            Special::Reverse => 3,
            Special::Italic => 4,
        }
    }

    pub fn osc4(self) -> u16 {
        // "The special colors can also be set by adding the maximum
        // number of colors (e.g., 88 or 256) to these codes in an
        // OSC 4  control" - xterm ctlseqs
        // PORT NOTE: `@typeInfo(Palette).array.len` == 256.
        let max: u16 = 256;
        self.int() + max
    }
}

/// The "dynamic colors" as denoted by xterm. These can be set via
/// OSC 10 through 19.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dynamic {
    Foreground = 10,
    Background = 11,
    Cursor = 12,
    PointerForeground = 13,
    PointerBackground = 14,
    TektronixForeground = 15,
    TektronixBackground = 16,
    HighlightBackground = 17,
    TektronixCursor = 18,
    HighlightForeground = 19,
}

impl Dynamic {
    /// The next dynamic color sequentially. This is required because
    /// specifying colors sequentially without their index will automatically
    /// use the next dynamic color.
    ///
    /// "Each successive parameter changes the next color in the list.  The
    /// value of Ps tells the starting point in the list."
    pub fn next(self) -> Option<Dynamic> {
        // PORT NOTE: `std.meta.intToEnum(Dynamic, @intFromEnum(self) + 1) catch null`.
        match (self as u8) + 1 {
            11 => Some(Dynamic::Background),
            12 => Some(Dynamic::Cursor),
            13 => Some(Dynamic::PointerForeground),
            14 => Some(Dynamic::PointerBackground),
            15 => Some(Dynamic::TektronixForeground),
            16 => Some(Dynamic::TektronixBackground),
            17 => Some(Dynamic::HighlightBackground),
            18 => Some(Dynamic::TektronixCursor),
            19 => Some(Dynamic::HighlightForeground),
            _ => None,
        }
    }
}

/// RGB
// PORT NOTE: the reference is `packed struct(u24) { r: u8, g: u8, b: u8 }` with
// comptime asserts `@bitSizeOf(RGB) == 24` and `@sizeOf(RGB) == 4`. The bit
// layout is r/g/b each 8 bits = 24 data bits. In Zig a `packed struct(u24)` is
// stored in a u32 (so `@sizeOf == 4`); in Rust the natural `#[repr(C)]` layout
// of three `u8` fields is exactly the 24 data bits with `size_of == 3` (no
// backing-integer rounding). The 24-bit DATA layout is identical and matches the
// C type `RGB.C` (`extern struct { r, g, b: u8 }`). The size assert is adapted
// in the test below with a PORT NOTE; the bit-size (24) invariant is preserved.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RGB {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Default for RGB {
    // PORT NOTE: reference field defaults `r/g/b = 0`.
    fn default() -> Self {
        RGB { r: 0, g: 0, b: 0 }
    }
}

impl RGB {
    // pub const C = extern struct { r: u8, g: u8, b: u8 };
    // PORT NOTE: the C ABI type and `fromC`/`cval` conversions are ported in the
    // rustty-vt-c crate; `RGB` and `RGB.C` carry identical r/g/b bytes.

    pub fn eql(self, other: RGB) -> bool {
        self.r == other.r && self.g == other.g && self.b == other.b
    }

    /// Calculates the contrast ratio between two colors. The contrast
    /// ration is a value between 1 and 21 where 1 is the lowest contrast
    /// and 21 is the highest contrast.
    ///
    /// https://www.w3.org/TR/WCAG20/#contrast-ratiodef
    pub fn contrast(self, other: RGB) -> f64 {
        // pair[0] = lighter, pair[1] = darker
        let pair: [f64; 2] = {
            let self_lum = self.luminance();
            let other_lum = other.luminance();
            if self_lum > other_lum {
                [self_lum, other_lum]
            } else {
                [other_lum, self_lum]
            }
        };

        (pair[0] + 0.05) / (pair[1] + 0.05)
    }

    /// Calculates luminance based on the W3C formula. This returns a
    /// normalized value between 0 and 1 where 0 is black and 1 is white.
    ///
    /// https://www.w3.org/TR/WCAG20/#relativeluminancedef
    pub fn luminance(self) -> f64 {
        let r_lum = component_luminance(self.r);
        let g_lum = component_luminance(self.g);
        let b_lum = component_luminance(self.b);
        0.2126 * r_lum + 0.7152 * g_lum + 0.0722 * b_lum
    }

    /// Calculates "perceived luminance" which is better for determining
    /// light vs dark.
    ///
    /// Source: https://www.w3.org/TR/AERT/#color-contrast
    pub fn perceived_luminance(self) -> f64 {
        let r_f64: f64 = self.r as f64;
        let g_f64: f64 = self.g as f64;
        let b_f64: f64 = self.b as f64;
        0.299 * (r_f64 / 255.0) + 0.587 * (g_f64 / 255.0) + 0.114 * (b_f64 / 255.0)
    }

    /// Parse a color from a floating point intensity value.
    ///
    /// The value should be between 0.0 and 1.0, inclusive.
    fn from_intensity(value: &str) -> Result<u8, InvalidFormat> {
        let i = value.parse::<f64>().map_err(|_| InvalidFormat)?;
        if !(0.0..=1.0).contains(&i) {
            return Err(InvalidFormat);
        }

        Ok((i * u8::MAX as f64) as u8)
    }

    /// Parse a color from a string of hexadecimal digits
    ///
    /// The string can contain 1, 2, 3, or 4 characters and represents the color
    /// value scaled in 4, 8, 12, or 16 bits, respectively.
    fn from_hex(value: &str) -> Result<u8, InvalidFormat> {
        if value.is_empty() || value.len() > 4 {
            return Err(InvalidFormat);
        }

        let color = u16::from_str_radix(value, 16).map_err(|_| InvalidFormat)?;

        let divisor: usize = match value.len() {
            1 => 0xF, // std.math.maxInt(u4)
            2 => u8::MAX as usize,
            3 => 0xFFF,  // std.math.maxInt(u12)
            4 => 0xFFFF, // std.math.maxInt(u16)
            _ => unreachable!(),
        };

        Ok((color as usize * u8::MAX as usize / divisor) as u8)
    }

    /// Parse a color specification.
    ///
    /// Leading and trailing spaces and tabs are ignored.
    ///
    /// Any of the following forms are accepted:
    ///
    /// 1. rgb:<red>/<green>/<blue>
    ///
    ///    <red>, <green>, <blue> := h | hh | hhh | hhhh
    ///
    ///    where `h` is a single hexadecimal digit.
    ///
    /// 2. rgbi:<red>/<green>/<blue>
    ///
    ///    where <red>, <green>, and <blue> are floating point values between
    ///    0.0 and 1.0 (inclusive).
    ///
    /// 3. #rgb, #rrggbb, rgb, rrggbb, #rrrgggbbb, #rrrrggggbbbb
    ///
    ///    where `r`, `g`, and `b` are hexadecimal digits. The forms with a
    ///    leading # specify a color with 4, 8, 12, and 16 bits of precision
    ///    per color channel. The forms without a leading # are accepted for
    ///    compatibility with Rustty config/theme color values.
    ///
    /// 4. X11 color names
    pub fn parse(value: &str) -> Result<RGB, InvalidFormat> {
        // const input = std.mem.trim(u8, value, " \t");
        let input = value.trim_matches(|c| c == ' ' || c == '\t');
        if input.is_empty() {
            return Err(InvalidFormat);
        }

        let bytes = input.as_bytes();
        if bytes[0] == b'#' {
            return match input.len() {
                4 => Ok(RGB {
                    r: RGB::from_hex(&input[1..2])?,
                    g: RGB::from_hex(&input[2..3])?,
                    b: RGB::from_hex(&input[3..4])?,
                }),
                7 => Ok(RGB {
                    r: RGB::from_hex(&input[1..3])?,
                    g: RGB::from_hex(&input[3..5])?,
                    b: RGB::from_hex(&input[5..7])?,
                }),
                10 => Ok(RGB {
                    r: RGB::from_hex(&input[1..4])?,
                    g: RGB::from_hex(&input[4..7])?,
                    b: RGB::from_hex(&input[7..10])?,
                }),
                13 => Ok(RGB {
                    r: RGB::from_hex(&input[1..5])?,
                    g: RGB::from_hex(&input[5..9])?,
                    b: RGB::from_hex(&input[9..13])?,
                }),

                _ => Err(InvalidFormat),
            };
        }

        // Check for X11 named colors. We allow whitespace around the edges.
        if let Some(rgb) = x11_color::map.get(input) {
            return Ok(rgb);
        }

        // switch (input.len) { 3 => ..., 6 => ..., else => {} }
        match input.len() {
            3 => {
                return Ok(RGB {
                    r: RGB::from_hex(&input[0..1])?,
                    g: RGB::from_hex(&input[1..2])?,
                    b: RGB::from_hex(&input[2..3])?,
                })
            }
            6 => {
                return Ok(RGB {
                    r: RGB::from_hex(&input[0..2])?,
                    g: RGB::from_hex(&input[2..4])?,
                    b: RGB::from_hex(&input[4..6])?,
                })
            }
            _ => {}
        }

        if input.len() < "rgb:a/a/a".len() || &input[0..3] != "rgb" {
            return Err(InvalidFormat);
        }

        let mut i: usize = 3;

        let use_intensity = if bytes[i] == b'i' {
            i += 1;
            true
        } else {
            false
        };

        if bytes[i] != b':' {
            return Err(InvalidFormat);
        }

        i += 1;

        let r = {
            // PORT NOTE: `std.mem.indexOfScalarPos(u8, input, i, '/')`.
            let slice = match input[i..].find('/') {
                Some(end_rel) => &input[i..i + end_rel],
                None => return Err(InvalidFormat),
            };

            i += slice.len() + 1;

            if use_intensity {
                RGB::from_intensity(slice)?
            } else {
                RGB::from_hex(slice)?
            }
        };

        let g = {
            let slice = match input[i..].find('/') {
                Some(end_rel) => &input[i..i + end_rel],
                None => return Err(InvalidFormat),
            };

            i += slice.len() + 1;

            if use_intensity {
                RGB::from_intensity(slice)?
            } else {
                RGB::from_hex(slice)?
            }
        };

        let b = if use_intensity {
            RGB::from_intensity(&input[i..])?
        } else {
            RGB::from_hex(&input[i..])?
        };

        Ok(RGB { r, g, b })
    }
}

/// The error returned by the RGB parse helpers.
// PORT NOTE: the reference `error{InvalidFormat}`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidFormat;

/// Calculates single-component luminance based on the W3C formula.
///
/// Expects sRGB color space which at the time of writing we don't
/// generally use but it's a good enough approximation until we fix that.
/// https://www.w3.org/TR/WCAG20/#relativeluminancedef
// PORT NOTE: the reference `componentLuminance` is a private method on RGB; in
// Rust it is a free function (it takes only the channel value).
fn component_luminance(c: u8) -> f64 {
    let c_f64: f64 = c as f64;
    let normalized: f64 = c_f64 / 255.0;
    if normalized <= 0.03928 {
        return normalized / 12.92;
    }
    ((normalized + 0.055) / 1.055).powf(2.4)
}

/// LAB color space
// PORT NOTE: the reference is a private `const LAB = struct { ... }`.
#[derive(Clone, Copy, Debug)]
struct LAB {
    l: f32,
    a: f32,
    b: f32,
}

impl LAB {
    /// RGB to LAB
    fn from_rgb(rgb: RGB) -> LAB {
        // Step 1: Normalize sRGB channels from [0, 255] to [0.0, 1.0].
        let mut r: f32 = rgb.r as f32 / 255.0;
        let mut g: f32 = rgb.g as f32 / 255.0;
        let mut b: f32 = rgb.b as f32 / 255.0;

        // Step 2: Apply the inverse sRGB companding (gamma correction) to
        // convert from sRGB to linear RGB. The sRGB transfer function has
        // two segments: a linear portion for small values and a power curve
        // for the rest.
        r = if r > 0.04045 {
            ((r + 0.055) / 1.055).powf(2.4)
        } else {
            r / 12.92
        };
        g = if g > 0.04045 {
            ((g + 0.055) / 1.055).powf(2.4)
        } else {
            g / 12.92
        };
        b = if b > 0.04045 {
            ((b + 0.055) / 1.055).powf(2.4)
        } else {
            b / 12.92
        };

        // Step 3: Convert linear RGB to CIE XYZ using the sRGB to XYZ
        // transformation matrix (D65 illuminant). The X and Z values are
        // normalized by the D65 white point reference values (Xn=0.95047,
        // Zn=1.08883; Yn=1.0 is implicit).
        let mut x = (r * 0.4124564 + g * 0.3575761 + b * 0.1804375) / 0.95047;
        let mut y = r * 0.2126729 + g * 0.7151522 + b * 0.0721750;
        let mut z = (r * 0.0193339 + g * 0.1191920 + b * 0.9503041) / 1.08883;

        // Step 4: Apply the CIE f(t) nonlinear transform to each XYZ
        // component. Above the threshold (epsilon ≈ 0.008856) the cube
        // root is used; below it, a linear approximation avoids numerical
        // instability near zero.
        x = if x > 0.008856 {
            x.cbrt()
        } else {
            7.787 * x + 16.0 / 116.0
        };
        y = if y > 0.008856 {
            y.cbrt()
        } else {
            7.787 * y + 16.0 / 116.0
        };
        z = if z > 0.008856 {
            z.cbrt()
        } else {
            7.787 * z + 16.0 / 116.0
        };

        // Step 5: Compute the final CIELAB values from the transformed XYZ.
        // L* is lightness (0–100), a* is green–red, b* is blue–yellow.
        LAB {
            l: 116.0 * y - 16.0,
            a: 500.0 * (x - y),
            b: 200.0 * (y - z),
        }
    }

    /// LAB to RGB
    fn to_rgb(self) -> RGB {
        // Step 1: Recover the intermediate f(Y), f(X), f(Z) values from
        // L*a*b* by inverting the CIELAB formulas.
        let y = (self.l + 16.0) / 116.0;
        let x = self.a / 500.0 + y;
        let z = y - self.b / 200.0;

        // Step 2: Apply the inverse CIE f(t) transform to get back to
        // XYZ. Above epsilon (≈0.008856) the cube is used; below it the
        // linear segment is inverted. Results are then scaled by the D65
        // white point reference values (Xn=0.95047, Zn=1.08883; Yn=1.0).
        let x3 = x * x * x;
        let y3 = y * y * y;
        let z3 = z * z * z;
        let xf = (if x3 > 0.008856 {
            x3
        } else {
            (x - 16.0 / 116.0) / 7.787
        }) * 0.95047;
        let yf = if y3 > 0.008856 {
            y3
        } else {
            (y - 16.0 / 116.0) / 7.787
        };
        let zf = (if z3 > 0.008856 {
            z3
        } else {
            (z - 16.0 / 116.0) / 7.787
        }) * 1.08883;

        // Step 3: Convert CIE XYZ back to linear RGB using the XYZ to sRGB
        // matrix (inverse of the sRGB to XYZ matrix, D65 illuminant).
        let mut r = xf * 3.2404542 - yf * 1.5371385 - zf * 0.4985314;
        let mut g = -xf * 0.9692660 + yf * 1.8760108 + zf * 0.0415560;
        let mut b = xf * 0.0556434 - yf * 0.2040259 + zf * 1.0572252;

        // Step 4: Apply sRGB companding (gamma correction) to convert from
        // linear RGB back to sRGB. This is the forward sRGB transfer
        // function with the same two-segment split as the inverse.
        r = if r > 0.0031308 {
            1.055 * r.powf(1.0 / 2.4) - 0.055
        } else {
            12.92 * r
        };
        g = if g > 0.0031308 {
            1.055 * g.powf(1.0 / 2.4) - 0.055
        } else {
            12.92 * g
        };
        b = if b > 0.0031308 {
            1.055 * b.powf(1.0 / 2.4) - 0.055
        } else {
            12.92 * b
        };

        // Step 5: Clamp to [0.0, 1.0], scale to [0, 255], and round to
        // the nearest integer to produce the final 8-bit sRGB values.
        RGB {
            r: (r.clamp(0.0, 1.0) * 255.0 + 0.5) as u8,
            g: (g.clamp(0.0, 1.0) * 255.0 + 0.5) as u8,
            b: (b.clamp(0.0, 1.0) * 255.0 + 0.5) as u8,
        }
    }

    /// Linearly interpolate between two LAB colors component-wise.
    /// `t` is the interpolation factor in [0, 1]: t=0 returns `a`,
    /// t=1 returns `b`, and values in between blend proportionally.
    fn lerp(t: f32, a: LAB, b: LAB) -> LAB {
        LAB {
            l: a.l + t * (b.l - a.l),
            a: a.a + t * (b.a - a.a),
            b: a.b + t * (b.b - a.b),
        }
    }
}

// PORT NOTE: the reference `comptime { assert(@bitSizeOf(RGB) == 24);
// assert(@sizeOf(RGB) == 4); }` plus the `test Special`/`test Dynamic`/`test
// "osc4"`/`next` reference tests are ported below.

#[cfg(test)]
#[allow(clippy::excessive_precision)]
// PARITY: the reference tests use index loops (`for (a..b) |i| palette[i]`) and
// `expectEqual(false, ...)`; keep them verbatim.
#[allow(clippy::needless_range_loop)]
#[allow(clippy::bool_assert_comparison)]
mod tests {
    use super::*;

    // test "parsePaletteEntry" (oracle 2970e9a2a)
    #[test]
    fn parse_palette_entry_cases() {
        let e = parse_palette_entry(b"0=#AABBCC").unwrap();
        assert_eq!(e.index, 0);
        assert_eq!(e.color, RGB { r: 170, g: 187, b: 204 });
        let e = parse_palette_entry(b"0b1=#014589").unwrap();
        assert_eq!((e.index, e.color), (1, RGB { r: 1, g: 69, b: 137 }));
        let e = parse_palette_entry(b"0o7=#234567").unwrap();
        assert_eq!((e.index, e.color), (7, RGB { r: 35, g: 69, b: 103 }));
        let e = parse_palette_entry(b"0xF=#ABCDEF").unwrap();
        assert_eq!((e.index, e.color), (15, RGB { r: 171, g: 205, b: 239 }));
        let e = parse_palette_entry(b"0 =  #AABBCC").unwrap();
        assert_eq!((e.index, e.color), (0, RGB { r: 170, g: 187, b: 204 }));
        let e = parse_palette_entry(b" 1= #DDEEFF    ").unwrap();
        assert_eq!((e.index, e.color), (1, RGB { r: 221, g: 238, b: 255 }));
        let e = parse_palette_entry(b"  2  =  #123456 ").unwrap();
        assert_eq!((e.index, e.color), (2, RGB { r: 18, g: 52, b: 86 }));
        let e = parse_palette_entry(b"1=black").unwrap();
        assert_eq!((e.index, e.color), (1, RGB { r: 0, g: 0, b: 0 }));

        assert_eq!(
            parse_palette_entry(b" "),
            Err(ParsePaletteEntryError::InvalidFormat)
        );
        assert_eq!(
            parse_palette_entry(b"a"),
            Err(ParsePaletteEntryError::InvalidFormat)
        );
        assert_eq!(
            parse_palette_entry(b"256=#AABBCC"),
            Err(ParsePaletteEntryError::Overflow)
        );
        assert_eq!(
            parse_palette_entry(b"1=notacolor"),
            Err(ParsePaletteEntryError::InvalidFormat)
        );
    }

    // new "RGB.parse" assertions (oracle 2970e9a2a)
    #[test]
    fn rgb_parse_bare_hex_and_trim() {
        assert_eq!(RGB { r: 10, g: 11, b: 12 }, RGB::parse("0A0B0C").unwrap());
        assert_eq!(
            RGB {
                r: 255,
                g: 255,
                b: 255
            },
            RGB::parse("FFFFFF").unwrap()
        );
        assert_eq!(
            RGB {
                r: 255,
                g: 255,
                b: 255
            },
            RGB::parse("FFF").unwrap()
        );
        assert_eq!(RGB { r: 51, g: 68, b: 85 }, RGB::parse("#345").unwrap());
        assert_eq!(
            RGB {
                r: 170,
                g: 187,
                b: 204
            },
            RGB::parse(" #AABBCC   ").unwrap()
        );
    }

    #[test]
    fn rgb_layout() {
        // comptime { assert(@bitSizeOf(RGB) == 24); assert(@sizeOf(RGB) == 4); }
        //
        // PORT NOTE: `@bitSizeOf(RGB) == 24` is the 3×8 data bits — preserved
        // exactly (`size_of::<RGB>() * 8`, with the Rust `#[repr(C)]` layout
        // carrying NO padding). `@sizeOf(RGB) == 4` is a Zig artifact: a
        // `packed struct(u24)` is stored in its backing integer (u32 → 4
        // bytes). The Rust `#[repr(C)]` representation of three `u8`s is 3 bytes
        // (and byte-identical to the C type `RGB.C`); we assert the true data
        // layout (3 bytes / 24 bits) here rather than the Zig backing size.
        assert_eq!(24, core::mem::size_of::<RGB>() * 8);
        assert_eq!(3, core::mem::size_of::<RGB>());
    }

    #[test]
    fn osc4() {
        // test "osc4" within Special
        assert_eq!(256, Special::Bold.osc4());
        assert_eq!(257, Special::Underline.osc4());
        assert_eq!(258, Special::Blink.osc4());
        assert_eq!(259, Special::Reverse.osc4());
        assert_eq!(260, Special::Italic.osc4());
    }

    // test Special {
    //     _ = Special;
    // }
    #[test]
    fn special() {
        // Compile-touch the decl exactly like the reference identifier-test
        // block (`_ = Special;` — refAllDecls-style touch, no assertions).
        let _ = core::any::type_name::<Special>();
    }

    #[test]
    fn next() {
        // test "next" within Dynamic
        assert_eq!(Some(Dynamic::Background), Dynamic::Foreground.next());
        assert_eq!(Some(Dynamic::Cursor), Dynamic::Background.next());
        assert_eq!(Some(Dynamic::PointerForeground), Dynamic::Cursor.next());
        assert_eq!(
            Some(Dynamic::PointerBackground),
            Dynamic::PointerForeground.next()
        );
        assert_eq!(
            Some(Dynamic::TektronixForeground),
            Dynamic::PointerBackground.next()
        );
        assert_eq!(
            Some(Dynamic::TektronixBackground),
            Dynamic::TektronixForeground.next()
        );
        assert_eq!(
            Some(Dynamic::HighlightBackground),
            Dynamic::TektronixBackground.next()
        );
        assert_eq!(
            Some(Dynamic::TektronixCursor),
            Dynamic::HighlightBackground.next()
        );
        assert_eq!(
            Some(Dynamic::HighlightForeground),
            Dynamic::TektronixCursor.next()
        );
        assert_eq!(None, Dynamic::HighlightForeground.next());
    }

    // test Dynamic {
    //     _ = Dynamic;
    // }
    #[test]
    fn dynamic() {
        // Compile-touch the decl exactly like the reference identifier-test
        // block (`_ = Dynamic;` — refAllDecls-style touch, no assertions).
        let _ = core::any::type_name::<Dynamic>();
    }

    #[test]
    fn palette_default() {
        // Safety check
        let mut i: u8 = 0;
        while i < 16 {
            assert_eq!(Name::from_int(i).default(), Ok(DEFAULT[i as usize]));
            i += 1;
        }
    }

    #[test]
    fn rgb_parse() {
        assert_eq!(Ok(RGB { r: 255, g: 0, b: 0 }), RGB::parse("rgbi:1.0/0/0"));
        assert_eq!(
            Ok(RGB {
                r: 127,
                g: 160,
                b: 0
            }),
            RGB::parse("rgb:7f/a0a0/0")
        );
        assert_eq!(
            Ok(RGB {
                r: 255,
                g: 255,
                b: 255
            }),
            RGB::parse("rgb:f/ff/fff")
        );
        assert_eq!(
            Ok(RGB {
                r: 255,
                g: 255,
                b: 255
            }),
            RGB::parse("#ffffff")
        );
        assert_eq!(
            Ok(RGB {
                r: 255,
                g: 255,
                b: 255
            }),
            RGB::parse("#fff")
        );
        assert_eq!(
            Ok(RGB {
                r: 255,
                g: 255,
                b: 255
            }),
            RGB::parse("#fffffffff")
        );
        assert_eq!(
            Ok(RGB {
                r: 255,
                g: 255,
                b: 255
            }),
            RGB::parse("#ffffffffffff")
        );
        assert_eq!(
            Ok(RGB {
                r: 255,
                g: 0,
                b: 16
            }),
            RGB::parse("#ff0010")
        );

        assert_eq!(Ok(RGB { r: 0, g: 0, b: 0 }), RGB::parse("black"));
        assert_eq!(Ok(RGB { r: 255, g: 0, b: 0 }), RGB::parse("red"));
        assert_eq!(Ok(RGB { r: 0, g: 255, b: 0 }), RGB::parse("green"));
        assert_eq!(Ok(RGB { r: 0, g: 0, b: 255 }), RGB::parse("blue"));
        assert_eq!(
            Ok(RGB {
                r: 255,
                g: 255,
                b: 255
            }),
            RGB::parse("white")
        );

        assert_eq!(
            Ok(RGB {
                r: 124,
                g: 252,
                b: 0
            }),
            RGB::parse("LawnGreen")
        );
        assert_eq!(
            Ok(RGB {
                r: 0,
                g: 250,
                b: 154
            }),
            RGB::parse("medium spring green")
        );
        assert_eq!(
            Ok(RGB {
                r: 34,
                g: 139,
                b: 34
            }),
            RGB::parse(" Forest Green ")
        );

        // Invalid format
        assert_eq!(Err(InvalidFormat), RGB::parse("rgb;"));
        assert_eq!(Err(InvalidFormat), RGB::parse("rgb:"));
        assert_eq!(Err(InvalidFormat), RGB::parse(":a/a/a"));
        assert_eq!(Err(InvalidFormat), RGB::parse("a/a/a"));
        assert_eq!(Err(InvalidFormat), RGB::parse("rgb:a/a/a/"));
        assert_eq!(Err(InvalidFormat), RGB::parse("rgb:00000///"));
        assert_eq!(Err(InvalidFormat), RGB::parse("rgb:000/"));
        assert_eq!(Err(InvalidFormat), RGB::parse("rgbi:a/a/a"));
        assert_eq!(Err(InvalidFormat), RGB::parse("rgb:0.5/0.0/1.0"));
        assert_eq!(Err(InvalidFormat), RGB::parse("rgb:not/hex/zz"));
        assert_eq!(Err(InvalidFormat), RGB::parse("#"));
        assert_eq!(Err(InvalidFormat), RGB::parse("#ff"));
        assert_eq!(Err(InvalidFormat), RGB::parse("#ffff"));
        assert_eq!(Err(InvalidFormat), RGB::parse("#fffff"));
        assert_eq!(Err(InvalidFormat), RGB::parse("#gggggg"));
    }

    #[test]
    fn dynamic_palette_init() {
        let p: DynamicPalette = DynamicPalette::init(DEFAULT);
        assert_eq!(DEFAULT, p.current);
        assert_eq!(DEFAULT, p.original);
        assert_eq!(0_usize, p.mask.count());
    }

    #[test]
    fn dynamic_palette_set() {
        let mut p: DynamicPalette = DynamicPalette::init(DEFAULT);
        let new_color = RGB { r: 255, g: 0, b: 0 };

        p.set(0, new_color);
        assert_eq!(new_color, p.current[0]);
        assert!(p.mask.is_set(0));
        assert_eq!(1_usize, p.mask.count());

        assert_eq!(DEFAULT[0], p.original[0]);
    }

    #[test]
    fn dynamic_palette_reset() {
        let mut p: DynamicPalette = DynamicPalette::init(DEFAULT);
        let new_color = RGB { r: 255, g: 0, b: 0 };

        p.set(0, new_color);
        assert!(p.mask.is_set(0));

        p.reset(0);
        assert_eq!(DEFAULT[0], p.current[0]);
        assert!(!p.mask.is_set(0));
        assert_eq!(0_usize, p.mask.count());
    }

    #[test]
    fn dynamic_palette_reset_all() {
        let mut p: DynamicPalette = DynamicPalette::init(DEFAULT);
        let new_color = RGB { r: 255, g: 0, b: 0 };

        p.set(0, new_color);
        p.set(5, new_color);
        p.set(10, new_color);
        assert_eq!(3_usize, p.mask.count());

        p.reset_all();
        assert_eq!(DEFAULT, p.current);
        assert_eq!(DEFAULT, p.original);
        assert_eq!(0_usize, p.mask.count());
    }

    #[test]
    fn dynamic_palette_change_default_with_no_changes() {
        let mut p: DynamicPalette = DynamicPalette::init(DEFAULT);
        let mut new_palette = DEFAULT;
        new_palette[0] = RGB {
            r: 100,
            g: 100,
            b: 100,
        };

        p.change_default(new_palette);
        assert_eq!(new_palette, p.original);
        assert_eq!(new_palette, p.current);
        assert_eq!(0_usize, p.mask.count());
    }

    #[test]
    fn dynamic_palette_change_default_preserves_changes() {
        let mut p: DynamicPalette = DynamicPalette::init(DEFAULT);
        let custom_color = RGB { r: 255, g: 0, b: 0 };

        p.set(5, custom_color);
        assert!(p.mask.is_set(5));

        let mut new_palette = DEFAULT;
        new_palette[0] = RGB {
            r: 100,
            g: 100,
            b: 100,
        };
        new_palette[5] = RGB {
            r: 50,
            g: 50,
            b: 50,
        };

        p.change_default(new_palette);

        assert_eq!(new_palette, p.original);
        assert_eq!(new_palette[0], p.current[0]);
        assert_eq!(custom_color, p.current[5]);
        assert!(p.mask.is_set(5));
        assert_eq!(1_usize, p.mask.count());
    }

    #[test]
    fn dynamic_palette_change_default_with_multiple_changes() {
        let mut p: DynamicPalette = DynamicPalette::init(DEFAULT);
        let red = RGB { r: 255, g: 0, b: 0 };
        let green = RGB { r: 0, g: 255, b: 0 };
        let blue = RGB { r: 0, g: 0, b: 255 };

        p.set(1, red);
        p.set(2, green);
        p.set(3, blue);

        let mut new_palette = DEFAULT;
        new_palette[0] = RGB {
            r: 50,
            g: 50,
            b: 50,
        };
        new_palette[1] = RGB {
            r: 60,
            g: 60,
            b: 60,
        };

        p.change_default(new_palette);

        assert_eq!(new_palette[0], p.current[0]);
        assert_eq!(red, p.current[1]);
        assert_eq!(green, p.current[2]);
        assert_eq!(blue, p.current[3]);
        assert_eq!(3_usize, p.mask.count());
    }

    #[test]
    fn lab_from_rgb() {
        let epsilon = 0.5;

        // White (255, 255, 255) -> L*=100, a*=0, b*=0
        let white = LAB::from_rgb(RGB {
            r: 255,
            g: 255,
            b: 255,
        });
        assert!((100.0_f32 - white.l).abs() <= epsilon);
        assert!((0.0_f32 - white.a).abs() <= epsilon);
        assert!((0.0_f32 - white.b).abs() <= epsilon);

        // Black (0, 0, 0) -> L*=0, a*=0, b*=0
        let black = LAB::from_rgb(RGB { r: 0, g: 0, b: 0 });
        assert!((0.0_f32 - black.l).abs() <= epsilon);
        assert!((0.0_f32 - black.a).abs() <= epsilon);
        assert!((0.0_f32 - black.b).abs() <= epsilon);

        // Pure red (255, 0, 0) -> L*≈53.23, a*≈80.11, b*≈67.22
        let red = LAB::from_rgb(RGB { r: 255, g: 0, b: 0 });
        assert!((53.23_f32 - red.l).abs() <= epsilon);
        assert!((80.11_f32 - red.a).abs() <= epsilon);
        assert!((67.22_f32 - red.b).abs() <= epsilon);

        // Pure green (0, 128, 0) -> L*≈46.23, a*≈-51.70, b*≈49.90
        let green = LAB::from_rgb(RGB { r: 0, g: 128, b: 0 });
        assert!((46.23_f32 - green.l).abs() <= epsilon);
        assert!((-51.70_f32 - green.a).abs() <= epsilon);
        assert!((49.90_f32 - green.b).abs() <= epsilon);

        // Pure blue (0, 0, 255) -> L*≈32.30, a*≈79.20, b*≈-107.86
        let blue = LAB::from_rgb(RGB { r: 0, g: 0, b: 255 });
        assert!((32.30_f32 - blue.l).abs() <= epsilon);
        assert!((79.20_f32 - blue.a).abs() <= epsilon);
        assert!((-107.86_f32 - blue.b).abs() <= epsilon);
    }

    #[test]
    fn generate_256_color_base16_preserved() {
        let bg = RGB { r: 0, g: 0, b: 0 };
        let fg = RGB {
            r: 255,
            g: 255,
            b: 255,
        };
        let palette = generate_256_color(DEFAULT, PaletteMask::init_empty(), bg, fg, false);

        // The first 16 colors (base16) must remain unchanged.
        for i in 0..16 {
            assert_eq!(DEFAULT[i], palette[i]);
        }
    }

    #[test]
    fn generate_256_color_cube_corners_match_base_colors() {
        let bg = RGB { r: 0, g: 0, b: 0 };
        let fg = RGB {
            r: 255,
            g: 255,
            b: 255,
        };
        let palette = generate_256_color(DEFAULT, PaletteMask::init_empty(), bg, fg, false);

        // Index 16 is cube (0,0,0) which should equal bg.
        assert_eq!(bg, palette[16]);

        // Index 231 is cube (5,5,5) which should equal fg.
        assert_eq!(fg, palette[231]);
    }

    #[test]
    fn generate_256_color_cube_corners_black_white_with_harmonious_false() {
        let black = RGB { r: 0, g: 0, b: 0 };
        let white = RGB {
            r: 255,
            g: 255,
            b: 255,
        };

        // Dark theme: bg=black, fg=white.
        let dark = generate_256_color(DEFAULT, PaletteMask::init_empty(), black, white, false);
        assert_eq!(black, dark[16]);
        assert_eq!(white, dark[231]);

        // Light theme: bg=white, fg=black. The bg/red swap ensures
        // the cube still runs from black (16) to white (231).
        let light = generate_256_color(DEFAULT, PaletteMask::init_empty(), white, black, false);
        assert_eq!(black, light[16]);
        assert_eq!(white, light[231]);
    }

    #[test]
    fn generate_256_color_light_theme_cube_corners_with_harmonious_true() {
        let white = RGB {
            r: 255,
            g: 255,
            b: 255,
        };
        let black = RGB { r: 0, g: 0, b: 0 };

        // harmonious=true skips the bg/fg swap, so the cube preserves the
        // original orientation: (0,0,0)=bg=white, (5,5,5)=fg=black.
        let palette = generate_256_color(DEFAULT, PaletteMask::init_empty(), white, black, true);
        assert_eq!(white, palette[16]);
        assert_eq!(black, palette[231]);
    }

    #[test]
    fn generate_256_color_grayscale_ramp_monotonic_luminance() {
        let bg = RGB { r: 0, g: 0, b: 0 };
        let fg = RGB {
            r: 255,
            g: 255,
            b: 255,
        };
        let palette = generate_256_color(DEFAULT, PaletteMask::init_empty(), bg, fg, false);

        // The grayscale ramp (232–255) should have monotonically increasing
        // luminance from near-black to near-white.
        let mut prev_lum: f64 = 0.0;
        for i in 232..256 {
            let lum = palette[i].luminance();
            assert!(lum >= prev_lum);
            prev_lum = lum;
        }
    }

    #[test]
    fn generate_256_color_skip_mask_preserves_original_colors() {
        let bg = RGB { r: 0, g: 0, b: 0 };
        let fg = RGB {
            r: 255,
            g: 255,
            b: 255,
        };

        // Mark a few indices as skipped; they should keep their base value.
        let mut skip: PaletteMask = PaletteMask::init_empty();
        skip.set(20);
        skip.set(100);
        skip.set(240);

        let palette = generate_256_color(DEFAULT, skip, bg, fg, false);
        assert_eq!(DEFAULT[20], palette[20]);
        assert_eq!(DEFAULT[100], palette[100]);
        assert_eq!(DEFAULT[240], palette[240]);

        // A non-skipped index in the cube should differ from the default.
        assert!(!palette[21].eql(DEFAULT[21]));
    }

    #[test]
    fn generate_256_color_dark_theme_harmonious_has_no_effect() {
        // For a dark theme (fg lighter than bg), harmonious should not change
        // the output because the inversion is only relevant for light themes.
        let bg = RGB { r: 0, g: 0, b: 0 };
        let fg = RGB {
            r: 255,
            g: 255,
            b: 255,
        };
        let normal = generate_256_color(DEFAULT, PaletteMask::init_empty(), bg, fg, false);
        let harmonious = generate_256_color(DEFAULT, PaletteMask::init_empty(), bg, fg, true);

        for i in 16..256 {
            assert_eq!(normal[i], harmonious[i]);
        }
    }

    #[test]
    fn generate_256_color_light_theme_harmonious_skips_inversion() {
        // For a light theme (fg darker than bg), harmonious=true skips the
        // bg/red swap, producing different cube colors than harmonious=false.
        let bg = RGB {
            r: 255,
            g: 255,
            b: 255,
        };
        let fg = RGB { r: 0, g: 0, b: 0 };
        let inverted = generate_256_color(DEFAULT, PaletteMask::init_empty(), bg, fg, false);
        let harmonious = generate_256_color(DEFAULT, PaletteMask::init_empty(), bg, fg, true);

        // Cube origin (0,0,0) at index 16: without harmonious, bg and red are
        // swapped so it becomes the red base; with harmonious it stays as bg.
        assert_eq!(bg, harmonious[16]);
        assert!(!inverted[16].eql(bg));

        // At least some cube colors should differ between the two modes.
        let mut differ: usize = 0;
        for i in 16..232 {
            if !inverted[i].eql(harmonious[i]) {
                differ += 1;
            }
        }
        assert!(differ > 0);
    }

    #[test]
    fn generate_256_color_light_theme_harmonious_grayscale_ramp() {
        let bg = RGB {
            r: 255,
            g: 255,
            b: 255,
        };
        let fg = RGB { r: 0, g: 0, b: 0 };

        // harmonious=false swaps bg/fg, so the ramp runs black→white (increasing).
        {
            let palette = generate_256_color(DEFAULT, PaletteMask::init_empty(), bg, fg, false);
            let mut prev_lum: f64 = 0.0;
            for i in 232..256 {
                let lum = palette[i].luminance();
                assert!(lum >= prev_lum);
                prev_lum = lum;
            }
        }

        // harmonious=true keeps original order, so the ramp runs white→black (decreasing).
        {
            let palette = generate_256_color(DEFAULT, PaletteMask::init_empty(), bg, fg, true);
            let mut prev_lum: f64 = 1.0;
            for i in 232..256 {
                let lum = palette[i].luminance();
                assert!(lum <= prev_lum);
                prev_lum = lum;
            }
        }
    }

    #[test]
    fn lab_to_rgb() {
        // Round-trip: RGB -> LAB -> RGB should recover the original values.
        let cases = [
            RGB {
                r: 255,
                g: 255,
                b: 255,
            },
            RGB { r: 0, g: 0, b: 0 },
            RGB { r: 255, g: 0, b: 0 },
            RGB { r: 0, g: 128, b: 0 },
            RGB { r: 0, g: 0, b: 255 },
            RGB {
                r: 128,
                g: 128,
                b: 128,
            },
            RGB {
                r: 64,
                g: 224,
                b: 208,
            },
        ];

        for expected in cases {
            let lab = LAB::from_rgb(expected);
            let actual = lab.to_rgb();
            assert_eq!(expected.r, actual.r);
            assert_eq!(expected.g, actual.g);
            assert_eq!(expected.b, actual.b);
        }
    }
}
