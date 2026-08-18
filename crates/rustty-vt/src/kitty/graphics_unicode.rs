//! rustty-vt kitty/graphics_unicode
//!
//! PORTS: ghostty src/terminal/kitty/graphics_unicode.zig
//!
//! PORT STATUS: ported

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
    /// Take this virtual placement and convert it to a render placement.
    // PORT NOTE: the reference returns `Error!RenderPlacement` and uses
    // `RenderPlacement` (= `graphics.RenderPlacement` = `graphics_render.Placement`).
    // We import it from the kitty graphics aggregator.
    pub fn render_placement(
        &self,
        storage: &crate::kitty::graphics::ImageStorage,
        img: &crate::kitty::graphics::Image,
        cell_width: u32,
        cell_height: u32,
    ) -> Result<crate::kitty::graphics::RenderPlacement, Error> {
        // In this function, there is a variable naming convention to try
        // to make it slightly less confusing. The prefix will tell you what
        // coordinate/size space a variable lives in:
        // - img_* is for the original image
        // - p_* is for the final placement
        // - vp_* is for the virtual placement

        // Determine the grid size that this virtual placement fits into.
        let p_grid = self.grid(storage, img, cell_width, cell_height)?;

        // From here on out we switch to floating point math. These are
        // constants that we'll reference repeatedly.
        let img_width_f64: f64 = img.width as f64;
        let img_height_f64: f64 = img.height as f64;

        // Next we have to fit the source image into the grid size while preserving
        // aspect ratio. We will center the image horizontally/vertically if
        // necessary.
        // PORT NOTE: the reference `p_scale` is an anonymous struct with field
        // defaults (`x_offset: f64 = 0`, ...) computed by a labeled `scale:` block.
        // We model it as a local struct initialized fully in each branch (the
        // unset offsets default to `0.0`, matching the reference defaults).
        struct PScale {
            /// The offsets are pixels from the top-left of the placement-sized
            /// image in order to center the image as necessary.
            x_offset: f64,
            y_offset: f64,

            /// The multipliers to apply to the width/height of the original
            /// image size in order to reach the placement size.
            x_scale: f64,
            y_scale: f64,
        }
        let p_scale: PScale = {
            let p_rows_px: f64 = (p_grid.rows * cell_height) as f64;
            let p_cols_px: f64 = (p_grid.columns * cell_width) as f64;
            if img_width_f64 * p_rows_px > img_height_f64 * p_cols_px {
                // Image is wider than the grid, fit width and center height
                let x_scale = p_cols_px / img_width_f64.max(1.0);
                let y_scale = x_scale;
                let y_offset = (p_rows_px - img_height_f64 * y_scale) / 2.0;
                PScale {
                    x_scale,
                    y_scale,
                    y_offset,
                    x_offset: 0.0,
                }
            } else {
                // Image is taller than the grid, fit height and center width
                let y_scale = p_rows_px / img_height_f64.max(1.0);
                let x_scale = y_scale;
                let x_offset = (p_cols_px - img_width_f64 * x_scale) / 2.0;
                PScale {
                    x_scale,
                    y_scale,
                    x_offset,
                    y_offset: 0.0,
                }
            }
        };

        // Scale our original image according to the aspect ratio
        // and padding calculated for p_scale.
        struct ImgScaled {
            x_offset: f64,
            y_offset: f64,
            width: f64,
            height: f64,
        }
        let img_scaled: ImgScaled = {
            let x_offset: f64 = p_scale.x_offset / p_scale.x_scale;
            let y_offset: f64 = p_scale.y_offset / p_scale.y_scale;
            let width: f64 = img_width_f64 + (x_offset * 2.0);
            let height: f64 = img_height_f64 + (y_offset * 2.0);
            ImgScaled {
                x_offset,
                y_offset,
                width,
                height,
            }
        };

        // Calculate the source rectangle for the scaled image. These
        // coordinates are in the scaled image space.
        struct ImgScaleSource {
            x: f64,
            y: f64,
            width: f64,
            height: f64,
        }
        let mut img_scale_source: ImgScaleSource = {
            // Float-converted values we already have
            let vp_width: f64 = self.width as f64;
            let vp_height: f64 = self.height as f64;
            let vp_col: f64 = self.col as f64;
            let vp_row: f64 = self.row as f64;
            let p_grid_cols: f64 = p_grid.columns as f64;
            let p_grid_rows: f64 = p_grid.rows as f64;

            // Calculate the scaled source rectangle for the image, undoing
            // the aspect ratio scaling as necessary.
            let width: f64 = img_scaled.width * (vp_width / p_grid_cols);
            let height: f64 = img_scaled.height * (vp_height / p_grid_rows);
            let x: f64 = img_scaled.width * (vp_col / p_grid_cols);
            let y: f64 = img_scaled.height * (vp_row / p_grid_rows);

            ImgScaleSource {
                width,
                height,
                x,
                y,
            }
        };

        // The destination rectangle. The x/y is specified by offsets from
        // the top-left since that's how our RenderPlacement works.
        struct PDest {
            x_offset: f64,
            y_offset: f64,
            width: f64,
            height: f64,
        }
        let p_dest: PDest = {
            let mut x_offset: f64 = 0.0;
            let mut y_offset: f64 = 0.0;
            let mut width: f64 = (self.width * cell_width) as f64;
            let mut height: f64 = (self.height * cell_height) as f64;

            if img_scale_source.y < img_scaled.y_offset {
                // If our source rect y is within the offset area, we need to
                // adjust our source rect and destination since the source texture
                // doesn't actually have the offset area blank.
                let offset: f64 = img_scaled.y_offset - img_scale_source.y;
                img_scale_source.height -= offset;
                y_offset = offset;
                height -= offset * p_scale.y_scale;
                img_scale_source.y = 0.0;

                // If our height is greater than our original height,
                // bring it back down. This addresses the case where the top
                // and bottom offsets are both used.
                if img_scale_source.height > img_height_f64 {
                    img_scale_source.height = img_height_f64;
                    height = img_height_f64 * p_scale.y_scale;
                }
            } else if img_scale_source.y + img_scale_source.height
                > img_scaled.height - img_scaled.y_offset
            {
                // if our y is in our bottom offset area, we need to shorten the
                // source to fit in the cell.
                img_scale_source.y -= img_scaled.y_offset;
                img_scale_source.height =
                    img_scaled.height - img_scaled.y_offset - img_scale_source.y;
                img_scale_source.height -= img_scaled.y_offset;
                height = img_scale_source.height * p_scale.y_scale;
            } else {
                img_scale_source.y -= img_scaled.y_offset;
            }

            if img_scale_source.x < img_scaled.x_offset {
                // If our source rect x is within the offset area, we need to
                // adjust our source rect and destination since the source texture
                // doesn't actually have the offset area blank.
                let offset: f64 = img_scaled.x_offset - img_scale_source.x;
                img_scale_source.width -= offset;
                x_offset = offset;
                width -= offset * p_scale.x_scale;
                img_scale_source.x = 0.0;

                // If our width is greater than our original width,
                // bring it back down. This addresses the case where the left
                // and right offsets are both used.
                if img_scale_source.width > img_width_f64 {
                    img_scale_source.width = img_width_f64;
                    width = img_width_f64 * p_scale.x_scale;
                }
            } else if img_scale_source.x + img_scale_source.width
                > img_scaled.width - img_scaled.x_offset
            {
                // if our x is in our right offset area, we need to shorten the
                // source to fit in the cell.
                img_scale_source.x -= img_scaled.x_offset;
                img_scale_source.width =
                    img_scaled.width - img_scaled.x_offset - img_scale_source.x;
                img_scale_source.width -= img_scaled.x_offset;
                width = img_scale_source.width * p_scale.x_scale;
            } else {
                img_scale_source.x -= img_scaled.x_offset;
            }

            // If our modified source width/height is less than zero then
            // we render nothing because it means we're rendering outside
            // of the visible image.
            if img_scale_source.width <= 0.0 || img_scale_source.height <= 0.0 {
                // PORT NOTE: `return .{ .top_left = self.pin }` — only `top_left`
                // set, the rest default to 0 (see RenderPlacement::from_top_left).
                return Ok(crate::kitty::graphics::RenderPlacement::from_top_left(
                    Pin {
                        node: self.pin.node,
                        y: self.pin.y,
                        x: self.pin.x,
                        garbage: self.pin.garbage,
                    },
                ));
            }

            PDest {
                x_offset: x_offset * p_scale.x_scale,
                y_offset: y_offset * p_scale.y_scale,
                width,
                height,
            }
        };
        // log.warn("img_width={} img_height={}\np_grid={}\np_scale={}\nimg_scaled={}\nimg_scale_source={}\np_dest={}\n", .{
        //     img_width_f64,
        //     img_height_f64,
        //     p_grid,
        //     p_scale,
        //     img_scaled,
        //     img_scale_source,
        //     p_dest,
        // });

        Ok(crate::kitty::graphics::RenderPlacement {
            top_left: Pin {
                node: self.pin.node,
                y: self.pin.y,
                x: self.pin.x,
                garbage: self.pin.garbage,
            },
            offset_x: p_dest.x_offset.round() as u32,
            offset_y: p_dest.y_offset.round() as u32,
            source_x: img_scale_source.x.round() as u32,
            source_y: img_scale_source.y.round() as u32,
            source_width: img_scale_source.width.round() as u32,
            source_height: img_scale_source.height.round() as u32,
            dest_width: p_dest.width.round() as u32,
            dest_height: p_dest.height.round() as u32,
        })
    }

    // Calculate the grid size for the placement. For virtual placements,
    // we use the requested row/cols. If either isn't specified, we choose
    // the best size based on the image size to fit the entire image in its
    // original size.
    //
    // This part of the code does NOT do preserve any aspect ratios. Its
    // dumbly fitting the image into the grid size -- possibly user specified.
    // PORT NOTE: the reference returns `!struct { rows: u32, columns: u32 }`. We
    // use a named local-ish struct `Grid` (defined below) for the success value.
    // PORT NOTE: the reference rounds up with the verbatim `(image.height +
    // cell_height - 1) / cell_height` idiom; clippy would rewrite it to
    // `div_ceil`. We keep the reference arithmetic per R1 and silence the lint.
    #[allow(clippy::manual_div_ceil)]
    fn grid(
        &self,
        storage: &crate::kitty::graphics::ImageStorage,
        image: &crate::kitty::graphics::Image,
        cell_width: u32,
        cell_height: u32,
    ) -> Result<Grid, Error> {
        // Get the placement. If an ID is specified we look for the exact one.
        // If no ID, then we find the first virtual placement for this image.
        // PORT NOTE: the reference uses `storage.placements.get(.{...}) orelse
        // return Error.PlacementMissingPlacement` for the ID case, and a
        // labeled `placement:` block iterating `storage.placements.iterator()`
        // for the no-ID case. The success value is `Placement` (copied by value
        // in the reference); we hold a reference to the stored placement, which
        // is sufficient since we only read `rows`/`columns`.
        let placement: &crate::kitty::graphics_storage::Placement = if self.placement_id > 0 {
            match storage
                .placements
                .get(&crate::kitty::graphics_storage::PlacementKey {
                    image_id: self.image_id,
                    placement_id: crate::kitty::graphics_storage::PlacementId {
                        tag: crate::kitty::graphics_storage::PlacementIdTag::External,
                        id: self.placement_id,
                    },
                }) {
                Some(p) => p,
                None => {
                    return Err(Error::PlacementMissingPlacement);
                }
            }
        } else {
            let mut found: Option<&crate::kitty::graphics_storage::Placement> = None;
            for (key, value) in storage.placements.iter() {
                if key.image_id == self.image_id
                    && matches!(
                        value.location,
                        crate::kitty::graphics_storage::Location::Virtual
                    )
                {
                    found = Some(value);
                    break;
                }
            }

            match found {
                Some(p) => p,
                None => return Err(Error::PlacementMissingPlacement),
            }
        };

        // Use requested rows/columns if specified
        // For unspecified rows/columns, calculate based on the image size.
        let mut rows = placement.rows;
        let mut columns = placement.columns;
        if rows == 0 {
            rows = (image.height + cell_height - 1) / cell_height;
        }
        if columns == 0 {
            columns = (image.width + cell_width - 1) / cell_width;
        }
        Ok(Grid {
            rows: match crate::size::CellCountInt::try_from(rows) {
                Ok(v) => v as u32,
                Err(_) => return Err(Error::PlacementGridOutOfBounds),
            },
            columns: match crate::size::CellCountInt::try_from(columns) {
                Ok(v) => v as u32,
                Err(_) => return Err(Error::PlacementGridOutOfBounds),
            },
        })
    }
}

/// PORT NOTE: the reference `grid` returns an anonymous `struct { rows: u32,
/// columns: u32 }`. We give it a name. The fields are cast through
/// `CellCountInt` (u16) for the bounds check, then widened back to `u32`
/// (the reference fields are `u32` even though the cast target is u16).
struct Grid {
    rows: u32,
    columns: u32,
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
    match c.tag() {
        crate::style::ColorTag::None => 0,
        crate::style::ColorTag::Palette => c.palette() as u32,
        crate::style::ColorTag::Rgb => {
            let rgb = c.rgb();
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

    // PORT NOTE: the following tests construct a `Terminal` and (for the render
    // tests) an `ImageStorage`. The reference relies on `Terminal.Options` field
    // defaults via `terminal.Terminal.init(alloc, .{ .rows, .cols })`; Rust
    // `Options` has no field defaults, so we use the same `opts` helper the rest
    // of the vt tests use (mirroring the reference `Options` declaration). The
    // reference uses `t.screens.active.pages.getTopLeft(.viewport)` for the pin;
    // we go through the raw `*mut Screen` like the rest of the crate.
    use crate::kitty::graphics::{Image, ImageStorage};
    use crate::kitty::graphics_storage::Location;
    use crate::point;
    use crate::sgr::Attribute;
    use crate::terminal::Terminal;

    fn opts(
        cols: crate::size::CellCountInt,
        rows: crate::size::CellCountInt,
    ) -> crate::terminal::Options {
        crate::terminal::Options {
            cols,
            rows,
            max_scrollback_bytes: Some(10_000),
            max_scrollback_lines: None,
            colors: crate::terminal::Colors::default(),
            default_modes: crate::modes::ModePacked::default(),
            default_cursor_style: crate::screen::CursorStyle::Block,
            default_cursor_blink: Some(false),
            kitty_image_storage_limit: 320 * 1000 * 1000,
            kitty_image_loading_limits: crate::kitty::graphics_image::Limits::DIRECT,
        }
    }

    #[test]
    fn unicode_placement_none() {
        let alloc = allocator_api2::alloc::Global;
        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();
        // defer t.deinit(alloc);
        t.modes.set(crate::modes::Mode::GraphemeCluster, true);

        // Single cell
        t.print_string("hello\nworld\n1\n2".as_bytes()).unwrap();

        // No placements
        let pin = unsafe { &(*t.screens.active).pages }.get_top_left(point::Tag::Viewport);
        let mut it = placement_iterator(pin, None);
        assert!(it.next().is_none());

        t.deinit(alloc);
    }

    #[test]
    fn unicode_placement_single_row_col() {
        let alloc = allocator_api2::alloc::Global;
        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();
        t.modes.set(crate::modes::Mode::GraphemeCluster, true);

        // Single cell
        t.print_string("\u{10EEEE}\u{0305}\u{0305}".as_bytes())
            .unwrap();

        // Get our top left pin
        let pin = unsafe { &(*t.screens.active).pages }.get_top_left(point::Tag::Viewport);

        // Should have exactly one placement
        let mut it = placement_iterator(pin, None);
        {
            let p = it.next().unwrap();
            assert_eq!(0, p.image_id);
            assert_eq!(0, p.placement_id);
            assert_eq!(0, p.row);
            assert_eq!(0, p.col);
        }
        assert!(it.next().is_none());

        t.deinit(alloc);
    }

    #[test]
    fn unicode_placement_continuation_break() {
        let alloc = allocator_api2::alloc::Global;
        let mut t = Terminal::init(alloc, opts(10, 5)).unwrap();
        t.modes.set(crate::modes::Mode::GraphemeCluster, true);

        // Two runs because it jumps cols
        t.print_string("\u{10EEEE}\u{0305}\u{0305}".as_bytes())
            .unwrap();
        t.print_string("\u{10EEEE}\u{0305}\u{030E}".as_bytes())
            .unwrap();

        // Get our top left pin
        let pin = unsafe { &(*t.screens.active).pages }.get_top_left(point::Tag::Viewport);

        // Should have exactly one placement
        let mut it = placement_iterator(pin, None);
        {
            let p = it.next().unwrap();
            assert_eq!(0, p.image_id);
            assert_eq!(0, p.placement_id);
            assert_eq!(0, p.row);
            assert_eq!(0, p.col);
            assert_eq!(1, p.width);
        }
        {
            let p = it.next().unwrap();
            assert_eq!(0, p.image_id);
            assert_eq!(0, p.placement_id);
            assert_eq!(0, p.row);
            assert_eq!(2, p.col);
            assert_eq!(1, p.width);
        }
        assert!(it.next().is_none());

        t.deinit(alloc);
    }

    #[test]
    fn unicode_placement_continuation_with_diacritics_set() {
        let alloc = allocator_api2::alloc::Global;
        let mut t = Terminal::init(alloc, opts(10, 5)).unwrap();
        t.modes.set(crate::modes::Mode::GraphemeCluster, true);

        // Three cells. They'll continue even though they're explicit
        t.print_string("\u{10EEEE}\u{0305}\u{0305}".as_bytes())
            .unwrap();
        t.print_string("\u{10EEEE}\u{0305}\u{030D}".as_bytes())
            .unwrap();
        t.print_string("\u{10EEEE}\u{0305}\u{030E}".as_bytes())
            .unwrap();

        // Get our top left pin
        let pin = unsafe { &(*t.screens.active).pages }.get_top_left(point::Tag::Viewport);

        // Should have exactly one placement
        let mut it = placement_iterator(pin, None);
        {
            let p = it.next().unwrap();
            assert_eq!(0, p.image_id);
            assert_eq!(0, p.placement_id);
            assert_eq!(0, p.row);
            assert_eq!(0, p.col);
            assert_eq!(3, p.width);
        }
        assert!(it.next().is_none());

        t.deinit(alloc);
    }

    #[test]
    fn unicode_placement_continuation_with_no_col() {
        let alloc = allocator_api2::alloc::Global;
        let mut t = Terminal::init(alloc, opts(10, 5)).unwrap();
        t.modes.set(crate::modes::Mode::GraphemeCluster, true);

        // Three cells. They'll continue even though they're explicit
        t.print_string("\u{10EEEE}\u{0305}\u{0305}".as_bytes())
            .unwrap();
        t.print_string("\u{10EEEE}\u{0305}".as_bytes()).unwrap();
        t.print_string("\u{10EEEE}\u{0305}".as_bytes()).unwrap();

        // Get our top left pin
        let pin = unsafe { &(*t.screens.active).pages }.get_top_left(point::Tag::Viewport);

        // Should have exactly one placement
        let mut it = placement_iterator(pin, None);
        {
            let p = it.next().unwrap();
            assert_eq!(0, p.image_id);
            assert_eq!(0, p.placement_id);
            assert_eq!(0, p.row);
            assert_eq!(0, p.col);
            assert_eq!(3, p.width);
        }
        assert!(it.next().is_none());

        t.deinit(alloc);
    }

    #[test]
    fn unicode_placement_continuation_with_no_diacritics() {
        let alloc = allocator_api2::alloc::Global;
        let mut t = Terminal::init(alloc, opts(10, 5)).unwrap();
        t.modes.set(crate::modes::Mode::GraphemeCluster, true);

        // Three cells. They'll continue even though they're explicit
        t.print_string("\u{10EEEE}".as_bytes()).unwrap();
        t.print_string("\u{10EEEE}".as_bytes()).unwrap();
        t.print_string("\u{10EEEE}".as_bytes()).unwrap();

        // Get our top left pin
        let pin = unsafe { &(*t.screens.active).pages }.get_top_left(point::Tag::Viewport);

        // Should have exactly one placement
        let mut it = placement_iterator(pin, None);
        {
            let p = it.next().unwrap();
            assert_eq!(0, p.image_id);
            assert_eq!(0, p.placement_id);
            assert_eq!(0, p.row);
            assert_eq!(0, p.col);
            assert_eq!(3, p.width);
        }
        assert!(it.next().is_none());

        t.deinit(alloc);
    }

    #[test]
    fn unicode_placement_run_ending() {
        let alloc = allocator_api2::alloc::Global;
        let mut t = Terminal::init(alloc, opts(10, 5)).unwrap();
        t.modes.set(crate::modes::Mode::GraphemeCluster, true);

        // Three cells. They'll continue even though they're explicit
        t.print_string("\u{10EEEE}\u{0305}\u{0305}".as_bytes())
            .unwrap();
        t.print_string("\u{10EEEE}\u{0305}\u{030D}".as_bytes())
            .unwrap();
        t.print_string("ABC".as_bytes()).unwrap();

        // Get our top left pin
        let pin = unsafe { &(*t.screens.active).pages }.get_top_left(point::Tag::Viewport);

        // Should have exactly one placement
        let mut it = placement_iterator(pin, None);
        {
            let p = it.next().unwrap();
            assert_eq!(0, p.image_id);
            assert_eq!(0, p.placement_id);
            assert_eq!(0, p.row);
            assert_eq!(0, p.col);
            assert_eq!(2, p.width);
        }
        assert!(it.next().is_none());

        t.deinit(alloc);
    }

    #[test]
    fn unicode_placement_run_starting_in_the_middle() {
        let alloc = allocator_api2::alloc::Global;
        let mut t = Terminal::init(alloc, opts(10, 5)).unwrap();
        t.modes.set(crate::modes::Mode::GraphemeCluster, true);

        // Three cells. They'll continue even though they're explicit
        t.print_string("ABC".as_bytes()).unwrap();
        t.print_string("\u{10EEEE}\u{0305}\u{0305}".as_bytes())
            .unwrap();
        t.print_string("\u{10EEEE}\u{0305}\u{030D}".as_bytes())
            .unwrap();

        // Get our top left pin
        let pin = unsafe { &(*t.screens.active).pages }.get_top_left(point::Tag::Viewport);

        // Should have exactly one placement
        let mut it = placement_iterator(pin, None);
        {
            let p = it.next().unwrap();
            assert_eq!(0, p.image_id);
            assert_eq!(0, p.placement_id);
            assert_eq!(0, p.row);
            assert_eq!(0, p.col);
            assert_eq!(2, p.width);
        }
        assert!(it.next().is_none());

        t.deinit(alloc);
    }

    #[test]
    fn unicode_placement_specifying_image_id_as_palette() {
        let alloc = allocator_api2::alloc::Global;
        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();
        t.modes.set(crate::modes::Mode::GraphemeCluster, true);

        // Single cell
        t.set_attribute(Attribute::Color256Fg(42)).unwrap();
        t.print_string("\u{10EEEE}\u{0305}\u{0305}".as_bytes())
            .unwrap();

        // Get our top left pin
        let pin = unsafe { &(*t.screens.active).pages }.get_top_left(point::Tag::Viewport);

        // Should have exactly one placement
        let mut it = placement_iterator(pin, None);
        {
            let p = it.next().unwrap();
            assert_eq!(42, p.image_id);
            assert_eq!(0, p.placement_id);
            assert_eq!(0, p.row);
            assert_eq!(0, p.col);
        }
        assert!(it.next().is_none());

        t.deinit(alloc);
    }

    #[test]
    fn unicode_placement_specifying_image_id_with_high_bits() {
        let alloc = allocator_api2::alloc::Global;
        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();
        t.modes.set(crate::modes::Mode::GraphemeCluster, true);

        // Single cell
        t.set_attribute(Attribute::Color256Fg(42)).unwrap();
        t.print_string("\u{10EEEE}\u{0305}\u{0305}\u{030E}".as_bytes())
            .unwrap();

        // Get our top left pin
        let pin = unsafe { &(*t.screens.active).pages }.get_top_left(point::Tag::Viewport);

        // Should have exactly one placement
        let mut it = placement_iterator(pin, None);
        {
            let p = it.next().unwrap();
            assert_eq!(33554474, p.image_id);
            assert_eq!(0, p.placement_id);
            assert_eq!(0, p.row);
            assert_eq!(0, p.col);
        }
        assert!(it.next().is_none());

        t.deinit(alloc);
    }

    #[test]
    fn unicode_placement_specifying_placement_id_as_palette() {
        let alloc = allocator_api2::alloc::Global;
        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();
        t.modes.set(crate::modes::Mode::GraphemeCluster, true);

        // Single cell
        t.set_attribute(Attribute::Color256Fg(42)).unwrap();
        t.set_attribute(Attribute::Color256Underline(21)).unwrap();
        t.print_string("\u{10EEEE}\u{0305}\u{0305}".as_bytes())
            .unwrap();

        // Get our top left pin
        let pin = unsafe { &(*t.screens.active).pages }.get_top_left(point::Tag::Viewport);

        // Should have exactly one placement
        let mut it = placement_iterator(pin, None);
        {
            let p = it.next().unwrap();
            assert_eq!(42, p.image_id);
            assert_eq!(21, p.placement_id);
            assert_eq!(0, p.row);
            assert_eq!(0, p.col);
        }
        assert!(it.next().is_none());

        t.deinit(alloc);
    }

    // Fish:
    // printf "\033_Gf=100,i=1,t=f,q=2;$(printf dog.png | base64)\033\\"
    // printf "\e[38;5;1m\U10EEEE\U0305\U0305\U10EEEE\U0305\U030D\U10EEEE\U0305\U030E\U10EEEE\U0305\U0310\e[39m\n"
    // printf "\e[38;5;1m\U10EEEE\U030D\U0305\U10EEEE\U030D\U030D\U10EEEE\U030D\U030E\U10EEEE\U030D\U0310\e[39m\n"
    // printf "\033_Ga=p,i=1,U=1,q=2,c=4,r=2\033\\"
    #[test]
    fn unicode_render_placement_dog_4x2() {
        let alloc = allocator_api2::alloc::Global;
        let cell_width = 36;
        let cell_height = 80;

        let mut t = Terminal::init(alloc, opts(100, 100)).unwrap();
        // defer t.deinit(alloc);
        let mut s: ImageStorage = ImageStorage::default();
        // defer s.deinit(alloc, t.screens.active);

        let image: Image = Image {
            id: 1,
            width: 500,
            height: 306,
            ..Image::default()
        };
        s.add_image(alloc, t.screens.active, image.clone()).unwrap();
        s.add_placement(
            alloc,
            t.screens.active,
            1,
            0,
            crate::kitty::graphics_storage::Placement {
                location: Location::Virtual,
                columns: 4,
                rows: 2,
                ..crate::kitty::graphics_storage::Placement::with_location(Location::Virtual)
            },
        )
        .unwrap();

        // Row 1
        {
            let cursor_pin = unsafe { *(*t.screens.active).cursor.page_pin };
            let p: Placement = Placement {
                pin: cursor_pin,
                image_id: 1,
                placement_id: 0,
                col: 0,
                row: 0,
                width: 4,
                height: 1,
            };
            let rp = p
                .render_placement(&s, &image, cell_width, cell_height)
                .unwrap();
            assert_eq!(0, rp.offset_x);
            assert_eq!(36, rp.offset_y);
            assert_eq!(0, rp.source_x);
            assert_eq!(0, rp.source_y);
            assert_eq!(500, rp.source_width);
            assert_eq!(153, rp.source_height);
            assert_eq!(144, rp.dest_width);
            assert_eq!(44, rp.dest_height);
        }
        // Row 2
        {
            let cursor_pin = unsafe { *(*t.screens.active).cursor.page_pin };
            let p: Placement = Placement {
                pin: cursor_pin,
                image_id: 1,
                placement_id: 0,
                col: 0,
                row: 1,
                width: 4,
                height: 1,
            };
            let rp = p
                .render_placement(&s, &image, cell_width, cell_height)
                .unwrap();
            assert_eq!(0, rp.offset_x);
            assert_eq!(0, rp.offset_y);
            assert_eq!(0, rp.source_x);
            assert_eq!(153, rp.source_y);
            assert_eq!(500, rp.source_width);
            assert_eq!(153, rp.source_height);
            assert_eq!(144, rp.dest_width);
            assert_eq!(44, rp.dest_height);
        }

        s.deinit(alloc, t.screens.active);
        t.deinit(alloc);
    }

    // Fish:
    // printf "\033_Gf=100,i=1,t=f,q=2;$(printf dog.png | base64)\033\\"
    // printf "\e[38;5;1m\U10EEEE\U0305\U0305\U10EEEE\U0305\U030D\U10EEEE\U0305\U030E\U10EEEE\U0305\U0310\e[39m\n"
    // printf "\e[38;5;1m\U10EEEE\U030D\U0305\U10EEEE\U030D\U030D\U10EEEE\U030D\U030E\U10EEEE\U030D\U0310\e[39m\n"
    // printf "\033_Ga=p,i=1,U=1,q=2,c=2,r=2\033\\"
    #[test]
    fn unicode_render_placement_dog_2x2_with_blank_cells() {
        let alloc = allocator_api2::alloc::Global;
        let cell_width = 36;
        let cell_height = 80;

        let mut t = Terminal::init(alloc, opts(100, 100)).unwrap();
        let mut s: ImageStorage = ImageStorage::default();

        let image: Image = Image {
            id: 1,
            width: 500,
            height: 306,
            ..Image::default()
        };
        s.add_image(alloc, t.screens.active, image.clone()).unwrap();
        s.add_placement(
            alloc,
            t.screens.active,
            1,
            0,
            crate::kitty::graphics_storage::Placement {
                location: Location::Virtual,
                columns: 2,
                rows: 2,
                ..crate::kitty::graphics_storage::Placement::with_location(Location::Virtual)
            },
        )
        .unwrap();

        // Row 1
        {
            let cursor_pin = unsafe { *(*t.screens.active).cursor.page_pin };
            let p: Placement = Placement {
                pin: cursor_pin,
                image_id: 1,
                placement_id: 0,
                col: 0,
                row: 0,
                width: 4,
                height: 1,
            };
            let rp = p
                .render_placement(&s, &image, cell_width, cell_height)
                .unwrap();
            assert_eq!(0, rp.offset_x);
            assert_eq!(58, rp.offset_y);
            assert_eq!(0, rp.source_x);
            assert_eq!(0, rp.source_y);
            assert_eq!(500, rp.source_width);
            assert_eq!(153, rp.source_height);
            assert_eq!(72, rp.dest_width);
            assert_eq!(22, rp.dest_height);
        }
        // Row 2
        {
            let cursor_pin = unsafe { *(*t.screens.active).cursor.page_pin };
            let p: Placement = Placement {
                pin: cursor_pin,
                image_id: 1,
                placement_id: 0,
                col: 0,
                row: 1,
                width: 4,
                height: 1,
            };
            let rp = p
                .render_placement(&s, &image, cell_width, cell_height)
                .unwrap();
            assert_eq!(0, rp.offset_x);
            assert_eq!(0, rp.offset_y);
            assert_eq!(0, rp.source_x);
            assert_eq!(153, rp.source_y);
            assert_eq!(500, rp.source_width);
            assert_eq!(153, rp.source_height);
            assert_eq!(72, rp.dest_width);
            assert_eq!(22, rp.dest_height);
        }

        s.deinit(alloc, t.screens.active);
        t.deinit(alloc);
    }

    // Fish:
    // printf "\033_Gf=100,i=1,t=f,q=2;$(printf dog.png | base64)\033\\"
    // printf "\e[38;5;1m\U10EEEE\U0305\U0305\U10EEEE\U0305\U030D\U10EEEE\U0305\U030E\U10EEEE\U0305\U0310\e[39m\n"
    // printf "\033_Ga=p,i=1,U=1,q=2,c=1,r=1\033\\"
    #[test]
    fn unicode_render_placement_dog_1x1() {
        let alloc = allocator_api2::alloc::Global;
        let cell_width = 36;
        let cell_height = 80;

        let mut t = Terminal::init(alloc, opts(100, 100)).unwrap();
        let mut s: ImageStorage = ImageStorage::default();

        let image: Image = Image {
            id: 1,
            width: 500,
            height: 306,
            ..Image::default()
        };
        s.add_image(alloc, t.screens.active, image.clone()).unwrap();
        s.add_placement(
            alloc,
            t.screens.active,
            1,
            0,
            crate::kitty::graphics_storage::Placement {
                location: Location::Virtual,
                columns: 1,
                rows: 1,
                ..crate::kitty::graphics_storage::Placement::with_location(Location::Virtual)
            },
        )
        .unwrap();

        // Row 1
        {
            let cursor_pin = unsafe { *(*t.screens.active).cursor.page_pin };
            let p: Placement = Placement {
                pin: cursor_pin,
                image_id: 1,
                placement_id: 0,
                col: 0,
                row: 0,
                width: 4,
                height: 1,
            };
            let rp = p
                .render_placement(&s, &image, cell_width, cell_height)
                .unwrap();
            assert_eq!(0, rp.offset_x);
            assert_eq!(29, rp.offset_y);
            assert_eq!(0, rp.source_x);
            assert_eq!(0, rp.source_y);
            assert_eq!(500, rp.source_width);
            assert_eq!(306, rp.source_height);
            assert_eq!(36, rp.dest_width);
            assert_eq!(22, rp.dest_height);
        }

        s.deinit(alloc, t.screens.active);
        t.deinit(alloc);
    }
}
