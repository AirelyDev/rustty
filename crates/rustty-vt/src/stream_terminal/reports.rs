//! PORTS: ghostty src/terminal/stream_terminal.zig — the `Handler`
//! response/report cluster: `reportMode2048`, `reportDeviceAttributes`,
//! `deviceStatus`, the visibility/enquiry/XTVERSION/size reports, window
//! title, report pwd, `requestMode`, `sendModeReport`, kitty keyboard query
//! and `apcEnd`.

use super::*;

impl<'a> Handler<'a> {
    // fn reportMode2048(self: *Handler) void {
    //     const get_size = self.effects.size orelse return;
    //     const current = get_size(self) orelse return;
    //     var buf: [128]u8 = undefined;
    //     var writer: std.Io.Writer = .fixed(buf[0 .. buf.len - 1]);
    //     size_report.encode(&writer, .mode_2048, current) catch return;
    //     buf[writer.end] = 0;
    //     self.writePty(buf[0..writer.end :0]);
    // }
    pub(super) fn report_mode_2048(&mut self) {
        let Some(get_size) = self.effects.size else {
            return;
        };
        let Some(current) = get_size(self) else {
            return;
        };

        // PORT NOTE: the reference reserves the final byte for the Zig
        // sentinel; rustty's `write_pty` takes a plain `&[u8]` (the resize
        // report precedent), so the encoded bytes are written directly.
        let mut buf = String::new();
        if size_report::encode(&mut buf, size_report::Style::Mode2048, current).is_err() {
            return;
        }
        self.write_pty(buf.as_bytes());
    }

    // fn reportDeviceAttributes(self: *Handler, req: device_attributes.Req) void {
    //     const func = self.effects.device_attributes orelse return;
    //     const attrs = func(self);
    //
    //     var stack = std.heap.stackFallback(128, self.terminal.gpa());
    //     const alloc = stack.get();
    //
    //     var aw: std.Io.Writer.Allocating = .init(alloc);
    //     defer aw.deinit();
    //
    //     attrs.encode(req, &aw.writer) catch return;
    //
    //     const written = aw.toOwnedSliceSentinel(0) catch return;
    //     defer alloc.free(written);
    //     self.writePty(written);
    // }
    //
    // PORT NOTE (vt-12b-7b, parity tail S7-4): the reference accumulates the
    // encoded response into a `std.Io.Writer.Allocating` over
    // `stackFallback(128, self.terminal.gpa())`: `StackFallbackWriter<128>` over
    // `gpa()`. `device_attributes::Attributes::encode` writes via
    // `core::fmt::Write` (matching the reference `attrs.encode`), and an encode
    // or allocation failure (`catch return`) is the `is_err()` early-out. The
    // resulting bytes (no sentinel needed at the `&[u8]` callback boundary) are
    // handed to `writePty`.
    pub(super) fn report_device_attributes(&mut self, req: device_attributes::Req) {
        let Some(func) = self.effects.device_attributes else {
            return;
        };
        let attrs = func(self);

        let gpa = self.term().gpa();
        let mut aw = crate::snapshot::io::StackFallbackWriter::<128>::new_in(gpa);
        if attrs.encode(req, &mut aw).is_err() {
            return;
        }

        self.write_pty(aw.written());
    }

    // fn deviceStatus(self: *Handler, req: device_status.Request) void {
    //     switch (req) {
    //         .operating_status => self.writePty("\x1B[0n"),
    //
    //         .cursor_position => { ... },
    //
    //         .color_scheme => { ... },
    //     }
    // }
    pub(super) fn device_status(&mut self, req: device_status::Request) {
        match req {
            // .operating_status => self.writePty("\x1B[0n"),
            device_status::Request::OperatingStatus => self.write_pty(b"\x1B[0n"),

            // .cursor_position => {
            //     const pos: struct { x: usize, y: usize } =
            //         if (self.terminal.modes.get(.origin)) .{
            //             .x = self.terminal.screens.active.cursor.x -| self.terminal.scrolling_region.left,
            //             .y = self.terminal.screens.active.cursor.y -| self.terminal.scrolling_region.top,
            //         } else .{
            //             .x = self.terminal.screens.active.cursor.x,
            //             .y = self.terminal.screens.active.cursor.y,
            //         };
            //
            //     var buf: [64]u8 = undefined;
            //     const resp = std.fmt.bufPrintZ(&buf, "\x1B[{};{}R", .{ pos.y + 1, pos.x + 1 }) catch return;
            //     self.writePty(resp);
            // },
            device_status::Request::CursorPosition => {
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

                // PORT NOTE: the reference formats into a fixed `[64]u8` via
                // `bufPrintZ`; Rust's `String` accumulator is the analog (the
                // 64-byte cap is a Zig stack-buffer detail, not observable).
                let resp = format!("\x1B[{};{}R", y + 1, x + 1);
                self.write_pty(resp.as_bytes());
            }

            // .color_scheme => {
            //     const func = self.effects.color_scheme orelse return;
            //     const scheme = func(self) orelse return;
            //     self.writePty(switch (scheme) {
            //         .dark => "\x1B[?997;1n",
            //         .light => "\x1B[?997;2n",
            //     });
            // },
            device_status::Request::ColorScheme => {
                let Some(func) = self.effects.color_scheme else {
                    return;
                };
                let Some(scheme) = func(self) else {
                    return;
                };
                let mut resp = Vec::new();
                device_status::encode_color_scheme_report(&mut resp, scheme);
                self.write_pty(&resp);
            }

            // .visibility => self.sendVisibilityReport(),
            device_status::Request::Visibility => self.send_visibility_report(),
        }
    }

    // fn sendVisibilityReport(self: *Handler) void {
    //     const write_pty = self.effects.write_pty orelse return;
    //     var buf: [device_status.max_visibility_report_encode_size + 1]u8 = undefined;
    //     var writer: std.Io.Writer = .fixed(buf[0..device_status.max_visibility_report_encode_size]);
    //     device_status.encodeVisibilityReport(&writer, ...) catch return;
    //     buf[writer.end] = 0;
    //     write_pty(self, buf[0..writer.end :0]);
    // }
    //
    // PORT NOTE: rustty's `encode_visibility_report` appends to a `Vec<u8>` (the
    // established seam shared with `encode_color_scheme_report`) and cannot
    // fail, so the reference's `catch return` has no reachable arm. The
    // sentinel byte the reference reserves is a Zig `[:0]const u8` encoding
    // detail; rustty's `write_pty` callback takes a plain `&[u8]`.
    pub(super) fn send_visibility_report(&mut self) {
        if self.effects.write_pty.is_none() {
            return;
        }
        let visibility = if self.term().flags.visible {
            device_status::Visibility::PotentiallyVisible
        } else {
            device_status::Visibility::NotVisible
        };
        let mut buf = Vec::with_capacity(device_status::MAX_VISIBILITY_REPORT_ENCODE_SIZE);
        device_status::encode_visibility_report(&mut buf, visibility);
        self.write_pty(&buf);
    }

    // fn reportEnquiry(self: *Handler) void {
    //     const func = self.effects.enquiry orelse return;
    //     const response = func(self);
    //     if (response.len == 0) return;
    //     var buf: [256]u8 = undefined;
    //     if (response.len >= buf.len) return;
    //     @memcpy(buf[0..response.len], response);
    //     buf[response.len] = 0;
    //     self.writePty(buf[0..response.len :0]);
    // }
    //
    // PORT NOTE (vt-12b-7b): the reference copies the response into a fixed
    // `[256]u8` (NUL-terminating it) and rejects responses `>= 256` bytes. We
    // preserve both guards (empty -> silent; `>= 256` -> silent); the sentinel
    // copy is a Zig encoding detail, so we pass the response slice directly.
    pub(super) fn report_enquiry(&mut self) {
        let Some(func) = self.effects.enquiry else {
            return;
        };
        let response = func(self);
        if response.is_empty() {
            return;
        }
        if response.len() >= 256 {
            return;
        }
        self.write_pty(response);
    }

    // fn reportXtversion(self: *Handler) void {
    //     const version = if (self.effects.xtversion) |func| func(self) else "";
    //     var buf: [288]u8 = undefined;
    //     const resp = std.fmt.bufPrintZ(
    //         &buf,
    //         "\x1BP>|{s}\x1B\\",
    //         .{if (version.len > 0) version else "libghostty"},
    //     ) catch return;
    //     self.writePty(resp);
    // }
    //
    // PORT NOTE (R2 ghostty->Rustty): the reference's fallback version string is
    // "libghostty"; retargeted to "librustty" per the rename rule. The DCS
    // wrapping (`ESC P > | <version> ESC \`) is unchanged.
    pub(super) fn report_xtversion(&mut self) {
        let version: &[u8] = if let Some(func) = self.effects.xtversion {
            func(self)
        } else {
            b""
        };
        // PORT NOTE: the reference formats into a fixed `[288]u8`; Rust's `Vec`
        // accumulator is the analog. A non-UTF-8 version slice is written
        // verbatim, so we build the byte sequence directly rather than via a
        // format string (the `{s}` in the reference is a raw byte splice).
        //
        // PORT NOTE: the reference's `bufPrintZ` must fit the formatted bytes
        // PLUS a NUL sentinel in the 288-byte buffer, and its `catch return`
        // turns an overflow into SILENT SUPPRESSION. A growable `Vec` has no
        // such bound, so the cap is enforced explicitly: responses whose
        // framed length + 1 exceeds 288 (i.e. an effective version string
        // longer than 281 bytes: 288 - 1 NUL - 4 `ESC P > |` - 2 `ESC \`) are
        // dropped without touching the pty.
        let mut resp: Vec<u8> = Vec::new();
        resp.extend_from_slice(b"\x1BP>|");
        if version.is_empty() {
            resp.extend_from_slice(b"librustty");
        } else {
            resp.extend_from_slice(version);
        }
        resp.extend_from_slice(b"\x1B\\");
        // `catch return` on `bufPrintZ`'s NoSpaceLeft: the formatted bytes plus
        // the NUL sentinel must fit in the 288-byte buffer.
        if resp.len() + 1 > 288 {
            return;
        }
        self.write_pty(&resp);
    }

    // fn reportSize(self: *Handler, style: csi.SizeReportStyle) void { ... }
    pub(super) fn report_size(&mut self, style: csi::SizeReportStyle) {
        // Almost all size reports will fit in 256 bytes so try that
        // on the stack before falling back to a heap allocation.
        // var stack = std.heap.stackFallback(256, self.terminal.gpa());
        // const alloc = stack.get();
        //
        // // Allocating writing to accumulate the response.
        // var aw: std.Io.Writer.Allocating = .init(alloc);
        // defer aw.deinit();
        //
        // PORT NOTE (vt-12b-7b, parity tail S7-4): `StackFallbackWriter<256>`
        // over `gpa()`. An encode/print or allocation failure (`catch return` /
        // `catch |err| { log.warn(...); return; }`) is the `is_err()` early-out.
        let gpa = self.term().gpa();
        let mut aw = crate::snapshot::io::StackFallbackWriter::<256>::new_in(gpa);

        // Build the response.
        match style {
            // .csi_21_t => {
            //     if (!self.title_report) return;
            //     const title = self.terminal.getTitle() orelse "";
            //     aw.writer.print("\x1b]l{s}\x1b\\", .{title}) catch return;
            // },
            csi::SizeReportStyle::Csi21T => {
                if !self.title_report {
                    return;
                }
                // PORT NOTE: the title is a raw byte slice spliced via `{s}`; we
                // write the bytes directly to avoid assuming UTF-8.
                use crate::snapshot::io::Writer as _;
                let title: &[u8] = self.term().get_title().unwrap_or(b"");
                if aw.write_all(b"\x1b]l").is_err()
                    || aw.write_all(title).is_err()
                    || aw.write_all(b"\x1b\\").is_err()
                {
                    return;
                }
            }

            // .csi_14_t, .csi_16_t, .csi_18_t => {
            //     const get_size = self.effects.size orelse return;
            //     const s = get_size(self) orelse return;
            //     const report_style: size_report.Style = switch (style) {
            //         .csi_14_t => .csi_14_t,
            //         .csi_16_t => .csi_16_t,
            //         .csi_18_t => .csi_18_t,
            //         .csi_21_t => unreachable,
            //     };
            //     size_report.encode(&aw.writer, report_style, s) catch |err| {
            //         log.warn("error encoding size report err={}", .{err});
            //         return;
            //     };
            // },
            csi::SizeReportStyle::Csi14T
            | csi::SizeReportStyle::Csi16T
            | csi::SizeReportStyle::Csi18T => {
                let Some(get_size) = self.effects.size else {
                    return;
                };
                let Some(s) = get_size(self) else {
                    return;
                };
                let report_style: size_report::Style = match style {
                    csi::SizeReportStyle::Csi14T => size_report::Style::Csi14T,
                    csi::SizeReportStyle::Csi16T => size_report::Style::Csi16T,
                    csi::SizeReportStyle::Csi18T => size_report::Style::Csi18T,
                    csi::SizeReportStyle::Csi21T => unreachable!(),
                };
                if let Err(err) = size_report::encode(&mut aw, report_style, s) {
                    log::warn!(target: LOG_SCOPE, "error encoding size report err={err:?}");
                    return;
                }
            }
        }

        // const resp = aw.toOwnedSliceSentinel(0) catch return;
        // defer alloc.free(resp);
        // self.writePty(resp);
        self.write_pty(aw.written());
    }

    // fn windowTitle(self: *Handler, title_raw: []const u8) !void { ... }
    pub(super) fn do_window_title(
        &mut self,
        title_raw: &[u8],
    ) -> Result<(), crate::pagelist::OutOfMemory> {
        // Prevent DoS attacks by limiting title length.
        // const max_title_len = 1024;
        // const title = if (title_raw.len > max_title_len) title: {
        //     log.warn("title length {d} exceeds max length {d}, truncating", ...);
        //     break :title title_raw[0..max_title_len];
        // } else title_raw;
        let max_title_len = 1024;
        let title = if title_raw.len() > max_title_len {
            log::warn!(
                target: LOG_SCOPE,
                "title length {} exceeds max length {max_title_len}, truncating",
                title_raw.len()
            );
            &title_raw[0..max_title_len]
        } else {
            title_raw
        };

        // try self.terminal.setTitle(title);
        //
        // PORT NOTE (oracle 439d35e27): PROMOTED from swallow-and-log to
        // propagating. The title is terminal-owned semantic state, so a failure
        // to set it must reach the shared `vt_catch` and raise
        // `semantic_failure`.
        self.term().set_title(title)?;

        // const func = self.effects.title_changed orelse return;
        // func(self);
        let Some(func) = self.effects.title_changed else {
            return Ok(());
        };
        func(self);
        Ok(())
    }

    // fn reportPwd(self: *Handler, url_raw: []const u8) !void { ... }
    pub(super) fn do_report_pwd(
        &mut self,
        url_raw: &[u8],
    ) -> Result<(), crate::pagelist::OutOfMemory> {
        // Prevent DoS attacks by limiting url length. Headroom for
        // Linux PATH_MAX (4096) plus URI scheme/host and percent-encoding.
        // const max_url_len = 4096;
        // const url = if (url_raw.len > max_url_len) url: {
        //     log.warn("pwd url length {d} exceeds max length {d}, truncating", ...);
        //     break :url url_raw[0..max_url_len];
        // } else url_raw;
        let max_url_len = 4096;
        let url = if url_raw.len() > max_url_len {
            log::warn!(
                target: LOG_SCOPE,
                "pwd url length {} exceeds max length {max_url_len}, truncating",
                url_raw.len()
            );
            &url_raw[0..max_url_len]
        } else {
            url_raw
        };

        // We store the raw payload unparsed. Embedders read it via
        // getPwd() and are responsible for decoding any URI scheme.
        // try self.terminal.setPwd(url);
        //
        // PORT NOTE (oracle 439d35e27): PROMOTED from swallow-and-log to
        // propagating; see `do_window_title`.
        self.term().set_pwd(url)?;

        // const func = self.effects.pwd_changed orelse return;
        // func(self);
        let Some(func) = self.effects.pwd_changed else {
            return Ok(());
        };
        func(self);
        Ok(())
    }

    // fn requestMode(self: *Handler, mode: modes.Mode) void {
    //     var report = self.terminal.modes.getReport(.fromMode(mode));
    //     ...
    //     self.sendModeReport(report);
    // }
    pub(super) fn do_request_mode(&mut self, mode: modes::Mode) {
        let mut report = self
            .term()
            .modes
            .get_report(modes::ReportTag::from_mode(mode));

        // Kitty paste events (mode 5522) can't work without a clipboard
        // read effect, so if that isn't set mark it as unrecognized.
        if mode == modes::Mode::KittyPasteEvents && self.effects.clipboard_read.is_none() {
            report.state = modes::State::NotRecognized;
        }

        self.send_mode_report(report);
    }

    // fn requestModeUnknown(self: *Handler, mode_raw: u16, ansi: bool) void {
    //     const report = self.terminal.modes.getReport(.{
    //         .value = mode_raw,
    //         .ansi = ansi,
    //     });
    //     self.sendModeReport(report);
    // }
    pub(super) fn do_request_mode_unknown(&mut self, mode_raw: u16, ansi: bool) {
        let report = self.term().modes.get_report(modes::ReportTag {
            value: mode_raw,
            ansi,
        });
        self.send_mode_report(report);
    }

    // fn sendModeReport(self: *Handler, report: modes.Report) void {
    //     var buf: [modes.Report.max_size + 1]u8 = undefined;
    //     var writer: std.Io.Writer = .fixed(&buf);
    //     report.encode(&writer) catch |err| {
    //         log.warn("error encoding mode report err={}", .{err});
    //         return;
    //     };
    //     const len = writer.buffered().len;
    //     buf[len] = 0;
    //     self.writePty(buf[0..len :0]);
    // }
    //
    // PORT NOTE (vt-12b-7b): the reference encodes into a fixed
    // `[Report.max_size + 1]u8` (the +1 is the NUL sentinel slot). Rust's
    // `String` accumulator is the analog; the sentinel slot is a Zig encoding
    // detail, so we pass the buffered bytes directly.
    fn send_mode_report(&mut self, report: modes::Report) {
        let mut buf = String::new();
        if let Err(err) = report.encode(&mut buf) {
            log::warn!(target: LOG_SCOPE, "error encoding mode report err={err:?}");
            return;
        }
        self.write_pty(buf.as_bytes());
    }

    // fn queryKittyKeyboard(self: *Handler) void {
    //     // Max response is "\x1b[?31u\x00" (7 bytes): the flags are a u5 (max 31).
    //     var buf: [32]u8 = undefined;
    //     const resp = std.fmt.bufPrintZ(&buf, "\x1b[?{}u", .{
    //         self.terminal.screens.active.kitty_keyboard.current().int(),
    //     }) catch return;
    //     self.writePty(resp);
    // }
    pub(super) fn query_kitty_keyboard(&mut self) {
        // Max response is "\x1b[?31u\x00" (7 bytes): the flags are a u5 (max 31).
        // PORT NOTE: the reference formats into a fixed `[32]u8`; Rust's `String`
        // is the analog accumulator.
        let flags = self.term().active_mut().kitty_keyboard.current().int();
        let resp = format!("\x1b[?{flags}u");
        self.write_pty(resp.as_bytes());
    }

    // fn apcEnd(self: *Handler) void {
    //     const alloc = self.terminal.gpa();
    //     var cmd = self.apc_handler.end() orelse return;
    //     defer cmd.deinit(alloc);
    //     switch (cmd) { ... };
    // }
    // PORT NOTE (Stage A.3 — apc complete): the apc cluster is unblocked. The
    // reference `end()` takes no allocator; the Rust `apc::Handler::end` threads
    // `alloc` for call-site fidelity (kitty `complete` consumes it in the
    // reference). The `defer cmd.deinit(alloc)` is reproduced by an explicit
    // `cmd.deinit(alloc)` at every exit path (both arms own their data via `Vec`s
    // and free on drop, so it is a no-op fidelity hook). The reference buffers the
    // response then appends a `\0` and writes `final[0..final.len-1 :0]` (a Zig
    // sentinel slice); our `write_pty` takes a bare `&[u8]`, so we write the
    // encoded bytes directly with no sentinel byte.
    pub(super) fn apc_end(&mut self, terminated: bool) {
        let alloc = self.term().gpa();
        let Some(mut cmd) = self.apc_handler.end(alloc) else {
            return;
        };

        match &mut cmd {
            // .unknown => |*unknown| {
            //     if (terminated) self.unknownSequence(.{ .apc = .{
            //         .content = unknown.content,
            //         .truncated = unknown.truncated,
            //     } });
            // },
            crate::apc::Command::Unknown(unknown) => {
                if terminated {
                    self.unknown_sequence(UnknownSequence::Apc(UnknownSequenceString {
                        content: &unknown.content,
                        truncated: unknown.truncated,
                    }));
                }
            }

            // .kitty => |*kitty_cmd| if (comptime build_options.kitty_graphics) {
            crate::apc::Command::Kitty(kitty_cmd) => {
                if crate::options::KITTY_GRAPHICS {
                    // if (self.terminal.kittyGraphics(alloc, kitty_cmd)) |resp| resp: {
                    if let Some(resp) = self.term().kitty_graphics(alloc, kitty_cmd) {
                        'resp: {
                            // Don't waste time encoding if we can't write responses
                            // anyways.
                            // if (self.effects.write_pty == null) break :resp;
                            if self.effects.write_pty.is_none() {
                                break 'resp;
                            }

                            // Encode and write the response if we have one.
                            // var buf: [1024]u8 = undefined;
                            // var writer: std.Io.Writer = .fixed(&buf);
                            // resp.encode(&writer) catch return;
                            // writer.writeByte(0) catch return;
                            // const final = writer.buffered();
                            // if (final.len > 3) self.writePty(final[0 .. final.len - 1 :0]);
                            // PORT NOTE: the reference encodes into a fixed `[1024]u8`
                            // and only writes when `final.len > 3` (i.e. more than the
                            // bare `\x1b_\x1b\\` framing plus the appended `\0`). With
                            // no sentinel byte the guard is `final.len > 2` on the
                            // encoded bytes — but the kitty `Response::encode` only
                            // emits when there is a real payload, so we keep the
                            // reference's `> 3` semantics by comparing the encoded
                            // length plus the (omitted) sentinel byte.
                            let mut buf: Vec<u8> = Vec::new();
                            // PORT NOTE: the reference `resp.encode(...) catch
                            // return` (and the glyph arm's `formatWire ... catch
                            // return`) bail without the `defer cmd.deinit` having
                            // run yet — but `cmd` is `var ...defer deinit` so it
                            // still deinits on the deferred path. Here `cmd` drops
                            // at scope end (freeing its data), so the bare `return`
                            // reproduces the reference cleanup without re-borrowing
                            // `cmd` while `kitty_cmd`/`glyph_req` is live.
                            if resp.encode(&mut buf).is_err() {
                                return;
                            }
                            let final_ = buf;
                            if final_.len() + 1 > 3 {
                                self.write_pty(&final_);
                            }
                        }
                    }
                } else {
                    unreachable!();
                }
            }

            // .glyph => |*glyph_req| if (comptime build_options.glyph_protocol) {
            //
            // PORT NOTE: the comptime gate is the const `options::GLYPH_PROTOCOL`
            // wrapped around the arm body (a Rust match guard would make the
            // match non-exhaustive when false); with the feature off the
            // sequence is consumed and safely ignored, like the reference.
            crate::apc::Command::Glyph(glyph_req) => {
                if crate::options::GLYPH_PROTOCOL {
                    // const resp = self.terminal.glyphProtocol(alloc, glyph_req);
                    let resp = self.term().glyph_protocol(alloc, glyph_req);
                    // if (resp) |r| resp_block: {
                    if let Some(r) = resp {
                        'resp_block: {
                            // Don't waste time encoding if we can't write responses
                            // anyways.
                            // if (self.effects.write_pty == null) break :resp_block;
                            if self.effects.write_pty.is_none() {
                                break 'resp_block;
                            }

                            // Glyph responses are short and bounded by the protocol
                            // fields we emit, so this matches the Kitty response
                            // buffer size above with ample headroom.
                            // var buf: [apc.glyph.Response.max_wire_bytes]u8 = undefined;
                            // var writer: std.Io.Writer = .fixed(&buf);
                            // r.formatWire(&writer) catch return;
                            // writer.writeByte(0) catch return;
                            // const final = writer.buffered();
                            // self.writePty(final[0 .. final.len - 1 :0]);
                            // PORT NOTE: `formatWire` writes to a `std.Io.Writer` over a
                            // fixed `[max_wire_bytes]u8`; the Rust `format_wire` writes to
                            // a `String` (the bounded wire payload is ASCII). We then
                            // write the UTF-8 bytes directly (no sentinel `\0`).
                            let mut buf = String::new();
                            // PORT NOTE: see the kitty-arm note above — bare `return`
                            // (cmd drops/frees) reproduces the reference `catch return`.
                            if r.format_wire(&mut buf).is_err() {
                                return;
                            }
                            self.write_pty(buf.as_bytes());
                        }
                    }
                }
            }
        }

        cmd.deinit(alloc);
    }
}
