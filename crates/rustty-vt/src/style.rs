//! rustty-vt style
//!
//! PORTS: ghostty src/terminal/style.zig
//!
//! PORT STATUS: done (vt-2b wired the previously-deferred page.Cell- and
//! RefCountedSet-dependent seams: `bg`, `bgCell`, the `Set` alias + its 2 tests)
//!
//! PORT NOTE: the reference imports are
//!   const std = @import("std");
//!   const assert = @import("../quirks.zig").inlineAssert;
//!   const color = @import("color.zig");
//!   const sgr = @import("sgr.zig");
//!   const page = @import("page.zig");
//!   const size = @import("size.zig");
//!   const Offset = size.Offset;
//!   const OffsetBuf = size.OffsetBuf;
//!   const RefCountedSet = @import("ref_counted_set.zig").RefCountedSet;
//! `page` (Cell) and `ref_counted_set` (RefCountedSet) are ported by later vt
//! sub-batches. To avoid pulling in unported siblings (R3), the page.Cell-
//! dependent methods (`bg`, `bgCell`) and the `Set` alias plus its two tests are
//! left as documented DEFERRED SEAMS below. Everything that depends only on
//! color/sgr/size (the leaf primitives) is fully ported here.

use crate::color;
use crate::sgr;
use rustty_render_types::term_size as size;

/// The unique identifier for a style. This is at most the number of cells
/// that can fit into a terminal page.
pub type Id = size::StyleCountInt;

/// The Id to use for default styling.
pub const DEFAULT_ID: Id = 0;

/// The style attributes for a cell.
#[derive(Clone, Copy, Debug)]
pub struct Style {
    /// Various colors, all self-explanatory.
    pub fg_color: Color,
    pub bg_color: Color,
    pub underline_color: Color,

    /// On/off attributes that don't require much bit width so we use
    /// a packed struct to make this take up significantly less space.
    pub flags: Flags,
}

impl Default for Style {
    // PORT NOTE: reference field defaults `*_color = .none`, `flags = .{}`.
    fn default() -> Self {
        Style {
            fg_color: Color::None,
            bg_color: Color::None,
            underline_color: Color::None,
            flags: Flags::default(),
        }
    }
}

/// On/off attributes packed into 16 bits.
// PORT NOTE: the reference is `packed struct(u16)` with 8 bools (bits 0..7), an
// `underline: sgr.Attribute.Underline` (u3, bits 8..10), and `_padding: u5`
// (bits 11..15). The bit layout matters because `PackedStyle` re-interprets
// these bytes for hashing; we expose `to_u16`/`from_u16` honoring that exact
// layout and compare via the packed integer (Zig packed-struct equality).
#[derive(Clone, Copy, Debug)]
pub struct Flags {
    pub bold: bool,
    pub italic: bool,
    pub faint: bool,
    pub blink: bool,
    pub inverse: bool,
    pub invisible: bool,
    pub strikethrough: bool,
    pub overline: bool,
    pub underline: sgr::Underline,
    // _padding: u5 = 0,
}

impl Default for Flags {
    // PORT NOTE: reference defaults all bools `false`, `underline = .none`.
    fn default() -> Self {
        Flags {
            bold: false,
            italic: false,
            faint: false,
            blink: false,
            inverse: false,
            invisible: false,
            strikethrough: false,
            overline: false,
            underline: sgr::Underline::None,
        }
    }
}

impl Flags {
    /// The 16-bit packed representation (Zig packed-struct bit layout).
    pub fn to_u16(self) -> u16 {
        (self.bold as u16)
            | (self.italic as u16) << 1
            | (self.faint as u16) << 2
            | (self.blink as u16) << 3
            | (self.inverse as u16) << 4
            | (self.invisible as u16) << 5
            | (self.strikethrough as u16) << 6
            | (self.overline as u16) << 7
            | (self.underline as u16) << 8
        // bits 11..15: _padding (always 0)
    }
}

impl PartialEq for Flags {
    // PORT NOTE: `self.flags == other.flags` in the reference is packed-struct
    // (integer) equality; we compare the packed representation.
    fn eq(&self, other: &Self) -> bool {
        self.to_u16() == other.to_u16()
    }
}
impl Eq for Flags {}

/// The color for an SGR attribute. A color can come from multiple
/// sources so we use this to track the source plus color value so that
/// we can properly react to things like palette changes.
// PORT NOTE: the reference is `union(Tag)` with `Tag = enum(u8) { none, palette,
// rgb }`. Ported as a Rust enum; the explicit `Tag` u8 values are reproduced by
// `ColorTag` below (used by PackedStyle).
#[derive(Clone, Copy, Debug)]
pub enum Color {
    None,
    Palette(u8),
    Rgb(color::RGB),
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ColorTag {
    None = 0,
    Palette = 1,
    Rgb = 2,
}

impl Color {
    fn tag(self) -> ColorTag {
        match self {
            Color::None => ColorTag::None,
            Color::Palette(_) => ColorTag::Palette,
            Color::Rgb(_) => ColorTag::Rgb,
        }
    }

    /// True if the color is equal to another color.
    pub fn eql(self, other: Color) -> bool {
        self.tag() == other.tag()
            && match self {
                Color::None => true,
                Color::Palette(p) => match other {
                    Color::Palette(op) => p == op,
                    _ => unreachable!(),
                },
                Color::Rgb(rgb) => match other {
                    Color::Rgb(orgb) => rgb == orgb,
                    _ => unreachable!(),
                },
            }
    }

    /// Formatting to make debug logs easier to read
    /// by only including non-default attributes.
    // PORT NOTE: the reference `format` writes "Color.none" / "Color.palette{ N }"
    // / "Color.rgb{ r, g, b }". Ported via `core::fmt::Write`.
    pub fn format<W: core::fmt::Write>(self, writer: &mut W) -> core::fmt::Result {
        match self {
            Color::None => writer.write_str("Color.none"),
            Color::Palette(p) => write!(writer, "Color.palette{{ {} }}", p),
            Color::Rgb(rgb) => write!(writer, "Color.rgb{{ {}, {}, {} }}", rgb.r, rgb.g, rgb.b),
        }
    }
}

impl Style {
    /// True if the style is the default style.
    #[inline]
    pub fn default_style(&self) -> bool {
        self.eql(&Style::default())
    }

    /// True if the style is equal to another style.
    pub fn eql(&self, other: &Style) -> bool {
        self.flags == other.flags
            && self.fg_color.eql(other.fg_color)
            && self.bg_color.eql(other.bg_color)
            && self.underline_color.eql(other.underline_color)
    }

    /// Returns the bg color for a cell with this style given the cell
    /// that has this style and the palette to use.
    ///
    /// Note that generally if a cell is a color-only cell, it SHOULD
    /// only have the default style, but this is meant to work with the
    /// default style as well.
    // PORT NOTE (vt-2b seam wired): `page.Cell` is now ported, so this is the
    // verbatim translation of the reference `bg`.
    pub fn bg(self, cell: &crate::page::Cell, palette: &color::Palette) -> Option<color::RGB> {
        match cell.content_tag() {
            crate::page::ContentTag::BgColorPalette => {
                Some(palette[cell.content_color_palette() as usize])
            }
            crate::page::ContentTag::BgColorRgb => {
                let rgb = cell.content_color_rgb();
                Some(color::RGB {
                    r: rgb.r,
                    g: rgb.g,
                    b: rgb.b,
                })
            }

            _ => match self.bg_color {
                Color::None => None,
                Color::Palette(idx) => Some(palette[idx as usize]),
                Color::Rgb(rgb) => Some(rgb),
            },
        }
    }

    /// Returns the fg color for a cell with this style given the palette
    /// and various configuration options.
    // PORT NOTE: `BoldColor`/`Fg` are hoisted out of `Style` to top-level types
    // (see below).
    pub fn fg(self, opts: Fg) -> color::RGB {
        // Note we don't pull the bold check to the top-level here because
        // we don't want to duplicate the conditional multiple times since
        // certain colors require more checks (e.g. `bold_is_bright`).

        match self.fg_color {
            Color::None => {
                if self.flags.bold {
                    if let Some(bold) = opts.bold {
                        match bold {
                            BoldColor::Bright => {}
                            BoldColor::Color(v) => return v,
                        }
                    }
                }

                opts.default
            }

            Color::Palette(idx) => {
                if self.flags.bold && opts.bold.is_some() {
                    let bright_offset = color::Name::BrightBlack.cval();
                    if idx < bright_offset {
                        return opts.palette[(idx + bright_offset) as usize];
                    }
                }

                opts.palette[idx as usize]
            }

            Color::Rgb(rgb) => {
                if self.flags.bold && rgb.eql(opts.default) {
                    if let Some(bold) = opts.bold {
                        match bold {
                            BoldColor::Color(v) => return v,
                            BoldColor::Bright => {}
                        }
                    }
                }

                rgb
            }
        }
    }

    /// Returns the underline color for this style.
    pub fn underline_color(self, palette: &color::Palette) -> Option<color::RGB> {
        match self.underline_color {
            Color::None => None,
            Color::Palette(idx) => Some(palette[idx as usize]),
            Color::Rgb(rgb) => Some(rgb),
        }
    }

    /// Returns a bg-color only cell from this style, if it exists.
    // PORT NOTE (vt-2b seam wired): `page.Cell` is now ported, so this is the
    // verbatim translation of the reference `bgCell`.
    pub fn bg_cell(self) -> Option<crate::page::Cell> {
        match self.bg_color {
            Color::None => None,
            Color::Palette(idx) => {
                let mut cell = crate::page::Cell::zeroed();
                cell.set_content_tag(crate::page::ContentTag::BgColorPalette);
                cell.set_content_color_palette(idx);
                Some(cell)
            }
            Color::Rgb(rgb) => {
                let mut cell = crate::page::Cell::zeroed();
                cell.set_content_tag(crate::page::ContentTag::BgColorRgb);
                cell.set_content_color_rgb(crate::page::CellRgb {
                    r: rgb.r,
                    g: rgb.g,
                    b: rgb.b,
                });
                Some(cell)
            }
        }
    }

    /// Formatting to make debug logs easier to read
    /// by only including non-default attributes.
    // PORT NOTE: the reference `format` uses comptime `std.meta.fields` reflection
    // to print only non-default fields. We port the same behavior explicitly
    // (the field set is fixed), preserving the "Style{ ... }" / "flags={ ... }"
    // shape.
    pub fn format<W: core::fmt::Write>(&self, writer: &mut W) -> core::fmt::Result {
        let dflt = Style::default();

        writer.write_str("Style{ ")?;

        let mut started = false;

        // PORT NOTE: the reference iterates fields in declaration order:
        // fg_color, bg_color, underline_color, flags. `flags` gets special
        // nested handling; the colors print as `name=<value>` when non-default.
        if !self.fg_color.eql(dflt.fg_color) {
            if started {
                writer.write_str(", ")?;
            }
            write!(writer, "fg_color=")?;
            self.fg_color.format(writer)?;
            started = true;
        }
        if !self.bg_color.eql(dflt.bg_color) {
            if started {
                writer.write_str(", ")?;
            }
            write!(writer, "bg_color=")?;
            self.bg_color.format(writer)?;
            started = true;
        }
        if !self.underline_color.eql(dflt.underline_color) {
            if started {
                writer.write_str(", ")?;
            }
            write!(writer, "underline_color=")?;
            self.underline_color.format(writer)?;
            started = true;
        }

        // flags={ ... } — only the non-default flag fields.
        {
            if started {
                writer.write_str(", ")?;
            }
            writer.write_str("flags={ ")?;
            started = false;

            if self.flags.bold {
                writer.write_str("bold")?;
                started = true;
            }
            for (name, v) in [
                ("italic", self.flags.italic),
                ("faint", self.flags.faint),
                ("blink", self.flags.blink),
                ("inverse", self.flags.inverse),
                ("invisible", self.flags.invisible),
                ("strikethrough", self.flags.strikethrough),
                ("overline", self.flags.overline),
            ] {
                if v {
                    if started {
                        writer.write_str(", ")?;
                    }
                    write!(writer, "{}", name)?;
                    started = true;
                }
            }
            if self.flags.underline != dflt.flags.underline {
                if started {
                    writer.write_str(", ")?;
                }
                write!(writer, "underline={:?}", self.flags.underline)?;
            }
            writer.write_str(" }")?;
        }

        writer.write_str(" }")
    }

    /// Returns a formatter that renders this style as VT sequences,
    /// to be used with `{f}`. This always resets the style first `\x1b[0m`
    /// since a style is meant to be fully self-contained.
    ///
    /// For individual styles, this always emits multiple SGR sequences
    /// (i.e. an individual `\x1b[<stuff>m` for each attribute) rather than
    /// trying to combine them into a single sequence. We do this because
    /// terminals have varying levels of support for combined sequences
    /// especially with mixed separators (e.g. `:` vs `;`).
    pub fn formatter_vt(&self) -> VtFormatter<'_> {
        VtFormatter {
            style: self,
            palette: None,
        }
    }

    /// Returns a formatter that renders this style as inline CSS properties,
    /// to be used with `{f}`. The output is a valid CSS style string suitable
    /// for use in a `style` attribute (e.g., "color: rgb(255, 0, 0); font-weight: bold;").
    ///
    /// Palette colors are emitted as CSS variables like `var(--vt-palette-N)`.
    pub fn formatter_html(&self) -> HtmlFormatter<'_> {
        HtmlFormatter {
            style: self,
            palette: None,
        }
    }

    /// Compute the style hash.
    pub fn hash(&self) -> u64 {
        // We pack the style in to 128 bits, fold it to 64 bits,
        // then use std.hash.int to make it sufficiently uniform.
        let packed_style = PackedStyle::from_style(*self);
        // const wide: [2]u64 = @bitCast(packed_style);
        let wide: [u64; 2] = packed_style.as_u64_pair();
        hash_int_u64(wide[0] ^ wide[1])
    }
}

/// The color to use for bold text. This avoids a dependency on the
/// config module by using terminal-native color types.
// PORT NOTE: hoisted from inside `Style` (Rust has no nested type decls); the
// reference is `pub const BoldColor = union(enum) { color: color.RGB, bright };`.
#[derive(Clone, Copy, Debug)]
pub enum BoldColor {
    Color(color::RGB),
    Bright,
}

/// Options for `Style::fg`.
// PORT NOTE: hoisted from `Style.Fg`.
#[derive(Clone, Copy)]
pub struct Fg<'a> {
    /// The default color to use if the style doesn't specify a
    /// foreground color and no configuration options override
    /// it.
    pub default: color::RGB,

    /// The current color palette. Required to map palette indices to
    /// real color values.
    pub palette: &'a color::Palette,

    /// If specified, the color to use for bold text.
    pub bold: Option<BoldColor>,
}

/// VT-sequence formatter for a `Style`.
// PORT NOTE: hoisted from `Style.VTFormatter`.
#[derive(Clone, Copy)]
pub struct VtFormatter<'a> {
    pub style: &'a Style,

    /// If set, palette colors will be emitted as RGB values instead of
    /// palette indices. This is useful when you want to capture the
    /// exact colors at formatting time rather than relying on the
    /// terminal's palette.
    pub palette: Option<&'a color::Palette>,
}

impl<'a> VtFormatter<'a> {
    pub fn format<W: core::fmt::Write>(&self, writer: &mut W) -> core::fmt::Result {
        // Always reset the style. Styles are fully self-contained.
        // Even if this style is empty, then that means we want to go
        // back to the default.
        writer.write_str("\x1b[0m")?;

        // Our flags
        if self.style.flags.bold {
            writer.write_str("\x1b[1m")?;
        }
        if self.style.flags.faint {
            writer.write_str("\x1b[2m")?;
        }
        if self.style.flags.italic {
            writer.write_str("\x1b[3m")?;
        }
        if self.style.flags.blink {
            writer.write_str("\x1b[5m")?;
        }
        if self.style.flags.inverse {
            writer.write_str("\x1b[7m")?;
        }
        if self.style.flags.invisible {
            writer.write_str("\x1b[8m")?;
        }
        if self.style.flags.strikethrough {
            writer.write_str("\x1b[9m")?;
        }
        if self.style.flags.overline {
            writer.write_str("\x1b[53m")?;
        }
        match self.style.flags.underline {
            sgr::Underline::None => {}
            sgr::Underline::Single => writer.write_str("\x1b[4m")?,
            sgr::Underline::Double => writer.write_str("\x1b[4:2m")?,
            sgr::Underline::Curly => writer.write_str("\x1b[4:3m")?,
            sgr::Underline::Dotted => writer.write_str("\x1b[4:4m")?,
            sgr::Underline::Dashed => writer.write_str("\x1b[4:5m")?,
        }

        // Various RGB colors.
        self.format_color(writer, 38, self.style.fg_color)?;
        self.format_color(writer, 48, self.style.bg_color)?;
        self.format_color(writer, 58, self.style.underline_color)?;
        Ok(())
    }

    fn format_color<W: core::fmt::Write>(
        &self,
        writer: &mut W,
        prefix: u8,
        value: Color,
    ) -> core::fmt::Result {
        match value {
            Color::None => Ok(()),
            Color::Palette(idx) => {
                if let Some(p) = self.palette {
                    let rgb = p[idx as usize];
                    write!(writer, "\x1b[{};2;{};{};{}m", prefix, rgb.r, rgb.g, rgb.b)
                } else {
                    write!(writer, "\x1b[{};5;{}m", prefix, idx)
                }
            }
            Color::Rgb(rgb) => {
                write!(writer, "\x1b[{};2;{};{};{}m", prefix, rgb.r, rgb.g, rgb.b)
            }
        }
    }
}

/// HTML/CSS formatter for a `Style`.
// PORT NOTE: hoisted from `Style.HtmlFormatter`.
#[derive(Clone, Copy)]
pub struct HtmlFormatter<'a> {
    pub style: &'a Style,

    /// If set, palette colors will be emitted as RGB values instead of
    /// CSS variables. This is useful when you want to capture the exact
    /// colors at formatting time rather than relying on CSS variables.
    pub palette: Option<&'a color::Palette>,
}

impl<'a> HtmlFormatter<'a> {
    pub fn format<W: core::fmt::Write>(&self, writer: &mut W) -> core::fmt::Result {
        // Colors
        self.format_color(writer, "color", self.style.fg_color)?;
        self.format_color(writer, "background-color", self.style.bg_color)?;
        self.format_color(writer, "text-decoration-color", self.style.underline_color)?;

        // Text decoration line
        let has_line = self.style.flags.underline != sgr::Underline::None
            || self.style.flags.strikethrough
            || self.style.flags.overline
            || self.style.flags.blink;
        if has_line {
            writer.write_str("text-decoration-line:")?;
            if self.style.flags.underline != sgr::Underline::None {
                writer.write_str(" underline")?;
            }
            if self.style.flags.strikethrough {
                writer.write_str(" line-through")?;
            }
            if self.style.flags.overline {
                writer.write_str(" overline")?;
            }
            if self.style.flags.blink {
                writer.write_str(" blink")?;
            }
            writer.write_str(";")?;
        }

        // Text decoration style
        match self.style.flags.underline {
            sgr::Underline::None => {}
            sgr::Underline::Single => writer.write_str("text-decoration-style: solid;")?,
            sgr::Underline::Double => writer.write_str("text-decoration-style: double;")?,
            sgr::Underline::Curly => writer.write_str("text-decoration-style: wavy;")?,
            sgr::Underline::Dotted => writer.write_str("text-decoration-style: dotted;")?,
            sgr::Underline::Dashed => writer.write_str("text-decoration-style: dashed;")?,
        }

        if self.style.flags.bold {
            writer.write_str("font-weight: bold;")?;
        }
        if self.style.flags.italic {
            writer.write_str("font-style: italic;")?;
        }
        if self.style.flags.faint {
            writer.write_str("opacity: 0.5;")?;
        }
        if self.style.flags.invisible {
            writer.write_str("visibility: hidden;")?;
        }
        if self.style.flags.inverse {
            writer.write_str("filter: invert(100%);")?;
        }
        Ok(())
    }

    fn format_color<W: core::fmt::Write>(
        &self,
        writer: &mut W,
        property: &str,
        c: Color,
    ) -> core::fmt::Result {
        match c {
            Color::None => Ok(()),
            Color::Palette(idx) => {
                if let Some(p) = self.palette {
                    let rgb = p[idx as usize];
                    write!(
                        writer,
                        "{}: rgb({}, {}, {});",
                        property, rgb.r, rgb.g, rgb.b
                    )
                } else {
                    write!(writer, "{}: var(--vt-palette-{});", property, idx)
                }
            }
            Color::Rgb(rgb) => {
                write!(
                    writer,
                    "{}: rgb({}, {}, {});",
                    property, rgb.r, rgb.g, rgb.b
                )
            }
        }
    }
}

/// `PackedStyle` represents the same data as `Style` but without padding,
/// which is necessary for hashing via re-interpretation of the underlying
/// bytes.
///
/// `Style` is still preferred for everything else as it has type-safety
/// when using the `Color` tagged union.
///
/// Empirical testing shows that storing all of the tags first and then the
/// data provides a better layout for serializing into and is faster on
/// benchmarks.
// PORT NOTE: the reference is `packed struct(u128)`:
//   tags:   3 × Color.Tag (u8)        = 24 bits  (bits 0..23)
//   data:   3 × Data (u24)            = 72 bits  (bits 24..95)
//   flags:  Flags (u16)               = 16 bits  (bits 96..111)
//   _padding: u16                     = 16 bits  (bits 112..127)
// Zig packed structs lay out least-significant-first. We assemble the 128-bit
// value with that exact ordering and expose it as a `[u64; 2]` for hashing.
struct PackedStyle {
    bits: u128,
}

impl PackedStyle {
    // /// After https://github.com/ziglang/zig/issues/19754 is implemented,
    // /// it will be an compiler-error to have packed union fields of
    // /// differing size.
    // ///
    // /// For now we just need to be careful not to accidentally introduce
    // /// padding.
    // const Data = packed union {
    //     none: u24,
    //     palette: packed struct(u24) { idx: u8, _padding: u16 = 0 },
    //     rgb: color.RGB,
    //     fn fromColor(c: Color) Data { ... }
    // };
    //
    // PORT NOTE: `Data` is a 24-bit union. For each color tag the 24 data bits
    // are: none => 0, palette => idx in low 8 bits (high 16 padding), rgb =>
    // r|g<<8|b<<16. `data_from_color` reproduces this.
    fn data_from_color(c: Color) -> u32 {
        match c {
            Color::None => 0,
            Color::Palette(idx) => idx as u32, // idx in low 8 bits, high 16 = 0
            Color::Rgb(rgb) => (rgb.r as u32) | (rgb.g as u32) << 8 | (rgb.b as u32) << 16,
        }
    }

    #[inline]
    fn from_style(style: Style) -> PackedStyle {
        let fg_tag = style.fg_color.tag() as u8;
        let bg_tag = style.bg_color.tag() as u8;
        let ul_tag = style.underline_color.tag() as u8;

        let fg_data = PackedStyle::data_from_color(style.fg_color) & 0xFF_FFFF;
        let bg_data = PackedStyle::data_from_color(style.bg_color) & 0xFF_FFFF;
        let ul_data = PackedStyle::data_from_color(style.underline_color) & 0xFF_FFFF;

        let flags = style.flags.to_u16();

        let mut bits: u128 = 0;
        // tags (bits 0..23): fg, bg, underline
        bits |= fg_tag as u128; // << 0
        bits |= (bg_tag as u128) << 8;
        bits |= (ul_tag as u128) << 16;
        // data (bits 24..95): fg, bg, underline (24 bits each)
        bits |= (fg_data as u128) << 24;
        bits |= (bg_data as u128) << 48;
        bits |= (ul_data as u128) << 72;
        // flags (bits 96..111)
        bits |= (flags as u128) << 96;
        // _padding (bits 112..127): 0

        PackedStyle { bits }
    }

    /// `@bitCast([2]u64)` — little-endian split of the 128-bit value.
    fn as_u64_pair(&self) -> [u64; 2] {
        [self.bits as u64, (self.bits >> 64) as u64]
    }
}

// PORT NOTE (vt-2b seam wired): `ref_counted_set.zig` is now ported, so the
// `Set` alias and its hash/eql context are live. The reference is:
//   pub const Set = RefCountedSet(
//       Style,
//       Id,
//       size.CellCountInt,
//       struct {
//           pub fn hash(self: *const @This(), style: Style) u64 {
//               _ = self;
//               return style.hash();
//           }
//           pub fn eql(self: *const @This(), a: Style, b: Style) bool {
//               _ = self;
//               return a.eql(b);
//           }
//       },
//   );
/// The context for the style `Set`. It is stateless (the reference's anonymous
/// struct has no fields); `hash`/`eql` forward to `Style.hash`/`Style.eql`.
#[derive(Default, Clone, Copy)]
pub struct SetContext;

impl crate::ref_counted_set::RefCountedSetContext<Style> for SetContext {
    fn hash(&self, style: Style) -> u64 {
        // _ = self; return style.hash();
        style.hash()
    }

    fn eql(&self, a: Style, b: Style) -> bool {
        // _ = self; return a.eql(b);
        a.eql(&b)
    }
}

/// The available set of styles in use on a page.
pub type Set = crate::ref_counted_set::RefCountedSet<Style, Id, size::CellCountInt, SetContext>;

// PORT NOTE: `std.hash.int` on a u64 dispatches to `uint64` (mx3). Ported
// verbatim so the deferred `Set` hashing matches when RefCountedSet lands.
fn hash_int_u64(input: u64) -> u64 {
    // fn uint64(input: u64) u64 {
    let mut x: u64 = input;
    let c: u64 = 0xbea225f9eb34556d;
    x = (x ^ (x >> 32)).wrapping_mul(c);
    x = (x ^ (x >> 29)).wrapping_mul(c);
    x = (x ^ (x >> 32)).wrapping_mul(c);
    x = x ^ (x >> 29);
    x
}

// PORT NOTE: the reference `comptime { assert(@sizeOf(PackedStyle) == 16);
// assert(std.meta.hasUniqueRepresentation(PackedStyle)); for (...Data fields...)
// assert(@bitSizeOf(field.type) == @bitSizeOf(Data)); }` — the 16-byte size and
// the equal-sized union fields. Reproduced in the `packed_style_layout` test.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packed_style_layout() {
        // assert(@sizeOf(PackedStyle) == 16);
        assert_eq!(16, core::mem::size_of::<PackedStyle>());
        // assert(@bitSizeOf(field.type) == @bitSizeOf(Data)) for the Data union:
        // none(u24), palette(u24), rgb(RGB == 24 bits) — all 24 bits. We assert
        // RGB carries exactly 24 data bits, matching `none`/`palette`.
        assert_eq!(24, core::mem::size_of::<color::RGB>() * 8);
    }

    fn vt(style: &Style) -> String {
        let mut s = String::new();
        style.formatter_vt().format(&mut s).unwrap();
        s
    }

    fn vt_with_palette(style: &Style, palette: &color::Palette) -> String {
        let mut s = String::new();
        let mut formatter = style.formatter_vt();
        formatter.palette = Some(palette);
        formatter.format(&mut s).unwrap();
        s
    }

    fn html(style: &Style) -> String {
        let mut s = String::new();
        style.formatter_html().format(&mut s).unwrap();
        s
    }

    fn html_with_palette(style: &Style, palette: &color::Palette) -> String {
        let mut s = String::new();
        let mut formatter = style.formatter_html();
        formatter.palette = Some(palette);
        formatter.format(&mut s).unwrap();
        s
    }

    #[test]
    fn style_vt_formatting_empty() {
        let style = Style::default();
        assert_eq!("\x1b[0m", vt(&style));
    }

    #[test]
    fn style_vt_formatting_bold() {
        let style = Style {
            flags: Flags {
                bold: true,
                ..Flags::default()
            },
            ..Style::default()
        };
        assert_eq!("\x1b[0m\x1b[1m", vt(&style));
    }

    #[test]
    fn style_vt_formatting_faint() {
        let style = Style {
            flags: Flags {
                faint: true,
                ..Flags::default()
            },
            ..Style::default()
        };
        assert_eq!("\x1b[0m\x1b[2m", vt(&style));
    }

    #[test]
    fn style_vt_formatting_italic() {
        let style = Style {
            flags: Flags {
                italic: true,
                ..Flags::default()
            },
            ..Style::default()
        };
        assert_eq!("\x1b[0m\x1b[3m", vt(&style));
    }

    #[test]
    fn style_vt_formatting_blink() {
        let style = Style {
            flags: Flags {
                blink: true,
                ..Flags::default()
            },
            ..Style::default()
        };
        assert_eq!("\x1b[0m\x1b[5m", vt(&style));
    }

    #[test]
    fn style_vt_formatting_inverse() {
        let style = Style {
            flags: Flags {
                inverse: true,
                ..Flags::default()
            },
            ..Style::default()
        };
        assert_eq!("\x1b[0m\x1b[7m", vt(&style));
    }

    #[test]
    fn style_vt_formatting_invisible() {
        let style = Style {
            flags: Flags {
                invisible: true,
                ..Flags::default()
            },
            ..Style::default()
        };
        assert_eq!("\x1b[0m\x1b[8m", vt(&style));
    }

    #[test]
    fn style_vt_formatting_strikethrough() {
        let style = Style {
            flags: Flags {
                strikethrough: true,
                ..Flags::default()
            },
            ..Style::default()
        };
        assert_eq!("\x1b[0m\x1b[9m", vt(&style));
    }

    #[test]
    fn style_vt_formatting_overline() {
        let style = Style {
            flags: Flags {
                overline: true,
                ..Flags::default()
            },
            ..Style::default()
        };
        assert_eq!("\x1b[0m\x1b[53m", vt(&style));
    }

    #[test]
    fn style_vt_formatting_underline_single() {
        let style = Style {
            flags: Flags {
                underline: sgr::Underline::Single,
                ..Flags::default()
            },
            ..Style::default()
        };
        assert_eq!("\x1b[0m\x1b[4m", vt(&style));
    }

    #[test]
    fn style_vt_formatting_underline_double() {
        let style = Style {
            flags: Flags {
                underline: sgr::Underline::Double,
                ..Flags::default()
            },
            ..Style::default()
        };
        assert_eq!("\x1b[0m\x1b[4:2m", vt(&style));
    }

    #[test]
    fn style_vt_formatting_underline_curly() {
        let style = Style {
            flags: Flags {
                underline: sgr::Underline::Curly,
                ..Flags::default()
            },
            ..Style::default()
        };
        assert_eq!("\x1b[0m\x1b[4:3m", vt(&style));
    }

    #[test]
    fn style_vt_formatting_underline_dotted() {
        let style = Style {
            flags: Flags {
                underline: sgr::Underline::Dotted,
                ..Flags::default()
            },
            ..Style::default()
        };
        assert_eq!("\x1b[0m\x1b[4:4m", vt(&style));
    }

    #[test]
    fn style_vt_formatting_underline_dashed() {
        let style = Style {
            flags: Flags {
                underline: sgr::Underline::Dashed,
                ..Flags::default()
            },
            ..Style::default()
        };
        assert_eq!("\x1b[0m\x1b[4:5m", vt(&style));
    }

    #[test]
    fn style_vt_formatting_fg_palette() {
        let style = Style {
            fg_color: Color::Palette(42),
            ..Style::default()
        };
        assert_eq!("\x1b[0m\x1b[38;5;42m", vt(&style));
    }

    #[test]
    fn style_vt_formatting_fg_rgb() {
        let style = Style {
            fg_color: Color::Rgb(color::RGB {
                r: 255,
                g: 128,
                b: 64,
            }),
            ..Style::default()
        };
        assert_eq!("\x1b[0m\x1b[38;2;255;128;64m", vt(&style));
    }

    #[test]
    fn style_vt_formatting_bg_palette() {
        let style = Style {
            bg_color: Color::Palette(7),
            ..Style::default()
        };
        assert_eq!("\x1b[0m\x1b[48;5;7m", vt(&style));
    }

    #[test]
    fn style_vt_formatting_bg_rgb() {
        let style = Style {
            bg_color: Color::Rgb(color::RGB {
                r: 32,
                g: 64,
                b: 96,
            }),
            ..Style::default()
        };
        assert_eq!("\x1b[0m\x1b[48;2;32;64;96m", vt(&style));
    }

    #[test]
    fn style_vt_formatting_underline_color_palette() {
        let style = Style {
            underline_color: Color::Palette(15),
            ..Style::default()
        };
        assert_eq!("\x1b[0m\x1b[58;5;15m", vt(&style));
    }

    #[test]
    fn style_vt_formatting_underline_color_rgb() {
        let style = Style {
            underline_color: Color::Rgb(color::RGB {
                r: 200,
                g: 100,
                b: 50,
            }),
            ..Style::default()
        };
        assert_eq!("\x1b[0m\x1b[58;2;200;100;50m", vt(&style));
    }

    #[test]
    fn style_vt_formatting_multiple_flags() {
        let style = Style {
            flags: Flags {
                bold: true,
                italic: true,
                underline: sgr::Underline::Single,
                ..Flags::default()
            },
            ..Style::default()
        };
        assert_eq!("\x1b[0m\x1b[1m\x1b[3m\x1b[4m", vt(&style));
    }

    #[test]
    fn style_vt_formatting_all_flags() {
        let style = Style {
            flags: Flags {
                bold: true,
                faint: true,
                italic: true,
                blink: true,
                inverse: true,
                invisible: true,
                strikethrough: true,
                overline: true,
                underline: sgr::Underline::Curly,
            },
            ..Style::default()
        };
        assert_eq!(
            "\x1b[0m\x1b[1m\x1b[2m\x1b[3m\x1b[5m\x1b[7m\x1b[8m\x1b[9m\x1b[53m\x1b[4:3m",
            vt(&style),
        );
    }

    #[test]
    fn style_vt_formatting_combined_colors_and_flags() {
        let style = Style {
            fg_color: Color::Rgb(color::RGB { r: 255, g: 0, b: 0 }),
            bg_color: Color::Palette(8),
            underline_color: Color::Rgb(color::RGB { r: 0, g: 255, b: 0 }),
            flags: Flags {
                bold: true,
                italic: true,
                underline: sgr::Underline::Double,
                ..Flags::default()
            },
        };
        assert_eq!(
            "\x1b[0m\x1b[1m\x1b[3m\x1b[4:2m\x1b[38;2;255;0;0m\x1b[48;5;8m\x1b[58;2;0;255;0m",
            vt(&style),
        );
    }

    #[test]
    fn style_vt_formatting_all_colors_rgb() {
        let style = Style {
            fg_color: Color::Rgb(color::RGB {
                r: 10,
                g: 20,
                b: 30,
            }),
            bg_color: Color::Rgb(color::RGB {
                r: 40,
                g: 50,
                b: 60,
            }),
            underline_color: Color::Rgb(color::RGB {
                r: 70,
                g: 80,
                b: 90,
            }),
            ..Style::default()
        };
        assert_eq!(
            "\x1b[0m\x1b[38;2;10;20;30m\x1b[48;2;40;50;60m\x1b[58;2;70;80;90m",
            vt(&style),
        );
    }

    #[test]
    fn style_vt_formatting_all_colors_palette() {
        let style = Style {
            fg_color: Color::Palette(1),
            bg_color: Color::Palette(2),
            underline_color: Color::Palette(3),
            ..Style::default()
        };
        assert_eq!("\x1b[0m\x1b[38;5;1m\x1b[48;5;2m\x1b[58;5;3m", vt(&style),);
    }

    #[test]
    fn style_vt_formatting_palette_with_palette_set_emits_rgb() {
        let style = Style {
            fg_color: Color::Palette(1),
            ..Style::default()
        };
        assert_eq!(
            "\x1b[0m\x1b[38;2;204;102;102m",
            vt_with_palette(&style, &color::DEFAULT),
        );
    }

    #[test]
    fn style_vt_formatting_all_palette_colors_with_palette_set() {
        let style = Style {
            fg_color: Color::Palette(1),
            bg_color: Color::Palette(2),
            underline_color: Color::Palette(3),
            ..Style::default()
        };
        assert_eq!(
            "\x1b[0m\x1b[38;2;204;102;102m\x1b[48;2;181;189;104m\x1b[58;2;240;198;116m",
            vt_with_palette(&style, &color::DEFAULT),
        );
    }

    #[test]
    fn style_html_formatting_basic_bold() {
        let style = Style {
            flags: Flags {
                bold: true,
                ..Flags::default()
            },
            ..Style::default()
        };
        assert_eq!("font-weight: bold;", html(&style));
    }

    #[test]
    fn style_html_formatting_fg_color_rgb() {
        let style = Style {
            fg_color: Color::Rgb(color::RGB {
                r: 255,
                g: 128,
                b: 64,
            }),
            ..Style::default()
        };
        assert_eq!("color: rgb(255, 128, 64);", html(&style));
    }

    #[test]
    fn style_html_formatting_bg_color_palette() {
        let style = Style {
            bg_color: Color::Palette(7),
            ..Style::default()
        };
        assert_eq!("background-color: var(--vt-palette-7);", html(&style));
    }

    #[test]
    fn style_html_formatting_combined_colors_and_flags() {
        let style = Style {
            fg_color: Color::Rgb(color::RGB { r: 255, g: 0, b: 0 }),
            bg_color: Color::Rgb(color::RGB { r: 0, g: 0, b: 255 }),
            flags: Flags {
                bold: true,
                italic: true,
                ..Flags::default()
            },
            ..Style::default()
        };
        let result = html(&style);
        assert!(result.contains("color: rgb(255, 0, 0);"));
        assert!(result.contains("background-color: rgb(0, 0, 255);"));
        assert!(result.contains("font-weight: bold;"));
        assert!(result.contains("font-style: italic;"));
    }

    #[test]
    fn style_html_formatting_single_decoration_line() {
        let style = Style {
            flags: Flags {
                underline: sgr::Underline::Single,
                ..Flags::default()
            },
            ..Style::default()
        };
        let result = html(&style);
        assert!(result.contains("text-decoration-line: underline;"));
        assert!(result.contains("text-decoration-style: solid;"));
    }

    #[test]
    fn style_html_formatting_multiple_decoration_lines() {
        let style = Style {
            flags: Flags {
                underline: sgr::Underline::Curly,
                strikethrough: true,
                overline: true,
                ..Flags::default()
            },
            ..Style::default()
        };
        let result = html(&style);
        assert!(result.contains("text-decoration-line: underline line-through overline;"));
        assert!(result.contains("text-decoration-style: wavy;"));
    }

    #[test]
    fn style_html_formatting_palette_with_palette_set_emits_rgb() {
        let style = Style {
            bg_color: Color::Palette(7),
            ..Style::default()
        };
        assert_eq!(
            "background-color: rgb(197, 200, 198);",
            html_with_palette(&style, &color::DEFAULT),
        );
    }

    #[test]
    fn style_html_formatting_all_palette_colors_with_palette_set() {
        let style = Style {
            fg_color: Color::Palette(1),
            bg_color: Color::Palette(2),
            underline_color: Color::Palette(3),
            ..Style::default()
        };
        assert_eq!(
            "color: rgb(204, 102, 102);background-color: rgb(181, 189, 104);text-decoration-color: rgb(240, 198, 116);",
            html_with_palette(&style, &color::DEFAULT),
        );
    }

    // PORT NOTE: a test-only RAII helper for the reference's
    // `alloc.alignedAlloc(u8, Set.base_align, layout.total_size)` /
    // `defer alloc.free(buf)`. Owns a `total_size`-byte heap allocation aligned
    // to `base_align` and frees it on drop.
    struct AlignedBuf {
        ptr: *mut u8,
        layout: std::alloc::Layout,
    }

    impl AlignedBuf {
        fn new(total_size: usize, base_align: usize) -> AlignedBuf {
            let layout = std::alloc::Layout::from_size_align(total_size, base_align).unwrap();
            let ptr = unsafe { std::alloc::alloc(layout) };
            assert!(!ptr.is_null());
            AlignedBuf { ptr, layout }
        }

        fn base(&self) -> *const u8 {
            self.ptr
        }

        fn offset_buf(&self) -> size::OffsetBuf {
            size::OffsetBuf::init(self.ptr as *const u8)
        }
    }

    impl Drop for AlignedBuf {
        fn drop(&mut self) {
            unsafe { std::alloc::dealloc(self.ptr, self.layout) };
        }
    }

    #[test]
    fn set_basic_usage() {
        // const layout: Set.Layout = .init(16);
        let layout = crate::ref_counted_set::Layout::<Id>::init::<Style, size::CellCountInt>(16);
        // const buf = try alloc.alignedAlloc(u8, Set.base_align, layout.total_size);
        let buf = AlignedBuf::new(layout.total_size, Set::base_align());

        let style = Style {
            flags: Flags {
                bold: true,
                ..Flags::default()
            },
            ..Style::default()
        };
        let style2 = Style {
            flags: Flags {
                italic: true,
                ..Flags::default()
            },
            ..Style::default()
        };

        // var set = Set.init(.init(buf), layout, .{});
        let mut set = unsafe { Set::init(buf.offset_buf(), layout, SetContext) };
        let base = buf.base();

        // Add style
        // const id = try set.add(buf, style);
        let id = unsafe { set.add(base, style) }.unwrap();
        // try testing.expect(id > 0);
        assert!(id > 0);

        // Second add should return the same metadata.
        {
            let id2 = unsafe { set.add(base, style) }.unwrap();
            assert_eq!(id, id2);
        }

        // Look it up
        {
            // PORT NOTE: the reference holds `v`/`v2` as `*Style` (raw); we keep
            // them raw too and read flags / compare pointers without forming a
            // long-lived `&Style` (which would alias under Stacked Borrows when
            // `get` is called again).
            let v: *mut Style = unsafe { set.get(base, id) };
            assert!(unsafe { (*v).flags.bold });

            let v2: *mut Style = unsafe { set.get(base, id) };
            // try testing.expectEqual(v, v2); — `v`/`v2` are the same slot pointer.
            assert_eq!(v, v2);
        }

        // Add a second style
        let id2 = unsafe { set.add(base, style2) }.unwrap();

        // Look it up
        {
            let v: *mut Style = unsafe { set.get(base, id2) };
            assert!(unsafe { (*v).flags.italic });
        }

        // Ref count
        assert!(unsafe { set.ref_count(base, id) } == 2);
        assert!(unsafe { set.ref_count(base, id2) } == 1);

        // Release
        unsafe { set.release(base, id) };
        assert!(unsafe { set.ref_count(base, id) } == 1);
        unsafe { set.release(base, id2) };
        assert!(unsafe { set.ref_count(base, id2) } == 0);

        // We added the first one twice, so
        unsafe { set.release(base, id) };
        assert!(unsafe { set.ref_count(base, id) } == 0);
    }

    #[test]
    fn set_capacities() {
        // We want to support at least this many styles without overflowing.
        // _ = Set.Layout.init(16384);
        let _ = crate::ref_counted_set::Layout::<Id>::init::<Style, size::CellCountInt>(16384);
    }

    // test "Set zero capacity"
    // PORT NOTE (parity e44f5cb0f): test hunk of the upstream zero-capacity
    // RefCountedSet lookup guard.
    #[test]
    fn set_zero_capacity() {
        // A zero-capacity set is a valid special case (see Layout.init).
        // It occurs in practice for pages with exact capacities where no
        // cell is styled (see Page.exactRowCapacity).
        let layout = crate::ref_counted_set::Layout::<Id>::init::<Style, size::CellCountInt>(0);
        assert_eq!(0, layout.total_size);

        // We allocate a nonzero buffer filled with 0xFF to simulate the
        // set being embedded in a larger structure (e.g. a Page) where
        // other data follows it. Lookups must not probe the zero-size
        // table: table[0] would read this adjacent memory and treat it
        // as an item ID, leading to far out-of-bounds item reads.
        let buf = AlignedBuf::new(64, Set::base_align());
        unsafe { core::ptr::write_bytes(buf.ptr, 0xFF, 64) };

        let mut set = unsafe { Set::init(buf.offset_buf(), layout, SetContext) };
        let base = buf.base();

        let style = Style {
            flags: Flags {
                bold: true,
                ..Flags::default()
            },
            ..Style::default()
        };
        assert_eq!(None, unsafe { set.lookup(base, style) });
        assert_eq!(
            Err(crate::ref_counted_set::AddError::OutOfMemory),
            unsafe { set.add(base, style) }
        );
        assert_eq!(0, set.count());

        // The adjacent memory must be untouched.
        for i in 0..64usize {
            assert_eq!(0xFFu8, unsafe { *buf.ptr.add(i) });
        }
    }
}
