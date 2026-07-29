//! PageList resize + reflow cluster.
//!
//! PORTS: ghostty src/terminal/PageList.zig — the `Resize` struct (+ `Cursor`),
//! `resize`, `resizeCols`, the `ReflowCursor` helper (`reflowRow`, `writeCell`,
//! `moveLastRowToNewPage`, `increaseCapacity`, `bottom`, `cursorForward`,
//! `cursorScroll`, `cursorNewPage`, `cursorScrollOrNewPage`, `cursorAbsolute`,
//! `countTrailingEmptyCells`, `copyRowMetadata`), `resizeWithoutReflow`,
//! `resizeWithoutReflowGrowCols`, `trailingBlankLines`, and
//! `trimTrailingBlankRows` cluster.
//!
//! PORT STATUS: methods ported (vt-3c). The `Resize` struct (+ `Cursor`),
//! `trailingBlankLines`, `resize`, `resizeWithoutReflow`,
//! `resizeWithoutReflowGrowCols`, `trimTrailingBlankRows`, `resizeCols`, and the
//! entire `ReflowCursor` helper (`init`, `reflowRow`, `writeCell`,
//! `moveLastRowToNewPage`, `increaseCapacity`, `bottom`, `cursorForward`,
//! `cursorScroll`, `cursorNewPage`, `cursorScrollOrNewPage`, `cursorAbsolute`,
//! `countTrailingEmptyCells`, `copyRowMetadata`) all land here fully ported. The
//! 205-block test split lands in the dedicated test phase.

use super::grow::{IncreaseCapacity, IncreaseCapacityError};
use super::iter::Direction;
use super::{Node, PageList, Pin};
use crate::page as pagepkg;
use crate::page::{Adjustment, Capacity, Cell, ContentTag, Page, Row, Wide};
use crate::pagelist::pin::CellSubset;
use crate::point;
use crate::style as stylepkg;
use rustty_render_types::term_size as size;

// PORT NOTE: `build_options.kitty_graphics`. The reference enables the Kitty
// graphics protocol by default for the ghostty artifact; reflow uses this only
// to set `Row.kitty_virtual_placeholder`. We keep the slot true so the layout
// and behavior match the reference's default build. (Mirrors the same const in
// `page.rs`, which is private to that module.)
const KITTY_GRAPHICS: bool = true;

// PORT NOTE: `kitty.graphics.unicode.placeholder` is U+10EEEE. The kitty module
// surface is ported by a different sub-batch; we inline the constant here (the
// only use is the codepoint comparison below). (Mirrors the same const in
// `page.rs`, which is private to that module.)
const KITTY_PLACEHOLDER: crate::page::u21 = 0x10EEEE;

/// `PageList.Resize`
//
// PORT NOTE: the reference `Resize` is a plain options struct with all-default
// fields. It ports to a Rust struct; the `?size.CellCountInt` optionals become
// `Option<size::CellCountInt>`, the optional cursor becomes `Option<Cursor>`,
// and `reflow` keeps its `true` default. A `Default` impl reproduces the
// reference field defaults (`cols = null`, `rows = null`, `reflow = true`,
// `cursor = null`).
//
// PORT NOTE: the reference `Resize` is a by-value options struct that `resize`
// copies (`var copy = opts`). We derive `Clone, Copy` so the same by-value copy
// is a bitwise copy, matching the reference.
#[derive(Clone, Copy)]
pub struct Resize {
    /// The new cols/cells of the screen.
    pub cols: Option<size::CellCountInt>,
    pub rows: Option<size::CellCountInt>,

    /// Whether to reflow the text. If this is false then the text will
    /// be truncated if the new size is smaller than the old size.
    pub reflow: bool,

    /// Set this to the current cursor position in the active area. Some
    /// resize/reflow behavior depends on the cursor position.
    pub cursor: Option<Cursor>,
}

impl Default for Resize {
    fn default() -> Resize {
        Resize {
            cols: None,
            rows: None,
            reflow: true,
            cursor: None,
        }
    }
}

/// `PageList.Resize.Cursor`
#[derive(Clone, Copy)]
pub struct Cursor {
    pub x: size::CellCountInt,
    pub y: size::CellCountInt,

    /// When set, this pin preserves right-side blank cells up to the cursor
    /// during reflow.
    pub pin: Option<*mut super::Pin>,
}

impl PageList {
    /// Returns the number of trailing blank lines, not to exceed max. Max
    /// is used to limit our traversal in the case of large scrollback.
    #[allow(dead_code)]
    fn trailing_blank_lines(&self, max: size::CellCountInt) -> size::CellCountInt {
        let mut count: size::CellCountInt = 0;

        // Go through our pages backwards since we're counting trailing blanks.
        let mut it = self.pages.last;
        while !it.is_null() {
            let page = it;
            let len = unsafe { (*page).data.size.rows };
            let base = unsafe { (*page).data.memory_ptr() };
            let rows = unsafe { (*page).data.rows.ptr(base) };
            for i in 0..len {
                let rev_i = len - i - 1;
                let row = unsafe { &*rows.add(rev_i as usize) };
                let cells = unsafe {
                    core::slice::from_raw_parts(
                        row.cells().ptr(base),
                        (*page).data.size.cols as usize,
                    )
                };

                // If the row has any text then we're done.
                if Cell::has_text_any(cells) {
                    return count;
                }

                // Inc count, if we're beyond max then we're done.
                count += 1;
                if count >= max {
                    return count;
                }
            }

            it = unsafe { (*page).prev };
        }

        count
    }

    /// Resize
    /// TODO: docs
    pub fn resize(&mut self, opts: Resize) -> Result<(), super::OutOfMemory> {
        // defer self.assertIntegrity();
        let result = self.resize_inner(opts);
        self.assert_integrity();
        result
    }

    // PORT NOTE: the reference `resize` ends with `defer self.assertIntegrity()`.
    // Rust has no `defer`, so the body is factored into `resize_inner` and the
    // public `resize` wraps it to run `assert_integrity()` on every exit path.
    fn resize_inner(&mut self, opts: Resize) -> Result<(), super::OutOfMemory> {
        if cfg!(debug_assertions) {
            // Resize does not work with 0 values, this should be protected
            // upstream
            if let Some(v) = opts.cols {
                assert!(v > 0);
            }
            if let Some(v) = opts.rows {
                assert!(v > 0);
            }
        }

        // Resizing (especially with reflow) can cause our row offset to
        // become invalid. Rather than do something fancy like we do other
        // places and try to update it in place, we just invalidate it because
        // its too easy to get the logic wrong in here.
        self.viewport_pin_row_offset = None;

        if !opts.reflow {
            return self.resize_without_reflow(opts);
        }

        // Recalculate our minimum max size. This allows grow to work properly
        // when increasing beyond our initial minimum max size or explicit max
        // size to fit the active area.
        let old_min_max_size = self.min_max_size;
        self.min_max_size = Self::min_max_size(
            opts.cols.unwrap_or(self.cols),
            opts.rows.unwrap_or(self.rows),
        );
        // errdefer self.min_max_size = old_min_max_size;

        // On reflow, the main thing that causes reflow is column changes. If
        // only rows change, reflow is impossible. So we change our behavior based
        // on the change of columns.
        let cols = opts.cols.unwrap_or(self.cols);
        let r: Result<(), super::OutOfMemory> = match cols.cmp(&self.cols) {
            core::cmp::Ordering::Equal => self.resize_without_reflow(opts),

            core::cmp::Ordering::Greater => {
                // We grow rows after cols so that we can do our unwrapping/reflow
                // before we do a no-reflow grow.
                self.resize_cols(cols, opts.cursor)
                    .and_then(|()| self.resize_without_reflow(opts))
            }

            core::cmp::Ordering::Less => {
                // We first change our row count so that we have the proper amount
                // we can use when shrinking our cols.
                self.resize_without_reflow({
                    let mut copy = opts;
                    copy.cols = Some(self.cols);
                    copy
                })
                .and_then(|()| self.resize_cols(cols, opts.cursor))
            }
        };
        if let Err(e) = r {
            // errdefer self.min_max_size = old_min_max_size;
            self.min_max_size = old_min_max_size;
            return Err(e);
        }

        // Various resize operations can change our total row count such
        // that our viewport pin is now in the active area and has insufficient
        // space. We need to check for this case and fix it up.
        match self.viewport {
            super::Viewport::Pin => {
                if self.pin_is_active(unsafe { (*self.viewport_pin).pin_copy() }) {
                    self.viewport = super::Viewport::Active;
                }
            }
            super::Viewport::Active | super::Viewport::Top => {}
        }

        Ok(())
    }

    /// Resize the pagelist with reflow by adding or removing columns.
    fn resize_cols(
        &mut self,
        cols: size::CellCountInt,
        cursor: Option<Cursor>,
    ) -> Result<(), super::OutOfMemory> {
        assert!(cols != self.cols);

        // If we have a cursor position (x,y), then we try under any col resizing
        // to keep the same number remaining active rows beneath it. This is a
        // very special case if you can imagine clearing the screen (i.e.
        // scrollClear), having an empty active area, and then resizing to less
        // cols then we don't want the active area to "jump" to the bottom and
        // pull down scrollback.
        //
        // PORT NOTE: the reference `preserved_cursor` is an anonymous optional
        // struct; we name it `PreservedCursor` (defined below this method) so the
        // `?struct { ... }` becomes `Option<PreservedCursor>`. The `c.pin`
        // (`?*Pin`) becomes `Option<*mut Pin>`.
        let preserved_cursor: Option<PreservedCursor> = if let Some(c) = cursor {
            'cursor: {
                let p = if let Some(cursor_pin) = c.pin {
                    unsafe { (*cursor_pin).pin_copy() }
                } else {
                    match self.pin(point::Point::Active(point::Coordinate {
                        x: c.x,
                        y: c.y as u32,
                    })) {
                        Some(v) => v,
                        None => break 'cursor None,
                    }
                };

                let active_pin = self.pin(point::Point::Active(point::Coordinate::default()));

                // We count how many wraps the cursor had before it to begin with
                // so that we can offset any additional wraps to avoid pushing the
                // original row contents in to the scrollback.
                let wrapped = 'wrapped: {
                    let mut wrapped: usize = 0;

                    // If shrinking rows (in the .lt branch of resize, rows shrink
                    // before we get here) pushed the cursor pin above the new active
                    // area, there are no rows to count and iterating .left_up toward
                    // the active-area top would be an invalid (reversed) range. The
                    // preserved-cursor growth below already no-ops for a cursor that
                    // isn't in the active area, so we just count zero here.
                    // PORT NOTE: `Pin` is not `Copy`, so we borrow the optional
                    // active pin (`pin_copy` reconstructs a value pin for the
                    // `before` call) to avoid consuming `active_pin` before the
                    // `row_iterator` limit below.
                    if let Some(ap) = active_pin.as_ref() {
                        if p.before(ap.pin_copy()) {
                            break 'wrapped 0;
                        }
                    }

                    let mut row_it = p.row_iterator(Direction::LeftUp, active_pin);
                    while let Some(next) = row_it.next() {
                        let row = next.row_and_cell().row;
                        if unsafe { (*row).wrap_continuation() } {
                            wrapped += 1;
                        }
                    }

                    break 'wrapped wrapped;
                };

                Some(PreservedCursor {
                    tracked_pin: match c.pin {
                        Some(pin) => pin,
                        None => self.track_pin(p)?,
                    },
                    untrack: c.pin.is_none(),
                    remaining_rows: (self.rows as usize).saturating_sub(c.y as usize + 1),
                    wrapped_rows: wrapped,
                })
            }
        } else {
            None
        };
        // defer if (preserved_cursor) |c| {
        //     if (c.untrack) self.untrackPin(c.tracked_pin);
        // };
        //
        // PORT NOTE: Rust has no `defer`. The reference body has a single
        // fallible tail with several `try` points; on any error path the
        // untrack must still run. We factor the body into `resize_cols_inner`
        // and run the untrack here on every exit.
        let result = self.resize_cols_inner(cols, preserved_cursor);
        if let Some(c) = preserved_cursor {
            if c.untrack {
                self.untrack_pin(c.tracked_pin);
            }
        }
        result
    }

    fn resize_cols_inner(
        &mut self,
        cols: size::CellCountInt,
        preserved_cursor: Option<PreservedCursor>,
    ) -> Result<(), super::OutOfMemory> {
        // Update our cols. We have to do this early because grow() that we
        // may call below relies on this to calculate the proper page size, but
        // after preserved_cursor so that the cursor pin can resolve coordinates in
        // the old active coordinate space.
        self.cols = cols;

        // Create the first node that contains our reflow.
        let first_rewritten_node: *mut Node = {
            let page: *mut Page = unsafe { &mut (*self.pages.first).data };
            let cap = match unsafe { (*page).capacity }.adjust(Adjustment { cols: Some(cols) }) {
                Ok(cap) => cap,
                Err(_err) => {
                    // comptime assert(@TypeOf(err) == error{OutOfMemory});

                    // We verify all maxed out page layouts work.
                    let mut cap = unsafe { (*page).capacity };
                    cap.cols = cols;

                    // We're growing columns so we can only get less rows so use
                    // the lesser of our capacity and size so we minimize wasted
                    // rows.
                    cap.rows = core::cmp::min(unsafe { (*page).size.rows }, cap.rows);
                    cap
                }
            };

            let node = self.create_page(super::CreatePage::new(cap))?;
            unsafe {
                (*node).data.size.rows = 1;
            }
            node
        };

        // We need to grab our rowIterator now before we rewrite our
        // linked list below.
        let mut it = self.row_iterator(
            Direction::RightDown,
            point::Point::Screen(point::Coordinate { x: 0, y: 0 }),
            None,
        );
        // errdefer {
        //     // If an error occurs, we're in a pretty disastrous broken state,
        //     // but we should still try to clean up our leaked memory. Free
        //     // any of the remaining orphaned pages from before. If we reflowed
        //     // successfully this will be null.
        //     var node_: ?*Node = if (it.chunk) |chunk| chunk.node else null;
        //     while (node_) |node| {
        //         node_ = node.next;
        //         self.destroyNode(node);
        //     }
        // }
        //
        // PORT NOTE: this reference `errdefer` cleans up orphaned pages on error.
        // We replay it inline at each fallible exit below (the `reflowRow` and
        // `grow` error arms).
        macro_rules! errdefer_cleanup {
            () => {{
                let mut node_: *mut Node = match it.chunk {
                    Some(chunk) => chunk.node,
                    None => core::ptr::null_mut(),
                };
                while !node_.is_null() {
                    let node = node_;
                    node_ = unsafe { (*node).next };
                    self.destroy_node(node);
                }
            }};
        }

        // Set our new page as the only page. This orphans the existing pages
        // in the list, but that's fine since we're gonna delete them anyway.
        self.pages.first = first_rewritten_node;
        self.pages.last = first_rewritten_node;

        // Reflow all our rows.
        {
            let mut reflow_cursor: ReflowCursor = ReflowCursor::init(first_rewritten_node);
            while let Some(row) = it.next() {
                // PORT NOTE: `Pin` is not `Copy` in the Rust port, and the
                // reference reads `row.y`/`row.node` after passing `row` by value
                // to `reflowRow`. We capture those fields before the (moving)
                // call so the post-call reads match the reference.
                let row_y = row.y;
                let row_node = row.node;
                if let Err(e) =
                    reflow_cursor.reflow_row(self, row, preserved_cursor.map(|c| c.tracked_pin))
                {
                    errdefer_cleanup!();
                    return Err(e);
                }

                // Once we're done reflowing a page, destroy it immediately.
                // This frees memory and makes it more likely in memory
                // constrained environments that the next reflow will work.
                if row_y == unsafe { (*row_node).data.size.rows } - 1 {
                    self.destroy_node(row_node);
                }
            }

            // At the end of the reflow, setup our total row cache
            // log.warn("total old={} new={}", .{ self.total_rows, reflow_cursor.total_rows });
            self.total_rows = reflow_cursor.total_rows;
        }

        // If our total rows is less than our active rows, we need to grow.
        // This can happen if you're growing columns such that enough active
        // rows unwrap that we no longer have enough.
        {
            let mut node_it = self.pages.first;
            let mut total: usize = 0;
            let mut reached = false;
            while !node_it.is_null() {
                let node = node_it;
                total += unsafe { (*node).data.size.rows } as usize;
                if total >= self.rows as usize {
                    reached = true;
                    break;
                }
                node_it = unsafe { (*node).next };
            }
            if !reached {
                for _ in total..(self.rows as usize) {
                    if let Err(e) = self.grow() {
                        errdefer_cleanup!();
                        return Err(e);
                    }
                }
            }
        }

        // Reflow can unwrap enough rows that a history viewport pin lands in the
        // active area before we do any preserved-cursor growth below. Switch back
        // to the active viewport now so intermediate grow() integrity checks stay
        // valid.
        match self.viewport {
            super::Viewport::Active | super::Viewport::Top => {}
            super::Viewport::Pin => {
                if self.pin_is_active(unsafe { (*self.viewport_pin).pin_copy() }) {
                    self.viewport = super::Viewport::Active;
                }
            }
        }

        // See preserved_cursor setup for why.
        if let Some(c) = preserved_cursor {
            'cursor: {
                let active_pt = match self
                    .point_from_pin(point::Tag::Active, unsafe { (*c.tracked_pin).pin_copy() })
                {
                    Some(v) => v,
                    None => break 'cursor,
                };

                let active_pin = self.pin(point::Point::Active(point::Coordinate::default()));

                // We need to determine how many rows we wrapped from the original
                // and subtract that from the remaining rows we expect because if
                // we wrap down we don't want to push our original row contents into
                // the scrollback.
                let wrapped = 'wrapped: {
                    let mut wrapped: usize = 0;

                    let mut row_it =
                        unsafe { (*c.tracked_pin).row_iterator(Direction::LeftUp, active_pin) };
                    while let Some(next) = row_it.next() {
                        let row = next.row_and_cell().row;
                        if unsafe { (*row).wrap_continuation() } {
                            wrapped += 1;
                        }
                    }

                    break 'wrapped wrapped;
                };

                let current = (self.rows as usize).saturating_sub(active_pt.coord().y as usize + 1);

                let mut req_rows = c.remaining_rows;
                req_rows = req_rows.saturating_sub(wrapped.saturating_sub(c.wrapped_rows));
                req_rows = req_rows.saturating_sub(current);

                while req_rows > 0 {
                    if let Err(e) = self.grow() {
                        errdefer_cleanup!();
                        return Err(e);
                    }
                    req_rows -= 1;
                }
            }
        }

        Ok(())
    }

    pub(crate) fn resize_without_reflow(&mut self, opts: Resize) -> Result<(), super::OutOfMemory> {
        // We only set the new min_max_size if we're not reflowing. If we are
        // reflowing, then resize handles this for us.
        let old_min_max_size = self.min_max_size;
        self.min_max_size = if !opts.reflow {
            Self::min_max_size(
                opts.cols.unwrap_or(self.cols),
                opts.rows.unwrap_or(self.rows),
            )
        } else {
            old_min_max_size
        };
        // errdefer self.min_max_size = old_min_max_size;

        // PORT NOTE: the reference `errdefer self.min_max_size = old_min_max_size`
        // restores `min_max_size` on any error below. Both fallible paths
        // (`resizeWithoutReflowGrowCols` and `grow`) restore it before returning
        // the error; on success the restore is skipped.

        // Important! We have to do cols first because cols may cause us to
        // destroy pages if we're increasing cols which will free up page_size
        // so that when we call grow() in the row mods, we won't prune.
        if let Some(cols) = opts.cols {
            // Any column change without reflow should not result in row counts
            // changing.
            let old_total_rows = self.total_rows;
            // defer assert(self.total_rows == old_total_rows);

            match cols.cmp(&self.cols) {
                core::cmp::Ordering::Equal => {}

                // Making our columns smaller. We always have space for this
                // in existing pages so we need to go through the pages,
                // resize the columns, and clear any cells that are beyond
                // the new size.
                core::cmp::Ordering::Less => {
                    let mut it = self.page_iterator(
                        Direction::RightDown,
                        point::Point::Screen(point::Coordinate { x: 0, y: 0 }),
                        None,
                    );
                    while let Some(chunk) = it.next() {
                        let page: *mut crate::page::Page = unsafe { &mut (*chunk.node).data };
                        // defer page.assertIntegrity();
                        let base = unsafe { (*page).memory_ptr() };
                        let rows = unsafe { (*page).rows.ptr(base) };
                        for i in 0..unsafe { (*page).size.rows } {
                            let row = unsafe { rows.add(i as usize) };
                            unsafe {
                                (*page).clear_cells(row, cols as usize, self.cols as usize);
                            }
                        }

                        unsafe {
                            (*page).size.cols = cols;
                        }
                        unsafe {
                            (*page).assert_integrity();
                        }
                    }

                    // Update all our tracked pins. If they have an X
                    // beyond the edge, clamp it.
                    for p in self.tracked_pins.keys() {
                        let p: *mut Pin = *p;
                        if unsafe { (*p).x } >= cols {
                            unsafe {
                                (*p).x = cols - 1;
                            }
                        }
                    }

                    self.cols = cols;
                }

                // Make our columns larger. This is a bit more complicated because
                // pages may not have the capacity for this. If they don't have
                // the capacity we need to allocate a new page and copy the data.
                core::cmp::Ordering::Greater => {
                    // See the comment in the while loop when setting self.cols
                    let old_cols = self.cols;

                    let mut it = self.page_iterator(
                        Direction::RightDown,
                        point::Point::Screen(point::Coordinate { x: 0, y: 0 }),
                        None,
                    );
                    while let Some(chunk) = it.next() {
                        // We need to restore our old cols after we resize because
                        // we have an assertion on this and we want to be able to
                        // call this method multiple times.
                        self.cols = old_cols;
                        if let Err(e) = self.resize_without_reflow_grow_cols(cols, chunk) {
                            // errdefer self.min_max_size = old_min_max_size;
                            self.min_max_size = old_min_max_size;
                            return Err(e);
                        }
                    }

                    self.cols = cols;
                }
            }

            // defer assert(self.total_rows == old_total_rows);
            assert!(self.total_rows == old_total_rows);
        }

        if let Some(rows) = opts.rows {
            match rows.cmp(&self.rows) {
                core::cmp::Ordering::Equal => {}

                // Making rows smaller, we simply change our rows value. Changing
                // the row size doesn't affect anything else since max size and
                // so on are all byte-based.
                core::cmp::Ordering::Less => {
                    // If our rows are shrinking, we prefer to trim trailing
                    // blank lines from the active area instead of creating
                    // history if we can.
                    //
                    // This matches macOS Terminal.app behavior. I chose to match that
                    // behavior because it seemed fine in an ocean of differing behavior
                    // between terminal apps. I'm completely open to changing it as long
                    // as resize behavior isn't regressed in a user-hostile way.
                    let trimmed = self.trim_trailing_blank_rows(self.rows - rows);

                    // Account for our trimmed rows in the total row cache
                    self.total_rows -= trimmed as usize;

                    // If we didn't trim enough, just modify our row count and this
                    // will create additional history.
                    self.rows = rows;
                }

                // Making rows larger we adjust our row count, and then grow
                // to the row count.
                core::cmp::Ordering::Greater => 'gt: {
                    // If our rows increased and our cursor is NOT at the bottom,
                    // we want to try to preserve the y value of the old cursor.
                    // In other words, we don't want to "pull down" scrollback.
                    // This is purely a UX feature.
                    if let Some(cursor) = opts.cursor {
                        'cursor: {
                            if cursor.y >= self.rows - 1 {
                                break 'cursor;
                            }

                            // Cursor is not at the bottom, so we just grow our
                            // rows and we're done. Cursor does NOT change for this
                            // since we're not pulling down scrollback.
                            let delta = rows - self.rows;
                            self.rows = rows;
                            for _ in 0..delta {
                                let _ = self.grow()?;
                            }
                            break 'gt;
                        }
                    }

                    // This must be set BEFORE any calls to grow() so that
                    // grow() doesn't prune pages that we need for the active
                    // area.
                    self.rows = rows;

                    // Cursor is at the bottom or we don't care about cursors.
                    // In this case, if we have enough rows in our pages, we
                    // just update our rows and we're done. This effectively
                    // "pulls down" scrollback.
                    //
                    // If we don't have enough scrollback, we add the difference,
                    // to the active area.
                    let mut count: usize = 0;
                    let mut page = self.pages.first;
                    let mut reached = false;
                    while !page.is_null() {
                        let p = page;
                        count += unsafe { (*p).data.size.rows } as usize;
                        if count >= rows as usize {
                            reached = true;
                            break;
                        }
                        page = unsafe { (*p).next };
                    }
                    if !reached {
                        assert!(count < rows as usize);
                        for _ in count..(rows as usize) {
                            let _ = self.grow()?;
                        }
                    }

                    // Make sure that the viewport pin isn't below the active
                    // area, since that will lead to all sorts of problems.
                    match self.viewport {
                        super::Viewport::Pin => {
                            if self.pin_is_active(unsafe { (*self.viewport_pin).pin_copy() }) {
                                self.viewport = super::Viewport::Active;
                            }
                        }
                        super::Viewport::Active | super::Viewport::Top => {}
                    }
                }
            }

            // PORT NOTE: the reference guards this assert behind
            // `build_options.slow_runtime_safety`; the crate maps that to
            // `cfg!(debug_assertions)` (see `pin_is_valid` in `mod.rs`,
            // `split` in `grow.rs`).
            if cfg!(debug_assertions) {
                // We never have less rows than our active screen has.
                assert!(self.total_rows() >= self.rows as usize);
            }
        }

        Ok(())
    }

    fn resize_without_reflow_grow_cols(
        &mut self,
        cols: size::CellCountInt,
        chunk: super::iter::Chunk,
    ) -> Result<(), super::OutOfMemory> {
        assert!(cols > self.cols);
        // PORT NOTE: raw `*mut` via addr_of_mut! (not `&mut .. as *mut`). The
        // reference `const page = &chunk.node.data` is a Zig pointer; a Rust
        // `&mut`-derived raw ptr gets a Unique tag that the page-splitting loop
        // below invalidates (create_page / row copies retag the same data),
        // causing a Stacked-Borrows use-after-invalidate. addr_of_mut! keeps a
        // plain raw pointer with no Unique parent. (Miri-verified.)
        let page: *mut crate::page::Page = unsafe { core::ptr::addr_of_mut!((*chunk.node).data) };

        // Update our col count
        let old_cols = self.cols;
        self.cols = cols;
        // errdefer self.cols = old_cols;

        // PORT NOTE: the reference `errdefer self.cols = old_cols` restores
        // `self.cols` on any error below. The only fallible op past this point is
        // `createPage`; we restore `self.cols` before returning that error (the
        // post-`errdefer comptime unreachable` tail is infallible).

        // Unlikely fast path: we have capacity in the page. This
        // is only true if we resized to less cols earlier.
        if unsafe { (*page).capacity.cols } >= cols {
            'fast: {
                // If any row has a spacer head at the old last column, it will
                // be invalid at the new (wider) size. Fall through to the slow
                // path which handles spacer heads correctly via cloneRowFrom.
                let base = unsafe { (*page).memory_ptr() };
                let rows = unsafe { (*page).rows.ptr(base) };
                for i in 0..unsafe { (*page).size.rows } {
                    let row = unsafe { rows.add(i as usize) };
                    let cells = unsafe { (*page).get_cells(row) };
                    let cell = unsafe { &*(cells as *mut Cell).add(old_cols as usize - 1) };
                    if cell.wide() == Wide::SpacerHead {
                        break 'fast;
                    }
                }

                unsafe {
                    (*page).size.cols = cols;
                }
                return Ok(());
            }
        }

        // Likely slow path: we don't have capacity, so we need
        // to allocate a page, and copy the old data into it.

        // Try to fit our new column size into our existing page capacity.
        // If that doesn't work then use a non-standard page with the
        // given columns.
        let cap = match unsafe { (*page).capacity }
            .adjust(crate::page::Adjustment { cols: Some(cols) })
        {
            Ok(cap) => cap,
            Err(_err) => {
                // comptime assert(@TypeOf(err) == error{OutOfMemory});

                // We verify all maxed out page layouts don't overflow,
                let mut cap = unsafe { (*page).capacity };
                cap.cols = cols;

                // We're growing columns so we can only get less rows so use
                // the lesser of our capacity and size so we minimize wasted
                // rows.
                cap.rows = core::cmp::min(unsafe { (*page).size.rows }, cap.rows);
                cap
            }
        };

        // On error, we need to undo all the pages we've added.
        let prev = unsafe { (*chunk.node).prev };
        // errdefer {
        //     var current = chunk.node.prev;
        //     while (current) |p| {
        //         if (current == prev) break;
        //         current = p.prev;
        //         self.pages.remove(p);
        //         self.destroyNode(p);
        //     }
        // }
        //
        // PORT NOTE: this reference `errdefer` removes any new pages added before
        // an error. In Rust we replay the same cleanup inline at each fallible
        // exit below (see the `createPage` error arm).

        // Keeps track of all our copied rows. Assertions at the end is that
        // we copied exactly our page size.
        let mut copied: size::CellCountInt = 0;

        // This function has an unfortunate side effect in that it causes memory
        // fragmentation on rows if the columns are increasing in a way that
        // shrinks capacity rows. If we have pages that don't divide evenly then
        // we end up creating a final page that is not using its full capacity.
        // If this chunk isn't the last chunk in the page list, then we've created
        // a page where we'll never reclaim that capacity. This makes our max size
        // calculation incorrect since we'll throw away data even though we have
        // excess capacity. To avoid this, we try to fill our previous page
        // first if it has capacity.
        //
        // This can fail for many reasons (can't fit styles/graphemes, etc.) so
        // if it fails then we give up and drop back into creating new pages.
        if !prev.is_null() {
            'prev: {
                let prev_node = prev;
                let prev_page: *mut crate::page::Page = unsafe { &mut (*prev_node).data };

                // We only want scenarios where we have excess capacity.
                if unsafe { (*prev_page).size.rows } >= unsafe { (*prev_page).capacity.rows } {
                    break 'prev;
                }

                // We can copy as much as we can to fill the capacity or our
                // current page size.
                let len = core::cmp::min(
                    unsafe { (*prev_page).capacity.rows } - unsafe { (*prev_page).size.rows },
                    unsafe { (*page).size.rows },
                );

                let src_base = unsafe { (*page).memory_ptr() };
                let src_rows = unsafe { (*page).rows.ptr(src_base) };
                let dst_base = unsafe { (*prev_page).memory_ptr() };
                let dst_rows = unsafe {
                    (*prev_page)
                        .rows
                        .ptr(dst_base)
                        .add((*prev_page).size.rows as usize)
                };
                let mut broke = false;
                for i in 0..len {
                    let dst_row = unsafe { dst_rows.add(i as usize) };
                    let src_row = unsafe { src_rows.add(i as usize) };
                    unsafe {
                        (*prev_page).size.rows += 1;
                    }
                    copied += 1;
                    if unsafe { (*prev_page).clone_row_from(page, dst_row, src_row) }.is_err() {
                        // If an error happens, we undo our row copy and break out
                        // into creating a new page.
                        unsafe {
                            (*prev_page).size.rows -= 1;
                        }
                        copied -= 1;
                        broke = true;
                        break;
                    }
                }
                if broke {
                    break 'prev;
                }

                assert!(copied == len);
                assert!(unsafe { (*prev_page).size.rows } <= unsafe { (*prev_page).capacity.rows });

                // Remap any tracked pins that pointed to rows we just copied to prev.
                for p in self.tracked_pins.keys() {
                    let p: *mut Pin = *p;
                    if !core::ptr::eq(unsafe { (*p).node }, chunk.node) || unsafe { (*p).y } >= len
                    {
                        continue;
                    }
                    unsafe {
                        (*p).node = prev_node;
                        (*p).y += (*prev_page).size.rows - len;
                    }
                }
            }
        }

        // If we have an error, we clear the rows we just added to our prev page.
        let prev_copied = copied;
        // errdefer if (prev_copied > 0) { ... }
        //
        // PORT NOTE: this reference `errdefer` (and the one below) unwinds the
        // rows we copied into `prev` and any pages we inserted, on error from
        // `createPage`. We replay both inline at the `createPage` error arm.

        // We delete any of the nodes we added.
        // errdefer {
        //     var it = chunk.node.prev;
        //     while (it) |node| {
        //         if (node == prev) break;
        //         it = node.prev;
        //         self.pages.remove(node);
        //         self.destroyNode(node);
        //     }
        // }

        // We need to loop because our col growth may force us
        // to split pages.
        while copied < unsafe { (*page).size.rows } {
            let new_node = match self.create_page(super::CreatePage::new(cap)) {
                Ok(n) => n,
                Err(e) => {
                    // errdefer (replayed): delete any of the nodes we added,
                    // then clear the rows we just added to our prev page, then
                    // restore self.cols.
                    let mut it = unsafe { (*chunk.node).prev };
                    while !it.is_null() {
                        let node = it;
                        if core::ptr::eq(node, prev) {
                            break;
                        }
                        it = unsafe { (*node).prev };
                        unsafe {
                            self.pages.remove(node);
                        }
                        self.destroy_node(node);
                    }
                    if prev_copied > 0 {
                        let prev_page: *mut crate::page::Page = unsafe { &mut (*prev).data };
                        let prev_size = unsafe { (*prev_page).size.rows } - prev_copied;
                        let base = unsafe { (*prev_page).memory_ptr() };
                        let prev_rows = unsafe { (*prev_page).rows.ptr(base) };
                        for i in prev_size..unsafe { (*prev_page).size.rows } {
                            let row = unsafe { prev_rows.add(i as usize) };
                            unsafe {
                                (*prev_page).clear_cells(row, 0, (*prev_page).size.cols as usize);
                            }
                        }
                        unsafe {
                            (*prev_page).size.rows = prev_size;
                        }
                    }
                    self.cols = old_cols;
                    return Err(e);
                }
            };
            // defer new_node.data.assertIntegrity();

            // The length we can copy into the new page is at most the number
            // of rows in our cap. But if we can finish our source page we use that.
            let len = core::cmp::min(cap.rows, unsafe { (*page).size.rows } - copied);

            // Perform the copy
            let y_start = copied;
            let src_base = unsafe { (*page).memory_ptr() };
            let src_rows = unsafe { (*page).rows.ptr(src_base).add(y_start as usize) };
            let dst_base = unsafe { (*new_node).data.memory_ptr() };
            let dst_rows = unsafe { (*new_node).data.rows.ptr(dst_base) };
            for i in 0..len {
                let dst_row = unsafe { dst_rows.add(i as usize) };
                let src_row = unsafe { src_rows.add(i as usize) };
                unsafe {
                    (*new_node).data.size.rows += 1;
                }
                match unsafe { (*new_node).data.clone_row_from(page, dst_row, src_row) } {
                    Ok(_) => {
                        copied += 1;
                    }
                    Err(_err) => {
                        // I don't THINK this should be possible, because while our
                        // row count may diminish due to the adjustment, our
                        // prior capacity should have been sufficient to hold all the
                        // managed memory.
                        // log.warn(
                        //     "unexpected cloneRowFrom failure during resizeWithoutReflowGrowCols: {}",
                        //     .{err},
                        // );
                        //
                        // PORT NOTE: the reference `log` is `std.log.scoped(.page_list)`
                        // (commented out at the module top). We preserve the warning
                        // verbatim above; the scoped logger is not yet wired in the
                        // Rust port.

                        // We can actually safely handle this though by exiting
                        // this loop early and cutting our copy short.
                        unsafe {
                            (*new_node).data.size.rows -= 1;
                        }
                        break;
                    }
                }
            }
            let y_end = copied;

            // Insert our new page
            unsafe {
                self.pages.insert_before(chunk.node, new_node);
            }

            unsafe {
                (*new_node).data.assert_integrity();
            }

            // Update our tracked pins that pointed to this previous page.
            for p in self.tracked_pins.keys() {
                let p: *mut Pin = *p;
                if !core::ptr::eq(unsafe { (*p).node }, chunk.node)
                    || unsafe { (*p).y } < y_start
                    || unsafe { (*p).y } >= y_end
                {
                    continue;
                }
                unsafe {
                    (*p).node = new_node;
                    (*p).y -= y_start;
                }
            }
        }
        assert!(copied == unsafe { (*page).size.rows });

        // Our prior errdeferes are invalid after this point so ensure
        // we don't have any more errors.
        // errdefer comptime unreachable;

        // Remove the old page.
        // Deallocate the old page.
        unsafe {
            self.pages.remove(chunk.node);
        }
        self.destroy_node(chunk.node);

        Ok(())
    }

    /// Trims up to max trailing blank rows from the pagelist and returns the
    /// number of rows trimmed. A blank row is any row with no text (but may
    /// have styling).
    ///
    /// IMPORTANT: This function does NOT update `total_rows`. It returns the
    /// number of rows trimmed, and the caller is responsible for decrementing
    /// `total_rows` by this amount.
    fn trim_trailing_blank_rows(&mut self, max: size::CellCountInt) -> size::CellCountInt {
        let mut trimmed: size::CellCountInt = 0;
        let bl_pin = self.get_bottom_right(point::Tag::Screen).unwrap();
        let mut it = bl_pin.row_iterator(Direction::LeftUp, None);
        while let Some(row_pin) = it.next() {
            let cells = row_pin.cells(CellSubset::All);

            // If the row has any text then we're done.
            if Cell::has_text_any(unsafe { &*cells }) {
                return trimmed;
            }

            // If our tracked pins are in this row then we cannot trim it
            // because it implies some sort of importance. If we trimmed this
            // we'd invalidate this pin, as well.
            for p in self.tracked_pins.keys() {
                let p: *mut Pin = *p;
                if !core::ptr::eq(unsafe { (*p).node }, row_pin.node)
                    || unsafe { (*p).y } != row_pin.y
                {
                    continue;
                }
                return trimmed;
            }

            // No text, we can trim this row. Because it has
            // no text we can also be sure it has no styling
            // so we don't need to worry about memory.
            unsafe {
                (*row_pin.node).data.size.rows -= 1;
            }
            if unsafe { (*row_pin.node).data.size.rows } == 0 {
                self.erase_page(row_pin.node);
            } else {
                unsafe {
                    (*row_pin.node).data.assert_integrity();
                }
            }

            trimmed += 1;
            if trimmed >= max {
                return trimmed;
            }
        }

        trimmed
    }
}

// PORT NOTE: the reference `resizeCols` `preserved_cursor` local is an anonymous
// optional struct (`?struct { tracked_pin, untrack, remaining_rows,
// wrapped_rows }`). Rust has no anonymous struct types that can be named in a
// `let` binding's type, so we name it here. It derives `Clone, Copy` so the
// by-value capture in the `defer`/error-tail mirrors the reference's value
// semantics.
#[derive(Clone, Copy)]
struct PreservedCursor {
    tracked_pin: *mut Pin,
    untrack: bool,
    remaining_rows: usize,
    wrapped_rows: usize,
}

// We use a cursor to track where we are in the src/dst. This is very
// similar to Screen.Cursor, so see that for docs on individual fields.
// We don't use a Screen because we don't need all the same data and we
// do our best to optimize having direct access to the page memory.
struct ReflowCursor {
    x: size::CellCountInt,
    y: size::CellCountInt,
    pending_wrap: bool,
    node: *mut Node,
    page: *mut Page,
    page_row: *mut Row,
    page_cell: *mut Cell,
    new_rows: usize,

    /// This is the final row count of the reflowed pages.
    total_rows: usize,
}

// PORT NOTE: the reference `writeCell` returns `IncreaseCapacityError!enum {
// success, repeat, skip_next }`. We name that anonymous enum `WriteCellResult`.
enum WriteCellResult {
    Success,
    Repeat,
    SkipNext,
}

impl ReflowCursor {
    fn init(node: *mut Node) -> ReflowCursor {
        let page: *mut Page = unsafe { &mut (*node).data };
        let base = unsafe { (*page).memory_ptr() };
        let rows = unsafe { (*page).rows.ptr(base) };
        ReflowCursor {
            x: 0,
            y: 0,
            pending_wrap: false,
            node,
            page,
            page_row: unsafe { rows.add(0) },
            page_cell: unsafe { (*rows.add(0)).cells().ptr(base).add(0) },
            new_rows: 0,

            // Initially whatever size our input node is.
            total_rows: unsafe { (*node).data.size.rows } as usize,
        }
    }

    /// Reflow the provided row in to this cursor.
    fn reflow_row(
        &mut self,
        list: &mut PageList,
        row: Pin,
        cursor_pin: Option<*mut Pin>,
    ) -> Result<(), super::OutOfMemory> {
        let src_page: *mut Page = unsafe { &mut (*row.node).data };
        let src_row = row.row_and_cell().row;
        let src_y = row.y;
        let cells = unsafe {
            core::slice::from_raw_parts(
                (*src_row).cells().ptr((*src_page).memory_ptr()),
                (*src_page).size.cols as usize,
            )
        };

        // Calculate the columns in this row. First up we trim non-semantic
        // rightmost blanks.
        let mut cols_len = unsafe { (*src_page).size.cols };
        if !unsafe { (*src_row).wrap() } {
            while cols_len > 0 {
                if !cells[cols_len as usize - 1].is_empty() {
                    break;
                }
                cols_len -= 1;
            }

            // If the row has a semantic prompt then the blank row is meaningful
            // so we just consider pretend the first cell of the row isn't empty.
            if cols_len == 0
                && unsafe { (*src_row).semantic_prompt() } != pagepkg::SemanticPrompt::None
            {
                cols_len = 1;
            }
        }

        // Handle tracked pin adjustments.
        {
            for p in list.tracked_pins.keys() {
                let p: *mut Pin = *p;
                if !core::ptr::eq(unsafe { &(*(*p).node).data }, src_page)
                    || unsafe { (*p).y } != src_y
                {
                    continue;
                }

                if cursor_pin.is_some() && p == cursor_pin.unwrap() {
                    continue;
                }

                // If this pin is in the blanks on the right and past the end
                // of the dst col width then we move it to the end of the dst
                // col width instead.
                if unsafe { (*p).x } >= cols_len {
                    unsafe {
                        (*p).x = core::cmp::min((*p).x, (*self.page).size.cols - 1 - self.x);
                    }
                }

                // We increase our col len to at least include this pin.
                // This ensures that blank rows with pins are processed,
                // so that the pins can be properly remapped.
                cols_len = core::cmp::max(cols_len, unsafe { (*p).x } + 1);
            }
        }

        // If the cursor is after blanks on the right, those cells are still
        // before the next write and must reflow with it.
        if let Some(p) = cursor_pin {
            if core::ptr::eq(unsafe { &(*(*p).node).data }, src_page) && unsafe { (*p).y } == src_y
            {
                cols_len = core::cmp::max(cols_len, unsafe { (*p).x } + 1);
            }
        }

        // Defer processing of blank rows so that blank rows
        // at the end of the page list are never written.
        if cols_len == 0 {
            // If this blank row was a wrap continuation somehow
            // then we won't need to write it since it should be
            // a part of the previously written row.
            if !unsafe { (*src_row).wrap_continuation() } {
                self.new_rows += 1;
            }
            return Ok(());
        }

        // Inherit increased styles or grapheme bytes from the src page
        // we're reflowing from for new pages.
        let cap = match unsafe { (*src_page).capacity }.adjust(Adjustment {
            cols: Some(unsafe { (*self.page).size.cols }),
        }) {
            Ok(cap) => cap,
            Err(_err) => {
                // comptime assert(@TypeOf(err) == error{OutOfMemory});

                let mut cap = unsafe { (*src_page).capacity };
                cap.cols = unsafe { (*self.page).size.cols };
                // We're already a non-standard page. We don't want to
                // inherit a massive set of rows, so cap it at our std size.
                cap.rows =
                    core::cmp::min(unsafe { (*src_page).size.rows }, super::std_capacity().rows);
                cap
            }
        };

        // Our row isn't blank, write any new rows we deferred.
        while self.new_rows > 0 {
            self.cursor_scroll_or_new_page(list, cap)?;
            self.new_rows -= 1;
        }

        self.copy_row_metadata(src_row);

        let mut x: usize = 0;
        while x < cols_len as usize {
            if self.pending_wrap {
                unsafe {
                    (*self.page_row).set_wrap(true);
                }
                self.cursor_scroll_or_new_page(list, cap)?;
                self.copy_row_metadata(src_row);
                unsafe {
                    (*self.page_row).set_wrap_continuation(true);
                }
            }

            // Move any tracked pins from the source.
            {
                for p in list.tracked_pins.keys() {
                    let p: *mut Pin = *p;
                    if !core::ptr::eq(unsafe { &(*(*p).node).data }, src_page)
                        || unsafe { (*p).y } != src_y
                        || unsafe { (*p).x } != x as size::CellCountInt
                    {
                        continue;
                    }

                    unsafe {
                        (*p).node = self.node;
                        (*p).x = self.x;
                        (*p).y = self.y;
                    }
                }
            }

            match self.write_cell(list, &cells[x], src_page) {
                Ok(result) => match result {
                    // Wrote the cell, move to the next.
                    WriteCellResult::Success => x += 1,

                    // Wrote the cell but request to skip the next so skip it.
                    // This is used for things like spacers.
                    WriteCellResult::SkipNext => {
                        // Remap any tracked pins at the skipped position (x+1)
                        // since we won't process that cell in the loop.
                        for p in list.tracked_pins.keys() {
                            let p: *mut Pin = *p;
                            if !core::ptr::eq(unsafe { &(*(*p).node).data }, src_page)
                                || unsafe { (*p).y } != src_y
                                || unsafe { (*p).x } != x as size::CellCountInt + 1
                            {
                                continue;
                            }

                            unsafe {
                                (*p).node = self.node;
                                (*p).x = self.x;
                                (*p).y = self.y;
                            }
                        }

                        x += 2;
                    }

                    // Didn't write the cell, repeat writing this same cell.
                    WriteCellResult::Repeat => {}
                },
                Err(err) => match err {
                    // System out of memory, we can't fix this.
                    IncreaseCapacityError::OutOfMemory => return Err(super::OutOfMemory),

                    // We reached the capacity of a single page and can't
                    // add any more of some type of managed memory. When this
                    // happens we split out the current row we're working on
                    // into a new page and continue from there.
                    IncreaseCapacityError::OutOfSpace => {
                        if self.y == 0 {
                            // If we're already on the first-row, we can't split
                            // any further, so we just ignore bad cells and take
                            // corrupted (but valid) cell contents.
                            // log.warn("reflowRow OutOfSpace on first row, discarding cell managed memory", .{});
                            x += 1;
                            self.cursor_forward();
                        } else {
                            // Move our last row to a new page.
                            self.move_last_row_to_new_page(list, cap)?;

                            // Do NOT increment x so that we retry writing
                            // the same existing cell.
                        }
                    }
                },
            }
        }

        // If the source row isn't wrapped then we should scroll afterwards.
        if !unsafe { (*src_row).wrap() } {
            self.new_rows += 1;
        }

        Ok(())
    }

    /// Write a cell. On error, this will not unwrite the cell but
    /// the cell may be incomplete (but valid). For example, if the source
    /// cell is styled and we failed to allocate space for styles, the
    /// written cell may not be styled but it is valid.
    ///
    /// The key failure to recognize for callers is when we can't increase
    /// capacity in our destination page. In this case, the caller may want
    /// to split the page at this row, rewrite the row into a new page
    /// and continue from there.
    ///
    /// But this function guarantees the terminal/page will be in a
    /// coherent state even on error.
    fn write_cell(
        &mut self,
        list: &mut PageList,
        cell: &Cell,
        src_page: *const Page,
    ) -> Result<WriteCellResult, IncreaseCapacityError> {
        // Initialize self.page_cell with basic, unmanaged memory contents.
        {
            // This must not fail because we want to make sure we atomically
            // setup our page cell to be valid.
            // errdefer comptime unreachable;

            // Copy cell contents.
            match cell.content_tag() {
                ContentTag::Codepoint | ContentTag::CodepointGrapheme => match cell.wide() {
                    Wide::Narrow => unsafe {
                        *self.page_cell = *cell;
                    },

                    Wide::Wide => {
                        if unsafe { (*self.page).size.cols } > 1 {
                            if self.x == unsafe { (*self.page).size.cols } - 1 {
                                // If there's a wide character in the last column of
                                // the reflowed page then we need to insert a spacer
                                // head and wrap before handling it.
                                let mut new_cell = Cell::from_bits(0);
                                new_cell.set_content_tag(ContentTag::Codepoint);
                                new_cell.set_content_codepoint(0);
                                new_cell.set_wide(Wide::SpacerHead);
                                unsafe {
                                    *self.page_cell = new_cell;
                                }

                                // Move to the next row (this sets pending wrap
                                // which will cause us to wrap on the next
                                // iteration).
                                self.cursor_forward();

                                // Decrement the source position so that when we
                                // loop we'll process this source cell again,
                                // since we can't copy it into a spacer head.
                                return Ok(WriteCellResult::Repeat);
                            } else {
                                unsafe {
                                    *self.page_cell = *cell;
                                }
                            }
                        } else {
                            // Edge case, when resizing to 1 column, wide
                            // characters are just destroyed and replaced
                            // with empty narrow cells.
                            unsafe {
                                (*self.page_cell).set_content_codepoint(0);
                                (*self.page_cell).set_wide(Wide::Narrow);
                            }
                            self.cursor_forward();

                            // Skip spacer tail so it doesn't cause a wrap.
                            return Ok(WriteCellResult::SkipNext);
                        }
                    }

                    Wide::SpacerTail => {
                        if unsafe { (*self.page).size.cols } > 1 {
                            unsafe {
                                *self.page_cell = *cell;
                            }
                        } else {
                            // Edge case, when resizing to 1 column, wide
                            // characters are just destroyed and replaced
                            // with empty narrow cells, so we should just
                            // discard any spacer tails.
                            return Ok(WriteCellResult::Success);
                        }
                    }

                    Wide::SpacerHead => {
                        // Spacer heads should be ignored. If we need a
                        // spacer head in our reflowed page, it is added
                        // when processing the wide cell it belongs to.
                        return Ok(WriteCellResult::Success);
                    }
                },

                ContentTag::BgColorPalette | ContentTag::BgColorRgb => {
                    // These are guaranteed to have no style or grapheme
                    // data associated with them so we can fast path them.
                    unsafe {
                        *self.page_cell = *cell;
                    }
                    self.cursor_forward();
                    return Ok(WriteCellResult::Success);
                }
            }

            // These will create issues by trying to clone managed memory that
            // isn't set if the current dst row needs to be moved to a new page.
            // They'll be fixed once we do properly copy the relevant memory.
            unsafe {
                (*self.page_cell).set_content_tag(ContentTag::Codepoint);
                (*self.page_cell).set_hyperlink(false);
                (*self.page_cell).set_style_id(stylepkg::DEFAULT_ID);
            }

            if KITTY_GRAPHICS {
                // Copy Kitty virtual placeholder status
                if cell.codepoint() == KITTY_PLACEHOLDER {
                    unsafe {
                        (*self.page_row).set_kitty_virtual_placeholder(true);
                    }
                }
            }
        }

        // std.log.warn("\nsrc_y={} src_x={} dst_y={} dst_x={} dst_cols={} cp={X} wide={} page_cell_wide={}", .{
        //     src_y,
        //     x,
        //     self.y,
        //     self.x,
        //     self.page.size.cols,
        //     cell.content.codepoint,
        //     cell.wide,
        //     self.page_cell.wide,
        // });

        // From this point on we're moving on to failable, managed memory.
        // If we reach an error, we do the minimal cleanup necessary to
        // not leave dangling memory but otherwise we gracefully degrade
        // into some functional but not strictly correct cell.

        // Copy grapheme data.
        if cell.content_tag() == ContentTag::CodepointGrapheme {
            // Copy the graphemes
            let cps = unsafe { Page::lookup_grapheme_of(src_page, cell) }.unwrap();

            // If our page can't support an additional cell
            // with graphemes then we increase capacity.
            if unsafe { (*self.page).grapheme_count() }
                >= unsafe { (*self.page).grapheme_capacity() }
            {
                self.increase_capacity(list, Some(IncreaseCapacity::GraphemeBytes))?;
            }

            // Attempt to allocate the space that would be required
            // for these graphemes, and if it's not available, then
            // increase capacity. Keep trying until we succeed.
            loop {
                let base = unsafe { (*self.page).memory_ptr() };
                match unsafe { (*self.page).grapheme_alloc.alloc::<u32, _>(base, cps.len()) } {
                    Ok(slice) => {
                        unsafe {
                            (*self.page).grapheme_alloc.free(base, slice);
                        }
                        break;
                    }
                    Err(_) => {
                        // Grow our capacity until we can fit the extra bytes.
                        self.increase_capacity(list, Some(IncreaseCapacity::GraphemeBytes))?;
                    }
                }
            }

            if let Err(_err) =
                unsafe { (*self.page).set_graphemes(self.page_row, self.page_cell, cps) }
            {
                // This shouldn't fail since we made sure we have space
                // above. There is no reasonable behavior we can take here
                // so we have a warn level log. This is ALMOST non-recoverable,
                // though we choose to recover by corrupting the cell
                // to a non-grapheme codepoint.
                // log.err("setGraphemes failed after capacity increase err={}", .{err});
                if cfg!(debug_assertions) {
                    // Force a crash with safe builds.
                    unreachable!();
                }

                // Unsafe builds we throw away grapheme data!
                unsafe {
                    (*self.page_cell).set_content_tag(ContentTag::Codepoint);
                    (*self.page_cell).set_content_codepoint(0xFFFD);
                }
            }
        }

        // Copy hyperlink data.
        if cell.hyperlink() {
            'hyperlink: {
                let src_id = unsafe { Page::lookup_hyperlink_of(src_page, cell) }.unwrap();
                let src_link = unsafe { Page::hyperlink_get_of(src_page, src_id) };

                // If our page can't support an additional cell
                // with a hyperlink then we increase capacity.
                if unsafe { (*self.page).hyperlink_count() }
                    >= unsafe { (*self.page).hyperlink_capacity() }
                {
                    self.increase_capacity(list, Some(IncreaseCapacity::HyperlinkBytes))?;
                }

                // Ensure that the string alloc has sufficient capacity
                // to dupe the link (and the ID if it's not implicit).
                let additional_required_string_capacity = src_link.uri.len
                    + match src_link.id {
                        crate::hyperlink::PageEntryId::Explicit(v) => v.len,
                        crate::hyperlink::PageEntryId::Implicit(_) => 0,
                    };
                // Keep trying until we have enough capacity.
                loop {
                    let base = unsafe { (*self.page).memory_ptr() };
                    match unsafe {
                        (*self.page)
                            .string_alloc
                            .alloc::<u8, _>(base, additional_required_string_capacity)
                    } {
                        Ok(slice) => {
                            // We have enough capacity, free the test alloc.
                            unsafe {
                                (*self.page).string_alloc.free(base, slice);
                            }
                            break;
                        }
                        Err(_) => {
                            // Grow our capacity until we can fit the extra bytes.
                            self.increase_capacity(list, Some(IncreaseCapacity::StringBytes))?;
                        }
                    }
                }

                let dst_link = match unsafe { src_link.dupe(src_page, self.page) } {
                    Ok(v) => v,
                    Err(_err) => {
                        // This shouldn't fail since we did a capacity
                        // check above.
                        // log.err("link dupe failed with capacity check err={}", .{err});
                        if cfg!(debug_assertions) {
                            // Force a crash with safe builds.
                            unreachable!();
                        }

                        break 'hyperlink;
                    }
                };

                let dst_id = {
                    let base = unsafe { (*self.page).memory_ptr() };
                    let self_ptr: *mut Page = self.page;
                    unsafe { (*self.page).hyperlink_set.context.set_page(self_ptr) };
                    let added = match unsafe {
                        (*self.page)
                            .hyperlink_set
                            .add_with_id(base, dst_link, src_id)
                    } {
                        Ok(v) => v,
                        Err(err) => {
                            // Always free our original link in case the increaseCap
                            // call fails so we aren't leaking memory.
                            unsafe { dst_link.free(self.page) };

                            // If the add failed then either the set needs to grow
                            // or it needs to be rehashed. Either one of those can
                            // be accomplished by increasing capacity, either with
                            // no actual change or with an increased hyperlink cap.
                            self.increase_capacity(
                                list,
                                match err {
                                    crate::ref_counted_set::AddError::OutOfMemory => {
                                        Some(IncreaseCapacity::HyperlinkBytes)
                                    }
                                    crate::ref_counted_set::AddError::NeedsRehash => None,
                                },
                            )?;

                            // We need to recreate the link into the new page.
                            let dst_link2 = match unsafe { src_link.dupe(src_page, self.page) } {
                                Ok(v) => v,
                                Err(_err2) => {
                                    // This shouldn't fail since we did a capacity
                                    // check above.
                                    // log.err("link dupe failed with capacity check err={}", .{err2});
                                    if cfg!(debug_assertions) {
                                        // Force a crash with safe builds.
                                        unreachable!();
                                    }

                                    break 'hyperlink;
                                }
                            };

                            // We assume this one will succeed. We dupe the link
                            // again, and don't have to worry about the other one
                            // because increasing the capacity naturally clears up
                            // any managed memory not associated with a cell yet.
                            let base = unsafe { (*self.page).memory_ptr() };
                            let self_ptr: *mut Page = self.page;
                            unsafe { (*self.page).hyperlink_set.context.set_page(self_ptr) };
                            match unsafe {
                                (*self.page)
                                    .hyperlink_set
                                    .add_with_id(base, dst_link2, src_id)
                            } {
                                Ok(v) => v,
                                Err(_err2) => {
                                    // This shouldn't happen since we increased capacity
                                    // above so we handle it like the other similar
                                    // cases and log it, crash in safe builds, and
                                    // remove the hyperlink in unsafe builds.
                                    // log.err(
                                    //     "addWithIdContext failed after capacity increase err={}",
                                    //     .{err2},
                                    // );
                                    if cfg!(debug_assertions) {
                                        // Force a crash with safe builds.
                                        unreachable!();
                                    }

                                    unsafe { dst_link2.free(self.page) };
                                    break 'hyperlink;
                                }
                            }
                        }
                    };
                    added.unwrap_or(src_id)
                };

                // We expect this to succeed due to the hyperlinkCapacity
                // check we did before. If it doesn't succeed let's
                // log it, crash (in safe builds), and clear our state.
                if let Err(_err) =
                    unsafe { (*self.page).set_hyperlink(self.page_row, self.page_cell, dst_id) }
                {
                    // log.err(
                    //     "setHyperlink failed after capacity increase err={}",
                    //     .{err},
                    // );
                    if cfg!(debug_assertions) {
                        // Force a crash with safe builds.
                        unreachable!();
                    }

                    // Unsafe builds we throw away hyperlink data!
                    let base = unsafe { (*self.page).memory_ptr() };
                    let self_ptr: *mut Page = self.page;
                    unsafe { (*self.page).hyperlink_set.context.set_page(self_ptr) };
                    unsafe { (*self.page).hyperlink_set.release(base, dst_id) };
                    unsafe {
                        (*self.page_cell).set_hyperlink(false);
                    }
                    break 'hyperlink;
                }
            }
        }

        // Copy style data.
        if cell.has_styling() {
            'style: {
                let style = unsafe { Page::style_get_of(src_page, cell.style_id()) };

                let base = unsafe { (*self.page).memory_ptr() };
                let id = match unsafe {
                    (*self.page)
                        .styles
                        .add_with_id(base, style, cell.style_id())
                } {
                    Ok(v) => v,
                    Err(err) => {
                        // If the add failed then either the set needs to grow
                        // or it needs to be rehashed. Either one of those can
                        // be accomplished by increasing capacity, either with
                        // no actual change or with an increased style cap.
                        self.increase_capacity(
                            list,
                            match err {
                                crate::ref_counted_set::AddError::OutOfMemory => {
                                    Some(IncreaseCapacity::Styles)
                                }
                                crate::ref_counted_set::AddError::NeedsRehash => None,
                            },
                        )?;

                        // We assume this one will succeed.
                        let base = unsafe { (*self.page).memory_ptr() };
                        match unsafe {
                            (*self.page)
                                .styles
                                .add_with_id(base, style, cell.style_id())
                        } {
                            Ok(v) => v,
                            Err(_err2) => {
                                // Should not fail since we just modified capacity
                                // above. Log it, crash in safe builds, clear style
                                // in unsafe builds.
                                // log.err(
                                //     "addWithId failed after capacity increase err={}",
                                //     .{err2},
                                // );
                                if cfg!(debug_assertions) {
                                    // Force a crash with safe builds.
                                    unreachable!();
                                }

                                unsafe {
                                    (*self.page_cell).set_style_id(stylepkg::DEFAULT_ID);
                                }
                                break 'style;
                            }
                        }
                    }
                }
                .unwrap_or(cell.style_id());

                unsafe {
                    (*self.page_row).set_styled(true);
                    (*self.page_cell).set_style_id(id);
                }
            }
        }

        self.cursor_forward();
        Ok(WriteCellResult::Success)
    }

    /// Create a new page in the provided list with the provided
    /// capacity then clone the row currently being worked on to
    /// it and delete it from the old page. Places cursor in the
    /// same position it was in in the old row in the new one.
    ///
    /// Asserts that the cursor is on the final row of the page.
    ///
    /// Expects that the provided capacity is sufficient to copy
    /// the row.
    ///
    /// If this is the only row in the page, the page is removed
    /// from the list after cloning the row.
    fn move_last_row_to_new_page(
        &mut self,
        list: &mut PageList,
        cap: Capacity,
    ) -> Result<(), super::OutOfMemory> {
        assert!(self.y == unsafe { (*self.page).size.rows } - 1);
        assert!(!self.pending_wrap);

        let old_node = self.node;
        let old_page = self.page;
        let old_row = self.page_row;
        let old_x = self.x;

        // Our total row count never changes, because we're removing one
        // row from the last page and moving it into a new page.
        let old_total_rows = self.total_rows;
        // defer self.total_rows = old_total_rows;
        //
        // PORT NOTE: the reference `defer` restores `total_rows` on every exit
        // path, including the `try cursorNewPage` failure below. We restore it
        // before propagating that error so the `defer` semantics are preserved.

        if let Err(e) = self.cursor_new_page(list, cap) {
            self.total_rows = old_total_rows;
            return Err(e);
        }
        assert!(!core::ptr::eq(self.node, old_node));
        assert!(self.y == 0);

        // We have no cleanup for our old state from here on out. No failures!
        // errdefer comptime unreachable;

        // Restore the x position of the cursor.
        self.cursor_absolute(old_x, 0);

        // Copy our old data. This should NOT fail because we have the
        // capacity of the old page which already fits the data we requested.
        if let Err(_err) = unsafe { (*self.page).clone_row_from(old_page, self.page_row, old_row) }
        {
            // log.err(
            //     "error cloning single row for moveLastRowToNewPage err={}",
            //     .{err},
            // );
            panic!("unexpected copy row failure");
        }

        // Move any tracked pins from that last row into this new node.
        {
            for p in list.tracked_pins.keys() {
                let p: *mut Pin = *p;
                if !core::ptr::eq(unsafe { &(*(*p).node).data }, old_page)
                    || unsafe { (*p).y } != unsafe { (*old_page).size.rows } - 1
                {
                    continue;
                }

                unsafe {
                    (*p).node = self.node;
                    (*p).y = self.y;
                    // p.x remains the same since we're copying the row as-is
                }
            }
        }

        // Clear the row from the old page and truncate it.
        unsafe {
            (*old_page).clear_cells(old_row, 0, (*old_page).size.cols as usize);
            (*old_page).size.rows -= 1;
        }

        // If that was the last row in that page
        // then we should remove it from the list.
        if unsafe { (*old_page).size.rows } == 0 {
            unsafe {
                list.pages.remove(old_node);
            }
            list.destroy_node(old_node);
        }

        // defer self.total_rows = old_total_rows;
        self.total_rows = old_total_rows;
        Ok(())
    }

    /// Increase the capacity of the current page.
    fn increase_capacity(
        &mut self,
        list: &mut PageList,
        adjustment: Option<IncreaseCapacity>,
    ) -> Result<(), IncreaseCapacityError> {
        let old_x = self.x;
        let old_y = self.y;
        let old_total_rows = self.total_rows;

        let node = {
            // Pause integrity checks because the total row count won't
            // be correct during a reflow.
            list.pause_integrity_checks(true);
            // defer list.pauseIntegrityChecks(false);
            let result = list.increase_capacity(self.node, adjustment);
            list.pause_integrity_checks(false);
            result?
        };
        // We must not fail after this, we've modified our self.node
        // and we need to fix it up.
        // errdefer comptime unreachable;

        *self = ReflowCursor::init(node);
        self.cursor_absolute(old_x, old_y);
        self.total_rows = old_total_rows;
        Ok(())
    }

    /// True if this cursor is at the bottom of the page by capacity,
    /// i.e. we can't scroll anymore.
    fn bottom(&self) -> bool {
        self.y == unsafe { (*self.page).capacity.rows } - 1
    }

    fn cursor_forward(&mut self) {
        if self.x == unsafe { (*self.page).size.cols } - 1 {
            self.pending_wrap = true;
        } else {
            self.page_cell = unsafe { self.page_cell.add(1) };
            self.x += 1;
        }
    }

    /// Create a new row and move the cursor down.
    ///
    /// Asserts that the cursor is on the bottom row of the
    /// page and that there is capacity to add a new one.
    fn cursor_scroll(&mut self) {
        // Scrolling requires that we're on the bottom of our page.
        // We also assert that we have capacity because reflow always
        // works within the capacity of the page.
        assert!(self.y == unsafe { (*self.page).size.rows } - 1);
        assert!(unsafe { (*self.page).size.rows } < unsafe { (*self.page).capacity.rows });

        // Increase our page size
        unsafe {
            (*self.page).size.rows += 1;
        }

        // With the increased page size, safely move down a row.
        let row: *mut Row = unsafe { self.page_row.add(1) };
        self.page_row = row;
        self.page_cell = unsafe { (*row).cells().ptr((*self.page).memory_ptr()).add(0) };
        self.pending_wrap = false;
        self.x = 0;
        self.y += 1;
    }

    /// Create a new page in the provided list with the provided
    /// capacity and one row and move the cursor in to it at 0,0
    fn cursor_new_page(
        &mut self,
        list: &mut PageList,
        cap: Capacity,
    ) -> Result<(), super::OutOfMemory> {
        // Remember our new row count so we can restore it
        // after reinitializing our cursor on the new page.
        let new_rows = self.new_rows;

        let node = list.create_page(super::CreatePage::new(cap))?;
        // errdefer comptime unreachable;
        unsafe {
            (*node).data.size.rows = 1;
            list.pages.insert_after(self.node, node);
        }

        *self = ReflowCursor::init(node);
        self.new_rows = new_rows;
        Ok(())
    }

    /// Performs `cursorScroll` or `cursorNewPage` as necessary
    /// depending on if the cursor is currently at the bottom.
    fn cursor_scroll_or_new_page(
        &mut self,
        list: &mut PageList,
        cap: Capacity,
    ) -> Result<(), super::OutOfMemory> {
        // The functions below may overwrite self so we need to cache
        // our total rows. We add one because no matter what when this
        // returns we'll have one more row added.
        let new_total_rows: usize = self.total_rows + 1;
        // defer self.total_rows = new_total_rows;

        let result = if self.bottom() {
            self.cursor_new_page(list, cap)
        } else {
            self.cursor_scroll();
            Ok(())
        };

        // defer self.total_rows = new_total_rows;
        self.total_rows = new_total_rows;
        result
    }

    fn cursor_absolute(&mut self, x: size::CellCountInt, y: size::CellCountInt) {
        assert!(x < unsafe { (*self.page).size.cols });
        assert!(y < unsafe { (*self.page).size.rows });

        let row: *mut Row = match y.cmp(&self.y) {
            core::cmp::Ordering::Equal => self.page_row,
            core::cmp::Ordering::Less => unsafe { self.page_row.sub((self.y - y) as usize) },
            core::cmp::Ordering::Greater => unsafe { self.page_row.add((y - self.y) as usize) },
        };
        self.page_row = row;
        self.page_cell = unsafe {
            (*row)
                .cells()
                .ptr((*self.page).memory_ptr())
                .add(x as usize)
        };
        self.pending_wrap = false;
        self.x = x;
        self.y = y;
    }

    #[allow(dead_code)]
    fn count_trailing_empty_cells(&self) -> usize {
        // If the row is wrapped, all empty cells are meaningful.
        if unsafe { (*self.page_row).wrap() } {
            return 0;
        }

        let cells: *const Cell = self.page_cell;
        let len: usize = unsafe { (*self.page).size.cols } as usize - self.x as usize;
        for i in 0..len {
            let rev_i = len - i - 1;
            if !unsafe { (*cells.add(rev_i)).is_empty() } {
                return i;
            }
        }

        // If the row has a semantic prompt then the blank row is meaningful
        // so we always return all but one so that the row is drawn.
        if unsafe { (*self.page_row).semantic_prompt() } != pagepkg::SemanticPrompt::None {
            return len - 1;
        }

        len
    }

    fn copy_row_metadata(&mut self, other: *const Row) {
        unsafe {
            (*self.page_row).set_semantic_prompt((*other).semantic_prompt());
        }
    }
}
