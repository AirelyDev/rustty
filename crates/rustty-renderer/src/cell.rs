//! PORTS: ghostty src/renderer/cell.zig
//!
//! PORT STATUS: done — every `test{}` block ported.

// const std = @import("std");
// const Allocator = std.mem.Allocator;
// const assert = @import("../quirks.zig").inlineAssert;
// const font = @import("../font/main.zig");
// const terminal = @import("../terminal/main.zig");
// const renderer = @import("../renderer.zig");
// const shaderpkg = renderer.Renderer.API.shaders;
// const ArrayListCollection = @import("../datastruct/array_list_collection.zig").ArrayListCollection;
// const symbols = @import("../unicode/symbols_table.zig").table;
//
// PORT NOTE: `font` (`../font/main.zig`) is imported by the reference but is not
// used by any declaration in this subset (the `SharedGrid` glyph consumption
// lives in the generic renderer, deferred to R-d/Phase 3), so no `use` is bound
// here. `shaderpkg` — the backend's shader package (`Renderer.API.shaders`) — is
// the vertex-data struct set; in this headless port it is `crate::shaders`.
use rustty_datastruct::array_list_collection::ArrayListCollection;
use rustty_render_types::GridSize;
use rustty_unicode::symbols_table::table as symbols;
use rustty_util::quirks::inlineAssert as assert;
use rustty_vt::page::{self, u21};
use rustty_vt::size::CellCountInt;

use crate::cursor::Style as CursorStyle;
use crate::shaders::{CellBg, CellText};

/// The possible cell content keys that exist.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Key {
    Bg,
    Text,
    Underline,
    Strikethrough,
    Overline,
}

impl Key {
    // Returns the GPU vertex type for this key.
    //
    // PORT NOTE: the reference returns a `type` from this `comptime` method
    // (`.bg => shaderpkg.CellBg`, the text-family keys => `shaderpkg.CellText`).
    // Rust cannot return a type from a value-level function; the mapping is
    // encoded structurally instead — `add` accepts `CellText` for the
    // text-family keys, and `.bg` is `unreachable` there (as in the reference).
    // The original body is preserved here as a comment:
    //
    //     pub fn CellType(self: Key) type {
    //         return switch (self) {
    //             .bg => shaderpkg.CellBg,
    //             .text,
    //             .underline,
    //             .strikethrough,
    //             .overline,
    //             => shaderpkg.CellText,
    //         };
    //     }
}

/// The contents of all the cells in the terminal.
///
/// The goal of this data structure is to allow for efficient row-wise
/// clearing of data from the GPU buffers, to allow for row-wise dirty
/// tracking to eliminate the overhead of rebuilding the GPU buffers
/// each frame.
///
/// Must be initialized by resizing before calling any operations.
pub struct Contents {
    pub size: GridSize,

    /// Flat array containing cell background colors for the terminal grid.
    ///
    /// Indexed as `bg_cells[row * size.columns + col]`.
    ///
    /// Prefer accessing with `Contents.bgCell(row, col).*` instead
    /// of directly indexing in order to avoid integer size bugs.
    //
    // PORT NOTE: the reference field is `[]shaderpkg.CellBg = undefined`; the Rust
    // port owns it as a `Vec<CellBg>` (empty until `resize`), which the global
    // allocator manages — the reference `deinit(alloc)` free is Rust's `Drop`.
    pub bg_cells: Vec<CellBg>,

    /// The ArrayListCollection which holds all of the foreground cells. When
    /// sized with Contents.resize the individual ArrayLists are given enough
    /// room that they can hold a single row with #cols glyphs, underlines, and
    /// strikethroughs; however, appendAssumeCapacity MUST NOT be used since
    /// it is possible to exceed this with combining glyphs that add a glyph
    /// but take up no column since they combine with the previous one, as
    /// well as with fonts that perform multi-substitutions for glyphs, which
    /// can result in a similar situation where multiple glyphs reside in the
    /// same column.
    ///
    /// Allocations should nevertheless be exceedingly rare since hitting the
    /// initial capacity of a list would require a row filled with underlined
    /// struck through characters, at least one of which is a multi-glyph
    /// composite.
    ///
    /// Rows are indexed as Contents.fg_rows[y + 1], because the first list in
    /// the collection is reserved for the cursor, which must be the first item
    /// in the buffer.
    ///
    /// Must be initialized by calling resize on the Contents struct before
    /// calling any operations.
    pub fg_rows: ArrayListCollection<CellText>,
}

// PORT NOTE: `pub const Contents = struct { ... = .{ ... } }` — the reference
// gives every field a default (`size = .{ .rows = 0, .columns = 0 }`,
// `bg_cells = undefined`, `fg_rows = .{ .lists = &.{} }`). `Default` reproduces
// the empty/zero initialization; `bg_cells`'s `undefined` becomes an empty `Vec`
// (never observed before `resize`, matching the reference contract).
impl Default for Contents {
    fn default() -> Self {
        Contents {
            size: GridSize {
                rows: 0,
                columns: 0,
            },
            bg_cells: Vec::new(),
            fg_rows: ArrayListCollection { lists: Vec::new() },
        }
    }
}

impl Contents {
    // PORT NOTE: the reference `deinit(self, alloc)` frees `bg_cells` and the
    // `fg_rows` collection. The Rust `Vec`s free on `Drop`; this method is kept
    // for API parity and returns the storage to empty.
    pub fn deinit(&mut self) {
        self.bg_cells = Vec::new();
        self.fg_rows.lists = Vec::new();
    }

    /// Resize the cell contents for the given grid size. This will
    /// always invalidate the entire cell contents.
    //
    // PORT NOTE: the reference returns `Allocator.Error!void` and threads
    // `errdefer` cleanup around fallible allocations. The Rust `Vec`/collection
    // allocations use the global allocator (which aborts on OOM, as the
    // datastruct leaf does), so this returns `()`; the reference's careful
    // errdefer ordering is unnecessary because no intermediate state is observed
    // on the abort-on-OOM path.
    pub fn resize(&mut self, size: GridSize) {
        self.size = size;

        let cell_count = (size.columns as usize) * (size.rows as usize);

        let bg_cells = vec![[0, 0, 0, 0]; cell_count];

        // The foreground lists can hold 3 types of items:
        // - Glyphs
        // - Underlines
        // - Strikethroughs
        // So we give them an initial capacity of size.columns * 3, which will
        // avoid any further allocations in the vast majority of cases. Sadly
        // we can not assume capacity though, since with combining glyphs that
        // form a single grapheme, and multi-substitutions in fonts, the number
        // of glyphs in a row is theoretically unlimited.
        //
        // We have size.rows + 2 lists because indexes 0 and size.rows - 1 are
        // used for special lists containing the cursor cell which need to
        // be first and last in the buffer, respectively.
        let mut fg_rows: ArrayListCollection<CellText> =
            ArrayListCollection::init((size.rows + 2) as usize, (size.columns * 3) as usize);

        // We don't need 3*cols worth of cells for the cursor lists, so we can
        // replace them with smaller lists. This is technically a tiny bit of
        // extra work but resize is not a hot function so it's worth it to not
        // waste the memory.
        fg_rows.lists[0] = Vec::with_capacity(1);
        fg_rows.lists[(size.rows + 1) as usize] = Vec::with_capacity(1);

        // Perform the swap, no going back from here.
        self.bg_cells = bg_cells;
        self.fg_rows = fg_rows;
    }

    /// Reset the cell contents to an empty state without resizing.
    pub fn reset(&mut self) {
        for c in self.bg_cells.iter_mut() {
            *c = [0, 0, 0, 0];
        }
        self.fg_rows.reset();
    }

    /// Set the cursor value. If the value is null then the cursor is hidden.
    pub fn set_cursor(&mut self, v: Option<CellText>, cursor_style: Option<CursorStyle>) {
        if self.size.rows == 0 {
            return;
        }
        self.fg_rows.lists[0].clear();
        let last = (self.size.rows + 1) as usize;
        self.fg_rows.lists[last].clear();

        let cell = match v {
            Some(c) => c,
            None => return,
        };
        let style = match cursor_style {
            Some(s) => s,
            None => return,
        };

        match style {
            // Block cursors should be drawn first
            CursorStyle::Block => self.fg_rows.lists[0].push(cell),
            // Other cursor styles should be drawn last
            CursorStyle::BlockHollow
            | CursorStyle::Bar
            | CursorStyle::Underline
            | CursorStyle::Lock => self.fg_rows.lists[last].push(cell),
        }
    }

    /// Returns the current cursor glyph if present, checking both cursor lists.
    pub fn get_cursor_glyph(&self) -> Option<CellText> {
        if self.size.rows == 0 {
            return None;
        }
        if !self.fg_rows.lists[0].is_empty() {
            return Some(self.fg_rows.lists[0][0]);
        }
        let last = (self.size.rows + 1) as usize;
        if !self.fg_rows.lists[last].is_empty() {
            return Some(self.fg_rows.lists[last][0]);
        }
        None
    }

    /// Access a background cell. Prefer this function over direct indexing
    /// of `bg_cells` in order to avoid integer size bugs causing overflows.
    #[inline]
    pub fn bg_cell(&mut self, row: usize, col: usize) -> &mut CellBg {
        &mut self.bg_cells[row * self.size.columns as usize + col]
    }

    /// Add a cell to the appropriate list. Adding the same cell twice will
    /// result in duplication in the vertex buffer. The caller should clear
    /// the corresponding row with Contents.clear to remove old cells first.
    //
    // PORT NOTE: the reference is `add(comptime key: Key, cell: key.CellType())`.
    // Every non-`.bg` key uses `CellText` (see `Key::CellType`), so this takes a
    // `CellText`; `.bg` is `unreachable!()` exactly as the reference's
    // `comptime unreachable`.
    pub fn add(&mut self, key: Key, cell: CellText) {
        let y = cell.grid_pos[1];

        assert(y < self.size.rows);

        match key {
            Key::Bg => unreachable!(),

            Key::Text | Key::Underline | Key::Strikethrough | Key::Overline =>
            // We have a special list containing the cursor cell at the start
            // of our fg row collection, so we need to add 1 to the y to get
            // the correct index.
            {
                self.fg_rows.lists[(y + 1) as usize].push(cell)
            }
        }
    }

    /// Clear all of the cell contents for a given row.
    pub fn clear(&mut self, y: CellCountInt) {
        assert(y < self.size.rows);

        let cols = self.size.columns as usize;
        let start = (y as usize) * cols;
        for c in self.bg_cells[start..start + cols].iter_mut() {
            *c = [0, 0, 0, 0];
        }

        // We have a special list containing the cursor cell at the start
        // of our fg row collection, so we need to add 1 to the y to get
        // the correct index.
        self.fg_rows.lists[(y + 1) as usize].clear();
    }
}

/// Returns true if a codepoint for a cell is a covering character. A covering
/// character is a character that covers the entire cell. This is used to
/// make window-padding-color=extend work better. See #2099.
pub fn is_covering(cp: u21) -> bool {
    match cp {
        // U+2588 FULL BLOCK
        0x2588 => true,

        _ => false,
    }
}

/// Returns true of the codepoint is a "symbol-like" character, which
/// for now we define as anything in a private use area, and anything
/// in several unicode blocks:
/// - Arrows
/// - Dingbats
/// - Emoticons
/// - Miscellaneous Symbols
/// - Enclosed Alphanumerics
/// - Enclosed Alphanumeric Supplement
/// - Miscellaneous Symbols and Pictographs
/// - Transport and Map Symbols
///
/// In the future it may be prudent to expand this to encompass more
/// symbol-like characters, and/or exclude some PUA sections.
pub fn is_symbol(cp: u21) -> bool {
    symbols().get(cp)
}

/// Returns the appropriate `constraint_width` for
/// the provided cell when rendering its glyph(s).
//
// PORT NOTE: the reference return type is `u2`; the ported `page::Cell::grid_width`
// returns `u8`, so this returns `u8` (values are only ever 1 or 2). The
// `raw_slice` is `[]const terminal.page.Cell` — see the `constraintWidth` test
// for how the reference's SoA `.items(.raw)` projection is rebuilt in the AoS
// Rust port.
pub fn constraint_width(raw_slice: &[page::Cell], x: usize, cols: usize) -> u8 {
    let cell = raw_slice[x];
    let cp = cell.codepoint();

    let grid_width = cell.grid_width();

    // If the grid width of the cell is 2, the constraint
    // width will always be 2, so we can just return early.
    if grid_width > 1 {
        return grid_width;
    }

    // We allow "symbol-like" glyphs to extend to 2 cells wide if there's
    // space, and if the previous glyph wasn't also a symbol. So if this
    // codepoint isn't a symbol then we can return the grid width.
    if !is_symbol(cp) {
        return grid_width;
    }

    // If we are at the end of the screen it must be constrained to one cell.
    if x == cols - 1 {
        return 1;
    }

    // If we have a previous cell and it was a symbol then we need
    // to also constrain. This is so that multiple PUA glyphs align.
    // This does not apply if the previous symbol is a graphics
    // element such as a block element or Powerline glyph.
    if x > 0 {
        let prev_cp = raw_slice[x - 1].codepoint();
        if is_symbol(prev_cp) && !is_graphics_element(prev_cp) {
            return 1;
        }
    }

    // If the next cell is whitespace, then we
    // allow the glyph to be up to two cells wide.
    let next_cp = raw_slice[x + 1].codepoint();
    if next_cp == 0 || is_space(next_cp) {
        return 2;
    }

    // Otherwise, this has to be 1 cell wide.
    1
}

/// Whether min contrast should be disabled for a given glyph. True
/// for graphics elements such as blocks and Powerline glyphs.
pub fn no_min_contrast(cp: u21) -> bool {
    is_graphics_element(cp)
}

// Some general spaces, others intentionally kept
// to force the font to render as a fixed width.
fn is_space(char: u21) -> bool {
    match char {
        0x0020 | // SPACE
        0x2002 // EN SPACE
        => true,
        _ => false,
    }
}

/// Returns true if the codepoint is used for terminal graphics, such
/// as box drawing characters, block elements, and Powerline glyphs.
fn is_graphics_element(char: u21) -> bool {
    is_box_drawing(char)
        || is_block_element(char)
        || is_legacy_computing(char)
        || is_powerline(char)
}

// Returns true if the codepoint is a box drawing character.
fn is_box_drawing(char: u21) -> bool {
    matches!(char, 0x2500..=0x257F)
}

// Returns true if the codepoint is a block element.
fn is_block_element(char: u21) -> bool {
    matches!(char, 0x2580..=0x259F)
}

// Returns true if the codepoint is in a Symbols for Legacy
// Computing block, including supplements.
fn is_legacy_computing(char: u21) -> bool {
    match char {
        0x1FB00..=0x1FBFF => true,
        0x1CC00..=0x1CEBF => true, // Supplement introduced in Unicode 16.0
        _ => false,
    }
}

// Returns true if the codepoint is a part of the Powerline range.
fn is_powerline(char: u21) -> bool {
    matches!(char, 0xE0B0..=0xE0D7)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shaders::{Atlas, CellTextBools};
    use allocator_api2::alloc::Global;
    use rustty_vt::kitty;
    use rustty_vt::modes::{self as modespkg};
    use rustty_vt::render::RenderState;
    use rustty_vt::size;
    use rustty_vt::terminal::{Colors, Options, Terminal};

    fn opts(cols: size::CellCountInt, rows: size::CellCountInt) -> Options {
        Options {
            cols,
            rows,
            max_scrollback: 10_000,
            colors: Colors::default(),
            default_modes: modespkg::ModePacked::default(),
            kitty_image_storage_limit: 320 * 1000 * 1000,
            kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
        }
    }

    // PORT NOTE: the reference passes `state.row_data.get(0).cells.items(.raw)` —
    // the SoA projection of the `raw` field across the row's cells. The Rust
    // render port stores cells AoS (`Vec<render::Cell>`, each with a `.raw`), so
    // this rebuilds the `[]const page::Cell` slice from each cell's `.raw`.
    fn raw_cells(state: &RenderState, row: usize) -> Vec<page::Cell> {
        state.row_data[row].cells.iter().map(|c| c.raw).collect()
    }

    // test Contents
    #[test]
    fn contents() {
        let rows: usize = 10;
        let cols: usize = 10;

        let mut c: Contents = Contents::default();
        c.resize(GridSize {
            rows: rows as u16,
            columns: cols as u16,
        });

        // We should start off empty after resizing.
        for y in 0..rows {
            assert!(c.fg_rows.lists[y + 1].is_empty());
            for x in 0..cols {
                assert_eq!([0, 0, 0, 0], *c.bg_cell(y, x));
            }
        }
        // And the cursor row should have a capacity of 1 and also be empty.
        assert!(c.fg_rows.lists[0].capacity() == 1);
        assert!(c.fg_rows.lists[0].is_empty());

        // Add some contents.
        let bg_cell: CellBg = [0, 0, 0, 1];
        let fg_cell: CellText = CellText {
            atlas: Atlas::Grayscale,
            grid_pos: [4, 1],
            color: [0, 0, 0, 1],
            ..Default::default()
        };
        *c.bg_cell(1, 4) = bg_cell;
        c.add(Key::Text, fg_cell);
        assert_eq!(bg_cell, *c.bg_cell(1, 4));
        // The fg row index is offset by 1 because of the cursor list.
        assert_eq!(fg_cell, c.fg_rows.lists[2][0]);

        // And we should be able to clear it.
        c.clear(1);
        for y in 0..rows {
            assert!(c.fg_rows.lists[y + 1].is_empty());
            for x in 0..cols {
                assert_eq!([0, 0, 0, 0], *c.bg_cell(y, x));
            }
        }

        // Add a block cursor.
        let cursor_cell: CellText = CellText {
            atlas: Atlas::Grayscale,
            bools: CellTextBools {
                is_cursor_glyph: true,
                ..Default::default()
            },
            grid_pos: [2, 3],
            color: [0, 0, 0, 1],
            ..Default::default()
        };
        c.set_cursor(Some(cursor_cell), Some(CursorStyle::Block));
        assert_eq!(cursor_cell, c.fg_rows.lists[0][0]);
        assert_eq!(cursor_cell, c.get_cursor_glyph().unwrap());

        // And remove it.
        c.set_cursor(None, None);
        assert_eq!(0, c.fg_rows.lists[0].len());
        assert!(c.get_cursor_glyph().is_none());

        // Add a hollow cursor.
        c.set_cursor(Some(cursor_cell), Some(CursorStyle::BlockHollow));
        assert_eq!(cursor_cell, c.fg_rows.lists[rows + 1][0]);
        assert_eq!(cursor_cell, c.get_cursor_glyph().unwrap());

        c.deinit();
    }

    // test "Contents clear retains other content"
    #[test]
    fn contents_clear_retains_other_content() {
        let rows: usize = 10;
        let cols: usize = 10;

        let mut c: Contents = Contents::default();
        c.resize(GridSize {
            rows: rows as u16,
            columns: cols as u16,
        });

        // Set some contents
        // bg and fg cells in row 1
        let bg_cell_1: CellBg = [0, 0, 0, 1];
        let fg_cell_1: CellText = CellText {
            atlas: Atlas::Grayscale,
            grid_pos: [4, 1],
            color: [0, 0, 0, 1],
            ..Default::default()
        };
        *c.bg_cell(1, 4) = bg_cell_1;
        c.add(Key::Text, fg_cell_1);
        // bg and fg cells in row 2
        let bg_cell_2: CellBg = [0, 0, 0, 1];
        let fg_cell_2: CellText = CellText {
            atlas: Atlas::Grayscale,
            grid_pos: [4, 2],
            color: [0, 0, 0, 1],
            ..Default::default()
        };
        *c.bg_cell(2, 4) = bg_cell_2;
        c.add(Key::Text, fg_cell_2);

        // Clear row 1, this should leave row 2 untouched
        c.clear(1);

        // Row 2 should still contain its cells.
        assert_eq!(bg_cell_2, *c.bg_cell(2, 4));
        // Fg row index is +1 because of cursor list at start
        assert_eq!(fg_cell_2, c.fg_rows.lists[3][0]);

        c.deinit();
    }

    // test "Contents clear last added content"
    #[test]
    fn contents_clear_last_added_content() {
        let rows: usize = 10;
        let cols: usize = 10;

        let mut c: Contents = Contents::default();
        c.resize(GridSize {
            rows: rows as u16,
            columns: cols as u16,
        });

        // Set some contents
        // bg and fg cells in row 1
        let bg_cell_1: CellBg = [0, 0, 0, 1];
        let fg_cell_1: CellText = CellText {
            atlas: Atlas::Grayscale,
            grid_pos: [4, 1],
            color: [0, 0, 0, 1],
            ..Default::default()
        };
        *c.bg_cell(1, 4) = bg_cell_1;
        c.add(Key::Text, fg_cell_1);
        // bg and fg cells in row 2
        let bg_cell_2: CellBg = [0, 0, 0, 1];
        let fg_cell_2: CellText = CellText {
            atlas: Atlas::Grayscale,
            grid_pos: [4, 2],
            color: [0, 0, 0, 1],
            ..Default::default()
        };
        *c.bg_cell(2, 4) = bg_cell_2;
        c.add(Key::Text, fg_cell_2);

        // Clear row 2, this should leave row 1 untouched
        c.clear(2);

        // Row 1 should still contain its cells.
        assert_eq!(bg_cell_1, *c.bg_cell(1, 4));
        // Fg row index is +1 because of cursor list at start
        assert_eq!(fg_cell_1, c.fg_rows.lists[2][0]);

        c.deinit();
    }

    // test "Contents with zero-sized screen"
    #[test]
    fn contents_with_zero_sized_screen() {
        let mut c: Contents = Contents::default();

        c.set_cursor(None, None);
        assert!(c.get_cursor_glyph().is_none());

        c.deinit();
    }

    // test "Cell constraint widths"
    #[test]
    fn cell_constraint_widths() {
        let alloc = Global;

        let mut t: Terminal = Terminal::init(alloc, opts(4, 1)).unwrap();

        let mut state: RenderState = RenderState::empty();

        // for each case, the numbers in the comment denote expected
        // constraint widths for the symbol-containing cells

        // symbol->nothing: 2
        {
            t.full_reset();
            {
                let mut s = t.vt_stream();
                s.next_slice("\u{e8ef}".as_bytes());
                s.deinit();
            }
            state.update(alloc, &mut t).unwrap();
            assert_eq!(
                2,
                constraint_width(&raw_cells(&state, 0), 0, state.cols as usize)
            );
        }

        // symbol->character: 1
        {
            t.full_reset();
            {
                let mut s = t.vt_stream();
                s.next_slice("\u{e8ef}z".as_bytes());
                s.deinit();
            }
            state.update(alloc, &mut t).unwrap();
            assert_eq!(
                1,
                constraint_width(&raw_cells(&state, 0), 0, state.cols as usize)
            );
        }

        // symbol->space: 2
        {
            t.full_reset();
            {
                let mut s = t.vt_stream();
                s.next_slice("\u{e8ef} z".as_bytes());
                s.deinit();
            }
            state.update(alloc, &mut t).unwrap();
            assert_eq!(
                2,
                constraint_width(&raw_cells(&state, 0), 0, state.cols as usize)
            );
        }
        // symbol->no-break space: 1
        {
            t.full_reset();
            {
                let mut s = t.vt_stream();
                s.next_slice("\u{e8ef}\u{00a0}z".as_bytes());
                s.deinit();
            }
            state.update(alloc, &mut t).unwrap();
            assert_eq!(
                1,
                constraint_width(&raw_cells(&state, 0), 0, state.cols as usize)
            );
        }

        // symbol->end of row: 1
        {
            t.full_reset();
            {
                let mut s = t.vt_stream();
                s.next_slice("   \u{e8ef}".as_bytes());
                s.deinit();
            }
            state.update(alloc, &mut t).unwrap();
            assert_eq!(
                1,
                constraint_width(&raw_cells(&state, 0), 3, state.cols as usize)
            );
        }

        // character->symbol: 2
        {
            t.full_reset();
            {
                let mut s = t.vt_stream();
                s.next_slice("z\u{e8ef}".as_bytes());
                s.deinit();
            }
            state.update(alloc, &mut t).unwrap();
            assert_eq!(
                2,
                constraint_width(&raw_cells(&state, 0), 1, state.cols as usize)
            );
        }

        // symbol->symbol: 1,1
        {
            t.full_reset();
            {
                let mut s = t.vt_stream();
                s.next_slice("\u{e8ef}\u{e8ef}".as_bytes());
                s.deinit();
            }
            state.update(alloc, &mut t).unwrap();
            assert_eq!(
                1,
                constraint_width(&raw_cells(&state, 0), 0, state.cols as usize)
            );
            assert_eq!(
                1,
                constraint_width(&raw_cells(&state, 0), 1, state.cols as usize)
            );
        }

        // symbol->space->symbol: 2,2
        {
            t.full_reset();
            {
                let mut s = t.vt_stream();
                s.next_slice("\u{e8ef} \u{e8ef}".as_bytes());
                s.deinit();
            }
            state.update(alloc, &mut t).unwrap();
            assert_eq!(
                2,
                constraint_width(&raw_cells(&state, 0), 0, state.cols as usize)
            );
            assert_eq!(
                2,
                constraint_width(&raw_cells(&state, 0), 2, state.cols as usize)
            );
        }

        // symbol->powerline: 1  (dedicated test because powerline is special-cased in cellpkg)
        {
            t.full_reset();
            {
                let mut s = t.vt_stream();
                s.next_slice("\u{e8ef}\u{e0b0}".as_bytes());
                s.deinit();
            }
            state.update(alloc, &mut t).unwrap();
            assert_eq!(
                1,
                constraint_width(&raw_cells(&state, 0), 0, state.cols as usize)
            );
        }

        // powerline->symbol: 2  (dedicated test because powerline is special-cased in cellpkg)
        {
            t.full_reset();
            {
                let mut s = t.vt_stream();
                s.next_slice("\u{e0b2}\u{e8ef}".as_bytes());
                s.deinit();
            }
            state.update(alloc, &mut t).unwrap();
            assert_eq!(
                2,
                constraint_width(&raw_cells(&state, 0), 1, state.cols as usize)
            );
        }

        // powerline->nothing: 2  (dedicated test because powerline is special-cased in cellpkg)
        {
            t.full_reset();
            {
                let mut s = t.vt_stream();
                s.next_slice("\u{e0b2}".as_bytes());
                s.deinit();
            }
            state.update(alloc, &mut t).unwrap();
            assert_eq!(
                2,
                constraint_width(&raw_cells(&state, 0), 0, state.cols as usize)
            );
        }

        // powerline->space: 2  (dedicated test because powerline is special-cased in cellpkg)
        {
            t.full_reset();
            {
                let mut s = t.vt_stream();
                s.next_slice("\u{e0b2} z".as_bytes());
                s.deinit();
            }
            state.update(alloc, &mut t).unwrap();
            assert_eq!(
                2,
                constraint_width(&raw_cells(&state, 0), 0, state.cols as usize)
            );
        }

        state.deinit(alloc);
        t.deinit(alloc);
    }
}
