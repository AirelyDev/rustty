//! PORTS: ghostty src/terminal/Terminal.zig — the `deccolm` / `resize` cluster.

use super::*;

impl Terminal {
    /// DECCOLM changes the terminal width between 80 and 132 columns. This
    /// function call will do NOTHING unless `setDeccolmSupported` has been
    /// called with "true".
    ///
    /// This breaks the expectation around modern terminals that they resize
    /// with the window. This will fix the grid at either 80 or 132 columns.
    /// The rows will continue to be variable.
    pub fn deccolm(&mut self, alloc: Allocator, mode: DeccolmMode) -> Result<(), OutOfMemory> {
        // If DEC mode 40 isn't enabled, then this is ignored. We also make
        // sure that we don't have deccolm set because we want to fully ignore
        // set mode.
        // if (!self.modes.get(.enable_mode_3)) {
        //     self.modes.set(.@"132_column", false);
        //     return;
        // }
        if !self.modes.get(modespkg::Mode::EnableMode3) {
            self.modes.set(modespkg::Mode::Column132, false);
            return Ok(());
        }

        // Enable it
        // self.modes.set(.@"132_column", mode == .@"132_cols");
        self.modes
            .set(modespkg::Mode::Column132, mode == DeccolmMode::Cols132);

        // Resize to the requested size
        // try self.resize(alloc, .{
        //     .cols = switch (mode) {
        //         .@"132_cols" => 132,
        //         .@"80_cols" => 80,
        //     },
        //     .rows = self.rows,
        // });
        self.resize(
            alloc,
            Resize {
                cols: match mode {
                    DeccolmMode::Cols132 => 132,
                    DeccolmMode::Cols80 => 80,
                },
                rows: self.rows,
                cell_size_px: None,
            },
        )
        .map_err(|e| match e {
            ResizeError::OutOfMemory => OutOfMemory,
            // PORT NOTE: `deccolm` passes a constant non-zero column count and
            // the terminal's own row count (always >= 1), so the reference's
            // inferred error set can never carry `InvalidValue` here. Rust's
            // error type is explicit, so the impossible arm is spelled out.
            ResizeError::InvalidValue => {
                unreachable!("deccolm resize dimensions are always non-zero")
            }
        })?;

        // Erase our display and move our cursor.
        // self.eraseDisplay(.complete, false);
        // self.setCursorPos(1, 1);
        self.erase_display(csi::EraseDisplay::Complete, false);
        self.set_cursor_pos(1, 1);

        Ok(())
    }

    /// Resize the underlying terminal.
    ///
    /// This has follow-on impacts:
    ///
    ///   - If the column count changes, tabstops are reset.
    ///   - The scroll region is always reset
    ///   - Synchronized output mode is reset for every successful resize.
    ///
    /// This handles errors gracefully and recovers the terminal back to
    /// a clean usable state.
    ///
    /// The only error handling edge case is in the highly exceptional scenario
    /// where the primary screen can be resized but the alternate screen cannot.
    /// In this scenario, we attempt to clear the alt screen at the desired
    /// new size. If that fails, we unconditionally deallocate the alt screen
    /// and move to primary screen. This can break terminal programs but it
    /// requires a really particular scenario where memory exists for one
    /// but not the other and we do our best.
    pub fn resize(
        &mut self,
        alloc: Allocator,
        opts: Resize,
    ) -> core::result::Result<(), ResizeError> {
        // Screen and scrolling-region invariants require non-zero dimensions.
        // Validate before changing any terminal state.
        // if (opts.cols == 0 or opts.rows == 0) return error.InvalidValue;
        if opts.cols == 0 || opts.rows == 0 {
            return Err(ResizeError::InvalidValue);
        }

        let cols = opts.cols;
        let rows = opts.rows;

        // Pixel geometry and synchronized output are updated on every valid
        // resize attempt, including one that doesn't change the grid dimensions.
        // Save their old values so later allocation failures can roll them back.
        // errdefer {
        //     self.width_px = old_width_px;
        //     self.height_px = old_height_px;
        //     self.modes.set(.synchronized_output, old_synchronized_output);
        // }
        //
        // PORT NOTE: Rust has no `errdefer`; the block is spelled out as
        // `resize_rollback_geometry` and invoked at every fallible exit below.
        let old_width_px = self.width_px;
        let old_height_px = self.height_px;
        let old_synchronized_output = self.modes.get(modespkg::Mode::SynchronizedOutput);

        // If our pixel geometry was set, then we set it even if our rows/cols
        // didn't change.
        // PORT NOTE: `std.math.mul(...) catch maxInt(u32)` is saturate-on-overflow.
        if let Some(cell_size) = opts.cell_size_px {
            self.width_px = u32::from(cols).saturating_mul(cell_size.width);
            self.height_px = u32::from(rows).saturating_mul(cell_size.height);
        }

        // self.modes.set(.synchronized_output, false);
        self.modes.set(modespkg::Mode::SynchronizedOutput, false);

        // If our cols/rows didn't change, skip grid work but still apply pixels.
        // if (self.cols == opts.cols and self.rows == opts.rows) return;
        if self.cols == cols && self.rows == rows {
            return Ok(());
        }

        // Build replacement tabstops without touching the current table. Keep
        // ownership here until every fallible resize operation has succeeded.
        // var new_tabstops: ?Tabstops = null;
        // errdefer if (new_tabstops) |*v| v.deinit(alloc);
        let mut new_tabstops: Option<Tabstops> = None;
        if self.cols != cols {
            // try tw.check(.tabstops);
            #[cfg(test)]
            if let Err(e) = resize_tw_check(ResizeFp::Tabstops) {
                self.resize_rollback_geometry(old_width_px, old_height_px, old_synchronized_output);
                return Err(e.into());
            }
            // PORT NOTE: `Tabstops::init`'s allocation-failure type is its own
            // `OutOfMemory`; both map to the reference `Allocator.Error`.
            match Tabstops::init(alloc, cols as usize, TABSTOP_INTERVAL as usize) {
                Ok(t) => new_tabstops = Some(t),
                Err(_e) => {
                    self.resize_rollback_geometry(
                        old_width_px,
                        old_height_px,
                        old_synchronized_output,
                    );
                    return Err(ResizeError::OutOfMemory);
                }
            }
        }

        // Resize primary screen, which supports reflow. We do this first
        // because the cleanup situation is a lot better if this succeeds
        // and alt fails than the reverse.
        // try tw.check(.primary_screen);
        // const primary = self.screens.get(.primary).?;
        // try primary.resize(.{
        //     .cols = opts.cols,
        //     .rows = opts.rows,
        //     .reflow = self.modes.get(.wraparound),
        //     .prompt_redraw = self.flags.shell_redraws_prompt,
        //     .pull_scrollback = self.flags.resize_pull_scrollback,
        // });
        let reflow = self.modes.get(modespkg::Mode::Wraparound);
        let prompt_redraw = self.flags.shell_redraws_prompt;
        let pull_scrollback = self.flags.resize_pull_scrollback;
        let primary: *mut Screen = self.screens.get(crate::screen_set::Key::Primary).unwrap();

        #[cfg(test)]
        if let Err(e) = resize_tw_check(ResizeFp::PrimaryScreen) {
            // errdefer if (new_tabstops) |*v| v.deinit(alloc);
            if let Some(v) = new_tabstops.take() {
                v.deinit(alloc);
            }
            self.resize_rollback_geometry(old_width_px, old_height_px, old_synchronized_output);
            return Err(e.into());
        }
        // SAFETY: `primary` is a stable heap pointer owned by `self.screens`;
        // `resize` takes `&mut Screen` and does not mutate `self.screens`.
        if let Err(e) = unsafe {
            (*primary).resize(crate::screen::Resize {
                cols,
                rows,
                reflow,
                prompt_redraw,
                pull_scrollback,
            })
        } {
            if let Some(v) = new_tabstops.take() {
                v.deinit(alloc);
            }
            self.resize_rollback_geometry(old_width_px, old_height_px, old_synchronized_output);
            return Err(e.into());
        }

        // Alternate screen, if it exists, doesn't reflow. The primary resize
        // above can't be losslessly undone, so if the alternate resize fails we
        // replace it with an empty screen at the requested size. If that
        // also fails, we fall back to the primary screen.
        if let Some(alt) = self.screens.get(crate::screen_set::Key::Alternate) {
            'alt: {
                let err: ResizeError = 'resize: {
                    #[cfg(test)]
                    if let Err(e) = resize_tw_check(ResizeFp::AlternateScreen) {
                        break 'resize e.into();
                    }
                    // SAFETY: `alt` is a stable heap pointer owned by `self.screens`.
                    match unsafe {
                        (*alt).resize(crate::screen::Resize {
                            cols,
                            rows,
                            reflow: false,
                            // PORT NOTE: the reference omits `prompt_redraw`
                            // here, so it takes the `Screen.Resize` struct field
                            // default, which is `.false` (`Screen.zig`
                            // `prompt_redraw: ... = .false`).
                            //
                            // PORT FINDING (P3-W4 T2): this previously read
                            // `Redraw::True`, citing a `.true` default the
                            // reference has never had. The alternate screen
                            // would therefore clear its cursor row on resize
                            // whenever the cursor's semantic content was not
                            // `.output`. Guarded by
                            // `terminal_resize_does_not_prompt_redraw_the_alternate_screen`.
                            prompt_redraw: osc::semantic_prompt::Redraw::False,
                            pull_scrollback,
                        })
                    } {
                        Err(e) => break 'resize e.into(),
                        // Resize succeeded.
                        Ok(()) => break 'alt,
                    }
                };

                log::warn!(target: LOG_SCOPE, "alternate screen resize failed, replacing it err={err:?}");

                // If the alternate screen isn't active, then we just free it
                // and move on. It'll be reallocated when it gets reinitialized
                // lazily. In this case, we just lose the prior data if the
                // terminal program expected it to be saved.
                if self.screens.active_key != crate::screen_set::Key::Alternate {
                    self.screens
                        .remove(alloc, crate::screen_set::Key::Alternate);
                    break 'alt;
                }

                // The alt screen is active, so we temporarily switch to primary
                // so we can safely remove the alt and recreate it blank. This
                // loses the data but hopefully keeps us on the alt screen.
                // SAFETY: `alt` is still live here (nothing has removed it yet).
                let charset = unsafe { (*alt).charset.clone() };
                self.screens.switch_to(crate::screen_set::Key::Primary);
                self.screens
                    .remove(alloc, crate::screen_set::Key::Alternate);

                // Replace the alt screen with an empty version. If this fails
                // we just go back to the primary screen. Not great, but best
                // we can do.
                // tw.check(.alternate_screen_init) catch break :alt;
                #[cfg(test)]
                if resize_tw_check(ResizeFp::AlternateScreenInit).is_err() {
                    break 'alt;
                }
                // PORT NOTE (oracle b988efcfe): upstream's 0.16 regression fix
                // changed this call's first argument from `alt.io` to
                // `self.io()`. rustty's `ScreenSet::get_init` has no `io`
                // parameter at all, so the pre-fix form is simply never
                // reproduced and there is nothing to port.
                let replacement = match self.screens.get_init(
                    alloc,
                    crate::screen_set::Key::Alternate,
                    crate::screen::Options {
                        cols,
                        rows,
                        // .max_scrollback = 0
                        max_scrollback_bytes: Some(0),
                        max_scrollback_lines: None,
                        // Inherit our Kitty image settings from the primary
                        // screen (see `switch_screen`'s PORT NOTE: kitty
                        // graphics IS enabled in rustty, so the reference's
                        // `build_options.kitty_graphics` arm is the live one).
                        kitty_image_storage_limit: unsafe { (*primary).kitty_images.total_limit },
                        kitty_image_loading_limits: unsafe {
                            (*primary).kitty_images.image_limits.clone()
                        },
                    },
                ) {
                    Ok(r) => r,
                    Err(init_err) => {
                        log::warn!(
                            target: LOG_SCOPE,
                            "alternate screen replacement failed, falling back to primary err={init_err:?}"
                        );
                        break 'alt;
                    }
                };

                // SAFETY: freshly initialized screen owned by `self.screens`.
                unsafe { (*replacement).charset = charset };
                self.screens.switch_to(crate::screen_set::Key::Alternate);
            }
        }

        // errdefer comptime unreachable;
        //
        // PORT NOTE: "No more failures are allowed after this point because the
        // screens have committed their new sizes and the remaining Terminal
        // state must follow." Zig's `errdefer comptime unreachable` is a
        // COMPILE-TIME assertion that no fallible operation follows; Rust has no
        // equivalent, so the property is held by construction — nothing below
        // returns `Err` and the function's only remaining exit is `Ok(())`.
        // The marker below is that contract written down at the commit point,
        // as a real (non-vacuous) invariant: reaching here means every screen
        // has already committed to the new size. A bare `debug_assert!(true)`
        // would be a no-op that clippy rightly rejects.
        debug_assert!(
            unsafe { (*primary).pages.cols == cols && (*primary).pages.rows == rows },
            "resize commit point reached without the primary screen at the new size"
        );

        // All fallible work is complete. Replace the old tabstop table only now.
        if let Some(v) = new_tabstops.take() {
            // PORT NOTE: `Tabstops::deinit` consumes `self` by value (mirrors
            // the reference `self.tabstops.deinit(alloc)`); we take the field
            // out via `core::mem::take` (leaving an empty default) to call it,
            // then overwrite with the freshly initialized tabstops.
            core::mem::take(&mut self.tabstops).deinit(alloc);
            self.tabstops = v;
        }

        // Whenever we resize we just mark it as a screen clear
        // self.flags.dirty.clear = true;
        self.flags.dirty.clear = true;

        // Set our size
        // self.cols = cols;
        // self.rows = rows;
        self.cols = cols;
        self.rows = rows;

        // Reset the scrolling region
        // self.scrolling_region = .{
        //     .top = 0,
        //     .bottom = rows - 1,
        //     .left = 0,
        //     .right = cols - 1,
        // };
        self.scrolling_region = ScrollingRegion {
            top: 0,
            bottom: rows - 1,
            left: 0,
            right: cols - 1,
        };

        Ok(())
    }

    /// The pixel-geometry / synchronized-output `errdefer` of `resize`,
    /// spelled out (Rust has no `errdefer`).
    fn resize_rollback_geometry(
        &mut self,
        old_width_px: u32,
        old_height_px: u32,
        old_synchronized_output: bool,
    ) {
        self.width_px = old_width_px;
        self.height_px = old_height_px;
        // NOTE: when `old_synchronized_output` is false this re-clears the mode
        // and therefore bumps `modes::sync_output_clear_count`. That counter
        // measures the SUCCESS path (see its PORT NOTE); a rollback restoring
        // `false` is indistinguishable from a clear at the `ModeState::set`
        // seam, and no assertion counts an error path.
        self.modes
            .set(modespkg::Mode::SynchronizedOutput, old_synchronized_output);
    }
}
