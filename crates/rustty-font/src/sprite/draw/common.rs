//! This file contains a set of useful helper functions
//! and types for drawing our sprite font glyphs. These
//! are generally applicable to multiple sets of glyphs
//! rather than being single-use.

// const std = @import("std");
// const Allocator = std.mem.Allocator;
//
// const font = @import("../../main.zig");
//
// const log = std.log.scoped(.sprite_font);
//
// PORT NOTE: `font = ../../main.zig` becomes the local crate (`crate::metrics`,
// `crate::sprite`). The `log` scope has no use sites in this module (it is used
// by the per-glyph draw modules ported in later batches), so it is preserved as
// a commented reference rather than an unused binding.
#![allow(non_snake_case)]
// PORT NOTE: the reference constant names (`one_eighth`, `Fraction.eighths`,
// etc.) are lowercase; preserved verbatim rather than upper-cased.
#![allow(non_upper_case_globals)]

use crate::metrics::Metrics;
use crate::sprite::canvas::{Canvas, Color};

// Utility names for common fractions
// pub const one_eighth: f64 = 0.125;
pub const one_eighth: f64 = 0.125;
// pub const one_quarter: f64 = 0.25;
pub const one_quarter: f64 = 0.25;
// pub const one_third: f64 = (1.0 / 3.0);
pub const one_third: f64 = 1.0 / 3.0;
// pub const three_eighths: f64 = 0.375;
pub const three_eighths: f64 = 0.375;
// pub const half: f64 = 0.5;
pub const half: f64 = 0.5;
// pub const five_eighths: f64 = 0.625;
pub const five_eighths: f64 = 0.625;
// pub const two_thirds: f64 = (2.0 / 3.0);
pub const two_thirds: f64 = 2.0 / 3.0;
// pub const three_quarters: f64 = 0.75;
pub const three_quarters: f64 = 0.75;
// pub const seven_eighths: f64 = 0.875;
pub const seven_eighths: f64 = 0.875;

/// The thickness of a line.
// pub const Thickness = enum {
//     super_light,
//     light,
//     heavy,
//     ...
// };
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Thickness {
    SuperLight,
    Light,
    Heavy,
}

impl Thickness {
    /// Calculate the real height of a line based on its
    /// thickness and a base thickness value. The base
    /// thickness value is expected to be in pixels.
    // pub fn height(self: Thickness, base: u32) u32 {
    pub fn height(self, base: u32) -> u32 {
        // return switch (self) {
        //     .super_light => @max(base / 2, 1),
        //     .light => base,
        //     .heavy => base * 2,
        // };
        match self {
            Thickness::SuperLight => (base / 2).max(1),
            Thickness::Light => base,
            Thickness::Heavy => base * 2,
        }
    }
}

/// Shades.
// pub const Shade = enum(u8) {
//     off = 0x00,
//     light = 0x40,
//     medium = 0x80,
//     dark = 0xc0,
//     on = 0xff,
//     _,
// };
//
// PORT NOTE: open (`_`) `enum(u8)`; faithfully modeled as a `u8` newtype with
// the named variants as associated constants (`@intFromEnum(shade)` == `shade.0`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Shade(pub u8);

impl Shade {
    pub const OFF: Shade = Shade(0x00);
    pub const LIGHT: Shade = Shade(0x40);
    pub const MEDIUM: Shade = Shade(0x80);
    pub const DARK: Shade = Shade(0xc0);
    pub const ON: Shade = Shade(0xff);
}

/// Applicable to any set of glyphs with features
/// that may be present or not in each quadrant.
// pub const Quads = packed struct(u4) {
//     tl: bool = false,
//     tr: bool = false,
//     bl: bool = false,
//     br: bool = false,
// };
//
// PORT NOTE: the reference packs four flags into a `u4`; the Rust port carries
// them as four `bool` fields (default `false`), which preserves the semantics
// used at every call site.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Quads {
    pub tl: bool,
    pub tr: bool,
    pub bl: bool,
    pub br: bool,
}

/// A corner of a cell.
// pub const Corner = enum(u2) {
//     tl,
//     tr,
//     bl,
//     br,
// };
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Corner {
    Tl,
    Tr,
    Bl,
    Br,
}

/// An edge of a cell.
// pub const Edge = enum(u2) {
//     top,
//     left,
//     bottom,
//     right,
// };
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Edge {
    Top,
    Left,
    Bottom,
    Right,
}

/// Alignment of a figure within a cell.
// pub const Alignment = struct {
//     horizontal: enum {
//         left,
//         right,
//         center,
//     } = .center,
//
//     vertical: enum {
//         top,
//         bottom,
//         middle,
//     } = .middle,
//     ...
// };
//
// PORT NOTE: Zig's anonymous field enums are named `Horizontal`/`Vertical` here
// so they can be referenced; the field defaults (`center`/`middle`) become the
// `Default` impl.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Horizontal {
    Left,
    Right,
    Center,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Vertical {
    Top,
    Bottom,
    Middle,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Alignment {
    pub horizontal: Horizontal,
    pub vertical: Vertical,
}

impl Default for Alignment {
    fn default() -> Alignment {
        Alignment {
            horizontal: Horizontal::Center,
            vertical: Vertical::Middle,
        }
    }
}

impl Alignment {
    // pub const upper: Alignment = .{ .vertical = .top };
    pub const UPPER: Alignment = Alignment {
        horizontal: Horizontal::Center,
        vertical: Vertical::Top,
    };
    // pub const lower: Alignment = .{ .vertical = .bottom };
    pub const LOWER: Alignment = Alignment {
        horizontal: Horizontal::Center,
        vertical: Vertical::Bottom,
    };
    // pub const left: Alignment = .{ .horizontal = .left };
    pub const LEFT: Alignment = Alignment {
        horizontal: Horizontal::Left,
        vertical: Vertical::Middle,
    };
    // pub const right: Alignment = .{ .horizontal = .right };
    pub const RIGHT: Alignment = Alignment {
        horizontal: Horizontal::Right,
        vertical: Vertical::Middle,
    };

    // pub const upper_left: Alignment = .{ .vertical = .top, .horizontal = .left };
    pub const UPPER_LEFT: Alignment = Alignment {
        vertical: Vertical::Top,
        horizontal: Horizontal::Left,
    };
    // pub const upper_right: Alignment = .{ .vertical = .top, .horizontal = .right };
    pub const UPPER_RIGHT: Alignment = Alignment {
        vertical: Vertical::Top,
        horizontal: Horizontal::Right,
    };
    // pub const lower_left: Alignment = .{ .vertical = .bottom, .horizontal = .left };
    pub const LOWER_LEFT: Alignment = Alignment {
        vertical: Vertical::Bottom,
        horizontal: Horizontal::Left,
    };
    // pub const lower_right: Alignment = .{ .vertical = .bottom, .horizontal = .right };
    pub const LOWER_RIGHT: Alignment = Alignment {
        vertical: Vertical::Bottom,
        horizontal: Horizontal::Right,
    };

    // pub const center: Alignment = .{};
    pub const CENTER: Alignment = Alignment {
        horizontal: Horizontal::Center,
        vertical: Vertical::Middle,
    };

    // pub const upper_center = upper;
    pub const UPPER_CENTER: Alignment = Self::UPPER;
    // pub const lower_center = lower;
    pub const LOWER_CENTER: Alignment = Self::LOWER;
    // pub const middle_left = left;
    pub const MIDDLE_LEFT: Alignment = Self::LEFT;
    // pub const middle_right = right;
    pub const MIDDLE_RIGHT: Alignment = Self::RIGHT;
    // pub const middle_center: Alignment = center;
    pub const MIDDLE_CENTER: Alignment = Self::CENTER;

    // pub const top = upper;
    pub const TOP: Alignment = Self::UPPER;
    // pub const bottom = lower;
    pub const BOTTOM: Alignment = Self::LOWER;
    // pub const center_top = top;
    pub const CENTER_TOP: Alignment = Self::TOP;
    // pub const center_bottom = bottom;
    pub const CENTER_BOTTOM: Alignment = Self::BOTTOM;

    // pub const top_left = upper_left;
    pub const TOP_LEFT: Alignment = Self::UPPER_LEFT;
    // pub const top_right = upper_right;
    pub const TOP_RIGHT: Alignment = Self::UPPER_RIGHT;
    // pub const bottom_left = lower_left;
    pub const BOTTOM_LEFT: Alignment = Self::LOWER_LEFT;
    // pub const bottom_right = lower_right;
    pub const BOTTOM_RIGHT: Alignment = Self::LOWER_RIGHT;
}

/// A value that indicates some fraction across
/// the cell either horizontally or vertically.
///
/// This has some redundant names in it so that you can
/// use whichever one feels most semantically appropriate.
// pub const Fraction = enum { ... };
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Fraction {
    // Names for the min edge
    Start,
    Left,
    Top,
    Zero,

    // Names based on eighths
    Eighth,
    OneEighth,
    TwoEighths,
    ThreeEighths,
    FourEighths,
    FiveEighths,
    SixEighths,
    SevenEighths,

    // Names based on quarters
    Quarter,
    OneQuarter,
    TwoQuarters,
    ThreeQuarters,

    // Names based on thirds
    Third,
    OneThird,
    TwoThirds,

    // Names based on halves
    Half,
    OneHalf,

    // Alternative names for 1/2
    Center,
    Middle,

    // Names for the max edge
    End,
    Right,
    Bottom,
    One,
    Full,
}

impl Fraction {
    /// This can be indexed to get the fraction for `i/8`.
    // pub const eighths: [9]Fraction = .{ ... };
    pub const eighths: [Fraction; 9] = [
        Fraction::Zero,
        Fraction::OneEighth,
        Fraction::TwoEighths,
        Fraction::ThreeEighths,
        Fraction::FourEighths,
        Fraction::FiveEighths,
        Fraction::SixEighths,
        Fraction::SevenEighths,
        Fraction::One,
    ];

    /// This can be indexed to get the fraction for `i/4`.
    // pub const quarters: [5]Fraction = .{ ... };
    pub const quarters: [Fraction; 5] = [
        Fraction::Zero,
        Fraction::OneQuarter,
        Fraction::TwoQuarters,
        Fraction::ThreeQuarters,
        Fraction::One,
    ];

    /// This can be indexed to get the fraction for `i/3`.
    // pub const thirds: [4]Fraction = .{ ... };
    pub const thirds: [Fraction; 4] = [
        Fraction::Zero,
        Fraction::OneThird,
        Fraction::TwoThirds,
        Fraction::One,
    ];

    /// This can be indexed to get the fraction for `i/2`.
    // pub const halves: [3]Fraction = .{ ... };
    pub const halves: [Fraction; 3] = [Fraction::Zero, Fraction::OneHalf, Fraction::One];

    /// Get the x position for this fraction across a particular
    /// size (width or height), assuming it will be used as the
    /// min (left/top) coordinate for a block.
    ///
    /// `size` can be any integer type, since it will be coerced
    // pub inline fn min(self: Fraction, size: anytype) i32 {
    //
    // PORT NOTE: `size: anytype` (any integer) is modeled as a generic `T` with
    // an exact `f64: From<T>` bound, mirroring `@floatFromInt`.
    #[inline]
    pub fn min<T>(self, size: T) -> i32
    where
        f64: From<T>,
    {
        // const s: f64 = @as(f64, @floatFromInt(size));
        let s: f64 = f64::from(size);
        // For min coordinates, we want to align with the complementary
        // fraction taken from the end, this ensures that rounding evens
        // out, so that for example, if `size` is `7`, and we're looking
        // at the `half` line, `size - round((1 - 0.5) * size)` => `3`;
        // whereas the max coordinate directly rounds, which means that
        // both `start` -> `half` and `half` -> `end` will be 4px, from
        // `0` -> `4` and `3` -> `7`.
        // return @intFromFloat(s - @round((1.0 - self.fraction()) * s));
        (s - ((1.0 - self.fraction()) * s).round()) as i32
    }

    /// Get the x position for this fraction across a particular
    /// size (width or height), assuming it will be used as the
    /// max (right/bottom) coordinate for a block.
    ///
    /// `size` can be any integer type, since it will be coerced
    /// with `@floatFromInt`.
    // pub inline fn max(self: Fraction, size: anytype) i32 {
    #[inline]
    pub fn max<T>(self, size: T) -> i32
    where
        f64: From<T>,
    {
        // const s: f64 = @as(f64, @floatFromInt(size));
        let s: f64 = f64::from(size);
        // See explanation of why these are different in `min`.
        // return @intFromFloat(@round(self.fraction() * s));
        (self.fraction() * s).round() as i32
    }

    /// Get this fraction across a particular size (width/height).
    /// If you need an integer, use `min` or `max` instead, since
    /// they contain special logic for consistent alignment. This
    /// is for when you're drawing with paths and don't care about
    /// pixel alignment.
    ///
    /// `size` can be any integer type, since it will be coerced
    /// with `@floatFromInt`.
    // pub inline fn float(self: Fraction, size: anytype) f64 {
    #[inline]
    pub fn float<T>(self, size: T) -> f64
    where
        f64: From<T>,
    {
        // return self.fraction() * @as(f64, @floatFromInt(size));
        self.fraction() * f64::from(size)
    }

    /// Get a float for the fraction this represents.
    // pub inline fn fraction(self: Fraction) f64 {
    #[inline]
    pub fn fraction(self) -> f64 {
        // return switch (self) {
        match self {
            Fraction::Start | Fraction::Left | Fraction::Top | Fraction::Zero => 0.0,

            Fraction::Eighth | Fraction::OneEighth => 0.125,

            Fraction::Quarter | Fraction::OneQuarter | Fraction::TwoEighths => 0.25,

            Fraction::Third | Fraction::OneThird => 1.0 / 3.0,

            Fraction::ThreeEighths => 0.375,

            Fraction::Half
            | Fraction::OneHalf
            | Fraction::TwoQuarters
            | Fraction::FourEighths
            | Fraction::Center
            | Fraction::Middle => 0.5,

            Fraction::FiveEighths => 0.625,

            Fraction::TwoThirds => 2.0 / 3.0,

            Fraction::ThreeQuarters | Fraction::SixEighths => 0.75,

            Fraction::SevenEighths => 0.875,

            Fraction::End | Fraction::Right | Fraction::Bottom | Fraction::One | Fraction::Full => {
                1.0
            }
        }
    }
}

/// Fill a section of the cell, specified by a
/// horizontal and vertical pair of fraction lines.
// pub fn fill(
//     metrics: font.Metrics,
//     canvas: *font.sprite.Canvas,
//     x0: Fraction,
//     x1: Fraction,
//     y0: Fraction,
//     y1: Fraction,
// ) void {
pub fn fill(
    metrics: Metrics,
    canvas: &mut Canvas,
    x0: Fraction,
    x1: Fraction,
    y0: Fraction,
    y1: Fraction,
) {
    // canvas.r#box(
    //     x0.min(metrics.cell_width),
    //     y0.min(metrics.cell_height),
    //     x1.max(metrics.cell_width),
    //     y1.max(metrics.cell_height),
    //     .on,
    // );
    canvas.r#box(
        x0.min(metrics.cell_width),
        y0.min(metrics.cell_height),
        x1.max(metrics.cell_width),
        y1.max(metrics.cell_height),
        Color::ON,
    );
}

/// Centered vertical line of the provided thickness.
// pub fn vlineMiddle(
//     metrics: font.Metrics,
//     canvas: *font.sprite.Canvas,
//     thickness: Thickness,
// ) void {
pub fn vlineMiddle(metrics: Metrics, canvas: &mut Canvas, thickness: Thickness) {
    // const thick_px = thickness.height(metrics.box_thickness);
    let thick_px = thickness.height(metrics.box_thickness);
    // vline(
    //     canvas,
    //     0,
    //     @intCast(metrics.cell_height),
    //     @intCast((metrics.cell_width -| thick_px) / 2),
    //     thick_px,
    // );
    vline(
        canvas,
        0,
        metrics.cell_height as i32,
        (metrics.cell_width.saturating_sub(thick_px) / 2) as i32,
        thick_px,
    );
}

/// Centered horizontal line of the provided thickness.
// pub fn hlineMiddle(
//     metrics: font.Metrics,
//     canvas: *font.sprite.Canvas,
//     thickness: Thickness,
// ) void {
pub fn hlineMiddle(metrics: Metrics, canvas: &mut Canvas, thickness: Thickness) {
    // const thick_px = thickness.height(metrics.box_thickness);
    let thick_px = thickness.height(metrics.box_thickness);
    // hline(
    //     canvas,
    //     0,
    //     @intCast(metrics.cell_width),
    //     @intCast((metrics.cell_height -| thick_px) / 2),
    //     thick_px,
    // );
    hline(
        canvas,
        0,
        metrics.cell_width as i32,
        (metrics.cell_height.saturating_sub(thick_px) / 2) as i32,
        thick_px,
    );
}

/// Vertical line with the left edge at `x`, between `y1` and `y2`.
// pub fn vline(
//     canvas: *font.sprite.Canvas,
//     y1: i32,
//     y2: i32,
//     x: i32,
//     thickness_px: u32,
// ) void {
pub fn vline(canvas: &mut Canvas, y1: i32, y2: i32, x: i32, thickness_px: u32) {
    // canvas.r#box(x, y1, x + @as(i32, @intCast(thickness_px)), y2, .on);
    canvas.r#box(x, y1, x + thickness_px as i32, y2, Color::ON);
}

/// Horizontal line with the top edge at `y`, between `x1` and `x2`.
// pub fn hline(
//     canvas: *font.sprite.Canvas,
//     x1: i32,
//     x2: i32,
//     y: i32,
//     thickness_px: u32,
// ) void {
pub fn hline(canvas: &mut Canvas, x1: i32, x2: i32, y: i32, thickness_px: u32) {
    // canvas.r#box(x1, y, x2, y + @as(i32, @intCast(thickness_px)), .on);
    canvas.r#box(x1, y, x2, y + thickness_px as i32, Color::ON);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sprite::canvas::Color;

    // The `min`/`max` alignment logic worked through on the `size == 7`, `half`
    // example spelled out in the `min` doc comment: `start -> half` and
    // `half -> end` are both 4px (0..4 and 3..7).
    #[test]
    fn fraction_min_max_alignment() {
        assert_eq!(Fraction::Start.max(7u32), 0);
        assert_eq!(Fraction::Half.max(7u32), 4);
        assert_eq!(Fraction::Half.min(7u32), 3);
        assert_eq!(Fraction::End.max(7u32), 7);
    }

    // Redundant fraction names resolve to the same float.
    #[test]
    fn fraction_aliases() {
        assert_eq!(Fraction::Half.fraction(), 0.5);
        assert_eq!(Fraction::OneHalf.fraction(), 0.5);
        assert_eq!(Fraction::TwoQuarters.fraction(), 0.5);
        assert_eq!(Fraction::FourEighths.fraction(), 0.5);
        assert_eq!(Fraction::Center.fraction(), 0.5);
        assert_eq!(Fraction::Middle.fraction(), 0.5);
        assert_eq!(Fraction::eighths[4], Fraction::FourEighths);
        assert_eq!(Fraction::quarters[2], Fraction::TwoQuarters);
    }

    // Thickness scales off a base pixel value.
    #[test]
    fn thickness_height() {
        assert_eq!(Thickness::SuperLight.height(4), 2);
        assert_eq!(Thickness::SuperLight.height(1), 1); // @max(0, 1)
        assert_eq!(Thickness::Light.height(3), 3);
        assert_eq!(Thickness::Heavy.height(3), 6);
    }

    // A minimal 8x8 `Metrics` for the drawing-helper tests.
    fn test_metrics() -> Metrics {
        Metrics {
            cell_width: 8,
            cell_height: 8,
            cell_baseline: 0,
            underline_position: 0,
            underline_thickness: 0,
            strikethrough_position: 0,
            strikethrough_thickness: 0,
            overline_position: 0,
            overline_thickness: 0,
            box_thickness: 2,
            cursor_thickness: 1,
            cursor_height: 0,
            icon_height: 0.0,
            icon_height_single: 0.0,
            face_width: 0.0,
            face_height: 0.0,
            face_y: 0.0,
        }
    }

    // `fill` maps fraction lines onto cell pixels through `canvas.box`.
    #[test]
    fn fill_writes_box() {
        let metrics = test_metrics();
        let mut canvas = Canvas::init(metrics.cell_width, metrics.cell_height, 0, 0).unwrap();
        // Fill the left half of the cell: box(0,0, 4,8).
        fill(
            metrics,
            &mut canvas,
            Fraction::Start,
            Fraction::Half,
            Fraction::Start,
            Fraction::End,
        );

        // Left column is filled, right column is not.
        canvas.pixel(0, 0, Color::ON); // no-op re-set to exercise `pixel`
        assert!(canvas.filled(0, 0));
        assert!(canvas.filled(3, 7));
        assert!(!canvas.filled(4, 0));
        assert!(!canvas.filled(7, 7));
    }

    // `hlineMiddle` draws a centered horizontal band of `box_thickness` height.
    #[test]
    fn hline_middle_centered() {
        let metrics = test_metrics();
        let mut canvas = Canvas::init(metrics.cell_width, metrics.cell_height, 0, 0).unwrap();
        // thick_px = Light.height(2) = 2; y = (8 - 2) / 2 = 3; rows 3..5.
        hlineMiddle(metrics, &mut canvas, Thickness::Light);
        assert!(!canvas.filled(0, 2));
        assert!(canvas.filled(0, 3));
        assert!(canvas.filled(7, 4));
        assert!(!canvas.filled(0, 5));
    }
}
