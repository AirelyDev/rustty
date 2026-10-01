//! PORTS: ghostty src/terminal/Screen.zig — the cursor cluster: `cursorCellRight`
//! through `cursorResetWrap` (cell/row movement, `cursorReload`,
//! `cursorDownScroll`, `cursorScrollAbove`, `cursorScrollRegionUp`,
//! `cursorDownOrScroll`, `cursorCopy`, `cursorChangePin`, `cursorMarkDirty`).

use super::*;

impl Screen {
    #[inline]
    pub fn cursor_cell_right(&mut self, n: size::CellCountInt) -> *mut pagepkg::Cell {
        debug_assert!(self.cursor.x + n < self.pages.cols);
        let cell: *mut pagepkg::Cell = self.cursor.page_cell;
        unsafe { cell.add(n as usize) }
    }

    #[inline]
    pub fn cursor_cell_left(&mut self, n: size::CellCountInt) -> *mut pagepkg::Cell {
        debug_assert!(self.cursor.x >= n);
        let cell: *mut pagepkg::Cell = self.cursor.page_cell;
        unsafe { cell.sub(n as usize) }
    }

    pub fn cursor_cell_end_of_prev(&mut self) -> *mut pagepkg::Cell {
        debug_assert!(self.cursor.y > 0);

        let mut page_pin = unsafe { (*self.cursor.page_pin).up(1) }.unwrap();
        // PORT NOTE (oracle a55850c98, `P3-W1 T4`): the column comes from the
        // page the pin actually REACHED, not from the desired screen width. If
        // incomplete reflow left that previous page narrower, the desired width
        // resolved an out-of-bounds column.
        page_pin.x = unsafe { crate::pagelist::Node::cols(page_pin.node) } - 1;
        let page_rac = page_pin.row_and_cell();
        page_rac.cell
    }

    /// Move the cursor right. This is a specialized function that is very fast
    /// if the caller can guarantee we have space to move right (no wrapping).
    pub fn cursor_right(&mut self, n: size::CellCountInt) {
        debug_assert!(self.cursor.x + n < self.pages.cols);

        let cell: *mut pagepkg::Cell = self.cursor.page_cell;
        self.cursor.page_cell = unsafe { cell.add(n as usize) };
        unsafe { (*self.cursor.page_pin).x += n };
        self.cursor.x += n;

        self.assert_integrity();
    }

    /// Move the cursor left.
    pub fn cursor_left(&mut self, n: size::CellCountInt) {
        debug_assert!(self.cursor.x >= n);

        let cell: *mut pagepkg::Cell = self.cursor.page_cell;
        self.cursor.page_cell = unsafe { cell.sub(n as usize) };
        unsafe { (*self.cursor.page_pin).x -= n };
        self.cursor.x -= n;

        self.assert_integrity();
    }

    /// Move the cursor up.
    ///
    /// Precondition: The cursor is not at the top of the screen.
    pub fn cursor_up(&mut self, n: size::CellCountInt) {
        debug_assert!(self.cursor.y >= n);

        self.cursor.y -= n; // Must be set before cursorChangePin
        let new = unsafe { (*self.cursor.page_pin).up(n as usize) }.unwrap();
        self.cursor_change_pin(new);
        let page_rac = unsafe { (*self.cursor.page_pin).row_and_cell() };
        self.cursor.page_row = page_rac.row;
        self.cursor.page_cell = page_rac.cell;

        self.assert_integrity();
    }

    pub fn cursor_row_up(&mut self, n: size::CellCountInt) -> *mut pagepkg::Row {
        debug_assert!(self.cursor.y >= n);

        let page_pin = unsafe { (*self.cursor.page_pin).up(n as usize) }.unwrap();
        let page_rac = page_pin.row_and_cell();

        self.assert_integrity();
        page_rac.row
    }

    /// Move the cursor down.
    ///
    /// Precondition: The cursor is not at the bottom of the screen.
    pub fn cursor_down(&mut self, n: size::CellCountInt) {
        debug_assert!(self.cursor.y + n < self.pages.rows);

        self.cursor.y += n; // Must be set before cursorChangePin

        // We move the offset into our page list to the next row and then
        // get the pointers to the row/cell and set all the cursor state up.
        let new = unsafe { (*self.cursor.page_pin).down(n as usize) }.unwrap();
        self.cursor_change_pin(new);
        let page_rac = unsafe { (*self.cursor.page_pin).row_and_cell() };
        self.cursor.page_row = page_rac.row;
        self.cursor.page_cell = page_rac.cell;

        self.assert_integrity();
    }

    /// Move the cursor to some absolute horizontal position.
    pub fn cursor_horizontal_absolute(&mut self, x: size::CellCountInt) {
        debug_assert!(x < self.pages.cols);

        unsafe { (*self.cursor.page_pin).x = x };
        let page_rac = unsafe { (*self.cursor.page_pin).row_and_cell() };
        self.cursor.page_cell = page_rac.cell;
        self.cursor.x = x;

        self.assert_integrity();
    }

    /// Move the cursor to some absolute position.
    pub fn cursor_absolute(&mut self, x: size::CellCountInt, y: size::CellCountInt) {
        debug_assert!(x < self.pages.cols);
        debug_assert!(y < self.pages.rows);

        let mut page_pin = if y < self.cursor.y {
            unsafe { (*self.cursor.page_pin).up((self.cursor.y - y) as usize) }.unwrap()
        } else if y > self.cursor.y {
            unsafe { (*self.cursor.page_pin).down((y - self.cursor.y) as usize) }.unwrap()
        } else {
            unsafe { *self.cursor.page_pin }
        };
        page_pin.x = x;
        self.cursor.x = x; // Must be set before cursorChangePin
        self.cursor.y = y;
        self.cursor_change_pin(page_pin);
        let page_rac = unsafe { (*self.cursor.page_pin).row_and_cell() };
        self.cursor.page_row = page_rac.row;
        self.cursor.page_cell = page_rac.cell;

        self.assert_integrity();
    }

    /// Reloads the cursor pointer information into the screen. This is expensive
    /// so it should only be done in cases where the pointers are invalidated
    /// in such a way that its difficult to recover otherwise.
    pub fn cursor_reload(&mut self) {
        // Our tracked pin is ALWAYS accurate, so we derive the active
        // point from the pin. If this returns null it means our pin
        // points outside the active area. In that case, we update the
        // pin to be the top-left.
        let pt: point::Point = match self
            .pages
            .point_from_pin(point::Tag::Active, unsafe { *self.cursor.page_pin })
        {
            Some(pt) => pt,
            None => {
                // Our cached row/cell pointers may be invalid (that is often
                // the reason cursorReload is being called), so refresh them
                // from the pin first since cursorChangePin below marks the
                // old cursor row as dirty.
                let old_rac = unsafe { (*self.cursor.page_pin).row_and_cell() };
                self.cursor.page_row = old_rac.row;
                self.cursor.page_cell = old_rac.cell;

                // The cursor style and hyperlink IDs are only valid within the
                // page that the pin points at, so the pin change must go through
                // cursorChangePin, which migrates them when the active top-left
                // is on a different page. Writing the pin directly here would
                // leave the cursor holding IDs that are dead or alias unrelated
                // entries on the new page.
                let pin = self
                    .pages
                    .pin(point::Point::Active(point::Coordinate::default()))
                    .unwrap();
                self.cursor.x = 0; // Must be set before cursorChangePin
                self.cursor.y = 0;
                self.cursor_change_pin(pin);

                // cursorChangePin can trigger a page capacity adjustment which
                // moves the pin again, so we re-read it to derive our point.
                self.pages
                    .point_from_pin(point::Tag::Active, unsafe { *self.cursor.page_pin })
                    .unwrap()
            }
        };

        self.cursor.x = pt.coord().x;
        self.cursor.y = pt.coord().y as size::CellCountInt;
        let page_rac = unsafe { (*self.cursor.page_pin).row_and_cell() };
        self.cursor.page_row = page_rac.row;
        self.cursor.page_cell = page_rac.cell;

        self.assert_integrity();
    }

    /// Scroll the active area and keep the cursor at the bottom of the screen.
    /// This is a very specialized function but it keeps it fast.
    pub fn cursor_down_scroll(&mut self) -> Result<(), OutOfMemory> {
        debug_assert!(self.cursor.y == self.pages.rows - 1);

        // if (comptime build_options.kitty_graphics) {
        //     // Scrolling dirties the images because it updates their placements pins.
        //     self.kitty_images.dirty = true;
        // }
        if crate::options::KITTY_GRAPHICS {
            // Scrolling dirties the images because it updates their placements pins.
            self.kitty_images.dirty = true;
        }

        // If we have no scrollback, then we shift all our rows instead.
        if self.no_scrollback {
            // If we have a single-row screen, we have no rows to shift
            // so our cursor is in the correct place we just have to clear
            // the cells.
            if self.pages.rows == 1 {
                let page: *mut Page =
                    unsafe { crate::pagelist::Node::page((*self.cursor.page_pin).node) };
                let row = self.cursor.page_row;
                let cells = unsafe { (*page).get_cells(row) };
                self.clear_cells(page, row, cells);

                // The row is a fresh blank row now and must not retain
                // metadata (wrap state, semantic prompt) from the
                // discarded content.
                // self.cursor.page_row.reset();
                unsafe { (*self.cursor.page_row).reset() };

                self.cursor_mark_dirty();
            } else {
                // The call to `eraseRow` will move the tracked cursor pin up by one
                // row, but we don't actually want that, so we keep the old pin and
                // put it back after calling `eraseRow`.
                let old_pin = unsafe { *self.cursor.page_pin };

                // eraseRow will shift everything below it up.
                self.pages
                    .erase_row(point::Point::Active(point::Coordinate::default()))?;

                // Note we don't need to mark anything dirty in this branch
                // because eraseRow will mark all the rotated rows as dirty
                // in the entire page.

                // We don't use `cursorChangePin` here because we aren't
                // actually changing the pin, we're keeping it the same.
                unsafe { *self.cursor.page_pin = old_pin };

                // We do, however, need to refresh the cached page row
                // and cell, because `eraseRow` will have moved the row.
                let page_rac = unsafe { (*self.cursor.page_pin).row_and_cell() };
                self.cursor.page_row = page_rac.row;
                self.cursor.page_cell = page_rac.cell;
            }
        } else {
            let old_pin = unsafe { *self.cursor.page_pin };

            // Grow our pages by one row. The PageList will handle if we need to
            // allocate, prune scrollback, whatever.
            let _ = self.pages.grow()?;

            // We do this all in a block here because referencing this pin
            // after cursorChangePin is unsafe, and we want to keep it out
            // of scope.
            let new_pin = {
                // Calculate this before cursorChangePin because that function may
                // adjust the underlying page and invalidate references to its pin.

                // If our pin page change it means that the page that the pin
                // was on was pruned. In this case, grow() moves the pin to
                // the top-left of the new page. This effectively moves it by
                // one already, we just need to fix up the x value.
                let page_pin =
                    if std::ptr::eq(old_pin.node, unsafe { (*self.cursor.page_pin).node }) {
                        unsafe { (*self.cursor.page_pin).down(1) }.unwrap()
                    } else {
                        let mut pin = unsafe { *self.cursor.page_pin };
                        pin.x = self.cursor.x;
                        pin
                    };

                // These assertions help catch some pagelist math errors. Our
                // x/y should be unchanged after the grow.
                if crate::options::SLOW_RUNTIME_SAFETY {
                    let active = self
                        .pages
                        .point_from_pin(point::Tag::Active, page_pin)
                        .unwrap()
                        .coord();
                    debug_assert!(active.x == self.cursor.x);
                    debug_assert!(active.y == self.cursor.y as u32);
                }

                page_pin
            };
            if std::ptr::eq(unsafe { (*self.cursor.page_pin).node }, new_pin.node) {
                // Scrolling normally stays within one page. Resolve the page once
                // while refreshing the cursor pointers. The cursor already holds
                // live row and cell pointers into this mapping, and grow does not
                // compress pages, so the node must still be resident here.
                self.cursor_mark_dirty();
                let page = unsafe { crate::pagelist::Node::page_assume_resident(new_pin.node) };
                let page_rac =
                    unsafe { (*page).get_row_and_cell(new_pin.x as usize, new_pin.y as usize) };
                unsafe { *self.cursor.page_pin = new_pin };
                self.cursor.page_row = page_rac.row;
                self.cursor.page_cell = page_rac.cell;
            } else {
                // Crossing a page may require migrating the cursor's style and
                // hyperlink references, so retain the general path here.
                self.cursor_change_pin(new_pin);
                let page_rac = unsafe { (*self.cursor.page_pin).row_and_cell() };
                self.cursor.page_row = page_rac.row;
                self.cursor.page_cell = page_rac.cell;
            }

            // Our new row is always dirty
            self.cursor_mark_dirty();

            // Clear the new row so it gets our bg color. We only do this
            // if we have a bg color at all.
            if !matches!(self.cursor.style.bg_color.tag(), style::ColorTag::None) {
                let page: *mut Page =
                    unsafe { crate::pagelist::Node::page((*self.cursor.page_pin).node) };
                let row = self.cursor.page_row;
                let cells = unsafe { (*page).get_cells(row) };
                self.clear_cells(page, row, cells);
            }
        }

        if self.cursor.style_id != style::DEFAULT_ID {
            // The newly created line needs to be styled according to
            // the bg color if it is set.
            if let Some(blank_cell) = self.cursor.style.bg_cell() {
                let cell_current: *mut pagepkg::Cell = self.cursor.page_cell;
                let cells = unsafe { cell_current.sub(self.cursor.x as usize) };
                unsafe {
                    core::slice::from_raw_parts_mut(cells, self.pages.cols as usize)
                        .fill(blank_cell)
                };
            }
        }

        self.assert_integrity();
        Ok(())
    }

    /// This scrolls the active area at and above the cursor.
    /// The lines below the cursor are not scrolled.
    pub fn cursor_scroll_above(&mut self) -> Result<(), OutOfMemory> {
        // We unconditionally mark the cursor row as dirty here because
        // the cursor always changes page rows inside this function, and
        // when that happens it can mean the text in the old row needs to
        // be re-shaped because the cursor splits runs to break ligatures.
        self.cursor_mark_dirty();

        // If the cursor is on the bottom of the screen, its faster to use
        // our specialized function for that case.
        if self.cursor.y == self.pages.rows - 1 {
            return self.cursor_down_scroll();
        }

        // Logic below assumes we always have at least one row that isn't moving
        debug_assert!(self.cursor.y < self.pages.rows - 1);

        // Explanation:
        //  We don't actually move everything that's at or above the cursor row,
        //  since this would require us to shift up our ENTIRE scrollback, which
        //  would be ridiculously expensive. Instead, we insert a new row at the
        //  end of the pagelist (`grow()`), and move everything BELOW the cursor
        //  DOWN by one row. This has the same practical result but it's a whole
        //  lot cheaper in 99% of cases.

        let old_pin = unsafe { *self.cursor.page_pin };
        if let Some(new_node) = self.pages.grow()? {
            self.cursor_scroll_above_rotate(new_node)?;
        } else {
            // In this case, it means grow() didn't allocate a new page.

            if std::ptr::eq(
                unsafe { (*self.cursor.page_pin).node },
                self.pages.pages.last,
            ) {
                // If we're on the last page we can do a very fast path because
                // all the rows we need to move around are within a single page.

                // Note: we don't need to call cursorChangePin here because
                // the pin page is the same so there is no accounting to do
                // for styles or any of that.
                debug_assert!(std::ptr::eq(old_pin.node, unsafe {
                    (*self.cursor.page_pin).node
                }));
                let down = unsafe { (*self.cursor.page_pin).down(1) }.unwrap();
                unsafe { *self.cursor.page_pin = down };

                let pin = self.cursor.page_pin;
                let page: *mut Page =
                    unsafe { crate::pagelist::Node::page((*self.cursor.page_pin).node) };

                // Rotate the rows so that the newly created empty row is at the
                // beginning. e.g. [ 0 1 2 3 ] in to [ 3 0 1 2 ].
                let rows = unsafe { (*page).rows.ptr((*page).memory_ptr()) };
                let pin_y = unsafe { (*pin).y } as usize;
                let size_rows = unsafe { (*page).size.rows } as usize;
                // Rotating this suffix changes which logical row its coordinates identify.
                self.pages.invalidate_node_layout(unsafe { (*pin).node });
                fastmem::rotateOnceR(unsafe {
                    core::slice::from_raw_parts_mut(rows.add(pin_y), size_rows - pin_y)
                });

                // Mark the whole page as dirty.
                //
                // Technically we only need to mark from the cursor row to the
                // end but this is a hot function, so we want to minimize work.
                unsafe { (*page).dirty = true };

                // Setup our cursor caches after the rotation so it points to the
                // correct data
                let page_rac = unsafe { (*self.cursor.page_pin).row_and_cell() };
                self.cursor.page_row = page_rac.row;
                self.cursor.page_cell = page_rac.cell;
            } else {
                // We didn't grow pages but our cursor isn't on the last page.
                // In this case we need to do more work because we need to copy
                // elements between pages.
                //
                // An example scenario of this is shown below:
                //
                //      +----------+ = PAGE 0
                //  ... :          :
                //     +-------------+ ACTIVE
                // 4302 |1A00000000| | 0
                // 4303 |2B00000000| | 1
                //      :^         : : = PIN 0
                // 4304 |3C00000000| | 2
                //      +----------+ :
                //      +----------+ : = PAGE 1
                //    0 |4D00000000| | 3
                //    1 |5E00000000| | 4
                //      +----------+ :
                //     +-------------+
                self.cursor_scroll_above_rotate(core::ptr::null_mut())?;
            }
        }

        if self.cursor.style_id != style::DEFAULT_ID {
            // The newly created line needs to be styled according to
            // the bg color if it is set.
            if let Some(blank_cell) = self.cursor.style.bg_cell() {
                let cell_current: *mut pagepkg::Cell = self.cursor.page_cell;
                let cells = unsafe { cell_current.sub(self.cursor.x as usize) };
                unsafe {
                    core::slice::from_raw_parts_mut(cells, self.pages.cols as usize)
                        .fill(blank_cell)
                };
            }
        }

        self.assert_integrity();
        Ok(())
    }

    //
    // PORT NOTE: the reference takes `fresh_node: ?*PageList.List.Node`; the
    // Rust port uses a null `*mut Node` for the `null` case (raw-pointer
    // discipline: no `&mut` into a node the caller also aliases).
    fn cursor_scroll_above_rotate(
        &mut self,
        fresh_node: *mut crate::pagelist::Node,
    ) -> Result<(), OutOfMemory> {
        // A fresh node given to us is always the newly allocated tail of
        // the page list (see grow), which is where our iteration starts.
        debug_assert!(fresh_node.is_null() || std::ptr::eq(fresh_node, self.pages.pages.last));

        let down = unsafe { (*self.cursor.page_pin).down(1) }.unwrap();
        self.cursor_change_pin(down);

        // Go through each of the pages following our pin, shift all rows
        // down by one, and copy the last row of the previous page. We start
        // at the tail, which is the only node that can be freshly allocated.
        let mut current = self.pages.pages.last;
        assert!(!current.is_null());
        let mut current_is_fresh = !fresh_node.is_null();
        while !std::ptr::eq(current, unsafe { (*self.cursor.page_pin).node }) {
            let prev = unsafe { (*current).prev };

            // A newly allocated tail has no earlier references to invalidate.
            if !current_is_fresh {
                // Rotating this page moves every cached row coordinate down by one.
                self.pages.invalidate_node_layout(current);
            }

            // Rotate the pages down: [ 0 1 2 3 ] => [ 3 0 1 2 ]
            {
                let cur_page: *mut Page = unsafe { crate::pagelist::Node::page(current) };
                let cur_rows = unsafe { (*cur_page).rows.ptr((*cur_page).memory_ptr()) };
                let cur_size_rows = unsafe { (*cur_page).size.rows } as usize;
                fastmem::rotateOnceR(unsafe {
                    core::slice::from_raw_parts_mut(cur_rows, cur_size_rows)
                });
            }

            // Copy the last row of the previous page to the top of current.
            // If the current page doesn't have enough capacity for the
            // managed memory of the copied row (styles, hyperlinks, etc.)
            // then its capacity is increased and the copy retried, which
            // may replace `current` in the page list. Our loop condition
            // guarantees `current` is never the cursor page, and `prev` is
            // unaffected by the replacement.
            {
                let prev_page: *mut Page = unsafe { crate::pagelist::Node::page(prev) };
                let prev_rows = unsafe { (*prev_page).rows.ptr((*prev_page).memory_ptr()) };
                let prev_size_rows = unsafe { (*prev_page).size.rows } as usize;
                let cols = self.pages.cols as usize;
                current = self.clone_partial_row_grow_capacity(
                    current,
                    0,
                    prev_page,
                    unsafe { prev_rows.add(prev_size_rows - 1) },
                    0,
                    cols,
                );
            }

            // Mark dirty on the page, since we are dirtying all rows with this.
            unsafe { (*crate::pagelist::Node::page(current)).dirty = true };

            current = prev;
            current_is_fresh = false;
        }

        // Our current is our cursor page, we need to rotate down from
        // our cursor and clear our row.
        debug_assert!(std::ptr::eq(current, unsafe {
            (*self.cursor.page_pin).node
        }));
        let cur_page: *mut Page = unsafe { crate::pagelist::Node::page(current) };
        let cur_rows = unsafe { (*cur_page).rows.ptr((*cur_page).memory_ptr()) };
        let pin_y = unsafe { (*self.cursor.page_pin).y } as usize;
        let cur_size_rows = unsafe { (*cur_page).size.rows } as usize;
        // Rotating the cursor-page suffix changes its cached row coordinates.
        self.pages.invalidate_node_layout(current);
        fastmem::rotateOnceR(unsafe {
            core::slice::from_raw_parts_mut(cur_rows.add(pin_y), cur_size_rows - pin_y)
        });
        let row = unsafe { cur_rows.add(pin_y) };
        let cells = unsafe { (*cur_page).get_cells(row) };
        self.clear_cells(cur_page, row, cells);

        // The recycled storage becomes the new blank cursor row and must
        // not retain metadata (wrap state, semantic prompt) from the row
        // whose content was moved to the next page.
        // cur_rows[self.cursor.page_pin.y].reset();
        unsafe { (*row).reset() };

        // Mark the whole page as dirty.
        //
        // Technically we only need to mark from the cursor row to the
        // end but this is a hot function, so we want to minimize work.
        unsafe { (*cur_page).dirty = true };

        // Setup cursor cache data after all the rotations so our
        // row is valid.
        let page_rac = unsafe { (*self.cursor.page_pin).row_and_cell() };
        self.cursor.page_row = page_rac.row;
        self.cursor.page_cell = page_rac.cell;
        Ok(())
    }

    /// Scroll a full-width scroll region that ends at the cursor row up by
    /// one row. The cursor must be on the bottom row of the region. `limit`
    /// is the number of rows in the region above the cursor (region height
    /// minus one) and must be at least 1.
    ///
    /// The top row of the region is discarded (NOT moved into scrollback).
    /// All other rows in the region shift up by one and the cursor row
    /// becomes a blank row, filled with the current background color (like
    /// other scroll operations such as deleteLines).
    ///
    /// The cursor stays at the same screen position (the new blank row).
    /// Content outside of the region is unmodified.
    ///
    /// This is a very hot path for scroll region usage (e.g. a program
    /// on the alt screen using DECSTBM and scrolling via LF/IND) so this
    /// is optimized for the common case where the full region is within
    /// a single page.
    //
    // pub fn cursorScrollRegionUp(self: *Screen, limit: usize) !void
    //
    // PORTS: oracle 77190bd02 (`Screen.cursorScrollRegionUp`) + the
    // `invalidateNodeLayout` hunk from 0bad91adb (Phase 3 re-vet row P3-RV3).
    //
    // PORT NOTE (UB HAZARD, `P3-W5 T3`): the fast path holds a `*mut Page`
    // while it mutates tracked pins and the cursor's cached row/cell pointers,
    // and `assert_integrity()` re-walks the same memory afterwards. Every
    // access here therefore goes through RAW POINTERS derived from the page's
    // memory base; the only `&mut` ever formed is the `&mut [Row]` handed to
    // `fastmem::rotateOnce`, created from the raw base inside the narrowest
    // possible scope and dropped before the next raw access. In particular the
    // cursor's new `page_row`/`page_cell` are recomputed from the RAW base,
    // never from that temporary slice, so no pointer survives the borrow that
    // created it. Never form `&mut Page` (that is the mutation the Miri
    // harness `.port/phase-3/mirihar-scroll-region-up` proves fails).
    pub fn cursor_scroll_region_up(&mut self, limit: usize) -> Result<(), OutOfMemory> {
        // assert(limit >= 1);
        debug_assert!(limit >= 1);
        // assert(self.cursor.y >= limit);
        debug_assert!(self.cursor.y as usize >= limit);
        // defer self.assertIntegrity();
        //
        // PORT NOTE: Zig's `defer` runs on ERROR returns too, so the body lives
        // in an inner fn and the integrity gate runs unconditionally after it,
        // on every exit path including the slow path's `?` OOM early-return.
        // (Same shape as `PageList::erase_row_bounded`.)
        let result = self.cursor_scroll_region_up_inner(limit);
        self.assert_integrity();
        result
    }

    /// The body of `cursor_scroll_region_up` (the reference fn minus its
    /// `defer`); see the PORT NOTE above.
    fn cursor_scroll_region_up_inner(&mut self, limit: usize) -> Result<(), OutOfMemory> {
        // const pin: *Pin = self.cursor.page_pin;
        let pin: *mut Pin = self.cursor.page_pin;
        let pin_y = unsafe { (*pin).y };
        let pin_node = unsafe { (*pin).node };

        // If the region crosses a page boundary we take a slower path. This
        // is rare: it requires the active area to span multiple pages with
        // the split point inside the scroll region.
        // if (pin.y < limit) return try self.cursorScrollRegionUpSlow(limit);
        if (pin_y as usize) < limit {
            return self.cursor_scroll_region_up_slow(limit);
        }

        // Fast path: the entire region is in a single page. We can clear
        // the top row and rotate it down to the cursor row, updating any
        // tracked pins along the way.
        //
        // The cursor's cached row and cell pointers refer into this mapping.
        // PageList cannot compress the cursor page while those pointers are
        // installed, so the node is known to be resident here.
        // const page: *Page = pin.node.pageAssumeResident();
        let page: *mut Page = unsafe { pagelist::Node::page_assume_resident(pin_node) };
        // const rows = page.rows.ptr(page.memory.ptr)[pin.y - limit ..][0 .. limit + 1];
        //
        // PORT NOTE: the reference's `rows` is a `[]Row` slice used for the
        // clear, the rotate AND the final cursor-row pointer. In Rust it is a
        // raw base pointer for exactly the aliasing reason in the fn's PORT
        // NOTE; the slice is materialized only for `rotateOnce` below.
        let rows: *mut Row = unsafe {
            (*page)
                .rows
                .ptr((*page).memory_ptr())
                .add(pin_y as usize - limit)
        };

        // Clear the erased (top) row.
        {
            // const row: *Row = &rows[0];
            let row: *mut Row = rows;

            // Whether our blank cell is a plain zero cell. This is true
            // unless the cursor has a background color set (see blankCell).
            // const blank_is_zero = self.cursor.style_id == style.default_id or
            //     self.cursor.style.bg_color == .none;
            let blank_is_zero = self.cursor.style_id == style::DEFAULT_ID
                || matches!(self.cursor.style.bg_color.tag(), style::ColorTag::None);

            // if (!row.managedMemory() and blank_is_zero)
            if !unsafe { *row }.managed_memory() && blank_is_zero {
                // Hot path: the row has no managed memory (styles,
                // graphemes, hyperlinks) and our blank is zero so this is
                // a straight zero fill. This is the overwhelmingly common
                // case for scroll region usage.
                // const cells = page.getCells(row);
                // @memset(@as([]u64, @ptrCast(cells)), 0);
                //
                // PORT NOTE: the reference re-casts the cell slice to `[]u64`
                // to memset the backing integers; `Cell` is
                // `#[repr(transparent)] struct Cell(u64)` here, so a byte-wise
                // zero of the same extent is the identical write.
                let cells = unsafe { (*page).get_cells(row) };
                let len = cells.len();
                unsafe {
                    core::ptr::write_bytes(cells as *mut Cell, 0u8, len);
                }
            } else {
                // The generic clear handles managed memory and fills the
                // row with our blank cell, preserving the background color.
                // self.clearCells(page, row, page.getCells(row));
                let cells = unsafe { (*page).get_cells(row) };
                self.clear_cells(page, row, cells);
            }

            // The row becomes the new blank cursor row after the rotation
            // below and must not retain metadata (wrap state, semantic
            // prompt) from the discarded content.
            // row.reset();
            unsafe { (*row).reset() };
        }

        // Rotate the region rows so the now-blank top row moves to the
        // bottom (the cursor row) and everything else shifts up by one.
        // Rotating the region changes which logical row its coordinates identify.
        // self.pages.invalidateNodeLayout(pin.node);
        // fastmem.rotateOnce(Row, rows);
        self.pages.invalidate_node_layout(pin_node);
        fastmem::rotateOnce(unsafe { core::slice::from_raw_parts_mut(rows, limit + 1) });

        // Mark the whole page as dirty.
        //
        // Technically we only need to mark the rotated rows but this is
        // a hot function, so we want to minimize work.
        // page.dirty = true;
        unsafe { (*page).dirty = true };

        // If our viewport is a pin and it's within the rotated region
        // then we need to shift its cached row offset up. See
        // PageList.eraseRowBounded for details; this mirrors that logic.
        // if (self.pages.viewport == .pin) viewport: { ... }
        if self.pages.viewport == pagelist::Viewport::Pin {
            'viewport: {
                if let Some(v) = self.pages.viewport_pin_row_offset.as_mut() {
                    let p = self.pages.viewport_pin;
                    if !core::ptr::eq(unsafe { (*p).node }, pin_node)
                        || unsafe { (*p).y } < pin_y - limit as size::CellCountInt
                        || unsafe { (*p).y } > pin_y
                        || unsafe { (*p).y } == 0
                    {
                        break 'viewport;
                    }
                    *v -= 1;
                }
            }
        }

        // Update tracked pins within the region since their rows moved up
        // by one. The cursor's own pin is skipped because the cursor stays
        // at the region bottom (the new blank row).
        // const pin_keys = self.pages.tracked_pins.keys();
        // for (pin_keys) |p| { ... }
        let pin_keys = self.pages.tracked_pins.keys();
        for p in pin_keys {
            let p: *mut Pin = *p;
            if !core::ptr::eq(unsafe { (*p).node }, pin_node)
                || core::ptr::eq(p, pin)
                || unsafe { (*p).y } < pin_y - limit as size::CellCountInt
                || unsafe { (*p).y } > pin_y
            {
                continue;
            }
            if unsafe { (*p).y } == 0 {
                unsafe { (*p).x = 0 };
            } else {
                unsafe { (*p).y -= 1 };
            }
        }

        // The cursor pin is unchanged, but the Row structure at the pin
        // position now contains the blank row, so we need to refresh our
        // cached row/cell pointers. We compute them directly from the row
        // slice we already have rather than going through the pin since
        // this is a hot path.
        // const cursor_row: *Row = &rows[limit];
        // self.cursor.page_row = cursor_row;
        // self.cursor.page_cell = &page.getCells(cursor_row)[self.cursor.x];
        let cursor_row: *mut Row = unsafe { rows.add(limit) };
        self.cursor.page_row = cursor_row;
        self.cursor.page_cell =
            unsafe { ((*page).get_cells(cursor_row) as *mut Cell).add(self.cursor.x as usize) };

        Ok(())
    }

    /// Slow path for cursorScrollRegionUp: the scroll region spans
    /// multiple pages so we use the generic PageList erase machinery.
    // fn cursorScrollRegionUpSlow(self: *Screen, limit: usize) !void
    pub(super) fn cursor_scroll_region_up_slow(&mut self, limit: usize) -> Result<(), OutOfMemory> {
        // The call to eraseRowBounded below will move our tracked cursor
        // pin up by one row since it is inside the erased region, but we
        // don't actually want that: the cursor stays put, on the new blank
        // row at the region bottom. We keep the old pin and put it back
        // after, exactly like cursorDownScroll does for the no-scrollback
        // case.
        //
        // This matters beyond performance: when the cursor is on the first
        // row of a page, eraseRowBounded moves the tracked pin to the
        // previous page. Moving the cursor back down with cursorDown would
        // then cross pages via cursorChangePin, which migrates the cursor
        // style refcount from a page that never held it, corrupting the
        // style accounting.
        // const old_pin = self.cursor.page_pin.*;
        let old_pin = unsafe { *self.cursor.page_pin };

        // try self.pages.eraseRowBounded(
        //     .{ .active = .{ .y = @intCast(self.cursor.y - limit) } },
        //     limit,
        // );
        let y = self.cursor.y as usize - limit;
        self.pages.erase_row_bounded(
            point::Point::Active(point::Coordinate { x: 0, y: y as u32 }),
            limit,
        )?;

        // We don't use `cursorChangePin` here because we aren't actually
        // changing the pin, we're keeping it the same. Since the page
        // never changes, the cursor's style ref stays valid and no style
        // accounting needs to be updated.
        // self.cursor.page_pin.* = old_pin;
        unsafe { *self.cursor.page_pin = old_pin };

        // We do, however, need to refresh the cached page row and cell,
        // because the row contents at our pin position changed (it now
        // contains the blank row).
        let page_rac = unsafe { (*self.cursor.page_pin).row_and_cell() };
        self.cursor.page_row = page_rac.row;
        self.cursor.page_cell = page_rac.cell;

        // eraseRowBounded clears the new row with zero cells so if our
        // blank cell isn't zero (bg color is set) we need to fill it.
        // const blank = self.blankCell();
        // if (!blank.isZero()) {
        //     const cells: [*]Cell = @ptrCast(self.cursor.page_cell);
        //     @memset((cells - self.cursor.x)[0..self.pages.cols], blank);
        // }
        let blank = self.blank_cell();
        if !blank.is_zero() {
            let cells = unsafe { self.cursor.page_cell.sub(self.cursor.x as usize) };
            unsafe { core::slice::from_raw_parts_mut(cells, self.pages.cols as usize).fill(blank) };
        }

        Ok(())
    }

    /// Move the cursor down if we're not at the bottom of the screen. Otherwise
    /// scroll. Currently only used for testing.
    pub(super) fn cursor_down_or_scroll(&mut self) -> Result<(), OutOfMemory> {
        if self.cursor.y + 1 < self.pages.rows {
            self.cursor_down(1);
            Ok(())
        } else {
            self.cursor_down_scroll()
        }
    }

    /// Always use this to write to cursor.page_pin.*.
    ///
    /// This specifically handles the case when the new pin is on a different
    /// page than the old AND we have a style or hyperlink set. In that case,
    /// we must release our old one and insert the new one, since styles are
    /// stored per-page.
    ///
    /// Note that this can change the cursor pin AGAIN if the process of
    /// setting up our cursor forces a capacity adjustment of the underlying
    /// cursor page, so any references to the page pin should be re-read
    /// from `self.cursor.page_pin` after calling this.
    pub(super) fn cursor_change_pin(&mut self, new: Pin) {
        // Moving the cursor affects text run splitting (ligatures) so
        // we must mark the old and new page dirty. We do this as long
        // as the pins are not equal
        if !unsafe { (*self.cursor.page_pin).eql(new) } {
            self.cursor_mark_dirty();
            new.mark_dirty();
        }

        // If our pin is on the same page, then we can just update the pin.
        // We don't need to migrate any state.
        if std::ptr::eq(unsafe { (*self.cursor.page_pin).node }, new.node) {
            unsafe { *self.cursor.page_pin = new };
            return;
        }

        // If we have an old style then we need to release it from the old page.
        let old_style_: Option<style::Style> = if self.cursor.style_id == style::DEFAULT_ID {
            None
        } else {
            Some(self.cursor.style)
        };
        if old_style_.is_some() {
            // Release the style directly from the old page instead of going through
            // manualStyleUpdate, because the cursor position may have already been
            // updated but the pin has not, which would fail integrity checks.
            let old_page: *mut Page =
                unsafe { crate::pagelist::Node::page((*self.cursor.page_pin).node) };
            unsafe {
                let base = (*old_page).memory_ptr();
                (*old_page).styles.release(base, self.cursor.style_id);
            }
            self.cursor.style = style::Style::default();
            self.cursor.style_id = style::DEFAULT_ID;
        }

        // If we have a hyperlink then we need to release it from the old page.
        if self.cursor.hyperlink.is_some() {
            let old_page: *mut Page =
                unsafe { crate::pagelist::Node::page((*self.cursor.page_pin).node) };
            unsafe {
                let base = (*old_page).memory_ptr();
                (*old_page)
                    .hyperlink_set
                    .release(base, self.cursor.hyperlink_id);
            }
            // Zero the ID, it is invalid now and style changes below may
            // run integrity checks. We still have self.cursor.hyperlink to
            // rebuild this later.
            self.cursor.hyperlink_id = 0;
        }

        // Update our pin to the new page
        unsafe { *self.cursor.page_pin = new };

        // On the new page, we need to migrate our style
        if let Some(old_style) = old_style_ {
            self.cursor.style = old_style;
            if let Err(err) = self.manual_style_update() {
                // This failure should not happen because manualStyleUpdate
                // handles page splitting, overflow, and more. This should only
                // happen if we're out of RAM. In this case, we'll just degrade
                // gracefully back to the default style.
                log::error!(target: LOG_SCOPE, "failed to update style on cursor change err={err:?}");
                self.cursor.style = style::Style::default();
                self.cursor.style_id = 0;
            }
        }

        // On the new page, we need to migrate our hyperlink
        if let Some(link) = self.cursor.hyperlink {
            // startHyperlink will try to free old hyperlinks, so set this
            // to null. We free it ourselves later since we're doing some
            // ref-counting shenanigans in this function.
            self.cursor.hyperlink = None;

            // Re-add
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
                log::error!(target: LOG_SCOPE, "failed to update hyperlink on cursor change err={err:?}");
            }

            // Remove our old link
            // link.deinit(self.alloc);
            // self.alloc.destroy(link);
            unsafe { destroy_cursor_hyperlink(link) };
        }
    }

    /// Copy another cursor. The cursor can be on any screen but the x/y
    /// must be within our screen bounds.
    pub fn cursor_copy(
        &mut self,
        other: Cursor,
        opts: CursorCopyOptions,
    ) -> Result<(), pagelist::grow::IncreaseCapacityError> {
        debug_assert!(other.x < self.pages.cols);
        debug_assert!(other.y < self.pages.rows);

        // End any currently active hyperlink on our cursor.
        self.end_hyperlink();

        // PORT NOTE: the reference does `const old = self.cursor; self.cursor =
        // other; errdefer self.cursor = old;`. `other` is taken by value
        // (`Cursor` is a plain value type here). We capture the fields of the old
        // cursor we still need (`style_id`, `page_pin`, `x`, `y`) before
        // overwriting.
        //
        // PORT NOTE (parity S133-F3): an earlier version of this note
        // claimed `Cursor` is "not trivially Copy" and that the `errdefer` revert
        // is "not reachable". Both are wrong. `Cursor` IS `#[derive(Copy)]` (its
        // `hyperlink` is a raw pointer whose box ownership is tracked
        // explicitly), and `manual_style_update` returns
        // `pagelist::grow::IncreaseCapacityError`, which has BOTH an `OutOfMemory`
        // and an `OutOfSpace` variant — so it really can fail, and on failure the
        // reference's `errdefer self.cursor = old` puts the WHOLE old cursor
        // back. Without it `self.cursor` was left half-applied: `other`'s
        // style/pending_wrap/protected/semantic_content over the old
        // style_id/pin/x/y. The error was also flattened to `OutOfMemory`,
        // losing `OutOfSpace`.
        let old = self.cursor;
        let old_style_id = self.cursor.style_id;
        let old_page_pin = self.cursor.page_pin;
        let old_x = self.cursor.x;
        let old_y = self.cursor.y;

        // Take ownership of `other`'s heap hyperlink (the reference moves the
        // whole struct); we then clear it on `self.cursor` and manage it below.
        let other_hyperlink_id = other.hyperlink_id;
        let other_page_pin = other.page_pin;
        let other_x = other.x;
        let other_y = other.y;

        let mut new_cursor = other;
        // Keep our old style ID so it can be properly cleaned up below.
        new_cursor.style_id = old_style_id;
        // Hyperlinks will be managed separately below.
        new_cursor.hyperlink_id = 0;
        new_cursor.hyperlink = None;
        // Keep our old page pin and X/Y because:
        // 1. The old style will need to be cleaned up from the page it's from.
        // 2. The new position navigated to by `cursorAbsolute` needs to be in our
        //    own screen.
        new_cursor.page_pin = old_page_pin;
        new_cursor.x = old_x;
        new_cursor.y = old_y;

        self.cursor = new_cursor;

        // Call manual style update in order to clean up our old style, if we have
        // one, and also to load the style from the other cursor, if it had one.
        // try self.manualStyleUpdate();
        #[cfg(test)]
        let forced = cursor_copy_tw_check(CursorCopyFp::ManualStyleUpdate);
        #[cfg(not(test))]
        let forced: Result<(), pagelist::grow::IncreaseCapacityError> = Ok(());
        if let Err(err) = forced.and_then(|()| self.manual_style_update()) {
            // errdefer self.cursor = old;
            self.cursor = old;
            return Err(err);
        }

        // Move to the correct location to match the other cursor.
        self.cursor_absolute(other_x, other_y);

        // If the other cursor had a hyperlink, add it to ours.
        if opts.hyperlink && other_hyperlink_id != 0 {
            // Get the hyperlink from the other cursor's page.
            let other_page: *mut Page =
                unsafe { crate::pagelist::Node::page((*other_page_pin).node) };
            let other_link = unsafe {
                let base = (*other_page).memory_ptr();
                (*other_page).hyperlink_set.get(base, other_hyperlink_id)
            };

            let other_base = unsafe { (*other_page).memory_ptr() };
            let uri = unsafe { (*other_link).uri.slice(other_base) }.to_vec();
            let other_id = unsafe { (*other_link).id };
            let id_: Option<Vec<u8>> = match other_id.tag() {
                hyperlink::PageEntryIdTag::Explicit => {
                    Some(unsafe { other_id.explicit().slice(other_base) }.to_vec())
                }
                hyperlink::PageEntryIdTag::Implicit => None,
            };

            // And it to our cursor.
            if let Err(err) = self.start_hyperlink(&uri, id_.as_deref()) {
                // This shouldn't happen because startHyperlink should handle
                // resizing. This only happens if we're truly out of RAM. Degrade
                // to forgetting the hyperlink.
                log::error!(target: LOG_SCOPE, "failed to update hyperlink on cursor change err={err:?}");
            }
        }

        Ok(())
    }

    /// Mark the cursor position as dirty.
    /// TODO: test
    #[inline]
    pub fn cursor_mark_dirty(&mut self) {
        unsafe { (*self.cursor.page_row).set_dirty(true) };
    }

    /// Reset the cursor row's soft-wrap state and the cursor's pending wrap.
    /// Also handles clearing the spacer head on the cursor row and resetting
    /// the wrap_continuation flag on the next row if necessary.
    ///
    /// NOTE(qwerasd): This method is not scrolling region aware, and cannot be
    /// since it's on Screen not Terminal. This needs to be addressed down the
    /// line. Not an extremely urgent issue since it's an edge case of an edge
    /// case, but not ideal.
    pub fn cursor_reset_wrap(&mut self) {
        // Reset the cursor's pending wrap state
        self.cursor.pending_wrap = false;

        let page_row = self.cursor.page_row;

        if !unsafe { (*page_row).wrap() } {
            return;
        }

        // This row does not wrap and the next row is not wrapped to
        unsafe { (*page_row).set_wrap(false) };

        if let Some(next_row) = unsafe { (*self.cursor.page_pin).down(1) } {
            unsafe { (*next_row.row_and_cell().row).set_wrap_continuation(false) };
        }

        // If the last cell in the row is a spacer head we need to clear it.
        let cells = unsafe { (*self.cursor.page_pin).cells(pagelist::pin::CellSubset::All) };
        let cols = unsafe { crate::pagelist::Node::cols((*self.cursor.page_pin).node) } as usize;
        let cell = unsafe { *(cells as *mut pagepkg::Cell).add(cols - 1) };
        if cell.wide() == pagepkg::Wide::SpacerHead {
            let page: *mut Page =
                unsafe { crate::pagelist::Node::page((*self.cursor.page_pin).node) };
            let last = unsafe { (cells as *mut pagepkg::Cell).add(cols - 1) };
            let slice = core::ptr::slice_from_raw_parts_mut(last, 1);
            self.clear_cells(page, page_row, slice);
        }
    }
}
