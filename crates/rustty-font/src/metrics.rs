// PORT NOTE: ghostty src/font/Metrics.zig (`const Metrics = @This();`). The Zig file
// imports `std` and `assert = quirks.inlineAssert`; here the assert seam is
// `rustty_util::quirks::inlineAssert` and the `ModifierSet` hash map is the std
// `HashMap` (the reference uses `std.AutoHashMapUnmanaged`).

use rustty_datastruct::comparison::{expect_approx_equal, ApproxEqual, Error as ApproxError};
use rustty_util::quirks::inlineAssert as assert;
use std::collections::HashMap;

// PORT NOTE (comparison glue): the reference's `comparison.expectApproxEqual`
// (datastruct/comparison.zig) is comptime-generic and recurses over every struct
// field. Rust dispatches that via the `ApproxEqual` trait, so `Metrics` and
// `FaceMetrics` (compared field-by-field by the Collection "metrics"/"face
// metrics" tests in `rustty-font-shaper`) implement it here — the orphan rule
// requires the impl live in this crate, which owns the types and deps datastruct.
// Each impl reproduces the reference struct arm: `inline for (fields) |f| try
// expectApproxEqual(@field(expected, f), @field(actual, f))`.
impl ApproxEqual for Metrics {
    fn approx_equal(&self, actual: &Self) -> Result<(), ApproxError> {
        expect_approx_equal(&self.cell_width, &actual.cell_width)?;
        expect_approx_equal(&self.cell_height, &actual.cell_height)?;
        expect_approx_equal(&self.cell_baseline, &actual.cell_baseline)?;
        expect_approx_equal(&self.underline_position, &actual.underline_position)?;
        expect_approx_equal(&self.underline_thickness, &actual.underline_thickness)?;
        expect_approx_equal(&self.strikethrough_position, &actual.strikethrough_position)?;
        expect_approx_equal(
            &self.strikethrough_thickness,
            &actual.strikethrough_thickness,
        )?;
        expect_approx_equal(&self.overline_position, &actual.overline_position)?;
        expect_approx_equal(&self.overline_thickness, &actual.overline_thickness)?;
        expect_approx_equal(&self.box_thickness, &actual.box_thickness)?;
        expect_approx_equal(&self.cursor_thickness, &actual.cursor_thickness)?;
        expect_approx_equal(&self.cursor_height, &actual.cursor_height)?;
        expect_approx_equal(&self.icon_height, &actual.icon_height)?;
        expect_approx_equal(&self.icon_height_single, &actual.icon_height_single)?;
        expect_approx_equal(&self.face_width, &actual.face_width)?;
        expect_approx_equal(&self.face_height, &actual.face_height)?;
        expect_approx_equal(&self.face_y, &actual.face_y)?;
        Ok(())
    }
}

impl ApproxEqual for FaceMetrics {
    fn approx_equal(&self, actual: &Self) -> Result<(), ApproxError> {
        expect_approx_equal(&self.px_per_em, &actual.px_per_em)?;
        expect_approx_equal(&self.cell_width, &actual.cell_width)?;
        expect_approx_equal(&self.ascent, &actual.ascent)?;
        expect_approx_equal(&self.descent, &actual.descent)?;
        expect_approx_equal(&self.line_gap, &actual.line_gap)?;
        expect_approx_equal(&self.underline_position, &actual.underline_position)?;
        expect_approx_equal(&self.underline_thickness, &actual.underline_thickness)?;
        expect_approx_equal(&self.strikethrough_position, &actual.strikethrough_position)?;
        expect_approx_equal(
            &self.strikethrough_thickness,
            &actual.strikethrough_thickness,
        )?;
        expect_approx_equal(&self.cap_height, &actual.cap_height)?;
        expect_approx_equal(&self.ex_height, &actual.ex_height)?;
        expect_approx_equal(&self.ascii_height, &actual.ascii_height)?;
        expect_approx_equal(&self.ic_width, &actual.ic_width)?;
        Ok(())
    }
}

/// Recommended cell width and height for a monospace grid using this font.
// PORT NOTE: the reference declares these as bare struct fields on the
// `Metrics = @This()` file-struct. In Rust they are fields of a `Metrics`
// struct. The field doc comments are preserved verbatim.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Metrics {
    /// Recommended cell width and height for a monospace grid using this font.
    pub cell_width: u32,
    pub cell_height: u32,

    /// Distance in pixels from the bottom of the cell to the text baseline.
    pub cell_baseline: u32,

    /// Distance in pixels from the top of the cell to the top of the underline.
    pub underline_position: u32,
    /// Thickness in pixels of the underline.
    pub underline_thickness: u32,

    /// Distance in pixels from the top of the cell to the top of the strikethrough.
    pub strikethrough_position: u32,
    /// Thickness in pixels of the strikethrough.
    pub strikethrough_thickness: u32,

    /// Distance in pixels from the top of the cell to the top of the overline.
    /// Can be negative to adjust the position above the top of the cell.
    pub overline_position: i32,
    /// Thickness in pixels of the overline.
    pub overline_thickness: u32,

    /// Thickness in pixels of box drawing characters.
    pub box_thickness: u32,

    /// The thickness in pixels of the cursor sprite. This has a default value
    /// because it is not determined by fonts but rather by user configuration.
    // PORT NOTE: the reference gives this a default `= 1` in the struct; Rust
    // struct fields have no per-field defaults, so the default is applied where
    // a `Metrics` is constructed without specifying it (see `init`).
    pub cursor_thickness: u32,

    /// The height in pixels of the cursor sprite.
    pub cursor_height: u32,

    /// The constraint height for nerd fonts icons.
    pub icon_height: f64,

    /// The constraint height for nerd fonts icons limited to a single cell width.
    pub icon_height_single: f64,

    /// The unrounded face width, used in scaling calculations.
    pub face_width: f64,

    /// The unrounded face height, used in scaling calculations.
    pub face_height: f64,

    /// The offset from the bottom of the cell to the bottom
    /// of the face's bounding box, based on the rounded and
    /// potentially adjusted cell height.
    pub face_y: f64,
}

/// Minimum acceptable values for some fields to prevent modifiers
/// from being able to, for example, cause 0-thickness underlines.
// PORT NOTE: the reference is `const Minimums = struct { const cell_width = 1; ... }`,
// a namespace of comptime constants consumed by `clamp` via `@hasDecl`. In Rust
// `clamp` is hand-unrolled per field (no comptime field iteration), so these are
// kept as a module of associated constants for fidelity to the source listing.
mod minimums {
    pub const CELL_WIDTH: u32 = 1;
    pub const CELL_HEIGHT: u32 = 1;
    pub const UNDERLINE_THICKNESS: u32 = 1;
    pub const STRIKETHROUGH_THICKNESS: u32 = 1;
    pub const OVERLINE_THICKNESS: u32 = 1;
    pub const BOX_THICKNESS: u32 = 1;
    pub const CURSOR_THICKNESS: u32 = 1;
    pub const CURSOR_HEIGHT: u32 = 1;
    pub const ICON_HEIGHT: f64 = 1.0;
    pub const ICON_HEIGHT_SINGLE: f64 = 1.0;
    pub const FACE_HEIGHT: f64 = 1.0;
    pub const FACE_WIDTH: f64 = 1.0;
}

/// Metrics extracted from a font face, based on
/// the metadata tables and glyph measurements.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FaceMetrics {
    /// Pixels per em, dividing the other values in this struct by this should
    /// yield sizes in ems, to allow comparing metrics from faces of different
    /// sizes.
    pub px_per_em: f64,

    /// The minimum cell width that can contain any glyph in the ASCII range.
    ///
    /// Determined by measuring all printable glyphs in the ASCII range.
    pub cell_width: f64,

    /// The typographic ascent metric from the font.
    /// This represents the maximum vertical position of the highest ascender.
    ///
    /// Relative to the baseline, in px, +Y=up
    pub ascent: f64,

    /// The typographic descent metric from the font.
    /// This represents the minimum vertical position of the lowest descender.
    ///
    /// Relative to the baseline, in px, +Y=up
    ///
    /// Note:
    /// As this value is generally below the baseline, it is typically negative.
    pub descent: f64,

    /// The typographic line gap (aka "leading") metric from the font.
    /// This represents the additional space to be added between lines in
    /// addition to the space defined by the ascent and descent metrics.
    ///
    /// Positive value in px
    pub line_gap: f64,

    /// The TOP of the underline stroke.
    ///
    /// Relative to the baseline, in px, +Y=up
    pub underline_position: Option<f64>,

    /// The thickness of the underline stroke in px.
    pub underline_thickness: Option<f64>,

    /// The TOP of the strikethrough stroke.
    ///
    /// Relative to the baseline, in px, +Y=up
    pub strikethrough_position: Option<f64>,

    /// The thickness of the strikethrough stroke in px.
    pub strikethrough_thickness: Option<f64>,

    /// The height of capital letters in the font, either derived from
    /// a provided cap height metric or measured from the height of the
    /// capital H glyph.
    pub cap_height: Option<f64>,

    /// The height of lowercase letters in the font, either derived from
    /// a provided ex height metric or measured from the height of the
    /// lowercase x glyph.
    pub ex_height: Option<f64>,

    /// The measured height of the bounding box containing all printable
    /// ASCII characters. This can be different from ascent - descent for
    /// two reasons: non-letter symbols like @ and $ often exceed the
    /// the ascender and descender lines; and fonts often bake the line
    /// gap into the ascent and descent metrics (as per, e.g., the Google
    /// Fonts guidelines: https://simoncozens.github.io/gf-docs/metrics.html).
    ///
    /// Positive value in px
    pub ascii_height: Option<f64>,

    /// The width of the character "水" (CJK water ideograph, U+6C34),
    /// if present. This is used for font size adjustment, to normalize
    /// the width of CJK fonts mixed with latin fonts.
    ///
    /// NOTE: IC = Ideograph Character
    pub ic_width: Option<f64>,
}

impl FaceMetrics {
    /// Convenience function for getting the line height
    /// (ascent - descent + line_gap).
    #[inline]
    pub fn line_height(self) -> f64 {
        self.ascent - self.descent + self.line_gap
    }

    /// Convenience function for getting the cap height. If this is not
    /// defined in the font, we estimate it as 75% of the ascent.
    #[inline]
    pub fn cap_height(self) -> f64 {
        if let Some(value) = self.cap_height {
            if value > 0.0 {
                return value;
            }
        }
        0.75 * self.ascent
    }

    /// Convenience function for getting the ex height. If this is not
    /// defined in the font, we estimate it as 75% of the cap height.
    #[inline]
    pub fn ex_height(self) -> f64 {
        if let Some(value) = self.ex_height {
            if value > 0.0 {
                return value;
            }
        }
        0.75 * self.cap_height()
    }

    /// Convenience function for getting the ASCII height. If we
    /// couldn't measure this, we use 1.5 * cap_height as our
    /// estimator, based on measurements across programming fonts.
    #[inline]
    pub fn ascii_height(self) -> f64 {
        if let Some(value) = self.ascii_height {
            if value > 0.0 {
                return value;
            }
        }
        1.5 * self.cap_height()
    }

    /// Convenience function for getting the ideograph width. If this is
    /// not defined in the font, we estimate it as the minimum of the
    /// ascii height and two cell widths.
    #[inline]
    pub fn ic_width(self) -> f64 {
        if let Some(value) = self.ic_width {
            if value > 0.0 {
                return value;
            }
        }
        self.ascii_height().min(2.0 * self.cell_width)
    }

    /// Convenience function for getting the underline thickness. If
    /// this is not defined in the font, we estimate it as 15% of the ex
    /// height.
    #[inline]
    pub fn underline_thickness(self) -> f64 {
        if let Some(value) = self.underline_thickness {
            if value > 0.0 {
                return value;
            }
        }
        0.15 * self.ex_height()
    }

    /// Convenience function for getting the strikethrough thickness. If
    /// this is not defined in the font, we set it equal to the
    /// underline thickness.
    #[inline]
    pub fn strikethrough_thickness(self) -> f64 {
        if let Some(value) = self.strikethrough_thickness {
            if value > 0.0 {
                return value;
            }
        }
        self.underline_thickness()
    }

    // NOTE: The getters below return positions, not sizes, so both
    // positive and negative values are valid, hence no sign validation.

    /// Convenience function for getting the underline position. If
    /// this is not defined in the font, we place it one underline
    /// thickness below the baseline.
    #[inline]
    pub fn underline_position(self) -> f64 {
        self.underline_position
            .unwrap_or(-self.underline_thickness())
    }

    /// Convenience function for getting the strikethrough position. If
    /// this is not defined in the font, we center it at half the ex
    /// height, so that it's perfectly centered on lower case text.
    #[inline]
    pub fn strikethrough_position(self) -> f64 {
        self.strikethrough_position
            .unwrap_or((self.ex_height() + self.strikethrough_thickness()) * 0.5)
    }
}

impl Metrics {
    /// Calculate our metrics based on values extracted from a font.
    ///
    /// Try to pass values with as much precision as possible,
    /// do not round them before using them for this function.
    ///
    /// For any nullable options that are not provided, estimates will be used.
    pub fn calc(face: FaceMetrics) -> Metrics {
        // These are the unrounded advance width and line height values,
        // which are retained separately from the rounded cell width and
        // height values (below), for calculations that need to know how
        // much error there is between the design dimensions of the font
        // and the pixel dimensions of our cells.
        let face_width = face.cell_width;
        let face_height = face.line_height();

        // The cell width and height values need to be integers since they
        // represent pixel dimensions of the grid cells in the terminal.
        //
        // We use @round for the cell width to limit the difference from
        // the "true" width value to no more than 0.5px. This is a better
        // approximation of the authorial intent of the font than ceiling
        // would be, and makes the apparent spacing match better between
        // low and high DPI displays.
        //
        // This does mean that it's possible for a glyph to overflow the
        // edge of the cell by a pixel if it has no side bearings, but in
        // reality such glyphs are generally meant to connect to adjacent
        // glyphs in some way so it's not really an issue.
        //
        // The same is true for the height. Some fonts are poorly authored
        // and have a descender on a normal glyph that extends right up to
        // the descent value of the face, and this can result in the glyph
        // overflowing the bottom of the cell by a pixel, which isn't good
        // but if we try to prevent it by increasing the cell height then
        // we get line heights that are too large for most users and even
        // more inconsistent across DPIs.
        //
        // Users who experience such cell-height overflows should:
        //
        // 1. Nag the font author to either redesign the glyph to not go
        //    so low, or else adjust the descent value in the metadata.
        //
        // 2. Add an `adjust-cell-height` entry to their config to give
        //    the cell enough room for the glyph.
        let cell_width = face_width.round();
        let cell_height = face_height.round();

        // We split our line gap in two parts, and put half of it on the top
        // of the cell and the other half on the bottom, so that our text never
        // bumps up against either edge of the cell vertically.
        let half_line_gap = face.line_gap / 2.0;

        // NOTE: Unlike all our other metrics, `cell_baseline` is
        // relative to the BOTTOM of the cell rather than the top.
        let face_baseline = half_line_gap - face.descent;
        // We calculate the baseline by trying to center the face vertically
        // in the pixel-rounded cell height, so that before rounding it will
        // be an even distance from the top and bottom of the cell, meaning
        // it either sticks out the same amount or is inset the same amount,
        // depending on whether the cell height was rounded up or down from
        // the line height. We do this by adding half the difference between
        // the cell height and the face height.
        let cell_baseline = (face_baseline - (cell_height - face_height) / 2.0).round();

        // We keep track of the offset from the bottom of the cell
        // to the bottom of the face's "true" bounding box, which at
        // this point, since nothing has been scaled yet, is equivalent
        // to the offset between the baseline we draw at (cell_baseline)
        // and the one the font wants (face_baseline).
        let face_y = cell_baseline - face_baseline;

        // We calculate a top_to_baseline to make following calculations simpler.
        let top_to_baseline = cell_height - cell_baseline;

        // Get the other font metrics or their estimates. See doc comments
        // in FaceMetrics for explanations of the estimation heuristics.
        let cap_height = face.cap_height();
        let underline_thickness = 1.0_f64.max(face.underline_thickness().ceil());
        let strikethrough_thickness = 1.0_f64.max(face.strikethrough_thickness().ceil());
        let underline_position = (top_to_baseline - face.underline_position()).round();
        let strikethrough_position = (top_to_baseline - face.strikethrough_position()).round();

        // Same heuristic as the font_patcher script. We store icon_height
        // separately from face_height such that modifiers can apply to the former
        // without affecting the latter.
        let icon_height = face_height;
        let icon_height_single = (2.0 * cap_height + face_height) / 3.0;

        let mut result: Metrics = Metrics {
            cell_width: cell_width as u32,
            cell_height: cell_height as u32,
            cell_baseline: cell_baseline as u32,
            underline_position: underline_position as u32,
            underline_thickness: underline_thickness as u32,
            strikethrough_position: strikethrough_position as u32,
            strikethrough_thickness: strikethrough_thickness as u32,
            overline_position: 0,
            overline_thickness: underline_thickness as u32,
            box_thickness: underline_thickness as u32,
            // PORT NOTE: `cursor_thickness` has a `= 1` default in the reference
            // struct and is not set in this aggregate initializer; we apply the
            // same default here.
            cursor_thickness: 1,
            cursor_height: cell_height as u32,
            icon_height,
            icon_height_single,
            face_width,
            face_height,
            face_y,
        };

        // Ensure all metrics are within their allowable range.
        result.clamp();

        // std.log.debug("metrics={}", .{result});

        result
    }

    /// Apply a set of modifiers.
    pub fn apply(&mut self, mods: &ModifierSet) {
        // PORT NOTE: the reference iterates the hash map and `switch`es on each
        // key, with `inline` prongs binding the active tag. We iterate entries
        // and match on the `Key` enum; the special-cased `cell_width`/
        // `cell_height` and `icon_height` prongs are reproduced explicitly, and
        // the catch-all `inline else` prong applies the modifier to the named
        // field via a small `set_field`/`get_field` helper.
        for (key, value) in mods.iter() {
            match *key {
                // We clamp these values to a minimum of 1 to prevent divide-by-zero
                // in downstream operations.
                Key::CellWidth | Key::CellHeight => {
                    let tag = *key;
                    // Compute the new value. If it is the same avoid the work.
                    let original = self.get_u32_field(tag);
                    let new = value.apply_u32(original).max(1);
                    if new == original {
                        continue;
                    }

                    // Set the new value
                    self.set_u32_field(tag, new);

                    // For cell height, we have to also modify some positions
                    // that are absolute from the top of the cell. The main goal
                    // here is to center the baseline so that text is vertically
                    // centered in the cell.
                    if tag == Key::CellHeight {
                        let original_f64: f64 = original as f64;
                        let new_f64: f64 = new as f64;
                        let diff = new_f64 - original_f64;
                        let half_diff = diff / 2.0;

                        // If the diff is even, the number of pixels we add
                        // will be the same for the top and the bottom, but
                        // if the diff is odd then we want to add the extra
                        // pixel to the edge of the cell that needs it most.
                        //
                        // How much the edge "needs it" depends on whether
                        // the face is higher or lower than it should be to
                        // be perfectly centered in the cell.
                        //
                        // If the face were perfectly centered then face_y
                        // would be equal to half of the difference between
                        // the cell height and the face height.
                        let position_with_respect_to_center =
                            self.face_y - (original_f64 - self.face_height) / 2.0;

                        let (diff_top, diff_bottom) = if position_with_respect_to_center > 0.0 {
                            // The baseline is higher than it should be, so we
                            // add the extra to the top, or if it's a negative
                            // diff it gets added to the bottom because of how
                            // floor and ceil work.
                            (half_diff.ceil(), half_diff.floor())
                        } else {
                            // The baseline is lower than it should be, so we
                            // add the extra to the bottom, or vice versa for
                            // negative diffs.
                            (half_diff.floor(), half_diff.ceil())
                        };

                        // The cell baseline and face_y values are relative to the
                        // bottom of the cell so we add the bottom diff to them.
                        add_float_to_int(&mut self.cell_baseline, diff_bottom);
                        self.face_y += diff_bottom;

                        // These are all relative to the top of the cell.
                        add_float_to_int(&mut self.underline_position, diff_top);
                        add_float_to_int(&mut self.strikethrough_position, diff_top);
                        self.overline_position =
                            self.overline_position.saturating_add(diff_top as i32);
                    }
                }
                Key::IconHeight => {
                    self.icon_height = value.apply_f64(self.icon_height);
                    self.icon_height_single = value.apply_f64(self.icon_height_single);
                }

                tag => {
                    // inline else => |tag| @field(self, @tagName(tag)) = ...
                    self.apply_else(tag, value);
                }
            }
        }

        // Prevent modifiers from pushing metrics out of their allowable range.
        self.clamp();
    }

    /// Clamp all metrics to their allowable range.
    // PORT NOTE: the reference iterates `std.meta.fields(Metrics)` and clamps any
    // field that has a matching `Minimums` decl. Rust has no comptime field
    // iteration, so this is hand-unrolled over exactly the fields that have a
    // `Minimums` entry (the others — `cell_baseline`, `underline_position`,
    // `strikethrough_position`, `overline_position`, `face_y` — have no minimum).
    fn clamp(&mut self) {
        self.cell_width = self.cell_width.max(minimums::CELL_WIDTH);
        self.cell_height = self.cell_height.max(minimums::CELL_HEIGHT);
        self.underline_thickness = self.underline_thickness.max(minimums::UNDERLINE_THICKNESS);
        self.strikethrough_thickness = self
            .strikethrough_thickness
            .max(minimums::STRIKETHROUGH_THICKNESS);
        self.overline_thickness = self.overline_thickness.max(minimums::OVERLINE_THICKNESS);
        self.box_thickness = self.box_thickness.max(minimums::BOX_THICKNESS);
        self.cursor_thickness = self.cursor_thickness.max(minimums::CURSOR_THICKNESS);
        self.cursor_height = self.cursor_height.max(minimums::CURSOR_HEIGHT);
        self.icon_height = self.icon_height.max(minimums::ICON_HEIGHT);
        self.icon_height_single = self.icon_height_single.max(minimums::ICON_HEIGHT_SINGLE);
        self.face_height = self.face_height.max(minimums::FACE_HEIGHT);
        self.face_width = self.face_width.max(minimums::FACE_WIDTH);
    }

    // PORT NOTE: helpers below model the reference's `@field(self, @tagName(tag))`
    // by-name access for the `apply` switch arms. `Key` only contains fields whose
    // type is `u32`, `i32`, or `f64` (see `Key`), so dispatch is by group.
    fn get_u32_field(&self, key: Key) -> u32 {
        match key {
            Key::CellWidth => self.cell_width,
            Key::CellHeight => self.cell_height,
            Key::CellBaseline => self.cell_baseline,
            Key::UnderlinePosition => self.underline_position,
            Key::UnderlineThickness => self.underline_thickness,
            Key::StrikethroughPosition => self.strikethrough_position,
            Key::StrikethroughThickness => self.strikethrough_thickness,
            Key::OverlineThickness => self.overline_thickness,
            Key::BoxThickness => self.box_thickness,
            Key::CursorThickness => self.cursor_thickness,
            Key::CursorHeight => self.cursor_height,
            _ => unreachable!(),
        }
    }

    fn set_u32_field(&mut self, key: Key, v: u32) {
        match key {
            Key::CellWidth => self.cell_width = v,
            Key::CellHeight => self.cell_height = v,
            Key::CellBaseline => self.cell_baseline = v,
            Key::UnderlinePosition => self.underline_position = v,
            Key::UnderlineThickness => self.underline_thickness = v,
            Key::StrikethroughPosition => self.strikethrough_position = v,
            Key::StrikethroughThickness => self.strikethrough_thickness = v,
            Key::OverlineThickness => self.overline_thickness = v,
            Key::BoxThickness => self.box_thickness = v,
            Key::CursorThickness => self.cursor_thickness = v,
            Key::CursorHeight => self.cursor_height = v,
            _ => unreachable!(),
        }
    }

    // The `inline else` prong of `apply`: applies the modifier to whatever field
    // the tag names, dispatching on the field's underlying type.
    fn apply_else(&mut self, key: Key, value: &Modifier) {
        match key {
            // u32 fields
            Key::CellWidth
            | Key::CellHeight
            | Key::CellBaseline
            | Key::UnderlinePosition
            | Key::UnderlineThickness
            | Key::StrikethroughPosition
            | Key::StrikethroughThickness
            | Key::OverlineThickness
            | Key::BoxThickness
            | Key::CursorThickness
            | Key::CursorHeight => {
                let v = self.get_u32_field(key);
                self.set_u32_field(key, value.apply_u32(v));
            }
            // i32 field
            Key::OverlinePosition => {
                self.overline_position = value.apply_i32(self.overline_position);
            }
            // f64 fields
            Key::IconHeight => {
                self.icon_height = value.apply_f64(self.icon_height);
            }
            Key::IconHeightSingle => {
                self.icon_height_single = value.apply_f64(self.icon_height_single);
            }
            Key::FaceWidth => self.face_width = value.apply_f64(self.face_width),
            Key::FaceHeight => self.face_height = value.apply_f64(self.face_height),
            Key::FaceY => self.face_y = value.apply_f64(self.face_y),
        }
    }
}

/// Helper function for adding an f64 to a u32.
///
/// Performs saturating addition or subtraction
/// depending on the sign of the provided float.
///
/// The f64 is asserted to have an integer value.
#[inline]
fn add_float_to_int(int: &mut u32, float: f64) {
    assert(float.floor() == float);
    *int = if float >= 0.0 {
        int.saturating_add(float as u32)
    } else {
        int.saturating_sub((-float) as u32)
    };
}

/// A set of modifiers to apply to metrics. We use a hash map because
/// we expect most metrics to be unmodified and want to take up as
/// little space as possible.
// PORT NOTE: the reference is `std.AutoHashMapUnmanaged(Key, Modifier)`. The
// unmanaged/managed distinction (explicit allocator threading) is a Zig memory
// idiom; the std `HashMap` is the faithful Rust equivalent of the data structure.
pub type ModifierSet = HashMap<Key, Modifier>;

/// A modifier to apply to a metrics value. The modifier value represents
/// a delta, so percent is a percentage to change, not a percentage of.
/// For example, "20%" is 20% larger, not 20% of the value. Likewise,
/// an absolute value of "20" is 20 larger, not literally 20.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Modifier {
    Percent(f64),
    Absolute(i32),
}

// `Modifier` (the `MetricModifier`) is used as the type of the `adjust-*` config
// fields (`?Modifier`). It is a `union(enum)` with NO `equal` decl, so the
// reference's `Config.changed` resolves its `deepEqual` via the `.@"union"` arm:
// compare the active tag, then `deepEqual` the active payload (`f64`/`i32`, both
// `old == new`). That is exactly the derived `PartialEq`. The orphan rule
// requires this `DeepEqual` impl to live in `Modifier`'s home crate.
impl rustty_datastruct::comparison::DeepEqual for Modifier {
    fn deep_equal(&self, other: &Self) -> bool {
        self == other
    }
}

/// Error type for `Modifier::parse`.
// PORT NOTE: the reference uses Zig error set values `error.InvalidFormat` and
// `error.ValueRequired` (and `error.OutOfMemory` in `formatEntry`); modeled as a
// Rust error enum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidFormat,
    ValueRequired,
    OutOfMemory,
}

impl Modifier {
    /// Parses the modifier value. If the value ends in "%" it is assumed
    /// to be a percent, otherwise the value is parsed as an integer.
    pub fn parse(input: &[u8]) -> Result<Modifier, Error> {
        if input.is_empty() {
            return Err(Error::InvalidFormat);
        }

        if input[input.len() - 1] == b'%' {
            let mut percent: f64 = std::str::from_utf8(&input[0..input.len() - 1])
                .map_err(|_| Error::InvalidFormat)?
                .parse::<f64>()
                .map_err(|_| Error::InvalidFormat)?;
            percent /= 100.0;

            if percent <= -1.0 {
                return Ok(Modifier::Percent(0.0));
            }
            if percent < 0.0 {
                return Ok(Modifier::Percent(1.0 + percent));
            }
            return Ok(Modifier::Percent(1.0 + percent));
        }

        Ok(Modifier::Absolute(
            std::str::from_utf8(input)
                .map_err(|_| Error::InvalidFormat)?
                .parse::<i32>()
                .map_err(|_| Error::InvalidFormat)?,
        ))
    }

    /// So it works with the config framework.
    pub fn parse_cli(input: Option<&[u8]>) -> Result<Modifier, Error> {
        Modifier::parse(input.ok_or(Error::ValueRequired)?)
    }

    /// Used by config formatter
    // PORT NOTE: the reference takes an `anytype` formatter and calls
    // `formatter.formatEntry([]const u8, ...)`. The config formatter framework is
    // not yet ported into rustty; faithfully, this produces the formatted entry
    // string (`"<value>%"` / `"<value>"`) into the supplied buffer, which is what
    // `formatEntry` ultimately writes. Callers (the config crate) will wire this
    // to the real formatter when that crate is carved.
    pub fn format_entry(self, buf: &mut [u8]) -> Result<&[u8], Error> {
        // var buf: [1024]u8 = undefined; (the reference's stack buffer; here the
        // caller provides it so the borrow outlives the call)
        use std::io::Write;
        match self {
            Modifier::Percent(v) => {
                let mut cursor = std::io::Cursor::new(buf);
                write!(cursor, "{}%", format_d((v - 1.0) * 100.0))
                    .map_err(|_| Error::OutOfMemory)?;
                let n = cursor.position() as usize;
                Ok(&cursor.into_inner()[0..n])
            }
            Modifier::Absolute(v) => {
                let mut cursor = std::io::Cursor::new(buf);
                write!(cursor, "{}", v).map_err(|_| Error::OutOfMemory)?;
                let n = cursor.position() as usize;
                Ok(&cursor.into_inner()[0..n])
            }
        }
    }

    /// Apply a modifier to a numeric value.
    // PORT NOTE: the reference is generic over `anytype` and dispatches on
    // `@typeInfo(T)` (`.int`/`.comptime_int`, `.float`/`.comptime_float`). Rust
    // has no value-level generic numeric dispatch, so this is split into one
    // method per metric field type actually used by `apply`: `apply_u32`,
    // `apply_i32`, `apply_f64`. Each reproduces the corresponding type arm.
    pub fn apply_u32(self, v: u32) -> u32 {
        // The unsigned-int arm (Tinfo.int.signedness == .unsigned).
        match self {
            Modifier::Percent(p) => {
                let p_clamped: f64 = 0.0_f64.max(p);
                let v_f64: f64 = v as f64;
                let applied_f64: f64 = (v_f64 * p_clamped).round();
                applied_f64 as u32
            }
            Modifier::Absolute(abs) => {
                let v_i64: i64 = v as i64;
                let abs_i64: i64 = abs as i64;
                let applied_i64: i64 = v_i64.saturating_add(abs_i64);
                // unsigned: clamp to >= 0
                let clamped_i64: i64 = 0.max(applied_i64);
                cast_or_saturate_u32(clamped_i64)
            }
        }
    }

    pub fn apply_i32(self, v: i32) -> i32 {
        // The signed-int arm (Tinfo.int.signedness == .signed).
        match self {
            Modifier::Percent(p) => {
                let p_clamped: f64 = 0.0_f64.max(p);
                let v_f64: f64 = v as f64;
                let applied_f64: f64 = (v_f64 * p_clamped).round();
                applied_f64 as i32
            }
            Modifier::Absolute(abs) => {
                let v_i64: i64 = v as i64;
                let abs_i64: i64 = abs as i64;
                let applied_i64: i64 = v_i64.saturating_add(abs_i64);
                // signed: no extra clamp
                let clamped_i64: i64 = applied_i64;
                cast_or_saturate_i32(clamped_i64)
            }
        }
    }

    pub fn apply_f64(self, v: f64) -> f64 {
        // The float arm.
        match self {
            Modifier::Percent(p) => v * 0.0_f64.max(p),
            Modifier::Absolute(abs) => v + (abs as f64),
        }
    }

    /// Hash using the hasher.
    // PORT NOTE: the reference feeds the active tag and value into an `anytype`
    // hasher via `std.hash.autoHash`. `Modifier` participates in `ModifierSet`
    // hashing through the derived `Hash` impl on the enum below, which hashes the
    // discriminant then the payload — for `percent` the f64 is bitcast to i64,
    // matching the reference's "floats can't be hashed directly" workaround.
    // PORT NOTE: named `hash` to mirror the reference's `pub fn hash(self, hasher)`
    // method; `clippy::should_implement_trait` flags the name collision with
    // `std::hash::Hash::hash`, allowed to keep the faithful method name.
    #[allow(clippy::should_implement_trait)]
    pub fn hash<H: std::hash::Hasher>(self, hasher: &mut H) {
        use std::hash::Hash;
        // autoHash(hasher, std.meta.activeTag(self));
        std::mem::discriminant(&self).hash(hasher);
        match self {
            // floats can't be hashed directly so we bitcast to i64.
            // for the purpose of what we're trying to do this seems
            // good enough but I would prefer value hashing.
            Modifier::Percent(v) => (v.to_bits() as i64).hash(hasher),
            Modifier::Absolute(v) => v.hash(hasher),
        }
    }
}

// PORT NOTE: `ModifierSet` is a `HashMap<Key, Modifier>`; only `Key` is hashed as
// a key, so `Modifier` does not itself need `Hash`/`Eq` for the map. The explicit
// `Modifier::hash` above mirrors the reference's `hash` method for any callers
// that hash a modifier value directly.

/// Cast an i64 to u32, saturating to `maxInt` with the sign of the input on
/// overflow, matching `std.math.cast(T, x) orelse maxInt(T) * sign(x)`.
fn cast_or_saturate_u32(x: i64) -> u32 {
    if let Ok(v) = u32::try_from(x) {
        v
    } else {
        // std.math.maxInt(T) * sign(x); for unsigned the only out-of-range
        // overflow here is the positive side (negatives were clamped to 0).
        u32::MAX.wrapping_mul(sign_i64(x) as u32)
    }
}

fn cast_or_saturate_i32(x: i64) -> i32 {
    if let Ok(v) = i32::try_from(x) {
        v
    } else {
        i32::MAX.wrapping_mul(sign_i64(x) as i32)
    }
}

#[inline]
fn sign_i64(x: i64) -> i64 {
    match x.cmp(&0) {
        std::cmp::Ordering::Greater => 1,
        std::cmp::Ordering::Less => -1,
        std::cmp::Ordering::Equal => 0,
    }
}

/// Format an f64 the way Zig's `{d}` does (shortest round-tripping decimal,
/// no trailing-zero noise).
// PORT NOTE: Rust's default `{}` for f64 already prints `24` for `24.0` and a
// shortest decimal otherwise, matching Zig's `{d}` for the values used here.
fn format_d(v: f64) -> String {
    format!("{}", v)
}

/// Key is an enum of all the available metrics keys.
// PORT NOTE: the reference builds this enum at comptime from `std.meta.fields`,
// including exactly the fields whose type is `u32`, `i32`, or `f64` (all of them
// in this struct). The enum's tag values equal the field index. We reproduce the
// enum with explicit discriminants matching the field order/index in `Metrics`.
// `cursor_thickness` is at index 11.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    CellWidth = 0,
    CellHeight = 1,
    CellBaseline = 2,
    UnderlinePosition = 3,
    UnderlineThickness = 4,
    StrikethroughPosition = 5,
    StrikethroughThickness = 6,
    OverlinePosition = 7,
    OverlineThickness = 8,
    BoxThickness = 9,
    CursorThickness = 10,
    CursorHeight = 11,
    IconHeight = 12,
    IconHeightSingle = 13,
    FaceWidth = 14,
    FaceHeight = 15,
    FaceY = 16,
}

// NOTE: This is purposely not pub because we want to force outside callers
// to use the `.{}` syntax so unused fields are detected by the compiler.
// PORT NOTE: Rust struct literals already require naming every field, so the
// "force `.{}`" intent is enforced by the language; `init` stays private.
// `#[allow(dead_code)]`: in the reference this private fn is only referenced by
// the test blocks; outside `cfg(test)` the lib build sees it as unused.
#[allow(dead_code)]
fn init() -> Metrics {
    Metrics {
        cell_width: 0,
        cell_height: 0,
        cell_baseline: 0,
        underline_position: 0,
        underline_thickness: 0,
        strikethrough_position: 0,
        strikethrough_thickness: 0,
        overline_position: 0,
        overline_thickness: 0,
        box_thickness: 0,
        // PORT NOTE: not named in the reference's `init` literal because of its
        // `= 1` default; here we set it explicitly to the default value.
        cursor_thickness: 1,
        cursor_height: 0,
        icon_height: 0.0,
        icon_height_single: 0.0,
        face_width: 0.0,
        face_height: 0.0,
        face_y: 0.0,
    }
}

#[cfg(test)]
// PORT NOTE: test fns keep the reference's `TypeName: case` naming
// (`Metrics: ...`, `Modifier: ...`, `formatConfig ...`) rendered as
// `TypeName_case`; `#[allow(non_snake_case)]` silences the Rust naming lint
// rather than mangling the faithful test names.
#[allow(non_snake_case)]
mod tests {
    use super::*;

    // PORT NOTE: the reference's two `formatConfig` tests live under the
    // `Modifier` struct and gate on `@import("terminal_options").artifact == .lib`
    // returning `error.SkipZigTest` before importing `../config.zig`. The
    // artifact gate is `options::ARTIFACT`.
    //
    // CORRECTION (S85-F4): the note that stood here said "The config crate is
    // not yet carved into rustty, so the formatter cannot be wired here", and
    // both tests asserted only the VALUE (`b"24%"` / `b"-30"`) where the
    // reference asserts the whole formatted entry line. That premise has been
    // false since `rustty-cfg-fmt` landed: `format_entry_string` IS the port of
    // `configpkg.entryFormatter`'s string arm, it is a dependency-free leaf, and
    // `rustty-config`'s `config_struct.rs` drives production formatting through
    // it. The tests now assert the reference's own strings.
    //
    // PORT NOTE: the reference's `Modifier.formatEntry(formatter)` writes the
    // whole line itself; the port splits it — `Modifier::format_entry` renders
    // the VALUE and the config layer wraps it with the key and the newline —
    // so the test composes the same two pieces production composes.
    #[test]
    fn formatConfig_percent() {
        if crate::options::ARTIFACT == crate::options::Artifact::Lib {
            return;
        }
        let p = Modifier::parse_cli(Some(b"24%")).unwrap();
        let mut buf = [0u8; 1024];
        let out = p.format_entry(&mut buf).unwrap();
        let mut w = rustty_cfg_fmt::AllocatingWriter::init();
        rustty_cfg_fmt::format_entry_string(b"a", out, &mut w).unwrap();
        // try std.testing.expectEqualSlices(u8, "a = 24%\n", buf.written());
        assert_eq!(w.written(), b"a = 24%\n");
    }

    #[test]
    fn formatConfig_absolute() {
        if crate::options::ARTIFACT == crate::options::Artifact::Lib {
            return;
        }
        let p = Modifier::parse_cli(Some(b"-30")).unwrap();
        let mut buf = [0u8; 1024];
        let out = p.format_entry(&mut buf).unwrap();
        let mut w = rustty_cfg_fmt::AllocatingWriter::init();
        rustty_cfg_fmt::format_entry_string(b"a", out, &mut w).unwrap();
        // try std.testing.expectEqualSlices(u8, "a = -30\n", buf.written());
        assert_eq!(w.written(), b"a = -30\n");
    }

    #[test]
    fn Metrics_apply_modifiers() {
        let mut set: ModifierSet = ModifierSet::new();
        set.insert(Key::CellWidth, Modifier::Percent(1.2));

        let mut m: Metrics = init();
        m.cell_width = 100;
        m.apply(&set);
        assert_eq!(120u32, m.cell_width);
    }

    #[test]
    fn Metrics_adjust_cell_height_smaller() {
        let mut set: ModifierSet = ModifierSet::new();
        // We choose numbers such that the subtracted number of pixels is odd,
        // as that's the case that could most easily have off-by-one errors.
        // Here we're removing 25 pixels: 13 on the bottom, 12 on top, split
        // that way because we're simulating a face that's 0.33px higher than
        // it "should" be (due to rounding).
        set.insert(Key::CellHeight, Modifier::Percent(0.75));

        let mut m: Metrics = init();
        m.face_y = 0.33;
        m.cell_baseline = 50;
        m.underline_position = 55;
        m.strikethrough_position = 30;
        m.overline_position = 0;
        m.cell_height = 100;
        m.face_height = 99.67;
        m.cursor_height = 100;
        m.apply(&set);
        assert_eq!(-12.67, m.face_y);
        assert_eq!(75u32, m.cell_height);
        assert_eq!(37u32, m.cell_baseline);
        assert_eq!(43u32, m.underline_position);
        assert_eq!(18u32, m.strikethrough_position);
        assert_eq!(-12i32, m.overline_position);
        // Cursor height is separate from cell height and does not follow it.
        assert_eq!(100u32, m.cursor_height);
    }

    #[test]
    fn Metrics_adjust_cell_height_larger() {
        let mut set: ModifierSet = ModifierSet::new();
        // We choose numbers such that the added number of pixels is odd,
        // as that's the case that could most easily have off-by-one errors.
        // Here we're adding 75 pixels: 37 on the bottom, 38 on top, split
        // that way because we're simulating a face that's 0.33px higher
        // than it "should" be (due to rounding).
        set.insert(Key::CellHeight, Modifier::Percent(1.75));

        let mut m: Metrics = init();
        m.face_y = 0.33;
        m.cell_baseline = 50;
        m.underline_position = 55;
        m.strikethrough_position = 30;
        m.overline_position = 0;
        m.cell_height = 100;
        m.face_height = 99.67;
        m.cursor_height = 100;
        m.apply(&set);
        assert_eq!(37.33, m.face_y);
        assert_eq!(175u32, m.cell_height);
        assert_eq!(87u32, m.cell_baseline);
        assert_eq!(93u32, m.underline_position);
        assert_eq!(68u32, m.strikethrough_position);
        assert_eq!(38i32, m.overline_position);
        // Cursor height is separate from cell height and does not follow it.
        assert_eq!(100u32, m.cursor_height);
    }

    #[test]
    fn Metrics_adjust_icon_height_by_percentage() {
        let mut set: ModifierSet = ModifierSet::new();
        set.insert(Key::IconHeight, Modifier::Percent(0.75));

        let mut m: Metrics = init();
        m.icon_height = 100.0;
        m.icon_height_single = 80.0;
        m.face_height = 100.0;
        m.face_y = 1.0;
        m.apply(&set);
        assert_eq!(75.0, m.icon_height);
        assert_eq!(60.0, m.icon_height_single);
        // Face metrics not affected
        assert_eq!(100.0, m.face_height);
        assert_eq!(1.0, m.face_y);
    }

    #[test]
    fn Metrics_adjust_icon_height_by_absolute_pixels() {
        let mut set: ModifierSet = ModifierSet::new();
        set.insert(Key::IconHeight, Modifier::Absolute(-5));

        let mut m: Metrics = init();
        m.icon_height = 100.0;
        m.icon_height_single = 80.0;
        m.face_height = 100.0;
        m.face_y = 1.0;
        m.apply(&set);
        assert_eq!(95.0, m.icon_height);
        assert_eq!(75.0, m.icon_height_single);
        // Face metrics not affected
        assert_eq!(100.0, m.face_height);
        assert_eq!(1.0, m.face_y);
    }

    #[test]
    fn Modifier_parse_absolute() {
        {
            let m = Modifier::parse(b"100").unwrap();
            assert_eq!(Modifier::Absolute(100), m);
        }

        {
            let m = Modifier::parse(b"-100").unwrap();
            assert_eq!(Modifier::Absolute(-100), m);
        }
    }

    #[test]
    fn Modifier_parse_percent() {
        {
            let m = Modifier::parse(b"20%").unwrap();
            assert_eq!(Modifier::Percent(1.2), m);
        }
        {
            let m = Modifier::parse(b"-20%").unwrap();
            assert_eq!(Modifier::Percent(0.8), m);
        }
        {
            let m = Modifier::parse(b"0%").unwrap();
            assert_eq!(Modifier::Percent(1.0), m);
        }
    }

    #[test]
    fn Modifier_percent() {
        {
            let m: Modifier = Modifier::Percent(0.8);
            let v: u32 = m.apply_u32(100);
            assert_eq!(80u32, v);
        }
        {
            let m: Modifier = Modifier::Percent(1.8);
            let v: u32 = m.apply_u32(100);
            assert_eq!(180u32, v);
        }
    }

    #[test]
    fn Modifier_absolute() {
        {
            let m: Modifier = Modifier::Absolute(-100);
            let v: u32 = m.apply_u32(100);
            assert_eq!(0u32, v);
        }
        {
            let m: Modifier = Modifier::Absolute(-120);
            let v: u32 = m.apply_u32(100);
            assert_eq!(0u32, v);
        }
        {
            let m: Modifier = Modifier::Absolute(100);
            let v: u32 = m.apply_u32(100);
            assert_eq!(200u32, v);
        }
    }
}
