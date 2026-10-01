//! PORTS: ghostty src/termio/stream_handler.zig — the `StreamHandler` DCS and APC methods (`dcsHook`,
//! `dcsPut`, `dcsUnhook`, `dcsCommand`, `apcEnd`, `apcCommand`).

use super::*;

impl<M: AppMailbox> StreamHandler<M> {
    // pub inline fn dcsHook(self: *StreamHandler, dcs: terminal.DCS) !void {
    #[inline]
    pub fn dcsHook(&mut self, dcs: rustty_vt::parser::DCS) -> Result<(), crate::Error> {
        // var cmd = self.dcs.hook(self.alloc, dcs) orelse return;
        // (PORT NOTE: the trait hands the parser's owned DCS; the dcs Handler takes
        // the borrowed view — the same coercion Zig's `terminal.DCS` does.)
        let mut cmd = match self.dcs.hook(
            self.alloc.into(),
            rustty_vt::dcs::DCS {
                intermediates: &dcs.intermediates,
                params: &dcs.params,
                final_: dcs.final_,
            },
        ) {
            Some(cmd) => cmd,
            None => return Ok(()),
        };
        // defer cmd.deinit();
        let result = self.dcsCommand(&mut cmd);
        cmd.deinit();
        // try self.dcsCommand(&cmd);
        result
    }

    // pub inline fn dcsPut(self: *StreamHandler, byte: u8) !void {
    #[inline]
    pub fn dcsPut(&mut self, byte: u8) -> Result<(), crate::Error> {
        // var cmd = self.dcs.put(byte) orelse return;
        let mut cmd = match self.dcs.put(byte) {
            Some(cmd) => cmd,
            None => return Ok(()),
        };
        // defer cmd.deinit();
        let result = self.dcsCommand(&mut cmd);
        cmd.deinit();
        // try self.dcsCommand(&cmd);
        result
    }

    // pub inline fn dcsUnhook(self: *StreamHandler) !void {
    #[inline]
    pub fn dcsUnhook(&mut self) -> Result<(), crate::Error> {
        // var cmd = self.dcs.unhook() orelse return;
        let mut cmd = match self.dcs.unhook() {
            Some(cmd) => cmd,
            None => return Ok(()),
        };
        // defer cmd.deinit();
        let result = self.dcsCommand(&mut cmd);
        cmd.deinit();
        // try self.dcsCommand(&cmd);
        result
    }

    // fn dcsCommand(self: *StreamHandler, cmd: *terminal.dcs.Command) !void {
    pub(super) fn dcsCommand(
        &mut self,
        cmd: &mut rustty_vt::dcs::Command,
    ) -> Result<(), crate::Error> {
        // log.warn("DCS command: {}", .{cmd});
        match cmd {
            // .tmux => |tmux| tmux: {
            //
            // PORT NOTE (Rust-only glue, three seams — the body below is
            // otherwise 1:1 with the reference):
            //
            //   1. `tmux: { ... break :tmux; }` is a Rust labeled block
            //      (`'tmux: { ... break 'tmux; }`) — the direct analogue.
            //   2. The reference binds `|tmux|` as a *copy* of the union
            //      payload, whose `[]const u8` members alias parser-owned
            //      memory; the Rust `ControlNotification` owns its `Vec<u8>`
            //      payloads, so the value is MOVED out of `*cmd` with a
            //      `mem::replace` tombstone. That is safe because
            //      `dcs::Command::deinit`'s `Tmux` arm is a no-op (dcs.rs) —
            //      the caller's `cmd.deinit()` never reads the payload again —
            //      and because the move happens only on the fallthrough arm,
            //      after `enter`/`exit` have been dispatched and broken out.
            //   3. `viewer.next()` returns a slice borrowed from the viewer
            //      while the loop body needs `&mut self` (`messageWriter`), so
            //      the `Box<Viewer>` is `take()`n out of `self.tmux_viewer` for
            //      the duration of the loop and restored after it. Nothing the
            //      loop calls reads `tmux_viewer`.
            //
            // The reference's `viewer.* = try .init(global.io(), self.alloc)`
            // passes the Zig 0.16 `std.Io` instance; rustty's
            // `tmux::Viewer::init` takes only the allocator (the vt port's
            // signature — no `std.Io` analogue), so only `alloc` is threaded.
            rustty_vt::dcs::Command::Tmux(tmux) => 'tmux: {
                // If tmux control mode is disabled at the build level,
                // then this whole block shouldn't be analyzed.
                // if (comptime !tmux_enabled) break :tmux;
                if !TMUX_ENABLED {
                    break 'tmux;
                }
                // log.info("tmux control mode event cmd={f}", .{tmux});
                log::info!(target: LOG_SCOPE, "tmux control mode event cmd={tmux}");

                // switch (tmux) {
                match tmux {
                    // .enter => {
                    rustty_vt::tmux::ControlNotification::Enter => {
                        // Setup our viewer state
                        // assert(self.tmux_viewer == null);
                        //
                        // PORT NOTE: reference defect (R13, UPSTREAM-DEFECTS.md
                        // UD6). In the reference this assert is reachable: a
                        // control-mode block over the parser's 1 MiB limit ends
                        // the DCS without an `.exit`, so the viewer survives
                        // into the next `ESC P 1000 p`. The port's
                        // `dcs::Handler::put` emits `.exit` when it discards a
                        // tmux state, so every way out of the tmux DCS state
                        // (`unhook`, a discarded `put`, handler teardown) frees
                        // the viewer first and the assert is an invariant again
                        // (R7). Tests: `tmux_oversized_control_block_ends_the_viewer`,
                        // `tmux_reattach_after_an_oversized_control_block_does_not_crash`.
                        assert!(self.tmux_viewer.is_none());
                        // const viewer = try self.alloc.create(terminal.tmux.Viewer);
                        // errdefer self.alloc.destroy(viewer);
                        // viewer.* = try .init(global.io(), self.alloc);
                        // errdefer viewer.deinit();
                        let viewer = rustty_vt::tmux::Viewer::init(self.alloc.into())
                            .map_err(|_| crate::Error)?;
                        // self.tmux_viewer = viewer;
                        self.tmux_viewer = Some(Box::new(viewer));
                        // break :tmux;
                        break 'tmux;
                    }

                    // .exit => {
                    rustty_vt::tmux::ControlNotification::Exit => {
                        // Free our viewer state if we have one
                        // if (self.tmux_viewer) |viewer| {
                        //     viewer.deinit();
                        //     self.alloc.destroy(viewer);
                        //     self.tmux_viewer = null;
                        // }
                        if let Some(mut viewer) = self.tmux_viewer.take() {
                            viewer.deinit();
                            // self.alloc.destroy(viewer); — Box drop
                            // self.tmux_viewer = null; — the take() above
                        }

                        // And always break since we assert below
                        // that we're not handling an exit command.
                        // break :tmux;
                        break 'tmux;
                    }

                    // else => {},
                    _ => {}
                }

                // assert(tmux != .enter);
                // assert(tmux != .exit);
                //
                // Both are structurally guaranteed: the two arms above always
                // `break 'tmux`, so only the `else` fallthrough reaches here.
                debug_assert!(!matches!(
                    tmux,
                    rustty_vt::tmux::ControlNotification::Enter
                        | rustty_vt::tmux::ControlNotification::Exit
                ));

                // const viewer = self.tmux_viewer orelse {
                let Some(mut viewer) = self.tmux_viewer.take() else {
                    // This can only really happen if we failed to
                    // initialize the viewer on enter.
                    // log.info(
                    //     "received tmux control mode command without viewer: {f}",
                    //     .{tmux},
                    // );
                    log::info!(
                        target: LOG_SCOPE,
                        "received tmux control mode command without viewer: {tmux}"
                    );
                    //
                    // break :tmux;
                    break 'tmux;
                };

                // (seam 2 — see the PORT NOTE above)
                let notification =
                    core::mem::replace(tmux, rustty_vt::tmux::ControlNotification::Exit);

                // for (viewer.next(.{ .tmux = tmux })) |action| {
                for action in viewer.next(rustty_vt::tmux::viewer::Input::Tmux(notification)) {
                    // log.info("tmux viewer action={f}", .{action});
                    log::info!(target: LOG_SCOPE, "tmux viewer action={action}");
                    // switch (action) {
                    match action {
                        // .exit => {
                        rustty_vt::tmux::viewer::Action::Exit => {
                            // We ignore this because we will fully exit when
                            // our DCS connection ends. We may want to handle
                            // this in the future to notify our GUI we're
                            // disconnected though.
                        }

                        // .command => |command| {
                        rustty_vt::tmux::viewer::Action::Command(command) => {
                            // assert(command.len > 0);
                            assert!(!command.is_empty());
                            // assert(command[command.len - 1] == '\n');
                            assert!(command[command.len() - 1] == b'\n');
                            // self.messageWriter(try termio.Message.writeReq(
                            //     self.alloc,
                            //     command,
                            // ));
                            self.messageWriter(Message::writeReq(command));
                        }

                        // .windows => {
                        rustty_vt::tmux::viewer::Action::Windows(_) => {
                            // TODO
                        }
                    }
                }
                self.tmux_viewer = Some(viewer);
            }

            // .xtgettcap => |*gettcap| {
            rustty_vt::dcs::Command::Xtgettcap(gettcap) => {
                // const map = comptime terminfo.ghostty.xtgettcapMap();
                let map = xtgettcapMap();
                // while (gettcap.next()) |key| {
                while let Some(key) = gettcap.next() {
                    // const response = map.get(key) orelse continue;
                    let response: &'static [u8] = match map.get(key) {
                        Some(response) => response,
                        None => continue,
                    };
                    // self.messageWriter(.{ .write_stable = response });
                    self.messageWriter(Message::WriteStable(response));
                }
            }

            // .decrqss => |decrqss| {
            //     var response: [terminal.dcs.Command.DECRQSS.max_response_bytes]u8 = undefined;
            //     const encoded = try decrqss.encode(self.terminal, &response);
            //     const msg = try termio.Message.writeReq(self.alloc, encoded);
            //     self.messageWriter(msg);
            // },
            //
            // PORT NOTE (oracle 39ae85f04, area E11): the 60-line inline
            // DECRQSS builder that used to live here — the Sgr/Decscusr/
            // Decstbm/Decslrm arms with hand-rolled prefix-offset arithmetic —
            // is DELETED. `dcs::Decrqss::encode` is now the single owner so the
            // full termio path and the libghostty-vt terminal stream emit
            // byte-identical replies.
            rustty_vt::dcs::Command::Decrqss(decrqss) => {
                let mut response = [0u8; rustty_vt::dcs::Decrqss::MAX_RESPONSE_BYTES];
                let encoded_len = {
                    let t: *mut Terminal = self.terminal;
                    // SAFETY: `self.terminal` is the live Termio-owned terminal
                    // (see `term()`); the deref is short-lived.
                    let encoded = decrqss
                        .encode(unsafe { &mut *t }, &mut response)
                        .map_err(|_| crate::Error)?;
                    encoded.len()
                };
                let msg = Message::writeReq(&response[0..encoded_len]);
                self.messageWriter(msg);
            }
        }
        Ok(())
    }

    // pub fn apcEnd(self: *StreamHandler) !void {
    pub fn apcEnd(&mut self) -> Result<(), crate::Error> {
        // var result = self.apc.end() orelse return;
        let mut result = match self.apc.end(self.alloc.into()) {
            Some(result) => result,
            None => return Ok(()),
        };
        // defer result.deinit(self.alloc);
        let r = self.apcCommand(&mut result);
        result.deinit(self.alloc.into());
        r
    }

    // (PORT NOTE, Rust-only glue: the `switch (result)` body of apcEnd, factored
    // so the `defer result.deinit(self.alloc)` runs on every path.)
    fn apcCommand(&mut self, cmd: &mut rustty_vt::apc::Command) -> Result<(), crate::Error> {
        // log.warn("APC command: {}", .{result});
        match cmd {
            // .unknown => return,
            rustty_vt::apc::Command::Unknown(_) => return Ok(()),
            // .kitty => |*kitty_cmd| {
            rustty_vt::apc::Command::Kitty(kitty_cmd) => {
                // if (self.terminal.kittyGraphics(self.alloc, kitty_cmd)) |resp| {
                if let Some(resp) = self.term().kitty_graphics(self.alloc.into(), kitty_cmd) {
                    // var buf: [1024]u8 = undefined;
                    // var writer: std.Io.Writer = .fixed(&buf);
                    let mut buf = [0u8; 1024];
                    let mut cursor = std::io::Cursor::new(&mut buf[..]);
                    // try resp.encode(&writer);
                    resp.encode(&mut cursor).map_err(|_| crate::Error)?;
                    // const final = writer.buffered();
                    let len = cursor.position() as usize;
                    let final_ = &buf[..len];
                    // if (final.len > 2) {
                    if final_.len() > 2 {
                        // log.debug("kitty graphics response: {x}", .{final});
                        log::debug!(
                            target: LOG_SCOPE,
                            "kitty graphics response: {}",
                            HexBytes(final_)
                        );
                        // self.messageWriter(try termio.Message.writeReq(self.alloc, final));
                        let msg = Message::writeReq(final_);
                        self.messageWriter(msg);
                    }
                }
            }

            // .glyph => |*glyph_req| {
            rustty_vt::apc::Command::Glyph(glyph_req) => {
                // const resp = self.terminal.glyphProtocol(self.alloc, glyph_req);
                let resp = self.term().glyph_protocol(self.alloc.into(), glyph_req);
                // switch (glyph_req.*) {
                //     .register, .clear => try self.queueRender(),
                //     .support, .query => {},
                // }
                match glyph_req {
                    rustty_vt::apc::glyph::Request::Register(_)
                    | rustty_vt::apc::glyph::Request::Clear(_) => {
                        self.queueRender().map_err(|_| crate::Error)?;
                    }
                    rustty_vt::apc::glyph::Request::Support
                    | rustty_vt::apc::glyph::Request::Query(_) => {}
                }

                // if (resp) |r| {
                if let Some(r) = resp {
                    // var buf: [terminal.apc.glyph.Response.max_wire_bytes]u8 = undefined;
                    // var writer: std.Io.Writer = .fixed(&buf);
                    let mut buf = String::new();
                    // try r.formatWire(&writer);
                    r.format_wire(&mut buf).map_err(|_| crate::Error)?;
                    // const final = writer.buffered();
                    let final_ = buf.as_bytes();
                    // log.debug("glyph protocol response: {x}", .{final});
                    log::debug!(
                        target: LOG_SCOPE,
                        "glyph protocol response: {}",
                        HexBytes(final_)
                    );
                    // self.messageWriter(try termio.Message.writeReq(self.alloc, final));
                    let msg = Message::writeReq(final_);
                    self.messageWriter(msg);
                }
            }
        }
        Ok(())
    }
}
