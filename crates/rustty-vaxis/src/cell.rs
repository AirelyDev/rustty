//! Port of `libvaxis` `Cell.zig` (221 LOC) — the `Cell` value struct plus its
//! nested value types `Segment`, `Character`, `CursorShape`, `Hyperlink`, `Scale`,
//! `Style` (+ `Underline`) and `Color` (+ `Kind`/`Report`/`Scheme`).
//!
//! PORT-NOTE (byte-slice representation, VAXIS_C1_DESIGN §7): Zig's byte-slice
//! fields (`Segment.text`, `Character.grapheme`, `Hyperlink.uri/params`) are
//! borrowed `[]const u8` with caller-managed lifetimes. We port them as `&'a [u8]`
//! with `&'static [u8]` defaults (the zero-copy, borrow-faithful representation),
//! NEVER a lossy `String` (zig-byteslice-to-rust-rawbytes; these are grapheme/UTF-8
//! bytes). `Cell`/`Segment`/`Character`/`Hyperlink` therefore carry the lifetime `'a`.
//!
//! PORT-NOTE (packed-struct fidelity, §7): Zig `packed struct` fields are LSB-first
//! and `@bitCast` to the backing integer is the canonical equality `std.meta.eql`
//! uses. `Scale` (u13) and the `Style` SGR bits (u7) reproduce that layout via an
//! explicit bit-pack (`to_u13` / the `SGRBits` pack in `Style::eql`).

use crate::image;

// char: Character = .{},
// style: Style = .{},
// link: Hyperlink = .{},
// image: ?Image.Placement = null,
// default: bool = false,
// wrapped: bool = ...,
// scale: Scale = .{},
// PORT-NOTE: every field's Zig default equals its Rust `Default`
// (Character/Hyperlink/Style/Scale carry Default impls, image=None, bools=false), so
// the whole-struct default is `derive(Default)` — faithful to the Zig `.{}` literal.
#[derive(Clone, Copy, Debug, Default)]
pub struct Cell<'a> {
    pub char: Character<'a>,
    pub style: Style,
    pub link: Hyperlink<'a>,
    pub image: Option<image::Placement>,
    pub default: bool,
    /// Set to true if this cell is the last cell printed in a row before wrap. Vaxis will determine if
    /// it should rely on the terminal's autowrap feature which can help with primary screen resizes
    pub wrapped: bool,
    pub scale: Scale,
}

/// Segment is a contiguous run of text that has a constant style
#[derive(Clone, Copy, Debug)]
pub struct Segment<'a> {
    pub text: &'a [u8],
    pub style: Style,
    pub link: Hyperlink<'a>,
}

#[derive(Clone, Copy, Debug)]
pub struct Character<'a> {
    pub grapheme: &'a [u8],
    /// width should only be provided when the application is sure the terminal
    /// will measure the same width. This can be ensure by using the gwidth method
    /// included in libvaxis. If width is 0, libvaxis will measure the glyph at
    /// render time
    pub width: u8,
}

impl Default for Character<'_> {
    fn default() -> Self {
        // grapheme: []const u8 = " ", width: u8 = 1
        Character {
            grapheme: b" ",
            width: 1,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CursorShape {
    Default,
    BlockBlink,
    Block,
    UnderlineBlink,
    Underline,
    BeamBlink,
    Beam,
}

#[derive(Clone, Copy, Debug)]
pub struct Hyperlink<'a> {
    pub uri: &'a [u8],
    /// ie "id=app-1234"
    pub params: &'a [u8],
}

impl Default for Hyperlink<'_> {
    fn default() -> Self {
        // uri: []const u8 = "", params: []const u8 = ""
        Hyperlink {
            uri: b"",
            params: b"",
        }
    }
}

/// `vertical_alignment: enum(u2)` — the `Scale` vertical alignment (u2, LSB bits 11-12).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VerticalAlignment {
    Top = 0,
    Bottom = 1,
    Center = 2,
}

// pub const Scale = packed struct { scale: u3 = 1, numerator: u4 = 1,
//   denominator: u4 = 1, vertical_alignment: enum(u2) {...} = .top };
// LSB-first: scale bits 0-2, numerator bits 3-6, denominator bits 7-10,
// vertical_alignment bits 11-12. Default bitcast = 1 | (1<<3) | (1<<7) | (0<<11) = 137.
#[derive(Clone, Copy, Debug)]
pub struct Scale {
    pub scale: u8,
    // The spec allows up to 15, but we limit to 7
    pub numerator: u8,
    // The spec allows up to 15, but we limit to 7
    pub denominator: u8,
    pub vertical_alignment: VerticalAlignment,
}

impl Default for Scale {
    fn default() -> Self {
        Scale {
            scale: 1,
            numerator: 1,
            denominator: 1,
            vertical_alignment: VerticalAlignment::Top,
        }
    }
}

impl Scale {
    /// `@bitCast(Scale) -> u13` — the LSB-first packed bit layout (§7). Held in a
    /// `u16` since Rust has no `u13`; only the low 13 bits are populated.
    fn to_u13(self) -> u16 {
        (self.scale as u16 & 0b111)
            | ((self.numerator as u16 & 0b1111) << 3)
            | ((self.denominator as u16 & 0b1111) << 7)
            | ((self.vertical_alignment as u16 & 0b11) << 11)
    }

    pub fn eql(self, other: Scale) -> bool {
        let a_scale: u16 = self.to_u13();
        let b_scale: u16 = other.to_u13();
        a_scale == b_scale
    }
}

/// `Style.Underline` — the underline style (off by default).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Underline {
    Off,
    Single,
    Double,
    Curly,
    Dotted,
    Dashed,
}

// pub const Style = struct { fg/bg/ul: Color = .default, ul_style: Underline = .off,
//   bold/dim/italic/blink/reverse/invisible/strikethrough: bool = false };
#[derive(Clone, Copy, Debug)]
pub struct Style {
    pub fg: Color,
    pub bg: Color,
    pub ul: Color,
    pub ul_style: Underline,

    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub blink: bool,
    pub reverse: bool,
    pub invisible: bool,
    pub strikethrough: bool,
}

impl Default for Style {
    fn default() -> Self {
        Style {
            fg: Color::Default,
            bg: Color::Default,
            ul: Color::Default,
            ul_style: Underline::Off,
            bold: false,
            dim: false,
            italic: false,
            blink: false,
            reverse: false,
            invisible: false,
            strikethrough: false,
        }
    }
}

impl Style {
    pub fn eql(a: Style, b: Style) -> bool {
        // SGRBits — the 7 SGR bools packed LSB-first into a u7 (bold=bit0 …
        // strikethrough=bit6); `@bitCast`-equality is `std.meta.eql` over the packed struct.
        fn sgr_bits(s: &Style) -> u8 {
            (s.bold as u8)
                | ((s.dim as u8) << 1)
                | ((s.italic as u8) << 2)
                | ((s.blink as u8) << 3)
                | ((s.reverse as u8) << 4)
                | ((s.invisible as u8) << 5)
                | ((s.strikethrough as u8) << 6)
        }
        let a_sgr: u8 = sgr_bits(&a);
        let b_sgr: u8 = sgr_bits(&b);
        a_sgr == b_sgr
            && Color::eql(a.fg, b.fg)
            && Color::eql(a.bg, b.bg)
            && Color::eql(a.ul, b.ul)
            && a.ul_style == b.ul_style
    }
}

// pub const Color = union(enum) { default, index: u8, rgb: [3]u8 };
#[derive(Clone, Copy, Debug)]
pub enum Color {
    Default,
    Index(u8),
    Rgb([u8; 3]),
}

impl Color {
    pub fn eql(a: Color, b: Color) -> bool {
        match a {
            Color::Default => matches!(b, Color::Default),
            Color::Index(a_idx) => match b {
                Color::Index(b_idx) => a_idx == b_idx,
                _ => false,
            },
            Color::Rgb(a_rgb) => match b {
                Color::Rgb(b_rgb) => {
                    a_rgb[0] == b_rgb[0] && a_rgb[1] == b_rgb[1] && a_rgb[2] == b_rgb[2]
                }
                _ => false,
            },
        }
    }

    // rgbFromUint(val: u24) — Rust has no u24; take a u32 constrained to 24 bits and
    // keep the mask/shift semantics exact.
    pub fn rgb_from_uint(val: u32) -> Color {
        let r_bits = val & 0b11111111_00000000_00000000;
        let g_bits = val & 0b00000000_11111111_00000000;
        let b_bits = val & 0b00000000_00000000_11111111;
        let rgb = [(r_bits >> 16) as u8, (g_bits >> 8) as u8, b_bits as u8];
        Color::Rgb(rgb)
    }

    /// parse an XParseColor-style rgb specification into an rgb Color. The spec
    /// is of the form: rgb:rrrr/gggg/bbbb. Generally, the high two bits will always
    /// be the same as the low two bits.
    pub fn rgb_from_spec(spec: &[u8]) -> Result<Color, ColorSpecError> {
        let mut iter = split_scalar(spec, b':');
        let prefix = iter.next().ok_or(ColorSpecError::InvalidColorSpec)?;
        if b"rgb" != prefix {
            return Err(ColorSpecError::InvalidColorSpec);
        }

        let spec_str = iter.next().ok_or(ColorSpecError::InvalidColorSpec)?;

        let mut spec_iter = split_scalar(spec_str, b'/');

        let r_raw = spec_iter.next().ok_or(ColorSpecError::InvalidColorSpec)?;
        if r_raw.len() != 4 {
            return Err(ColorSpecError::InvalidColorSpec);
        }

        let g_raw = spec_iter.next().ok_or(ColorSpecError::InvalidColorSpec)?;
        if g_raw.len() != 4 {
            return Err(ColorSpecError::InvalidColorSpec);
        }

        let b_raw = spec_iter.next().ok_or(ColorSpecError::InvalidColorSpec)?;
        if b_raw.len() != 4 {
            return Err(ColorSpecError::InvalidColorSpec);
        }

        let r = parse_unsigned_u8(&r_raw[2..], 16)?;
        let g = parse_unsigned_u8(&g_raw[2..], 16)?;
        let b = parse_unsigned_u8(&b_raw[2..], 16)?;

        Ok(Color::Rgb([r, g, b]))
    }
}

// pub const Kind = union(enum) { fg, bg, cursor, index: u8 };
// PORT-NOTE (Chunk 3a): `PartialEq`/`Eq` added so the Parser color-report tests can
// `assert_eq!` a whole `Report`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Fg,
    Bg,
    Cursor,
    Index(u8),
}

/// Returned when querying a color from the terminal
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Report {
    pub kind: Kind,
    pub value: [u8; 3],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scheme {
    Dark,
    Light,
}

/// Error returned by [`Color::rgb_from_spec`] — the `error.InvalidColorSpec` sentinel
/// plus the `std.fmt.parseUnsigned` error surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorSpecError {
    InvalidColorSpec,
    /// `std.fmt.parseUnsigned` failed (invalid digit / overflow).
    InvalidCharacter,
    Overflow,
}

/// Faithful port of `std.mem.splitScalar` (over `u8`): yields the byte segments
/// between (and around) occurrences of `delim`. The first `next()` is never `None`;
/// after the final segment the iterator returns `None`.
struct SplitScalar<'a> {
    buf: &'a [u8],
    delim: u8,
    index: Option<usize>,
}

impl<'a> SplitScalar<'a> {
    fn next(&mut self) -> Option<&'a [u8]> {
        let start = self.index?;
        let end = match self.buf[start..].iter().position(|&c| c == self.delim) {
            Some(off) => {
                let delim_start = start + off;
                self.index = Some(delim_start + 1);
                delim_start
            }
            None => {
                self.index = None;
                self.buf.len()
            }
        };
        Some(&self.buf[start..end])
    }
}

fn split_scalar(buf: &[u8], delim: u8) -> SplitScalar<'_> {
    SplitScalar {
        buf,
        delim,
        index: Some(0),
    }
}

/// Faithful port of `std.fmt.parseUnsigned(u8, bytes, base)` for the bases used here:
/// each byte is a base-`base` digit; the accumulator overflows into `Overflow`.
fn parse_unsigned_u8(bytes: &[u8], base: u8) -> Result<u8, ColorSpecError> {
    if bytes.is_empty() {
        return Err(ColorSpecError::InvalidCharacter);
    }
    let mut x: u8 = 0;
    for &c in bytes {
        let digit = char_to_digit(c, base).ok_or(ColorSpecError::InvalidCharacter)?;
        x = x
            .checked_mul(base)
            .ok_or(ColorSpecError::Overflow)?
            .checked_add(digit)
            .ok_or(ColorSpecError::Overflow)?;
    }
    Ok(x)
}

fn char_to_digit(c: u8, base: u8) -> Option<u8> {
    let value = match c {
        b'0'..=b'9' => c - b'0',
        b'A'..=b'Z' => c - b'A' + 10,
        b'a'..=b'z' => c - b'a' + 10,
        _ => return None,
    };
    if value < base {
        Some(value)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgb_from_spec() {
        let spec = b"rgb:aaaa/bbbb/cccc";
        let actual = Color::rgb_from_spec(spec).unwrap();
        match actual {
            Color::Rgb(rgb) => {
                assert_eq!(0xAA, rgb[0]);
                assert_eq!(0xBB, rgb[1]);
                assert_eq!(0xCC, rgb[2]);
            }
            _ => panic!("expected rgb"),
        }
    }
}
