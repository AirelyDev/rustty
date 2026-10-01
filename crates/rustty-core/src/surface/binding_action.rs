//! PORTS: ghostty src/Surface.zig — `performBindingAction` and `writeScreenFile`.

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
    /// Perform a binding action. A binding is a keybinding. This function
    /// must be called from the GUI thread.
    ///
    /// This function returns true if the binding action was performed. This
    /// may return false if the binding action is not supported or if the
    /// binding action would do nothing (i.e. previous tab with no tabs).
    ///
    /// NOTE: At the time of writing this comment, only previous/next tab
    /// will ever return false. We can expand this in the future if it becomes
    /// useful. We did previous/next tab so we could implement #498.
    //
    // pub fn performBindingAction(self: *Surface, action: input.Binding.Action) !bool {
    //
    // PORT NOTE (chunk C14a — Surface.zig:4775-5202, the first half of the
    // switch; the second half `.scroll_to_selection`.. lands with C14b). The
    // `.search` arm is DEFERRED (DEFERRALS.md "core C14a"): it constructs the
    // vt search thread sharing `self.renderer_state.mutex`, which needs a
    // cross-crate mutex-type unification (the search `Options.mutex` is still
    // `*const std::sync::Mutex<()>` while the shared renderer mutex is the
    // guard-less `rustty_zigstd::thread::Mutex`) AND a self-referential
    // long-lived thread spawn (a stable pointer into `self.search.?.state`).
    //
    // PORT NOTE (N442, P7-W7 T5b): raw-self. A `&mut self` receiver is a
    // protected Unique for the whole call, and the `new_window`, `new_tab` and
    // `new_split` arms run the embedder's action callback, which re-enters this
    // surface synchronously (the macOS host calls
    // `rustty_surface_inherited_config` there, reading `font_size` and the pwd
    // through the parent pointer). Those three arms touch the surface only by
    // place through `self_` and the `Target`; `this`, an unprotected local
    // reborrow the other arms use, is dead in them. Audit of the other
    // `perform_action` arms: the macOS host's handlers re-enter no surface for
    // them, except `reset_window_size`, whose handler resizes the window
    // synchronously and so WRITES this surface (`rustty_surface_set_size` ->
    // `sizeCallback`). That is sound because `Target::Surface` is a raw pointer
    // with no protector (N456, P7-W6 T3) and `this` is dead once the arm has
    // built the target. Proofs: `.port/phase-2/mirihar-n441`, `mirihar-n456`.
    ///
    /// # Safety
    /// `self_` is the stable Surface allocation (init's stability contract); no
    /// protected `&mut Surface` may span the call.
    pub unsafe fn performBindingAction(
        self_: *mut Surface<A>,
        action: binding::Action,
    ) -> Result<bool, Error> {
        let this = unsafe { &mut *self_ };
        // Forward app-scoped actions to the app. Some app-scoped actions are
        // special-cased here because they do some special things when performed
        // from the surface.
        // if (action.scoped(.app)) |app_action| {
        if let Some(app_action) = action.scoped_app() {
            // switch (app_action) {
            match app_action {
                // .new_window => try self.app.newWindow(
                //     self.rt_app,
                //     .{ .parent = self },
                // ),
                //
                // (N442/N443: re-entrant, so by place through `self_`, and
                // `App::newWindow` is raw-self: the host re-enters both this
                // surface and the core app.)
                binding::ScopedApp::NewWindow => {
                    let rt_app: &A = unsafe { &*(*self_).rt_app };
                    unsafe {
                        crate::app::App::newWindow(
                            (*self_).app,
                            rt_app,
                            crate::app::NewWindow {
                                parent: Some(self_),
                            },
                        )
                    }?;
                }

                // Undo and redo both support both surface and app targeting.
                // If we are triggering on a surface then we perform the
                // action with the surface target.
                // .undo => return try self.rt_app.performAction(
                //     .{ .surface = self },
                //     .undo,
                //     {},
                // ),
                binding::ScopedApp::Undo => {
                    return Ok(unsafe { &*this.rt_app }.perform_action(
                        apprt::Target::surface_ref(&*this),
                        apprt::Action::Undo,
                    )?);
                }

                // .redo => return try self.rt_app.performAction(
                //     .{ .surface = self },
                //     .redo,
                //     {},
                // ),
                binding::ScopedApp::Redo => {
                    return Ok(unsafe { &*this.rt_app }.perform_action(
                        apprt::Target::surface_ref(&*this),
                        apprt::Action::Redo,
                    )?);
                }

                // else => try self.app.performAction(
                //     self.rt_app,
                //     action.scoped(.app).?,
                // ),
                //
                // (`app_action` IS `action.scoped(.app).?`, the value the
                // reference re-derives here.)
                //
                // (N443: raw-self through `self_`; e.g. `reload_config`'s host
                // callback rewrites the core app and every surface, this one
                // included.)
                _ => {
                    let rt_app: &A = unsafe { &*(*self_).rt_app };
                    unsafe { crate::app::App::performAction((*self_).app, rt_app, app_action) }?;
                }
            }
            // return true;
            return Ok(true);
        }

        // switch (action.scoped(.surface).?) {
        match action.scoped_surface().unwrap() {
            // .csi, .esc => |data| {
            binding::ScopedSurface::Csi(data) | binding::ScopedSurface::Esc(data) => {
                // We need to send the CSI/ESC sequence as a single write request.
                // If you split it across two then the shell can interpret it
                // as two literals.
                // var buf: [128]u8 = undefined;
                let mut buf = [0u8; 128];
                // const full_data = switch (action) {
                //     .csi => try std.fmt.bufPrint(&buf, "\x1b[{s}", .{data}),
                //     .esc => try std.fmt.bufPrint(&buf, "\x1b{s}", .{data}),
                //     else => unreachable,
                // };
                //
                // (fixed-buffer `bufPrint` over a Cursor — the encodeKey
                // precedent; the reference's `error.NoSpaceLeft` surfaces as
                // io's `WriteZero`, `try`-propagated [`Error::Io`].)
                let full_data_len = {
                    use std::io::Write as _;
                    let mut writer = std::io::Cursor::new(&mut buf[..]);
                    match action {
                        binding::Action::Csi(_) => {
                            writer.write_all(b"\x1b[")?;
                            writer.write_all(data)?;
                        }
                        binding::Action::Esc(_) => {
                            writer.write_all(b"\x1b")?;
                            writer.write_all(data)?;
                        }
                        // else => unreachable,
                        _ => unreachable!(),
                    }
                    writer.position() as usize
                };
                let full_data = &buf[..full_data_len];
                // self.queueIo(try termio.Message.writeReq(
                //     self.alloc,
                //     full_data,
                // ), .unlocked);
                this.queueIo(
                    termio::Message::writeReq(full_data),
                    termio::termio_impl::MutexState::Unlocked,
                );

                // CSI/ESC triggers a scroll.
                // {
                //     self.renderer_state.mutex.lock();
                //     defer self.renderer_state.mutex.unlock();
                //     self.scrollToBottom() catch |err| {
                //         log.warn("error scrolling to bottom err={}", .{err});
                //     };
                // }
                {
                    unsafe { (*(*this.renderer_state.as_ptr()).mutex).lock() };
                    if let Err(err) = this.scrollToBottom() {
                        // log.warn("error scrolling to bottom err={}", .{err});
                        log::warn!(target: LOG_SCOPE, "error scrolling to bottom err={err:?}");
                    }
                    unsafe { (*(*this.renderer_state.as_ptr()).mutex).unlock() };
                }
            }

            // .text => |data| {
            binding::ScopedSurface::Text(data) => {
                // For text we always allocate just because its easier to
                // handle all cases that way.
                // const buf = try self.alloc.alloc(u8, data.len);
                // defer self.alloc.free(buf);
                let mut buf = vec![0u8; data.len()];
                // const text = configpkg.string.parse(buf, data) catch |err| {
                //     log.warn(
                //         "error parsing text binding text={s} err={}",
                //         .{ data, err },
                //     );
                //     return true;
                // };
                //
                // (the reference `parse` returns a slice; the ported `parse`
                // writes into `buf` and returns the written length.)
                let text_len = match configpkg::string::parse(&mut buf, data) {
                    Ok(n) => n,
                    Err(err) => {
                        // log.warn("error parsing text binding text={s} err={}", .{ data, err });
                        log::warn!(
                            target: LOG_SCOPE,
                            "error parsing text binding text={} err={err:?}",
                            String::from_utf8_lossy(data)
                        );
                        return Ok(true);
                    }
                };
                let text = &buf[..text_len];
                // self.queueIo(try termio.Message.writeReq(
                //     self.alloc,
                //     text,
                // ), .unlocked);
                this.queueIo(
                    termio::Message::writeReq(text),
                    termio::termio_impl::MutexState::Unlocked,
                );

                // Text triggers a scroll.
                // {
                //     self.renderer_state.mutex.lock();
                //     defer self.renderer_state.mutex.unlock();
                //     self.scrollToBottom() catch |err| {
                //         log.warn("error scrolling to bottom err={}", .{err});
                //     };
                // }
                {
                    unsafe { (*(*this.renderer_state.as_ptr()).mutex).lock() };
                    if let Err(err) = this.scrollToBottom() {
                        // log.warn("error scrolling to bottom err={}", .{err});
                        log::warn!(target: LOG_SCOPE, "error scrolling to bottom err={err:?}");
                    }
                    unsafe { (*(*this.renderer_state.as_ptr()).mutex).unlock() };
                }
            }

            // .cursor_key => |ck| {
            binding::ScopedSurface::CursorKey(ck) => {
                // We send a different sequence depending on if we're
                // in cursor keys mode. We're in "normal" mode if cursor
                // keys mode is NOT set.
                // const normal = normal: {
                //     self.renderer_state.mutex.lock();
                //     defer self.renderer_state.mutex.unlock();
                //
                //     // With the lock held, we must scroll to the bottom.
                //     // We always scroll to the bottom for these inputs.
                //     self.scrollToBottom() catch |err| {
                //         log.warn("error scrolling to bottom err={}", .{err});
                //     };
                //
                //     break :normal !self.io.terminal.modes.get(.cursor_keys);
                // };
                let normal = {
                    unsafe { (*(*this.renderer_state.as_ptr()).mutex).lock() };

                    // With the lock held, we must scroll to the bottom.
                    // We always scroll to the bottom for these inputs.
                    if let Err(err) = this.scrollToBottom() {
                        // log.warn("error scrolling to bottom err={}", .{err});
                        log::warn!(target: LOG_SCOPE, "error scrolling to bottom err={err:?}");
                    }

                    let normal = !unsafe { &(*this.io.as_ptr()).terminal }
                        .modes
                        .get(terminal::modes::Mode::CursorKeys);
                    unsafe { (*(*this.renderer_state.as_ptr()).mutex).unlock() };
                    normal
                };

                // if (normal) {
                //     self.queueIo(.{ .write_stable = ck.normal }, .unlocked);
                // } else {
                //     self.queueIo(.{ .write_stable = ck.application }, .unlocked);
                // }
                //
                // PORT NOTE (write_stable -> writeReq): the reference
                // `write_stable` aliases the keybind config's stable slice
                // storage. The Rust `CursorKey` OWNS its `Vec<u8>` sequences
                // (the byte-slice convention) and this `action` is a cloned,
                // stack-local copy dropped at return, so no "stable pointer"
                // exists; `queueIo` requires a `Message<'static>`, so the bytes
                // are COPIED via `writeReq` (write_small/write_alloc).
                if normal {
                    this.queueIo(
                        termio::Message::writeReq(&ck.normal),
                        termio::termio_impl::MutexState::Unlocked,
                    );
                } else {
                    this.queueIo(
                        termio::Message::writeReq(&ck.application),
                        termio::termio_impl::MutexState::Unlocked,
                    );
                }
            }

            // .reset => {
            binding::ScopedSurface::Reset => {
                // self.renderer_state.mutex.lock();
                // defer self.renderer_state.mutex.unlock();
                // self.renderer_state.terminal.fullReset();
                unsafe { (*(*this.renderer_state.as_ptr()).mutex).lock() };
                unsafe { (*(*this.renderer_state.as_ptr()).terminal).full_reset() };
                unsafe { (*(*this.renderer_state.as_ptr()).mutex).unlock() };
            }

            // .start_search => {
            binding::ScopedSurface::StartSearch => {
                // To save resources, we don't actually start a search here,
                // we just notify the apprt. The real thread will start when
                // the first needles are set.
                // return try self.rt_app.performAction(
                //     .{ .surface = self },
                //     .start_search,
                //     .{ .needle = "" },
                // );
                return Ok(unsafe { &*this.rt_app }.perform_action(
                    apprt::Target::surface_ref(&*this),
                    apprt::Action::StartSearch(StartSearch { needle: b"" }),
                )?);
            }

            // .search_selection => {
            binding::ScopedSurface::SearchSelection => {
                // const selection = try self.selectionString(self.alloc) orelse return false;
                // defer self.alloc.free(selection);
                //
                // (the owned `Vec` drops at scope end.)
                let selection = match this.selectionString(this.alloc)? {
                    Some(s) => s,
                    None => return Ok(false),
                };
                // return try self.rt_app.performAction(
                //     .{ .surface = self },
                //     .start_search,
                //     .{ .needle = selection },
                // );
                return Ok(unsafe { &*this.rt_app }.perform_action(
                    apprt::Target::surface_ref(&*this),
                    apprt::Action::StartSearch(StartSearch { needle: &selection }),
                )?);
            }

            // .end_search => {
            binding::ScopedSurface::EndSearch => {
                // We only return that this was performed if we actually
                // stopped a search, but we also send the apprt end_search so
                // that GUIs can clean up stale stuff.
                // const performed = self.search != null;
                let performed = this.search.is_some();

                // if (self.search) |*s| {
                //     s.deinit();
                //     self.search = null;
                // }
                //
                // (`take()` is the reference's `deinit` + `= null`; `deinit`
                // consumes the by-value `Search`.)
                if let Some(s) = this.search.take() {
                    s.deinit();
                }

                // _ = try self.rt_app.performAction(
                //     .{ .surface = self },
                //     .end_search,
                //     {},
                // );
                let _ = unsafe { &*this.rt_app }
                    .perform_action(apprt::Target::surface_ref(&*this), apprt::Action::EndSearch)?;

                // return performed;
                return Ok(performed);
            }

            // .search => |text| search: {
            //
            // PORT NOTE (SEARCH_BINDING_ARM_DESIGN): the two blockers are
            // resolved — (a) the vt `search::thread::Options.mutex` is retyped to
            // the shared guard-less `rustty_zigstd::thread::Mutex` (so
            // `self.renderer_state.mutex` is type-compatible), and (b) the
            // self-referential construct-then-spawn lands via a stable-address
            // raw-ptr discipline: the `Search` is stored FIRST (`thread: None` ==
            // `.thread = undefined`), a stable `*mut Thread` is projected from the
            // stored state, the thread is spawned on THAT raw pointer, and the
            // returned handle is written back through a disjoint raw sub-place.
            // Post-spawn producers reach the running thread ONLY through decoupled
            // handles (§3; proven SB+TB clean on `mirihar-chunk3b`
            // `search_shipped`/`callback_shipped`).
            binding::ScopedSurface::Search(text) => {
                // const s: *Search = if (self.search) |*s| s else init: {
                if this.search.is_none() {
                    // init: {
                    //     // If we're stopping the search and we had no prior search,
                    //     // then there is nothing to do.
                    //     if (text.len == 0) return false;
                    if text.is_empty() {
                        return Ok(false);
                    }

                    // Stable base address of the pinned Surface: `init` writes it
                    // through a `*mut Surface`. (N444 fix round 1: the search
                    // thread no longer re-enters through it; `searchCallback`
                    // gets the spawn-time `SearchCallbackCtx` below, and the
                    // surface pointer is only the surface mailbox's identity.)
                    //
                    // (N442: the caller's raw root itself, not a pointer derived
                    // from the local reborrow, which later reborrows retire.)
                    let surface_ptr: *mut Surface<A> = self_;

                    // We need to assign directly to self.search because we need
                    // a stable pointer back to the thread state.
                    // self.search = .{
                    //     .state = try .init(self.alloc, .{
                    //         .mutex = self.renderer_state.mutex,
                    //         .terminal = self.renderer_state.terminal,
                    //         .event_cb = &searchCallback,
                    //         .event_userdata = self,
                    //     }),
                    //     .thread = undefined,
                    // };
                    //
                    // ONE-SHOT BUILD (§2.4): build `state` as a LOCAL, arm its
                    // wakeup/stop waits + capture the decoupled wakeup/stop
                    // notifiers OFF the local (Arc<Shared>-backed, move-stable),
                    // THEN move it whole into `self.search`. `thread: None` is the
                    // `.thread = undefined` placeholder; `mailbox` is filled from
                    // the POST-MOVE `state_ptr` lineage below (a pre-move
                    // `addr_of!` alias would be popped by the child's
                    // `Box<Mailbox>` noalias — mirihar `search_shipped`).
                    // PORT NOTE (N444 fix round 1, P7-W7 T5c): the callback's
                    // userdata is a context captured here, on the main thread,
                    // instead of the surface (`SearchCallbackCtx`'s doc). It is
                    // freed once: on the `init` error just below, on the spawn
                    // failure, or by `Search::deinit` after the join.
                    let callback_ctx: *mut SearchCallbackCtx<A> = unsafe {
                        let rt = (*surface_ptr).renderer_thread.as_ptr();
                        Box::into_raw(Box::new(SearchCallbackCtx {
                            renderer_mailbox: (*rt).mailbox,
                            renderer_wakeup: &raw const (*rt).wakeup,
                            surface_mailbox: Self::surface_mailbox_raw(surface_ptr),
                        }))
                    };
                    let state_result = terminal::search::thread::Thread::init(
                        this.alloc.into(),
                        terminal::search::thread::Options {
                            // .mutex = self.renderer_state.mutex,
                            mutex: unsafe { (*this.renderer_state.as_ptr()).mutex }, // *mut -> *const (§1)
                            // .terminal = self.renderer_state.terminal,
                            terminal: unsafe { (*this.renderer_state.as_ptr()).terminal },
                            // .event_cb = &searchCallback,
                            event_cb: Some(Self::searchCallback),
                            // .event_userdata = self,
                            //
                            // (N444 fix round 1: the spawn-time context.)
                            event_userdata: callback_ctx.cast::<core::ffi::c_void>(),
                        },
                    );
                    let mut state = match state_result {
                        Ok(v) => v,
                        Err(e) => {
                            // (N444 fix round 1: nothing else holds the context.)
                            unsafe { SearchCallbackCtx::<A>::free(callback_ctx.cast()) };
                            // needs From<..search::thread::xev::Error> (added above)
                            return Err(e.into());
                        }
                    };

                    // Arm the wakeup/stop waits ON THE LOCAL + capture the
                    // decoupled notifiers BEFORE the move (identical to the vt
                    // `search_thread` test setup; `thread_main` re-arms them
                    // idempotently).
                    state.wakeup.wait(
                        &state.loop_,
                        &mut state.wakeup_c,
                        terminal::search::thread::xev::Callback::Wakeup,
                    );
                    state.stop.wait(
                        &state.loop_,
                        &mut state.stop_c,
                        terminal::search::thread::xev::Callback::Stop,
                    );
                    let wn = state.wakeup.notifier(); // holds Arc<Shared>
                    let sn = state.stop.notifier();

                    // self.search = .{ .state = <local>, .thread = undefined };
                    //
                    // (N444: the state moves into its own allocation here, the
                    // move `self.search = ..` made; `state_ptr` below is its raw
                    // root, which the child and the decoupled mailbox share.)
                    this.search = Some(Search {
                        state: ThreadSharedBox::new(state),
                        thread: None,               // == `.thread = undefined`
                        mailbox: core::ptr::null(), // set from state_ptr below
                        wakeup: wn,
                        stop: sn,
                        callback_ctx: callback_ctx.cast(),
                        callback_ctx_free: SearchCallbackCtx::<A>::free,
                    });

                    // const s: *Search = &self.search.?;   — ONE transient &mut,
                    // taken at birth (nothing aliases `state` yet), lowered
                    // immediately to raw. After this NO &mut Search / &mut Thread
                    // is ever formed again.
                    let search_ptr: *mut Search = &raw mut *this.search.as_mut().unwrap();
                    let state_ptr: *mut terminal::search::thread::Thread =
                        unsafe { (*search_ptr).state.as_ptr() };
                    // Decoupled mailbox = the Box<Mailbox> pointee, via the
                    // POST-MOVE state_ptr lineage (mirihar `search_shipped`).
                    unsafe {
                        (*search_ptr).mailbox = &raw const *(*state_ptr).mailbox;
                    }

                    // errdefer s.state.deinit();
                    // s.thread = try .spawn(
                    //     .{},
                    //     terminal.search.Thread.threadMain,
                    //     .{&s.state},
                    // );
                    // s.thread.setName("search") catch {};
                    struct SendState(*mut terminal::search::thread::Thread);
                    unsafe impl Send for SendState {}
                    let send = SendState(state_ptr);
                    let spawned = rustty_zigstd::thread::spawn_config()
                        .name("search".into()) // == setName("search")
                        .spawn(move || {
                            let s = send;
                            unsafe { (*s.0).thread_main() };
                        });
                    let handle = match spawned {
                        Ok(h) => h,
                        Err(e) => {
                            // errdefer s.state.deinit(); + the `try` unwinds
                            // performBindingAction's `!bool` (`Ok(false)` would
                            // SWALLOW the spawn error — not faithful). Deviation
                            // (§2.4): also clear `self.search` — a `Some` holding a
                            // deinit'd state + `thread: None` is a dangling hazard;
                            // `None` is the Rust empty. `Builder::spawn`'s
                            // `io::Error` already has `From` for surface `Error`.
                            //
                            // (N444: then the boxed state is dropped and freed,
                            // where the inline field was dropped with the
                            // `Search`; the child never started.)
                            unsafe {
                                (*state_ptr).deinit();
                                (*search_ptr).state.drop_and_dealloc();
                                // (N444 fix round 1: the never-used context.)
                                SearchCallbackCtx::<A>::free(callback_ctx.cast());
                            }
                            this.search = None;
                            return Err(e.into());
                        }
                    };
                    // s.thread = handle;  — through the disjoint `thread` sub-place
                    // of search_ptr (no &mut Search reborrow, so the child's live
                    // state_ptr tag is not popped; SB-clean).
                    unsafe {
                        (*search_ptr).thread = Some(handle);
                    }
                    // break :init s;
                }

                // Zero-length text means stop searching.
                // if (text.len == 0) {
                //     s.deinit();
                //     self.search = null;
                //     break :search;
                // }
                if text.is_empty() {
                    // `take()` is the reference's `deinit` + `= null`; `deinit`
                    // stop-notifies (decoupled handle) THEN joins THEN frees the
                    // state (§3.3). After `take()` the child is dead — no alias.
                    this.search.take().unwrap().deinit();
                    // break :search
                } else {
                    // _ = s.state.mailbox.push(
                    //     .{ .change_needle = try .init(
                    //         self.alloc,
                    //         text,
                    //     ) },
                    //     .forever,
                    // );
                    // s.state.wakeup.notify() catch {};
                    //
                    // PRODUCER PATH: reach the mailbox + wakeup through the
                    // DECOUPLED handles, NEVER `self.search.as_mut().state` (a
                    // &mut Thread reborrow while the child holds a live &mut self
                    // → SB UB; mirihar `search_naive`).
                    let s = this.search.as_ref().unwrap(); // &Search: sibling handles only
                    let mailbox = s.mailbox;
                    let wakeup = s.wakeup.clone();
                    let needle = terminal::search::thread::WriteReq::init(text);
                    let _ = unsafe {
                        (*mailbox).push(
                            terminal::search::thread::Message::ChangeNeedle(needle),
                            Timeout::Forever,
                        )
                    };
                    let _ = wakeup.notify();
                }
            }

            // .navigate_search => |nav| {
            binding::ScopedSurface::NavigateSearch(nav) => {
                // const s: *Search = if (self.search) |*s| s else return false;
                //
                // PORT NOTE (§3.3 retrofit): `as_ref` (not `as_mut`) — the
                // producer reaches the running thread through the DECOUPLED
                // `s.mailbox`/`s.wakeup` handles, NEVER `s.state.mailbox`
                // (a &mut Thread reborrow while the child holds a live &mut self
                // → SB UB; mirihar `search_naive`).
                let (mailbox, wakeup) = match this.search.as_ref() {
                    Some(s) => (s.mailbox, s.wakeup.clone()),
                    None => return Ok(false),
                };
                // _ = s.state.mailbox.push(
                //     .{ .select = switch (nav) {
                //         .next => .next,
                //         .previous => .prev,
                //     } },
                //     .forever,
                // );
                let _ = unsafe {
                    (*mailbox).push(
                        terminal::search::thread::Message::Select(match nav {
                            binding::NavigateSearch::Next => terminal::search::screen::Select::Next,
                            binding::NavigateSearch::Previous => {
                                terminal::search::screen::Select::Prev
                            }
                        }),
                        Timeout::Forever,
                    )
                };
                // s.state.wakeup.notify() catch {};
                let _ = wakeup.notify();
            }

            // .copy_to_clipboard => |format| {
            binding::ScopedSurface::CopyToClipboard(format) => {
                // self.renderer_state.mutex.lock();
                // defer self.renderer_state.mutex.unlock();
                //
                // (the deferred unlock at every exit — the selectionString C9
                // convention; the fallible calls unlock before propagating.)
                unsafe { (*(*this.renderer_state.as_ptr()).mutex).lock() };

                // if (self.io.terminal.screens.active.selection) |sel| {
                if let Some(sel) =
                    unsafe { (*(*this.io.as_ptr()).terminal.screens.active).selection }
                {
                    // try self.copySelectionToClipboards(
                    //     sel,
                    //     &.{.standard},
                    //     format,
                    // );
                    if let Err(err) =
                        this.copySelectionToClipboards(sel, &[apprt::Clipboard::Standard], format)
                    {
                        unsafe { (*(*this.renderer_state.as_ptr()).mutex).unlock() };
                        return Err(err);
                    }

                    // Clear the selection if configured to do so.
                    // if (self.config.selection_clear_on_copy) {
                    if this.config.selection_clear_on_copy {
                        // if (self.setSelection(null)) {
                        //     self.queueRender() catch |err| {
                        //         log.warn("failed to queue render after clear selection err={}", .{err});
                        //     };
                        // } else |err| {
                        //     log.warn("failed to clear selection after copy err={}", .{err});
                        // }
                        match this.setSelection(None) {
                            Ok(()) => {
                                if let Err(err) = this.queueRender() {
                                    // log.warn("failed to queue render after clear selection err={}", .{err});
                                    log::warn!(
                                        target: LOG_SCOPE,
                                        "failed to queue render after clear selection err={err:?}"
                                    );
                                }
                            }
                            Err(err) => {
                                // log.warn("failed to clear selection after copy err={}", .{err});
                                log::warn!(
                                    target: LOG_SCOPE,
                                    "failed to clear selection after copy err={err:?}"
                                );
                            }
                        }
                    }

                    // return true;
                    unsafe { (*(*this.renderer_state.as_ptr()).mutex).unlock() };
                    return Ok(true);
                }

                // return false;
                unsafe { (*(*this.renderer_state.as_ptr()).mutex).unlock() };
                return Ok(false);
            }

            // .copy_url_to_clipboard => {
            binding::ScopedSurface::CopyUrlToClipboard => {
                // If the mouse isn't over a link, nothing we can do.
                // if (!self.mouse.over_link) return false;
                if !this.mouse.over_link {
                    return Ok(false);
                }
                // const pos = try self.rt_surface.getCursorPos();
                let pos = unsafe { &*this.rt_surface }.get_cursor_pos()?;

                // self.renderer_state.mutex.lock();
                // defer self.renderer_state.mutex.unlock();
                //
                // (the deferred unlock at every exit — the C9 convention.)
                unsafe { (*(*this.renderer_state.as_ptr()).mutex).lock() };
                // if (try self.linkAtPos(pos)) |link_info| {
                let link_info = match this.linkAtPos(pos) {
                    Ok(v) => v,
                    Err(err) => {
                        unsafe { (*(*this.renderer_state.as_ptr()).mutex).unlock() };
                        return Err(err);
                    }
                };
                if let Some(link_info) = link_info {
                    // const url_text = switch (link_info.action) {
                    let url_text: Vec<u8> = match link_info.action {
                        // .open => url_text: {
                        link::Action::Open => {
                            // For regex links, get the text from selection
                            // break :url_text (self.io.terminal.screens.active.selectionString(self.alloc, .{
                            //     .sel = link_info.selection,
                            //     .trim = self.config.clipboard_trim_trailing_spaces,
                            // })) catch |err| {
                            //     log.err("error reading url string err={}", .{err});
                            //     return false;
                            // };
                            let r = unsafe { &mut *(*this.io.as_ptr()).terminal.screens.active }
                                .selection_string(
                                    this.alloc.into(),
                                    terminal::screen::SelectionString {
                                        sel: link_info.selection,
                                        trim: this.config.clipboard_trim_trailing_spaces,
                                    },
                                );
                            match r {
                                Ok(v) => v,
                                Err(err) => {
                                    // log.err("error reading url string err={}", .{err});
                                    log::error!(
                                        target: LOG_SCOPE,
                                        "error reading url string err={err:?}"
                                    );
                                    unsafe { (*(*this.renderer_state.as_ptr()).mutex).unlock() };
                                    return Ok(false);
                                }
                            }
                        }

                        // ._open_osc8 => url_text: {
                        link::Action::OpenOsc8 => {
                            // For OSC8 links, get the URI directly from hyperlink data
                            // const uri = self.osc8URI(link_info.selection.start()) orelse {
                            //     log.warn("failed to get URI for OSC8 hyperlink", .{});
                            //     return false;
                            // };
                            let uri = match this.osc8URI(link_info.selection.start()) {
                                Some(v) => v,
                                None => {
                                    // log.warn("failed to get URI for OSC8 hyperlink", .{});
                                    log::warn!(
                                        target: LOG_SCOPE,
                                        "failed to get URI for OSC8 hyperlink"
                                    );
                                    unsafe { (*(*this.renderer_state.as_ptr()).mutex).unlock() };
                                    return Ok(false);
                                }
                            };
                            // break :url_text try self.alloc.dupeZ(u8, uri);
                            //
                            // (`dupeZ` — an owned copy of the raw URI bytes; the
                            // NUL sentinel drops per the byte-slice convention,
                            // the processLinks C13 precedent.)
                            uri.to_vec()
                        }
                    };
                    // defer self.alloc.free(url_text);
                    // (the owned `Vec` drops at scope end.)

                    // self.rt_surface.setClipboard(.standard, &.{.{
                    //     .mime = "text/plain",
                    //     .data = url_text,
                    // }}, false) catch |err| {
                    //     log.err("error copying url to clipboard err={}", .{err});
                    //     return false;
                    // };
                    if let Err(err) = unsafe { &*this.rt_surface }.set_clipboard(
                        apprt::Clipboard::Standard,
                        &[apprt::ClipboardContent {
                            mime: b"text/plain".as_slice(),
                            data: &url_text,
                        }],
                        false,
                    ) {
                        // log.err("error copying url to clipboard err={}", .{err});
                        log::error!(target: LOG_SCOPE, "error copying url to clipboard err={err:?}");
                        unsafe { (*(*this.renderer_state.as_ptr()).mutex).unlock() };
                        return Ok(false);
                    }

                    // return true;
                    unsafe { (*(*this.renderer_state.as_ptr()).mutex).unlock() };
                    return Ok(true);
                }

                // return false;
                unsafe { (*(*this.renderer_state.as_ptr()).mutex).unlock() };
                return Ok(false);
            }

            // .copy_title_to_clipboard => return try self.rt_app.performAction(
            //     .{ .surface = self },
            //     .copy_title_to_clipboard,
            //     {},
            // ),
            binding::ScopedSurface::CopyTitleToClipboard => {
                return Ok(unsafe { &*this.rt_app }.perform_action(
                    apprt::Target::surface_ref(&*this),
                    apprt::Action::CopyTitleToClipboard,
                )?);
            }

            // .paste_from_clipboard => return (try self.startClipboardRequest(
            //     .standard,
            //     .{ .paste = .standard },
            // )) == .started,
            binding::ScopedSurface::PasteFromClipboard => {
                return Ok(this.startClipboardRequest(
                    apprt::Clipboard::Standard,
                    apprt::ClipboardRequest::Paste(apprt::Clipboard::Standard),
                )? == apprt::ClipboardReadResult::Started);
            }

            // .paste_from_selection => return (try self.startClipboardRequest(
            //     .selection,
            //     .{ .paste = .selection },
            // )) == .started,
            binding::ScopedSurface::PasteFromSelection => {
                return Ok(this.startClipboardRequest(
                    apprt::Clipboard::Selection,
                    apprt::ClipboardRequest::Paste(apprt::Clipboard::Selection),
                )? == apprt::ClipboardReadResult::Started);
            }

            // .increase_font_size => |delta| {
            binding::ScopedSurface::IncreaseFontSize(delta) => {
                // Max delta is somewhat arbitrary.
                // const clamped_delta = @max(0, @min(255, delta));
                //
                // PORT NOTE: kept as nested max/min (NOT `.clamp()`): Zig
                // `@max`/`@min` return the non-NaN operand on NaN input (so a
                // NaN `delta` clamps to 255), which Rust `f32::max`/`f32::min`
                // match but `f32::clamp` does NOT (it returns NaN) — the
                // manual-clamp lint is allowed to preserve that semantics.
                #[allow(clippy::manual_clamp)]
                let clamped_delta = f32::max(0.0, f32::min(255.0, delta));

                // log.debug("increase font size={}", .{clamped_delta});
                log::debug!(target: LOG_SCOPE, "increase font size={clamped_delta}");

                // Max point size is somewhat arbitrary.
                // var size = self.font_size;
                let mut size = this.font_size;
                // size.points = @min(size.points + clamped_delta, 255);
                size.points = f32::min(size.points + clamped_delta, 255.0);
                // try self.setFontSize(size);
                this.setFontSize(size)?;

                // Mark that we manually adjusted the font size
                // self.font_size_adjusted = true;
                this.font_size_adjusted = true;
            }

            // .decrease_font_size => |delta| {
            binding::ScopedSurface::DecreaseFontSize(delta) => {
                // Max delta is somewhat arbitrary.
                // const clamped_delta = @max(0, @min(255, delta));
                //
                // PORT NOTE: kept as nested max/min (NOT `.clamp()`): Zig
                // `@max`/`@min` return the non-NaN operand on NaN input (so a
                // NaN `delta` clamps to 255), which Rust `f32::max`/`f32::min`
                // match but `f32::clamp` does NOT (it returns NaN) — the
                // manual-clamp lint is allowed to preserve that semantics.
                #[allow(clippy::manual_clamp)]
                let clamped_delta = f32::max(0.0, f32::min(255.0, delta));

                // log.debug("decrease font size={}", .{clamped_delta});
                log::debug!(target: LOG_SCOPE, "decrease font size={clamped_delta}");

                // var size = self.font_size;
                let mut size = this.font_size;
                // size.points = @max(1, size.points - clamped_delta);
                size.points = f32::max(1.0, size.points - clamped_delta);
                // try self.setFontSize(size);
                this.setFontSize(size)?;

                // Mark that we manually adjusted the font size
                // self.font_size_adjusted = true;
                this.font_size_adjusted = true;
            }

            // .reset_font_size => {
            binding::ScopedSurface::ResetFontSize => {
                // log.debug("reset font size", .{});
                log::debug!(target: LOG_SCOPE, "reset font size");

                // var size = self.font_size;
                let mut size = this.font_size;
                // size.points = self.config.original_font_size;
                size.points = this.config.original_font_size;
                // try self.setFontSize(size);
                this.setFontSize(size)?;

                // Reset font size also resets the manual adjustment state
                // self.font_size_adjusted = false;
                this.font_size_adjusted = false;
            }

            // .set_font_size => |points| {
            binding::ScopedSurface::SetFontSize(points) => {
                // log.debug("set font size={d}", .{points});
                log::debug!(target: LOG_SCOPE, "set font size={points}");

                // var size = self.font_size;
                let mut size = this.font_size;
                // size.points = std.math.clamp(points, 1.0, 255.0);
                size.points = points.clamp(1.0, 255.0);
                // try self.setFontSize(size);
                this.setFontSize(size)?;

                // Mark that we manually adjusted the font size
                // self.font_size_adjusted = true;
                this.font_size_adjusted = true;
            }

            // .prompt_surface_title => return try self.rt_app.performAction(
            //     .{ .surface = self },
            //     .prompt_title,
            //     .surface,
            // ),
            binding::ScopedSurface::PromptSurfaceTitle => {
                return Ok(unsafe { &*this.rt_app }.perform_action(
                    apprt::Target::surface_ref(&*this),
                    apprt::Action::PromptTitle(PromptTitle::Surface),
                )?);
            }

            // .prompt_tab_title => return try self.rt_app.performAction(
            //     .{ .surface = self },
            //     .prompt_title,
            //     .tab,
            // ),
            binding::ScopedSurface::PromptTabTitle => {
                return Ok(unsafe { &*this.rt_app }.perform_action(
                    apprt::Target::surface_ref(&*this),
                    apprt::Action::PromptTitle(PromptTitle::Tab),
                )?);
            }

            // .set_surface_title => |v| {
            binding::ScopedSurface::SetSurfaceTitle(v) => {
                // const title = try self.alloc.dupeZ(u8, v);
                // defer self.alloc.free(title);
                //
                // PORT NOTE: the `dupeZ` NUL-termination collapses —
                // `SetTitle.title` models `[:0]const u8` as borrowed `&[u8]`
                // (the sentinel is deferred C-ABI pinning, apprt-api action.rs),
                // so the owned copy is unnecessary and the borrowed binding
                // bytes are passed directly to the synchronous performAction.
                // return try self.rt_app.performAction(
                //     .{ .surface = self },
                //     .set_title,
                //     .{ .title = title },
                // );
                return Ok(unsafe { &*this.rt_app }.perform_action(
                    apprt::Target::surface_ref(&*this),
                    apprt::Action::SetTitle(SetTitle { title: v }),
                )?);
            }

            // .prompt_window_title => return try self.rt_app.performAction(
            //     .{ .surface = self },
            //     .prompt_title,
            //     .window,
            // ),
            binding::ScopedSurface::PromptWindowTitle => {
                return Ok(unsafe { &*this.rt_app }.perform_action(
                    apprt::Target::surface_ref(&*this),
                    apprt::Action::PromptTitle(PromptTitle::Window),
                )?);
            }

            // .set_tab_title => |v| {
            binding::ScopedSurface::SetTabTitle(v) => {
                // const title = try self.alloc.dupeZ(u8, v);
                // defer self.alloc.free(title);
                //
                // PORT NOTE: see `.set_surface_title` — the `dupeZ` collapses
                // into the deferred C-ABI sentinel; the borrowed bytes pass
                // directly.
                // return try self.rt_app.performAction(
                //     .{ .surface = self },
                //     .set_tab_title,
                //     .{ .title = title },
                // );
                return Ok(unsafe { &*this.rt_app }.perform_action(
                    apprt::Target::surface_ref(&*this),
                    apprt::Action::SetTabTitle(SetTitle { title: v }),
                )?);
            }

            // .set_window_title => |v| {
            binding::ScopedSurface::SetWindowTitle(v) => {
                // const title = try self.alloc.dupeZ(u8, v);
                // defer self.alloc.free(title);
                //
                // PORT NOTE: see `.set_surface_title` — the `dupeZ` collapses
                // into the deferred C-ABI sentinel; the borrowed bytes pass
                // directly.
                // return try self.rt_app.performAction(
                //     .{ .surface = self },
                //     .set_window_title,
                //     .{ .title = title },
                // );
                return Ok(unsafe { &*this.rt_app }.perform_action(
                    apprt::Target::surface_ref(&*this),
                    apprt::Action::SetWindowTitle(SetTitle { title: v }),
                )?);
            }

            // .clear_screen => {
            binding::ScopedSurface::ClearScreen => {
                // This is a duplicate of some of the logic in termio.clearScreen
                // but we need to do this here so we can know the answer before
                // we send the message. If the currently active screen is on the
                // alternate screen then clear screen does nothing so we want to
                // return false so the keybind can be unconsumed.
                // {
                //     self.renderer_state.mutex.lock();
                //     defer self.renderer_state.mutex.unlock();
                //     if (self.io.terminal.screens.active_key == .alternate) return false;
                // }
                {
                    unsafe { (*(*this.renderer_state.as_ptr()).mutex).lock() };
                    if unsafe { (*this.io.as_ptr()).terminal.screens.active_key }
                        == terminal::screen_set::Key::Alternate
                    {
                        unsafe { (*(*this.renderer_state.as_ptr()).mutex).unlock() };
                        return Ok(false);
                    }
                    unsafe { (*(*this.renderer_state.as_ptr()).mutex).unlock() };
                }

                // self.queueIo(.{
                //     .clear_screen = .{ .history = true },
                // }, .unlocked);
                this.queueIo(
                    termio::Message::ClearScreen { history: true },
                    termio::termio_impl::MutexState::Unlocked,
                );
            }

            // .scroll_to_top => {
            binding::ScopedSurface::ScrollToTop => {
                // self.queueIo(.{
                //     .scroll_viewport = .{ .top = {} },
                // }, .unlocked);
                this.queueIo(
                    termio::Message::ScrollViewport(terminal::terminal::ScrollViewport::Top),
                    termio::termio_impl::MutexState::Unlocked,
                );
            }

            // .scroll_to_bottom => {
            binding::ScopedSurface::ScrollToBottom => {
                // self.queueIo(.{
                //     .scroll_viewport = .{ .bottom = {} },
                // }, .unlocked);
                this.queueIo(
                    termio::Message::ScrollViewport(terminal::terminal::ScrollViewport::Bottom),
                    termio::termio_impl::MutexState::Unlocked,
                );
            }

            // .scroll_to_row => |n| {
            binding::ScopedSurface::ScrollToRow(n) => {
                // {
                //     self.renderer_state.mutex.lock();
                //     defer self.renderer_state.mutex.unlock();
                //     const t: *terminal.Terminal = self.renderer_state.terminal;
                //     t.screens.active.scroll(.{ .row = n });
                // }
                {
                    unsafe { (*(*this.renderer_state.as_ptr()).mutex).lock() };
                    let t: *mut Terminal = unsafe { (*this.renderer_state.as_ptr()).terminal };
                    unsafe { (*(*t).screens.active).scroll(terminal::screen::Scroll::Row(n)) };
                    unsafe { (*(*this.renderer_state.as_ptr()).mutex).unlock() };
                }

                // try self.queueRender();
                this.queueRender()?;
            }

            // .scroll_to_selection => {
            binding::ScopedSurface::ScrollToSelection => {
                // {
                //     self.renderer_state.mutex.lock();
                //     defer self.renderer_state.mutex.unlock();
                //     const sel = self.io.terminal.screens.active.selection orelse return false;
                //     const tl = sel.topLeft(self.io.terminal.screens.active);
                //     self.io.terminal.screens.active.scroll(.{ .pin = tl });
                // }
                {
                    unsafe { (*(*this.renderer_state.as_ptr()).mutex).lock() };
                    let sel =
                        match unsafe { (*(*this.io.as_ptr()).terminal.screens.active).selection } {
                            Some(sel) => sel,
                            None => {
                                unsafe { (*(*this.renderer_state.as_ptr()).mutex).unlock() };
                                return Ok(false);
                            }
                        };
                    let tl = sel.top_left(unsafe { &*(*this.io.as_ptr()).terminal.screens.active });
                    unsafe {
                        (*(*this.io.as_ptr()).terminal.screens.active)
                            .scroll(terminal::screen::Scroll::Pin(tl))
                    };
                    unsafe { (*(*this.renderer_state.as_ptr()).mutex).unlock() };
                }

                // try self.queueRender();
                this.queueRender()?;
            }

            // .scroll_page_up => {
            binding::ScopedSurface::ScrollPageUp => {
                // const rows: isize = @intCast(self.size.grid().rows);
                let rows: isize = this.size.grid().rows as isize;
                // self.queueIo(.{
                //     .scroll_viewport = .{ .delta = -1 * rows },
                // }, .unlocked);
                //
                // (`-1 * rows` -> `-rows`; clippy rejects the literal-multiply.)
                this.queueIo(
                    termio::Message::ScrollViewport(terminal::terminal::ScrollViewport::Delta(
                        -rows,
                    )),
                    termio::termio_impl::MutexState::Unlocked,
                );
            }

            // .scroll_page_down => {
            binding::ScopedSurface::ScrollPageDown => {
                // const rows: isize = @intCast(self.size.grid().rows);
                let rows: isize = this.size.grid().rows as isize;
                // self.queueIo(.{
                //     .scroll_viewport = .{ .delta = rows },
                // }, .unlocked);
                this.queueIo(
                    termio::Message::ScrollViewport(terminal::terminal::ScrollViewport::Delta(
                        rows,
                    )),
                    termio::termio_impl::MutexState::Unlocked,
                );
            }

            // .scroll_page_fractional => |fraction| {
            binding::ScopedSurface::ScrollPageFractional(fraction) => {
                // const rows: f32 = @floatFromInt(self.size.grid().rows);
                let rows: f32 = f32::from(this.size.grid().rows);
                // const delta: isize = @intFromFloat(@trunc(fraction * rows));
                // (f32 site: `f64::from` is exact, so the f64 helper checks the
                // identical value.)
                //
                // PORT NOTE (R13, UD62): a fraction the `isize` cannot hold means
                // "scroll to the end", which `scroll_viewport` clamps to the top
                // or the active area; the reference traps instead.
                let delta: isize =
                    isize_from_float_saturating(f64::from((fraction * rows).trunc()));
                // self.queueIo(.{
                //     .scroll_viewport = .{ .delta = delta },
                // }, .unlocked);
                this.queueIo(
                    termio::Message::ScrollViewport(terminal::terminal::ScrollViewport::Delta(
                        delta,
                    )),
                    termio::termio_impl::MutexState::Unlocked,
                );
            }

            // .scroll_page_lines => |lines| {
            binding::ScopedSurface::ScrollPageLines(lines) => {
                // self.queueIo(.{
                //     .scroll_viewport = .{ .delta = lines },
                // }, .unlocked);
                //
                // (`lines: i16` widens to the `delta: isize` field.)
                this.queueIo(
                    termio::Message::ScrollViewport(terminal::terminal::ScrollViewport::Delta(
                        isize::from(lines),
                    )),
                    termio::termio_impl::MutexState::Unlocked,
                );
            }

            // .jump_to_prompt => |delta| {
            binding::ScopedSurface::JumpToPrompt(delta) => {
                // self.queueIo(.{
                //     .jump_to_prompt = @intCast(delta),
                // }, .unlocked);
                this.queueIo(
                    termio::Message::JumpToPrompt(isize::from(delta)),
                    termio::termio_impl::MutexState::Unlocked,
                );
            }

            // .write_screen_file => |v| try self.writeScreenFile(
            //     .screen,
            //     v,
            // ),
            binding::ScopedSurface::WriteScreenFile(v) => {
                this.writeScreenFile(WriteScreenLoc::Screen, v)?
            }

            // .write_scrollback_file => |v| try self.writeScreenFile(
            //     .history,
            //     v,
            // ),
            binding::ScopedSurface::WriteScrollbackFile(v) => {
                this.writeScreenFile(WriteScreenLoc::History, v)?
            }

            // .write_selection_file => |v| try self.writeScreenFile(
            //     .selection,
            //     v,
            // ),
            binding::ScopedSurface::WriteSelectionFile(v) => {
                this.writeScreenFile(WriteScreenLoc::Selection, v)?
            }

            // .new_tab => return try self.rt_app.performAction(
            //     .{ .surface = self },
            //     .new_tab,
            //     {},
            // ),
            //
            // (N442: re-entrant, so by place through `self_`.)
            binding::ScopedSurface::NewTab => {
                return Ok(unsafe { &*(*self_).rt_app }.perform_action(
                    unsafe { apprt::Target::surface_ptr(self_) },
                    apprt::Action::NewTab,
                )?);
            }

            // .close_tab => |v| return try self.rt_app.performAction(
            //     .{ .surface = self },
            //     .close_tab,
            //     switch (v) {
            //         .this => .this,
            //         .other => .other,
            //         .right => .right,
            //     },
            // ),
            binding::ScopedSurface::CloseTab(v) => {
                return Ok(unsafe { &*this.rt_app }.perform_action(
                    apprt::Target::surface_ref(&*this),
                    apprt::Action::CloseTab(match v {
                        binding::CloseTabMode::This => apprt::action::CloseTabMode::This,
                        binding::CloseTabMode::Other => apprt::action::CloseTabMode::Other,
                        binding::CloseTabMode::Right => apprt::action::CloseTabMode::Right,
                    }),
                )?);
            }

            // inline .previous_tab,
            // .next_tab,
            // .last_tab,
            // .goto_tab,
            // => |v, tag| return try self.rt_app.performAction(
            //     .{ .surface = self },
            //     .goto_tab,
            //     switch (tag) {
            //         .previous_tab => .previous,
            //         .next_tab => .next,
            //         .last_tab => .last,
            //         .goto_tab => @enumFromInt(v),
            //         else => comptime unreachable,
            //     },
            // ),
            //
            // (the `inline` capture's `v` payload + `tag` — the ScopedSurface
            // value is re-projected to recover both, mirroring `|v, tag|`.)
            sfc @ (binding::ScopedSurface::PreviousTab
            | binding::ScopedSurface::NextTab
            | binding::ScopedSurface::LastTab
            | binding::ScopedSurface::GotoTab(_)) => {
                return Ok(unsafe { &*this.rt_app }.perform_action(
                    apprt::Target::surface_ref(&*this),
                    apprt::Action::GotoTab(match sfc {
                        binding::ScopedSurface::PreviousTab => apprt::action::GotoTab::PREVIOUS,
                        binding::ScopedSurface::NextTab => apprt::action::GotoTab::NEXT,
                        binding::ScopedSurface::LastTab => apprt::action::GotoTab::LAST,
                        // PORT NOTE: reference defect (R13, UPSTREAM-DEFECTS.md
                        // UD58). The reference's `@enumFromInt(v)` narrows the
                        // keybind's `usize` into `GotoTab = enum(c_int)`: above
                        // `c_int` max, ReleaseSafe panics and ReleaseFast
                        // truncates, so `goto_tab:4294967295` becomes `.previous`.
                        // The port rejects such a value where the keybind is
                        // parsed (`binding::GOTO_TAB_MAX`), as the parser rejects
                        // any other integer argument its type cannot hold. Here the
                        // value fits by construction, so the conversion is checked
                        // (R7) rather than `as`, which wrapped (LEDGER S42-F4).
                        binding::ScopedSurface::GotoTab(v) => apprt::action::GotoTab(
                            i32::try_from(v)
                                .expect("goto_tab index bounded by binding::GOTO_TAB_MAX at parse"),
                        ),
                        // else => comptime unreachable,
                        _ => unreachable!(),
                    }),
                )?);
            }

            // .move_tab => |position| return try self.rt_app.performAction(
            //     .{ .surface = self },
            //     .move_tab,
            //     .{ .amount = position },
            // ),
            binding::ScopedSurface::MoveTab(position) => {
                return Ok(unsafe { &*this.rt_app }.perform_action(
                    apprt::Target::surface_ref(&*this),
                    apprt::Action::MoveTab(apprt::action::MoveTab { amount: position }),
                )?);
            }

            // .move_tab_to_new_window => return try self.rt_app.performAction(
            //     .{ .surface = self },
            //     .move_tab_to_new_window,
            //     {},
            // ),
            binding::ScopedSurface::MoveTabToNewWindow => {
                return Ok(unsafe { &*this.rt_app }.perform_action(
                    apprt::Target::surface_ref(&*this),
                    apprt::Action::MoveTabToNewWindow,
                )?);
            }

            // .new_split => |direction| return try self.rt_app.performAction(
            //     .{ .surface = self },
            //     .new_split,
            //     switch (direction) {
            //         .right => .right,
            //         .left => .left,
            //         .down => .down,
            //         .up => .up,
            //         .auto => if (self.size.screen.width > self.size.screen.height)
            //             .right
            //         else
            //             .down,
            //     },
            // ),
            //
            // (N442: re-entrant, so by place through `self_`.)
            binding::ScopedSurface::NewSplit(direction) => {
                return Ok(unsafe { &*(*self_).rt_app }.perform_action(
                    unsafe { apprt::Target::surface_ptr(self_) },
                    apprt::Action::NewSplit(match direction {
                        binding::SplitDirection::Right => apprt::action::SplitDirection::Right,
                        binding::SplitDirection::Left => apprt::action::SplitDirection::Left,
                        binding::SplitDirection::Down => apprt::action::SplitDirection::Down,
                        binding::SplitDirection::Up => apprt::action::SplitDirection::Up,
                        binding::SplitDirection::Auto => {
                            if unsafe { (*self_).size.screen.width > (*self_).size.screen.height } {
                                apprt::action::SplitDirection::Right
                            } else {
                                apprt::action::SplitDirection::Down
                            }
                        }
                    }),
                )?);
            }

            // .goto_split => |direction| return try self.rt_app.performAction(
            //     .{ .surface = self },
            //     .goto_split,
            //     switch (direction) {
            //         inline else => |tag| @field(
            //             apprt.action.GotoSplit,
            //             @tagName(tag),
            //         ),
            //     },
            // ),
            //
            // (`inline else => @field(GotoSplit, @tagName(tag))` maps each
            // binding tag onto the same-named apprt `GotoSplit` variant.)
            binding::ScopedSurface::GotoSplit(direction) => {
                return Ok(unsafe { &*this.rt_app }.perform_action(
                    apprt::Target::surface_ref(&*this),
                    apprt::Action::GotoSplit(match direction {
                        binding::SplitFocusDirection::Previous => {
                            apprt::action::GotoSplit::Previous
                        }
                        binding::SplitFocusDirection::Next => apprt::action::GotoSplit::Next,
                        binding::SplitFocusDirection::Up => apprt::action::GotoSplit::Up,
                        binding::SplitFocusDirection::Left => apprt::action::GotoSplit::Left,
                        binding::SplitFocusDirection::Down => apprt::action::GotoSplit::Down,
                        binding::SplitFocusDirection::Right => apprt::action::GotoSplit::Right,
                    }),
                )?);
            }

            // .goto_window => |direction| return try self.rt_app.performAction(
            //     .{ .surface = self },
            //     .goto_window,
            //     switch (direction) {
            //         .previous => .previous,
            //         .next => .next,
            //     },
            // ),
            binding::ScopedSurface::GotoWindow(direction) => {
                return Ok(unsafe { &*this.rt_app }.perform_action(
                    apprt::Target::surface_ref(&*this),
                    apprt::Action::GotoWindow(match direction {
                        binding::GotoWindow::Previous => apprt::action::GotoWindow::Previous,
                        binding::GotoWindow::Next => apprt::action::GotoWindow::Next,
                    }),
                )?);
            }

            // .resize_split => |value| return try self.rt_app.performAction(
            //     .{ .surface = self },
            //     .resize_split,
            //     .{
            //         .amount = value[1],
            //         .direction = switch (value[0]) {
            //             inline else => |tag| @field(
            //                 apprt.action.ResizeSplit.Direction,
            //                 @tagName(tag),
            //             ),
            //         },
            //     },
            // ),
            binding::ScopedSurface::ResizeSplit(value) => {
                return Ok(unsafe { &*this.rt_app }.perform_action(
                    apprt::Target::surface_ref(&*this),
                    apprt::Action::ResizeSplit(apprt::action::ResizeSplit {
                        amount: value.1,
                        direction: match value.0 {
                            binding::SplitResizeDirection::Up => {
                                apprt::action::ResizeSplitDirection::Up
                            }
                            binding::SplitResizeDirection::Down => {
                                apprt::action::ResizeSplitDirection::Down
                            }
                            binding::SplitResizeDirection::Left => {
                                apprt::action::ResizeSplitDirection::Left
                            }
                            binding::SplitResizeDirection::Right => {
                                apprt::action::ResizeSplitDirection::Right
                            }
                        },
                    }),
                )?);
            }

            // .equalize_splits => return try self.rt_app.performAction(
            //     .{ .surface = self },
            //     .equalize_splits,
            //     {},
            // ),
            binding::ScopedSurface::EqualizeSplits => {
                return Ok(unsafe { &*this.rt_app }.perform_action(
                    apprt::Target::surface_ref(&*this),
                    apprt::Action::EqualizeSplits,
                )?);
            }

            // .toggle_split_zoom => return try self.rt_app.performAction(
            //     .{ .surface = self },
            //     .toggle_split_zoom,
            //     {},
            // ),
            binding::ScopedSurface::ToggleSplitZoom => {
                return Ok(unsafe { &*this.rt_app }.perform_action(
                    apprt::Target::surface_ref(&*this),
                    apprt::Action::ToggleSplitZoom,
                )?);
            }

            // .toggle_readonly => {
            binding::ScopedSurface::ToggleReadonly => {
                // self.readonly = !self.readonly;
                this.readonly = !this.readonly;
                // _ = try self.rt_app.performAction(
                //     .{ .surface = self },
                //     .readonly,
                //     if (self.readonly) .on else .off,
                // );
                let _ = unsafe { &*this.rt_app }.perform_action(
                    apprt::Target::surface_ref(&*this),
                    apprt::Action::Readonly(if this.readonly {
                        apprt::action::Readonly::On
                    } else {
                        apprt::action::Readonly::Off
                    }),
                )?;
                // return true;
                return Ok(true);
            }

            // .reset_window_size => return try self.rt_app.performAction(
            //     .{ .surface = self },
            //     .reset_window_size,
            //     {},
            // ),
            binding::ScopedSurface::ResetWindowSize => {
                return Ok(unsafe { &*this.rt_app }.perform_action(
                    apprt::Target::surface_ref(&*this),
                    apprt::Action::ResetWindowSize,
                )?);
            }

            // .toggle_maximize => return try self.rt_app.performAction(
            //     .{ .surface = self },
            //     .toggle_maximize,
            //     {},
            // ),
            binding::ScopedSurface::ToggleMaximize => {
                return Ok(unsafe { &*this.rt_app }.perform_action(
                    apprt::Target::surface_ref(&*this),
                    apprt::Action::ToggleMaximize,
                )?);
            }

            // .toggle_fullscreen => return try self.rt_app.performAction(
            //     .{ .surface = self },
            //     .toggle_fullscreen,
            //     switch (self.config.macos_non_native_fullscreen) {
            //         .false => .native,
            //         .true => .macos_non_native,
            //         .@"visible-menu" => .macos_non_native_visible_menu,
            //         .@"padded-notch" => .macos_non_native_padded_notch,
            //     },
            // ),
            binding::ScopedSurface::ToggleFullscreen => {
                return Ok(unsafe { &*this.rt_app }.perform_action(
                    apprt::Target::surface_ref(&*this),
                    apprt::Action::ToggleFullscreen(
                        match this.config.macos_non_native_fullscreen {
                            configpkg::value_types::NonNativeFullscreen::False => {
                                apprt::action::Fullscreen::Native
                            }
                            configpkg::value_types::NonNativeFullscreen::True => {
                                apprt::action::Fullscreen::MacosNonNative
                            }
                            configpkg::value_types::NonNativeFullscreen::VisibleMenu => {
                                apprt::action::Fullscreen::MacosNonNativeVisibleMenu
                            }
                            configpkg::value_types::NonNativeFullscreen::PaddedNotch => {
                                apprt::action::Fullscreen::MacosNonNativePaddedNotch
                            }
                        },
                    ),
                )?);
            }

            // .toggle_window_decorations => return try self.rt_app.performAction(
            //     .{ .surface = self },
            //     .toggle_window_decorations,
            //     {},
            // ),
            binding::ScopedSurface::ToggleWindowDecorations => {
                return Ok(unsafe { &*this.rt_app }.perform_action(
                    apprt::Target::surface_ref(&*this),
                    apprt::Action::ToggleWindowDecorations,
                )?);
            }

            // .toggle_tab_overview => return try self.rt_app.performAction(
            //     .{ .surface = self },
            //     .toggle_tab_overview,
            //     {},
            // ),
            binding::ScopedSurface::ToggleTabOverview => {
                return Ok(unsafe { &*this.rt_app }.perform_action(
                    apprt::Target::surface_ref(&*this),
                    apprt::Action::ToggleTabOverview,
                )?);
            }

            // .toggle_window_float_on_top => return try self.rt_app.performAction(
            //     .{ .surface = self },
            //     .float_window,
            //     .toggle,
            // ),
            binding::ScopedSurface::ToggleWindowFloatOnTop => {
                return Ok(unsafe { &*this.rt_app }.perform_action(
                    apprt::Target::surface_ref(&*this),
                    apprt::Action::FloatWindow(apprt::action::FloatWindow::Toggle),
                )?);
            }

            // .toggle_secure_input => return try self.rt_app.performAction(
            //     .{ .surface = self },
            //     .secure_input,
            //     .toggle,
            // ),
            binding::ScopedSurface::ToggleSecureInput => {
                return Ok(unsafe { &*this.rt_app }.perform_action(
                    apprt::Target::surface_ref(&*this),
                    apprt::Action::SecureInput(SecureInput::Toggle),
                )?);
            }

            // .toggle_mouse_reporting => {
            binding::ScopedSurface::ToggleMouseReporting => {
                // self.config.mouse_reporting = !self.config.mouse_reporting;
                this.config.mouse_reporting = !this.config.mouse_reporting;
                // log.debug("mouse reporting toggled: {}", .{self.config.mouse_reporting});
                log::debug!(
                    target: LOG_SCOPE,
                    "mouse reporting toggled: {}",
                    this.config.mouse_reporting
                );
            }

            // .toggle_command_palette => return try self.rt_app.performAction(
            //     .{ .surface = self },
            //     .toggle_command_palette,
            //     {},
            // ),
            binding::ScopedSurface::ToggleCommandPalette => {
                return Ok(unsafe { &*this.rt_app }.perform_action(
                    apprt::Target::surface_ref(&*this),
                    apprt::Action::ToggleCommandPalette,
                )?);
            }

            // .toggle_background_opacity => return try self.rt_app.performAction(
            //     .{ .surface = self },
            //     .toggle_background_opacity,
            //     {},
            // ),
            binding::ScopedSurface::ToggleBackgroundOpacity => {
                return Ok(unsafe { &*this.rt_app }.perform_action(
                    apprt::Target::surface_ref(&*this),
                    apprt::Action::ToggleBackgroundOpacity,
                )?);
            }

            // .show_on_screen_keyboard => return try self.rt_app.performAction(
            //     .{ .surface = self },
            //     .show_on_screen_keyboard,
            //     {},
            // ),
            binding::ScopedSurface::ShowOnScreenKeyboard => {
                return Ok(unsafe { &*this.rt_app }.perform_action(
                    apprt::Target::surface_ref(&*this),
                    apprt::Action::ShowOnScreenKeyboard,
                )?);
            }

            // .select_all => {
            binding::ScopedSurface::SelectAll => {
                // self.renderer_state.mutex.lock();
                // defer self.renderer_state.mutex.unlock();
                unsafe { (*(*this.renderer_state.as_ptr()).mutex).lock() };

                // const sel = self.io.terminal.screens.active.selectAll();
                let sel = unsafe { (*(*this.io.as_ptr()).terminal.screens.active).select_all() };
                // if (sel) |s| {
                if let Some(s) = sel {
                    // try self.setSelectionAndCopy(s);
                    if let Err(err) = this.setSelectionAndCopy(s) {
                        unsafe { (*(*this.renderer_state.as_ptr()).mutex).unlock() };
                        return Err(err);
                    }
                    // try self.queueRender();
                    if let Err(err) = this.queueRender() {
                        unsafe { (*(*this.renderer_state.as_ptr()).mutex).unlock() };
                        return Err(err.into());
                    }
                }
                unsafe { (*(*this.renderer_state.as_ptr()).mutex).unlock() };
            }

            // .inspector => |mode| return try self.rt_app.performAction(
            //     .{ .surface = self },
            //     .inspector,
            //     switch (mode) {
            //         inline else => |tag| @field(
            //             apprt.action.Inspector,
            //             @tagName(tag),
            //         ),
            //     },
            // ),
            binding::ScopedSurface::Inspector(mode) => {
                return Ok(unsafe { &*this.rt_app }.perform_action(
                    apprt::Target::surface_ref(&*this),
                    apprt::Action::Inspector(match mode {
                        binding::InspectorMode::Toggle => apprt::action::Inspector::Toggle,
                        binding::InspectorMode::Show => apprt::action::Inspector::Show,
                        binding::InspectorMode::Hide => apprt::action::Inspector::Hide,
                    }),
                )?);
            }

            // .close_surface => self.close(),
            //
            // (N465: raw-self; `this` is dead in this arm and after the match.)
            binding::ScopedSurface::CloseSurface => unsafe { Self::close(self_) },

            // .close_window => return try self.rt_app.performAction(
            //     .{ .surface = self },
            //     .close_window,
            //     {},
            // ),
            binding::ScopedSurface::CloseWindow => {
                return Ok(unsafe { &*this.rt_app }.perform_action(
                    apprt::Target::surface_ref(&*this),
                    apprt::Action::CloseWindow,
                )?);
            }

            // inline .activate_key_table,
            // .activate_key_table_once,
            // => |name, tag| {
            //
            // (the `inline` capture's `name` payload + `tag` — recovered by
            // re-projecting the ScopedSurface value, mirroring `|name, tag|`.)
            sfc @ (binding::ScopedSurface::ActivateKeyTable(_)
            | binding::ScopedSurface::ActivateKeyTableOnce(_)) => {
                let (name, is_once): (&[u8], bool) = match sfc {
                    binding::ScopedSurface::ActivateKeyTable(name) => (name, false),
                    // tag == .activate_key_table_once
                    binding::ScopedSurface::ActivateKeyTableOnce(name) => (name, true),
                    _ => unreachable!(),
                };

                // Look up the table in our config
                // const set = self.config.keybind.tables.getPtr(name) orelse {
                //     log.debug("key table not found: {s}", .{name});
                //     return false;
                // };
                //
                // PORT NOTE: `getPtr` (`?*Set`) -> `get` (`?&Set` cast to a
                // raw `*const Set`) — the stack only STORES and compares the
                // pointer, never mutates through it, so the shared borrow is
                // sufficient. The `Vec<u8>`-keyed map takes `&Vec<u8>`; the
                // `[]const u8` name is copied into a lookup key (a small
                // temporary, behavior-preserving).
                let set: *const binding::Set = match this.config.keybind.tables.get(&name.to_vec())
                {
                    Some(s) => s as *const binding::Set,
                    None => {
                        // log.debug("key table not found: {s}", .{name});
                        log::debug!(
                            target: LOG_SCOPE,
                            "key table not found: {}",
                            String::from_utf8_lossy(name)
                        );
                        return Ok(false);
                    }
                };

                // If this is the same table as is currently active, then
                // do nothing.
                // if (self.keyboard.table_stack.items.len > 0) {
                if !this.keyboard.table_stack.is_empty() {
                    // const items = self.keyboard.table_stack.items;
                    // const active = items[items.len - 1].set;
                    let active = this.keyboard.table_stack[this.keyboard.table_stack.len() - 1].set;
                    // if (active == set) {
                    //     log.debug("ignoring duplicate activate table: {s}", .{name});
                    //     return false;
                    // }
                    if std::ptr::eq(active, set) {
                        // log.debug("ignoring duplicate activate table: {s}", .{name});
                        log::debug!(
                            target: LOG_SCOPE,
                            "ignoring duplicate activate table: {}",
                            String::from_utf8_lossy(name)
                        );
                        return Ok(false);
                    }
                }

                // If we're already at the max, ignore it.
                // if (self.keyboard.table_stack.items.len >= max_active_key_tables) {
                //     log.info(
                //         "ignoring activate table, max depth reached: {s}",
                //         .{name},
                //     );
                //     return false;
                // }
                if this.keyboard.table_stack.len() >= MAX_ACTIVE_KEY_TABLES {
                    // log.info("ignoring activate table, max depth reached: {s}", .{name});
                    log::info!(
                        target: LOG_SCOPE,
                        "ignoring activate table, max depth reached: {}",
                        String::from_utf8_lossy(name)
                    );
                    return Ok(false);
                }

                // Add the table to the stack.
                // try self.keyboard.table_stack.append(self.alloc, .{
                //     .set = set,
                //     .once = tag == .activate_key_table_once,
                // });
                this.keyboard
                    .table_stack
                    .push(TableStackEntry { set, once: is_once });

                // Notify the UI.
                // _ = self.rt_app.performAction(
                //     .{ .surface = self },
                //     .key_table,
                //     .{ .activate = name },
                // ) catch |err| {
                //     log.warn(
                //         "failed to notify app of key table err={}",
                //         .{err},
                //     );
                // };
                if let Err(err) = unsafe { &*this.rt_app }.perform_action(
                    apprt::Target::surface_ref(&*this),
                    apprt::Action::KeyTable(KeyTable::Activate(name)),
                ) {
                    // log.warn("failed to notify app of key table err={}", .{err});
                    log::warn!(target: LOG_SCOPE, "failed to notify app of key table err={err:?}");
                }

                // log.debug("key table activated: {s}", .{name});
                log::debug!(
                    target: LOG_SCOPE,
                    "key table activated: {}",
                    String::from_utf8_lossy(name)
                );
            }

            // .deactivate_key_table => {
            binding::ScopedSurface::DeactivateKeyTable => {
                // switch (self.keyboard.table_stack.items.len) {
                match this.keyboard.table_stack.len() {
                    // No key table active. This does nothing.
                    // 0 => return false,
                    0 => return Ok(false),

                    // Final key table active, clear our state.
                    // 1 => self.keyboard.table_stack.clearAndFree(self.alloc),
                    //
                    // (`clearAndFree` — drop the storage too, the deinit
                    // mem::take precedent.)
                    1 => drop(std::mem::take(&mut this.keyboard.table_stack)),

                    // Restore the prior key table. We don't free any memory in
                    // this case because we assume it will be freed later when
                    // we finish our key table.
                    // else => _ = self.keyboard.table_stack.pop(),
                    _ => {
                        let _ = this.keyboard.table_stack.pop();
                    }
                }

                // Notify the UI.
                // _ = self.rt_app.performAction(
                //     .{ .surface = self },
                //     .key_table,
                //     .deactivate,
                // ) catch |err| {
                //     log.warn(
                //         "failed to notify app of key table err={}",
                //         .{err},
                //     );
                // };
                if let Err(err) = unsafe { &*this.rt_app }.perform_action(
                    apprt::Target::surface_ref(&*this),
                    apprt::Action::KeyTable(KeyTable::Deactivate),
                ) {
                    // log.warn("failed to notify app of key table err={}", .{err});
                    log::warn!(target: LOG_SCOPE, "failed to notify app of key table err={err:?}");
                }
            }

            // .deactivate_all_key_tables => {
            //     return try self.deactivateAllKeyTables();
            // },
            binding::ScopedSurface::DeactivateAllKeyTables => {
                return this.deactivateAllKeyTables();
            }

            // .end_key_sequence => {
            binding::ScopedSurface::EndKeySequence => {
                // End the key sequence and flush queued keys to the terminal,
                // but don't encode the key that triggered this action. This
                // will do that because leaf keys (keys with bindings) aren't
                // in the queued encoding list.
                // self.endKeySequence(.flush, .retain);
                this.endKeySequence(KeySequenceQueued::Flush, KeySequenceMemory::Retain);
            }

            // .crash => |location| switch (location) {
            binding::ScopedSurface::Crash(location) => match location {
                // .main => @panic("crash binding action, crashing intentionally"),
                binding::CrashThread::Main => {
                    panic!("crash binding action, crashing intentionally")
                }

                // .render => {
                binding::CrashThread::Render => {
                    // _ = self.renderer_thread.mailbox.push(.{ .crash = {} }, .{ .forever = {} });
                    let _ = unsafe { &*(*this.renderer_thread.as_ptr()).mailbox }
                        .push(rendererpkg::Message::Crash, Timeout::Forever);
                    // self.queueRender() catch |err| {
                    //     // Not a big deal if this fails.
                    //     log.warn("failed to notify renderer of crash message err={}", .{err});
                    // };
                    if let Err(err) = this.queueRender() {
                        // Not a big deal if this fails.
                        // log.warn("failed to notify renderer of crash message err={}", .{err});
                        log::warn!(
                            target: LOG_SCOPE,
                            "failed to notify renderer of crash message err={err:?}"
                        );
                    }
                }

                // .io => self.queueIo(.{ .crash = {} }, .unlocked),
                binding::CrashThread::Io => this.queueIo(
                    termio::Message::Crash,
                    termio::termio_impl::MutexState::Unlocked,
                ),
            },

            // .adjust_selection => |direction| {
            binding::ScopedSurface::AdjustSelection(direction) => {
                // self.renderer_state.mutex.lock();
                // defer self.renderer_state.mutex.unlock();
                unsafe { (*(*this.renderer_state.as_ptr()).mutex).lock() };

                // const screen: *terminal.Screen = self.io.terminal.screens.active;
                let screen: *mut terminal::screen::Screen =
                    unsafe { (*this.io.as_ptr()).terminal.screens.active };
                // const sel = if (screen.selection) |*sel| sel else {
                //     // If we don't have a selection we do not perform this
                //     // action, allowing the keybind to fall through to the
                //     // terminal.
                //     return false;
                // };
                //
                // PORT NOTE: the reference `sel` is a `*Selection` aliasing
                // `screen.selection` which `sel.adjust(screen, ...)` mutates
                // in place while ALSO passing `screen` as `&Screen` — a
                // `&mut`-field / `&`-parent overlap that is UB in Rust
                // (rust-mut-args memory). We copy the selection out
                // (`Selection: Copy`), adjust the independent copy against
                // `&*screen`, then write it back — behavior-identical because
                // `adjust` reads only `s.pages`, never `s.selection`.
                let mut sel = match unsafe { (*screen).selection } {
                    Some(sel) => sel,
                    None => {
                        unsafe { (*(*this.renderer_state.as_ptr()).mutex).unlock() };
                        return Ok(false);
                    }
                };
                // sel.adjust(screen, switch (direction) {
                //     .left => .left,
                //     .right => .right,
                //     .up => .up,
                //     .down => .down,
                //     .page_up => .page_up,
                //     .page_down => .page_down,
                //     .home => .home,
                //     .end => .end,
                //     .beginning_of_line => .beginning_of_line,
                //     .end_of_line => .end_of_line,
                // });
                sel.adjust(
                    unsafe { &*screen },
                    match direction {
                        binding::AdjustSelection::Left => terminal::selection::Adjustment::Left,
                        binding::AdjustSelection::Right => terminal::selection::Adjustment::Right,
                        binding::AdjustSelection::Up => terminal::selection::Adjustment::Up,
                        binding::AdjustSelection::Down => terminal::selection::Adjustment::Down,
                        binding::AdjustSelection::PageUp => terminal::selection::Adjustment::PageUp,
                        binding::AdjustSelection::PageDown => {
                            terminal::selection::Adjustment::PageDown
                        }
                        binding::AdjustSelection::Home => terminal::selection::Adjustment::Home,
                        binding::AdjustSelection::End => terminal::selection::Adjustment::End,
                        binding::AdjustSelection::BeginningOfLine => {
                            terminal::selection::Adjustment::BeginningOfLine
                        }
                        binding::AdjustSelection::EndOfLine => {
                            terminal::selection::Adjustment::EndOfLine
                        }
                    },
                );
                // (write the adjusted copy back — the reference `*Selection`
                // mutated `screen.selection` in place.)
                unsafe { (*screen).selection = Some(sel) };

                // If the selection endpoint is outside of the current viewpoint,
                // scroll it in to view. Note we always specifically use sel.end
                // because that is what adjust modifies.
                // scroll: {
                'scroll: {
                    // const viewport_tl = screen.pages.getTopLeft(.viewport);
                    let viewport_tl =
                        unsafe { (*screen).pages.get_top_left(terminal::point::Tag::Viewport) };
                    // const viewport_br = screen.pages.getBottomRight(.viewport).?;
                    let viewport_br = unsafe {
                        (*screen)
                            .pages
                            .get_bottom_right(terminal::point::Tag::Viewport)
                    }
                    .unwrap();
                    // if (sel.end().isBetween(viewport_tl, viewport_br))
                    //     break :scroll;
                    if sel.end().is_between(viewport_tl, viewport_br) {
                        break 'scroll;
                    }

                    // Our end point is not within the viewport. If the end
                    // point is after the br then we need to adjust the end so
                    // that it is at the bottom right of the viewport.
                    // const target = if (sel.end().before(viewport_tl))
                    //     sel.end()
                    // else
                    //     sel.end().up(screen.pages.rows - 1) orelse sel.end();
                    let target = if sel.end().before(viewport_tl) {
                        sel.end()
                    } else {
                        sel.end()
                            .up(usize::from(unsafe { (*screen).pages.rows }) - 1)
                            .unwrap_or(sel.end())
                    };

                    // screen.scroll(.{ .pin = target });
                    unsafe { (*screen).scroll(terminal::screen::Scroll::Pin(target)) };
                }

                // Queue a render so its shown
                // screen.dirty.selection = true;
                unsafe { (*screen).dirty.selection = true };
                // try self.queueRender();
                if let Err(err) = this.queueRender() {
                    unsafe { (*(*this.renderer_state.as_ptr()).mutex).unlock() };
                    return Err(err.into());
                }
                unsafe { (*(*this.renderer_state.as_ptr()).mutex).unlock() };
            }
        }

        // return true;
        Ok(true)
    }

    // fn writeScreenFile(
    //     self: *Surface,
    //     loc: WriteScreenLoc,
    //     write_screen: input.Binding.Action.WriteScreen,
    // ) !void {
    //
    // PORT NOTE (chunk C14b forward-declared this skeleton + the file-scope
    // [`WriteScreenLoc`] so C14b's `.write_screen_file`/`.write_scrollback_file`/
    // `.write_selection_file` binding arms compile against a real callee; C15
    // fills the BODY IN PLACE, Surface.zig:5651-5777).
    fn writeScreenFile(
        &mut self,
        loc: WriteScreenLoc,
        write_screen: binding::WriteScreen,
    ) -> Result<(), Error> {
        // Create a temporary directory to store our scrollback.
        // var tmp_dir = try internal_os.TempDir.init();
        // var retain_tmp_dir = false;
        // defer if (retain_tmp_dir) tmp_dir.close(.retain) else tmp_dir.deinit();
        //
        // PORT NOTE (`2b32b5b75`): `internal_os.TempDir` is the struct-file
        // `os/TempDir.zig`, ported to the `tempdir` module
        // (`internal_os::tempdir::TempDir`). The reference defer ALWAYS closes
        // the directory handles; the `retain_tmp_dir` flag — set only after
        // successful dispatch — selects between deleting the tree
        // (`deinit` == `close(.delete)`) and retaining the temp file on disk
        // for the copy/open/paste consumer (`close(.retain)`). [`TmpDirGuard`]'s
        // `Drop` is that defer: every `?` error exit and the null-selection
        // exit run it with `retain` still false (the delete arm), and only the
        // success-with-data path flips `retain` (the
        // [`ThreadStateGuard`] Drop-guard precedent).
        struct TmpDirGuard {
            td: internal_os::tempdir::TempDir,
            retain: bool,
        }
        impl Drop for TmpDirGuard {
            fn drop(&mut self) {
                if self.retain {
                    self.td.close(internal_os::tempdir::CloseMode::Retain);
                } else {
                    self.td.deinit();
                }
            }
        }
        let mut tmp_dir = TmpDirGuard {
            td: internal_os::tempdir::TempDir::init()?,
            retain: false,
        };

        // var filename_buf: [std.fs.max_path_bytes]u8 = undefined;
        // const filename = try std.fmt.bufPrint(
        //     &filename_buf,
        //     "{s}.{s}",
        //     .{
        //         @tagName(loc),
        //         switch (write_screen.emit) {
        //             .plain, .vt => "txt",
        //             .html => "html",
        //         },
        //     },
        // );
        //
        // (`bufPrint` into a fixed [max_path_bytes]u8 -> an owned `String`; the
        // SURFACE_ID `bufPrint` precedent. `@tagName(loc)` is the lowercase
        // enum-tag name.)
        let filename = format!(
            "{}.{}",
            match loc {
                WriteScreenLoc::Screen => "screen",
                WriteScreenLoc::History => "history",
                WriteScreenLoc::Selection => "selection",
            },
            match write_screen.emit {
                binding::WriteScreenFormat::Plain | binding::WriteScreenFormat::Vt => "txt",
                binding::WriteScreenFormat::Html => "html",
            },
        );

        // Open our scrollback file
        // var file = try tmp_dir.dir.createFile(
        //     filename,
        //     switch (builtin.os.tag) {
        //         .windows => .{},
        //         else => .{ .mode = 0o600 },
        //     },
        // );
        // defer file.close();
        //
        // (the rustty-os `createFile(sub_path, read)` collapses the reference
        // `CreateFlags` to `read` — `read = false` in BOTH reference arms; the
        // `.mode = 0o600` security bit is re-applied via `set_permissions`
        // (fchmod on the open handle) on unix — the reference's non-windows
        // `else` arm; file modes are a no-op concept on windows. Byte-identical
        // END state for a fresh temp file (`cfg(unix)` gates the unix-only
        // `PermissionsExt`, matching the `path_to_bytes`/`os_str_from_bytes`
        // helpers). `file.close()` -> the `File` drops at scope end, closing it
        // — BEFORE `tmp_dir` (reverse-declaration drop == Zig defer LIFO).)
        let file = tmp_dir.td.dir().createFile(&filename, false)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
        }

        // Screen.dumpString writes byte-by-byte, so buffer it
        // var buf: [4096]u8 = undefined;
        // var file_writer = file.writer(&buf);
        // var buf_writer = &file_writer.interface;
        //
        // (Zig 0.15's buffered `Writer` -> a 4096-capacity `BufWriter`.)
        let mut buf_writer = std::io::BufWriter::with_capacity(4096, file);

        // Write the scrollback contents. This requires a lock.
        // {
        {
            // self.renderer_state.mutex.lock();
            // defer self.renderer_state.mutex.unlock();
            //
            // (guard-less zigstd Mutex — the defer is the explicit unlock at
            // every exit below.)
            unsafe { (*(*self.renderer_state.as_ptr()).mutex).lock() };

            // We only dump history if we have history. We still keep
            // the file and write the empty file to the pty so that this
            // command always works on the primary screen.
            // const pages = &self.io.terminal.screens.active.pages;
            let pages = unsafe { &(*(*self.io.as_ptr()).terminal.screens.active).pages };
            // const sel_: ?terminal.Selection = switch (loc) {
            let sel_: Option<terminal::selection::Selection> = match loc {
                // .history => history: {
                WriteScreenLoc::History => 'history: {
                    // We do not support this for alternate screens
                    // because they don't have scrollback anyways.
                    // if (self.io.terminal.screens.active_key == .alternate) {
                    //     break :history null;
                    // }
                    if unsafe { (*self.io.as_ptr()).terminal.screens.active_key }
                        == terminal::screen_set::Key::Alternate
                    {
                        break 'history None;
                    }

                    // break :history terminal.Selection.init(
                    //     pages.getTopLeft(.history),
                    //     pages.getBottomRight(.history) orelse
                    //         break :history null,
                    //     false,
                    // );
                    let br = match pages.get_bottom_right(terminal::point::Tag::History) {
                        Some(v) => v,
                        None => break 'history None,
                    };
                    Some(terminal::selection::Selection::init(
                        pages.get_top_left(terminal::point::Tag::History),
                        br,
                        false,
                    ))
                }

                // .screen => screen: {
                WriteScreenLoc::Screen => 'screen: {
                    // break :screen terminal.Selection.init(
                    //     pages.getTopLeft(.screen),
                    //     pages.getBottomRight(.screen) orelse
                    //         break :screen null,
                    //     false,
                    // );
                    let br = match pages.get_bottom_right(terminal::point::Tag::Screen) {
                        Some(v) => v,
                        None => break 'screen None,
                    };
                    Some(terminal::selection::Selection::init(
                        pages.get_top_left(terminal::point::Tag::Screen),
                        br,
                        false,
                    ))
                }

                // .selection => self.io.terminal.screens.active.selection,
                WriteScreenLoc::Selection => unsafe {
                    (*(*self.io.as_ptr()).terminal.screens.active).selection
                },
            };

            // const sel = sel_ orelse {
            //     // If we have no selection we have no data so we do nothing.
            //     return;
            // };
            let sel = match sel_ {
                Some(v) => v,
                None => {
                    // If we have no selection we have no data so we do nothing.
                    // (the null-selection exit: unlock, then return — `tmp_dir`
                    // drops with `retain` false, so its `Drop` runs the
                    // reference defer's `tmp_dir.deinit()` arm.)
                    unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
                    return Ok(());
                }
            };

            // const ScreenFormatter = terminal.formatter.ScreenFormatter;
            use rustty_vt::formatter::ScreenFormatter;
            // var formatter: ScreenFormatter = .init(self.io.terminal.screens.active, .{
            let mut formatter: ScreenFormatter = ScreenFormatter::init(
                unsafe { (*self.io.as_ptr()).terminal.screens.active },
                terminal::formatter::Options {
                    // .emit = switch (write_screen.emit) {
                    //     .plain => .plain,
                    //     .vt => .vt,
                    //     .html => .html,
                    // },
                    emit: match write_screen.emit {
                        binding::WriteScreenFormat::Plain => terminal::formatter::Format::Plain,
                        binding::WriteScreenFormat::Vt => terminal::formatter::Format::Vt,
                        binding::WriteScreenFormat::Html => terminal::formatter::Format::Html,
                    },
                    // .unwrap = true,
                    unwrap: true,
                    // .trim = false,
                    trim: false,
                    // (the reference omits `codepoint_map`, i.e. the field
                    // default `.{}` = present-but-empty — the formatter.rs
                    // `Options::base` note.)
                    codepoint_map: Some(Vec::new()),
                    // .background = self.io.terminal.colors.background.get(),
                    background: unsafe { (*self.io.as_ptr()).terminal.colors.background.get() },
                    // .foreground = self.io.terminal.colors.foreground.get(),
                    foreground: unsafe { (*self.io.as_ptr()).terminal.colors.foreground.get() },
                    // .palette = &self.io.terminal.colors.palette.current,
                    palette: Some(unsafe {
                        &raw const (*self.io.as_ptr()).terminal.colors.palette.current
                    }),
                },
            );
            // formatter.content = .{ .selection = sel.ordered(
            //     self.io.terminal.screens.active,
            //     .forward,
            // ) };
            formatter.content = terminal::formatter::Content::Selection(Some(sel.ordered(
                unsafe { &*(*self.io.as_ptr()).terminal.screens.active },
                terminal::selection::Order::Forward,
            )));
            // try formatter.format(buf_writer);
            if let Err(err) = formatter.format(&mut buf_writer) {
                // (the deferred mutex unlock — the error exit; `tmp_dir` stays
                // armed so its `Drop` reproduces `errdefer tmp_dir.deinit()`.)
                unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
                return Err(err.into());
            }

            // (the deferred mutex unlock — the normal block exit.)
            unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
        }
        // try buf_writer.flush();
        {
            use std::io::Write as _;
            buf_writer.flush()?;
        }

        // Get the final path
        // var path_buf: [std.fs.max_path_bytes]u8 = undefined;
        // const path = try tmp_dir.dir.realpath(filename, &path_buf);
        //
        // (rustty-os `realpath` returns an owning `PathBuf`; `path` is its raw
        // bytes — never a lossy String, R3.)
        let path_buf = tmp_dir.td.dir().realpath(&filename)?;
        let path: &[u8] = path_to_bytes(&path_buf);

        // switch (write_screen.action) {
        match write_screen.action {
            // .copy => {
            binding::WriteScreenAction::Copy => {
                // const pathZ = try self.alloc.dupeZ(u8, path);
                // defer self.alloc.free(pathZ);
                //
                // PORT NOTE: `dupeZ` NUL-terminates for the C-ABI; Rustty
                // `ClipboardContent` borrows `&[u8]` and `setClipboard` copies
                // synchronously ("The data is immediately copied"), so `path` is
                // passed directly (the `PwdC` deferred-pinning precedent).
                // try self.rt_surface.setClipboard(.standard, &.{.{
                //     .mime = "text/plain",
                //     .data = pathZ,
                // }}, false);
                unsafe { &*self.rt_surface }.set_clipboard(
                    apprt::Clipboard::Standard,
                    &[apprt::ClipboardContent {
                        mime: b"text/plain",
                        data: path,
                    }],
                    false,
                )?;
            }

            // .open => try self.openUrl(.{
            //     .kind = switch (write_screen.emit) {
            //         .plain, .vt => .text,
            //         .html => .html,
            //     },
            //     .url = path,
            // }),
            binding::WriteScreenAction::Open => {
                self.openUrl(OpenUrl {
                    kind: match write_screen.emit {
                        binding::WriteScreenFormat::Plain | binding::WriteScreenFormat::Vt => {
                            OpenUrlKind::Text
                        }
                        binding::WriteScreenFormat::Html => OpenUrlKind::Html,
                    },
                    url: path,
                })?;
            }

            // .paste => self.queueIo(try termio.Message.writeReq(
            //     self.alloc,
            //     path,
            // ), .unlocked),
            binding::WriteScreenAction::Paste => {
                self.queueIo(
                    termio::Message::writeReq(path),
                    termio::termio_impl::MutexState::Unlocked,
                );
            }
        }

        // The action accepted the path, so retain the file for its consumer.
        // retain_tmp_dir = true;
        //
        // (the guard drop now takes the `close(.retain)` arm — handles
        // released, the temp file left on disk for the copy/open/paste
        // consumer.)
        tmp_dir.retain = true;
        Ok(())
    }
}
