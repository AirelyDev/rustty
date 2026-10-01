//! PORTS: ghostty src/Surface.zig — the clipboard and tail cluster:
//! `completeClipboardRequest`, `startClipboardRequest`, `completeClipboardPaste`,
//! `completeClipboardReadOSC52`, the kitty clipboard protocol handlers,
//! `showDesktopNotification`, `crashThreadState`, `presentSurface`, `getProcessInfo`.

#![allow(non_snake_case)]

use super::*;

impl<A> Surface<A>
where
    A: apprt::App + 'static,
    // C0 DECISION (CORE_BLUEPRINT §1, option 1 — the app.rs impl-block
    // precedent): the self-referential associated-type bound; Surface's
    // method tier needs it to call into `App<A>`'s constrained methods
    // (maybeHandleBinding's `performAllChainedAction`, C10).
    A::Surface: ApprtSurface<App = A, Core = Surface<A>>,
{
    // (the module-scope `CompleteClipboard` payload + `clipboardTextContent`
    // helper this consumes are declared after this impl block — Rust cannot
    // nest module items inside impls.)
    /// Call this to complete a clipboard request sent to apprt. This should
    /// only be called once for each request.
    ///
    /// If `complete.confirmed` is false then this may return either an
    /// UnsafePaste or UnauthorizedPaste error, depending on the type of
    /// clipboard request. The request state remains alive in that case so
    /// the apprt can run its confirmation flow.
    // pub fn completeClipboardRequest(
    //     self: *Surface,
    //     req: apprt.ClipboardRequest,
    //     complete: CompleteClipboard,
    // ) !void {
    pub fn completeClipboardRequest(
        &mut self,
        req: apprt::ClipboardRequest,
        complete: CompleteClipboard<'_>,
    ) -> Result<(), Error> {
        // switch (req) {
        match req {
            // .paste => try self.completeClipboardPaste(
            //     clipboardTextContent(complete.contents) orelse "",
            //     complete.confirmed,
            // ),
            apprt::ClipboardRequest::Paste(_) => self.completeClipboardPaste(
                clipboardTextContent(complete.contents).unwrap_or(b""),
                complete.confirmed,
            )?,

            // .list => |clipboard| if (!try self.completeClipboardPasteEvent(
            //     clipboard,
            //     complete.available,
            // )) {
            //     log.debug("mode 5522 paste event was not written", .{});
            // },
            apprt::ClipboardRequest::List(clipboard) => {
                if !self.completeClipboardPasteEvent(clipboard, complete.available)? {
                    // log.debug("mode 5522 paste event was not written", .{});
                    log::debug!(target: LOG_SCOPE, "mode 5522 paste event was not written");
                }
            }

            // .osc_52_read => |clipboard| try self.completeClipboardReadOSC52(
            //     clipboardTextContent(complete.contents) orelse "",
            //     clipboard,
            //     complete.confirmed,
            // ),
            apprt::ClipboardRequest::Osc52Read(clipboard) => self.completeClipboardReadOSC52(
                clipboardTextContent(complete.contents).unwrap_or(b""),
                clipboard,
                complete.confirmed,
            )?,

            // .osc_52_write => |clipboard| { ... },
            apprt::ClipboardRequest::Osc52Write(clipboard) => {
                // The write API wants sentinel-terminated data; the write
                // text round-tripped through the apprt confirmation flow as
                // a plain representation.
                // const data = try self.alloc.dupeZ(u8, clipboardTextContent(...) orelse "");
                // defer self.alloc.free(data);
                //
                // (the `dupeZ` NUL-termination is the C-boundary detail — the
                // Rust apprt seam carries `&[u8]`, re-terminated at the C
                // boundary by the embedded backend; the dupe collapses to the
                // borrow.)
                let data = clipboardTextContent(complete.contents).unwrap_or(b"");
                unsafe { &*self.rt_surface }.set_clipboard(
                    clipboard,
                    &[apprt::ClipboardContent {
                        mime: b"text/plain",
                        data,
                    }],
                    !complete.confirmed,
                )?;
            }

            // .kitty_read => |kitty| { ... },
            apprt::ClipboardRequest::KittyRead(kitty) => {
                // If we need confirmation we return an error without
                // consuming the request state; the apprt keeps it alive
                // for the confirmation flow. A session grant carried by
                // the request skips the prompt, and a request with no
                // data types is exempt from prompting entirely; see
                // readPromptExempt.
                //
                // SAFETY: `kitty` is the leaked `Box` (see `KittyRead::destroy`);
                // it stays valid until the destroy below, and no other alias
                // exists (single-owner contract).
                let kitty_ref: &apprt::KittyRead = unsafe { &*kitty };
                if self.config.clipboard_read == configpkg::value_types::ClipboardAccess::Ask
                    && !complete.confirmed
                    && !kitty_ref.granted
                    && !rustty_vt::kitty::clipboard::read_prompt_exempt(kitty_ref.mimes.len())
                {
                    return Err(Error::UnauthorizedPaste);
                }

                // Past the confirmation check the request is consumed:
                // every path from here, including errors, must destroy it.
                // defer kitty.destroy();
                //
                // (Rust: a scope guard would fight the borrows below; the
                // handler's exits are enumerable — the single `?` propagation
                // plus the fall-through — so the destroy is called explicitly
                // on each, LIFO-faithful.)

                // Record a session grant when the user asked to remember
                // their decision and the request carried a usable
                // password. The grants live with the terminal state on
                // the IO thread.
                if complete.remember && !kitty_ref.pw.is_empty() {
                    // const pw = try self.alloc.dupe(u8, kitty.pw);
                    let pw = kitty_ref.pw.clone();
                    // self.queueIo(.{ .kitty_clipboard_grant_read = .{ .alloc, .pw } }, .unlocked);
                    self.queueIo(
                        termio::Message::KittyClipboardGrantRead(
                            termio::message::KittyClipboardGrant { pw },
                        ),
                        termio::termio_impl::MutexState::Unlocked,
                    );
                }

                let result = self.completeKittyClipboardRead(
                    kitty_ref,
                    complete.contents,
                    complete.available,
                );
                // (the deferred destroy — every consumed path.)
                unsafe { apprt::KittyRead::destroy(kitty) };
                result?;
            }

            // .kitty_write => |kitty| { ... },
            apprt::ClipboardRequest::KittyWrite(kitty) => {
                // If we need confirmation we return an error without
                // consuming the request state; the apprt keeps it alive
                // for the confirmation flow. A session grant carried by
                // the request skips the prompt.
                //
                // SAFETY: `kitty` is the leaked `Box` (see `KittyWrite::destroy`).
                let kitty_ref: &apprt::KittyWrite = unsafe { &*kitty };
                if self.config.clipboard_write == configpkg::value_types::ClipboardAccess::Ask
                    && !complete.confirmed
                    && !kitty_ref.granted
                {
                    return Err(Error::UnauthorizedPaste);
                }

                // Past the confirmation check the request is consumed:
                // every path from here, including errors, must destroy it.
                // defer kitty.destroy();

                // Record a session grant when the user asked to remember
                // their decision and the request carried a usable
                // password. The grants live with the terminal state on
                // the IO thread.
                if complete.remember && !kitty_ref.pw.is_empty() {
                    let pw = kitty_ref.pw.clone();
                    self.queueIo(
                        termio::Message::KittyClipboardGrantWrite(
                            termio::message::KittyClipboardGrant { pw },
                        ),
                        termio::termio_impl::MutexState::Unlocked,
                    );
                }

                // Apply the committed representations carried by the
                // request itself; any contents echoed back by the apprt
                // are only what its confirmation prompt displayed. An
                // empty commit clears the clipboard, which the apprt
                // write API expresses as a single empty text entry.
                //
                // (the borrowed `ClipboardContent` views over the owned
                // `ClipboardContentOwned` payload — the KittyWrite PORT NOTE.)
                let borrowed: Vec<apprt::ClipboardContent<'_>> = kitty_ref
                    .contents
                    .iter()
                    .map(|c| apprt::ClipboardContent {
                        mime: &c.mime,
                        data: &c.data,
                    })
                    .collect();
                let set_result = unsafe { &*self.rt_surface }.set_clipboard(
                    kitty_ref.location,
                    if !borrowed.is_empty() {
                        &borrowed
                    } else {
                        &[apprt::ClipboardContent {
                            mime: b"text/plain",
                            data: b"",
                        }]
                    },
                    false,
                );
                if let Err(err) = &set_result {
                    // log.err("error setting clipboard err={}", .{err});
                    log::error!(target: LOG_SCOPE, "error setting clipboard err={err:?}");
                    let result = self.kittyClipboardStatus(
                        kitty_clipboard::Operation::Write,
                        &kitty_ref.id.clone(),
                        kitty_ref.terminator,
                        kitty_clipboard::Status::Eio,
                    );
                    unsafe { apprt::KittyWrite::destroy(kitty) };
                    result?;
                    return Ok(());
                }

                let result = self.kittyClipboardStatus(
                    kitty_clipboard::Operation::Write,
                    &kitty_ref.id.clone(),
                    kitty_ref.terminator,
                    kitty_clipboard::Status::Done,
                );
                unsafe { apprt::KittyWrite::destroy(kitty) };
                result?;
            }
        }
        Ok(())
    }

    /// Deny an in-flight clipboard request. This consumes the request: for
    /// request types whose protocol expects an answer, the denial reply is
    /// written to the pty.
    // pub fn denyClipboardRequest(self: *Surface, req: apprt.ClipboardRequest) void {
    pub fn denyClipboardRequest(&mut self, req: apprt::ClipboardRequest) {
        // switch (req) {
        match req {
            // A denied paste simply doesn't happen.
            // .paste, .list => {},
            apprt::ClipboardRequest::Paste(_) | apprt::ClipboardRequest::List(_) => {}

            // OSC 52 has no error responses, but the client is waiting on
            // a reply, so a denied read is answered with empty contents.
            // .osc_52_read => |clipboard| self.completeClipboardReadOSC52("", clipboard, true)
            //     catch |err| { log.warn(...); },
            apprt::ClipboardRequest::Osc52Read(clipboard) => {
                if let Err(err) = self.completeClipboardReadOSC52(b"", clipboard, true) {
                    // log.warn("error replying to OSC 52 clipboard read err={}", .{err});
                    log::warn!(
                        target: LOG_SCOPE,
                        "error replying to OSC 52 clipboard read err={err:?}"
                    );
                }
            }

            // A denied write simply doesn't happen.
            // .osc_52_write => {},
            apprt::ClipboardRequest::Osc52Write(_) => {}

            // The Kitty clipboard protocol reports denial explicitly.
            // .kitty_read => |kitty| { defer kitty.destroy(); ... },
            apprt::ClipboardRequest::KittyRead(kitty) => {
                // SAFETY: single-owner leaked Box; destroyed exactly once below.
                let kitty_ref: &apprt::KittyRead = unsafe { &*kitty };
                if let Err(err) = self.kittyClipboardStatus(
                    kitty_clipboard::Operation::Read,
                    &kitty_ref.id.clone(),
                    kitty_ref.terminator,
                    kitty_clipboard::Status::Eperm,
                ) {
                    // log.warn("error replying to kitty clipboard read err={}", .{err});
                    log::warn!(
                        target: LOG_SCOPE,
                        "error replying to kitty clipboard read err={err:?}"
                    );
                }
                unsafe { apprt::KittyRead::destroy(kitty) };
            }

            // .kitty_write => |kitty| { defer kitty.destroy(); ... },
            apprt::ClipboardRequest::KittyWrite(kitty) => {
                let kitty_ref: &apprt::KittyWrite = unsafe { &*kitty };
                if let Err(err) = self.kittyClipboardStatus(
                    kitty_clipboard::Operation::Write,
                    &kitty_ref.id.clone(),
                    kitty_ref.terminator,
                    kitty_clipboard::Status::Eperm,
                ) {
                    // log.warn("error replying to kitty clipboard write err={}", .{err});
                    log::warn!(
                        target: LOG_SCOPE,
                        "error replying to kitty clipboard write err={err:?}"
                    );
                }
                unsafe { apprt::KittyWrite::destroy(kitty) };
            }
        }
    }

    /// This starts a clipboard request, with some basic validation. For example,
    /// an OSC 52 request is not actually requested if OSC 52 is disabled.
    ///
    /// The result reports whether the request was started; requests that
    /// weren't started never complete. Callers own reacting to that, e.g.
    /// performable paste keybinds pass through and Kitty clipboard reads
    /// answer the program.
    //
    // fn startClipboardRequest(
    //     self: *Surface,
    //     loc: apprt.Clipboard,
    //     req: apprt.ClipboardRequest,
    // ) !apprt.ClipboardReadResult {
    pub(super) fn startClipboardRequest(
        &mut self,
        loc: apprt::Clipboard,
        req: apprt::ClipboardRequest,
    ) -> Result<apprt::ClipboardReadResult, Error> {
        // const effective_req: apprt.ClipboardRequest = switch (req) {
        let effective_req: apprt::ClipboardRequest = match req {
            // .paste => |clipboard| effective: { ... },
            apprt::ClipboardRequest::Paste(clipboard) => {
                // Snapshot the mode before asking the apprt for clipboard data.
                // Event pastes request only a MIME listing, while ordinary
                // pastes request the text representation as before.
                // self.renderer_state.mutex.lock();
                // const event = self.io.terminal.modes.get(.kitty_paste_events);
                // self.renderer_state.mutex.unlock();
                unsafe { (*(*self.renderer_state.as_ptr()).mutex).lock() };
                let event = unsafe { &(*self.io.as_ptr()).terminal }
                    .modes
                    .get(rustty_vt::modes::Mode::KittyPasteEvents);
                unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };

                // break :effective if (event) .{ .list = clipboard } else req;
                if event {
                    apprt::ClipboardRequest::List(clipboard)
                } else {
                    req
                }
            }
            _ => req,
        };

        // switch (effective_req) {
        match effective_req {
            // .paste, .list => {}, // always allowed
            apprt::ClipboardRequest::Paste(_) | apprt::ClipboardRequest::List(_) => {}
            // .osc_52_read => if (self.config.clipboard_read == .deny) {
            apprt::ClipboardRequest::Osc52Read(_) => {
                if self.config.clipboard_read == configpkg::value_types::ClipboardAccess::Deny {
                    // log.info(
                    //     "application attempted to read clipboard, but 'clipboard-read' is set to deny",
                    //     .{},
                    // );
                    log::info!(
                        target: LOG_SCOPE,
                        "application attempted to read clipboard, but 'clipboard-read' is set to deny"
                    );
                    // return .unsupported;
                    return Ok(apprt::ClipboardReadResult::Unsupported);
                }
            }

            // The clipboard access policies were already applied by
            // kittyClipboardRead and kittyClipboardWrite, which own
            // replying on denial.
            // .kitty_read, .kitty_write => {},
            apprt::ClipboardRequest::KittyRead(_) | apprt::ClipboardRequest::KittyWrite(_) => {}

            // OSC 52 writes don't travel through this function; they go
            // straight to the apprt setClipboard API.
            // .osc_52_write => unreachable,
            apprt::ClipboardRequest::Osc52Write(_) => unreachable!(),
        }

        // return try self.rt_surface.clipboardRequest(loc, effective_req);
        Ok(unsafe { &*self.rt_surface }.clipboard_request(loc, effective_req)?)
    }

    /// Send a Kitty clipboard-protocol paste event when mode 5522 is enabled.
    /// The event only lists the available MIME types; it does not read any of
    /// their data. The shared terminal paste implementation generates and records
    /// the one-time password used by the program's follow-up OSC 5522 read.
    //
    // fn completeClipboardPasteEvent(
    //     self: *Surface,
    //     clipboard: apprt.Clipboard,
    //     available: []const []const u8,
    // ) !bool {
    pub(super) fn completeClipboardPasteEvent(
        &mut self,
        clipboard: apprt::Clipboard,
        available: &[&[u8]],
    ) -> Result<bool, Error> {
        // if (self.readonly) return false;
        if self.readonly {
            return Ok(false);
        }

        // const kitty_clipboard = terminal.kitty.clipboard;
        // const location: terminal.clipboard.Location = switch (clipboard) { ... };
        let location: terminal::clipboard::Location = match clipboard {
            apprt::Clipboard::Standard => terminal::clipboard::Location::Standard,
            apprt::Clipboard::Selection => terminal::clipboard::Location::Selection,
            apprt::Clipboard::Primary => terminal::clipboard::Location::Primary,
        };

        // The protocol implementation caps listings at this size too. Cap here
        // so the temporary Content array stays on the stack.
        // var contents_buf: [kitty_clipboard.max_listing_mimes]terminal.clipboard.Content = undefined;
        // const contents_len = @min(available.len, contents_buf.len);
        let mut contents_buf: [terminal::clipboard::Content<'_>;
            kitty_clipboard::max_listing_mimes] = [terminal::clipboard::Content {
            mime: b"",
            data: b"",
        }; kitty_clipboard::max_listing_mimes];
        let contents_len = available.len().min(contents_buf.len());
        // for (available[0..contents_len], contents_buf[0..contents_len]) |mime, *content| {
        //     content.* = .{ .mime = mime, .data = "" };
        // }
        for (mime, content) in available[..contents_len]
            .iter()
            .zip(contents_buf[..contents_len].iter_mut())
        {
            *content = terminal::clipboard::Content { mime, data: b"" };
        }

        // var aw: std.Io.Writer.Allocating = .init(self.alloc);
        // defer aw.deinit();
        let mut aw = rustty_vt::snapshot::io::AllocatingWriter::new();

        // self.renderer_state.mutex.lock();
        // defer self.renderer_state.mutex.unlock();
        //
        // (guard-less zigstd Mutex — the defer is the explicit unlock at
        // every exit below.)
        unsafe { (*(*self.renderer_state.as_ptr()).mutex).lock() };

        // const pasted = try terminal.paste.paste(.{
        //     .terminal = &self.io.terminal,
        //     .alloc = self.alloc,
        //     .writer = &aw.writer,
        //     .kitty_clipboard = .{
        //         .grants = &self.io.terminal_stream.handler.kitty_clipboard_grants,
        //         .io = global.io(),
        //     },
        // }, .{ ... });
        //
        // SAFETY (grants aliasing): the grants live in the IO thread's stream
        // handler; every access — the handler's own and this one — happens
        // under the renderer/terminal mutex held above (the KittyClipboardGrant
        // mailbox-arm precedent), so the `&mut` cannot interleave with an IO
        // thread access.
        //
        // SAFETY (N454: `paste` takes raw `terminal`/`grants` pointers): both
        // are live fields of the IO state for the call, reached under the
        // mutex, and no reference to either is live across it; the contents
        // are in memory, so no reader runs.
        let io = unsafe { (*self.io.as_ptr()).terminal.io };
        let pasted = match unsafe {
            rustty_vt::paste::paste(
                rustty_vt::paste::Context {
                    terminal: std::ptr::addr_of!((*self.io.as_ptr()).terminal),
                    // .alloc = self.alloc: the application's allocator, which is
                    // the terminal's (the Rust heap; the grants are freed with it).
                    alloc: rustty_vt::alloc::Global,
                    writer: &mut aw,
                    kitty_clipboard: Some(rustty_vt::paste::KittyClipboard {
                        grants: std::ptr::addr_of_mut!(
                            (*self.io.as_ptr())
                                .terminal_stream
                                .handler
                                .kitty_clipboard_grants
                        ),
                        io,
                    }),
                },
                rustty_vt::paste::Request {
                    // .source = .{ .clipboard = location },
                    source: rustty_vt::paste::Source::Clipboard(location),
                    // .contents = .{ .memory = contents_buf[0..contents_len] },
                    contents: rustty_vt::paste::Contents::Memory(&contents_buf[..contents_len]),
                    // A paste event discloses no clipboard data, so unsafe-text
                    // confirmation does not apply. If mode 5522 is reset, the empty
                    // stand-in representations cause the shared helper to write nothing.
                    // .allow_unsafe = true,
                    allow_unsafe: true,
                },
            )
        } {
            Ok(v) => v,
            Err(err) => {
                unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
                // (the reference propagates the inferred paste error union;
                // the port folds it into the surface `Error` set:
                // `UnsafePaste` keeps its distinguishable variant, the
                // writer/reader arms are unreachable here — the Allocating
                // writer and in-memory contents never fail — and the
                // allocation/entropy arms surface as the vt OutOfMemory
                // marker, the crate's Zig-`Allocator.Error` convention.)
                return Err(match err {
                    rustty_vt::paste::Error::UnsafePaste => Error::UnsafePaste,
                    rustty_vt::paste::Error::WriteFailed | rustty_vt::paste::Error::ReadFailed => {
                        unreachable!()
                    }
                    _ => Error::OutOfMemory(terminal::pagelist::OutOfMemory),
                });
            }
        };
        // if (!pasted) return false;
        if !pasted {
            unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
            return Ok(false);
        }

        // self.queueIo(.{ .write_alloc = .{
        //     .alloc = self.alloc,
        //     .data = try aw.toOwnedSlice(),
        // } }, .locked);
        // return true;
        //
        // PORT NOTE (parity S42-F1): the `defer ... unlock` above runs at
        // function exit, so the reference queues with the renderer mutex STILL
        // HELD, which is what `.locked` tells the mailbox: on a full queue it
        // unlocks that mutex while it waits and relocks it after. CORRECTION:
        // the port used to unlock before this call, so the full-queue path
        // unlocked a mutex nobody held and returned with it locked.
        self.queueIo(
            termio::Message::WriteAlloc(Alloc {
                data: aw.written().to_vec().into_boxed_slice(),
            }),
            termio::termio_impl::MutexState::Locked,
        );
        unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
        Ok(true)
    }

    /// Handle a Kitty clipboard protocol (OSC 5522) read request forwarded
    /// by the IO thread. This takes ownership of the request state.
    //
    // fn kittyClipboardRead(
    //     self: *Surface,
    //     req: *apprt.ClipboardRequest.KittyRead,
    // ) !void {
    #[allow(non_snake_case)]
    pub(super) fn kittyClipboardRead(&mut self, req: *mut apprt::KittyRead) -> Result<(), Error> {
        // SAFETY: `req` is the leaked `Box` from the mailbox arm (single
        // owner); it stays valid until a destroy on one of the consumed paths.
        let req_ref: &apprt::KittyRead = unsafe { &*req };

        // A read denied by policy answers EPERM so clients degrade
        // gracefully instead of waiting on a response that never comes.
        // if (self.config.clipboard_read == .deny) {
        if self.config.clipboard_read == configpkg::value_types::ClipboardAccess::Deny {
            // defer req.destroy();
            // log.info("application attempted to read clipboard, but 'clipboard-read' is set to deny", .{});
            log::info!(
                target: LOG_SCOPE,
                "application attempted to read clipboard, but 'clipboard-read' is set to deny"
            );
            // try self.kittyClipboardStatus(.read, req.id, req.terminator, .EPERM);
            // return;
            let result = self.kittyClipboardStatus(
                kitty_clipboard::Operation::Read,
                &req_ref.id.clone(),
                req_ref.terminator,
                kitty_clipboard::Status::Eperm,
            );
            unsafe { apprt::KittyRead::destroy(req) };
            result?;
            return Ok(());
        }

        // const result = self.startClipboardRequest(
        //     req.location,
        //     .{ .kitty_read = req },
        // ) catch |err| {
        //     defer req.destroy();
        //     self.kittyClipboardStatus(.read, req.id, req.terminator, .EIO) catch {};
        //     return err;
        // };
        let result = match self
            .startClipboardRequest(req_ref.location, apprt::ClipboardRequest::KittyRead(req))
        {
            Ok(v) => v,
            Err(err) => {
                let _ = self.kittyClipboardStatus(
                    kitty_clipboard::Operation::Read,
                    &req_ref.id.clone(),
                    req_ref.terminator,
                    kitty_clipboard::Status::Eio,
                );
                unsafe { apprt::KittyRead::destroy(req) };
                return Err(err);
            }
        };

        // switch (result) {
        match result {
            // The request completes asynchronously.
            // .started => {},
            apprt::ClipboardReadResult::Started => {}

            // The clipboard has nothing we can serve, which is a
            // successful read that serves no representations. This never
            // prompts even under an ask policy since there are no contents
            // to disclose.
            // .unavailable => { defer req.destroy(); try self.completeKittyClipboardRead(req, &.{}, &.{}); },
            apprt::ClipboardReadResult::Unavailable => {
                let result = self.completeKittyClipboardRead(req_ref, &[], &[]);
                unsafe { apprt::KittyRead::destroy(req) };
                result?;
            }

            // The apprt can't serve this clipboard at all, e.g. an
            // unsupported primary selection.
            // .unsupported => { defer req.destroy(); try self.kittyClipboardStatus(.read, req.id, req.terminator, .ENOSYS); },
            apprt::ClipboardReadResult::Unsupported => {
                let result = self.kittyClipboardStatus(
                    kitty_clipboard::Operation::Read,
                    &req_ref.id.clone(),
                    req_ref.terminator,
                    kitty_clipboard::Status::Enosys,
                );
                unsafe { apprt::KittyRead::destroy(req) };
                result?;
            }
        }
        Ok(())
    }

    /// Handle a committed Kitty clipboard protocol (OSC 5522) write
    /// transaction forwarded by the IO thread. This takes ownership of the
    /// request state.
    //
    // fn kittyClipboardWrite(
    //     self: *Surface,
    //     req: *apprt.ClipboardRequest.KittyWrite,
    // ) !void {
    #[allow(non_snake_case)]
    pub(super) fn kittyClipboardWrite(&mut self, req: *mut apprt::KittyWrite) -> Result<(), Error> {
        // SAFETY: `req` is the leaked `Box` from the mailbox arm (single
        // owner); it stays valid until a destroy on one of the consumed paths.
        let req_ref: &apprt::KittyWrite = unsafe { &*req };

        // A write denied by policy answers EPERM so clients degrade
        // gracefully instead of waiting on a response that never comes.
        // The IO thread already fails transactions that begin under a
        // deny policy, but the policy may have changed mid-transaction.
        // if (self.config.clipboard_write == .deny) {
        if self.config.clipboard_write == configpkg::value_types::ClipboardAccess::Deny {
            // defer req.destroy();
            // log.info("application attempted to write clipboard, but 'clipboard-write' is set to deny", .{});
            log::info!(
                target: LOG_SCOPE,
                "application attempted to write clipboard, but 'clipboard-write' is set to deny"
            );
            // try self.kittyClipboardStatus(.write, req.id, req.terminator, .EPERM);
            // return;
            let result = self.kittyClipboardStatus(
                kitty_clipboard::Operation::Write,
                &req_ref.id.clone(),
                req_ref.terminator,
                kitty_clipboard::Status::Eperm,
            );
            unsafe { apprt::KittyWrite::destroy(req) };
            result?;
            return Ok(());
        }

        // const result = self.startClipboardRequest(
        //     req.location,
        //     .{ .kitty_write = req },
        // ) catch |err| {
        //     defer req.destroy();
        //     self.kittyClipboardStatus(.write, req.id, req.terminator, .EIO) catch {};
        //     return err;
        // };
        let result = match self
            .startClipboardRequest(req_ref.location, apprt::ClipboardRequest::KittyWrite(req))
        {
            Ok(v) => v,
            Err(err) => {
                let _ = self.kittyClipboardStatus(
                    kitty_clipboard::Operation::Write,
                    &req_ref.id.clone(),
                    req_ref.terminator,
                    kitty_clipboard::Status::Eio,
                );
                unsafe { apprt::KittyWrite::destroy(req) };
                return Err(err);
            }
        };

        // switch (result) {
        match result {
            // The request completes asynchronously.
            // .started => {},
            apprt::ClipboardReadResult::Started => {}

            // The apprt can't write this clipboard at all, e.g. an
            // unsupported primary selection. Writes carry their own
            // contents so there is no meaningful unavailable state; treat
            // it the same.
            // .unavailable, .unsupported => { defer req.destroy();
            //     try self.kittyClipboardStatus(.write, req.id, req.terminator, .ENOSYS); },
            apprt::ClipboardReadResult::Unavailable | apprt::ClipboardReadResult::Unsupported => {
                let result = self.kittyClipboardStatus(
                    kitty_clipboard::Operation::Write,
                    &req_ref.id.clone(),
                    req_ref.terminator,
                    kitty_clipboard::Status::Enosys,
                );
                unsafe { apprt::KittyWrite::destroy(req) };
                result?;
            }
        }
        Ok(())
    }

    /// Reply to a Kitty clipboard request with a single status packet.
    //
    // fn kittyClipboardStatus(
    //     self: *Surface,
    //     op: terminal.kitty.clipboard.Operation,
    //     id: []const u8,
    //     terminator: terminal.osc.Terminator,
    //     status: terminal.kitty.clipboard.Status,
    // ) error{ OutOfMemory, WriteFailed }!void {
    #[allow(non_snake_case)]
    fn kittyClipboardStatus(
        &mut self,
        op: kitty_clipboard::Operation,
        id: &[u8],
        terminator: rustty_vt::osc::Terminator,
        status: kitty_clipboard::Status,
    ) -> Result<(), Error> {
        // var aw: std.Io.Writer.Allocating = .init(self.alloc);
        // defer aw.deinit();
        let mut aw = rustty_vt::snapshot::io::AllocatingWriter::new();
        // try (terminal.kitty.clipboard.Response{
        //     .op = op,
        //     .status = status,
        //     .id = id,
        //     .terminator = terminator,
        // }).encode(&aw.writer);
        kitty_clipboard::Response {
            id,
            terminator,
            ..kitty_clipboard::Response::new(op, status)
        }
        .encode(&mut aw)
        .expect("allocating writer never fails"); // error{WriteFailed} unreachable through the Allocating writer

        // self.queueIo(.{ .write_alloc = .{
        //     .alloc = self.alloc,
        //     .data = try aw.toOwnedSlice(),
        // } }, .unlocked);
        self.queueIo(
            termio::Message::WriteAlloc(Alloc {
                data: aw.written().to_vec().into_boxed_slice(),
            }),
            termio::termio_impl::MutexState::Unlocked,
        );
        Ok(())
    }

    /// Complete a Kitty clipboard protocol read with the clipboard
    /// contents.
    //
    // fn completeKittyClipboardRead(
    //     self: *Surface,
    //     req: *const apprt.ClipboardRequest.KittyRead,
    //     contents: []const terminal.clipboard.Content,
    //     available: []const []const u8,
    // ) !void {
    #[allow(non_snake_case)]
    fn completeKittyClipboardRead(
        &mut self,
        req: &apprt::KittyRead,
        contents: &[terminal::clipboard::Content<'_>],
        available: &[&[u8]],
    ) -> Result<(), Error> {
        // const kitty_clipboard = terminal.kitty.clipboard;

        // Serve the requested representations in request order under their
        // requested names. Text-like MIME aliases all match the canonical
        // text representation, since that is the only name the apprt
        // serves text under. Requested types without a representation are
        // simply never served, which is how the protocol communicates an
        // unavailable representation.
        // var contents_buf: [kitty_clipboard.max_read_mimes]terminal.clipboard.Content = undefined;
        // var contents_len: usize = 0;
        let mut contents_buf: [terminal::clipboard::Content<'_>; kitty_clipboard::max_read_mimes] =
            [terminal::clipboard::Content {
                mime: b"",
                data: b"",
            }; kitty_clipboard::max_read_mimes];
        let mut contents_len: usize = 0;
        // for (req.mimes) |mime| { ... }
        // (Rust: the Zig `continue` inside the labeled `data:` block needs an
        // explicit loop label.)
        'mimes: for mime in req.mimes.iter() {
            // const data: []const u8 = data: { ... continue; };
            let data: &[u8] = 'data: {
                for content in contents {
                    // if (std.mem.eql(u8, content.mime, mime)) break :data content.data;
                    if content.mime == &mime[..] {
                        break 'data content.data;
                    }
                    // if (terminal.clipboard.isTextMime(mime) and
                    //     terminal.clipboard.isTextMime(content.mime))
                    // { break :data content.data; }
                    if terminal::clipboard::is_text_mime(mime)
                        && terminal::clipboard::is_text_mime(content.mime)
                    {
                        break 'data content.data;
                    }
                }
                continue 'mimes;
            };

            contents_buf[contents_len] = terminal::clipboard::Content { mime, data };
            contents_len += 1;
        }

        // Encode the full success sequence: the OK packet, the targets
        // listing if it was requested, DATA chunks for each served
        // representation, and the final DONE packet.
        // var aw: std.Io.Writer.Allocating = .init(self.alloc);
        // defer aw.deinit();
        let mut aw = rustty_vt::snapshot::io::AllocatingWriter::new();
        // try (kitty_clipboard.ReadSuccess{ ... }).encode(&aw.writer);
        kitty_clipboard::ReadSuccess {
            primary: req.location == apprt::Clipboard::Primary,
            id: &req.id,
            list: req.list,
            available,
            contents: &contents_buf[..contents_len],
            terminator: req.terminator,
            ..kitty_clipboard::ReadSuccess::default()
        }
        .encode(&mut aw)
        .expect("allocating writer never fails"); // error{WriteFailed} unreachable through the Allocating writer

        // self.queueIo(.{ .write_alloc = .{
        //     .alloc = self.alloc,
        //     .data = try aw.toOwnedSlice(),
        // } }, .unlocked);
        self.queueIo(
            termio::Message::WriteAlloc(Alloc {
                data: aw.written().to_vec().into_boxed_slice(),
            }),
            termio::termio_impl::MutexState::Unlocked,
        );
        Ok(())
    }

    // fn completeClipboardPaste(
    //     self: *Surface,
    //     data: []const u8,
    //     allow_unsafe: bool,
    // ) !void {
    //
    // (`data: []const u8` -> raw bytes `&[u8]`, never a lossy String — R3.)
    pub(super) fn completeClipboardPaste(
        &mut self,
        data: &[u8],
        allow_unsafe: bool,
    ) -> Result<(), Error> {
        // if (data.len == 0) return;
        if data.is_empty() {
            return Ok(());
        }

        // const encode_opts: input.paste.Options = encode_opts: {
        let encode_opts: paste::Options = {
            // self.renderer_state.mutex.lock();
            // defer self.renderer_state.mutex.unlock();
            //
            // (guard-less zigstd Mutex — the defer is the explicit unlock at
            // every exit below.)
            unsafe { (*(*self.renderer_state.as_ptr()).mutex).lock() };
            // const opts: input.paste.Options = .fromTerminal(&self.io.terminal);
            let opts: paste::Options =
                paste::Options::from_terminal(unsafe { &(*self.io.as_ptr()).terminal });

            // If we have paste protection enabled, we detect unsafe pastes and return
            // an error. The error approach allows apprt to attempt to complete the paste
            // before falling back to requesting confirmation.
            //
            // We do not do this for bracketed pastes because bracketed pastes are
            // by definition safe since they're framed.
            // const unsafe = unsafe: {
            let unsafe_ = 'unsafe_blk: {
                // If we've disabled paste protection then we always allow the paste.
                // if (!self.config.clipboard_paste_protection) break :unsafe false;
                if !self.config.clipboard_paste_protection {
                    break 'unsafe_blk false;
                }

                // If we're allowed to paste unsafe data then we always allow the paste.
                // This is set during confirmation usually.
                // if (allow_unsafe) break :unsafe false;
                if allow_unsafe {
                    break 'unsafe_blk false;
                }

                // if (opts.bracketed) {
                if opts.bracketed {
                    // If we're bracketed and the paste contains and ending
                    // bracket then something naughty might be going on and we
                    // never trust it.
                    // if (std.mem.indexOf(u8, data, "\x1B[201~") != null) break :unsafe true;
                    //
                    // (`std.mem.indexOf(..) != null` -> a subslice scan — the
                    // paste.rs `index_of` precedent.)
                    if data.windows(6).any(|w| w == b"\x1b[201~") {
                        break 'unsafe_blk true;
                    }

                    // If we are bracketed and configured to trust that then the
                    // paste is not unsafe.
                    // if (self.config.clipboard_paste_bracketed_safe) break :unsafe false;
                    if self.config.clipboard_paste_bracketed_safe {
                        break 'unsafe_blk false;
                    }
                }

                // break :unsafe !input.paste.isSafe(data);
                !paste::is_safe(data)
            };

            // if (unsafe) {
            if unsafe_ {
                // log.info("potentially unsafe paste detected, rejecting until confirmation", .{});
                log::info!(
                    target: LOG_SCOPE,
                    "potentially unsafe paste detected, rejecting until confirmation"
                );
                // return error.UnsafePaste;
                //
                // (unlock before propagating — the explicit-unlock convention.)
                unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
                return Err(Error::UnsafePaste);
            }

            // With the lock held, we must scroll to the bottom.
            // We always scroll to the bottom for these inputs.
            // self.scrollToBottom() catch |err| {
            //     log.warn("error scrolling to bottom err={}", .{err});
            // };
            if let Err(err) = self.scrollToBottom() {
                // log.warn("error scrolling to bottom err={}", .{err});
                log::warn!(target: LOG_SCOPE, "error scrolling to bottom err={err:?}");
            }

            // break :encode_opts opts;
            unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
            opts
        };

        // Encode the data. In most cases this doesn't require any
        // copies, so we optimize for that case.
        // var data_duped: ?[]u8 = null;
        let mut data_duped: Option<Vec<u8>> = None;
        // const vecs = input.paste.encode(data, encode_opts) catch |err| switch (err) {
        //     error.MutableRequired => vecs: {
        //         const buf: []u8 = try self.alloc.dupe(u8, data);
        //         errdefer self.alloc.free(buf);
        //         data_duped = buf;
        //         break :vecs input.paste.encode(buf, encode_opts);
        //     },
        // };
        // defer if (data_duped) |v| {
        //     // This code path means the data did require a copy and mutation.
        //     // We must free it.
        //     self.alloc.free(v);
        // };
        //
        // PORT NOTE: the reference `encode` is comptime-dispatched on slice
        // mutability — const `data` may return `error.MutableRequired`; the
        // mutable retry over the duped `buf` cannot. Rustty splits this into
        // `paste::encode` (const, may err) and `paste::encode_mut` (mutable,
        // infallible), exactly the two `@TypeOf(data)` branches (the paste.rs
        // split note). `data_duped` is declared BEFORE `vecs` so it outlives the
        // borrow (`encode_mut`'s output aliases the duped buffer); the `defer
        // free` == its `Vec` dropping at scope end. `Option::insert` fuses the
        // reference's `data_duped = buf` store with the `encode(buf, ...)` call
        // (it returns `&mut` to the just-stored buffer).
        let vecs: [&[u8]; 3] = match paste::encode(data, encode_opts) {
            Ok(v) => v,
            Err(paste::Error::MutableRequired) => {
                // const buf: []u8 = try self.alloc.dupe(u8, data);
                // data_duped = buf;
                // break :vecs input.paste.encode(buf, encode_opts);
                paste::encode_mut(data_duped.insert(data.to_vec()), encode_opts)
            }
        };
        // (the reference `defer if (data_duped) |v| self.alloc.free(v);` == the
        // owned `data_duped: Option<Vec<u8>>` dropping at scope end below.)

        // for (vecs) |vec| if (vec.len > 0) {
        for vec in vecs {
            if !vec.is_empty() {
                // self.queueIo(try termio.Message.writeReq(
                //     self.alloc,
                //     vec,
                // ), .unlocked);
                self.queueIo(
                    termio::Message::writeReq(vec),
                    termio::termio_impl::MutexState::Unlocked,
                );
            }
        }

        Ok(())
    }

    // fn completeClipboardReadOSC52(
    //     self: *Surface,
    //     data: []const u8,
    //     clipboard_type: apprt.Clipboard,
    //     confirmed: bool,
    // ) !void {
    //
    // (`data: []const u8` -> raw bytes `&[u8]`, never a lossy String — R3.)
    fn completeClipboardReadOSC52(
        &mut self,
        data: &[u8],
        clipboard_type: apprt::Clipboard,
        confirmed: bool,
    ) -> Result<(), Error> {
        // We should never get here if clipboard-read is set to deny
        // assert(self.config.clipboard_read != .deny);
        debug_assert!(self.config.clipboard_read != configpkg::value_types::ClipboardAccess::Deny);

        // If clipboard-read is set to ask and we haven't confirmed with the user,
        // do that now
        // if (self.config.clipboard_read == .ask and !confirmed) {
        //     return error.UnauthorizedPaste;
        // }
        if self.config.clipboard_read == configpkg::value_types::ClipboardAccess::Ask && !confirmed
        {
            return Err(Error::UnauthorizedPaste);
        }

        // Even if the clipboard data is empty we reply, since presumably
        // the client app is expecting a reply. We first allocate our buffer.
        // This must hold the base64 encoded data PLUS the OSC code surrounding it.
        // const enc = std.base64.standard.Encoder;
        // const size = enc.calcSize(data.len);
        let size = zig_base64::calc_size(data.len());
        // const buf = try self.alloc.alloc(u8, size + 9); // const for OSC
        // errdefer self.alloc.free(buf);
        //
        // (the owned `buf` drops on every error exit below; the success exit
        // transfers it into the `.write_alloc` message — `c11fe5486`.)
        let mut buf: Vec<u8> = vec![0u8; size + 9]; // const for OSC

        // const kind: u8 = switch (clipboard_type) {
        let kind: u8 = match clipboard_type {
            // .standard => 'c',
            apprt::Clipboard::Standard => b'c',
            // .selection => 's',
            apprt::Clipboard::Selection => b's',
            // .primary => 'p',
            apprt::Clipboard::Primary => b'p',
        };

        // Wrap our data with the OSC code
        // const prefix = try std.fmt.bufPrint(buf, "\x1b]52;{c};", .{kind});
        // assert(prefix.len == 7);
        //
        // (the fixed `bufPrint` format `\x1b]52;{c};` is 7 bytes — written
        // directly, the SURFACE_ID `bufPrint` precedent.)
        buf[0] = 0x1b;
        buf[1] = b']';
        buf[2] = b'5';
        buf[3] = b'2';
        buf[4] = b';';
        buf[5] = kind;
        buf[6] = b';';
        let prefix_len = 7;
        debug_assert!(prefix_len == 7);
        // buf[buf.len - 2] = '\x1b';
        // buf[buf.len - 1] = '\\';
        let n = buf.len();
        buf[n - 2] = 0x1b;
        buf[n - 1] = b'\\';

        // Do the base64 encoding
        // const encoded = enc.encode(buf[prefix.len..], data);
        // assert(encoded.len == size);
        let encoded_len = zig_base64::encode(&mut buf[prefix_len..], data).len();
        debug_assert!(encoded_len == size);

        // self.queueIo(.{ .write_alloc = .{
        //     .alloc = self.alloc,
        //     .data = buf,
        // } }, .unlocked);
        //
        // (the allocated response transfers directly — a move, no copy; the
        // exact-capacity `Vec` boxes without reallocating.)
        self.queueIo(
            termio::Message::WriteAlloc(Alloc {
                data: buf.into_boxed_slice(),
            }),
            termio::termio_impl::MutexState::Unlocked,
        );

        Ok(())
    }

    // fn showDesktopNotification(self: *Surface, title: [:0]const u8, body: [:0]const u8) !void {
    //
    // (the `[:0]const u8` params are raw bytes — `&[u8]`, never a lossy String.)
    pub(super) fn showDesktopNotification(
        &mut self,
        title: &[u8],
        body: &[u8],
    ) -> Result<(), Error> {
        // Wyhash is used to hash the contents of the desktop notification to limit
        // how fast identical notifications can be sent sequentially.
        // const hash_algorithm = std.hash.Wyhash;
        //
        // PORT NOTE: Rustty uses the stdlib `DefaultHasher` (the font-shaper
        // `shared_grid_set::Key::hashcode` precedent). The digest is an internal
        // de-dup key compared only against the previous notification within the
        // same process (never serialized/observed cross-implementation), so the
        // exact algorithm is not part of the parity contract; SipHash's `write`
        // is unframed, so `write(title); write(body)` streams `title++body`
        // exactly as Wyhash's `update; update`.

        // const now = try std.time.Instant.now();
        //
        // (Rust `Instant::now` is infallible — the `try` collapses; the RingBell
        // precedent.)
        let now = std::time::Instant::now();

        // PARITY: std.time.ns_per_s = 1_000_000_000 (the config_struct.rs / RingBell
        // NS_PER_S precedent). `Instant.since` -> `saturating_duration_since` (ns).
        const NS_PER_S: u128 = 1_000_000_000;

        // Set a limit of one desktop notification per second so that the OS
        // doesn't kill us when we run out of resources.
        // if (self.app.last_notification_time) |last| {
        if let Some(last) = unsafe { (*self.app).last_notification_time } {
            // if (now.since(last) < 1 * std.time.ns_per_s) {
            if now.saturating_duration_since(last).as_nanos() < NS_PER_S {
                // log.warn("rate limiting desktop notifications", .{});
                log::warn!(target: LOG_SCOPE, "rate limiting desktop notifications");
                return Ok(());
            }
        }

        // const new_digest = d: {
        let new_digest = {
            use core::hash::Hasher;
            // var hash = hash_algorithm.init(0);
            let mut hash = std::collections::hash_map::DefaultHasher::new();
            // hash.update(title);
            hash.write(title);
            // hash.update(body);
            hash.write(body);
            // break :d hash.final();
            hash.finish()
        };

        // Set a limit of one notification per five seconds for desktop
        // notifications with identical content.
        // if (self.app.last_notification_time) |last| {
        if let Some(last) = unsafe { (*self.app).last_notification_time } {
            // if (self.app.last_notification_digest == new_digest) {
            if unsafe { (*self.app).last_notification_digest } == new_digest {
                // if (now.since(last) < 5 * std.time.ns_per_s) {
                if now.saturating_duration_since(last).as_nanos() < 5 * NS_PER_S {
                    // log.warn("suppressing identical desktop notification", .{});
                    log::warn!(target: LOG_SCOPE, "suppressing identical desktop notification");
                    return Ok(());
                }
            }
        }

        // self.app.last_notification_time = now;
        unsafe { (*self.app).last_notification_time = Some(now) };
        // self.app.last_notification_digest = new_digest;
        unsafe { (*self.app).last_notification_digest = new_digest };
        // _ = try self.rt_app.performAction(
        //     .{ .surface = self },
        //     .desktop_notification,
        //     .{
        //         .title = title,
        //         .body = body,
        //     },
        // );
        let _ = unsafe { &*self.rt_app }.perform_action(
            apprt::Target::surface_ref(&*self),
            apprt::Action::DesktopNotification(DesktopNotification { title, body }),
        )?;
        Ok(())
    }

    // fn crashThreadState(self: *Surface) crash.sentry.ThreadState {
    //
    // PORT NOTE (ported with C8, ahead of its C10 row — Surface.zig:6014;
    // sizeCallback/preeditCallback are the first `crash.sentry.thread_state`
    // sites, [ADV 6.1]; the C10 key tier added the keyCallback site).
    pub(super) fn crashThreadState(&self) -> crash::sentry::ThreadState<Surface<A>> {
        // return .{
        crash::sentry::ThreadState {
            // .type = .main,
            type_: crash::sentry::Type::Main,
            // .surface = self,
            surface: self,
        }
        // };
    }

    /// Tell the surface to present itself to the user. This may involve raising the
    /// window and switching tabs.
    // fn presentSurface(self: *Surface) !void {
    //
    // (Surface.zig:6023 — the C10 range's 6014-end tail; the inferred
    // `!void` is the performAction error.)
    //
    // PORT NOTE (N459): raw-self; see `handleMessage`.
    ///
    /// # Safety
    /// As [`Surface::handleMessage`].
    pub(super) unsafe fn presentSurface(self_: *mut Self) -> Result<(), Error> {
        // _ = try self.rt_app.performAction(
        //     .{ .surface = self },
        //     .present_terminal,
        //     {},
        // );
        let rt_app: &A = unsafe { &*(*self_).rt_app };
        let _ = rt_app.perform_action(
            // SAFETY: `self_` is live for the call (this fn's contract).
            unsafe { apprt::Target::surface_ptr(self_) },
            apprt::Action::PresentTerminal,
        )?;
        Ok(())
    }

    /// Get information about the process(es) running within the surface. Returns
    /// `null` if there was an error getting the information or the information is
    /// not available on a particular platform.
    //
    // pub fn getProcessInfo(self: *Surface, comptime info: ProcessInfo) ?ProcessInfo.Type(info) {
    //     return self.io.getProcessInfo(info);
    // }
    //
    // PORT NOTE (comptime-return split — the rustty-os pty.rs /
    // `Termio::getProcessInfo` precedent): Rust has no comptime
    // type-from-value, so the `info` selector splits into the two concrete
    // accessors (`.foreground_pid` -> `u64`, `.tty_name` -> `&CStr`).
    pub fn getForegroundPid(&mut self) -> Option<u64> {
        // (N444: raw-self, through the io block's raw root.)
        unsafe { termio::Termio::getForegroundPid(self.io.as_ptr()) }
    }

    /// See [`Surface::getForegroundPid`] (the `.tty_name` half of the
    /// reference `getProcessInfo`).
    pub fn getTtyName(&mut self) -> Option<&std::ffi::CStr> {
        // (N444: raw-self, through the io block's raw root.)
        unsafe { termio::Termio::getTtyName(self.io.as_ptr()) }
    }
}
