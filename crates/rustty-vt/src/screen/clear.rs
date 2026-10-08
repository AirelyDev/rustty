//! PORTS: ghostty src/terminal/Screen.zig — the scroll/erase/clear cluster
//! (`scroll`, `scrollClear`, `viewportIsBottom`, `eraseHistory`, `eraseActive`,
//! `clearRows`, `clearCells`, `clearUnprotectedCells`, `splitCellBoundary`,
//! `blankCell`) and `resize` with `clearPromptForRedraw`.

use super::*;

impl Screen {
    /// Scroll the viewport of the terminal grid.
    pub fn scroll(&mut self, behavior: Scroll) {
        // if (comptime build_options.kitty_graphics) {
        //     // No matter what, scrolling marks our image state as dirty since
        //     // it could move placements. If there are no placements or no images
        //     // this is still a very cheap operation.
        //     self.kitty_images.dirty = true;
        // }
        if crate::options::KITTY_GRAPHICS {
            // No matter what, scrolling marks our image state as dirty since
            // it could move placements. If there are no placements or no images
            // this is still a very cheap operation.
            self.kitty_images.dirty = true;
        }

        match behavior {
            Scroll::Active => self.pages.scroll(pagelist::scroll::Scroll::Active),
            Scroll::Top => self.pages.scroll(pagelist::scroll::Scroll::Top),
            Scroll::Pin(p) => self.pages.scroll(pagelist::scroll::Scroll::Pin(p)),
            Scroll::Row(v) => self.pages.scroll(pagelist::scroll::Scroll::Row(v)),
            Scroll::DeltaRow(v) => self.pages.scroll(pagelist::scroll::Scroll::DeltaRow(v)),
            Scroll::DeltaPrompt(v) => self.pages.scroll(pagelist::scroll::Scroll::DeltaPrompt(v)),
        }

        self.assert_integrity();
    }

    /// See PageList.scrollClear. In addition to that, we reset the cursor
    /// to be on top.
    pub fn scroll_clear(&mut self) -> Result<(), OutOfMemory> {
        self.pages.scroll_clear()?;
        self.cursor_reload();

        // if (comptime build_options.kitty_graphics) {
        //     // No matter what, scrolling marks our image state as dirty since
        //     // it could move placements. If there are no placements or no images
        //     // this is still a very cheap operation.
        //     self.kitty_images.dirty = true;
        // }
        if crate::options::KITTY_GRAPHICS {
            // No matter what, scrolling marks our image state as dirty since
            // it could move placements. If there are no placements or no images
            // this is still a very cheap operation.
            self.kitty_images.dirty = true;
        }

        self.assert_integrity();
        Ok(())
    }

    /// Returns true if the viewport is scrolled to the bottom of the screen.
    pub fn viewport_is_bottom(&self) -> bool {
        matches!(self.pages.viewport, pagelist::Viewport::Active)
    }

    /// Erase the region specified by tl and br, inclusive. This will physically
    /// erase the rows meaning the memory will be reclaimed (if the underlying
    /// page is empty) and other rows will be shifted up.
    pub fn erase_history(&mut self, bl: Option<point::Point>) {
        // defer self.assertIntegrity();
        // PORT NOTE: Rust has no `defer`; the reference runs `assertIntegrity` at
        // the end, which here is the single exit path.
        self.pages.erase_history(bl);
        self.cursor_reload();
        self.assert_integrity();
    }

    pub fn erase_active(&mut self, y: size::CellCountInt) {
        // defer self.assertIntegrity();
        self.pages.erase_active(y);
        self.cursor_reload();
        self.assert_integrity();
    }

    // Clear the region specified by tl and bl, inclusive. Cleared cells are
    // colored with the current style background color. This will clear all
    // cells in the rows.
    //
    // If protected is true, the protected flag will be respected and only
    // unprotected cells will be cleared. Otherwise, all cells will be cleared.
    pub fn clear_rows(&mut self, tl: point::Point, bl: Option<point::Point>, protected: bool) {
        // defer self.assertIntegrity();
        // PORT NOTE: Rust has no `defer`; we run `assertIntegrity` at the end,
        // the single exit path of this body.

        // PORT NOTE (oracle e727a3658, `P3-W1 T4`): the desired `self.pages.cols`
        // is NOT used to slice the backing cells any more. During incomplete
        // reflow a stored page can be narrower than the desired width, and
        // slicing to the desired width ran off the end of the row and tripped
        // `clear_cells`' own bounds `debug_assert!`s before a single cell was
        // cleared. The cells come from the OWNING page instead.
        let mut it = self
            .pages
            .page_iterator(pagelist::iter::Direction::RightDown, tl, bl);
        while let Some(chunk) = it.next() {
            // PORT NOTE: `chunk.node` is the `*mut Node`; `&chunk.node.data` is
            // the page. We take a raw `*mut Page` via `addr_of_mut!` (the page
            // lives in the node) and read `chunk.rows()` (a `*mut [Row]`).
            let page: *mut Page = unsafe { crate::pagelist::Node::page(chunk.node) };
            let rows = chunk.rows();
            let rows_len = rows.len();
            let rows_base = rows as *mut Row;
            for i in 0..rows_len {
                let row: *mut Row = unsafe { rows_base.add(i) };

                let cells_offset = unsafe { (*row).cells() };
                // const cells = page.getCells(row);
                //
                // Kept in the existing raw-pointer style: `get_cells` returns a
                // `*mut [Cell]` and no `&mut [Cell]` is formed while the
                // `*mut Page` alias is live (see the `clear_cells` PORT NOTE).
                let cells: *mut [Cell] = unsafe { (*page).get_cells(row) };

                // Clear all cells
                if protected {
                    self.clear_unprotected_cells(page, row, cells);
                    // We need to preserve other row attributes since we only
                    // cleared unprotected cells.
                    unsafe { (*row).set_cells(cells_offset) };
                } else {
                    self.clear_cells(page, row, cells);
                    // row.* = .{ .cells = cells_offset };
                    // PORT NOTE: the reference resets the entire `Row` to a fresh
                    // value carrying only the `cells` offset (all other row flags
                    // — wrap, grapheme, styled, dirty, etc. — reset to their
                    // zero/false defaults). We mirror that with a zeroed `Row`
                    // whose cells offset is restored.
                    let mut fresh = Row::zeroed();
                    fresh.set_cells(cells_offset);
                    unsafe { *row = fresh };
                }

                unsafe { (*row).set_dirty(true) };
            }
        }

        self.assert_integrity();
    }

    /// Clear the cells with the blank cell.
    ///
    /// This takes care to handle cleaning up graphemes and styles.
    ///
    /// PORT NOTE (Stacked Borrows / Miri): the reference signature is
    /// `clearCells(self, page: *Page, row: *Row, cells: []Cell)` — Zig pointers
    /// and slices are UNPROTECTED. If we mirror it with `&mut Page`/`&mut Row`/
    /// `&mut [Cell]` ARGS, those references are *strongly protected* for the
    /// entire call; the trailing `assert_integrity()` then walks `self.pages`
    /// and re-reads the SAME cell/page memory through a different tag, which
    /// Miri (correctly) rejects as "would remove a strongly-protected tag" UB.
    /// We therefore take RAW pointers (`*mut Page`, `*mut Row`, `*mut [Cell]`),
    /// matching Zig's unprotected semantics, and reconstruct local `&mut`
    /// references ONLY for the brief mutation scopes — never live across an
    /// `assert_integrity()` call.
    #[allow(clippy::not_unsafe_ptr_arg_deref)] // reference takes `page: *Page, row: *Row, cells: []Cell`
    pub fn clear_cells(&mut self, page: *mut Page, row: *mut Row, cells: *mut [Cell]) {
        // if (cells.len == 0) return;
        // PORT NOTE (parity 0cb004734): the runtime-safety validation below
        // indexes the first and last elements unconditionally; an empty range
        // is a valid no-op clear.
        if cells.len() == 0 {
            return;
        }

        let cells_base = cells as *mut Cell;
        let cells_len = cells.len();

        // This whole operation does unsafe things, so we just want to assert
        // the end state.
        unsafe { (*page).pause_integrity_checks(true) };

        // PORT NOTE: the reference uses a `defer` block to resume integrity
        // checks + assert at every exit; with a single exit path we run it at
        // the end. (No early returns in this body after the empty-range
        // guard, which returns before checks are paused.)

        if cfg!(debug_assertions) {
            // Our row and cells should be within the page.
            let page_rows = unsafe { (*page).rows.ptr((*page).memory_ptr()) };
            debug_assert!(row as usize >= page_rows as usize);
            debug_assert!(
                row as usize <= unsafe { page_rows.add((*page).size.rows as usize - 1) } as usize
            );

            let row_cells = unsafe { (*page).get_cells(row) };
            let row_cells_base = row_cells as *mut Cell;
            let row_cells_len = row_cells.len();
            debug_assert!(cells_base as usize >= row_cells_base as usize);
            debug_assert!(
                unsafe { cells_base.add(cells_len - 1) } as usize
                    <= unsafe { row_cells_base.add(row_cells_len - 1) } as usize
            );
        }

        // If we have managed memory (styles, graphemes, or hyperlinks)
        // in this row then we go cell by cell and clear them if present.
        if unsafe { (*row).grapheme() } {
            for i in 0..cells_len {
                let cell = unsafe { cells_base.add(i) };
                if unsafe { (*cell).has_grapheme() } {
                    unsafe { (*page).clear_grapheme(cell) };
                }
            }

            // If we have no left/right scroll region we can be sure
            // that we've cleared all the graphemes, so we clear the
            // flag, otherwise we ask the page to update the flag.
            if cells_len == unsafe { (*page).size.cols } as usize {
                unsafe { (*row).set_grapheme(false) };
            } else {
                unsafe { (*page).update_row_grapheme_flag(row) };
            }
        }

        if unsafe { (*row).hyperlink() } {
            for i in 0..cells_len {
                let cell = unsafe { cells_base.add(i) };
                if unsafe { (*cell).hyperlink() } {
                    unsafe { (*page).clear_hyperlink(cell) };
                }
            }

            // If we have no left/right scroll region we can be sure
            // that we've cleared all the hyperlinks, so we clear the
            // flag, otherwise we ask the page to update the flag.
            if cells_len == unsafe { (*page).size.cols } as usize {
                unsafe { (*row).set_hyperlink(false) };
            } else {
                unsafe { (*page).update_row_hyperlink_flag(row) };
            }
        }

        if unsafe { (*row).styled() } {
            // Styled cells overwhelmingly come in runs sharing the same
            // style (e.g. a colored status bar or a highlighted region),
            // so group them and release each run with a single ref-count
            // update rather than per cell.
            //
            // PORT NOTE (oracle 8d663a76e, `P3-W1 T6`). UB HAZARD: the style
            // ids are read through the raw `*mut Cell` (`cells_base.add(..)`),
            // exactly as the previous per-cell loop did, because
            // `release_multiple` mutates page memory through `*mut Page`. A
            // live `&mut [Cell]` across that call would be Stacked-Borrows UB
            // (memory: rust-mut-args-strongly-protected).
            //
            // `release_multiple`'s own `ref_count >= n` assertion holds by
            // construction: every cell in the run held a reference.
            let mut i: usize = 0;
            while i < cells_len {
                let id = unsafe { (*cells_base.add(i)).style_id() };
                if id == style::DEFAULT_ID {
                    i += 1;
                    continue;
                }
                let mut j = i + 1;
                while j < cells_len && unsafe { (*cells_base.add(j)).style_id() } == id {
                    j += 1;
                }
                unsafe {
                    let base = (*page).memory_ptr();
                    (*page)
                        .styles
                        .release_multiple(base, id, (j - i) as style::Id);
                }
                i = j;
            }

            // If we have no left/right scroll region we can be sure
            // that we've cleared all the styles, so we clear the
            // flag, otherwise we ask the page to update the flag.
            if cells_len == unsafe { (*page).size.cols } as usize {
                unsafe { (*row).set_styled(false) };
            } else {
                unsafe { (*page).update_row_styled_flag(row) };
            }
        }

        // if (comptime build_options.kitty_graphics) {
        //     if (row.kitty_virtual_placeholder and
        //         cells.len == page.size.cols)
        //     {
        //         for (cells) |c| {
        //             if (c.codepoint() == kitty.graphics.unicode.placeholder) {
        //                 break;
        //             }
        //         } else row.kitty_virtual_placeholder = false;
        //     }
        // }
        //
        // PORT NOTE: the Zig `for ... else` clears the flag only when the loop
        // ran to completion, i.e. when NO surviving cell still carries the
        // placeholder codepoint. The cells are read through the raw
        // `cells_base` for the same Stacked-Borrows reason as the style loop
        // above.
        if crate::options::KITTY_GRAPHICS
            && unsafe { (*row).kitty_virtual_placeholder() }
            && cells_len == unsafe { (*page).size.cols } as usize
        {
            let mut found = false;
            for i in 0..cells_len {
                if unsafe { (*cells_base.add(i)).codepoint() }
                    == crate::kitty::graphics_unicode::PLACEHOLDER
                {
                    found = true;
                    break;
                }
            }
            if !found {
                unsafe { (*row).set_kitty_virtual_placeholder(false) };
            }
        }

        let blank = self.blank_cell();
        // Reconstruct a local `&mut [Cell]` ONLY for the brief fill; it is
        // dropped before any assert_integrity() below.
        {
            let cells_slice: &mut [Cell] =
                unsafe { core::slice::from_raw_parts_mut(cells_base, cells_len) };
            cells_slice.fill(blank);
        }

        // defer { page.pauseIntegrityChecks(false); page.assertIntegrity();
        //   self.assertIntegrity(); }
        unsafe { (*page).pause_integrity_checks(false) };
        unsafe { (*page).assert_integrity() };
        self.assert_integrity();
    }

    /// Clear cells but only if they are not protected.
    ///
    /// PORT NOTE (Stacked Borrows / Miri): like `clear_cells`, the reference
    /// signature `clearUnprotectedCells(self, page: *Page, row: *Row,
    /// cells: []Cell)` uses UNPROTECTED Zig pointers/slices. We take RAW
    /// pointers and read each cell's `protected` flag through the raw pointer,
    /// passing raw sub-slice pointers down to `clear_cells`.
    #[allow(clippy::not_unsafe_ptr_arg_deref)] // reference takes `page: *Page, row: *Row, cells: []Cell`
    pub fn clear_unprotected_cells(&mut self, page: *mut Page, row: *mut Row, cells: *mut [Cell]) {
        let cells_base = cells as *mut Cell;
        let cells_len = cells.len();

        let mut x0: usize = 0;
        let mut x1: usize;

        'clear: while x0 < cells_len {
            while unsafe { (*cells_base.add(x0)).protected() } {
                x0 += 1;
                if x0 >= cells_len {
                    break 'clear;
                }
            }
            x1 = x0 + 1;
            while x1 < cells_len && !unsafe { (*cells_base.add(x1)).protected() } {
                x1 += 1;
            }
            let sub: *mut [Cell] =
                core::ptr::slice_from_raw_parts_mut(unsafe { cells_base.add(x0) }, x1 - x0);
            self.clear_cells(page, row, sub);
            x0 = x1;
        }

        unsafe { (*page).assert_integrity() };
        self.assert_integrity();
    }

    /// Clean up boundary conditions where a cell will become discontiguous with
    /// a neighboring cell because either one of them will be moved and/or cleared.
    ///
    /// For performance reasons this is specialized to operate on the cursor row.
    ///
    /// Handles the boundary between the cell at `x` and the cell at `x - 1`.
    ///
    /// So, for example, when moving a region of cells [a, b] (inclusive), call this
    /// function with `x = a` and `x = b + 1`. It is okay if `x` is out of bounds by
    /// 1, this will be interpreted correctly.
    ///
    /// DOES NOT MODIFY ROW WRAP STATE! See `cursor_reset_wrap` for that.
    ///
    /// The following boundary conditions are handled:
    ///
    /// - `x - 1` is a wide character and `x` is a spacer tail:
    ///   o Both cells will be cleared.
    ///   o If `x - 1` is the start of the row and was wrapped from a previous row
    ///     then the previous row is checked for a spacer head, which is cleared if
    ///     present.
    ///
    /// - `x == 0` and is a wide character:
    ///   o If the row is a wrap continuation then the previous row will be checked
    ///     for a spacer head, which is cleared if present.
    ///
    /// - `x == cols` and `x - 1` is a spacer head:
    ///   o `x - 1` will be cleared.
    ///
    /// NOTE(qwerasd): This method is not scrolling region aware, and cannot be
    /// since it's on Screen not Terminal. This needs to be addressed down the
    /// line. Not an extremely urgent issue since it's an edge case of an edge
    /// case, but not ideal.
    pub fn split_cell_boundary(&mut self, x: size::CellCountInt) {
        // const page = &self.cursor.page_pin.node.data;
        let page: *mut Page = unsafe { crate::pagelist::Node::page((*self.cursor.page_pin).node) };

        // PORT NOTE: stash the raw cursor pin/row pointers in locals up front so
        // the `clear_cells(&mut self, ...)` calls below don't reborrow `self`
        // through `self.cursor.*` while `self` is already mutably borrowed by the
        // method receiver (a Rust borrow-checker constraint with no reference
        // analogue). These are non-owning copies of the same raw pointers.
        let cursor_page_pin: *mut Pin = self.cursor.page_pin;
        let cursor_page_row: *mut Row = self.cursor.page_row;

        unsafe { (*page).pause_integrity_checks(true) };
        // defer page.pauseIntegrityChecks(false);
        // PORT NOTE: Rust has no `defer`; this body has multiple early returns,
        // so we resume integrity checks before each `return` and at the natural
        // end. The reference's single `defer` runs at every exit identically.

        // const cols = self.cursor.page_pin.node.data.size.cols;
        let cols = unsafe { crate::pagelist::Node::cols((*cursor_page_pin).node) };

        // `x` may be up to an INCLUDING `cols`, since that signifies splitting
        // the boundary to the right of the final cell in the row.
        debug_assert!(x <= cols);

        // [ A B C D E F|]
        //              ^ Boundary between final cell and row end.
        if x == cols {
            if !unsafe { (*cursor_page_row).wrap() } {
                unsafe { (*page).pause_integrity_checks(false) };
                return;
            }

            let cells = unsafe { (*cursor_page_pin).cells(pagelist::pin::CellSubset::All) };
            let cells_base = cells as *mut Cell;

            // Spacer head at end of wrapped row.
            if unsafe { (*cells_base.add(cols as usize - 1)).wide() } == pagepkg::Wide::SpacerHead {
                let slice = core::ptr::slice_from_raw_parts_mut(
                    unsafe { cells_base.add(cols as usize - 1) },
                    1,
                );
                self.clear_cells(page, cursor_page_row, slice);
            }

            unsafe { (*page).pause_integrity_checks(false) };
            return;
        }

        // [|A B C D E F ]
        //  ^ Boundary between first cell and row start.
        //
        //  OR
        //
        // [ A|B C D E F ]
        //    ^ Boundary between first cell and second cell.
        //
        // First cell may be a wrapped wide cell with a spacer
        // head on the previous row that needs to be cleared.
        if (x == 0 || x == 1) && unsafe { (*cursor_page_row).wrap_continuation() } {
            let cells = unsafe { (*cursor_page_pin).cells(pagelist::pin::CellSubset::All) };
            let cells_base = cells as *mut Cell;

            // If the first cell in a row is wide the previous row
            // may have a spacer head which needs to be cleared.
            if unsafe { (*cells_base).wide() } == pagepkg::Wide::Wide {
                if let Some(p_row) = unsafe { (*cursor_page_pin).up(1) } {
                    let p_rac = p_row.row_and_cell();
                    let p_cells = p_row.cells(pagelist::pin::CellSubset::All);
                    let p_cells_base = p_cells as *mut Cell;
                    let p_cols = unsafe { crate::pagelist::Node::cols(p_row.node) };
                    let p_cell = unsafe { *p_cells_base.add(p_cols as usize - 1) };
                    if p_cell.wide() == pagepkg::Wide::SpacerHead {
                        let p_page: *mut Page = unsafe { crate::pagelist::Node::page(p_row.node) };
                        let slice = core::ptr::slice_from_raw_parts_mut(
                            unsafe { p_cells_base.add(p_cols as usize - 1) },
                            1,
                        );
                        self.clear_cells(p_page, p_rac.row, slice);

                        // `clearCells` does not mark rows dirty, and our
                        // callers only mark the cursor row, so mark the
                        // previous row here.
                        p_row.mark_dirty();
                    }
                }
            }
        }

        // If x is 0 then we're done.
        if x == 0 {
            unsafe { (*page).pause_integrity_checks(false) };
            return;
        }

        // [ ... X|Y ... ]
        //        ^ Boundary between two cells in the middle of the row.
        {
            debug_assert!(x > 0);
            debug_assert!(x < cols);

            let cells = unsafe { (*cursor_page_pin).cells(pagelist::pin::CellSubset::All) };
            let cells_base = cells as *mut Cell;

            let left = unsafe { *cells_base.add(x as usize - 1) };
            match left.wide() {
                // There should not be spacer heads in the middle of the row.
                pagepkg::Wide::SpacerHead => unreachable!(),

                // We don't need to do anything for narrow cells or spacer tails.
                pagepkg::Wide::Narrow | pagepkg::Wide::SpacerTail => {}

                // A wide char would be split, so must be cleared.
                pagepkg::Wide::Wide => {
                    let slice = core::ptr::slice_from_raw_parts_mut(
                        unsafe { cells_base.add(x as usize - 1) },
                        2,
                    );
                    self.clear_cells(page, cursor_page_row, slice);
                }
            }
        }

        unsafe { (*page).pause_integrity_checks(false) };
    }

    /// Returns the blank cell to use when doing terminal operations that
    /// require preserving the bg color.
    #[inline]
    pub fn blank_cell(&self) -> Cell {
        if self.cursor.style_id == style::DEFAULT_ID {
            return Cell::zeroed();
        }
        self.cursor.style.bg_cell().unwrap_or_else(Cell::zeroed)
    }

    /// Resize the screen. The rows or cols can be bigger or smaller. If this
    /// returns an error, the screen is unchanged.
    // PERF(port): `#[inline]` — the reference is `pub inline fn resize`; the
    // extra call frame was a surplus fixed cost on the 5M-call rows benchmark.
    #[inline]
    pub fn resize(&mut self, opts: Resize) -> Result<(), OutOfMemory> {
        // defer self.assertIntegrity();
        // PORT NOTE: Rust has no `defer`. The reference runs `assertIntegrity`
        // at every exit; we wrap the body in a helper and run the assert after.
        let result = self.resize_inner(opts);
        self.assert_integrity();
        result
    }

    fn resize_inner(&mut self, opts: Resize) -> Result<(), OutOfMemory> {
        // We need to insert a tracked pin for our saved cursor so we can
        // modify its X/Y for reflow. Do this before changing any state since
        // tracking the pin can fail.
        let saved_cursor_pin: Option<*mut Pin> = 'saved_cursor: {
            let Some(sc) = self.saved_cursor.as_ref() else {
                break 'saved_cursor None;
            };
            let (sc_x, sc_y) = (sc.x, sc.y as u32);
            let Some(pin) = self
                .pages
                .pin(point::Point::Active(point::Coordinate { x: sc_x, y: sc_y }))
            else {
                break 'saved_cursor None;
            };
            #[cfg(test)]
            resize_tw_check(ResizeFp::SavedCursorPin)?;
            Some(self.pages.track_pin(pin)?)
        };
        // PORT NOTE: the reference `defer if (saved_cursor_pin) |p|
        // self.pages.untrackPin(p);` is run at every exit below via
        // `resize_tracked`.
        let result = self.resize_tracked(opts, saved_cursor_pin);
        if let Some(p) = saved_cursor_pin {
            self.pages.untrack_pin(p);
        }
        result
    }

    /// The remainder of `resize` once the saved-cursor pin is tracked.
    //
    // PORT NOTE (UB HAZARD, area A §5(a)): `PageList::resize` reallocates and
    // can destroy pages, so nothing here may hold a `&mut Page` (or any
    // reference derived from one) across it. Every page touch below goes
    // through a freshly-materialized `*mut Page`, and the only value that
    // outlives the resize is the raw `cursor_node` POINTER, which is compared
    // but never dereferenced afterwards (see the survival check at the bottom).
    fn resize_tracked(
        &mut self,
        opts: Resize,
        saved_cursor_pin: Option<*mut Pin>,
    ) -> Result<(), OutOfMemory> {
        // A cursor style and hyperlink are partly stored in the page containing
        // the cursor. Their IDs only have meaning within that page, and the page
        // keeps reference counts for them. Resizing can replace or destroy the
        // page, so below we temporarily remove both values from the cursor before
        // asking PageList to resize.
        //
        // Save everything first so we can put the cursor back together afterward,
        // or restore it unchanged if the resize fails. We also save the original
        // node and serial so after a successful resize we can tell whether that
        // page survived and still needs its temporary references released.
        let cursor_node = unsafe { (*self.cursor.page_pin).node };
        let cursor_node_serial = unsafe { (*cursor_node).serial };
        let cursor_style = self.cursor.style;
        let cursor_style_id = self.cursor.style_id;
        let cursor_hyperlink = self.cursor.hyperlink;
        let cursor_hyperlink_id = self.cursor.hyperlink_id;

        // Keep an extra reference to the cursor style and hyperlink while the
        // resize is in progress. Releasing the cursor references below therefore
        // can't delete either entry, and an error can restore the cursor without
        // any allocation.
        {
            let page: *mut Page = unsafe { crate::pagelist::Node::page(cursor_node) };
            unsafe {
                let base = (*page).memory_ptr();
                if cursor_style_id != style::DEFAULT_ID {
                    (*page).styles.use_id(base, cursor_style_id);
                }
                if cursor_hyperlink_id != 0 {
                    (*page).hyperlink_set.use_id(base, cursor_hyperlink_id);
                }
            }
        }
        // errdefer {
        //     self.cursor.style = cursor_style;
        //     self.cursor.style_id = cursor_style_id;
        //     self.cursor.hyperlink = cursor_hyperlink;
        //     self.cursor.hyperlink_id = cursor_hyperlink_id;
        // }
        //
        // PORT NOTE: Rust has no `errdefer`; the block above is spelled out as
        // `resize_restore_cursor` and invoked on each of the two fallible exits
        // below. It performs NO allocation, which is the point of the extra
        // references taken just above.

        // Release the cursor style while resizing just in case the cursor ends
        // up on a different page.
        self.cursor.style = style::Style::default();
        self.manual_style_update()
            .expect("manualStyleUpdate to default cannot fail");

        // If we have a hyperlink, release it from the old page
        // and then we need to re-add it to the new page. This needs
        // to happen because resize below typically reallocates a
        // new page so the old hyperlink is invalid.
        if self.cursor.hyperlink_id != 0 {
            // Note we do NOT use endHyperlink because we want to keep
            // our allocated self.cursor.hyperlink valid.
            let page: *mut Page =
                unsafe { crate::pagelist::Node::page((*self.cursor.page_pin).node) };
            unsafe {
                let base = (*page).memory_ptr();
                (*page)
                    .hyperlink_set
                    .release(base, self.cursor.hyperlink_id);
            }
            self.cursor.hyperlink_id = 0;
            self.cursor.hyperlink = None;
        }

        // Perform the resize operation.
        // try tw.check(.pages);
        #[cfg(test)]
        if let Err(e) = resize_tw_check(ResizeFp::Pages) {
            self.resize_restore_cursor(
                cursor_style,
                cursor_style_id,
                cursor_hyperlink,
                cursor_hyperlink_id,
            );
            return Err(e);
        }
        if let Err(e) = self.pages.resize(pagelist::resize::Resize {
            rows: Some(opts.rows),
            cols: Some(opts.cols),
            reflow: opts.reflow,
            cursor: Some(pagelist::resize::Cursor {
                x: self.cursor.x,
                y: self.cursor.y,
                pin: Some(self.cursor.page_pin),
            }),
            pull_scrollback: opts.pull_scrollback,
        }) {
            self.resize_restore_cursor(
                cursor_style,
                cursor_style_id,
                cursor_hyperlink,
                cursor_hyperlink_id,
            );
            return Err(e);
        }

        // errdefer comptime unreachable;
        //
        // PORT NOTE: "No more failures are possible after this. Enforced by
        // compiler because we do NO cleanup of the state below." Zig's
        // `errdefer comptime unreachable` is a COMPILE-TIME assertion that no
        // fallible operation follows. Rust has no equivalent construct; the
        // property is held by construction — every call below either returns
        // `()` or swallows its error the way the reference does — and the
        // function's single remaining `return` is the `Ok(())` at the end.

        // Resizing moves image placements, so mark their geometry as dirty.
        if crate::options::KITTY_GRAPHICS {
            self.kitty_images.dirty = true;
        }

        // If we have no scrollback and we shrunk our rows, we must explicitly
        // erase our history. This is because PageList always keeps at least
        // a page size of history.
        if self.no_scrollback {
            self.pages.erase_history(None);
        }

        // If our cursor was updated, we do a full reload so all our cursor
        // state is correct.
        self.cursor_reload();

        // If resize moved a cursor with pending wrap away from the right edge,
        // advance to the next cell instead of wrapping on the next print.
        if self.cursor.pending_wrap && self.cursor.x != opts.cols - 1 {
            self.cursor.pending_wrap = false;
            self.cursor_right(1);
        }

        // Clear any redrawable prompt after the fallible resize but before
        // restoring the cursor style and hyperlink, so cleared cells retain the
        // same default styling they had with the previous ordering.
        self.clear_prompt_for_redraw(opts.prompt_redraw);

        // Restore the cursor style.
        self.cursor.style = cursor_style;
        if let Err(err) = self.manual_style_update() {
            // This failure should not happen because manualStyleUpdate handles
            // page splitting, overflow, and more. This should only happen if
            // we're out of RAM. In this case, we'll just degrade gracefully back
            // to the default style.
            log::error!(target: LOG_SCOPE, "failed to update style on cursor reload err={err:?}");
            self.cursor.style = style::Style::default();
            self.cursor.style_id = 0;
        }

        // If we reflowed a saved cursor, update it.
        if let Some(p) = saved_cursor_pin {
            // This should never fail because a non-null saved_cursor_pin
            // implies a non-null saved_cursor.
            let pin_val = unsafe { *p };
            match self.pages.point_from_pin(point::Tag::Active, pin_val) {
                Some(pt) => {
                    let sc = self.saved_cursor.as_mut().unwrap();
                    sc.x = pt.coord().x;
                    sc.y = pt.coord().y as size::CellCountInt;

                    // If we had pending wrap set and we're no longer at the end of
                    // the line, we unset the pending wrap and move the cursor to
                    // reflect the correct next position.
                    if sc.pending_wrap && sc.x != opts.cols - 1 {
                        sc.pending_wrap = false;
                        sc.x += 1;
                    }
                }
                None => {
                    // I think this can happen if the screen is resized to be
                    // less rows or less cols and our saved cursor moves outside
                    // the active area. In this case, there isn't anything really
                    // reasonable we can do so we just move the cursor to the
                    // top-left. It may be reasonable to also move the cursor to
                    // match the primary cursor. Any behavior is fine since this is
                    // totally unspecified.
                    let sc = self.saved_cursor.as_mut().unwrap();
                    sc.x = 0;
                    sc.y = 0;
                    sc.pending_wrap = false;
                }
            }
        }

        // Fix up our hyperlink if we had one.
        if let Some(link) = cursor_hyperlink {
            // PORT NOTE: the reference passes the old link's slices straight
            // through. `link` is detached from `self.cursor` here, so borrowing
            // its bytes does not alias the state `start_hyperlink` releases, and
            // no copy is made.
            let uri: &[u8] = unsafe { &(*link).uri };
            let id_: Option<&[u8]> = match unsafe { &(*link).id } {
                hyperlink::HyperlinkId::Explicit(v) => Some(v),
                hyperlink::HyperlinkId::Implicit(_) => None,
            };
            if let Err(err) = self.start_hyperlink(uri, id_) {
                // This shouldn't happen because startHyperlink should handle
                // resizing. This only happens if we're truly out of RAM. Degrade
                // to forgetting the hyperlink.
                log::error!(target: LOG_SCOPE, "failed to update hyperlink on resize err={err:?}");
            }

            // Remove our old link
            // link.deinit(self.alloc);
            // self.alloc.destroy(link);
            unsafe { destroy_cursor_hyperlink(link) };
        }

        // A tracked pin follows its content when PageList moves or replaces a
        // page. If the cursor's pin still points to the node we started with,
        // that node survived the resize and still owns the temporary style and
        // hyperlink references we added above.
        //
        // Comparing pointers is not enough by itself. PageList keeps a pool of
        // nodes, so it can destroy a node and create a replacement at the same
        // memory address. Every new use of a node gets a different serial number,
        // making the pointer-and-serial pair an O(1) identity check.
        let live_node = unsafe { (*self.cursor.page_pin).node };
        let cursor_node_survived = core::ptr::eq(
            live_node as *const crate::pagelist::Node,
            cursor_node as *const crate::pagelist::Node,
        ) && unsafe { (*live_node).serial } == cursor_node_serial;
        if crate::options::SLOW_RUNTIME_SAFETY {
            assert!(
                cursor_node_survived == self.pages.node_is_valid(cursor_node, cursor_node_serial)
            );
        }

        // A surviving node still contains our temporary references, so remove
        // them now. If the node was replaced, its destruction removed those
        // references along with the rest of the old page so we don't need to
        // fix it up.
        //
        // PORT NOTE: the reference writes `cursor_node.page()`. We deref
        // `live_node` instead — the same address when we get here, but with
        // live provenance; `cursor_node`'s allocation may have been retired and
        // re-created by the pool, which would make a deref through it UB under
        // Stacked Borrows even though the addresses match.
        if cursor_node_survived {
            let page: *mut Page = unsafe { crate::pagelist::Node::page(live_node) };
            unsafe {
                let base = (*page).memory_ptr();
                if cursor_style_id != style::DEFAULT_ID {
                    (*page).styles.release(base, cursor_style_id);
                }
                if cursor_hyperlink_id != 0 {
                    (*page).hyperlink_set.release(base, cursor_hyperlink_id);
                }
            }
        }

        Ok(())
    }

    /// The `errdefer` of `resize`, spelled out: put the cursor back exactly as
    /// it was, with NO allocation (the extra style/hyperlink references taken
    /// at the top of `resize_tracked` guarantee the IDs are still live).
    fn resize_restore_cursor(
        &mut self,
        cursor_style: style::Style,
        cursor_style_id: style::Id,
        cursor_hyperlink: Option<*mut CursorHyperlink>,
        cursor_hyperlink_id: hyperlink::Id,
    ) {
        self.cursor.style = cursor_style;
        self.cursor.style_id = cursor_style_id;
        self.cursor.hyperlink = cursor_hyperlink;
        self.cursor.hyperlink_id = cursor_hyperlink_id;
    }

    fn clear_prompt_for_redraw(&mut self, redraw: osc::semantic_prompt::Redraw) {
        // If our cursor is on a prompt or input line, clear it so the shell can
        // redraw it. This works with OSC 133 semantic prompts. We do this after
        // the fallible resize so an error leaves the original prompt untouched.
        //
        // We check cursor.semantic_content rather than page_row.semantic_prompt
        // because some shells (e.g., Nu) mark input areas with OSC 133 B but don't
        // mark continuation lines with k=s. If the input spans multiple lines and
        // continuation lines are unmarked, checking only page_row.semantic_prompt
        // would miss them. By checking semantic_content, we assume that if the
        // cursor is on anything other than command output, we're at a prompt/input
        // line and should clear from there.
        if !matches!(redraw, osc::semantic_prompt::Redraw::False)
            && !matches!(
                self.cursor.semantic_content,
                pagepkg::SemanticContent::Output
            )
        {
            'prompt: {
                match redraw {
                    osc::semantic_prompt::Redraw::False => unreachable!(),

                    // For `.last`, only clear the current line where the cursor is.
                    // For `.true`, clear all prompt lines starting from the beginning.
                    osc::semantic_prompt::Redraw::Last => {
                        let page: *mut Page =
                            unsafe { crate::pagelist::Node::page((*self.cursor.page_pin).node) };
                        let row = self.cursor.page_row;
                        let cells = unsafe { (*page).get_cells(row) };
                        self.clear_cells(page, row, cells);
                    }

                    osc::semantic_prompt::Redraw::True => {
                        let start = {
                            let mut it = unsafe { *self.cursor.page_pin }
                                .prompt_iterator(pagelist::iter::Direction::LeftUp, None);
                            match it.next() {
                                Some(p) => p,
                                None => {
                                    // This should never happen because promptIterator
                                    // should always find a prompt if we already verified
                                    // our row is some kind of prompt.
                                    log::warn!(
                                        target: LOG_SCOPE,
                                        "cursor on prompt line but promptIterator found no prompt"
                                    );
                                    break 'prompt;
                                }
                            }
                        };

                        // Clear cells from our start down. We replace it with spaces,
                        // and do not physically erase the rows (eraseRows) because the
                        // shell is going to expect this space to be available.
                        let mut it = start.row_iterator(pagelist::iter::Direction::RightDown, None);
                        while let Some(pin) = it.next() {
                            let page: *mut Page = unsafe { crate::pagelist::Node::page(pin.node) };
                            let row = pin.row_and_cell().row;
                            let cells = unsafe { (*page).get_cells(row) };
                            self.clear_cells(page, row, cells);
                        }
                    }
                }
            }
        }
    }
}
