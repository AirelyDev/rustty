//! PORTS: ghostty src/terminal/Terminal.zig — the `rowWillBeShifted` / `insertLines` / `deleteLines` / `insertBlanks` / `deleteChars` / `eraseChars` / `eraseLine` / `eraseDisplay` / `decaln` cluster.

use super::*;

impl Terminal {
    /// To be called before shifting a row (as in insertLines and deleteLines)
    ///
    /// Takes care of boundary conditions such as potentially split wide chars
    /// across scrolling region boundaries and orphaned spacer heads at line
    /// ends.
    // PORT NOTE (SOUNDNESS): the reference takes `page: *Page, row: *Row` and
    // re-reads that memory through `clearGrapheme`/`updateRowGraphemeFlag`
    // (which call `assertIntegrity`) while those pointers are live. Zig
    // pointers are UNPROTECTED, so we mirror them as raw `*mut Page`/`*mut Row`
    // rather than `&mut` to avoid Stacked-Borrows aliasing UB (the
    // clear_cells UB-fix rule). We reconstruct local mutable accesses only for
    // the brief field mutations, never holding a `&mut Page` across an
    // integrity check.
    fn row_will_be_shifted(&mut self, page: *mut Page, row: *mut Row) {
        // const cells = row.cells.ptr(page.memory.ptr);
        let cells: *mut pagepkg::Cell = unsafe { (*row).cells().ptr((*page).memory_ptr()) };

        // If our scrolling region includes the rightmost column then we
        // need to turn any spacer heads in to normal empty cells, since
        // once we move them they no longer correspond with soft-wrapped
        // wide characters.
        //
        // If it contains either of the 2 leftmost columns, then the wide
        // characters in the first column which may be associated with a
        // spacer head will be either moved or cleared, so we also need
        // to turn the spacer heads in to empty cells in that case.
        // if (self.scrolling_region.right == self.cols - 1 or
        //     self.scrolling_region.left < 2)
        if self.scrolling_region.right == self.cols - 1 || self.scrolling_region.left < 2 {
            // const end_cell: *Cell = &cells[page.size.cols - 1];
            let page_cols = unsafe { (*page).size.cols };
            let end_cell: *mut pagepkg::Cell = unsafe { cells.add((page_cols - 1) as usize) };
            // if (end_cell.wide == .spacer_head) {
            if unsafe { (*end_cell).wide() } == pagepkg::Wide::SpacerHead {
                // end_cell.wide = .narrow;
                unsafe { (*end_cell).set_wide(pagepkg::Wide::Narrow) };
            }
        }

        // If the leftmost or rightmost cells of our scrolling region
        // are parts of wide chars, we need to clear the cells' contents
        // since they'd be split by the move.
        // const left_cell: *Cell = &cells[self.scrolling_region.left];
        let left_cell: *mut pagepkg::Cell =
            unsafe { cells.add(self.scrolling_region.left as usize) };
        // const right_cell: *Cell = &cells[self.scrolling_region.right];
        let right_cell: *mut pagepkg::Cell =
            unsafe { cells.add(self.scrolling_region.right as usize) };

        // if (left_cell.wide == .spacer_tail) {
        if unsafe { (*left_cell).wide() } == pagepkg::Wide::SpacerTail {
            // const wide_cell: *Cell = &cells[self.scrolling_region.left - 1];
            let wide_cell: *mut pagepkg::Cell =
                unsafe { cells.add((self.scrolling_region.left - 1) as usize) };
            // if (wide_cell.hasGrapheme()) {
            if unsafe { (*wide_cell).has_grapheme() } {
                // page.clearGrapheme(wide_cell);
                unsafe { (*page).clear_grapheme(wide_cell) };
                // page.updateRowGraphemeFlag(row);
                unsafe { (*page).update_row_grapheme_flag(row) };
            }
            // wide_cell.content.codepoint = 0;
            unsafe { (*wide_cell).set_content_codepoint(0) };
            // wide_cell.wide = .narrow;
            unsafe { (*wide_cell).set_wide(pagepkg::Wide::Narrow) };
            // left_cell.wide = .narrow;
            unsafe { (*left_cell).set_wide(pagepkg::Wide::Narrow) };
        }

        // if (right_cell.wide == .wide) {
        if unsafe { (*right_cell).wide() } == pagepkg::Wide::Wide {
            // const tail_cell: *Cell = &cells[self.scrolling_region.right + 1];
            let tail_cell: *mut pagepkg::Cell =
                unsafe { cells.add((self.scrolling_region.right + 1) as usize) };
            // if (right_cell.hasGrapheme()) {
            if unsafe { (*right_cell).has_grapheme() } {
                // page.clearGrapheme(right_cell);
                unsafe { (*page).clear_grapheme(right_cell) };
                // page.updateRowGraphemeFlag(row);
                unsafe { (*page).update_row_grapheme_flag(row) };
            }
            // right_cell.content.codepoint = 0;
            unsafe { (*right_cell).set_content_codepoint(0) };
            // right_cell.wide = .narrow;
            unsafe { (*right_cell).set_wide(pagepkg::Wide::Narrow) };
            // tail_cell.wide = .narrow;
            unsafe { (*tail_cell).set_wide(pagepkg::Wide::Narrow) };
        }
    }

    /// Renew every live page generation in an inclusive range before a
    /// full-width line operation moves logical rows between their coordinates.
    ///
    /// PORTS: oracle@main Terminal.zig `invalidateFullWidthRowRange`
    /// (0bad91adb).
    //
    // PORT NOTE: raw-pointer node arguments, never `&mut Node` — the caller
    // holds live tracked-pin aliases into these nodes across the call (memory:
    // `rust-mut-args-strongly-protected`).
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    fn invalidate_full_width_row_range(
        &mut self,
        first: *mut crate::pagelist::Node,
        last: *mut crate::pagelist::Node,
    ) {
        let mut node = first;
        loop {
            // Full-width line movement remaps cached row coordinates in this page.
            unsafe { (*self.screens.active).pages.invalidate_node_layout(node) };
            if std::ptr::eq(node, last) {
                break;
            }
            node = unsafe { (*node).next };
        }
    }

    // TODO(qwerasd): `insertLines` and `deleteLines` are 99% identical,
    // the majority of their logic can (and should) be abstracted in to
    // a single shared helper function, probably on `Screen` not here.
    // I'm just too lazy to do that rn :p

    /// Insert amount lines at the current cursor row. The contents of the line
    /// at the current cursor row and below (to the bottom-most line in the
    /// scrolling region) are shifted down by amount lines. The contents of the
    /// amount bottom-most lines in the scroll region are lost.
    ///
    /// This unsets the pending wrap state without wrapping. If the current cursor
    /// position is outside of the current scroll region it does nothing.
    ///
    /// If amount is greater than the remaining number of lines in the scrolling
    /// region it is adjusted down (still allowing for scrolling out every remaining
    /// line in the scrolling region)
    ///
    /// In left and right margin mode the margins are respected; lines are only
    /// scrolled in the scroll region.
    ///
    /// All cleared space is colored according to the current SGR state.
    ///
    /// Moves the cursor to the left margin.
    pub fn insert_lines(&mut self, count: usize) {
        // Rare, but happens
        // if (count == 0) return;
        if count == 0 {
            return;
        }

        // If the cursor is outside the scroll region we do nothing.
        // if (self.screens.active.cursor.y < self.scrolling_region.top or
        //     self.screens.active.cursor.y > self.scrolling_region.bottom or
        //     self.screens.active.cursor.x < self.scrolling_region.left or
        //     self.screens.active.cursor.x > self.scrolling_region.right) return;
        if unsafe { (*self.screens.active).cursor.y } < self.scrolling_region.top
            || unsafe { (*self.screens.active).cursor.y } > self.scrolling_region.bottom
            || unsafe { (*self.screens.active).cursor.x } < self.scrolling_region.left
            || unsafe { (*self.screens.active).cursor.x } > self.scrolling_region.right
        {
            return;
        }

        // if (comptime build_options.kitty_graphics) {
        //     // Scrolling dirties the images because it updates their placements pins.
        //     self.screens.active.kitty_images.dirty = true;
        // }
        if crate::options::KITTY_GRAPHICS {
            // SAFETY: `self.screens.active` is a stable heap pointer owned by
            // `self.screens`; we hold `&mut self`.
            unsafe { (*self.screens.active).kitty_images.dirty = true };
        }

        // At the end we need to return the cursor to the row it started on.
        // const start_y = self.screens.active.cursor.y;
        let start_y = unsafe { (*self.screens.active).cursor.y };
        // defer {
        //     self.screens.active.cursorAbsolute(self.scrolling_region.left, start_y);
        //     // Always unset pending wrap
        //     self.screens.active.cursor.pending_wrap = false;
        // }
        // PORT NOTE: RAII guard reproduces the reference `defer`, restoring the
        // cursor to the left margin / start row and clearing pending_wrap on
        // every exit path.
        let _restore = ShiftLinesEndGuard {
            screen: self.screens.active,
            left: self.scrolling_region.left,
            start_y,
        };

        // We have a slower path if we have left or right scroll margins.
        // const left_right = self.scrolling_region.left > 0 or
        //     self.scrolling_region.right < self.cols - 1;
        let left_right =
            self.scrolling_region.left > 0 || self.scrolling_region.right < self.cols - 1;

        // Remaining rows from our cursor to the bottom of the scroll region.
        // const rem = self.scrolling_region.bottom - self.screens.active.cursor.y + 1;
        let rem = self.scrolling_region.bottom - unsafe { (*self.screens.active).cursor.y } + 1;

        // We can only insert lines up to our remaining lines in the scroll
        // region. So we take whichever is smaller.
        // const adjusted_count = @min(count, rem);
        let adjusted_count = core::cmp::min(count, rem as usize) as size::CellCountInt;

        // Create a new tracked pin which we'll use to navigate the page list
        // so that if we need to adjust capacity it will be properly tracked.
        // var cur_p = self.screens.active.pages.trackPin(
        //     self.screens.active.cursor.page_pin.down(rem - 1).?,
        // ) catch |err| { ... @panic("insertLines trackPin OOM"); };
        // defer self.screens.active.pages.untrackPin(cur_p);
        let down_pin =
            unsafe { (*(*self.screens.active).cursor.page_pin).down((rem - 1) as usize) }.unwrap();
        let cur_p: *mut crate::pagelist::Pin = match self.active_mut().pages.track_pin(down_pin) {
            Ok(p) => p,
            Err(err) => {
                // comptime assert(@TypeOf(err) == error{OutOfMemory});
                //
                // This error scenario means that our GPA is OOM. This is not a
                // situation we can gracefully handle. We can't just ignore insertLines
                // because it'll result in a corrupted screen. Ideally in the future
                // we flag the state as broken and show an error message to the user.
                // For now, we panic.
                log::error!(target: LOG_SCOPE, "insertLines trackPin error err={err:?}");
                panic!("insertLines trackPin OOM");
            }
        };
        // PORT NOTE: RAII guard reproduces `defer untrackPin(cur_p)`.
        let _untrack = TrackedPinGuard {
            screen: self.screens.active,
            pin: cur_p,
        };

        // Partial-width margins edit cells in stable rows; full-width moves rows.
        if !left_right {
            self.invalidate_full_width_row_range(
                unsafe { (*(*self.screens.active).cursor.page_pin).node },
                unsafe { (*cur_p).node },
            );
        }

        // Our current y position relative to the cursor
        // var y: usize = rem;
        let mut y: usize = rem as usize;

        // Traverse from the bottom up
        // while (y > 0) {
        while y > 0 {
            // const cur_rac = cur_p.rowAndCell();
            // const cur_row: *Row = cur_rac.row;
            let cur_row: *mut Row = unsafe { (*cur_p).row_and_cell() }.row;

            // If this is one of the lines we need to shift, do so
            // if (y > adjusted_count) {
            if y > adjusted_count as usize {
                // const off_p = cur_p.up(adjusted_count).?;
                let off_p = unsafe { (*cur_p).up(adjusted_count as usize) }.unwrap();
                // const off_rac = off_p.rowAndCell();
                // const off_row: *Row = off_rac.row;
                let off_row: *mut Row = off_p.row_and_cell().row;

                // self.rowWillBeShifted(&cur_p.node.data, cur_row);
                let cur_page: *mut Page = unsafe { crate::pagelist::Node::page((*cur_p).node) };
                self.row_will_be_shifted(cur_page, cur_row);
                // self.rowWillBeShifted(&off_p.node.data, off_row);
                let off_page: *mut Page = unsafe { crate::pagelist::Node::page(off_p.node) };
                self.row_will_be_shifted(off_page, off_row);

                // If our scrolling region is full width, then we unset wrap.
                // if (!left_right) {
                if !left_right {
                    // off_row.wrap = false;
                    unsafe { (*off_row).set_wrap(false) };
                    // cur_row.wrap = false;
                    unsafe { (*cur_row).set_wrap(false) };
                    // off_row.wrap_continuation = false;
                    unsafe { (*off_row).set_wrap_continuation(false) };
                    // cur_row.wrap_continuation = false;
                    unsafe { (*cur_row).set_wrap_continuation(false) };
                }

                // const src_p = off_p;
                // const src_row = off_row;
                // const dst_p = cur_p;
                // const dst_row = cur_row;
                let src_p = off_p;
                let src_row = off_row;
                // dst_p is cur_p (the tracked pin), dst_row is cur_row.

                // If our page doesn't match, then we need to do a copy from
                // one page to another. This is the slow path.
                // if (src_p.node != dst_p.node) {
                if !std::ptr::eq(src_p.node, unsafe { (*cur_p).node }) {
                    // The copy may replace the destination node in order
                    // to increase its capacity. Our pins are tracked so
                    // they update automatically; we can discard the
                    // replacement because the remainder of this iteration
                    // only accesses rows through the pins.
                    let dst_node = unsafe { (*cur_p).node };
                    let dst_y = unsafe { (*cur_p).y } as usize;
                    let src_page: *mut Page = unsafe { crate::pagelist::Node::page(src_p.node) };
                    let x_start = self.scrolling_region.left as usize;
                    let x_end = (self.scrolling_region.right + 1) as usize;
                    let _ = self.active_mut().clone_partial_row_grow_capacity(
                        dst_node, dst_y, src_page, src_row, x_start, x_end,
                    );
                } else {
                    // if (!left_right) {
                    if !left_right {
                        // Swap the src/dst cells. This ensures that our dst gets the
                        // proper shifted rows and src gets non-garbage cell data that
                        // we can clear.
                        // const dst = dst_row.*;
                        // dst_row.* = src_row.*;
                        // src_row.* = dst;
                        // PORT NOTE: `Row` is `Copy`; the three-step swap of the
                        // pointed-to rows is `core::ptr::swap` (same memory effect,
                        // no overlap since src_p.node == dst_p.node but distinct rows).
                        unsafe { core::ptr::swap(cur_row, src_row) };

                        // Ensure what we did didn't corrupt the page
                        // cur_p.node.data.assertIntegrity();
                        let cur_page: *const Page =
                            unsafe { crate::pagelist::Node::page((*cur_p).node) };
                        unsafe { (*cur_page).assert_integrity() };
                    } else {
                        // Left/right scroll margins we have to
                        // copy cells, which is much slower...
                        // const page = &cur_p.node.data;
                        let page: *mut Page = unsafe { crate::pagelist::Node::page((*cur_p).node) };
                        // page.moveCells(src_row, self.scrolling_region.left,
                        //     dst_row, self.scrolling_region.left,
                        //     (self.scrolling_region.right - self.scrolling_region.left) + 1);
                        unsafe {
                            (*page).move_cells(
                                src_row,
                                self.scrolling_region.left as usize,
                                cur_row,
                                self.scrolling_region.left as usize,
                                ((self.scrolling_region.right - self.scrolling_region.left) + 1)
                                    as usize,
                            )
                        };
                    }
                }
            } else {
                // Clear the cells for this row, it has been shifted.
                // self.rowWillBeShifted(&cur_p.node.data, cur_row);
                let cur_page: *mut Page = unsafe { crate::pagelist::Node::page((*cur_p).node) };
                self.row_will_be_shifted(cur_page, cur_row);
                // const page = &cur_p.node.data;
                let page: *mut Page = unsafe { crate::pagelist::Node::page((*cur_p).node) };
                // const cells = page.getCells(cur_row);
                let cells: *mut [pagepkg::Cell] = unsafe { (*page).get_cells(cur_row) };
                // self.screens.active.clearCells(page, cur_row,
                //     cells[self.scrolling_region.left .. self.scrolling_region.right + 1]);
                let sub = ptr_slice_range(
                    cells,
                    self.scrolling_region.left as usize,
                    (self.scrolling_region.right + 1) as usize,
                );
                self.active_mut().clear_cells(page, cur_row, sub);

                // With a full-width scroll region the entire row is a
                // fresh blank row: reset the metadata so nothing (wrap
                // state, semantic prompt) is retained from the row whose
                // storage it recycles. With left/right margins the row
                // keeps content outside the margins so the metadata is
                // preserved, matching the shift case above.
                // if (!left_right) cur_row.reset();
                if !left_right {
                    unsafe { (*cur_row).reset() };
                }
            }

            // Mark the row as dirty
            // cur_p.markDirty();
            unsafe { (*cur_p).mark_dirty() };

            // We have successfully processed a line.
            // y -= 1;
            y -= 1;
            // Move our pin up to the next row.
            // if (cur_p.up(1)) |p| cur_p.* = p;
            if let Some(p) = unsafe { (*cur_p).up(1) } {
                unsafe { *cur_p = p };
            }
        }
    }

    /// Removes amount lines from the current cursor row down. The remaining lines
    /// to the bottom margin are shifted up and space from the bottom margin up is
    /// filled with empty lines.
    ///
    /// If the current cursor position is outside of the current scroll region it
    /// does nothing. If amount is greater than the remaining number of lines in the
    /// scrolling region it is adjusted down.
    ///
    /// In left and right margin mode the margins are respected; lines are only
    /// scrolled in the scroll region.
    ///
    /// If the cell movement splits a multi cell character that character cleared,
    /// by replacing it by spaces, keeping its current attributes. All other
    /// cleared space is colored according to the current SGR state.
    ///
    /// Moves the cursor to the left margin.
    pub fn delete_lines(&mut self, count: usize) {
        // Rare, but happens
        // if (count == 0) return;
        if count == 0 {
            return;
        }

        // If the cursor is outside the scroll region we do nothing.
        // if (self.screens.active.cursor.y < self.scrolling_region.top or
        //     self.screens.active.cursor.y > self.scrolling_region.bottom or
        //     self.screens.active.cursor.x < self.scrolling_region.left or
        //     self.screens.active.cursor.x > self.scrolling_region.right) return;
        if unsafe { (*self.screens.active).cursor.y } < self.scrolling_region.top
            || unsafe { (*self.screens.active).cursor.y } > self.scrolling_region.bottom
            || unsafe { (*self.screens.active).cursor.x } < self.scrolling_region.left
            || unsafe { (*self.screens.active).cursor.x } > self.scrolling_region.right
        {
            return;
        }

        // if (comptime build_options.kitty_graphics) {
        //     // Scrolling dirties the images because it updates their placements pins.
        //     self.screens.active.kitty_images.dirty = true;
        // }
        if crate::options::KITTY_GRAPHICS {
            // SAFETY: `self.screens.active` is a stable heap pointer owned by
            // `self.screens`; we hold `&mut self`.
            unsafe { (*self.screens.active).kitty_images.dirty = true };
        }

        // At the end we need to return the cursor to the row it started on.
        // const start_y = self.screens.active.cursor.y;
        let start_y = unsafe { (*self.screens.active).cursor.y };
        // defer {
        //     self.screens.active.cursorAbsolute(self.scrolling_region.left, start_y);
        //     // Always unset pending wrap
        //     self.screens.active.cursor.pending_wrap = false;
        // }
        // PORT NOTE: RAII guard reproduces the reference `defer` (see insertLines).
        let _restore = ShiftLinesEndGuard {
            screen: self.screens.active,
            left: self.scrolling_region.left,
            start_y,
        };

        // We have a slower path if we have left or right scroll margins.
        // const left_right = self.scrolling_region.left > 0 or
        //     self.scrolling_region.right < self.cols - 1;
        let left_right =
            self.scrolling_region.left > 0 || self.scrolling_region.right < self.cols - 1;

        // Remaining rows from our cursor to the bottom of the scroll region.
        // const rem = self.scrolling_region.bottom - self.screens.active.cursor.y + 1;
        let rem = self.scrolling_region.bottom - unsafe { (*self.screens.active).cursor.y } + 1;

        // We can only insert lines up to our remaining lines in the scroll
        // region. So we take whichever is smaller.
        // const adjusted_count = @min(count, rem);
        let adjusted_count = core::cmp::min(count, rem as usize) as size::CellCountInt;

        // Create a new tracked pin which we'll use to navigate the page list
        // so that if we need to adjust capacity it will be properly tracked.
        // var cur_p = self.screens.active.pages.trackPin(
        //     self.screens.active.cursor.page_pin.*,
        // ) catch |err| { ... @panic("deleteLines trackPin OOM"); };
        // defer self.screens.active.pages.untrackPin(cur_p);
        let cursor_pin = unsafe { *(*self.screens.active).cursor.page_pin };
        let cur_p: *mut crate::pagelist::Pin = match self.active_mut().pages.track_pin(cursor_pin) {
            Ok(p) => p,
            Err(err) => {
                // See insertLines
                // comptime assert(@TypeOf(err) == error{OutOfMemory});
                log::error!(target: LOG_SCOPE, "deleteLines trackPin error err={err:?}");
                panic!("deleteLines trackPin OOM");
            }
        };
        // PORT NOTE: RAII guard reproduces `defer untrackPin(cur_p)`.
        let _untrack = TrackedPinGuard {
            screen: self.screens.active,
            pin: cur_p,
        };

        // Partial-width margins edit cells in stable rows; full-width moves rows.
        if !left_right {
            let last = unsafe { (*cur_p).down((rem - 1) as usize) }.unwrap().node;
            self.invalidate_full_width_row_range(unsafe { (*cur_p).node }, last);
        }

        // Our current y position relative to the cursor
        // var y: usize = 0;
        let mut y: usize = 0;

        // Traverse from the top down
        // while (y < rem) {
        while y < rem as usize {
            // const cur_rac = cur_p.rowAndCell();
            // const cur_row: *Row = cur_rac.row;
            let cur_row: *mut Row = unsafe { (*cur_p).row_and_cell() }.row;

            // If this is one of the lines we need to shift, do so
            // if (y < rem - adjusted_count) {
            if y < (rem - adjusted_count) as usize {
                // const off_p = cur_p.down(adjusted_count).?;
                let off_p = unsafe { (*cur_p).down(adjusted_count as usize) }.unwrap();
                // const off_rac = off_p.rowAndCell();
                // const off_row: *Row = off_rac.row;
                let off_row: *mut Row = off_p.row_and_cell().row;

                // self.rowWillBeShifted(&cur_p.node.data, cur_row);
                let cur_page: *mut Page = unsafe { crate::pagelist::Node::page((*cur_p).node) };
                self.row_will_be_shifted(cur_page, cur_row);
                // self.rowWillBeShifted(&off_p.node.data, off_row);
                let off_page: *mut Page = unsafe { crate::pagelist::Node::page(off_p.node) };
                self.row_will_be_shifted(off_page, off_row);

                // If our scrolling region is full width, then we unset wrap.
                // if (!left_right) {
                if !left_right {
                    // off_row.wrap = false;
                    unsafe { (*off_row).set_wrap(false) };
                    // cur_row.wrap = false;
                    unsafe { (*cur_row).set_wrap(false) };
                    // off_row.wrap_continuation = false;
                    unsafe { (*off_row).set_wrap_continuation(false) };
                    // cur_row.wrap_continuation = false;
                    unsafe { (*cur_row).set_wrap_continuation(false) };
                }

                // const src_p = off_p;
                // const src_row = off_row;
                // const dst_p = cur_p;
                // const dst_row = cur_row;
                let src_p = off_p;
                let src_row = off_row;
                // dst_p is cur_p (the tracked pin), dst_row is cur_row.

                // If our page doesn't match, then we need to do a copy from
                // one page to another. This is the slow path.
                // if (src_p.node != dst_p.node) {
                if !std::ptr::eq(src_p.node, unsafe { (*cur_p).node }) {
                    // The copy may replace the destination node in order
                    // to increase its capacity. Our pins are tracked so
                    // they update automatically; we can discard the
                    // replacement because the remainder of this iteration
                    // only accesses rows through the pins.
                    let dst_node = unsafe { (*cur_p).node };
                    let dst_y = unsafe { (*cur_p).y } as usize;
                    let src_page: *mut Page = unsafe { crate::pagelist::Node::page(src_p.node) };
                    let x_start = self.scrolling_region.left as usize;
                    let x_end = (self.scrolling_region.right + 1) as usize;
                    let _ = self.active_mut().clone_partial_row_grow_capacity(
                        dst_node, dst_y, src_page, src_row, x_start, x_end,
                    );
                } else {
                    // if (!left_right) {
                    if !left_right {
                        // Swap the src/dst cells. This ensures that our dst gets the
                        // proper shifted rows and src gets non-garbage cell data that
                        // we can clear.
                        // const dst = dst_row.*;
                        // dst_row.* = src_row.*;
                        // src_row.* = dst;
                        // PORT NOTE: see insertLines — `core::ptr::swap` of the rows.
                        unsafe { core::ptr::swap(cur_row, src_row) };

                        // Ensure what we did didn't corrupt the page
                        // cur_p.node.data.assertIntegrity();
                        let cur_page: *const Page =
                            unsafe { crate::pagelist::Node::page((*cur_p).node) };
                        unsafe { (*cur_page).assert_integrity() };
                    } else {
                        // Left/right scroll margins we have to
                        // copy cells, which is much slower...
                        // const page = &cur_p.node.data;
                        let page: *mut Page = unsafe { crate::pagelist::Node::page((*cur_p).node) };
                        // page.moveCells(src_row, self.scrolling_region.left,
                        //     dst_row, self.scrolling_region.left,
                        //     (self.scrolling_region.right - self.scrolling_region.left) + 1);
                        unsafe {
                            (*page).move_cells(
                                src_row,
                                self.scrolling_region.left as usize,
                                cur_row,
                                self.scrolling_region.left as usize,
                                ((self.scrolling_region.right - self.scrolling_region.left) + 1)
                                    as usize,
                            )
                        };
                    }
                }
            } else {
                // Clear the cells for this row, it's from out of bounds.
                // self.rowWillBeShifted(&cur_p.node.data, cur_row);
                let cur_page: *mut Page = unsafe { crate::pagelist::Node::page((*cur_p).node) };
                self.row_will_be_shifted(cur_page, cur_row);
                // const page = &cur_p.node.data;
                let page: *mut Page = unsafe { crate::pagelist::Node::page((*cur_p).node) };
                // const cells = page.getCells(cur_row);
                let cells: *mut [pagepkg::Cell] = unsafe { (*page).get_cells(cur_row) };
                // self.screens.active.clearCells(page, cur_row,
                //     cells[self.scrolling_region.left .. self.scrolling_region.right + 1]);
                let sub = ptr_slice_range(
                    cells,
                    self.scrolling_region.left as usize,
                    (self.scrolling_region.right + 1) as usize,
                );
                self.active_mut().clear_cells(page, cur_row, sub);

                // With a full-width scroll region the entire row is a
                // fresh blank row: reset the metadata so nothing (wrap
                // state, semantic prompt) is retained from the row whose
                // storage it recycles. With left/right margins the row
                // keeps content outside the margins so the metadata is
                // preserved, matching the shift case above.
                // if (!left_right) cur_row.reset();
                if !left_right {
                    unsafe { (*cur_row).reset() };
                }
            }

            // Mark the row as dirty
            // cur_p.markDirty();
            unsafe { (*cur_p).mark_dirty() };

            // We have successfully processed a line.
            // y += 1;
            y += 1;
            // Move our pin down to the next row.
            // if (cur_p.down(1)) |p| cur_p.* = p;
            if let Some(p) = unsafe { (*cur_p).down(1) } {
                unsafe { *cur_p = p };
            }
        }
    }

    /// Inserts spaces at current cursor position moving existing cell contents
    /// to the right. The contents of the count right-most columns in the scroll
    /// region are lost. The cursor position is not changed.
    ///
    /// This unsets the pending wrap state without wrapping.
    ///
    /// The inserted cells are colored according to the current SGR state.
    pub fn insert_blanks(&mut self, count: usize) {
        // Unset pending wrap state without wrapping. Note: this purposely
        // happens BEFORE the scroll region check below, because that's what
        // xterm does.
        // self.screens.active.cursor.pending_wrap = false;
        unsafe { (*self.screens.active).cursor.pending_wrap = false };

        // If we're given a zero then we do nothing. The rest of this function
        // assumes count > 0 and will crash if zero so return early. Note that
        // this shouldn't be possible with real CSI sequences because the value
        // is clamped to 1 min.
        // if (count == 0) return;
        if count == 0 {
            return;
        }

        // If our cursor is outside the margins then do nothing. We DO reset
        // wrap state still so this must remain below the above logic.
        // if (self.screens.active.cursor.x < self.scrolling_region.left or
        //     self.screens.active.cursor.x > self.scrolling_region.right) return;
        if unsafe { (*self.screens.active).cursor.x } < self.scrolling_region.left
            || unsafe { (*self.screens.active).cursor.x } > self.scrolling_region.right
        {
            return;
        }

        // If our count is larger than the remaining amount, we just erase right.
        // We only do this if we can erase the entire line (no right margin).
        // if (right_limit == self.cols and
        //     count > right_limit - self.screens.active.cursor.x)
        // {
        //     self.eraseLine(.right, false);
        //     return;
        // }

        // left is just the cursor position but as a multi-pointer
        // const left: [*]Cell = @ptrCast(self.screens.active.cursor.page_cell);
        let left: *mut pagepkg::Cell = unsafe { (*self.screens.active).cursor.page_cell };
        // var page = &self.screens.active.cursor.page_pin.node.data;
        let page: *mut Page =
            unsafe { crate::pagelist::Node::page((*(*self.screens.active).cursor.page_pin).node) };

        // If our X is a wide spacer tail then we need to erase the
        // previous cell too so we don't split a multi-cell character.
        // if (self.screens.active.cursor.page_cell.wide == .spacer_tail) {
        if unsafe { (*(*self.screens.active).cursor.page_cell).wide() } == pagepkg::Wide::SpacerTail
        {
            // assert(self.screens.active.cursor.x > 0);
            debug_assert!(unsafe { (*self.screens.active).cursor.x } > 0);
            // self.screens.active.clearCells(page, self.screens.active.cursor.page_row, (left - 1)[0..2]);
            let row: *mut Row = unsafe { (*self.screens.active).cursor.page_row };
            let cells = core::ptr::slice_from_raw_parts_mut(unsafe { left.sub(1) }, 2);
            self.active_mut().clear_cells(page, row, cells);
        }

        // Remaining cols from our cursor to the right margin.
        // const rem = self.scrolling_region.right - self.screens.active.cursor.x + 1;
        let rem = self.scrolling_region.right - unsafe { (*self.screens.active).cursor.x } + 1;

        // If the cell at the right margin is wide, its spacer tail is
        // outside the scroll region and would be orphaned by either the
        // shift or the clear. Clean up both halves up front.
        {
            // const right_cell: *Cell = @ptrCast(left + (rem - 1));
            let right_cell: *mut pagepkg::Cell = unsafe { left.add((rem - 1) as usize) };
            // if (right_cell.wide == .wide) self.screens.active.clearCells(
            //     page, self.screens.active.cursor.page_row,
            //     @as([*]Cell, @ptrCast(right_cell))[0..2]);
            if unsafe { (*right_cell).wide() } == pagepkg::Wide::Wide {
                let row: *mut Row = unsafe { (*self.screens.active).cursor.page_row };
                let cells = core::ptr::slice_from_raw_parts_mut(right_cell, 2);
                self.active_mut().clear_cells(page, row, cells);
            }
        }

        // We can only insert blanks up to our remaining cols
        // const adjusted_count = @min(count, rem);
        let adjusted_count = core::cmp::min(count, rem as usize) as size::CellCountInt;

        // This is the amount of space at the right of the scroll region
        // that will NOT be blank, so we need to shift the correct cols right.
        // "scroll_amount" is the number of such cols.
        // const scroll_amount = rem - adjusted_count;
        let scroll_amount = rem - adjusted_count;
        // if (scroll_amount > 0) {
        if scroll_amount > 0 {
            // page.pauseIntegrityChecks(true);
            unsafe { (*page).pause_integrity_checks(true) };
            // defer page.pauseIntegrityChecks(false);

            // var x: [*]Cell = left + (scroll_amount - 1);
            let mut x: *mut pagepkg::Cell = unsafe { left.add((scroll_amount - 1) as usize) };

            // If our last cell we're shifting is wide, then we need to clear
            // it to be empty so we don't split the multi-cell char.
            // const end: *Cell = @ptrCast(x);
            let end: *mut pagepkg::Cell = x;
            // if (end.wide == .wide) {
            if unsafe { (*end).wide() } == pagepkg::Wide::Wide {
                // const end_multi: [*]Cell = @ptrCast(end);
                let end_multi: *mut pagepkg::Cell = end;
                // assert(end_multi[1].wide == .spacer_tail);
                debug_assert!(unsafe { (*end_multi.add(1)).wide() } == pagepkg::Wide::SpacerTail);
                // self.screens.active.clearCells(page, self.screens.active.cursor.page_row, end_multi[0..2]);
                let row: *mut Row = unsafe { (*self.screens.active).cursor.page_row };
                let cells = core::ptr::slice_from_raw_parts_mut(end_multi, 2);
                self.active_mut().clear_cells(page, row, cells);
            }

            // We work backwards so we don't overwrite data.
            // while (@intFromPtr(x) >= @intFromPtr(left)) : (x -= 1) {
            while (x as usize) >= (left as usize) {
                // const src: *Cell = @ptrCast(x);
                let src: *mut pagepkg::Cell = x;
                // const dst: *Cell = @ptrCast(x + adjusted_count);
                let dst: *mut pagepkg::Cell = unsafe { x.add(adjusted_count as usize) };
                // page.swapCells(src, dst);
                unsafe { (*page).swap_cells(src, dst) };
                // PORT NOTE: the reference `x -= 1` decrements a `[*]Cell` (one
                // Cell). On the final iteration `x == left`, so the next
                // decrement would form a pointer one-before-`left`; the loop then
                // exits on the condition. We mirror this without computing an
                // out-of-bounds pointer: decrement only while we won't underflow,
                // otherwise break (the condition `x >= left` would be false).
                if std::ptr::eq(x, left) {
                    break;
                }
                x = unsafe { x.sub(1) };
            }

            unsafe { (*page).pause_integrity_checks(false) };
        }

        // Insert blanks. The blanks preserve the background color.
        // self.screens.active.clearCells(page, self.screens.active.cursor.page_row, left[0..adjusted_count]);
        let row: *mut Row = unsafe { (*self.screens.active).cursor.page_row };
        let cells = core::ptr::slice_from_raw_parts_mut(left, adjusted_count as usize);
        self.active_mut().clear_cells(page, row, cells);

        // Our row is always dirty
        // self.screens.active.cursorMarkDirty();
        self.active_mut().cursor_mark_dirty();
    }

    /// Removes amount characters from the current cursor position to the right.
    /// The remaining characters are shifted to the left and space from the right
    /// margin is filled with spaces.
    ///
    /// If amount is greater than the remaining number of characters in the
    /// scrolling region, it is adjusted down.
    ///
    /// Does not change the cursor position.
    pub fn delete_chars(&mut self, count_req: usize) {
        if count_req == 0 {
            return;
        }

        // If our cursor is outside the margins then do nothing. We DO reset
        // wrap state still so this must remain below the above logic.
        if unsafe { (*self.screens.active).cursor.x } < self.scrolling_region.left
            || unsafe { (*self.screens.active).cursor.x } > self.scrolling_region.right
        {
            return;
        }

        // left is just the cursor position but as a multi-pointer
        // const left: [*]Cell = @ptrCast(self.screens.active.cursor.page_cell);
        let left: *mut pagepkg::Cell = unsafe { (*self.screens.active).cursor.page_cell };
        // var page = &self.screens.active.cursor.page_pin.node.data;
        let page: *mut Page =
            unsafe { crate::pagelist::Node::page((*(*self.screens.active).cursor.page_pin).node) };

        // Remaining cols from our cursor to the right margin.
        let rem = self.scrolling_region.right - unsafe { (*self.screens.active).cursor.x } + 1;

        // We can only insert blanks up to our remaining cols
        let count = core::cmp::min(count_req, rem as usize) as size::CellCountInt;

        let cursor_x = unsafe { (*self.screens.active).cursor.x };
        let sr_right = self.scrolling_region.right;
        self.active_mut().split_cell_boundary(cursor_x);
        self.active_mut().split_cell_boundary(cursor_x + count);
        self.active_mut().split_cell_boundary(sr_right + 1);

        // This is the amount of space at the right of the scroll region
        // that will NOT be blank, so we need to shift the correct cols right.
        // "scroll_amount" is the number of such cols.
        let scroll_amount = rem - count;
        // var x: [*]Cell = left;
        let mut x: *mut pagepkg::Cell = left;
        if scroll_amount > 0 {
            unsafe { (*page).pause_integrity_checks(true) };
            // defer page.pauseIntegrityChecks(false);

            // const right: [*]Cell = left + (scroll_amount - 1);
            let right: *mut pagepkg::Cell = unsafe { left.add((scroll_amount - 1) as usize) };

            // while (@intFromPtr(x) <= @intFromPtr(right)) : (x += 1) {
            while (x as usize) <= (right as usize) {
                // const src: *Cell = @ptrCast(x + count);
                let src: *mut pagepkg::Cell = unsafe { x.add(count as usize) };
                // const dst: *Cell = @ptrCast(x);
                let dst: *mut pagepkg::Cell = x;
                unsafe { (*page).swap_cells(src, dst) };
                x = unsafe { x.add(1) };
            }

            unsafe { (*page).pause_integrity_checks(false) };
        }

        // Insert blanks. The blanks preserve the background color.
        // self.screens.active.clearCells(page, ..., x[0 .. rem - scroll_amount]);
        let row: *mut Row = unsafe { (*self.screens.active).cursor.page_row };
        let cells = core::ptr::slice_from_raw_parts_mut(x, (rem - scroll_amount) as usize);
        self.active_mut().clear_cells(page, row, cells);

        // Our row's soft-wrap is always reset.
        self.active_mut().cursor_reset_wrap();

        // Our row is always dirty
        self.active_mut().cursor_mark_dirty();
    }

    pub fn erase_chars(&mut self, count_req: usize) {
        let count = {
            let remaining = self.cols - unsafe { (*self.screens.active).cursor.x };
            // var end = @min(remaining, @max(count_req, 1));
            // PORT NOTE (VALIDATION_REPORT §3c.8): Zig's peer-type resolution
            // runs the `@min`/`@max` in usize and only then uses the (≤
            // remaining, so ≤ u16::MAX) result in u16 contexts. Clamp in usize
            // FIRST, then narrow — truncating `count_req` to u16 before the min
            // would mangle counts > u16::MAX (e.g. 65537 → 1), reachable via
            // this pub API / FFI.
            let mut end = core::cmp::min(remaining as usize, core::cmp::max(count_req, 1))
                as size::CellCountInt;

            // If our last cell is a wide char then we need to also clear the
            // cell beyond it since we can't just split a wide char.
            if end != remaining {
                let last = self.active_mut().cursor_cell_right(end - 1);
                if unsafe { (*last).wide() } == pagepkg::Wide::Wide {
                    end += 1;
                }
            }

            end
        };

        // Handle any boundary conditions on the edges of the erased area.
        //
        // TODO(qwerasd): This isn't actually correct if you take in to account
        // protected modes. We need to figure out how to make `clearCells` or at
        // least `clearUnprotectedCells` handle boundary conditions...
        let cursor_x = unsafe { (*self.screens.active).cursor.x };
        self.active_mut().split_cell_boundary(cursor_x);
        self.active_mut().split_cell_boundary(cursor_x + count);

        // Reset our row's soft-wrap.
        self.active_mut().cursor_reset_wrap();

        // Mark our cursor row as dirty
        self.active_mut().cursor_mark_dirty();

        // Clear the cells
        // const cells: [*]Cell = @ptrCast(self.screens.active.cursor.page_cell);
        let cells: *mut pagepkg::Cell = unsafe { (*self.screens.active).cursor.page_cell };

        let page: *mut Page =
            unsafe { crate::pagelist::Node::page((*(*self.screens.active).cursor.page_pin).node) };
        let row: *mut Row = unsafe { (*self.screens.active).cursor.page_row };
        let cells_slice = core::ptr::slice_from_raw_parts_mut(cells, count as usize);

        // If we never had a protection mode, then we can assume no cells
        // are protected and go with the fast path. If the last protection
        // mode was not ISO we also always ignore protection attributes.
        if unsafe { (*self.screens.active).protected_mode } != ansi::ProtectedMode::Iso {
            self.active_mut().clear_cells(page, row, cells_slice);
            return;
        }

        self.active_mut()
            .clear_unprotected_cells(page, row, cells_slice);
    }

    /// Erase the line.
    pub fn erase_line(&mut self, mode: csi::EraseLine, protected_req: bool) {
        // Get our start/end positions depending on mode.
        let (start, end): (size::CellCountInt, size::CellCountInt) = match mode {
            csi::EraseLine::Right => {
                let mut x = unsafe { (*self.screens.active).cursor.x };

                // If our X is a wide spacer tail then we need to erase the
                // previous cell too so we don't split a multi-cell character.
                if x > 0
                    && unsafe { (*(*self.screens.active).cursor.page_cell).wide() }
                        == pagepkg::Wide::SpacerTail
                {
                    x -= 1;
                }

                // Reset our row's soft-wrap.
                self.active_mut().cursor_reset_wrap();

                (x, self.cols)
            }

            csi::EraseLine::Left => {
                let mut x = unsafe { (*self.screens.active).cursor.x };

                // If our x is a wide char we need to delete the tail too.
                if unsafe { (*(*self.screens.active).cursor.page_cell).wide() }
                    == pagepkg::Wide::Wide
                {
                    x += 1;
                }

                (0, x + 1)
            }

            // .complete => complete: { ... }
            csi::EraseLine::Complete => {
                // Xterm preserves this flag for EL2, but it also doesn't reflow
                // rows when resizing. Since we do, the erased row must no longer
                // continue onto the next row.
                self.active_mut().cursor_reset_wrap();

                (0, self.cols)
            }

            _ => {
                log::error!(target: LOG_SCOPE, "unimplemented erase line mode: {mode:?}");
                return;
            }
        };

        // All modes will clear the pending wrap state and we know we have
        // a valid mode at this point.
        unsafe { (*self.screens.active).cursor.pending_wrap = false };

        // We always mark our row as dirty
        self.active_mut().cursor_mark_dirty();

        // Start of our cells
        // const cells: [*]Cell = cells - self.screens.active.cursor.x;
        let cells: *mut pagepkg::Cell = {
            let cells: *mut pagepkg::Cell = unsafe { (*self.screens.active).cursor.page_cell };
            let x = unsafe { (*self.screens.active).cursor.x };
            unsafe { cells.sub(x as usize) }
        };

        // We respect protected attributes if explicitly requested (probably
        // a DECSEL sequence) or if our last protected mode was ISO even if its
        // not currently set.
        let protected = unsafe { (*self.screens.active).protected_mode }
            == ansi::ProtectedMode::Iso
            || protected_req;

        let page: *mut Page =
            unsafe { crate::pagelist::Node::page((*(*self.screens.active).cursor.page_pin).node) };
        let row: *mut Row = unsafe { (*self.screens.active).cursor.page_row };
        // cells[start..end]
        let cells_slice = core::ptr::slice_from_raw_parts_mut(
            unsafe { cells.add(start as usize) },
            (end - start) as usize,
        );

        // If we're not respecting protected attributes, we can use a fast-path
        // to fill the entire line.
        if !protected {
            self.active_mut().clear_cells(page, row, cells_slice);
            return;
        }

        self.active_mut()
            .clear_unprotected_cells(page, row, cells_slice);
    }

    /// Clear the active screen's Kitty graphics state for a terminal clear
    /// (the reference's `kitty_images.clearScreen(...)`; upstream `f8b40a023`
    /// replaced the old `delete(.., .{ .all = true })` here so ED2 preserves
    /// scrollback references while reclaiming every unreferenced image).
    ///
    /// PORT NOTE: the reference writes this inline as
    /// `self.screens.active.kitty_images.clearScreen(self.io(), self.screens.active.alloc, self)`,
    /// which aliases the storage field and the whole terminal at once. Rust
    /// cannot, so — exactly as `kitty::graphics_exec::delete` and
    /// `set_kitty_graphics_size_limit` already do — the storage is MOVED out of
    /// the screen, `clear_screen` runs against the borrowed terminal, and the
    /// storage is moved back. `ImageStorage::clear_screen` only reaches the
    /// active screen itself through `t` (never its `kitty_images` slot), so the
    /// temporarily-defaulted slot is never observed.
    fn clear_screen_active_kitty_images(&mut self) {
        // SAFETY: `self.screens.active` is a stable heap pointer owned by
        // `self.screens`; we hold `&mut self`, so no other alias exists.
        let alloc = unsafe { (*self.screens.active).alloc };
        let mut storage = core::mem::take(unsafe { &mut (*self.screens.active).kitty_images });
        storage.clear_screen(alloc, self);
        unsafe { (*self.screens.active).kitty_images = storage };
    }

    /// Erase the display.
    pub fn erase_display(&mut self, mode: csi::EraseDisplay, protected_req: bool) {
        // We respect protected attributes if explicitly requested (probably
        // a DECSEL sequence) or if our last protected mode was ISO even if its
        // not currently set.
        let protected = unsafe { (*self.screens.active).protected_mode }
            == ansi::ProtectedMode::Iso
            || protected_req;

        match mode {
            csi::EraseDisplay::ScrollComplete => {
                // self.screens.active.scrollClear() catch |err| { ... };
                if let Err(err) = self.active_mut().scroll_clear() {
                    log::warn!(target: LOG_SCOPE, "scroll clear failed, doing a normal clear err={err:?}");
                    self.erase_display(csi::EraseDisplay::Complete, protected_req);
                    return;
                }

                // Unsets pending wrap state
                unsafe { (*self.screens.active).cursor.pending_wrap = false };

                // if (comptime build_options.kitty_graphics) {
                //     // Clear only placements still visible after moving the active
                //     // area into scrollback.
                //     self.screens.active.kitty_images.clearScreen(
                //         self.io(),
                //         self.screens.active.alloc,
                //         self,
                //     );
                // }
                if crate::options::KITTY_GRAPHICS {
                    // Clear only placements still visible after moving the active
                    // area into scrollback.
                    self.clear_screen_active_kitty_images();
                }
            }

            csi::EraseDisplay::Complete => {
                // If we're on the primary screen and our last non-empty row is
                // a prompt, then we do a scroll_complete instead. This is a
                // heuristic to get the generally desirable behavior that ^L
                // at a prompt scrolls the screen contents prior to clearing.
                // Most shells send `ESC [ H ESC [ 2 J` so we can't just check
                // our current cursor position. See #905
                if self.screens.active_key == crate::screen_set::Key::Primary {
                    'at_prompt: {
                        // Go from the bottom of the active up and see if we're
                        // at a prompt.
                        let active_br =
                            match self.active_mut().pages.get_bottom_right(point::Tag::Active) {
                                Some(p) => p,
                                None => break 'at_prompt,
                            };
                        let tl = self.active_mut().pages.get_top_left(point::Tag::Active);
                        let mut it = active_br
                            .row_iterator(crate::pagelist::iter::Direction::LeftUp, Some(tl));
                        // while (it.next()) |p| { switch (...) { .prompt,
                        // .prompt_continuation => break, .none => break :at_prompt } }
                        // else break :at_prompt;
                        // PORT NOTE: Zig's `while (cond) {...} else {...}` runs the
                        // `else` only on normal loop exhaustion. We mirror it: a
                        // `found` flag set true only on the prompt/continuation
                        // `break`; any other exit (`.none` or exhaustion) leaves it
                        // false and falls through to `break 'at_prompt`.
                        let mut found = false;
                        // PORT NOTE: every match arm `break`s, so this `while let`
                        // only ever runs one iteration (the reference `while` is
                        // identical: the bottom-most active row decides). We keep
                        // the loop form to mirror the reference `while...else`
                        // exhaustion semantics and allow the resulting lint.
                        #[allow(clippy::never_loop)]
                        while let Some(p) = it.next() {
                            let row = p.row_and_cell().row;
                            match unsafe { (*row).semantic_prompt() } {
                                // If we're at a prompt or input area, then we are at a prompt.
                                pagepkg::SemanticPrompt::Prompt
                                | pagepkg::SemanticPrompt::PromptContinuation => {
                                    found = true;
                                    break;
                                }

                                // If we have command output, then we're most certainly not
                                // at a prompt.
                                pagepkg::SemanticPrompt::None => break,
                            }
                        }
                        if !found {
                            break 'at_prompt;
                        }

                        // self.screens.active.scrollClear() catch {};
                        // If we fail, we just fall back to doing a normal clear
                        // so we don't worry about the error.
                        let _ = self.active_mut().scroll_clear();
                    }
                }

                // All active area
                self.active_mut().clear_rows(
                    point::Point::Active(point::Coordinate::default()),
                    None,
                    protected,
                );

                // Unsets pending wrap state
                unsafe { (*self.screens.active).cursor.pending_wrap = false };

                // if (comptime build_options.kitty_graphics) {
                //     // ED2 clears visible placements but preserves graphics that
                //     // are wholly in scrollback.
                //     self.screens.active.kitty_images.clearScreen(
                //         self.io(),
                //         self.screens.active.alloc,
                //         self,
                //     );
                // }
                if crate::options::KITTY_GRAPHICS {
                    // ED2 clears visible placements but preserves graphics that
                    // are wholly in scrollback.
                    self.clear_screen_active_kitty_images();
                }

                // Cleared screen dirty bit
                self.flags.dirty.clear = true;
            }

            csi::EraseDisplay::Below => {
                // All lines to the right (including the cursor)
                self.erase_line(csi::EraseLine::Right, protected_req);

                // All lines below
                if unsafe { (*self.screens.active).cursor.y } + 1 < self.rows {
                    let y = unsafe { (*self.screens.active).cursor.y } + 1;
                    self.active_mut().clear_rows(
                        point::Point::Active(point::Coordinate { x: 0, y: y as u32 }),
                        None,
                        protected,
                    );
                }

                // Unsets pending wrap state. Should be done by eraseLine.
                debug_assert!(!unsafe { (*self.screens.active).cursor.pending_wrap });
            }

            csi::EraseDisplay::Above => {
                // Erase to the left (including the cursor)
                self.erase_line(csi::EraseLine::Left, protected_req);

                // All lines above
                if unsafe { (*self.screens.active).cursor.y } > 0 {
                    let y = unsafe { (*self.screens.active).cursor.y } - 1;
                    self.active_mut().clear_rows(
                        point::Point::Active(point::Coordinate::default()),
                        Some(point::Point::Active(point::Coordinate {
                            x: 0,
                            y: y as u32,
                        })),
                        protected,
                    );
                }

                // Unsets pending wrap state
                debug_assert!(!unsafe { (*self.screens.active).cursor.pending_wrap });
            }

            csi::EraseDisplay::Scrollback => self.active_mut().erase_history(None),
        }
    }

    /// Resets all margins and fills the whole screen with the character 'E'
    ///
    /// Sets the cursor to the top left corner.
    pub fn decaln(&mut self) -> Result<(), OutOfMemory> {
        // Clear our stylistic attributes. This is the only thing that can
        // fail so we do it first so we can undo it.
        let old_style = unsafe { (*self.screens.active).cursor.style };
        unsafe {
            (*self.screens.active).cursor.style = style::Style {
                bg_color: (*self.screens.active).cursor.style.bg_color,
                fg_color: (*self.screens.active).cursor.style.fg_color,
                ..style::Style::default()
            }
        };
        // errdefer self.screens.active.cursor.style = old_style;
        // PORT NOTE: the reference `errdefer` restores the saved style if
        // `manualStyleUpdate` fails. In Rust we restore explicitly on the error
        // path before returning.
        if self.active_mut().manual_style_update().is_err() {
            unsafe { (*self.screens.active).cursor.style = old_style };
            return Err(OutOfMemory);
        }

        // Reset margins, also sets cursor to top-left
        self.scrolling_region = ScrollingRegion {
            top: 0,
            bottom: self.rows - 1,
            left: 0,
            right: self.cols - 1,
        };

        // Origin mode is disabled
        self.modes.set(modespkg::Mode::Origin, false);

        // Move our cursor to the top-left
        self.set_cursor_pos(1, 1);

        // Use clearRows instead of eraseDisplay because we must NOT respect
        // protected attributes here.
        self.active_mut().clear_rows(
            point::Point::Active(point::Coordinate::default()),
            None,
            false,
        );

        // Fill with Es by moving the cursor but reset it after.
        loop {
            // const page = &self.screens.active.cursor.page_pin.node.data;
            let page: *mut Page = unsafe {
                crate::pagelist::Node::page((*(*self.screens.active).cursor.page_pin).node)
            };
            // const row = self.screens.active.cursor.page_row;
            let row: *mut Row = unsafe { (*self.screens.active).cursor.page_row };
            // const cells_multi: [*]Cell = row.cells.ptr(page.memory);
            // const cells = cells_multi[0..page.size.cols];
            let cells_offset = unsafe { (*row).cells() };
            let cells_multi: *mut pagepkg::Cell = unsafe { cells_offset.ptr((*page).memory_ptr()) };
            let cols = unsafe { (*page).size.cols } as usize;

            // @memset(cells, .{ ... });
            let style_id = unsafe { (*self.screens.active).cursor.style_id };
            for i in 0..cols {
                let cell: *mut pagepkg::Cell = unsafe { cells_multi.add(i) };
                let mut new_cell = pagepkg::Cell::zeroed();
                new_cell.set_content_tag(pagepkg::ContentTag::Codepoint);
                new_cell.set_content_codepoint('E' as u21);
                new_cell.set_style_id(style_id);
                // DECALN does not respect protected state. Verified with xterm.
                new_cell.set_protected(false);
                unsafe { *cell = new_cell };
            }

            // If we have a ref-counted style, increase
            if style_id != style::DEFAULT_ID {
                unsafe {
                    let base = (*page).memory_ptr();
                    (*page).styles.use_multiple(base, style_id, cols as _);
                }
                unsafe { (*row).set_styled(true) };
            }

            // We messed with the page so assert its integrity here.
            unsafe { (*page).assert_integrity() };

            self.active_mut().cursor_mark_dirty();
            if unsafe { (*self.screens.active).cursor.y } == self.rows - 1 {
                break;
            }
            self.active_mut().cursor_down(1);
        }

        // Reset the cursor to the top-left
        self.set_cursor_pos(1, 1);

        Ok(())
    }
}
