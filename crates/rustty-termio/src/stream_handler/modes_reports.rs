//! PORTS: ghostty src/termio/stream_handler.zig — the `StreamHandler` control-character, mode and report
//! methods (`bell` through `fullReset`, `queryKittyKeyboard`, `reportXtversion`).

use super::*;

impl<M: AppMailbox> StreamHandler<M> {
    // inline fn bell(self: *StreamHandler) void {
    #[inline]
    pub(super) fn bell(&mut self) {
        // self.surfaceMessageWriter(.ring_bell);
        self.surfaceMessageWriter(SurfaceMessage::RingBell);
    }

    // inline fn horizontalTab(self: *StreamHandler, count: u16) void {
    #[inline]
    pub(super) fn horizontalTab(&mut self, count: u16) {
        // for (0..count) |_| {
        for _ in 0..count {
            // const x = self.terminal.screens.active.cursor.x;
            let x = self.term().active_mut().cursor.x;
            // self.terminal.horizontalTab();
            self.term().horizontal_tab();
            // if (x == self.terminal.screens.active.cursor.x) break;
            if x == self.term().active_mut().cursor.x {
                break;
            }
        }
    }

    // inline fn horizontalTabBack(self: *StreamHandler, count: u16) void {
    #[inline]
    pub(super) fn horizontalTabBack(&mut self, count: u16) {
        // for (0..count) |_| {
        for _ in 0..count {
            // const x = self.terminal.screens.active.cursor.x;
            let x = self.term().active_mut().cursor.x;
            // self.terminal.horizontalTabBack();
            self.term().horizontal_tab_back();
            // if (x == self.terminal.screens.active.cursor.x) break;
            if x == self.term().active_mut().cursor.x {
                break;
            }
        }
    }

    // inline fn linefeed(self: *StreamHandler) !void {
    #[inline]
    pub(super) fn linefeed(&mut self) -> Result<(), crate::Error> {
        // Small optimization: call index instead of linefeed because they're
        // identical and this avoids one layer of function call overhead.
        // try self.terminal.index();
        self.term().index().map_err(|_| crate::Error)
    }

    // pub inline fn reverseIndex(self: *StreamHandler) !void {
    #[inline]
    pub fn reverseIndex(&mut self) -> Result<(), crate::Error> {
        // self.terminal.reverseIndex();
        self.term().reverse_index();
        Ok(())
    }

    // pub inline fn index(self: *StreamHandler) !void {
    #[inline]
    pub fn index(&mut self) -> Result<(), crate::Error> {
        // try self.terminal.index();
        self.term().index().map_err(|_| crate::Error)
    }

    // pub inline fn nextLine(self: *StreamHandler) !void {
    #[inline]
    pub fn nextLine(&mut self) -> Result<(), crate::Error> {
        // try self.terminal.index();
        self.term().index().map_err(|_| crate::Error)?;
        // self.terminal.carriageReturn();
        self.term().carriage_return();
        Ok(())
    }

    // pub fn setModifyKeyFormat(self: *StreamHandler, format: terminal.ModifyKeyFormat) !void {
    pub fn setModifyKeyFormat(
        &mut self,
        format: ansi::ModifyKeyFormat,
    ) -> Result<(), crate::Error> {
        // self.terminal.flags.modify_other_keys_2 = false;
        self.term().flags.modify_other_keys_2 = false;
        // switch (format) {
        //     .other_keys_numeric => self.terminal.flags.modify_other_keys_2 = true,
        //     else => {},
        // }
        match format {
            ansi::ModifyKeyFormat::OtherKeysNumeric => {
                self.term().flags.modify_other_keys_2 = true;
            }
            _ => {}
        }
        Ok(())
    }

    // fn requestMode(self: *StreamHandler, mode: terminal.Mode) !void {
    pub(super) fn requestMode(&mut self, mode: modes::Mode) -> Result<(), crate::Error> {
        // self.sendModeReport(self.terminal.modes.getReport(.fromMode(mode)));
        let report = self
            .term()
            .modes
            .get_report(modes::ReportTag::from_mode(mode));
        self.sendModeReport(report);
        Ok(())
    }

    // fn requestModeUnknown(self: *StreamHandler, mode_raw: u16, ansi: bool) !void {
    pub(super) fn requestModeUnknown(
        &mut self,
        mode_raw: u16,
        ansi_: bool,
    ) -> Result<(), crate::Error> {
        // self.sendModeReport(self.terminal.modes.getReport(.{ .value = mode_raw, .ansi = ansi }));
        let report = self.term().modes.get_report(modes::ReportTag {
            value: mode_raw,
            ansi: ansi_,
        });
        self.sendModeReport(report);
        Ok(())
    }

    // fn sendModeReport(self: *StreamHandler, report: terminal.modes.Report) void {
    fn sendModeReport(&mut self, report: modes::Report) {
        // var data: termio.Message.WriteReq.Small.Array = undefined;
        // var writer: std.Io.Writer = .fixed(&data);
        let mut data = [0u8; 38];
        let mut writer = FixedFmt {
            buf: &mut data,
            pos: 0,
        };
        // report.encode(&writer) catch |err| {
        //     log.err("error encoding mode report err={}", .{err});
        //     return;
        // };
        if let Err(err) = report.encode(&mut writer) {
            // log.err("error encoding mode report err={}", .{err});
            log::error!(target: LOG_SCOPE, "error encoding mode report err={err:?}");
            return;
        }
        let len = writer.pos;
        // self.messageWriter(.{ .write_small = .{
        //     .data = data,
        //     .len = @intCast(writer.buffered().len),
        // } });
        self.messageWriter(Message::WriteSmall(
            rustty_datastruct::message_data::Small { data, len },
        ));
    }

    // pub fn setMode(self: *StreamHandler, mode: terminal.Mode, enabled: bool) !void {
    pub fn setMode(&mut self, mode: modes::Mode, enabled: bool) -> Result<(), crate::Error> {
        // Note: this function doesn't need to grab the render state or
        // terminal locks because it is only called from process() which
        // grabs the lock.

        // If we are setting cursor blinking, we ignore it if we have
        // a default cursor blink setting set. This is a really weird
        // behavior so this comment will go deep into trying to explain it.
        //
        // There are two ways to set cursor blinks: DECSCUSR (CSI _ q)
        // and DEC mode 12. DECSCUSR is the modern approach and has a
        // way to revert to the "default" (as defined by the terminal)
        // cursor style and blink by doing "CSI 0 q". DEC mode 12 controls
        // blinking and is either on or off and has no way to set a
        // default. DEC mode 12 is also the more antiquated approach.
        //
        // The problem is that if the user specifies a desired default
        // cursor blink with `cursor-style-blink`, the moment a running
        // program uses DEC mode 12, the cursor blink can never be reset
        // to the default without an explicit DECSCUSR. But if a program
        // is using mode 12, it is by definition not using DECSCUSR.
        // This makes for somewhat annoying interactions where a poorly
        // (or legacy) behaved program will stop blinking, and it simply
        // never restarts.
        //
        // To get around this, we have a special case where if the user
        // specifies some explicit default cursor blink desire, we ignore
        // DEC mode 12. We allow DECSCUSR to still set the cursor blink
        // because programs using DECSCUSR usually are well behaved and
        // reset the cursor blink to the default when they exit.
        //
        // To be extra safe, users can also add a manual `CSI 0 q` to
        // their shell config when they render prompts to ensure the
        // cursor is exactly as they request.
        // if (mode == .cursor_blinking and
        //     self.terminal.cursor.default_blink != null)
        // {
        //     return;
        // }
        //
        // PORT NOTE (oracle c594031d5): this reads the field that MOVED onto
        // `Terminal`. Forgetting to re-point it here silently breaks the
        // "shell set an explicit blink, ignore mode 12 changes" rule; guarded
        // by `stream_handler_mode_12_ignored_when_default_blink_is_explicit`.
        if mode == modes::Mode::CursorBlinking && self.term().cursor.default_blink.is_some() {
            return Ok(());
        }

        // We first always set the raw mode on our mode state.
        // self.terminal.modes.set(mode, enabled);
        self.term().modes.set(mode, enabled);

        // And then some modes require additional processing.
        // switch (mode) {
        match mode {
            // Just noting here that autorepeat has no effect on
            // the terminal. xterm ignores this mode and so do we.
            // We know about just so that we don't log that it is
            // an unknown mode.
            // .autorepeat => {},
            modes::Mode::Autorepeat => {}

            // Schedule a render since we changed colors
            // .reverse_colors => self.terminal.flags.dirty.reverse_colors = true,
            modes::Mode::ReverseColors => {
                self.term().flags.dirty.reverse_colors = true;
            }

            // Origin resets cursor pos. This is called whether or not
            // we're enabling or disabling origin mode and whether or
            // not the value changed.
            // .origin => self.terminal.setCursorPos(1, 1),
            modes::Mode::Origin => self.term().set_cursor_pos(1, 1),

            // .enable_left_and_right_margin => if (!enabled) {
            modes::Mode::EnableLeftAndRightMargin => {
                if !enabled {
                    // When we disable left/right margin mode we need to
                    // reset the left/right margins.
                    // self.terminal.scrolling_region.left = 0;
                    self.term().scrolling_region.left = 0;
                    // self.terminal.scrolling_region.right = self.terminal.cols - 1;
                    let right = self.term().cols - 1;
                    self.term().scrolling_region.right = right;
                }
            }

            // .alt_screen_legacy => {
            //     try self.terminal.switchScreenMode(.@"47", enabled);
            // },
            modes::Mode::AltScreenLegacy => {
                self.term()
                    .switch_screen_mode(rustty_vt::terminal::SwitchScreenMode::M47, enabled)
                    .map_err(|_| crate::Error)?;
            }

            // .alt_screen => {
            //     try self.terminal.switchScreenMode(.@"1047", enabled);
            // },
            modes::Mode::AltScreen => {
                self.term()
                    .switch_screen_mode(rustty_vt::terminal::SwitchScreenMode::M1047, enabled)
                    .map_err(|_| crate::Error)?;
            }

            // .alt_screen_save_cursor_clear_enter => {
            //     try self.terminal.switchScreenMode(.@"1049", enabled);
            // },
            modes::Mode::AltScreenSaveCursorClearEnter => {
                self.term()
                    .switch_screen_mode(rustty_vt::terminal::SwitchScreenMode::M1049, enabled)
                    .map_err(|_| crate::Error)?;
            }

            // Mode 1048 is xterm's conditional save cursor depending
            // on if alt screen is enabled or not (at the terminal emulator
            // level). Alt screen is always enabled for us so this just
            // does a save/restore cursor.
            // .save_cursor => {
            modes::Mode::SaveCursor => {
                if enabled {
                    self.term().save_cursor();
                } else {
                    self.term().restore_cursor();
                }
            }

            // Force resize back to the window size
            // .enable_mode_3 => {
            modes::Mode::EnableMode3 => {
                // const grid_size = self.size.grid();
                let grid_size = unsafe { *self.size }.grid();
                // self.terminal.resize(
                //     self.alloc,
                //     .{
                //         .cols = grid_size.columns,
                //         .rows = grid_size.rows,
                //     },
                // ) catch |err| {
                //     log.err("error updating terminal size: {}", .{err});
                // };
                //
                // PORT NOTE (oracle 89b103dd5): no cell size here — this path
                // only restores the grid dimensions, so the terminal's existing
                // pixel geometry is left alone.
                let alloc = self.alloc;
                if let Err(err) = self.term().resize(
                    alloc.into(),
                    rustty_vt::terminal::Resize {
                        cols: grid_size.columns,
                        rows: grid_size.rows,
                        cell_size_px: None,
                    },
                ) {
                    // log.err("error updating terminal size: {}", .{err});
                    log::error!(target: LOG_SCOPE, "error updating terminal size: {err:?}");
                }
            }

            // .@"132_column" => try self.terminal.deccolm(
            //     self.alloc,
            //     if (enabled) .@"132_cols" else .@"80_cols",
            // ),
            modes::Mode::Column132 => {
                let alloc = self.alloc;
                self.term()
                    .deccolm(
                        alloc.into(),
                        if enabled {
                            rustty_vt::terminal::DeccolmMode::Cols132
                        } else {
                            rustty_vt::terminal::DeccolmMode::Cols80
                        },
                    )
                    .map_err(|_| crate::Error)?;
            }

            // We need to start a timer to prevent the emulator being hung
            // forever.
            // .synchronized_output => {
            //     if (enabled) self.messageWriter(.{ .start_synchronized_output = {} });
            // },
            modes::Mode::SynchronizedOutput => {
                if enabled {
                    self.messageWriter(Message::StartSynchronizedOutput);
                }
            }

            // .linefeed => {
            //     self.messageWriter(.{ .linefeed_mode = enabled });
            // },
            modes::Mode::Linefeed => {
                self.messageWriter(Message::LinefeedMode(enabled));
            }

            // .in_band_size_reports => if (enabled) self.messageWriter(.{
            //     .size_report = .mode_2048,
            // }),
            modes::Mode::InBandSizeReports => {
                if enabled {
                    self.messageWriter(Message::SizeReport(
                        rustty_vt::size_report::Style::Mode2048,
                    ));
                }
            }

            // .report_visibility => if (enabled) self.messageWriter(.{
            //     .visibility_report = .{
            //         .visible = self.terminal.flags.visible,
            //         .force = true,
            //     },
            // }),
            modes::Mode::ReportVisibility => {
                if enabled {
                    let visible = self.term().flags.visible;
                    self.messageWriter(Message::VisibilityReport {
                        visible,
                        force: true,
                    });
                }
            }

            // .focus_event => if (enabled) self.messageWriter(.{
            //     .focused = self.terminal.flags.focused,
            // }),
            modes::Mode::FocusEvent => {
                if enabled {
                    let focused = self.term().flags.focused;
                    self.messageWriter(Message::Focused(focused));
                }
            }

            // .mouse_event_x10 => {
            modes::Mode::MouseEventX10 => {
                if enabled {
                    self.term().flags.mouse_event = rustty_vt::mouse::Event::X10;
                    self.setMouseShape(rustty_vt::mouse::Shape::Default)?;
                } else {
                    self.term().flags.mouse_event = rustty_vt::mouse::Event::None;
                    self.setMouseShape(rustty_vt::mouse::Shape::Text)?;
                }
            }
            // .mouse_event_normal => {
            modes::Mode::MouseEventNormal => {
                if enabled {
                    self.term().flags.mouse_event = rustty_vt::mouse::Event::Normal;
                    self.setMouseShape(rustty_vt::mouse::Shape::Default)?;
                } else {
                    self.term().flags.mouse_event = rustty_vt::mouse::Event::None;
                    self.setMouseShape(rustty_vt::mouse::Shape::Text)?;
                }
            }
            // .mouse_event_button => {
            modes::Mode::MouseEventButton => {
                if enabled {
                    self.term().flags.mouse_event = rustty_vt::mouse::Event::Button;
                    self.setMouseShape(rustty_vt::mouse::Shape::Default)?;
                } else {
                    self.term().flags.mouse_event = rustty_vt::mouse::Event::None;
                    self.setMouseShape(rustty_vt::mouse::Shape::Text)?;
                }
            }
            // .mouse_event_any => {
            modes::Mode::MouseEventAny => {
                if enabled {
                    self.term().flags.mouse_event = rustty_vt::mouse::Event::Any;
                    self.setMouseShape(rustty_vt::mouse::Shape::Default)?;
                } else {
                    self.term().flags.mouse_event = rustty_vt::mouse::Event::None;
                    self.setMouseShape(rustty_vt::mouse::Shape::Text)?;
                }
            }

            // .mouse_format_utf8 => self.terminal.flags.mouse_format = if (enabled) .utf8 else .x10,
            modes::Mode::MouseFormatUtf8 => {
                self.term().flags.mouse_format = if enabled {
                    rustty_vt::mouse::Format::Utf8
                } else {
                    rustty_vt::mouse::Format::X10
                };
            }
            // .mouse_format_sgr => self.terminal.flags.mouse_format = if (enabled) .sgr else .x10,
            modes::Mode::MouseFormatSgr => {
                self.term().flags.mouse_format = if enabled {
                    rustty_vt::mouse::Format::Sgr
                } else {
                    rustty_vt::mouse::Format::X10
                };
            }
            // .mouse_format_urxvt => self.terminal.flags.mouse_format = if (enabled) .urxvt else .x10,
            modes::Mode::MouseFormatUrxvt => {
                self.term().flags.mouse_format = if enabled {
                    rustty_vt::mouse::Format::Urxvt
                } else {
                    rustty_vt::mouse::Format::X10
                };
            }
            // .mouse_format_sgr_pixels => self.terminal.flags.mouse_format = if (enabled) .sgr_pixels else .x10,
            modes::Mode::MouseFormatSgrPixels => {
                self.term().flags.mouse_format = if enabled {
                    rustty_vt::mouse::Format::SgrPixels
                } else {
                    rustty_vt::mouse::Format::X10
                };
            }

            // else => {},
            _ => {}
        }

        Ok(())
    }

    // inline fn startHyperlink(self: *StreamHandler, uri: []const u8, id: ?[]const u8) !void {
    #[inline]
    pub(super) fn startHyperlink(
        &mut self,
        uri: &[u8],
        id: Option<&[u8]>,
    ) -> Result<(), crate::Error> {
        // try self.terminal.screens.active.startHyperlink(uri, id);
        self.term()
            .active_mut()
            .start_hyperlink(uri, id)
            .map_err(|_| crate::Error)
    }

    // pub inline fn endHyperlink(self: *StreamHandler) !void {
    #[inline]
    pub fn endHyperlink(&mut self) -> Result<(), crate::Error> {
        // self.terminal.screens.active.endHyperlink();
        self.term().active_mut().end_hyperlink();
        Ok(())
    }

    // pub fn deviceAttributes(
    //     self: *StreamHandler,
    //     req: terminal.DeviceAttributeReq,
    // ) !void {
    pub fn deviceAttributes(&mut self, req: device_attributes::Req) -> Result<(), crate::Error> {
        // For the below, we quack as a VT220. We don't quack as
        // a 420 because we don't support DCS sequences.
        match req {
            // .primary => self.messageWriter(.{
            //     // 62 = Level 2 conformance
            //     // 22 = Color text
            //     // 52 = Clipboard access
            //     .write_stable = if (self.clipboard_write != .deny)
            //         "\x1B[?62;22;52c"
            //     else
            //         "\x1B[?62;22c",
            // }),
            device_attributes::Req::Primary => self.messageWriter(Message::WriteStable(
                // 62 = Level 2 conformance
                // 22 = Color text
                // 52 = Clipboard access
                if self.clipboard_write != ClipboardAccess::Deny {
                    b"\x1B[?62;22;52c"
                } else {
                    b"\x1B[?62;22c"
                },
            )),

            // .secondary => self.messageWriter(.{
            //     .write_stable = "\x1B[>1;10;0c",
            // }),
            device_attributes::Req::Secondary => {
                self.messageWriter(Message::WriteStable(b"\x1B[>1;10;0c"));
            }

            // else => log.warn("unimplemented device attributes req: {}", .{req}),
            _ => {
                // log.warn("unimplemented device attributes req: {}", .{req});
                log::warn!(target: LOG_SCOPE, "unimplemented device attributes req: {req:?}");
            }
        }
        Ok(())
    }

    // pub fn deviceStatusReport(
    //     self: *StreamHandler,
    //     req: terminal.device_status.Request,
    // ) !void {
    pub fn deviceStatusReport(&mut self, req: device_status::Request) -> Result<(), crate::Error> {
        match req {
            // .operating_status => self.messageWriter(.{ .write_stable = "\x1B[0n" }),
            device_status::Request::OperatingStatus => {
                self.messageWriter(Message::WriteStable(b"\x1B[0n"));
            }

            // .cursor_position => {
            device_status::Request::CursorPosition => {
                // const pos: struct {
                //     x: usize,
                //     y: usize,
                // } = if (self.terminal.modes.get(.origin)) .{
                //     .x = self.terminal.screens.active.cursor.x -| self.terminal.scrolling_region.left,
                //     .y = self.terminal.screens.active.cursor.y -| self.terminal.scrolling_region.top,
                // } else .{
                //     .x = self.terminal.screens.active.cursor.x,
                //     .y = self.terminal.screens.active.cursor.y,
                // };
                let (x, y): (usize, usize) = if self.term().modes.get(modes::Mode::Origin) {
                    let cx = self.term().active_mut().cursor.x;
                    let cy = self.term().active_mut().cursor.y;
                    let left = self.term().scrolling_region.left;
                    let top = self.term().scrolling_region.top;
                    (
                        cx.saturating_sub(left) as usize,
                        cy.saturating_sub(top) as usize,
                    )
                } else {
                    (
                        self.term().active_mut().cursor.x as usize,
                        self.term().active_mut().cursor.y as usize,
                    )
                };

                // Response always is at least 4 chars, so this leaves the
                // remainder for the row/column as base-10 numbers. This
                // will support a very large terminal.
                // var msg: termio.Message = .{ .write_small = .{} };
                // const resp = try std.fmt.bufPrint(&msg.write_small.data, "\x1B[{};{}R", .{
                //     pos.y + 1,
                //     pos.x + 1,
                // });
                // msg.write_small.len = @intCast(resp.len);
                let mut data = [0u8; 38];
                let len = write_fixed(&mut data[..], format_args!("\x1B[{};{}R", y + 1, x + 1))?;
                let msg = Message::WriteSmall(rustty_datastruct::message_data::Small { data, len });

                // self.messageWriter(msg);
                self.messageWriter(msg);
            }

            // .color_scheme => self.messageWriter(.{ .color_scheme_report = .{ .force = true } }),
            device_status::Request::ColorScheme => {
                self.messageWriter(Message::ColorSchemeReport { force: true });
            }

            // .visibility => self.messageWriter(.{ .visibility_report = .{
            //     .visible = self.terminal.flags.visible,
            //     .force = true,
            // } }),
            device_status::Request::Visibility => {
                let visible = self.term().flags.visible;
                self.messageWriter(Message::VisibilityReport {
                    visible,
                    force: true,
                });
            }
        }
        Ok(())
    }

    // pub fn setCursorStyle(...) !void { ... }
    //
    // PORT NOTE (oracle c594031d5): DELETED. The whole style/blink resolution
    // moved onto `Terminal::set_cursor_style`; keeping a copy here would
    // double-apply DECSCUSR and re-introduce a second owner of the defaults.

    // pub inline fn decaln(self: *StreamHandler) !void {
    #[inline]
    pub fn decaln(&mut self) -> Result<(), crate::Error> {
        // try self.terminal.decaln();
        self.term().decaln().map_err(|_| crate::Error)
    }

    // pub inline fn saveCursor(self: *StreamHandler) !void {
    #[inline]
    pub fn saveCursor(&mut self) -> Result<(), crate::Error> {
        // self.terminal.saveCursor();
        self.term().save_cursor();
        Ok(())
    }

    // pub inline fn restoreCursor(self: *StreamHandler) !void {
    #[inline]
    pub fn restoreCursor(&mut self) -> Result<(), crate::Error> {
        // self.terminal.restoreCursor();
        self.term().restore_cursor();
        Ok(())
    }

    // pub fn enquiry(self: *StreamHandler) !void {
    pub fn enquiry(&mut self) -> Result<(), crate::Error> {
        // log.debug("sending enquiry response={s}", .{self.enquiry_response});
        log::debug!(
            target: LOG_SCOPE,
            "sending enquiry response={}",
            String::from_utf8_lossy(&self.enquiry_response)
        );
        // self.messageWriter(try termio.Message.writeReq(self.alloc, self.enquiry_response));
        let msg = Message::writeReq(&self.enquiry_response);
        self.messageWriter(msg);
        Ok(())
    }

    // fn configureCharset(
    //     self: *StreamHandler,
    //     slot: terminal.CharsetSlot,
    //     set: terminal.Charset,
    // ) void {
    pub(super) fn configureCharset(
        &mut self,
        slot: rustty_vt::charsets::Slots,
        set: rustty_vt::charsets::Charset,
    ) {
        // self.terminal.configureCharset(slot, set);
        self.term().configure_charset(slot, set);
    }

    // pub fn fullReset(
    //     self: *StreamHandler,
    // ) !void {
    pub fn fullReset(&mut self) -> Result<(), crate::Error> {
        // self.terminal.fullReset();
        self.term().full_reset();
        // try self.setMouseShape(.text);
        self.setMouseShape(rustty_vt::mouse::Shape::Text)?;

        // Full reset clears Kitty clipboard session grants.
        // self.kitty_clipboard_grants.deinit(self.alloc);
        // self.kitty_clipboard_grants = .{};
        self.kitty_clipboard_grants = rustty_vt::kitty::clipboard::Grants::default();

        // Reset resets our palette so we report it for mode 2031.
        // self.messageWriter(.{ .color_scheme_report = .{ .force = false } });
        self.messageWriter(Message::ColorSchemeReport { force: false });

        // Clear the progress bar
        // self.progressReport(.{ .state = .remove });
        self.progressReport(osc::ProgressReport {
            state: osc::ProgressReportState::Remove,
            progress: None,
        });
        Ok(())
    }

    // pub fn queryKittyKeyboard(self: *StreamHandler) !void {
    pub fn queryKittyKeyboard(&mut self) -> Result<(), crate::Error> {
        // log.debug("querying kitty keyboard mode", .{});
        log::debug!(target: LOG_SCOPE, "querying kitty keyboard mode");
        // var data: termio.Message.WriteReq.Small.Array = undefined;
        // const resp = try std.fmt.bufPrint(&data, "\x1b[?{}u", .{
        //     self.terminal.screens.active.kitty_keyboard.current().int(),
        // });
        let mut data = [0u8; 38];
        let current = self.term().active_mut().kitty_keyboard.current().int();
        let len = write_fixed(&mut data[..], format_args!("\x1b[?{current}u"))?;

        // self.messageWriter(.{
        //     .write_small = .{
        //         .data = data,
        //         .len = @intCast(resp.len),
        //     },
        // });
        self.messageWriter(Message::WriteSmall(
            rustty_datastruct::message_data::Small { data, len },
        ));
        Ok(())
    }

    // pub fn reportXtversion(
    //     self: *StreamHandler,
    // ) !void {
    pub fn reportXtversion(&mut self) -> Result<(), crate::Error> {
        // log.debug("reporting XTVERSION: ghostty {s}", .{build_config.version_string});
        //
        // (R2 identity: `ghostty` in the reference's literal is the product name.)
        log::debug!(
            target: LOG_SCOPE,
            "reporting XTVERSION: {} {}",
            rustty_identity::XTVERSION_NAME,
            rustty_version::VERSION_STRING
        );
        // var buf: [288]u8 = undefined;
        // const resp = try std.fmt.bufPrint(
        //     &buf,
        //     "\x1BP>|{s} {s}\x1B\\",
        //     .{
        //         "ghostty",
        //         build_config.version_string,
        //     },
        // );
        //
        // (R2 identity: the XTVERSION product name.)
        let mut buf = [0u8; 288];
        let len = write_fixed(
            &mut buf[..],
            format_args!(
                "\x1BP>|{} {}\x1B\\",
                rustty_identity::XTVERSION_NAME,
                rustty_version::VERSION_STRING,
            ),
        )?;
        // const msg = try termio.Message.writeReq(self.alloc, resp);
        let msg = Message::writeReq(&buf[..len]);
        // self.messageWriter(msg);
        self.messageWriter(msg);
        Ok(())
    }
}
