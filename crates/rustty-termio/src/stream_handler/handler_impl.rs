//! PORTS: ghostty src/termio/stream_handler.zig — `vtFallible`, the `switch (action)`
//! dispatch, as the vt `stream::Handler` trait impl.

use super::*;

// PORT NOTE (stream_terminal precedent): the `vtFallible` switch, distributed
// across the vt `Handler` trait methods — each arm's body below is the
// corresponding `switch (action)` arm, with the reference's `@branchHint`s
// preserved as comments (Rust has no stable branch-hint intrinsic at this
// altitude). The reference's switch preamble:
//
// The branch hints here are based on real world data
// which indicates that the most common actions are:
//
// 1. print
// 2. set_attribute
// 3. carriage_return
// 4. line_feed
// 5. cursor_pos
//
// Together, these 5 actions make up nearly 98% of
// all actions encountered in real world scenarios.
//
// ref: https://github.com/qwerasd205/asciinema-stats
impl<M: AppMailbox> stream::Handler for StreamHandler<M> {
    fn deinit(&mut self) {
        StreamHandler::deinit(self);
    }

    // .print => {
    //     @branchHint(.likely);
    //     try self.terminal.print(value.cp);
    // },
    fn print(&mut self, cp: u32) {
        let r = self.term().print(cp).map_err(|_| crate::Error);
        self.vt("print", r);
    }

    // .print_slice => { @branchHint(.likely); try self.terminal.printSlice(value.cps); },
    //
    // VERIFICATION NOTE (P3-W7 T6, area-E `E10`): the plan's proposed control
    // for this arm was "assert a multi-codepoint run through `print_slice`
    // produces the SAME SCREEN as the same bytes fed one at a time". At THIS
    // level that assertion is **vacuous by construction** and must not be
    // written: `Handler::print_slice`'s trait default is *defined* as fanning
    // the run back out to `print` (see `rustty_vt::stream`), so deleting this
    // override leaves the resulting screen byte-identical — the test would pass
    // either way. The arm's presence is a throughput property, not an
    // observable-behaviour one, which is why upstream ships it with no test.
    //
    // The behavioural equivalence IS pinned, one layer down where it is
    // falsifiable:
    //   * `rustty_vt::terminal::tests::attrs::terminal_print_slice_differential_fuzz_vs_print`
    //     — randomized differential, `print` vs `print_slice`, asserted after
    //     EVERY op;
    //   * `rustty_vt::stream::tests::stream_print_slice_matches_per_byte` —
    //     the ground-state dispatch path, driven through `next_slice_capped`
    //     so it is not silently skipped in a non-SIMD build.
    fn print_slice(&mut self, cps: &[u32]) {
        let r = self.term().print_slice(cps).map_err(|_| crate::Error);
        self.vt("print_slice", r);
    }

    // .print_repeat => try self.terminal.printRepeat(value),
    fn print_repeat(&mut self, count: usize) {
        let r = self.term().print_repeat(count).map_err(|_| crate::Error);
        self.vt("print_repeat", r);
    }

    // .bell => self.bell(),
    fn bell(&mut self) {
        StreamHandler::bell(self);
    }

    // .backspace => self.terminal.backspace(),
    fn backspace(&mut self) {
        self.term().backspace();
    }

    // .horizontal_tab => self.horizontalTab(value),
    fn horizontal_tab(&mut self, count: u16) {
        self.horizontalTab(count);
    }

    // .horizontal_tab_back => self.horizontalTabBack(value),
    fn horizontal_tab_back(&mut self, count: u16) {
        self.horizontalTabBack(count);
    }

    // .linefeed => {
    //     @branchHint(.likely);
    //     try self.linefeed();
    // },
    fn linefeed(&mut self) {
        let r = StreamHandler::linefeed(self);
        self.vt("linefeed", r);
    }

    // .carriage_return => {
    //     @branchHint(.likely);
    //     self.terminal.carriageReturn();
    // },
    fn carriage_return(&mut self) {
        self.term().carriage_return();
    }

    // .enquiry => try self.enquiry(),
    fn enquiry(&mut self) {
        let r = StreamHandler::enquiry(self);
        self.vt("enquiry", r);
    }

    // .invoke_charset => self.terminal.invokeCharset(value.bank, value.charset, value.locking),
    fn invoke_charset(&mut self, v: stream::InvokeCharset) {
        self.term().invoke_charset(v.bank, v.charset, v.locking);
    }

    // .cursor_up => self.terminal.cursorUp(value.value),
    fn cursor_up(&mut self, v: stream::CursorMovement) {
        self.term().cursor_up(v.value as usize);
    }

    // .cursor_down => self.terminal.cursorDown(value.value),
    fn cursor_down(&mut self, v: stream::CursorMovement) {
        self.term().cursor_down(v.value as usize);
    }

    // .cursor_left => self.terminal.cursorLeft(value.value),
    fn cursor_left(&mut self, v: stream::CursorMovement) {
        self.term().cursor_left(v.value as usize);
    }

    // .cursor_right => self.terminal.cursorRight(value.value),
    fn cursor_right(&mut self, v: stream::CursorMovement) {
        self.term().cursor_right(v.value as usize);
    }

    // .cursor_pos => {
    //     @branchHint(.likely);
    //     self.terminal.setCursorPos(value.row, value.col);
    // },
    fn cursor_pos(&mut self, v: stream::CursorPos) {
        self.term().set_cursor_pos(v.row as usize, v.col as usize);
    }

    // .cursor_col => self.terminal.setCursorPos(self.terminal.screens.active.cursor.y + 1, value.value),
    fn cursor_col(&mut self, v: stream::CursorMovement) {
        let y = self.term().active_mut().cursor.y;
        self.term().set_cursor_pos(y as usize + 1, v.value as usize);
    }

    // .cursor_row => self.terminal.setCursorPos(value.value, self.terminal.screens.active.cursor.x + 1),
    fn cursor_row(&mut self, v: stream::CursorMovement) {
        let x = self.term().active_mut().cursor.x;
        self.term().set_cursor_pos(v.value as usize, x as usize + 1);
    }

    // .cursor_col_relative => self.terminal.setCursorPos(
    //     self.terminal.screens.active.cursor.y + 1,
    //     self.terminal.screens.active.cursor.x + 1 +| value.value,
    // ),
    fn cursor_col_relative(&mut self, v: stream::CursorMovement) {
        let y = self.term().active_mut().cursor.y;
        let x = self.term().active_mut().cursor.x;
        self.term()
            .set_cursor_pos(y as usize + 1, (x + 1).saturating_add(v.value) as usize);
    }

    // .cursor_row_relative => self.terminal.setCursorPos(
    //     self.terminal.screens.active.cursor.y + 1 +| value.value,
    //     self.terminal.screens.active.cursor.x + 1,
    // ),
    fn cursor_row_relative(&mut self, v: stream::CursorMovement) {
        let y = self.term().active_mut().cursor.y;
        let x = self.term().active_mut().cursor.x;
        self.term()
            .set_cursor_pos((y + 1).saturating_add(v.value) as usize, x as usize + 1);
    }

    // .cursor_style => self.terminal.setCursorStyle(value),
    fn cursor_style(&mut self, v: ansi::CursorStyle) {
        self.term().set_cursor_style(v);
    }

    // .erase_display_below => self.terminal.eraseDisplay(.below, value),
    fn erase_display_below(&mut self, protected: bool) {
        self.term()
            .erase_display(csi::EraseDisplay::Below, protected);
    }

    // .erase_display_above => self.terminal.eraseDisplay(.above, value),
    fn erase_display_above(&mut self, protected: bool) {
        self.term()
            .erase_display(csi::EraseDisplay::Above, protected);
    }

    // .erase_display_complete => {
    //     self.terminal.scrollViewport(.{ .bottom = {} });
    //     self.terminal.eraseDisplay(.complete, value);
    // },
    fn erase_display_complete(&mut self, protected: bool) {
        self.term()
            .scroll_viewport(rustty_vt::terminal::ScrollViewport::Bottom);
        self.term()
            .erase_display(csi::EraseDisplay::Complete, protected);
    }

    // .erase_display_scrollback => self.terminal.eraseDisplay(.scrollback, value),
    fn erase_display_scrollback(&mut self, protected: bool) {
        self.term()
            .erase_display(csi::EraseDisplay::Scrollback, protected);
    }

    // .erase_display_scroll_complete => self.terminal.eraseDisplay(.scroll_complete, value),
    fn erase_display_scroll_complete(&mut self, protected: bool) {
        self.term()
            .erase_display(csi::EraseDisplay::ScrollComplete, protected);
    }

    // .erase_line_right => self.terminal.eraseLine(.right, value),
    fn erase_line_right(&mut self, protected: bool) {
        self.term().erase_line(csi::EraseLine::Right, protected);
    }

    // .erase_line_left => self.terminal.eraseLine(.left, value),
    fn erase_line_left(&mut self, protected: bool) {
        self.term().erase_line(csi::EraseLine::Left, protected);
    }

    // .erase_line_complete => self.terminal.eraseLine(.complete, value),
    fn erase_line_complete(&mut self, protected: bool) {
        self.term().erase_line(csi::EraseLine::Complete, protected);
    }

    // .erase_line_right_unless_pending_wrap => self.terminal.eraseLine(.right_unless_pending_wrap, value),
    fn erase_line_right_unless_pending_wrap(&mut self, protected: bool) {
        self.term()
            .erase_line(csi::EraseLine::RightUnlessPendingWrap, protected);
    }

    // .delete_chars => self.terminal.deleteChars(value),
    fn delete_chars(&mut self, count: usize) {
        self.term().delete_chars(count);
    }

    // .erase_chars => self.terminal.eraseChars(value),
    fn erase_chars(&mut self, count: usize) {
        self.term().erase_chars(count);
    }

    // .insert_lines => self.terminal.insertLines(value),
    fn insert_lines(&mut self, count: usize) {
        self.term().insert_lines(count);
    }

    // .insert_blanks => self.terminal.insertBlanks(value),
    fn insert_blanks(&mut self, count: usize) {
        self.term().insert_blanks(count);
    }

    // .delete_lines => self.terminal.deleteLines(value),
    fn delete_lines(&mut self, count: usize) {
        self.term().delete_lines(count);
    }

    // .scroll_up => try self.terminal.scrollUp(value),
    fn scroll_up(&mut self, count: usize) {
        let r = self.term().scroll_up(count).map_err(|_| crate::Error);
        self.vt("scroll_up", r);
    }

    // .scroll_down => self.terminal.scrollDown(value),
    fn scroll_down(&mut self, count: usize) {
        self.term().scroll_down(count);
    }

    // .tab_clear_current => self.terminal.tabClear(.current),
    fn tab_clear_current(&mut self) {
        self.term().tab_clear(csi::TabClear::Current);
    }

    // .tab_clear_all => self.terminal.tabClear(.all),
    fn tab_clear_all(&mut self) {
        self.term().tab_clear(csi::TabClear::All);
    }

    // .tab_set => self.terminal.tabSet(),
    fn tab_set(&mut self) {
        self.term().tab_set();
    }

    // .tab_reset => self.terminal.tabReset(),
    fn tab_reset(&mut self) {
        self.term().tab_reset();
    }

    // .index => try self.index(),
    fn index(&mut self) {
        let r = StreamHandler::index(self);
        self.vt("index", r);
    }

    // .next_line => try self.nextLine(),
    fn next_line(&mut self) {
        let r = self.nextLine();
        self.vt("next_line", r);
    }

    // .reverse_index => try self.reverseIndex(),
    fn reverse_index(&mut self) {
        let r = self.reverseIndex();
        self.vt("reverse_index", r);
    }

    // .full_reset => try self.fullReset(),
    fn full_reset(&mut self) {
        let r = self.fullReset();
        self.vt("full_reset", r);
    }

    // .set_mode => try self.setMode(value.mode, true),
    fn set_mode(&mut self, v: stream::Mode) {
        let r = self.setMode(v.mode, true);
        self.vt("set_mode", r);
    }

    // .reset_mode => try self.setMode(value.mode, false),
    fn reset_mode(&mut self, v: stream::Mode) {
        let r = self.setMode(v.mode, false);
        self.vt("reset_mode", r);
    }

    // .save_mode => self.terminal.modes.save(value.mode),
    fn save_mode(&mut self, v: stream::Mode) {
        self.term().modes.save(v.mode);
    }

    // .restore_mode => {
    //     // For restore mode we have to restore but if we set it, we
    //     // always have to call setMode because setting some modes have
    //     // side effects and we want to make sure we process those.
    //     const v = self.terminal.modes.restore(value.mode);
    //     try self.setMode(value.mode, v);
    // },
    fn restore_mode(&mut self, v: stream::Mode) {
        // For restore mode we have to restore but if we set it, we
        // always have to call setMode because setting some modes have
        // side effects and we want to make sure we process those.
        let restored = self.term().modes.restore(v.mode);
        let r = self.setMode(v.mode, restored);
        self.vt("restore_mode", r);
    }

    // .request_mode => try self.requestMode(value.mode),
    fn request_mode(&mut self, v: stream::Mode) {
        let r = self.requestMode(v.mode);
        self.vt("request_mode", r);
    }

    // .request_mode_unknown => try self.requestModeUnknown(value.mode, value.ansi),
    fn request_mode_unknown(&mut self, v: stream::RawMode) {
        let r = self.requestModeUnknown(v.mode, v.ansi);
        self.vt("request_mode_unknown", r);
    }

    // .top_and_bottom_margin => self.terminal.setTopAndBottomMargin(value.top_left, value.bottom_right),
    fn top_and_bottom_margin(&mut self, v: stream::Margin) {
        self.term()
            .set_top_and_bottom_margin(v.top_left as usize, v.bottom_right as usize);
    }

    // .left_and_right_margin => self.terminal.setLeftAndRightMargin(value.top_left, value.bottom_right),
    fn left_and_right_margin(&mut self, v: stream::Margin) {
        self.term()
            .set_left_and_right_margin(v.top_left as usize, v.bottom_right as usize);
    }

    // .left_and_right_margin_ambiguous => {
    //     if (self.terminal.modes.get(.enable_left_and_right_margin)) {
    //         self.terminal.setLeftAndRightMargin(0, 0);
    //     } else {
    //         self.terminal.saveCursor();
    //     }
    // },
    fn left_and_right_margin_ambiguous(&mut self) {
        if self.term().modes.get(modes::Mode::EnableLeftAndRightMargin) {
            self.term().set_left_and_right_margin(0, 0);
        } else {
            self.term().save_cursor();
        }
    }

    // .save_cursor => try self.saveCursor(),
    fn save_cursor(&mut self) {
        let r = self.saveCursor();
        self.vt("save_cursor", r);
    }

    // .restore_cursor => try self.restoreCursor(),
    fn restore_cursor(&mut self) {
        let r = self.restoreCursor();
        self.vt("restore_cursor", r);
    }

    // .modify_key_format => try self.setModifyKeyFormat(value),
    fn modify_key_format(&mut self, v: ansi::ModifyKeyFormat) {
        let r = self.setModifyKeyFormat(v);
        self.vt("modify_key_format", r);
    }

    // .protected_mode_off => self.terminal.setProtectedMode(.off),
    fn protected_mode_off(&mut self) {
        self.term().set_protected_mode(ansi::ProtectedMode::Off);
    }

    // .protected_mode_iso => self.terminal.setProtectedMode(.iso),
    fn protected_mode_iso(&mut self) {
        self.term().set_protected_mode(ansi::ProtectedMode::Iso);
    }

    // .protected_mode_dec => self.terminal.setProtectedMode(.dec),
    fn protected_mode_dec(&mut self) {
        self.term().set_protected_mode(ansi::ProtectedMode::Dec);
    }

    // .mouse_shift_capture => self.terminal.flags.mouse_shift_capture = if (value) .true else .false,
    fn mouse_shift_capture(&mut self, capture: bool) {
        self.term().flags.mouse_shift_capture = if capture {
            rustty_vt::terminal::MouseShiftCapture::True
        } else {
            rustty_vt::terminal::MouseShiftCapture::False
        };
    }

    // .xt_checksum_extension => if (self.xt_checksum_report) {
    //     self.terminal.flags.xt_checksum = value.flags;
    // },
    fn xt_checksum_extension(&mut self, v: stream::XtChecksumExtension) {
        if self.xt_checksum_report {
            self.term().flags.xt_checksum = v.flags;
        }
    }

    // .request_xt_checksum => self.reportXtChecksum(value),
    fn request_xt_checksum(&mut self, v: rustty_vt::xt_checksum::Request) {
        self.reportXtChecksum(v);
    }

    // .size_report => self.sendSizeReport(value),
    fn size_report(&mut self, v: csi::SizeReportStyle) {
        self.sendSizeReport(v);
    }

    // .resize_window => self.surfaceMessageWriter(.{ .resize_window = value }),
    fn resize_window(&mut self, v: stream::ResizeWindow) {
        self.surfaceMessageWriter(SurfaceMessage::ResizeWindow(v));
    }

    // .xtversion => try self.reportXtversion(),
    fn xtversion(&mut self) {
        let r = self.reportXtversion();
        self.vt("xtversion", r);
    }

    // .device_attributes => try self.deviceAttributes(value),
    fn device_attributes(&mut self, req: device_attributes::Req) {
        let r = self.deviceAttributes(req);
        self.vt("device_attributes", r);
    }

    // .device_status => try self.deviceStatusReport(value.request),
    fn device_status(&mut self, v: stream::DeviceStatus) {
        let r = self.deviceStatusReport(v.request);
        self.vt("device_status", r);
    }

    // .kitty_keyboard_query => try self.queryKittyKeyboard(),
    fn kitty_keyboard_query(&mut self) {
        let r = self.queryKittyKeyboard();
        self.vt("kitty_keyboard_query", r);
    }

    // .kitty_keyboard_push => {
    //     log.debug("pushing kitty keyboard mode: {}", .{value.flags});
    //     self.terminal.screens.active.kitty_keyboard.push(value.flags);
    // },
    fn kitty_keyboard_push(&mut self, v: stream::KittyKeyboardFlags) {
        // log.debug("pushing kitty keyboard mode: {}", .{value.flags});
        log::debug!(target: LOG_SCOPE, "pushing kitty keyboard mode: {:?}", v.flags);
        self.term().active_mut().kitty_keyboard.push(v.flags);
    }

    // .kitty_keyboard_pop => {
    //     log.debug("popping kitty keyboard mode n={}", .{value});
    //     self.terminal.screens.active.kitty_keyboard.pop(@intCast(value));
    // },
    fn kitty_keyboard_pop(&mut self, v: u16) {
        // log.debug("popping kitty keyboard mode n={}", .{value});
        log::debug!(target: LOG_SCOPE, "popping kitty keyboard mode n={v}");
        self.term().active_mut().kitty_keyboard.pop(v as usize);
    }

    // .kitty_keyboard_set => {
    //     log.debug("setting kitty keyboard mode: set {}", .{value.flags});
    //     self.terminal.screens.active.kitty_keyboard.set(.set, value.flags);
    // },
    fn kitty_keyboard_set(&mut self, v: stream::KittyKeyboardFlags) {
        // log.debug("setting kitty keyboard mode: set {}", .{value.flags});
        log::debug!(target: LOG_SCOPE, "setting kitty keyboard mode: set {:?}", v.flags);
        self.term()
            .active_mut()
            .kitty_keyboard
            .set(kitty::key::SetMode::Set, v.flags);
    }

    // .kitty_keyboard_set_or => {
    //     log.debug("setting kitty keyboard mode: or {}", .{value.flags});
    //     self.terminal.screens.active.kitty_keyboard.set(.@"or", value.flags);
    // },
    fn kitty_keyboard_set_or(&mut self, v: stream::KittyKeyboardFlags) {
        // log.debug("setting kitty keyboard mode: or {}", .{value.flags});
        log::debug!(target: LOG_SCOPE, "setting kitty keyboard mode: or {:?}", v.flags);
        self.term()
            .active_mut()
            .kitty_keyboard
            .set(kitty::key::SetMode::Or, v.flags);
    }

    // .kitty_keyboard_set_not => {
    //     log.debug("setting kitty keyboard mode: not {}", .{value.flags});
    //     self.terminal.screens.active.kitty_keyboard.set(.not, value.flags);
    // },
    fn kitty_keyboard_set_not(&mut self, v: stream::KittyKeyboardFlags) {
        // log.debug("setting kitty keyboard mode: not {}", .{value.flags});
        log::debug!(target: LOG_SCOPE, "setting kitty keyboard mode: not {:?}", v.flags);
        self.term()
            .active_mut()
            .kitty_keyboard
            .set(kitty::key::SetMode::Not, v.flags);
    }

    // .kitty_color_report => try self.kittyColorReport(value),
    fn kitty_color_report(&mut self, v: &kitty::color::OSC) {
        let r = self.kittyColorReport(v);
        self.vt("kitty_color_report", r);
    }

    // .color_operation => try self.colorOperation(value.op, &value.requests, value.terminator),
    fn color_operation(&mut self, v: stream::ColorOperation<'_>) {
        let r = self.colorOperation(v.op, v.requests, v.terminator);
        self.vt("color_operation", r);
    }

    // .end_hyperlink => try self.endHyperlink(),
    fn end_hyperlink(&mut self) {
        let r = self.endHyperlink();
        self.vt("end_hyperlink", r);
    }

    // .active_status_display => self.terminal.status_display = value,
    fn active_status_display(&mut self, v: ansi::StatusDisplay) {
        self.term().status_display = v;
    }

    // .decaln => try self.decaln(),
    fn decaln(&mut self) {
        let r = StreamHandler::decaln(self);
        self.vt("decaln", r);
    }

    // .window_title => try self.windowTitle(value.title),
    fn window_title(&mut self, v: stream::WindowTitle) {
        let r = self.windowTitle(v.title);
        self.vt("window_title", r);
    }

    // .report_pwd => try self.reportPwd(value.url),
    fn report_pwd(&mut self, v: stream::ReportPwd) {
        let r = self.reportPwd(v.url);
        self.vt("report_pwd", r);
    }

    // .show_desktop_notification => try self.showDesktopNotification(value.title, value.body),
    fn show_desktop_notification(&mut self, v: stream::ShowDesktopNotification) {
        let r = self.showDesktopNotification(v.title, v.body);
        self.vt("show_desktop_notification", r);
    }

    // .progress_report => self.progressReport(value),
    fn progress_report(&mut self, v: osc::ProgressReport) {
        self.progressReport(v);
    }

    // .start_hyperlink => try self.startHyperlink(value.uri, value.id),
    fn start_hyperlink(&mut self, v: stream::StartHyperlink) {
        let r = self.startHyperlink(v.uri, v.id);
        self.vt("start_hyperlink", r);
    }

    // .clipboard_contents => try self.clipboardContents(value.kind, value.data),
    fn clipboard_contents(&mut self, v: stream::ClipboardContents) {
        let r = self.clipboardContents(v.kind, v.data);
        self.vt("clipboard_contents", r);
    }

    // .semantic_prompt => try self.semanticPrompt(value),
    fn semantic_prompt(&mut self, v: stream::SemanticPrompt) {
        let r = self.semanticPrompt(v);
        self.vt("semantic_prompt", r);
    }

    // .mouse_shape => try self.setMouseShape(value),
    fn mouse_shape(&mut self, v: rustty_vt::mouse::Shape) {
        let r = self.setMouseShape(v);
        self.vt("mouse_shape", r);
    }

    // .configure_charset => self.configureCharset(value.slot, value.charset),
    fn configure_charset(&mut self, v: stream::ConfigureCharset) {
        self.configureCharset(v.slot, v.charset);
    }

    // .set_attribute => {
    //     @branchHint(.likely);
    //     switch (value) {
    //         .unknown => |unk| {
    //             // We optimize for the happy path scenario here, since
    //             // unknown/invalid SGRs aren't that common in the wild.
    //             @branchHint(.unlikely);
    //             log.warn("unimplemented or unknown SGR attribute: {any}", .{unk});
    //         },
    //         else => {
    //             @branchHint(.likely);
    //             self.terminal.setAttribute(value) catch |err| {
    //                 @branchHint(.cold);
    //                 log.warn("error setting attribute {}: {}", .{ value, err });
    //             };
    //         },
    //     }
    // },
    fn set_attribute(&mut self, attr: rustty_vt::sgr::Attribute) {
        match attr {
            rustty_vt::sgr::Attribute::Unknown(unk) => {
                // We optimize for the happy path scenario here, since
                // unknown/invalid SGRs aren't that common in the wild.
                // log.warn("unimplemented or unknown SGR attribute: {any}", .{unk});
                log::warn!(
                    target: LOG_SCOPE,
                    "unimplemented or unknown SGR attribute: {unk:?}"
                );
            }
            _ => {
                if let Err(err) = self.term().set_attribute(attr) {
                    // log.warn("error setting attribute {}: {}", .{ value, err });
                    log::warn!(
                        target: LOG_SCOPE,
                        "error setting attribute {attr:?}: {err:?}"
                    );
                }
            }
        }
    }

    // .dcs_hook => try self.dcsHook(value),
    fn dcs_hook(&mut self, dcs: rustty_vt::parser::DCS) {
        let r = self.dcsHook(dcs);
        self.vt("dcs_hook", r);
    }

    // .dcs_put => try self.dcsPut(value),
    fn dcs_put(&mut self, byte: u8) {
        let r = self.dcsPut(byte);
        self.vt("dcs_put", r);
    }

    // .dcs_unhook => try self.dcsUnhook(),
    fn dcs_unhook(&mut self) {
        let r = self.dcsUnhook();
        self.vt("dcs_unhook", r);
    }

    // .apc_start => self.apc.start(),
    fn apc_start(&mut self) {
        self.apc.start();
    }

    // .apc_end => try self.apcEnd(),
    fn apc_end(&mut self, _v: rustty_vt::stream::ApcEnd) {
        let r = self.apcEnd();
        self.vt("apc_end", r);
    }

    // .apc_put => self.apc.feed(self.alloc, value),
    fn apc_put(&mut self, byte: u8) {
        let alloc = self.alloc;
        self.apc.feed(alloc.into(), byte);
    }

    // .apc_put_slice => self.apc.feedSlice(self.alloc, value.bytes),
    fn apc_put_slice(&mut self, bytes: &[u8]) {
        let alloc = self.alloc;
        self.apc.feed_slice(alloc.into(), bytes);
    }

    // // Unimplemented
    // .title_push,
    // .title_pop,
    // => {},
    fn title_push(&mut self, _v: u16) {}
    fn title_pop(&mut self, _v: u16) {}

    // .kitty_clipboard => try self.kittyClipboard(value),
    fn kitty_clipboard(&mut self, v: osc::KittyClipboardProtocol) {
        let r = StreamHandler::kittyClipboard(self, &v);
        self.vt("kitty_clipboard", r);
    }

    // Unimplemented
    // .title_push,
    // .title_pop,
    // .kitty_dnd,
    // => {},
    // (af8d28a94, assessment E row P5-E11: the dnd Action is added next to the
    // title_push/title_pop no-ops.)
    fn kitty_dnd(&mut self, _v: osc::KittyDndProtocol) {}
}
