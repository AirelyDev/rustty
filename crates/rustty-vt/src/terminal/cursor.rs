//! PORTS: ghostty src/terminal/Terminal.zig — the charset, carriage-return / linefeed / backspace, `cursor*` movement, save/restore cursor, protected mode, semantic prompt and tabstop cluster.

use super::*;

impl Terminal {
    /// Set the charset into the given slot.
    pub fn configure_charset(&mut self, slot: charsets::Slots, set: charsets::Charset) {
        // self.screens.active.charset.charsets.set(slot, set);
        self.active_mut().charset.charsets.set(slot, set);
    }

    /// Invoke the charset in slot into the active slot. If single is true,
    /// then this will only be invoked for a single character.
    pub fn invoke_charset(
        &mut self,
        active: charsets::ActiveSlot,
        slot: charsets::Slots,
        single: bool,
    ) {
        if single {
            debug_assert!(active == charsets::ActiveSlot::GL);
            self.active_mut().charset.single_shift = Some(slot);
            return;
        }

        match active {
            charsets::ActiveSlot::GL => self.active_mut().charset.gl = slot,
            charsets::ActiveSlot::GR => self.active_mut().charset.gr = slot,
        }
    }

    /// Carriage return moves the cursor to the first column.
    pub fn carriage_return(&mut self) {
        // Always reset pending wrap state
        // self.screens.active.cursor.pending_wrap = false;
        unsafe { (*self.screens.active).cursor.pending_wrap = false };

        // In origin mode we always move to the left margin
        // self.screens.active.cursorHorizontalAbsolute(if (self.modes.get(.origin))
        //     self.scrolling_region.left
        // else if (self.screens.active.cursor.x >= self.scrolling_region.left)
        //     self.scrolling_region.left
        // else
        //     0);
        // PORT NOTE: the reference's first two branches both evaluate to
        // `scrolling_region.left`; this is intentional verbatim structure (origin
        // mode vs. cursor-at-or-right-of-left-margin are distinct conditions that
        // happen to share a result), so we silence `if_same_then_else`.
        #[allow(clippy::if_same_then_else)]
        let x = if self.modes.get(modespkg::Mode::Origin) {
            self.scrolling_region.left
        } else if unsafe { (*self.screens.active).cursor.x } >= self.scrolling_region.left {
            self.scrolling_region.left
        } else {
            0
        };
        self.active_mut().cursor_horizontal_absolute(x);
    }

    /// Linefeed moves the cursor to the next line.
    pub fn linefeed(&mut self) -> Result<(), OutOfMemory> {
        // try self.index();
        // if (self.modes.get(.linefeed)) self.carriageReturn();
        self.index()?;
        if self.modes.get(modespkg::Mode::Linefeed) {
            self.carriage_return();
        }
        Ok(())
    }

    /// Backspace moves the cursor back a column (but not less than 0).
    pub fn backspace(&mut self) {
        // self.cursorLeft(1);
        self.cursor_left(1);
    }

    /// Move the cursor up amount lines. If amount is greater than the maximum
    /// move distance then it is internally adjusted to the maximum. If amount is
    /// 0, adjust it to 1.
    pub fn cursor_up(&mut self, count_req: usize) {
        // Always resets pending wrap
        // self.screens.active.cursor.pending_wrap = false;
        unsafe { (*self.screens.active).cursor.pending_wrap = false };

        // The maximum amount the cursor can move up depends on scrolling regions
        // const max = if (self.screens.active.cursor.y >= self.scrolling_region.top)
        //     self.screens.active.cursor.y - self.scrolling_region.top
        // else
        //     self.screens.active.cursor.y;
        let cursor_y = unsafe { (*self.screens.active).cursor.y };
        let max: usize = if cursor_y >= self.scrolling_region.top {
            (cursor_y - self.scrolling_region.top) as usize
        } else {
            cursor_y as usize
        };
        // const count = @min(max, @max(count_req, 1));
        let count = core::cmp::min(max, core::cmp::max(count_req, 1));

        // We can safely intCast below because of the min/max clamping we did above.
        // self.screens.active.cursorUp(@intCast(count));
        self.active_mut().cursor_up(count as size::CellCountInt);
    }

    /// Move the cursor down amount lines. If amount is greater than the maximum
    /// move distance then it is internally adjusted to the maximum. This sequence
    /// will not scroll the screen or scroll region. If amount is 0, adjust it to 1.
    pub fn cursor_down(&mut self, count_req: usize) {
        // Always resets pending wrap
        // self.screens.active.cursor.pending_wrap = false;
        unsafe { (*self.screens.active).cursor.pending_wrap = false };

        // The max the cursor can move to depends where the cursor currently is
        // const max = if (self.screens.active.cursor.y <= self.scrolling_region.bottom)
        //     self.scrolling_region.bottom - self.screens.active.cursor.y
        // else
        //     self.rows - self.screens.active.cursor.y - 1;
        let cursor_y = unsafe { (*self.screens.active).cursor.y };
        let max: usize = if cursor_y <= self.scrolling_region.bottom {
            (self.scrolling_region.bottom - cursor_y) as usize
        } else {
            (self.rows - cursor_y - 1) as usize
        };
        // const count = @min(max, @max(count_req, 1));
        let count = core::cmp::min(max, core::cmp::max(count_req, 1));
        // self.screens.active.cursorDown(@intCast(count));
        self.active_mut().cursor_down(count as size::CellCountInt);
    }

    /// Move the cursor right amount columns. If amount is greater than the
    /// maximum move distance then it is internally adjusted to the maximum.
    /// This sequence will not scroll the screen or scroll region. If amount is
    /// 0, adjust it to 1.
    pub fn cursor_right(&mut self, count_req: usize) {
        // Always resets pending wrap
        // self.screens.active.cursor.pending_wrap = false;
        unsafe { (*self.screens.active).cursor.pending_wrap = false };

        // The max the cursor can move to depends where the cursor currently is
        // const max = if (self.screens.active.cursor.x <= self.scrolling_region.right)
        //     self.scrolling_region.right - self.screens.active.cursor.x
        // else
        //     self.cols - self.screens.active.cursor.x - 1;
        let cursor_x = unsafe { (*self.screens.active).cursor.x };
        let max: usize = if cursor_x <= self.scrolling_region.right {
            (self.scrolling_region.right - cursor_x) as usize
        } else {
            (self.cols - cursor_x - 1) as usize
        };
        // const count = @min(max, @max(count_req, 1));
        let count = core::cmp::min(max, core::cmp::max(count_req, 1));
        // self.screens.active.cursorRight(@intCast(count));
        self.active_mut().cursor_right(count as size::CellCountInt);
    }

    /// Move the cursor to the left amount cells. If amount is 0, adjust it to 1.
    pub fn cursor_left(&mut self, count_req: usize) {
        // Wrapping behavior depends on various terminal modes
        // const WrapMode = enum { none, reverse, reverse_extended };
        enum WrapMode {
            None,
            Reverse,
            ReverseExtended,
        }
        // const wrap_mode: WrapMode = wrap_mode: {
        //     if (!self.modes.get(.wraparound)) break :wrap_mode .none;
        //     if (self.modes.get(.reverse_wrap_extended)) break :wrap_mode .reverse_extended;
        //     if (self.modes.get(.reverse_wrap)) break :wrap_mode .reverse;
        //     break :wrap_mode .none;
        // };
        let wrap_mode: WrapMode = 'wrap_mode: {
            if !self.modes.get(modespkg::Mode::Wraparound) {
                break 'wrap_mode WrapMode::None;
            }
            if self.modes.get(modespkg::Mode::ReverseWrapExtended) {
                break 'wrap_mode WrapMode::ReverseExtended;
            }
            if self.modes.get(modespkg::Mode::ReverseWrap) {
                break 'wrap_mode WrapMode::Reverse;
            }
            WrapMode::None
        };

        // var count = @max(count_req, 1);
        let mut count: usize = core::cmp::max(count_req, 1);

        // If we are in no wrap mode, then we move the cursor left and exit
        // since this is the fastest and most typical path.
        if matches!(wrap_mode, WrapMode::None) {
            // self.screens.active.cursorLeft(@min(count, self.screens.active.cursor.x));
            let cursor_x = unsafe { (*self.screens.active).cursor.x };
            self.active_mut()
                .cursor_left(core::cmp::min(count, cursor_x as usize) as size::CellCountInt);
            // self.screens.active.cursor.pending_wrap = false;
            unsafe { (*self.screens.active).cursor.pending_wrap = false };
            return;
        }

        // If we have a pending wrap state and we are in either reverse wrap
        // modes then we decrement the amount we move by one to match xterm.
        // if (self.screens.active.cursor.pending_wrap) {
        //     count -= 1;
        //     self.screens.active.cursor.pending_wrap = false;
        //     if (count == 0) return;
        // }
        if unsafe { (*self.screens.active).cursor.pending_wrap } {
            count -= 1;
            unsafe { (*self.screens.active).cursor.pending_wrap = false };
            if count == 0 {
                return;
            }
        }

        // The margins we can move to.
        // const top = self.scrolling_region.top;
        // const bottom = self.scrolling_region.bottom;
        // const right_margin = self.scrolling_region.right;
        // const left_margin = if (self.screens.active.cursor.x < self.scrolling_region.left)
        //     0
        // else
        //     self.scrolling_region.left;
        let top = self.scrolling_region.top;
        let bottom = self.scrolling_region.bottom;
        let right_margin = self.scrolling_region.right;
        let left_margin = if unsafe { (*self.screens.active).cursor.x } < self.scrolling_region.left
        {
            0
        } else {
            self.scrolling_region.left
        };

        // Handle some edge cases when our cursor is already on the left margin.
        // if (self.screens.active.cursor.x == left_margin) {
        if unsafe { (*self.screens.active).cursor.x } == left_margin {
            match wrap_mode {
                // In reverse mode, if we're already before the top margin
                // then we just set our cursor to the top-left and we're done.
                // .reverse => if (self.screens.active.cursor.y <= top) {
                //     self.screens.active.cursorAbsolute(left_margin, top);
                //     return;
                // },
                WrapMode::Reverse => {
                    if unsafe { (*self.screens.active).cursor.y } <= top {
                        self.active_mut().cursor_absolute(left_margin, top);
                        return;
                    }
                }

                // Handled in while loop
                WrapMode::ReverseExtended => {}

                // Handled above
                WrapMode::None => unreachable!(),
            }
        }

        // while (true) {
        loop {
            // We can move at most to the left margin.
            // const max = self.screens.active.cursor.x - left_margin;
            let max = unsafe { (*self.screens.active).cursor.x } - left_margin;

            // We want to move at most the number of columns we have left
            // or our remaining count. Do the move.
            // const amount = @min(max, count);
            // count -= amount;
            // self.screens.active.cursorLeft(amount);
            let amount = core::cmp::min(max as usize, count);
            count -= amount;
            self.active_mut().cursor_left(amount as size::CellCountInt);

            // If we have no more to move, then we're done.
            // if (count == 0) break;
            if count == 0 {
                break;
            }

            // If we are at the top, then we are done.
            // if (self.screens.active.cursor.y == top) {
            //     if (wrap_mode != .reverse_extended) break;
            //     self.screens.active.cursorAbsolute(right_margin, bottom);
            //     count -= 1;
            //     continue;
            // }
            if unsafe { (*self.screens.active).cursor.y } == top {
                if !matches!(wrap_mode, WrapMode::ReverseExtended) {
                    break;
                }

                self.active_mut().cursor_absolute(right_margin, bottom);
                count -= 1;
                continue;
            }

            // UNDEFINED TERMINAL BEHAVIOR. This situation is not handled in xterm
            // and currently results in a crash in xterm. Given no other known
            // terminal [to me] implements XTREVWRAP2, I decided to just mimic
            // the behavior of xterm up and not including the crash by wrapping
            // up to the (0, 0) and stopping there. My reasoning is that for an
            // appropriately sized value of "count" this is the behavior that xterm
            // would have. This is unit tested.
            // if (self.screens.active.cursor.y == 0) {
            //     assert(self.screens.active.cursor.x == left_margin);
            //     break;
            // }
            if unsafe { (*self.screens.active).cursor.y } == 0 {
                debug_assert!(unsafe { (*self.screens.active).cursor.x } == left_margin);
                break;
            }

            // If our previous line is not wrapped then we are done.
            // if (wrap_mode != .reverse_extended) {
            //     const prev_row = self.screens.active.cursorRowUp(1);
            //     if (!prev_row.wrap) break;
            // }
            if !matches!(wrap_mode, WrapMode::ReverseExtended) {
                let prev_row = self.active_mut().cursor_row_up(1);
                if !unsafe { (*prev_row).wrap() } {
                    break;
                }
            }

            // self.screens.active.cursorAbsolute(right_margin, self.screens.active.cursor.y - 1);
            // count -= 1;
            let new_y = unsafe { (*self.screens.active).cursor.y } - 1;
            self.active_mut().cursor_absolute(right_margin, new_y);
            count -= 1;
        }
    }

    /// Save cursor position and further state.
    ///
    /// The primary and alternate screen have distinct save state. One saved state
    /// is kept per screen (main / alternative). If for the current screen state
    /// was already saved it is overwritten.
    pub fn save_cursor(&mut self) {
        // self.screens.active.saved_cursor = .{
        //     .x = self.screens.active.cursor.x,
        //     .y = self.screens.active.cursor.y,
        //     .style = self.screens.active.cursor.style,
        //     .protected = self.screens.active.cursor.protected,
        //     .pending_wrap = self.screens.active.cursor.pending_wrap,
        //     .origin = self.modes.get(.origin),
        //     .charset = self.screens.active.charset,
        // };
        let origin = self.modes.get(modespkg::Mode::Origin);
        let active: *mut Screen = self.screens.active;
        let saved = crate::screen::SavedCursor {
            x: unsafe { (*active).cursor.x },
            y: unsafe { (*active).cursor.y },
            style: unsafe { (*active).cursor.style },
            protected: unsafe { (*active).cursor.protected },
            pending_wrap: unsafe { (*active).cursor.pending_wrap },
            origin,
            charset: unsafe { (*active).charset.clone() },
        };
        self.active_mut().saved_cursor = Some(saved);
    }

    /// Restore cursor position and other state.
    ///
    /// The primary and alternate screen have distinct save state.
    /// If no save was done before values are reset to their initial values.
    pub fn restore_cursor(&mut self) {
        // const saved: Screen.SavedCursor = self.screens.active.saved_cursor orelse .{
        //     .x = 0,
        //     .y = 0,
        //     .style = .{},
        //     .protected = false,
        //     .pending_wrap = false,
        //     .origin = false,
        //     .charset = .{},
        // };
        // PORT NOTE: `SavedCursor` is a Zig value struct copied by `orelse`; in
        // Rust it is not `Copy` (it holds a `CharsetState`/`CharsetArray`). We
        // borrow the saved state (or a fresh default), then clone the `charset`
        // only where the reference reads `saved.charset` below.
        let default_saved;
        let saved: &crate::screen::SavedCursor =
            match unsafe { &(*self.screens.active).saved_cursor } {
                Some(s) => s,
                None => {
                    default_saved = crate::screen::SavedCursor {
                        x: 0,
                        y: 0,
                        style: style::Style::default(),
                        protected: false,
                        pending_wrap: false,
                        origin: false,
                        charset: crate::screen::CharsetState::default(),
                    };
                    &default_saved
                }
            };
        // Copy out the scalar saved fields up front so we don't hold the borrow
        // of `self.screens.active.saved_cursor` across the `&mut self` calls.
        let saved_style = saved.style;
        let saved_charset = saved.charset.clone();
        let saved_origin = saved.origin;
        let saved_pending_wrap = saved.pending_wrap;
        let saved_protected = saved.protected;
        let saved_x = saved.x;
        let saved_y = saved.y;

        // Set the style first because it can fail
        // self.screens.active.cursor.style = saved.style;
        // self.screens.active.manualStyleUpdate() catch |err| {
        //     log.warn("restoreCursor error updating style err={}", .{err});
        //     const screen: *Screen = self.screens.active;
        //     screen.cursor.style = .{};
        //     self.screens.active.manualStyleUpdate() catch unreachable;
        // };
        unsafe { (*self.screens.active).cursor.style = saved_style };
        if let Err(err) = self.active_mut().manual_style_update() {
            // Regardless of the error here, we revert back to an unstyled
            // cursor. It is more important that the restore succeeds in
            // other attributes because terminals have no way to communicate
            // failure back.
            log::warn!(target: LOG_SCOPE, "restoreCursor error updating style err={err:?}");
            let screen: *mut Screen = self.screens.active;
            unsafe { (*screen).cursor.style = style::Style::default() };
            self.active_mut().manual_style_update().unwrap();
        }

        // self.screens.active.charset = saved.charset;
        // self.modes.set(.origin, saved.origin);
        // self.screens.active.cursor.pending_wrap = saved.pending_wrap;
        // self.screens.active.cursor.protected = saved.protected;
        unsafe { (*self.screens.active).charset = saved_charset };
        self.modes.set(modespkg::Mode::Origin, saved_origin);
        unsafe { (*self.screens.active).cursor.pending_wrap = saved_pending_wrap };
        unsafe { (*self.screens.active).cursor.protected = saved_protected };
        // self.screens.active.cursorAbsolute(
        //     @min(saved.x, self.cols - 1),
        //     @min(saved.y, self.rows - 1),
        // );
        let x = core::cmp::min(saved_x, self.cols - 1);
        let y = core::cmp::min(saved_y, self.rows - 1);
        self.active_mut().cursor_absolute(x, y);

        // Ensure our screen is consistent
        // self.screens.active.assertIntegrity();
        self.active_mut().assert_integrity();
    }

    /// Set the character protection mode for the terminal.
    pub fn set_protected_mode(&mut self, mode: ansi::ProtectedMode) {
        match mode {
            ansi::ProtectedMode::Off => {
                self.active_mut().cursor.protected = false;

                // screen.protected_mode is NEVER reset to ".off" because
                // logic such as eraseChars depends on knowing what the
                // _most recent_ mode was.
            }

            ansi::ProtectedMode::Iso => {
                self.active_mut().cursor.protected = true;
                self.active_mut().protected_mode = ansi::ProtectedMode::Iso;
            }

            ansi::ProtectedMode::Dec => {
                self.active_mut().cursor.protected = true;
                self.active_mut().protected_mode = ansi::ProtectedMode::Dec;
            }
        }
    }

    /// Perform a semantic prompt command.
    ///
    /// If there is an error, we do our best to get the terminal into
    /// some coherent state, since callers typically can't handle errors
    /// (since they're sending sequences via the pty).
    pub fn semantic_prompt(&mut self, cmd: osc::SemanticPrompt) -> Result<(), OutOfMemory> {
        use crate::osc::semantic_prompt::option;
        use crate::osc::semantic_prompt::Action;
        match cmd.action {
            // try self.semanticPromptFreshLine();
            Action::FreshLine => self.semantic_prompt_fresh_line()?,

            Action::FreshLineNewPrompt => {
                // "First do a fresh-line."
                self.semantic_prompt_fresh_line()?;

                // const screen: *Screen = self.screens.active;
                let screen: *mut Screen = self.screens.active;

                // "Subsequent text (until a OSC "133;B" or OSC "133;I" command)
                // is a prompt string (as if followed by OSC 133;P;k=i\007)."
                // screen.cursorSetSemanticContent(.{
                //     .prompt = cmd.readOption(.prompt_kind) orelse .initial,
                // });
                unsafe {
                    (*screen).cursor_set_semantic_content(
                        crate::screen::SemanticContentArg::Prompt(
                            cmd.read_option(option::PROMPT_KIND)
                                .unwrap_or(osc::semantic_prompt::PromptKind::Initial),
                        ),
                    )
                };

                // This is a kitty-specific flag that notes that the shell
                // is NOT capable of redraw. Redraw defaults to true so this
                // usually just disables it, but either is possible.
                // if (cmd.readOption(.redraw)) |v| {
                //     self.flags.shell_redraws_prompt = v;
                // }
                if let Some(v) = cmd.read_option(option::REDRAW) {
                    self.flags.shell_redraws_prompt = v;
                }

                // click: {
                //     // Handle click_events as a priority over cl. click_events
                //     // is another Kitty-specific extension that converts clicks
                //     // within a prompt area to SGR mouse events and defers to the
                //     // shell to handle them.
                //     if (cmd.readOption(.click_events)) |v| {
                //         screen.semantic_prompt.click = .{ .click_events = v };
                //         break :click;
                //     }
                //
                //     // If click_events was not set or disabled, fallback to `cl`.
                //     if (cmd.readOption(.cl)) |v| {
                //         screen.semantic_prompt.click = .{ .cl = v };
                //     }
                // }
                #[allow(clippy::collapsible_else_if)]
                if let Some(v) = cmd.read_option(option::CLICK_EVENTS) {
                    unsafe {
                        (*screen).semantic_prompt.click =
                            crate::screen::SemanticClick::ClickEvents(v);
                    }
                } else {
                    if let Some(v) = cmd.read_option(option::CL) {
                        unsafe {
                            (*screen).semantic_prompt.click = crate::screen::SemanticClick::Cl(v);
                        }
                    }
                }

                // The "aid" and "cl" options are also valid for this
                // command but we don't yet handle these in any meaningful way.
            }

            Action::NewCommand => {
                // Spec:
                // Same as OSC "133;A" but may first implicitly terminate a
                // previous command: if the options specify an aid and there
                // is an active (open) command with matching aid, finish the
                // innermost such command (as well as any other commands
                // nested more deeply). If no aid is specified, treat as an
                // aid whose value is the empty string.

                // Ghostty:
                // We don't currently do explicit command tracking in any way
                // so there is no need to terminate prior commands. We just
                // perform the `A` action.
                // try self.semanticPrompt(.{
                //     .action = .fresh_line_new_prompt,
                //     .options_unvalidated = cmd.options_unvalidated,
                // });
                self.semantic_prompt(osc::SemanticPrompt {
                    action: Action::FreshLineNewPrompt,
                    options_unvalidated: cmd.options_unvalidated,
                })?;
            }

            Action::PromptStart => {
                // Explicit start of prompt. Optional after an A or N command.
                // The k (kind) option specifies the type of prompt:
                // regular primary prompt (k=i or default),
                // right-side prompts (k=r), or prompts for continuation lines (k=c or k=s).
                // self.screens.active.cursorSetSemanticContent(.{
                //     .prompt = cmd.readOption(.prompt_kind) orelse .initial,
                // });
                let screen: *mut Screen = self.screens.active;
                unsafe {
                    (*screen).cursor_set_semantic_content(
                        crate::screen::SemanticContentArg::Prompt(
                            cmd.read_option(option::PROMPT_KIND)
                                .unwrap_or(osc::semantic_prompt::PromptKind::Initial),
                        ),
                    )
                };
            }

            Action::EndPromptStartInput => {
                // End of prompt and start of user input, terminated by a OSC
                // "133;C" or another prompt (OSC "133;P").
                // self.screens.active.cursorSetSemanticContent(.{
                //     .input = .clear_explicit,
                // });
                let screen: *mut Screen = self.screens.active;
                unsafe {
                    (*screen).cursor_set_semantic_content(crate::screen::SemanticContentArg::Input(
                        crate::screen::SemanticContentInput::ClearExplicit,
                    ))
                };
            }

            Action::EndPromptStartInputTerminateEol => {
                // End of prompt and start of user input, terminated by end-of-line.
                // self.screens.active.cursorSetSemanticContent(.{
                //     .input = .clear_eol,
                // });
                let screen: *mut Screen = self.screens.active;
                unsafe {
                    (*screen).cursor_set_semantic_content(crate::screen::SemanticContentArg::Input(
                        crate::screen::SemanticContentInput::ClearEol,
                    ))
                };
            }

            Action::EndInputStartOutput => {
                // "End of input, and start of output."
                // self.screens.active.cursorSetSemanticContent(.output);
                let screen: *mut Screen = self.screens.active;
                unsafe {
                    (*screen).cursor_set_semantic_content(crate::screen::SemanticContentArg::Output)
                };

                // If our current row is marked as a prompt and we're
                // at column zero then we assume we're un-prompting. This
                // is a heuristic to deal with fish, mostly. The issue that
                // fish brings up is that it has no PS2 equivalent and its
                // builtin OSC133 marking doesn't output continuation lines
                // as k=s. So, we assume when we get a newline with a prompt
                // cursor that the new line is also a prompt. But fish changes
                // to output on the newline. So if we're at col 0 we just assume
                // we're overwriting the prompt.
                // if (self.screens.active.cursor.page_row.semantic_prompt != .none and
                //     self.screens.active.cursor.x == 0)
                // {
                //     self.screens.active.cursor.page_row.semantic_prompt = .none;
                // }
                if unsafe { (*(*screen).cursor.page_row).semantic_prompt() }
                    != pagepkg::SemanticPrompt::None
                    && unsafe { (*screen).cursor.x } == 0
                {
                    unsafe {
                        (*(*screen).cursor.page_row)
                            .set_semantic_prompt(pagepkg::SemanticPrompt::None)
                    };
                }
            }

            Action::EndCommand => {
                // From a terminal state perspective, this doesn't really do
                // anything. Other terminals appear to do nothing here. I think
                // its reasonable at this point to reset our semantic content
                // state but the spec doesn't really say what to do.
                // self.screens.active.cursorSetSemanticContent(.output);
                let screen: *mut Screen = self.screens.active;
                unsafe {
                    (*screen).cursor_set_semantic_content(crate::screen::SemanticContentArg::Output)
                };
            }
        }

        Ok(())
    }

    // OSC 133;L
    fn semantic_prompt_fresh_line(&mut self) -> Result<(), OutOfMemory> {
        // const left_margin = if (self.screens.active.cursor.x < self.scrolling_region.left)
        //     0
        // else
        //     self.scrolling_region.left;
        let left_margin = if unsafe { (*self.screens.active).cursor.x } < self.scrolling_region.left
        {
            0
        } else {
            self.scrolling_region.left
        };

        // Spec: "If the cursor is the initial column (left, assuming
        // left-to-right writing), do nothing" This specification is very under
        // specified. We are taking the liberty to assume that in a left/right
        // margin context, if the cursor is outside of the left margin, we treat
        // it as being at the left margin for the purposes of this command.
        // This is arbitrary. If someone has a better reasonable idea we can
        // apply it.
        // if (self.screens.active.cursor.x == left_margin) return;
        if unsafe { (*self.screens.active).cursor.x } == left_margin {
            return Ok(());
        }

        // self.carriageReturn();
        self.carriage_return();
        // try self.index();
        self.index()?;

        Ok(())
    }

    /// Returns true if the cursor is currently at a prompt. Another way to look
    /// at this is it returns false if the shell is currently outputting something.
    /// This requires shell integration (semantic prompt integration).
    ///
    /// If the shell integration doesn't exist, this will always return false.
    pub fn cursor_is_at_prompt(&mut self) -> bool {
        // If we're on the secondary screen, we're never at a prompt.
        // if (self.screens.active_key == .alternate) return false;
        if self.screens.active_key == crate::screen_set::Key::Alternate {
            return false;
        }

        // If our page row is a prompt then we're always at a prompt
        // const cursor: *const Screen.Cursor = &self.screens.active.cursor;
        let cursor: *const crate::screen::Cursor = unsafe { &(*self.screens.active).cursor };
        // if (cursor.page_row.semantic_prompt != .none) return true;
        if unsafe { (*(*cursor).page_row).semantic_prompt() } != pagepkg::SemanticPrompt::None {
            return true;
        }

        // Otherwise, determine our cursor state
        // return switch (cursor.semantic_content) {
        //     .input, .prompt => true,
        //     .output => false,
        // };
        match unsafe { (*cursor).semantic_content } {
            pagepkg::SemanticContent::Input | pagepkg::SemanticContent::Prompt => true,
            pagepkg::SemanticContent::Output => false,
        }
    }

    /// Horizontal tab moves the cursor to the next tabstop, clearing
    /// the screen to the left the tabstop.
    pub fn horizontal_tab(&mut self) {
        // while (self.screens.active.cursor.x < self.scrolling_region.right) {
        while unsafe { (*self.screens.active).cursor.x } < self.scrolling_region.right {
            // Move the cursor right
            // self.screens.active.cursorRight(1);
            self.active_mut().cursor_right(1);

            // If the last cursor position was a tabstop we return. We do
            // "last cursor position" because we want a space to be written
            // at the tabstop unless we're at the end (the while condition).
            // if (self.tabstops.get(self.screens.active.cursor.x)) return;
            if self
                .tabstops
                .get(unsafe { (*self.screens.active).cursor.x } as usize)
            {
                return;
            }
        }
    }

    // Same as horizontalTab but moves to the previous tabstop instead of the next.
    pub fn horizontal_tab_back(&mut self) {
        // With origin mode enabled, our leftmost limit is the left margin.
        // const left_limit = if (self.modes.get(.origin)) self.scrolling_region.left else 0;
        let left_limit = if self.modes.get(modespkg::Mode::Origin) {
            self.scrolling_region.left
        } else {
            0
        };

        // while (true) {
        loop {
            // If we're already at the edge of the screen, then we're done.
            // if (self.screens.active.cursor.x <= left_limit) return;
            if unsafe { (*self.screens.active).cursor.x } <= left_limit {
                return;
            }

            // Move the cursor left
            // self.screens.active.cursorLeft(1);
            // if (self.tabstops.get(self.screens.active.cursor.x)) return;
            self.active_mut().cursor_left(1);
            if self
                .tabstops
                .get(unsafe { (*self.screens.active).cursor.x } as usize)
            {
                return;
            }
        }
    }

    /// Clear tab stops.
    pub fn tab_clear(&mut self, cmd: csi::TabClear) {
        // switch (cmd) {
        //     .current => self.tabstops.unset(self.screens.active.cursor.x),
        //     .all => self.tabstops.reset(0),
        //     else => log.warn("invalid or unknown tab clear setting: {}", .{cmd}),
        // }
        match cmd {
            csi::TabClear::Current => self
                .tabstops
                .unset(unsafe { (*self.screens.active).cursor.x } as usize),
            csi::TabClear::All => self.tabstops.reset(0),
            csi::TabClear::Other(_) => {
                log::warn!(target: LOG_SCOPE, "invalid or unknown tab clear setting: {cmd:?}");
            }
        }
    }

    /// Set a tab stop on the current cursor.
    /// TODO: test
    pub fn tab_set(&mut self) {
        // self.tabstops.set(self.screens.active.cursor.x);
        self.tabstops
            .set(unsafe { (*self.screens.active).cursor.x } as usize);
    }

    /// TODO: test
    pub fn tab_reset(&mut self) {
        // self.tabstops.reset(TABSTOP_INTERVAL);
        self.tabstops.reset(TABSTOP_INTERVAL as usize);
    }
}
