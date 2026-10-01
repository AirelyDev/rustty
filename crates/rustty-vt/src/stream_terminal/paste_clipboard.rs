//! PORTS: ghostty src/terminal/stream_terminal.zig — the `Handler` paste,
//! clipboard (OSC 52), Kitty clipboard (OSC 5522) and Kitty drag-and-drop
//! (OSC 72) cluster: `paste`, `clipboardContents`, `clipboardRead`,
//! `kittyClipboard*`, `kittyDnd`, plus the `PtyWriter` chunker and the
//! clipboard read/write reply states.

use super::*;

impl<'a> Handler<'a> {
    /// Paste into the terminal, applying the terminal's current state
    /// as necessary to owner mode 5522, bracketed paste, unsafe paste, etc.
    /// Returns true if anything was written to the pty.
    ///
    /// The output streams to write_pty in chunks of `paste_chunk_size`.
    /// The contents are read at most once and only the pasted text
    /// representation is ever read, buffered whole while it is checked
    /// and encoded; see `terminal.paste`.
    // pub fn paste(self: *Handler, req: Paste) PasteError!bool {
    //
    // PORT NOTE (SB discipline): the reference `PtyWriter` holds `*Handler`
    // and drains through `handler.writePty` DURING the paste call, while the
    // paste context also holds `&grants` into the same Handler — mirroring
    // that would interleave a raw Handler deref with a live field borrow
    // (the `shaper-hook-sb-clean-interleave` hazard). The port's `PtyChunker`
    // therefore queues the identical chunk sequence and the handler delivers
    // it through `write_pty` immediately after the paste call returns —
    // the observable call sequence (count, sizes, order, delivery on error
    // too: "a partial paste has its frame closed") is unchanged, and no raw
    // Handler alias exists while the grants borrow is live.
    pub fn paste(&mut self, req: Paste<'_>) -> Result<bool, PasteError> {
        // SAFETY: `self` is a live, exclusively borrowed handler.
        unsafe { Self::paste_raw(self, req) }
    }

    /// [`Handler::paste`] over the reference's unprotected `self: *Handler`.
    ///
    /// PORT NOTE (N454, P7-W2 T10; LIFETIMES.tsv `stream_terminal.zig Handler
    /// (self: *Handler) in paste`): the paste pulls the text through the
    /// embedder's reader synchronously, and a C reader may re-enter the
    /// owning terminal (the reference's `io.h`/`paste.h` forbid nothing
    /// there): set a handler option, `resize`, `reset`, even `vt_write`. A
    /// `&mut self` would be a protected borrow of every handler byte for the
    /// whole call, so any of those would be Stacked/Tree Borrows UB. Here the
    /// handler is reached by place, the paste `Context` carries raw pointers
    /// (the reference's `*const Terminal` and `*Grants`), and the queued
    /// chunks go out through a narrow `&mut *this` per `write_pty` call,
    /// after the read. Behaviour is identical to the `&mut self` form.
    /// rustty-vt-c `terminal_paste` calls this over its handler pointer.
    ///
    /// # Safety
    /// `this` must point to a live handler whose `terminal` is live, and no
    /// reference to the handler may be live across the call.
    pub unsafe fn paste_raw(this: *mut Self, req: Paste<'_>) -> Result<bool, PasteError> {
        // if (self.effects.write_pty == null) return error.NoWritePty;
        // SAFETY: `this` is live (caller contract); place reads.
        if unsafe { (*this).effects.write_pty }.is_none() {
            return Err(PasteError::NoWritePty);
        }

        // var buf: [paste_chunk_size]u8 = undefined;
        // var pty: PtyWriter = .init(self, &buf);
        let mut pty = PtyChunker::new();

        // SAFETY: `(*this).terminal` aliases the Terminal that created this
        // Handler (see `term()`); only raw pointers and place reads, so no
        // borrow of it spans the reader call.
        let terminal: *const Terminal = unsafe { (*this).terminal };
        let io = unsafe { (*terminal).io };

        // .alloc = self.terminal.gpa(),
        // SAFETY: the active screen is live for the terminal's life; a raw
        // read of its allocator field (`Terminal::gpa` without `&mut`).
        let alloc = unsafe { (*(*terminal).screens.active).alloc };
        // SAFETY: `terminal` is live for the call; `grants` is a place
        // projection of the live handler and no reference to it is live
        // across the call (this function's contract).
        let result = unsafe {
            paste_pkg::paste(
                paste_pkg::Context {
                    terminal,
                    alloc,
                    // Paste events need the program's follow-up Kitty
                    // clipboard read served.
                    // .kitty_clipboard = if (self.effects.clipboard_read != null) .{
                    //     .grants = &self.kitty_clipboard_grants,
                    //     .io = self.terminal.io(),
                    // } else null,
                    kitty_clipboard: if (*this).effects.clipboard_read.is_some() {
                        Some(paste_pkg::KittyClipboard {
                            grants: std::ptr::addr_of_mut!((*this).kitty_clipboard_grants),
                            io,
                        })
                    } else {
                        None
                    },
                    writer: &mut pty,
                },
                req,
            )
        };

        // Delivered on error too: a partial paste has its frame closed
        // and the program must see that.
        // defer pty.writer.flush() catch unreachable;
        pty.flush();
        for chunk in &pty.chunks {
            // SAFETY: place read of the live handler.
            let Some(func) = (unsafe { (*this).effects.write_pty }) else {
                break;
            };
            // SAFETY: a narrow `&mut` for the effect call only (the
            // trampoline publishes its own pointer, N452).
            func(unsafe { &mut *this }, chunk);
        }

        // ... catch |err| switch (err) {
        //     // The pty writer never fails.
        //     error.WriteFailed => unreachable,
        //     error.ReadFailed, error.OutOfMemory, error.UnsafePaste,
        //     error.EntropyUnavailable, error.Canceled, => |e| e,
        // };
        result.map_err(|err| match err {
            paste_pkg::Error::WriteFailed => unreachable!("the pty writer never fails"),
            paste_pkg::Error::ReadFailed => PasteError::ReadFailed,
            paste_pkg::Error::OutOfMemory => PasteError::OutOfMemory,
            paste_pkg::Error::UnsafePaste => PasteError::UnsafePaste,
            paste_pkg::Error::EntropyUnavailable => PasteError::EntropyUnavailable,
            paste_pkg::Error::Canceled => PasteError::Canceled,
        })
    }

    // fn clipboardContents(
    //     self: *Handler,
    //     kind: u8,
    //     data: []const u8,
    //     terminator: osc.Terminator,
    // ) !void
    //
    // PORT NOTE: the reference is `!void` and its dispatch arm logs the error
    // ("Clipboard operations are external effects, not terminal state."). The
    // decode scratch is allocated through the terminal's `gpa()` (parity tail
    // S7-4 retired the `CLIPBOARD_CONTENTS_TW` stand-in); its failure returns
    // `error.OutOfMemory` without invoking the effect.
    pub(super) fn do_clipboard_contents(
        &mut self,
        kind: u8,
        data: &[u8],
        terminator: osc::Terminator,
    ) -> Result<(), crate::alloc::OutOfMemory> {
        // const location: clipboard.Location = switch (kind) { ... };
        let location = match kind {
            b's' => clipboard::Location::Selection,
            b'p' => clipboard::Location::Primary,
            _ => clipboard::Location::Standard,
        };

        // OSC 52 uses a "?" payload to request the clipboard contents.
        if data.len() == 1 && data[0] == b'?' {
            self.do_clipboard_read(location, terminator);
            return Ok(());
        }

        // const func = self.effects.clipboard_write orelse return;
        let Some(func) = self.effects.clipboard_write else {
            return Ok(());
        };

        // OSC 52 uses an empty payload to clear the selected clipboard.
        if data.is_empty() {
            func(
                self,
                clipboard::Write {
                    location,
                    contents: &[],
                    name: b"",
                    granted: false,
                    can_remember: false,
                    reply_ctx: core::ptr::null_mut(),
                    reply_fn: ignore_write_reply,
                },
            );
            return Ok(());
        }

        // Decode the base64 payload with the SIMD decoder (the same one
        // used for Kitty clipboard payloads) rather than the scalar std
        // implementation; clipboard payloads can be megabytes. The
        // Kitty clipboard spec governs OSC 52 base64 handling too: a
        // request with characters outside the base64 alphabet is
        // discarded entirely (never partially decoded), while a
        // missing-padding tail is tolerated since OSC 52 has no way
        // to report errors to the client.
        // const alloc = self.terminal.gpa();
        // const buf = try alloc.alloc(u8, simd.base64.maxLen(data));
        // defer alloc.free(buf);
        let alloc = self.term().gpa();
        let mut buf =
            kitty::clipboard_command::alloc_buf(alloc, rustty_simd::base64::max_len(data))?;
        // const decoded = simd.base64.decodeStrict(data, buf, .optional) catch {
        //     log.warn("OSC 52 clipboard write is not valid base64, ignoring", .{});
        //     return;
        // };
        let Ok(decoded) = rustty_simd::base64::decode_strict(
            data,
            &mut buf,
            rustty_simd::base64::Padding::Optional,
        ) else {
            log::warn!(target: LOG_SCOPE, "OSC 52 clipboard write is not valid base64, ignoring");
            return Ok(());
        };

        // const contents = [_]clipboard.Content{.{ .mime = "text/plain", .data = decoded }};
        let contents = [clipboard::Content {
            mime: b"text/plain",
            data: decoded,
        }];
        func(
            self,
            clipboard::Write {
                location,
                contents: &contents,
                name: b"",
                granted: false,
                can_remember: false,
                reply_ctx: core::ptr::null_mut(),
                reply_fn: ignore_write_reply,
            },
        );
        Ok(())
    }

    // fn clipboardRead(
    //     self: *Handler,
    //     location: clipboard.Location,
    //     terminator: osc.Terminator,
    // ) void
    fn do_clipboard_read(&mut self, location: clipboard::Location, terminator: osc::Terminator) {
        // const func = self.effects.clipboard_read orelse return;
        let Some(func) = self.effects.clipboard_read else {
            return;
        };

        // var state: ClipboardReadState = .{ ... };
        let mut state = ClipboardReadState {
            handler: core::ptr::from_ref(&*self).cast(),
            location,
            terminator,
            replied: false,
        };
        func(
            self,
            clipboard::Read {
                location,
                mimes: &[b"text/plain"],
                list: false,
                name: b"",
                granted: false,
                can_remember: false,
                reply_ctx: (&mut state) as *mut ClipboardReadState as *mut core::ffi::c_void,
                reply_fn: ClipboardReadState::reply,
            },
        );

        // The program is waiting on us, so a callback that returned
        // without a (successful) reply gets an empty clipboard rather
        // than silence.
        // if (!state.replied) state.respond("") catch |err| {
        //     log.warn("error replying to clipboard read err={}", .{err});
        // };
        if !state.replied {
            if let Err(err) = state.respond(self, b"") {
                log::warn!(target: LOG_SCOPE, "error replying to clipboard read err={err:?}");
            }
        }
    }

    /// Handle one Kitty clipboard protocol (OSC 5522) packet.
    // fn kittyClipboard(self: *Handler, v: Action.Value(.kitty_clipboard))
    //     error{OutOfMemory}!void
    pub(super) fn do_kitty_clipboard(
        &mut self,
        v: &osc::KittyClipboardProtocol,
    ) -> Result<(), crate::alloc::OutOfMemory> {
        // Decode and validate the metadata.
        // var arena: std.heap.ArenaAllocator = .init(self.terminal.gpa());
        // defer arena.deinit();
        // (PORT NOTE: the arena scopes the Metadata allocations; the Rust
        // Metadata owns its values, allocated through `gpa()`, and frees them
        // on drop at the end of this packet.)
        use crate::osc::parsers::kitty_clipboard_protocol::Operation;
        let gpa = self.term().gpa();
        let meta = match kitty::clipboard::Metadata::parse(gpa, v.metadata) {
            Ok(Some(meta)) => meta,
            Ok(None) => return Ok(()),
            // error.OutOfMemory => return error.OutOfMemory,
            Err(kitty::clipboard::ParseError::OutOfMemory) => {
                return Err(crate::alloc::OutOfMemory)
            }
            Err(kitty::clipboard::ParseError::InvalidValue) => {
                // const state = self.kitty_clipboard_write orelse return;
                // switch (kitty_clipboard.Metadata.operation(v.metadata) orelse return) {
                //     .wdata, .walias => self.kittyClipboardFinish(state, .EINVAL, v.terminator),
                //     .read, .write => {},
                // }
                if self.kitty_clipboard_write.is_none() {
                    return Ok(());
                }
                match kitty::clipboard::Metadata::operation(v.metadata) {
                    Some(Operation::Wdata) | Some(Operation::Walias) => {
                        if let Some(state) = self.kitty_clipboard_write.take() {
                            self.kitty_clipboard_finish(
                                &state,
                                kitty::clipboard::Status::Einval,
                                v.terminator,
                            );
                        }
                    }
                    Some(Operation::Read) | Some(Operation::Write) | None => {}
                }
                return Ok(());
            }
        };

        // const payload = v.payload orelse "";
        let payload: &[u8] = v.payload.unwrap_or(b"");
        match meta.op {
            Operation::Read => self.kitty_clipboard_read(&meta, payload, v.terminator),
            Operation::Write => self.kitty_clipboard_write_begin(&meta, v.terminator),
            Operation::Wdata => self.kitty_clipboard_data(&meta, payload, v.terminator),
            Operation::Walias => self.kitty_clipboard_alias(&meta, payload, v.terminator),
        }
    }

    // fn kittyClipboardRead(self, meta, payload, terminator) error{OutOfMemory}!void
    fn kitty_clipboard_read(
        &mut self,
        meta: &kitty::clipboard::Metadata,
        payload: &[u8],
        terminator: osc::Terminator,
    ) -> Result<(), crate::alloc::OutOfMemory> {
        // The payload is the requested MIME list. Kitty drops a read
        // request with an undecodable payload without any response.
        // const alloc = self.terminal.gpa();
        // const decoded = kitty_clipboard.Payload.init(alloc, payload) catch |err| switch (err) {
        //     error.OutOfMemory => return error.OutOfMemory,
        //     error.Invalid => return,
        // };
        // defer decoded.deinit(alloc);
        let alloc = self.term().gpa();
        let decoded = match kitty::clipboard::Payload::init(alloc, payload) {
            Ok(decoded) => decoded,
            Err(kitty::clipboard_command::PayloadError::OutOfMemory) => {
                return Err(crate::alloc::OutOfMemory)
            }
            Err(kitty::clipboard_command::PayloadError::Invalid) => return Ok(()),
        };
        if !decoded.is_valid_utf8() {
            return Ok(());
        }

        // Without a clipboard_read effect nothing can serve the read.
        // EPERM is the protocol's denial so clients degrade gracefully.
        let Some(func) = self.effects.clipboard_read else {
            self.kitty_clipboard_respond(&kitty::clipboard::Response {
                id: &meta.id,
                terminator,
                ..kitty::clipboard::Response::new(
                    crate::osc::parsers::kitty_clipboard_protocol::Operation::Read,
                    kitty::clipboard::Status::Eperm,
                )
            });
            return Ok(());
        };

        // The targets type ('.') asks for the listing of available
        // types rather than data. Requested types beyond the cap are
        // dropped and simply never served, which is how the protocol
        // reports an unavailable type anyway.
        // var mimes_buf: [kitty_clipboard.max_read_mimes][]const u8 = undefined;
        let mut mimes_buf: [&[u8]; kitty::clipboard::max_read_mimes] =
            [b""; kitty::clipboard::max_read_mimes];
        let (mimes, list): (&[&[u8]], bool) = {
            let mut targets = false;
            let mut len: usize = 0;
            let mut it = decoded.mime_iterator();
            while let Some(mime) = it.next() {
                if mime == kitty::clipboard::targets_mime {
                    targets = true;
                    continue;
                }
                if len == mimes_buf.len() {
                    continue;
                }
                mimes_buf[len] = mime;
                len += 1;
            }
            (&mimes_buf[0..len], targets)
        };

        // Per the spec a password without a name is no password. A
        // stored grant for it lets the embedder skip its prompt. A
        // prompt-exempt request never consults the grants: the
        // embedder serves it without a prompt anyway, and consuming a
        // one-time paste password on a listing would burn the grant
        // before the follow-up data read.
        let pw: &[u8] = if !meta.name.is_empty() { &meta.pw } else { b"" };
        let granted = !kitty::clipboard::read_prompt_exempt(mimes.len())
            && self
                .kitty_clipboard_grants
                .use_(alloc, pw, kitty::clipboard::Direction::Read);

        // var state: KittyClipboardReadState = .{ ... };
        let mut state = KittyClipboardReadState {
            handler: core::ptr::from_ref(&*self).cast(),
            primary: meta.loc == clipboard::Location::Primary,
            id: &meta.id,
            pw,
            mimes,
            list,
            terminator,
            replied: false,
        };
        func(
            self,
            clipboard::Read {
                location: meta.loc,
                mimes,
                list,
                name: &meta.name,
                granted,
                can_remember: !pw.is_empty(),
                reply_ctx: (&mut state) as *mut KittyClipboardReadState as *mut core::ffi::c_void,
                reply_fn: KittyClipboardReadState::reply,
            },
        );

        // The program is waiting on us, so a callback that returned
        // without a reply is answered as a denial rather than silence.
        // if (!state.replied) state.respondStatus(.EPERM);
        if !state.replied {
            state.respond_status(self, kitty::clipboard::Status::Eperm);
        }
        Ok(())
    }

    // fn kittyClipboardWriteBegin(self, meta, terminator) error{OutOfMemory}!void
    fn kitty_clipboard_write_begin(
        &mut self,
        meta: &kitty::clipboard::Metadata,
        terminator: osc::Terminator,
    ) -> Result<(), crate::alloc::OutOfMemory> {
        // A new write silently replaces any in-flight transaction.
        self.kitty_clipboard_abort();

        // Without a clipboard_write effect a commit can never succeed,
        // so fail the transaction up front instead of spooling data
        // we'd only throw away. Later wdata packets are ignored.
        if self.effects.clipboard_write.is_none() {
            self.kitty_clipboard_respond(&kitty::clipboard::Response {
                id: &meta.id,
                terminator,
                ..kitty::clipboard::Response::new(
                    crate::osc::parsers::kitty_clipboard_protocol::Operation::Write,
                    kitty::clipboard::Status::Enosys,
                )
            });
            return Ok(());
        }

        // Setup our write state
        // const alloc = self.terminal.gpa();
        // const state = try alloc.create(kitty_clipboard.WriteState);
        // errdefer alloc.destroy(state);
        // state.* = try .init(alloc, meta, .{ .max_size = ... });
        // self.kitty_clipboard_write = state;
        //
        // PORT NOTE: the state is built first and then moved into its
        // allocation; a failed `init` allocates no storage, and a failed
        // `create` drops the built state (the errdefer).
        let alloc = self.term().gpa();
        let state = kitty::clipboard::WriteState::init(
            alloc,
            meta,
            kitty::clipboard::WriteOptions {
                max_size: self.kitty_clipboard_write_max_bytes,
            },
        )?;
        let state = crate::alloc::try_create(alloc, state)?;
        self.kitty_clipboard_write = Some(state);
        Ok(())
    }

    // fn kittyClipboardData(self, meta, payload, terminator) error{OutOfMemory}!void
    fn kitty_clipboard_data(
        &mut self,
        meta: &kitty::clipboard::Metadata,
        payload: &[u8],
        terminator: osc::Terminator,
    ) -> Result<(), crate::alloc::OutOfMemory> {
        // Data without a transaction is silently ignored.
        // const state = self.kitty_clipboard_write orelse return;
        // PORT NOTE: the Box is taken out of the slot while it is operated on
        // (and put back on success) so the failure paths can respond through
        // `&mut self` without aliasing the stored transaction.
        let Some(mut state) = self.kitty_clipboard_write.take() else {
            return Ok(());
        };

        // A wdata packet without a MIME type commits the transaction.
        if meta.mime.is_empty() {
            return self.kitty_clipboard_commit(state, terminator);
        }

        // state.data(self.terminal.gpa(), meta, payload) catch |err| switch (err) { ... };
        let gpa = self.term().gpa();
        match state.data(gpa, meta, payload) {
            Ok(()) => self.kitty_clipboard_write = Some(state),

            // Failing to spool matches kitty's EIO for a failed buffer
            // write.
            Err(kitty::clipboard::DataError::OutOfMemory) => {
                self.kitty_clipboard_finish(&state, kitty::clipboard::Status::Eio, terminator);
                return Err(crate::alloc::OutOfMemory);
            }

            // Data over the write limit aborts the transaction and is
            // reported to the client.
            Err(kitty::clipboard::DataError::TooLarge) => {
                self.kitty_clipboard_finish(&state, kitty::clipboard::Status::Efbig, terminator);
            }

            // An invalid base64 payload stream aborts the transaction.
            Err(kitty::clipboard::DataError::Invalid) => {
                self.kitty_clipboard_finish(&state, kitty::clipboard::Status::Einval, terminator);
            }
        }
        Ok(())
    }

    // fn kittyClipboardAlias(self, meta, payload, terminator) error{OutOfMemory}!void
    fn kitty_clipboard_alias(
        &mut self,
        meta: &kitty::clipboard::Metadata,
        payload: &[u8],
        terminator: osc::Terminator,
    ) -> Result<(), crate::alloc::OutOfMemory> {
        // Aliases without a transaction are silently ignored. Once a
        // transaction exists, a missing target MIME type is invalid and
        // aborts the transaction.
        let Some(mut state) = self.kitty_clipboard_write.take() else {
            return Ok(());
        };
        if meta.mime.is_empty() {
            self.kitty_clipboard_finish(&state, kitty::clipboard::Status::Einval, terminator);
            return Ok(());
        }

        // state.alias(self.terminal.gpa(), meta, payload) catch |err| switch (err) { ... };
        let gpa = self.term().gpa();
        match state.alias(gpa, meta, payload) {
            Ok(()) => self.kitty_clipboard_write = Some(state),

            Err(kitty::clipboard::AliasError::OutOfMemory) => {
                self.kitty_clipboard_finish(&state, kitty::clipboard::Status::Eio, terminator);
                return Err(crate::alloc::OutOfMemory);
            }

            // An undecodable alias payload aborts the transaction.
            Err(kitty::clipboard::AliasError::Invalid) => {
                self.kitty_clipboard_finish(&state, kitty::clipboard::Status::Einval, terminator);
            }
        }
        Ok(())
    }

    // fn kittyClipboardCommit(self, state, terminator) error{OutOfMemory}!void
    fn kitty_clipboard_commit(
        &mut self,
        mut state: crate::alloc::ABox<kitty::clipboard::WriteState>,
        terminator: osc::Terminator,
    ) -> Result<(), crate::alloc::OutOfMemory> {
        // const alloc = self.terminal.gpa();
        let alloc = self.term().gpa();
        // const committed = state.commit(alloc) catch |err| switch (err) { ... };
        // defer committed.deinit(alloc);
        //
        // PORT NOTE: `commit` borrows the transaction, so the effect is
        // invoked while `committed` is live and the replied status is carried
        // out of that scope. The failure arms (a commit error, or `None` for
        // the ENOSYS arm) answer after the match, once that borrow has ended.
        let committed_status: Result<
            Option<kitty::clipboard::Status>,
            Option<kitty::clipboard::CommitError>,
        > = match state.commit(alloc) {
            Err(err) => Err(Some(err)),
            Ok(committed) => match self.effects.clipboard_write {
                // The effect can't be null here (checked when the transaction
                // began) but if an embedder cleared it mid-transaction that's
                // ENOSYS.
                None => {
                    committed.deinit(alloc);
                    Err(None)
                }
                Some(func) => {
                    // Per the spec a password without a name is no password. A
                    // stored grant for it lets the embedder skip its prompt.
                    let pw: &[u8] = if !committed.name.is_empty() {
                        committed.pw
                    } else {
                        b""
                    };
                    let granted = self.kitty_clipboard_grants.use_(
                        alloc,
                        pw,
                        kitty::clipboard::Direction::Write,
                    );

                    // var reply_state: KittyClipboardWriteReplyState = .{ .handler = self, .pw = pw };
                    let mut reply_state = KittyClipboardWriteReplyState {
                        handler: core::ptr::from_ref(&*self).cast(),
                        pw,
                        status: None,
                    };
                    func(
                        self,
                        clipboard::Write {
                            location: committed.loc,
                            contents: &committed.contents,
                            name: committed.name,
                            granted,
                            can_remember: !pw.is_empty(),
                            reply_ctx: (&mut reply_state) as *mut KittyClipboardWriteReplyState
                                as *mut core::ffi::c_void,
                            reply_fn: KittyClipboardWriteReplyState::reply,
                        },
                    );

                    let status = reply_state.status;
                    committed.deinit(alloc);
                    Ok(status)
                }
            },
        };
        let status = match committed_status {
            Ok(status) => status,
            Err(None) => {
                self.kitty_clipboard_finish(&state, kitty::clipboard::Status::Enosys, terminator);
                return Ok(());
            }
            Err(Some(kitty::clipboard::CommitError::OutOfMemory)) => {
                self.kitty_clipboard_finish(&state, kitty::clipboard::Status::Eio, terminator);
                return Err(crate::alloc::OutOfMemory);
            }

            // The last MIME type's payload stream was not correctly
            // padded, which aborts the transaction.
            Err(Some(kitty::clipboard::CommitError::Invalid)) => {
                self.kitty_clipboard_finish(&state, kitty::clipboard::Status::Einval, terminator);
                return Ok(());
            }
        };

        // The program is waiting on the commit status, so a callback
        // that returned without a reply is answered as a denial rather
        // than silence.
        self.kitty_clipboard_finish(
            &state,
            status.unwrap_or(kitty::clipboard::Status::Eperm),
            terminator,
        );
        Ok(())
    }

    /// Answer a write transaction with its final status and drop it.
    /// The id echoed is the one from the transaction's opening write
    /// packet, matching kitty.
    // fn kittyClipboardFinish(self, state: *const WriteState, status, terminator) void
    //
    // PORT NOTE: callers hand the taken-out transaction by reference; the
    // reference's trailing `kittyClipboardAbort` (destroy + null) is the drop
    // of that Box at the caller plus the abort of any stored one.
    fn kitty_clipboard_finish(
        &mut self,
        state: &kitty::clipboard::WriteState,
        status: kitty::clipboard::Status,
        terminator: osc::Terminator,
    ) {
        self.kitty_clipboard_respond(&kitty::clipboard::Response {
            id: &state.id,
            terminator,
            ..kitty::clipboard::Response::new(
                crate::osc::parsers::kitty_clipboard_protocol::Operation::Write,
                status,
            )
        });
        self.kitty_clipboard_abort();
    }

    /// Drop any in-flight write transaction without responding.
    // fn kittyClipboardAbort(self: *Handler) void {
    //     if (self.kitty_clipboard_write) |state| {
    //         state.deinit(alloc); alloc.destroy(state);
    //         self.kitty_clipboard_write = null;
    //     }
    // }
    pub(super) fn kitty_clipboard_abort(&mut self) {
        // PORT NOTE: dropping the box (allocated through `gpa()`) is the
        // deinit + destroy through that allocator.
        if let Some(state) = self.kitty_clipboard_write.take() {
            let alloc = self.term().gpa();
            crate::alloc::ABox::into_inner(state).deinit(alloc);
        }
    }

    /// Encode and write a single response packet. Unlike kitty, which
    /// always terminates responses with ST, we echo the terminator of
    /// the request being answered, matching our other OSC responses.
    // fn kittyClipboardRespond(self: *Handler, response: *const kitty_clipboard.Response) void
    fn kitty_clipboard_respond(&mut self, response: &kitty::clipboard::Response) {
        if self.effects.write_pty.is_none() {
            return;
        }

        // Our responses carry at most a status and the echoed id so
        // they virtually always fit on the stack.
        // var stack = std.heap.stackFallback(1024, self.terminal.gpa());
        // const alloc = stack.get();
        // var aw: std.Io.Writer.Allocating = .init(alloc);
        // defer aw.deinit();
        // response.encode(&aw.writer) catch return;
        // const resp = aw.toOwnedSliceSentinel(0) catch return;
        // defer alloc.free(resp);
        // self.writePty(resp);
        let mut aw = crate::snapshot::io::StackFallbackWriter::<1024>::new_in(self.term().gpa());
        if response.encode(&mut aw).is_err() {
            return;
        }
        self.write_pty(aw.written());
    }

    /// Handle an OSC 72 drag and drop command.
    // fn kittyDnd(self: *Handler, v: Action.KittyDnd)
    //     (Allocator.Error || std.Io.Writer.Error)!void
    pub(super) fn do_kitty_dnd(&mut self, v: &osc::KittyDndProtocol) -> std::io::Result<()> {
        // Responses are usually small (queries, errors) but data
        // serving can produce many chunks, so fall back to the heap.
        // var stack = std.heap.stackFallback(512, self.terminal.gpa());
        // const response_alloc = stack.get();
        // var aw: std.Io.Writer.Allocating = .init(response_alloc);
        // defer aw.deinit();
        let response_alloc = self.term().gpa();
        let mut aw = crate::snapshot::io::StackFallbackWriter::<512>::new_in(response_alloc);

        // The state is allocated on registration and owned by the
        // terminal, so it uses the terminal's allocator, not the
        // response's.
        let event = {
            // self.terminal.gpa()
            let gpa = self.term().gpa();
            // SAFETY: `self.terminal` aliases the Terminal that created this
            // Handler (see `term()`); the `&mut` on its `kitty_dnd` slot lives
            // only for this call and does not overlap another Terminal borrow.
            let slot = unsafe { &mut (*self.terminal).kitty_dnd };
            kitty::dnd::handle_command(slot, gpa, &mut aw, v)?
        };

        if !aw.written().is_empty() {
            self.write_pty(aw.written());
        }

        if let Some(ev) = event {
            let Some(func) = self.effects.drag_and_drop else {
                return Ok(());
            };
            func(self, ev);
        }
        Ok(())
    }
}

/// A writer that delivers everything through the write_pty effect:
/// the buffer as it fills, and data that doesn't fit it directly.
/// Never fails, since the effect can't.
// const PtyWriter = struct { handler: *Handler, writer: std.Io.Writer, ... };
//
// PORT NOTE (SB discipline): the reference PtyWriter drains straight through
// `handler.writePty`; the port queues the identical `paste_chunk_size` chunk
// sequence and the handler delivers it right after the paste call returns —
// see the PORT NOTE on `Handler::paste`.
struct PtyChunker {
    buf: [u8; PASTE_CHUNK_SIZE],
    end: usize,
    chunks: Vec<Vec<u8>>,
}

impl PtyChunker {
    fn new() -> PtyChunker {
        PtyChunker {
            buf: [0u8; PASTE_CHUNK_SIZE],
            end: 0,
            chunks: Vec::new(),
        }
    }

    /// The reference `pty.writer.flush()`: everything still buffered becomes
    /// the final (partial) chunk.
    fn flush(&mut self) {
        if self.end > 0 {
            self.chunks.push(self.buf[..self.end].to_vec());
            self.end = 0;
        }
    }
}

impl crate::snapshot::io::Writer for PtyChunker {
    // fn drain(w: *std.Io.Writer, data: []const []const u8, splat: usize) ...
    // (the buffered bytes go first to keep the order; full buffers become
    // whole `paste_chunk_size` chunks, exactly the reference drain cadence.)
    fn write_all(&mut self, mut bytes: &[u8]) -> Result<(), crate::snapshot::io::WriteFailed> {
        while !bytes.is_empty() {
            let n = (PASTE_CHUNK_SIZE - self.end).min(bytes.len());
            self.buf[self.end..self.end + n].copy_from_slice(&bytes[..n]);
            self.end += n;
            bytes = &bytes[n..];
            if self.end == PASTE_CHUNK_SIZE {
                self.chunks.push(self.buf.to_vec());
                self.end = 0;
            }
        }
        Ok(())
    }
}

/// Reply target for clipboard writes on protocols without a write
/// acknowledgement (OSC 52, OSC 1337 Copy): the reply is accepted
/// and discarded.
// fn ignoreWriteReply(_: *anyopaque, _: clipboard.Write.Result) void {}
fn ignore_write_reply(
    _: *mut core::ffi::c_void,
    _: *mut core::ffi::c_void,
    _: clipboard::WriteResult,
) {
}

/// Recovers the handler a reply was given (`clipboard::Read::reply` /
/// `clipboard::Write::reply`) and checks it is the one the request was made
/// on.
///
/// PORT NOTE (soundness, parity S166-F1): the reference reply states hold
/// `handler: *Handler` and use it inside `reply`. The port's states record
/// only the handler's address (never dereferenced), and the reply reaches
/// the handler through the pointer `reply` derived from the callback's own
/// `&mut Handler`; see `clipboard::Write::reply`. The assert is an invariant
/// (R7): an effect callback replies with the handler it was given.
///
/// # Safety
///
/// `handler` is the pointer `clipboard::{Read,Write}::reply` derived from a
/// live `&mut Handler`, and no other reference to the handler is used while
/// the returned one is.
unsafe fn reply_handler<'h>(
    handler: *mut core::ffi::c_void,
    expected: *const (),
) -> &'h mut Handler<'h> {
    assert!(
        core::ptr::eq(handler.cast_const().cast::<()>(), expected),
        "clipboard reply given a different handler than the request's"
    );
    // SAFETY: see the function's contract.
    unsafe { &mut *handler.cast::<Handler<'h>>() }
}

/// Reply state for one synchronous clipboard read. This lives on the
/// do_clipboard_read stack frame, so it is only valid during the callback.
// const ClipboardReadState = struct { handler, location, terminator, replied };
struct ClipboardReadState {
    /// The handler's address, compared only (see `reply_handler`).
    // handler: *Handler,
    handler: *const (),
    location: clipboard::Location,
    terminator: osc::Terminator,
    replied: bool,
}

impl ClipboardReadState {
    // fn reply(ctx: *anyopaque, result: clipboard.Read.Result) void {
    fn reply(
        ctx: *mut core::ffi::c_void,
        handler: *mut core::ffi::c_void,
        result: clipboard::ReadResult<'_>,
    ) {
        // const self: *ClipboardReadState = @ptrCast(@alignCast(ctx));
        // SAFETY: `ctx` is the `&mut ClipboardReadState` the caller passed as
        // `reply_ctx`, valid for the duration of the effect callback; no other
        // reference to the state exists while the callback runs.
        let state = unsafe { &mut *(ctx as *mut ClipboardReadState) };
        // SAFETY: `handler` comes from `clipboard::Read::reply`.
        let handler = unsafe { reply_handler(handler, state.handler) };
        if state.replied {
            log::warn!(target: LOG_SCOPE, "clipboard read replied more than once, ignoring");
            return;
        }

        // OSC 52 carries a single text value.
        let data: &[u8] = match &result {
            clipboard::ReadResult::Denied
            | clipboard::ReadResult::Unsupported
            | clipboard::ReadResult::Busy
            | clipboard::ReadResult::IoError => b"",
            clipboard::ReadResult::Success(s) => s
                .contents
                .iter()
                .find(|c| clipboard::is_text_mime(c.mime))
                .map(|c| c.data)
                .unwrap_or(b""),
        };

        // self.respond(data) catch |err| {
        //     // Leave replied unset so clipboardRead falls back to the
        //     // empty reply.
        //     log.warn("error replying to clipboard read err={}", .{err});
        //     return;
        // };
        // self.replied = true;
        if let Err(err) = state.respond(handler, data) {
            // Leave replied unset so clipboardRead falls back to the
            // empty reply.
            log::warn!(target: LOG_SCOPE, "error replying to clipboard read err={err:?}");
            return;
        }
        state.replied = true;
    }

    // fn respond(self: *ClipboardReadState, data: []const u8)
    //     error{ OutOfMemory, WriteFailed }!void
    //
    // PORT NOTE: `handler` is the reference's `self.handler`. The response is
    // encoded over `stackFallback(256, handler.terminal.gpa())`
    // (`StackFallbackWriter`), so a long reply spills through the terminal's
    // allocator and a refused allocation is `error.WriteFailed` (`Allocating`
    // maps its OutOfMemory to it; the reference's `toOwnedSliceSentinel` copy
    // is not made).
    fn respond(
        &self,
        handler: &mut Handler<'_>,
        data: &[u8],
    ) -> Result<(), crate::snapshot::io::WriteFailed> {
        use crate::snapshot::io::Writer as _;
        let kind: u8 = match self.location {
            clipboard::Location::Selection => b's',
            clipboard::Location::Primary => b'p',
            // .standard, _ => 'c',
            _ => b'c',
        };
        // var stack = std.heap.stackFallback(256, handler.terminal.gpa());
        // const alloc = stack.get();
        // var aw: std.Io.Writer.Allocating = .init(alloc);
        // defer aw.deinit();
        let mut aw = crate::snapshot::io::StackFallbackWriter::<256>::new_in(handler.term().gpa());
        // try aw.writer.print("\x1b]52;{c};", .{kind});
        aw.write_all(b"\x1b]52;")?;
        aw.write_byte(kind)?;
        aw.write_all(b";")?;
        // try std.base64.standard.Encoder.encodeWriter(&aw.writer, data);
        crate::kitty::clipboard_response::b64_encode_writer(&mut aw, data)?;
        // try aw.writer.writeAll(self.terminator.string());
        aw.write_all(self.terminator.string())?;
        // handler.writePty(written);
        handler.write_pty(aw.written());
        Ok(())
    }
}

/// Reply state for one synchronous Kitty clipboard read. This lives
/// on the kitty_clipboard_read stack frame, so it is only valid during
/// the callback.
// const KittyClipboardReadState = struct { handler, primary, id, pw, mimes,
//     list, terminator, replied };
struct KittyClipboardReadState<'s> {
    /// The handler's address, compared only (see `reply_handler`).
    // handler: *Handler,
    handler: *const (),
    primary: bool,
    id: &'s [u8],

    /// The effective password, empty when the request had none.
    pw: &'s [u8],

    /// The requested types; only these are served from a reply.
    mimes: &'s [&'s [u8]],
    list: bool,
    terminator: osc::Terminator,
    replied: bool,
}

impl KittyClipboardReadState<'_> {
    // fn reply(ctx: *anyopaque, result: clipboard.Read.Result) void {
    fn reply(
        ctx: *mut core::ffi::c_void,
        handler: *mut core::ffi::c_void,
        result: clipboard::ReadResult<'_>,
    ) {
        // SAFETY: `ctx` is the `&mut KittyClipboardReadState` the caller
        // passed as `reply_ctx`, valid for the duration of the effect
        // callback; no other reference to the state exists while it runs.
        let state = unsafe { &mut *(ctx as *mut KittyClipboardReadState<'_>) };
        // SAFETY: `handler` comes from `clipboard::Read::reply`.
        let handler = unsafe { reply_handler(handler, state.handler) };
        if state.replied {
            log::warn!(target: LOG_SCOPE, "clipboard read replied more than once, ignoring");
            return;
        }
        state.replied = true;

        let success = match result {
            clipboard::ReadResult::Denied => {
                return state.respond_status(handler, kitty::clipboard::Status::Eperm);
            }
            clipboard::ReadResult::Unsupported => {
                return state.respond_status(handler, kitty::clipboard::Status::Enosys);
            }
            clipboard::ReadResult::Busy => {
                return state.respond_status(handler, kitty::clipboard::Status::Ebusy);
            }
            clipboard::ReadResult::IoError => {
                return state.respond_status(handler, kitty::clipboard::Status::Eio);
            }
            clipboard::ReadResult::Success(s) => s,
        };

        // Remembering is only offered when the request carried a
        // usable password.
        // if (success.remember and self.pw.len > 0) {
        //     self.handler.kitty_clipboard_grants.grant(gpa, self.pw, .read, false) catch |err| {
        //         log.warn("error recording clipboard grant err={}", .{err});
        //     };
        // }
        if success.remember && !state.pw.is_empty() {
            let gpa = handler.term().gpa();
            if let Err(err) = handler.kitty_clipboard_grants.grant(
                gpa,
                state.pw,
                kitty::clipboard::Direction::Read,
                false,
            ) {
                log::warn!(target: LOG_SCOPE, "error recording clipboard grant err={err:?}");
            }
        }

        // self.respondSuccess(&success) catch |err| {
        //     log.warn("error replying to clipboard read err={}", .{err});
        //     self.respondStatus(.EIO);
        // };
        if let Err(err) = state.respond_success(handler, &success) {
            log::warn!(target: LOG_SCOPE, "error replying to clipboard read err={err:?}");
            state.respond_status(handler, kitty::clipboard::Status::Eio);
        }
    }

    /// Answer with a single status packet.
    // fn respondStatus(self, status: kitty_clipboard.Status) void
    fn respond_status(&self, handler: &mut Handler<'_>, status: kitty::clipboard::Status) {
        // self.handler.kittyClipboardRespond(&.{ .op = .read, .status = status,
        //     .id = self.id, .terminator = self.terminator });
        handler.kitty_clipboard_respond(&kitty::clipboard::Response {
            id: self.id,
            terminator: self.terminator,
            ..kitty::clipboard::Response::new(
                crate::osc::parsers::kitty_clipboard_protocol::Operation::Read,
                status,
            )
        });
    }

    /// Answer with the full success sequence (OK, listing, DATA
    /// chunks, DONE), serving only the requested representations
    /// in request order.
    // fn respondSuccess(self, success: *const clipboard.Read.Result.Success)
    //     error{ OutOfMemory, WriteFailed }!void
    fn respond_success(
        &self,
        handler: &mut Handler<'_>,
        success: &clipboard::ReadResultSuccess<'_>,
    ) -> Result<(), crate::snapshot::io::WriteFailed> {
        // const handler = self.handler;
        // if (handler.effects.write_pty == null) return;
        if handler.effects.write_pty.is_none() {
            return Ok(());
        }

        // var served_buf: [kitty_clipboard.max_read_mimes]clipboard.Content = undefined;
        let mut served_buf: [clipboard::Content; kitty::clipboard::max_read_mimes] =
            [clipboard::Content {
                mime: b"",
                data: b"",
            }; kitty::clipboard::max_read_mimes];
        let mut served_len: usize = 0;
        for mime in self.mimes {
            for content in success.contents {
                if content.mime != *mime {
                    continue;
                }
                served_buf[served_len] = *content;
                served_len += 1;
                break;
            }
        }

        // Status packets fit on the stack; DATA packets carry the
        // clipboard contents and fall back to the heap.
        // var stack = std.heap.stackFallback(1024, handler.terminal.gpa());
        // const alloc = stack.get();
        // var aw: std.Io.Writer.Allocating = .init(alloc);
        // defer aw.deinit();
        let mut aw = crate::snapshot::io::StackFallbackWriter::<1024>::new_in(handler.term().gpa());
        // try (kitty_clipboard.ReadSuccess{ ... }).encode(&aw.writer);
        (kitty::clipboard::ReadSuccess {
            primary: self.primary,
            id: self.id,
            list: self.list,
            available: success.available,
            contents: &served_buf[0..served_len],
            terminator: self.terminator,
            ..kitty::clipboard::ReadSuccess::default()
        })
        .encode(&mut aw)?;
        // handler.writePty(written);
        handler.write_pty(aw.written());
        Ok(())
    }
}

/// Reply state for one synchronous Kitty clipboard write. This lives
/// on the kitty_clipboard_commit stack frame, so it is only valid
/// during the callback.
// const KittyClipboardWriteReplyState = struct { handler, pw, status };
struct KittyClipboardWriteReplyState<'s> {
    /// The handler's address, compared only (see `reply_handler`).
    // handler: *Handler,
    handler: *const (),

    /// The effective password, empty when the request had none.
    pw: &'s [u8],

    /// The replied commit status, mapped 1:1 from the reply result;
    /// null until the callback replies.
    status: Option<kitty::clipboard::Status>,
}

impl KittyClipboardWriteReplyState<'_> {
    // fn reply(ctx: *anyopaque, result: clipboard.Write.Result) void {
    fn reply(
        ctx: *mut core::ffi::c_void,
        handler: *mut core::ffi::c_void,
        result: clipboard::WriteResult,
    ) {
        // SAFETY: `ctx` is the `&mut KittyClipboardWriteReplyState` the caller
        // passed as `reply_ctx`, valid for the duration of the effect
        // callback; no other reference to the state exists while it runs.
        let state = unsafe { &mut *(ctx as *mut KittyClipboardWriteReplyState<'_>) };
        // SAFETY: `handler` comes from `clipboard::Write::reply`.
        let handler = unsafe { reply_handler(handler, state.handler) };
        if state.status.is_some() {
            log::warn!(target: LOG_SCOPE, "clipboard write replied more than once, ignoring");
            return;
        }
        state.status = Some(match result {
            clipboard::WriteResult::Denied => kitty::clipboard::Status::Eperm,
            clipboard::WriteResult::Unsupported => kitty::clipboard::Status::Enosys,
            clipboard::WriteResult::Busy => kitty::clipboard::Status::Ebusy,
            clipboard::WriteResult::InvalidData => kitty::clipboard::Status::Einval,
            clipboard::WriteResult::IoError => kitty::clipboard::Status::Eio,
            clipboard::WriteResult::Success(success) => {
                // Remembering is only offered when the request
                // carried a usable password.
                if success.remember && !state.pw.is_empty() {
                    // self.handler.kitty_clipboard_grants.grant(
                    //     self.handler.terminal.gpa(), self.pw, .write, false,
                    // ) catch |err| {
                    //     log.warn("error recording clipboard grant err={}", .{err});
                    // };
                    let gpa = handler.term().gpa();
                    if let Err(err) = handler.kitty_clipboard_grants.grant(
                        gpa,
                        state.pw,
                        kitty::clipboard::Direction::Write,
                        false,
                    ) {
                        log::warn!(
                            target: LOG_SCOPE,
                            "error recording clipboard grant err={err:?}"
                        );
                    }
                }
                kitty::clipboard::Status::Done
            }
        });
    }
}
