//! PORTS: ghostty src/terminal/Terminal.zig — the `index` / `reverseIndex` / `setCursorPos` / margins / `scrollDown` / `scrollUp` / `scrollViewport` / compression cluster.

use super::*;

impl Terminal {
    /// Move the cursor to the next line in the scrolling region, possibly scrolling.
    ///
    /// If the cursor is outside of the scrolling region: move the cursor one line
    /// down if it is not on the bottom-most line of the screen.
    ///
    /// If the cursor is inside the scrolling region:
    ///   If the cursor is on the bottom-most line of the scrolling region:
    ///     invoke scroll up with amount=1
    ///   If the cursor is not on the bottom-most line of the scrolling region:
    ///     move the cursor one line down
    ///
    /// This unsets the pending wrap state without wrapping.
    pub fn index(&mut self) -> Result<(), OutOfMemory> {
        // const screen: *Screen = self.screens.active;
        let screen: *mut Screen = self.screens.active;

        // Unset pending wrap state
        // screen.cursor.pending_wrap = false;
        unsafe { (*screen).cursor.pending_wrap = false };

        // We handle our cursor semantic prompt state AFTER doing the
        // scrolling, because we may need to apply to new rows.
        // defer if (screen.cursor.semantic_content != .output) { ... } else { ... };
        // PORT NOTE: Rust has no `defer`; this RAII guard reproduces the
        // semantic-content handling that the reference defers to AFTER the
        // scroll. Its `Drop` reads `screen.cursor` at drop time (post-scroll),
        // matching the reference's deferred evaluation, and runs on every exit
        // path (including the early `return`s and the `?`).
        let _semantic = SemanticIndexGuard { screen };

        // Outside of the scroll region we move the cursor one line down.
        // if (screen.cursor.y < self.scrolling_region.top or
        //     screen.cursor.y > self.scrolling_region.bottom)
        if unsafe { (*screen).cursor.y } < self.scrolling_region.top
            || unsafe { (*screen).cursor.y } > self.scrolling_region.bottom
        {
            // We only move down if we're not already at the bottom of
            // the screen.
            // if (screen.cursor.y < self.rows - 1) {
            if unsafe { (*screen).cursor.y } < self.rows - 1 {
                // screen.cursorDown(1);
                unsafe { (*screen).cursor_down(1) };
            }

            return Ok(());
        }

        // If the cursor is inside the scrolling region and on the bottom-most
        // line, then we scroll up. If our scrolling region is the full screen
        // we create scrollback.
        // if (screen.cursor.y == self.scrolling_region.bottom and
        //     screen.cursor.x >= self.scrolling_region.left and
        //     screen.cursor.x <= self.scrolling_region.right)
        if unsafe { (*screen).cursor.y } == self.scrolling_region.bottom
            && unsafe { (*screen).cursor.x } >= self.scrolling_region.left
            && unsafe { (*screen).cursor.x } <= self.scrolling_region.right
        {
            // if (comptime build_options.kitty_graphics) {
            //     // Scrolling dirties the images because it updates their placements pins.
            //     screen.kitty_images.dirty = true;
            // }
            if crate::options::KITTY_GRAPHICS {
                // Scrolling dirties the images because it updates their placements pins.
                // SAFETY: `screen` is the active screen's stable heap pointer,
                // owned by `self.screens`; we hold `&mut self`.
                unsafe { (*screen).kitty_images.dirty = true };
            }

            // If our scrolling region is at the top, we create scrollback,
            // but only if our screen retains scrollback. If our screen
            // doesn't retain scrollback (e.g. the alternate screen) then
            // creating scrollback is pure overhead: the rows are never
            // visible and are simply pruned later. In that case we use the
            // in-place region scroll below, unless the region is a single
            // row (a one row screen) which cursorScrollRegionUp can't
            // handle (and cursorDownScroll special-cases).
            // if (self.scrolling_region.top == 0 and
            //     self.scrolling_region.left == 0 and
            //     self.scrolling_region.right == self.cols - 1 and
            //     (!screen.no_scrollback or
            //         self.scrolling_region.bottom == 0))
            if self.scrolling_region.top == 0
                && self.scrolling_region.left == 0
                && self.scrolling_region.right == self.cols - 1
                && (!unsafe { (*screen).no_scrollback } || self.scrolling_region.bottom == 0)
            {
                // If a bottom margin is set, kitty image placements may
                // need adjusting around the scroll. The rare placements-
                // present case is handled out of line so this hot path
                // only pays a count check (a load from a cache line we
                // already write, above).
                // if (comptime build_options.kitty_graphics) {
                if crate::options::KITTY_GRAPHICS {
                    // if (screen.kitty_images.placements.count() != 0) {
                    //     @branchHint(.unlikely);
                    // SAFETY: `screen` is the active screen's stable heap
                    // pointer, owned by `self.screens`; we hold `&mut self`.
                    if !unsafe { (*screen).kitty_images.placements.is_empty() } {
                        self.index_scroll_with_images(
                            crate::kitty::graphics_storage::ScrollOp::WindowShift,
                        )?;
                        return Ok(());
                    }
                }

                // try screen.cursorScrollAbove();
                self.active_mut().cursor_scroll_above()?;
                return Ok(());
            }

            // Slow path for left and right scrolling region margins.
            // scrollUp handles the kitty image adjustment itself.
            // if (self.scrolling_region.left != 0 or
            //     self.scrolling_region.right != self.cols - 1)
            //
            // PORT NOTE (oracle 77190bd02, `P3-W5 T3`): the third disjunct
            // `!screen.blankCell().isZero()` was DELETED by the oracle along
            // with its `PERF(mitchellh)` comment. It existed because
            // `eraseRowBounded` (the old fast path) did not preserve an SGR
            // background in the erased row; `cursorScrollRegionUp` does, so
            // the background case now takes the fast path too.
            if self.scrolling_region.left != 0 || self.scrolling_region.right != self.cols - 1 {
                // try self.scrollUp(1);
                self.scroll_up(1)?;
                return Ok(());
            }

            // Kitty image placements may need adjusting around the scroll;
            // handled out of line like the scrollback path above.
            // if (comptime build_options.kitty_graphics) {
            if crate::options::KITTY_GRAPHICS {
                // if (screen.kitty_images.placements.count() != 0) {
                //     @branchHint(.unlikely);
                // SAFETY: `screen` is the active screen's stable heap pointer,
                // owned by `self.screens`; we hold `&mut self`.
                if !unsafe { (*screen).kitty_images.placements.is_empty() } {
                    self.index_scroll_with_images(
                        crate::kitty::graphics_storage::ScrollOp::InPlace,
                    )?;
                    return Ok(());
                }
            }

            // Otherwise use a fast path function to efficiently scroll
            // the contents of the scrolling region.
            // try screen.cursorScrollRegionUp(
            //     self.scrolling_region.bottom - self.scrolling_region.top,
            // );
            let sr_top = self.scrolling_region.top;
            let sr_bottom = self.scrolling_region.bottom;
            self.active_mut()
                .cursor_scroll_region_up((sr_bottom - sr_top) as usize)?;

            return Ok(());
        }

        // Increase cursor by 1, maximum to bottom of scroll region
        // if (screen.cursor.y < self.scrolling_region.bottom) {
        if unsafe { (*screen).cursor.y } < self.scrolling_region.bottom {
            // screen.cursorDown(1);
            unsafe { (*screen).cursor_down(1) };
        }

        Ok(())
    }

    /// The operation when we have Kitty image placements during index.
    /// Split out of index() so its hot paths don't carry the adjustment
    /// state in their stack frame, which measurably slows the scroll
    /// hot path.
    //
    // PORT NOTE (R8): the reference `op` is a `comptime` parameter — a pure
    // branch-specialization for the scroll hot path, with no compile-time
    // grammar or emitted-bytes dependence (no runtime bytes can alias any
    // comptime shape here; the value only selects between two calls below).
    // Rust const generics cannot carry an enum on stable, so it is a plain
    // runtime parameter; the hot paths above still pay only the count check
    // before entering this out-of-line fn.
    fn index_scroll_with_images(
        &mut self,
        op: crate::kitty::graphics_storage::ScrollOp,
    ) -> Result<(), OutOfMemory> {
        // var kitty_scroll = self.kittyScrollMarginsBegin(-1, op);
        // defer if (kitty_scroll) |*state| state.end();
        // PORT NOTE: the guard's `Drop` performs `end()` (exactly-once by
        // ownership), so the Zig `defer` semantics — including the error
        // path through `?` — fall out of scope-drop.
        let kitty_scroll = self.kitty_scroll_margins_begin(-1, op);
        let result = match op {
            // .window_shift => try self.screens.active.cursorScrollAbove(),
            crate::kitty::graphics_storage::ScrollOp::WindowShift => {
                self.active_mut().cursor_scroll_above()
            }
            // .in_place => try self.screens.active.cursorScrollRegionUp(
            //     self.scrolling_region.bottom - self.scrolling_region.top,
            // ),
            crate::kitty::graphics_storage::ScrollOp::InPlace => {
                let sr_top = self.scrolling_region.top;
                let sr_bottom = self.scrolling_region.bottom;
                self.active_mut()
                    .cursor_scroll_region_up((sr_bottom - sr_top) as usize)
            }
        };
        if let Some(state) = kitty_scroll {
            state.end();
        }
        result
    }

    // Handle when Kitty graphics is disabled.
    // const KittyScrollMargins = if (build_options.kitty_graphics)
    //     kitty.graphics.ImageStorage.ScrollMargins
    // else
    //     struct {
    //         pub inline fn end(self: *@This()) void {
    //             _ = self;
    //         }
    //     };
    //
    // PORT NOTE: Zig needs the empty stub type because the disabled comptime
    // branch must still typecheck against a named type. Rust compiles the real
    // `ScrollMargins` type unconditionally (the kitty module is always built;
    // the `KITTY_GRAPHICS` const-gate dead-codes the calls), so no stub type
    // is required.

    /// Begin adjusting kitty image placements for a scroll of the
    /// scrolling region by delta rows (negative moves content up). If
    /// adjustment is needed this returns state whose end() must be called
    /// after the scroll's row operations complete (see
    /// ImageStorage.scrollMarginsBegin for why this is two phases). This
    /// returns null when the scrolling region is the full screen, because
    /// placements then follow their anchored rows via pin tracking which
    /// matches kitty's marginless behavior.
    ///
    /// Callers must comptime-gate on build_options.kitty_graphics and
    /// check that placements exist before calling, which keeps the cost
    /// on the hot scroll paths cheap.
    // @branchHint(.cold) → #[cold].
    #[cold]
    fn kitty_scroll_margins_begin(
        &mut self,
        delta: isize,
        op: crate::kitty::graphics_storage::ScrollOp,
    ) -> Option<crate::kitty::graphics_storage::ScrollMargins> {
        // Full-screen scrolls need no adjustment: placements follow their
        // anchored rows (possibly into the scrollback) via pin tracking.
        if self.scrolling_region.top == 0
            && self.scrolling_region.bottom == self.rows - 1
            && self.scrolling_region.left == 0
            && self.scrolling_region.right == self.cols - 1
        {
            return None;
        }

        // const screen: *Screen = self.screens.active;
        // return screen.kitty_images.scrollMarginsBegin(self.io(), self, delta, op);
        //
        // SAFETY: the storage lives INSIDE the active screen; the `&mut`
        // projection below is scoped to the `kitty_images` field of the
        // screen's separate heap allocation, and `scroll_margins_begin`
        // touches the rest of the terminal/screen only through the raw `t`
        // pointer in narrow read scopes (its hazard-#1 PORT NOTE).
        let t: *mut Terminal = self;
        let screen: *mut Screen = self.screens.active;
        Some(unsafe { &mut (*screen).kitty_images }.scroll_margins_begin(t, delta, op))
    }

    /// Move the cursor to the previous line in the scrolling region, possibly
    /// scrolling.
    ///
    /// If the cursor is outside of the scrolling region, move the cursor one
    /// line up if it is not on the top-most line of the screen.
    ///
    /// If the cursor is inside the scrolling region:
    ///
    ///   * If the cursor is on the top-most line of the scrolling region:
    ///     invoke scroll down with amount=1
    ///   * If the cursor is not on the top-most line of the scrolling region:
    ///     move the cursor one line up
    pub fn reverse_index(&mut self) {
        // if (self.screens.active.cursor.y != self.scrolling_region.top or
        //     self.screens.active.cursor.x < self.scrolling_region.left or
        //     self.screens.active.cursor.x > self.scrolling_region.right)
        if unsafe { (*self.screens.active).cursor.y } != self.scrolling_region.top
            || unsafe { (*self.screens.active).cursor.x } < self.scrolling_region.left
            || unsafe { (*self.screens.active).cursor.x } > self.scrolling_region.right
        {
            // self.cursorUp(1);
            self.cursor_up(1);
            return;
        }

        // self.scrollDown(1);
        self.scroll_down(1);
    }

    /// Set Cursor Position. Move cursor to the position indicated
    /// by row and column (1-indexed). If column is 0, it is adjusted to 1.
    /// If column is greater than the right-most column it is adjusted to
    /// the right-most column. If row is 0, it is adjusted to 1. If row is
    /// greater than the bottom-most row it is adjusted to the bottom-most
    /// row.
    pub fn set_cursor_pos(&mut self, row_req: usize, col_req: usize) {
        // If cursor origin mode is set the cursor row will be moved relative to
        // the top margin row and adjusted to be above or at bottom-most row in
        // the current scroll region.
        //
        // If origin mode is set and left and right margin mode is set the cursor
        // will be moved relative to the left margin column and adjusted to be on
        // or left of the right margin column.
        // const params: struct {
        //     x_offset: size.CellCountInt = 0,
        //     y_offset: size.CellCountInt = 0,
        //     x_max: size.CellCountInt,
        //     y_max: size.CellCountInt,
        // } = if (self.modes.get(.origin)) .{
        //     .x_offset = self.scrolling_region.left,
        //     .y_offset = self.scrolling_region.top,
        //     .x_max = self.scrolling_region.right + 1, // We need this 1-indexed
        //     .y_max = self.scrolling_region.bottom + 1, // We need this 1-indexed
        // } else .{
        //     .x_max = self.cols,
        //     .y_max = self.rows,
        // };
        struct Params {
            x_offset: size::CellCountInt,
            y_offset: size::CellCountInt,
            x_max: size::CellCountInt,
            y_max: size::CellCountInt,
        }
        let params: Params = if self.modes.get(modespkg::Mode::Origin) {
            Params {
                x_offset: self.scrolling_region.left,
                y_offset: self.scrolling_region.top,
                x_max: self.scrolling_region.right + 1, // We need this 1-indexed
                y_max: self.scrolling_region.bottom + 1, // We need this 1-indexed
            }
        } else {
            Params {
                x_offset: 0,
                y_offset: 0,
                x_max: self.cols,
                y_max: self.rows,
            }
        };

        // Unset pending wrap state
        // self.screens.active.cursor.pending_wrap = false;
        unsafe { (*self.screens.active).cursor.pending_wrap = false };

        // Calculate our new x/y
        // const row = if (row_req == 0) 1 else row_req;
        // const col = if (col_req == 0) 1 else col_req;
        // const x = @min(params.x_max, col +| params.x_offset) -| 1;
        // const y = @min(params.y_max, row +| params.y_offset) -| 1;
        let row = if row_req == 0 { 1 } else { row_req };
        let col = if col_req == 0 { 1 } else { col_req };
        // PORT NOTE: `col +| params.x_offset` is `usize +| CellCountInt`; we widen
        // the offset to usize for the add, then `@min` with `x_max` (also widened)
        // keeps the result within `CellCountInt` range before the saturating `-| 1`.
        // PORT NOTE (parity 0aaedf436): the origin-offset adds are saturating
        // (`+|`); requests near maxInt(usize) land on the region boundary
        // instead of overflowing.
        let x: size::CellCountInt = (core::cmp::min(
            params.x_max as usize,
            col.saturating_add(params.x_offset as usize),
        ) as size::CellCountInt)
            .saturating_sub(1);
        let y: size::CellCountInt = (core::cmp::min(
            params.y_max as usize,
            row.saturating_add(params.y_offset as usize),
        ) as size::CellCountInt)
            .saturating_sub(1);

        // If the y is unchanged then this is fast pointer math
        // if (y == self.screens.active.cursor.y) {
        //     if (x > self.screens.active.cursor.x) {
        //         self.screens.active.cursorRight(x - self.screens.active.cursor.x);
        //     } else {
        //         self.screens.active.cursorLeft(self.screens.active.cursor.x - x);
        //     }
        //     return;
        // }
        if y == unsafe { (*self.screens.active).cursor.y } {
            let cursor_x = unsafe { (*self.screens.active).cursor.x };
            if x > cursor_x {
                self.active_mut().cursor_right(x - cursor_x);
            } else {
                self.active_mut().cursor_left(cursor_x - x);
            }

            return;
        }

        // If everything changed we do an absolute change which is slightly slower
        // self.screens.active.cursorAbsolute(x, y);
        self.active_mut().cursor_absolute(x, y);
        // log.info("set cursor position: col={} row={}", .{ self.screens.active.cursor.x, self.screens.active.cursor.y });
    }

    /// Set Top and Bottom Margins If bottom is not specified, 0 or bigger than
    /// the number of the bottom-most row, it is adjusted to the number of the
    /// bottom most row.
    ///
    /// If top < bottom set the top and bottom row of the scroll region according
    /// to top and bottom and move the cursor to the top-left cell of the display
    /// (when in cursor origin mode is set to the top-left cell of the scroll region).
    ///
    /// Otherwise: Set the top and bottom row of the scroll region to the top-most
    /// and bottom-most line of the screen.
    ///
    /// Top and bottom are 1-indexed.
    pub fn set_top_and_bottom_margin(&mut self, top_req: usize, bottom_req: usize) {
        // const top = @max(1, top_req);
        let top = core::cmp::max(1, top_req);
        // const bottom = @min(self.rows, if (bottom_req == 0) self.rows else bottom_req);
        let bottom = core::cmp::min(
            self.rows as usize,
            if bottom_req == 0 {
                self.rows as usize
            } else {
                bottom_req
            },
        );
        // if (top >= bottom) return;
        if top >= bottom {
            return;
        }

        // self.scrolling_region.top = @intCast(top - 1);
        self.scrolling_region.top = (top - 1) as size::CellCountInt;
        // self.scrolling_region.bottom = @intCast(bottom - 1);
        self.scrolling_region.bottom = (bottom - 1) as size::CellCountInt;
        // self.setCursorPos(1, 1);
        self.set_cursor_pos(1, 1);
    }

    /// DECSLRM
    pub fn set_left_and_right_margin(&mut self, left_req: usize, right_req: usize) {
        // We must have this mode enabled to do anything
        // if (!self.modes.get(.enable_left_and_right_margin)) return;
        if !self.modes.get(modespkg::Mode::EnableLeftAndRightMargin) {
            return;
        }

        // const left = @max(1, left_req);
        let left = core::cmp::max(1, left_req);
        // const right = @min(self.cols, if (right_req == 0) self.cols else right_req);
        let right = core::cmp::min(
            self.cols as usize,
            if right_req == 0 {
                self.cols as usize
            } else {
                right_req
            },
        );
        // if (left >= right) return;
        if left >= right {
            return;
        }

        // self.scrolling_region.left = @intCast(left - 1);
        self.scrolling_region.left = (left - 1) as size::CellCountInt;
        // self.scrolling_region.right = @intCast(right - 1);
        self.scrolling_region.right = (right - 1) as size::CellCountInt;
        // self.setCursorPos(1, 1);
        self.set_cursor_pos(1, 1);
    }

    /// Scroll the text down by one row.
    pub fn scroll_down(&mut self, count: usize) {
        // Preserve our x/y to restore.
        // const old_x = self.screens.active.cursor.x;
        // const old_y = self.screens.active.cursor.y;
        // const old_wrap = self.screens.active.cursor.pending_wrap;
        // defer {
        //     self.screens.active.cursorAbsolute(old_x, old_y);
        //     self.screens.active.cursor.pending_wrap = old_wrap;
        // }
        // PORT NOTE: Rust has no `defer`; this RAII guard restores the cursor
        // x/y and pending_wrap when it drops at end of scope, mirroring the
        // reference `defer`. It runs on every exit path.
        let _restore = CursorRestoreGuard {
            screen: self.screens.active,
            old_x: unsafe { (*self.screens.active).cursor.x },
            old_y: unsafe { (*self.screens.active).cursor.y },
            old_wrap: unsafe { (*self.screens.active).cursor.pending_wrap },
        };

        // If margins are set and kitty image placements exist, they need
        // adjusting around the scroll. Note this wraps scrollDown and NOT
        // insertLines: kitty scrolls images for SD/RI but leaves them
        // alone for IL/DL.
        // var kitty_scroll: ?KittyScrollMargins = null;
        // defer if (kitty_scroll) |*state| state.end();
        // PORT NOTE: the guard's `Drop` performs `end()`; locals drop in
        // reverse declaration order, so the guard ends BEFORE `_restore`
        // fires — exactly the reference's LIFO `defer` order.
        let mut kitty_scroll: Option<crate::kitty::graphics_storage::ScrollMargins> = None;
        // if (comptime build_options.kitty_graphics) {
        if crate::options::KITTY_GRAPHICS {
            // if (self.screens.active.kitty_images.placements.count() != 0) {
            //     @branchHint(.unlikely);
            // SAFETY: `self.screens.active` is a stable heap pointer owned by
            // `self.screens`; we hold `&mut self`.
            if !unsafe { (*self.screens.active).kitty_images.placements.is_empty() } {
                // const region_height: usize =
                //     @as(usize, self.scrolling_region.bottom - self.scrolling_region.top) + 1;
                let region_height: usize =
                    (self.scrolling_region.bottom - self.scrolling_region.top) as usize + 1;
                // kitty_scroll = self.kittyScrollMarginsBegin(
                //     @intCast(@min(count, region_height)),
                //     .in_place,
                // );
                kitty_scroll = self.kitty_scroll_margins_begin(
                    isize::try_from(core::cmp::min(count, region_height)).unwrap(),
                    crate::kitty::graphics_storage::ScrollOp::InPlace,
                );
            }
        }

        // Move to the top of the scroll region
        // self.screens.active.cursorAbsolute(self.scrolling_region.left, self.scrolling_region.top);
        let sr_left = self.scrolling_region.left;
        let sr_top = self.scrolling_region.top;
        self.active_mut().cursor_absolute(sr_left, sr_top);
        // self.insertLines(count);
        self.insert_lines(count);

        // defer if (kitty_scroll) |*state| state.end();
        if let Some(state) = kitty_scroll.take() {
            state.end();
        }
    }

    /// Removes amount lines from the top of the scroll region. The remaining lines
    /// to the bottom margin are shifted up and space from the bottom margin up
    /// is filled with empty lines.
    ///
    /// The new lines are created according to the current SGR state.
    ///
    /// Does not change the (absolute) cursor position.
    pub fn scroll_up(&mut self, count: usize) -> Result<(), OutOfMemory> {
        // Preserve our x/y to restore.
        // const old_x = self.screens.active.cursor.x;
        // const old_y = self.screens.active.cursor.y;
        // const old_wrap = self.screens.active.cursor.pending_wrap;
        // defer {
        //     self.screens.active.cursorAbsolute(old_x, old_y);
        //     self.screens.active.cursor.pending_wrap = old_wrap;
        // }
        // PORT NOTE: see scroll_down — RAII guard mirrors the reference `defer`,
        // restoring the cursor on every return (including the early `?`).
        let _restore = CursorRestoreGuard {
            screen: self.screens.active,
            old_x: unsafe { (*self.screens.active).cursor.x },
            old_y: unsafe { (*self.screens.active).cursor.y },
            old_wrap: unsafe { (*self.screens.active).cursor.pending_wrap },
        };

        // If margins are set and kitty image placements exist, they need
        // adjusting around the scroll. Note this wraps scrollUp and NOT
        // deleteLines: kitty scrolls images for SU/IND but leaves them
        // alone for IL/DL.
        // var kitty_scroll: ?KittyScrollMargins = null;
        // defer if (kitty_scroll) |*state| state.end();
        // PORT NOTE: the guard's `Drop` performs `end()` on every exit path
        // (including the early `?`), mirroring the reference `defer`; locals
        // drop in reverse declaration order, so it ends BEFORE `_restore`
        // fires — the reference's LIFO `defer` order.
        let mut _kitty_scroll: Option<crate::kitty::graphics_storage::ScrollMargins> = None;
        // if (comptime build_options.kitty_graphics) {
        if crate::options::KITTY_GRAPHICS {
            // if (self.screens.active.kitty_images.placements.count() != 0) {
            //     @branchHint(.unlikely);
            // SAFETY: `self.screens.active` is a stable heap pointer owned by
            // `self.screens`; we hold `&mut self`.
            if !unsafe { (*self.screens.active).kitty_images.placements.is_empty() } {
                // The op must mirror the branch below: the scrollback path
                // shifts the active window while the deleteLines path
                // moves rows in place.
                // const region_height: usize =
                //     @as(usize, self.scrolling_region.bottom - self.scrolling_region.top) + 1;
                let region_height: usize =
                    (self.scrolling_region.bottom - self.scrolling_region.top) as usize + 1;
                // kitty_scroll = self.kittyScrollMarginsBegin(
                //     -@as(isize, @intCast(@min(count, region_height))),
                //     if (...) .window_shift else .in_place,
                // );
                _kitty_scroll = self.kitty_scroll_margins_begin(
                    -isize::try_from(core::cmp::min(count, region_height)).unwrap(),
                    if self.scrolling_region.top == 0
                        && self.scrolling_region.left == 0
                        && self.scrolling_region.right == self.cols - 1
                        && (!unsafe { (*self.screens.active).no_scrollback }
                            || self.scrolling_region.bottom == self.rows - 1)
                    {
                        crate::kitty::graphics_storage::ScrollOp::WindowShift
                    } else {
                        crate::kitty::graphics_storage::ScrollOp::InPlace
                    },
                );
            }
        }

        // If our scroll region is at the top and we have no left/right
        // margins then we move the scrolled out text into the scrollback.
        //
        // If our screen doesn't retain scrollback (e.g. the alternate
        // screen) then creating scrollback is pure overhead, so we use the
        // deleteLines path below instead, unless the region is the full
        // screen where cursorScrollAbove has a specialized fast path
        // (cursorDownScroll) for scrolling without scrollback.
        // if (self.scrolling_region.top == 0 and
        //     self.scrolling_region.left == 0 and
        //     self.scrolling_region.right == self.cols - 1 and
        //     (!self.screens.active.no_scrollback or
        //         self.scrolling_region.bottom == self.rows - 1))
        if self.scrolling_region.top == 0
            && self.scrolling_region.left == 0
            && self.scrolling_region.right == self.cols - 1
            && (!unsafe { (*self.screens.active).no_scrollback }
                || self.scrolling_region.bottom == self.rows - 1)
        {
            // Scrolling dirties the images because it updates their placements pins.
            // if (comptime build_options.kitty_graphics) {
            //     self.screens.active.kitty_images.dirty = true;
            // }
            if crate::options::KITTY_GRAPHICS {
                // SAFETY: `self.screens.active` is a stable heap pointer owned
                // by `self.screens`; we hold `&mut self`.
                unsafe { (*self.screens.active).kitty_images.dirty = true };
            }

            // Clamp count to the scroll region height.
            // const region_height = self.scrolling_region.bottom + 1;
            let region_height = self.scrolling_region.bottom as usize + 1;
            // const adjusted_count = @min(count, region_height);
            let adjusted_count = core::cmp::min(count, region_height);

            // TODO: Create an optimized version that can scroll N times
            // This isn't critical because in most cases, scrollUp is used
            // with count=1, but it's still a big optimization opportunity.

            // Move our cursor to the bottom of the scroll region so we can
            // use the cursorScrollAbove function to create scrollback
            // self.screens.active.cursorAbsolute(0, self.scrolling_region.bottom);
            let sr_bottom = self.scrolling_region.bottom;
            self.active_mut().cursor_absolute(0, sr_bottom);
            // for (0..adjusted_count) |_| try self.screens.active.cursorScrollAbove();
            for _ in 0..adjusted_count {
                self.active_mut().cursor_scroll_above()?;
            }
            return Ok(());
        }

        // Move to the top of the scroll region
        // self.screens.active.cursorAbsolute(self.scrolling_region.left, self.scrolling_region.top);
        let sr_left = self.scrolling_region.left;
        let sr_top = self.scrolling_region.top;
        self.active_mut().cursor_absolute(sr_left, sr_top);
        // self.deleteLines(count);
        self.delete_lines(count);

        Ok(())
    }

    /// Scroll the viewport of the terminal grid.
    pub fn scroll_viewport(&mut self, behavior: ScrollViewport) {
        // self.screens.active.scroll(switch (behavior) {
        //     .top => .{ .top = {} },
        //     .bottom => .{ .active = {} },
        //     .delta => |delta| .{ .delta_row = delta },
        //     .row => |row| .{ .row = row },
        // });
        self.active_mut().scroll(match behavior {
            ScrollViewport::Top => crate::screen::Scroll::Top,
            ScrollViewport::Bottom => crate::screen::Scroll::Active,
            ScrollViewport::Delta(delta) => crate::screen::Scroll::DeltaRow(delta),
            ScrollViewport::Row(row) => crate::screen::Scroll::Row(row),
        });
    }

    /// Return the current compression activity value.
    ///
    /// Callers should schedule a `compress` call whenever this value changes.
    /// The direction of the change has no meaning; this is an opaque change
    /// token rather than a monotonic sequence exposed by Terminal.
    ///
    /// It is up to the terminal what it decides to compress, but currently
    /// we compress cold (non-viewed, non-editable) scrollback history on
    /// the primary screen.
    ///
    /// Note that compression requires specific system features, namely
    /// the ability to retain virtual memory allocations while discarding their
    /// physical memory backings. Callers must still use `compress` to determine
    /// whether compression is supported on the current target.
    pub fn compression_activity(&self) -> u64 {
        // const state = &self.screens.get(.primary).?.pages.page_compression;
        let primary: *mut Screen = self.screens.get(crate::screen_set::Key::Primary).unwrap();
        let state = unsafe { &(*primary).pages.page_compression };
        // For now we don't use the extra 16 bits.
        // return @as(u64, state.activity_serial);
        //
        // PORT NOTE: the reference field is a `u48` widened here to `u64`.
        // Rustty stores it as a `u64` already masked to 48 bits by
        // `mark_activity`, so the widening is a plain read.
        state.activity_serial
    }

    /// Compress cold memory to save resident memory space.
    ///
    /// Full compression does a full pass compressing everything it can before
    /// returning. This is not recommended for interactive terminals because
    /// compression is relatively slow and with large scrollbacks this can cause
    /// stalls.
    ///
    /// Incremental compression bounds itself on how much data it can look
    /// up to compress and how much compression work it does before returning.
    /// It is stateful (we maintain the state) and the return value tells callers
    /// whether they should continue calling it in the future.
    ///
    /// Callers should schedule compression when it doesn't impact user
    /// experience, for example during idle times.
    ///
    /// PORT NOTE (WS19 T18) — `Pending` does NOT mean "unfinished work remains".
    /// A fresh `compress(Incremental)` issued right after a pass that returned
    /// `Complete` returns `Pending` again, because the incremental traversal
    /// RESTARTS from the beginning rather than remembering that it converged.
    /// So `Pending` means "call me again", never "there is work outstanding", and
    /// `result == Complete` is NOT a durable "compression is done" probe — it is
    /// only true of the pass that just ran. Callers that want an idle-detection
    /// signal must debounce on activity (see the renderer's compression scheduler,
    /// `rustty-core/src/renderer_seam.rs`, which schedules another tick on
    /// `Pending` and stops on `Complete`/`Unsupported` per tick — correct, because
    /// it re-arms from activity, not from a remembered `Complete`), not on this
    /// return value.
    ///
    /// This is rustty-only clarification of ported behavior, not reference prose:
    /// the reference doc comment above says only that "the return value tells
    /// callers whether they should continue calling it in the future", which reads
    /// naturally as the *opposite* of what happens. Observed, not assumed — the
    /// behavior is pinned by
    /// `terminal_compress_incremental_restarts_after_complete` in this file's test
    /// module, and cross-referenced from the compression narrative in
    /// `.port/phase-2/MIRI_EXCEPTIONS.md`.
    pub fn compress(&mut self, mode: TerminalCompressionMode) -> TerminalCompressionResult {
        // const pages = &self.screens.get(.primary).?.pages;
        let primary: *mut Screen = self.screens.get(crate::screen_set::Key::Primary).unwrap();
        let pages = unsafe { &mut (*primary).pages };
        // const result = switch (mode) {
        //     .incremental => pages.compress(.incremental),
        //     .full => pages.compress(.full),
        // };
        let result = match mode {
            TerminalCompressionMode::Incremental => {
                pages.compress(pagelist::compress::CompressionMode::Incremental)
            }
            TerminalCompressionMode::Full => {
                pages.compress(pagelist::compress::CompressionMode::Full)
            }
        };

        // return switch (result) {
        //     .unsupported => .unsupported,
        //     .pending => .pending,
        //     .complete => .complete,
        // };
        match result {
            pagelist::compress::IncrementalCompressionResult::Unsupported => {
                TerminalCompressionResult::Unsupported
            }
            pagelist::compress::IncrementalCompressionResult::Pending => {
                TerminalCompressionResult::Pending
            }
            pagelist::compress::IncrementalCompressionResult::Complete => {
                TerminalCompressionResult::Complete
            }
        }
    }
}
