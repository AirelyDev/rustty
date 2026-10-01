//! PORTS: ghostty src/Surface.zig — the key-input cluster: `preeditCallback`,
//! `keyEventIsBinding`, `keyCallback`, `maybeHandleBinding`, `deactivateAllKeyTables`,
//! `catchAllIsIgnore`, `endKeySequence`, `encodeKey`, `encodeKeyOpts`, `textCallback`.

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
    /// Called to set the preedit state for character input. Preedit is used
    /// with dead key states, for example, when typing an accent character.
    /// This should be called with null to reset the preedit state.
    ///
    /// The core surface will NOT reset the preedit state on charCallback or
    /// keyCallback and we rely completely on the apprt implementation to track
    /// the preedit state correctly.
    ///
    /// The preedit input must be UTF-8 encoded.
    // pub fn preeditCallback(self: *Surface, preedit_: ?[]const u8) !void {
    pub fn preeditCallback(&mut self, preedit_: Option<&[u8]>) -> Result<(), Error> {
        // log.debug("text preeditCallback value={any}", .{preedit_});
        // (COMMENT-ONLY BY DESIGN: commented out in the ORACLE too, Surface.zig:2540.)

        // Crash metadata in case we crash in here
        // crash.sentry.thread_state = self.crashThreadState();
        crash::sentry::setThreadState(self.crashThreadState());
        // defer crash.sentry.thread_state = null;
        // ([ADV 6.1] — the Drop guard clears at every exit below, AFTER the
        // explicit mutex unlocks: the reference's LIFO defer order.)
        let _thread_state = ThreadStateGuard;

        // self.renderer_state.mutex.lock();
        // defer self.renderer_state.mutex.unlock();
        //
        // (guard-less zigstd Mutex — CORE_BLUEPRINT §3; the defer is the
        // explicit unlock at every exit below.)
        unsafe { (*(*self.renderer_state.as_ptr()).mutex).lock() };

        // We clear our selection when ANY OF:
        // 1. We have an existing preedit
        // 2. We have preedit text
        // if (self.renderer_state.preedit != null or
        //     preedit_ != null)
        // {
        if unsafe { (*self.renderer_state.as_ptr()).preedit.is_some() } || preedit_.is_some() {
            // if (self.config.selection_clear_on_typing) {
            if self.config.selection_clear_on_typing {
                // self.setSelection(null) catch {};
                let _ = self.setSelection(None);
            }
        }

        // We always clear our prior preedit
        // if (self.renderer_state.preedit) |p| {
        if let Some(p) = unsafe { (*self.renderer_state.as_ptr()).preedit.take() } {
            // self.alloc.free(p.codepoints);
            drop(p.codepoints);
            // self.renderer_state.preedit = null;
            // (the `take()` above.)
        }

        // Mark preedit dirty flag
        // self.io.terminal.flags.dirty.preedit = true;
        unsafe { (*self.io.as_ptr()).terminal.flags.dirty.preedit = true };

        // If we have no text, we're done. We queue a render in case we cleared
        // a prior preedit (likely).
        // const text = preedit_ orelse {
        //     try self.queueRender();
        //     return;
        // };
        let text = match preedit_ {
            Some(v) => v,
            None => {
                let result = self.queueRender();
                // (the deferred mutex unlock)
                unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
                return result.map_err(Error::from);
            }
        };

        // We convert the UTF-8 text to codepoints.
        // const view = try std.unicode.Utf8View.init(text);
        // var it = view.iterator();
        let view = match std::str::from_utf8(text) {
            Ok(v) => v,
            Err(err) => {
                // (the deferred mutex unlock — the error exit)
                unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
                return Err(err.into());
            }
        };
        let it = view.chars();

        // Allocate the codepoints slice
        // const Codepoint = rendererpkg.State.Preedit.Codepoint;
        //
        // (the reference nests it as `State.Preedit.Codepoint`; the port
        // hoists it to `state::Codepoint` — the state.rs PORT NOTE.)
        // var codepoints: std.ArrayListUnmanaged(Codepoint) = .{};
        // defer codepoints.deinit(self.alloc);
        //
        // (the defer covers the toOwnedSlice-emptied list on every exit —
        // Vec ownership: moved into the Preedit below or dropped.)
        let mut codepoints: Vec<rendererpkg::state::Codepoint> = Vec::new();
        // while (it.nextCodepoint()) |cp| {
        for cp in it {
            // const width: usize = @intCast(unicode.table.get(cp).width);
            let width: usize = usize::from(unicode::table().get(cp as u32).width);

            // I've never seen a preedit text with a zero-width character. In
            // theory its possible but we can't really handle it right now.
            // Let's just ignore it.
            // if (width <= 0) continue;
            //
            // (`<= 0` on the unsigned width is `== 0`.)
            if width == 0 {
                continue;
            }

            // try codepoints.append(
            //     self.alloc,
            //     .{ .codepoint = cp, .wide = width >= 2 },
            // );
            codepoints.push(rendererpkg::state::Codepoint {
                codepoint: cp as u32,
                wide: width >= 2,
            });
        }

        // If we have no codepoints, then we're done.
        // if (codepoints.items.len == 0) {
        if codepoints.is_empty() {
            // try self.queueRender();
            let result = self.queueRender();
            // (the deferred mutex unlock)
            unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
            // return;
            return result.map_err(Error::from);
        }

        // self.renderer_state.preedit = .{
        //     .codepoints = try codepoints.toOwnedSlice(self.alloc),
        // };
        //
        // (`toOwnedSlice` — the `Vec` IS the owned slice; the state.rs
        // Preedit PORT NOTE.)
        unsafe {
            (*self.renderer_state.as_ptr()).preedit =
                Some(rendererpkg::state::Preedit { codepoints })
        };
        // try self.queueRender();
        let result = self.queueRender();
        // (the deferred mutex unlock — the normal exit)
        unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
        result.map_err(Error::from)
    }

    /// Returns true if the given key event would trigger a keybinding
    /// if it were to be processed. This is useful for determining if
    /// a key event should be sent to the terminal or not.
    ///
    /// Note that this function does not check if the binding itself
    /// is performable, only if the key event would trigger a binding.
    /// If a performable binding is found and the event is not performable,
    /// then Ghosty will act as though the binding does not exist.
    // pub fn keyEventIsBinding(
    //     self: *Surface,
    //     event_orig: input.KeyEvent,
    // ) ?input.Binding.Flags {
    pub fn keyEventIsBinding(&mut self, event_orig: KeyEvent) -> Option<binding::Flags> {
        // Apply key remappings for consistency with keyCallback
        // var event = event_orig;
        //
        // (PORT NOTE: `event_orig` moves into `event` — the Rust KeyEvent owns its
        // utf8 bytes; both `event_orig.mods` reads below still see the
        // pre-assignment mods, the RHS evaluates first.)
        let mut event = event_orig;
        // if (self.config.key_remaps.isRemapped(event_orig.mods)) {
        if self.config.key_remaps.is_remapped(event.mods) {
            // event.mods = self.config.key_remaps.apply(event_orig.mods);
            event.mods = self.config.key_remaps.apply(event.mods);
        }

        // switch (event.action) {
        match event.action {
            // .release => return null,
            key::Action::Release => return None,
            // .press, .repeat => {},
            key::Action::Press | key::Action::Repeat => {}
        }

        // Look up our entry
        // const entry: input.Binding.Set.Entry = entry: {
        //
        // PORT NOTE: the reference `Entry` is a key/value POINTER pair; the
        // ported `Set.getEvent` returns the entry INDEX (the app.rs keyEvent
        // precedent), so the entry is the `(set, index)` pair here —
        // `value_at`/`key_at` are the `entry.value_ptr.*`/`entry.key_ptr.*`
        // reads. The set side is a raw pointer because two of the three
        // sources (`sequence_set`, the table stack) already store raw
        // pointers into config-owned sets, exactly as the reference.
        let entry: (*const binding::Set, usize) = 'entry: {
            // If we're in a sequence, check the sequence set
            // if (self.keyboard.sequence_set) |set| {
            //     break :entry set.getEvent(event) orelse return null;
            // }
            if let Some(set) = self.keyboard.sequence_set {
                match unsafe { &*set }.get_event(&event) {
                    Some(v) => break 'entry (set, v),
                    None => return None,
                }
            }

            // Check active key tables (inner-most to outer-most)
            // const table_items = self.keyboard.table_stack.items;
            let table_items = &self.keyboard.table_stack;
            // for (0..table_items.len) |i| {
            for i in 0..table_items.len() {
                // const rev_i: usize = table_items.len - 1 - i;
                let rev_i: usize = table_items.len() - 1 - i;
                // if (table_items[rev_i].set.getEvent(event)) |entry| {
                //     break :entry entry;
                // }
                if let Some(entry) = unsafe { &*table_items[rev_i].set }.get_event(&event) {
                    break 'entry (table_items[rev_i].set, entry);
                }
            }

            // Check the root set
            // break :entry self.config.keybind.set.getEvent(event) orelse return null;
            match self.config.keybind.set.get_event(&event) {
                Some(v) => break 'entry (&self.config.keybind.set as *const binding::Set, v),
                None => return None,
            }
        };

        // Return flags based on the
        // return switch (entry.value_ptr.*) {
        match unsafe { &*entry.0 }.value_at(entry.1) {
            // .leader => .{},
            binding::Value::Leader(_) => Some(binding::Flags::default()),
            // inline .leaf, .leaf_chained => |v| v.flags,
            binding::Value::Leaf(v) => Some(v.flags),
            binding::Value::LeafChained(v) => Some(v.flags),
        }
        // };
    }

    /// Called for any key events. This handles keybindings, encoding and
    /// sending to the terminal, etc.
    // pub fn keyCallback(
    //     self: *Surface,
    //     event_orig: input.KeyEvent,
    // ) !InputEffect {
    //
    // (the inferred `!InputEffect` is the [`Error`] union.)
    //
    // PORT NOTE (N442, P7-W7 T5b): raw-self (`self: *Surface` has no aliasing
    // rule in Zig; a Rust `&mut self` receiver is a protected Unique for the
    // whole call). A keybind can run the embedder's action callback, which
    // re-enters this surface (`rustty_surface_inherited_config`); a protector
    // spanning it is UB. `this` is an UNPROTECTED local reborrow, dead before the
    // binding dispatch and taken afresh after it. Proof: `.port/phase-2/mirihar-n441`.
    ///
    /// # Safety
    /// `self_` is the stable Surface allocation (init's stability contract); no
    /// protected `&mut Surface` may span the call.
    pub unsafe fn keyCallback(
        self_: *mut Surface<A>,
        event_orig: KeyEvent,
    ) -> Result<InputEffect, Error> {
        let this = unsafe { &mut *self_ };
        // log.warn("text keyCallback event={}", .{event_orig});
        // (COMMENT-ONLY BY DESIGN: commented out in the ORACLE too, Surface.zig:2666.)

        // Apply key remappings to transform modifiers before any processing.
        // This allows users to remap modifier keys at the app level.
        // var event = event_orig;
        //
        // (PORT NOTE: the keyEventIsBinding move-and-read note above.)
        let mut event = event_orig;
        // if (self.config.key_remaps.isRemapped(event_orig.mods)) {
        if this.config.key_remaps.is_remapped(event.mods) {
            // event.mods = self.config.key_remaps.apply(event_orig.mods);
            event.mods = this.config.key_remaps.apply(event.mods);
        }

        // Crash metadata in case we crash in here
        // crash.sentry.thread_state = self.crashThreadState();
        crash::sentry::setThreadState(this.crashThreadState());
        // defer crash.sentry.thread_state = null;
        //
        // ([ADV 6.1] — the Drop guard clears at every exit below; declared
        // BEFORE the inspector guard so it drops AFTER it, the reference's
        // LIFO defer order.)
        let _thread_state = ThreadStateGuard;

        // Setup our inspector event if we have an inspector.
        // var insp_ev: ?inspectorpkg.KeyEvent = if (self.inspector != null) ev: {
        //     var copy = event;
        //     copy.utf8 = "";
        //     if (event.utf8.len > 0) copy.utf8 = try self.alloc.dupe(u8, event.utf8);
        //     break :ev .{ .event = copy };
        // } else null;
        //
        // (the OOM-only `try alloc.dupe` collapses to the owned clone — the
        // inspector widgets/key.rs Event::init precedent.)
        let insp_ev_: Option<inspectorpkg::KeyEvent> = if this.inspector.is_some() {
            let mut copy = event.clone();
            copy.utf8 = Vec::new();
            if !event.utf8.is_empty() {
                copy.utf8 = event.utf8.clone();
            }
            Some(inspectorpkg::KeyEvent {
                event: copy,
                ..Default::default()
            })
        } else {
            None
        };

        // When we're done processing, we always want to add the event to
        // the inspector.
        //
        // (the reference `defer if (insp_ev) |ev| ev: {...}` — Surface.zig:
        // 2674-2689 — is [`InspEvGuard`]'s `Drop`; the `if (insp_ev) |*ev| ev
        // else null` argument sites below are the `.ev.as_mut()` reborrows.)
        let mut insp_ev = InspEvGuard::<A> {
            surface: self_,
            ev: insp_ev_,
        };

        // Handle keybindings first. We need to handle this on all events
        // (press, repeat, release) because a press may perform a binding but
        // a release should not encode if we consumed the press.
        // if (try self.maybeHandleBinding(
        //     event,
        //     if (insp_ev) |*ev| ev else null,
        // )) |v| return v;
        if let Some(v) = unsafe { Self::maybeHandleBinding(self_, &event, insp_ev.ev.as_mut()) }? {
            return Ok(v);
        }
        // (N442: a fresh reborrow; the binding dispatch may have re-entered.)
        let this = unsafe { &mut *self_ };

        // If we allow KAM and KAM is enabled then we do nothing.
        // if (self.config.vt_kam_allowed) {
        if this.config.vt_kam_allowed {
            // self.renderer_state.mutex.lock();
            unsafe { (*(*this.renderer_state.as_ptr()).mutex).lock() };
            // defer self.renderer_state.mutex.unlock();
            // if (self.io.terminal.modes.get(.disable_keyboard)) return .consumed;
            if unsafe { &(*this.io.as_ptr()).terminal }
                .modes
                .get(terminal::modes::Mode::DisableKeyboard)
            {
                // (the deferred mutex unlock — the consumed return)
                unsafe { (*(*this.renderer_state.as_ptr()).mutex).unlock() };
                return Ok(InputEffect::Consumed);
            }
            // (the deferred mutex unlock — the fall-through)
            unsafe { (*(*this.renderer_state.as_ptr()).mutex).unlock() };
        }

        // If this input event has text, then we hide the mouse if configured.
        // We only do this on pressed events to avoid hiding the mouse when we
        // change focus due to a keybinding (i.e. switching tabs).
        // if (self.config.mouse_hide_while_typing and
        //     event.action == .press and
        //     !self.mouse.hidden and
        //     event.utf8.len > 0)
        if this.config.mouse_hide_while_typing
            && event.action == key::Action::Press
            && !this.mouse.hidden
            && !event.utf8.is_empty()
        {
            this.hideMouse();
        }

        // If our mouse modifiers change we may need to change our
        // link highlight state.
        // if (!self.mouse.mods.equal(event.mods)) mouse_mods: {
        if !this.mouse.mods.equal(event.mods) {
            'mouse_mods: {
                // Update our modifiers, this will update mouse mods too
                this.modsChanged(event.mods);

                // We only refresh links if
                // 1. mouse reporting is off
                // OR
                // 2. mouse reporting is on and we are not reporting shift to the terminal
                // if (self.io.terminal.flags.mouse_event == .none or
                //     (self.mouse.mods.shift and !self.mouseShiftCapture(false)))
                if unsafe { (*this.io.as_ptr()).terminal.flags.mouse_event }
                    == terminal::mouse::Event::None
                    || (this.mouse.mods.shift && !this.mouseShiftCapture(false))
                {
                    // Refresh our link state
                    // const pos = self.rt_surface.getCursorPos() catch break :mouse_mods;
                    let pos = match unsafe { &*this.rt_surface }.get_cursor_pos() {
                        Ok(v) => v,
                        Err(_) => break 'mouse_mods,
                    };
                    // self.renderer_state.mutex.lock();
                    unsafe { (*(*this.renderer_state.as_ptr()).mutex).lock() };
                    // defer self.renderer_state.mutex.unlock();
                    // self.mouseRefreshLinks(
                    //     pos,
                    //     self.posToViewport(pos.x, pos.y),
                    //     self.mouse.over_link,
                    // ) catch |err| {
                    //     log.warn("failed to refresh links err={}", .{err});
                    //     break :mouse_mods;
                    // };
                    //
                    // (Zig widens the f32 cursor coordinates to the f64
                    // params — the selectionScrollTick precedent.)
                    let pos_vp = this.posToViewport(f64::from(pos.x), f64::from(pos.y));
                    if let Err(err) = this.mouseRefreshLinks(pos, pos_vp, this.mouse.over_link) {
                        // log.warn("failed to refresh links err={}", .{err});
                        log::warn!(target: LOG_SCOPE, "failed to refresh links err={err:?}");
                        // (the deferred mutex unlock — the catch break)
                        unsafe { (*(*this.renderer_state.as_ptr()).mutex).unlock() };
                        break 'mouse_mods;
                    }
                    // (the deferred mutex unlock — the normal exit)
                    unsafe { (*(*this.renderer_state.as_ptr()).mutex).unlock() };
                // } else if (self.io.terminal.flags.mouse_event != .none and !self.mouse.mods.shift) {
                } else if unsafe { (*this.io.as_ptr()).terminal.flags.mouse_event }
                    != terminal::mouse::Event::None
                    && !this.mouse.mods.shift
                {
                    // If we have mouse reports on and we don't have shift pressed, we reset state
                    // _ = try self.rt_app.performAction(
                    //     .{ .surface = self },
                    //     .mouse_shape,
                    //     self.io.terminal.mouse_shape,
                    // );
                    let _ = unsafe { &*this.rt_app }.perform_action(
                        apprt::Target::surface_ref(&*this),
                        apprt::Action::MouseShape(unsafe {
                            (*this.io.as_ptr()).terminal.mouse_shape
                        }),
                    )?;
                    // _ = try self.rt_app.performAction(
                    //     .{ .surface = self },
                    //     .mouse_over_link,
                    //     .{ .url = "" },
                    // );
                    let _ = unsafe { &*this.rt_app }.perform_action(
                        apprt::Target::surface_ref(&*this),
                        apprt::Action::MouseOverLink(MouseOverLink { url: b"" }),
                    )?;
                    // try self.queueRender();
                    this.queueRender()?;
                }
            }
        }

        // Process the cursor state logic. This will update the cursor shape if
        // needed, depending on the key state.
        // if ((SurfaceMouse{
        //     .physical_key = event.key,
        //     .mouse_event = self.io.terminal.flags.mouse_event,
        //     .mouse_shape = self.io.terminal.mouse_shape,
        //     .mods = self.mouse.mods,
        //     .over_link = self.mouse.over_link,
        //     .hidden = self.mouse.hidden,
        // }).keyToMouseShape()) |shape| _ = try self.rt_app.performAction(
        //     .{ .surface = self },
        //     .mouse_shape,
        //     shape,
        // );
        if let Some(shape) = (SurfaceMouse {
            physical_key: event.key,
            mouse_event: unsafe { (*this.io.as_ptr()).terminal.flags.mouse_event },
            mouse_shape: unsafe { (*this.io.as_ptr()).terminal.mouse_shape },
            mods: this.mouse.mods,
            over_link: this.mouse.over_link,
            hidden: this.mouse.hidden,
        })
        .key_to_mouse_shape()
        {
            let _ = unsafe { &*this.rt_app }.perform_action(
                apprt::Target::surface_ref(&*this),
                apprt::Action::MouseShape(shape),
            )?;
        }

        // We've processed a key event that produced some data so we want to
        // track the last pressed key.
        // self.pressed_key = event: {
        this.pressed_key = 'event: {
            // We need to unset the allocated fields that will become invalid
            // var copy = event;
            // copy.utf8 = "";
            let mut copy = event.clone();
            copy.utf8 = Vec::new();

            // If we have a previous pressed key and we're releasing it
            // then we set it to invalid to prevent repeating the release event.
            // if (event.action == .release) {
            if event.action == key::Action::Release {
                // if we didn't have a previous event and this is a release
                // event then we just want to set it to null.
                // const prev = self.pressed_key orelse break :event null;
                let prev = match &this.pressed_key {
                    Some(v) => v,
                    None => break 'event None,
                };
                // if (prev.key == copy.key) copy.key = .unidentified;
                if prev.key == copy.key {
                    copy.key = Key::Unidentified;
                }
            }

            // If our key is invalid and we have no mods, then we're done!
            // This helps catch the state that we naturally released all keys.
            // if (copy.key == .unidentified and copy.mods.empty()) break :event null;
            if copy.key == Key::Unidentified && copy.mods.empty() {
                break 'event None;
            }

            // break :event copy;
            Some(copy)
        };

        // Encode and send our key. If we didn't encode anything, then we
        // return the effect as ignored.
        // if (try self.encodeKey(
        //     event,
        //     if (insp_ev) |*ev| ev else null,
        // )) |write_req| {
        if let Some(write_req) = this.encodeKey(&event, insp_ev.ev.as_mut())? {
            // If our process is exited and we press a key that results in
            // an encoded value, we close the surface. We want to eventually
            // move this behavior to the apprt probably.
            // if (self.child_exited) {
            if this.child_exited {
                // write_req.deinit();
                //
                // (Drop-owned `WriteReq` — freed by the return below.)
                // self.close();
                //
                // (N465: raw-self; `this` is dead from here on.)
                unsafe { Self::close(self_) };
                // return .closed;
                return Ok(InputEffect::Closed);
            }

            // (the reference's former `errdefer write_req.deinit()` is gone:
            // `queueIo` takes ownership of the message and cannot fail.)
            // self.queueIo(switch (write_req) {
            this.queueIo(
                match write_req {
                    // .small => |v| .{ .write_small = v },
                    MessageData::Small(v) => termio::Message::WriteSmall(v),
                    // .stable => |v| .{ .write_stable = v },
                    MessageData::Stable(v) => termio::Message::WriteStable(v),
                    // .alloc => |v| .{ .write_alloc = v },
                    MessageData::Alloc(v) => termio::Message::WriteAlloc(v),
                },
                // }, .unlocked);
                termio::termio_impl::MutexState::Unlocked,
            );
        } else {
            // No valid request means that we didn't encode anything.
            return Ok(InputEffect::Ignored);
        }

        // If our event is any keypress that isn't a modifier and we generated
        // some data to send to the pty, then we move the viewport down to the
        // bottom. We also clear the selection for any key other then modifiers.
        // if (!event.key.modifier()) {
        if !event.key.modifier() {
            // self.renderer_state.mutex.lock();
            unsafe { (*(*this.renderer_state.as_ptr()).mutex).lock() };
            // defer self.renderer_state.mutex.unlock();

            // if (self.config.selection_clear_on_typing or
            //     event.key == .escape)
            if this.config.selection_clear_on_typing || event.key == Key::Escape {
                // try self.setSelection(null);
                if let Err(err) = this.setSelection(None) {
                    // (the deferred mutex unlock — the error return)
                    unsafe { (*(*this.renderer_state.as_ptr()).mutex).unlock() };
                    return Err(err);
                }
            }

            // if (self.config.scroll_to_bottom.keystroke) self.io.terminal.scrollViewport(.bottom);
            if this.config.scroll_to_bottom.keystroke {
                unsafe { &mut (*this.io.as_ptr()).terminal }
                    .scroll_viewport(terminal::terminal::ScrollViewport::Bottom);
            }

            // try self.queueRender();
            let result = this.queueRender();
            // (the deferred mutex unlock — the normal/error exit)
            unsafe { (*(*this.renderer_state.as_ptr()).mutex).unlock() };
            result?;
        }

        // return .consumed;
        Ok(InputEffect::Consumed)
    }

    /// Maybe handles a binding for a given event and if so returns the effect.
    /// Returns null if the event is not handled in any way and processing should
    /// continue.
    // fn maybeHandleBinding(
    //     self: *Surface,
    //     event: input.KeyEvent,
    //     insp_ev: ?*inspectorpkg.KeyEvent,
    // ) !?InputEffect {
    //
    // (by-value `event` -> `&KeyEvent` — the Rust KeyEvent owns its utf8
    // bytes, the inspector Event::init receiver precedent; the inferred
    // `!?InputEffect` is the [`Error`] union.)
    //
    // PORT NOTE (N442, P7-W7 T5b): raw-self, as `keyCallback`. `this` is an
    // unprotected local reborrow; it is taken afresh after every
    // `performBindingAction` / `performAllChainedAction` call, since those can
    // run the embedder's action callback, which re-enters this surface.
    ///
    /// # Safety
    /// As [`Surface::keyCallback`].
    unsafe fn maybeHandleBinding(
        self_: *mut Surface<A>,
        event: &KeyEvent,
        mut insp_ev: Option<&mut inspectorpkg::KeyEvent>,
    ) -> Result<Option<InputEffect>, Error> {
        let mut this = unsafe { &mut *self_ };
        // switch (event.action) {
        match event.action {
            // Release events never trigger a binding but we need to check if
            // we consumed the press event so we don't encode the release.
            // .release => {
            key::Action::Release => {
                // if (self.keyboard.last_trigger) |last| {
                if let Some(last) = this.keyboard.last_trigger {
                    // if (last == event.bindingHash()) {
                    if last == event.binding_hash() {
                        // We don't reset the last trigger on release because
                        // an apprt may send multiple release events for a single
                        // press event.
                        return Ok(Some(InputEffect::Consumed));
                    }
                }

                // return null;
                return Ok(None);
            }

            // Carry on processing.
            // .press, .repeat => {},
            key::Action::Press | key::Action::Repeat => {}
        }

        // Find an entry in the keybind set that matches our event.
        // const entry: input.Binding.Set.Entry = entry: {
        //
        // (the `(set, index)` entry representation — the keyEventIsBinding
        // PORT NOTE above.)
        let entry: (*const binding::Set, usize) = 'entry: {
            // Handle key sequences first.
            // if (self.keyboard.sequence_set) |set| {
            if let Some(set) = this.keyboard.sequence_set {
                // Get our entry from the set for the given event.
                // if (set.getEvent(event)) |v| break :entry v;
                if let Some(v) = unsafe { &*set }.get_event(event) {
                    break 'entry (set, v);
                }

                // No entry found. We need to encode everything up to this
                // point and send to the pty since we're in a sequence.

                // We ignore modifiers so that nested sequences such as
                // ctrl+a>ctrl+b>c work.
                // if (event.key.modifier()) return null;
                if event.key.modifier() {
                    return Ok(None);
                }

                // If we have a catch-all of ignore, then we special case our
                // invalid sequence handling to ignore it.
                // if (self.catchAllIsIgnore()) {
                if this.catchAllIsIgnore() {
                    // self.endKeySequence(.drop, .retain);
                    this.endKeySequence(KeySequenceQueued::Drop, KeySequenceMemory::Retain);
                    // return .ignored;
                    return Ok(Some(InputEffect::Ignored));
                }

                // Encode everything up to this point
                // self.endKeySequence(.flush, .retain);
                this.endKeySequence(KeySequenceQueued::Flush, KeySequenceMemory::Retain);

                // return null;
                return Ok(None);
            }

            // No currently active sequence, move on to tables. For tables,
            // we search inner-most table to outer-most. The table stack does
            // NOT include the root set.
            // const table_items = self.keyboard.table_stack.items;
            // if (table_items.len > 0) {
            //
            // (the loop performs a binding action MID-iteration —
            // `deactivate_key_table` pops this very stack — so it iterates
            // by the captured length and copies each element out; the
            // reference `const table = table_items[rev_i];` is a by-value
            // copy too.)
            let table_items_len = this.keyboard.table_stack.len();
            if table_items_len > 0 {
                // for (0..table_items.len) |i| {
                for i in 0..table_items_len {
                    // const rev_i: usize = table_items.len - 1 - i;
                    let rev_i: usize = table_items_len - 1 - i;
                    // const table = table_items[rev_i];
                    let table = this.keyboard.table_stack[rev_i];
                    // if (table.set.getEvent(event)) |v| {
                    if let Some(v) = unsafe { &*table.set }.get_event(event) {
                        // If this is a one-shot activation AND its the currently
                        // active table, then we deactivate it after this.
                        // Note: we may want to change the semantics here to
                        // remove this table no matter where it is in the stack,
                        // maybe.
                        // if (table.once and i == 0) _ = try self.performBindingAction(
                        //     .deactivate_key_table,
                        // );
                        if table.once && i == 0 {
                            let _ = unsafe {
                                Self::performBindingAction(
                                    self_,
                                    binding::Action::DeactivateKeyTable,
                                )
                            }?;
                            // (N442: a fresh reborrow after the dispatch.)
                            this = unsafe { &mut *self_ };
                        }

                        // break :entry v;
                        break 'entry (table.set, v);
                    }
                }
            }

            // No table, use our default set
            // break :entry self.config.keybind.set.getEvent(event) orelse
            //     return null;
            match this.config.keybind.set.get_event(event) {
                Some(v) => break 'entry (&this.config.keybind.set as *const binding::Set, v),
                None => return Ok(None),
            }
        };

        // Determine if this entry has an action or if its a leader key.
        // const leaf: input.Binding.Set.GenericLeaf = switch (entry.value_ptr.*) {
        let leaf: binding::GenericLeaf = match unsafe { &*entry.0 }.value_at(entry.1) {
            // .leader => |set| {
            binding::Value::Leader(set) => {
                // PORT NOTE (N442): the trigger (`entry.key_ptr.*`, read for the
                // `key_sequence` action below) is read here, before `encodeKey`
                // reborrows the surface: for the default set `entry.0` points
                // into `this.config`, and a later `&mut` reborrow of `this`
                // would retire that pointer's tag. A pure read; order-neutral.
                let trigger = *unsafe { &*entry.0 }.key_at(entry.1);

                // Store this event so that we can drain and encode on invalid.
                // We don't need to cap this because it is naturally capped by
                // the config validation.
                // if (try self.encodeKey(event, insp_ev)) |req| {
                //     self.keyboard.sequence_queued.append(self.alloc, req) catch |err| {
                //         req.deinit();
                //         return err;
                //     };
                // }
                //
                // (the append `catch` collapses: Vec `push` cannot fail, and
                // `req` is Drop-owned so the reference's on-error `req.deinit()`
                // has no counterpart.)
                if let Some(req) = this.encodeKey(event, insp_ev.as_deref_mut())? {
                    this.keyboard.sequence_queued.push(req);
                }

                // Setup the next set we'll look at only after all fallible work.
                // self.keyboard.sequence_set = set;
                this.keyboard.sequence_set = Some(&**set as *const binding::Set);

                // Start or continue our key sequence
                // _ = self.rt_app.performAction(
                //     .{ .surface = self },
                //     .key_sequence,
                //     .{ .trigger = entry.key_ptr.* },
                // ) catch |err| {
                //     log.warn(
                //         "failed to notify app of key sequence err={}",
                //         .{err},
                //     );
                // };
                //
                // (`trigger` is read at the top of this arm; PORT NOTE there.)
                if let Err(err) = unsafe { &*this.rt_app }.perform_action(
                    apprt::Target::surface_ref(&*this),
                    apprt::Action::KeySequence(KeySequence::Trigger(trigger)),
                ) {
                    // log.warn("failed to notify app of key sequence err={}", .{err});
                    log::warn!(target: LOG_SCOPE, "failed to notify app of key sequence err={err:?}");
                }

                // return .consumed;
                return Ok(Some(InputEffect::Consumed));
            }

            // inline .leaf, .leaf_chained => |leaf| leaf.generic(),
            binding::Value::Leaf(leaf) => leaf.generic(),
            binding::Value::LeafChained(leaf) => leaf.generic(),
        };

        // consumed determines if the input is consumed or if we continue
        // encoding the key (if we have a key to encode).
        // const consumed = consumed: {
        let consumed = 'consumed: {
            // If the consumed flag is explicitly set, then we are consumed.
            // if (leaf.flags.consumed) break :consumed true;
            if leaf.flags.consumed {
                break 'consumed true;
            }

            // If the global or all flag is set, we always consume.
            // if (leaf.flags.global or leaf.flags.all) break :consumed true;
            if leaf.flags.global || leaf.flags.all {
                break 'consumed true;
            }

            // break :consumed false;
            false
        };

        // We have an action, so at this point we're handling SOMETHING so
        // we reset the last trigger to null. We only set this if we actually
        // perform an action (below)
        // self.keyboard.last_trigger = null;
        this.keyboard.last_trigger = None;

        // An action also always resets the sequence set.
        // self.keyboard.sequence_set = null;
        this.keyboard.sequence_set = None;

        // Setup our actions
        // const actions = leaf.actionsSlice();
        //
        // PORT NOTE: reference defect (R13), UD365. The reference's slice points
        // into the config a chained `reload_config` frees mid-loop; `leaf` here
        // owns a copy of the actions (`GenericLeaf`), so the loop and the checks
        // after it never read the replaced config.
        let actions: &[binding::Action] = leaf.actions_slice();

        // Attempt to perform the action
        // log.debug("key event binding flags={} action={any}", .{
        //     leaf.flags,
        //     actions,
        // });
        log::debug!(
            target: LOG_SCOPE,
            "key event binding flags={:?} action={:?}",
            leaf.flags,
            actions
        );
        // const performed = performed: {
        let performed = 'performed: {
            // If this is a global or all action, then we perform it on
            // the app and it applies to every surface.
            // if (leaf.flags.global or leaf.flags.all) {
            if leaf.flags.global || leaf.flags.all {
                // self.app.performAllChainedAction(
                //     self.rt_app,
                //     actions,
                // );
                //
                // (N442: through the raw root; it performs on every surface,
                // this one included.)
                unsafe {
                    crate::app::App::performAllChainedAction(
                        (*self_).app,
                        &*(*self_).rt_app,
                        actions,
                    )
                };

                // "All" actions are always performed since they are global.
                // break :performed true;
                break 'performed true;
            }

            // Perform each action. We are performed if ANY of the chained
            // actions perform.
            // var performed: bool = false;
            let mut performed: bool = false;
            // for (actions) |action| {
            for action in actions {
                // if (self.performBindingAction(action)) |v| {
                //     performed = performed or v;
                // } else |err| {
                //     log.info(
                //         "key binding action failed action={t} err={}",
                //         .{ action, err },
                //     );
                // }
                //
                // (Zig passes the action by value — the app.rs
                // performAllChainedAction clone precedent.)
                match unsafe { Self::performBindingAction(self_, action.clone()) } {
                    Ok(v) => performed = performed || v,
                    Err(err) => {
                        // log.info("key binding action failed action={t} err={}", .{ action, err });
                        log::info!(
                            target: LOG_SCOPE,
                            "key binding action failed action={} err={err:?}",
                            action.tag_name()
                        );
                    }
                }
            }

            // break :performed performed;
            performed
        };
        // (N442: from here the surface is reborrowed afresh at each use: the
        // dispatch above may have re-entered it, and a closing binding may have
        // freed it — the closing check below touches no surface byte.)
        let this = || unsafe { &mut *self_ };

        // if (performed) {
        if performed {
            // If we performed an action and it was a closing action,
            // our "self" pointer is not safe to use anymore so we need to
            // just exit immediately.
            // for (actions) |action| if (closingAction(action)) {
            for action in actions {
                if closingAction(action) {
                    // log.debug("key binding is a closing binding, halting key event processing", .{});
                    log::debug!(
                        target: LOG_SCOPE,
                        "key binding is a closing binding, halting key event processing"
                    );
                    // return .closed;
                    return Ok(Some(InputEffect::Closed));
                }
            }

            // If our action was "ignore" then we return the special input
            // effect of "ignored".
            // for (actions) |action| if (action == .ignore) {
            //
            // (Zig `==` on a tagged union with an enum literal is a TAG
            // compare; `Ignore` is payloadless so `matches!` is exact.)
            for action in actions {
                if matches!(action, binding::Action::Ignore) {
                    // If we're in a sequence, clear it.
                    // self.endKeySequence(.drop, .retain);
                    this().endKeySequence(KeySequenceQueued::Drop, KeySequenceMemory::Retain);

                    // return .ignored;
                    return Ok(Some(InputEffect::Ignored));
                }
            }
        }

        // If we have the performable flag and the action was not performed,
        // then we act as though a binding didn't exist.
        // if (leaf.flags.performable and !performed) {
        if leaf.flags.performable && !performed {
            // If we're in a sequence, we treat this as if we pressed a key
            // that doesn't exist in the sequence. Reset our sequence and flush
            // any queued events.
            // self.endKeySequence(.flush, .retain);
            this().endKeySequence(KeySequenceQueued::Flush, KeySequenceMemory::Retain);

            // return null;
            return Ok(None);
        }

        // If we consume this event, then we are done. If we don't consume
        // it, we processed the action but we still want to process our
        // encodings, too.
        // if (consumed) {
        if consumed {
            // If we had queued events, we deinit them since we consumed
            // self.endKeySequence(.drop, .retain);
            this().endKeySequence(KeySequenceQueued::Drop, KeySequenceMemory::Retain);

            // Store our last trigger so we don't encode the release event
            // self.keyboard.last_trigger = event.bindingHash();
            this().keyboard.last_trigger = Some(event.binding_hash());

            // if (insp_ev) |ev| {
            if let Some(ev) = insp_ev {
                // ev.binding = self.alloc.dupe(
                //     input.Binding.Action,
                //     actions,
                // ) catch |err| binding: {
                //     log.warn(
                //         "error allocating binding action for inspector err={}",
                //         .{err},
                //     );
                //     break :binding &.{};
                // };
                //
                // (the OOM-only catch collapses — infallible global
                // allocator; the `&.{}` fallback arm is unreachable.)
                ev.binding = actions.to_vec();
            }
            // return .consumed;
            return Ok(Some(InputEffect::Consumed));
        }

        // If we didn't perform OR we didn't consume, then we want to
        // encode any queued events for a sequence.
        // self.endKeySequence(.flush, .retain);
        this().endKeySequence(KeySequenceQueued::Flush, KeySequenceMemory::Retain);

        // return null;
        Ok(None)
    }

    // fn deactivateAllKeyTables(self: *Surface) !bool {
    //
    // (the inferred `!bool` — the body's only failure is `catch`-swallowed,
    // so the set is empty; the opaque [`Error`] marker keeps the callers'
    // `try` shape — updateConfig :1755.)
    pub(super) fn deactivateAllKeyTables(&mut self) -> Result<bool, Error> {
        // switch (self.keyboard.table_stack.items.len) {
        match self.keyboard.table_stack.len() {
            // No key table active. This does nothing.
            // 0 => return false,
            0 => return Ok(false),

            // Clear the entire table stack.
            // else => self.keyboard.table_stack.clearAndFree(self.alloc),
            //
            // (`clearAndFree` — drop the storage too, the deinit mem::take
            // precedent.)
            _ => drop(std::mem::take(&mut self.keyboard.table_stack)),
        }

        // Notify the UI.
        // _ = self.rt_app.performAction(
        //     .{ .surface = self },
        //     .key_table,
        //     .deactivate_all,
        // ) catch |err| {
        //     log.warn(
        //         "failed to notify app of key table err={}",
        //         .{err},
        //     );
        // };
        if let Err(err) = unsafe { &*self.rt_app }.perform_action(
            apprt::Target::surface_ref(&*self),
            apprt::Action::KeyTable(KeyTable::DeactivateAll),
        ) {
            // log.warn("failed to notify app of key table err={}", .{err});
            log::warn!(target: LOG_SCOPE, "failed to notify app of key table err={err:?}");
        }

        // return true;
        Ok(true)
    }

    /// This checks if the current keybinding sets have a catch_all binding
    /// with `ignore`. This is used to determine some special input cases.
    // fn catchAllIsIgnore(self: *Surface) bool {
    fn catchAllIsIgnore(&mut self) -> bool {
        // Get our catch all
        // const entry: input.Binding.Set.Entry = entry: {
        //
        // (the `(set, index)` entry representation — the keyEventIsBinding
        // PORT NOTE above; `Set.get` is the ported `get_index`.)
        let entry: (*const binding::Set, usize) = 'entry: {
            // const trigger: input.Binding.Trigger = .{ .key = .catch_all };
            let trigger = binding::Trigger {
                key: binding::TriggerKey::CatchAll,
                ..Default::default()
            };

            // const table_items = self.keyboard.table_stack.items;
            let table_items = &self.keyboard.table_stack;
            // for (0..table_items.len) |i| {
            for i in 0..table_items.len() {
                // const rev_i: usize = table_items.len - 1 - i;
                let rev_i: usize = table_items.len() - 1 - i;
                // const entry = table_items[rev_i].set.get(trigger) orelse continue;
                let entry = match unsafe { &*table_items[rev_i].set }.get_index(trigger) {
                    Some(v) => v,
                    None => continue,
                };
                // break :entry entry;
                break 'entry (table_items[rev_i].set, entry);
            }

            // break :entry self.config.keybind.set.get(trigger) orelse
            //     return false;
            match self.config.keybind.set.get_index(trigger) {
                Some(v) => break 'entry (&self.config.keybind.set as *const binding::Set, v),
                None => return false,
            }
        };

        // We have a catch-all entry, see if its an ignore
        // return switch (entry.value_ptr.*) {
        match unsafe { &*entry.0 }.value_at(entry.1) {
            // .leader => false,
            binding::Value::Leader(_) => false,
            // .leaf => |leaf| leaf.action == .ignore,
            binding::Value::Leaf(leaf) => matches!(leaf.action, binding::Action::Ignore),
            // .leaf_chained => |leaf| chained: for (leaf.actions.items) |action| {
            //     if (action == .ignore) break :chained true;
            // } else false,
            binding::Value::LeafChained(leaf) => 'chained: {
                for action in &leaf.actions {
                    if matches!(action, binding::Action::Ignore) {
                        break 'chained true;
                    }
                }
                false
            }
        }
        // };
    }

    /// End a key sequence. Safe to call if no key sequence is active.
    ///
    /// Action and mem determine the behavior of the queued inputs up to this
    /// point.
    // fn endKeySequence(
    //     self: *Surface,
    //     action: KeySequenceQueued,
    //     mem: KeySequenceMemory,
    // ) void {
    pub(super) fn endKeySequence(&mut self, action: KeySequenceQueued, mem: KeySequenceMemory) {
        // Notify apprt key sequence ended
        // _ = self.rt_app.performAction(
        //     .{ .surface = self },
        //     .key_sequence,
        //     .end,
        // ) catch |err| {
        //     log.warn(
        //         "failed to notify app of key sequence end err={}",
        //         .{err},
        //     );
        // };
        if let Err(err) = unsafe { &*self.rt_app }.perform_action(
            apprt::Target::surface_ref(&*self),
            apprt::Action::KeySequence(KeySequence::End),
        ) {
            // log.warn("failed to notify app of key sequence end err={}", .{err});
            log::warn!(target: LOG_SCOPE, "failed to notify app of key sequence end err={err:?}");
        }

        // No matter what we clear our current sequence set. This restores
        // the set we look at to the root set.
        // self.keyboard.sequence_set = null;
        self.keyboard.sequence_set = None;

        // If we have no queued data, there is nothing else to do.
        // if (self.keyboard.sequence_queued.items.len == 0) return;
        if self.keyboard.sequence_queued.is_empty() {
            return;
        }

        // Run the proper action first
        //
        // PORT NOTE: the Zig loops copy each queued request out of the list
        // (`items` stays populated until the `mem` switch); the Rust
        // `WriteReq` is move-only, so the list moves out for the loop and
        // the emptied (capacity-retaining) vec goes back for the `mem`
        // switch below — same observable content.
        let mut queued = std::mem::take(&mut self.keyboard.sequence_queued);
        // switch (action) {
        match action {
            // .flush => for (self.keyboard.sequence_queued.items) |write_req| {
            //     self.queueIo(switch (write_req) {
            //         .small => |v| .{ .write_small = v },
            //         .stable => |v| .{ .write_stable = v },
            //         .alloc => |v| .{ .write_alloc = v },
            //     }, .unlocked);
            // },
            KeySequenceQueued::Flush => {
                for write_req in queued.drain(..) {
                    self.queueIo(
                        match write_req {
                            MessageData::Small(v) => termio::Message::WriteSmall(v),
                            MessageData::Stable(v) => termio::Message::WriteStable(v),
                            MessageData::Alloc(v) => termio::Message::WriteAlloc(v),
                        },
                        termio::termio_impl::MutexState::Unlocked,
                    );
                }
            }

            // .drop => for (self.keyboard.sequence_queued.items) |req| req.deinit(),
            //
            // (`WriteReq.deinit` — the alloc-variant free — is Drop in
            // Rustty; the deinit precedent.)
            KeySequenceQueued::Drop => {
                for req in queued.drain(..) {
                    drop(req);
                }
            }
        }
        self.keyboard.sequence_queued = queued;

        // Memory handling of the sequence after the action
        // switch (mem) {
        match mem {
            // .free => self.keyboard.sequence_queued.clearAndFree(self.alloc),
            KeySequenceMemory::Free => drop(std::mem::take(&mut self.keyboard.sequence_queued)),
            // .retain => self.keyboard.sequence_queued.clearRetainingCapacity(),
            KeySequenceMemory::Retain => self.keyboard.sequence_queued.clear(),
        }
    }

    /// Encodes the key event into a write request. The write request will
    /// always copy or allocate so the caller can safely free the event.
    // fn encodeKey(
    //     self: *Surface,
    //     event: input.KeyEvent,
    //     insp_ev: ?*inspectorpkg.KeyEvent,
    // ) !?termio.Message.WriteReq {
    //
    // (by-value `event` -> `&KeyEvent`, the maybeHandleBinding note; the
    // inferred `!?WriteReq` is the [`Error`] union at the `'static` queued
    // instantiation.)
    pub(super) fn encodeKey(
        &mut self,
        event: &KeyEvent,
        insp_ev: Option<&mut inspectorpkg::KeyEvent>,
    ) -> Result<Option<termio::message::WriteReq<'static>>, Error> {
        // const write_req: termio.Message.WriteReq = req: {
        let write_req: termio::message::WriteReq<'static> = 'req: {
            // Build our encoding options, which requires the lock.
            // const encoding_opts = self.encodeKeyOpts();
            let encoding_opts = self.encodeKeyOpts();

            // Try to write the input into a small array. This fits almost
            // every scenario. Larger situations can happen due to long
            // pre-edits.
            // var data: termio.Message.WriteReq.Small.Array = undefined;
            // var writer: std.Io.Writer = .fixed(&data);
            //
            // PORT NOTE: `Small.Array` == `[u8; WriteReq::MAX]`; Zig
            // `undefined` -> zeroed (the tail past `len` is never read). The
            // `.fixed` writer is a Cursor over the stack array — a write
            // past the end is io's `WriteZero`, the reference's
            // `error.WriteFailed`.
            let mut data = [0u8; termio::message::WriteReq::MAX];
            let mut writer = std::io::Cursor::new(&mut data[..]);
            // if (input.key_encode.encode(
            //     &writer,
            //     event,
            //     encoding_opts,
            // )) {
            if let Ok(()) = key_encode::encode(&mut writer, event, encoding_opts) {
                // const written = writer.buffered();
                let written_len = writer.position() as usize;

                // Special-case: we did nothing.
                // if (written.len == 0) return null;
                if written_len == 0 {
                    return Ok(None);
                }

                // break :req .{ .small = .{
                //     .data = data,
                //     .len = @intCast(written.len),
                // } };
                break 'req MessageData::Small(Small {
                    data,
                    len: written_len,
                });
            }
            // } else |err| switch (err) {
            //     // Means we need to allocate
            //     error.WriteFailed => {},
            // }
            //
            // (the fixed Cursor's only failure is buffer-full — the
            // `error.WriteFailed` arm; fall through to allocate.)

            // We need to allocate. We allocate double the UTF-8 length
            // or double the small array size, whichever is larger. That's
            // a heuristic that should work. The only scenario I know while
            // typing this where we don't have enough space is a long preedit,
            // and in that case the size we need is exactly the UTF-8 length,
            // so the double is being safe.
            // var alloc_writer: std.Io.Writer.Allocating = try .initCapacity(
            //     self.alloc,
            //     @max(event.utf8.len * 2, data.len * 2),
            // );
            // defer alloc_writer.deinit();
            //
            // (the growable `Vec` sink is the Allocating writer; the defer
            // is its drop — `toOwnedSlice` below transfers out of it.)
            let mut alloc_writer: Vec<u8> =
                Vec::with_capacity(std::cmp::max(event.utf8.len() * 2, data.len() * 2));

            // try input.key_encode.encode(
            //     &alloc_writer.writer,
            //     event,
            //     encoding_opts,
            // );
            //
            // (the Allocating writer's only error is OOM — the `Vec` sink is
            // infallible, the `try` collapses.)
            key_encode::encode(&mut alloc_writer, event, encoding_opts)
                .expect("Vec writer is infallible");
            // break :req .{ .alloc = .{
            //     .alloc = self.alloc,
            //     .data = try alloc_writer.toOwnedSlice(),
            // } };
            //
            // (`fb4c56159`: transfer the encoding buffer instead of copying it.
            // `toOwnedSlice` shrinks the allocation to the written length and
            // hands over ownership — `into_boxed_slice` is the same contract:
            // shrink-to-fit, remapping or copy-and-free as the allocator
            // allows; its only error is OOM, the `try` collapses.)
            break 'req MessageData::Alloc(Alloc {
                data: alloc_writer.into_boxed_slice(),
            });
        };

        // Copy the encoded data into the inspector event if we have one.
        // We do this before the mailbox because the IO thread could
        // release the memory before we get a chance to copy it.
        // if (insp_ev) |ev| pty: {
        if let Some(ev) = insp_ev {
            // const slice = write_req.slice();
            let slice = write_req.slice();
            // const copy = self.alloc.alloc(u8, slice.len) catch |err| {
            //     log.warn("error allocating pty data for inspector err={}", .{err});
            //     break :pty;
            // };
            // errdefer self.alloc.free(copy);
            // @memcpy(copy, slice);
            //
            // (the OOM-only catch + errdefer collapse — infallible owned
            // copy.)
            let copy = slice.to_vec();
            // ev.pty = copy;
            ev.pty = copy;
        }

        // return write_req;
        Ok(Some(write_req))
    }

    // fn encodeKeyOpts(self: *const Surface) input.key_encode.Options {
    fn encodeKeyOpts(&self) -> key_encode::Options {
        // self.renderer_state.mutex.lock();
        unsafe { (*(*self.renderer_state.as_ptr()).mutex).lock() };
        // defer self.renderer_state.mutex.unlock();
        // const t = &self.io.terminal;
        let t = unsafe { &(*self.io.as_ptr()).terminal };

        // var opts: input.key_encode.Options = .fromTerminal(t);
        let mut opts: key_encode::Options = key_encode::Options::from_terminal(t);
        // if (comptime builtin.os.tag != .macos) return opts;
        //
        // (`cfg!` — the comptime OS check, the init xdg-terminal-exec
        // precedent.)
        if cfg!(not(target_os = "macos")) {
            // (the deferred mutex unlock — the non-macOS comptime return)
            unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
            return opts;
        }

        // opts.macos_option_as_alt = self.config.macos_option_as_alt orelse detect: {
        //
        // PORT NOTE (crate-split glue): the reference has ONE
        // `input.OptionAsAlt`; the port has the config-side enum
        // (rustty-input — `self.config` and `detectOptionAsAlt`) and the
        // VT-side copy (the key_encode Options field, duplicated there so
        // the VT key encoder has no config dependency). The 1:1 variant map
        // below is the seam (identical discriminants).
        let v: OptionAsAlt = match self.config.macos_option_as_alt {
            Some(v) => v,
            None => 'detect: {
                // If we don't have alt pressed, it doesn't matter what this
                // config is so we can just say "false" and break out and avoid
                // more expensive checks below.
                // if (!self.mouse.mods.alt) break :detect .false;
                if !self.mouse.mods.alt {
                    break 'detect OptionAsAlt::False;
                }

                // Alt is pressed, we're on macOS. We break some encapsulation
                // here and assume libghostty for ease...
                // break :detect self.rt_app.keyboardLayout().detectOptionAsAlt();
                break 'detect unsafe { &*self.rt_app }
                    .keyboard_layout()
                    .detect_option_as_alt();
            }
        };
        opts.macos_option_as_alt = match v {
            OptionAsAlt::False => rustty_vt::input::config::OptionAsAlt::r#false,
            OptionAsAlt::True => rustty_vt::input::config::OptionAsAlt::r#true,
            OptionAsAlt::Left => rustty_vt::input::config::OptionAsAlt::left,
            OptionAsAlt::Right => rustty_vt::input::config::OptionAsAlt::right,
        };

        // return opts;
        // (the deferred mutex unlock — the macOS return)
        unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
        opts
    }

    /// Sends text as-is to the terminal without triggering any keyboard
    /// protocol. This will treat the input text as if it was pasted
    /// from the clipboard so the same logic will be applied. Namely,
    /// if bracketed mode is on this will do a bracketed paste. Otherwise,
    /// this will filter newlines to '\r'.
    // pub fn textCallback(self: *Surface, text: []const u8) !void {
    //
    // (raw bytes `&[u8]`, never a lossy String; the inferred `!void` is
    // `completeClipboardPaste`'s — the [`Error`] union.)
    pub fn textCallback(&mut self, text: &[u8]) -> Result<(), Error> {
        // Crash metadata in case we crash in here
        // crash.sentry.thread_state = self.crashThreadState();
        crash::sentry::setThreadState(self.crashThreadState());
        // defer crash.sentry.thread_state = null;
        // ([ADV 6.1] — the Drop guard clears at every exit below.)
        let _thread_state = ThreadStateGuard;

        // try self.completeClipboardPaste(text, true);
        self.completeClipboardPaste(text, true)?;
        Ok(())
    }
}
