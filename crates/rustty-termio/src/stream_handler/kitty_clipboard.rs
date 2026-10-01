//! PORTS: ghostty src/termio/stream_handler.zig — the `StreamHandler` Kitty clipboard protocol (OSC 5522)
//! methods (`kittyClipboardGrant` through `kittyClipboardWriteAbort`).

use super::*;

impl<M: AppMailbox> StreamHandler<M> {
    /// Record a Kitty clipboard protocol session grant so future
    /// requests with this password skip the permission prompt.
    // pub fn kittyClipboardGrant(
    //     self: *StreamHandler,
    //     pw: []const u8,
    //     dir: terminal.kitty.clipboard.Grants.Direction,
    // ) error{OutOfMemory}!void {
    //     try self.kitty_clipboard_grants.grant(self.alloc, pw, dir, false);
    // }
    pub fn kittyClipboardGrant(
        &mut self,
        pw: &[u8],
        dir: rustty_vt::kitty::clipboard::Direction,
    ) -> Result<(), rustty_vt::alloc::OutOfMemory> {
        self.kitty_clipboard_grants
            .grant(self.alloc.into(), pw, dir, false)
    }

    /// Handle one Kitty clipboard protocol (OSC 5522) packet.
    // fn kittyClipboard(
    //     self: *StreamHandler,
    //     v: terminal.osc.Command.KittyClipboardProtocol,
    // ) error{ OutOfMemory, WriteFailed }!void {
    pub(super) fn kittyClipboard(
        &mut self,
        v: &osc::KittyClipboardProtocol,
    ) -> Result<(), crate::Error> {
        use rustty_vt::kitty::clipboard as kitty_clipboard;
        use rustty_vt::osc::parsers::kitty_clipboard_protocol::Operation;

        // Decode and validate the metadata. Malformed structure drops
        // the packet without a response. Invalid decoded text on a write
        // data or alias packet aborts an in-flight transaction.
        // var arena: std.heap.ArenaAllocator = .init(self.alloc);
        // defer arena.deinit();
        // (the arena scopes the Metadata allocations; the Rust Metadata owns
        // its Vecs and frees on drop.)
        let meta = match kitty_clipboard::Metadata::parse(self.alloc.into(), v.metadata) {
            Ok(Some(meta)) => meta,
            Ok(None) => return Ok(()),
            Err(kitty_clipboard::ParseError::OutOfMemory) => return Ok(()),
            Err(kitty_clipboard::ParseError::InvalidValue) => {
                // const state = self.kitty_clipboard_write orelse return;
                // switch (kitty_clipboard.Metadata.operation(v.metadata) orelse return) {
                //     .wdata, .walias => try self.kittyClipboardWriteFinish(
                //         state, .EINVAL, v.terminator),
                //     .read, .write => {},
                // }
                if self.kitty_clipboard_write.is_none() {
                    return Ok(());
                }
                match kitty_clipboard::Metadata::operation(v.metadata) {
                    Some(Operation::Wdata) | Some(Operation::Walias) => {
                        if let Some(state) = self.kitty_clipboard_write.take() {
                            self.kittyClipboardWriteFinish(
                                &state,
                                kitty_clipboard::Status::Einval,
                                v.terminator,
                            )?;
                        }
                    }
                    Some(Operation::Read) | Some(Operation::Write) | None => {}
                }
                return Ok(());
            }
        };

        let payload: &[u8] = v.payload.unwrap_or(b"");
        match meta.op {
            Operation::Read => self.kittyClipboardRead(&meta, payload, v.terminator),
            Operation::Write => self.kittyClipboardWriteBegin(&meta, v.terminator),
            Operation::Wdata => self.kittyClipboardWriteData(&meta, payload, v.terminator),
            Operation::Walias => self.kittyClipboardWriteAlias(&meta, payload, v.terminator),
        }
    }

    // fn kittyClipboardRead(self, meta, payload, terminator) !void {
    fn kittyClipboardRead(
        &mut self,
        meta: &rustty_vt::kitty::clipboard::Metadata,
        payload: &[u8],
        terminator: osc::Terminator,
    ) -> Result<(), crate::Error> {
        use rustty_vt::kitty::clipboard as kitty_clipboard;

        // Everything about the request, including the request struct
        // itself, lives in a single arena that crosses to the surface
        // thread, which owns it from the moment the message is sent.
        //
        // PORT NOTE (assessment E hazard #1): the arena-owned request maps to
        // the OWNED `apprt::KittyRead` boxed into the surface message.

        // The payload is the requested MIME list. A read request with
        // an undecodable payload is dropped without any response,
        // matching kitty.
        let decoded = match kitty_clipboard::Payload::init(self.alloc.into(), payload) {
            Ok(decoded) => decoded,
            Err(_) => return Ok(()),
        };
        if !decoded.is_valid_utf8() {
            return Ok(());
        }

        // The targets type ('.') asks for the listing of available
        // types rather than data. Requested types beyond the cap are
        // dropped and simply never served, which is how the protocol
        // reports an unavailable type anyway.
        // var mimes_buf: [kitty_clipboard.max_read_mimes][]const u8 = undefined;
        let mut mimes: Vec<Vec<u8>> = Vec::new();
        let mut list = false;
        let mut it = decoded.mime_iterator();
        while let Some(mime) = it.next() {
            if mime == kitty_clipboard::targets_mime {
                list = true;
                continue;
            }
            if mimes.len() == kitty_clipboard::max_read_mimes {
                continue;
            }
            mimes.push(mime.to_vec());
        }

        // Per the spec a password without a name is no password. A
        // stored session grant for it lets the surface skip its
        // permission prompt.
        let pw: &[u8] = if !meta.name.is_empty() { &meta.pw } else { b"" };
        let granted = self.kittyClipboardReadGranted(pw, mimes.len());

        // const req = try alloc.create(apprt.ClipboardRequest.KittyRead);
        // ...dupes... req.* = .{
        //     // The arena must be copied in last so it tracks every
        //     // allocation above.
        //     .arena = arena,
        //     ... };
        // (no arena in the port: the Box and its owned Vecs carry every
        // allocation, so there is no copy-order hazard.)
        let req = Box::new(rustty_apprt_api::KittyRead {
            location: match meta.loc {
                rustty_vt::clipboard::Location::Primary => rustty_apprt_api::Clipboard::Primary,
                _ => rustty_apprt_api::Clipboard::Standard,
            },
            mimes,
            list,
            id: meta.id.to_vec(),
            pw: pw.to_vec(),
            name: meta.name.to_vec(),
            granted,
            terminator,
        });

        // self.surfaceMessageWriter(.{ .kitty_clipboard_read = req });
        self.surfaceMessageWriter(SurfaceMessage::KittyClipboardRead(req));
        Ok(())
    }

    /// Whether a session grant covers a read request, consuming
    /// one-time grants. A prompt-exempt request never consults the
    /// grants: consuming a one-time paste password on a listing would
    /// burn the grant before the follow-up data read.
    // fn kittyClipboardReadGranted(self, pw: []const u8, mimes_len: usize) bool {
    pub(super) fn kittyClipboardReadGranted(&mut self, pw: &[u8], mimes_len: usize) -> bool {
        if rustty_vt::kitty::clipboard::read_prompt_exempt(mimes_len) {
            return false;
        }
        self.kitty_clipboard_grants.use_(
            self.alloc.into(),
            pw,
            rustty_vt::kitty::clipboard::Direction::Read,
        )
    }

    /// Begin a Kitty clipboard write transaction (type=write).
    // fn kittyClipboardWriteBegin(self, meta, terminator)
    //     error{ OutOfMemory, WriteFailed }!void {
    pub(super) fn kittyClipboardWriteBegin(
        &mut self,
        meta: &rustty_vt::kitty::clipboard::Metadata,
        terminator: osc::Terminator,
    ) -> Result<(), crate::Error> {
        use rustty_vt::kitty::clipboard as kitty_clipboard;

        // A new write silently replaces any in-flight transaction.
        self.kittyClipboardWriteAbort();

        // A write denied by policy can never succeed, so fail the
        // transaction up front instead of spooling data we'd only
        // throw away. Later wdata packets are ignored.
        if self.clipboard_write == ClipboardAccess::Deny {
            // log.info("application attempted to write clipboard, but 'clipboard-write' is set to deny", .{});
            log::info!(
                target: LOG_SCOPE,
                "application attempted to write clipboard, but 'clipboard-write' is set to deny"
            );
            self.kittyClipboardWriteStatus(kitty_clipboard::Status::Eperm, &meta.id, terminator)?;
            return Ok(());
        }

        // const state = try self.alloc.create(terminal.kitty.clipboard.WriteState);
        // errdefer self.alloc.destroy(state);
        // state.* = try .init(self.alloc, meta, .{ .max_size = self.clipboard_write_limit });
        let Ok(state) = kitty_clipboard::WriteState::init(
            self.alloc.into(),
            meta,
            kitty_clipboard::WriteOptions {
                max_size: self.clipboard_write_limit,
            },
        ) else {
            return Err(crate::Error::Other);
        };
        self.kitty_clipboard_write = Some(Box::new(state));
        Ok(())
    }

    /// Accumulate one wdata chunk, or commit the transaction when the
    /// chunk carries no MIME type.
    // fn kittyClipboardWriteData(self, meta, payload, terminator)
    //     error{ OutOfMemory, WriteFailed }!void {
    fn kittyClipboardWriteData(
        &mut self,
        meta: &rustty_vt::kitty::clipboard::Metadata,
        payload: &[u8],
        terminator: osc::Terminator,
    ) -> Result<(), crate::Error> {
        use rustty_vt::kitty::clipboard as kitty_clipboard;

        // Data without a transaction is silently ignored, matching
        // kitty.
        // const state = self.kitty_clipboard_write orelse return;
        // (the Box is taken out while operated on and put back on success —
        // see the vt stream_terminal precedent.)
        let Some(mut state) = self.kitty_clipboard_write.take() else {
            return Ok(());
        };

        // A wdata packet without a MIME type commits the transaction.
        if meta.mime.is_empty() {
            return self.kittyClipboardWriteCommit(state, terminator);
        }

        match state.data(self.alloc.into(), meta, payload) {
            Ok(()) => {
                self.kitty_clipboard_write = Some(state);
                Ok(())
            }

            // Failing to spool matches kitty's EIO for a failed buffer write.
            Err(kitty_clipboard::DataError::OutOfMemory) => {
                self.kittyClipboardWriteFinish(&state, kitty_clipboard::Status::Eio, terminator)
            }

            // Data over the write limit aborts the transaction and is
            // reported to the client.
            Err(kitty_clipboard::DataError::TooLarge) => {
                self.kittyClipboardWriteFinish(&state, kitty_clipboard::Status::Efbig, terminator)
            }

            // An invalid base64 payload stream aborts the transaction.
            Err(kitty_clipboard::DataError::Invalid) => {
                self.kittyClipboardWriteFinish(&state, kitty_clipboard::Status::Einval, terminator)
            }
        }
    }

    /// Register aliases from a walias packet.
    // fn kittyClipboardWriteAlias(self, meta, payload, terminator)
    //     error{ OutOfMemory, WriteFailed }!void {
    fn kittyClipboardWriteAlias(
        &mut self,
        meta: &rustty_vt::kitty::clipboard::Metadata,
        payload: &[u8],
        terminator: osc::Terminator,
    ) -> Result<(), crate::Error> {
        use rustty_vt::kitty::clipboard as kitty_clipboard;

        // Aliases without a transaction are silently ignored. Once a
        // transaction exists, a missing target MIME type is invalid and
        // aborts the transaction.
        let Some(mut state) = self.kitty_clipboard_write.take() else {
            return Ok(());
        };
        if meta.mime.is_empty() {
            return self.kittyClipboardWriteFinish(
                &state,
                kitty_clipboard::Status::Einval,
                terminator,
            );
        }

        match state.alias(self.alloc.into(), meta, payload) {
            Ok(()) => {
                self.kitty_clipboard_write = Some(state);
                Ok(())
            }

            Err(kitty_clipboard::AliasError::OutOfMemory) => {
                self.kittyClipboardWriteFinish(&state, kitty_clipboard::Status::Eio, terminator)
            }

            // An undecodable alias payload aborts the transaction.
            Err(kitty_clipboard::AliasError::Invalid) => {
                self.kittyClipboardWriteFinish(&state, kitty_clipboard::Status::Einval, terminator)
            }
        }
    }

    /// Commit the transaction: resolve the final contents and forward
    /// them to the surface thread, which owns policy, any permission
    /// prompt, the clipboard write itself, and the final reply.
    // fn kittyClipboardWriteCommit(self, state, terminator)
    //     error{ OutOfMemory, WriteFailed }!void {
    fn kittyClipboardWriteCommit(
        &mut self,
        mut state: Box<rustty_vt::kitty::clipboard::WriteState>,
        terminator: osc::Terminator,
    ) -> Result<(), crate::Error> {
        use rustty_vt::kitty::clipboard as kitty_clipboard;

        match self.kittyClipboardWriteSend(&mut state, terminator) {
            Ok(()) => {}
            // The last MIME type's payload stream was not correctly
            // padded, which aborts the transaction.
            // (error.OutOfMemory: the reference EIO-finishes then re-raises;
            // infallible here, carried for parity.)
            Err(kitty_clipboard::CommitError::OutOfMemory) => {
                return self.kittyClipboardWriteFinish(
                    &state,
                    kitty_clipboard::Status::Eio,
                    terminator,
                );
            }
            Err(kitty_clipboard::CommitError::Invalid) => {
                return self.kittyClipboardWriteFinish(
                    &state,
                    kitty_clipboard::Status::Einval,
                    terminator,
                );
            }
        }

        // The transaction is complete; the surface owns the reply.
        // self.kittyClipboardWriteAbort();
        // (the taken-out Box drops here.)
        drop(state);
        Ok(())
    }

    // fn kittyClipboardWriteSend(self, state, terminator)
    //     error{ OutOfMemory, Invalid }!void {
    fn kittyClipboardWriteSend(
        &mut self,
        state: &mut rustty_vt::kitty::clipboard::WriteState,
        terminator: osc::Terminator,
    ) -> Result<(), rustty_vt::kitty::clipboard::CommitError> {
        // const committed = try state.commit(self.alloc);
        // defer committed.deinit(self.alloc);
        //
        // PORT NOTE: `commit` borrows the state, so the grant lookup happens
        // before and the request is built from owned copies while `committed`
        // is live (`granted` is computed from `pw`, which needs `committed`
        // first — so it is read inside the borrow via a split: name/pw copies
        // are taken from the borrowed committed view).
        let (pw_owned, location, contents, id, name): (
            Vec<u8>,
            rustty_apprt_api::Clipboard,
            Vec<rustty_apprt_api::ClipboardContentOwned>,
            Vec<u8>,
            Vec<u8>,
        ) = {
            let committed = state.commit(self.alloc.into())?;
            let pw: &[u8] = if !committed.name.is_empty() {
                committed.pw
            } else {
                b""
            };
            (
                pw.to_vec(),
                match committed.loc {
                    rustty_vt::clipboard::Location::Primary => rustty_apprt_api::Clipboard::Primary,
                    _ => rustty_apprt_api::Clipboard::Standard,
                },
                committed
                    .contents
                    .iter()
                    .map(|c| rustty_apprt_api::ClipboardContentOwned {
                        mime: c.mime.to_vec(),
                        data: c.data.to_vec(),
                    })
                    .collect(),
                committed.id.to_vec(),
                committed.name.to_vec(),
            )
        };

        // Per the spec a password without a name is no password. A
        // stored session grant for it lets the surface skip its
        // permission prompt.
        // const granted = self.kitty_clipboard_grants.use(self.alloc, pw, .write);
        let granted = self.kitty_clipboard_grants.use_(
            self.alloc.into(),
            &pw_owned,
            rustty_vt::kitty::clipboard::Direction::Write,
        );

        // Everything about the request, including the request struct itself,
        // lives in a single arena that crosses to the surface thread ...
        // req.* = .{
        //     // The arena must be copied in last so it tracks every
        //     // allocation above.
        //     .arena = arena,
        //     ... };
        // (PORT NOTE: owned Box — see apprt::KittyRead; no copy-order hazard.)
        let req = Box::new(rustty_apprt_api::KittyWrite {
            location,
            contents,
            id,
            pw: pw_owned,
            name,
            granted,
            terminator,
        });

        // self.surfaceMessageWriter(.{ .kitty_clipboard_write = req });
        self.surfaceMessageWriter(SurfaceMessage::KittyClipboardWrite(req));
        Ok(())
    }

    /// Answer the write transaction with its final status and drop it.
    /// The id echoed is the one from the transaction's opening write
    /// packet, matching kitty.
    // fn kittyClipboardWriteFinish(self, state: *const WriteState, status,
    //     terminator) error{ OutOfMemory, WriteFailed }!void {
    //     defer self.kittyClipboardWriteAbort();
    //     try self.kittyClipboardWriteStatus(status, state.id, terminator);
    // }
    // (callers hand the taken-out Box by reference; its drop at the caller is
    // the abort — any STORED transaction was already taken.)
    pub(super) fn kittyClipboardWriteFinish(
        &mut self,
        state: &rustty_vt::kitty::clipboard::WriteState,
        status: rustty_vt::kitty::clipboard::Status,
        terminator: osc::Terminator,
    ) -> Result<(), crate::Error> {
        self.kittyClipboardWriteAbort();
        self.kittyClipboardWriteStatus(status, &state.id, terminator)
    }

    /// Reply to a write transaction with a single status packet.
    // fn kittyClipboardWriteStatus(self, status, id, terminator)
    //     error{ OutOfMemory, WriteFailed }!void {
    fn kittyClipboardWriteStatus(
        &mut self,
        status: rustty_vt::kitty::clipboard::Status,
        id: &[u8],
        terminator: osc::Terminator,
    ) -> Result<(), crate::Error> {
        // var stream: std.Io.Writer.Allocating = .init(self.alloc);
        // defer stream.deinit();
        let mut stream = rustty_vt::snapshot::io::AllocatingWriter::new();
        (rustty_vt::kitty::clipboard::Response {
            id,
            terminator,
            ..rustty_vt::kitty::clipboard::Response::new(
                rustty_vt::osc::parsers::kitty_clipboard_protocol::Operation::Write,
                status,
            )
        })
        .encode(&mut stream)
        .map_err(|_| crate::Error)?;
        // self.messageWriter(.{ .write_alloc = .{
        //     .alloc = self.alloc,
        //     .data = try stream.toOwnedSlice(),
        // } });
        self.messageWriter(Message::WriteAlloc(
            rustty_datastruct::message_data::Alloc {
                data: stream.written().to_vec().into_boxed_slice(),
            },
        ));
        Ok(())
    }

    /// Drop any in-flight write transaction without responding.
    // fn kittyClipboardWriteAbort(self: *StreamHandler) void {
    //     if (self.kitty_clipboard_write) |state| {
    //         state.deinit(self.alloc);
    //         self.alloc.destroy(state);
    //         self.kitty_clipboard_write = null;
    //     }
    // }
    pub(super) fn kittyClipboardWriteAbort(&mut self) {
        // (dropping the Box is the deinit + destroy.)
        self.kitty_clipboard_write = None;
    }
}
