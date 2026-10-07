//! PORTS: ghostty src/apprt/gtk/class/surface.zig — the template callbacks
//! (`surface.zig:3623-3661`), bound through GTK's C builder scope by
//! [`Surface::bind_template_callbacks`] (NEW-FINDINGS N516).

use super::*;

impl Surface {
    /// PORTS: `surface.zig:3838-3882` — the 45 `bindTemplateCallback` calls of
    /// `Class.init`, bound through GTK's C builder scope (NEW-FINDINGS N516; see
    /// [`template_callback`](crate::gtk::class::template_callback)). Each line
    /// is the reference callback's C signature.
    pub(super) fn bind_template_callbacks(klass: &mut <imp::Surface as ObjectSubclass>::Class) {
        crate::gtk::class::template_callback::bind_template_callbacks!(klass;
            c"focus_enter" => Surface::focus_enter,
                (ec: obj(gtk4::EventControllerFocus), this: obj(Surface));
            c"focus_leave" => Surface::focus_leave,
                (ec: obj(gtk4::EventControllerFocus), this: obj(Surface));
            c"key_pressed" => Surface::key_pressed, (
                ec: obj(gtk4::EventControllerKey), keyval: prim(u32), keycode: prim(u32),
                mods: glib(gdk4::ModifierType), this: obj(Surface),
            ) -> glib(bool);
            c"key_released" => Surface::key_released, (
                ec: obj(gtk4::EventControllerKey), keyval: prim(u32), keycode: prim(u32),
                mods: glib(gdk4::ModifierType), this: obj(Surface),
            );
            c"mouse_down" => Surface::mouse_down, (
                gesture: obj(gtk4::GestureClick), n_press: prim(i32), x: prim(f64), y: prim(f64),
                this: obj(Surface),
            );
            c"mouse_up" => Surface::mouse_up, (
                gesture: obj(gtk4::GestureClick), n_press: prim(i32), x: prim(f64), y: prim(f64),
                this: obj(Surface),
            );
            c"mouse_motion" => Surface::mouse_motion,
                (ec: obj(gtk4::EventControllerMotion), x: prim(f64), y: prim(f64), this: obj(Surface));
            c"mouse_leave" => Surface::mouse_leave,
                (ec: obj(gtk4::EventControllerMotion), this: obj(Surface));
            c"scroll_vertical" => Surface::scroll_vertical,
                (ec: obj(gtk4::EventControllerScroll), x: prim(f64), y: prim(f64), this: obj(Surface))
                -> glib(bool);
            c"scroll_vertical_begin" => Surface::scroll_vertical_begin,
                (ec: obj(gtk4::EventControllerScroll), this: obj(Surface));
            c"scroll_vertical_end" => Surface::scroll_vertical_end,
                (ec: obj(gtk4::EventControllerScroll), this: obj(Surface));
            c"scroll_horizontal" => Surface::scroll_horizontal,
                (ec: obj(gtk4::EventControllerScroll), x: prim(f64), y: prim(f64), this: obj(Surface))
                -> glib(bool);
            c"drop" => Surface::drop, (
                target: obj(gtk4::DropTarget), value: value(), x: prim(f64), y: prim(f64),
                this: obj(Surface),
            ) -> glib(bool);
            c"render_surface_realize" => Surface::render_surface_realize,
                (render_surface: obj(RenderSurface), this: obj(Surface));
            c"render_surface_unrealize" => Surface::render_surface_unrealize,
                (render_surface: obj(RenderSurface), this: obj(Surface));
            c"render_surface_map" => Surface::render_surface_map,
                (render_surface: obj(RenderSurface), this: obj(Surface));
            c"render_surface_unmap" => Surface::render_surface_unmap,
                (render_surface: obj(RenderSurface), this: obj(Surface));
            c"render_surface_resize" => Surface::render_surface_resize, (
                render_surface: obj(RenderSurface), width: prim(i32), height: prim(i32),
                this: obj(Surface),
            );
            c"im_preedit_start" => Surface::im_preedit_start,
                (ctx: obj(gtk4::IMMulticontext), this: obj(Surface));
            c"im_preedit_changed" => Surface::im_preedit_changed,
                (ctx: obj(gtk4::IMMulticontext), this: obj(Surface));
            c"im_preedit_end" => Surface::im_preedit_end,
                (ctx: obj(gtk4::IMMulticontext), this: obj(Surface));
            c"im_commit" => Surface::im_commit,
                (ctx: obj(gtk4::IMMulticontext), bytes: bytes(), this: obj(Surface));
            c"url_mouse_enter" => Surface::url_mouse_enter,
                (ec: obj(gtk4::EventControllerMotion), x: prim(f64), y: prim(f64), this: obj(Surface));
            c"url_mouse_leave" => Surface::url_mouse_leave,
                (ec: obj(gtk4::EventControllerMotion), this: obj(Surface));
            c"child_exited_close" => Surface::child_exited_close,
                (child_exited: obj(SurfaceChildExited), this: obj(Surface));
            c"context_menu_closed" => Surface::context_menu_closed,
                (popover: obj(gtk4::PopoverMenu), this: obj(Surface));
            c"notify_config" => Surface::on_notify_config, (this: obj(Surface));
            c"notify_error" => Surface::on_notify_error, (this: obj(Surface));
            c"notify_mouse_hover_url" => Surface::on_notify_mouse_hover_url, (this: obj(Surface));
            c"notify_mouse_hidden" => Surface::on_notify_mouse_hidden, (this: obj(Surface));
            c"notify_mouse_shape" => Surface::on_notify_mouse_shape, (this: obj(Surface));
            c"notify_vadjustment" => Surface::on_notify_vadjustment, (this: obj(Surface));
            c"should_border_be_shown" => Surface::should_border_be_shown,
                (this: obj(Surface), config: opt_obj(Config), bell_ringing: boolean()) -> glib(bool);
            c"should_unfocused_split_be_shown" => Surface::should_unfocused_split_be_shown, (
                this: obj(Surface), search_active: boolean(), focused: boolean(), is_split: boolean(),
            ) -> glib(bool);
            c"search_stop" => Surface::search_stop, (overlay: obj(SearchOverlay), this: obj(Surface));
            c"search_changed" => Surface::search_changed,
                (overlay: obj(SearchOverlay), needle: opt_str(), this: obj(Surface));
            c"search_next_match" => Surface::search_next_match,
                (overlay: obj(SearchOverlay), this: obj(Surface));
            c"search_previous_match" => Surface::search_previous_match,
                (overlay: obj(SearchOverlay), this: obj(Surface));
            c"should_drag_handle_be_shown" => Surface::should_drag_handle_be_shown,
                (this: obj(Surface), config: opt_obj(Config), is_split: boolean()) -> glib(bool);
            c"surface_drag_prepare" => Surface::surface_drag_prepare,
                (src: obj(gtk4::DragSource), x: prim(f64), y: prim(f64), this: obj(Surface))
                -> full(gdk4::ContentProvider);
            c"surface_drag_begin" => Surface::surface_drag_begin,
                (src: obj(gtk4::DragSource), drag: obj(gdk4::Drag), this: obj(Surface));
            c"surface_drop" => Surface::surface_drop, (
                target: obj(gtk4::DropTarget), value: value(), x: prim(f64), y: prim(f64),
                this: obj(Surface),
            ) -> glib(bool);
            c"surface_drop_leave" => Surface::surface_drop_leave,
                (target: obj(gtk4::DropTarget), this: obj(Surface));
            c"surface_drop_motion" => Surface::surface_drop_motion,
                (target: obj(gtk4::DropTarget), x: prim(f64), y: prim(f64), this: obj(Surface))
                -> glib(gdk4::DragAction);
            c"notify_drop_value" => Surface::notify_drop_value,
                (target: obj(gtk4::DropTarget), pspec: obj(glib::ParamSpec), this: obj(Surface));
        );
    }

    // -- EventControllerFocus (surface.blp:47-50) --------------------------
    // PORT NOTE (arg order): these controllers are connected NON-swapped in the
    // blueprint, so GTK calls the C callback with the EMITTING controller
    // first, the signal arguments, and the template instance (`this: &Surface`)
    // LAST, the reference's Zig callback signatures (e.g.
    // `ecKeyPressed(ec_key, keyval, keycode, gtk_mods, self)`).
    /// PORTS: `surface.zig:2746-2748` `ecFocusEnter`.
    fn focus_enter(_ec: &gtk4::EventControllerFocus, this: &Surface) {
        this.update_focus(true);
    }
    /// PORTS: `surface.zig:2750-2752` `ecFocusLeave`.
    fn focus_leave(_ec: &gtk4::EventControllerFocus, this: &Surface) {
        this.update_focus(false);
    }

    // -- EventControllerKey (surface.blp:52-55) ----------------------------
    /// PORTS: `surface.zig:2714-2728` `ecKeyPressed`.
    fn key_pressed(
        ec_key: &gtk4::EventControllerKey,
        keyval: u32,
        keycode: u32,
        gtk_mods: gdk4::ModifierType,
        this: &Surface,
    ) -> bool {
        this.key_event(Action::Press, ec_key, keyval, keycode, gtk_mods)
    }
    /// PORTS: `surface.zig:2730-2744` `ecKeyReleased`.
    fn key_released(
        ec_key: &gtk4::EventControllerKey,
        keyval: u32,
        keycode: u32,
        state: gdk4::ModifierType,
        this: &Surface,
    ) {
        let _ = this.key_event(Action::Release, ec_key, keyval, keycode, state);
    }

    // -- GestureClick (surface.blp:74-78) ----------------------------------
    /// PORTS: `surface.zig:2785-2857` `gcMouseDown`.
    fn mouse_down(gesture: &gtk4::GestureClick, _n_press: i32, x: f64, y: f64, this: &Surface) {
        let imp = this.imp();
        let Some(event) = gesture.current_event() else {
            return;
        };

        // Bell stops ringing if any mouse button is pressed (`:2794-2795`).
        this.set_bell_ringing_inner(false);

        // Get our surface. If we don't have one, ignore this (`:2797-2799`).
        let Some(core) = this.core() else {
            return;
        };

        // If we don't have focus, grab it (`:2853-2858`).
        let widget = imp.render_surface.upcast_ref::<gtk4::Widget>();
        let had_focus = widget.has_focus();
        if !had_focus {
            widget.grab_focus();
        }

        // Report the event (`:2808-2809`).
        let button = translate_mouse_button(gesture.current_button());

        // If this click is only transitioning split focus, suppress it so it
        // doesn't get forwarded to the terminal as a mouse event (`:2811-2816`).
        if !had_focus && button == Button::Left {
            imp.suppress_left_mouse_release.set(true);
            return;
        }

        // (488b7109b) `button == .middle and !priv.gtk_enable_primary_paste and
        // !core_surface.mouseReportingActive()`: middle clicks are forwarded to
        // the TUI when it is actively reporting mouse events, even with primary
        // paste disabled.
        //
        // SAFETY: `core` is the widget's boxed, initialized core surface.
        if button == Button::Middle
            && !imp.gtk_enable_primary_paste.get()
            && !unsafe { (*core).mouseReportingActive() }
        {
            return;
        }

        let mods = key::translate_mods(event.modifier_state());
        // SAFETY: `core` is the widget's boxed, initialized core surface.
        let consumed =
            match unsafe { (*core).mouseButtonCallback(ButtonState::Press, button, mods) } {
                Ok(consumed) => consumed,
                Err(err) => {
                    // log.warn("error in key callback err={}", .{err});
                    log::warn!(target: LOG_SCOPE, "error in key callback err={err:?}");
                    false
                }
            };

        // If a right click isn't consumed, mouseButtonCallback selects the
        // hovered word and returns false. We can use this to handle the context
        // menu opening under normal scenarios (`:2835-2856`).
        if !consumed && button == Button::Right {
            this.emit_by_name::<()>("menu", &[]);

            let rect = gdk4::Rectangle::new(x as i32, y as i32, 1, 1);
            imp.context_menu.set_pointing_to(Some(&rect));
            imp.context_menu.popup();
        }
    }
    /// PORTS: `surface.zig:2859-2902` `gcMouseUp`.
    fn mouse_up(gesture: &gtk4::GestureClick, _n_press: i32, _x: f64, _y: f64, this: &Surface) {
        let imp = this.imp();
        let Some(event) = gesture.current_event() else {
            return;
        };
        let Some(core) = this.core() else {
            return;
        };
        let button = translate_mouse_button(gesture.current_button());

        if button == Button::Left && imp.suppress_left_mouse_release.get() {
            imp.suppress_left_mouse_release.set(false);
            return;
        }

        // (488b7109b) see the matching `mouse_down` guard: forward middle-click
        // releases to a mouse-reporting TUI.
        //
        // SAFETY: `core` is the widget's boxed, initialized core surface.
        if button == Button::Middle
            && !imp.gtk_enable_primary_paste.get()
            && !unsafe { (*core).mouseReportingActive() }
        {
            return;
        }

        let mods = key::translate_mods(event.modifier_state());
        // SAFETY: `core` is the widget's boxed, initialized core surface.
        let consumed =
            match unsafe { (*core).mouseButtonCallback(ButtonState::Release, button, mods) } {
                Ok(consumed) => consumed,
                Err(err) => {
                    // log.warn("error in key callback err={}", .{err});
                    log::warn!(target: LOG_SCOPE, "error in key callback err={err:?}");
                    return;
                }
            };

        // Trigger the on-screen keyboard if we have no selection,
        // and that the mouse event hasn't been intercepted by the callback.
        //
        // It's better to do this here rather than within the core callback
        // since we have direct access to the underlying gdk.Event here.
        // (`:2892-2901`)
        // SAFETY: `core` is the initialized core surface.
        if !consumed
            && button == Button::Left
            && !unsafe { (*core).hasSelection() }
            && !this.show_on_screen_keyboard(Some(&event))
        {
            log::warn!(target: LOG_SCOPE, "failed to activate the on-screen keyboard");
        }
    }

    // -- EventControllerMotion (surface.blp:69-72) -------------------------
    /// PORTS: `surface.zig:2904-2953` `ecMouseMotion`.
    fn mouse_motion(ec: &gtk4::EventControllerMotion, x: f64, y: f64, this: &Surface) {
        let imp = this.imp();
        let Some(event) = ec.current_event() else {
            return;
        };

        let (sx, sy) = this.scaled_coordinates(x, y);
        let pos = CursorPos {
            x: sx as f32,
            y: sy as f32,
        };

        // There seem to be at least two cases where GTK issues a mouse motion
        // event without the cursor actually moving:
        // 1. GLArea is resized under the mouse. This has the unfortunate
        //    side effect of causing focus to potentially change when
        //    `focus-follows-mouse` is enabled.
        // 2. The window title is updated. This can cause the mouse to unhide
        //    incorrectly when hide-mouse-when-typing is enabled.
        // To prevent incorrect behavior, we'll only grab focus and
        // continue with callback logic if the cursor has actually moved.
        // (`:2919-2930`)
        let cur = imp.cursor_pos.get();
        let is_cursor_still = (cur.x - pos.x).abs() < 1.0 && (cur.y - pos.y).abs() < 1.0;
        if is_cursor_still {
            return;
        }

        // If we don't have focus, and we want it, grab it (`:2932-2940`).
        {
            let cfg = imp.config.borrow();
            if let Some(config) = cfg.as_ref() {
                let widget = imp.render_surface.upcast_ref::<gtk4::Widget>();
                if !widget.has_focus() && config.get().focus_follows_mouse {
                    widget.grab_focus();
                }
            }
        }

        // Our pos changed, update (`:2942-2943`).
        imp.cursor_pos.set(pos);

        // Notify the callback (`:2945-2952`).
        if let Some(core) = this.core() {
            let mods = key::translate_mods(event.modifier_state());
            // SAFETY: `core` is the widget's boxed, initialized core surface.
            if let Err(err) = unsafe { (*core).cursorPosCallback(pos, Some(mods)) } {
                // log.warn("error in cursor pos callback err={}", .{err});
                log::warn!(target: LOG_SCOPE, "error in cursor pos callback err={err:?}");
            }
        }
    }
    /// PORTS: `surface.zig:2955-2976` `ecMouseLeave`.
    fn mouse_leave(ec_motion: &gtk4::EventControllerMotion, this: &Surface) {
        let Some(event) = ec_motion.current_event() else {
            return;
        };

        // If we have a core surface then we can send the cursor pos
        // callback with an invalid position to indicate the mouse left.
        // (`:2962-2975`)
        if let Some(core) = this.core() {
            let mods = key::translate_mods(event.modifier_state());
            let pos = CursorPos { x: -1.0, y: -1.0 };
            // SAFETY: `core` is the widget's boxed, initialized core surface.
            if let Err(err) = unsafe { (*core).cursorPosCallback(pos, Some(mods)) } {
                // log.warn("error in cursor pos callback err={}", .{err});
                log::warn!(target: LOG_SCOPE, "error in cursor pos callback err={err:?}");
            }
        }
    }

    // -- EventControllerScroll (surface.blp:57-67) -------------------------
    /// PORTS: `surface.zig:2992-3023` `ecMouseScrollVertical`.
    fn scroll_vertical(_ec: &gtk4::EventControllerScroll, x: f64, y: f64, this: &Surface) -> bool {
        let imp = this.imp();
        let Some(core) = this.core() else {
            return false;
        };

        // Multiply precision scrolls by 10 to get a better response from touchpad
        // scrolling (`:3001-3006`).
        let precision = imp.precision_scroll.get();
        let multiplier: f64 = if precision { 10.0 } else { 1.0 };
        let scroll_mods = ScrollMods {
            precision,
            ..ScrollMods::default()
        };

        let (sx, sy) = this.scaled_coordinates(x, y);
        // We invert because we apply natural scrolling to the values.
        // This behavior has existed for years without Linux users complaining
        // but I suspect we'll have to make this configurable in the future
        // or read a system setting.
        // (`:3009-3016`); written `-x * m` to satisfy clippy::neg_multiply.
        // SAFETY: `core` is the widget's boxed, initialized core surface.
        if let Err(err) =
            unsafe { (*core).scrollCallback(-sx * multiplier, -sy * multiplier, scroll_mods) }
        {
            // log.warn("error in scroll callback err={}", .{err});
            log::warn!(target: LOG_SCOPE, "error in scroll callback err={err:?}");
            return false;
        }

        true
    }
    /// PORTS: `surface.zig:2978-2983` `ecMouseScrollVerticalPrecisionBegin`.
    fn scroll_vertical_begin(_ec: &gtk4::EventControllerScroll, this: &Surface) {
        this.imp().precision_scroll.set(true);
    }
    /// PORTS: `surface.zig:2985-2990` `ecMouseScrollVerticalPrecisionEnd`.
    fn scroll_vertical_end(_ec: &gtk4::EventControllerScroll, this: &Surface) {
        this.imp().precision_scroll.set(false);
    }
    /// PORTS: `surface.zig:3025-3080` `ecMouseScrollHorizontal` (oracle
    /// `5ea45919d`).
    ///
    /// PORT NOTE (`P3-W11 T5` = `P3-W8 T5` Step 3): the tab-switch accumulator
    /// used to run unconditionally for `unit == .surface`, and any other unit
    /// was DROPPED. It now runs only when `gtk-horizontal-tab-scroll` is on AND
    /// the unit is `.surface` (i.e. a touchpad); otherwise the horizontal
    /// scroll is FORWARDED to the terminal, so applications like neovim receive
    /// it instead of it being swallowed. The predicate is extracted into
    /// [`Self::horizontal_tab_scroll_applies`] so it is testable without a
    /// widget — a test of only the accumulator arm passes without the new
    /// behaviour.
    fn scroll_horizontal(
        ec: &gtk4::EventControllerScroll,
        x: f64,
        _y: f64,
        this: &Surface,
    ) -> bool {
        let imp = this.imp();

        // Check if horizontal tab scrolling is enabled and this is a
        // touchpad surface scroll. If not, forward to the terminal.
        //
        // `const tab_scroll_enabled = if (priv.config) |config|
        //      config.get().@"gtk-horizontal-tab-scroll" else true;`
        let tab_scroll_enabled = {
            let config_ref = imp.config.borrow();
            match config_ref.as_ref() {
                Some(config) => config.get().gtk_horizontal_tab_scroll,
                None => true,
            }
        };

        if !Self::horizontal_tab_scroll_applies(tab_scroll_enabled, ec.unit()) {
            // Forward horizontal scroll to the terminal (e.g. for neovim).
            // `const surface = priv.core_surface orelse return @intFromBool(false);`
            let Some(core) = this.core() else {
                return false;
            };
            // `const scaled = self.scaledCoordinates(x, 0);`
            let (sx, _) = this.scaled_coordinates(x, 0.0);
            // `surface.scrollCallback(scaled.x * -1, 0, .{})` — written `-sx`
            // to satisfy clippy::neg_multiply, as in `scroll_vertical`.
            //
            // SAFETY: `core` is the widget's boxed, initialized core surface.
            if let Err(err) = unsafe { (*core).scrollCallback(-sx, 0.0, ScrollMods::default()) } {
                // log.warn("error in scroll callback err={}", .{err});
                log::warn!(target: LOG_SCOPE, "error in scroll callback err={err:?}");
                return false;
            }
            return true;
        }

        let pending = imp.pending_horizontal_scroll.get() + x;
        imp.pending_horizontal_scroll.set(pending);

        if pending.abs() < 120.0 {
            // if (priv.pending_horizontal_scroll_reset) |v| {
            //     _ = glib.Source.remove(v);
            //     priv.pending_horizontal_scroll_reset = null;
            // }
            discard_source_id(imp.pending_horizontal_scroll_reset.take());

            // The reset source clears the accumulator after 500ms of inactivity
            // (`:3046`). A weak backref keeps the reference's non-owning semantics
            // (the source is removed in `dispose`, **invariant 11**).
            let weak = this.downgrade();
            let id = glib::timeout_add_local(Duration::from_millis(500), move || {
                if let Some(this) = weak.upgrade() {
                    let imp = this.imp();
                    imp.pending_horizontal_scroll.set(0.0);
                    imp.pending_horizontal_scroll_reset.replace(None);
                }
                glib::ControlFlow::Break
            });
            imp.pending_horizontal_scroll_reset.replace(Some(id));
            return true;
        }

        let action_name = if pending < 0.0 {
            "tab.next-page"
        } else {
            "tab.previous-page"
        };
        let _ = this
            .upcast_ref::<gtk4::Widget>()
            .activate_action(action_name, None);

        // if (priv.pending_horizontal_scroll_reset) |v| {
        //     _ = glib.Source.remove(v);
        //     priv.pending_horizontal_scroll_reset = null;
        // }
        discard_source_id(imp.pending_horizontal_scroll_reset.take());
        imp.pending_horizontal_scroll.set(0.0);

        true
    }

    /// The `5ea45919d` predicate, extracted so it is testable without a widget:
    /// the tab-switch accumulator runs only when the `gtk-horizontal-tab-scroll`
    /// option is on AND the scroll unit is `.surface` (a touchpad two-finger
    /// scroll). Every other combination forwards the event to the terminal.
    pub(super) fn horizontal_tab_scroll_applies(
        tab_scroll_enabled: bool,
        unit: gdk4::ScrollUnit,
    ) -> bool {
        tab_scroll_enabled && matches!(unit, gdk4::ScrollUnit::Surface)
    }

    // -- DropTarget (surface.blp:224-227) ----------------------------------
    /// PORTS: `surface.zig:2692-2777` `dtDrop`.
    ///
    /// NON-SWAPPED template callback (`surface.blp:225` `drop => $drop()`): the
    /// `DropTarget` emitter is arg0 and the `Surface` template instance is the
    /// trailing user-data arg (mirrors `focus_enter`/`mouse_down`; the
    /// reference's `callconv(.c) fn(_: *gtk.DropTarget, value, x, y, self)`).
    ///
    /// Files can be dropped from Nautilus and strings from many programs; the
    /// drop-target gtypes are set IN ORDER FileList, File, string
    /// (`constructed`, `:1845-1850`), and we branch on the value's runtime type
    /// with [`ext::g_value_holds`] in the same order (`:2701`, `:2743`, `:2769`).
    ///
    /// CORRECTION (task L-4b): the line numbers in this handler's comments
    /// predated the pin (`:2627-2718`, `:1818-1826`, `:2636`, `:2678`, `:2703`,
    /// ...); they are now the lines at `20abdb50a`.
    fn drop(
        _target: &gtk4::DropTarget,
        value: &glib::Value,
        _x: f64,
        _y: f64,
        this: &Surface,
    ) -> bool {
        use rustty_os::string_encoding::{AllocatingWriter, Writer};
        use rustty_os::ShellEscapeWriter;
        use std::os::unix::ffi::OsStrExt;

        // `value: *gobject.Value`: GTK's C marshaller hands the C callback the
        // dropped value itself (the signal's `G_TYPE_VALUE` parameter, peeked
        // as a pointer), as in the reference (N338, N516).

        // gdk.FileList: shell-escape each file path and join with newlines, then
        // paste the whole blob (`:2701-2741`).
        if ext::g_value_holds(value, gdk4::FileList::static_type()) {
            let Ok(list) = value.get::<gdk4::FileList>() else {
                return false;
            };

            // `var stream: Allocating` + `ShellEscapeWriter.init(&stream.writer)`
            // (`:2702-2706`). The escape writer excludes linefeeds so newlines
            // delimit the list of paths.
            let mut stream = AllocatingWriter::init();
            {
                let mut shell_escape_writer = ShellEscapeWriter::init(&mut stream);

                // `while (current) |item| : file.getPath()` (`:2716-2731`).
                for file in list.files() {
                    let Some(path) = file.path() else {
                        continue;
                    };
                    if let Err(err) = shell_escape_writer.writeAll(path.as_os_str().as_bytes()) {
                        // log.err("unable to write path to buffer: {}", .{err});
                        log::error!(target: LOG_SCOPE, "unable to write path to buffer: {err:?}");
                        continue;
                    }
                    if let Err(err) = shell_escape_writer.writeAll(b"\n") {
                        // log.err("unable to write to buffer: {}", .{err});
                        log::error!(target: LOG_SCOPE, "unable to write to buffer: {err:?}");
                        continue;
                    }
                }
            }

            // `Clipboard.paste(self, string)` (`:2739`).
            this.paste(stream.written());
            return true;
        }

        // gio.File: shell-escape the single path (`:2743-2766`).
        //
        // PORT NOTE: reference defect (R13), `UPSTREAM-DEFECTS.md` UD321. The
        // reference builds the escaped path and then returns 1 without a
        // `Clipboard.paste` (`:2761-2766`), so the drop is accepted and the path
        // silently lost, where the file-list branch above pastes the same
        // escaped text. Reachable by a drag source that offers a single `GFile`
        // but no `GdkFileList` (the list type is tried first). The port pastes
        // it, as the file-list branch does (controller ruling, task L-4b).
        // CORRECTION (task L-4b): this said "we mirror that — the byte-faithful
        // behaviour is to consume the drop without inserting".
        if ext::g_value_holds(value, gio::File::static_type()) {
            let Ok(file) = value.get::<gio::File>() else {
                return false;
            };
            let Some(path) = file.path() else {
                return false;
            };
            let mut stream = AllocatingWriter::init();
            {
                let mut shell_escape_writer = ShellEscapeWriter::init(&mut stream);
                if let Err(err) = shell_escape_writer.writeAll(path.as_os_str().as_bytes()) {
                    // log.err("unable to write path to buffer: {}", .{err});
                    log::error!(target: LOG_SCOPE, "unable to write path to buffer: {err:?}");
                    return false;
                }
                if let Err(err) = shell_escape_writer.writeAll(b"\n") {
                    // log.err("unable to write to buffer: {}", .{err});
                    log::error!(target: LOG_SCOPE, "unable to write to buffer: {err:?}");
                    return false;
                }
            }
            this.paste(stream.written());
            return true;
        }

        // string: paste it verbatim (`:2769-2774`).
        //
        // PORT NOTE (task L-4, NEW-FINDINGS N337): `value.getString()` is a
        // `?[*:0]const u8` and `std.mem.span` takes its bytes as they are. This
        // was `value.get::<String>()`, which glib-rs builds with
        // `CStr::to_str().expect("Invalid UTF-8")` (glib 0.22 `value.rs:898`):
        // a `G_TYPE_STRING` value holding bytes that are not UTF-8 panicked
        // inside this C callback and aborted the process (MEASURED on the VM by
        // `drop_pastes_a_non_utf8_string_verbatim`). Whether GTK itself can
        // deliver such a value (its text deserializers convert through a
        // charset converter) is UNVERIFIED; the bytes are now read raw
        // ([`raw_string_bytes`]), as the reference reads them, and NULL is the
        // reference's `orelse`.
        if ext::g_value_holds(value, String::static_type()) {
            if let Some(string) = raw_string_bytes(value) {
                this.paste(&string);
            }
            return true;
        }

        true
    }

    /// Paste explicit text directly into the surface, regardless of the
    /// actual clipboard contents.
    ///
    /// PORTS: `surface.zig:3898-3931` `Clipboard.paste`.
    /// Mirrors the Phase-A `clipboard_request` completion path (the core mutable
    /// deref + the `UnsafePaste`/`UnauthorizedPaste` → confirmation fallback).
    fn paste(&self, text: &[u8]) {
        #[cfg(test)]
        tests::LAST_PASTE.with(|l| *l.borrow_mut() = Some(text.to_vec()));
        // `if (text.len == 0) return;` (`:3903`).
        if text.is_empty() {
            return;
        }

        // `const surface = self.private().core_surface orelse return;` (`:3905`).
        let Some(core) = self.core() else {
            return;
        };

        // surface.completeClipboardRequest(
        //     .{ .paste = .standard },
        //     .{ .contents = &.{.{ .mime = "text/plain", .data = text }} },
        // ) catch |err| switch (err) { ... };
        // SAFETY: `core` is the widget's boxed, initialized core surface;
        // `completeClipboardRequest` takes `&mut self` (mirror the deref used by
        // the async `clipboard_request` completion above).
        match unsafe { &mut *core }.completeClipboardRequest(
            ClipboardRequest::Paste(Clipboard::Standard),
            rustty_core::surface::CompleteClipboard {
                contents: &[rustty_vt::clipboard::Content {
                    mime: b"text/plain",
                    data: text,
                }],
                ..rustty_core::surface::CompleteClipboard::default()
            },
        ) {
            Ok(()) => {}

            // error.UnsafePaste, error.UnauthorizedPaste => confirmation
            // (`:3911-3921`).
            Err(CoreSurfaceError::UnsafePaste | CoreSurfaceError::UnauthorizedPaste) => {
                self.show_clipboard_confirmation(
                    ClipboardRequest::Paste(Clipboard::Standard),
                    &String::from_utf8_lossy(text),
                );
            }

            Err(err) => {
                log::warn!(target: LOG_SCOPE, "failed to complete clipboard request err={err}");
            }
        }
    }

    // -- RenderSurface lifecycle (surface.blp:23-32) -----------------------
    // These are the RenderSurface CHILD's signals, connected NON-swapped as in
    // the reference's blueprint: the emitting RenderSurface first, the signal
    // arguments, then the template instance (`this`), the reference's
    // `renderSurfaceRealize(_: *RenderSurface, self: *Self)` signatures. (The
    // port's blueprint carried `swapped` for gtk4-rs's `&self` handlers until
    // N516 moved the callbacks to GTK's C scope.) Ported from
    // `surface.zig:3305-3437` (`2f0b65346`).

    /// PORTS: `surface.zig:3305-3324` `renderSurfaceRealize`.
    fn render_surface_realize(_render_surface: &RenderSurface, this: &Surface) {
        log::debug!(target: LOG_SCOPE, "render surface realize");

        // Notify our core surface that it should realize.
        let imp = this.imp();
        if let Some(core) = this.core() {
            // SAFETY: `core` is the widget's boxed, initialized core surface; the
            // renderer is aliased raw by the render thread (W5), so use raw-this.
            if let Err(err) = unsafe { CoreSurface::displayRealized(core) } {
                // log.warn("core displayRealized failed err={}", .{err});
                log::warn!(target: LOG_SCOPE, "core displayRealized failed err={err:?}");
            }
            this.redraw();
        }

        // Setup our input method. We do this here because this will
        // create a strong reference back to ourself and we want to be
        // able to release that in unrealize.
        imp.im_context.set_client_widget(Some(this));
    }

    /// PORTS: `surface.zig:3326-3341` `renderSurfaceUnrealize`.
    fn render_surface_unrealize(_render_surface: &RenderSurface, this: &Surface) {
        log::debug!(target: LOG_SCOPE, "render surface unrealize");

        let imp = this.imp();
        if let Some(core) = this.core() {
            // SAFETY: as in `render_surface_realize`.
            unsafe { CoreSurface::displayUnrealized(core) };
        }

        // Unset our input method
        imp.im_context.set_client_widget(gtk4::Widget::NONE);
    }

    /// PORTS: `surface.zig:3343-3349` `renderSurfaceMap`.
    fn render_surface_map(_render_surface: &RenderSurface, this: &Surface) {
        this.update_mapped(true);
        this.update_occlusion();
    }

    /// PORTS: `surface.zig:3351-3357` `renderSurfaceUnmap`.
    fn render_surface_unmap(_render_surface: &RenderSurface, this: &Surface) {
        this.update_mapped(false);
        this.update_occlusion();
    }

    // (`2f0b65346` deleted `glareaRender`: nothing draws on this thread.)

    /// `renderSurfaceResize`'s debug block (`surface.zig:3386-3396`).
    ///
    /// ```zig
    /// {
    ///     const widget = self.private().render_surface.as(gtk.Widget);
    ///     log.debug("gl resize width={} height={} scale={d} scale_factor={}", .{
    ///         width,
    ///         height,
    ///         scale_util.widgetSurfaceScale(widget),
    ///         widget.getScaleFactor(),
    ///     });
    /// }
    /// ```
    pub(super) fn render_surface_resize_debug_log(&self, width: i32, height: i32) {
        let widget = self.imp().render_surface.upcast_ref::<gtk4::Widget>();
        log::debug!(
            target: LOG_SCOPE,
            "gl resize width={width} height={height} scale={} scale_factor={}",
            scale::widget_surface_scale(widget),
            widget.scale_factor(),
        );
    }

    /// PORTS: `surface.zig:3380-3436` `renderSurfaceResize`. Caches the size,
    /// drives the content-scale / size callbacks when the surface exists, and
    /// otherwise LAZILY builds the core surface (**invariant 3** — on first
    /// resize, not realize, so we have proper initial dimensions). The size is in
    /// device pixels (`RenderSurface`'s `resize` signal).
    fn render_surface_resize(
        _render_surface: &RenderSurface,
        width: i32,
        height: i32,
        this: &Surface,
    ) {
        let imp = this.imp();

        // Some debug output to help understand what GTK is telling us.
        this.render_surface_resize_debug_log(width, height);

        // Store our cached size
        // PORT NOTE (R7): `@intCast(width)` of a `c_int` into `u32` traps on a
        // negative value.
        let new_size = SurfaceSize {
            width: u32::try_from(width).expect("negative resize width"),
            height: u32::try_from(height).expect("negative resize height"),
        };
        let changed = !imp.size.get().eql(&new_size);
        imp.size.set(new_size);

        // If our surface is initialized, we send callbacks.
        if let Some(core) = this.core() {
            // We also update the content scale because there is no signal for
            // content scale change and it seems to trigger a resize event.
            // SAFETY: `core` is the widget's boxed, initialized core surface.
            if let Err(err) = unsafe { &mut *core }.contentScaleCallback(this.get_content_scale()) {
                // log.warn("error in content scale callback err={}", .{err});
                log::warn!(target: LOG_SCOPE, "error in content scale callback err={err:?}");
            }

            if changed {
                // SAFETY: as above.
                if let Err(err) = unsafe { &mut *core }.sizeCallback(new_size) {
                    // log.warn("error in size callback err={}", .{err});
                    log::warn!(target: LOG_SCOPE, "error in size callback err={err:?}");
                }
                // Setup our resize overlay if configured
                this.resize_overlay_schedule();
            }
        } else {
            // If we haven't initalized a surface yet, now's the time to
            // do so. We cannot do this any earlier since GTK defers the
            // layouting and size allocation until after widget initialization.
            // Note that initialization is different from realization:
            // `initSurface` will start the core surface in an unrealized
            // state and wait for the `realize` signal, unless the widget
            // is somehow realized before the first resize signal ever fires.
            if let Err(err) = this.init_surface() {
                // log.warn("surface failed to initialize err={}", .{err});
                log::warn!(target: LOG_SCOPE, "surface failed to initialize err={err:?}");

                // PORT NOTE: reference defect (R13), UD356. `2f0b65346` deleted
                // the only `setError(true)` (the pin's GLArea context check), so a
                // surface whose renderer cannot initialize (no EGL, no 4.3 core
                // context) is left blank; the error page exists to report exactly
                // that. The port shows it here, where the failure lands.
                this.set_error(true);
            }
        }
    }

    // -- IMMulticontext (surface.blp:253-256) ------------------------------
    /// PORTS: `surface.zig:3076-3087` `imPreeditStart`.
    fn im_preedit_start(_ctx: &gtk4::IMMulticontext, this: &Surface) {
        // log.warn("GTKIM: preedit start", .{});

        // Start our composing state for the input method and reset our
        // input buffer to empty. (`:3082-3086`)
        let mut ime = this.imp().ime.borrow_mut();
        ime.im_composing = true;
        ime.im_len = 0;
    }
    /// PORTS: `surface.zig:3089-3126` `imPreeditChanged`.
    fn im_preedit_changed(ctx: &gtk4::IMMulticontext, this: &Surface) {
        // Any preedit change should mark that we're composing. Its possible this
        // is false using fcitx5-hangul and typing "dkssud<space>" ("안녕"). The
        // second "s" results in a "commit" for "안" which sets composing to false,
        // but then immediately sends a preedit change for the next symbol. With
        // composing set to false we won't commit this text. Therefore, we must
        // ensure it is set here. (`:3095-3101`)
        this.imp().ime.borrow_mut().im_composing = true;

        // We can't set our preedit on our surface unless we're realized.
        // We do this now because we want to still keep our input method
        // state coherent. (`:3103-3106`)
        let Some(core) = this.core() else {
            return;
        };

        // Get our pre-edit string that we'll use to show the user (`:3108-3116`).
        let (preedit, _attrs, _pos) = ctx.preedit_string();

        // Update our preedit state in Rustty core
        // log.warn("GTKIM: preedit change str={s}", .{str});
        // (`:3118-3125`)
        // SAFETY: `core` is the widget's boxed, initialized core surface.
        if let Err(err) = unsafe { (*core).preeditCallback(Some(preedit.as_bytes())) } {
            // log.warn("error in preedit callback err={}", .{err});
            log::warn!(target: LOG_SCOPE, "error in preedit callback err={err:?}");
        }
    }
    /// PORTS: `surface.zig:3128-3143` `imPreeditEnd`.
    fn im_preedit_end(_ctx: &gtk4::IMMulticontext, this: &Surface) {
        // log.warn("GTKIM: preedit end", .{});

        // End our composing state for GTK, allowing us to commit the text.
        // (`:3134-3136`)
        this.imp().ime.borrow_mut().im_composing = false;

        // End our preedit state in Ghostty core (`:3138-3142`).
        let Some(core) = this.core() else {
            return;
        };
        // SAFETY: `core` is the widget's boxed, initialized core surface.
        if let Err(err) = unsafe { (*core).preeditCallback(None) } {
            // log.warn("error in preedit callback err={}", .{err});
            log::warn!(target: LOG_SCOPE, "error in preedit callback err={err:?}");
        }
    }
    /// PORTS: `surface.zig:3145-3226` `imCommit`. **Invariant 9**: how a commit
    /// event is handled depends on the current `in_keyevent` state — during a
    /// keypress a simple commit (e.g. "a") is BUFFERED for the key encoder rather
    /// than committed as IME text.
    pub(super) fn im_commit(_ctx: &gtk4::IMMulticontext, bytes: &[u8], this: &Surface) {
        let imp = this.imp();
        // `bytes: [*:0]u8`: the commit's raw bytes, passed through unchanged.
        let str_bytes = bytes;

        // log.debug("GTKIM: input commit composing={} keyevent={} str={s}", .{
        //     self.im_composing,
        //     self.in_keyevent,
        //     str,
        // });

        // We need to handle commit specially if we're in a key event.
        // Specifically, GTK will send us a commit event for basic key
        // encodings like "a" (on a US layout keyboard). We don't want
        // to treat this as IME committed text because we want to associate
        // it with a key event (i.e. "a" key press). (`:3159-3192`)
        let in_keyevent = imp.ime.borrow().in_keyevent;
        match in_keyevent {
            // If we're not in a key event then this commit is from
            // some other source (i.e. on-screen keyboard, tablet, etc.)
            // and we want to commit the text to the core surface. (`:3165-3168`)
            ImKeyEvent::False => {}

            // If we're in a composing state and in a key event then this
            // key event is resulting in a commit of multiple keypresses
            // and we don't want to encode it alongside the keypress. (`:3170-3173`)
            ImKeyEvent::Composing => {}

            // If we're not composing then this commit is just a normal
            // key encoding and we want our key event to handle it so
            // that Rustty can be aware of the key event alongside
            // the text. (`:3175-3191`,
            // `.not_composing`). See the [`ImKeyEvent`] naming note in `key_event`.
            ImKeyEvent::True => {
                let mut ime = imp.ime.borrow_mut();
                if str_bytes.len() > ime.im_buf.len() {
                    log::warn!(target: LOG_SCOPE, "not enough buffer space for input method commit");
                    return;
                }

                // Copy our committed text to the buffer (`:3185-3187`).
                ime.im_buf[..str_bytes.len()].copy_from_slice(str_bytes);
                // **Invariant 9**: `im_len ≤ 128`, the buffer's length (guarded
                // just above).
                //
                // PORT NOTE: reference defect (R13), UPSTREAM-DEFECTS.md UD90.
                // The reference's `im_len` is a `u7` (0..=127) but the guard
                // admits a commit of exactly `im_buf.len()` = 128 bytes, so
                // `@intCast(str.len)` traps in ReleaseSafe on a 128-byte commit
                // and truncates it to 0 in ReleaseFast (measured, Zig 0.16.0),
                // silently dropping the text. The input method (IBus, fcitx,
                // XCompose) decides the commit. The port's `u8` holds 128, so the
                // commit is buffered whole, which is what the guard intends.
                debug_assert!(str_bytes.len() <= ime.im_buf.len());
                ime.im_len = str_bytes.len() as u8;

                // log.debug("input commit len={}", .{priv.im_len});
                return;
            }
        }

        // If we reach this point from above it means we're composing OR
        // not in a keypress. In either case, we want to commit the text
        // given to us because that's what GTK is asking us to do. If we're
        // not in a keypress it means that this commit came via a non-keyboard
        // event (i.e. on-screen keyboard, tablet of some kind, etc.).
        // (`:3194-3198`)

        // Committing ends composing state (`:3200-3201`).
        imp.ime.borrow_mut().im_composing = false;

        // We can't set our preedit on our surface unless we're realized.
        // We do this now because we want to still keep our input method
        // state coherent. (`:3203-3225`)
        if let Some(core) = this.core() {
            // End our preedit state. Well-behaved input methods do this for us by
            // triggering a preedit-end event but some do not (ibus 1.5.29)
            // (`:3207-3211`).
            // SAFETY: `core` is the widget's boxed, initialized core surface.
            if let Err(err) = unsafe { (*core).preeditCallback(None) } {
                // log.warn("error in preedit callback err={}", .{err});
                log::warn!(target: LOG_SCOPE, "error in preedit callback err={err:?}");
            }

            // Send the text to the core surface, associated with no key (an
            // invalid key, which should produce no PTY encoding) (`:3213-3224`).
            let event = KeyEvent {
                action: Action::Press,
                key: Key::Unidentified,
                mods: Mods::default(),
                consumed_mods: Mods::default(),
                composing: false,
                utf8: str_bytes.to_vec(),
                unshifted_codepoint: 0,
            };
            // SAFETY: `core` is the widget's boxed, initialized core surface.
            if let Err(err) = unsafe { CoreSurface::keyCallback(core, event) } {
                // log.warn("error in key callback err={}", .{err});
                log::warn!(target: LOG_SCOPE, "error in key callback err={err:?}");
            }
        }
    }

    // -- URL hover (surface.blp:171-174) -----------------------------------
    // PORT NOTE (arg order): the `url_ec_motion` EventControllerMotion is connected
    // NON-swapped in the blueprint (`surface.blp:172-173`), so GTK calls the
    // callbacks with the emitting controller first and `this: &Surface` LAST,
    // the reference `callconv(.c)` signatures (`ecUrlMouseEnter`
    // takes `(_: *EventControllerMotion, _: f64, _: f64, self)`).
    /// PORTS: `surface.zig:3523-3532` `ecUrlMouseEnter`. When the pointer enters the
    /// `url_left` overlay label (bottom-left), reveal `url_right` (bottom-right) so
    /// the URL text moves to the opposite corner, out from under the cursor.
    fn url_mouse_enter(_ec: &gtk4::EventControllerMotion, _x: f64, _y: f64, this: &Surface) {
        this.imp().url_right.set_visible(true);
    }
    /// PORTS: `surface.zig:3534-3537` `ecUrlMouseLeave`. Hide `url_right` again when
    /// the pointer leaves `url_left`.
    fn url_mouse_leave(_ec: &gtk4::EventControllerMotion, this: &Surface) {
        this.imp().url_right.set_visible(false);
    }

    // -- child-exited / context menu ---------------------------------------
    fn child_exited_close(_child_exited: &SurfaceChildExited, this: &Surface) {
        // PORTS: `surface.zig:2600-2603` `childExitedClose(_: *ChildExited, self)`.
        // This closes the surface with no confirmation.
        // NON-swapped signal: the emitting
        // ChildExited overlay is arg 0, `this: &Surface` is last (PORT NOTE above).
        this.close();
    }
    fn context_menu_closed(_popover: &gtk4::PopoverMenu, this: &Surface) {
        // PORTS: `surface.zig:2608-2618` `contextMenuClosed(_: *gtk.PopoverMenu, self)`.
        // When the context menu closes, it moves focus back to the tab
        // bar if there are tabs. That's not correct. We need to grab it
        // on the surface.
        // NON-swapped signal: the emitting PopoverMenu is arg 0, `this: &Surface` last.
        this.grab_focus();
    }

    // -- notify:: property handlers (surface.blp:235-240) ------------------
    /// PORTS: `surface.zig:2258-2308` `propConfig`. **Reached during `init`**
    /// (`:1832`), so this must not panic. Pushes `resize-overlay-duration` and
    /// `resize-overlay-position` from the config onto the `resize_overlay`.
    // Renamed `on_notify_*` (blueprint name kept via `name = …`) so they don't
    // clash with the `glib::Properties`-generated `notify_<prop>()` emitters.
    pub(super) fn on_notify_config(&self) {
        use rustty_config::value_types::ResizeOverlayPosition;

        let imp = self.imp();
        // `const config = if (priv.config) |c| c.get() else return` (`:2263-2264`).
        let cfg_ref = imp.config.borrow();
        let Some(config) = cfg_ref.as_ref() else {
            return;
        };
        let config = config.get();

        // resize-overlay-duration (`:2266-2276`).
        imp.resize_overlay
            .set_duration(config.resize_overlay_duration.as_milliseconds());

        // resize-overlay-position (`:2278-2307`).
        let (halign, valign): (gtk4::Align, gtk4::Align) = match config.resize_overlay_position {
            ResizeOverlayPosition::Center => (gtk4::Align::Center, gtk4::Align::Center),
            ResizeOverlayPosition::TopLeft => (gtk4::Align::Start, gtk4::Align::Start),
            ResizeOverlayPosition::TopRight => (gtk4::Align::End, gtk4::Align::Start),
            ResizeOverlayPosition::TopCenter => (gtk4::Align::Center, gtk4::Align::Start),
            ResizeOverlayPosition::BottomLeft => (gtk4::Align::Start, gtk4::Align::End),
            ResizeOverlayPosition::BottomRight => (gtk4::Align::End, gtk4::Align::End),
            ResizeOverlayPosition::BottomCenter => (gtk4::Align::Center, gtk4::Align::End),
        };
        imp.resize_overlay.set_overlay_halign(halign);
        imp.resize_overlay.set_overlay_valign(valign);
    }
    /// PORTS: `surface.zig:2310-2348` `propError` + `onIdleRechild`. **Reached
    /// from [`Surface::set_error`]** (the GL-error path), so this must not panic.
    /// Toggles the opaque-background CSS (`:2316-2326`) and schedules an idle-tick
    /// child swap between the `terminal_page` and the `error_page` (`:2328-2348`).
    fn on_notify_error(&self) {
        let widget = self.upcast_ref::<gtk4::Widget>();
        if self.imp().error.get() {
            // Ensure we have an opaque background. The window will NOT set
            // this if we have transparency set and we need an opaque
            // background for the error message to be readable. (`:2316-2322`)
            widget.add_css_class("background");
        } else {
            // Regardless of transparency setting, we remove the background
            // CSS class from this widget. Parent widgets will set it
            // appropriately (see window.zig for example). (`:2323-2326`)
            widget.remove_css_class("background");
        }

        // We need to set our child property on an idle tick, because the
        // error property can be triggered by signals that are in the middle
        // of widget mapping and changing our child during that time
        // results in a hard gtk crash. (`:2328-2335`)
        // A weak backref keeps the source non-owning (removed in `dispose`,
        // **invariant 11**).
        let imp = self.imp();
        if imp.idle_rechild.borrow().is_none() {
            let weak = self.downgrade();
            let id = glib::idle_add_local(move || {
                // PORTS: `onIdleRechild` (`:2338-2348`). Re-reads `error` at idle
                // time (not capture time), matching the reference.
                if let Some(this) = weak.upgrade() {
                    let imp = this.imp();
                    imp.idle_rechild.replace(None);
                    if imp.error.get() {
                        this.set_child(Some(&*imp.error_page));
                    } else {
                        this.set_child(Some(&*imp.terminal_page));
                    }
                }
                glib::ControlFlow::Break
            });
            imp.idle_rechild.replace(Some(id));
        }
    }
    /// PORTS: `surface.zig:2350-2358` `propMouseHoverUrl`. Reveal `url_left` (the
    /// bottom-left URL overlay label, whose text is bound to `mouse-hover-url` in the
    /// blueprint) when the hovered URL is non-empty, hide it otherwise.
    fn on_notify_mouse_hover_url(&self) {
        let visible = self
            .imp()
            .mouse_hover_url
            .borrow()
            .as_deref()
            .is_some_and(|v| !v.is_empty());
        self.imp().url_left.set_visible(visible);
    }
    /// PORTS: `surface.zig:2360-2376` `propMouseHidden`. When the mouse is hidden set
    /// the widget cursor to `"none"`; otherwise re-run the mouse-shape logic to
    /// restore the proper cursor.
    fn on_notify_mouse_hidden(&self) {
        // If we're hidden we set it to "none" (`:2367-2371`).
        if self.imp().mouse_hidden.get() {
            self.upcast_ref::<gtk4::Widget>()
                .set_cursor_from_name(Some("none"));
            return;
        }
        // If we're not hidden we just trigger the mouse shape
        // prop notification to handle setting the proper mouse shape. (`:2373-2375`)
        self.apply_mouse_shape_cursor();
    }
    /// PORTS: `surface.zig:2378-2429` `propMouseShape`. Set the widget cursor
    /// from the `mouse-shape` property (a no-op while the mouse is hidden).
    fn on_notify_mouse_shape(&self) {
        self.apply_mouse_shape_cursor();
    }
    /// PORTS: `surface.zig:2441-2465` `propVAdjustment`. **Reached during
    /// construction** — GtkScrollable's `vadjustment` is a construct property GTK
    /// sets to a default adjustment, firing `notify::vadjustment` — so this must
    /// not panic. Lazily builds the vadj `SignalGroup`, connects `value-changed`
    /// to [`vadjValueChanged`](Self::on_notify_vadjustment) (dragging the
    /// scrollbar performs `.scroll_to_row`), and points the group at the current
    /// adjustment. Idempotent: the group is built once, then re-targeted.
    fn on_notify_vadjustment(&self) {
        let imp = self.imp();

        // When vadjustment is first set, we setup the signal group lazily.
        // This makes it so that if we don't use scrollbars, we never
        // pay the memory cost of this. (`:2448-2461`) The typed `gobject.SignalGroup` binding is
        // behind a glib feature this build does not enable, so the group is built
        // and connected via the libgobject-2.0 C symbols directly (see the FFI
        // block by `g_signal_group_set_target`).
        // Bind the current group out of the RefCell in its OWN statement first:
        // a `borrow()` temporary in an `if let` scrutinee lives for the whole
        // `if/else` (temporary lifetime extension), so borrowing inline would
        // still be alive when the `else` branch calls `replace()` below — a
        // guaranteed "RefCell already borrowed" abort. The `let` drops the `Ref`
        // at the semicolon.
        let existing = imp.vadj_signal_group.borrow().clone();
        let group = if let Some(group) = existing {
            group
        } else {
            // `gobject.SignalGroup.new(gtk.Adjustment.getGObjectType())` (`:2452`).
            // `g_signal_group_new` transfers full ownership of the new group.
            let group: glib::Object = unsafe {
                glib::translate::from_glib_full(g_signal_group_new(
                    glib::translate::IntoGlib::into_glib(gtk4::Adjustment::static_type()),
                ))
            };

            // `group.connect("value-changed", &vadjValueChanged, self)`
            // (`:2453-2457`). The reference passes the raw `self` pointer as the
            // untyped user-data (no ref, sidestepping a cycle); rustty captures a
            // WEAK backref to the widget in the closure for the same reason — the
            // group is owned by the widget (`vadj_signal_group`), so a strong
            // capture would leak.
            let weak = self.downgrade();
            let closure = glib::Closure::new_local(move |values| {
                // PORTS: `vadjValueChanged` (`:2430-2439`).
                // This will trigger for every single pixel change in the adjustment,
                // but our core surface handles the noise from this so that identical
                // rows are cheap.
                // `values[0]` is the emitting `gtk.Adjustment`.
                let this: Surface = weak.upgrade()?;
                // `const core_surface = self.core() orelse return` (`:2434`).
                let core = this.core()?;
                let adj: gtk4::Adjustment = values.first()?.get().ok()?;
                // `const row: usize = @intFromFloat(@round(adj.getValue()))`
                // (`:2435`).
                let row = adj.value().round() as usize;
                // `surface.performBindingAction(.{ .scroll_to_row = row })`
                // (`:2436-2438`).
                if let Err(err) = unsafe {
                    CoreSurface::performBindingAction(
                        core,
                        rustty_input::binding::Action::ScrollToRow(row),
                    )
                } {
                    log::warn!(target: LOG_SCOPE, "error performing scroll_to_row action err={err:?}");
                }
                None
            });
            // SAFETY: `group` is a live `GSignalGroup`; `closure` is a live
            // `GClosure`. `g_signal_group_connect_closure` takes its own ref on
            // the closure, so the local `closure`'s scope-end unref is balanced.
            unsafe {
                g_signal_group_connect_closure(
                    group.as_ptr().cast(),
                    c"value-changed".as_ptr(),
                    closure.as_ptr(),
                    glib::ffi::GFALSE,
                );
            }

            imp.vadj_signal_group.replace(Some(group.clone()));
            group
        };

        // Setup our signal group target (`:2463-2464`).
        let target = imp.vadjustment.borrow().clone();
        // SAFETY: `group` is a live `GSignalGroup`; the target is a live
        // `gtk.Adjustment` (or null). `GSignalGroup*`/`GtkAdjustment*` are
        // ABI-compatible with `GObject*`; `set_target` is always valid.
        unsafe {
            g_signal_group_set_target(
                group.as_ptr().cast(),
                target.map_or(std::ptr::null_mut(), |t| t.as_ptr().cast()),
            );
        }
    }

    // -- bind-time closures — MUST return safe values (evaluated at init) ---
    /// Callback used to determine whether border should be shown around the
    /// surface.
    ///
    /// PORTS: `surface.zig:829-848` `closureShouldBorderBeShown`. `false` unless
    /// the bell is ringing AND the config's `bell-features.border` is set.
    fn should_border_be_shown(&self, config: Option<Config>, bell_ringing: bool) -> bool {
        // If the bell isn't ringing exit early because when the surface is
        // first created there's a race between this code being run and the
        // config being set on the surface. That way we don't overwhelm people
        // with the warning that we issue if the config isn't set and overwhelm
        // ourselves with large numbers of bug reports. (`surface.zig:836-841`)
        if !bell_ringing {
            return false;
        }
        let Some(config) = config else {
            // log.warn("config unavailable for computing whether border should be shown, likely bug", .{});
            log::warn!(
                target: LOG_SCOPE,
                "config unavailable for computing whether border should be shown, likely bug"
            );
            return false;
        };
        // `config.@"bell-features".border` (`surface.zig:848`).
        config.get().bell_features.border
    }

    /// Callback used to determine whether unfocused-split-fill / unfocused-split-opacity
    /// should be applied to the surface
    ///
    /// PORTS: `surface.zig:853-860` `closureShouldUnfocusedSplitBeShown`.
    fn should_unfocused_split_be_shown(
        &self,
        search_active: bool,
        focused: bool,
        is_split: bool,
    ) -> bool {
        !search_active && !focused && is_split
    }

    // -- SearchOverlay (surface.blp:192-195) -------------------------------
    // PORT NOTE (arg order): these four signals are connected NON-swapped in the
    // blueprint, so GTK calls them with the emitting `SearchOverlay` FIRST and
    // the template instance (`this: &Surface`) LAST, the reference callconv(.c)
    // signatures (`surface.zig:3552-3577`, `fn searchStop(_: *SearchOverlay,
    // self: *Self)` etc.).

    /// PORTS: `surface.zig:3552-3558` `searchStop`. Perform the `end_search`
    /// binding action on the core surface, then return focus to the RenderSurface.
    fn search_stop(_o: &SearchOverlay, this: &Surface) {
        let Some(core) = this.core() else {
            return;
        };
        // `surface.performBindingAction(.end_search)` (`:3554`).
        if let Err(err) = unsafe {
            CoreSurface::performBindingAction(core, rustty_input::binding::Action::EndSearch)
        } {
            log::warn!(target: LOG_SCOPE, "unable to perform end_search action err={err:?}");
        }
        // `self.private().render_surface.as(gtk.Widget).grabFocus()` (`:3597`).
        this.grab_focus();
    }

    /// PORTS: `surface.zig:3560-3565` `searchChanged`. Drive the `search` binding
    /// action with the (nullable) needle. `needle` is the debounced search-entry
    /// text; a null needle is treated as the empty string (`std.mem.sliceTo(needle
    /// orelse "", 0)`), which the core search arm interprets as "stop search".
    fn search_changed(_o: &SearchOverlay, needle: Option<&str>, this: &Surface) {
        let Some(core) = this.core() else {
            return;
        };
        // `.{ .search = std.mem.sliceTo(needle orelse "", 0) }` (`:3562`).
        let needle_bytes = needle.unwrap_or("").as_bytes().to_vec();
        if let Err(err) = unsafe {
            CoreSurface::performBindingAction(
                core,
                rustty_input::binding::Action::Search(needle_bytes),
            )
        } {
            log::warn!(target: LOG_SCOPE, "unable to perform search action err={err:?}");
        }
    }

    /// PORTS: `surface.zig:3567-3572` `searchNextMatch`. Drive
    /// `navigate_search = .next` on the core surface.
    fn search_next_match(_o: &SearchOverlay, this: &Surface) {
        let Some(core) = this.core() else {
            return;
        };
        // `.{ .navigate_search = .next }` (`:3569`).
        if let Err(err) = unsafe {
            CoreSurface::performBindingAction(
                core,
                rustty_input::binding::Action::NavigateSearch(
                    rustty_input::binding::NavigateSearch::Next,
                ),
            )
        } {
            log::warn!(target: LOG_SCOPE, "unable to perform navigate_search action err={err:?}");
        }
    }

    /// PORTS: `surface.zig:3574-3579` `searchPreviousMatch`. Drive
    /// `navigate_search = .previous` on the core surface.
    fn search_previous_match(_o: &SearchOverlay, this: &Surface) {
        let Some(core) = this.core() else {
            return;
        };
        // `.{ .navigate_search = .previous }` (`:3576`).
        if let Err(err) = unsafe {
            CoreSurface::performBindingAction(
                core,
                rustty_input::binding::Action::NavigateSearch(
                    rustty_input::binding::NavigateSearch::Previous,
                ),
            )
        } {
            log::warn!(target: LOG_SCOPE, "unable to perform navigate_search action err={err:?}");
        }
    }

    // -- Split drag-and-drop (B6, 1d053bd6e; surface.blp `drag_handle` /
    // -- `drop_overlay` / `surface_drop_target`) ---------------------------

    /// PORTS: `surface.zig:3671-3684` `closureShouldDragHandleBeShown` (bind-time
    /// closure; `&self` first like the other closures).
    fn should_drag_handle_be_shown(&self, config: Option<Config>, is_split: bool) -> bool {
        let Some(config) = config else {
            return false;
        };

        // `switch (config.get().@"drag-handle")` (74f91d1b4's `drag-handle`
        // config option).
        match config.get().drag_handle {
            rustty_config::value_types::DragHandle::Always => true,
            rustty_config::value_types::DragHandle::Auto => is_split,
            rustty_config::value_types::DragHandle::Never => false,
        }
    }

    /// PORTS: `surface.zig:3686-3699` `surfaceDragPrepare` — the DragSource
    /// `prepare` closure: a uint64 content provider carrying this surface's
    /// core id.
    fn surface_drag_prepare(
        _src: &gtk4::DragSource,
        _x: f64,
        _y: f64,
        this: &Surface,
    ) -> gdk4::ContentProvider {
        // TODO: Use a static content provider once we make `self.core()`
        // available immediately when constructing the surface widget
        //
        // `self.core().?.id` — the reference `.?` unwrap (panic -> abort in a C
        // callback per DG5).
        let core = this.core().expect("surface core is initialized");
        // SAFETY: a non-null `core()` result is a live, published core surface.
        let id = unsafe { (*core).id };
        gdk4::ContentProvider::for_value(&id.to_value())
    }

    /// PORTS: `surface.zig:3701-3745` `surfaceDragBegin` — set a 0.2×-scaled
    /// `WidgetPaintable` snapshot of the surface as the drag icon.
    fn surface_drag_begin(src: &gtk4::DragSource, _drag: &gdk4::Drag, this: &Surface) {
        // The scale of the preview
        const PREVIEW_SCALE: f32 = 0.2;

        // Snapshot the entire surface widget as the icon preview
        let paintable = gtk4::WidgetPaintable::new(Some(this));

        // Center the preview
        let width = this.width();
        let height = this.height();
        let mid_x = width as f32 / 2.0;
        let mid_y = height as f32 / 2.0;

        // Create a snapshot to render the scaled paintable
        let snap = gtk4::Snapshot::new();
        snap.scale(PREVIEW_SCALE, PREVIEW_SCALE);
        paintable.snapshot(&snap, f64::from(width), f64::from(height));

        // `snap.freeToPaintable(null)` — `to_paintable` consumes the snapshot
        // (the "free" half) and returns the rendered paintable.
        if let Some(scaled) = snap.to_paintable(None) {
            src.set_icon(
                Some(&scaled),
                (mid_x * PREVIEW_SCALE) as i32,
                (mid_y * PREVIEW_SCALE) as i32,
            );
        } else {
            // The scaling process somehow failed.
            // Use the original paintable as a fail-safe
            log::warn!(target: LOG_SCOPE, "preview scaling failed - falling back to original paintable");
            src.set_icon(Some(&paintable), mid_x as i32, mid_y as i32);
        }
    }

    /// PORTS: `surface.zig:3747-3774` `surfaceDrop` — resolve the dropped
    /// surface id, then `SplitTree::move_split` it onto this surface in the
    /// pointer-derived direction.
    ///
    /// PORT NOTE: reference defect (R13), `UPSTREAM-DEFECTS.md` UD295. The
    /// reference's handler is `callconv(.c) void`, but `GtkDropTarget::drop` is a
    /// `gboolean` signal ("whether the drop was accepted"), so GTK reads an
    /// undefined return value on every surface drag-and-drop (a C
    /// function-type mismatch). In the port a `()` handler on a `gboolean` signal
    /// makes glib-rs panic inside the C callback, which ABORTS the process:
    /// MEASURED on the VM (task L-2) by emitting `drop` on this target ("Closure
    /// returned no value but the caller expected a value of type gboolean",
    /// then "panic in a function that cannot unwind"). The port returns `true`
    /// when the split was moved and `false` on every early return, the signal's
    /// documented meaning.
    fn surface_drop(
        _tgt: &gtk4::DropTarget,
        v: &glib::Value,
        x: f64,
        y: f64,
        this: &Surface,
    ) -> bool {
        // `v: *const gobject.Value`: the dropped value itself, as in `drop`.
        // `v.getUint64()`.
        let Ok(dropped_id) = v.get::<u64>() else {
            return false;
        };
        // `self.core().?.app.findSurfaceByID(dropped_id) orelse return`.
        let core = this.core().expect("surface core is initialized");
        // SAFETY: a non-null `core()` result is a live core surface; its `app`
        // backref is the live core App (app-thread dispatch).
        let Some(dropped) = (unsafe { (*(*core).app).findSurfaceByID(dropped_id) }) else {
            return false;
        };
        // `dropped.rt_surface.gobj()`.
        // SAFETY: `findSurfaceByID` returns a validated live surface whose
        // `rt_surface` points at the pinned apprt surface in the widget private.
        let Some(from) = (unsafe { (*(*dropped).rt_surface).widget() }) else {
            return false;
        };

        let Some(st) = ext::ancestor::<SplitTree>(this) else {
            log::warn!(target: LOG_SCOPE, "surface is not placed in a split tree");
            return false;
        };

        let dir = this.calc_drop_direction(x, y);

        // The only error that could happen here is an OOM,
        // and in that case we're already milliseconds away from crashing, so...
        if st.move_split(&from, this, dir).is_err() {
            return false;
        }

        // Clean up overlay state
        this.set_drop_overlay_direction(None);
        true
    }

    /// PORTS: `surface.zig:3776-3781` `surfaceDropLeave`.
    fn surface_drop_leave(_tgt: &gtk4::DropTarget, this: &Surface) {
        // Hide overlay
        this.set_drop_overlay_direction(None);
    }

    /// PORTS: `surface.zig:3783-3793` `surfaceDropMotion` — recalculate the
    /// drop region and light the overlay gradient.
    fn surface_drop_motion(
        _tgt: &gtk4::DropTarget,
        x: f64,
        y: f64,
        this: &Surface,
    ) -> gdk4::DragAction {
        // Recalculate the drop region
        let dir = this.calc_drop_direction(x, y);
        this.set_drop_overlay_direction(Some(dir));
        gdk4::DragAction::MOVE
    }

    /// PORTS: `surface.zig:3795-3809` `propDropValue` (`notify::value` on the
    /// preloading `surface_drop_target`).
    ///
    /// Reject the drop if we're dropping a surface onto itself.
    /// Note that we cannot implement this via the `accept` signal,
    /// since the decision of whether to accept or deny a drop is dependent
    /// on the payload (i.e. the surface being dropped). This is
    /// well-documented in GTK docs.
    fn notify_drop_value(tgt: &gtk4::DropTarget, _pspec: &glib::ParamSpec, this: &Surface) {
        let Some(core_surface) = this.core() else {
            return;
        };
        let Some(value) = tgt.value() else {
            return;
        };
        let Ok(surface_id) = value.get::<u64>() else {
            return;
        };
        // SAFETY: a non-null `core()` result is a live core surface.
        if unsafe { (*core_surface).id } == surface_id {
            tgt.reject();
        }
    }
}
