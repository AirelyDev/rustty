//! PORTS: ghostty src/Surface.zig — the message cluster: `handleMessage`,
//! `selectionScrollTick`, `childExited`, `passwordInput`, `searchCallback`, `modsChanged`,
//! `mouseRefreshLinks`, `updateRendererHealth`, `updateScrollbar`, `updateConfig`,
//! `recomputeInitialSize`, `resizeWindow`.

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
    /// Called from the app thread to handle mailbox messages to our specific
    /// surface.
    //
    // pub fn handleMessage(self: *Surface, msg: Message) !void {
    //
    // PORT NOTE: `Message<'_>` (not `'static`): the one borrowed variant,
    // `ChangeConfig(&Config)`, is DIRECT-dispatched by `App.updateConfig`,
    // never queued (apprt-api `AppMailbox` note). The inferred `!void` is the
    // [`Error`] union.
    //
    // PORT NOTE (N456, P7-W6 T1): raw-self. The `.resize_window` arm runs the
    // embedder's action callback, and the macOS host resizes the window inside
    // it (`TerminalController.resizeWindow` → `setFrame(display: true)`), which
    // lays the surface view out and so re-enters this surface through
    // `rustty_surface_set_size`. That arm is dispatched here, by place through
    // `self_`, so no protected `&mut Surface` spans the callback (N442's
    // shape). Every other arm runs in `handleMessageArms` on an unprotected
    // reborrow, as before.
    //
    // PORT NOTE (N459, P7-W6 T3 fix round 1): `.present_surface` is dispatched
    // here raw-self too. Its macOS handler (`makeKeyAndOrderFront`) may fire the
    // focus publishers synchronously and re-enter this surface
    // (`SurfaceView.swift:40`); unverified, and cheap to make sound.
    //
    // PORT NOTE (N465, P7-W12 T1): `.close` and `.child_exited` are dispatched
    // here raw-self too: both reach `close`, whose runtime callback re-enters
    // this surface synchronously (see `close`).
    ///
    /// # Safety
    /// `self_` is the stable Surface allocation (init's stability contract); no
    /// protected `&mut Surface` may span the call.
    pub unsafe fn handleMessage(self_: *mut Self, msg: Message<'_>) -> Result<(), Error> {
        // switch (msg) {
        match msg {
            // .resize_window => |size| try self.resizeWindow(size),
            Message::ResizeWindow(size) => unsafe { Self::resizeWindow(self_, size) },
            // .present_surface => try self.presentSurface(),
            Message::PresentSurface => unsafe { Self::presentSurface(self_) },
            // .close => self.close(),
            Message::Close => {
                unsafe { Self::close(self_) };
                Ok(())
            }
            // .child_exited => |v| self.childExited(v),
            Message::ChildExited(v) => {
                unsafe { Self::childExited(self_, v) };
                Ok(())
            }
            msg => unsafe { &mut *self_ }.handleMessageArms(msg),
        }
    }

    /// The `handleMessage` arms that run no re-entrant host callback (see
    /// `handleMessage`'s N456 note).
    fn handleMessageArms(&mut self, msg: Message<'_>) -> Result<(), Error> {
        match msg {
            // (dispatched raw-self by `handleMessage`)
            Message::ResizeWindow(_) => unreachable!("handleMessage dispatches resize_window"),

            // .change_config => |config| try self.updateConfig(config),
            Message::ChangeConfig(config) => self.updateConfig(config)?,

            // .set_title => |*v| {
            Message::SetTitle(v) => {
                // We ignore the message in case the title was set via config.
                // if (self.config.title != null) {
                if self.config.title.is_some() {
                    // log.debug("ignoring title change request since static title is set via config", .{});
                    log::debug!(
                        target: LOG_SCOPE,
                        "ignoring title change request since static title is set via config"
                    );
                    return Ok(());
                }

                // The ptrCast just gets sliceTo to return the proper type.
                // We know that our title should end in 0.
                // const slice = std.mem.sliceTo(@as([*:0]const u8, @ptrCast(v)), 0);
                let slice = &v[..v.iter().position(|&b| b == 0).unwrap_or(v.len())];
                // log.debug("changing title \"{s}\"", .{slice});
                log::debug!(
                    target: LOG_SCOPE,
                    "changing title \"{}\"",
                    String::from_utf8_lossy(slice)
                );
                // _ = try self.rt_app.performAction(
                //     .{ .surface = self },
                //     .set_title,
                //     .{ .title = slice },
                // );
                let _ = unsafe { &*self.rt_app }.perform_action(
                    apprt::Target::surface_ref(&*self),
                    apprt::Action::SetTitle(SetTitle { title: slice }),
                )?;
            }

            // .report_title => |style| report_title: {
            Message::ReportTitle(style) => 'report_title: {
                // if (!self.config.title_report) {
                if !self.config.title_report {
                    // log.info("report_title requested, but disabled via config", .{});
                    log::info!(
                        target: LOG_SCOPE,
                        "report_title requested, but disabled via config"
                    );
                    break 'report_title;
                }

                // const title: ?[:0]const u8 = self.rt_surface.getTitle();
                let title: Option<&std::ffi::CStr> = unsafe { &*self.rt_surface }.get_title();
                // const data = switch (style) {
                //     .csi_21_t => try std.fmt.allocPrint(
                //         self.alloc,
                //         "\x1b]l{s}\x1b\\",
                //         .{title orelse ""},
                //     ),
                // };
                //
                // PORT NOTE: `allocPrint` over the raw title bytes -> an owned
                // byte Vec (the title is a raw C string — never a lossy
                // String); the OOM-only `try` collapses (infallible Rust
                // allocation).
                let data: Vec<u8> = match style {
                    apprt::ReportTitleStyle::Csi21t => {
                        let mut data = Vec::new();
                        data.extend_from_slice(b"\x1b]l");
                        // (`title orelse ""`)
                        data.extend_from_slice(match title {
                            Some(v) => v.to_bytes(),
                            None => &[],
                        });
                        data.extend_from_slice(b"\x1b\\");
                        data
                    }
                };

                // We always use an allocating message because we don't know
                // the length of the title and this isn't a performance critical
                // path.
                // self.queueIo(.{
                //     .write_alloc = .{
                //         .alloc = self.alloc,
                //         .data = data,
                //     },
                // }, .unlocked);
                //
                // (the `.alloc` field collapses into the `Alloc` payload's Box
                // ownership — the datastruct message_data PORT NOTE.)
                self.queueIo(
                    termio::Message::WriteAlloc(Alloc {
                        data: data.into_boxed_slice(),
                    }),
                    termio::termio_impl::MutexState::Unlocked,
                );
            }

            // .color_change => |change| color_change: {
            Message::ColorChange(change) => 'color_change: {
                // Notify our apprt, but don't send a mode 2031 DSR report
                // because VT sequences were used to change the color.
                // _ = try self.rt_app.performAction(
                //     .{ .surface = self },
                //     .color_change,
                //     .{ ... },
                // );
                let _ = unsafe { &*self.rt_app }.perform_action(
                    apprt::Target::surface_ref(&*self),
                    apprt::Action::ColorChange(ColorChange {
                        // .kind = switch (change.target) {
                        kind: match change.target {
                            // .palette => |v| @enumFromInt(v),
                            terminal::osc::color::Target::Palette(v) => ColorKind(i32::from(v)),
                            // .dynamic => |dyn| switch (dyn) {
                            terminal::osc::color::Target::Dynamic(dynamic) => match dynamic {
                                // .foreground => .foreground,
                                terminal::color::Dynamic::Foreground => ColorKind::FOREGROUND,
                                // .background => .background,
                                terminal::color::Dynamic::Background => ColorKind::BACKGROUND,
                                // .cursor => .cursor,
                                terminal::color::Dynamic::Cursor => ColorKind::CURSOR,
                                // Unsupported dynamic color change notification type
                                // else => break :color_change,
                                _ => break 'color_change,
                            },
                            // Special colors aren't supported for change notification
                            // .special => break :color_change,
                            terminal::osc::color::Target::Special(_) => break 'color_change,
                        },
                        // .r = change.color.r,
                        r: change.color.r,
                        // .g = change.color.g,
                        g: change.color.g,
                        // .b = change.color.b,
                        b: change.color.b,
                    }),
                )?;
            }

            // .set_mouse_shape => |shape| {
            Message::SetMouseShape(shape) => {
                // log.debug("changing mouse shape: {}", .{shape});
                log::debug!(target: LOG_SCOPE, "changing mouse shape: {shape:?}");
                // _ = try self.rt_app.performAction(
                //     .{ .surface = self },
                //     .mouse_shape,
                //     shape,
                // );
                let _ = unsafe { &*self.rt_app }.perform_action(
                    apprt::Target::surface_ref(&*self),
                    apprt::Action::MouseShape(shape),
                )?;
            }

            // .kitty_clipboard_read => |req| try self.kittyClipboardRead(req),
            //
            // (the Box leaks to the reference's raw `*KittyRead` here; the
            // handler owns destroying it — see `KittyRead::destroy`.)
            Message::KittyClipboardRead(req) => {
                self.kittyClipboardRead(Box::into_raw(req))?;
            }
            // .kitty_clipboard_write => |req| try self.kittyClipboardWrite(req),
            Message::KittyClipboardWrite(req) => {
                self.kittyClipboardWrite(Box::into_raw(req))?;
            }

            // .clipboard_read => |clipboard| {
            Message::ClipboardRead(clipboard) => {
                // if (self.config.clipboard_read == .deny) {
                if self.config.clipboard_read == configpkg::value_types::ClipboardAccess::Deny {
                    // log.info("application attempted to read clipboard, but 'clipboard-read' is set to deny", .{});
                    log::info!(
                        target: LOG_SCOPE,
                        "application attempted to read clipboard, but 'clipboard-read' is set to deny"
                    );
                    return Ok(());
                }

                // _ = try self.startClipboardRequest(.standard, .{ .osc_52_read = clipboard });
                let _ = self.startClipboardRequest(
                    apprt::Clipboard::Standard,
                    apprt::ClipboardRequest::Osc52Read(clipboard),
                )?;
            }

            // .clipboard_write => |w| switch (w.req) {
            Message::ClipboardWrite {
                clipboard_type,
                req,
            } => match req {
                // .small => |v| try self.clipboardWrite(v.data[0..v.len], w.clipboard_type),
                MessageData::Small(v) => self.clipboardWrite(&v.data[0..v.len], clipboard_type)?,
                // .stable => |v| try self.clipboardWrite(v, w.clipboard_type),
                MessageData::Stable(v) => self.clipboardWrite(v, clipboard_type)?,
                // .alloc => |v| {
                MessageData::Alloc(v) => {
                    // defer v.alloc.free(v.data);
                    // (ownership drop at arm scope end — the collapsed
                    // `Alloc.alloc` Box ownership.)
                    // try self.clipboardWrite(v.data, w.clipboard_type);
                    self.clipboardWrite(&v.data, clipboard_type)?;
                }
            },

            // .pwd_change => |w| {
            Message::PwdChange(w) => {
                // defer w.deinit();
                // (`WriteReq.deinit` — the alloc-variant free — is Drop in
                // Rustty; runs at arm scope end, the datastruct precedent.)

                // var stack = std.heap.stackFallback(256, self.alloc);
                // const alloc = stack.get();
                // const str = try alloc.dupeZ(u8, w.slice());
                // defer alloc.free(str);
                //
                // PORT NOTE (`f1d225020`): the apprt-api `Pwd` payload carries
                // raw bytes (`&[u8]`); the reference's NUL-termination
                // (`dupeZ`) is part of the deferred C-ABI pinning (the
                // action.rs `PwdC` PORT NOTE). `stackFallback(256, ..)` serves
                // the `dupeZ` allocation (len + the NUL byte) from a 256-byte
                // stack buffer when it fits and falls back to the heap for
                // longer paths — the two arms below reproduce that split
                // exactly (the SURFACE_ID stack-`bufPrint` precedent); the
                // heap arm's owned dupe + scope-end drop realize the
                // alloc/free pair (Zig defer LIFO == Rust reverse-declaration
                // drop: `str` before `w`).
                let mut stack = [0u8; 256];
                let heap: Vec<u8>;
                let str_: &[u8] = if w.slice().len() + 1 <= stack.len() {
                    let len = w.slice().len();
                    stack[..len].copy_from_slice(w.slice());
                    &stack[..len]
                } else {
                    heap = w.slice().to_vec();
                    &heap
                };

                // _ = try self.rt_app.performAction(
                //     .{ .surface = self },
                //     .pwd,
                //     .{ .pwd = str },
                // );
                // (performAction continues to borrow the value only for the
                // duration of the call.)
                let _ = unsafe { &*self.rt_app }.perform_action(
                    apprt::Target::surface_ref(&*self),
                    apprt::Action::Pwd(Pwd { pwd: str_ }),
                )?;
            }

            // .close => self.close(),
            // .child_exited => |v| self.childExited(v),
            //
            // (dispatched raw-self by `handleMessage`, N465)
            Message::Close => unreachable!("handleMessage dispatches close"),
            Message::ChildExited(_) => unreachable!("handleMessage dispatches child_exited"),

            // .desktop_notification => |notification| {
            Message::DesktopNotification(notification) => {
                // if (!self.config.desktop_notifications) {
                if !self.config.desktop_notifications {
                    // log.info("application attempted to display a desktop notification, but 'desktop-notifications' is disabled", .{});
                    log::info!(
                        target: LOG_SCOPE,
                        "application attempted to display a desktop notification, but 'desktop-notifications' is disabled"
                    );
                    return Ok(());
                }

                // const title = std.mem.sliceTo(&notification.title, 0);
                let title = &notification.title[..notification
                    .title
                    .iter()
                    .position(|&b| b == 0)
                    .unwrap_or(notification.title.len())];
                // const body = std.mem.sliceTo(&notification.body, 0);
                let body = &notification.body[..notification
                    .body
                    .iter()
                    .position(|&b| b == 0)
                    .unwrap_or(notification.body.len())];
                // try self.showDesktopNotification(title, body);
                self.showDesktopNotification(title, body)?;
            }

            // .redraw => self.redraw(),
            Message::Redraw => self.redraw(),

            // .renderer_health => |health| self.updateRendererHealth(health),
            Message::RendererHealth(health) => self.updateRendererHealth(health),

            // .scrollbar => |scrollbar| self.updateScrollbar(scrollbar),
            Message::Scrollbar(scrollbar) => self.updateScrollbar(scrollbar),

            // (dispatched raw-self by `handleMessage`)
            Message::PresentSurface => unreachable!("handleMessage dispatches present_surface"),

            // .password_input => |v| try self.passwordInput(v),
            Message::PasswordInput(v) => self.passwordInput(v)?,

            // .ring_bell => bell: {
            Message::RingBell => 'bell: {
                // const now = std.time.Instant.now() catch unreachable;
                //
                // (Rust `Instant::now` is infallible — the `catch unreachable`
                // collapses.)
                let now = std::time::Instant::now();
                // if (self.last_bell_time) |last| {
                if let Some(last) = self.last_bell_time {
                    // if (now.since(last) < 100 * std.time.ns_per_ms) break :bell;
                    //
                    // (`Instant.since` -> `duration_since` nanoseconds; the
                    // config_struct.rs NS_PER_MS precedent.)
                    const NS_PER_MS: u128 = 1_000_000;
                    if now.duration_since(last).as_nanos() < 100 * NS_PER_MS {
                        break 'bell;
                    }
                }
                // self.last_bell_time = now;
                self.last_bell_time = Some(now);
                // _ = self.rt_app.performAction(
                //     .{ .surface = self },
                //     .ring_bell,
                //     {},
                // ) catch |err| {
                //     log.warn("apprt failed to ring bell={}", .{err});
                // };
                if let Err(err) = unsafe { &*self.rt_app }
                    .perform_action(apprt::Target::surface_ref(&*self), apprt::Action::RingBell)
                {
                    // log.warn("apprt failed to ring bell={}", .{err});
                    log::warn!(target: LOG_SCOPE, "apprt failed to ring bell={err:?}");
                }
            }

            // .progress_report => |v| {
            Message::ProgressReport(v) => {
                // _ = self.rt_app.performAction(
                //     .{ .surface = self },
                //     .progress_report,
                //     v,
                // ) catch |err| {
                //     log.warn("apprt failed to report progress err={}", .{err});
                // };
                if let Err(err) = unsafe { &*self.rt_app }.perform_action(
                    apprt::Target::surface_ref(&*self),
                    apprt::Action::ProgressReport(v),
                ) {
                    // log.warn("apprt failed to report progress err={}", .{err});
                    log::warn!(target: LOG_SCOPE, "apprt failed to report progress err={err:?}");
                }
            }

            // .selection_scroll_tick => |active| {
            Message::SelectionScrollTick(active) => {
                // self.selection_scroll_active = active;
                self.selection_scroll_active = active;
                // try self.selectionScrollTick();
                self.selectionScrollTick()?;
            }

            // .start_command => {
            Message::StartCommand => {
                // self.command_timer = try .now();
                //
                // (Rust `Instant::now` is infallible — the `try` collapses.)
                self.command_timer = Some(std::time::Instant::now());
            }

            // .stop_command => |v| timer: {
            Message::StopCommand(v) => 'timer: {
                // const end: std.time.Instant = try .now();
                let end: std::time::Instant = std::time::Instant::now();
                // const start = self.command_timer orelse break :timer;
                let start = match self.command_timer {
                    Some(start) => start,
                    None => break 'timer,
                };
                // self.command_timer = null;
                self.command_timer = None;

                // const duration: Duration = .{ .duration = end.since(start) };
                let duration: Duration = Duration {
                    duration: end.duration_since(start).as_nanos() as u64,
                };
                // log.debug("command took {f}", .{duration});
                //
                // (`{f}` calls the type's own `format`; `Duration::format` is
                // the faithful port of it, so this is byte-identical.)
                {
                    let mut duration_buf: Vec<u8> = Vec::new();
                    duration.format(&mut duration_buf);
                    log::debug!(
                        target: LOG_SCOPE,
                        "command took {}",
                        String::from_utf8_lossy(&duration_buf)
                    );
                }

                // _ = self.rt_app.performAction(
                //     .{ .surface = self },
                //     .command_finished,
                //     .{
                //         .exit_code = v,
                //         .duration = duration,
                //     },
                // ) catch |err| {
                //     log.warn("apprt failed to notify command finish={}", .{err});
                // };
                if let Err(err) = unsafe { &*self.rt_app }.perform_action(
                    apprt::Target::surface_ref(&*self),
                    apprt::Action::CommandFinished(CommandFinished {
                        exit_code: v,
                        duration,
                    }),
                ) {
                    // log.warn("apprt failed to notify command finish={}", .{err});
                    log::warn!(target: LOG_SCOPE, "apprt failed to notify command finish={err:?}");
                }
            }

            // .search_total => |v| {
            Message::SearchTotal(v) => {
                // _ = try self.rt_app.performAction(
                //     .{ .surface = self },
                //     .search_total,
                //     .{ .total = v },
                // );
                let _ = unsafe { &*self.rt_app }.perform_action(
                    apprt::Target::surface_ref(&*self),
                    apprt::Action::SearchTotal(SearchTotal { total: v }),
                )?;
            }

            // .search_selected => |v| {
            Message::SearchSelected(v) => {
                // _ = try self.rt_app.performAction(
                //     .{ .surface = self },
                //     .search_selected,
                //     .{ .selected = v },
                // );
                let _ = unsafe { &*self.rt_app }.perform_action(
                    apprt::Target::surface_ref(&*self),
                    apprt::Action::SearchSelected(SearchSelected { selected: v }),
                )?;
            }
        }

        Ok(())
    }

    // fn selectionScrollTick(self: *Surface) !void {
    fn selectionScrollTick(&mut self) -> Result<(), Error> {
        // If we're no longer active then we don't do anything.
        // if (!self.selection_scroll_active) return;
        if !self.selection_scroll_active {
            return Ok(());
        }

        // If our gesture doesn't want autoscrolling then disable it.
        // const was_autoscrolling = self.mouse.selection_gesture.left_drag_autoscroll != .none;
        let was_autoscrolling = self.mouse.selection_gesture.left_drag_autoscroll
            != terminal::selection_gesture::Autoscroll::None;
        // if (!was_autoscrolling) {
        if !was_autoscrolling {
            // self.queueIo(
            //     .{ .selection_scroll = false },
            //     .unlocked,
            // );
            self.queueIo(
                termio::Message::SelectionScroll(false),
                termio::termio_impl::MutexState::Unlocked,
            );
            return Ok(());
        }

        // const pos = try self.rt_surface.getCursorPos();
        let pos = unsafe { &*self.rt_surface }.get_cursor_pos()?;
        // const pos_vp = self.posToViewport(pos.x, pos.y);
        //
        // (Zig widens the f32 cursor coordinates to the f64 params.)
        let pos_vp = self.posToViewport(f64::from(pos.x), f64::from(pos.y));

        // We need our locked state for the remainder
        // self.renderer_state.mutex.lock();
        // defer self.renderer_state.mutex.unlock();
        //
        // (guard-less zigstd Mutex — CORE_BLUEPRINT §3; the defer is the
        // explicit unlock at every exit below.)
        unsafe { (*(*self.renderer_state.as_ptr()).mutex).lock() };
        // const t: *terminal.Terminal = self.renderer_state.terminal;
        let t: *mut Terminal = unsafe { (*self.renderer_state.as_ptr()).terminal };

        // const selection = self.mouse.selection_gesture.autoscrollTick(t, .{
        //     .viewport = pos_vp,
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
        let selection = self.mouse.selection_gesture.autoscroll_tick(
            unsafe { &mut *t },
            terminal::selection_gesture::AutoscrollTick {
                viewport: pos_vp,
                xpos: f64::from(pos.x),
                ypos: f64::from(pos.y),
                rectangle: crate::surface_mouse::SurfaceMouse::is_rectangle_select_state(
                    self.mouse.mods,
                ),
                word_boundary_codepoints: &self.config.selection_word_chars,
                geometry: terminal::selection_gesture::Geometry {
                    columns: u32::from(self.size.grid().columns),
                    cell_width: self.size.cell.width,
                    padding_left: self.size.padding.left,
                    screen_height: self.size.screen.height,
                },
            },
        );

        // If we're no longer autoscrolling for whatever reason, disable it.
        // if (self.mouse.selection_gesture.left_drag_autoscroll == .none) {
        if self.mouse.selection_gesture.left_drag_autoscroll
            == terminal::selection_gesture::Autoscroll::None
        {
            // self.queueIo(
            //     .{ .selection_scroll = false },
            //     .locked,
            // );
            self.queueIo(
                termio::Message::SelectionScroll(false),
                termio::termio_impl::MutexState::Locked,
            );
        }

        // If our left click was invalidated, ignore the result. This isn't
        // strictly necessary but its a nice to have.
        // if (self.mouse.selection_gesture.left_click_count == 0) return;
        if self.mouse.selection_gesture.left_click_count == 0 {
            // (the deferred mutex unlock)
            unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
            return Ok(());
        }

        // We modified our viewport and selection so we need to queue
        // a render.
        // try self.setSelection(selection);
        if let Err(err) = self.setSelection(selection) {
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

        // (the deferred mutex unlock — the normal exit)
        unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
        Ok(())
    }

    // fn childExited(self: *Surface, info: apprt.surface.Message.ChildExited) void {
    //
    // PARITY: not `.unwrap_or(false)` on the two performAction results —
    // each Err arm IS the reference's labeled `catch |err| gui:` block with
    // its log site (the init conditional-state precedent).
    //
    // PORT NOTE (N465, P7-W12 T1): raw-self. The closing `self.close()` runs the
    // runtime's close callback, which re-enters this surface (see `close`).
    ///
    /// # Safety
    /// As [`Surface::handleMessage`].
    #[allow(clippy::manual_unwrap_or, clippy::manual_unwrap_or_default)]
    unsafe fn childExited(self_: *mut Self, info: apprt::surface::ChildExited) {
        // (N465: an unprotected local reborrow, dead before `close`.)
        let this = unsafe { &mut *self_ };
        // Mark our flag that we exited immediately
        // self.child_exited = true;
        this.child_exited = true;

        // If our runtime was below some threshold then we assume that this
        // was an abnormal exit and we show an error message.
        // if (info.runtime_ms <= self.config.abnormal_command_exit_runtime_ms) runtime: {
        if info.runtime_ms <= u64::from(this.config.abnormal_command_exit_runtime_ms) {
            'runtime: {
                // On macOS, our exit code detection doesn't work, possibly
                // because of our `login` wrapper. More investigation required.
                // if (comptime !builtin.target.os.tag.isDarwin()) {
                if !cfg!(target_vendor = "apple") {
                    // If the exit code is 0 then it was a good exit.
                    // if (info.exit_code == 0) break :runtime;
                    if info.exit_code == 0 {
                        break 'runtime;
                    }
                }

                // log.warn("abnormal process exit detected, showing error message", .{});
                log::warn!(
                    target: LOG_SCOPE,
                    "abnormal process exit detected, showing error message"
                );

                // Try and show a GUI message. If it returns true, don't do anything else.
                // if (self.rt_app.performAction(
                //     .{ .surface = self },
                //     .show_child_exited,
                //     info,
                // ) catch |err| gui: {
                //     log.err("error trying to show native child exited GUI err={}", .{err});
                //     break :gui false;
                // }) return;
                //
                // (N465, ruling 62: the host callback may re-enter this surface,
                // so it runs by place through `self_`; `this` is re-taken after.)
                if match unsafe { &*(*self_).rt_app }.perform_action(
                    unsafe { apprt::Target::surface_ptr(self_) },
                    apprt::Action::ShowChildExited(info),
                ) {
                    Ok(shown) => shown,
                    Err(err) => {
                        // log.err("error trying to show native child exited GUI err={}", .{err});
                        log::error!(
                            target: LOG_SCOPE,
                            "error trying to show native child exited GUI err={err:?}"
                        );
                        // break :gui false;
                        false
                    }
                } {
                    return;
                }
                let this = unsafe { &mut *self_ };

                // If a native GUI notification was not shown, update our terminal to
                // note the abnormal exit.
                // self.childExitedAbnormally(info) catch |err| {
                //     log.err("error handling abnormal child exit err={}", .{err});
                //     return;
                // };
                if let Err(err) = this.childExitedAbnormally(info) {
                    // log.err("error handling abnormal child exit err={}", .{err});
                    log::error!(target: LOG_SCOPE, "error handling abnormal child exit err={err:?}");
                    return;
                }

                return;
            }
        }

        // We output a message so that the user knows what's going on and
        // doesn't think their terminal just froze. We show this unconditionally
        // on close even if `wait_after_command` is false and the surface closes
        // immediately because if a user does an `undo` to restore a closed
        // surface then they will see this message and know the process has
        // completed.
        // terminal: {
        'terminal: {
            // First try and show a native GUI message.
            // if (self.rt_app.performAction(
            //     .{ .surface = self },
            //     .show_child_exited,
            //     info,
            // ) catch |err| gui: {
            //     log.err("error trying to show native child exited GUI err={}", .{err});
            //     break :gui false;
            // }) break :terminal;
            //
            // (N465, ruling 62: by place through `self_`, as above.)
            if match unsafe { &*(*self_).rt_app }.perform_action(
                unsafe { apprt::Target::surface_ptr(self_) },
                apprt::Action::ShowChildExited(info),
            ) {
                Ok(shown) => shown,
                Err(err) => {
                    // log.err("error trying to show native child exited GUI err={}", .{err});
                    log::error!(
                        target: LOG_SCOPE,
                        "error trying to show native child exited GUI err={err:?}"
                    );
                    // break :gui false;
                    false
                }
            } {
                break 'terminal;
            }
            let this = unsafe { &mut *self_ };

            // If the native GUI can't be shown, display a text message in the
            // terminal.
            // self.renderer_state.mutex.lock();
            // defer self.renderer_state.mutex.unlock();
            //
            // (guard-less zigstd Mutex — the defer is the explicit unlock at
            // every exit of this block.)
            unsafe { (*(*this.renderer_state.as_ptr()).mutex).lock() };
            // const t: *terminal.Terminal = self.renderer_state.terminal;
            let t: *mut Terminal = unsafe { (*this.renderer_state.as_ptr()).terminal };
            // t.carriageReturn();
            unsafe { (*t).carriage_return() };
            // t.linefeed() catch break :terminal;
            if unsafe { (*t).linefeed() }.is_err() {
                unsafe { (*(*this.renderer_state.as_ptr()).mutex).unlock() };
                break 'terminal;
            }
            // t.printString("Process exited. Press any key to close the terminal.") catch
            //     break :terminal;
            if unsafe { (*t).print_string(b"Process exited. Press any key to close the terminal.") }
                .is_err()
            {
                unsafe { (*(*this.renderer_state.as_ptr()).mutex).unlock() };
                break 'terminal;
            }
            // t.modes.set(.cursor_visible, false);
            unsafe { (*t).modes.set(terminal::modes::Mode::CursorVisible, false) };

            // We also want to ensure that normal keyboard encoding is on
            // so that we can close the terminal. We close the terminal on
            // any key press that encodes a character.
            // t.modes.set(.disable_keyboard, false);
            unsafe {
                (*t).modes
                    .set(terminal::modes::Mode::DisableKeyboard, false)
            };
            // t.screens.active.kitty_keyboard.set(.set, .disabled);
            unsafe {
                (*(*t).screens.active).kitty_keyboard.set(
                    terminal::kitty::key::SetMode::Set,
                    terminal::kitty::KeyFlags::DISABLED,
                )
            };

            // (the deferred mutex unlock — the normal exit of the block)
            unsafe { (*(*this.renderer_state.as_ptr()).mutex).unlock() };
        }

        // Waiting after command we stop here. The terminal is updated, our
        // state is updated, and now its up to the user to decide what to do.
        // if (self.config.wait_after_command) return;
        //
        // (N465: re-taken after the show_child_exited callback.)
        let this = unsafe { &mut *self_ };
        if this.config.wait_after_command {
            return;
        }

        // If we aren't waiting after the command, then we exit immediately
        // with no confirmation.
        // self.close();
        unsafe { Self::close(self_) };
    }

    /// Called when the child process exited abnormally.
    //
    // fn childExitedAbnormally(
    //     self: *Surface,
    //     info: apprt.surface.Message.ChildExited,
    // ) !void {
    //
    // PORT NOTE: the inferred `!void` error set is the vt terminal storage
    // failures (`setAttribute`/`plainString`/`linefeed`/`print`/`printString`
    // — Zig `Allocator.Error` surfaced as vt `OutOfMemory`) plus the
    // `printString` UTF-8 validation error (`error.InvalidUtf8`), together
    // `PrintStringError`; the join/allocPrint OOMs collapse (infallible Rust
    // allocation).
    #[allow(clippy::redundant_closure_call)]
    fn childExitedAbnormally(
        &mut self,
        info: apprt::surface::ChildExited,
    ) -> Result<(), PrintStringError> {
        // var arena = ArenaAllocator.init(self.alloc);
        // defer arena.deinit();
        // const alloc = arena.allocator();
        //
        // PORT NOTE (collapsed arena): the allocations below are owned values
        // that free on drop (the C5 DerivedConfig arena-collapse precedent);
        // the arena's allocator maps to the surface allocator.
        let alloc = self.alloc;

        // Build up our command for the error message
        // const command = try std.mem.join(alloc, " ", switch (self.io.backend) {
        //     .exec => |*exec| exec.subprocess.args,
        // });
        //
        // PORT NOTE (N449, P7-W7 T5d): borrows `subprocess.args` only, through
        // the raw `.exec => |*exec|` projection (`Backend::exec_ptr`). A
        // `&Backend` covers the whole subprocess, whose `grid_size` /
        // `screen_size` / `process` the io writer thread writes meanwhile
        // (`resize`, `threadExit`): a race (`.port/phase-2/mirihar-n444`
        // `n449_close_confirm_backend_ref`, UB under SB and TB). `args` is
        // written only at init.
        let args: &Vec<std::ffi::CString> = {
            let exec = termio::Backend::exec_ptr(unsafe { &raw mut (*self.io.as_ptr()).backend });
            unsafe { &(*exec).subprocess.args }
        };
        let command: Vec<u8> = args
            .iter()
            .map(|arg| arg.to_bytes().to_vec())
            .collect::<Vec<Vec<u8>>>()
            .join(&b' ');
        // const runtime_str = try std.fmt.allocPrint(alloc, "{d} ms", .{info.runtime_ms});
        let runtime_str = format!("{} ms", info.runtime_ms);

        // self.renderer_state.mutex.lock();
        // defer self.renderer_state.mutex.unlock();
        //
        // (guard-less zigstd Mutex — the deferred unlock is realized ONCE at
        // the single exit below; the `?`-propagating body is scoped in an
        // immediately-invoked closure so the unlock also runs on the error
        // paths — the vt search/screen.rs error-scoping precedent.)
        unsafe { (*(*self.renderer_state.as_ptr()).mutex).lock() };
        // const t: *terminal.Terminal = self.renderer_state.terminal;
        let t: *mut Terminal = unsafe { (*self.renderer_state.as_ptr()).terminal };

        let result = (|| -> Result<(), PrintStringError> {
            unsafe {
                // No matter what move the cursor back to the column 0.
                // t.carriageReturn();
                (*t).carriage_return();

                // Reset styles
                // try t.setAttribute(.{ .unset = {} });
                (*t).set_attribute(terminal::sgr::Attribute::Unset)?;

                // If there is data in the viewport, we want to scroll down
                // a little bit and write a horizontal rule before writing
                // our message. This lets the use see the error message the
                // command may have output.
                // const viewport_str = try t.plainString(alloc);
                let viewport_str = (*t).plain_string(alloc.into())?;
                // if (viewport_str.len > 0) {
                if !viewport_str.is_empty() {
                    // try t.linefeed();
                    (*t).linefeed()?;
                    // for (0..t.cols) |_| try t.print(0x2501);
                    for _ in 0..(*t).cols {
                        (*t).print(0x2501)?;
                    }
                    // t.carriageReturn();
                    (*t).carriage_return();
                    // try t.linefeed();
                    (*t).linefeed()?;
                    // try t.linefeed();
                    (*t).linefeed()?;
                }

                // Output our error message
                // try t.setAttribute(.{ .@"8_fg" = .bright_red });
                (*t).set_attribute(terminal::sgr::Attribute::Color8Fg(
                    terminal::color::Name::BrightRed,
                ))?;
                // try t.setAttribute(.{ .bold = {} });
                (*t).set_attribute(terminal::sgr::Attribute::Bold)?;
                // try t.printString("Ghostty failed to launch the requested command:");
                //
                // PORT NOTE (R2 identity): the product name in the user-facing
                // message — `rustty_identity::APP_NAME_TITLECASE` (the oracle
                // prints the capitalized product name here).
                (*t).print_string(
                    format!(
                        "{} failed to launch the requested command:",
                        rustty_identity::APP_NAME_TITLECASE
                    )
                    .as_bytes(),
                )?;
                // try t.setAttribute(.{ .unset = {} });
                (*t).set_attribute(terminal::sgr::Attribute::Unset)?;

                // t.carriageReturn();
                (*t).carriage_return();
                // try t.linefeed();
                (*t).linefeed()?;
                // try t.linefeed();
                (*t).linefeed()?;
                // try t.printString(command);
                (*t).print_string(&command)?;
                // try t.setAttribute(.{ .unset = {} });
                (*t).set_attribute(terminal::sgr::Attribute::Unset)?;

                // t.carriageReturn();
                (*t).carriage_return();
                // try t.linefeed();
                (*t).linefeed()?;
                // try t.linefeed();
                (*t).linefeed()?;
                // try t.printString("Runtime: ");
                (*t).print_string(b"Runtime: ")?;
                // try t.setAttribute(.{ .@"8_fg" = .red });
                (*t).set_attribute(terminal::sgr::Attribute::Color8Fg(
                    terminal::color::Name::Red,
                ))?;
                // try t.printString(runtime_str);
                (*t).print_string(runtime_str.as_bytes())?;
                // try t.setAttribute(.{ .unset = {} });
                (*t).set_attribute(terminal::sgr::Attribute::Unset)?;

                // We don't print this on macOS because the exit code is always 0
                // due to the way we launch the process.
                // if (comptime !builtin.target.os.tag.isDarwin()) {
                if !cfg!(target_vendor = "apple") {
                    // const exit_code_str = try std.fmt.allocPrint(alloc, "{d}", .{info.exit_code});
                    let exit_code_str = format!("{}", info.exit_code);
                    // t.carriageReturn();
                    (*t).carriage_return();
                    // try t.linefeed();
                    (*t).linefeed()?;
                    // try t.printString("Exit Code: ");
                    (*t).print_string(b"Exit Code: ")?;
                    // try t.setAttribute(.{ .@"8_fg" = .red });
                    (*t).set_attribute(terminal::sgr::Attribute::Color8Fg(
                        terminal::color::Name::Red,
                    ))?;
                    // try t.printString(exit_code_str);
                    (*t).print_string(exit_code_str.as_bytes())?;
                    // try t.setAttribute(.{ .unset = {} });
                    (*t).set_attribute(terminal::sgr::Attribute::Unset)?;
                }

                // t.carriageReturn();
                (*t).carriage_return();
                // try t.linefeed();
                (*t).linefeed()?;
                // try t.linefeed();
                (*t).linefeed()?;
                // try t.printString("Press any key to close the window.");
                (*t).print_string(b"Press any key to close the window.")?;

                // Hide the cursor
                // t.modes.set(.cursor_visible, false);
                (*t).modes.set(terminal::modes::Mode::CursorVisible, false);
            }

            Ok(())
        })();

        // (the deferred mutex unlock — runs on both the Ok and Err paths)
        unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
        result
    }

    /// Called when the terminal detects there is a password input prompt.
    // fn passwordInput(self: *Surface, v: bool) !void {
    fn passwordInput(&mut self, v: bool) -> Result<(), Error> {
        {
            // self.renderer_state.mutex.lock();
            // defer self.renderer_state.mutex.unlock();
            //
            // (guard-less zigstd Mutex — the defer is the explicit unlock at
            // both exits of this block.)
            unsafe { (*(*self.renderer_state.as_ptr()).mutex).lock() };

            // If our password input state is unchanged then we don't
            // waste time doing anything more.
            // const old = self.io.terminal.flags.password_input;
            let old = unsafe { (*self.io.as_ptr()).terminal.flags.password_input };
            // if (old == v) return;
            if old == v {
                unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
                return Ok(());
            }

            // self.io.terminal.flags.password_input = v;
            unsafe { (*self.io.as_ptr()).terminal.flags.password_input = v };

            unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
        }

        // Notify our apprt so it can do whatever it wants.
        // _ = self.rt_app.performAction(
        //     .{ .surface = self },
        //     .secure_input,
        //     if (v) .on else .off,
        // ) catch |err| {
        //     // We ignore this error because we don't want to fail this
        //     // entire operation just because the apprt failed to set
        //     // the secure input state.
        //     log.warn("apprt failed to set secure input state err={}", .{err});
        // };
        if let Err(err) = unsafe { &*self.rt_app }.perform_action(
            apprt::Target::surface_ref(&*self),
            apprt::Action::SecureInput(if v { SecureInput::On } else { SecureInput::Off }),
        ) {
            // We ignore this error because we don't want to fail this
            // entire operation just because the apprt failed to set
            // the secure input state.
            // log.warn("apprt failed to set secure input state err={}", .{err});
            log::warn!(target: LOG_SCOPE, "apprt failed to set secure input state err={err:?}");
        }

        // try self.queueRender();
        self.queueRender()?;
        Ok(())
    }

    // fn searchCallback(event: terminal.search.Thread.Event, ud: ?*anyopaque) void {
    //
    // PORT NOTE: the C14 `performBindingAction` search arm registers this as
    // the vt `search::thread::Options.event_cb` fn pointer (`?*anyopaque` ->
    // the seam's bare `*mut c_void`). Now LIVE (the `.search` arm registers it).
    pub(super) fn searchCallback(event: SearchThreadEvent<'_>, ud: *mut core::ffi::c_void) {
        // IMPORTANT: This function is run on the SEARCH THREAD! It is NOT SAFE
        // to access anything other than values that never change on the surface.
        // The surface is guaranteed to be valid for the lifetime of the search
        // thread.
        // const self: *Surface = @ptrCast(@alignCast(ud.?));
        //
        // (`ud.?` — the vt seam models the userdata as a bare pointer, so the
        // unwrap collapses into the non-null registration contract.)
        //
        // PORT NOTE (N444 fix round 1): `ud` is the `SearchCallbackCtx<A>`
        // captured at spawn, not the surface (see its doc).
        let ctx: *const SearchCallbackCtx<A> = ud.cast_const().cast();
        // self.searchCallback_(event) catch |err| {
        //     log.warn("error in search callback err={}", .{err});
        // };
        //
        // PORT NOTE (§3.3a): pass the RAW `self_` (NOT `&mut *self_`) — a whole
        // `&mut Surface` reborrow here, on the search thread, races the main
        // thread's `&mut self` = SB UB (mirihar `callback_naive`).
        if let Err(err) = unsafe { Self::searchCallback_(ctx, event) } {
            // log.warn("error in search callback err={}", .{err});
            //
            // NOTE (task 14d-1): this emission fires on the SEARCH THREAD, so
            // `log_capture::start`'s thread-isolated capture cannot observe it;
            // a test for it needs `log_capture::start_admitting_all_threads`.
            log::warn!(target: LOG_SCOPE, "error in search callback err={err:?}");
        }
    }

    // fn searchCallback_(
    //     self: *Surface,
    //     event: terminal.search.Thread.Event,
    // ) !void {
    //
    // PORT NOTE (§3.3a): the reference `self: *Surface` (a raw ptr) is retyped
    // off `&mut self` to `self_: *mut Surface` — the callback fires on the SEARCH
    // THREAD while the main thread holds `&mut self`, so a whole `&mut Surface`
    // reborrow is SB UB. Every access below is a NARROW raw field projection off
    // `self_` (never a whole `&mut Surface`/`&Surface`); verdict SB + TB clean
    // (mirihar `callback_shipped`). This preserves the reference's cross-thread
    // Surface access — only the Rust `&mut`-exclusivity narrowing changes.
    //
    // PORT NOTE (N444 fix round 1, P7-W7 T5c): the receiver is now the
    // spawn-time `SearchCallbackCtx`, not the surface: even narrow field reads
    // through the surface pointer (`renderer_thread`, `rt_app`, `app`) raced
    // the main thread's `&mut Surface` retags under SB. The pushes and notifies
    // are the same; only where their pointers are read from changes.
    unsafe fn searchCallback_(
        ctx: *const SearchCallbackCtx<A>,
        event: SearchThreadEvent<'_>,
    ) -> Result<(), Error> {
        // NOTE: This runs on the search thread.

        // switch (event) {
        match event {
            // .viewport_matches => |matches_unowned| {
            SearchThreadEvent::ViewportMatches(matches_unowned) => {
                // var arena: ArenaAllocator = .init(self.alloc);
                // errdefer arena.deinit();
                // const alloc = arena.allocator();
                //
                // PORT NOTE (collapsed arena): the renderer `SearchMatches`
                // payload owns its Vec (the renderer message.rs arena-collapse
                // note); the errdefer is the Vec drop on the `?` unwind.

                // const matches = try alloc.dupe(terminal.highlight.Flattened, matches_unowned);
                // for (matches) |*m| m.* = try m.clone(alloc);
                //
                // (the shallow dupe + in-place deep clone collapse into one
                // clone-into-Vec pass — same elements, same failure points.
                // The collapsed arena is the Rust heap, so each clone goes
                // through the Rust heap handle.)
                let mut matches: Vec<terminal::highlight::Flattened> =
                    Vec::with_capacity(matches_unowned.len());
                for m in matches_unowned {
                    matches.push(m.clone(terminal::alloc::Global)?);
                }

                // _ = self.renderer_thread.mailbox.push(
                //     .{ .search_viewport_matches = .{
                //         .arena = arena,
                //         .matches = matches,
                //     } },
                //     .forever,
                // );
                let _ = unsafe { &*(*ctx).renderer_mailbox }.push(
                    rendererpkg::Message::SearchViewportMatches(
                        rendererpkg::message::SearchMatches { matches },
                    ),
                    Timeout::Forever,
                );
                // try self.renderer_thread.wakeup.notify();
                unsafe { &*(*ctx).renderer_wakeup }.notify()?;
            }

            // .selected_match => |selected_| {
            SearchThreadEvent::SelectedMatch(selected_) => {
                // if (selected_) |sel| {
                if let Some(sel) = selected_ {
                    // Copy the flattened match.
                    // var arena: ArenaAllocator = .init(self.alloc);
                    // errdefer arena.deinit();
                    // const alloc = arena.allocator();
                    // const match = try sel.highlight.clone(alloc);
                    //
                    // (same collapsed arena as above; `match` is a Rust
                    // keyword — the renderer message.rs `r#match` note.)
                    let match_ = sel.highlight.clone(terminal::alloc::Global)?;

                    // _ = self.renderer_thread.mailbox.push(
                    //     .{ .search_selected_match = .{
                    //         .arena = arena,
                    //         .match = match,
                    //     } },
                    //     .forever,
                    // );
                    let _ = unsafe { &*(*ctx).renderer_mailbox }.push(
                        rendererpkg::Message::SearchSelectedMatch(Some(
                            rendererpkg::message::SearchMatch { r#match: match_ },
                        )),
                        Timeout::Forever,
                    );

                    // Send the selected index to the surface mailbox
                    // _ = self.surfaceMailbox().push(
                    //     .{ .search_selected = sel.idx },
                    //     .forever,
                    // );
                    let _ = unsafe { &(*ctx).surface_mailbox }
                        .push(Message::SearchSelected(Some(sel.idx)), Timeout::Forever);
                } else {
                    // Reset our selected match
                    // _ = self.renderer_thread.mailbox.push(
                    //     .{ .search_selected_match = null },
                    //     .forever,
                    // );
                    let _ = unsafe { &*(*ctx).renderer_mailbox }.push(
                        rendererpkg::Message::SearchSelectedMatch(None),
                        Timeout::Forever,
                    );

                    // Reset the selected index
                    // _ = self.surfaceMailbox().push(
                    //     .{ .search_selected = null },
                    //     .forever,
                    // );
                    let _ = unsafe { &(*ctx).surface_mailbox }
                        .push(Message::SearchSelected(None), Timeout::Forever);
                }

                // try self.renderer_thread.wakeup.notify();
                unsafe { &*(*ctx).renderer_wakeup }.notify()?;
            }

            // .total_matches => |total| {
            SearchThreadEvent::TotalMatches(total) => {
                // _ = self.surfaceMailbox().push(
                //     .{ .search_total = total },
                //     .forever,
                // );
                //
                // (Zig coerces the `usize` into the `?usize` payload.)
                let _ = unsafe { &(*ctx).surface_mailbox }
                    .push(Message::SearchTotal(Some(total)), Timeout::Forever);
            }

            // When we quit, tell our renderer to reset any search state.
            // .quit => {
            SearchThreadEvent::Quit => {
                // _ = self.renderer_thread.mailbox.push(
                //     .{ .search_selected_match = null },
                //     .forever,
                // );
                let _ = unsafe { &*(*ctx).renderer_mailbox }.push(
                    rendererpkg::Message::SearchSelectedMatch(None),
                    Timeout::Forever,
                );
                // _ = self.renderer_thread.mailbox.push(
                //     .{ .search_viewport_matches = .{
                //         .arena = .init(self.alloc),
                //         .matches = &.{},
                //     } },
                //     .forever,
                // );
                let _ = unsafe { &*(*ctx).renderer_mailbox }.push(
                    rendererpkg::Message::SearchViewportMatches(
                        rendererpkg::message::SearchMatches {
                            matches: Vec::new(),
                        },
                    ),
                    Timeout::Forever,
                );
                // try self.renderer_thread.wakeup.notify();
                unsafe { &*(*ctx).renderer_wakeup }.notify()?;

                // Reset search totals in the surface
                // _ = self.surfaceMailbox().push(
                //     .{ .search_total = null },
                //     .forever,
                // );
                let _ = unsafe { &(*ctx).surface_mailbox }
                    .push(Message::SearchTotal(None), Timeout::Forever);
                // _ = self.surfaceMailbox().push(
                //     .{ .search_selected = null },
                //     .forever,
                // );
                let _ = unsafe { &(*ctx).surface_mailbox }
                    .push(Message::SearchSelected(None), Timeout::Forever);
            }

            // Unhandled, so far.
            // .complete => {},
            SearchThreadEvent::Complete => {}
        }

        Ok(())
    }

    /// Call this when modifiers change. This is safe to call even if modifiers
    /// match the previous state.
    ///
    /// This is not publicly exported because modifier changes happen implicitly
    /// on mouse callbacks, key callbacks, etc.
    ///
    /// The renderer state mutex MUST NOT be held.
    // fn modsChanged(self: *Surface, mods: input.Mods) void {
    //
    // PORT NOTE: the C10 keyCallback (:2720) / C12 mouseButtonCallback
    // (:3774) / C13 cursorPosCallback (:4558) callsites are live.
    pub(super) fn modsChanged(&mut self, mods: Mods) {
        // The only place we keep track of mods currently is on the mouse.
        // if (!self.mouse.mods.equal(mods)) {
        if !self.mouse.mods.equal(mods) {
            // The mouse mods only contain binding modifiers since we don't
            // want caps/num lock or sided modifiers to affect the mouse.
            // self.mouse.mods = mods.binding();
            self.mouse.mods = mods.binding();

            // We also need to update the renderer so it knows if it should
            // highlight links. Additionally, mark the screen as dirty so
            // that the highlight state of all links is properly updated.
            {
                // self.renderer_state.mutex.lock();
                // defer self.renderer_state.mutex.unlock();
                //
                // (guard-less zigstd Mutex — CORE_BLUEPRINT §3; the defer is
                // the explicit unlock at the block end.)
                unsafe { (*(*self.renderer_state.as_ptr()).mutex).lock() };
                // self.renderer_state.mouse.mods = self.mouseModsWithCapture(self.mouse.mods);
                unsafe {
                    (*self.renderer_state.as_ptr()).mouse.mods =
                        self.mouseModsWithCapture(self.mouse.mods)
                };

                // We use the clear screen dirty flag to force a rebuild of all
                // rows because changing mouse mods can affect the highlight state
                // of a link. If there is no link this seems very wasteful but
                // its really only one frame so it's not so bad.
                // self.renderer_state.terminal.flags.dirty.clear = true;
                unsafe {
                    (*(*self.renderer_state.as_ptr()).terminal)
                        .flags
                        .dirty
                        .clear = true
                };

                // (the deferred mutex unlock — block end)
                unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
            }

            // self.queueRender() catch |err| {
            //     // Not a big deal if this fails.
            //     log.warn("failed to notify renderer of mods change err={}", .{err});
            // };
            if let Err(err) = self.queueRender() {
                // Not a big deal if this fails.
                // log.warn("failed to notify renderer of mods change err={}", .{err});
                log::warn!(
                    target: LOG_SCOPE,
                    "failed to notify renderer of mods change err={err:?}"
                );
            }
        }
    }

    /// Call this whenever the mouse moves or mods changed. The time
    /// at which this is called may matter for the correctness of other
    /// mouse events (see cursorPosCallback) but this is shared logic
    /// for multiple events.
    // fn mouseRefreshLinks(
    //     self: *Surface,
    //     pos: apprt.CursorPos,
    //     pos_vp: terminal.point.Coordinate,
    //     over_link: bool,
    // ) !void {
    //
    // PORT NOTE: the C10 keyCallback (:2733) and C13 cursorPosCallback
    // (:4609) callsites are live (both call with the renderer mutex held).
    // The inferred `!void` is the [`Error`] union.
    pub(super) fn mouseRefreshLinks(
        &mut self,
        pos: apprt::CursorPos,
        pos_vp: terminal::point::Coordinate,
        over_link: bool,
    ) -> Result<(), Error> {
        // If the position is outside our viewport, do nothing
        // if (pos.x < 0 or pos.y < 0) return;
        if pos.x < 0.0 || pos.y < 0.0 {
            return Ok(());
        }

        // Update the last point that we checked for links so we don't
        // recheck if the mouse moves some pixels to the same point.
        // self.mouse.link_point = pos_vp;
        self.mouse.link_point = Some(pos_vp);

        // We use an arena for everything below to make things easy to clean up.
        // In the case we don't do any allocs this is very cheap to setup
        // (effectively just struct init).
        // var arena = ArenaAllocator.init(self.alloc);
        // defer arena.deinit();
        // const alloc = arena.allocator();
        //
        // PORT NOTE (collapsed arena): the allocations below are owned values
        // that free on drop (the DerivedConfig arena-collapse precedent).
        let alloc = self.alloc;

        // Get our link at the current position. This returns null if there
        // isn't a link OR if we shouldn't be showing links for some reason
        // (see further comments for cases).
        // const link_: ?apprt.action.MouseOverLink, const preview: bool = link: {
        //
        // PORT NOTE: the Zig multi-value labeled break is a Rust tuple; the
        // `MouseOverLink` payload BORROWS (`url: &[u8]`), so the block yields
        // the OWNED url bytes (the collapsed arena's allocation) and the
        // `.{ .url = ... }` literal is realized at the performAction sites
        // below.
        let (link_, preview): (Option<Vec<u8>>, bool) = 'link: {
            // If we clicked and our mouse moved cells then we never
            // highlight links until the mouse is unclicked. This follows
            // standard macOS and Linux behavior where a click and drag cancels
            // mouse actions.
            // const left_idx = @intFromEnum(input.MouseButton.left);
            let left_idx = input_mouse::Button::Left as usize;
            // if (self.mouse.click_state[left_idx] == .press) click: {
            if self.mouse.click_state[left_idx] == input_mouse::ButtonState::Press {
                'click: {
                    // const pin = self.mouse.activeLeftClickPin(&self.io.terminal.screens) orelse break :click;
                    let pin = match self
                        .mouse
                        .activeLeftClickPin(unsafe { &(*self.io.as_ptr()).terminal.screens })
                    {
                        Some(v) => v,
                        None => break 'click,
                    };
                    // const click_pt = self.io.terminal.screens.active.pages.pointFromPin(
                    //     .viewport,
                    //     pin.*,
                    // ) orelse break :click;
                    let click_pt = match unsafe {
                        (*(*self.io.as_ptr()).terminal.screens.active)
                            .pages
                            .point_from_pin(terminal::point::Tag::Viewport, *pin)
                    } {
                        Some(v) => v,
                        None => break 'click,
                    };

                    // if (!click_pt.coord().eql(pos_vp)) {
                    if !click_pt.coord().eql(pos_vp) {
                        // log.debug("mouse moved while left click held, ignoring link hover", .{});
                        log::debug!(
                            target: LOG_SCOPE,
                            "mouse moved while left click held, ignoring link hover"
                        );
                        // break :link .{ null, false };
                        break 'link (None, false);
                    }
                }
            }

            // const link = (try self.linkAtPos(pos)) orelse break :link .{ null, false };
            let link = match self.linkAtPos(pos)? {
                Some(v) => v,
                None => break 'link (None, false),
            };
            // switch (link.action) {
            match link.action {
                // .open => {
                link::Action::Open => {
                    // const str = try self.io.terminal.screens.active.selectionString(alloc, .{
                    //     .sel = link.selection,
                    //     .trim = false,
                    // });
                    let str_ = unsafe { &mut *(*self.io.as_ptr()).terminal.screens.active }
                        .selection_string(
                            alloc.into(),
                            terminal::screen::SelectionString {
                                sel: link.selection,
                                trim: false,
                            },
                        )?;
                    // break :link .{
                    //     .{ .url = str },
                    //     self.config.link_previews == .true,
                    // };
                    break 'link (
                        Some(str_),
                        self.config.link_previews == configpkg::value_types::LinkPreviews::True,
                    );
                }

                // ._open_osc8 => {
                link::Action::OpenOsc8 => {
                    // Show the URL in the status bar
                    // const pin = link.selection.start();
                    let pin = link.selection.start();
                    // const uri = self.osc8URI(pin) orelse {
                    //     log.warn("failed to get URI for OSC8 hyperlink", .{});
                    //     break :link .{ null, false };
                    // };
                    let uri = match self.osc8URI(pin) {
                        Some(v) => v,
                        None => {
                            // log.warn("failed to get URI for OSC8 hyperlink", .{});
                            log::warn!(target: LOG_SCOPE, "failed to get URI for OSC8 hyperlink");
                            break 'link (None, false);
                        }
                    };
                    // break :link .{
                    //     .{ .url = try alloc.dupeZ(u8, uri) },
                    //     self.config.link_previews != .false,
                    // };
                    //
                    // (`dupeZ` — an owned copy of the raw URI bytes; the
                    // OOM-only `try` collapses.)
                    break 'link (
                        Some(uri.to_vec()),
                        self.config.link_previews != configpkg::value_types::LinkPreviews::False,
                    );
                }
            }
        };

        // If we found a link, setup our internal state and notify the
        // apprt so it can highlight it.
        // if (link_) |link| {
        if let Some(link) = link_ {
            // self.renderer_state.mouse.point = pos_vp;
            unsafe { (*self.renderer_state.as_ptr()).mouse.point = Some(pos_vp) };
            // self.mouse.over_link = true;
            self.mouse.over_link = true;
            // self.renderer_state.terminal.screens.active.dirty.hyperlink_hover = true;
            unsafe {
                (*(*(*self.renderer_state.as_ptr()).terminal).screens.active)
                    .dirty
                    .hyperlink_hover = true;
            }
            // _ = try self.rt_app.performAction(
            //     .{ .surface = self },
            //     .mouse_shape,
            //     .pointer,
            // );
            let _ = unsafe { &*self.rt_app }.perform_action(
                apprt::Target::surface_ref(&*self),
                apprt::Action::MouseShape(terminal::mouse::Shape::Pointer),
            )?;

            // if (preview) {
            if preview {
                // _ = try self.rt_app.performAction(
                //     .{ .surface = self },
                //     .mouse_over_link,
                //     link,
                // );
                let _ = unsafe { &*self.rt_app }.perform_action(
                    apprt::Target::surface_ref(&*self),
                    apprt::Action::MouseOverLink(MouseOverLink { url: &link }),
                )?;
            }

            // try self.queueRender();
            self.queueRender()?;
            // return;
            return Ok(());
        }

        // No link, if we're previously over a link then we need to clear
        // the over-link apprt state.
        // if (over_link) {
        if over_link {
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
            // return;
            return Ok(());
        }

        Ok(())
    }

    /// Called when our renderer health state changes.
    // fn updateRendererHealth(self: *Surface, health: rendererpkg.Health) void {
    fn updateRendererHealth(&mut self, health: rendererpkg::Health) {
        // log.warn("renderer health status change status={}", .{health});
        log::warn!(target: LOG_SCOPE, "renderer health status change status={health:?}");
        // _ = self.rt_app.performAction(
        //     .{ .surface = self },
        //     .renderer_health,
        //     health,
        // ) catch |err| {
        //     log.warn("failed to notify app of renderer health change err={}", .{err});
        // };
        if let Err(err) = unsafe { &*self.rt_app }.perform_action(
            apprt::Target::surface_ref(&*self),
            apprt::Action::RendererHealth(health),
        ) {
            // log.warn("failed to notify app of renderer health change err={}", .{err});
            log::warn!(
                target: LOG_SCOPE,
                "failed to notify app of renderer health change err={err:?}"
            );
        }
    }

    /// Called when the scrollbar state changes.
    // fn updateScrollbar(self: *Surface, scrollbar: terminal.Scrollbar) void {
    fn updateScrollbar(&mut self, scrollbar: terminal::pagelist::grow::Scrollbar) {
        // _ = self.rt_app.performAction(
        //     .{ .surface = self },
        //     .scrollbar,
        //     scrollbar,
        // ) catch |err| {
        //     log.warn("failed to notify app of scrollbar change err={}", .{err});
        // };
        if let Err(err) = unsafe { &*self.rt_app }.perform_action(
            apprt::Target::surface_ref(&*self),
            apprt::Action::Scrollbar(scrollbar),
        ) {
            // log.warn("failed to notify app of scrollbar change err={}", .{err});
            log::warn!(target: LOG_SCOPE, "failed to notify app of scrollbar change err={err:?}");
        }
    }

    /// Called when the render thread has pushed a new frame.
    /// Notifies the apprt to redraw this surface.
    // fn redraw(self: *Surface) void {   (`2f0b65346`, Surface.zig:1743-1753)
    fn redraw(&mut self) {
        // _ = self.rt_app.performAction(
        //     .{ .surface = self },
        //     .render,
        //     {},
        // ) catch |err| {
        //     log.warn("failed to notify app of frame present err={}", .{err});
        // };
        if let Err(err) = unsafe { &*self.rt_app }
            .perform_action(apprt::Target::surface_ref(&*self), apprt::Action::Render)
        {
            // log.warn("failed to notify app of frame present err={}", .{err});
            log::warn!(target: LOG_SCOPE, "failed to notify app of frame present err={err:?}");
        }
    }

    /// This should be called anytime `config_conditional_state` changes
    /// so that the apprt can reload the configuration.
    // fn notifyConfigConditionalState(self: *Surface) void {
    //
    // PORT NOTE: the C13 colorSchemeCallback (:4720) callsite is live.
    //
    // PORT NOTE (N456, ruling 57): raw-self. The host's soft `reload_config`
    // handler re-enters this surface synchronously and writes it: macOS
    // `reloadConfig(surface:soft:)` -> `rustty_surface_update_config` ->
    // `updateConfig`, GTK `reload_config_inner` -> `updateConfig`. So no
    // protected reference to the surface may span `perform_action`; it reads
    // `rt_app` by place and passes `Target::surface_ptr(self_)`. Proof:
    // `.port/phase-2/mirihar-n456`.
    ///
    /// # Safety
    /// `self_` is the live, stable core surface; main thread; no protected
    /// reference to it may span the call.
    pub(super) unsafe fn notifyConfigConditionalState(self_: *mut Self) {
        // _ = self.rt_app.performAction(
        //     .{ .surface = self },
        //     .reload_config,
        //     .{ .soft = true },
        // ) catch |err| {
        //     log.warn("failed to notify app of config state change err={}", .{err});
        // };
        let rt_app: &A = unsafe { &*(*self_).rt_app };
        if let Err(err) = rt_app.perform_action(
            // SAFETY: `self_` is live for the call (this fn's contract).
            unsafe { apprt::Target::surface_ptr(self_) },
            apprt::Action::ReloadConfig(ReloadConfig { soft: true }),
        ) {
            // log.warn("failed to notify app of config state change err={}", .{err});
            log::warn!(
                target: LOG_SCOPE,
                "failed to notify app of config state change err={err:?}"
            );
        }
    }

    /// Update our configuration at runtime. This can be called by the apprt
    /// to set a surface-specific configuration that differs from the app
    /// or other surfaces.
    // pub fn updateConfig(
    //     self: *Surface,
    //     original: *const configpkg.Config,
    // ) !void {
    pub fn updateConfig(&mut self, original: &configpkg::Config) -> Result<(), Error> {
        // Apply our conditional state. If we fail to apply the conditional state
        // then we log and attempt to move forward with the old config.
        // var config_: ?configpkg.Config = original.changeConditionalState(
        //     self.config_conditional_state,
        // ) catch |err| err: {
        //     log.warn("failed to apply conditional state to config err={}", .{err});
        //     break :err null;
        // };
        //
        // PARITY: not `.unwrap_or_default()` — the arm IS the reference's
        // labeled catch block with its log site (the init precedent).
        #[allow(clippy::manual_unwrap_or_default)]
        let mut config_: Option<configpkg::Config> =
            match original.change_conditional_state(self.config_conditional_state) {
                Ok(v) => v,
                Err(err) => {
                    // log.warn("failed to apply conditional state to config err={}", .{err});
                    log::warn!(
                        target: LOG_SCOPE,
                        "failed to apply conditional state to config err={err:?}"
                    );
                    None
                }
            };
        // defer if (config_) |*c| c.deinit();
        //
        // PORT NOTE: fired explicitly on the success path at the end of this
        // fn; every early-return path maps to `config_`'s Drop (the init
        // precedent).

        // We want a config pointer for everything so we get that either
        // based on our conditional state or the original config.
        // const config: *const configpkg.Config = if (config_) |*c| c else original;
        let config: &configpkg::Config = match config_.as_ref() {
            Some(c) => c,
            None => original,
        };

        // Update our new derived config immediately
        // const derived = DerivedConfig.init(self.alloc, config) catch |err| {
        //     // If the derivation fails then we just log and return. We don't
        //     // hard fail in this case because we don't want to error the surface
        //     // when config fails we just want to keep using the old config.
        //     log.err("error updating configuration err={}", .{err});
        //     return;
        // };
        let derived = match DerivedConfig::init(self.alloc, config) {
            Ok(v) => v,
            Err(err) => {
                // If the derivation fails then we just log and return. We don't
                // hard fail in this case because we don't want to error the surface
                // when config fails we just want to keep using the old config.
                // log.err("error updating configuration err={}", .{err});
                log::error!(target: LOG_SCOPE, "error updating configuration err={err:?}");
                return Ok(());
            }
        };
        // self.config.deinit();
        self.config.deinit();
        // self.config = derived;
        //
        // (the assignment drops the deinit-ed old value — the initErrdefers
        // deinit-then-drop precedent.)
        self.config = derived;

        // If our mouse is hidden but we disabled mouse hiding, then show it again.
        // if (!self.config.mouse_hide_while_typing and self.mouse.hidden) {
        if !self.config.mouse_hide_while_typing && self.mouse.hidden {
            // self.showMouse();
            self.showMouse();
        }

        // If we are in the middle of a key sequence, clear it.
        // self.endKeySequence(.drop, .free);
        self.endKeySequence(KeySequenceQueued::Drop, KeySequenceMemory::Free);

        // Deactivate all key tables since they may have changed. Importantly,
        // we store pointers into the config as part of our table stack so
        // we can't keep them active across config changes. But this behavior
        // also matches key sequences.
        // _ = self.deactivateAllKeyTables() catch |err| {
        //     log.warn("failed to deactivate key tables err={}", .{err});
        // };
        if let Err(err) = self.deactivateAllKeyTables() {
            // log.warn("failed to deactivate key tables err={}", .{err});
            log::warn!(target: LOG_SCOPE, "failed to deactivate key tables err={err:?}");
        }

        // Before sending any other config changes, we give the renderer a new font
        // grid. We could check to see if there was an actual change to the font,
        // but this is easier and pretty rare so it's not a performance concern.
        //
        // (Calling setFontSize builds and sends a new font grid to the renderer.)
        // try self.setFontSize(font_size: {
        self.setFontSize('font_size: {
            // If we have manually adjusted the font size, keep it that way.
            // if (self.font_size_adjusted) {
            if self.font_size_adjusted {
                // log.info("font size manually adjusted, preserving previous size on config reload", .{});
                log::info!(
                    target: LOG_SCOPE,
                    "font size manually adjusted, preserving previous size on config reload"
                );
                // break :font_size self.font_size;
                break 'font_size self.font_size;
            }

            // If we haven't, then we update to the configured font size.
            // This allows config changes to update the font size. We used to
            // never do this but it was a common source of confusion and people
            // assumed that Rustty was broken! This logic makes more sense.
            // var size = self.font_size;
            let mut size = self.font_size;
            // size.points = std.math.clamp(config.@"font-size", 1.0, 255.0);
            size.points = config.font_size.clamp(1.0, 255.0);
            // break :font_size size;
            size
        })?;

        // We need to store our configs in a heap-allocated pointer so that
        // our messages aren't huge.
        // var renderer_message = try rendererpkg.Message.initChangeConfig(self.alloc, config);
        let renderer_message = rendererpkg::Message::initChangeConfig(self.alloc, config)?;
        // errdefer renderer_message.deinit();
        // var termio_config_ptr = try self.alloc.create(termio.Termio.DerivedConfig);
        // errdefer self.alloc.destroy(termio_config_ptr);
        // termio_config_ptr.* = try termio.Termio.DerivedConfig.init(self.alloc, config);
        // errdefer termio_config_ptr.deinit();
        //
        // PORT NOTE: `alloc.create` + the out-write collapse into `Box::new`
        // and the ported `termio::DerivedConfig::init` is infallible (its
        // PORT NOTE), so no fallible site remains inside the errdefers' live
        // ranges — all three collapse into ownership (message/Box drops).
        let termio_config_ptr: Box<termio::DerivedConfig> =
            Box::new(termio::DerivedConfig::init(config));

        // _ = self.renderer_thread.mailbox.push(renderer_message, .{ .forever = {} });
        let _ = unsafe { &*(*self.renderer_thread.as_ptr()).mailbox }
            .push(renderer_message, Timeout::Forever);
        // self.queueIo(.{
        //     .change_config = .{
        //         .alloc = self.alloc,
        //         .ptr = termio_config_ptr,
        //     },
        // }, .unlocked);
        //
        // (the `.alloc` field collapses into the `ChangeConfig` Box ownership
        // — the termio message.rs PORT NOTE.)
        self.queueIo(
            termio::Message::ChangeConfig {
                ptr: termio_config_ptr,
            },
            termio::termio_impl::MutexState::Unlocked,
        );

        // With mailbox messages sent, we have to wake them up so they process it.
        // self.queueRender() catch |err| {
        //     log.warn("failed to notify renderer of config change err={}", .{err});
        // };
        if let Err(err) = self.queueRender() {
            // log.warn("failed to notify renderer of config change err={}", .{err});
            log::warn!(target: LOG_SCOPE, "failed to notify renderer of config change err={err:?}");
        }

        // If we have a title set then we update our window to have the
        // newly configured title.
        // if (config.title) |title| _ = try self.rt_app.performAction(
        //     .{ .surface = self },
        //     .set_title,
        //     .{ .title = title },
        // );
        if let Some(title) = &config.title {
            let _ = unsafe { &*self.rt_app }.perform_action(
                apprt::Target::surface_ref(&*self),
                apprt::Action::SetTitle(SetTitle { title }),
            )?;
        }

        // Notify the window
        // _ = try self.rt_app.performAction(
        //     .{ .surface = self },
        //     .config_change,
        //     .{ .config = config },
        // );
        let _ = unsafe { &*self.rt_app }.perform_action(
            apprt::Target::surface_ref(&*self),
            apprt::Action::ConfigChange(ConfigChange { config }),
        )?;

        // (the reference `defer if (config_) |*c| c.deinit();` — success
        // path; see the PORT NOTE at the defer site above.)
        if let Some(c) = config_.as_mut() {
            c.deinit();
        }

        Ok(())
    }

    /// Recalculate the initial size of the window based on the
    /// configuration and invoke the apprt `initial_size` action if
    /// necessary.
    // fn recomputeInitialSize(
    //     self: *Surface,
    // ) InitialSizeError!void {
    pub(super) fn recomputeInitialSize(&mut self) -> Result<(), InitialSizeError> {
        // Both width and height must be set for this to work, as
        // documented on the config options.
        // if (self.config.window_height <= 0 or
        //     self.config.window_width <= 0) return;
        //
        // (`<= 0` on the unsigned reference fields is `== 0`.)
        if self.config.window_height == 0 || self.config.window_width == 0 {
            return Ok(());
        }

        // const scale = self.rt_surface.getContentScale() catch
        //     return error.ContentScaleUnavailable;
        let scale = match unsafe { &*self.rt_surface }.get_content_scale() {
            Ok(v) => v,
            Err(_) => return Err(InitialSizeError::ContentScaleUnavailable),
        };
        // const height = @max(
        //     self.config.window_height,
        //     min_window_height_cells,
        // ) * self.size.cell.height;
        // const width = @max(
        //     self.config.window_width,
        //     min_window_width_cells,
        // ) * self.size.cell.width;
        // const width_f32: f32 = @floatFromInt(width);
        // const height_f32: f32 = @floatFromInt(height);
        //
        // The final values are affected by content scale and we need to
        // account for the padding so we get the exact correct grid size.
        // const final_width: u32 =
        //     @as(u32, @intFromFloat(@ceil(width_f32 / scale.x))) +
        //     self.size.padding.left +
        //     self.size.padding.right;
        // const final_height: u32 =
        //     @as(u32, @intFromFloat(@ceil(height_f32 / scale.y))) +
        //     self.size.padding.top +
        //     self.size.padding.bottom;
        //
        // PORT NOTE: reference defect (R13, UPSTREAM-DEFECTS.md UD59). The
        // computation above is in `initial_size_px`, checked. On overflow the
        // port returns here, the function's existing skip path (the one it
        // takes when either option is unset), so no `initial_size` is sent.
        let Some((final_width, final_height)) = initial_size_px(
            self.config.window_width,
            self.config.window_height,
            self.size,
            scale,
        ) else {
            return Ok(());
        };

        // _ = self.rt_app.performAction(
        //     .{ .surface = self },
        //     .initial_size,
        //     .{ .width = final_width, .height = final_height },
        // ) catch return error.AppActionFailed;
        match unsafe { &*self.rt_app }.perform_action(
            apprt::Target::surface_ref(&*self),
            apprt::Action::InitialSize(InitialSize {
                width: final_width,
                height: final_height,
            }),
        ) {
            Ok(_) => {}
            Err(_) => return Err(InitialSizeError::AppActionFailed),
        }

        Ok(())
    }

    /// Handle a request from the running program (CSI 8 t) to resize the
    /// window to the given grid size.
    // fn resizeWindow(
    //     self: *Surface,
    //     size: terminal.StreamAction.ResizeWindow,
    // ) !void {
    //
    // PORT NOTE (N456): raw-self. The surface is read in short reborrows that
    // end before `perform_action`, whose host callback resizes (so writes)
    // this surface on macOS; the callback gets only the `Target`, a raw pointer
    // with no protector (`Target::surface_ptr(self_)`, P7-W6 T3). Proof:
    // `.port/phase-2/mirihar-n456`.
    ///
    /// # Safety
    /// As [`Surface::handleMessage`].
    unsafe fn resizeWindow(
        self_: *mut Self,
        size: terminal::stream::ResizeWindow,
    ) -> Result<(), Error> {
        // if (!self.config.vt_window_resize_allowed) {
        if !unsafe { (*self_).config.vt_window_resize_allowed } {
            // log.info("resize_window requested, but disabled via config", .{});
            log::info!(
                target: LOG_SCOPE,
                "resize_window requested, but disabled via config"
            );
            // return;
            return Ok(());
        }

        // Our cell size and padding are in pixels but the apprt expects
        // points. We use the configured padding rather than the current
        // padding since balanced padding depends on the size.
        // const scale = try self.rt_surface.getContentScale();
        let scale = unsafe { &*(*self_).rt_surface }.get_content_scale()?;
        // const padding = self.config.scaledPadding(
        //     scale.x * font.face.default_dpi,
        //     scale.y * font.face.default_dpi,
        // );
        let padding = unsafe { &(*self_).config }.scaledPadding(
            scale.x * f32::from(face::DEFAULT_DPI),
            scale.y * f32::from(face::DEFAULT_DPI),
        );
        // const width: f32 = @floatFromInt(@max(size.columns, min_resize_width_cells) *
        //     self.size.cell.width + padding.left + padding.right);
        // const height: f32 = @floatFromInt(@max(size.rows, min_resize_height_cells) *
        //     self.size.cell.height + padding.top + padding.bottom);
        //
        // PORT NOTE: reference defect (R13, UD371): the arithmetic, and the
        // conversion below, are `resize_window_pts`, checked. On overflow no
        // action is sent (UD59's skip).
        let cell = unsafe { (*self_).size.cell };
        let Some((width, height)) = resize_window_pts(size, cell, padding, scale) else {
            log::warn!(
                target: LOG_SCOPE,
                "resize_window requested, but the size does not fit a u32, ignoring"
            );
            return Ok(());
        };

        // A zero dimension is passed through so the apprt keeps it as is.
        // _ = try self.rt_app.performAction(
        //     .{ .surface = self },
        //     .resize_window,
        //     .{
        //         .width = if (size.columns > 0) @intFromFloat(@ceil(width / scale.x)) else 0,
        //         .height = if (size.rows > 0) @intFromFloat(@ceil(height / scale.y)) else 0,
        //     },
        // );
        let rt_app: &A = unsafe { &*(*self_).rt_app };
        let _ = rt_app.perform_action(
            unsafe { apprt::Target::surface_ptr(self_) },
            apprt::Action::ResizeWindow(ResizeWindow { width, height }),
        )?;
        Ok(())
    }
}
