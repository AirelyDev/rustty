//! PageList erase cluster.
//!
//! PORTS: ghostty src/terminal/PageList.zig — the `eraseRow`, `eraseRowBounded`,
//! `eraseHistory`, `eraseActive`, `eraseRows`, and `erasePage` cluster.
//!
//! PORT STATUS: methods ported (vt-3c). The 205-block test split lands in the
//! dedicated test phase.

use super::{Node, PageList};
use crate::page::Row;
use crate::point;
use rustty_render_types::term_size as size;
use rustty_util::fastmem;

impl PageList {
    /// Fast-path function to erase exactly 1 row. Erasing means that the row
    /// is completely REMOVED, not just cleared. All rows following the removed
    /// row will be shifted up by 1 to fill the empty space.
    ///
    /// Unlike eraseRows, eraseRow does not change the size of any pages. The
    /// caller is responsible for adjusting the row count of the final page if
    /// that behavior is required.
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub fn erase_row(&mut self, pt: point::Point) -> Result<(), super::OutOfMemory> {
        // PORT NOTE: `defer self.assertIntegrity()` — Zig's defer runs on ERROR
        // returns too, so the body lives in an inner fn and the integrity gate
        // runs unconditionally after it (a `?` early-return must not skip the
        // corruption tripwire at exactly the OOM point the oracle checks it —
        // VALIDATION_REPORT §3e / §3c.9).
        let result = self.erase_row_inner(pt);
        self.assert_integrity();
        result
    }

    /// The body of `erase_row` (the reference fn minus its `defer`); see the
    /// PORT NOTE above.
    fn erase_row_inner(&mut self, pt: point::Point) -> Result<(), super::OutOfMemory> {
        // Erasing history may restore compressed pages. Mark unconditionally
        // because incrementing the activity token is cheaper than locating the
        // active boundary.
        self.page_compression.mark_activity();

        let pn = self.pin(pt).unwrap();

        let mut node = pn.node;
        let mut rows = unsafe {
            (*super::Node::page(node))
                .rows
                .ptr((*super::Node::page(node)).memory_ptr())
        };

        // In order to move the following rows up we rotate the rows array by 1.
        // The rotate operation turns e.g. [ 0 1 2 3 ] in to [ 1 2 3 0 ], which
        // works perfectly to move all of our elements where they belong.
        fastmem::rotateOnce(unsafe {
            core::slice::from_raw_parts_mut(
                rows.add(pn.y as usize),
                (super::Node::rows(node) - pn.y) as usize,
            )
        });

        // We adjust the tracked pins in this page, moving up any that were below
        // the removed row.
        {
            let pin_keys = self.tracked_pins.keys();
            for p in pin_keys {
                let p: *mut super::Pin = *p;
                if core::ptr::eq(unsafe { (*p).node }, node) && unsafe { (*p).y } > pn.y {
                    unsafe { (*p).y -= 1 };
                }
            }
        }

        // If we have a pinned viewport, we need to adjust for active area.
        self.fixup_viewport(1);

        // Mark the whole page as dirty.
        //
        // Technically we only need to mark rows from the erased row to the end
        // of the page as dirty, but that's slower and this is a hot function.
        unsafe { (*super::Node::page(node)).dirty = true };

        // We iterate through all of the following pages in order to move their
        // rows up by 1 as well.
        while !unsafe { (*node).next }.is_null() {
            let next = unsafe { (*node).next };
            let next_rows = unsafe { (*super::Node::page(next)).rows.ptr((*super::Node::page(next)).memory_ptr()) };

            // We take the top row of the page and clone it in to the bottom
            // row of the previous page, which gets rid of the top row that was
            // rotated down in the previous page, and accounts for the row in
            // this page that will be rotated down as well.
            //
            //  rotate -> clone --> rotate -> result
            //    0 -.      1         1         1
            //    1  |      2         2         2
            //    2  |      3         3         3
            //    3 <'      0 <.      4         4
            //   ---       --- |     ---       ---  <- page boundary
            //    4         4 -'      4 -.      5
            //    5         5         5  |      6
            //    6         6         6  |      7
            //    7         7         7 <'      4
            unsafe {
                (*super::Node::page(node)).clone_row_from(
                    &(*super::Node::page(next)),
                    rows.add(super::Node::rows(node) as usize - 1),
                    next_rows,
                )
            }
            .map_err(|_| super::OutOfMemory)?;

            node = next;
            rows = next_rows;

            fastmem::rotateOnce(unsafe {
                core::slice::from_raw_parts_mut(rows, super::Node::rows(node) as usize)
            });

            // Mark the whole page as dirty.
            unsafe { (*super::Node::page(node)).dirty = true };

            // Our tracked pins for this page need to be updated.
            // If the pin is in row 0 that means the corresponding row has
            // been moved to the previous page. Otherwise, move it up by 1.
            let pin_keys = self.tracked_pins.keys();
            for p in pin_keys {
                let p: *mut super::Pin = *p;
                if !core::ptr::eq(unsafe { (*p).node }, node) {
                    continue;
                }
                if unsafe { (*p).y } == 0 {
                    unsafe {
                        (*p).node = (*node).prev;
                        (*p).y = super::Node::rows((*p).node) - 1;
                    }
                    continue;
                }
                unsafe { (*p).y -= 1 };
            }
        }

        // Clear the final row which was rotated from the top of the page.
        unsafe {
            let cols = super::Node::cols(node) as usize;
            let row = rows.add(super::Node::rows(node) as usize - 1);
            (*super::Node::page(node)).clear_cells(row, 0, cols);
        }

        Ok(())
    }

    /// A variant of eraseRow that shifts only a bounded number of following
    /// rows up, filling the space they leave behind with blank rows.
    ///
    /// `limit` is exclusive of the erased row. A limit of 1 will erase the target
    /// row and shift the row below in to its position, leaving a blank row below.
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub fn erase_row_bounded(
        &mut self,
        pt: point::Point,
        limit: usize,
    ) -> Result<(), super::OutOfMemory> {
        // PORT NOTE: `defer self.assertIntegrity()` — Zig's defer runs on ERROR
        // returns too, so the body lives in an inner fn and the integrity gate
        // runs unconditionally after it, on every exit path including the `?`
        // OOM early-return (VALIDATION_REPORT §3e / §3c.9).
        let result = self.erase_row_bounded_inner(pt, limit);
        self.assert_integrity();
        result
    }

    /// The body of `erase_row_bounded` (the reference fn minus its `defer`);
    /// see the PORT NOTE above.
    fn erase_row_bounded_inner(
        &mut self,
        pt: point::Point,
        limit: usize,
    ) -> Result<(), super::OutOfMemory> {
        // Erasing history may restore compressed pages. Mark unconditionally
        // because incrementing the activity token is cheaper than locating the
        // active boundary.
        self.page_compression.mark_activity();

        // This function has a lot of repeated code in it because it is a hot path.
        //
        // To get a better idea of what's happening, read eraseRow first for more
        // in-depth explanatory comments. To avoid repetition, the only comments for
        // this function are for where it differs from eraseRow.

        let pn = self.pin(pt).unwrap();

        let mut node: *mut Node = pn.node;
        let mut rows = unsafe {
            (*super::Node::page(node))
                .rows
                .ptr((*super::Node::page(node)).memory_ptr())
        };

        // If the row limit is less than the remaining rows before the end of the
        // page, then we clear the row, rotate it to the end of the boundary limit
        // and update our pins.
        if (unsafe { super::Node::rows(node) } - pn.y) as usize > limit {
            unsafe {
                let cols = super::Node::cols(node) as usize;
                (*super::Node::page(node)).clear_cells(rows.add(pn.y as usize), 0, cols);
            }
            fastmem::rotateOnce(unsafe {
                core::slice::from_raw_parts_mut(rows.add(pn.y as usize), limit + 1)
            });

            // Mark the whole page as dirty.
            //
            // Technically we only need to mark from the erased row to the
            // limit but this is a hot function, so we want to minimize work.
            unsafe { (*super::Node::page(node)).dirty = true };

            // If our viewport is a pin and our pin is within the erased
            // region we need to maybe shift our cache up. We do this here instead
            // of in the pin loop below because its unlikely to be true and we
            // don't want to run the conditional N times.
            if self.viewport == super::Viewport::Pin {
                'viewport: {
                    if let Some(v) = self.viewport_pin_row_offset.as_mut() {
                        let p = self.viewport_pin;
                        if !core::ptr::eq(unsafe { (*p).node }, node)
                            || unsafe { (*p).y } < pn.y
                            || unsafe { (*p).y } > pn.y + limit as size::CellCountInt
                            || unsafe { (*p).y } == 0
                        {
                            break 'viewport;
                        }
                        *v -= 1;
                    }
                }
            }

            // Update pins in the shifted region.
            let pin_keys = self.tracked_pins.keys();
            for p in pin_keys {
                let p: *mut super::Pin = *p;
                if core::ptr::eq(unsafe { (*p).node }, node)
                    && unsafe { (*p).y } >= pn.y
                    && unsafe { (*p).y } <= pn.y + limit as size::CellCountInt
                {
                    if unsafe { (*p).y } == 0 {
                        unsafe { (*p).x = 0 };
                    } else {
                        unsafe { (*p).y -= 1 };
                    }
                }
            }

            return Ok(());
        }

        fastmem::rotateOnce(unsafe {
            core::slice::from_raw_parts_mut(
                rows.add(pn.y as usize),
                (super::Node::rows(node) - pn.y) as usize,
            )
        });

        // Mark the whole page as dirty.
        //
        // Technically we only need to mark rows from the erased row to the end
        // of the page as dirty, but that's slower and this is a hot function.
        unsafe { (*super::Node::page(node)).dirty = true };

        // We need to keep track of how many rows we've shifted so that we can
        // determine at what point we need to do a partial shift on subsequent
        // pages.
        let mut shifted: usize = (unsafe { super::Node::rows(node) } - pn.y) as usize;

        // Update tracked pins.
        {
            // See the other places we do something similar in this function
            // for a detailed explanation.
            if self.viewport == super::Viewport::Pin {
                'viewport: {
                    if let Some(v) = self.viewport_pin_row_offset.as_mut() {
                        let p = self.viewport_pin;
                        if !core::ptr::eq(unsafe { (*p).node }, node)
                            || unsafe { (*p).y } < pn.y
                            || unsafe { (*p).y } == 0
                        {
                            break 'viewport;
                        }
                        *v -= 1;
                    }
                }
            }

            let pin_keys = self.tracked_pins.keys();
            for p in pin_keys {
                let p: *mut super::Pin = *p;
                if core::ptr::eq(unsafe { (*p).node }, node) && unsafe { (*p).y } >= pn.y {
                    if unsafe { (*p).y } == 0 {
                        unsafe { (*p).x = 0 };
                    } else {
                        unsafe { (*p).y -= 1 };
                    }
                }
            }
        }

        while !unsafe { (*node).next }.is_null() {
            let next = unsafe { (*node).next };
            let next_rows = unsafe { (*super::Node::page(next)).rows.ptr((*super::Node::page(next)).memory_ptr()) };

            unsafe {
                (*super::Node::page(node)).clone_row_from(
                    &(*super::Node::page(next)),
                    rows.add(super::Node::rows(node) as usize - 1),
                    next_rows,
                )
            }
            .map_err(|_| super::OutOfMemory)?;

            node = next;
            rows = next_rows;

            // We check to see if this page contains enough rows to satisfy the
            // specified limit, accounting for rows we've already shifted in prior
            // pages.
            //
            // The logic here is very similar to the one before the loop.
            let shifted_limit = limit - shifted;
            if (unsafe { super::Node::rows(node) } as usize) > shifted_limit {
                unsafe {
                    let cols = super::Node::cols(node) as usize;
                    (*super::Node::page(node)).clear_cells(rows, 0, cols);
                }
                fastmem::rotateOnce(unsafe {
                    core::slice::from_raw_parts_mut(rows, shifted_limit + 1)
                });

                // Mark the whole page as dirty.
                //
                // Technically we only need to mark from the erased row to the
                // limit but this is a hot function, so we want to minimize work.
                unsafe { (*super::Node::page(node)).dirty = true };

                // See the other places we do something similar in this function
                // for a detailed explanation.
                if self.viewport == super::Viewport::Pin {
                    'viewport: {
                        if let Some(v) = self.viewport_pin_row_offset.as_mut() {
                            let p = self.viewport_pin;
                            if !core::ptr::eq(unsafe { (*p).node }, node)
                                || unsafe { (*p).y } > shifted_limit as size::CellCountInt
                            {
                                break 'viewport;
                            }
                            *v -= 1;
                        }
                    }
                }

                // Update pins in the shifted region.
                let pin_keys = self.tracked_pins.keys();
                for p in pin_keys {
                    let p: *mut super::Pin = *p;
                    if !core::ptr::eq(unsafe { (*p).node }, node)
                        || unsafe { (*p).y } > shifted_limit as size::CellCountInt
                    {
                        continue;
                    }
                    if unsafe { (*p).y } == 0 {
                        unsafe {
                            (*p).node = (*node).prev;
                            (*p).y = super::Node::rows((*p).node) - 1;
                        }
                        continue;
                    }
                    unsafe { (*p).y -= 1 };
                }

                return Ok(());
            }

            fastmem::rotateOnce(unsafe {
                core::slice::from_raw_parts_mut(rows, super::Node::rows(node) as usize)
            });

            // Mark the whole page as dirty.
            unsafe { (*super::Node::page(node)).dirty = true };

            // Account for the rows shifted in this node.
            shifted += unsafe { super::Node::rows(node) } as usize;

            // See the other places we do something similar in this function
            // for a detailed explanation.
            if self.viewport == super::Viewport::Pin {
                // viewport:
                if let Some(v) = self.viewport_pin_row_offset.as_mut() {
                    let p = self.viewport_pin;
                    if core::ptr::eq(unsafe { (*p).node }, node) {
                        *v -= 1;
                    }
                }
            }

            // Update tracked pins.
            let pin_keys = self.tracked_pins.keys();
            for p in pin_keys {
                let p: *mut super::Pin = *p;
                if !core::ptr::eq(unsafe { (*p).node }, node) {
                    continue;
                }
                if unsafe { (*p).y } == 0 {
                    unsafe {
                        (*p).node = (*node).prev;
                        (*p).y = super::Node::rows((*p).node) - 1;
                    }
                    continue;
                }
                unsafe { (*p).y -= 1 };
            }
        }

        // We reached the end of the page list before the limit, so we clear
        // the final row since it was rotated down from the top of this page.
        unsafe {
            let cols = super::Node::cols(node) as usize;
            let row = rows.add(super::Node::rows(node) as usize - 1);
            (*super::Node::page(node)).clear_cells(row, 0, cols);
        }

        Ok(())
    }

    /// Erase all history rows, optionally up to a bottom-left bound.
    /// This always starts from the beginning of the history area.
    pub fn erase_history(&mut self, bl_pt: Option<point::Point>) {
        self.erase_rows(point::Point::History(point::Coordinate::default()), bl_pt);
    }

    /// Erase active area rows, from the top of the active area to the
    /// given row (inclusive).
    pub fn erase_active(&mut self, y: size::CellCountInt) {
        assert!(y < self.rows);
        self.erase_rows(
            point::Point::Active(point::Coordinate::default()),
            Some(point::Point::Active(point::Coordinate {
                x: 0,
                y: y as u32,
            })),
        );
    }

    /// Erase rows from tl_pt to bl_pt (inclusive), physically removing
    /// them rather than just clearing their contents. If a point falls
    /// in the middle of a page, remaining rows in that page are shifted
    /// and the page becomes underutilized (size < capacity).
    ///
    /// Callers must ensure that the erased range only removes pages from
    /// the front or back of the linked list, never the middle. Middle-page
    /// erasure would create serial gaps that page_serial_epoch cannot
    /// represent, leaving dangling references in consumers such as search.
    /// Use the public eraseHistory/eraseActive wrappers which enforce this.
    fn erase_rows(&mut self, tl_pt: point::Point, bl_pt: Option<point::Point>) {
        // PORT NOTE: `defer self.assertIntegrity()` — Zig's defer runs on the
        // grow-failure EARLY `return` too, so the body lives in an inner fn and
        // the integrity gate runs unconditionally after it (VALIDATION_REPORT
        // §3e / §3c.9).
        self.erase_rows_inner(tl_pt, bl_pt);
        self.assert_integrity();
    }

    /// The body of `erase_rows` (the reference fn minus its `defer`); see the
    /// PORT NOTE above.
    fn erase_rows_inner(&mut self, tl_pt: point::Point, bl_pt: Option<point::Point>) {
        self.page_compression.mark_activity();

        // The count of rows that was erased.
        let mut erased: usize = 0;

        // A pageIterator iterates one page at a time from the back forward.
        // "back" here is in terms of scrollback, but actually the front of the
        // linked list.
        let mut it = self.page_iterator(super::iter::Direction::RightDown, tl_pt, bl_pt);
        while let Some(chunk) = it.next() {
            // If the chunk is a full page, deinit thit page and remove it from
            // the linked list.
            if chunk.full_page() {
                // A rare special case is that we're deleting everything
                // in our linked list. erasePage requires at least one other
                // page so to handle this we reinit this page, set it to zero
                // size which will let us grow our active area back.
                if unsafe { (*chunk.node).next }.is_null()
                    && unsafe { (*chunk.node).prev }.is_null()
                {
                    let page: *mut crate::page::Page = unsafe { super::Node::page(chunk.node) };
                    erased += unsafe { (*page).size.rows } as usize;
                    unsafe { (*page).reinit() };
                    unsafe { (*page).size.rows = 0 };
                    break;
                }

                erased += unsafe { super::Node::rows(chunk.node) } as usize;
                self.erase_page(chunk.node);
                continue;
            }

            // We are modifying our chunk so make sure it is in a good state.
            //
            // PORT NOTE: `defer chunk.node.data.assertIntegrity()` — reproduced
            // by calling `assert_integrity` on the chunk's page before the end of
            // this loop iteration's body.

            // The chunk is not a full page so we need to move the rows.
            // This is a cheap operation because we're just moving cell offsets,
            // not the actual cell contents.
            assert!(chunk.start == 0);
            let rows = unsafe { (*super::Node::page(chunk.node)).rows.ptr((*super::Node::page(chunk.node)).memory_ptr()) };
            let scroll_amount = unsafe { super::Node::rows(chunk.node) } - chunk.end;
            for i in 0..scroll_amount {
                let src: *mut Row = unsafe { rows.add((i + chunk.end) as usize) };
                let dst: *mut Row = unsafe { rows.add(i as usize) };
                let old_dst = unsafe { *dst };
                unsafe { *dst = *src };
                unsafe { *src = old_dst };

                // Mark the moved row as dirty.
                unsafe { (*dst).set_dirty(true) };
            }

            // Clear our remaining cells that we didn't shift or swapped
            // in case we grow back into them.
            for i in scroll_amount..unsafe { super::Node::rows(chunk.node) } {
                let row: *mut Row = unsafe { rows.add(i as usize) };
                unsafe {
                    let cols = super::Node::cols(chunk.node) as usize;
                    (*super::Node::page(chunk.node)).clear_cells(row, 0, cols);
                }
            }

            // Update any tracked pins to shift their y. If it was in the erased
            // row then we move it to the top of this page.
            let pin_keys = self.tracked_pins.keys();
            for p in pin_keys {
                let p: *mut super::Pin = *p;
                if !core::ptr::eq(unsafe { (*p).node }, chunk.node) {
                    continue;
                }
                if unsafe { (*p).y } >= chunk.end {
                    unsafe { (*p).y -= chunk.end };
                } else {
                    unsafe {
                        (*p).y = 0;
                        (*p).x = 0;
                    }
                }
            }

            // Our new size is the amount we scrolled
            unsafe { (*super::Node::page(chunk.node)).size.rows = scroll_amount };
            erased += chunk.end as usize;

            // PORT NOTE: `defer chunk.node.data.assertIntegrity()`.
            unsafe { (*super::Node::page(chunk.node)).assert_integrity() };
        }

        // Update our total row count
        self.total_rows -= erased;

        // If we deleted active, we need to regrow because one of our invariants
        // is that we always have full active space.
        if tl_pt.tag() == point::Tag::Active {
            for _ in 0..erased {
                if self.grow().is_err() {
                    // If this fails its a pretty big issue actually... but I don't
                    // want to turn this function into an error-returning function
                    // because erasing active is so rare and even if it happens failing
                    // is even more rare...
                    //
                    // PORT NOTE: the reference logs the error and returns; we keep
                    // the early return and elide the `log.err` call (no logging
                    // facade is wired into the crate yet — see `grow.rs` for the
                    // same `// log.err(...)` convention).
                    // log.err("failed to regrow active area after erase err={}", .{err});
                    return;
                }
            }
        }

        // If we have a pinned viewport, we need to adjust for active area.
        self.fixup_viewport(erased);
    }

    /// Erase a single page, freeing all its resources. The page must be
    /// at the front or back of the linked list (not the middle) and must
    /// NOT be the final page in the entire list (i.e. must not make the
    /// list empty).
    ///
    /// IMPORTANT: This function does NOT update `total_rows`. The caller is
    /// responsible for accounting for the removed rows before or after calling
    /// this function.
    // PORT NOTE: `pub(crate)` so sibling cluster files in the `pagelist` module
    // (e.g. `resize.rs::trim_trailing_blank_rows`) can call this private-in-Zig
    // helper across the file split. The reference `erasePage` is file-private.
    pub(crate) fn erase_page(&mut self, node: *mut Node) {
        // Must not be the final page.
        assert!(!unsafe { (*node).next }.is_null() || !unsafe { (*node).prev }.is_null());

        // We only support erasing from the front or back, never the middle.
        // Middle erasure would create serial gaps that page_serial_epoch can't
        // represent. If this ever needs to change, we'll need a more
        // sophisticated invalidation mechanism.
        assert!(unsafe { (*node).prev }.is_null() || unsafe { (*node).next }.is_null());

        // If we're erasing the first page, update page_serial_epoch so that
        // any external references holding this page's serial will know it
        // has been invalidated.
        if unsafe { (*node).prev }.is_null() {
            self.page_serial_epoch = unsafe { (*(*node).next).serial };
        }

        // Update any tracked pins to move to the previous or next page.
        let pin_keys = self.tracked_pins.keys();
        for p in pin_keys {
            let p: *mut super::Pin = *p;
            if !core::ptr::eq(unsafe { (*p).node }, node) {
                continue;
            }
            unsafe {
                (*p).node = if !(*node).prev.is_null() {
                    (*node).prev
                } else if !(*node).next.is_null() {
                    (*node).next
                } else {
                    unreachable!()
                };
                (*p).y = 0;
                (*p).x = 0;
            }

            // This doesn't get marked garbage because the tracked pin
            // movement is sensical.
        }

        // Remove the page from the linked list
        unsafe { self.pages.remove(node) };
        self.destroy_node(node);
    }
}
