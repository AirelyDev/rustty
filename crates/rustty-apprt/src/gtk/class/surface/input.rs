//! PORTS: ghostty src/apprt/gtk/class/surface.zig — `showOnScreenKeyboard`,
//! `scaledCoordinates`, `keyEvent`, the focus/mapped/occlusion updates,
//! `resizeOverlaySchedule`, and `initSurface` (the RenderSurface lifecycle,
//! `:3305-3528`).

use super::*;

impl Surface {
    /// PORTS: `surface.zig:983-986` `showOnScreenKeyboard`. Activates the
    /// on-screen keyboard through the IME context for the given event.
    #[must_use]
    pub fn show_on_screen_keyboard(&self, event: Option<&gdk4::Event>) -> bool {
        self.imp().im_context.activate_osk(event)
    }

    /// Scale x/y by the GDK device scale.
    ///
    /// PORTS: `surface.zig:1488-1502` `scaledCoordinates`. Scales `x`/`y` by the
    /// (fractional) GdkSurface scale (`scale_util.widgetSurfaceScale`).
    pub(super) fn scaled_coordinates(&self, x: f64, y: f64) -> (f64, f64) {
        let widget = self.imp().render_surface.upcast_ref::<gtk4::Widget>();
        let scale_factor = scale::widget_surface_scale(widget);
        (x * scale_factor, y * scale_factor)
    }

    /// Key press event (press or release).
    ///
    /// At a high level, we want to construct an `input.KeyEvent` and
    /// pass that to `keyCallback`. At a low level, this is more complicated
    /// than it appears because we need to construct all of this information
    /// and its not given to us.
    ///
    /// For all events, we run the GdkEvent through the input method context.
    /// This allows the input method to capture the event and trigger
    /// callbacks such as preedit, commit, etc.
    ///
    /// There are a couple important aspects to the prior paragraph: we must
    /// send ALL events through the input method context. This is because
    /// input methods use both key press and key release events to determine
    /// the state of the input method. For example, fcitx uses key release
    /// events on modifiers (i.e. ctrl+shift) to switch the input method.
    ///
    /// We set some state to note we're in a key event (self.in_keyevent)
    /// because some of the input method callbacks change behavior based on
    /// this state. For example, we don't want to send character events
    /// like "a" via the input "commit" event if we're actively processing
    /// a keypress because we'd lose access to the keycode information.
    /// However, a "commit" event may still happen outside of a keypress
    /// event from e.g. a tablet or on-screen keyboard.
    ///
    /// Finally, we take all of the information in order to determine if we have
    /// a unicode character or if we have to map the keyval to a code to
    /// get the underlying logical key, etc.
    ///
    /// Then we can emit the keyCallback.
    ///
    /// PORTS: `surface.zig:1240-1463` `keyEvent` — the tier's subtlest function.
    /// A GTK key event (press/repeat/release) is passed FIRST through the input
    /// method (`filterKeypress`), then — depending on the composition state —
    /// either consumed by the IME or encoded into a core `keyCallback`.
    ///
    /// **Invariant 9**: the whole IME state group ([`ImeState`]) is read/written
    /// coherently here. Port the whole IM block including its comments verbatim.
    ///
    /// PORT NOTE ([`ImKeyEvent`] naming): the reference's `IMKeyEvent` enum is
    /// `{ .false, .composing, .not_composing }`. rustty's [`ImKeyEvent`] (defined
    /// in T13) spells `not_composing` as [`ImKeyEvent::True`]; the mapping used
    /// throughout is `False ⇔ .false`, `Composing ⇔ .composing`, `True ⇔
    /// .not_composing`.
    pub(super) fn key_event(
        &self,
        action: Action,
        ec_key: &gtk4::EventControllerKey,
        keyval: u32,
        keycode: u32,
        gtk_mods: gdk4::ModifierType,
    ) -> bool {
        let imp = self.imp();

        // `getCurrentEvent()` + cast to a `gdk.KeyEvent` (`:1249-1250`).
        let Some(event) = ec_key.current_event() else {
            return false;
        };
        let Some(key_event) = event.downcast_ref::<gdk4::KeyEvent>() else {
            return false;
        };

        // The block below is all related to input method handling. See the function
        // comment for some high level details and then the comments within
        // the block for more specifics. (`:1253-1343`)
        {
            // This can trigger an input method so we need to notify the im
            // context where the cursor is so it can render the dropdowns in the
            // correct place (`:1257-1268`).
            if let Some(core) = self.core() {
                // SAFETY: `core` is the widget's boxed, initialized core surface.
                let ime_point = unsafe { (*core).imePoint() };
                imp.im_context.set_cursor_location(&gdk4::Rectangle::new(
                    ime_point.x as i32,
                    ime_point.y as i32,
                    1,
                    1,
                ));
            }

            // We note that we're in a keypress because we want some logic to
            // depend on this. For example, we don't want to send character
            // events like "a" via the input "commit" event if we're actively
            // processing a keypress because we'd lose access to the keycode
            // information.
            //
            // We have to maintain some additional state here of whether we were
            // composing because different input methods call the callbacks in
            // different orders. For example, ibus calls commit THEN preedit end
            // but simple calls preedit end THEN commit.
            //
            // `priv.in_keyevent = if (priv.im_composing) .composing else
            // .not_composing` (`:1279`); the `defer priv.in_keyevent = .false`
            // (`:1280`) is reproduced by the explicit reset after the IM block.
            {
                let mut ime = imp.ime.borrow_mut();
                ime.in_keyevent = if ime.im_composing {
                    ImKeyEvent::Composing
                } else {
                    ImKeyEvent::True
                };
            }

            // Pass the event through the input method which returns true if handled.
            // Confusingly, not all events handled by the input method result
            // in this returning true so we have to maintain some additional
            // state about whether we were composing or not to determine if
            // we should proceed with key encoding.
            //
            // Cases where the input method does not mark the event as handled:
            //
            // - If we change the input method via keypress while we have preedit
            //   text, the input method will commit the pending text but will not
            //   mark it as handled. We use the `.composing` state to detect
            //   this case.
            //
            // - If we switch input methods (i.e. via ctrl+shift with fcitx),
            //   the input method will handle the key release event but will not
            //   mark it as handled. I don't know any way to detect this case so
            //   it will result in a key event being sent to the key callback.
            //   For Kitty text encoding, this will result in modifiers being
            //   triggered despite being technically consumed. At the time of
            //   writing, both Kitty and Alacritty have the same behavior. I
            //   know of no way to fix this. (`:1282-1302`)
            let im_handled = imp.im_context.filter_keypress(&event);
            // log.warn("GTKIM: im_handled={} im_len={} im_composing={}", .{
            //     im_handled,
            //     self.im_len,
            //     self.im_composing,
            // });

            // Read the (possibly IME-mutated) state, then reset `in_keyevent` to
            // `.false` — the block-scoped `defer` at `:1280`, which fires AFTER
            // the early-return arms below read `in_keyevent`.
            let (im_composing, in_keyevent, im_len) = {
                let ime = imp.ime.borrow();
                (ime.im_composing, ime.in_keyevent, ime.im_len)
            };
            imp.ime.borrow_mut().in_keyevent = ImKeyEvent::False;

            // If the input method handled the event, you would think we would
            // never proceed with key encoding for Rustty but that is not the
            // case. Input methods will handle basic character encoding like
            // typing "a" and we want to associate that with the key event.
            // So we have to check additional state to determine if we exit.
            // (`:1310-1336`)
            if im_handled {
                // If we are composing then we're in a preedit state and do
                // not want to encode any keys. For example: type a deadkey
                // such as single quote on a US international keyboard layout.
                // (`:1316-1319`)
                if im_composing {
                    return true;
                }

                // If we were composing and now we're not, it means that we committed
                // the text. We also don't want to encode a key event for this.
                // Example: enable Japanese input method, press "konn" and then
                // press enter. The final enter should not be encoded and "konn"
                // (in hiragana) should be written as "こん". (`:1321-1326`)
                if in_keyevent == ImKeyEvent::Composing {
                    return true;
                }

                // Not composing and our input method buffer is empty. This could
                // mean that the input method reacted to this event by activating
                // an onscreen keyboard or something equivalent. We don't know.
                // But the input method handled it and didn't give us text so
                // we will just assume we should not encode this. This handles a
                // real scenario when ibus starts the emoji input method
                // (super+.). (`:1328-1335`)
                if im_len == 0 {
                    return true;
                }
            }

            // At this point, for the sake of explanation of internal state:
            // it is possible that im_len > 0 and im_composing == false. This
            // means that we received a commit event from the input method that
            // we want associated with the key event. This is common: its how
            // basic character translation for simple inputs like "a" work.
            // (`:1338-1342`)
        }

        // We always reset the length of the im buffer. There's only one scenario
        // we reach this point with im_len > 0 and that's if we received a commit
        // event from the input method. We don't want to keep that state around
        // since we've handled it here.
        // `defer priv.im_len = 0` (`:1349`) — the guard resets on every return
        // past this point.
        let _im_len_reset = ImLenReset(&imp.ime);

        // Get the keyvals for this event (`:1352-1357`).
        let keyval_unicode = key::keyval_to_unicode(keyval);
        let keyval_unicode_unshifted = key::keyval_unicode_unshifted(
            imp.render_surface.upcast_ref::<gtk4::Widget>(),
            key_event,
            keycode,
        );

        // We want to get the physical unmapped key to process physical keybinds.
        // (These are keybinds explicitly marked as requesting physical mapping).
        // (`:1359-1379`; `ef01d5fda`: the inline remap block is now
        // `key::remap_key`, folded into `physical_key` with the GDK modifier
        // flag).
        // break :keycode gtk_key.remapKey(w3c_key, keyval, key_event.isModifier() != 0);
        let physical_key = key::physical_key(keycode, keyval, key_event.is_modifier());

        // Get our modifier for the event (`:1381-1388`).
        let mods = {
            let app = Application::default();
            let wp = app.imp().winproto.borrow();
            key::event_mods(&event, physical_key, gtk_mods, action, &wp)
        };

        // Get our consumed modifiers (`:1390-1398`).
        let consumed_mods = {
            let masked =
                key_event.consumed_modifiers().bits() & (gdk4::ffi::GDK_MODIFIER_MASK as u32);
            key::translate_mods(gdk4::ModifierType::from_bits_truncate(masked))
        };

        // log.debug("key pressed key={} keyval={x} physical_key={} composing={} text_len={} mods={}", .{
        //     key,
        //     keyval,
        //     physical_key,
        //     priv.im_composing,
        //     priv.im_len,
        //     mods,
        // });

        // If we have no UTF-8 text, we try to convert our keyval to
        // a text value. We have to do this because GTK will not process
        // "Ctrl+Shift+1" (on US keyboards) as "Ctrl+!" but instead as "".
        // But the keyval is set correctly so we can at least extract that.
        // (`:1409-1424`)
        {
            let mut ime = imp.ime.borrow_mut();
            if ime.im_len == 0 && keyval_unicode > 0 {
                if let Some(cp) = char::from_u32(keyval_unicode) {
                    // We don't want to send control characters as IM text.
                    // Control characters are handled already by the encoder
                    // directly (`:1415-1418`).
                    if (cp as u32) >= 0x20 {
                        let mut buf = [0u8; 4];
                        let encoded = cp.encode_utf8(&mut buf);
                        let bytes = encoded.as_bytes();
                        ime.im_buf[..bytes.len()].copy_from_slice(bytes);
                        ime.im_len = bytes.len() as u8;
                    }
                }
            }
        }

        // Invoke the core Ghostty logic to handle this input (`:1426-1439`).
        let Some(core) = self.core() else {
            return false;
        };
        let (utf8, composing) = {
            let ime = imp.ime.borrow();
            (ime.im_buf[..ime.im_len as usize].to_vec(), ime.im_composing)
        };
        let core_event = KeyEvent {
            action,
            key: physical_key,
            mods,
            consumed_mods,
            composing,
            utf8,
            unshifted_codepoint: keyval_unicode_unshifted,
        };
        // SAFETY: `core` is the widget's boxed, initialized core surface; called
        // on the app/main thread from the key controller.
        let effect = match unsafe { CoreSurface::keyCallback(core, core_event) } {
            Ok(effect) => effect,
            Err(err) => {
                // log.err("error in key callback err={}", .{err});
                //
                // S54-F6: this was `log.warn` with NO error value. The
                // reference uses `log.err` HERE specifically
                // (`surface.zig:1454`) while its three sibling
                // `error in key callback` sites (`:2898 :2959 :3313`) are
                // `log.warn` — the level is per-site, not per-message.
                log::error!(target: LOG_SCOPE, "error in key callback err={err:?}");
                return false;
            }
        };

        match effect {
            InputEffect::Closed => true,
            InputEffect::Ignored => false,
            InputEffect::Consumed => {
                if action == Action::Press || action == Action::Repeat {
                    // If we were in the composing state then we reset our context.
                    // We do NOT want to reset if we're not in the composing state
                    // because there is other IME state that we want to preserve,
                    // such as quotation mark ordering for Chinese input
                    // (`:1444-1452`).
                    if imp.ime.borrow().im_composing {
                        imp.im_context.reset();
                        // SAFETY: `core` is the initialized core surface.
                        let _ = unsafe { (*core).preeditCallback(None) };
                    }

                    // Bell stops ringing when any key is pressed that is used by
                    // the core in any way. (`:1454-1456`)
                    self.set_bell_ringing_inner(false);

                    return true;
                }
                false
            }
        }
    }

    /// PORTS: `surface.zig:2754-2766` `updateFocus`. Records focus, drives the IME
    /// focus, schedules the (idle) core focus callback, notifies, and stops the
    /// bell on focus gain.
    pub(super) fn update_focus(&self, focused: bool) {
        let imp = self.imp();
        imp.focused.set(focused);

        // `if (focused) ctx.focusIn() else ctx.focusOut()` (`:2758-2759`).
        if focused {
            imp.im_context.focus_in();
        } else {
            imp.im_context.focus_out();
        }

        // `idleFocus` (`:2761,2768-2783`):
        // The focus callback must be triggered on an idle loop source because
        // there are actions within librustty callbacks (such as showing close
        // confirmation dialogs) that can trigger focus loss and cause a deadlock
        // because the lock may be held during the callback.
        //
        // Userdata should be a `*Surface`. This will unref once.
        //
        // The reference `ref`s the widget and
        // `unref`s in the callback; capturing an owned strong ref here is the same
        // (dropped at the closure's end).
        let obj = self.clone();
        glib::idle_add_local_once(move || {
            let Some(core) = obj.core() else {
                return;
            };
            // SAFETY: `core` points at this widget's boxed, initialized core
            // surface (pinned in the imp); the idle source runs on the app thread.
            if let Err(err) = unsafe { &mut *core }.focusCallback(obj.imp().focused.get()) {
                // log.warn("error in focus callback err={}", .{err});
                log::warn!(target: LOG_SCOPE, "error in focus callback err={err:?}");
            }
        });

        self.notify("focused");

        // Bell stops ringing as soon as we gain focus (`:2830`).
        //
        // CORRECTION (task 20, S55-F2): this used the generated property setter,
        // under a note calling it "set + notify-on-change" and saying the
        // freeze-notify semantics "only matter for `setBellRinging(true)`". The
        // generated setter notifies on EVERY write, so every focus-in fired
        // `notify::bell-ringing`; the change gate matters for `false` too.
        if focused {
            self.set_bell_ringing_inner(false);
        }
    }

    /// PORTS: `surface.zig:3315-3319` `updateMapped`.
    pub(super) fn update_mapped(&self, mapped: bool) {
        self.imp().mapped.set(mapped);
        self.notify("mapped");
    }

    /// Update the core surface visibility based on both GTK widget and
    /// toplevel state. This is public so the window can call it when its
    /// suspended state changes.
    ///
    /// PORTS: `surface.zig:3344-3358` `updateOcclusion` (oracle `2f3814ca5`,
    /// P3-W8 T5). Before that commit this took the visibility as an argument
    /// and was private; it now derives visibility from `mapped` AND the
    /// Wayland `xdg_toplevel` suspended state, so a mapped surface on an
    /// inactive workspace (or fully covered) stops producing frames.
    pub fn update_occlusion(&self) {
        let Some(core) = self.core() else {
            return;
        };
        let visible = self.imp().mapped.get() && !self.window_suspended();
        // SAFETY: `core` is the widget's boxed, initialized core surface; called
        // on the app thread from map/unmap or from the window's
        // `notify::suspended` handler.
        if let Err(err) = unsafe { &mut *core }.occlusionCallback(visible) {
            // log.warn("error in occlusion callback err={}", .{err});
            log::warn!(target: LOG_SCOPE, "error in occlusion callback err={err:?}");
        }
    }

    /// PORTS: `surface.zig:3360-3364` `windowSuspended` (oracle `2f3814ca5`).
    fn window_suspended(&self) -> bool {
        // const native = self.as(gtk.Widget).getNative() orelse return false;
        // const window = gobject.ext.cast(gtk.Window, native) orelse return false;
        // return window.isSuspended() != 0;
        let Some(native) = self.native() else {
            return false;
        };
        let Ok(window) = native.downcast::<gtk4::Window>() else {
            return false;
        };
        window.is_suspended()
    }

    /// PORTS: `surface.zig:3491-3520` `resizeOverlaySchedule`. Sets the overlay
    /// label to the current `cols x rows` and schedules it to show/hide.
    pub(super) fn resize_overlay_schedule(&self) {
        let imp = self.imp();

        // `const surface = priv.core_surface orelse return` (`:3493`).
        let Some(core) = self.core() else {
            return;
        };

        // Only show the resize overlay if its enabled (`:3496-3501`).
        let cfg_ref = imp.config.borrow();
        let Some(config) = cfg_ref.as_ref() else {
            return;
        };
        match config.get().resize_overlay {
            rustty_config::value_types::ResizeOverlay::Always
            | rustty_config::value_types::ResizeOverlay::AfterFirst => {}
            rustty_config::value_types::ResizeOverlay::Never => return,
        }

        // If we have resize overlays enabled, setup an idler
        // to show that. We do this in an idle tick because doing it
        // during the resize results in flickering. (`:3503-3519`)
        // SAFETY: `core` is the widget's boxed, initialized core surface.
        let grid = unsafe { &*core }.size.grid();
        let text = format!("{} x {}", grid.columns, grid.rows);
        imp.resize_overlay.set_label(Some(text.as_str()));
        imp.resize_overlay.schedule();
    }

    /// PORTS: `surface.zig:3443-3528` `initSurface`. The lazy core-surface
    /// constructor — built on the FIRST render-surface resize (**invariant 3**),
    /// NOT on realize, so we have proper initial dimensions before constructing
    /// the pty. The core surface starts unrealized when the widget is not
    /// realized yet (`2f0b65346`).
    pub(super) fn init_surface(&self) -> Result<(), InitError> {
        let imp = self.imp();
        // `assert(priv.core_surface == null)` (`:3445`).
        assert!(
            imp.core_surface.load(Ordering::Acquire).is_null(),
            "initSurface called with a core surface already present"
        );

        // (`2f0b65346` deleted the `gl_area.makeCurrent()`/`getError()` check
        // and its `error.GLAreaError`: the renderer owns its context.)

        let app = Application::default();
        let alloc = app.allocator();

        // The core app / rt app backrefs (`app.core()` / `app.rt()`). In rustty
        // these are reconstituted from the singleton's type-erased cells; when
        // unbound (widget tests) we cannot construct a surface — bail (the lazy
        // init simply doesn't complete, leaving `core_surface` None).
        let Some(core_app) = app_core_ptr() else {
            log::warn!(target: LOG_SCOPE, "core app not bound; cannot initialize surface");
            return Err(InitError::Surface);
        };
        let Some(rt_app) = app_rt_ptr() else {
            log::warn!(target: LOG_SCOPE, "rt app not bound; cannot initialize surface");
            return Err(InitError::Surface);
        };

        // Make our pointer to store our surface
        // Our stable heap slot for the core surface (`alloc.create(CoreSurface)`,
        // **invariant 2**, `:3430`). `Box`'s allocation is the pin the renderer/io
        // threads' pointers rely on; on the error path the `MaybeUninit` box is
        // dropped WITHOUT dropping a `CoreSurface` (the faithful `alloc.destroy`).
        let mut slot = Box::<std::mem::MaybeUninit<CoreSurface>>::new_uninit();
        // `MaybeUninit::as_mut_ptr` (via the cast) — `Box::as_mut_ptr` would yield
        // `*mut MaybeUninit<_>`; the slot's payload address is layout-identical.
        let surface_ptr: *mut CoreSurface = slot.as_mut_ptr().cast::<CoreSurface>();

        // The `rt_surface` self-ref pointer handed to core (`&priv.rt_surface`,
        // `:3470`). Stable because the GObject private is pinned.
        let rt_surface: *mut ApprtSurface = self.rt().cast_mut();

        // Add ourselves to the app's surface list (`:3434`); on any later failure
        // we remove ourselves again (the `errdefer deleteSurface`, `:3435`).
        // SAFETY: `core_app` is the live `App<App>` bound by `bind_core_app`.
        unsafe { &mut *core_app }.addSurface(rt_surface);

        // Initialize our surface configuration. (`:3438-3442`), applying the
        // caller overrides and font-size / pwd (`:3445-3462`).
        let mut config = {
            let cfg_ref = imp.config.borrow();
            let core_cfg = cfg_ref
                .as_ref()
                .expect("surface config is set by init's app fallback")
                .get();
            // SAFETY: `core_app` is live; `new_config` reads the focused surface's
            // pwd through the app's validated surface list.
            rustty_core::app::new_config(unsafe { &*core_app }, core_cfg, imp.context.get())
        };

        // `try applyCommandOverrides(&config, priv.overrides.command,
        // priv.overrides.shell_integration)` (`:3533-3537`, 3650bef4e).
        apply_command_overrides(
            &mut config,
            imp.overrides_command.borrow().as_ref(),
            imp.overrides_shell_integration.get(),
        );
        // `if (priv.overrides.working_directory) |wd| ...` (`:3448-3453`).
        if let Some(wd) = imp.overrides_working_directory.borrow().as_ref() {
            let mut wd_val = WorkingDirectory::Path(wd.to_bytes().to_vec());
            wd_val.finalize();
            config.working_directory = Some(wd_val);
        }
        // Properties that can impact surface init
        // `if (priv.font_size_request) |size| config.@"font-size" = size.points` (`:3456`).
        if let Some(size) = imp.font_size_request.borrow().as_ref() {
            config.font_size = size.0.points;
        }
        // `if (priv.pwd) |pwd| config.@"working-directory" = ...` (`:3457-3462`).
        if let Some(pwd) = imp.pwd.borrow().as_ref() {
            let mut wd_val = WorkingDirectory::Path(pwd.clone().into_bytes());
            wd_val.finalize();
            config.working_directory = Some(wd_val);
        }

        // PRE-PUBLISH the stable slot pointer BEFORE `CoreSurface::init` spawns the
        // renderer/io threads (**startup soundness**). This is a rustty-specific
        // necessity, NOT in the reference: the reference's renderer thread reads its
        // surface from `renderer.surface_mailbox.surface` (set at renderer
        // construction), but rustty dropped `surface_mailbox`, so the renderer
        // thread's FIRST action (`renderer_seam.rs` `threadMain_`, crash-sentry
        // setup) calls `apprt::Surface::core()` — which reads THIS field. Publishing
        // after `init` (the reference's `:3477`) would race: the just-spawned render
        // thread could read a null `core_surface` and hit the non-null assert. We
        // publish the (stable, about-to-be-initialized) slot address now; the render
        // thread's startup read only STORES the pointer as crash metadata and never
        // dereferences it before `init` completes and sends the first wakeup, so the
        // brief "published but not yet constructed" window is sound. `Release` pairs
        // with the `Acquire` loads in `core()`.
        imp.core_surface.store(surface_ptr, Ordering::Release);

        // const content_scale = self.getContentScale();
        // log.info("surface content scale: x={d} y={d} dpi={}x{} font_size={}", .{
        //     content_scale.x,
        //     content_scale.y,
        //     @round(content_scale.x * font.face.default_dpi),
        //     @round(content_scale.y * font.face.default_dpi),
        //     config.@"font-size",
        // });   (`:3484-3491`)
        //
        // `{}` on the `@round`ed f32 and on the f32 font size renders Zig's
        // shortest float form; `{d}` the same. The port prints both with Rust's
        // `Display`, which is that form too.
        {
            let content_scale = self.get_content_scale();
            let default_dpi = f32::from(rustty_font_shaper::face::DEFAULT_DPI);
            log::info!(
                target: LOG_SCOPE,
                "surface content scale: x={} y={} dpi={}x{} font_size={}",
                content_scale.x,
                content_scale.y,
                (content_scale.x * default_dpi).round(),
                (content_scale.y * default_dpi).round(),
                config.font_size,
            );
        }

        // Initialize the surface (`:3493-3503`). `config`'s Drop is the reference's
        // `defer config.deinit()`.
        // SAFETY: `surface_ptr` is a stable, uninitialized `CoreSurface` slot;
        // `core_app`/`rt_app`/`rt_surface` are live for the call; on `Err` the slot
        // is (still) uninitialized and freed below without being dropped.
        let init_res =
            unsafe { CoreSurface::init(surface_ptr, alloc, &config, core_app, rt_app, rt_surface) };
        drop(config);

        if let Err(ref err) = init_res {
            // log.warn("failed to initialize surface err={}", .{err});
            log::warn!(target: LOG_SCOPE, "failed to initialize surface err={err:?}");
            // Un-publish the slot before freeing it — `init` failed, so no live
            // `CoreSurface` backs the pointer (and any late startup read must see
            // null again, restoring **invariant 3**).
            imp.core_surface
                .store(std::ptr::null_mut(), Ordering::Release);
            // `errdefer app.core().deleteSurface(self.rt())` (`:3435`).
            // SAFETY: `core_app` is live; `rt_surface` still points at our pinned
            // embedded apprt surface.
            unsafe { &mut *core_app }.deleteSurface(rt_surface);
            // `slot` (still `MaybeUninit`) drops here, freeing the heap slot
            // without running `CoreSurface`'s teardown (`alloc.destroy`).
            return Err(InitError::Surface);
        }

        // Store it! (`:3477-3478`). Rebuild the `Box` over the SAME heap allocation
        // (now initialized) and LEAK it back into the atomic field, transferring
        // ownership of the stable heap slot to `core_surface` (reclaimed by
        // `Box::from_raw` in finalize). The published pointer is unchanged (the box
        // is the same `surface_ptr` address, already visible to the threads); this
        // step exists to move OWNERSHIP out of the local `slot`/`boxed` so it is not
        // freed at scope end.
        // SAFETY: `init_res` is `Ok`, so the slot's payload is fully initialized;
        // the raw pointer came from `Box::into_raw` and is reconstituted exactly
        // once as `Box<CoreSurface>` (layout-identical to `Box<MaybeUninit<_>>`).
        let boxed: Box<CoreSurface> =
            unsafe { Box::from_raw(Box::into_raw(slot).cast::<CoreSurface>()) };
        let boxed_ptr = Box::into_raw(boxed);
        debug_assert_eq!(
            boxed_ptr, surface_ptr,
            "the initialized box is the same pinned slot we pre-published"
        );
        imp.core_surface.store(boxed_ptr, Ordering::Release);

        // Give the render surface a pointer to the core surface so it
        // can pull presents from the renderer in its snapshot handler.
        // priv.render_surface.setCoreSurface(surface);   (`:3509-3511`)
        imp.render_surface.set_core_surface(boxed_ptr);

        // If the widget isn't realized yet, start the renderer in an
        // unrealized state so it waits for `displayRealized` (called from
        // `renderSurfaceRealize`) before building GPU resources.
        // if (priv.render_surface.as(gtk.Widget).getRealized() == 0) {
        //     surface.displayUnrealized();
        // }   (`:3513-3518`)
        if !imp.render_surface.is_realized() {
            // SAFETY: `boxed_ptr` is the live core surface just stored; raw-self
            // per the render-thread aliasing contract (W5).
            unsafe { CoreSurface::displayUnrealized(boxed_ptr) };
        }

        // Emit the signal that we initialized the surface (`:3520-3526`).
        self.emit_by_name::<()>("init", &[]);

        // `self.updateFocus(priv.focused)` (`:3488`).
        self.update_focus(imp.focused.get());

        Ok(())
    }
}
