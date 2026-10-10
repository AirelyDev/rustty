//! PORTS: ghostty src/terminal/stream.zig — the `Stream` CSI dispatch
//! cluster: `csiDispatchFinal` (the fast-path finalizer) and the
//! `csiDispatch` switch, kept whole.

use super::*;

impl<H: Handler> Stream<H> {
    /// Finalize and dispatch a CSI directly from parser state for
    /// the fast paths in nextNonUtf8, without going through
    /// Parser.next. This must match the behavior of the parser's
    /// csi_dispatch action.
    // fn csiDispatchFinal(self: *Self, c: u8) void {
    pub(super) unsafe fn csi_dispatch_final(this: *mut Self, c: u8) {
        // SAFETY (N452, LIFETIMES.tsv `Stream (self: *Self)`): `this` is a live
        // stream (the caller's contract). Every access is a narrow place
        // access through it, and none outlives a handler dispatch.
        unsafe {
            // const p = &self.parser;
            // p.state = .ground;
            let p = &mut (*this).parser;
            p.state = parser::State::Ground;

            // Ignore sequences with too many parameters, matching the
            // parser's behavior of dropping the dispatch entirely.
            // if (p.params_idx >= Parser.MAX_PARAMS) { @branchHint(.unlikely); return; }
            if p.params_idx as usize >= parser::MAX_PARAMS {
                return;
            }

            // Finalize the last parameter if we have one.
            // if (p.param_acc_idx > 0) { p.params[p.params_idx] = p.param_acc; p.params_idx += 1; }
            if p.param_acc_idx > 0 {
                p.params[p.params_idx as usize] = p.param_acc;
                p.params_idx += 1;
            }

            // const action: Parser.Action.CSI = .{
            //     .intermediates = p.intermediates[0..p.intermediates_idx],
            //     .params = p.params[0..p.params_idx],
            //     .params_sep = p.params_sep,
            //     .final = c,
            // };
            //
            // PORT NOTE: the reference's `action` slices into the parser's buffers;
            // rustty's `parser::CSI` owns `InlineVec` prefixes (the `Action` PORT
            // NOTE), so this is the same `InlineVec::from_buf` construction the
            // parser's own `csi_dispatch` arm performs (`parser.rs:807`) — and
            // deliberately so: any divergence between the two would be a
            // fast-path-only behaviour difference, which is precisely what
            // `stream_csi_bulk_params_match_per_byte` exists to catch.
            let action = parser::CSI {
                intermediates: parser::InlineVec::from_buf(p.intermediates, p.intermediates_idx),
                params: parser::InlineVec::from_buf(p.params, p.params_idx),
                params_sep: p.params_sep,
                final_: c,
            };

            // We only allow colon or mixed separators for the 'm' command.
            // if (c != 'm' and p.params_sep.count() > 0) {
            //     @branchHint(.cold);
            //     log.warn(
            //         "CSI colon or mixed separators only allowed for 'm' command, got: {f}",
            //         .{action},
            //     );
            //     return;
            // }
            if c != b'm' && p.params_sep.count() > 0 {
                log::warn!(
                    target: LOG_SCOPE,
                    "CSI colon or mixed separators only allowed for 'm' command, got: {action:?}"
                );
                return;
            }

            if DEBUG {
                log::info!(
                    target: LOG_SCOPE,
                    "action: {:?}",
                    parser::Action::CsiDispatch(Box::new(action.clone()))
                );
            }
            Self::csi_dispatch(&mut (*this).handler, action);
        }
    }

    // inline fn csiDispatch(self: *Self, input: Parser.Action.CSI) void { ... }
    pub(super) fn csi_dispatch(handler: &mut H, input: parser::CSI) {
        // The branch hints here are based on real world data
        // which indicates that the most common CSI finals are:
        //
        // 1. m
        // 2. H
        // 3. K
        // 4. A
        // 5. C
        // 6. X
        // 7. l
        // 8. h
        // 9. r
        //
        // Together, these 9 finals make up about 96% of all
        // CSI sequences encountered in real world scenarios.
        //
        // Additionally, within the prongs, unlikely branch
        // hints have been added to branches that deal with
        // invalid sequences/commands, this is in order to
        // optimize for the happy path where we're getting
        // valid data from the program we're running.
        //
        // ref: https://github.com/qwerasd205/asciinema-stats

        // switch (input.final) {
        match input.final_ {
            // CUU - Cursor Up
            // 'A', 'k' => { ... }
            b'A' | b'k' => {
                // @branchHint(.likely);
                // switch (input.intermediates.len) {
                match input.intermediates.len() {
                    // 0 => self.handler.vt(.cursor_up, .{ .value = ... }),
                    0 => handler.cursor_up(CursorMovement {
                        value: match input.params.len() {
                            0 => 1,
                            1 => input.params[0],
                            _ => {
                                // @branchHint(.unlikely);
                                // log.warn("invalid cursor up command: {f}", .{input});
                                log::warn!(
                                    target: LOG_SCOPE,
                                    "invalid cursor up command: {input:?}"
                                );
                                return;
                            }
                        },
                    }),

                    // else => log.warn("ignoring unimplemented CSI A with intermediates: {s}", ...),
                    _ => {
                        log::warn!(
                            target: LOG_SCOPE,
                            "ignoring unimplemented CSI A with intermediates: {}",
                            String::from_utf8_lossy(&input.intermediates)
                        );
                    }
                }
            }

            // CUD - Cursor Down
            // 'B' => switch (input.intermediates.len) { ... }
            b'B' => match input.intermediates.len() {
                0 => handler.cursor_down(CursorMovement {
                    value: match input.params.len() {
                        0 => 1,
                        1 => input.params[0],
                        _ => {
                            // @branchHint(.unlikely);
                            // log.warn("invalid cursor down command: {f}", .{input});
                            log::warn!(target: LOG_SCOPE, "invalid cursor down command: {input:?}");
                            return;
                        }
                    },
                }),

                // else => log.warn("ignoring unimplemented CSI B with intermediates: {s}", ...),
                _ => {
                    log::warn!(
                        target: LOG_SCOPE,
                        "ignoring unimplemented CSI B with intermediates: {}",
                        String::from_utf8_lossy(&input.intermediates)
                    );
                }
            },

            // CUF - Cursor Right
            // 'C' => { ... }
            b'C' => {
                // @branchHint(.likely);
                // switch (input.intermediates.len) {
                match input.intermediates.len() {
                    0 => handler.cursor_right(CursorMovement {
                        value: match input.params.len() {
                            0 => 1,
                            1 => input.params[0],
                            _ => {
                                // @branchHint(.unlikely);
                                // log.warn("invalid cursor right command: {f}", .{input});
                                log::warn!(
                                    target: LOG_SCOPE,
                                    "invalid cursor right command: {input:?}"
                                );
                                return;
                            }
                        },
                    }),

                    // else => log.warn("ignoring unimplemented CSI C with intermediates: {s}", ...),
                    _ => {
                        log::warn!(
                            target: LOG_SCOPE,
                            "ignoring unimplemented CSI C with intermediates: {}",
                            String::from_utf8_lossy(&input.intermediates)
                        );
                    }
                }
            }

            // CUB - Cursor Left
            // 'D', 'j' => switch (input.intermediates.len) { ... }
            b'D' | b'j' => match input.intermediates.len() {
                0 => handler.cursor_left(CursorMovement {
                    value: match input.params.len() {
                        0 => 1,
                        1 => input.params[0],
                        _ => {
                            // @branchHint(.unlikely);
                            // log.warn("invalid cursor left command: {f}", .{input});
                            log::warn!(target: LOG_SCOPE, "invalid cursor left command: {input:?}");
                            return;
                        }
                    },
                }),

                // else => log.warn("ignoring unimplemented CSI D with intermediates: {s}", ...),
                _ => {
                    log::warn!(
                        target: LOG_SCOPE,
                        "ignoring unimplemented CSI D with intermediates: {}",
                        String::from_utf8_lossy(&input.intermediates)
                    );
                }
            },

            // CNL - Cursor Next Line
            // 'E' => switch (input.intermediates.len) { ... }
            b'E' => match input.intermediates.len() {
                0 => {
                    handler.cursor_down(CursorMovement {
                        value: match input.params.len() {
                            0 => 1,
                            1 => input.params[0],
                            _ => {
                                // @branchHint(.unlikely);
                                // log.warn("invalid cursor up command: {f}", .{input});
                                log::warn!(
                                    target: LOG_SCOPE,
                                    "invalid cursor up command: {input:?}"
                                );
                                return;
                            }
                        },
                    });
                    handler.carriage_return();
                }

                // else => log.warn("ignoring unimplemented CSI E with intermediates: {s}", ...),
                _ => {
                    log::warn!(
                        target: LOG_SCOPE,
                        "ignoring unimplemented CSI E with intermediates: {}",
                        String::from_utf8_lossy(&input.intermediates)
                    );
                }
            },

            // CPL - Cursor Previous Line
            // 'F' => switch (input.intermediates.len) { ... }
            b'F' => match input.intermediates.len() {
                0 => {
                    handler.cursor_up(CursorMovement {
                        value: match input.params.len() {
                            0 => 1,
                            1 => input.params[0],
                            _ => {
                                // @branchHint(.unlikely);
                                // log.warn("invalid cursor down command: {f}", .{input});
                                log::warn!(
                                    target: LOG_SCOPE,
                                    "invalid cursor down command: {input:?}"
                                );
                                return;
                            }
                        },
                    });
                    handler.carriage_return();
                }

                // else => log.warn("ignoring unimplemented CSI F with intermediates: {s}", ...),
                _ => {
                    log::warn!(
                        target: LOG_SCOPE,
                        "ignoring unimplemented CSI F with intermediates: {}",
                        String::from_utf8_lossy(&input.intermediates)
                    );
                }
            },

            // HPA - Cursor Horizontal Position Absolute
            // TODO: test
            // 'G', '`' => switch (input.intermediates.len) { ... }
            b'G' | b'`' => match input.intermediates.len() {
                0 => handler.cursor_col(CursorMovement {
                    value: match input.params.len() {
                        0 => 1,
                        1 => input.params[0],
                        _ => {
                            // @branchHint(.unlikely);
                            // log.warn("invalid HPA command: {f}", .{input});
                            log::warn!(target: LOG_SCOPE, "invalid HPA command: {input:?}");
                            return;
                        }
                    },
                }),

                // else => log.warn("ignoring unimplemented CSI G with intermediates: {s}", ...),
                _ => {
                    log::warn!(
                        target: LOG_SCOPE,
                        "ignoring unimplemented CSI G with intermediates: {}",
                        String::from_utf8_lossy(&input.intermediates)
                    );
                }
            },

            // CUP - Set Cursor Position.
            // TODO: test
            // 'H', 'f' => { ... }
            b'H' | b'f' => {
                // @branchHint(.likely);
                // switch (input.intermediates.len) {
                match input.intermediates.len() {
                    0 => {
                        // const pos: streampkg.Action.CursorPos = switch (input.params.len) {
                        let pos: CursorPos = match input.params.len() {
                            0 => CursorPos { row: 1, col: 1 },
                            1 => CursorPos {
                                row: input.params[0],
                                col: 1,
                            },
                            2 => CursorPos {
                                row: input.params[0],
                                col: input.params[1],
                            },
                            _ => {
                                // @branchHint(.unlikely);
                                // log.warn("invalid CUP command: {f}", .{input});
                                log::warn!(target: LOG_SCOPE, "invalid CUP command: {input:?}");
                                return;
                            }
                        };
                        handler.cursor_pos(pos);
                    }

                    // else => log.warn("ignoring unimplemented CSI H with intermediates: {s}", ...),
                    _ => {
                        log::warn!(
                            target: LOG_SCOPE,
                            "ignoring unimplemented CSI H with intermediates: {}",
                            String::from_utf8_lossy(&input.intermediates)
                        );
                    }
                }
            }

            // CHT - Cursor Horizontal Tabulation
            // 'I' => switch (input.intermediates.len) { ... }
            b'I' => match input.intermediates.len() {
                0 => handler.horizontal_tab(match input.params.len() {
                    0 => 1,
                    1 => input.params[0],
                    _ => {
                        // log.warn("invalid horizontal tab command: {f}", .{input});
                        log::warn!(target: LOG_SCOPE, "invalid horizontal tab command: {input:?}");
                        return;
                    }
                }),

                // else => log.warn("ignoring unimplemented CSI I with intermediates: {s}", ...),
                _ => {
                    log::warn!(
                        target: LOG_SCOPE,
                        "ignoring unimplemented CSI I with intermediates: {}",
                        String::from_utf8_lossy(&input.intermediates)
                    );
                }
            },

            // Erase Display
            // 'J' => { ... }
            b'J' => {
                // const protected_: ?bool = switch (input.intermediates.len) {
                let protected_: Option<bool> = match input.intermediates.len() {
                    0 => Some(false),
                    1 => {
                        if input.intermediates[0] == b'?' {
                            Some(true)
                        } else {
                            None
                        }
                    }
                    _ => None,
                };

                // const protected = protected_ orelse { ... };
                let protected = match protected_ {
                    Some(p) => p,
                    None => {
                        // log.warn("invalid erase display command: {f}", .{input});
                        log::warn!(target: LOG_SCOPE, "invalid erase display command: {input:?}");
                        return;
                    }
                };

                // const mode_: ?csi.EraseDisplay = switch (input.params.len) {
                let mode_: Option<csi::EraseDisplay> = match input.params.len() {
                    0 => Some(csi::EraseDisplay::Below),
                    // 1 => std.meta.intToEnum(csi.EraseDisplay, input.params[0]) catch null,
                    1 => match input.params[0] {
                        0 => Some(csi::EraseDisplay::Below),
                        1 => Some(csi::EraseDisplay::Above),
                        2 => Some(csi::EraseDisplay::Complete),
                        3 => Some(csi::EraseDisplay::Scrollback),
                        22 => Some(csi::EraseDisplay::ScrollComplete),
                        _ => None,
                    },
                    _ => None,
                };

                // const mode = mode_ orelse { ... };
                let mode = match mode_ {
                    Some(m) => m,
                    None => {
                        // log.warn("invalid erase display command: {f}", .{input});
                        log::warn!(target: LOG_SCOPE, "invalid erase display command: {input:?}");
                        return;
                    }
                };

                // switch (mode) {
                match mode {
                    csi::EraseDisplay::Below => handler.erase_display_below(protected),
                    csi::EraseDisplay::Above => handler.erase_display_above(protected),
                    csi::EraseDisplay::Complete => handler.erase_display_complete(protected),
                    csi::EraseDisplay::Scrollback => handler.erase_display_scrollback(protected),
                    csi::EraseDisplay::ScrollComplete => {
                        handler.erase_display_scroll_complete(protected)
                    }
                }
            }

            // Erase Line
            // 'K' => { ... }
            b'K' => {
                // @branchHint(.likely);
                // const protected_: ?bool = switch (input.intermediates.len) {
                let protected_: Option<bool> = match input.intermediates.len() {
                    0 => Some(false),
                    1 => {
                        if input.intermediates[0] == b'?' {
                            Some(true)
                        } else {
                            None
                        }
                    }
                    _ => None,
                };

                // const protected = protected_ orelse { ... };
                let protected = match protected_ {
                    Some(p) => p,
                    None => {
                        // @branchHint(.unlikely);
                        // log.warn("invalid erase line command: {f}", .{input});
                        log::warn!(target: LOG_SCOPE, "invalid erase line command: {input:?}");
                        return;
                    }
                };

                // const mode_: ?csi.EraseLine = switch (input.params.len) {
                let mode_: Option<csi::EraseLine> = match input.params.len() {
                    0 => Some(csi::EraseLine::Right),
                    // 1 => if (input.params[0] < 3) @enumFromInt(input.params[0]) else null,
                    1 => {
                        if input.params[0] < 3 {
                            Some(match input.params[0] {
                                0 => csi::EraseLine::Right,
                                1 => csi::EraseLine::Left,
                                2 => csi::EraseLine::Complete,
                                _ => csi::EraseLine::Other(input.params[0] as u8),
                            })
                        } else {
                            None
                        }
                    }
                    _ => None,
                };

                // const mode = mode_ orelse { ... };
                let mode = match mode_ {
                    Some(m) => m,
                    None => {
                        // @branchHint(.unlikely);
                        // log.warn("invalid erase line command: {f}", .{input});
                        log::warn!(target: LOG_SCOPE, "invalid erase line command: {input:?}");
                        return;
                    }
                };

                // switch (mode) {
                match mode {
                    csi::EraseLine::Right => handler.erase_line_right(protected),
                    csi::EraseLine::Left => handler.erase_line_left(protected),
                    csi::EraseLine::Complete => handler.erase_line_complete(protected),
                    csi::EraseLine::RightUnlessPendingWrap => {
                        handler.erase_line_right_unless_pending_wrap(protected)
                    }
                    // _ => { ... }
                    csi::EraseLine::Other(_) => {
                        // @branchHint(.unlikely);
                        // log.warn("invalid erase line mode: {}", .{mode});
                        log::warn!(target: LOG_SCOPE, "invalid erase line mode: {mode:?}");
                    }
                }
            }

            // IL - Insert Lines
            // TODO: test
            // 'L' => switch (input.intermediates.len) { ... }
            b'L' => match input.intermediates.len() {
                0 => handler.insert_lines(match input.params.len() {
                    0 => 1,
                    1 => input.params[0] as usize,
                    _ => {
                        // log.warn("invalid IL command: {f}", .{input});
                        log::warn!(target: LOG_SCOPE, "invalid IL command: {input:?}");
                        return;
                    }
                }),

                // else => log.warn("ignoring unimplemented CSI L with intermediates: {s}", ...),
                _ => {
                    log::warn!(
                        target: LOG_SCOPE,
                        "ignoring unimplemented CSI L with intermediates: {}",
                        String::from_utf8_lossy(&input.intermediates)
                    );
                }
            },

            // DL - Delete Lines
            // TODO: test
            // 'M' => switch (input.intermediates.len) { ... }
            b'M' => match input.intermediates.len() {
                0 => handler.delete_lines(match input.params.len() {
                    0 => 1,
                    1 => input.params[0] as usize,
                    _ => {
                        // log.warn("invalid DL command: {f}", .{input});
                        log::warn!(target: LOG_SCOPE, "invalid DL command: {input:?}");
                        return;
                    }
                }),

                // else => log.warn("ignoring unimplemented CSI M with intermediates: {s}", ...),
                _ => {
                    log::warn!(
                        target: LOG_SCOPE,
                        "ignoring unimplemented CSI M with intermediates: {}",
                        String::from_utf8_lossy(&input.intermediates)
                    );
                }
            },

            // Delete Character (DCH)
            // 'P' => switch (input.intermediates.len) { ... }
            b'P' => match input.intermediates.len() {
                0 => handler.delete_chars(match input.params.len() {
                    0 => 1,
                    1 => input.params[0] as usize,
                    _ => {
                        // log.warn("invalid delete characters command: {f}", .{input});
                        log::warn!(
                            target: LOG_SCOPE,
                            "invalid delete characters command: {input:?}"
                        );
                        return;
                    }
                }),

                // else => log.warn("ignoring unimplemented CSI P with intermediates: {s}", ...),
                _ => {
                    log::warn!(
                        target: LOG_SCOPE,
                        "ignoring unimplemented CSI P with intermediates: {}",
                        String::from_utf8_lossy(&input.intermediates)
                    );
                }
            },

            // Scroll Up (SD)
            // 'S' => switch (input.intermediates.len) { ... }
            b'S' => match input.intermediates.len() {
                0 => handler.scroll_up(match input.params.len() {
                    0 => 1,
                    1 => input.params[0] as usize,
                    _ => {
                        // log.warn("invalid scroll up command: {f}", .{input});
                        log::warn!(target: LOG_SCOPE, "invalid scroll up command: {input:?}");
                        return;
                    }
                }),

                // else => log.warn("ignoring unimplemented CSI S with intermediates: {s}", ...),
                _ => {
                    log::warn!(
                        target: LOG_SCOPE,
                        "ignoring unimplemented CSI S with intermediates: {}",
                        String::from_utf8_lossy(&input.intermediates)
                    );
                }
            },

            // Scroll Down (SD)
            // 'T' => switch (input.intermediates.len) { ... }
            b'T' => match input.intermediates.len() {
                0 => handler.scroll_down(match input.params.len() {
                    0 => 1,
                    1 => input.params[0] as usize,
                    _ => {
                        // log.warn("invalid scroll down command: {f}", .{input});
                        log::warn!(target: LOG_SCOPE, "invalid scroll down command: {input:?}");
                        return;
                    }
                }),

                // else => log.warn("ignoring unimplemented CSI T with intermediates: {s}", ...),
                _ => {
                    log::warn!(
                        target: LOG_SCOPE,
                        "ignoring unimplemented CSI T with intermediates: {}",
                        String::from_utf8_lossy(&input.intermediates)
                    );
                }
            },

            // Cursor Tabulation Control
            // 'W' => switch (input.intermediates.len) { ... }
            b'W' => match input.intermediates.len() {
                0 => {
                    // if (input.params.len == 0 or
                    //     (input.params.len == 1 and input.params[0] == 0))
                    if input.params.len() == 0 || (input.params.len() == 1 && input.params[0] == 0)
                    {
                        handler.tab_set();
                        return;
                    }

                    // switch (input.params.len) {
                    match input.params.len() {
                        // 0 => unreachable,
                        0 => unreachable!(),

                        // 1 => switch (input.params[0]) {
                        1 => match input.params[0] {
                            // 0 => unreachable,
                            0 => unreachable!(),

                            2 => handler.tab_clear_current(),

                            5 => handler.tab_clear_all(),

                            _ => {}
                        },

                        _ => {}
                    }

                    // log.warn("invalid cursor tabulation control: {f}", .{input});
                    log::warn!(target: LOG_SCOPE, "invalid cursor tabulation control: {input:?}");
                    #[allow(clippy::needless_return)]
                    return;
                }

                // 1 => if (...) { self.handler.vt(.tab_reset, {}); } else log.warn(...),
                1 => {
                    if input.intermediates[0] == b'?'
                        && input.params.len() == 1
                        && input.params[0] == 5
                    {
                        handler.tab_reset();
                    } else {
                        // else log.warn("invalid cursor tabulation control: {f}", .{input});
                        log::warn!(
                            target: LOG_SCOPE,
                            "invalid cursor tabulation control: {input:?}"
                        );
                    }
                }

                // else => log.warn("ignoring unimplemented CSI W with intermediates: {s}", ...),
                _ => {
                    log::warn!(
                        target: LOG_SCOPE,
                        "ignoring unimplemented CSI W with intermediates: {}",
                        String::from_utf8_lossy(&input.intermediates)
                    );
                }
            },

            // Erase Characters (ECH)
            // 'X' => { ... }
            b'X' => {
                // @branchHint(.likely);
                // switch (input.intermediates.len) {
                match input.intermediates.len() {
                    0 => handler.erase_chars(match input.params.len() {
                        0 => 1,
                        1 => input.params[0] as usize,
                        _ => {
                            // @branchHint(.unlikely);
                            // log.warn("invalid erase characters command: {f}", .{input});
                            log::warn!(
                                target: LOG_SCOPE,
                                "invalid erase characters command: {input:?}"
                            );
                            return;
                        }
                    }),

                    // else => log.warn("ignoring unimplemented CSI X with intermediates: {s}", ...),
                    _ => {
                        log::warn!(
                            target: LOG_SCOPE,
                            "ignoring unimplemented CSI X with intermediates: {}",
                            String::from_utf8_lossy(&input.intermediates)
                        );
                    }
                }
            }

            // CHT - Cursor Horizontal Tabulation Back
            // 'Z' => switch (input.intermediates.len) { ... }
            b'Z' => match input.intermediates.len() {
                0 => handler.horizontal_tab_back(match input.params.len() {
                    0 => 1,
                    1 => input.params[0],
                    _ => {
                        // log.warn("invalid horizontal tab back command: {f}", .{input});
                        log::warn!(
                            target: LOG_SCOPE,
                            "invalid horizontal tab back command: {input:?}"
                        );
                        return;
                    }
                }),

                // else => log.warn("ignoring unimplemented CSI Z with intermediates: {s}", ...),
                _ => {
                    log::warn!(
                        target: LOG_SCOPE,
                        "ignoring unimplemented CSI Z with intermediates: {}",
                        String::from_utf8_lossy(&input.intermediates)
                    );
                }
            },

            // HPR - Cursor Horizontal Position Relative
            // 'a' => switch (input.intermediates.len) { ... }
            b'a' => match input.intermediates.len() {
                0 => handler.cursor_col_relative(CursorMovement {
                    value: match input.params.len() {
                        0 => 1,
                        1 => input.params[0],
                        _ => {
                            // log.warn("invalid HPR command: {f}", .{input});
                            log::warn!(target: LOG_SCOPE, "invalid HPR command: {input:?}");
                            return;
                        }
                    },
                }),

                // else => log.warn("ignoring unimplemented CSI a with intermediates: {s}", ...),
                _ => {
                    log::warn!(
                        target: LOG_SCOPE,
                        "ignoring unimplemented CSI a with intermediates: {}",
                        String::from_utf8_lossy(&input.intermediates)
                    );
                }
            },

            // Repeat Previous Char (REP)
            // 'b' => switch (input.intermediates.len) { ... }
            b'b' => match input.intermediates.len() {
                0 => handler.print_repeat(match input.params.len() {
                    0 => 1,
                    // (u16 param widens into the reference's `usize` payload.)
                    1 => usize::from(input.params[0]),
                    _ => {
                        // log.warn("invalid print repeat command: {f}", .{input});
                        log::warn!(target: LOG_SCOPE, "invalid print repeat command: {input:?}");
                        return;
                    }
                }),

                // else => log.warn("ignoring unimplemented CSI b with intermediates: {s}", ...),
                _ => {
                    log::warn!(
                        target: LOG_SCOPE,
                        "ignoring unimplemented CSI b with intermediates: {}",
                        String::from_utf8_lossy(&input.intermediates)
                    );
                }
            },

            // c - Device Attributes (DA1)
            // 'c' => { ... }
            b'c' => {
                // const req: ?device_attributes.Req = switch (input.intermediates.len) {
                let req: Option<device_attributes::Req> = match input.intermediates.len() {
                    0 => Some(device_attributes::Req::Primary),
                    1 => match input.intermediates[0] {
                        b'>' => Some(device_attributes::Req::Secondary),
                        b'=' => Some(device_attributes::Req::Tertiary),
                        _ => None,
                    },
                    _ => None,
                };

                // if (req) |r| { ... } else { ... }
                if let Some(r) = req {
                    handler.device_attributes(r);
                } else {
                    // logUnsupportedOnce(
                    //     "invalid device attributes command: {f}",
                    //     .{input},
                    //     if (input.params.len > 0) input.params[0] else 0,
                    // );
                    if log_unsupported_once(
                        &SEEN_INVALID_DEVICE_ATTRIBUTES,
                        if input.params.len() > 0 {
                            input.params[0]
                        } else {
                            0
                        },
                    ) {
                        log::warn!(
                            target: LOG_SCOPE,
                            "invalid device attributes command: {input:?}"
                        );
                    }
                    #[allow(clippy::needless_return)]
                    return;
                }
            }

            // VPA - Cursor Vertical Position Absolute
            // 'd' => switch (input.intermediates.len) { ... }
            b'd' => match input.intermediates.len() {
                0 => handler.cursor_row(CursorMovement {
                    value: match input.params.len() {
                        0 => 1,
                        1 => input.params[0],
                        _ => {
                            // log.warn("invalid VPA command: {f}", .{input});
                            log::warn!(target: LOG_SCOPE, "invalid VPA command: {input:?}");
                            return;
                        }
                    },
                }),

                // else => log.warn("ignoring unimplemented CSI d with intermediates: {s}", ...),
                _ => {
                    log::warn!(
                        target: LOG_SCOPE,
                        "ignoring unimplemented CSI d with intermediates: {}",
                        String::from_utf8_lossy(&input.intermediates)
                    );
                }
            },

            // VPR - Cursor Vertical Position Relative
            // 'e' => switch (input.intermediates.len) { ... }
            b'e' => match input.intermediates.len() {
                0 => handler.cursor_row_relative(CursorMovement {
                    value: match input.params.len() {
                        0 => 1,
                        1 => input.params[0],
                        _ => {
                            // log.warn("invalid VPR command: {f}", .{input});
                            log::warn!(target: LOG_SCOPE, "invalid VPR command: {input:?}");
                            return;
                        }
                    },
                }),

                // else => log.warn("ignoring unimplemented CSI e with intermediates: {s}", ...),
                _ => {
                    log::warn!(
                        target: LOG_SCOPE,
                        "ignoring unimplemented CSI e with intermediates: {}",
                        String::from_utf8_lossy(&input.intermediates)
                    );
                }
            },

            // TBC - Tab Clear
            // TODO: test
            // 'g' => switch (input.intermediates.len) { ... }
            b'g' => match input.intermediates.len() {
                0 => {
                    // const mode: csi.TabClear = switch (input.params.len) {
                    let mode: csi::TabClear = match input.params.len() {
                        // 1 => std.meta.intToEnum(csi.TabClear, input.params[0]) catch { ... },
                        1 => match u8::try_from(input.params[0]) {
                            Ok(v) => match v {
                                0 => csi::TabClear::Current,
                                3 => csi::TabClear::All,
                                _ => csi::TabClear::Other(v),
                            },
                            Err(_) => {
                                // log.warn("invalid tab clear mode: {}", .{input.params[0]});
                                log::warn!(
                                    target: LOG_SCOPE,
                                    "invalid tab clear mode: {}",
                                    input.params[0]
                                );
                                return;
                            }
                        },
                        _ => {
                            // log.warn("invalid tab clear command: {f}", .{input});
                            log::warn!(target: LOG_SCOPE, "invalid tab clear command: {input:?}");
                            return;
                        }
                    };
                    // switch (mode) {
                    match mode {
                        csi::TabClear::Current => handler.tab_clear_current(),
                        csi::TabClear::All => handler.tab_clear_all(),
                        // _ => log.warn("unknown tab clear mode: {}", .{mode}),
                        csi::TabClear::Other(_) => {
                            log::warn!(target: LOG_SCOPE, "unknown tab clear mode: {mode:?}");
                        }
                    }
                }

                // else => log.warn("ignoring unimplemented CSI g with intermediates: {s}", ...),
                _ => {
                    log::warn!(
                        target: LOG_SCOPE,
                        "ignoring unimplemented CSI g with intermediates: {}",
                        String::from_utf8_lossy(&input.intermediates)
                    );
                }
            },

            // SM - Set Mode
            // 'h' => mode: { ... }
            b'h' => 'mode: {
                // @branchHint(.likely);
                // const ansi_mode = ansi: { ... };
                let ansi_mode = 'ansi: {
                    if input.intermediates.len() == 0 {
                        break 'ansi true;
                    }
                    if input.intermediates.len() == 1 && input.intermediates[0] == b'?' {
                        break 'ansi false;
                    }

                    // log.warn("invalid set mode command: {f}", .{input});
                    log::warn!(target: LOG_SCOPE, "invalid set mode command: {input:?}");
                    break 'mode;
                };

                // for (input.params) |mode_int| {
                for &mode_int in input.params.iter() {
                    // if (modes.modeFromInt(mode_int, ansi_mode)) |mode| {
                    if let Some(mode) = modes::mode_from_int(mode_int, ansi_mode) {
                        handler.set_mode(Mode { mode });
                    } else {
                        // logUnsupportedOnce("unimplemented mode: {}", .{mode_int}, mode_int);
                        if log_unsupported_once(&SEEN_UNIMPLEMENTED_MODE, mode_int) {
                            log::warn!(target: LOG_SCOPE, "unimplemented mode: {mode_int}");
                        }
                    }
                }
            }

            // RM - Reset Mode
            // 'l' => mode: { ... }
            b'l' => 'mode: {
                // @branchHint(.likely);
                // const ansi_mode = ansi: { ... };
                let ansi_mode = 'ansi: {
                    if input.intermediates.len() == 0 {
                        break 'ansi true;
                    }
                    if input.intermediates.len() == 1 && input.intermediates[0] == b'?' {
                        break 'ansi false;
                    }

                    // log.warn("invalid set mode command: {f}", .{input});
                    log::warn!(target: LOG_SCOPE, "invalid set mode command: {input:?}");
                    break 'mode;
                };

                // for (input.params) |mode_int| {
                for &mode_int in input.params.iter() {
                    // if (modes.modeFromInt(mode_int, ansi_mode)) |mode| {
                    if let Some(mode) = modes::mode_from_int(mode_int, ansi_mode) {
                        handler.reset_mode(Mode { mode });
                    } else {
                        // logUnsupportedOnce("unimplemented mode: {}", .{mode_int}, mode_int);
                        if log_unsupported_once(&SEEN_UNIMPLEMENTED_MODE, mode_int) {
                            log::warn!(target: LOG_SCOPE, "unimplemented mode: {mode_int}");
                        }
                    }
                }
            }

            // SGR - Select Graphic Rendition
            // 'm' => { ... }
            b'm' => {
                // @branchHint(.likely);
                // switch (input.intermediates.len) {
                match input.intermediates.len() {
                    0 => {
                        // This is the most common case.
                        // @branchHint(.likely);
                        // log.info("parse SGR params={any}", .{input.params});
                        // var p: sgr.Parser = .{ .params = ..., .params_sep = ... };
                        // PORT NOTE: the reference passes the single `SepList` type
                        // straight through. In the port, `Parser.Action.CSI.SepList`
                        // (`parser::SepList`) and the `sgr` module's structurally
                        // identical local seam (`sgr::SepList`) are still distinct
                        // Rust types (see the PORT NOTE on `parser::SepList`); bridge
                        // them by copying the set separator bits through the public
                        // `is_set`/`set` API so the SGR parser observes the identical
                        // separator set.
                        let mut params_sep = sgr::SepList::init_empty();
                        for i in 0..parser::MAX_PARAMS {
                            if input.params_sep.is_set(i) {
                                params_sep.set(i);
                            }
                        }
                        let mut p: sgr::Parser = sgr::Parser {
                            params: &input.params,
                            params_sep,
                            idx: 0,
                        };
                        // while (p.next()) |attr| {
                        while let Some(attr) = p.next() {
                            // log.info("SGR attribute: {}", .{attr});
                            handler.set_attribute(attr);
                        }
                    }

                    // 1 => switch (input.intermediates[0]) {
                    1 => match input.intermediates[0] {
                        // '>' => blk: { ... }
                        b'>' => 'blk: {
                            // if (input.params.len == 0) { ... }
                            if input.params.len() == 0 {
                                // Reset
                                handler.modify_key_format(ansi::ModifyKeyFormat::Legacy);
                                break 'blk;
                            }

                            // var format: ansi.ModifyKeyFormat = switch (input.params[0]) {
                            let mut format: ansi::ModifyKeyFormat = match input.params[0] {
                                0 => ansi::ModifyKeyFormat::Legacy,
                                1 => ansi::ModifyKeyFormat::CursorKeys,
                                2 => ansi::ModifyKeyFormat::FunctionKeys,
                                4 => ansi::ModifyKeyFormat::OtherKeysNone,
                                _ => {
                                    // @branchHint(.unlikely);
                                    // log.warn("invalid setModifyKeyFormat: {f}", .{input});
                                    log::warn!(
                                        target: LOG_SCOPE,
                                        "invalid setModifyKeyFormat: {input:?}"
                                    );
                                    break 'blk;
                                }
                            };

                            // if (input.params.len > 2) { ... }
                            if input.params.len() > 2 {
                                // @branchHint(.unlikely);
                                // log.warn("invalid setModifyKeyFormat: {f}", .{input});
                                log::warn!(
                                    target: LOG_SCOPE,
                                    "invalid setModifyKeyFormat: {input:?}"
                                );
                                break 'blk;
                            }

                            // if (input.params.len == 2) {
                            if input.params.len() == 2 {
                                // switch (format) {
                                match format {
                                    // We don't support any of the subparams yet for these.
                                    ansi::ModifyKeyFormat::Legacy => {}
                                    ansi::ModifyKeyFormat::CursorKeys => {}
                                    ansi::ModifyKeyFormat::FunctionKeys => {}

                                    // We only support the numeric form.
                                    // .other_keys_none => switch (input.params[1]) {
                                    ansi::ModifyKeyFormat::OtherKeysNone => match input.params[1] {
                                        2 => format = ansi::ModifyKeyFormat::OtherKeysNumeric,
                                        _ => {}
                                    },
                                    ansi::ModifyKeyFormat::OtherKeysNumericExcept => {}
                                    ansi::ModifyKeyFormat::OtherKeysNumeric => {}
                                }
                            }

                            handler.modify_key_format(format);
                        }

                        // else => logUnsupportedOnce(
                        //     "unknown CSI m with intermediate: {}",
                        //     .{input.intermediates[0]},
                        //     input.intermediates[0],
                        // ),
                        _ => {
                            if log_unsupported_once(
                                &SEEN_UNKNOWN_CSI_M_INTERMEDIATE,
                                u16::from(input.intermediates[0]),
                            ) {
                                log::warn!(
                                    target: LOG_SCOPE,
                                    "unknown CSI m with intermediate: {}",
                                    input.intermediates[0]
                                );
                            }
                        }
                    },

                    _ => {
                        // Nothing, but I wanted a place to put this comment:
                        // there are others forms of CSI m that have intermediates.
                        // `vim --clean` uses `CSI ? 4 m` and I don't know what
                        // that means.
                        // log.warn("ignoring unimplemented CSI m with intermediates: {s}", ...);
                        log::warn!(
                            target: LOG_SCOPE,
                            "ignoring unimplemented CSI m with intermediates: {}",
                            String::from_utf8_lossy(&input.intermediates)
                        );
                    }
                }
            }

            // TODO: test
            // 'n' => { ... }
            b'n' => {
                // Handle deviceStatusReport first
                // if (input.intermediates.len == 0 or input.intermediates[0] == '?')
                if input.intermediates.len() == 0 || input.intermediates[0] == b'?' {
                    // if (input.params.len != 1) { ... }
                    if input.params.len() != 1 {
                        // log.warn("invalid device status report command: {f}", .{input});
                        log::warn!(
                            target: LOG_SCOPE,
                            "invalid device status report command: {input:?}"
                        );
                        return;
                    }

                    // const question = question: { ... };
                    let question = 'question: {
                        if input.intermediates.len() == 0 {
                            break 'question false;
                        }
                        if input.intermediates.len() == 1 && input.intermediates[0] == b'?' {
                            break 'question true;
                        }

                        // log.warn("invalid set mode command: {f}", .{input});
                        log::warn!(target: LOG_SCOPE, "invalid set mode command: {input:?}");
                        return;
                    };

                    // const req = device_status.reqFromInt(input.params[0], question) orelse { ... };
                    let req = match device_status::req_from_int(input.params[0], question) {
                        Some(req) => req,
                        None => {
                            // log.warn("invalid device status report command: {f}", .{input});
                            log::warn!(
                                target: LOG_SCOPE,
                                "invalid device status report command: {input:?}"
                            );
                            return;
                        }
                    };

                    handler.device_status(DeviceStatus { request: req });
                    return;
                }

                // Handle other forms of CSI n
                // switch (input.intermediates.len) {
                match input.intermediates.len() {
                    // 0 => unreachable, // handled above
                    0 => unreachable!(), // handled above

                    // 1 => switch (input.intermediates[0]) {
                    1 => match input.intermediates[0] {
                        b'>' => {
                            // This isn't strictly correct. CSI > n has parameters that
                            // control what exactly is being disabled. However, we
                            // only support reverting back to modify other keys in
                            // numeric except format.
                            handler
                                .modify_key_format(ansi::ModifyKeyFormat::OtherKeysNumericExcept);
                        }

                        // else => log.warn("unknown CSI n with intermediate: {}", ...),
                        _ => {
                            log::warn!(
                                target: LOG_SCOPE,
                                "unknown CSI n with intermediate: {}",
                                input.intermediates[0]
                            );
                        }
                    },

                    // else => log.warn("ignoring unimplemented CSI n with intermediates: {s}", ...),
                    _ => {
                        log::warn!(
                            target: LOG_SCOPE,
                            "ignoring unimplemented CSI n with intermediates: {}",
                            String::from_utf8_lossy(&input.intermediates)
                        );
                    }
                }
            }

            // DECRQM - Request Mode, DECSTR - Soft Terminal Reset
            // 'p' => switch (input.intermediates.len) { ... }
            b'p' => match input.intermediates.len() {
                // 1, 2 => decrqm: { ... }
                1 | 2 => 'decrqm: {
                    // DECSTR - Soft Terminal Reset
                    // if (input.intermediates.len == 1 and
                    //     input.intermediates[0] == '!')
                    // { ... }
                    if input.intermediates.len() == 1 && input.intermediates[0] == b'!' {
                        // if (input.params.len != 0) {
                        //     log.warn("invalid DECSTR command: {f}", .{input});
                        //     break :decrqm;
                        // }
                        if !input.params.is_empty() {
                            log::warn!(target: LOG_SCOPE, "invalid DECSTR command: {input:?}");
                            break 'decrqm;
                        }

                        // self.handler.vt(.soft_reset, {});
                        handler.soft_reset();
                        break 'decrqm;
                    }

                    // const ansi_mode = ansi: { ... };
                    let ansi_mode = 'ansi: {
                        // switch (input.intermediates.len) {
                        match input.intermediates.len() {
                            1 => {
                                if input.intermediates[0] == b'$' {
                                    break 'ansi true;
                                }
                            }
                            2 => {
                                if input.intermediates[0] == b'?' && input.intermediates[1] == b'$'
                                {
                                    break 'ansi false;
                                }
                            }
                            _ => {}
                        }

                        // log.warn("ignoring unimplemented CSI p with intermediates: {s}", ...);
                        log::warn!(
                            target: LOG_SCOPE,
                            "ignoring unimplemented CSI p with intermediates: {}",
                            String::from_utf8_lossy(&input.intermediates)
                        );
                        break 'decrqm;
                    };

                    // if (input.params.len != 1) { ... }
                    if input.params.len() != 1 {
                        // log.warn("invalid DECRQM command: {f}", .{input});
                        log::warn!(target: LOG_SCOPE, "invalid DECRQM command: {input:?}");
                        break 'decrqm;
                    }

                    // const mode_raw = input.params[0];
                    let mode_raw = input.params[0];
                    // const mode = modes.modeFromInt(mode_raw, ansi_mode);
                    let mode = modes::mode_from_int(mode_raw, ansi_mode);
                    // if (mode) |m| { ... } else { ... }
                    if let Some(m) = mode {
                        handler.request_mode(Mode { mode: m });
                    } else {
                        handler.request_mode_unknown(RawMode {
                            mode: mode_raw,
                            ansi: ansi_mode,
                        });
                    }
                }

                // else => log.warn("ignoring unimplemented CSI p with intermediates: {s}", ...),
                _ => {
                    log::warn!(
                        target: LOG_SCOPE,
                        "ignoring unimplemented CSI p with intermediates: {}",
                        String::from_utf8_lossy(&input.intermediates)
                    );
                }
            },

            // 'q' => switch (input.intermediates.len) { ... }
            b'q' => match input.intermediates.len() {
                // 1 => switch (input.intermediates[0]) {
                1 => match input.intermediates[0] {
                    // DECSCUSR - Select Cursor Style
                    // TODO: test
                    // ' ' => { ... }
                    b' ' => {
                        // const style: ansi.CursorStyle = switch (input.params.len) {
                        let style: ansi::CursorStyle = match input.params.len() {
                            0 => ansi::CursorStyle::Default,
                            // 1 => switch (input.params[0]) {
                            1 => match input.params[0] {
                                0 => ansi::CursorStyle::Default,
                                1 => ansi::CursorStyle::BlinkingBlock,
                                2 => ansi::CursorStyle::SteadyBlock,
                                3 => ansi::CursorStyle::BlinkingUnderline,
                                4 => ansi::CursorStyle::SteadyUnderline,
                                5 => ansi::CursorStyle::BlinkingBar,
                                6 => ansi::CursorStyle::SteadyBar,
                                _ => {
                                    // log.warn("invalid cursor style value: {}", .{input.params[0]});
                                    log::warn!(
                                        target: LOG_SCOPE,
                                        "invalid cursor style value: {}",
                                        input.params[0]
                                    );
                                    return;
                                }
                            },
                            _ => {
                                // log.warn("invalid set cursor style command: {f}", .{input});
                                log::warn!(
                                    target: LOG_SCOPE,
                                    "invalid set cursor style command: {input:?}"
                                );
                                return;
                            }
                        };
                        handler.cursor_style(style);
                    }

                    // DECSCA
                    // '"' => { ... }
                    b'"' => {
                        // const mode_: ?ansi.ProtectedMode = switch (input.params.len) {
                        let mode_: Option<ansi::ProtectedMode> = match input.params.len() {
                            0 => Some(ansi::ProtectedMode::Off),
                            // 1 => switch (input.params[0]) {
                            1 => match input.params[0] {
                                0 | 2 => Some(ansi::ProtectedMode::Off),
                                1 => Some(ansi::ProtectedMode::Dec),
                                _ => None,
                            },
                            _ => None,
                        };

                        // const mode = mode_ orelse { ... };
                        let mode = match mode_ {
                            Some(m) => m,
                            None => {
                                // log.warn("invalid set protected mode command: {f}", .{input});
                                log::warn!(
                                    target: LOG_SCOPE,
                                    "invalid set protected mode command: {input:?}"
                                );
                                return;
                            }
                        };

                        // switch (mode) {
                        match mode {
                            ansi::ProtectedMode::Off => handler.protected_mode_off(),
                            ansi::ProtectedMode::Iso => handler.protected_mode_iso(),
                            ansi::ProtectedMode::Dec => handler.protected_mode_dec(),
                        }
                    }

                    // XTVERSION
                    b'>' => handler.xtversion(),
                    _ => {
                        // log.warn("ignoring unimplemented CSI q with intermediates: {s}", ...);
                        log::warn!(
                            target: LOG_SCOPE,
                            "ignoring unimplemented CSI q with intermediates: {}",
                            String::from_utf8_lossy(&input.intermediates)
                        );
                    }
                },

                // else => log.warn("ignoring unimplemented CSI p with intermediates: {s}", ...),
                _ => {
                    log::warn!(
                        target: LOG_SCOPE,
                        "ignoring unimplemented CSI p with intermediates: {}",
                        String::from_utf8_lossy(&input.intermediates)
                    );
                }
            },

            // 'r' => { ... }
            b'r' => {
                // @branchHint(.likely);
                // switch (input.intermediates.len) {
                match input.intermediates.len() {
                    // DECSTBM - Set Top and Bottom Margins
                    // 0 => switch (input.params.len) {
                    0 => match input.params.len() {
                        0 => handler.top_and_bottom_margin(Margin {
                            top_left: 0,
                            bottom_right: 0,
                        }),
                        1 => handler.top_and_bottom_margin(Margin {
                            top_left: input.params[0],
                            bottom_right: 0,
                        }),
                        2 => handler.top_and_bottom_margin(Margin {
                            top_left: input.params[0],
                            bottom_right: input.params[1],
                        }),
                        _ => {
                            // @branchHint(.unlikely);
                            // log.warn("invalid DECSTBM command: {f}", .{input});
                            log::warn!(target: LOG_SCOPE, "invalid DECSTBM command: {input:?}");
                        }
                    },

                    // 1 => switch (input.intermediates[0]) {
                    1 => match input.intermediates[0] {
                        // Restore Mode
                        b'?' => {
                            // for (input.params) |mode_int| {
                            for &mode_int in input.params.iter() {
                                // if (modes.modeFromInt(mode_int, false)) |mode| {
                                if let Some(mode) = modes::mode_from_int(mode_int, false) {
                                    handler.restore_mode(Mode { mode });
                                } else {
                                    // log.warn("unimplemented restore mode: {}", .{mode_int});
                                    log::warn!(
                                        target: LOG_SCOPE,
                                        "unimplemented restore mode: {mode_int}"
                                    );
                                }
                            }
                        }

                        // else => log.warn("unknown CSI s with intermediate: {f}", .{input}),
                        _ => {
                            log::warn!(
                                target: LOG_SCOPE,
                                "unknown CSI s with intermediate: {input:?}"
                            );
                        }
                    },

                    // else => log.warn("ignoring unimplemented CSI s with intermediates: {f}", ...),
                    _ => {
                        log::warn!(
                            target: LOG_SCOPE,
                            "ignoring unimplemented CSI s with intermediates: {input:?}"
                        );
                    }
                }
            }

            // 's' => switch (input.intermediates.len) { ... }
            b's' => match input.intermediates.len() {
                // DECSLRM
                // 0 => switch (input.params.len) {
                0 => match input.params.len() {
                    // CSI S is ambiguous with zero params so we defer
                    // to our handler to do the proper logic. If mode 69
                    // is set, then we should invoke DECSLRM, otherwise
                    // we should invoke SC.
                    0 => handler.left_and_right_margin_ambiguous(),
                    1 => handler.left_and_right_margin(Margin {
                        top_left: input.params[0],
                        bottom_right: 0,
                    }),
                    2 => handler.left_and_right_margin(Margin {
                        top_left: input.params[0],
                        bottom_right: input.params[1],
                    }),
                    // else => log.warn("invalid DECSLRM command: {f}", .{input}),
                    _ => {
                        log::warn!(target: LOG_SCOPE, "invalid DECSLRM command: {input:?}");
                    }
                },

                // 1 => switch (input.intermediates[0]) {
                1 => match input.intermediates[0] {
                    b'?' => {
                        // for (input.params) |mode_int| {
                        for &mode_int in input.params.iter() {
                            // if (modes.modeFromInt(mode_int, false)) |mode| {
                            if let Some(mode) = modes::mode_from_int(mode_int, false) {
                                handler.save_mode(Mode { mode });
                            } else {
                                // log.warn("unimplemented save mode: {}", .{mode_int});
                                log::warn!(
                                    target: LOG_SCOPE,
                                    "unimplemented save mode: {mode_int}"
                                );
                            }
                        }
                    }

                    // XTSHIFTESCAPE
                    // '>' => capture: { ... }
                    b'>' => 'capture: {
                        // const capture = switch (input.params.len) {
                        let capture = match input.params.len() {
                            0 => false,
                            // 1 => switch (input.params[0]) {
                            1 => match input.params[0] {
                                0 => false,
                                1 => true,
                                _ => {
                                    // log.warn("invalid XTSHIFTESCAPE command: {f}", .{input});
                                    log::warn!(
                                        target: LOG_SCOPE,
                                        "invalid XTSHIFTESCAPE command: {input:?}"
                                    );
                                    break 'capture;
                                }
                            },
                            _ => {
                                // log.warn("invalid XTSHIFTESCAPE command: {f}", .{input});
                                log::warn!(
                                    target: LOG_SCOPE,
                                    "invalid XTSHIFTESCAPE command: {input:?}"
                                );
                                break 'capture;
                            }
                        };

                        handler.mouse_shift_capture(capture);
                    }

                    // else => log.warn("unknown CSI s with intermediate: {f}", .{input}),
                    _ => {
                        log::warn!(target: LOG_SCOPE, "unknown CSI s with intermediate: {input:?}");
                    }
                },

                // else => log.warn("ignoring unimplemented CSI s with intermediates: {f}", ...),
                _ => {
                    log::warn!(
                        target: LOG_SCOPE,
                        "ignoring unimplemented CSI s with intermediates: {input:?}"
                    );
                }
            },

            // XTWINOPS
            // 't' => switch (input.intermediates.len) { ... }
            b't' => match input.intermediates.len() {
                0 => {
                    // if (input.params.len > 0) {
                    if input.params.len() > 0 {
                        // switch (input.params[0]) {
                        match input.params[0] {
                            // 8 => if (input.params.len <= 3) { ... } else log.warn(...),
                            8 => {
                                if input.params.len() <= 3 {
                                    // resize the text area in characters
                                    handler.resize_window(ResizeWindow {
                                        rows: if input.params.len() > 1 {
                                            input.params[1]
                                        } else {
                                            0
                                        },
                                        columns: if input.params.len() > 2 {
                                            input.params[2]
                                        } else {
                                            0
                                        },
                                    });
                                } else {
                                    // else log.warn("ignoring CSI 8 t with extra parameters: {f}", ...),
                                    log::warn!(
                                        target: LOG_SCOPE,
                                        "ignoring CSI 8 t with extra parameters: {input:?}"
                                    );
                                }
                            }
                            // 14 => if (input.params.len == 1) { ... } else log.warn(...),
                            14 => {
                                if input.params.len() == 1 {
                                    // report the text area size in pixels
                                    handler.size_report(csi::SizeReportStyle::Csi14T);
                                } else {
                                    // else log.warn("ignoring CSI 14 t with extra parameters: {f}", ...),
                                    log::warn!(
                                        target: LOG_SCOPE,
                                        "ignoring CSI 14 t with extra parameters: {input:?}"
                                    );
                                }
                            }
                            // 16 => if (input.params.len == 1) { ... } else log.warn(...),
                            16 => {
                                if input.params.len() == 1 {
                                    // report cell size in pixels
                                    handler.size_report(csi::SizeReportStyle::Csi16T);
                                } else {
                                    // else log.warn("ignoring CSI 16 t with extra parameters: {f}", ...),
                                    log::warn!(
                                        target: LOG_SCOPE,
                                        "ignoring CSI 16 t with extra parameters: {input:?}"
                                    );
                                }
                            }
                            // 18 => if (input.params.len == 1) { ... } else log.warn(...),
                            18 => {
                                if input.params.len() == 1 {
                                    // report screen size in characters
                                    handler.size_report(csi::SizeReportStyle::Csi18T);
                                } else {
                                    // else log.warn("ignoring CSI 18 t with extra parameters: {f}", ...),
                                    log::warn!(
                                        target: LOG_SCOPE,
                                        "ignoring CSI 18 t with extra parameters: {input:?}"
                                    );
                                }
                            }
                            // 21 => if (input.params.len == 1) { ... } else log.warn(...),
                            21 => {
                                if input.params.len() == 1 {
                                    // report window title
                                    handler.size_report(csi::SizeReportStyle::Csi21T);
                                } else {
                                    // else log.warn("ignoring CSI 21 t with extra parameters: {f}", ...),
                                    log::warn!(
                                        target: LOG_SCOPE,
                                        "ignoring CSI 21 t with extra parameters: {input:?}"
                                    );
                                }
                            }
                            // inline 22, 23 => |number| if (...) { ... } else log.warn(...),
                            number @ (22 | 23) => {
                                if (input.params.len() == 2 || input.params.len() == 3) &&
                                    // we only support window title
                                    (input.params[1] == 0 || input.params[1] == 2)
                                {
                                    // push/pop title
                                    // const index: u16 = if (input.params.len == 3) ... else 0;
                                    let index: u16 = if input.params.len() == 3 {
                                        input.params[2]
                                    } else {
                                        0
                                    };
                                    // switch (number) {
                                    match number {
                                        22 => handler.title_push(index),
                                        23 => handler.title_pop(index),
                                        // else => @compileError("unreachable"),
                                        _ => unreachable!(),
                                    }
                                }
                                // else logUnsupportedOnce(
                                //     "ignoring CSI 22/23 t with extra parameters: {f}",
                                //     .{input},
                                //     input.params[0],
                                // ),
                                else if log_unsupported_once(
                                    &SEEN_CSI_22_23_T_EXTRA_PARAMS,
                                    input.params[0],
                                ) {
                                    log::warn!(
                                        target: LOG_SCOPE,
                                        "ignoring CSI 22/23 t with extra parameters: {input:?}"
                                    );
                                }
                            }
                            // else => logUnsupportedOnce(
                            //     "ignoring CSI t with unimplemented parameter: {f}",
                            //     .{input},
                            //     input.params[0],
                            // ),
                            _ => {
                                if log_unsupported_once(
                                    &SEEN_CSI_T_UNIMPLEMENTED_PARAM,
                                    input.params[0],
                                ) {
                                    log::warn!(
                                        target: LOG_SCOPE,
                                        "ignoring CSI t with unimplemented parameter: {input:?}"
                                    );
                                }
                            }
                        }
                    } else {
                        // else log.err("ignoring CSI t with no parameters: {f}", .{input});
                        log::error!(
                            target: LOG_SCOPE,
                            "ignoring CSI t with no parameters: {input:?}"
                        );
                    }
                }
                // else => log.warn("ignoring unimplemented CSI t with intermediates: {f}", ...),
                _ => {
                    log::warn!(
                        target: LOG_SCOPE,
                        "ignoring unimplemented CSI t with intermediates: {input:?}"
                    );
                }
            },

            // 'u' => switch (input.intermediates.len) { ... }
            b'u' => match input.intermediates.len() {
                0 => handler.restore_cursor(),

                // Kitty keyboard protocol
                // 1 => switch (input.intermediates[0]) {
                1 => match input.intermediates[0] {
                    b'?' => handler.kitty_keyboard_query(),

                    // '>' => push: { ... }
                    b'>' => 'push: {
                        // const flags: u5 = if (input.params.len == 1) std.math.cast(u5, ...) else 0;
                        let flags: u8 = if input.params.len() == 1 {
                            match u8::try_from(input.params[0]).ok().filter(|v| *v < 32) {
                                Some(v) => v,
                                None => {
                                    // log.warn("invalid pushKittyKeyboard command: {f}", .{input});
                                    log::warn!(
                                        target: LOG_SCOPE,
                                        "invalid pushKittyKeyboard command: {input:?}"
                                    );
                                    break 'push;
                                }
                            }
                        } else {
                            0
                        };

                        handler.kitty_keyboard_push(KittyKeyboardFlags {
                            flags: kitty::KeyFlags::from_int(flags),
                        });
                    }

                    // '<' => { ... }
                    b'<' => {
                        // const number: u16 = if (input.params.len == 1) input.params[0] else 1;
                        let number: u16 = if input.params.len() == 1 {
                            input.params[0]
                        } else {
                            1
                        };

                        handler.kitty_keyboard_pop(number);
                    }

                    // '=' => set: { ... }
                    b'=' => 'set: {
                        // const flags: u5 = if (input.params.len >= 1) std.math.cast(u5, ...) else 0;
                        let flags: u8 = if input.params.len() >= 1 {
                            match u8::try_from(input.params[0]).ok().filter(|v| *v < 32) {
                                Some(v) => v,
                                None => {
                                    // log.warn("invalid setKittyKeyboard command: {f}", .{input});
                                    log::warn!(
                                        target: LOG_SCOPE,
                                        "invalid setKittyKeyboard command: {input:?}"
                                    );
                                    break 'set;
                                }
                            }
                        } else {
                            0
                        };

                        // const number: u16 = if (input.params.len >= 2) input.params[1] else 1;
                        let number: u16 = if input.params.len() >= 2 {
                            input.params[1]
                        } else {
                            1
                        };

                        // const action_tag: streampkg.Action.Tag = switch (number) {
                        // PORT NOTE: the reference selects an `Action.Tag` then re-switches
                        // on it to dispatch; the Rust port folds the two switches into one
                        // since there is no separate tag value to carry. The `kitty_flags`
                        // payload and the per-`number` dispatch are preserved verbatim.
                        // const kitty_flags: streampkg.Action.KittyKeyboardFlags =
                        //     .{ .flags = @as(kitty.KeyFlags, @bitCast(flags)) };
                        let kitty_flags: KittyKeyboardFlags = KittyKeyboardFlags {
                            flags: kitty::KeyFlags::from_int(flags),
                        };
                        match number {
                            // 1 => .kitty_keyboard_set,
                            1 => handler.kitty_keyboard_set(kitty_flags),
                            // 2 => .kitty_keyboard_set_or,
                            2 => handler.kitty_keyboard_set_or(kitty_flags),
                            // 3 => .kitty_keyboard_set_not,
                            3 => handler.kitty_keyboard_set_not(kitty_flags),
                            _ => {
                                // log.warn("invalid setKittyKeyboard command: {f}", .{input});
                                log::warn!(
                                    target: LOG_SCOPE,
                                    "invalid setKittyKeyboard command: {input:?}"
                                );
                                break 'set;
                            }
                        }
                    }

                    // else => log.warn("unknown CSI s with intermediate: {f}", .{input}),
                    _ => {
                        log::warn!(target: LOG_SCOPE, "unknown CSI s with intermediate: {input:?}");
                    }
                },

                // else => log.warn("ignoring unimplemented CSI u: {f}", .{input}),
                _ => {
                    log::warn!(target: LOG_SCOPE, "ignoring unimplemented CSI u: {input:?}");
                }
            },

            // ICH - Insert Blanks
            // '@' => switch (input.intermediates.len) { ... }
            b'@' => match input.intermediates.len() {
                0 => handler.insert_blanks(match input.params.len() {
                    0 => 1,
                    1 => std::cmp::max(1, input.params[0]) as usize,
                    _ => {
                        // @branchHint(.unlikely);
                        // log.warn("invalid ICH command: {f}", .{input});
                        log::warn!(target: LOG_SCOPE, "invalid ICH command: {input:?}");
                        return;
                    }
                }),

                // else => log.warn("ignoring unimplemented CSI @: {f}", .{input}),
                _ => {
                    log::warn!(target: LOG_SCOPE, "ignoring unimplemented CSI @: {input:?}");
                }
            },

            // 'y' => switch (input.intermediates.len) { ... }
            b'y' => match input.intermediates.len() {
                1 => match input.intermediates[0] {
                    // DECRQCRA - Request Checksum of Rectangular Area.
                    // The page number (the second parameter) is ignored.
                    b'*' => {
                        if input.params.len() > 6 {
                            // log.warn("invalid DECRQCRA command: {f}", .{input});
                            log::warn!(target: LOG_SCOPE, "invalid DECRQCRA command: {input:?}");
                            return;
                        }

                        // var params: [6]u16 = @splat(0);
                        // @memcpy(params[0..input.params.len], input.params);
                        let mut params: [u16; 6] = [0; 6];
                        params[..input.params.len()].copy_from_slice(input.params.as_slice());
                        handler.request_xt_checksum(crate::xt_checksum::Request {
                            id: params[0],
                            top: params[2],
                            left: params[3],
                            bottom: params[4],
                            right: params[5],
                        });
                    }

                    // XTCHECKSUM - Select checksum extension. Bits
                    // beyond the ones we know are ignored, as in xterm.
                    b'#' => {
                        if input.params.len() > 1 {
                            // log.warn("invalid XTCHECKSUM command: {f}", .{input});
                            log::warn!(target: LOG_SCOPE, "invalid XTCHECKSUM command: {input:?}");
                            return;
                        }

                        let bits: u16 = if input.params.len() == 1 {
                            input.params[0]
                        } else {
                            0
                        };
                        handler.xt_checksum_extension(XtChecksumExtension {
                            // .flags = @as(xt_checksum.Flags, @bitCast(@as(u5, @truncate(bits)))),
                            flags: crate::xt_checksum::Flags::from_bits((bits & 0x1F) as u8),
                        });
                    }

                    // else => log.warn("ignoring unimplemented CSI y: {f}", .{input}),
                    _ => {
                        log::warn!(target: LOG_SCOPE, "ignoring unimplemented CSI y: {input:?}");
                    }
                },

                // else => log.warn("ignoring unimplemented CSI y: {f}", .{input}),
                _ => {
                    log::warn!(target: LOG_SCOPE, "ignoring unimplemented CSI y: {input:?}");
                }
            },

            // DECSASD - Select Active Status Display
            // '}' => decsasd: { ... }
            b'}' => 'decsasd: {
                // Verify we're getting a DECSASD command
                // if (input.intermediates.len != 1 or input.intermediates[0] != '$') { ... }
                if input.intermediates.len() != 1 || input.intermediates[0] != b'$' {
                    // log.warn("unimplemented CSI callback: {f}", .{input});
                    log::warn!(target: LOG_SCOPE, "unimplemented CSI callback: {input:?}");
                    break 'decsasd;
                }
                // if (input.params.len != 1) { ... }
                if input.params.len() != 1 {
                    // log.warn("unimplemented CSI callback: {f}", .{input});
                    log::warn!(target: LOG_SCOPE, "unimplemented CSI callback: {input:?}");
                    break 'decsasd;
                }

                // const display: ansi.StatusDisplay = switch (input.params[0]) {
                let display: ansi::StatusDisplay = match input.params[0] {
                    0 => ansi::StatusDisplay::Main,
                    1 => ansi::StatusDisplay::StatusLine,
                    _ => {
                        // log.warn("unimplemented CSI callback: {f}", .{input});
                        log::warn!(target: LOG_SCOPE, "unimplemented CSI callback: {input:?}");
                        break 'decsasd;
                    }
                };

                handler.active_status_display(display);
            }

            // else => log.warn("unimplemented CSI action: {f}", .{input}),
            _ => {
                log::warn!(target: LOG_SCOPE, "unimplemented CSI action: {input:?}");
            }
        }
    }
}
