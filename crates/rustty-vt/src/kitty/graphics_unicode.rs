//! rustty-vt kitty/graphics_unicode
//!
//! PORTS: ghostty src/terminal/kitty/graphics_unicode.zig
//!
//! PORT STATUS: ported (renderPlacement/grid + their tests deferred to vt-12;
//! see PORT NOTE below)

//! This file contains various logic and data for working with the
//! Kitty graphics protocol unicode placeholder, virtual placement feature.

// const std = @import("std");
// const assert = @import("../../quirks.zig").inlineAssert;
// const testing = std.testing;
// const terminal = @import("../main.zig");
// const kitty_gfx = terminal.kitty.graphics;
// const Image = kitty_gfx.Image;
// const ImageStorage = kitty_gfx.ImageStorage;
// const RenderPlacement = kitty_gfx.RenderPlacement;
use crate::pagelist::iter::{Direction, RowIterator};
use crate::pagelist::Pin;

// const log = std.log.scoped(.kitty_gfx);

// PORT NOTE: `assert` is `quirks.inlineAssert`. We retarget to the same helper
// used by the rest of the vt crate.
use rustty_util::quirks::inlineAssert as assert;

/// Codepoint for the unicode placeholder character.
pub const PLACEHOLDER: u32 = 0x10EEEE;

/// Returns an iterator that iterates over all of the virtual placements
/// in the given pin. If `limit` is provided, the iterator will stop
/// when it reaches that pin (inclusive). If `limit` is not provided,
/// the iterator will continue until the end of the page list.
pub fn placement_iterator(pin: Pin, limit: Option<Pin>) -> PlacementIterator {
    let mut row_it = pin.row_iterator(Direction::RightDown, limit);
    let row = row_it.next();
    PlacementIterator { row_it, row }
}

/// Iterator over unicode virtual placements.
pub struct PlacementIterator {
    row_it: RowIterator,
    row: Option<Pin>,
}

impl PlacementIterator {
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Option<Placement> {
        while let Some(row) = self.row.as_mut() {
            // This row flag is set on rows that have the virtual placeholder
            if !unsafe { (*row.row_and_cell().row).kitty_virtual_placeholder() } {
                self.row = self.row_it.next();
                continue;
            }

            // Our current run. A run is always only a single row. This
            // assumption is built-in to our logic so if we want to change
            // this later we have to redo the logic; tests should cover;
            let mut run: Option<IncompletePlacement> = None;

            // Iterate over our remaining cells and find one with a placeholder.
            let cells = row.cells(crate::pagelist::pin::CellSubset::Right);
            // PORT NOTE: the reference iterates `for (cells, row.x..) |*cell, x|`.
            // `cells` is the `.right` subset (starting at `row.x`); `x` runs from
            // the pin's original x upward. We capture the original x first because
            // we mutate `row.x` inside the loop (which the reference also does).
            let cells_len = cells.len();
            let cells_base = cells as *mut crate::page::Cell;
            let x_start = row.x;
            for i in 0..cells_len {
                let cell: *mut crate::page::Cell = unsafe { cells_base.add(i) };
                let x = x_start as usize + i;
                // "row" now points to the top-left pin of the placement.
                // We need this temporary state to build our incomplete
                // placement.
                //
                // PORT NOTE: the reference asserts `@intFromPtr(row) ==
                // @intFromPtr(&self.row)` — i.e. that the `row` loop variable
                // aliases `self.row`'s payload. In Rust `row` is a `&mut Pin`
                // borrowed from `self.row`, which is guaranteed by the borrow
                // (no pointer-identity check is meaningful), so we drop the
                // pointer comparison and keep the mutation it guards.
                row.x = x as crate::size::CellCountInt;

                // If this cell doesn't have the placeholder, then we
                // complete the run if we have it otherwise we just move
                // on and keep searching.
                if unsafe { (*cell).codepoint() } != PLACEHOLDER {
                    if let Some(prev) = run {
                        return Some(prev.complete());
                    }
                    continue;
                }

                // If we don't have a previous run, then we save this
                // incomplete one, start a run, and move on.
                let curr = IncompletePlacement::init(row, cell);
                if let Some(prev) = run.as_mut() {
                    // If we can't append, then we complete the previous
                    // run and return it.
                    if !prev.append(&curr) {
                        // Note: self.row is already updated due to the
                        // row pointer above. It points back at this same
                        // cell so we can continue the new placements from
                        // here.
                        return Some(prev.complete());
                    }

                    // append is mutating so if we reached this point
                    // then prev has been updated.
                } else {
                    // For appending, we need to set our initial values.
                    let mut prev = curr;
                    if prev.row.is_none() {
                        prev.row = Some(0);
                    }
                    if prev.col.is_none() {
                        prev.col = Some(0);
                    }
                    run = Some(prev);
                }
            }

            // We move to the next row no matter what
            self.row = self.row_it.next();

            // If we have a run, we complete it here.
            if let Some(prev) = run {
                return Some(prev.complete());
            }
        }

        None
    }
}

/// A virtual placement in the terminal. This can represent more than
/// one cell if the cells combine to be a run.
pub struct Placement {
    /// The top-left pin of the placement. This can be used to get the
    /// screen x/y.
    pub pin: Pin,

    /// The image ID and placement ID for this virtual placement. The
    /// image ID is encoded in the fg color (plus optional a 8-bit high
    /// value in the 3rd diacritic). The placement ID is encoded in the
    /// underline color (optionally).
    pub image_id: u32,
    pub placement_id: u32,

    /// Starting row/col index for the image itself. This is the "fragment"
    /// of the image we want to show in this placement. This is 0-indexed.
    pub col: u32,
    pub row: u32,

    /// The width/height in cells of this placement.
    pub width: u32,
    pub height: u32,
}

/// PORT NOTE: the reference `Placement.Error` error set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    PlacementGridOutOfBounds,
    PlacementMissingPlacement,
}

impl Placement {
    // PORT NOTE: `renderPlacement` and `grid` are DEFERRED to vt-12. Both
    // require `ImageStorage` (`kitty/graphics_storage.zig`, deferred to vt-12
    // because it depends on `Screen`) and `Image` (`kitty/graphics_image.zig`,
    // ported later in the vt-7 workflow), neither of which is available at this
    // point in the port. They will be ported faithfully — assertion-for-
    // assertion with the three `unicode render placement: dog ...` tests — once
    // `ImageStorage`/`Image` land. The signatures (for reference) are:
    //   fn render_placement(&self, storage: &ImageStorage, img: &Image,
    //       cell_width: u32, cell_height: u32) -> Result<RenderPlacement, Error>
    //   fn grid(&self, storage: &ImageStorage, image: &Image,
    //       cell_width: u32, cell_height: u32) -> Result<{rows,columns}, Error>
}

/// IncompletePlacement is the placement information present in a single
/// cell. It is "incomplete" because the specification allows for missing
/// diacritics and so on that continue from previous valid placements.
struct IncompletePlacement {
    /// The pin of the cell that created this incomplete placement.
    pin: Pin,

    /// Lower 24 bits of the image ID. This is specified in the fg color
    /// and is always required.
    image_id_low: u32, // u24

    /// Higher 8 bits of the image ID specified using the 3rd diacritic.
    /// This is optional.
    image_id_high: Option<u8>,

    /// Placement ID is optionally specified in the underline color.
    placement_id: Option<u32>, // u24

    /// The row/col index for the image. These are 0-indexed. These
    /// are specified using diacritics. The row is first and the col
    /// is second. Both are optional. If not specified, they can continue
    /// a previous placement under certain conditions.
    row: Option<u32>,
    col: Option<u32>,

    /// The run width so far in cells.
    width: u32,
}

impl IncompletePlacement {
    /// Parse the incomplete placement information from a row and cell.
    ///
    /// The cell could be derived from the row but in our usage we already
    /// have the cell and we don't want to waste cycles recomputing it.
    // PORT NOTE: the reference decodes each diacritic with
    // `getIndex(cp) orelse value: { log.warn(...); break :value null; }`. The
    // `log.warn` is a commented-out seam (the scoped logger is not yet wired),
    // which leaves a bare `match { Some => Some, None => { /*log*/ None } }`
    // that clippy would rewrite to `.map`/`.ok()`. We keep the match structure
    // verbatim (it carries the commented log seam) and silence those lints.
    #[allow(
        clippy::manual_map,
        clippy::manual_ok_err,
        clippy::match_single_binding,
        clippy::needless_match
    )]
    pub fn init(row: &Pin, cell: *const crate::page::Cell) -> IncompletePlacement {
        assert(unsafe { (*cell).codepoint() } == PLACEHOLDER);
        let style = row.style(cell);

        let mut result: IncompletePlacement = IncompletePlacement {
            // PORT NOTE: the reference does `.pin = row.*` (a value copy of the
            // `Pin`). `Pin` is deliberately non-`Copy`/non-`Clone` in the Rust
            // port (it holds a raw `*mut Node`), so we copy it field-by-field,
            // matching how the rest of the crate reconstructs pins.
            pin: Pin {
                node: row.node,
                y: row.y,
                x: row.x,
                garbage: row.garbage,
            },
            image_id_low: color_to_id(style.fg_color),
            image_id_high: None,
            placement_id: {
                let id = color_to_id(style.underline_color);
                if id != 0 {
                    Some(id)
                } else {
                    None
                }
            },
            row: None,
            col: None,
            width: 1,
        };

        // Try to decode all our diacritics. Any invalid diacritics are
        // treated as if they don't exist. This isn't explicitly specified
        // at the time of writing this but it appears to be how Kitty behaves.
        let empty: &[u32] = &[];
        let cps: &[u32] = row.grapheme(cell).unwrap_or(empty);
        if !cps.is_empty() {
            result.row = match get_index(cps[0]) {
                Some(v) => Some(v),
                None => {
                    // log.warn("virtual placement with invalid row diacritic cp={X}", .{cps[0]});
                    None
                }
            };

            if cps.len() > 1 {
                result.col = match get_index(cps[1]) {
                    Some(v) => Some(v),
                    None => {
                        // log.warn("virtual placement with invalid col diacritic cp={X}", .{cps[1]});
                        None
                    }
                };

                if cps.len() > 2 {
                    let high_ = match get_index(cps[2]) {
                        Some(v) => Some(v),
                        None => {
                            // log.warn("virtual placement with invalid high diacritic cp={X}", .{cps[2]});
                            None
                        }
                    };

                    if let Some(high) = high_ {
                        result.image_id_high = match u8::try_from(high) {
                            Ok(v) => Some(v),
                            Err(_) => {
                                // log.warn("virtual placement with invalid high diacritic cp={X} value={}", .{
                                //     cps[2],
                                //     high,
                                // });
                                None
                            }
                        };
                    }

                    // Any additional diacritics are ignored.
                }
            }
        }

        result
    }

    /// Append this incomplete placement to an existing placement to
    /// create a run. This returns true if the placements are compatible
    /// and were combined. If this returns false, the other placement is
    /// unchanged.
    pub fn append(&mut self, other: &IncompletePlacement) -> bool {
        if !self.can_append(other) {
            return false;
        }
        self.width += 1;
        true
    }

    fn can_append(&self, other: &IncompletePlacement) -> bool {
        // Converted from Kitty's logic, don't @ me.
        self.image_id_low == other.image_id_low
            && self.placement_id == other.placement_id
            && (other.row.is_none() || other.row == self.row)
            && (other.col.is_none() || other.col == Some(self.col.unwrap() + self.width))
            && (other.image_id_high.is_none() || other.image_id_high == self.image_id_high)
    }

    /// Complete the incomplete placement to create a full placement.
    /// This creates a new placement that isn't continuous with any previous
    /// placements.
    ///
    /// The pin is the pin of the cell that created this incomplete placement.
    pub fn complete(&self) -> Placement {
        Placement {
            // PORT NOTE: see `init` — `Pin` is non-`Copy`, copied field-by-field.
            pin: Pin {
                node: self.pin.node,
                y: self.pin.y,
                x: self.pin.x,
                garbage: self.pin.garbage,
            },
            image_id: {
                let low: u32 = self.image_id_low;
                let high: u32 = self.image_id_high.unwrap_or(0) as u32;
                low | (high << 24)
            },

            placement_id: self.placement_id.unwrap_or(0),
            col: self.col.unwrap_or(0),
            row: self.row.unwrap_or(0),
            width: self.width,
            height: 1,
        }
    }
}

/// Convert a style color to a Kitty image protocol ID. This works by
/// taking the 24 most significant bits of the color, which lets it work
/// for both palette and rgb-based colors.
// PORT NOTE: the reference signature is `colorToId(c: terminal.Style.Color) u24`.
// We retarget `terminal.Style.Color` to `crate::style::Color` and the `u24`
// return to a `u32` (the value always fits in 24 bits; callers `@intCast` to
// `u24` fields which we model as `u32`).
fn color_to_id(c: crate::style::Color) -> u32 {
    // u24
    match c {
        crate::style::Color::None => 0,
        crate::style::Color::Palette(v) => v as u32,
        crate::style::Color::Rgb(rgb) => {
            let r: u32 = rgb.r as u32;
            let g: u32 = rgb.g as u32;
            let b: u32 = rgb.b as u32;
            (r << 16) | (g << 8) | b
        }
    }
}

/// Get the row/col index for a diacritic codepoint. These are 0-indexed.
fn get_index(cp: u32) -> Option<u32> {
    // PORT NOTE: `std.sort.binarySearch(u21, diacritics, cp, compare)`. We use
    // the slice `binary_search` which returns `Ok(idx)` on a match; the reference
    // returns the found index or null.
    match DIACRITICS.binary_search(&cp) {
        Ok(idx) => Some(idx as u32),
        Err(_) => None,
    }
}

/// These are the diacritics used with the Kitty graphics protocol
/// Unicode placement feature to specify the row/column for placement.
/// The index into the array determines the value.
///
/// This is derived from:
/// https://sw.kovidgoyal.net/kitty/_downloads/f0a0de9ec8d9ff4456206db8e0814937/rowcolumn-diacritics.txt
const DIACRITICS: &[u32] = &[
    0x0305, 0x030D, 0x030E, 0x0310, 0x0312, 0x033D, 0x033E, 0x033F, 0x0346, 0x034A, 0x034B, 0x034C,
    0x0350, 0x0351, 0x0352, 0x0357, 0x035B, 0x0363, 0x0364, 0x0365, 0x0366, 0x0367, 0x0368, 0x0369,
    0x036A, 0x036B, 0x036C, 0x036D, 0x036E, 0x036F, 0x0483, 0x0484, 0x0485, 0x0486, 0x0487, 0x0592,
    0x0593, 0x0594, 0x0595, 0x0597, 0x0598, 0x0599, 0x059C, 0x059D, 0x059E, 0x059F, 0x05A0, 0x05A1,
    0x05A8, 0x05A9, 0x05AB, 0x05AC, 0x05AF, 0x05C4, 0x0610, 0x0611, 0x0612, 0x0613, 0x0614, 0x0615,
    0x0616, 0x0617, 0x0657, 0x0658, 0x0659, 0x065A, 0x065B, 0x065D, 0x065E, 0x06D6, 0x06D7, 0x06D8,
    0x06D9, 0x06DA, 0x06DB, 0x06DC, 0x06DF, 0x06E0, 0x06E1, 0x06E2, 0x06E4, 0x06E7, 0x06E8, 0x06EB,
    0x06EC, 0x0730, 0x0732, 0x0733, 0x0735, 0x0736, 0x073A, 0x073D, 0x073F, 0x0740, 0x0741, 0x0743,
    0x0745, 0x0747, 0x0749, 0x074A, 0x07EB, 0x07EC, 0x07ED, 0x07EE, 0x07EF, 0x07F0, 0x07F1, 0x07F3,
    0x0816, 0x0817, 0x0818, 0x0819, 0x081B, 0x081C, 0x081D, 0x081E, 0x081F, 0x0820, 0x0821, 0x0822,
    0x0823, 0x0825, 0x0826, 0x0827, 0x0829, 0x082A, 0x082B, 0x082C, 0x082D, 0x0951, 0x0953, 0x0954,
    0x0F82, 0x0F83, 0x0F86, 0x0F87, 0x135D, 0x135E, 0x135F, 0x17DD, 0x193A, 0x1A17, 0x1A75, 0x1A76,
    0x1A77, 0x1A78, 0x1A79, 0x1A7A, 0x1A7B, 0x1A7C, 0x1B6B, 0x1B6D, 0x1B6E, 0x1B6F, 0x1B70, 0x1B71,
    0x1B72, 0x1B73, 0x1CD0, 0x1CD1, 0x1CD2, 0x1CDA, 0x1CDB, 0x1CE0, 0x1DC0, 0x1DC1, 0x1DC3, 0x1DC4,
    0x1DC5, 0x1DC6, 0x1DC7, 0x1DC8, 0x1DC9, 0x1DCB, 0x1DCC, 0x1DD1, 0x1DD2, 0x1DD3, 0x1DD4, 0x1DD5,
    0x1DD6, 0x1DD7, 0x1DD8, 0x1DD9, 0x1DDA, 0x1DDB, 0x1DDC, 0x1DDD, 0x1DDE, 0x1DDF, 0x1DE0, 0x1DE1,
    0x1DE2, 0x1DE3, 0x1DE4, 0x1DE5, 0x1DE6, 0x1DFE, 0x20D0, 0x20D1, 0x20D4, 0x20D5, 0x20D6, 0x20D7,
    0x20DB, 0x20DC, 0x20E1, 0x20E7, 0x20E9, 0x20F0, 0x2CEF, 0x2CF0, 0x2CF1, 0x2DE0, 0x2DE1, 0x2DE2,
    0x2DE3, 0x2DE4, 0x2DE5, 0x2DE6, 0x2DE7, 0x2DE8, 0x2DE9, 0x2DEA, 0x2DEB, 0x2DEC, 0x2DED, 0x2DEE,
    0x2DEF, 0x2DF0, 0x2DF1, 0x2DF2, 0x2DF3, 0x2DF4, 0x2DF5, 0x2DF6, 0x2DF7, 0x2DF8, 0x2DF9, 0x2DFA,
    0x2DFB, 0x2DFC, 0x2DFD, 0x2DFE, 0x2DFF, 0xA66F, 0xA67C, 0xA67D, 0xA6F0, 0xA6F1, 0xA8E0, 0xA8E1,
    0xA8E2, 0xA8E3, 0xA8E4, 0xA8E5, 0xA8E6, 0xA8E7, 0xA8E8, 0xA8E9, 0xA8EA, 0xA8EB, 0xA8EC, 0xA8ED,
    0xA8EE, 0xA8EF, 0xA8F0, 0xA8F1, 0xAAB0, 0xAAB2, 0xAAB3, 0xAAB7, 0xAAB8, 0xAABE, 0xAABF, 0xAAC1,
    0xFE20, 0xFE21, 0xFE22, 0xFE23, 0xFE24, 0xFE25, 0xFE26, 0x10A0F, 0x10A38, 0x1D185, 0x1D186,
    0x1D187, 0x1D188, 0x1D189, 0x1D1AA, 0x1D1AB, 0x1D1AC, 0x1D1AD, 0x1D242, 0x1D243, 0x1D244,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unicode_diacritic_sorted() {
        // diacritics must be sorted since we use a binary search.
        // PORT NOTE: `std.sort.isSorted(u21, diacritics, {}, lessThan)` with a
        // strict `lhs < rhs`. `is_sorted` over the slice is the direct
        // equivalent (non-strict, but the data has no duplicates, so identical).
        assert!(DIACRITICS.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn unicode_diacritic() {
        // Some spot checks based on Kitty behavior
        assert_eq!(30, get_index(0x483).unwrap());
        assert_eq!(294, get_index(0x1d242).unwrap());
    }

    // PORT NOTE: The following 14 reference tests are DEFERRED to vt-12. They
    // exercise `placementIterator` and `Placement.renderPlacement` against a
    // live `Terminal` (and, for the render tests, an `ImageStorage`). Both
    // `Terminal` (`Terminal.zig`) and `ImageStorage` (`graphics_storage.zig`)
    // are deferred to vt-12 (they depend on `Screen`), so these tests cannot be
    // built here. They will be ported assertion-for-assertion once those deps
    // land. The reference test names are:
    //   - "unicode placement: none"
    //   - "unicode placement: single row/col"
    //   - "unicode placement: continuation break"
    //   - "unicode placement: continuation with diacritics set"
    //   - "unicode placement: continuation with no col"
    //   - "unicode placement: continuation with no diacritics"
    //   - "unicode placement: run ending"
    //   - "unicode placement: run starting in the middle"
    //   - "unicode placement: specifying image id as palette"
    //   - "unicode placement: specifying image id with high bits"
    //   - "unicode placement: specifying placement id as palette"
    //   - "unicode render placement: dog 4x2"
    //   - "unicode render placement: dog 2x2 with blank cells"
    //   - "unicode render placement: dog 1x1"
}
