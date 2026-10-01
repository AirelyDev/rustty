//! PORTS: ghostty src/termio/stream_handler.zig — the `StreamHandler` OSC methods (`windowTitle` through
//! `progressReport`). Named `osc_handlers` because `osc` is already the
//! imported `rustty_vt::osc` module.

use super::*;

impl<M: AppMailbox> StreamHandler<M> {
    //-------------------------------------------------------------------------
    // OSC

    // fn windowTitle(self: *StreamHandler, title: []const u8) !void {
    pub(super) fn windowTitle(&mut self, title: &[u8]) -> Result<(), crate::Error> {
        // var buf: [256]u8 = undefined;
        let mut buf = [0u8; 256];
        // if (title.len >= buf.len) {
        //     log.warn("change title requested larger than our buffer size, ignoring", .{});
        //     return;
        // }
        if title.len() >= buf.len() {
            // log.warn("change title requested larger than our buffer size, ignoring", .{});
            log::warn!(
                target: LOG_SCOPE,
                "change title requested larger than our buffer size, ignoring"
            );
            return Ok(());
        }

        // Set the title on the terminal state. We ignore any errors since
        // we can continue to operate just fine without it.
        // self.terminal.setTitle(title) catch |err| {
        //     log.warn("error setting title in terminal state: {}", .{err});
        // };
        if let Err(err) = self.term().set_title(title) {
            // log.warn("error setting title in terminal state: {}", .{err});
            log::warn!(target: LOG_SCOPE, "error setting title in terminal state: {err:?}");
        }

        // @memcpy(buf[0..title.len], title);
        buf[..title.len()].copy_from_slice(title);
        // buf[title.len] = 0;
        buf[title.len()] = 0;

        // Special handling for the empty title. We treat the empty title
        // as resetting to as if we never saw a title. Other terminals
        // behave this way too (e.g. iTerm2).
        // if (title.len == 0) {
        if title.is_empty() {
            // If we have a pwd then we set the title as the pwd else
            // we just set it to blank.
            // if (self.terminal.getPwd()) |pwd| pwd: {
            //     if (pwd.len >= buf.len) break :pwd;
            //     @memcpy(buf[0..pwd.len], pwd);
            //     buf[pwd.len] = 0;
            // }
            'pwd: {
                let term = self.term();
                if let Some(pwd) = term.get_pwd() {
                    if pwd.len() >= buf.len() {
                        break 'pwd;
                    }
                    let pwd_len = pwd.len();
                    buf[..pwd_len].copy_from_slice(pwd);
                    buf[pwd_len] = 0;
                }
            }

            // self.surfaceMessageWriter(.{ .set_title = buf });
            self.surfaceMessageWriter(SurfaceMessage::SetTitle(buf));
            // self.seen_title = false;
            self.seen_title = false;
            return Ok(());
        }

        // self.seen_title = true;
        self.seen_title = true;
        // self.surfaceMessageWriter(.{ .set_title = buf });
        self.surfaceMessageWriter(SurfaceMessage::SetTitle(buf));
        Ok(())
    }

    // inline fn setMouseShape(
    //     self: *StreamHandler,
    //     shape: terminal.MouseShape,
    // ) !void {
    #[inline]
    pub(super) fn setMouseShape(
        &mut self,
        shape: rustty_vt::mouse::Shape,
    ) -> Result<(), crate::Error> {
        // Avoid changing the shape if it is already set to avoid excess
        // cross-thread messaging.
        // if (self.terminal.mouse_shape == shape) return;
        if self.term().mouse_shape == shape {
            return Ok(());
        }

        // self.terminal.mouse_shape = shape;
        self.term().mouse_shape = shape;
        // self.surfaceMessageWriter(.{ .set_mouse_shape = shape });
        self.surfaceMessageWriter(SurfaceMessage::SetMouseShape(shape));
        Ok(())
    }

    // fn clipboardContents(self: *StreamHandler, kind: u8, data: []const u8) !void {
    pub(super) fn clipboardContents(&mut self, kind: u8, data: &[u8]) -> Result<(), crate::Error> {
        // Note: we ignore the "kind" field and always use the standard clipboard.
        // iTerm also appears to do this but other terminals seem to only allow
        // certain. Let's investigate more.

        // const clipboard_type: apprt.Clipboard = switch (kind) {
        //     'c' => .standard,
        //     's' => .selection,
        //     'p' => .primary,
        //     else => .standard,
        // };
        let clipboard_type: Clipboard = match kind {
            b'c' => Clipboard::Standard,
            b's' => Clipboard::Selection,
            b'p' => Clipboard::Primary,
            _ => Clipboard::Standard,
        };

        // Get clipboard contents
        // if (data.len == 1 and data[0] == '?') {
        if data.len() == 1 && data[0] == b'?' {
            // self.surfaceMessageWriter(.{ .clipboard_read = clipboard_type });
            self.surfaceMessageWriter(SurfaceMessage::ClipboardRead(clipboard_type));
            return Ok(());
        }

        // Write clipboard contents
        // self.surfaceMessageWriter(.{
        //     .clipboard_write = .{
        //         .req = try apprt.surface.Message.WriteReq.init(
        //             self.alloc,
        //             data,
        //         ),
        //         .clipboard_type = clipboard_type,
        //     },
        // });
        self.surfaceMessageWriter(SurfaceMessage::ClipboardWrite {
            req: rustty_apprt_api::surface::WriteReq::init(data),
            clipboard_type,
        });
        Ok(())
    }

    // fn semanticPrompt(
    //     self: *StreamHandler,
    //     cmd: Stream.Action.SemanticPrompt,
    // ) !void {
    pub(super) fn semanticPrompt(
        &mut self,
        cmd: stream::SemanticPrompt,
    ) -> Result<(), crate::Error> {
        use rustty_vt::osc::semantic_prompt::option;
        use rustty_vt::osc::semantic_prompt::Action;

        // switch (cmd.action) {
        match cmd.action {
            // .end_input_start_output => {
            //     self.surfaceMessageWriter(.start_command);
            // },
            Action::EndInputStartOutput => {
                self.surfaceMessageWriter(SurfaceMessage::StartCommand);
            }

            // .end_command => {
            Action::EndCommand => {
                // The specification seems to not specify the type but
                // other terminals accept 32-bits, but exit codes are really
                // bytes, so we just do our best here.
                // const code: u8 = code: {
                //     const raw: i32 = cmd.readOption(.exit_code) orelse 0;
                //     break :code std.math.cast(u8, raw) orelse 1;
                // };
                let code: u8 = {
                    let raw: i32 = cmd.read_option(option::EXIT_CODE).unwrap_or(0);
                    u8::try_from(raw).unwrap_or(1)
                };

                // self.surfaceMessageWriter(.{ .stop_command = code });
                self.surfaceMessageWriter(SurfaceMessage::StopCommand(Some(code)));
            }

            // Handled by Terminal, no special handling by us
            // .end_prompt_start_input,
            // .end_prompt_start_input_terminate_eol,
            // .fresh_line,
            // .fresh_line_new_prompt,
            // .new_command,
            // .prompt_start,
            // => {},
            Action::EndPromptStartInput
            | Action::EndPromptStartInputTerminateEol
            | Action::FreshLine
            | Action::FreshLineNewPrompt
            | Action::NewCommand
            | Action::PromptStart => {}
        }

        // We do this last so failures are still processed correctly
        // above.
        // try self.terminal.semanticPrompt(cmd);
        self.term().semantic_prompt(cmd).map_err(|_| crate::Error)
    }

    // fn reportPwd(self: *StreamHandler, url: []const u8) !void {
    pub(super) fn reportPwd(&mut self, url: &[u8]) -> Result<(), crate::Error> {
        // Special handling for the empty URL. We treat the empty URL
        // as resetting the pwd as if we never saw a pwd. I can't find any
        // other terminal that does this but it seems like a reasonable
        // behavior that enables some useful features. For example, the macOS
        // proxy icon can be hidden when a program reports it doesn't know
        // the pwd rather than showing a stale pwd.
        // if (url.len == 0) {
        if url.is_empty() {
            // Blank value can never fail because no allocs happen.
            // self.terminal.setPwd("") catch unreachable;
            self.term().set_pwd(b"").unwrap();

            // If we haven't seen a title, we're using the pwd as our title.
            // Set it to blank which will reset our title behavior.
            // if (!self.seen_title) {
            if !self.seen_title {
                // try self.windowTitle("");
                self.windowTitle(b"")?;
                // assert(!self.seen_title);
                debug_assert!(!self.seen_title);
            }

            // Report the change.
            // self.surfaceMessageWriter(.{ .pwd_change = .{ .stable = "" } });
            self.surfaceMessageWriter(SurfaceMessage::PwdChange(
                rustty_datastruct::MessageData::Stable(b""),
            ));
            return Ok(());
        }

        // if (builtin.os.tag == .windows) {
        //     log.warn("reportPwd unimplemented on windows", .{});
        //     return;
        // }
        if cfg!(windows) {
            // log.warn("reportPwd unimplemented on windows", .{});
            log::warn!(target: LOG_SCOPE, "reportPwd unimplemented on windows");
            return Ok(());
        }

        // Attempt to parse this file-style URI using options appropriate
        // for this OSC 7 context (e.g. kitty-shell-cwd expects the full,
        // unencoded path).
        // const uri: std.Uri = internal_os.uri.parse(url, .{
        //     .mac_address = comptime builtin.os.tag != .macos,
        //     .raw_path = std.mem.startsWith(u8, url, "kitty-shell-cwd://"),
        // }) catch |e| {
        //     log.warn("invalid url in OSC 7: {}", .{e});
        //     return;
        // };
        let uri = match rustty_os::uri::parse(
            url,
            rustty_os::uri::ParseOptions {
                mac_address: cfg!(not(target_os = "macos")),
                raw_path: url.starts_with(b"kitty-shell-cwd://"),
            },
        ) {
            Ok(uri) => uri,
            Err(e) => {
                // log.warn("invalid url in OSC 7: {}", .{e});
                log::warn!(target: LOG_SCOPE, "invalid url in OSC 7: {e:?}");
                return Ok(());
            }
        };

        // if (!std.mem.eql(u8, "file", uri.scheme) and
        //     !std.mem.eql(u8, "kitty-shell-cwd", uri.scheme))
        // {
        //     log.warn("OSC 7 scheme must be file or kitty-shell-cwd, got: {s}", .{uri.scheme});
        //     return;
        // }
        if uri.scheme != b"file" && uri.scheme != b"kitty-shell-cwd" {
            // log.warn("OSC 7 scheme must be file or kitty-shell-cwd, got: {s}", .{uri.scheme});
            log::warn!(
                target: LOG_SCOPE,
                "OSC 7 scheme must be file or kitty-shell-cwd, got: {}",
                String::from_utf8_lossy(uri.scheme)
            );
            return Ok(());
        }

        // var host_buffer: [std.Io.net.HostName.max_len]u8 = undefined;
        // const host = uri.getHost(&host_buffer) catch |err| switch (err) {
        //     error.UriMissingHost => {
        //         log.warn("OSC 7 uri must contain a hostname: {}", .{err});
        //         return;
        //     },
        // };
        let mut host_buffer = [0u8; URI_HOST_NAME_MAX];
        let host = match uri_get_host(&uri, &mut host_buffer) {
            Ok(host) => host,
            Err(err @ GetHostError::UriMissingHost) => {
                // log.warn("OSC 7 uri must contain a hostname: {}", .{err});
                log::warn!(target: LOG_SCOPE, "OSC 7 uri must contain a hostname: {err:?}");
                return Ok(());
            }
            // PORT NOTE: reference defect (R13), UD351 (parity tail U-1,
            // 2026-09-28). The reference has no arm for this: at pin
            // 20abdb50a `getHost`'s error set is `error{UriMissingHost}` only
            // (the port's old comment quoted a pre-resync `UriHostTooLong` arm),
            // and Zig 0.16.0 `std.Uri.getHost`
            // does `toRaw(buffer) catch |err| switch (err) { error.NoSpaceLeft
            // => unreachable }` on the belief that the host was "already
            // validated". `std.Uri.parse` never validates it (only
            // `resolveInPlace` calls `validateHostComponent`), so an OSC 7 host
            // that contains a `%` and percent-decodes to more than 255 bytes
            // reaches that `unreachable`: a panic in Debug/ReleaseSafe and
            // undefined behaviour in ReleaseFast (measured: a 100%-CPU hang).
            // Any program writing to the terminal can emit it. A host without a
            // `%` is returned undecoded and never overflows.
            //
            // Safe handling: the OSC 7 is ignored, as the reference would have
            // ignored it had `getHost` not trapped (a host over 255 bytes can
            // never pass `isLocal`). No log call is added: the reference has no
            // message for this case (R13: no invented message text).
            Err(GetHostError::UriHostTooLong) => {
                return Ok(());
            }
        };

        // OSC 7 is a little sketchy because anyone can send any value from
        // any host (such an SSH session). The best practice terminals follow
        // is to valid the hostname to be local.
        // const host_valid = internal_os.hostname.isLocal(host) catch |err| switch (err) {
        //     error.PermissionDenied,
        //     error.Unexpected,
        //     => {
        //         log.warn("failed to get hostname for OSC 7 validation: {}", .{err});
        //         return;
        //     },
        // };
        let host_valid = match rustty_os::hostname::isLocal(host) {
            Ok(v) => v,
            Err(
                err @ (rustty_os::hostname::LocalHostnameValidationError::PermissionDenied
                | rustty_os::hostname::LocalHostnameValidationError::Unexpected),
            ) => {
                // log.warn("failed to get hostname for OSC 7 validation: {}", .{err});
                log::warn!(
                    target: LOG_SCOPE,
                    "failed to get hostname for OSC 7 validation: {err:?}"
                );
                return Ok(());
            }
        };
        // if (!host_valid) {
        //     log.warn("OSC 7 host ({s}) must be local", .{host});
        //     return;
        // }
        if !host_valid {
            // log.warn("OSC 7 host ({s}) must be local", .{host});
            log::warn!(
                target: LOG_SCOPE,
                "OSC 7 host ({}) must be local",
                String::from_utf8_lossy(host)
            );
            return Ok(());
        }

        // We need the raw path, which might require unescaping. We try to
        // avoid making any heap allocations by using the stack first.
        // var arena_alloc: std.heap.ArenaAllocator = .init(self.alloc);
        // var stack_alloc = std.heap.stackFallback(1024, arena_alloc.allocator());
        // defer arena_alloc.deinit();
        // const path = try uri.path.toRawMaybeAlloc(stack_alloc.get());
        //
        // (PORT NOTE: the stackFallback/arena collapse into the owned Vec.)
        let path: Vec<u8> = {
            let mut w = VecWriter(Vec::new());
            uri.path.formatRaw(&mut w).map_err(|_| crate::Error)?;
            w.0
        };

        // log.debug("terminal pwd: {s}", .{path});
        log::debug!(
            target: LOG_SCOPE,
            "terminal pwd: {}",
            String::from_utf8_lossy(&path)
        );
        // try self.terminal.setPwd(path);
        self.term().set_pwd(&path).map_err(|_| crate::Error)?;

        // Report it to the surface. If creating our write request fails
        // then we just ignore it.
        // if (apprt.surface.Message.WriteReq.init(self.alloc, path)) |req| {
        //     self.surfaceMessageWriter(.{ .pwd_change = req });
        // } else |err| {
        //     log.warn("error notifying surface of pwd change err={}", .{err});
        // }
        //
        // (PORT NOTE: `WriteReq.init` is infallible in Rust's allocation model —
        // the error branch collapses, preserved verbatim above.)
        //
        // PORT NOTE (G-LOGFILE / task-14e-1, 2026-09-10): this is
        // `stream_handler.zig:1547`, the ONE site of this file's 41 that stays
        // UNRESTORED. There is no failure arm to hang it on and emitting it
        // unconditionally would put a diagnostic on a SUCCESS path — the bug this
        // project already shipped once. `LOG-PORTMAP.tsv`'s
        // `src/termio/stream_handler.zig` allowance is held at 1 for exactly this
        // site, with the reason in its `reason` column; see `ADJUDICATION.md`.
        let req = rustty_apprt_api::surface::WriteReq::init(&path);
        self.surfaceMessageWriter(SurfaceMessage::PwdChange(req));

        // If we haven't seen a title, use our pwd as the title.
        // if (!self.seen_title) {
        if !self.seen_title {
            // try self.windowTitle(path);
            self.windowTitle(&path)?;
            // self.seen_title = false;
            self.seen_title = false;
        }
        Ok(())
    }

    // fn colorOperation(
    //     self: *StreamHandler,
    //     op: terminal.osc.color.Operation,
    //     requests: *const terminal.osc.color.List,
    //     terminator: terminal.osc.Terminator,
    // ) !void {
    pub(super) fn colorOperation(
        &mut self,
        op: osc::color::Operation,
        requests: &osc::color::List,
        terminator: osc::Terminator,
    ) -> Result<(), crate::Error> {
        // We'll need op one day if we ever implement reporting special colors.
        // _ = op;
        let _ = op;

        // return early if there is nothing to do
        // if (requests.count() == 0) return;
        if requests.is_empty() {
            return Ok(());
        }

        // var buffer: [1024]u8 = undefined;
        // var fba: std.heap.FixedBufferAllocator = .init(&buffer);
        // const alloc = fba.allocator();
        //
        // var response: std.Io.Writer.Allocating = .init(alloc);
        //
        // CORRECTION (S214-F1): this block quoted `var response:
        // std.ArrayListUnmanaged(u8) = .empty; const writer =
        // response.writer(alloc);`, an older reference; the pin uses
        // `std.Io.Writer.Allocating` (`stream_handler.zig:1554-1557`).
        //
        // PORT NOTE: reference defect (R13), UPSTREAM-DEFECTS.md UD205. The
        // reports accumulate in an allocating writer over a 1024-byte fixed
        // buffer, and once it cannot grow, `try response.writer.print(...)`
        // fails: `colorOperation` returns the error, NO report is sent, and
        // every request after the one that failed, sets and resets included,
        // is skipped. The writer's growth (`growCapacity`, `2*n + 1` bytes asked
        // for a write that does not fit) puts the point well below 1024 bytes:
        // measured with zig 0.16.0 (aarch64-macos and x86_64-macos), 25 16-bit
        // palette reports for indices 0, 1, 2, ... fit (640 bytes with BEL,
        // 665 with ST) and the 26th fails; the number depends on how many
        // digits the indices have (24th to 27th measured). An OSC 4
        // or OSC 104 is bounded only by the OSC parser, so a program querying
        // its palette in one sequence gets no answer. The port accumulates in a
        // `Vec`, so every report is sent and every request is applied, which is
        // what the function is for. S214-F1.
        let mut response: Vec<u8> = Vec::new();

        // var it = requests.constIterator(0);
        // while (it.next()) |req| {
        for req in requests.iter() {
            match req {
                // .set => |set| {
                osc::color::Request::Set(set) => {
                    match &set.target {
                        // .palette => |i| {
                        //     self.terminal.flags.dirty.palette = true;
                        //     self.terminal.colors.palette.set(i, set.color);
                        // },
                        osc::color::Target::Palette(i) => {
                            self.term().flags.dirty.palette = true;
                            self.term().colors.palette.set(*i, set.color);
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
                            // => log.info("setting dynamic color {s} not implemented", .{
                            //     @tagName(dynamic),
                            // }),
                            color::Dynamic::PointerForeground
                            | color::Dynamic::PointerBackground
                            | color::Dynamic::TektronixForeground
                            | color::Dynamic::TektronixBackground
                            | color::Dynamic::HighlightBackground
                            | color::Dynamic::TektronixCursor
                            | color::Dynamic::HighlightForeground => {
                                // log.info("setting dynamic color {s} not implemented", ...);
                                log::info!(
                                    target: LOG_SCOPE,
                                    "setting dynamic color {} not implemented",
                                    dynamic.tag_name()
                                );
                            }
                        },
                        // .special => log.info("setting special colors not implemented", .{}),
                        osc::color::Target::Special(_) => {
                            // log.info("setting special colors not implemented", .{});
                            log::info!(
                                target: LOG_SCOPE,
                                "setting special colors not implemented"
                            );
                        }
                    }

                    // Notify the surface of the color change
                    // self.surfaceMessageWriter(.{ .color_change = .{
                    //     .target = set.target,
                    //     .color = set.color,
                    // } });
                    self.surfaceMessageWriter(SurfaceMessage::ColorChange(
                        osc::color::ColoredTarget {
                            target: set.target,
                            color: set.color,
                        },
                    ));
                }

                // .reset => |target| switch (target) {
                osc::color::Request::Reset(target) => match target {
                    // .palette => |i| {
                    osc::color::Target::Palette(i) => {
                        self.term().flags.dirty.palette = true;
                        self.term().colors.palette.reset(*i);

                        // self.surfaceMessageWriter(.{
                        //     .color_change = .{
                        //         .target = target,
                        //         .color = self.terminal.colors.palette.current[i],
                        //     },
                        // });
                        let current = self.term().colors.palette.current[*i as usize];
                        self.surfaceMessageWriter(SurfaceMessage::ColorChange(
                            osc::color::ColoredTarget {
                                target: *target,
                                color: current,
                            },
                        ));
                    }
                    // .dynamic => |dynamic| switch (dynamic) {
                    osc::color::Target::Dynamic(dynamic) => match dynamic {
                        // .foreground => {
                        color::Dynamic::Foreground => {
                            self.term().colors.foreground.reset();

                            // if (self.terminal.colors.foreground.default) |c| {
                            if let Some(c) = self.term().colors.foreground.default {
                                self.surfaceMessageWriter(SurfaceMessage::ColorChange(
                                    osc::color::ColoredTarget {
                                        target: *target,
                                        color: c,
                                    },
                                ));
                            }
                        }
                        // .background => {
                        color::Dynamic::Background => {
                            self.term().colors.background.reset();

                            // if (self.terminal.colors.background.default) |c| {
                            if let Some(c) = self.term().colors.background.default {
                                self.surfaceMessageWriter(SurfaceMessage::ColorChange(
                                    osc::color::ColoredTarget {
                                        target: *target,
                                        color: c,
                                    },
                                ));
                            }
                        }
                        // .cursor => {
                        color::Dynamic::Cursor => {
                            self.term().colors.cursor.reset();

                            // if (self.terminal.colors.cursor.default) |c| {
                            if let Some(c) = self.term().colors.cursor.default {
                                self.surfaceMessageWriter(SurfaceMessage::ColorChange(
                                    osc::color::ColoredTarget {
                                        target: *target,
                                        color: c,
                                    },
                                ));
                            }
                        }
                        // .pointer_foreground,
                        // .pointer_background,
                        // .tektronix_foreground,
                        // .tektronix_background,
                        // .highlight_background,
                        // .tektronix_cursor,
                        // .highlight_foreground,
                        // => log.warn("resetting dynamic color {s} not implemented", .{
                        //     @tagName(dynamic),
                        // }),
                        color::Dynamic::PointerForeground
                        | color::Dynamic::PointerBackground
                        | color::Dynamic::TektronixForeground
                        | color::Dynamic::TektronixBackground
                        | color::Dynamic::HighlightBackground
                        | color::Dynamic::TektronixCursor
                        | color::Dynamic::HighlightForeground => {
                            // log.warn("resetting dynamic color {s} not implemented", ...);
                            log::warn!(
                                target: LOG_SCOPE,
                                "resetting dynamic color {} not implemented",
                                dynamic.tag_name()
                            );
                        }
                    },
                    // .special => log.info("resetting special colors not implemented", .{}),
                    osc::color::Target::Special(_) => {
                        // log.info("resetting special colors not implemented", .{});
                        log::info!(target: LOG_SCOPE, "resetting special colors not implemented");
                    }
                },

                // .reset_palette => {
                osc::color::Request::ResetPalette => {
                    // const mask = &self.terminal.colors.palette.mask;
                    // var mask_it = mask.iterator(.{});
                    // while (mask_it.next()) |i| {
                    //     self.terminal.flags.dirty.palette = true;
                    //     self.terminal.colors.palette.reset(@intCast(i));
                    //     self.surfaceMessageWriter(.{
                    //         .color_change = .{
                    //             .target = .{ .palette = @intCast(i) },
                    //             .color = self.terminal.colors.palette.current[i],
                    //         },
                    //     });
                    // }
                    // mask.* = .initEmpty();
                    let indices: Vec<usize> = self.term().colors.palette.mask.iter_set().collect();
                    for i in indices {
                        self.term().flags.dirty.palette = true;
                        self.term().colors.palette.reset(i as u8);
                        let current = self.term().colors.palette.current[i];
                        self.surfaceMessageWriter(SurfaceMessage::ColorChange(
                            osc::color::ColoredTarget {
                                target: osc::color::Target::Palette(i as u8),
                                color: current,
                            },
                        ));
                    }
                    self.term().colors.palette.mask = color::PaletteMask::init_empty();
                }

                // .reset_special => log.warn(
                //     "resetting all special colors not implemented",
                //     .{},
                // ),
                osc::color::Request::ResetSpecial => {
                    // log.warn("resetting all special colors not implemented", .{});
                    log::warn!(
                        target: LOG_SCOPE,
                        "resetting all special colors not implemented"
                    );
                }

                // .query => |kind| report: {
                osc::color::Request::Query(kind) => 'report: {
                    // if (self.osc_color_report_format == .none) break :report;
                    if self.osc_color_report_format == OSCColorReportFormat::None {
                        break 'report;
                    }

                    // const color = switch (kind) {
                    let color_val: color::RGB = match kind {
                        // .palette => |i| self.terminal.colors.palette.current[i],
                        osc::color::Target::Palette(i) => {
                            self.term().colors.palette.current[*i as usize]
                        }
                        // .dynamic => |dynamic| switch (dynamic) {
                        osc::color::Target::Dynamic(dynamic) => match dynamic {
                            // .foreground => self.terminal.colors.foreground.get().?,
                            color::Dynamic::Foreground => {
                                self.term().colors.foreground.get().unwrap()
                            }
                            // .background => self.terminal.colors.background.get().?,
                            color::Dynamic::Background => {
                                self.term().colors.background.get().unwrap()
                            }
                            // .cursor => self.terminal.colors.cursor.get() orelse
                            //     self.terminal.colors.foreground.get().?,
                            color::Dynamic::Cursor => {
                                let cursor = self.term().colors.cursor.get();
                                match cursor {
                                    Some(c) => c,
                                    None => self.term().colors.foreground.get().unwrap(),
                                }
                            }
                            // .pointer_foreground,
                            // .pointer_background,
                            // .tektronix_foreground,
                            // .tektronix_background,
                            // .highlight_background,
                            // .tektronix_cursor,
                            // .highlight_foreground,
                            // => {
                            //     log.info(
                            //         "reporting dynamic color {s} not implemented",
                            //         .{@tagName(dynamic)},
                            //     );
                            //     break :report;
                            // },
                            color::Dynamic::PointerForeground
                            | color::Dynamic::PointerBackground
                            | color::Dynamic::TektronixForeground
                            | color::Dynamic::TektronixBackground
                            | color::Dynamic::HighlightBackground
                            | color::Dynamic::TektronixCursor
                            | color::Dynamic::HighlightForeground => {
                                // log.info("reporting dynamic color {s} not implemented", ...);
                                log::info!(
                                    target: LOG_SCOPE,
                                    "reporting dynamic color {} not implemented",
                                    dynamic.tag_name()
                                );
                                break 'report;
                            }
                        },
                        // .special => {
                        //     log.info("reporting special colors not implemented", .{});
                        //     break :report;
                        // },
                        osc::color::Target::Special(_) => {
                            // log.info("reporting special colors not implemented", .{});
                            log::info!(target: LOG_SCOPE, "reporting special colors not implemented");
                            break 'report;
                        }
                    };

                    // switch (self.osc_color_report_format) {
                    match self.osc_color_report_format {
                        // .@"16-bit" => switch (kind) {
                        OSCColorReportFormat::Bit16 => match kind {
                            // .palette => |i| try writer.print(
                            //     "\x1b]4;{d};rgb:{x:0>4}/{x:0>4}/{x:0>4}",
                            //     .{
                            //         i,
                            //         @as(u16, color.r) * 257,
                            //         @as(u16, color.g) * 257,
                            //         @as(u16, color.b) * 257,
                            //     },
                            // ),
                            osc::color::Target::Palette(i) => write!(
                                VecFmt(&mut response),
                                "\x1b]4;{};rgb:{:0>4x}/{:0>4x}/{:0>4x}",
                                i,
                                u16::from(color_val.r) * 257,
                                u16::from(color_val.g) * 257,
                                u16::from(color_val.b) * 257,
                            )
                            .map_err(|_| crate::Error)?,
                            // .dynamic => |dynamic| try writer.print(
                            //     "\x1b]{d};rgb:{x:0>4}/{x:0>4}/{x:0>4}",
                            //     .{
                            //         @intFromEnum(dynamic),
                            //         @as(u16, color.r) * 257,
                            //         @as(u16, color.g) * 257,
                            //         @as(u16, color.b) * 257,
                            //     },
                            // ),
                            osc::color::Target::Dynamic(dynamic) => write!(
                                VecFmt(&mut response),
                                "\x1b]{};rgb:{:0>4x}/{:0>4x}/{:0>4x}",
                                *dynamic as u16,
                                u16::from(color_val.r) * 257,
                                u16::from(color_val.g) * 257,
                                u16::from(color_val.b) * 257,
                            )
                            .map_err(|_| crate::Error)?,
                            // .special => unreachable,
                            osc::color::Target::Special(_) => unreachable!(),
                        },

                        // .@"8-bit" => switch (kind) {
                        OSCColorReportFormat::Bit8 => match kind {
                            // .palette => |i| try writer.print(
                            //     "\x1b]4;{d};rgb:{x:0>2}/{x:0>2}/{x:0>2}",
                            //     .{
                            //         i,
                            //         @as(u16, color.r),
                            //         @as(u16, color.g),
                            //         @as(u16, color.b),
                            //     },
                            // ),
                            osc::color::Target::Palette(i) => write!(
                                VecFmt(&mut response),
                                "\x1b]4;{};rgb:{:0>2x}/{:0>2x}/{:0>2x}",
                                i,
                                u16::from(color_val.r),
                                u16::from(color_val.g),
                                u16::from(color_val.b),
                            )
                            .map_err(|_| crate::Error)?,
                            // .dynamic => |dynamic| try writer.print(
                            //     "\x1b]{d};rgb:{x:0>2}/{x:0>2}/{x:0>2}",
                            //     .{
                            //         @intFromEnum(dynamic),
                            //         @as(u16, color.r),
                            //         @as(u16, color.g),
                            //         @as(u16, color.b),
                            //     },
                            // ),
                            osc::color::Target::Dynamic(dynamic) => write!(
                                VecFmt(&mut response),
                                "\x1b]{};rgb:{:0>2x}/{:0>2x}/{:0>2x}",
                                *dynamic as u16,
                                u16::from(color_val.r),
                                u16::from(color_val.g),
                                u16::from(color_val.b),
                            )
                            .map_err(|_| crate::Error)?,
                            // .special => unreachable,
                            osc::color::Target::Special(_) => unreachable!(),
                        },

                        // .none => unreachable,
                        OSCColorReportFormat::None => unreachable!(),
                    }

                    // try writer.writeAll(terminator.string());
                    response.extend_from_slice(terminator.string());
                }
            }
        }

        // if (response.items.len > 0) {
        if !response.is_empty() {
            // If any of the operations were reports, finalize the report
            // string and send it to the terminal.
            // const msg = try termio.Message.writeReq(self.alloc, response.items);
            let msg = Message::writeReq(&response);
            // self.messageWriter(msg);
            self.messageWriter(msg);
        }
        Ok(())
    }

    // fn showDesktopNotification(
    //     self: *StreamHandler,
    //     title: []const u8,
    //     body: []const u8,
    // ) !void {
    pub(super) fn showDesktopNotification(
        &mut self,
        title: &[u8],
        body: &[u8],
    ) -> Result<(), crate::Error> {
        // self.surfaceMessageWriter(.{
        //     .desktop_notification = .init(title, body),
        // });
        self.surfaceMessageWriter(SurfaceMessage::DesktopNotification(
            SurfaceDesktopNotification::init(title, body),
        ));
        Ok(())
    }

    /// Send a report to the pty.
    // pub fn sendSizeReport(self: *StreamHandler, style: terminal.SizeReportStyle) void {
    pub fn sendSizeReport(&mut self, style: csi::SizeReportStyle) {
        match style {
            // .csi_14_t => self.messageWriter(.{ .size_report = .csi_14_t }),
            csi::SizeReportStyle::Csi14T => {
                self.messageWriter(Message::SizeReport(rustty_vt::size_report::Style::Csi14T));
            }
            // .csi_16_t => self.messageWriter(.{ .size_report = .csi_16_t }),
            csi::SizeReportStyle::Csi16T => {
                self.messageWriter(Message::SizeReport(rustty_vt::size_report::Style::Csi16T));
            }
            // .csi_18_t => self.messageWriter(.{ .size_report = .csi_18_t }),
            csi::SizeReportStyle::Csi18T => {
                self.messageWriter(Message::SizeReport(rustty_vt::size_report::Style::Csi18T));
            }
            // .csi_21_t => self.surfaceMessageWriter(.{ .report_title = .csi_21_t }),
            csi::SizeReportStyle::Csi21T => {
                self.surfaceMessageWriter(SurfaceMessage::ReportTitle(ReportTitleStyle::Csi21t));
            }
        }
    }

    // fn kittyColorReport(
    //     self: *StreamHandler,
    //     request: terminal.kitty.color.OSC,
    // ) !void {
    pub(super) fn kittyColorReport(
        &mut self,
        request: &kitty::color::OSC,
    ) -> Result<(), crate::Error> {
        // var stream: std.Io.Writer.Allocating = .init(self.alloc);
        // defer stream.deinit();
        // const writer = &stream.writer;
        let mut stream: String = String::new();

        // for (request.list.items) |item| {
        for item in request.list.iter() {
            match item {
                // .query => |key| {
                kitty::color::Request::Query(key) => {
                    // If the writer buffer is empty, we need to write our prefix
                    // if (stream.written().len == 0) try writer.writeAll("\x1b]21");
                    if stream.is_empty() {
                        stream.push_str("\x1b]21");
                    }

                    // const color: terminal.color.RGB = switch (key) {
                    let color_val: Option<color::RGB> = match key {
                        // .palette => |palette| self.terminal.colors.palette.current[palette],
                        kitty::color::Kind::Palette(palette) => {
                            Some(self.term().colors.palette.current[*palette as usize])
                        }
                        // .special => |special| switch (special) {
                        kitty::color::Kind::Special(special) => match special {
                            // .foreground => self.terminal.colors.foreground.get(),
                            kitty::color::Special::Foreground => {
                                self.term().colors.foreground.get()
                            }
                            // .background => self.terminal.colors.background.get(),
                            kitty::color::Special::Background => {
                                self.term().colors.background.get()
                            }
                            // .cursor => self.terminal.colors.cursor.get(),
                            kitty::color::Special::Cursor => self.term().colors.cursor.get(),
                            // else => {
                            //     log.warn("ignoring unsupported kitty color protocol key: {f}", .{key});
                            //     continue;
                            // },
                            _ => {
                                // log.warn("ignoring unsupported kitty color protocol key: {f}", .{key});
                                log::warn!(
                                    target: LOG_SCOPE,
                                    "ignoring unsupported kitty color protocol key: {key}"
                                );
                                continue;
                            }
                        },
                    };

                    // } orelse {
                    //     try writer.print(";{f}=", .{key});
                    //     continue;
                    // };
                    let color_val = match color_val {
                        Some(c) => c,
                        None => {
                            write!(stream, ";{key}=").map_err(|_| crate::Error)?;
                            continue;
                        }
                    };

                    // try writer.print(
                    //     ";{f}=rgb:{x:0>2}/{x:0>2}/{x:0>2}",
                    //     .{ key, color.r, color.g, color.b },
                    // );
                    write!(
                        stream,
                        ";{}=rgb:{:0>2x}/{:0>2x}/{:0>2x}",
                        key, color_val.r, color_val.g, color_val.b
                    )
                    .map_err(|_| crate::Error)?;
                }
                // .set => |v| switch (v.key) {
                kitty::color::Request::Set {
                    key,
                    color: v_color,
                } => match key {
                    // .palette => |palette| {
                    kitty::color::Kind::Palette(palette) => {
                        self.term().flags.dirty.palette = true;
                        self.term().colors.palette.set(*palette, *v_color);
                    }

                    // .special => |special| switch (special) {
                    kitty::color::Kind::Special(special) => match special {
                        // .foreground => self.terminal.colors.foreground.set(v.color),
                        kitty::color::Special::Foreground => {
                            self.term().colors.foreground.set(*v_color);
                        }
                        // .background => self.terminal.colors.background.set(v.color),
                        kitty::color::Special::Background => {
                            self.term().colors.background.set(*v_color);
                        }
                        // .cursor => self.terminal.colors.cursor.set(v.color),
                        kitty::color::Special::Cursor => {
                            self.term().colors.cursor.set(*v_color);
                        }
                        // else => {
                        //     log.warn(
                        //         "ignoring unsupported kitty color protocol key: {f}",
                        //         .{v.key},
                        //     );
                        //     continue;
                        // },
                        _ => {
                            // log.warn("ignoring unsupported kitty color protocol key: {f}", .{v.key});
                            log::warn!(
                                target: LOG_SCOPE,
                                "ignoring unsupported kitty color protocol key: {key}"
                            );
                            continue;
                        }
                    },
                },
                // .reset => |key| switch (key) {
                kitty::color::Request::Reset(key) => match key {
                    // .palette => |palette| {
                    kitty::color::Kind::Palette(palette) => {
                        self.term().flags.dirty.palette = true;
                        self.term().colors.palette.reset(*palette);
                    }

                    // .special => |special| switch (special) {
                    kitty::color::Kind::Special(special) => match special {
                        // .foreground => self.terminal.colors.foreground.reset(),
                        kitty::color::Special::Foreground => {
                            self.term().colors.foreground.reset();
                        }
                        // .background => self.terminal.colors.background.reset(),
                        kitty::color::Special::Background => {
                            self.term().colors.background.reset();
                        }
                        // .cursor => self.terminal.colors.cursor.reset(),
                        kitty::color::Special::Cursor => {
                            self.term().colors.cursor.reset();
                        }
                        // else => {
                        //     log.warn(
                        //         "ignoring unsupported kitty color protocol key: {f}",
                        //         .{key},
                        //     );
                        //     continue;
                        // },
                        _ => {
                            // log.warn("ignoring unsupported kitty color protocol key: {f}", .{key});
                            log::warn!(
                                target: LOG_SCOPE,
                                "ignoring unsupported kitty color protocol key: {key}"
                            );
                            continue;
                        }
                    },
                },
            }
        }

        // If we had any writes to our buffer, we queue them now
        // if (stream.written().len > 0) {
        if !stream.is_empty() {
            // try writer.writeAll(request.terminator.string());
            stream.push_str(
                std::str::from_utf8(request.terminator.string()).map_err(|_| crate::Error)?,
            );
            // self.messageWriter(.{
            //     .write_alloc = .{
            //         .alloc = self.alloc,
            //         .data = try stream.toOwnedSlice(),
            //     },
            // });
            self.messageWriter(Message::WriteAlloc(
                rustty_datastruct::message_data::Alloc {
                    data: stream.into_bytes().into_boxed_slice(),
                },
            ));
        }

        // Note: we don't have to do a queueRender here because every
        // processed stream will queue a render once it is done processing
        // the read() syscall.
        Ok(())
    }

    /// Display a GUI progress report.
    // fn progressReport(self: *StreamHandler, report: terminal.osc.Command.ProgressReport) void {
    pub(super) fn progressReport(&mut self, report: osc::ProgressReport) {
        // self.surfaceMessageWriter(.{ .progress_report = report });
        self.surfaceMessageWriter(SurfaceMessage::ProgressReport(report));
    }
}
