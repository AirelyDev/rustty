//! SGR (Select Graphic Rendition) attrinvbute parsing and types.
//!
//! PORTS: ghostty src/terminal/sgr.zig
//!
//! PORT STATUS: done

// PARITY: `Parser::next` is named after the reference `Parser.next`, not the
// `Iterator::next` trait; the `Parser` Default mirrors the reference's explicit
// field defaults.
#![allow(clippy::should_implement_trait)]
#![allow(clippy::derivable_impls)]

// PORT NOTE: the reference imports are
//   const std = @import("std");
//   const assert = @import("../quirks.zig").inlineAssert;
//   const testing = std.testing;
//   const lib = @import("lib.zig");
//   const color = @import("color.zig");
//   const SepList = @import("Parser.zig").Action.CSI.SepList;
// `SepList` lives in `Parser.zig` (ported by a later vt sub-batch). To avoid
// pulling in an unported sibling (R3), it is defined here as a DEFERRED SEAM: a
// `std.StaticBitSet(MAX_PARAMS)` with `MAX_PARAMS == 24` (the reference value),
// carrying only the bitset operations sgr uses. When Parser lands, this seam is
// replaced by a re-export of `parser::Action::CSI::SepList`.

use crate::color;
use rustty_util::quirks::inlineAssert as assert;

// PORT NOTE: `sgr.zig:8` is `const SepList = @import("Parser.zig").Action.CSI.SepList;`
// — the reference has exactly ONE `SepList` type, owned by `Parser`. The local
// bitset that stood here was a vt-sub-batch seam for the era when `Parser` was
// not yet ported; `Parser` has since landed, so this re-export restores the
// reference's single-type structure (R1). `parser::SepList` is the same
// `StaticBitSet(MAX_PARAMS)` projection with `MAX_PARAMS == 24`.
pub use crate::parser::SepList;

/// C: RusttySgrUnderline (R2) — checked by the A5 enum-discriminant arm.
/// Underline style (sub-parameter of SGR 4).
// PORT NOTE: the reference `Attribute.Underline` is `enum(u3)`. Hoisted to a
// top-level enum (Rust has no nested enum types); `#[repr(u8)]` since Rust has
// no u3.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Underline {
    None = 0,
    Single = 1,
    Double = 2,
    Curly = 3,
    Dotted = 4,
    Dashed = 5,
    // pub const C = c_int;
    // pub fn cval(self) -> Underline.C { @intFromEnum(self) }
    // PORT NOTE: the C value helper is ported in rustty-vt-c.
}

/// The remaining, where we got hung up.
// PORT NOTE: `Attribute.Unknown` holds two `[]const u16` slices. Hoisted to a
// top-level struct holding borrowed slices.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Unknown<'a> {
    /// Full is the full SGR input.
    pub full: &'a [u16],

    /// Partial is the remaining, where we got hung up.
    pub partial: &'a [u16],
    // pub const C = extern struct { full_ptr, full_len, partial_ptr, partial_len };
    // pub fn cval(self) -> Unknown.C { ... }
    // PORT NOTE: the C ABI struct + `cval` are ported in rustty-vt-c.
}

/// Attribute type for SGR
// PORT NOTE: the reference is `union(Tag)`. Ported as a Rust enum carrying the
// variant payloads. The C ABI tagged-union surface is ported in rustty-vt-c:
//
//     /// C ABI functions.
//     const c_union = lib.TaggedUnion(
//         lib.target,
//         @This(),
//         // Padding size for C ABI compatibility.
//         // Largest variant is Unknown.C: 2 pointers + 2 usize = 32 bytes on 64-bit.
//         // We use [8]u64 (64 bytes) to allow room for future expansion while
//         // maintaining ABI compatibility.
//         [8]u64,
//     );
//     pub const Value = c_union.Value;
//     pub const C = c_union.C;
//     pub const CValue = c_union.CValue;
//     pub const cval = c_union.cval;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Attribute<'a> {
    /// Unset all attributes
    Unset,

    /// Unknown attribute, the raw CSI command parameters are here.
    Unknown(Unknown<'a>),

    /// Bold the text.
    Bold,
    ResetBold,

    /// Italic text.
    Italic,
    ResetItalic,

    /// Faint/dim text.
    /// Note: reset faint is the same SGR code as reset bold
    Faint,

    /// Underline the text
    Underline(Underline),
    UnderlineColor(color::RGB),
    Color256Underline(u8), // "256_underline_color"
    ResetUnderlineColor,

    // Overline the text
    Overline,
    ResetOverline,

    /// Blink the text
    Blink,
    ResetBlink,

    /// Invert fg/bg colors.
    Inverse,
    ResetInverse,

    /// Invisible
    Invisible,
    ResetInvisible,

    /// Strikethrough the text.
    Strikethrough,
    ResetStrikethrough,

    /// Set foreground color as RGB values.
    DirectColorFg(color::RGB),

    /// Set background color as RGB values.
    DirectColorBg(color::RGB),

    /// Set the background/foreground as a named color attribute.
    Color8Bg(color::Name), // "8_bg"
    Color8Fg(color::Name), // "8_fg"

    /// Reset the fg/bg to their default values.
    ResetFg,
    ResetBg,

    /// Set the background/foreground as a named bright color attribute.
    Color8BrightBg(color::Name), // "8_bright_bg"
    Color8BrightFg(color::Name), // "8_bright_fg"

    /// Set background color as 256-color palette.
    Color256Bg(u8), // "256_bg"

    /// Set foreground color as 256-color palette.
    Color256Fg(u8), // "256_fg"
}

// PORT NOTE: the reference `Attribute.Tag = lib.Enum(lib.target, &.{...})` is the
// union's tag enum, listed in declaration order. The Rust enum above carries the
// same variants in the same order; an explicit `Tag` enum is not needed for the
// `.zig` target (only the C ABI uses the standalone tag). The direct-color
// target selector below names the three color tags `parseDirectColor` writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DirectColorTarget {
    DirectColorFg,
    DirectColorBg,
    UnderlineColor,
}

/// Parser parses the attributes from a list of SGR parameters.
#[derive(Clone, Copy, Debug)]
pub struct Parser<'a> {
    pub params: &'a [u16],
    pub params_sep: SepList,
    pub idx: usize,
}

impl<'a> Default for Parser<'a> {
    // PORT NOTE: reference defaults `params = &.{}`, `params_sep = .initEmpty()`,
    // `idx = 0`.
    fn default() -> Self {
        Parser {
            params: &[],
            params_sep: SepList::init_empty(),
            idx: 0,
        }
    }
}

impl<'a> Parser<'a> {
    /// Empty state parser.
    pub const EMPTY: Parser<'static> = Parser {
        params: &[],
        params_sep: SepList::init_empty(),
        idx: 0,
    };

    /// Next returns the next attribute or null if there are no more attributes.
    pub fn next(&mut self) -> Option<Attribute<'a>> {
        if self.idx >= self.params.len() {
            // We're more likely to not be done than to be done.

            // Add one to ensure we don't loop on unset
            // (defer self.idx += 1)
            let at_zero = self.idx == 0;
            self.idx += 1;

            // If we're at index zero it means we must have an empty list
            // and an empty list implicitly means unset, otherwise we're
            // done and return null.
            return if at_zero {
                Some(Attribute::Unset)
            } else {
                None
            };
        }

        let slice = &self.params[self.idx..self.params.len()];
        // Call inlined for performance reasons.
        let colon = self.params_sep.is_set(self.idx);
        self.idx += 1;

        // Our last one will have an idx be the last value.
        if slice.is_empty() {
            return None;
        }

        // If we have a colon separator then we need to ensure we're
        // parsing a value that allows it.
        if colon {
            // Colons are fairly rare in the wild.

            match slice[0] {
                4 | 38 | 48 | 58 => {}

                _ => {
                    // In real world use it's very rare
                    // that we receive an invalid sequence.

                    // Consume all the colon separated
                    // values and return them as unknown.
                    let start = self.idx;
                    while self.params_sep.is_set(self.idx) {
                        self.idx += 1;
                    }
                    self.idx += 1;
                    return Some(Attribute::Unknown(Unknown {
                        full: self.params,
                        partial: &slice[0..core::cmp::min(self.idx - start + 1, slice.len())],
                    }));
                }
            }
        }

        match slice[0] {
            0 => return Some(Attribute::Unset),

            1 => return Some(Attribute::Bold),

            2 => return Some(Attribute::Faint),

            3 => return Some(Attribute::Italic),

            4 => {
                // underline: {
                'underline: {
                    if colon {
                        // Colons are fairly rare in the wild.

                        // A trailing colon with no following sub-param
                        // (e.g. "ESC[58:4:m") leaves the colon separator
                        // bit set on the last param without adding another
                        // entry, so we can see param 4 with a colon but
                        // nothing after it.
                        if slice.len() < 2 {
                            break 'underline;
                        }

                        if self.is_colon() {
                            // Invalid/unknown SGRs are just not very likely.

                            self.consume_unknown_colon();
                            break 'underline;
                        }

                        self.idx += 1;
                        return Some(Attribute::Underline(match slice[1] {
                            0 => Underline::None,
                            1 => Underline::Single,
                            2 => Underline::Double,
                            3 => Underline::Curly,
                            4 => Underline::Dotted,
                            5 => Underline::Dashed,

                            // For unknown underline styles,
                            // just render a single underline.
                            _ => {
                                // This is quite a rare condition.
                                Underline::Single
                            }
                        }));
                    }

                    return Some(Attribute::Underline(Underline::Single));
                }
            }

            5 => return Some(Attribute::Blink),

            6 => return Some(Attribute::Blink),

            7 => return Some(Attribute::Inverse),

            8 => return Some(Attribute::Invisible),

            9 => return Some(Attribute::Strikethrough),

            21 => return Some(Attribute::Underline(Underline::Double)),

            22 => return Some(Attribute::ResetBold),

            23 => return Some(Attribute::ResetItalic),

            24 => return Some(Attribute::Underline(Underline::None)),

            25 => return Some(Attribute::ResetBlink),

            27 => return Some(Attribute::ResetInverse),

            28 => return Some(Attribute::ResetInvisible),

            29 => return Some(Attribute::ResetStrikethrough),

            30..=37 => {
                return Some(Attribute::Color8Fg(color::Name::from_int(
                    (slice[0] - 30) as u8,
                )))
            }

            38 => {
                if slice.len() >= 2 {
                    // We are very likely to have enough parameters.

                    match slice[1] {
                        // `2` indicates direct-color (r, g, b).
                        // We need at least 3 more params for this to make sense.
                        2 => {
                            if let Some(v) = self.parse_direct_color(
                                DirectColorTarget::DirectColorFg,
                                slice,
                                colon,
                            ) {
                                return Some(v);
                            }
                        }

                        // `5` indicates indexed color.
                        5 => {
                            if slice.len() >= 3 {
                                // We are very likely to have enough parameters.

                                self.idx += 2;
                                return Some(Attribute::Color256Fg(slice[2] as u8));
                            }
                        }

                        _ => {}
                    }
                }
            }

            39 => return Some(Attribute::ResetFg),

            40..=47 => {
                return Some(Attribute::Color8Bg(color::Name::from_int(
                    (slice[0] - 40) as u8,
                )))
            }

            48 => {
                if slice.len() >= 2 {
                    // We are very likely to have enough parameters.

                    match slice[1] {
                        // `2` indicates direct-color (r, g, b).
                        // We need at least 3 more params for this to make sense.
                        2 => {
                            if let Some(v) = self.parse_direct_color(
                                DirectColorTarget::DirectColorBg,
                                slice,
                                colon,
                            ) {
                                return Some(v);
                            }
                        }

                        // `5` indicates indexed color.
                        5 => {
                            if slice.len() >= 3 {
                                // We are very likely to have enough parameters.

                                self.idx += 2;
                                return Some(Attribute::Color256Bg(slice[2] as u8));
                            }
                        }

                        _ => {}
                    }
                }
            }

            49 => return Some(Attribute::ResetBg),

            53 => return Some(Attribute::Overline),
            55 => return Some(Attribute::ResetOverline),

            58 => {
                if slice.len() >= 2 {
                    // We are very likely to have enough parameters.

                    match slice[1] {
                        // `2` indicates direct-color (r, g, b).
                        // We need at least 3 more params for this to make sense.
                        2 => {
                            if let Some(v) = self.parse_direct_color(
                                DirectColorTarget::UnderlineColor,
                                slice,
                                colon,
                            ) {
                                return Some(v);
                            }
                        }

                        // `5` indicates indexed color.
                        5 => {
                            if slice.len() >= 3 {
                                // We are very likely to have enough parameters.

                                self.idx += 2;
                                return Some(Attribute::Color256Underline(slice[2] as u8));
                            }
                        }
                        _ => {}
                    }
                }
            }

            59 => return Some(Attribute::ResetUnderlineColor),

            90..=97 => {
                return Some(Attribute::Color8BrightFg(color::Name::from_int(
                    // 82 instead of 90 to offset to "bright" colors
                    (slice[0] - 82) as u8,
                )));
            }

            100..=107 => {
                return Some(Attribute::Color8BrightBg(color::Name::from_int(
                    (slice[0] - 92) as u8,
                )))
            }

            _ => {}
        }

        Some(Attribute::Unknown(Unknown {
            full: self.params,
            partial: slice,
        }))
    }

    fn parse_direct_color(
        &mut self,
        // comptime tag: Attribute.Tag,
        tag: DirectColorTarget,
        slice: &'a [u16],
        colon: bool,
    ) -> Option<Attribute<'a>> {
        // Any direct color style must have at least 5 values.
        if slice.len() < 5 {
            return None;
        }

        // Only used for direct color sets (38, 48, 58) and subparam 2.
        assert(slice[1] == 2);

        // Note: We use @truncate because the value should be 0 to 255. If
        // it isn't, the behavior is undefined so we just... truncate it.

        // If we don't have a colon, then we expect exactly 3 semicolon
        // separated values.
        if !colon {
            // Semicolons are much more common than colons.

            self.idx += 4;
            return Some(direct_color_attr(
                tag,
                color::RGB {
                    r: slice[2] as u8,
                    g: slice[3] as u8,
                    b: slice[4] as u8,
                },
            ));
        }

        // We have a colon, we might have either 5 or 6 values depending
        // on if the colorspace is present.
        let count = self.count_colon();
        match count {
            3 => {
                // This is the much more common case in the wild.

                self.idx += 4;
                Some(direct_color_attr(
                    tag,
                    color::RGB {
                        r: slice[2] as u8,
                        g: slice[3] as u8,
                        b: slice[4] as u8,
                    },
                ))
            }

            4 => {
                self.idx += 5;
                Some(direct_color_attr(
                    tag,
                    color::RGB {
                        r: slice[3] as u8,
                        g: slice[4] as u8,
                        b: slice[5] as u8,
                    },
                ))
            }

            _ => {
                // Invalid/unknown SGRs just don't happen very often at all.

                self.consume_unknown_colon();
                None
            }
        }
    }

    /// Returns true if the present position has a colon separator.
    /// This always returns false for the last value since it has no
    /// separator.
    #[inline]
    fn is_colon(&self) -> bool {
        // Call inlined for performance reasons.
        self.params_sep.is_set(self.idx)
    }

    fn count_colon(&self) -> usize {
        let mut count: usize = 0;
        let mut idx = self.idx;
        while idx < self.params.len() - 1 && self.params_sep.is_set(idx) {
            count += 1;
            idx += 1;
        }
        count
    }

    /// Consumes all the remaining parameters separated by a colon and
    /// returns an unknown attribute.
    fn consume_unknown_colon(&mut self) {
        let count = self.count_colon();
        self.idx += count + 1;
    }
}

// PORT NOTE: the reference uses `@unionInit(Attribute, @tagName(tag), rgb)` to
// build the RGB-payload variant named by the comptime `tag`. We dispatch on the
// `DirectColorTarget` selector instead.
fn direct_color_attr(tag: DirectColorTarget, rgb: color::RGB) -> Attribute<'static> {
    match tag {
        DirectColorTarget::DirectColorFg => Attribute::DirectColorFg(rgb),
        DirectColorTarget::DirectColorBg => Attribute::DirectColorBg(rgb),
        DirectColorTarget::UnderlineColor => Attribute::UnderlineColor(rgb),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_parse(params: &[u16]) -> Attribute<'_> {
        let mut p = Parser {
            params,
            ..Default::default()
        };
        p.next().unwrap()
    }

    fn test_parse_colon(params: &[u16]) -> Attribute<'_> {
        let mut p = Parser {
            params,
            ..Default::default()
        };
        // Mark all parameters except the last as having a colon after.
        for i in 0..params.len() - 1 {
            p.params_sep.set(i);
        }
        p.next().unwrap()
    }

    #[test]
    fn sgr_attribute_c_compat() {
        // test "sgr: Attribute C compat" { _ = Attribute.C; }
        // PORT NOTE: `Attribute.C` is the C ABI type, ported in rustty-vt-c.
        // The smoke reference here only forces the type to exist.
    }

    #[test]
    fn sgr_parser() {
        assert!(test_parse(&[]) == Attribute::Unset);
        assert!(test_parse(&[0]) == Attribute::Unset);

        {
            let v = test_parse(&[38, 2, 40, 44, 52]);
            assert!(matches!(v, Attribute::DirectColorFg(_)));
            if let Attribute::DirectColorFg(c) = v {
                assert_eq!(40_u8, c.r);
                assert_eq!(44_u8, c.g);
                assert_eq!(52_u8, c.b);
            }
        }

        assert!(matches!(
            test_parse(&[38, 2, 44, 52]),
            Attribute::Unknown(_)
        ));

        {
            let v = test_parse(&[48, 2, 40, 44, 52]);
            assert!(matches!(v, Attribute::DirectColorBg(_)));
            if let Attribute::DirectColorBg(c) = v {
                assert_eq!(40_u8, c.r);
                assert_eq!(44_u8, c.g);
                assert_eq!(52_u8, c.b);
            }
        }

        assert!(matches!(
            test_parse(&[48, 2, 44, 52]),
            Attribute::Unknown(_)
        ));
    }

    #[test]
    fn sgr_parser_multiple() {
        let mut p = Parser {
            params: &[0, 38, 2, 40, 44, 52],
            ..Default::default()
        };
        assert!(p.next().unwrap() == Attribute::Unset);
        assert!(matches!(p.next().unwrap(), Attribute::DirectColorFg(_)));
        assert!(p.next().is_none());
        assert!(p.next().is_none());
    }

    #[test]
    fn sgr_unsupported_with_colon() {
        let mut p = Parser {
            params: &[0, 4, 1],
            params_sep: {
                let mut list = SepList::init_empty();
                list.set(0);
                list
            },
            ..Default::default()
        };
        assert!(matches!(p.next().unwrap(), Attribute::Unknown(_)));
        assert!(p.next().unwrap() == Attribute::Bold);
        assert!(p.next().is_none());
    }

    #[test]
    fn sgr_unsupported_with_multiple_colon() {
        let mut p = Parser {
            params: &[0, 4, 2, 1],
            params_sep: {
                let mut list = SepList::init_empty();
                list.set(0);
                list.set(1);
                list
            },
            ..Default::default()
        };
        assert!(matches!(p.next().unwrap(), Attribute::Unknown(_)));
        assert!(p.next().unwrap() == Attribute::Bold);
        assert!(p.next().is_none());
    }

    #[test]
    fn sgr_bold() {
        {
            let v = test_parse(&[1]);
            assert!(v == Attribute::Bold);
        }

        {
            let v = test_parse(&[22]);
            assert!(v == Attribute::ResetBold);
        }
    }

    #[test]
    fn sgr_italic() {
        {
            let v = test_parse(&[3]);
            assert!(v == Attribute::Italic);
        }

        {
            let v = test_parse(&[23]);
            assert!(v == Attribute::ResetItalic);
        }
    }

    #[test]
    fn sgr_underline() {
        {
            let v = test_parse(&[4]);
            assert!(matches!(v, Attribute::Underline(_)));
        }

        {
            let v = test_parse(&[24]);
            assert!(matches!(v, Attribute::Underline(_)));
            assert!(matches!(v, Attribute::Underline(Underline::None)));
        }
    }

    #[test]
    fn sgr_underline_styles() {
        {
            let v = test_parse_colon(&[4, 2]);
            assert!(matches!(v, Attribute::Underline(_)));
            assert!(matches!(v, Attribute::Underline(Underline::Double)));
        }

        {
            let v = test_parse_colon(&[4, 0]);
            assert!(matches!(v, Attribute::Underline(_)));
            assert!(matches!(v, Attribute::Underline(Underline::None)));
        }

        {
            let v = test_parse_colon(&[4, 1]);
            assert!(matches!(v, Attribute::Underline(_)));
            assert!(matches!(v, Attribute::Underline(Underline::Single)));
        }

        {
            let v = test_parse_colon(&[4, 3]);
            assert!(matches!(v, Attribute::Underline(_)));
            assert!(matches!(v, Attribute::Underline(Underline::Curly)));
        }

        {
            let v = test_parse_colon(&[4, 4]);
            assert!(matches!(v, Attribute::Underline(_)));
            assert!(matches!(v, Attribute::Underline(Underline::Dotted)));
        }

        {
            let v = test_parse_colon(&[4, 5]);
            assert!(matches!(v, Attribute::Underline(_)));
            assert!(matches!(v, Attribute::Underline(Underline::Dashed)));
        }
    }

    #[test]
    fn sgr_underline_style_with_more() {
        let mut p = Parser {
            params: &[4, 2, 1],
            params_sep: {
                let mut list = SepList::init_empty();
                list.set(0);
                list
            },
            ..Default::default()
        };

        assert!(matches!(p.next().unwrap(), Attribute::Underline(_)));
        assert!(p.next().unwrap() == Attribute::Bold);
        assert!(p.next().is_none());
    }

    #[test]
    fn sgr_underline_style_with_too_many_colons() {
        let mut p = Parser {
            params: &[4, 2, 3, 1],
            params_sep: {
                let mut list = SepList::init_empty();
                list.set(0);
                list.set(1);
                list
            },
            ..Default::default()
        };

        assert!(matches!(p.next().unwrap(), Attribute::Unknown(_)));
        assert!(p.next().unwrap() == Attribute::Bold);
        assert!(p.next().is_none());
    }

    #[test]
    fn sgr_blink() {
        {
            let v = test_parse(&[5]);
            assert!(v == Attribute::Blink);
        }

        {
            let v = test_parse(&[6]);
            assert!(v == Attribute::Blink);
        }

        {
            let v = test_parse(&[25]);
            assert!(v == Attribute::ResetBlink);
        }
    }

    #[test]
    fn sgr_inverse() {
        {
            let v = test_parse(&[7]);
            assert!(v == Attribute::Inverse);
        }

        {
            let v = test_parse(&[27]);
            assert!(v == Attribute::ResetInverse);
        }
    }

    #[test]
    fn sgr_strikethrough() {
        {
            let v = test_parse(&[9]);
            assert!(v == Attribute::Strikethrough);
        }

        {
            let v = test_parse(&[29]);
            assert!(v == Attribute::ResetStrikethrough);
        }
    }

    #[test]
    fn sgr_8_color() {
        let mut p = Parser {
            params: &[31, 43, 90, 103],
            ..Default::default()
        };

        {
            let v = p.next().unwrap();
            assert!(matches!(v, Attribute::Color8Fg(_)));
            assert!(matches!(v, Attribute::Color8Fg(color::Name::Red)));
        }

        {
            let v = p.next().unwrap();
            assert!(matches!(v, Attribute::Color8Bg(_)));
            assert!(matches!(v, Attribute::Color8Bg(color::Name::Yellow)));
        }

        {
            let v = p.next().unwrap();
            assert!(matches!(v, Attribute::Color8BrightFg(_)));
            assert!(matches!(
                v,
                Attribute::Color8BrightFg(color::Name::BrightBlack)
            ));
        }

        {
            let v = p.next().unwrap();
            assert!(matches!(v, Attribute::Color8BrightBg(_)));
            assert!(matches!(
                v,
                Attribute::Color8BrightBg(color::Name::BrightYellow)
            ));
        }
    }

    #[test]
    fn sgr_256_color() {
        let mut p = Parser {
            params: &[38, 5, 161, 48, 5, 236],
            ..Default::default()
        };
        assert!(matches!(p.next().unwrap(), Attribute::Color256Fg(_)));
        assert!(matches!(p.next().unwrap(), Attribute::Color256Bg(_)));
        assert!(p.next().is_none());
    }

    #[test]
    fn sgr_256_color_underline() {
        let mut p = Parser {
            params: &[58, 5, 9],
            ..Default::default()
        };
        assert!(matches!(p.next().unwrap(), Attribute::Color256Underline(_)));
        assert!(p.next().is_none());
    }

    #[test]
    fn sgr_24_bit_bg_color() {
        {
            let v = test_parse_colon(&[48, 2, 1, 2, 3]);
            assert!(matches!(v, Attribute::DirectColorBg(_)));
            if let Attribute::DirectColorBg(c) = v {
                assert_eq!(1_u8, c.r);
                assert_eq!(2_u8, c.g);
                assert_eq!(3_u8, c.b);
            }
        }
    }

    #[test]
    fn sgr_underline_color() {
        {
            let v = test_parse_colon(&[58, 2, 1, 2, 3]);
            assert!(matches!(v, Attribute::UnderlineColor(_)));
            if let Attribute::UnderlineColor(c) = v {
                assert_eq!(1_u8, c.r);
                assert_eq!(2_u8, c.g);
                assert_eq!(3_u8, c.b);
            }
        }

        {
            let v = test_parse_colon(&[58, 2, 0, 1, 2, 3]);
            assert!(matches!(v, Attribute::UnderlineColor(_)));
            if let Attribute::UnderlineColor(c) = v {
                assert_eq!(1_u8, c.r);
                assert_eq!(2_u8, c.g);
                assert_eq!(3_u8, c.b);
            }
        }
    }

    #[test]
    fn sgr_reset_underline_color() {
        let mut p = Parser {
            params: &[59],
            ..Default::default()
        };
        assert!(p.next().unwrap() == Attribute::ResetUnderlineColor);
    }

    #[test]
    fn sgr_invisible() {
        let mut p = Parser {
            params: &[8, 28],
            ..Default::default()
        };
        assert!(p.next().unwrap() == Attribute::Invisible);
        assert!(p.next().unwrap() == Attribute::ResetInvisible);
    }

    #[test]
    fn sgr_underline_bg_and_fg() {
        let mut p = Parser {
            params: &[4, 38, 2, 255, 247, 219, 48, 2, 242, 93, 147, 4],
            ..Default::default()
        };
        {
            let v = p.next().unwrap();
            assert!(matches!(v, Attribute::Underline(_)));
            assert_eq!(Underline::Single, expect_underline(v));
        }
        {
            let v = p.next().unwrap();
            assert!(matches!(v, Attribute::DirectColorFg(_)));
            if let Attribute::DirectColorFg(c) = v {
                assert_eq!(255_u8, c.r);
                assert_eq!(247_u8, c.g);
                assert_eq!(219_u8, c.b);
            }
        }
        {
            let v = p.next().unwrap();
            assert!(matches!(v, Attribute::DirectColorBg(_)));
            if let Attribute::DirectColorBg(c) = v {
                assert_eq!(242_u8, c.r);
                assert_eq!(93_u8, c.g);
                assert_eq!(147_u8, c.b);
            }
        }
        {
            let v = p.next().unwrap();
            assert!(matches!(v, Attribute::Underline(_)));
            assert_eq!(Underline::Single, expect_underline(v));
        }
    }

    #[test]
    fn sgr_direct_color_fg_missing_color() {
        // This used to crash
        let mut p = Parser {
            params: &[38, 5],
            ..Default::default()
        };
        while p.next().is_some() {}
    }

    #[test]
    fn sgr_direct_color_bg_missing_color() {
        // This used to crash
        let mut p = Parser {
            params: &[48, 5],
            ..Default::default()
        };
        while p.next().is_some() {}
    }

    #[test]
    fn sgr_direct_fg_bg_underline_ignore_optional_color_space() {
        // These behaviors have been verified against xterm.

        // Colon version should skip the optional color space identifier
        {
            // 3 8 : 2 : Pi : Pr : Pg : Pb
            let v = test_parse_colon(&[38, 2, 0, 1, 2, 3]);
            assert!(matches!(v, Attribute::DirectColorFg(_)));
            if let Attribute::DirectColorFg(c) = v {
                assert_eq!(1_u8, c.r);
                assert_eq!(2_u8, c.g);
                assert_eq!(3_u8, c.b);
            }
        }
        {
            // 4 8 : 2 : Pi : Pr : Pg : Pb
            let v = test_parse_colon(&[48, 2, 0, 1, 2, 3]);
            assert!(matches!(v, Attribute::DirectColorBg(_)));
            if let Attribute::DirectColorBg(c) = v {
                assert_eq!(1_u8, c.r);
                assert_eq!(2_u8, c.g);
                assert_eq!(3_u8, c.b);
            }
        }
        {
            // 5 8 : 2 : Pi : Pr : Pg : Pb
            let v = test_parse_colon(&[58, 2, 0, 1, 2, 3]);
            assert!(matches!(v, Attribute::UnderlineColor(_)));
            if let Attribute::UnderlineColor(c) = v {
                assert_eq!(1_u8, c.r);
                assert_eq!(2_u8, c.g);
                assert_eq!(3_u8, c.b);
            }
        }

        // Semicolon version should not parse optional color space identifier
        {
            // 3 8 ; 2 ; Pr ; Pg ; Pb
            let v = test_parse(&[38, 2, 0, 1, 2, 3]);
            assert!(matches!(v, Attribute::DirectColorFg(_)));
            if let Attribute::DirectColorFg(c) = v {
                assert_eq!(0_u8, c.r);
                assert_eq!(1_u8, c.g);
                assert_eq!(2_u8, c.b);
            }
        }
        {
            // 4 8 ; 2 ; Pr ; Pg ; Pb
            let v = test_parse(&[48, 2, 0, 1, 2, 3]);
            assert!(matches!(v, Attribute::DirectColorBg(_)));
            if let Attribute::DirectColorBg(c) = v {
                assert_eq!(0_u8, c.r);
                assert_eq!(1_u8, c.g);
                assert_eq!(2_u8, c.b);
            }
        }
        {
            // 5 8 ; 2 ; Pr ; Pg ; Pb
            let v = test_parse(&[58, 2, 0, 1, 2, 3]);
            assert!(matches!(v, Attribute::UnderlineColor(_)));
            if let Attribute::UnderlineColor(c) = v {
                assert_eq!(0_u8, c.r);
                assert_eq!(1_u8, c.g);
                assert_eq!(2_u8, c.b);
            }
        }
    }

    #[test]
    fn sgr_direct_fg_colon_with_too_many_colons() {
        let mut p = Parser {
            params: &[38, 2, 0, 1, 2, 3, 4, 1],
            params_sep: {
                let mut list = SepList::init_empty();
                for idx in 0..6 {
                    list.set(idx);
                }
                list
            },
            ..Default::default()
        };

        assert!(matches!(p.next().unwrap(), Attribute::Unknown(_)));
        assert!(p.next().unwrap() == Attribute::Bold);
        assert!(p.next().is_none());
    }

    #[test]
    fn sgr_direct_fg_colon_with_colorspace_and_extra_param() {
        let mut p = Parser {
            params: &[38, 2, 0, 1, 2, 3, 1],
            params_sep: {
                let mut list = SepList::init_empty();
                for idx in 0..5 {
                    list.set(idx);
                }
                list
            },
            ..Default::default()
        };

        {
            let v = p.next().unwrap();
            assert!(matches!(v, Attribute::DirectColorFg(_)));
            if let Attribute::DirectColorFg(c) = v {
                assert_eq!(1_u8, c.r);
                assert_eq!(2_u8, c.g);
                assert_eq!(3_u8, c.b);
            }
        }

        assert!(p.next().unwrap() == Attribute::Bold);
        assert!(p.next().is_none());
    }

    #[test]
    fn sgr_direct_fg_colon_no_colorspace_and_extra_param() {
        let mut p = Parser {
            params: &[38, 2, 1, 2, 3, 1],
            params_sep: {
                let mut list = SepList::init_empty();
                for idx in 0..4 {
                    list.set(idx);
                }
                list
            },
            ..Default::default()
        };

        {
            let v = p.next().unwrap();
            assert!(matches!(v, Attribute::DirectColorFg(_)));
            if let Attribute::DirectColorFg(c) = v {
                assert_eq!(1_u8, c.r);
                assert_eq!(2_u8, c.g);
                assert_eq!(3_u8, c.b);
            }
        }

        assert!(p.next().unwrap() == Attribute::Bold);
        assert!(p.next().is_none());
    }

    // Kakoune sent this complex SGR sequence that caused invalid behavior.
    #[test]
    fn sgr_kakoune_input() {
        // This used to crash
        let mut p = Parser {
            params: &[0, 4, 3, 38, 2, 175, 175, 215, 58, 2, 0, 190, 80, 70],
            params_sep: {
                let mut list = SepList::init_empty();
                list.set(1);
                list.set(8);
                list.set(9);
                list.set(10);
                list.set(11);
                list.set(12);
                list
            },
            ..Default::default()
        };

        {
            let v = p.next().unwrap();
            assert!(v == Attribute::Unset);
        }
        {
            let v = p.next().unwrap();
            assert!(matches!(v, Attribute::Underline(_)));
            assert_eq!(Underline::Curly, expect_underline(v));
        }
        {
            let v = p.next().unwrap();
            assert!(matches!(v, Attribute::DirectColorFg(_)));
            if let Attribute::DirectColorFg(c) = v {
                assert_eq!(175_u8, c.r);
                assert_eq!(175_u8, c.g);
                assert_eq!(215_u8, c.b);
            }
        }
        {
            let v = p.next().unwrap();
            assert!(matches!(v, Attribute::UnderlineColor(_)));
            if let Attribute::UnderlineColor(c) = v {
                assert_eq!(190_u8, c.r);
                assert_eq!(80_u8, c.g);
                assert_eq!(70_u8, c.b);
            }
        }

        //try testing.expect(p.next() == null);
    }

    // Discussion #5930, another input sent by kakoune
    #[test]
    fn sgr_kakoune_input_issue_underline_fg_and_bg() {
        // echo -e "\033[4:3;38;2;51;51;51;48;2;170;170;170;58;2;255;97;136mset everything in one sequence, broken\033[m"

        // This used to crash
        let mut p = Parser {
            params: &[
                4, 3, 38, 2, 51, 51, 51, 48, 2, 170, 170, 170, 58, 2, 255, 97, 136,
            ],
            params_sep: {
                let mut list = SepList::init_empty();
                list.set(0);
                list
            },
            ..Default::default()
        };

        {
            let v = p.next().unwrap();
            assert!(matches!(v, Attribute::Underline(_)));
            assert_eq!(Underline::Curly, expect_underline(v));
        }

        {
            let v = p.next().unwrap();
            assert!(matches!(v, Attribute::DirectColorFg(_)));
            if let Attribute::DirectColorFg(c) = v {
                assert_eq!(51_u8, c.r);
                assert_eq!(51_u8, c.g);
                assert_eq!(51_u8, c.b);
            }
        }

        {
            let v = p.next().unwrap();
            assert!(matches!(v, Attribute::DirectColorBg(_)));
            if let Attribute::DirectColorBg(c) = v {
                assert_eq!(170_u8, c.r);
                assert_eq!(170_u8, c.g);
                assert_eq!(170_u8, c.b);
            }
        }

        {
            let v = p.next().unwrap();
            assert!(matches!(v, Attribute::UnderlineColor(_)));
            if let Attribute::UnderlineColor(c) = v {
                assert_eq!(255_u8, c.r);
                assert_eq!(97_u8, c.g);
                assert_eq!(136_u8, c.b);
            }
        }

        assert!(p.next().is_none());
    }

    // Fuzz crash: afl-out/stream/default/crashes/id:000021
    // Input "ESC [ 5 8 : 4 : m" produces params [58, 4] with colon
    // separator bits set at indices 0 and 1. The trailing colon causes
    // the second iteration to see param 4 (underline) with a colon,
    // triggering assert(slice.len >= 2) with slice.len == 1.
    #[test]
    fn sgr_underline_colon_with_trailing_separator_and_short_slice() {
        let mut p = Parser {
            params: &[58, 4],
            params_sep: {
                let mut list = SepList::init_empty();
                list.set(0);
                list.set(1);
                list
            },
            ..Default::default()
        };

        // 58:4 is not a valid underline color (sub-param 4 is not 2 or 5),
        // so it falls through as unknown.
        assert!(matches!(p.next().unwrap(), Attribute::Unknown(_)));

        // Param 4 with a trailing colon but no sub-param is malformed,
        // so it also falls through as unknown rather than panicking.
        assert!(matches!(p.next().unwrap(), Attribute::Unknown(_)));

        assert!(p.next().is_none());
    }

    // Helper to extract the Underline payload for `expectEqual(Underline, v.underline)`.
    fn expect_underline(v: Attribute<'_>) -> Underline {
        match v {
            Attribute::Underline(u) => u,
            _ => unreachable!(),
        }
    }
}
