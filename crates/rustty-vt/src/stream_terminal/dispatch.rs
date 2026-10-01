//! PORTS: ghostty src/terminal/stream_terminal.zig — the `Handler` DCS
//! (`dcsHook`/`dcsPut`/`dcsUnhook`/`dcsCommand`, `writeTerminfoName`), bell,
//! horizontal tab, render hold, `setMode` and color operation
//! (`colorOperation`/`kittyColorOperation`) cluster.

use super::*;

impl<'a> Handler<'a> {
    // fn dcsHook(self: *Handler, value: Action.Value(.dcs_hook)) !void {
    //     var cmd = self.dcs_handler.hook(self.terminal.gpa(), value) orelse return;
    //     defer cmd.deinit();
    //     try self.dcsCommand(&cmd);
    // }
    pub(super) fn do_dcs_hook(
        &mut self,
        dcs: crate::parser::DCS,
    ) -> Result<(), crate::pagelist::OutOfMemory> {
        let gpa = self.term().gpa();
        // (PORT NOTE: the trait hands the parser's owned DCS; the dcs Handler
        // takes the borrowed view — the same coercion Zig's `terminal.DCS` does.)
        let mut cmd = match self.dcs_handler.hook(
            gpa,
            crate::dcs::DCS {
                intermediates: &dcs.intermediates,
                params: &dcs.params,
                final_: dcs.final_,
            },
        ) {
            Some(cmd) => cmd,
            None => return Ok(()),
        };
        // defer cmd.deinit();
        let result = self.do_dcs_command(&mut cmd);
        cmd.deinit();
        result
    }

    // fn dcsPut(self: *Handler, value: u8) !void { ... }
    pub(super) fn do_dcs_put(&mut self, value: u8) -> Result<(), crate::pagelist::OutOfMemory> {
        let mut cmd = match self.dcs_handler.put(value) {
            Some(cmd) => cmd,
            None => return Ok(()),
        };
        let result = self.do_dcs_command(&mut cmd);
        cmd.deinit();
        result
    }

    // fn dcsUnhook(self: *Handler) !void { ... }
    pub(super) fn do_dcs_unhook(&mut self) -> Result<(), crate::pagelist::OutOfMemory> {
        let mut cmd = match self.dcs_handler.unhook() {
            Some(cmd) => cmd,
            None => return Ok(()),
        };
        let result = self.do_dcs_command(&mut cmd);
        cmd.deinit();
        result
    }

    // fn dcsCommand(self: *Handler, cmd: *dcs.Command) !void {
    fn do_dcs_command(
        &mut self,
        cmd: &mut crate::dcs::Command,
    ) -> Result<(), crate::pagelist::OutOfMemory> {
        match cmd {
            // .decrqss => |request| { ... }
            crate::dcs::Command::Decrqss(request) => {
                // var response: [max_response_bytes + 1]u8 = undefined;
                // const encoded = try request.encode(self.terminal, response[0 .. response.len - 1]);
                // response[encoded.len] = 0;
                // self.writePty(response[0..encoded.len :0]);
                //
                // PORT NOTE: the reference reserves the final byte for the Zig
                // `[:0]const u8` sentinel the callback takes. rustty's
                // `write_pty` callback takes a plain `&[u8]`, so the reserved
                // byte is kept (identical capacity budget) but no NUL is
                // appended.
                let request = *request;
                let mut response = [0u8; crate::dcs::Decrqss::MAX_RESPONSE_BYTES + 1];
                let last = response.len() - 1;
                let encoded_len = {
                    let t: *mut Terminal = self.terminal;
                    // SAFETY: `self.terminal` aliases the Terminal that created
                    // this Handler (see `term()`); the deref is short-lived and
                    // does not overlap another live `&mut Terminal`.
                    // (PORT NOTE: `dcs::OutOfMemory` and `pagelist::OutOfMemory`
                    // are both the reference's single `Allocator.Error`; the
                    // crate has no `From` between them, so map explicitly.)
                    let encoded = request
                        .encode(unsafe { &mut *t }, &mut response[..last])
                        .map_err(|_| crate::pagelist::OutOfMemory)?;
                    encoded.len()
                };
                self.write_pty(&response[..encoded_len]);
                Ok(())
            }

            // .xtgettcap => |*gettcap| { ... }
            crate::dcs::Command::Xtgettcap(gettcap) => {
                // if (self.effects.write_pty == null) return;
                if self.effects.write_pty.is_none() {
                    return Ok(());
                }
                // const map = comptime terminfo.ghostty.xtgettcapMap();
                let map = xtgettcap_map();
                // while (gettcap.next()) |key| { ... }
                while let Some(key) = gettcap.next() {
                    if key == ENCODED_TN_KEY {
                        self.write_terminfo_name();
                        continue;
                    }
                    // self.writePty(map.get(key) orelse continue);
                    let Some(response) = map.get(key) else {
                        continue;
                    };
                    self.write_pty(response);
                }
                Ok(())
            }

            // .tmux => {},
            crate::dcs::Command::Tmux(_) => Ok(()),
        }
    }

    /// Answer an XTGETTCAP "TN" query from the configured terminfo name.
    /// Unset, empty, or over-long names leave the query unanswered.
    // fn writeTerminfoName(self: *Handler) void { ... }
    fn write_terminfo_name(&mut self) {
        // const name = self.terminfo_name orelse return;
        let Some(name) = self.terminfo_name else {
            return;
        };
        // if (name.len == 0 or name.len > max_terminfo_name_bytes) return;
        if name.is_empty() || name.len() > MAX_TERMINFO_NAME_BYTES {
            return;
        }

        // Fixed upper bound for an encoded "TN" reply calculated
        // at comptime from our max terminfo size.
        const MAX_TN_RESPONSE_BYTES: usize = "\x1bP1+r".len()
            + ENCODED_TN_KEY.len()
            + "=".len()
            + (MAX_TERMINFO_NAME_BYTES * 2)
            + "\x1b\\".len()
            + 1; // null terminator

        // Values are hex-encoded uppercase, matching the static map. The
        // buffer fits any name allowed above, so the print cannot fail.
        //
        // PORT NOTE: the reference `std.fmt.bufPrintZ(&buf, "\x1bP1+r" ++
        // encoded_tn_key ++ "={X}\x1b\\", .{name}) catch unreachable` appends a
        // NUL sentinel for the Zig `[:0]const u8` callback parameter; rustty's
        // `write_pty` takes a plain `&[u8]` (the DCS-response precedent), so
        // the reserved byte is kept in the capacity budget but no NUL is
        // appended. `{X}` is uppercase hex, one byte -> two digits.
        let mut buf = [0u8; MAX_TN_RESPONSE_BYTES];
        let mut len: usize = 0;
        for part in [b"\x1bP1+r".as_slice(), ENCODED_TN_KEY, b"="] {
            buf[len..len + part.len()].copy_from_slice(part);
            len += part.len();
        }
        const CHARSET: &[u8; 16] = b"0123456789ABCDEF";
        for &b in name {
            buf[len] = CHARSET[(b >> 4) as usize];
            buf[len + 1] = CHARSET[(b & 0xf) as usize];
            len += 2;
        }
        buf[len..len + 2].copy_from_slice(b"\x1b\\");
        len += 2;
        self.write_pty(&buf[..len]);
    }

    // fn bell(self: *Handler) void {
    //     const func = self.effects.bell orelse return;
    //     func(self);
    // }
    pub(super) fn do_bell(&mut self) {
        let Some(func) = self.effects.bell else {
            return;
        };
        func(self);
    }

    // inline fn horizontalTab(self: *Handler, count: u16) void {
    //     for (0..count) |_| {
    //         const x = self.terminal.screens.active.cursor.x;
    //         self.terminal.horizontalTab();
    //         if (x == self.terminal.screens.active.cursor.x) break;
    //     }
    // }
    #[inline]
    pub(super) fn do_horizontal_tab(&mut self, count: u16) {
        for _ in 0..count {
            let x = self.term().active_mut().cursor.x;
            self.term().horizontal_tab();
            if x == self.term().active_mut().cursor.x {
                break;
            }
        }
    }

    // inline fn horizontalTabBack(self: *Handler, count: u16) void {
    //     for (0..count) |_| {
    //         const x = self.terminal.screens.active.cursor.x;
    //         self.terminal.horizontalTabBack();
    //         if (x == self.terminal.screens.active.cursor.x) break;
    //     }
    // }
    #[inline]
    pub(super) fn do_horizontal_tab_back(&mut self, count: u16) {
        for _ in 0..count {
            let x = self.term().active_mut().cursor.x;
            self.term().horizontal_tab_back();
            if x == self.term().active_mut().cursor.x {
                break;
            }
        }
    }

    /// Report that a render hold began or ended. See `Effects.render_hold`.
    // inline fn renderHold(self: *Handler, held: bool) void {
    //     const func = self.effects.render_hold orelse return;
    //     func(self, held);
    // }
    #[inline]
    pub(super) fn render_hold(&mut self, held: bool) {
        let Some(func) = self.effects.render_hold else {
            return;
        };
        func(self, held);
    }

    // fn setMode(self: *Handler, mode: modes.Mode, enabled: bool) !void
    pub(super) fn do_set_mode(
        &mut self,
        mode: modes::Mode,
        enabled: bool,
    ) -> Result<(), crate::pagelist::OutOfMemory> {
        // Synchronized output is reported as a render hold. We only report
        // real changes. Reporting a set during a hold would be harmful
        // because the screen is half-drawn at that point and the callback
        // is expected to capture it.
        if mode == modes::Mode::SynchronizedOutput {
            if self.term().modes.get(mode) == enabled {
                return Ok(());
            }
            self.term().modes.set(mode, enabled);
            self.render_hold(enabled);
            return Ok(());
        }

        // Set the mode on the terminal
        // self.terminal.modes.set(mode, enabled);
        self.term().modes.set(mode, enabled);

        // Some modes require additional processing
        match mode {
            // .autorepeat,
            // .reverse_colors,
            // => {},
            modes::Mode::Autorepeat | modes::Mode::ReverseColors => {}

            // .origin => self.terminal.setCursorPos(1, 1),
            modes::Mode::Origin => self.term().set_cursor_pos(1, 1),

            // .enable_left_and_right_margin => if (!enabled) {
            //     self.terminal.scrolling_region.left = 0;
            //     self.terminal.scrolling_region.right = self.terminal.cols - 1;
            // },
            modes::Mode::EnableLeftAndRightMargin => {
                if !enabled {
                    self.term().scrolling_region.left = 0;
                    let right = self.term().cols - 1;
                    self.term().scrolling_region.right = right;
                }
            }

            // .alt_screen_legacy => try self.terminal.switchScreenMode(.@"47", enabled),
            modes::Mode::AltScreenLegacy => {
                self.term()
                    .switch_screen_mode(SwitchScreenMode::M47, enabled)?;
            }
            // .alt_screen => try self.terminal.switchScreenMode(.@"1047", enabled),
            modes::Mode::AltScreen => {
                self.term()
                    .switch_screen_mode(SwitchScreenMode::M1047, enabled)?;
            }
            // .alt_screen_save_cursor_clear_enter => try self.terminal.switchScreenMode(.@"1049", enabled),
            modes::Mode::AltScreenSaveCursorClearEnter => {
                self.term()
                    .switch_screen_mode(SwitchScreenMode::M1049, enabled)?;
            }

            // .save_cursor => if (enabled) {
            //     self.terminal.saveCursor();
            // } else {
            //     self.terminal.restoreCursor();
            // },
            modes::Mode::SaveCursor => {
                if enabled {
                    self.term().save_cursor();
                } else {
                    self.term().restore_cursor();
                }
            }

            // .enable_mode_3 => {},
            modes::Mode::EnableMode3 => {}

            // .@"132_column" => try self.terminal.deccolm(
            //     self.terminal.screens.active.alloc,
            //     if (enabled) .@"132_cols" else .@"80_cols",
            // ),
            modes::Mode::Column132 => {
                let alloc = unsafe { (*self.term().screens.active).alloc };
                self.term().deccolm(
                    alloc,
                    if enabled {
                        DeccolmMode::Cols132
                    } else {
                        DeccolmMode::Cols80
                    },
                )?;
            }

            // Handled above
            // .synchronized_output => unreachable,
            //
            // PORT NOTE (R7): an invariant `unreachable`, ported as a panic.
            modes::Mode::SynchronizedOutput => unreachable!(),

            // .linefeed,
            // .focus_event,
            // => {},
            modes::Mode::Linefeed | modes::Mode::FocusEvent => {}

            // Enabling mode 2048 reports already-committed pixel geometry.
            // Waiting for the next resize leaves late-enabling clients without
            // the dimensions they need for their first image frame.
            // .in_band_size_reports => if (enabled) self.reportMode2048(),
            modes::Mode::InBandSizeReports => {
                if enabled {
                    self.report_mode_2048();
                }
            }

            // .report_visibility => if (enabled) self.sendVisibilityReport(),
            modes::Mode::ReportVisibility => {
                if enabled {
                    self.send_visibility_report();
                }
            }

            // .mouse_event_x10 => {
            //     if (enabled) {
            //         self.terminal.flags.mouse_event = .x10;
            //     } else {
            //         self.terminal.flags.mouse_event = .none;
            //     }
            // },
            modes::Mode::MouseEventX10 => {
                self.term().flags.mouse_event = if enabled {
                    crate::mouse::Event::X10
                } else {
                    crate::mouse::Event::None
                };
            }
            // .mouse_event_normal => {
            //     if (enabled) {
            //         self.terminal.flags.mouse_event = .normal;
            //     } else {
            //         self.terminal.flags.mouse_event = .none;
            //     }
            // },
            modes::Mode::MouseEventNormal => {
                self.term().flags.mouse_event = if enabled {
                    crate::mouse::Event::Normal
                } else {
                    crate::mouse::Event::None
                };
            }
            // .mouse_event_button => {
            //     if (enabled) {
            //         self.terminal.flags.mouse_event = .button;
            //     } else {
            //         self.terminal.flags.mouse_event = .none;
            //     }
            // },
            modes::Mode::MouseEventButton => {
                self.term().flags.mouse_event = if enabled {
                    crate::mouse::Event::Button
                } else {
                    crate::mouse::Event::None
                };
            }
            // .mouse_event_any => {
            //     if (enabled) {
            //         self.terminal.flags.mouse_event = .any;
            //     } else {
            //         self.terminal.flags.mouse_event = .none;
            //     }
            // },
            modes::Mode::MouseEventAny => {
                self.term().flags.mouse_event = if enabled {
                    crate::mouse::Event::Any
                } else {
                    crate::mouse::Event::None
                };
            }

            // .mouse_format_utf8 => self.terminal.flags.mouse_format = if (enabled) .utf8 else .x10,
            modes::Mode::MouseFormatUtf8 => {
                self.term().flags.mouse_format = if enabled {
                    crate::mouse::Format::Utf8
                } else {
                    crate::mouse::Format::X10
                };
            }
            // .mouse_format_sgr => self.terminal.flags.mouse_format = if (enabled) .sgr else .x10,
            modes::Mode::MouseFormatSgr => {
                self.term().flags.mouse_format = if enabled {
                    crate::mouse::Format::Sgr
                } else {
                    crate::mouse::Format::X10
                };
            }
            // .mouse_format_urxvt => self.terminal.flags.mouse_format = if (enabled) .urxvt else .x10,
            modes::Mode::MouseFormatUrxvt => {
                self.term().flags.mouse_format = if enabled {
                    crate::mouse::Format::Urxvt
                } else {
                    crate::mouse::Format::X10
                };
            }
            // .mouse_format_sgr_pixels => self.terminal.flags.mouse_format = if (enabled) .sgr_pixels else .x10,
            modes::Mode::MouseFormatSgrPixels => {
                self.term().flags.mouse_format = if enabled {
                    crate::mouse::Format::SgrPixels
                } else {
                    crate::mouse::Format::X10
                };
            }

            // else => {},
            _ => {}
        }

        Ok(())
    }

    // fn colorOperation(
    //     self: *Handler,
    //     requests: *const osc_color.List,
    //     terminator: osc.Terminator,
    // ) !void
    pub(super) fn do_color_operation(
        &mut self,
        requests: &osc::color::List,
        terminator: osc::Terminator,
    ) -> Result<(), crate::snapshot::io::WriteFailed> {
        // if (requests.count() == 0) return;
        if requests.is_empty() {
            return Ok(());
        }

        // var stack = std.heap.stackFallback(1024, self.terminal.gpa());
        // const alloc = stack.get();
        // var response: std.Io.Writer.Allocating = .init(alloc);
        // defer response.deinit();
        // const writer = &response.writer;
        //
        //
        // PORT NOTE (parity tail S7-4): the response is a
        // `StackFallbackWriter<1024>` over the terminal's `gpa()`, so a response
        // past 1024 bytes spills through that allocator and a refused
        // allocation fails the write (`error.WriteFailed`), which the dispatch
        // arm logs. (This retires the `COLOR_OPERATION_TW` stand-in that P3-W4
        // T5 added while the port's writer could not fail.)
        let gpa = self.term().gpa();
        let mut response = crate::snapshot::io::StackFallbackWriter::<1024>::new_in(gpa);

        // var it = requests.constIterator(0);
        // while (it.next()) |req| {
        for req in requests.iter() {
            match req {
                // .set => |set| {
                osc::color::Request::Set(set) => {
                    match set.target {
                        // .palette => |i| {
                        //     self.terminal.flags.dirty.palette = true;
                        //     self.terminal.colors.palette.set(i, set.color);
                        // },
                        osc::color::Target::Palette(i) => {
                            self.term().flags.dirty.palette = true;
                            self.term().colors.palette.set(i, set.color);
                        }
                        // .dynamic => |dynamic| switch (dynamic) {
                        osc::color::Target::Dynamic(dynamic) => match dynamic {
                            // .foreground => self.terminal.colors.foreground.set(set.color),
                            color::Dynamic::Foreground => {
                                self.term().colors.foreground.set(set.color);
                            }
                            // .background => self.terminal.colors.background.set(set.color),
                            color::Dynamic::Background => {
                                self.term().colors.background.set(set.color);
                            }
                            // .cursor => self.terminal.colors.cursor.set(set.color),
                            color::Dynamic::Cursor => {
                                self.term().colors.cursor.set(set.color);
                            }
                            // .pointer_foreground,
                            // .pointer_background,
                            // .tektronix_foreground,
                            // .tektronix_background,
                            // .highlight_background,
                            // .tektronix_cursor,
                            // .highlight_foreground,
                            // => {},
                            color::Dynamic::PointerForeground
                            | color::Dynamic::PointerBackground
                            | color::Dynamic::TektronixForeground
                            | color::Dynamic::TektronixBackground
                            | color::Dynamic::HighlightBackground
                            | color::Dynamic::TektronixCursor
                            | color::Dynamic::HighlightForeground => {}
                        },
                        // .special => {},
                        osc::color::Target::Special(_) => {}
                    }
                }

                // .reset => |target| switch (target) {
                osc::color::Request::Reset(target) => match target {
                    // .palette => |i| {
                    //     self.terminal.flags.dirty.palette = true;
                    //     self.terminal.colors.palette.reset(i);
                    // },
                    osc::color::Target::Palette(i) => {
                        self.term().flags.dirty.palette = true;
                        self.term().colors.palette.reset(*i);
                    }
                    // .dynamic => |dynamic| switch (dynamic) {
                    osc::color::Target::Dynamic(dynamic) => match dynamic {
                        // .foreground => self.terminal.colors.foreground.reset(),
                        color::Dynamic::Foreground => self.term().colors.foreground.reset(),
                        // .background => self.terminal.colors.background.reset(),
                        color::Dynamic::Background => self.term().colors.background.reset(),
                        // .cursor => self.terminal.colors.cursor.reset(),
                        color::Dynamic::Cursor => self.term().colors.cursor.reset(),
                        // .pointer_foreground,
                        // ...
                        // => {},
                        color::Dynamic::PointerForeground
                        | color::Dynamic::PointerBackground
                        | color::Dynamic::TektronixForeground
                        | color::Dynamic::TektronixBackground
                        | color::Dynamic::HighlightBackground
                        | color::Dynamic::TektronixCursor
                        | color::Dynamic::HighlightForeground => {}
                    },
                    // .special => {},
                    osc::color::Target::Special(_) => {}
                },

                // .reset_palette => {
                //     const mask = &self.terminal.colors.palette.mask;
                //     var mask_it = mask.iterator(.{});
                //     while (mask_it.next()) |i| {
                //         self.terminal.flags.dirty.palette = true;
                //         self.terminal.colors.palette.reset(@intCast(i));
                //     }
                //     mask.* = .initEmpty();
                // },
                osc::color::Request::ResetPalette => {
                    let indices: Vec<usize> = self.term().colors.palette.mask.iter_set().collect();
                    for i in indices {
                        self.term().flags.dirty.palette = true;
                        self.term().colors.palette.reset(i as u8);
                    }
                    self.term().colors.palette.mask = color::PaletteMask::init_empty();
                }

                // .query => |target| {
                //     if (self.effects.write_pty == null) continue;
                //     const c = self.terminal.colorForXterm(target) orelse continue;
                //     try writeXtermColorReport(writer, target, c, terminator);
                // },
                osc::color::Request::Query(target) => {
                    if self.effects.write_pty.is_none() {
                        continue;
                    }
                    let Some(c) = self.term().color_for_xterm(*target) else {
                        continue;
                    };
                    Self::write_xterm_color_report(&mut response, target, c, terminator)?;
                }

                // .reset_special => {},
                osc::color::Request::ResetSpecial => {}
            }
        }

        // if (response.written().len > 0) {
        //     const resp = try response.toOwnedSliceSentinel(0);
        //     defer alloc.free(resp);
        //     self.writePty(resp);
        // }
        if !response.written().is_empty() {
            self.write_pty(response.written());
        }

        Ok(())
    }

    // fn writeXtermColorReport(
    //     writer: *std.Io.Writer,
    //     target: osc_color.Target,
    //     c: color.RGB,
    //     terminator: osc.Terminator,
    // ) !void
    fn write_xterm_color_report(
        out: &mut crate::snapshot::io::StackFallbackWriter<1024>,
        target: &osc::color::Target,
        c: color::RGB,
        terminator: osc::Terminator,
    ) -> Result<(), crate::snapshot::io::WriteFailed> {
        use crate::snapshot::io::WriteFailed;
        use std::io::Write as _;
        match target {
            // .palette => |i| {
            //     try writer.print("\x1b]4;{d};", .{i});
            //     try c.encodeRgb16(writer);
            //     try writer.writeAll(terminator.string());
            // },
            osc::color::Target::Palette(i) => {
                write!(out, "\x1b]4;{};", i).map_err(|_| WriteFailed)?;
                c.encode_rgb16(out).map_err(|_| WriteFailed)?;
                out.write_all(terminator.string())
                    .map_err(|_| WriteFailed)?;
            }
            // .dynamic => |dynamic| switch (dynamic) {
            osc::color::Target::Dynamic(dynamic) => match dynamic {
                // .foreground, .background, .cursor => {
                //     try writer.print("\x1b]{d};", .{@intFromEnum(dynamic)});
                //     try c.encodeRgb16(writer);
                //     try writer.writeAll(terminator.string());
                // },
                color::Dynamic::Foreground
                | color::Dynamic::Background
                | color::Dynamic::Cursor => {
                    write!(out, "\x1b]{};", *dynamic as u32).map_err(|_| WriteFailed)?;
                    c.encode_rgb16(out).map_err(|_| WriteFailed)?;
                    out.write_all(terminator.string())
                        .map_err(|_| WriteFailed)?;
                }
                // .pointer_foreground, ... => {},
                color::Dynamic::PointerForeground
                | color::Dynamic::PointerBackground
                | color::Dynamic::TektronixForeground
                | color::Dynamic::TektronixBackground
                | color::Dynamic::HighlightBackground
                | color::Dynamic::TektronixCursor
                | color::Dynamic::HighlightForeground => {}
            },
            // .special => {},
            osc::color::Target::Special(_) => {}
        }
        Ok(())
    }

    // fn kittyColorOperation(
    //     self: *Handler,
    //     request: kitty_color.OSC,
    // ) !void
    pub(super) fn do_kitty_color_operation(
        &mut self,
        request: &kitty::color::OSC,
    ) -> Result<(), crate::snapshot::io::WriteFailed> {
        // var stack = std.heap.stackFallback(1024, self.terminal.gpa());
        // const alloc = stack.get();
        // var response: std.Io.Writer.Allocating = .init(alloc);
        // defer response.deinit();
        // const writer = &response.writer;
        let gpa = self.term().gpa();
        let mut response = crate::snapshot::io::StackFallbackWriter::<1024>::new_in(gpa);

        // for (request.list.items) |item| {
        for item in request.list.iter() {
            match item {
                // .set => |v| switch (v.key) {
                kitty::color::Request::Set { key, color } => match key {
                    // .palette => |palette| {
                    //     self.terminal.flags.dirty.palette = true;
                    //     self.terminal.colors.palette.set(palette, v.color);
                    // },
                    kitty::color::Kind::Palette(palette) => {
                        self.term().flags.dirty.palette = true;
                        self.term().colors.palette.set(*palette, *color);
                    }
                    // .special => |special| switch (special) {
                    kitty::color::Kind::Special(special) => match special {
                        // .foreground => self.terminal.colors.foreground.set(v.color),
                        kitty::color::Special::Foreground => {
                            self.term().colors.foreground.set(*color);
                        }
                        // .background => self.terminal.colors.background.set(v.color),
                        kitty::color::Special::Background => {
                            self.term().colors.background.set(*color);
                        }
                        // .cursor => self.terminal.colors.cursor.set(v.color),
                        kitty::color::Special::Cursor => {
                            self.term().colors.cursor.set(*color);
                        }
                        // else => {},
                        _ => {}
                    },
                },
                // .reset => |key| switch (key) {
                kitty::color::Request::Reset(key) => match key {
                    // .palette => |palette| {
                    //     self.terminal.flags.dirty.palette = true;
                    //     self.terminal.colors.palette.reset(palette);
                    // },
                    kitty::color::Kind::Palette(palette) => {
                        self.term().flags.dirty.palette = true;
                        self.term().colors.palette.reset(*palette);
                    }
                    // .special => |special| switch (special) {
                    kitty::color::Kind::Special(special) => match special {
                        // .foreground => self.terminal.colors.foreground.reset(),
                        kitty::color::Special::Foreground => self.term().colors.foreground.reset(),
                        // .background => self.terminal.colors.background.reset(),
                        kitty::color::Special::Background => self.term().colors.background.reset(),
                        // .cursor => self.terminal.colors.cursor.reset(),
                        kitty::color::Special::Cursor => self.term().colors.cursor.reset(),
                        // else => {},
                        _ => {}
                    },
                },
                // .query => |key| {
                //     if (self.effects.write_pty == null) continue;
                //     const c = self.terminal.colorForKitty(key) orelse {
                //         if (!key.hasTerminalQueryColor()) continue;
                //         if (response.written().len == 0) try writer.writeAll("\x1b]21");
                //         try writer.print(";{f}=", .{key});
                //         continue;
                //     };
                //
                //     if (response.written().len == 0) try writer.writeAll("\x1b]21");
                //     try writer.print(";{f}=", .{key});
                //     try c.encodeRgb8(writer);
                // },
                kitty::color::Request::Query(key) => {
                    use crate::snapshot::io::WriteFailed;
                    use std::io::Write as _;
                    if self.effects.write_pty.is_none() {
                        continue;
                    }
                    match self.term().color_for_kitty(*key) {
                        None => {
                            if !key.has_terminal_query_color() {
                                continue;
                            }
                            if response.written().is_empty() {
                                response.write_all(b"\x1b]21").map_err(|_| WriteFailed)?;
                            }
                            write!(response, ";{}=", key).map_err(|_| WriteFailed)?;
                        }
                        Some(c) => {
                            if response.written().is_empty() {
                                response.write_all(b"\x1b]21").map_err(|_| WriteFailed)?;
                            }
                            write!(response, ";{}=", key).map_err(|_| WriteFailed)?;
                            c.encode_rgb8(&mut response).map_err(|_| WriteFailed)?;
                        }
                    }
                }
            }
        }

        // if (response.written().len > 0) {
        //     try writer.writeAll(request.terminator.string());
        //     const resp = try response.toOwnedSliceSentinel(0);
        //     defer alloc.free(resp);
        //     self.writePty(resp);
        // }
        if !response.written().is_empty() {
            crate::snapshot::io::Writer::write_all(&mut response, request.terminator.string())?;
            self.write_pty(response.written());
        }

        Ok(())
    }
}
