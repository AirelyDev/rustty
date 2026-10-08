//! PORTS: ghostty src/terminal/stream.zig — the `Stream` print, execute,
//! OSC dispatch (`oscDispatch` and the action loop's `osc_dispatch` body),
//! `configureCharset` and `escDispatch` cluster.

use super::*;

impl<H: Handler> Stream<H> {
    /// The action loop's body for an `osc_dispatch` action (see the PORT NOTE
    /// in `next_non_utf8`): the debug log, the raw hook, then `oscDispatch`.
    pub(super) fn run_osc_dispatch_action(handler: &mut H, cmd: Box<osc::Command<'_>>) {
        let action = parser::Action::OscDispatch(cmd);
        if DEBUG {
            log::info!(target: LOG_SCOPE, "action: {action:?}");
        }
        // if (@hasDecl(T, "vtRaw")) { ... } (see the action loop)
        if handler.vt_raw(&action) {
            return;
        }
        // .osc_dispatch => |cmd| self.oscDispatch(cmd),
        let parser::Action::OscDispatch(cmd) = action else {
            unreachable!()
        };
        Self::osc_dispatch(handler, *cmd);
    }

    // inline fn print(self: *Self, c: u21) void {
    pub(super) fn print(handler: &mut H, c: u32) {
        // self.handler.vt(.print, .{ .cp = c });
        handler.print(c);
    }

    // inline fn execute(self: *Self, c: u8) void { ... }
    pub(super) fn execute(handler: &mut H, c: u8) {
        // If the character is > 0x7F, it's a C1 (8-bit) control,
        // which is strictly equivalent to `ESC` plus `c - 0x40`.
        // if (c > 0x7F) { ... self.escDispatch(.{ .intermediates = &.{}, .final = c - 0x40 }); return; }
        if c > 0x7F {
            // @branchHint(.unlikely);
            // log.info("executing C1 0x{x} as ESC {c}", .{ c, c - 0x40 });
            log::info!(
                target: LOG_SCOPE,
                "executing C1 0x{c:x} as ESC {}",
                char::from(c - 0x40)
            );
            Self::esc_dispatch(
                handler,
                parser::ESC {
                    intermediates: parser::InlineVec::new(),
                    final_: c - 0x40,
                },
            );
            return;
        }

        // const c0: ansi.C0 = @enumFromInt(c);
        let c0 = ansi::C0::from_int(c);
        if DEBUG {
            log::info!(target: LOG_SCOPE, "execute: {c0:?}");
        }
        // switch (c0) { ... }
        match c0 {
            // We ignore SOH/STX: https://github.com/microsoft/terminal/issues/10786
            ansi::C0::NUL | ansi::C0::SOH | ansi::C0::STX => {}

            ansi::C0::ENQ => handler.enquiry(),
            ansi::C0::BEL => handler.bell(),
            ansi::C0::BS => handler.backspace(),
            ansi::C0::HT => handler.horizontal_tab(1),
            ansi::C0::LF | ansi::C0::VT | ansi::C0::FF => handler.linefeed(),
            ansi::C0::CR => handler.carriage_return(),
            ansi::C0::SO => handler.invoke_charset(InvokeCharset {
                bank: charsets::ActiveSlot::GL,
                charset: charsets::Slots::G1,
                locking: false,
            }),
            ansi::C0::SI => handler.invoke_charset(InvokeCharset {
                bank: charsets::ActiveSlot::GL,
                charset: charsets::Slots::G0,
                locking: false,
            }),

            // else => logUnsupportedOnce("invalid C0 character, ignoring: 0x{x}", .{c}, c),
            _ => {
                if log_unsupported_once(&SEEN_INVALID_C0, u16::from(c)) {
                    log::warn!(target: LOG_SCOPE, "invalid C0 character, ignoring: 0x{c:x}");
                }
            }
        }
    }

    // inline fn oscDispatch(self: *Self, cmd: osc.Command) void { ... }
    // PORT NOTE (parity tail S7-7): an associated function over the handler,
    // not a method: the command borrows `self.parser`, so the caller passes
    // the disjoint `handler`. The handler receives the borrowed payloads
    // (`title: &[u8]` and so on) exactly as the reference's does.
    pub(super) fn osc_dispatch(handler: &mut H, cmd: osc::Command<'_>) {
        // The branch hints here are based on real world data
        // which indicates that the most common OSC commands are:
        //
        // 1. hyperlink_end
        // 2. change_window_title
        // 3. change_window_icon
        // 4. hyperlink_start
        // 5. report_pwd
        // 6. color_operation
        // 7. semantic_prompt
        //
        // Together, these 7 commands make up about 96% of all
        // OSC commands encountered in real world scenarios.
        //
        // Additionally, within the prongs, unlikely branch
        // hints have been added to branches that deal with
        // invalid sequences/commands, this is in order to
        // optimize for the happy path where we're getting
        // valid data from the program we're running.
        //
        // ref: https://github.com/qwerasd205/asciinema-stats

        // switch (cmd) {
        match cmd {
            // .semantic_prompt => |sp| { ... }
            osc::Command::SemanticPrompt(sp) => {
                // @branchHint(.likely);
                handler.semantic_prompt(sp);
            }

            // .change_window_title => |title| { ... }
            osc::Command::ChangeWindowTitle(title) => {
                // @branchHint(.likely);
                // if (!std.unicode.utf8ValidateSlice(title)) { ... }
                if std::str::from_utf8(&title).is_err() {
                    // @branchHint(.unlikely);
                    // log.warn("change title request: invalid utf-8, ignoring request", .{});
                    log::warn!(
                        target: LOG_SCOPE,
                        "change title request: invalid utf-8, ignoring request"
                    );
                    return;
                }

                handler.window_title(WindowTitle { title: &title });
            }

            // .change_window_icon => |icon| { ... }
            osc::Command::ChangeWindowIcon(icon) => {
                // @branchHint(.likely);
                // logUnsupportedOnce(
                //     "OSC 1 (change icon) received and ignored icon={s}",
                //     .{icon},
                //     0,
                // );
                //
                // b5053153f also moved this site from `log.info` to the
                // `logUnsupportedOnce` (warn) family. The key is a literal 0, so
                // this message is emitted at most ONCE per process regardless of
                // the icon value.
                if log_unsupported_once(&SEEN_OSC1_CHANGE_ICON, 0) {
                    log::warn!(
                        target: LOG_SCOPE,
                        "OSC 1 (change icon) received and ignored icon={}",
                        String::from_utf8_lossy(&icon)
                    );
                }
            }

            // .clipboard_contents => |clip| { ... }
            osc::Command::ClipboardContents {
                kind,
                data,
                terminator,
            } => {
                handler.clipboard_contents(ClipboardContents {
                    kind,
                    data: &data,
                    terminator,
                });
            }

            // .report_pwd => |v| { ... }
            osc::Command::ReportPwd { value } => {
                // @branchHint(.likely);
                handler.report_pwd(ReportPwd { url: &value });
            }

            // .mouse_shape => |v| { ... }
            osc::Command::MouseShape { value } => {
                // const shape = MouseShape.fromString(v.value) orelse { ... };
                let shape = match MouseShape::from_string(&value) {
                    Some(shape) => shape,
                    None => {
                        // @branchHint(.unlikely);
                        // log.warn("unknown cursor shape: {s}", .{v.value});
                        log::warn!(
                            target: LOG_SCOPE,
                            "unknown cursor shape: {}",
                            String::from_utf8_lossy(&value)
                        );
                        return;
                    }
                };

                handler.mouse_shape(shape);
            }

            // .color_operation => |v| { ... }
            osc::Command::ColorOperation {
                op,
                requests,
                terminator,
            } => {
                // @branchHint(.likely);
                handler.color_operation(ColorOperation {
                    op,
                    requests,
                    terminator,
                });
            }

            // .kitty_color_protocol => |v| { ... }
            osc::Command::KittyColorProtocol(v) => {
                handler.kitty_color_report(v);
            }

            // .show_desktop_notification => |v| { ... }
            osc::Command::ShowDesktopNotification { title, body } => {
                handler.show_desktop_notification(ShowDesktopNotification {
                    title: &title,
                    body: &body,
                });
            }

            // .hyperlink_start => |v| { ... }
            osc::Command::HyperlinkStart { id, uri } => {
                // @branchHint(.likely);
                handler.start_hyperlink(StartHyperlink {
                    uri: &uri,
                    id: id.as_deref(),
                });
            }

            // .hyperlink_end => { ... }
            osc::Command::HyperlinkEnd => {
                // @branchHint(.likely);
                handler.end_hyperlink();
            }

            // .conemu_progress_report => |v| { ... }
            osc::Command::ConemuProgressReport(v) => {
                handler.progress_report(v);
            }

            // .conemu_sleep, .conemu_show_message_box, .conemu_change_tab_title,
            // .conemu_wait_input, .conemu_guimacro, .conemu_comment,
            // .conemu_xterm_emulation, .conemu_output_environment_variable,
            // .conemu_run_process, .kitty_text_sizing, .kitty_clipboard_protocol,
            // .kitty_desktop_notification, .context_signal => { ... }
            // .kitty_clipboard_protocol => |v| {
            //     handler.vt(.kitty_clipboard, v);
            // },
            osc::Command::KittyClipboardProtocol(v) => {
                handler.kitty_clipboard(v);
            }

            // .kitty_dnd_protocol => |v| {
            //     handler.vt(.kitty_dnd, v);
            // },
            osc::Command::KittyDndProtocol(v) => {
                handler.kitty_dnd(v);
            }

            // .unknown => |v| {
            //     @branchHint(.unlikely);
            //     self.handler.vt(.osc_unknown, v);
            // },
            // PORT NOTE (P8-W4 T2): the `osc_unknown` action and the stream's
            // `osc_unknown_max_bytes` option land with P8-W5 T2. Until then the
            // stream never enables unknown capture (`unknown_max_bytes` stays
            // 0), so the parser cannot produce this command here.
            osc::Command::Unknown(_) => {
                log::debug!(target: LOG_SCOPE, "unimplemented OSC callback: {cmd:?}");
            }

            osc::Command::ConemuSleep { .. }
            | osc::Command::ConemuShowMessageBox(_)
            | osc::Command::ConemuChangeTabTitle(_)
            | osc::Command::ConemuWaitInput
            | osc::Command::ConemuGuimacro(_)
            | osc::Command::ConemuComment(_)
            | osc::Command::ConemuXtermEmulation { .. }
            | osc::Command::ConemuOutputEnvironmentVariable(_)
            | osc::Command::ConemuRunProcess(_)
            | osc::Command::KittyTextSizing(_)
            | osc::Command::KittyDesktopNotification(_)
            | osc::Command::ContextSignal(_) => {
                // log.debug("unimplemented OSC callback: {}", .{cmd});
                log::debug!(target: LOG_SCOPE, "unimplemented OSC callback: {cmd:?}");
            }

            // .invalid => { ... }
            osc::Command::Invalid => {
                // @branchHint(.cold);
                // This is an invalid internal state, not an invalid OSC
                // string being parsed. We shouldn't see this.
                // log.warn("invalid OSC, should never happen", .{});
                log::warn!(target: LOG_SCOPE, "invalid OSC, should never happen");
            }
        }
    }

    // inline fn configureCharset(self: *Self, intermediates: []const u8, set: charsets.Charset) void { ... }
    fn configure_charset(handler: &mut H, intermediates: &[u8], set: charsets::Charset) {
        // if (intermediates.len != 1) { ... }
        if intermediates.len() != 1 {
            // log.warn("invalid charset intermediate: {any}", .{intermediates});
            log::warn!(target: LOG_SCOPE, "invalid charset intermediate: {intermediates:?}");
            return;
        }

        // const slot: charsets.Slots = switch (intermediates[0]) {
        let slot: charsets::Slots = match intermediates[0] {
            // TODO: support slots '-', '.', '/'
            b'(' => charsets::Slots::G0,
            b')' => charsets::Slots::G1,
            b'*' => charsets::Slots::G2,
            b'+' => charsets::Slots::G3,
            _ => {
                // @branchHint(.unlikely);
                // log.warn("invalid charset intermediate: {any}", .{intermediates});
                log::warn!(target: LOG_SCOPE, "invalid charset intermediate: {intermediates:?}");
                return;
            }
        };

        handler.configure_charset(ConfigureCharset { slot, charset: set });
    }

    // inline fn escDispatch(self: *Self, action: Parser.Action.ESC) void { ... }
    pub(super) fn esc_dispatch(handler: &mut H, action: parser::ESC) {
        // The branch hints here are based on real world data
        // which indicates that the most common ESC finals are:
        //
        // 1. B
        // 2. \
        // 3. 0
        // 4. M
        // 5. 8
        // 6. 7
        // 7. >
        // 8. =
        //
        // Together, these 8 finals make up nearly 99% of all
        // ESC sequences encountered in real world scenarios.
        //
        // Additionally, within the prongs, unlikely branch
        // hints have been added to branches that deal with
        // invalid sequences/commands, this is in order to
        // optimize for the happy path where we're getting
        // valid data from the program we're running.
        //
        // ref: https://github.com/qwerasd205/asciinema-stats

        // switch (action.final) {
        match action.final_ {
            // Charsets
            // 'B' => { ... }
            b'B' => {
                // @branchHint(.likely);
                Self::configure_charset(handler, &action.intermediates, charsets::Charset::Ascii);
            }
            // 'A' => self.configureCharset(action.intermediates, .british),
            b'A' => {
                Self::configure_charset(handler, &action.intermediates, charsets::Charset::British)
            }
            // '0' => { ... }
            b'0' => {
                // @branchHint(.likely);
                Self::configure_charset(
                    handler,
                    &action.intermediates,
                    charsets::Charset::DecSpecial,
                );
            }

            // DECSC - Save Cursor
            // '7' => { ... }
            b'7' => {
                // @branchHint(.likely);
                // switch (action.intermediates.len) {
                match action.intermediates.len() {
                    0 => handler.save_cursor(),
                    _ => {
                        // @branchHint(.unlikely);
                        // log.warn("invalid command: {f}", .{action});
                        log::warn!(target: LOG_SCOPE, "invalid command: {action:?}");
                        #[allow(clippy::needless_return)]
                        return;
                    }
                }
            }

            // '8' => blk: { ... }
            b'8' => 'blk: {
                // @branchHint(.likely);
                // switch (action.intermediates.len) {
                match action.intermediates.len() {
                    // DECRC - Restore Cursor
                    0 => {
                        handler.restore_cursor();
                        break 'blk;
                    }

                    // 1 => switch (action.intermediates[0]) {
                    1 => match action.intermediates[0] {
                        // DECALN - Fill Screen with E
                        b'#' => {
                            handler.decaln();
                            break 'blk;
                        }

                        _ => {}
                    },

                    _ => {} // fall through
                }

                // logUnsupportedOnce(
                //     "unimplemented ESC action: {f}",
                //     .{action},
                //     action.final,
                // );
                if log_unsupported_once(&SEEN_UNIMPLEMENTED_ESC_ACTION, u16::from(action.final_)) {
                    log::warn!(target: LOG_SCOPE, "unimplemented ESC action: {action:?}");
                }
            }

            // IND - Index
            // 'D' => switch (action.intermediates.len) { ... }
            b'D' => match action.intermediates.len() {
                0 => handler.index(),
                _ => {
                    // @branchHint(.unlikely);
                    // log.warn("invalid index command: {f}", .{action});
                    log::warn!(target: LOG_SCOPE, "invalid index command: {action:?}");
                    #[allow(clippy::needless_return)]
                    return;
                }
            },

            // NEL - Next Line
            // 'E' => switch (action.intermediates.len) { ... }
            b'E' => match action.intermediates.len() {
                0 => handler.next_line(),
                _ => {
                    // @branchHint(.unlikely);
                    // log.warn("invalid next line command: {f}", .{action});
                    log::warn!(target: LOG_SCOPE, "invalid next line command: {action:?}");
                    #[allow(clippy::needless_return)]
                    return;
                }
            },

            // HTS - Horizontal Tab Set
            // 'H' => switch (action.intermediates.len) { ... }
            b'H' => match action.intermediates.len() {
                0 => handler.tab_set(),
                _ => {
                    // @branchHint(.unlikely);
                    // log.warn("invalid tab set command: {f}", .{action});
                    log::warn!(target: LOG_SCOPE, "invalid tab set command: {action:?}");
                    #[allow(clippy::needless_return)]
                    return;
                }
            },

            // RI - Reverse Index
            // 'M' => { ... }
            b'M' => {
                // @branchHint(.likely);
                // switch (action.intermediates.len) {
                match action.intermediates.len() {
                    0 => handler.reverse_index(),
                    _ => {
                        // @branchHint(.unlikely);
                        // log.warn("invalid reverse index command: {f}", .{action});
                        log::warn!(target: LOG_SCOPE, "invalid reverse index command: {action:?}");
                        #[allow(clippy::needless_return)]
                        return;
                    }
                }
            }

            // SS2 - Single Shift 2
            // 'N' => switch (action.intermediates.len) { ... }
            b'N' => match action.intermediates.len() {
                0 => handler.invoke_charset(InvokeCharset {
                    bank: charsets::ActiveSlot::GL,
                    charset: charsets::Slots::G2,
                    locking: true,
                }),
                _ => {
                    // @branchHint(.unlikely);
                    // log.warn("invalid single shift 2 command: {f}", .{action});
                    log::warn!(target: LOG_SCOPE, "invalid single shift 2 command: {action:?}");
                    #[allow(clippy::needless_return)]
                    return;
                }
            },

            // SS3 - Single Shift 3
            // 'O' => switch (action.intermediates.len) { ... }
            b'O' => match action.intermediates.len() {
                0 => handler.invoke_charset(InvokeCharset {
                    bank: charsets::ActiveSlot::GL,
                    charset: charsets::Slots::G3,
                    locking: true,
                }),
                _ => {
                    // @branchHint(.unlikely);
                    // log.warn("invalid single shift 3 command: {f}", .{action});
                    log::warn!(target: LOG_SCOPE, "invalid single shift 3 command: {action:?}");
                    #[allow(clippy::needless_return)]
                    return;
                }
            },

            // SPA - Start of Guarded Area
            // 'V' => switch (action.intermediates.len) { ... }
            b'V' => match action.intermediates.len() {
                0 => handler.protected_mode_iso(),
                // else => log.warn("unimplemented ESC callback: {f}", .{action}),
                _ => {
                    log::warn!(target: LOG_SCOPE, "unimplemented ESC callback: {action:?}");
                }
            },

            // EPA - End of Guarded Area
            // 'W' => switch (action.intermediates.len) { ... }
            b'W' => match action.intermediates.len() {
                0 => handler.protected_mode_off(),
                // else => log.warn("unimplemented ESC callback: {f}", .{action}),
                _ => {
                    log::warn!(target: LOG_SCOPE, "unimplemented ESC callback: {action:?}");
                }
            },

            // DECID
            // 'Z' => if (action.intermediates.len == 0) { ... } else log.warn(...),
            b'Z' => {
                if action.intermediates.len() == 0 {
                    handler.device_attributes(device_attributes::Req::Primary);
                } else {
                    // else log.warn("unimplemented ESC callback: {f}", .{action});
                    log::warn!(target: LOG_SCOPE, "unimplemented ESC callback: {action:?}");
                }
            }

            // RIS - Full Reset
            // 'c' => switch (action.intermediates.len) { ... }
            b'c' => match action.intermediates.len() {
                0 => handler.full_reset(),
                _ => {
                    // log.warn("invalid full reset command: {f}", .{action});
                    log::warn!(target: LOG_SCOPE, "invalid full reset command: {action:?}");
                    #[allow(clippy::needless_return)]
                    return;
                }
            },

            // LS2 - Locking Shift 2
            // 'n' => switch (action.intermediates.len) { ... }
            b'n' => match action.intermediates.len() {
                0 => handler.invoke_charset(InvokeCharset {
                    bank: charsets::ActiveSlot::GL,
                    charset: charsets::Slots::G2,
                    locking: false,
                }),
                _ => {
                    // @branchHint(.unlikely);
                    // log.warn("invalid single shift 2 command: {f}", .{action});
                    log::warn!(target: LOG_SCOPE, "invalid single shift 2 command: {action:?}");
                    #[allow(clippy::needless_return)]
                    return;
                }
            },

            // LS3 - Locking Shift 3
            // 'o' => switch (action.intermediates.len) { ... }
            b'o' => match action.intermediates.len() {
                0 => handler.invoke_charset(InvokeCharset {
                    bank: charsets::ActiveSlot::GL,
                    charset: charsets::Slots::G3,
                    locking: false,
                }),
                _ => {
                    // @branchHint(.unlikely);
                    // log.warn("invalid single shift 3 command: {f}", .{action});
                    log::warn!(target: LOG_SCOPE, "invalid single shift 3 command: {action:?}");
                    #[allow(clippy::needless_return)]
                    return;
                }
            },

            // LS1R - Locking Shift 1 Right
            // '~' => switch (action.intermediates.len) { ... }
            b'~' => match action.intermediates.len() {
                0 => handler.invoke_charset(InvokeCharset {
                    bank: charsets::ActiveSlot::GR,
                    charset: charsets::Slots::G1,
                    locking: false,
                }),
                _ => {
                    // @branchHint(.unlikely);
                    // log.warn("invalid locking shift 1 right command: {f}", .{action});
                    log::warn!(
                        target: LOG_SCOPE,
                        "invalid locking shift 1 right command: {action:?}"
                    );
                    #[allow(clippy::needless_return)]
                    return;
                }
            },

            // LS2R - Locking Shift 2 Right
            // '}' => switch (action.intermediates.len) { ... }
            b'}' => match action.intermediates.len() {
                0 => handler.invoke_charset(InvokeCharset {
                    bank: charsets::ActiveSlot::GR,
                    charset: charsets::Slots::G2,
                    locking: false,
                }),
                _ => {
                    // @branchHint(.unlikely);
                    // log.warn("invalid locking shift 2 right command: {f}", .{action});
                    log::warn!(
                        target: LOG_SCOPE,
                        "invalid locking shift 2 right command: {action:?}"
                    );
                    #[allow(clippy::needless_return)]
                    return;
                }
            },

            // LS3R - Locking Shift 3 Right
            // '|' => switch (action.intermediates.len) { ... }
            b'|' => match action.intermediates.len() {
                0 => handler.invoke_charset(InvokeCharset {
                    bank: charsets::ActiveSlot::GR,
                    charset: charsets::Slots::G3,
                    locking: false,
                }),
                _ => {
                    // @branchHint(.unlikely);
                    // log.warn("invalid locking shift 3 right command: {f}", .{action});
                    log::warn!(
                        target: LOG_SCOPE,
                        "invalid locking shift 3 right command: {action:?}"
                    );
                    #[allow(clippy::needless_return)]
                    return;
                }
            },

            // Set application keypad mode
            // '=' => { ... }
            b'=' => {
                // @branchHint(.likely);
                // switch (action.intermediates.len) {
                match action.intermediates.len() {
                    0 => handler.set_mode(Mode {
                        mode: modes::Mode::KeypadKeys,
                    }),
                    // else => log.warn("unimplemented setMode: {f}", .{action}),
                    _ => {
                        log::warn!(target: LOG_SCOPE, "unimplemented setMode: {action:?}");
                    }
                }
            }

            // Reset application keypad mode
            // '>' => { ... }
            b'>' => {
                // @branchHint(.likely);
                // switch (action.intermediates.len) {
                match action.intermediates.len() {
                    0 => handler.reset_mode(Mode {
                        mode: modes::Mode::KeypadKeys,
                    }),
                    // else => log.warn("unimplemented setMode: {f}", .{action}),
                    _ => {
                        log::warn!(target: LOG_SCOPE, "unimplemented setMode: {action:?}");
                    }
                }
            }

            // Sets ST (string terminator). We don't have to do anything
            // because our parser always accepts ST.
            // '\\' => { ... }
            b'\\' => {
                // @branchHint(.likely);
            }

            // else => logUnsupportedOnce(
            //     "unimplemented ESC action: {f}",
            //     .{action},
            //     action.final,
            // ),
            _ => {
                if log_unsupported_once(&SEEN_UNIMPLEMENTED_ESC_ACTION, u16::from(action.final_)) {
                    log::warn!(target: LOG_SCOPE, "unimplemented ESC action: {action:?}");
                }
            }
        }
    }
}
