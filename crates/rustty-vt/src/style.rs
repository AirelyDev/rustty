//! rustty-vt style
//!
//! PORTS: ghostty src/terminal/style.zig
//!
//! PORT STATUS: done (vt-2b wired the previously-deferred page.Cell- and
//! RefCountedSet-dependent seams: `bg`, `bgCell`, the `Set` alias + its 2 tests)
//!
//! PORT NOTE: the reference imports are
//!   const std = @import("std");
//!   const builtin = @import("builtin");
//!   const assert = @import("../quirks.zig").inlineAssert;
//!   const fastprint = @import("../fastprint.zig");
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
use crate::fastprint;
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

// The byte-equality in `Flags::eq` below transmutes `Flags` to `[u8; 9]`;
// this compile-time gate keeps the transmute honest if a field is ever added.
const _: () = assert!(core::mem::size_of::<Flags>() == 9);

impl PartialEq for Flags {
    // PORT NOTE: `self.flags == other.flags` in the reference is packed-struct
    // (integer) equality; we compare the packed representation.
    // PERF(port): byte equality == packed (to_u16) equality; was a ~50-insn
    // repack per compare, 45% of screen-clone.clone. All 9 fields are 1-byte
    // (8 bools + fieldless #[repr(u8)] sgr::Underline), so the struct has zero
    // padding regardless of field order and every field has a unique
    // representation (bool 0/1, distinct enum discriminants 0..=5): byte-eq
    // <=> field-eq <=> to_u16-eq (to_u16 is injective on valid values). The
    // `flags_eq_matches_packed_eq` test machine-checks this exhaustively.
    #[inline(always)]
    fn eq(&self, other: &Self) -> bool {
        let a: [u8; 9] = unsafe { core::mem::transmute(*self) };
        let b: [u8; 9] = unsafe { core::mem::transmute(*other) };
        a == b
    }
}
impl Eq for Flags {}

/// The color for an SGR attribute. A color can come from multiple
/// sources so we use this to track the source plus color value so that
/// we can properly react to things like palette changes.
// PORT NOTE: the reference is
//   pub const Color = union(Tag) {
//       none: void,
//       palette: u8,
//       rgb: color.RGB,
//       pub const Tag = enum(u8) { none, palette, rgb };
//   };
// The explicit `Tag` u8 values are reproduced by `ColorTag` below.
//
// PORT NOTE (WS19 T5 — uninit-in-page-mapping): this was originally ported as a
// Rust `enum Color { None, Palette(u8), Rgb(color::RGB) }`. That is a faithful
// *value* model but NOT a faithful *byte* model, and here the byte model is
// load-bearing: `Style` is stored in a `RefCountedSet` whose item array lives
// inside the terminal page's backing mapping, and `compress::CompressedPage::init`
// compresses that whole mapping as one `&[u8]` — exactly as the oracle's
// `compress/Page.zig` compresses `page.memory`. A typed Rust write of an enum
// initializes only the *active* variant's payload and actively DE-initializes
// the remainder of the payload slot, even over freshly-zeroed memory. So storing
// a `Color::None` / `Color::Palette` left 3 / 2 bytes of the page mapping
// uninitialized, and the compressor then read them (Miri: "reading memory at
// alloc…[0x17c..0x17d], but memory is uninitialized").
//
// `Color` is therefore now a byte-complete `#[repr(C)]` value: a `u8` tag
// followed by a 3-byte payload that is ALWAYS fully written. Its content
// follows the reference's `PackedStyle` hash-key encoding, which pairs
// `Color.Tag` with a 24-bit `Data` union whose per-tag content is `none => 0`,
// `palette => idx` in the low byte with the high 16 bits `_padding: u16 = 0`,
// and `rgb => r | g<<8 | b<<16` (see `PackedStyle::data_from_color`). A
// `Color` is literally `{ Tag, Data }`, and the unused payload bytes carry
// precisely the zero that `_padding = 0` mandates. Type safety at the API is
// preserved: the payload is private and every read goes through a
// tag-checked accessor (`palette()` / `rgb()`), which is the analogue of the
// reference's checked union-field access.
//
// CORRECTION (NEW-FINDINGS N88): this note used to say the 4 bytes are
// "exactly what the reference itself specifies for a color". They are not.
// `PackedStyle.Data` is only the hash key; the color the reference STORES in a
// `Style` is the tagged union above, 8 bytes at align 4 (its `color.RGB`
// payload is a `packed struct(u24)`, 4/4). So the port's `Style` is 21/1
// against the reference's 28/4, and its style-set item 28/2 against 36/4.
// That size reached the page layout (the styles section, and through it the
// rows of every page), which was observable. The style set is now laid out
// with the reference's item geometry (`RefShadowColor`, next to `Set`, and
// `ref_counted_set::RefItemGeometry`), so `Color` stays 4/1 in memory, which
// the compact `Style` compare and hash paths rely on.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Color {
    /// A `ColorTag` held as a raw `u8`. Raw, not `ColorTag`, so that a `Color`
    /// read back out of a page mapping can never be an invalid-enum-value UB;
    /// the mapping arrives zeroed, and `0` is `ColorTag::None`.
    tag: u8,
    /// The reference's 24-bit `PackedStyle.Data`, little-endian: `none => 0`,
    /// `palette => [idx, 0, 0]`, `rgb => [r, g, b]`. Always fully initialized.
    data: [u8; 3],
}

// `Color` must have no padding at all, or the whole point of the reshape above
// is lost. 1 + 3 bytes at align 1 == 4.
const _: () = assert!(core::mem::size_of::<Color>() == 4);
const _: () = assert!(core::mem::align_of::<Color>() == 1);

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorTag {
    None = 0,
    Palette = 1,
    Rgb = 2,
}

impl Color {
    /// The reference's `.none`. An associated const rather than an enum
    /// variant, so that the ~160 `Color::None` construction sites are unchanged
    /// by the byte-complete reshape (see the PORT NOTE on `Color`).
    #[allow(non_upper_case_globals)]
    pub const None: Color = Color {
        tag: ColorTag::None as u8,
        data: [0, 0, 0],
    };

    /// The reference's `.{ .palette = idx }`. Named like the former enum
    /// variant it replaces, for the same reason as `Color::None`.
    #[allow(non_snake_case)]
    #[inline(always)]
    pub const fn Palette(idx: u8) -> Color {
        Color {
            tag: ColorTag::Palette as u8,
            // high 16 data bits are the reference's `_padding: u16 = 0`
            data: [idx, 0, 0],
        }
    }

    /// The reference's `.{ .rgb = rgb }`. Named like the former enum variant it
    /// replaces, for the same reason as `Color::None`.
    #[allow(non_snake_case)]
    #[inline(always)]
    pub const fn Rgb(rgb: color::RGB) -> Color {
        Color {
            tag: ColorTag::Rgb as u8,
            data: [rgb.r, rgb.g, rgb.b],
        }
    }

    /// The active tag — the reference's `@as(Tag, self)`.
    // PERF(port): no panic path; `tag()` is on `PackedStyle::from_style`'s hot
    // hash path. Only 0/1/2 are constructible (all writes go through the
    // constructors above, and a zeroed mapping reads as `.none`), so the
    // fallback arm is unreachable and is `debug_assert`ed rather than trapping.
    #[inline(always)]
    pub fn tag(self) -> ColorTag {
        debug_assert!(self.tag <= ColorTag::Rgb as u8);
        match self.tag {
            1 => ColorTag::Palette,
            2 => ColorTag::Rgb,
            _ => ColorTag::None,
        }
    }

    /// The reference's `self.palette` union-field access. Only valid when
    /// `self.tag() == ColorTag::Palette`, which the reference also requires
    /// (Zig checks it in safe modes; we `debug_assert` it).
    #[inline(always)]
    pub fn palette(self) -> u8 {
        debug_assert!(self.tag == ColorTag::Palette as u8);
        self.data[0]
    }

    /// The reference's `self.rgb` union-field access. Only valid when
    /// `self.tag() == ColorTag::Rgb` (see `palette`).
    #[inline(always)]
    pub fn rgb(self) -> color::RGB {
        debug_assert!(self.tag == ColorTag::Rgb as u8);
        color::RGB {
            r: self.data[0],
            g: self.data[1],
            b: self.data[2],
        }
    }

    /// Normalization of a `Color` into a single comparable integer: the low
    /// byte is the tag (`ColorTag`: none=0, palette=1, rgb=2) and the upper 24
    /// bits are exactly `PackedStyle::data_from_color`'s payload normalization
    /// (none => 0, palette => idx, rgb => r|g<<8|b<<16), shifted past the tag
    /// byte. The key is injective across variants, so key equality is exactly
    /// the reference relation (tag equal && payload equal).
    // PERF(port): branch-free comparable key; the previous nested-match compare
    // carried `unreachable!()` panic paths into `Style::eql`'s hot loop.
    // Post-WS19-T5 this is a straight little-endian read of the type's own four
    // bytes: the reshaped layout *is* the normalization it used to compute.
    #[inline(always)]
    fn color_key(c: Color) -> u32 {
        u32::from_le_bytes([c.tag, c.data[0], c.data[1], c.data[2]])
    }

    /// True if the color is equal to another color.
    // PORT NOTE (reshape): the reference is
    //   return @as(Tag, self) == @as(Tag, other) and switch (self) {
    //       .none => true,
    //       .palette => self.palette == other.palette,
    //       .rgb => self.rgb == other.rgb,
    //   };
    // (style.zig Color.eql). Comparing normalized keys is the same relation;
    // the `color_eql_matches_reference` test machine-checks the equivalence.
    #[inline(always)]
    pub fn eql(self, other: Color) -> bool {
        Color::color_key(self) == Color::color_key(other)
    }

    /// Formatting to make debug logs easier to read
    /// by only including non-default attributes.
    // PORT NOTE: the reference `format` writes "Color.none" / "Color.palette{ N }"
    // / "Color.rgb{ r, g, b }". Ported via `core::fmt::Write`.
    pub fn format<W: core::fmt::Write>(self, writer: &mut W) -> core::fmt::Result {
        match self.tag() {
            ColorTag::None => writer.write_str("Color.none"),
            ColorTag::Palette => write!(writer, "Color.palette{{ {} }}", self.palette()),
            ColorTag::Rgb => {
                let rgb = self.rgb();
                write!(writer, "Color.rgb{{ {}, {}, {} }}", rgb.r, rgb.g, rgb.b)
            }
        }
    }
}

// PORT NOTE (WS19 T5): hand-written to reproduce exactly what `#[derive(Debug)]`
// on the former `enum Color { None, Palette(u8), Rgb(color::RGB) }` printed, so
// the reshape is invisible to `{:?}` output.
impl core::fmt::Debug for Color {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.tag() {
            ColorTag::None => f.write_str("None"),
            ColorTag::Palette => f.debug_tuple("Palette").field(&self.palette()).finish(),
            ColorTag::Rgb => f.debug_tuple("Rgb").field(&self.rgb()).finish(),
        }
    }
}

impl Style {
    /// True if the style is the default style.
    #[inline]
    pub fn default_style(&self) -> bool {
        // On wasm, eql converts both sides to packed form; the default
        // side is comptime-known, so bake it and convert only self.
        // This is called on every SGR change so it's worth it.
        // PORT NOTE: the reference bakes the default's packed form via
        // `comptime @bitCast(PackedStyle.fromStyle(.{}))` inside `default()`;
        // the port delegates to `eql`, whose wasm arm packs both sides — the
        // `Style::default()` side is a constant, so its conversion folds at
        // compile time to the same baked value.
        self.eql(&Style::default())
    }

    /// True if the style is equal to another style.
    // PERF(port): inline(always) so the probe compare lands inside
    // `RefCountedSet::add_with_id`'s hot branch (screen-clone.clone spent 45%
    // of its self-time in an out-of-line `Style::eql`; plain #[inline] was
    // measured insufficient to inline it there).
    #[inline(always)]
    pub fn eql(&self, other: &Style) -> bool {
        // On wasm, comparing packed forms wins (~13% on SGR-heavy streams).
        // On native, the branchy early-exit field compare below measures
        // ~5% faster (unequal styles usually differ in the first
        // field or two), so each target keeps its own strategy.
        // if (comptime builtin.cpu.arch.isWasm()) {
        if cfg!(target_arch = "wasm32") {
            // const a: u128 = @bitCast(PackedStyle.fromStyle(self));
            let a: u128 = PackedStyle::from_style(*self).bits;
            // const b: u128 = @bitCast(PackedStyle.fromStyle(other));
            let b: u128 = PackedStyle::from_style(*other).bits;
            return a == b;
        }

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

            _ => match self.bg_color.tag() {
                ColorTag::None => None,
                ColorTag::Palette => Some(palette[self.bg_color.palette() as usize]),
                ColorTag::Rgb => Some(self.bg_color.rgb()),
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

        match self.fg_color.tag() {
            ColorTag::None => {
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

            ColorTag::Palette => {
                let idx = self.fg_color.palette();
                if self.flags.bold && opts.bold.is_some() {
                    let bright_offset = color::Name::BrightBlack.cval();
                    if idx < bright_offset {
                        return opts.palette[(idx + bright_offset) as usize];
                    }
                }

                opts.palette[idx as usize]
            }

            ColorTag::Rgb => {
                let rgb = self.fg_color.rgb();
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
        match self.underline_color.tag() {
            ColorTag::None => None,
            ColorTag::Palette => Some(palette[self.underline_color.palette() as usize]),
            ColorTag::Rgb => Some(self.underline_color.rgb()),
        }
    }

    /// Returns a bg-color only cell from this style, if it exists.
    // PORT NOTE (vt-2b seam wired): `page.Cell` is now ported, so this is the
    // verbatim translation of the reference `bgCell`.
    pub fn bg_cell(self) -> Option<crate::page::Cell> {
        match self.bg_color.tag() {
            ColorTag::None => None,
            ColorTag::Palette => {
                let mut cell = crate::page::Cell::zeroed();
                cell.set_content_tag(crate::page::ContentTag::BgColorPalette);
                cell.set_content_color_palette(self.bg_color.palette());
                Some(cell)
            }
            ColorTag::Rgb => {
                let rgb = self.bg_color.rgb();
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
    // PERF(port): `#[inline]` — the reference force-inlines this whole chain
    // (`inline fn fromStyle`, `@call(.always_inline, std.hash.int, ...)`);
    // out-of-line it was a standalone hotspot in the rows-resize profile.
    #[inline]
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
        let mut buf: [u8; 128] = [0; 128];
        let len = self.format_into(&mut buf);
        // PORT NOTE: `writer.writeAll(buf[0..len])` — the `core::fmt::Write`
        // seam takes `&str`; every emitted byte is ASCII so the conversion is
        // infallible.
        writer.write_str(core::str::from_utf8(&buf[..len]).unwrap())
    }

    /// Assemble the style's complete SGR emission into `buf`, returning the
    /// emitted length.
    //
    // PERF(port): the byte core is split from `format` so the hot formatter
    // path (`format_style_open`) can emit the bytes straight into its
    // `io::Write` sink — the reference writes bytes directly
    // (`writer.writeAll(buf[0..len])`), and routing through the
    // `core::fmt::Write` seam added a UTF-8 validation scan plus a
    // non-inlinable adapter hop per style change.
    #[inline]
    pub fn format_into(&self, buf: &mut [u8; 128]) -> usize {
        // Style emission is a hot path when formatting styled terminal
        // contents, so all of the sequences are assembled in a buffer
        // and written in one call rather than going through the
        // (slower) format string machinery with a write per sequence.
        //
        // Worst case: `\x1b[0m` (4) + 7 flags (28) + `\x1b[53m` (5) +
        // `\x1b[4:2m` (6) + 3 RGB colors (19 each = 57) = 100.

        // Always reset the style. Styles are fully self-contained.
        // Even if this style is empty, then that means we want to go
        // back to the default.
        buf[0..4].copy_from_slice(b"\x1b[0m");
        let mut len: usize = 4;

        // Our flags
        if self.style.flags.bold {
            buf[len..len + 4].copy_from_slice(b"\x1b[1m");
            len += 4;
        }
        if self.style.flags.faint {
            buf[len..len + 4].copy_from_slice(b"\x1b[2m");
            len += 4;
        }
        if self.style.flags.italic {
            buf[len..len + 4].copy_from_slice(b"\x1b[3m");
            len += 4;
        }
        if self.style.flags.blink {
            buf[len..len + 4].copy_from_slice(b"\x1b[5m");
            len += 4;
        }
        if self.style.flags.inverse {
            buf[len..len + 4].copy_from_slice(b"\x1b[7m");
            len += 4;
        }
        if self.style.flags.invisible {
            buf[len..len + 4].copy_from_slice(b"\x1b[8m");
            len += 4;
        }
        if self.style.flags.strikethrough {
            buf[len..len + 4].copy_from_slice(b"\x1b[9m");
            len += 4;
        }
        if self.style.flags.overline {
            buf[len..len + 5].copy_from_slice(b"\x1b[53m");
            len += 5;
        }
        match self.style.flags.underline {
            sgr::Underline::None => {}
            sgr::Underline::Single => {
                buf[len..len + 4].copy_from_slice(b"\x1b[4m");
                len += 4;
            }
            sgr::Underline::Double => {
                buf[len..len + 6].copy_from_slice(b"\x1b[4:2m");
                len += 6;
            }
            sgr::Underline::Curly => {
                buf[len..len + 6].copy_from_slice(b"\x1b[4:3m");
                len += 6;
            }
            sgr::Underline::Dotted => {
                buf[len..len + 6].copy_from_slice(b"\x1b[4:4m");
                len += 6;
            }
            sgr::Underline::Dashed => {
                buf[len..len + 6].copy_from_slice(b"\x1b[4:5m");
                len += 6;
            }
        }

        // Various colors.
        len = self.append_color(buf, len, 38, self.style.fg_color);
        len = self.append_color(buf, len, 48, self.style.bg_color);
        len = self.append_color(buf, len, 58, self.style.underline_color);

        len
    }

    /// Appends a standalone `\x1b[{prefix};5;{idx}m` or
    /// `\x1b[{prefix};2;{r};{g};{b}m` sequence to buf, returning the
    /// length written.
    // PERF(port): `#[inline(always)]` — SGR emission helper on the
    // per-style-change hot path; the reference inlines the whole chain (plain
    // `#[inline]` lost to the cost model — verified out-of-line in the
    // binary). Takes the whole fixed buffer plus a cursor and RETURNS the new
    // cursor, instead of a runtime-length subslice, so the internal indexing
    // checks fold against the known 128-byte array.
    #[inline(always)]
    fn append_color(&self, buf: &mut [u8; 128], len: usize, prefix: u8, value: Color) -> usize {
        match value.tag() {
            ColorTag::None => len,

            ColorTag::Palette => {
                let idx = value.palette();

                // Direct RGB: `\x1b[{prefix};2;{r};{g};{b}m`
                if let Some(p) = self.palette {
                    return append_color_rgb(buf, len, prefix, p[idx as usize]);
                }

                // Palette reference: `\x1b[{prefix};5;{idx}m`
                let mut len = len;
                buf[len..len + 2].copy_from_slice(b"\x1b[");
                len += 2;
                len += fastprint::print_decimal(&mut buf[len..], prefix);
                buf[len..len + 3].copy_from_slice(b";5;");
                len += 3;
                len += fastprint::print_decimal(&mut buf[len..], idx);
                buf[len] = b'm';
                len += 1;
                len
            }

            ColorTag::Rgb => append_color_rgb(buf, len, prefix, value.rgb()),
        }
    }
}

/// Appends `\x1b[{prefix};2;{r};{g};{b}m` to buf, returning the
/// length written.
// PORT NOTE: the reference nests this self-less fn inside `VTFormatter`; a
// module-level fn is the Rust equivalent.
// PERF(port): `#[inline(always)]` + whole-buffer-plus-cursor signature — see
// `append_color`.
#[inline(always)]
fn append_color_rgb(buf: &mut [u8; 128], len: usize, prefix: u8, rgb: color::RGB) -> usize {
    let mut len = len;
    buf[len..len + 2].copy_from_slice(b"\x1b[");
    len += 2;
    len += fastprint::print_decimal(&mut buf[len..], prefix);
    buf[len..len + 3].copy_from_slice(b";2;");
    len += 3;
    len += fastprint::print_decimal(&mut buf[len..], rgb.r);
    buf[len] = b';';
    len += 1;
    len += fastprint::print_decimal(&mut buf[len..], rgb.g);
    buf[len] = b';';
    len += 1;
    len += fastprint::print_decimal(&mut buf[len..], rgb.b);
    buf[len] = b'm';
    len += 1;
    len
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
        // Style emission is a hot path when formatting styled terminal
        // contents, so the values are assembled in a buffer and written
        // in one call rather than going through the (slower) format
        // string machinery.
        let mut buf: [u8; 32] = [0; 32];
        // PORT NOTE: `var len: usize = 0;` — deferred init (every live arm
        // assigns; `.none` returns) so the dead zero write doesn't trip the
        // unused-assignment lint.
        let mut len: usize;
        match c.tag() {
            ColorTag::None => return Ok(()),

            // `{property}: rgb({r}, {g}, {b});`
            ColorTag::Palette => {
                let idx = c.palette();
                if let Some(p) = self.palette {
                    len = format_color_rgb(&mut buf, p[idx as usize]);
                } else {
                    // `{property}: var(--vt-palette-{idx});`
                    const PREFIX: &[u8] = b": var(--vt-palette-";
                    buf[0..PREFIX.len()].copy_from_slice(PREFIX);
                    len = PREFIX.len();
                    len += fastprint::print_decimal(&mut buf[len..], idx);
                    buf[len..len + 2].copy_from_slice(b");");
                    len += 2;
                }
            }

            ColorTag::Rgb => len = format_color_rgb(&mut buf, c.rgb()),
        }

        writer.write_str(property)?;
        // PORT NOTE: `writer.writeAll(buf[0..len])` — all emitted bytes are
        // ASCII so the `&str` conversion is infallible.
        writer.write_str(core::str::from_utf8(&buf[..len]).unwrap())
    }
}

/// Writes `: rgb({r}, {g}, {b});` into buf, returning the length
/// written.
// PORT NOTE: the reference nests this self-less fn inside `HtmlFormatter`; a
// module-level fn is the Rust equivalent.
fn format_color_rgb(buf: &mut [u8; 32], rgb: color::RGB) -> usize {
    buf[0..6].copy_from_slice(b": rgb(");
    let mut len: usize = 6;
    len += fastprint::print_decimal(&mut buf[len..], rgb.r);
    buf[len..len + 2].copy_from_slice(b", ");
    len += 2;
    len += fastprint::print_decimal(&mut buf[len..], rgb.g);
    buf[len..len + 2].copy_from_slice(b", ");
    len += 2;
    len += fastprint::print_decimal(&mut buf[len..], rgb.b);
    buf[len..len + 2].copy_from_slice(b");");
    len += 2;
    len
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
    // PORT NOTE (WS19 T5): since the reshape, `Color`'s own 3 payload bytes ARE
    // this 24-bit `Data` — `none => 0`, `palette => idx` with the high 16 bits
    // zero, `rgb => r|g<<8|b<<16` — so this is a plain little-endian widening
    // rather than a per-tag reconstruction. Kept as a named function because the
    // reference has `Data.fromColor` and the bit contract documented above is
    // what makes the reshape byte-faithful.
    fn data_from_color(c: Color) -> u32 {
        let d = c.data;
        u32::from_le_bytes([d[0], d[1], d[2], 0])
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
    // PERF(port): inline the trait trampolines so `Style::eql`/`Style::hash`
    // reach the `RefCountedSet` probe loops without a call (see Style::eql).
    #[inline]
    fn hash(&self, style: Style) -> u64 {
        // _ = self; return style.hash();
        style.hash()
    }

    #[inline(always)]
    fn eql(&self, a: Style, b: Style) -> bool {
        // _ = self; return a.eql(b);
        a.eql(&b)
    }
}

/// The available set of styles in use on a page.
pub type Set = crate::ref_counted_set::RefCountedSet<Style, Id, size::CellCountInt, SetContext>;

// PORT NOTE (layout parity, NEW-FINDINGS N88): the reference's `Set.Item` for
// `Style` is 36 bytes at align 4; the port's is 28 at align 2, because its
// `Style` is 21/1 against the reference's 28/4 (`Color` 4/1 against 8/4,
// `color.RGB` 3/1 against 4/4, `Flags` 9/1 against 2/2). The item array's size
// decides the page layout (see `RefItemGeometry`), so the set is laid out with
// the reference's geometry. It is derived here from shadows of the reference's
// declarations at `20abdb50a`, each field the size and alignment of the
// reference's own type:
//   - `Style.Color = union(Tag) { none: void, palette: u8, rgb: color.RGB }`,
//     `Tag = enum(u8)` (`style.zig:47`); the payload is `color.RGB`,
//     `packed struct(u24)` (`color.zig:540`), 4/4, so the tagged union is 8/4;
//   - `Style.Flags = packed struct(u16)` (`style.zig:31`), 2/2;
//   - `Style = struct { fg_color, bg_color, underline_color: Color, flags }`
//     (`style.zig:21`), 26 rounded to 28/4;
//   - `RefCountedSet.Item = struct { value: T, meta: Metadata }` with
//     `Metadata = struct { bucket: Id, psl: Id, ref: RefCountInt }`
//     (`ref_counted_set.zig:83`), `Id = size.StyleCountInt` (u16) and
//     `RefCountInt = size.CellCountInt` (u16) (`style.zig:15`, `:681`),
//     34 rounded to 36/4.
// `ref_item_geometry_matches_the_reference_probe` pins the result to what a
// Zig probe of the reference reports.
#[repr(C)]
#[allow(dead_code)] // never built: only its size and alignment are read
struct RefShadowColor {
    payload: u32,
    tag: u8,
}
#[repr(C)]
#[allow(dead_code)] // never built: only its size and alignment are read
struct RefShadowStyle {
    fg_color: RefShadowColor,
    bg_color: RefShadowColor,
    underline_color: RefShadowColor,
    flags: u16,
}
#[repr(C)]
#[allow(dead_code)] // never built: only its size and alignment are read
struct RefShadowItem {
    value: RefShadowStyle,
    bucket: Id,
    psl: Id,
    ref_count: size::CellCountInt,
}

impl crate::ref_counted_set::RefItemGeometry for Style {
    const REF_ITEM_SIZE: usize = core::mem::size_of::<RefShadowItem>();
    const REF_ITEM_ALIGN: usize = core::mem::align_of::<RefShadowItem>();
}

// The port's own item fits the reference's slot (also checked, generically,
// by `ItemStride`).
const _: () = assert!(
    core::mem::size_of::<crate::ref_counted_set::Item<Style, Id, size::CellCountInt>>()
        <= <Style as crate::ref_counted_set::RefItemGeometry>::REF_ITEM_SIZE
);

// PORT NOTE: `std.hash.int` on a u64 dispatches to `uint64` (mx3). Ported
// verbatim so the deferred `Set` hashing matches when RefCountedSet lands.
// PERF(port): `#[inline]` — the reference calls it `@call(.always_inline, ...)`.
#[inline]
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

    // PERF(port) gate: `Flags::eq` compares raw bytes; prove byte equality is
    // exactly packed (to_u16) equality, exhaustively over BOTH sides of the
    // compare (256 bool combos x 6 underline values per side).
    #[test]
    fn flags_eq_matches_packed_eq() {
        fn flags_from(bits: u8, underline: sgr::Underline) -> Flags {
            Flags {
                bold: bits & 1 != 0,
                italic: bits & 2 != 0,
                faint: bits & 4 != 0,
                blink: bits & 8 != 0,
                inverse: bits & 16 != 0,
                invisible: bits & 32 != 0,
                strikethrough: bits & 64 != 0,
                overline: bits & 128 != 0,
                underline,
            }
        }
        const UNDERLINES: [sgr::Underline; 6] = [
            sgr::Underline::None,
            sgr::Underline::Single,
            sgr::Underline::Double,
            sgr::Underline::Curly,
            sgr::Underline::Dotted,
            sgr::Underline::Dashed,
        ];

        let mut all: Vec<Flags> = Vec::with_capacity(256 * UNDERLINES.len());
        for bits in 0..=255u8 {
            for ul in UNDERLINES {
                all.push(flags_from(bits, ul));
            }
        }
        for &a in &all {
            for &b in &all {
                assert_eq!(a == b, a.to_u16() == b.to_u16());
            }
        }
    }

    // PERF(port) gate: `Color::eql` compares normalized keys; prove key
    // equality is exactly the reference relation (tag equal && payload equal)
    // over all tag pairs, all 256x256 palette pairs, and an RGB corner
    // lattice ({0,1,127,255}^3 per side).
    #[test]
    fn color_eql_matches_reference() {
        // The reference relation, verbatim:
        //   @as(Tag, self) == @as(Tag, other) and switch (self) {
        //       .none => true,
        //       .palette => self.palette == other.palette,
        //       .rgb => self.rgb == other.rgb,
        //   }
        fn reference_eql(a: Color, b: Color) -> bool {
            a.tag() == b.tag()
                && match a.tag() {
                    ColorTag::None => true,
                    ColorTag::Palette => a.palette() == b.palette(),
                    ColorTag::Rgb => a.rgb() == b.rgb(),
                }
        }

        const LATTICE: [u8; 4] = [0, 1, 127, 255];
        let mut rgbs: Vec<Color> = Vec::new();
        for r in LATTICE {
            for g in LATTICE {
                for b in LATTICE {
                    rgbs.push(Color::Rgb(color::RGB { r, g, b }));
                }
            }
        }

        // All tag pairs (representatives of each variant), including
        // cross-variant pairs which must always be unequal.
        let reps = [
            Color::None,
            Color::Palette(0),
            Color::Palette(255),
            Color::Rgb(color::RGB { r: 0, g: 0, b: 0 }),
            Color::Rgb(color::RGB {
                r: 255,
                g: 255,
                b: 255,
            }),
        ];
        for &a in &reps {
            for &b in &reps {
                assert_eq!(a.eql(b), reference_eql(a, b));
            }
        }

        // All 256x256 palette pairs.
        for p in 0..=255u8 {
            for op in 0..=255u8 {
                let (a, b) = (Color::Palette(p), Color::Palette(op));
                assert_eq!(a.eql(b), reference_eql(a, b));
            }
        }

        // RGB corner-lattice pairs.
        for &a in &rgbs {
            for &b in &rgbs {
                assert_eq!(a.eql(b), reference_eql(a, b));
            }
        }
    }

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

    // Rustty-only (N88): the geometry the style and hyperlink sets are laid
    // out with is the reference's. The numbers are what a Zig probe of the
    // reference at `20abdb50a` printed (`@sizeOf`/`@alignOf` of `Style`,
    // `style.Style.Color`, `color.RGB`, `style.Set.Item`, `hyperlink.Set.Item`,
    // and `style.Set.Layout.init(n)` for small `n`), on macOS arm64 and (all
    // but `Color` and `RGB`) on Linux aarch64, with the same results.
    #[test]
    fn ref_item_geometry_matches_the_reference_probe() {
        use crate::ref_counted_set::{ItemStride, Layout, RefItemGeometry};
        // `Style.Color` 8/4 (its `color.RGB` payload 4/4), `Style` 28/4.
        assert_eq!(4, core::mem::size_of::<u32>()); // the shadow's `RGB`
        assert_eq!(8, core::mem::size_of::<RefShadowColor>());
        assert_eq!(4, core::mem::align_of::<RefShadowColor>());
        assert_eq!(28, core::mem::size_of::<RefShadowStyle>());
        assert_eq!(4, core::mem::align_of::<RefShadowStyle>());
        assert_eq!(36, <Style as RefItemGeometry>::REF_ITEM_SIZE);
        assert_eq!(4, <Style as RefItemGeometry>::REF_ITEM_ALIGN);
        assert_eq!(36, ItemStride::<Style, Id, size::CellCountInt>::SIZE);
        assert_eq!(4, ItemStride::<Style, Id, size::CellCountInt>::ALIGN);
        type HlStride =
            ItemStride<crate::hyperlink::PageEntry, crate::hyperlink::Id, size::CellCountInt>;
        assert_eq!(48, HlStride::SIZE);
        assert_eq!(8, HlStride::ALIGN);
        assert_eq!(48, crate::page::hyperlink_set_item_size());

        // `(n, items_start, total_size)` of `Set.Layout.init(n)`. At n = 1
        // the reference aligns the (empty) item array to 4, the port's own
        // item alignment would give 2.
        for (n, items_start, total_size) in [
            (0, 0, 0),
            (1, 4, 4),
            (2, 4, 40),
            (16, 32, 500),
            (128, 256, 4000),
        ] {
            let l = Layout::<Id>::init::<Style, size::CellCountInt>(n);
            assert_eq!(items_start, l.items_start, "Layout.init({n}).items_start");
            assert_eq!(total_size, l.total_size, "Layout.init({n}).total_size");
        }
    }

    // Rustty-only (N88): the style set's items sit at the reference's stride
    // (36 bytes), not at the port's `size_of::<Item>()` (28), and every
    // access path reads and writes them there. The set is filled to
    // capacity with distinct styles and distinct reference counts; each item
    // is then read back through `get`, `get_at`, `lookup`, `ref_count`, the
    // iterator, `item_at`, and the raw bytes at `items_start + id * 36`, after
    // which the tail of every slot must still be zero (an item written at
    // the wrong stride lands in some slot's tail). It then releases a third
    // of the items, adds new styles into the freed ids (`insert`'s reap and
    // placement, `delete_item`), and checks everything again.
    //
    // NEGATIVE CONTROL: address any one path with `items.add(id)` (the port's
    // own stride) instead of `item_at` — e.g. `get`, `get_at`, the iterator,
    // `use_id`, `release`, or `insert`'s final placement — and this goes red.
    #[test]
    fn set_items_are_addressed_at_the_reference_stride() {
        use crate::ref_counted_set::{item_at, Item, ItemStride};
        type SetItem = Item<Style, Id, size::CellCountInt>;
        const STRIDE: usize = 36;
        assert_eq!(STRIDE, ItemStride::<Style, Id, size::CellCountInt>::SIZE);
        assert!(
            core::mem::size_of::<SetItem>() < STRIDE,
            "the test needs a stride larger than the item"
        );

        let layout = crate::ref_counted_set::Layout::<Id>::init::<Style, size::CellCountInt>(256);
        let buf = AlignedBuf::new(layout.total_size, Set::base_align());
        // SAFETY: fresh allocation of `total_size` bytes.
        unsafe { core::ptr::write_bytes(buf.ptr, 0xA5, layout.total_size) };
        let mut set = unsafe { Set::init(buf.offset_buf(), layout, SetContext) };
        let base = buf.base();

        fn style_for(n: u32) -> Style {
            Style {
                fg_color: Color::Rgb(color::RGB {
                    r: n as u8,
                    g: (n >> 8) as u8,
                    b: 0x5A,
                }),
                bg_color: Color::Palette((n * 7) as u8),
                underline_color: if n.is_multiple_of(2) {
                    Color::None
                } else {
                    Color::Palette(n as u8)
                },
                flags: Flags {
                    bold: n.is_multiple_of(3),
                    italic: n.is_multiple_of(5),
                    ..Flags::default()
                },
            }
        }

        // `(id, style, refs)` of every live item.
        let mut live: Vec<(Id, Style, u16)> = Vec::new();
        let mut n: u32 = 0;
        loop {
            let style = style_for(n);
            let Ok(id) = (unsafe { set.add(base, style) }) else {
                break;
            };
            let refs = 1 + (n % 4) as u16;
            for _ in 1..refs {
                unsafe { set.use_id(base, id) };
            }
            live.push((id, style, refs));
            n += 1;
        }
        assert!(live.len() > 150, "filled {} items", live.len());

        let check = |set: &Set, live: &[(Id, Style, u16)]| {
            let items = unsafe { set.items_ptr(base) };
            for &(id, style, refs) in live {
                let i = id as usize;
                assert!(unsafe { (*set.get(base, id)).eql(&style) }, "get({id})");
                assert!(
                    unsafe { (*set.get_at(items, id)).eql(&style) },
                    "get_at({id})"
                );
                assert_eq!(Some(id), unsafe { set.lookup(base, style) }, "lookup({id})");
                assert_eq!(refs, unsafe { set.ref_count(base, id) }, "ref_count({id})");
                let via_item_at = unsafe { &*item_at(items, i) };
                assert!(via_item_at.value.eql(&style), "item_at({id})");
                // The raw bytes at the reference's offset.
                let raw = unsafe { base.add(layout.items_start + i * STRIDE) };
                let from_raw: SetItem = unsafe { core::ptr::read(raw as *const SetItem) };
                assert!(from_raw.value.eql(&style), "raw bytes of item {id}");
                assert_eq!(refs, from_raw.meta.ref_count, "raw ref count of item {id}");
            }
            // Every live item exactly once, in id order, from the iterator.
            let mut it = unsafe { set.iterator(base) };
            let mut seen: Vec<(Id, Style)> = Vec::new();
            while let Some(e) = it.next() {
                seen.push((e.id, unsafe { *e.value_ptr }));
            }
            let mut want: Vec<(Id, Style)> = live.iter().map(|&(id, s, _)| (id, s)).collect();
            want.sort_by_key(|&(id, _)| id);
            assert_eq!(want.len(), seen.len(), "iterator count");
            for ((wid, ws), (sid, ss)) in want.iter().zip(seen.iter()) {
                assert_eq!(wid, sid, "iterator id");
                assert!(ws.eql(ss), "iterator value of {sid}");
            }
            // The tail of every slot is untouched by the set.
            for i in 0..layout.cap {
                let slot = unsafe { base.add(layout.items_start + i * STRIDE) };
                for b in core::mem::size_of::<SetItem>()..STRIDE {
                    assert_eq!(0, unsafe { *slot.add(b) }, "slot {i} tail byte {b}");
                }
            }
        };
        check(&set, &live);

        // Release every third item to zero, then add new styles into the
        // freed ids (`add_with_id` on a dead id: `delete_item`, then
        // `insert`'s probe, reap and placement).
        let mut kept: Vec<(Id, Style, u16)> = Vec::new();
        let mut freed: Vec<Id> = Vec::new();
        for (k, &(id, style, refs)) in live.iter().enumerate() {
            if k % 3 == 1 {
                for _ in 0..refs {
                    unsafe { set.release(base, id) };
                }
                freed.push(id);
            } else {
                kept.push((id, style, refs));
            }
        }
        for &dead in &freed {
            let style = style_for(n);
            n += 1;
            let id = match unsafe { set.add_with_id(base, style, dead) }.unwrap() {
                None => dead,
                Some(other) => other,
            };
            kept.push((id, style, 1));
        }
        assert!(freed.len() > 50, "freed {}", freed.len());
        check(&set, &kept);
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
        assert_eq!(Err(crate::ref_counted_set::AddError::OutOfMemory), unsafe {
            set.add(base, style)
        });
        assert_eq!(0, set.count());

        // The adjacent memory must be untouched.
        for i in 0..64usize {
            assert_eq!(0xFFu8, unsafe { *buf.ptr.add(i) });
        }
    }
}
