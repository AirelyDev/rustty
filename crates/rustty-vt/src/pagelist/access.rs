//! PageList access + introspection cluster.
//!
//! PORTS: ghostty src/terminal/PageList.zig — the `getCell`, `getTopLeft`,
//! `getBottomRight`, `totalRows`, `totalPages`, `clearDirty`, `isDirty`,
//! `markDirty`, `Cell` struct (`isDirty`, `style`, `screenPoint`, ...), the
//! `Clone` struct (+ `TrackedPinsRemap`) and `clone`, `split`, `diagram`, and
//! `highlightSemanticContent` cluster, plus the `Scrollbar`/`Cell` `cval`/`eql`
//! helpers that belong to introspection.
//!
//! PORT STATUS: methods ported (vt-3c). This file owns `getCell`,
//! `getBottomRight`, `clone` (+ the `Clone` options struct and its
//! `TrackedPinsRemap`), `diagram`, `highlightSemanticContent`, `clearDirty`,
//! `isDirty`, and the introspection `Cell` struct (`isDirty`/`screenPoint`/
//! `style`). The
//! four PageList-level iterator wrappers (`pageIterator`/`rowIterator`/
//! `cellIterator`/`promptIterator`) land here too: they were deferred from the
//! iter cluster because they depend on `getBottomRight` (see the DEFERRED SEAM
//! note at the bottom of `iter.rs`). `getTopLeft`/`totalRows`/`totalPages`/
//! `growRows`/`markDirty` and the `Scrollbar` `cval`/`eql` already landed in
//! `mod.rs`/`grow.rs`. The 205-block test split lands in the dedicated test
//! phase.

use super::iter::{CellIterator, Direction, Limit, PageIterator, PromptIterator, RowIterator};
use super::OutOfMemory;
use super::{List, MemoryPool, Node, PageList, Pin, PinSet};
use crate::highlight;
use crate::page as pagepkg;
use crate::page::{CloneFromError, Page, Row};
use crate::point;
use crate::style as stylepkg;
use rustty_render_types::term_size as size;

impl PageList {
    /// Get the cell at the given point, or null if the cell does not
    /// exist or is out of bounds.
    ///
    /// Warning: this is slow and should not be used in performance critical paths
    pub fn get_cell(&self, pt: point::Point) -> Option<Cell> {
        let pt_pin = self.pin(pt)?;
        let rac = unsafe {
            (*super::Node::page(pt_pin.node)).get_row_and_cell(pt_pin.x as usize, pt_pin.y as usize)
        };
        Some(Cell {
            node: pt_pin.node,
            row: rac.row,
            cell: rac.cell,
            row_idx: pt_pin.y,
            col_idx: pt_pin.x,
        })
    }

    /// Log a debug diagram of the page list to the provided writer.
    ///
    /// EXAMPLE:
    ///
    // PORT NOTE: the verbatim reference ASCII diagram is wrapped in a ```text```
    // fence (crate convention; see rustty-zigstd/arena_allocator.rs) so rustdoc
    // treats it as non-compiled text rather than a Rust doctest. The diagram
    // content is preserved byte-for-byte.
    /// ```text
    ///      +-----+ = PAGE 0
    ///  ... |     |
    ///   50 | foo |
    ///  ... |     |
    ///     +--------+ ACTIVE
    ///  124 |     | | 0
    ///  125 |Text | | 1
    ///      :  ^  : : = PIN 0
    ///  126 |Wrap…  | 2
    ///      +-----+ :
    ///      +-----+ : = PAGE 1
    ///    0 …ed   | | 3
    ///    1 | etc.| | 4
    ///      +-----+ :
    ///     +--------+
    /// ```
    //
    // PORT NOTE: the reference takes a `*std.Io.Writer` and returns
    // `std.Io.Writer.Error!void`. We port it to any `core::fmt::Write` sink (the
    // crate convention; see `size_report.rs`), and the inferred error set
    // `std.Io.Writer.Error || error{TooManyTrackedPinsInRow}` becomes
    // `DiagramError` below. `writeByteNTimes(b, n)` becomes a helper loop and
    // `writer.print("{u}", .{cp})` writes the Unicode scalar via `char`.
    pub fn diagram<W: core::fmt::Write>(&self, writer: &mut W) -> Result<(), DiagramError> {
        let active_pin = self.get_top_left(point::Tag::Active);

        let mut active = false;
        let mut active_index: usize = 0;

        let mut page_index: usize = 0;
        let mut cols: usize = 0;

        let mut it = self.page_iterator(
            Direction::RightDown,
            point::Point::Screen(point::Coordinate { x: 0, y: 0 }),
            None,
        );
        while let Some(chunk) = it.next() {
            cols = unsafe { super::Node::cols(chunk.node) } as usize;

            // Whether we've just skipped some number of rows and drawn
            // an ellipsis row (this is reset when a row is not skipped).
            let mut skipped = false;

            for y in 0..(unsafe { super::Node::rows(chunk.node) } as usize) {
                // Active header
                if !active
                    && core::ptr::eq(chunk.node, active_pin.node)
                    && active_pin.y as usize == y
                {
                    active = true;
                    writer.write_str("     +-")?;
                    write_byte_n_times(writer, b'-', cols)?;
                    writer.write_str("--+ ACTIVE")?;
                    writer.write_char('\n')?;
                }

                // Page header
                if y == 0 {
                    writer.write_str("      +")?;
                    write_byte_n_times(writer, b'-', cols)?;
                    writer.write_char('+')?;
                    if active {
                        writer.write_str(" :")?;
                    }
                    write!(writer, " = PAGE {}", page_index)?;
                    writer.write_char('\n')?;
                }

                // Row contents
                {
                    let row = unsafe { (*super::Node::page(chunk.node)).get_row(y) };
                    let cells_ptr = unsafe { (*super::Node::page(chunk.node)).get_cells(row) };
                    let cells: &[pagepkg::Cell] = unsafe {
                        core::slice::from_raw_parts(cells_ptr as *const pagepkg::Cell, cols)
                    };

                    let mut row_has_content = false;

                    for cell in cells {
                        if cell.has_text() {
                            row_has_content = true;
                            break;
                        }
                    }

                    // We don't want to print this row's contents
                    // unless it has text or is in the active area.
                    if !active && !row_has_content {
                        // If we haven't, draw an ellipsis row.
                        if !skipped {
                            writer.write_str("  ... :")?;
                            write_byte_n_times(writer, b' ', cols)?;
                            writer.write_char(':')?;
                            if active {
                                writer.write_str(" :")?;
                            }
                            writer.write_char('\n')?;
                        }
                        skipped = true;
                        continue;
                    }

                    skipped = false;

                    // Left pad row number to 5 wide
                    let y_digits = if y == 0 {
                        0
                    } else {
                        (y as u64).ilog10() as usize
                    };
                    write_byte_n_times(writer, b' ', 4 - y_digits)?;
                    write!(writer, "{} ", y)?;

                    // Left edge or wrap continuation marker
                    writer.write_str(if unsafe { (*row).wrap_continuation() } {
                        "…"
                    } else {
                        "|"
                    })?;

                    // Row text
                    if row_has_content {
                        for cell in cells {
                            // Skip spacer tails, since wide cells are, well, wide.
                            if cell.wide() == pagepkg::Wide::SpacerTail {
                                continue;
                            }

                            // Write non-printing bytes as base36, for convenience.
                            if cell.codepoint() < ' ' as u32 {
                                writer.write_char(
                                    "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ".as_bytes()
                                        [cell.codepoint() as usize]
                                        as char,
                                )?;
                                continue;
                            }
                            write!(writer, "{}", char::from_u32(cell.codepoint()).unwrap())?;
                            if cell.has_grapheme() {
                                let grapheme = unsafe {
                                    (*super::Node::page(chunk.node)).lookup_grapheme(cell)
                                }
                                .unwrap();
                                for cp in grapheme {
                                    write!(writer, "{}", char::from_u32(*cp).unwrap())?;
                                }
                            }
                        }
                    } else {
                        write_byte_n_times(writer, b' ', cols)?;
                    }

                    // Right edge or wrap marker
                    writer.write_str(if unsafe { (*row).wrap() } { "…" } else { "|" })?;
                    if active {
                        write!(writer, " | {}", active_index)?;
                        active_index += 1;
                    }

                    writer.write_char('\n')?;
                }

                // Tracked pin marker(s)
                'pins: {
                    // If we have more than 16 tracked pins in a row, oh well,
                    // don't wanna bother making this function allocating.
                    let mut pin_buf: [*mut Pin; 16] = [core::ptr::null_mut(); 16];
                    let mut pin_count: usize = 0;
                    let pin_keys = self.tracked_pins.keys();
                    for p in pin_keys {
                        let p = *p;
                        if !core::ptr::eq(unsafe { (*p).node }, chunk.node) {
                            continue;
                        }
                        if unsafe { (*p).y } as usize != y {
                            continue;
                        }
                        pin_buf[pin_count] = p;
                        pin_count += 1;
                        if pin_count >= pin_buf.len() {
                            return Err(DiagramError::TooManyTrackedPinsInRow);
                        }
                    }

                    if pin_count == 0 {
                        break 'pins;
                    }

                    let pins = &mut pin_buf[0..pin_count];
                    pins.sort_by(|a, b| {
                        // fn lt(_: void, a: *Pin, b: *Pin) bool {
                        //     return a.x < b.x;
                        // }
                        unsafe { (*(*a)).x }.cmp(&unsafe { (*(*b)).x })
                    });

                    writer.write_str("      :")?;
                    let mut x: usize = 0;

                    for p in pins.iter() {
                        let px = unsafe { (*(*p)).x } as usize;
                        if x > px {
                            continue;
                        }
                        write_byte_n_times(writer, b' ', px - x)?;
                        writer.write_char('^')?;
                        x = px + 1;
                    }

                    write_byte_n_times(writer, b' ', cols - x)?;
                    writer.write_char(':')?;

                    if active {
                        writer.write_str(" :")?;
                    }

                    write!(writer, " = PIN{}", if pin_count > 1 { "S" } else { "" })?;

                    x = unsafe { (*pins[0]).x } as usize;
                    for (i, p) in pins.iter().enumerate() {
                        if unsafe { (*(*p)).x } as usize != x {
                            writer.write_char(',')?;
                        }
                        write!(writer, " {}", i)?;
                    }

                    writer.write_char('\n')?;
                }
            }

            // Page footer
            {
                writer.write_str("      +")?;
                write_byte_n_times(writer, b'-', cols)?;
                writer.write_char('+')?;
                if active {
                    writer.write_str(" :")?;
                }
                writer.write_char('\n')?;
            }

            page_index += 1;
        }

        // Active footer
        {
            writer.write_str("     +-")?;
            write_byte_n_times(writer, b'-', cols)?;
            writer.write_str("--+")?;
            writer.write_char('\n')?;
        }

        Ok(())
    }

    /// Returns the boundaries of the given semantic content type for
    /// the prompt at the given pin. The pin row MUST be the first row
    /// of a prompt, otherwise the results may be nonsense.
    ///
    /// To get prompt pins, use promptIterator. Warning that if there are
    /// no semantic prompts ever present, promptIterator will iterate the
    /// entire PageList. Downstream callers should keep track of a flag if
    /// they've ever seen semantic prompt operations to prevent this performance
    /// case.
    ///
    /// Note that some semantic content type such as "input" is usually
    /// nested within prompt boundaries, so the returned boundaries may include
    /// prompt text.
    pub fn highlight_semantic_content(
        &self,
        at: Pin,
        content: pagepkg::SemanticContent,
    ) -> Option<highlight::Untracked> {
        // Performance note: we can do this more efficiently in a single
        // forward-pass. Semantic content operations aren't usually fast path
        // but if someone wants to optimize them someday that's great.

        let end: Pin = {
            // Safety assertion, our starting point should be a prompt row.
            // so the first returned prompt should be ourselves.
            let mut it = at.prompt_iterator(Direction::RightDown, None);
            assert!(it.next().unwrap().y == at.y);

            // Our end is the end of the line just before the next prompt
            // line, which should exist since we verified we have at least
            // two prompts here.
            'end: {
                if let Some(next) = it.next() {
                    'next: {
                        let mut prev = match next.up(1) {
                            Some(p) => p,
                            None => break 'next,
                        };
                        prev.x = unsafe { super::Node::cols(prev.node) } - 1;
                        break 'end prev;
                    }
                }

                // Didn't find any further prompt so the end of our zone is
                // the end of the screen.
                self.get_bottom_right(point::Tag::Screen).unwrap()
            }
        };

        match content {
            // For the prompt, we select all the way up to command output.
            // We include all the input lines, too.
            pagepkg::SemanticContent::Prompt => {
                let mut result: highlight::Untracked = highlight::Untracked {
                    start: at.left(at.x as usize),
                    end: at.pin_copy(),
                };

                let mut it = at.cell_iterator(Direction::RightDown, Some(end));
                while let Some(p) = it.next() {
                    match unsafe { (*p.row_and_cell().cell).semantic_content() } {
                        pagepkg::SemanticContent::Prompt | pagepkg::SemanticContent::Input => {
                            result.end = p
                        }
                        pagepkg::SemanticContent::Output => break,
                    }
                }

                Some(result)
            }

            // For input, we include the start of the input to the end of
            // the input, which may include all the prompts in the middle, too.
            pagepkg::SemanticContent::Input => {
                let mut result: highlight::Untracked = highlight::Untracked {
                    start: Pin {
                        node: core::ptr::null_mut(),
                        y: 0,
                        x: 0,
                        garbage: false,
                    },
                    end: Pin {
                        node: core::ptr::null_mut(),
                        y: 0,
                        x: 0,
                        garbage: false,
                    },
                };

                // Find the start
                let mut it = at.cell_iterator(Direction::RightDown, Some(end));
                let mut found = false;
                while let Some(p) = it.next() {
                    match unsafe { (*p.row_and_cell().cell).semantic_content() } {
                        pagepkg::SemanticContent::Prompt => {}
                        pagepkg::SemanticContent::Input => {
                            result.start = p.pin_copy();
                            result.end = p;
                            found = true;
                            break;
                        }
                        pagepkg::SemanticContent::Output => return None,
                    }
                }
                if !found {
                    // No input found
                    return None;
                }

                // Find the end
                while let Some(p) = it.next() {
                    match unsafe { (*p.row_and_cell().cell).semantic_content() } {
                        // Prompts can be nested in our input for continuation
                        pagepkg::SemanticContent::Prompt => {}

                        // Output means we're done
                        pagepkg::SemanticContent::Output => break,

                        pagepkg::SemanticContent::Input => result.end = p,
                    }
                }

                Some(result)
            }

            pagepkg::SemanticContent::Output => {
                let mut result: highlight::Untracked = highlight::Untracked {
                    start: Pin {
                        node: core::ptr::null_mut(),
                        y: 0,
                        x: 0,
                        garbage: false,
                    },
                    end: Pin {
                        node: core::ptr::null_mut(),
                        y: 0,
                        x: 0,
                        garbage: false,
                    },
                };

                // Find the start
                let mut it = at.cell_iterator(Direction::RightDown, Some(end));
                let mut found = false;
                while let Some(p) = it.next() {
                    let cell = unsafe { *p.row_and_cell().cell };
                    match cell.semantic_content() {
                        pagepkg::SemanticContent::Prompt | pagepkg::SemanticContent::Input => {}
                        pagepkg::SemanticContent::Output => {
                            // Skip empty cells - they default to .output but aren't real output
                            if !cell.has_text() {
                                continue;
                            }
                            result.start = p.pin_copy();
                            result.end = p;
                            found = true;
                            break;
                        }
                    }
                }
                if !found {
                    // No output found
                    return None;
                }

                // Find the end
                while let Some(p) = it.next() {
                    let cell = unsafe { *p.row_and_cell().cell };
                    match cell.semantic_content() {
                        pagepkg::SemanticContent::Prompt | pagepkg::SemanticContent::Input => break,
                        pagepkg::SemanticContent::Output => {
                            // Only extend to cells with actual text
                            if cell.has_text() {
                                result.end = p;
                            }
                        }
                    }
                }

                Some(result)
            }
        }
    }

    /// Returns the bottom right of the screen for the given tag. This can
    /// return null because it is possible that a tag is not in the screen
    /// (e.g. history does not yet exist).
    pub fn get_bottom_right(&self, tag: point::Tag) -> Option<Pin> {
        match tag {
            point::Tag::Screen | point::Tag::Active => {
                let node = self.pages.last;
                Some(Pin {
                    node,
                    y: unsafe { super::Node::rows(node) } - 1,
                    x: unsafe { super::Node::cols(node) } - 1,
                    garbage: false,
                })
            }

            point::Tag::Viewport => {
                let mut br = self.get_top_left(point::Tag::Viewport);
                br = br.down(self.rows as usize - 1).unwrap();
                br.x = unsafe { super::Node::cols(br.node) } - 1;
                Some(br)
            }

            point::Tag::History => {
                let mut br = self.get_top_left(point::Tag::Active);
                br = br.up(1)?;
                br.x = unsafe { super::Node::cols(br.node) } - 1;
                Some(br)
            }
        }
    }

    /// Clear all dirty bits on all pages. This is not efficient since it
    /// traverses the entire list of pages. This is used for testing/debugging.
    pub fn clear_dirty(&mut self) {
        let mut page = self.pages.first;
        while !page.is_null() {
            let p = page;
            unsafe {
                (*super::Node::page(p)).dirty = false;
                let base = (*super::Node::page(p)).memory_ptr();
                let rows_ptr = (*super::Node::page(p)).rows.ptr(base);
                let rows = super::Node::rows(p) as usize;
                for i in 0..rows {
                    (*rows_ptr.add(i)).set_dirty(false);
                }
                page = (*p).next;
            }
        }
    }

    /// Returns true if the point is dirty, used for testing.
    pub fn is_dirty(&self, pt: point::Point) -> bool {
        self.get_cell(pt).unwrap().is_dirty()
    }
}

// PORT NOTE (DEFERRED-SEAM RESOLUTION): the four PageList-level iterator wrapper
// functions (`pageIterator`/`rowIterator`/`cellIterator`/`promptIterator` on
// `*const PageList`) were deferred from the iter cluster (`iter.rs`) because
// their bodies are driven by `getBottomRight`, which the access cluster owns.
// `getBottomRight` now exists above, so they land here, fully ported. They
// delegate to the per-`Pin` inline iterators in `iter.rs`.
impl PageList {
    pub fn prompt_iterator(
        &self,
        direction: Direction,
        tl_pt: point::Point,
        bl_pt: Option<point::Point>,
    ) -> PromptIterator {
        let tl_pin = self.pin(tl_pt).unwrap();
        let bl_pin = if let Some(pt) = bl_pt {
            self.pin(pt).unwrap()
        } else {
            match self.get_bottom_right(tl_pt.tag()) {
                Some(p) => p,
                None => return PromptIterator::EMPTY,
            }
        };

        match direction {
            Direction::RightDown => tl_pin.prompt_iterator(Direction::RightDown, Some(bl_pin)),
            Direction::LeftUp => bl_pin.prompt_iterator(Direction::LeftUp, Some(tl_pin)),
        }
    }

    pub fn cell_iterator(
        &self,
        direction: Direction,
        tl_pt: point::Point,
        bl_pt: Option<point::Point>,
    ) -> CellIterator {
        let tl_pin = self.pin(tl_pt).unwrap();
        let bl_pin = if let Some(pt) = bl_pt {
            self.pin(pt).unwrap()
        } else {
            match self.get_bottom_right(tl_pt.tag()) {
                Some(p) => p,
                None => {
                    // PORT NOTE: reference returns `.{ .row_it = undefined }`. The
                    // inner `row_it` is never read because `cell == null`, so we
                    // build an empty `CellIterator` (empty row_it + `cell: None`)
                    // whose `next()` immediately yields `None`.
                    return CellIterator {
                        row_it: empty_row_iterator(),
                        cell: None,
                    };
                }
            }
        };

        match direction {
            Direction::RightDown => tl_pin.cell_iterator(Direction::RightDown, Some(bl_pin)),
            Direction::LeftUp => bl_pin.cell_iterator(Direction::LeftUp, Some(tl_pin)),
        }
    }

    /// Create an iterator that can be used to iterate all the rows in
    /// a region of the screen from the given top-left. The tag of the
    /// top-left point will also determine the end of the iteration,
    /// so convert from one reference point to another to change the
    /// iteration bounds.
    pub fn row_iterator(
        &self,
        direction: Direction,
        tl_pt: point::Point,
        bl_pt: Option<point::Point>,
    ) -> RowIterator {
        let tl_pin = self.pin(tl_pt).unwrap();
        let bl_pin = if let Some(pt) = bl_pt {
            self.pin(pt).unwrap()
        } else {
            match self.get_bottom_right(tl_pt.tag()) {
                Some(p) => p,
                None => {
                    // PORT NOTE: reference returns `.{ .page_it = undefined }`. The
                    // inner `page_it` is never read because `chunk == null`, so we
                    // build an empty `RowIterator` whose `next()` yields `None`.
                    return empty_row_iterator();
                }
            }
        };

        match direction {
            Direction::RightDown => tl_pin.row_iterator(Direction::RightDown, Some(bl_pin)),
            Direction::LeftUp => bl_pin.row_iterator(Direction::LeftUp, Some(tl_pin)),
        }
    }

    /// Return an iterator that iterates through the rows in the tagged area
    /// of the point. The iterator returns row "chunks", which are the largest
    /// contiguous set of rows in a single backing page for a given portion of
    /// the point region.
    ///
    /// This is a more efficient way to iterate through the data in a region,
    /// since you can do simple pointer math and so on.
    ///
    /// If bl_pt is non-null, iteration will stop at the bottom left point
    /// (inclusive). If bl_pt is null, the entire region specified by the point
    /// tag will be iterated over. tl_pt and bl_pt must be the same tag, and
    /// bl_pt must be greater than or equal to tl_pt.
    ///
    /// If direction is left_up, iteration will go from bl_pt to tl_pt. If
    /// direction is right_down, iteration will go from tl_pt to bl_pt.
    /// Both inclusive.
    pub fn page_iterator(
        &self,
        direction: Direction,
        tl_pt: point::Point,
        bl_pt: Option<point::Point>,
    ) -> PageIterator {
        let tl_pin = self.pin(tl_pt).unwrap();
        let bl_pin = if let Some(pt) = bl_pt {
            self.pin(pt).unwrap()
        } else {
            match self.get_bottom_right(tl_pt.tag()) {
                Some(p) => p,
                None => {
                    // PORT NOTE: reference returns `.{ .row = null }`.
                    return PageIterator {
                        row: None,
                        limit: Limit::None,
                        direction,
                    };
                }
            }
        };

        // PORT NOTE: the reference guards this assert behind
        // `build_options.slow_runtime_safety`; the crate maps that to
        // `cfg!(debug_assertions)` (see `pin`/`track_pin`).
        if cfg!(debug_assertions) {
            assert!(tl_pin.eql(bl_pin.pin_copy()) || tl_pin.before(bl_pin.pin_copy()));
        }

        match direction {
            Direction::RightDown => tl_pin.page_iterator(Direction::RightDown, Some(bl_pin)),
            Direction::LeftUp => bl_pin.page_iterator(Direction::LeftUp, Some(tl_pin)),
        }
    }
}

/// `PageList.Clone` options.
pub struct Clone {
    /// The top and bottom (inclusive) points of the region to clone.
    /// The x coordinate is ignored; the full row is always cloned.
    pub top: point::Point,
    pub bot: Option<point::Point>,

    // If this is non-null then cloning will attempt to remap the tracked
    // pins into the new cloned area and will keep track of the old to
    // new mapping in this map. If this is null, the cloned pagelist will
    // not retain any previously tracked pins except those required for
    // internal operations.
    //
    // Any pins not present in the map were not remapped.
    pub tracked_pins: Option<TrackedPinsRemap>,
}

// PORT NOTE: the reference `TrackedPinsRemap = std.AutoHashMap(*Pin, *Pin)`.
// Ported to the std `HashMap` keyed by the raw `*mut Pin` pointer identity.
// PERF(port): Zig's `AutoHashMap` hashes a pointer key with a cheap integer
// autoHash and no per-map random seed; the std default `RandomState`
// (SipHash-1-3) is a heavyweight keyed hash. Restore the reference's cheap
// idiom with a small multiply/xor mix over the pointer bits ([`PtrHasher`]).
pub type TrackedPinsRemap =
    std::collections::HashMap<*mut Pin, *mut Pin, std::hash::BuildHasherDefault<PtrHasher>>;

/// Pointer-identity hasher for [`TrackedPinsRemap`].
// PERF(port): see the `TrackedPinsRemap` note above. `*mut Pin` keys hash
// through `Hash for *mut T`, which feeds the thin-pointer address via
// `write_usize`; a single splitmix-style multiply + xor-shift spreads the
// (aligned, heap-clustered) address bits across the table.
#[derive(Default)]
pub struct PtrHasher(u64);

impl std::hash::Hasher for PtrHasher {
    #[inline]
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        // Pointer keys enter through `write_usize`; keep the hasher total
        // for any other input shape.
        for &b in bytes {
            self.write_u64(u64::from(b));
        }
    }

    #[inline]
    fn write_u64(&mut self, i: u64) {
        let mut x = self.0 ^ i;
        x = x.wrapping_mul(0x9E37_79B9_7F4A_7C15); // 2^64 / phi (splitmix64 increment)
        x ^= x >> 32;
        self.0 = x;
    }

    #[inline]
    fn write_usize(&mut self, i: usize) {
        self.write_u64(i as u64);
    }
}

/// `PageList.CloneError`.
//
// PORT NOTE: the reference `clone` returns the inferred error set
// `Allocator.Error || Page.CloneFromError` (pool init / pin create / grow are
// `Allocator.Error`; `cloneFrom` is `CloneFromError`). We model this union
// explicitly: `OutOfMemory` for the allocator path and `CloneFrom` wrapping the
// already-ported `page::CloneFromError`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloneError {
    OutOfMemory,
    CloneFrom(CloneFromError),
}

impl From<OutOfMemory> for CloneError {
    fn from(_: OutOfMemory) -> Self {
        CloneError::OutOfMemory
    }
}

impl From<CloneFromError> for CloneError {
    fn from(e: CloneFromError) -> Self {
        CloneError::CloneFrom(e)
    }
}

impl PageList {
    /// Clone this pagelist from the top to bottom (inclusive).
    ///
    /// The viewport is always moved to the active area.
    ///
    /// The cloned pagelist must contain at least enough rows for the active
    /// area. If the region specified has less rows than the active area then
    /// rows will be added to the bottom of the region to make up the difference.
    //
    // PORT NOTE: the reference `clone(self, alloc, opts)` takes an `Allocator`
    // for the node/pin pools; per the crate convention all pools are backed by
    // the Rust system allocator, so no allocator parameter is threaded through
    // (see `init`). `opts.tracked_pins` is taken by value (the remap map is
    // populated and ownership passes back via the returned `Clone`-less map);
    // because the reference threads a `*TrackedPinsRemap` we accept `&mut`.
    pub fn clone(&self, opts: &mut Clone) -> Result<PageList, CloneError> {
        let mut it = self.page_iterator(Direction::RightDown, opts.top, opts.bot);

        // First, count our pages so our preheat is exactly what we need.
        //
        // PORT NOTE: the reference does `var it_copy = it;` (a by-value copy of
        // the iterator). `PageIterator` is not `Copy` in the Rust port (its
        // `Limit` carries a `Pin`, which is intentionally not `Copy`), so we
        // construct an independent counting iterator with identical arguments;
        // the iterator is a pure function of the (unmodified) PageList state, so
        // this is behaviorally identical to copying.
        let mut it_copy = self.page_iterator(Direction::RightDown, opts.top, opts.bot);
        let page_count: usize = {
            let mut count: usize = 0;
            while it_copy.next().is_some() {
                count += 1;
            }
            count
        };

        // Setup our pool
        let mut pool: MemoryPool = MemoryPool::init(page_count)?;
        // errdefer pool.deinit();

        // Create our viewport. In a clone, the viewport always goes
        // to the top.
        let viewport_pin = pool.pins.create().map_err(|_| OutOfMemory)?.as_ptr();
        let mut tracked_pins: PinSet = PinSet::new();
        // errdefer tracked_pins.deinit(pool.alloc);
        tracked_pins.put_no_clobber(viewport_pin, ());

        // Our list of pages
        let mut page_list: List = List::default();
        // errdefer {
        //     const page_alloc = pool.pages.arena.child_allocator;
        //     var page_it = page_list.first;
        //     while (page_it) |node| : (page_it = node.next) {
        //         if (node.data.memory.len > std_size) {
        //             page_alloc.free(node.data.memory);
        //         }
        //     }
        // }
        //
        // PORT NOTE: the reference errdefer frees only the NON-STANDARD page
        // buffers (those whose backing `memory.len > std_size`) because the
        // standard-size buffers are owned by `pool.pages` and freed when the
        // pool is dropped. `free_non_standard_pages` (mod.rs) performs exactly
        // that walk over `page_list`. Because Rust cannot express a Zig
        // `errdefer` that captures `&mut page_list` while the loop also mutates
        // it, we reproduce the errdefer EXPLICITLY: every early-error exit in
        // the page-copy loop frees the non-standard pages first, then `pool`
        // drops on return to release the rest. The `clone_err!` helper macro
        // performs that free + return so the cleanup cannot be forgotten.
        macro_rules! clone_err {
            ($e:expr) => {{
                Self::free_non_standard_pages(&mut page_list);
                return Err($e);
            }};
        }

        // Copy our pages
        let mut page_serial: u64 = 0;
        let mut total_rows: usize = 0;
        let mut page_size: usize = 0;
        while let Some(chunk) = it.next() {
            // Clone the page. We have to use createPageExt here because
            // we don't know if the source page has a standard size.
            let node = match Self::create_page_ext(
                &mut pool,
                super::CreatePage::new(unsafe { super::Node::capacity(chunk.node) }),
                &mut page_serial,
                Some(&mut page_size),
            ) {
                Ok(n) => n,
                Err(e) => clone_err!(e.into()),
            };
            assert!(
                unsafe { super::Node::capacity(node).rows } as usize
                    >= chunk.end as usize - chunk.start as usize
            );
            // defer node.data.assertIntegrity();
            unsafe {
                (*super::Node::page(node)).size.rows = chunk.end - chunk.start;
                (*super::Node::page(node)).size.cols = super::Node::cols(chunk.node);
            }
            let clone_res = unsafe {
                (*super::Node::page(node)).clone_from(
                    &(*super::Node::page(chunk.node)) as *const Page,
                    chunk.start as usize,
                    chunk.end as usize,
                )
            };
            if let Err(e) = clone_res {
                unsafe {
                    (*super::Node::page(node)).assert_integrity();
                }
                clone_err!(e.into());
            }
            unsafe {
                (*super::Node::page(node)).assert_integrity();
            }

            unsafe {
                (*super::Node::page(node)).dirty = (*super::Node::page(chunk.node)).dirty;
            }

            unsafe { page_list.append(node) };

            total_rows += unsafe { super::Node::rows(node) } as usize;

            // Remap our tracked pins by changing the page and
            // offsetting the Y position based on the chunk start.
            if let Some(remap) = opts.tracked_pins.as_mut() {
                // PERF(port): the reference `const pin_keys =
                // self.tracked_pins.keys();` (PageList.zig:866) is a free borrow
                // of the keys array; the port collected an owned Vec per chunk.
                // `self` is `&self` here and the loop mutates only the local
                // pool/remap/tracked_pins, so iterate the keys directly like the
                // sibling sites in grow.rs/erase.rs.
                let pin_keys = self.tracked_pins.keys();
                for p in pin_keys {
                    let p: *mut Pin = *p;
                    // We're only interested in pins that were within the chunk.
                    if !core::ptr::eq(unsafe { (*p).node }, chunk.node)
                        || unsafe { (*p).y } < chunk.start
                        || unsafe { (*p).y } >= chunk.end
                    {
                        continue;
                    }
                    let new_p = match pool.pins.create() {
                        Ok(v) => v.as_ptr(),
                        Err(_) => clone_err!(CloneError::OutOfMemory),
                    };
                    unsafe {
                        new_p.write((*p).pin_copy());
                        (*new_p).node = node;
                        (*new_p).y -= chunk.start;
                    }
                    // PORT NOTE: `AutoHashMap.putNoClobber` is `Allocator.Error!void`
                    // in the reference; the Rust `HashMap`/`PinSet` insertions own
                    // their backing storage and are infallible. We assert no clobber
                    // to preserve the `putNoClobber` contract.
                    assert!(remap.insert(p, new_p).is_none());
                    tracked_pins.put_no_clobber(new_p, ());
                }
            }
        }

        // Initialize our viewport pin to point to the first cloned page
        // so it points to valid memory.
        unsafe {
            viewport_pin.write(Pin {
                node: page_list.first,
                y: 0,
                x: 0,
                garbage: false,
            });
        }

        let mut result: PageList = PageList {
            pool,
            pages: page_list,
            page_serial,
            page_serial_epoch: 0,
            page_compression: super::compress::IncrementalCompressionState::default(),
            page_size,
            limits: self.limits,
            cols: self.cols,
            rows: self.rows,
            total_rows,
            tracked_pins,
            viewport: super::Viewport::Active,
            viewport_pin,
            viewport_pin_row_offset: None,
            pause_integrity_checks: 0,
        };

        // We always need to have enough rows for our viewport because this is
        // a pagelist invariant that other code relies on.
        if total_rows < self.rows as usize {
            let len = self.rows as usize - total_rows;
            for _ in 0..len {
                let _ = result.grow()?;

                // Clear the row. This is not very fast but in reality right
                // now we rarely clone less than the active area and if we do
                // the area is by definition very small.
                let last = result.pages.last;
                unsafe {
                    let base = (*super::Node::page(last)).memory_ptr();
                    let rows_ptr = (*super::Node::page(last)).rows.ptr(base);
                    let row = rows_ptr.add(super::Node::rows(last) as usize - 1);
                    let cols = result.cols as usize;
                    (*super::Node::page(last)).clear_cells(row, 0, cols);
                }
            }

            // Update our total rows to be our row size.
            result.total_rows = result.rows as usize;
        }

        // A clone can copy more history than its inherited line limit.
        result.enforce_limit(super::Key::Lines);

        result.assert_integrity();
        Ok(result)
    }
}

/// `PageList.diagram` error set.
//
// PORT NOTE: the reference inferred error set is
// `std.Io.Writer.Error || error{TooManyTrackedPinsInRow}`. The writer error maps
// to `core::fmt::Error`; the explicit error case is preserved as a variant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagramError {
    Write,
    TooManyTrackedPinsInRow,
}

impl From<core::fmt::Error> for DiagramError {
    fn from(_: core::fmt::Error) -> Self {
        DiagramError::Write
    }
}

// PORT NOTE: an empty `RowIterator` whose `next()` immediately yields `None`.
// Mirrors the reference's `.{ .page_it = undefined }`: the inner `page_it` is
// never read because `chunk == null`. Used by the iterator wrappers above when
// `getBottomRight` returns null (an empty region).
fn empty_row_iterator() -> RowIterator {
    RowIterator {
        page_it: PageIterator {
            row: None,
            limit: Limit::None,
            direction: Direction::RightDown,
        },
        chunk: None,
        offset: 0,
    }
}

// PORT NOTE: the reference `writer.writeByteNTimes(byte, n)` writes `byte` `n`
// times. We provide the faithful helper over a `core::fmt::Write` sink.
fn write_byte_n_times<W: core::fmt::Write>(
    writer: &mut W,
    byte: u8,
    n: usize,
) -> core::fmt::Result {
    for _ in 0..n {
        writer.write_char(byte as char)?;
    }
    Ok(())
}

pub struct Cell {
    pub node: *mut Node,
    pub row: *mut Row,
    pub cell: *mut pagepkg::Cell,
    pub row_idx: size::CellCountInt,
    pub col_idx: size::CellCountInt,
}

impl Cell {
    /// Returns true if this cell is marked as dirty.
    ///
    /// This is not very performant this is primarily used for assertions
    /// and testing.
    pub fn is_dirty(&self) -> bool {
        unsafe { (*super::Node::page(self.node)).dirty || (*self.row).dirty() }
    }

    /// Get the cell style.
    ///
    /// Not meant for non-test usage since this is inefficient.
    pub fn style(&self) -> stylepkg::Style {
        if unsafe { (*self.cell).style_id() } == stylepkg::DEFAULT_ID {
            return stylepkg::Style::default();
        }
        unsafe {
            *(*super::Node::page(self.node)).styles.get(
                (*super::Node::page(self.node)).memory_ptr(),
                (*self.cell).style_id(),
            )
        }
    }

    /// Gets the screen point for the given cell.
    ///
    /// This is REALLY expensive/slow so it isn't pub. This was built
    /// for debugging and tests. If you have a need for this outside of
    /// this file then consider a different approach and ask yourself very
    /// carefully if you really need this.
    pub fn screen_point(&self) -> point::Point {
        // PORT NOTE (oracle e6e4a9fdc, `REVIEW_CLASSES.md` §1): the accumulator
        // is `u32`, not `size::CellCountInt`. `point::Coordinate.y` is already
        // `u32`; accumulating page-local `u16` row counts in a `u16` overflowed
        // once scrollback crossed 65,535 physical rows — which PANICS in debug
        // and WRAPS SILENTLY in release, producing a wrong coordinate with no
        // diagnostic. Widening makes the addition exact over the full range the
        // coordinate type promises, so there is no lossy softening to guard.
        let mut y: u32 = self.row_idx as u32;
        let mut node_ = self.node;
        while !unsafe { (*node_).prev }.is_null() {
            let node = unsafe { (*node_).prev };
            y += unsafe { super::Node::rows(node) } as u32;
            node_ = node;
        }

        point::Point::Screen(point::Coordinate { x: self.col_idx, y })
    }
}
