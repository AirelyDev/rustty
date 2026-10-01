//! PORTS: ghostty src/Surface.zig — the focus, scroll and mouse cluster: `occlusionCallback`,
//! `focusCallback`, `refreshCallback`, `scrollCallback`, `contentScaleCallback`,
//! `mouseReport`, `mouseButtonCallback`, `maybePromptClick`, `linkAtPos`, `linkAtPin`,
//! `processLinks`, `openUrl`, `mousePressureCallback`, `cursorPosCallback`,
//! `colorSchemeCallback`, `posToViewport`, `scrollToBottom`, `hideMouse`, `showMouse`.

#![allow(non_snake_case)]

use super::*;

impl<A> Surface<A>
where
    A: apprt::App + 'static,
    // C0 DECISION (CORE_BLUEPRINT §1, option 1 — the app.rs impl-block
    // precedent): the self-referential associated-type bound; Surface's
    // method tier needs it to call into `App<A>`'s constrained methods
    // (maybeHandleBinding's `performAllChainedAction`, C10).
    A::Surface: ApprtSurface<App = A, Core = Surface<A>>,
{
    /// Callback for when the surface is fully visible or not, regardless
    /// of focus state. This is used to pause rendering when the surface
    /// is not visible, and also re-render when it becomes visible again.
    // pub fn occlusionCallback(self: *Surface, visible: bool) !void {
    //
    // (the inferred `!void` is `queueRender`'s xev notify error — carried by
    // the [`Error`] union, the sizeCallback convention.)
    pub fn occlusionCallback(&mut self, visible: bool) -> Result<(), Error> {
        // Crash metadata in case we crash in here
        // crash.sentry.thread_state = self.crashThreadState();
        crash::sentry::setThreadState(self.crashThreadState());
        // defer crash.sentry.thread_state = null;
        // ([ADV 6.1] — the Drop guard clears at every exit below.)
        let _thread_state = ThreadStateGuard;

        // PORT NOTE (oracle 6c8c07981): this CHANGES EXISTING BEHAVIOUR, it is
        // not additive. Before this commit every occlusion callback pushed a
        // renderer `Visible` message and queued a render; now a callback whose
        // state matches the last one returns early, so repeated same-state
        // callbacks NO LONGER reach the renderer at all.
        //
        // VERIFIED (V21, `P3-W11 Task 6`): the dedup, the `flags.visible` write
        // under the renderer lock and the mode-2033-gated report are covered by
        // `crates/rustty-core/tests/surface_visibility_kat.rs`, which builds the
        // `TestSurface` the V21 recipe describes. Deleting the early-return
        // below makes it fail (a repeat callback then pushes a second renderer
        // message). The neighbouring halves stay covered by
        //   - `rustty-vt` `stream_terminal::tests::visibility_reports` (the RIS
        //     exemption and the vt-side report/mode arms), and
        //   - `rustty-termio`
        //     `stream_handler::tests::stream_handler_visibility_report_arms_push_forced_messages`
        //     (the mode-2033 + DSR arms).
        // STILL OPEN in V21: the last hop, "the queued `VisibilityReport` is
        // drained by `termio::Thread` and the encoded bytes land in the pty
        // write queue" — that needs a live pty + io thread. The KAT pins the
        // encoded payload (`\x1b[?999;2n`) but not the delivery.
        //
        // Avoid duplicate renderer and visibility reports.
        // if (self.visible == visible) return;
        // self.visible = visible;
        if self.visible == visible {
            return Ok(());
        }
        self.visible = visible;

        // Update the terminal state for synchronous queries, then notify the IO
        // thread so it can emit a mode 2033 report when enabled.
        // self.renderer_state.mutex.lockUncancelable(global.io());
        // self.io.terminal.flags.visible = visible;
        // const report_visibility = self.io.terminal.modes.get(.report_visibility);
        // self.renderer_state.mutex.unlock(global.io());
        //
        // SAFETY: `renderer_state.mutex` is the guard-less shared lock; `io` is
        // the live Termio owned by this Surface.
        unsafe { (*(*self.renderer_state.as_ptr()).mutex).lock() };
        let report_visibility = unsafe {
            let t = &raw mut (*self.io.as_ptr()).terminal;
            (*t).flags.visible = visible;
            (*t).modes.get(rustty_vt::modes::Mode::ReportVisibility)
        };
        unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
        // if (report_visibility) {
        //     self.queueIo(.{ .visibility_report = .{ .visible = visible, .force = false } }, .unlocked);
        // }
        if report_visibility {
            self.queueIo(
                termio::Message::VisibilityReport {
                    visible,
                    force: false,
                },
                termio::termio_impl::MutexState::Unlocked,
            );
        }

        // _ = self.renderer_thread.mailbox.push(.{
        //     .visible = visible,
        // }, .{ .forever = {} });
        let _ = unsafe { &*(*self.renderer_thread.as_ptr()).mailbox }
            .push(rendererpkg::Message::Visible(visible), Timeout::Forever);

        // try self.queueRender();
        self.queueRender()?;
        Ok(())
    }

    // pub fn focusCallback(self: *Surface, focused: bool) !void {
    pub fn focusCallback(&mut self, focused: bool) -> Result<(), Error> {
        // Crash metadata in case we crash in here
        // crash.sentry.thread_state = self.crashThreadState();
        crash::sentry::setThreadState(self.crashThreadState());
        // defer crash.sentry.thread_state = null;
        // ([ADV 6.1] — the Drop guard clears at every exit below.)
        let _thread_state = ThreadStateGuard;

        // Always update the app focused surface, otherwise we miss
        // the first surface created.
        // if (focused) self.app.focusSurface(self);
        if focused {
            let app = self.app;
            unsafe { (*app).focusSurface(self) };
        }

        // If our focus state is unchanged we do nothing else.
        // if (self.focused == focused) return;
        if self.focused == focused {
            return Ok(());
        }
        // self.focused = focused;
        self.focused = focused;

        // Notify our render thread of the new state
        // _ = self.renderer_thread.mailbox.push(.{
        //     .focus = focused,
        // }, .{ .forever = {} });
        let _ = unsafe { &*(*self.renderer_thread.as_ptr()).mailbox }
            .push(rendererpkg::Message::Focus(focused), Timeout::Forever);

        // if (!focused) unfocused: {
        if !focused {
            'unfocused: {
                // If we lost focus and we have a keypress, then we want to send a key
                // release event for it. Depending on the apprt, this CAN result in
                // duplicate key release events, but that is better than not sending
                // a key release event at all.
                // var pressed_key = self.pressed_key orelse break :unfocused;
                // self.pressed_key = null;
                //
                // (the `orelse` value copy + the null-out collapse into the
                // owning `take()` — KeyEvent owns its utf8 bytes.)
                let mut pressed_key = match self.pressed_key.take() {
                    Some(v) => v,
                    None => break 'unfocused,
                };

                // All our actions will be releases
                // pressed_key.action = .release;
                pressed_key.action = key::Action::Release;

                // Release the full key first
                // if (pressed_key.key != .unidentified) {
                if pressed_key.key != Key::Unidentified {
                    // assert(self.keyCallback(pressed_key) catch |err| err: {
                    //     log.warn("error releasing key on focus loss err={}", .{err});
                    //     break :err .ignored;
                    // } != .closed);
                    //
                    // (`assert` -> `debug_assert!` on the hoisted keyCallback
                    // result — the side-effecting call always evaluates, only
                    // the comparison is safety-gated; the caught error
                    // collapses to the `.ignored` fallback, the log is the
                    // comment above.)
                    // CORRECTED (task 14d-1): `unwrap_or` DISCARDED the error
                    // the reference logs. The `catch |err| err: { log.warn(...);
                    // break :err .ignored; }` block is restored whole — the
                    // fallback AND its diagnostic.
                    let effect = match unsafe { Self::keyCallback(self, pressed_key.clone()) } {
                        Ok(v) => v,
                        Err(err) => {
                            // log.warn("error releasing key on focus loss err={}", .{err});
                            log::warn!(
                                target: LOG_SCOPE,
                                "error releasing key on focus loss err={err:?}"
                            );
                            InputEffect::Ignored
                        }
                    };
                    debug_assert!(effect != InputEffect::Closed);
                }

                // Release any modifiers if set
                // if (pressed_key.mods.empty()) break :unfocused;
                if pressed_key.mods.empty() {
                    break 'unfocused;
                }

                // This is kind of nasty comptime meta programming but all we're doing
                // here is going through all the modifiers and if they're set, releasing
                // both the left and right sides of the modifier. This may not match
                // the exact input event but it ensures a full reset.
                // const keys = &.{ "shift", "ctrl", "alt", "super" };
                // const original_key = pressed_key.key;
                let original_key = pressed_key.key;
                // inline for (keys) |key| {
                //     if (@field(pressed_key.mods, key)) {
                //         @field(pressed_key.mods, key) = false;
                //         inline for (&.{ "right", "left" }) |side| {
                //             const keyname = comptime keyname: {
                //                 break :keyname if (std.mem.eql(u8, key, "ctrl"))
                //                     "control"
                //                 else if (std.mem.eql(u8, key, "super"))
                //                     "meta"
                //                 else
                //                     key;
                //             };
                //             pressed_key.key = @field(input.Key, keyname ++ "_" ++ side);
                //             if (pressed_key.key != original_key) {
                //                 assert(self.keyCallback(pressed_key) catch |err| err: {
                //                     log.warn("error releasing key on focus loss err={}", .{err});
                //                     break :err .ignored;
                //                 } != .closed);
                //             }
                //         }
                //     }
                // }
                //
                // PORT NOTE (comptime unroll): Rust has no `@field`; the
                // local macro replays the comptime loop — same mods order
                // (shift, ctrl, alt, super), same side order (right, left),
                // same keyname map (ctrl -> control, super -> meta); the
                // inner assert maps exactly as the release-the-full-key site
                // above.
                macro_rules! release_mod {
                    ($field:ident, $right:ident, $left:ident) => {
                        if pressed_key.mods.$field {
                            pressed_key.mods.$field = false;
                            for side in [Key::$right, Key::$left] {
                                pressed_key.key = side;
                                if pressed_key.key != original_key {
                                    // (same restored catch block as the
                                    // release-the-full-key site above.)
                                    let effect = match unsafe { Self::keyCallback(self, pressed_key.clone()) } {
                                        Ok(v) => v,
                                        Err(err) => {
                                            // log.warn("error releasing key on focus loss err={}", .{err});
                                            log::warn!(
                                                target: LOG_SCOPE,
                                                "error releasing key on focus loss err={err:?}"
                                            );
                                            InputEffect::Ignored
                                        }
                                    };
                                    debug_assert!(effect != InputEffect::Closed);
                                }
                            }
                        }
                    };
                }
                release_mod!(shift, ShiftRight, ShiftLeft);
                release_mod!(ctrl, ControlRight, ControlLeft);
                release_mod!(alt, AltRight, AltLeft);
                release_mod!(super_, MetaRight, MetaLeft);
            }
        }

        // Schedule render which also drains our mailbox
        // try self.queueRender();
        self.queueRender()?;

        // Whenever our focus changes we unhide the mouse. The mouse will be
        // hidden again if the user starts typing. This helps alleviate some
        // buggy behavior upstream in macOS with the mouse never becoming visible
        // again when tabbing between programs (see #2525).
        // self.showMouse();
        self.showMouse();

        // Update the focus state and notify the terminal
        {
            // self.renderer_state.mutex.lock();
            unsafe { (*(*self.renderer_state.as_ptr()).mutex).lock() };
            // self.io.terminal.flags.focused = focused;
            unsafe { (*self.io.as_ptr()).terminal.flags.focused = focused };
            // self.renderer_state.mutex.unlock();
            unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
            // self.queueIo(.{ .focused = focused }, .unlocked);
            self.queueIo(
                termio::Message::Focused(focused),
                termio::termio_impl::MutexState::Unlocked,
            );
        }
        Ok(())
    }

    // pub fn refreshCallback(self: *Surface) !void {
    pub fn refreshCallback(&mut self) -> Result<(), Error> {
        // Crash metadata in case we crash in here
        // crash.sentry.thread_state = self.crashThreadState();
        crash::sentry::setThreadState(self.crashThreadState());
        // defer crash.sentry.thread_state = null;
        // ([ADV 6.1] — the Drop guard clears at every exit below.)
        let _thread_state = ThreadStateGuard;

        // The point of this callback is to schedule a render, so do that.
        // try self.queueRender();
        self.queueRender()?;
        Ok(())
    }

    // PORT NOTE: the file-scope `ScrollAmount` struct sits HERE in the
    // reference (Surface.zig:3400-3415, between refreshCallback and
    // scrollCallback); Rust impl blocks cannot hold type items, so it is
    // ported at module scope below the impl (the KeySequenceQueued trailing
    // cluster) — see [`ScrollAmount`].

    /// Mouse scroll event. Negative is down, left. Positive is up, right.
    ///
    /// "Natural scrolling" is a macOS term for inverting the scroll direction.
    /// This should be handled by the apprt implementation. At this layer,
    /// negative is always down, left.
    // pub fn scrollCallback(
    //     self: *Surface,
    //     xoff: f64,
    //     yoff: f64,
    //     scroll_mods: input.ScrollMods,
    // ) !void {
    pub fn scrollCallback(
        &mut self,
        xoff: f64,
        yoff: f64,
        scroll_mods: input_mouse::ScrollMods,
    ) -> Result<(), Error> {
        // log.info("SCROLL: xoff={} yoff={} mods={}", .{ xoff, yoff, scroll_mods });
        // (COMMENT-ONLY BY DESIGN: commented out in the ORACLE too, Surface.zig:3460.)

        // Crash metadata in case we crash in here
        // crash.sentry.thread_state = self.crashThreadState();
        crash::sentry::setThreadState(self.crashThreadState());
        // defer crash.sentry.thread_state = null;
        // ([ADV 6.1] — the Drop guard clears at every exit below, AFTER the
        // explicit mutex unlocks: the reference's LIFO defer order.)
        let _thread_state = ThreadStateGuard;

        // Always show the mouse again if it is hidden
        // if (self.mouse.hidden) self.showMouse();
        if self.mouse.hidden {
            self.showMouse();
        }

        // const y: ScrollAmount = if (yoff == 0) .{} else y: { ... };
        //
        // PORT NOTE (task R13-P2, UD62): the block body is `scroll_amount_y`,
        // lifted out verbatim so its float cast can be driven by a test
        // (`Surface::init` needs a live apprt). It reads exactly the three
        // fields the block reads: the mouse state, the cell size and the
        // scroll multiplier.
        let y: ScrollAmount = scroll_amount_y(
            &mut self.mouse,
            self.size.cell,
            self.config.mouse_scroll_multiplier,
            yoff,
            scroll_mods,
        );

        // For detailed comments see the y calculation above.
        // const x: ScrollAmount = if (xoff == 0) .{} else x: { ... };
        //
        // (task R13-P2 — lifted into `scroll_amount_x`, as the y block is.)
        let x: ScrollAmount = scroll_amount_x(&mut self.mouse, self.size.cell, xoff, scroll_mods);

        // log.info("SCROLL: delta_y={} delta_x={}", .{ y.delta, x.delta });
        // (COMMENT-ONLY BY DESIGN: commented out in the ORACLE too, Surface.zig:3548.)

        {
            // self.renderer_state.mutex.lock();
            // defer self.renderer_state.mutex.unlock();
            //
            // (guard-less zigstd Mutex — CORE_BLUEPRINT §3; the defer is the
            // explicit unlock at every exit below.)
            unsafe { (*(*self.renderer_state.as_ptr()).mutex).lock() };

            // If we have an active mouse reporting mode, clear the selection.
            // The selection can occur if the user uses the shift mod key to
            // override mouse grabbing from the window.
            // if (self.isMouseReporting()) {
            if self.isMouseReporting() {
                // try self.setSelection(null);
                if let Err(err) = self.setSelection(None) {
                    // (the deferred mutex unlock — the error exit)
                    unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
                    return Err(err);
                }
            }

            // If we're in alternate screen with alternate scroll enabled, then
            // we convert to cursor keys. This only happens if we're:
            // (1) alt screen (2) no explicit mouse reporting and (3) alt
            // scroll mode enabled.
            // if (self.io.terminal.screens.active_key == .alternate and
            //     self.io.terminal.flags.mouse_event == .none and
            //     self.io.terminal.modes.get(.mouse_alternate_scroll))
            // {
            if unsafe { (*self.io.as_ptr()).terminal.screens.active_key }
                == terminal::screen_set::Key::Alternate
                && unsafe { (*self.io.as_ptr()).terminal.flags.mouse_event }
                    == terminal::mouse::Event::None
                && unsafe { &(*self.io.as_ptr()).terminal }
                    .modes
                    .get(terminal::modes::Mode::MouseAlternateScroll)
            {
                // if (y.delta != 0) {
                if y.delta != 0 {
                    // When we send mouse events as cursor keys we always
                    // clear the selection.
                    // try self.setSelection(null);
                    if let Err(err) = self.setSelection(None) {
                        // (the deferred mutex unlock — the error exit)
                        unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
                        return Err(err);
                    }

                    // const seq = if (self.io.terminal.modes.get(.cursor_keys)) seq: {
                    //     // cursor key: application mode
                    //     break :seq switch (y.direction()) {
                    //         .up_right => "\x1bOA",
                    //         .down_left => "\x1bOB",
                    //     };
                    // } else seq: {
                    //     // cursor key: normal mode
                    //     break :seq switch (y.direction()) {
                    //         .up_right => "\x1b[A",
                    //         .down_left => "\x1b[B",
                    //     };
                    // };
                    //
                    // (the `[:0]const u8` literals are static byte strings.)
                    let seq: &'static [u8] = if unsafe { &(*self.io.as_ptr()).terminal }
                        .modes
                        .get(terminal::modes::Mode::CursorKeys)
                    {
                        // cursor key: application mode
                        match y.direction() {
                            ScrollAmountDirection::UpRight => b"\x1bOA",
                            ScrollAmountDirection::DownLeft => b"\x1bOB",
                        }
                    } else {
                        // cursor key: normal mode
                        match y.direction() {
                            ScrollAmountDirection::UpRight => b"\x1b[A",
                            ScrollAmountDirection::DownLeft => b"\x1b[B",
                        }
                    };
                    // for (0..y.magnitude()) |_| {
                    for _ in 0..y.magnitude() {
                        // self.queueIo(.{ .write_stable = seq }, .locked);
                        self.queueIo(
                            termio::Message::WriteStable(seq),
                            termio::termio_impl::MutexState::Locked,
                        );
                    }
                }

                // return;
                // (the deferred mutex unlock — the alternate-scroll return)
                unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
                return Ok(());
            }

            // We have mouse events, are not in an alternate scroll buffer,
            // or have alternate scroll disabled. In this case, we just run
            // the normal logic.

            // If we're scrolling up or down, then send a mouse event.
            // if (self.isMouseReporting()) {
            if self.isMouseReporting() {
                // for (0..@abs(y.delta)) |_| {
                for _ in 0..y.delta.unsigned_abs() {
                    // const pos = try self.rt_surface.getCursorPos();
                    let pos = match unsafe { &*self.rt_surface }.get_cursor_pos() {
                        Ok(v) => v,
                        Err(err) => {
                            // (the deferred mutex unlock — the error exit)
                            unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
                            return Err(err.into());
                        }
                    };
                    // self.mouseReport(switch (y.direction()) {
                    //     .up_right => .four,
                    //     .down_left => .five,
                    // }, .press, self.mouse.mods, pos);
                    self.mouseReport(
                        Some(match y.direction() {
                            ScrollAmountDirection::UpRight => input_mouse::Button::Four,
                            ScrollAmountDirection::DownLeft => input_mouse::Button::Five,
                        }),
                        input_mouse::Action::Press,
                        self.mouse.mods,
                        pos,
                    );
                }

                // for (0..@abs(x.delta)) |_| {
                for _ in 0..x.delta.unsigned_abs() {
                    // const pos = try self.rt_surface.getCursorPos();
                    let pos = match unsafe { &*self.rt_surface }.get_cursor_pos() {
                        Ok(v) => v,
                        Err(err) => {
                            // (the deferred mutex unlock — the error exit)
                            unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
                            return Err(err.into());
                        }
                    };
                    // self.mouseReport(switch (x.direction()) {
                    //     .up_right => .six,
                    //     .down_left => .seven,
                    // }, .press, self.mouse.mods, pos);
                    self.mouseReport(
                        Some(match x.direction() {
                            ScrollAmountDirection::UpRight => input_mouse::Button::Six,
                            ScrollAmountDirection::DownLeft => input_mouse::Button::Seven,
                        }),
                        input_mouse::Action::Press,
                        self.mouse.mods,
                        pos,
                    );
                }

                // If mouse reporting is on, we do not want to scroll the
                // viewport.
                // return;
                // (the deferred mutex unlock — the mouse-report return)
                unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
                return Ok(());
            }

            // if (y.delta != 0) {
            if y.delta != 0 {
                // Modify our viewport, this requires a lock since it affects
                // rendering. We have to switch signs here because our delta
                // is negative down but our viewport is positive down.
                // self.io.terminal.scrollViewport(.{ .delta = y.delta * -1 });
                unsafe { &mut (*self.io.as_ptr()).terminal }
                    .scroll_viewport(terminal::terminal::ScrollViewport::Delta(-y.delta));
            }

            // (the deferred mutex unlock — the block end)
            unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
        }

        // try self.queueRender();
        self.queueRender()?;
        Ok(())
    }

    /// This is called when the content scale of the surface changes. The surface
    /// can then update any DPI-sensitive state.
    // pub fn contentScaleCallback(self: *Surface, content_scale: apprt.ContentScale) !void {
    pub fn contentScaleCallback(
        &mut self,
        content_scale: apprt::ContentScale,
    ) -> Result<(), Error> {
        // Crash metadata in case we crash in here
        // crash.sentry.thread_state = self.crashThreadState();
        crash::sentry::setThreadState(self.crashThreadState());
        // defer crash.sentry.thread_state = null;
        // ([ADV 6.1] — the Drop guard clears at every exit below.)
        let _thread_state = ThreadStateGuard;

        // Calculate the new DPI
        // const x_dpi = content_scale.x * font.face.default_dpi;
        let x_dpi = content_scale.x * f32::from(face::DEFAULT_DPI);
        // const y_dpi = content_scale.y * font.face.default_dpi;
        let y_dpi = content_scale.y * f32::from(face::DEFAULT_DPI);

        // Update our font size which is dependent on the DPI
        // const size = size: {
        let size = 'size: {
            // var size = self.font_size;
            let mut size = self.font_size;
            // size.xdpi = @intFromFloat(@round(x_dpi));
            size.xdpi = x_dpi.round() as u16;
            // size.ydpi = @intFromFloat(@round(y_dpi));
            size.ydpi = y_dpi.round() as u16;
            // break :size size;
            break 'size size;
        };

        // If our DPI didn't actually change, save a lot of work by doing nothing.
        // if (size.xdpi == self.font_size.xdpi and size.ydpi == self.font_size.ydpi) {
        //     return;
        // }
        if size.xdpi == self.font_size.xdpi && size.ydpi == self.font_size.ydpi {
            return Ok(());
        }

        // try self.setFontSize(size);
        self.setFontSize(size)?;

        // Update our padding which is dependent on DPI. We only do this for
        // unbalanced padding since balanced padding is not dependent on DPI.
        // if (self.config.window_padding_balance == .false) {
        if self.config.window_padding_balance == configpkg::value_types::WindowPaddingBalance::False
        {
            // self.size.padding = self.config.scaledPadding(x_dpi, y_dpi);
            self.size.padding = self.config.scaledPadding(x_dpi, y_dpi);
        }

        // Force a resize event because the change in padding will affect
        // pixel-level changes to the renderer and viewport.
        // try self.resize(self.size.screen);
        self.resize(self.size.screen)?;
        Ok(())
    }

    /// Same as `isMouseReporting` but takes the renderer state lock, so it
    /// is safe to call from the apprt (any thread that is not already holding
    /// the lock).
    /// Returns true if mouse reporting is enabled both in the config and
    /// the terminal state.
    // fn isMouseReporting(self: *const Surface) bool {
    //
    // (`*const` receiver -> `&self`.)
    fn isMouseReporting(&self) -> bool {
        // return self.config.mouse_reporting and
        //     self.io.terminal.flags.mouse_event != .none;
        self.config.mouse_reporting
            && unsafe { (*self.io.as_ptr()).terminal.flags.mouse_event }
                != terminal::mouse::Event::None
    }

    // pub fn mouseReportingActive(self: *Surface) bool {
    //
    // (`488b7109b` + the `48d85eaeb` lock fix, END STATE: the gtk apprt asks
    // whether to forward a middle click to a mouse-reporting TUI.)
    pub fn mouseReportingActive(&mut self) -> bool {
        // self.renderer_state.mutex.lockUncancelable(global.io());
        // defer self.renderer_state.mutex.unlock(global.io());
        //
        // (guard-less zigstd Mutex — the `lockUncancelable`/deferred-unlock
        // pair maps to the explicit lock/unlock around the read, the
        // `setVisible` precedent.)
        unsafe { (*(*self.renderer_state.as_ptr()).mutex).lock() };
        // return self.isMouseReporting();
        let result = self.isMouseReporting();
        unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
        result
    }

    // fn mouseReport(
    //     self: *Surface,
    //     button: ?input.MouseButton,
    //     action: input.MouseAction,
    //     mods: input.Mods,
    //     pos: apprt.CursorPos,
    // ) void {
    fn mouseReport(
        &mut self,
        button: Option<input_mouse::Button>,
        action: input_mouse::Action,
        mods: Mods,
        pos: apprt::CursorPos,
    ) {
        // Mouse reporting must be enabled by both config and terminal state
        // assert(self.config.mouse_reporting);
        debug_assert!(self.config.mouse_reporting);
        // assert(self.io.terminal.flags.mouse_event != .none);
        debug_assert!(
            unsafe { (*self.io.as_ptr()).terminal.flags.mouse_event }
                != terminal::mouse::Event::None
        );

        // Build our encoding options.
        // const encoding_opts: input.mouse_encode.Options = opts: {
        let encoding_opts: mouse_encode::Options<'_> = 'opts: {
            // Terminal and size state.
            // var opts: input.mouse_encode.Options = .fromTerminal(
            //     &self.io.terminal,
            //     self.size,
            // );
            //
            // (the ported `from_terminal` takes the two terminal-flags reads
            // directly — the mouse_encode.rs constructor-seam note.)
            let mut opts: mouse_encode::Options<'_> = mouse_encode::Options::from_terminal(
                unsafe { (*self.io.as_ptr()).terminal.flags.mouse_event },
                unsafe { (*self.io.as_ptr()).terminal.flags.mouse_format },
                self.size,
            );

            // Whether any button is pressed at all.
            // opts.any_button_pressed = pressed: {
            opts.any_button_pressed = 'pressed: {
                // for (self.mouse.click_state) |state| {
                for state in self.mouse.click_state {
                    // if (state != .release) break :pressed true;
                    if state != input_mouse::ButtonState::Release {
                        break 'pressed true;
                    }
                }

                // break :pressed false;
                false
            };

            // Keep track of our last reported viewport cell for event
            // deduplication.
            // opts.last_cell = &self.mouse.event_point;
            opts.last_cell = Some(&mut self.mouse.event_point);

            // break :opts opts;
            break 'opts opts;
        };

        // var data: termio.Message.WriteReq.Small.Array = undefined;
        // var writer: std.Io.Writer = .fixed(&data);
        //
        // PORT NOTE: `Small.Array` == `[u8; WriteReq::MAX]`; Zig `undefined`
        // -> zeroed (the tail past `len` is never read). The `.fixed` writer
        // is a Cursor over the stack array — a write past the end is io's
        // `WriteZero`, the reference's `error.WriteFailed` (the encodeKey
        // precedent).
        let mut data = [0u8; termio::message::WriteReq::MAX];
        let mut writer = std::io::Cursor::new(&mut data[..]);
        // input.mouse_encode.encode(&writer, .{
        //     .button = button,
        //     .action = action,
        //     .mods = mods,
        //     .pos = .{
        //         .x = pos.x,
        //         .y = pos.y,
        //     },
        // }, encoding_opts) catch |err| switch (err) {
        //     error.WriteFailed => { ... },
        // };
        if let Err(_err) = mouse_encode::encode(
            &mut writer,
            mouse_encode::Event {
                button,
                action,
                mods,
                pos: mouse_encode::Pos { x: pos.x, y: pos.y },
            },
            encoding_opts,
        ) {
            // This should never happen since mouse events should never
            // be able to overflow the size of our small array. But if it
            // does, let's log it and return. No need to crash upstreams.
            // In the future we may want to fall back to allocation.
            // log.warn("failed to encode mouse event err={}", .{err});
            log::warn!(target: LOG_SCOPE, "failed to encode mouse event err={_err:?}");
            return;
        }
        // const written = writer.buffered();
        let written_len = writer.position() as usize;
        // if (written.len == 0) return;
        if written_len == 0 {
            return;
        }

        // self.queueIo(.{ .write_small = .{
        //     .data = data,
        //     .len = @intCast(written.len),
        // } }, .locked);
        self.queueIo(
            termio::Message::WriteSmall(Small {
                data,
                len: written_len,
            }),
            termio::termio_impl::MutexState::Locked,
        );
    }

    /// Returns true if the shift modifier is allowed to be captured by modifier
    /// events. It is up to the caller to still verify it is a situation in which
    /// shift capture makes sense (i.e. left button, mouse click, etc.)
    // fn mouseShiftCapture(self: *const Surface, lock: bool) bool {
    //
    // (`*const` receiver -> `&self`.)
    pub(super) fn mouseShiftCapture(&self, lock: bool) -> bool {
        // Handle our never/always case where we don't need a lock.
        // switch (self.config.mouse_shift_capture) {
        match self.config.mouse_shift_capture {
            // .never => return false,
            configpkg::value_types::MouseShiftCapture::Never => return false,
            // .always => return true,
            configpkg::value_types::MouseShiftCapture::Always => return true,
            // .false, .true => {},
            configpkg::value_types::MouseShiftCapture::False
            | configpkg::value_types::MouseShiftCapture::True => {}
        }

        // if (lock) self.renderer_state.mutex.lock();
        if lock {
            unsafe { (*(*self.renderer_state.as_ptr()).mutex).lock() };
        }
        // defer if (lock) self.renderer_state.mutex.unlock();
        //
        // (guard-less zigstd Mutex — the defer is the conditional explicit
        // unlock at every exit below.)

        // If the terminal explicitly requests it then we always allow it
        // since we processed never/always at this point.
        // switch (self.io.terminal.flags.mouse_shift_capture) {
        match unsafe { (*self.io.as_ptr()).terminal.flags.mouse_shift_capture } {
            // .false => return false,
            terminal::terminal::MouseShiftCapture::False => {
                // (the deferred conditional unlock — the false return)
                if lock {
                    unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
                }
                return false;
            }
            // .true => return true,
            terminal::terminal::MouseShiftCapture::True => {
                // (the deferred conditional unlock — the true return)
                if lock {
                    unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
                }
                return true;
            }
            // .null => {},
            terminal::terminal::MouseShiftCapture::Null => {}
        }

        // Otherwise, go with the user's preference
        // return switch (self.config.mouse_shift_capture) {
        let result = match self.config.mouse_shift_capture {
            // .false => false,
            configpkg::value_types::MouseShiftCapture::False => false,
            // .true => true,
            configpkg::value_types::MouseShiftCapture::True => true,
            // .never, .always => unreachable, // handled earlier
            configpkg::value_types::MouseShiftCapture::Never
            | configpkg::value_types::MouseShiftCapture::Always => unreachable!(), // handled earlier
        };
        // (the deferred conditional unlock)
        if lock {
            unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
        }
        result
    }

    /// Returns true if the mouse is currently captured by the terminal
    /// (i.e. reporting events).
    // pub fn mouseCaptured(self: *Surface) bool {
    pub fn mouseCaptured(&mut self) -> bool {
        // self.renderer_state.mutex.lock();
        unsafe { (*(*self.renderer_state.as_ptr()).mutex).lock() };
        // defer self.renderer_state.mutex.unlock();
        //
        // (guard-less zigstd Mutex — the defer is the explicit unlock below.)
        // return self.io.terminal.flags.mouse_event != .none;
        let result = unsafe { (*self.io.as_ptr()).terminal.flags.mouse_event }
            != terminal::mouse::Event::None;
        // (the deferred mutex unlock)
        unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
        result
    }

    /// Called for mouse button press/release events. This will return true
    /// if the mouse event was consumed in some way (i.e. the program is capturing
    /// mouse events). If the event was not consumed, then false is returned.
    // pub fn mouseButtonCallback(
    //     self: *Surface,
    //     action: input.MouseButtonState,
    //     button: input.MouseButton,
    //     mods: input.Mods,
    // ) !bool {
    //
    // (the inferred `!bool` is the [`Error`] union.)
    pub fn mouseButtonCallback(
        &mut self,
        action: input_mouse::ButtonState,
        button: input_mouse::Button,
        mods: Mods,
    ) -> Result<bool, Error> {
        // Crash metadata in case we crash in here
        // crash.sentry.thread_state = self.crashThreadState();
        crash::sentry::setThreadState(self.crashThreadState());
        // defer crash.sentry.thread_state = null;
        // ([ADV 6.1] — the Drop guard clears at every exit below.)
        let _thread_state = ThreadStateGuard;

        // log.debug("mouse action={} button={} mods={}", .{ action, button, mods });
        // (COMMENT-ONLY BY DESIGN: commented out in the ORACLE too, Surface.zig:3798.)

        // If we have an inspector, we always queue a render
        // if (self.inspector != null) {
        //     defer self.queueRender() catch {};
        // }
        //
        // PORT NOTE: the reference `defer` is scoped to the `if` BLOCK, so it
        // fires at the block's closing brace — immediately.
        if self.inspector.is_some() {
            let _ = self.queueRender();
        }

        // Always record our latest mouse state
        // self.mouse.click_state[@intCast(@intFromEnum(button))] = action;
        self.mouse.click_state[button as usize] = action;

        // Always show the mouse again if it is hidden
        // if (self.mouse.hidden) self.showMouse();
        if self.mouse.hidden {
            self.showMouse();
        }

        // Update our modifiers if they changed
        // self.modsChanged(mods);
        self.modsChanged(mods);

        // This is set to true if the terminal is allowed to capture the shift
        // modifier. Note we can do this more efficiently probably with less
        // locking/unlocking but clicking isn't that frequent enough to be a
        // bottleneck.
        // const shift_capture = self.mouseShiftCapture(true);
        let shift_capture = self.mouseShiftCapture(true);

        // Shift-click continues the previous mouse state if we have a selection.
        // cursorPosCallback will also do a mouse report so we don't need to do any
        // of the logic below.
        // if (button == .left and action == .press) {
        //
        // PARITY: the reference deliberately splits the two `if`s ("more
        // readable as a human"); clippy's `collapsible_if` is allowed rather
        // than rewriting the control flow (the mouse_encode.rs precedent).
        #[allow(clippy::collapsible_if)]
        if button == input_mouse::Button::Left && action == input_mouse::ButtonState::Press {
            // We could do all the conditionals in one but I find it more
            // readable as a human to break this one up.
            // if (mods.shift and
            //     self.mouse.selection_gesture.left_click_count > 0 and
            //     !shift_capture)
            // extend_selection: {
            if mods.shift && self.mouse.selection_gesture.left_click_count > 0 && !shift_capture {
                'extend_selection: {
                    // We split this conditional out on its own because this is the
                    // only one that requires a renderer mutex grab which is VERY
                    // expensive because it could block all our threads.
                    // if (!self.hasSelection()) break :extend_selection;
                    if !self.hasSelection() {
                        break 'extend_selection;
                    }

                    // If we are within the interval that the click would register
                    // an increment then we do not extend the selection.
                    // if (std.time.Instant.now()) |now| {
                    //
                    // (Rust `Instant::now` is infallible — the `|now|` arm is
                    // live; the reference `else |err|` arm is preserved
                    // verbatim below.)
                    let now = std::time::Instant::now();
                    // const click_time = self.mouse.selection_gesture.left_click_time orelse
                    //     break :extend_selection;
                    let click_time = match self.mouse.selection_gesture.left_click_time {
                        Some(v) => v,
                        None => break 'extend_selection,
                    };
                    // const since = now.since(click_time);
                    //
                    // (`Instant.since` is nanoseconds — the selection_gesture
                    // `time_since` mapping.)
                    let since = now.duration_since(click_time).as_nanos() as u64;
                    // if (since <= self.config.mouse_interval) {
                    if since <= self.config.mouse_interval {
                        // Click interval very short, we may be increasing
                        // click counts so we don't extend the selection.
                        // break :extend_selection;
                        break 'extend_selection;
                    }
                    // } else |err| {
                    //     // This is a weird behavior, I think either behavior is actually
                    //     // fine. This failure should be exceptionally rare anyways.
                    //     // My thinking here is that we can't be sure if we should extend
                    //     // the selection or not so we just don't.
                    //     log.warn("failed to get time, not extending selection err={}", .{err});
                    //     break :extend_selection;
                    // }

                    // const pos = try self.rt_surface.getCursorPos();
                    let pos = unsafe { &*self.rt_surface }.get_cursor_pos()?;
                    // try self.cursorPosCallback(pos, null);
                    self.cursorPosCallback(pos, None)?;
                    // return true;
                    return Ok(true);
                }
            }
        }

        // if (button == .left and action == .release) {
        if button == input_mouse::Button::Left && action == input_mouse::ButtonState::Release {
            // self.renderer_state.mutex.lock();
            // defer self.renderer_state.mutex.unlock();
            //
            // (guard-less zigstd Mutex — the defer is the explicit unlock at
            // every exit below.)
            unsafe { (*(*self.renderer_state.as_ptr()).mutex).lock() };

            // The selection gesture tracks whether a press became a drag by
            // comparing the release cell to the original press cell. Resolve the
            // release position and pin before notifying the gesture so later
            // release handling can query that state.
            // const release_pos: ?apprt.CursorPos = self.rt_surface.getCursorPos() catch |err| pos: {
            //     log.warn("error reading cursor position for mouse release err={}", .{err});
            //     break :pos null;
            // };
            //
            // PARITY: not `.ok()` — the Err arm IS the reference's labeled
            // `catch |err| pos:` block with its log site (the childExited
            // precedent).
            #[allow(clippy::manual_ok_err)]
            let release_pos: Option<apprt::CursorPos> =
                match unsafe { &*self.rt_surface }.get_cursor_pos() {
                    Ok(v) => Some(v),
                    Err(err) => {
                        // log.warn("error reading cursor position for mouse release err={}", .{err});
                        log::warn!(
                            target: LOG_SCOPE,
                            "error reading cursor position for mouse release err={err:?}"
                        );
                        None
                    }
                };

            // If we can't map the release position to a cell, pass null so the
            // gesture can conservatively treat the release as having moved away
            // from the pressed cell.
            // const release_pin: ?terminal.Pin = if (release_pos) |pos| pin: {
            //     const release_vp = self.posToViewport(pos.x, pos.y);
            //     break :pin self.io.terminal.screens.active.pages.pin(.{ .viewport = .{
            //         .x = release_vp.x,
            //         .y = release_vp.y,
            //     } });
            // } else null;
            let release_pin: Option<terminal::pagelist::Pin> = if let Some(pos) = release_pos {
                let release_vp = self.posToViewport(f64::from(pos.x), f64::from(pos.y));
                unsafe {
                    (*(*self.io.as_ptr()).terminal.screens.active).pages.pin(
                        terminal::point::Point::Viewport(terminal::point::Coordinate {
                            x: release_vp.x,
                            y: release_vp.y,
                        }),
                    )
                }
            } else {
                None
            };
            // self.mouse.selection_gesture.release(
            //     self.renderer_state.terminal,
            //     .{ .pin = release_pin },
            // );
            self.mouse.selection_gesture.release(
                unsafe { &mut *(*self.renderer_state.as_ptr()).terminal },
                terminal::selection_gesture::Release { pin: release_pin },
            );

            // Stop selection scrolling when releasing the left mouse button
            // but only when selection scrolling is active.
            // if (self.selection_scroll_active) {
            if self.selection_scroll_active {
                // self.queueIo(
                //     .{ .selection_scroll = false },
                //     .locked,
                // );
                self.queueIo(
                    termio::Message::SelectionScroll(false),
                    termio::termio_impl::MutexState::Locked,
                );
            }

            // The selection clipboard is only updated for left-click drag when
            // the left button is released. This is to avoid the clipboard
            // being updated on every mouse move which would be noisy.
            // if (self.config.copy_on_select != .none) {
            if self.config.copy_on_select != configpkg::value_types::CopyOnSelect::None {
                // const prev_ = self.io.terminal.screens.active.selection;
                let prev_ = unsafe { (*(*self.io.as_ptr()).terminal.screens.active).selection };
                // if (prev_) |prev| {
                if let Some(prev) = prev_ {
                    // try self.setSelectionAndCopy(terminal.Selection.init(
                    //     prev.start(),
                    //     prev.end(),
                    //     prev.rectangle,
                    // ));
                    if let Err(err) =
                        self.setSelectionAndCopy(terminal::selection::Selection::init(
                            prev.start(),
                            prev.end(),
                            prev.rectangle,
                        ))
                    {
                        // (the deferred mutex unlock — the error exit)
                        unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
                        return Err(err);
                    }
                }
            }

            // Handle link clicking. We want to do this before we do mouse
            // reporting or any other mouse handling because a successfully
            // clicked link will swallow the event.
            // if (self.mouse.over_link and !self.mouse.selection_gesture.left_click_dragged) {
            if self.mouse.over_link && !self.mouse.selection_gesture.left_click_dragged {
                // We are holding the renderer lock, but this should just be
                // a cached value.
                // const pos = release_pos orelse try self.rt_surface.getCursorPos();
                let pos = match release_pos {
                    Some(v) => v,
                    None => match unsafe { &*self.rt_surface }.get_cursor_pos() {
                        Ok(v) => v,
                        Err(err) => {
                            // (the deferred mutex unlock — the error exit)
                            unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
                            return Err(err.into());
                        }
                    },
                };
                // if (self.processLinks(pos)) |processed| {
                //     if (processed) return true;
                // } else |err| {
                //     log.warn("error processing links err={}", .{err});
                // }
                match self.processLinks(pos) {
                    Ok(processed) => {
                        if processed {
                            // (the deferred mutex unlock — the consumed return)
                            unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
                            return Ok(true);
                        }
                    }
                    Err(err) => {
                        // log.warn("error processing links err={}", .{err});
                        log::warn!(target: LOG_SCOPE, "error processing links err={err:?}");
                    }
                }
            }

            // Handle prompt clicking. If we released our mouse on a prompt
            // and we support some kind of click events, then we need to
            // move to it.
            // if (self.maybePromptClick()) |handled| {
            //     if (handled) return true;
            // } else |err| {
            //     log.warn("error processing prompt click err={}", .{err});
            // }
            match self.maybePromptClick() {
                Ok(handled) => {
                    if handled {
                        // (the deferred mutex unlock — the handled return)
                        unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
                        return Ok(true);
                    }
                }
                Err(err) => {
                    // log.warn("error processing prompt click err={}", .{err});
                    log::warn!(target: LOG_SCOPE, "error processing prompt click err={err:?}");
                }
            }

            // (the deferred mutex unlock — the block end)
            unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
        }

        // Report mouse events if enabled
        {
            // self.renderer_state.mutex.lock();
            // defer self.renderer_state.mutex.unlock();
            //
            // (guard-less zigstd Mutex — the defer is the explicit unlock at
            // every exit below.)
            unsafe { (*(*self.renderer_state.as_ptr()).mutex).lock() };
            // if (self.isMouseReporting()) report: {
            if self.isMouseReporting() {
                'report: {
                    // If we have shift-pressed and we aren't allowed to capture it,
                    // then we do not do a mouse report.
                    // if (mods.shift and !shift_capture) break :report;
                    if mods.shift && !shift_capture {
                        break 'report;
                    }

                    // In any other mouse button scenario without shift pressed we
                    // clear the selection since the underlying application can handle
                    // that in any way (i.e. "scrolling").
                    // try self.setSelection(null);
                    if let Err(err) = self.setSelection(None) {
                        // (the deferred mutex unlock — the error exit)
                        unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
                        return Err(err);
                    }

                    // We also set the left click count to 0 so that if mouse reporting
                    // is disabled in the middle of press (before release) we don't
                    // suddenly start selecting text.
                    // self.mouse.selection_gesture.reset(self.renderer_state.terminal);
                    self.mouse
                        .selection_gesture
                        .reset(unsafe { &mut *(*self.renderer_state.as_ptr()).terminal });

                    // const pos = try self.rt_surface.getCursorPos();
                    let pos = match unsafe { &*self.rt_surface }.get_cursor_pos() {
                        Ok(v) => v,
                        Err(err) => {
                            // (the deferred mutex unlock — the error exit)
                            unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
                            return Err(err.into());
                        }
                    };

                    // const report_action: input.MouseAction = switch (action) {
                    //     .press => .press,
                    //     .release => .release,
                    // };
                    let report_action: input_mouse::Action = match action {
                        input_mouse::ButtonState::Press => input_mouse::Action::Press,
                        input_mouse::ButtonState::Release => input_mouse::Action::Release,
                    };

                    // self.mouseReport(
                    //     button,
                    //     report_action,
                    //     self.mouse.mods,
                    //     pos,
                    // );
                    self.mouseReport(Some(button), report_action, self.mouse.mods, pos);

                    // If we're doing mouse reporting, we do not support any other
                    // selection or highlighting.
                    // return true;
                    // (the deferred mutex unlock — the reporting return)
                    unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
                    return Ok(true);
                }
            }
            // (the deferred mutex unlock — the block end)
            unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
        }

        // For left button clicks we always record some information for
        // selection/highlighting purposes.
        // if (button == .left and action == .press) click: {
        if button == input_mouse::Button::Left && action == input_mouse::ButtonState::Press {
            'click: {
                // self.renderer_state.mutex.lock();
                // defer self.renderer_state.mutex.unlock();
                //
                // (guard-less zigstd Mutex — the defer is the explicit unlock
                // at every exit below.)
                unsafe { (*(*self.renderer_state.as_ptr()).mutex).lock() };
                // const t: *terminal.Terminal = self.renderer_state.terminal;
                let t: *mut Terminal = unsafe { (*self.renderer_state.as_ptr()).terminal };
                // const screen: *terminal.Screen = self.renderer_state.terminal.screens.active;
                let screen: *mut terminal::screen::Screen =
                    unsafe { (*(*self.renderer_state.as_ptr()).terminal).screens.active };

                // const pos = try self.rt_surface.getCursorPos();
                let pos = match unsafe { &*self.rt_surface }.get_cursor_pos() {
                    Ok(v) => v,
                    Err(err) => {
                        // (the deferred mutex unlock — the error exit)
                        unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
                        return Err(err.into());
                    }
                };
                // const pin = pin: {
                let pin = 'pin: {
                    // const pt_viewport = self.posToViewport(pos.x, pos.y);
                    let pt_viewport = self.posToViewport(f64::from(pos.x), f64::from(pos.y));
                    // const pin = screen.pages.pin(.{
                    //     .viewport = .{
                    //         .x = pt_viewport.x,
                    //         .y = pt_viewport.y,
                    //     },
                    // }) orelse {
                    let pin = match unsafe {
                        (*screen).pages.pin(terminal::point::Point::Viewport(
                            terminal::point::Coordinate {
                                x: pt_viewport.x,
                                y: pt_viewport.y,
                            },
                        ))
                    } {
                        Some(v) => v,
                        None => {
                            // Weird... our viewport x/y that we just converted isn't
                            // found in our pages. This is probably a bug but we don't
                            // want to crash in releases because its harmless. So, we
                            // only assert in debug mode.
                            // if (comptime std.debug.runtime_safety) unreachable;
                            if cfg!(debug_assertions) {
                                unreachable!();
                            }
                            // break :click;
                            // (the deferred mutex unlock — the break exit)
                            unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
                            break 'click;
                        }
                    };

                    // break :pin pin;
                    break 'pin pin;
                };

                // const time = std.time.Instant.now() catch |err| time: {
                //     log.err("error reading time, mouse multi-click won't work err={}", .{err});
                //     break :time null;
                // };
                //
                // (Rust `Instant::now` is infallible — the catch arm
                // collapses.)
                let time = Some(std::time::Instant::now());
                // var press_selection = try self.mouse.selection_gesture.press(t, .{
                //     .time = time,
                //     .pin = pin,
                //     .xpos = pos.x,
                //     .ypos = pos.y,
                //     .max_distance = @floatFromInt(self.size.cell.width),
                //     .repeat_interval = self.config.mouse_interval,
                //     .word_boundary_codepoints = self.config.selection_word_chars,
                //     .behaviors = &.{
                //         .cell,
                //         .word,
                //         if (mods.ctrlOrSuper()) .output else .line,
                //     },
                // });
                let mut press_selection = match self.mouse.selection_gesture.press(
                    unsafe { &mut *t },
                    terminal::selection_gesture::Press {
                        time,
                        pin,
                        xpos: f64::from(pos.x),
                        ypos: f64::from(pos.y),
                        max_distance: f64::from(self.size.cell.width),
                        repeat_interval: self.config.mouse_interval,
                        word_boundary_codepoints: &self.config.selection_word_chars,
                        behaviors: &[
                            terminal::selection_gesture::Behavior::Cell,
                            terminal::selection_gesture::Behavior::Word,
                            if mods.ctrl_or_super() {
                                terminal::selection_gesture::Behavior::Output
                            } else {
                                terminal::selection_gesture::Behavior::Line
                            },
                        ],
                    },
                ) {
                    Ok(v) => v,
                    Err(err) => {
                        // (the deferred mutex unlock — the error exit)
                        unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
                        return Err(err.into());
                    }
                };

                // The gesture owns the standard single/double/triple-click selection
                // behavior. Surface keeps terminal-surface-specific overrides here.
                // switch (self.mouse.selection_gesture.left_click_count) {
                match self.mouse.selection_gesture.left_click_count {
                    // 1 => {},
                    1 => {}

                    // Double click on a URL selects the entire URL instead of the
                    // standard word selection returned by the gesture.
                    // 2 => {
                    2 => {
                        // Try link detection without requiring modifier keys.
                        // if (self.linkAtPin(
                        //     pin,
                        //     null,
                        // )) |result_| {
                        match self.linkAtPin(pin, None) {
                            Ok(result_) => {
                                // if (result_) |result| {
                                if let Some(result) = result_ {
                                    // press_selection = result.selection;
                                    press_selection = Some(result.selection);
                                }
                            }
                            // } else |_| {
                            Err(_) => {
                                // Ignore any errors, likely regex errors.
                            }
                        }
                    }

                    // 3 => {},
                    3 => {}

                    // We should be bounded by 1 to 3
                    // else => unreachable,
                    _ => unreachable!(),
                }

                // Use `setSelection` (not `setSelectionAndCopy`) here to avoid
                // touching the selection clipboard: for left mouse clicks we only
                // copy on release.
                // if (press_selection) |selection| {
                if let Some(selection) = press_selection {
                    // try self.setSelection(selection);
                    if let Err(err) = self.setSelection(Some(selection)) {
                        // (the deferred mutex unlock — the error exit)
                        unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
                        return Err(err);
                    }
                    // try self.queueRender();
                    if let Err(err) = self.queueRender() {
                        // (the deferred mutex unlock — the error exit)
                        unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
                        return Err(err.into());
                    }
                // } else if (self.mouse.selection_gesture.left_click_count == 1 and
                //     self.io.terminal.screens.active.selection != null)
                // {
                } else if self.mouse.selection_gesture.left_click_count == 1
                    && unsafe { (*(*self.io.as_ptr()).terminal.screens.active).selection }.is_some()
                {
                    // try self.setSelection(null);
                    if let Err(err) = self.setSelection(None) {
                        // (the deferred mutex unlock — the error exit)
                        unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
                        return Err(err);
                    }
                    // try self.queueRender();
                    if let Err(err) = self.queueRender() {
                        // (the deferred mutex unlock — the error exit)
                        unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
                        return Err(err.into());
                    }
                }

                // (the deferred mutex unlock — the block end)
                unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
            }
        }

        // Middle-click action, either ignore, or paste from clipboard or paste from the selection clipboard if supported.
        // if (button == .middle and action == .press) switch (self.config.middle_click_action) {
        if button == input_mouse::Button::Middle && action == input_mouse::ButtonState::Press {
            match self.config.middle_click_action {
                // .ignore => {},
                configpkg::value_types::MiddleClickAction::Ignore => {}
                // .@"clipboard-paste" => {
                //     _ = try self.startClipboardRequest(.standard, .{ .paste = .standard });
                // },
                configpkg::value_types::MiddleClickAction::ClipboardPaste => {
                    let _ = self.startClipboardRequest(
                        apprt::Clipboard::Standard,
                        apprt::ClipboardRequest::Paste(apprt::Clipboard::Standard),
                    )?;
                }
                // .@"primary-paste" => {
                //     if (self.rt_surface.supportsClipboard(.selection)) {
                //         _ = try self.startClipboardRequest(.selection, .{ .paste = .selection });
                //     }
                // },
                configpkg::value_types::MiddleClickAction::PrimaryPaste => {
                    if unsafe { &*self.rt_surface }.supports_clipboard(apprt::Clipboard::Selection)
                    {
                        let _ = self.startClipboardRequest(
                            apprt::Clipboard::Selection,
                            apprt::ClipboardRequest::Paste(apprt::Clipboard::Selection),
                        )?;
                    }
                }
            }
        }

        // Right-click down selects word for context menus. If the apprt
        // doesn't implement context menus this can be a bit weird but they
        // are supported by our two main apprts so we always do this. If we
        // want to be careful in the future we can add a function to apprts
        // that let's us know.
        // if (button == .right and action == .press) sel: {
        if button == input_mouse::Button::Right && action == input_mouse::ButtonState::Press {
            'sel: {
                // self.renderer_state.mutex.lock();
                // defer self.renderer_state.mutex.unlock();
                //
                // (guard-less zigstd Mutex — the defer is the explicit unlock
                // at every exit below.)
                unsafe { (*(*self.renderer_state.as_ptr()).mutex).lock() };

                // Get our viewport pin
                // const screen: *terminal.Screen = self.renderer_state.terminal.screens.active;
                let screen: *mut terminal::screen::Screen =
                    unsafe { (*(*self.renderer_state.as_ptr()).terminal).screens.active };
                // const pos = try self.rt_surface.getCursorPos();
                let pos = match unsafe { &*self.rt_surface }.get_cursor_pos() {
                    Ok(v) => v,
                    Err(err) => {
                        // (the deferred mutex unlock — the error exit)
                        unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
                        return Err(err.into());
                    }
                };
                // const pin = pin: {
                let pin = 'pin: {
                    // const pt_viewport = self.posToViewport(pos.x, pos.y);
                    let pt_viewport = self.posToViewport(f64::from(pos.x), f64::from(pos.y));
                    // const pin = screen.pages.pin(.{
                    //     .viewport = .{
                    //         .x = pt_viewport.x,
                    //         .y = pt_viewport.y,
                    //     },
                    // }) orelse {
                    let pin = match unsafe {
                        (*screen).pages.pin(terminal::point::Point::Viewport(
                            terminal::point::Coordinate {
                                x: pt_viewport.x,
                                y: pt_viewport.y,
                            },
                        ))
                    } {
                        Some(v) => v,
                        None => {
                            // if (comptime std.debug.runtime_safety) unreachable;
                            if cfg!(debug_assertions) {
                                unreachable!();
                            }
                            // break :sel;
                            // (the deferred mutex unlock — the break exit)
                            unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
                            break 'sel;
                        }
                    };

                    // break :pin pin;
                    break 'pin pin;
                };

                // switch (self.config.right_click_action) {
                match self.config.right_click_action {
                    // .ignore => {},
                    configpkg::value_types::RightClickAction::Ignore => {}
                    // .@"context-menu" => {
                    configpkg::value_types::RightClickAction::ContextMenu => {
                        // If we already have a selection and the selection contains
                        // where we clicked then we don't want to modify the selection.
                        // if (self.io.terminal.screens.active.selection) |prev_sel| {
                        if let Some(prev_sel) =
                            unsafe { (*(*self.io.as_ptr()).terminal.screens.active).selection }
                        {
                            // if (prev_sel.contains(screen, pin)) break :sel;
                            if prev_sel.contains(unsafe { &*screen }, pin) {
                                // (the deferred mutex unlock — the break exit)
                                unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
                                break 'sel;
                            }

                            // The selection doesn't contain our pin, so we create a new
                            // word selection where we clicked.
                        }

                        // If there is a link at this position, we want to
                        // select the link. Otherwise, select the word.
                        // if (try self.linkAtPos(pos)) |link| {
                        let link_ = match self.linkAtPos(pos) {
                            Ok(v) => v,
                            Err(err) => {
                                // (the deferred mutex unlock — the error exit)
                                unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
                                return Err(err);
                            }
                        };
                        if let Some(link) = link_ {
                            // try self.setSelectionAndCopy(link.selection);
                            if let Err(err) = self.setSelectionAndCopy(link.selection) {
                                // (the deferred mutex unlock — the error exit)
                                unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
                                return Err(err);
                            }
                        } else {
                            // const sel = screen.selectWord(
                            //     pin,
                            //     self.config.selection_word_chars,
                            // ) orelse break :sel;
                            let sel = match unsafe { &*screen }
                                .select_word(pin, &self.config.selection_word_chars)
                            {
                                Some(v) => v,
                                None => {
                                    // (the deferred mutex unlock — the break
                                    // exit)
                                    unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
                                    break 'sel;
                                }
                            };
                            // try self.setSelectionAndCopy(sel);
                            if let Err(err) = self.setSelectionAndCopy(sel) {
                                // (the deferred mutex unlock — the error exit)
                                unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
                                return Err(err);
                            }
                        }
                        // try self.queueRender();
                        if let Err(err) = self.queueRender() {
                            // (the deferred mutex unlock — the error exit)
                            unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
                            return Err(err.into());
                        }

                        // Don't consume so that we show the context menu in apprt.
                        // return false;
                        // (the deferred mutex unlock — the context-menu return)
                        unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
                        return Ok(false);
                    }
                    // .copy => {
                    configpkg::value_types::RightClickAction::Copy => {
                        // if (self.io.terminal.screens.active.selection) |sel| {
                        if let Some(sel) =
                            unsafe { (*(*self.io.as_ptr()).terminal.screens.active).selection }
                        {
                            // try self.copySelectionToClipboards(
                            //     sel,
                            //     &.{.standard},
                            //     .mixed,
                            // );
                            if let Err(err) = self.copySelectionToClipboards(
                                sel,
                                &[apprt::Clipboard::Standard],
                                binding::CopyToClipboard::Mixed,
                            ) {
                                // (the deferred mutex unlock — the error exit)
                                unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
                                return Err(err);
                            }
                        }

                        // try self.setSelection(null);
                        if let Err(err) = self.setSelection(None) {
                            // (the deferred mutex unlock — the error exit)
                            unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
                            return Err(err);
                        }
                        // try self.queueRender();
                        if let Err(err) = self.queueRender() {
                            // (the deferred mutex unlock — the error exit)
                            unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
                            return Err(err.into());
                        }
                    }
                    // .@"copy-or-paste" => if (self.io.terminal.screens.active.selection) |sel| {
                    configpkg::value_types::RightClickAction::CopyOrPaste => {
                        if let Some(sel) =
                            unsafe { (*(*self.io.as_ptr()).terminal.screens.active).selection }
                        {
                            // try self.copySelectionToClipboards(
                            //     sel,
                            //     &.{.standard},
                            //     .mixed,
                            // );
                            if let Err(err) = self.copySelectionToClipboards(
                                sel,
                                &[apprt::Clipboard::Standard],
                                binding::CopyToClipboard::Mixed,
                            ) {
                                // (the deferred mutex unlock — the error exit)
                                unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
                                return Err(err);
                            }
                            // try self.setSelection(null);
                            if let Err(err) = self.setSelection(None) {
                                // (the deferred mutex unlock — the error exit)
                                unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
                                return Err(err);
                            }
                            // try self.queueRender();
                            if let Err(err) = self.queueRender() {
                                // (the deferred mutex unlock — the error exit)
                                unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
                                return Err(err.into());
                            }
                        } else {
                            // Pasting can trigger a lock grab in complete clipboard
                            // request so we need to unlock.
                            // self.renderer_state.mutex.unlock();
                            unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
                            // defer self.renderer_state.mutex.lock();
                            //
                            // (the inverted defer — the explicit relock at
                            // every exit of this arm below.)
                            // _ = try self.startClipboardRequest(.standard, .{ .paste = .standard });
                            match self.startClipboardRequest(
                                apprt::Clipboard::Standard,
                                apprt::ClipboardRequest::Paste(apprt::Clipboard::Standard),
                            ) {
                                Ok(_) => {}
                                Err(err) => {
                                    // (the LIFO defers on the error exit —
                                    // the arm's relock, then the outer
                                    // unlock)
                                    unsafe { (*(*self.renderer_state.as_ptr()).mutex).lock() };
                                    unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
                                    return Err(err);
                                }
                            }
                            // (the deferred relock — the arm end)
                            unsafe { (*(*self.renderer_state.as_ptr()).mutex).lock() };

                            // We don't need to clear selection because we didn't have
                            // one to begin with.
                        }
                    }
                    // .paste => {
                    configpkg::value_types::RightClickAction::Paste => {
                        // Before we yield the lock, clear our selection if we have
                        // one.
                        // try self.setSelection(null);
                        if let Err(err) = self.setSelection(None) {
                            // (the deferred mutex unlock — the error exit)
                            unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
                            return Err(err);
                        }
                        // try self.queueRender();
                        if let Err(err) = self.queueRender() {
                            // (the deferred mutex unlock — the error exit)
                            unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
                            return Err(err.into());
                        }

                        // Pasting can trigger a lock grab in complete clipboard
                        // request so we need to unlock.
                        // self.renderer_state.mutex.unlock();
                        unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
                        // defer self.renderer_state.mutex.lock();
                        //
                        // (the inverted defer — the explicit relock at every
                        // exit of this arm below.)
                        // _ = try self.startClipboardRequest(.standard, .{ .paste = .standard });
                        match self.startClipboardRequest(
                            apprt::Clipboard::Standard,
                            apprt::ClipboardRequest::Paste(apprt::Clipboard::Standard),
                        ) {
                            Ok(_) => {}
                            Err(err) => {
                                // (the LIFO defers on the error exit — the
                                // arm's relock, then the outer unlock)
                                unsafe { (*(*self.renderer_state.as_ptr()).mutex).lock() };
                                unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
                                return Err(err);
                            }
                        }
                        // (the deferred relock — the arm end)
                        unsafe { (*(*self.renderer_state.as_ptr()).mutex).lock() };
                    }
                }

                // Consume the event such that the context menu is not displayed.
                // return true;
                // (the deferred mutex unlock — the consumed return)
                unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
                return Ok(true);
            }
        }

        // return false;
        Ok(false)
    }

    /// Requires the renderer state mutex is held.
    // fn maybePromptClick(self: *Surface) !bool {
    //
    // (the inferred `!bool` is the [`Error`] union.)
    fn maybePromptClick(&mut self) -> Result<bool, Error> {
        // const t: *terminal.Terminal = self.renderer_state.terminal;
        let t: *mut Terminal = unsafe { (*self.renderer_state.as_ptr()).terminal };
        // const screen: *terminal.Screen = t.screens.active;
        let screen: *mut terminal::screen::Screen = unsafe { (*t).screens.active };

        // If our screen doesn't handle any prompt clicks, then we never
        // do anything.
        // if (screen.semantic_prompt.click == .none) return false;
        if unsafe { (*screen).semantic_prompt.click } == terminal::screen::SemanticClick::None {
            return Ok(false);
        }

        // If cursor-click-to-move is disabled, we don't do any prompt clicking.
        // if (!self.config.cursor_click_to_move) return false;
        if !self.config.cursor_click_to_move {
            return Ok(false);
        }

        // If our cursor isn't currently at a prompt then we don't handle
        // prompt clicks because we can't move if we're not in a prompt!
        // if (!t.cursorIsAtPrompt()) return false;
        if !unsafe { (*t).cursor_is_at_prompt() } {
            return Ok(false);
        }

        // If the left click moved away from its pressed cell then releasing the
        // mouse completes the drag gesture and we don't do prompt moving.
        // if (self.mouse.selection_gesture.left_click_dragged) return false;
        if self.mouse.selection_gesture.left_click_dragged {
            return Ok(false);
        }

        // If we have a selection currently, then releasing the mouse completes
        // the selection and we don't do prompt moving.
        // if (screen.selection != null) return false;
        if unsafe { (*screen).selection }.is_some() {
            return Ok(false);
        }

        // Get the pin for our mouse click.
        // const pos = try self.rt_surface.getCursorPos();
        let pos = unsafe { &*self.rt_surface }.get_cursor_pos()?;
        // const pos_vp = self.posToViewport(pos.x, pos.y);
        let pos_vp = self.posToViewport(f64::from(pos.x), f64::from(pos.y));
        // const click_pin: terminal.Pin = pin: {
        let click_pin: terminal::pagelist::Pin = 'pin: {
            // const pin = screen.pages.pin(.{
            //     .viewport = .{
            //         .x = pos_vp.x,
            //         .y = pos_vp.y,
            //     },
            // }) orelse {
            let pin = match unsafe {
                (*screen).pages.pin(terminal::point::Point::Viewport(
                    terminal::point::Coordinate {
                        x: pos_vp.x,
                        y: pos_vp.y,
                    },
                ))
            } {
                Some(v) => v,
                None => {
                    // See mouseButtonCallback for explanation
                    // if (comptime std.debug.runtime_safety) unreachable;
                    if cfg!(debug_assertions) {
                        unreachable!();
                    }
                    // return false;
                    return Ok(false);
                }
            };

            // break :pin pin;
            break 'pin pin;
        };

        // Get our cursor's most current prompt.
        // const prompt_pin: terminal.Pin = prompt_pin: {
        let prompt_pin: terminal::pagelist::Pin = 'prompt_pin: {
            // var it = screen.cursor.page_pin.promptIterator(
            //     .left_up,
            //     null,
            // );
            let mut it = unsafe {
                (*(*screen).cursor.page_pin)
                    .prompt_iterator(terminal::pagelist::iter::Direction::LeftUp, None)
            };
            // break :prompt_pin it.next() orelse {
            match it.next() {
                Some(v) => break 'prompt_pin v,
                None => {
                    // This shouldn't be possible because we asserted we're at
                    // a prompt above, so we MUST find some prompt in a left_up search.
                    // log.warn("cursor is at prompt but no prompt found", .{});
                    log::warn!(target: LOG_SCOPE, "cursor is at prompt but no prompt found");
                    // if (comptime std.debug.runtime_safety) unreachable;
                    if cfg!(debug_assertions) {
                        unreachable!();
                    }
                    // return false;
                    return Ok(false);
                }
            }
        };

        // If our mouse click is before the prompt, we don't move.
        // We DO ALLOW clicks AFTER the prompt, specifically with Kitty's
        // click_events=1 since we rely on the shell to validate out of
        // bounds clicks. This matches Kitty's logic as best I can tell.
        // if (click_pin.before(prompt_pin)) return false;
        if click_pin.before(prompt_pin) {
            return Ok(false);
        }

        // At this point we've established:
        // - Screen supports prompt clicks
        // - Cursor is at a prompt
        // - Click is at or below our prompt
        // switch (screen.semantic_prompt.click) {
        match unsafe { (*screen).semantic_prompt.click } {
            // Guarded at the start of this function
            // .none => unreachable,
            terminal::screen::SemanticClick::None => unreachable!(),

            // .click_events => |v| {
            terminal::screen::SemanticClick::ClickEvents(v) => {
                // For the event, we always send a left-click press event.
                // This matches what Kitty sends.
                // const key: u8, const y: u32 = switch (v) {
                //     .absolute => .{ 1, pos_vp.y +| 1 },
                //     .relative => .{ 2, pos_vp.y -| prompt_pin.y +| 1 },
                // };
                let (key, y): (u8, u32) = match v {
                    terminal::osc::semantic_prompt::ClickEvents::Absolute => {
                        (1, pos_vp.y.saturating_add(1))
                    }
                    terminal::osc::semantic_prompt::ClickEvents::Relative => (
                        2,
                        pos_vp
                            .y
                            .saturating_sub(u32::from(prompt_pin.y))
                            .saturating_add(1),
                    ),
                };
                // var data: termio.Message.WriteReq.Small.Array = undefined;
                // const resp = try std.fmt.bufPrint(
                //     &data,
                //     "\x1B[<0;{d};{d}M",
                //     .{ pos_vp.x + 1, y },
                // );
                //
                // PORT NOTE: the fixed Cursor over the Small array is
                // `bufPrint` (the encodeKey precedent); its buffer-full error
                // is io's `WriteZero` — the reference's `error.NoSpaceLeft`,
                // `try`-propagated ([`Error::Io`]).
                let mut data = [0u8; termio::message::WriteReq::MAX];
                let resp_len = {
                    use std::io::Write as _;
                    let mut writer = std::io::Cursor::new(&mut data[..]);
                    write!(writer, "\x1B[<0;{};{}M", pos_vp.x + 1, y)?;
                    writer.position() as usize
                };

                // Not that noisy since this only happens on prompt clicks.
                // log.debug(
                //     "sending click_events={} event=ESC{s}",
                //     .{ key, resp[1..] },
                // );
                log::debug!(
                    target: LOG_SCOPE,
                    "sending click_events={} event=ESC{}",
                    key,
                    String::from_utf8_lossy(&data[1..resp_len])
                );

                // Ask our IO thread to write the data
                // self.queueIo(.{ .write_small = .{
                //     .data = data,
                //     .len = @intCast(resp.len),
                // } }, .locked);
                self.queueIo(
                    termio::Message::WriteSmall(Small {
                        data,
                        len: resp_len,
                    }),
                    termio::termio_impl::MutexState::Locked,
                );
            }

            // .cl => {
            terminal::screen::SemanticClick::Cl(_) => {
                // const left_arrow = if (t.modes.get(.cursor_keys)) "\x1bOD" else "\x1b[D";
                let left_arrow: &'static [u8] =
                    if unsafe { (*t).modes.get(terminal::modes::Mode::CursorKeys) } {
                        b"\x1bOD"
                    } else {
                        b"\x1b[D"
                    };
                // const right_arrow = if (t.modes.get(.cursor_keys)) "\x1bOC" else "\x1b[C";
                let right_arrow: &'static [u8] =
                    if unsafe { (*t).modes.get(terminal::modes::Mode::CursorKeys) } {
                        b"\x1bOC"
                    } else {
                        b"\x1b[C"
                    };

                // const move = screen.promptClickMove(click_pin);
                //
                // (`move` is a Rust keyword — the raw identifier keeps the
                // reference name.)
                let r#move = unsafe { (*screen).prompt_click_move(click_pin) };
                // for (0..move.left) |_| {
                for _ in 0..r#move.left {
                    // self.queueIo(
                    //     .{ .write_stable = left_arrow },
                    //     .locked,
                    // );
                    self.queueIo(
                        termio::Message::WriteStable(left_arrow),
                        termio::termio_impl::MutexState::Locked,
                    );
                }
                // for (0..move.right) |_| {
                for _ in 0..r#move.right {
                    // self.queueIo(
                    //     .{ .write_stable = right_arrow },
                    //     .locked,
                    // );
                    self.queueIo(
                        termio::Message::WriteStable(right_arrow),
                        termio::termio_impl::MutexState::Locked,
                    );
                }
            }
        }

        // return true;
        Ok(true)
    }

    // (the file-scope `const Link` the reference declares HERE, Surface.zig:
    // 4271, lives in this file's trailing type cluster — see [`Link`].)

    /// Returns the link at the given cursor position, if any.
    ///
    /// Requires the renderer mutex is held.
    // fn linkAtPos(
    //     self: *Surface,
    //     pos: apprt.CursorPos,
    // ) !?Link {
    //
    // (the inferred `!?Link` is the [`Error`] union.)
    pub(super) fn linkAtPos(&mut self, pos: apprt::CursorPos) -> Result<Option<Link>, Error> {
        // Convert our cursor position to a screen point.
        // const screen: *terminal.Screen = self.renderer_state.terminal.screens.active;
        let screen: *mut terminal::screen::Screen =
            unsafe { (*(*self.renderer_state.as_ptr()).terminal).screens.active };
        // const mouse_pin: terminal.Pin = mouse_pin: {
        let mouse_pin: terminal::pagelist::Pin = 'mouse_pin: {
            // const point = self.posToViewport(pos.x, pos.y);
            //
            // (Zig widens the f32 cursor coordinates to the f64 params.)
            let point = self.posToViewport(f64::from(pos.x), f64::from(pos.y));
            // const pin = screen.pages.pin(.{ .viewport = point }) orelse {
            //     log.warn("failed to get pin for clicked point", .{});
            //     return null;
            // };
            let pin = match unsafe { (*screen).pages.pin(terminal::point::Point::Viewport(point)) }
            {
                Some(v) => v,
                None => {
                    // log.warn("failed to get pin for clicked point", .{});
                    log::warn!(target: LOG_SCOPE, "failed to get pin for clicked point");
                    return Ok(None);
                }
            };
            // break :mouse_pin pin;
            break 'mouse_pin pin;
        };

        // Get our comparison mods
        // const mouse_mods = self.mouseModsWithCapture(self.mouse.mods);
        let mouse_mods = self.mouseModsWithCapture(self.mouse.mods);

        // If we have the proper modifiers set then we can check for OSC8 links.
        // if (self.config.link_osc8 and
        //     mouse_mods.equal(input.ctrlOrSuper(.{})))
        // hyperlink: {
        if self.config.link_osc8 && mouse_mods.equal(key::ctrl_or_super(Mods::default())) {
            'hyperlink: {
                // const rac = mouse_pin.rowAndCell();
                let rac = mouse_pin.row_and_cell();
                // const cell = rac.cell;
                let cell = rac.cell;
                // if (!cell.hyperlink) break :hyperlink;
                if !unsafe { (*cell).hyperlink() } {
                    break 'hyperlink;
                }
                // const sel = terminal.Selection.init(mouse_pin, mouse_pin, false);
                let sel = terminal::selection::Selection::init(mouse_pin, mouse_pin, false);
                // return .{ .action = ._open_osc8, .selection = sel };
                return Ok(Some(Link {
                    action: link::Action::OpenOsc8,
                    selection: sel,
                }));
            }
        }

        // Fall back to configured links
        // return try self.linkAtPin(mouse_pin, mouse_mods);
        //
        // (the non-optional `mouse_mods` coerces to the `?input.Mods` param.)
        self.linkAtPin(mouse_pin, Some(mouse_mods))
    }

    /// Detects if a link is present at the given pin.
    ///
    /// If mouse mods is null then mouse mod requirements are ignored (all
    /// configured links are checked).
    ///
    /// Requires the renderer state mutex is held.
    // fn linkAtPin(
    //     self: *Surface,
    //     mouse_pin: terminal.Pin,
    //     mouse_mods: ?input.Mods,
    // ) !?Link {
    //
    // (the inferred `!?Link` is the [`Error`] union — the `it.next()` regex
    // search is its `Oni` growth point.)
    fn linkAtPin(
        &mut self,
        mouse_pin: terminal::pagelist::Pin,
        mouse_mods: Option<Mods>,
    ) -> Result<Option<Link>, Error> {
        // if (self.config.links.len == 0) return null;
        if self.config.links.is_empty() {
            return Ok(None);
        }

        // const screen: *terminal.Screen = self.renderer_state.terminal.screens.active;
        let screen: *mut terminal::screen::Screen =
            unsafe { (*(*self.renderer_state.as_ptr()).terminal).screens.active };
        // const line = screen.selectLine(.{
        //     .pin = mouse_pin,
        //     .whitespace = null,
        //     // Respect semantic prompt boundaries so link/path matching doesn't
        //     // merge shell prompt content with the text beside it.
        //     .semantic_prompt_boundary = true,
        // }) orelse return null;
        let line = match unsafe { &*screen }.select_line(terminal::screen::SelectLine {
            pin: mouse_pin,
            whitespace: None,
            // Respect semantic prompt boundaries so link/path matching doesn't
            // merge shell prompt content with the text beside it.
            semantic_prompt_boundary: true,
        }) {
            Some(v) => v,
            None => return Ok(None),
        };

        // const strmap = try screen.selectionStringMap(self.alloc, .{
        //     .sel = line,
        //     .trim = false,
        // });
        let mut strmap: terminal::string_map::StringMap = unsafe { &mut *screen }
            .selection_string_map(
                self.alloc.into(),
                terminal::screen::SelectionString {
                    sel: line,
                    trim: false,
                },
            )?;
        // defer strmap.deinit(self.alloc);
        //
        // PORT NOTE: the owned map frees on drop at every exit below — either
        // as the local itself or moved into the live iterator (see the
        // searchIterator note in the loop).

        // for (self.config.links) |link| {
        for link in &self.config.links {
            // Skip highlight/mods check when mouse_mods is null (double-click mode)
            // if (mouse_mods) |mods| switch (link.highlight) {
            //     .always, .hover => {},
            //     .always_mods, .hover_mods => |v| if (!v.equal(mods)) continue,
            // };
            if let Some(mods) = mouse_mods {
                match link.highlight {
                    link::Highlight::Always | link::Highlight::Hover => {}
                    link::Highlight::AlwaysMods(v) | link::Highlight::HoverMods(v) => {
                        if !v.equal(mods) {
                            continue;
                        }
                    }
                }
            }

            // var it = strmap.searchIterator(link.regex);
            //
            // PORT NOTE (Zig value-copies): the reference `searchIterator`
            // copies BOTH the `StringMap` slice headers and the `oni.Regex`
            // handle by value — shallow, non-owning copies aliasing the same
            // backing data. The vt `search_iterator` takes both by value, so
            // the regex copy is the aliasing handle struct (`link.regex`
            // stays owned by `DerivedConfig`; only its explicit deinit chain
            // frees the OnigRegex — the struct has no `Drop`) and the map
            // moves through the iterator, recovered after each link's scan.
            let mut it = strmap.search_iterator(oni::Regex {
                value: link.regex.value,
            });
            // while (true) {
            //
            // PARITY: clippy's `while_let_loop` fold would hide the
            // reference's explicit `while (true)` + `orelse break` control
            // flow (the vt string_map.rs precedent, verbatim).
            #[allow(clippy::while_let_loop)]
            loop {
                // var match = (try it.next()) orelse break;
                let mut match_ = match it.next()? {
                    Some(v) => v,
                    None => break,
                };
                // defer match.deinit();
                //
                // (explicit at every exit of this iteration below.)
                // const sel = match.selection();
                let sel = match_.selection();
                // if (!sel.contains(screen, mouse_pin)) continue;
                if !sel.contains(unsafe { &*screen }, mouse_pin) {
                    // (the deferred match.deinit — the continue)
                    match_.deinit();
                    continue;
                }
                // return .{
                //     .action = link.action,
                //     .selection = sel,
                // };
                //
                // (the deferred match.deinit — the return)
                match_.deinit();
                return Ok(Some(Link {
                    action: link.action,
                    selection: sel,
                }));
            }
            // (the map moves back out of the finished iterator — the
            // searchIterator PORT NOTE above.)
            strmap = it.map;
        }

        // return null;
        Ok(None)
    }

    /// This returns the mouse mods to consider for link highlighting or
    /// other purposes taking into account when shift is pressed for releasing
    /// the mouse from capture.
    ///
    /// The renderer state mutex must be held.
    // fn mouseModsWithCapture(self: *Surface, mods: input.Mods) input.Mods {
    pub(super) fn mouseModsWithCapture(&mut self, mods: Mods) -> Mods {
        // In any of these scenarios, whatever mods are set (even shift)
        // are preserved.
        // if (self.io.terminal.flags.mouse_event == .none) return mods;
        if unsafe { (*self.io.as_ptr()).terminal.flags.mouse_event } == terminal::mouse::Event::None
        {
            return mods;
        }
        // if (!mods.shift) return mods;
        if !mods.shift {
            return mods;
        }
        // if (self.mouseShiftCapture(false)) return mods;
        if self.mouseShiftCapture(false) {
            return mods;
        }

        // We have mouse capture, shift set, and we're not allowed to capture
        // shift, so we can clear shift.
        // var final = mods;
        //
        // (`final` is a Rust reserved keyword — the raw identifier keeps the
        // reference name, the `r#move` precedent.)
        let mut r#final = mods;
        // final.shift = false;
        r#final.shift = false;
        // return final;
        r#final
    }

    /// Attempt to invoke the action of any link that is under the
    /// given position.
    ///
    /// Requires the renderer state mutex is held.
    // fn processLinks(self: *Surface, pos: apprt.CursorPos) !bool {
    fn processLinks(&mut self, pos: apprt::CursorPos) -> Result<bool, Error> {
        // const link = try self.linkAtPos(pos) orelse return false;
        let link = match self.linkAtPos(pos)? {
            Some(v) => v,
            None => return Ok(false),
        };
        // switch (link.action) {
        match link.action {
            // .open => {
            link::Action::Open => {
                // const str = try self.io.terminal.screens.active.selectionString(self.alloc, .{
                //     .sel = link.selection,
                //     .trim = false,
                // });
                let str_ = unsafe { &mut *(*self.io.as_ptr()).terminal.screens.active }
                    .selection_string(
                        self.alloc.into(),
                        terminal::screen::SelectionString {
                            sel: link.selection,
                            trim: false,
                        },
                    )?;
                // defer self.alloc.free(str);
                //
                // (owned bytes — freed on drop at every exit.)

                // const resolved_path = try self.resolvePathForOpening(str);
                let resolved_path = self.resolvePathForOpening(&str_);
                // defer if (resolved_path) |p| self.alloc.free(p);
                //
                // (owned bytes — freed on drop at every exit.)

                // const url_to_open = resolved_path orelse str;
                let url_to_open: &[u8] = match &resolved_path {
                    Some(p) => p,
                    None => &str_,
                };
                // try self.openUrl(.{ .kind = .unknown, .url = url_to_open });
                self.openUrl(OpenUrl {
                    kind: OpenUrlKind::Unknown,
                    url: url_to_open,
                })?;
            }

            // ._open_osc8 => {
            link::Action::OpenOsc8 => {
                // const uri = self.osc8URI(link.selection.start()) orelse {
                //     log.warn("failed to get URI for OSC8 hyperlink", .{});
                //     return false;
                // };
                let uri = match self.osc8URI(link.selection.start()) {
                    Some(v) => v,
                    None => {
                        // log.warn("failed to get URI for OSC8 hyperlink", .{});
                        log::warn!(target: LOG_SCOPE, "failed to get URI for OSC8 hyperlink");
                        return Ok(false);
                    }
                };
                // try self.openUrl(.{ .kind = .osc8, .url = uri });
                self.openUrl(OpenUrl {
                    kind: OpenUrlKind::Osc8,
                    url: uri,
                })?;
            }
        }

        // return true;
        Ok(true)
    }

    // fn openUrl(
    //     self: *Surface,
    //     action: apprt.action.OpenUrl,
    // ) !void {
    //
    // (the inferred `!void` is the [`Error`] union — the fallback opener is
    // its `OsOpen` growth point.)
    pub(super) fn openUrl(&mut self, action: OpenUrl<'_>) -> Result<(), Error> {
        // If the apprt handles it then we're done.
        // if (try self.rt_app.performAction(
        //     .{ .surface = self },
        //     .open_url,
        //     action,
        // )) return;
        if unsafe { &*self.rt_app }.perform_action(
            apprt::Target::surface_ref(&*self),
            apprt::Action::OpenUrl(action),
        )? {
            return Ok(());
        }

        // apprt didn't handle it, fallback to our simple cross-platform
        // URL opener. We log a warning because we want well-behaved
        // apprts to handle this themselves.
        // log.warn("apprt did not handle open URL action, falling back to default opener", .{});
        log::warn!(
            target: LOG_SCOPE,
            "apprt did not handle open URL action, falling back to default opener"
        );
        // try internal_os.open(
        //     self.alloc,
        //     action.kind,
        //     action.url,
        // );
        //
        // PORT NOTE: the ported `internal_os::open` collapses the allocator
        // param (Rust `Command` ownership) and takes the os-local R4-seam
        // `Kind` — the parity image of `apprt.action.OpenUrl.Kind` (the
        // os/open.rs break-#14 note) — so the enum maps arm-for-arm.
        internal_os::open::open(
            match action.kind {
                OpenUrlKind::Unknown => internal_os::open::Kind::Unknown,
                OpenUrlKind::Text => internal_os::open::Kind::Text,
                OpenUrlKind::Html => internal_os::open::Kind::Html,
                // (`77537c806`: the os-side `Kind::Osc8` arm fails closed on
                // macOS — apprt owns the OSC 8 safe-opening policy.)
                OpenUrlKind::Osc8 => internal_os::open::Kind::Osc8,
            },
            action.url,
        )?;
        Ok(())
    }

    /// Return the URI for an OSC8 hyperlink at the given position or null
    /// if there is no hyperlink.
    // fn osc8URI(self: *Surface, pin: terminal.Pin) ?[]const u8 {
    //
    // PORT NOTE: the returned slice borrows the pin's PAGE memory (the
    // reference `entry.uri.slice(page.memory)`), NOT `self` — the body
    // discards `self` (`_ = self`). The explicit `'a` states that contract
    // (the raw-ptr discipline; vt's `OffsetSlice::slice` is itself
    // caller-lifetime): the bytes are valid while the renderer state mutex is
    // held and the page node is alive — exactly the reference's implicit
    // contract. Raw bytes (`&[u8]`), never a lossy String.
    //
    // PARITY: clippy's `question_mark` fold of the `orelse return null` match
    // is allowed rather than rewriting the reference's explicit early return
    // (the collapsible_if precedent).
    #[allow(clippy::question_mark)]
    pub(super) fn osc8URI<'a>(&self, pin: terminal::pagelist::Pin) -> Option<&'a [u8]> {
        // _ = self;
        //
        // (the receiver is kept for signature parity.)
        // const page = &pin.node.data;
        let page: *mut terminal::page::Page = unsafe { terminal::pagelist::Node::page(pin.node) };
        // const cell = pin.rowAndCell().cell;
        let cell = pin.row_and_cell().cell;
        // const link_id = page.lookupHyperlink(cell) orelse return null;
        let link_id = match unsafe { (*page).lookup_hyperlink(cell) } {
            Some(v) => v,
            None => return None,
        };
        // const entry = page.hyperlink_set.get(page.memory, link_id);
        let entry = unsafe { (*page).hyperlink_set.get((*page).memory_ptr(), link_id) };
        // return entry.uri.slice(page.memory);
        Some(unsafe { (*entry).uri.slice((*page).memory_ptr()) })
    }

    // pub fn mousePressureCallback(
    //     self: *Surface,
    //     stage: input.MousePressureStage,
    //     pressure: f64,
    // ) !void {
    //
    // (the inferred `!void` is the [`Error`] union.)
    pub fn mousePressureCallback(
        &mut self,
        stage: input_mouse::PressureStage,
        pressure: f64,
    ) -> Result<(), Error> {
        // Crash metadata in case we crash in here
        // crash.sentry.thread_state = self.crashThreadState();
        crash::sentry::setThreadState(self.crashThreadState());
        // defer crash.sentry.thread_state = null;
        // ([ADV 6.1] — the Drop guard clears at every exit below.)
        let _thread_state = ThreadStateGuard;

        // We don't currently use the pressure value for anything. In the
        // future, we could report this to applications using new mouse
        // events or utilize it for some custom UI.
        // _ = pressure;
        let _ = pressure;

        // If the pressure stage is the same as what we already have do nothing
        // if (self.mouse.pressure_stage == stage) return;
        if self.mouse.pressure_stage == stage {
            return Ok(());
        }

        // Update our pressure stage.
        // self.mouse.pressure_stage = stage;
        self.mouse.pressure_stage = stage;

        // A deep press is pressure-sensitive pointer input, such as macOS force
        // click / deep click on a trackpad, that occurs while the left mouse
        // button is already down. Treat it as the platform text-selection
        // affordance: select the pressed word, then consume the active gesture so
        // further cursor motion doesn't drag the selection.
        // const left_idx = @intFromEnum(input.MouseButton.left);
        let left_idx = input_mouse::Button::Left as usize;
        // if (self.mouse.click_state[left_idx] == .press and
        //     stage == .deep)
        // select: {
        if self.mouse.click_state[left_idx] == input_mouse::ButtonState::Press
            && stage == input_mouse::PressureStage::Deep
        {
            'select: {
                // self.renderer_state.mutex.lock();
                // defer self.renderer_state.mutex.unlock();
                //
                // (guard-less zigstd Mutex — the defer is the explicit unlock
                // at every exit below.)
                unsafe { (*(*self.renderer_state.as_ptr()).mutex).lock() };

                // const sel = self.mouse.selection_gesture.deepPress(
                //     self.renderer_state.terminal,
                //     .{ .word_boundary_codepoints = self.config.selection_word_chars },
                // );
                let sel = self.mouse.selection_gesture.deep_press(
                    unsafe { &mut *(*self.renderer_state.as_ptr()).terminal },
                    terminal::selection_gesture::DeepPress {
                        word_boundary_codepoints: &self.config.selection_word_chars,
                    },
                );

                // Deep press consumes the active drag gesture, so stop any pending
                // selection autoscroll timer that may have been started by the drag.
                // if (self.selection_scroll_active) {
                if self.selection_scroll_active {
                    // self.queueIo(
                    //     .{ .selection_scroll = false },
                    //     .locked,
                    // );
                    self.queueIo(
                        termio::Message::SelectionScroll(false),
                        termio::termio_impl::MutexState::Locked,
                    );
                }

                // try self.setSelection(sel orelse break :select);
                let sel = match sel {
                    Some(v) => v,
                    None => {
                        // (the deferred mutex unlock — the labeled break)
                        unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
                        break 'select;
                    }
                };
                if let Err(err) = self.setSelection(Some(sel)) {
                    // (the deferred mutex unlock — the error exit)
                    unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
                    return Err(err);
                }
                // try self.queueRender();
                if let Err(err) = self.queueRender() {
                    // (the deferred mutex unlock — the error exit)
                    unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
                    return Err(err.into());
                }

                // (the deferred mutex unlock — the block end)
                unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
            }
        }

        Ok(())
    }

    /// Cursor position callback.
    ///
    /// Send negative x or y values to indicate the cursor is outside the
    /// viewport. The magnitude of the negative values are meaningless;
    /// they are only used to indicate the cursor is outside the viewport.
    /// It's important to do this to ensure hover states are cleared.
    ///
    /// The mods parameter is optional because some apprts do not provide
    /// modifier information on cursor position events. If mods is null then
    /// we'll use the last known mods. This is usually accurate since mod events
    /// will trigger key press events but on some platforms we don't get them.
    /// For example, on macOS, unfocused surfaces don't receive key events but
    /// do receive mouse events so we have to rely on updated mods.
    // pub fn cursorPosCallback(
    //     self: *Surface,
    //     pos: apprt.CursorPos,
    //     mods: ?input.Mods,
    // ) !void {
    //
    // (the inferred `!void` is the [`Error`] union.)
    pub fn cursorPosCallback(
        &mut self,
        pos: apprt::CursorPos,
        mods: Option<Mods>,
    ) -> Result<(), Error> {
        // Crash metadata in case we crash in here
        // crash.sentry.thread_state = self.crashThreadState();
        crash::sentry::setThreadState(self.crashThreadState());
        // defer crash.sentry.thread_state = null;
        // ([ADV 6.1] — the Drop guard clears at every exit below.)
        let _thread_state = ThreadStateGuard;

        // log.debug("cursor pos x={} y={} mods={?}", .{ pos.x, pos.y, mods });
        // (COMMENT-ONLY BY DESIGN: commented out in the ORACLE too, Surface.zig:4539.)

        // If the position is negative, it is outside our viewport and
        // we need to clear any hover states.
        // if (pos.x < 0 or pos.y < 0) {
        if pos.x < 0.0 || pos.y < 0.0 {
            // Reset our hyperlink state
            // self.mouse.link_point = null;
            self.mouse.link_point = None;
            // if (self.mouse.over_link) {
            if self.mouse.over_link {
                // self.mouse.over_link = false;
                self.mouse.over_link = false;
                // _ = try self.rt_app.performAction(
                //     .{ .surface = self },
                //     .mouse_shape,
                //     self.io.terminal.mouse_shape,
                // );
                let _ = unsafe { &*self.rt_app }.perform_action(
                    apprt::Target::surface_ref(&*self),
                    apprt::Action::MouseShape(unsafe { (*self.io.as_ptr()).terminal.mouse_shape }),
                )?;
                // _ = try self.rt_app.performAction(
                //     .{ .surface = self },
                //     .mouse_over_link,
                //     .{ .url = "" },
                // );
                let _ = unsafe { &*self.rt_app }.perform_action(
                    apprt::Target::surface_ref(&*self),
                    apprt::Action::MouseOverLink(MouseOverLink { url: b"" }),
                )?;
                // try self.queueRender();
                self.queueRender()?;
            }

            // self.renderer_state.mutex.lock();
            // defer self.renderer_state.mutex.unlock();
            //
            // (guard-less zigstd Mutex — the defer is the explicit unlock at
            // this if-block's end.)
            unsafe { (*(*self.renderer_state.as_ptr()).mutex).lock() };

            // No mouse point so we don't highlight links
            // self.renderer_state.mouse.point = null;
            unsafe { (*self.renderer_state.as_ptr()).mouse.point = None };

            // Mark the link's row as dirty, but continue with updating the
            // mouse state below so we can scroll when our position is negative.
            // self.renderer_state.terminal.screens.active.dirty.hyperlink_hover = true;
            unsafe {
                (*(*(*self.renderer_state.as_ptr()).terminal).screens.active)
                    .dirty
                    .hyperlink_hover = true;
            }

            // (the deferred mutex unlock — the block end)
            unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
        }

        // Always show the mouse again if it is hidden
        // if (self.mouse.hidden) self.showMouse();
        if self.mouse.hidden {
            self.showMouse();
        }

        // Update our modifiers if they changed
        // if (mods) |v| self.modsChanged(v);
        if let Some(v) = mods {
            self.modsChanged(v);
        }

        // The mouse position in the viewport
        // const pos_vp = self.posToViewport(pos.x, pos.y);
        let pos_vp = self.posToViewport(f64::from(pos.x), f64::from(pos.y));

        // We always reset the over link status because it will be reprocessed
        // below. But we need the old value to know if we need to undo mouse
        // shape changes.
        // const over_link = self.mouse.over_link;
        let over_link = self.mouse.over_link;
        // self.mouse.over_link = false;
        self.mouse.over_link = false;

        // We are reading/writing state for the remainder
        // self.renderer_state.mutex.lock();
        // defer self.renderer_state.mutex.unlock();
        //
        // (guard-less zigstd Mutex — the defer is the explicit unlock at
        // every exit below.)
        unsafe { (*(*self.renderer_state.as_ptr()).mutex).lock() };

        // Update our mouse state. We set this to null initially because we only
        // want to set it when we're not selecting or doing any other mouse
        // event.
        // self.renderer_state.mouse.point = null;
        unsafe { (*self.renderer_state.as_ptr()).mouse.point = None };

        // If we have an inspector, we need to always record position information
        // if (self.inspector) |insp| {
        //     insp.mouse.last_xpos = pos.x;
        //     insp.mouse.last_ypos = pos.y;
        //
        //     const screen: *terminal.Screen = self.renderer_state.terminal.screens.active;
        //     insp.mouse.last_point = screen.pages.pin(.{ .viewport = .{
        //         .x = pos_vp.x,
        //         .y = pos_vp.y,
        //     } });
        //     try self.queueRender();
        // }
        //
        // PORT NOTE (Phase-3 fill): `inspectorpkg::Inspector` is now inhabited
        // (its `mouse: widgets::surface::Mouse` sub-state carries
        // `last_xpos`/`last_ypos`/`last_point`, the R3-proven Pin), so the
        // Some-arm preserved verbatim above is reachable and is inhabited here.
        // `insp` is a raw `*mut Inspector` (core's own field, `self.inspector:
        // Option<*mut inspectorpkg::Inspector>`) — dereferenced directly, no
        // new lock (this mirrors the reference, which takes no additional lock
        // beyond the `renderer_state.mutex` already held above). `screen` is
        // obtained the same raw-pointer way the rest of this function/file does
        // (`(*(*self.renderer_state.terminal).screens.active)`), so it never
        // escapes this still-locked scope (INV-1).
        if let Some(insp) = self.inspector {
            // insp.mouse.last_xpos = pos.x;
            // insp.mouse.last_ypos = pos.y;
            unsafe {
                (*insp).mouse.last_xpos = f64::from(pos.x);
                (*insp).mouse.last_ypos = f64::from(pos.y);
            }

            // const screen: *terminal.Screen = self.renderer_state.terminal.screens.active;
            let screen = unsafe { (*(*self.renderer_state.as_ptr()).terminal).screens.active };
            // insp.mouse.last_point = screen.pages.pin(.{ .viewport = .{
            //     .x = pos_vp.x,
            //     .y = pos_vp.y,
            // } });
            unsafe {
                (*insp).mouse.last_point = (*screen).pages.pin(terminal::point::Point::Viewport(
                    terminal::point::Coordinate {
                        x: pos_vp.x,
                        y: pos_vp.y,
                    },
                ));
            }

            // try self.queueRender();
            //
            // (guard-less zigstd Mutex — every other exit in this function
            // unlocks explicitly before returning; do the same here.)
            if let Err(err) = self.queueRender() {
                unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
                return Err(err.into());
            }
        }

        // Handle link hovering
        // We refresh links when
        // 1. we were previously over a link
        // OR
        // 2. the cursor position has changed (either we have no previous state, or the state has
        //    changed)
        // AND
        // 1. mouse reporting is off
        // OR
        // 2. mouse reporting is on and we are not reporting shift to the terminal
        // if ((over_link or
        //     self.mouse.link_point == null or
        //     (self.mouse.link_point != null and !self.mouse.link_point.?.eql(pos_vp))) and
        //     (self.io.terminal.flags.mouse_event == .none or
        //         (self.mouse.mods.shift and !self.mouseShiftCapture(false))))
        // {
        //
        // PARITY: the `!= null and ... .?.eql` pair keeps the reference's
        // explicit two-step null test; clippy's `unnecessary_unwrap` fold is
        // allowed rather than restructuring (the collapsible_if precedent).
        #[allow(clippy::unnecessary_unwrap)]
        if (over_link
            || self.mouse.link_point.is_none()
            || (self.mouse.link_point.is_some() && !self.mouse.link_point.unwrap().eql(pos_vp)))
            && (unsafe { (*self.io.as_ptr()).terminal.flags.mouse_event }
                == terminal::mouse::Event::None
                || (self.mouse.mods.shift && !self.mouseShiftCapture(false)))
        {
            // If we were previously over a link, we always update. We do this so that if the text
            // changed underneath us, even if the mouse didn't move, we update the URL hints and state
            // try self.mouseRefreshLinks(pos, pos_vp, over_link);
            if let Err(err) = self.mouseRefreshLinks(pos, pos_vp, over_link) {
                // (the deferred mutex unlock — the error exit)
                unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
                return Err(err);
            }
        }

        // Do a mouse report
        // if (self.isMouseReporting()) report: {
        if self.isMouseReporting() {
            'report: {
                // Shift overrides mouse "grabbing" in the window, taken from Kitty.
                // This only applies if there is a mouse button pressed so that
                // movement reports are not affected.
                // if (self.mouse.mods.shift and !self.mouseShiftCapture(false)) {
                if self.mouse.mods.shift && !self.mouseShiftCapture(false) {
                    // for (self.mouse.click_state) |state| {
                    for state in self.mouse.click_state {
                        // if (state != .release) break :report;
                        if state != input_mouse::ButtonState::Release {
                            break 'report;
                        }
                    }
                }

                // We use the first mouse button we find pressed in order to report
                // since the spec (afaict) does not say...
                // const button: ?input.MouseButton = button: for (self.mouse.click_state, 0..) |state, i| {
                //     if (state == .press)
                //         break :button @enumFromInt(i);
                // } else null;
                let button: Option<input_mouse::Button> = 'button: {
                    for (i, state) in self.mouse.click_state.iter().enumerate() {
                        if *state == input_mouse::ButtonState::Press {
                            // (`@enumFromInt(i)` — the click_state indices ARE
                            // the `Button` discriminants; realized as the
                            // exhaustive index match.)
                            break 'button Some(match i {
                                0 => input_mouse::Button::Unknown,
                                1 => input_mouse::Button::Left,
                                2 => input_mouse::Button::Right,
                                3 => input_mouse::Button::Middle,
                                4 => input_mouse::Button::Four,
                                5 => input_mouse::Button::Five,
                                6 => input_mouse::Button::Six,
                                7 => input_mouse::Button::Seven,
                                8 => input_mouse::Button::Eight,
                                9 => input_mouse::Button::Nine,
                                10 => input_mouse::Button::Ten,
                                // (the array length is `Button.max` = 11 —
                                // indices stop at 10.)
                                _ => unreachable!(),
                            });
                        }
                    }
                    // } else null;
                    None
                };

                // self.mouseReport(button, .motion, self.mouse.mods, pos);
                self.mouseReport(button, input_mouse::Action::Motion, self.mouse.mods, pos);

                // If we're doing mouse motion tracking, we do not support text
                // selection.
                // return;
                //
                // (the deferred mutex unlock — the return)
                unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
                return Ok(());
            }
        }

        // Handle cursor position for text selection
        // if (self.mouse.click_state[@intFromEnum(input.MouseButton.left)] == .press) select: {
        if self.mouse.click_state[input_mouse::Button::Left as usize]
            == input_mouse::ButtonState::Press
        {
            'select: {
                // Left click pressed but count zero can happen if mouse reporting is on.
                // In this scenario, we mark the click state because we need that to
                // properly make some mouse reports, but we don't keep track of the
                // count because we don't want to handle selection.
                // if (self.mouse.selection_gesture.left_click_count == 0) break :select;
                if self.mouse.selection_gesture.left_click_count == 0 {
                    break 'select;
                }

                // If our left-click pin no longer belongs to the active screen then we
                // don't process this. We don't invalidate our pin or mouse state
                // because if the same screen switches back then we can continue our
                // selection.
                // const t: *terminal.Terminal = self.renderer_state.terminal;
                let t: *mut Terminal = unsafe { (*self.renderer_state.as_ptr()).terminal };
                // if (self.mouse.activeLeftClickPin(&t.screens) == null) break :select;
                if self
                    .mouse
                    .activeLeftClickPin(unsafe { &(*t).screens })
                    .is_none()
                {
                    break 'select;
                }

                // All roads lead to requiring a re-render at this point.
                // try self.queueRender();
                if let Err(err) = self.queueRender() {
                    // (the deferred mutex unlock — the error exit)
                    unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
                    return Err(err.into());
                }

                // Convert to points
                // const screen: *terminal.Screen = t.screens.active;
                let screen: *mut terminal::screen::Screen = unsafe { (*t).screens.active };
                // const pin = screen.pages.pin(.{
                //     .viewport = .{
                //         .x = pos_vp.x,
                //         .y = pos_vp.y,
                //     },
                // }) orelse {
                //     if (comptime std.debug.runtime_safety) unreachable;
                //     return;
                // };
                let pin = match unsafe {
                    (*screen).pages.pin(terminal::point::Point::Viewport(
                        terminal::point::Coordinate {
                            x: pos_vp.x,
                            y: pos_vp.y,
                        },
                    ))
                } {
                    Some(v) => v,
                    None => {
                        // if (comptime std.debug.runtime_safety) unreachable;
                        if cfg!(debug_assertions) {
                            unreachable!();
                        }
                        // return;
                        //
                        // (the deferred mutex unlock — the return)
                        unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
                        return Ok(());
                    }
                };

                // Perform our drag behavior in our gesture handler.
                // const drag_selection = self.mouse.selection_gesture.drag(t, .{
                //     .pin = pin,
                //     .xpos = pos.x,
                //     .ypos = pos.y,
                //     .rectangle = SurfaceMouse.isRectangleSelectState(self.mouse.mods),
                //     .word_boundary_codepoints = self.config.selection_word_chars,
                //     .geometry = .{
                //         .columns = @intCast(self.size.grid().columns),
                //         .cell_width = self.size.cell.width,
                //         .padding_left = self.size.padding.left,
                //         .screen_height = self.size.screen.height,
                //     },
                // });
                let drag_selection = self.mouse.selection_gesture.drag(
                    unsafe { &mut *t },
                    terminal::selection_gesture::Drag {
                        pin,
                        xpos: f64::from(pos.x),
                        ypos: f64::from(pos.y),
                        rectangle: SurfaceMouse::is_rectangle_select_state(self.mouse.mods),
                        word_boundary_codepoints: &self.config.selection_word_chars,
                        geometry: terminal::selection_gesture::Geometry {
                            columns: u32::from(self.size.grid().columns),
                            cell_width: self.size.cell.width,
                            padding_left: self.size.padding.left,
                            screen_height: self.size.screen.height,
                        },
                    },
                );

                // Update our autoscroll timer based on the gesture state
                // switch (self.mouse.selection_gesture.left_drag_autoscroll) {
                match self.mouse.selection_gesture.left_drag_autoscroll {
                    // .none => if (self.selection_scroll_active) {
                    terminal::selection_gesture::Autoscroll::None => {
                        if self.selection_scroll_active {
                            // self.queueIo(
                            //     .{ .selection_scroll = false },
                            //     .locked,
                            // );
                            self.queueIo(
                                termio::Message::SelectionScroll(false),
                                termio::termio_impl::MutexState::Locked,
                            );
                        }
                    }
                    // .up, .down => if (!self.selection_scroll_active) {
                    terminal::selection_gesture::Autoscroll::Up
                    | terminal::selection_gesture::Autoscroll::Down => {
                        if !self.selection_scroll_active {
                            // self.queueIo(
                            //     .{ .selection_scroll = true },
                            //     .locked,
                            // );
                            self.queueIo(
                                termio::Message::SelectionScroll(true),
                                termio::termio_impl::MutexState::Locked,
                            );
                        }
                    }
                }

                // Update our selection based on the gesture state
                // try self.setSelection(drag_selection);
                if let Err(err) = self.setSelection(drag_selection) {
                    // (the deferred mutex unlock — the error exit)
                    unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
                    return Err(err);
                }
            }
        }

        // (the deferred mutex unlock — the function end)
        unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
        Ok(())
    }

    /// Call to notify Rustty that the color scheme for the terminal has
    /// changed.
    // pub fn colorSchemeCallback(self: *Surface, scheme: apprt.ColorScheme) !void {
    //
    // (the inferred `!void` is the [`Error`] union.)
    //
    // PORT NOTE (N456, ruling 57): raw-self. `notifyConfigConditionalState`
    // runs the host's soft `reload_config` handler, which writes this surface
    // (`updateConfig`) synchronously. `this` is an unprotected local reborrow,
    // dead across that call and taken afresh after it.
    ///
    /// # Safety
    /// `self_` is the live, stable core surface; main thread; no protected
    /// reference to it may span the call.
    pub unsafe fn colorSchemeCallback(
        self_: *mut Self,
        scheme: apprt::ColorScheme,
    ) -> Result<(), Error> {
        let this = unsafe { &mut *self_ };
        // Crash metadata in case we crash in here
        // crash.sentry.thread_state = self.crashThreadState();
        crash::sentry::setThreadState(this.crashThreadState());
        // defer crash.sentry.thread_state = null;
        // ([ADV 6.1] — the Drop guard clears at every exit below.)
        let _thread_state = ThreadStateGuard;

        // const new_scheme: configpkg.ConditionalState.Theme = switch (scheme) {
        //     .light => .light,
        //     .dark => .dark,
        // };
        let new_scheme: configpkg::conditional::Theme = match scheme {
            apprt::ColorScheme::Light => configpkg::conditional::Theme::Light,
            apprt::ColorScheme::Dark => configpkg::conditional::Theme::Dark,
        };

        // If our scheme didn't change, then we don't do anything.
        // if (self.config_conditional_state.theme == new_scheme) return;
        if this.config_conditional_state.theme == new_scheme {
            return Ok(());
        }

        // Setup our conditional state which has the current color theme.
        // self.config_conditional_state.theme = new_scheme;
        this.config_conditional_state.theme = new_scheme;
        // self.notifyConfigConditionalState();
        //
        // (`this` is dead from here: the host callback writes the surface.)
        unsafe { Self::notifyConfigConditionalState(self_) };

        // If mode 2031 is on, then we report the change live.
        // self.queueIo(.{ .color_scheme_report = .{ .force = false } }, .unlocked);
        let this = unsafe { &mut *self_ };
        this.queueIo(
            termio::Message::ColorSchemeReport { force: false },
            termio::termio_impl::MutexState::Unlocked,
        );

        Ok(())
    }

    // pub fn posToViewport(self: Surface, xpos: f64, ypos: f64) terminal.point.Coordinate {
    //
    // (the reference's BY-VALUE receiver is a read-only copy — `&self`.)
    pub fn posToViewport(&self, xpos: f64, ypos: f64) -> terminal::point::Coordinate {
        // Get our grid cell
        // const coord: rendererpkg.Coordinate = .{ .surface = .{ .x = xpos, .y = ypos } };
        let coord: rendererpkg::size::Coordinate =
            rendererpkg::size::Coordinate::Surface(rendererpkg::size::CoordinateSurface {
                x: xpos,
                y: ypos,
            });
        // const grid = coord.convert(.grid, self.size).grid;
        //
        // (the `.grid` union-field read — the tag is known post-convert.)
        let grid = match coord.convert(rendererpkg::size::CoordinateTag::Grid, self.size) {
            rendererpkg::size::Coordinate::Grid(v) => v,
            _ => unreachable!(),
        };
        // return .{ .x = grid.x, .y = grid.y };
        terminal::point::Coordinate {
            x: grid.x,
            y: u32::from(grid.y),
        }
    }

    /// Scroll to the bottom of the viewport.
    ///
    /// Precondition: the render_state mutex must be held.
    // fn scrollToBottom(self: *Surface) !void {
    //
    // PORT NOTE: now LIVE via the C14a performBindingAction csi/esc/cursor_key
    // arms (:4829/:4856/:4872); the remaining C15 completeClipboardPaste
    // (:5891) callsite lands later. The inferred `!void` is the [`Error`]
    // union.
    pub(super) fn scrollToBottom(&mut self) -> Result<(), Error> {
        // self.io.terminal.scrollViewport(.{ .bottom = {} });
        unsafe { &mut (*self.io.as_ptr()).terminal }
            .scroll_viewport(terminal::terminal::ScrollViewport::Bottom);
        // try self.queueRender();
        self.queueRender()?;
        Ok(())
    }

    // fn hideMouse(self: *Surface) void {
    pub(super) fn hideMouse(&mut self) {
        // if (self.mouse.hidden) return;
        if self.mouse.hidden {
            return;
        }
        // self.mouse.hidden = true;
        self.mouse.hidden = true;
        // _ = self.rt_app.performAction(
        //     .{ .surface = self },
        //     .mouse_visibility,
        //     .hidden,
        // ) catch |err| {
        //     log.warn("apprt failed to set mouse visibility err={}", .{err});
        // };
        if let Err(err) = unsafe { &*self.rt_app }.perform_action(
            apprt::Target::surface_ref(&*self),
            apprt::Action::MouseVisibility(MouseVisibility::Hidden),
        ) {
            // log.warn("apprt failed to set mouse visibility err={}", .{err});
            log::warn!(target: LOG_SCOPE, "apprt failed to set mouse visibility err={err:?}");
        }
    }

    // fn showMouse(self: *Surface) void {
    pub(super) fn showMouse(&mut self) {
        // if (!self.mouse.hidden) return;
        if !self.mouse.hidden {
            return;
        }
        // self.mouse.hidden = false;
        self.mouse.hidden = false;
        // _ = self.rt_app.performAction(
        //     .{ .surface = self },
        //     .mouse_visibility,
        //     .visible,
        // ) catch |err| {
        //     log.warn("apprt failed to set mouse visibility err={}", .{err});
        // };
        if let Err(err) = unsafe { &*self.rt_app }.perform_action(
            apprt::Target::surface_ref(&*self),
            apprt::Action::MouseVisibility(MouseVisibility::Visible),
        ) {
            // log.warn("apprt failed to set mouse visibility err={}", .{err});
            log::warn!(target: LOG_SCOPE, "apprt failed to set mouse visibility err={err:?}");
        }
    }
}
