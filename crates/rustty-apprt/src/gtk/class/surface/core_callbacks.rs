//! PORTS: ghostty src/apprt/gtk/class/surface.zig — the "Libghostty
//! Callbacks" region (`:1497-...`): the `Surface` methods the apprt seam
//! (`gtk::surface::Surface`) and the core surface call, from `close` through
//! `cgroup`.

use super::*;

// ===========================================================================
// Libghostty callbacks + RenderSurface lifecycle (L5.2 T15/T17)
//
// The apprt seam (`gtk::surface::Surface`) forwards to these; the
// RenderSurface signal handlers (the `render_surface_*` template callbacks)
// drive them. Ported from `surface.zig` — the "Libghostty Callbacks" region
// (`:1497-...`) and the RenderSurface lifecycle (`:3305-3528`).
// ===========================================================================

impl Surface {
    // PORTS: `surface.zig:2225-2229` `setError`. The `glib::Properties`-generated
    // `set_error(bool)` setter for the `error` property already does the reference's
    // `priv.@"error" = v; notifyByPspec(...)` (set the cell + notify on change), so
    // callers use it directly (e.g. `Surface::render_surface_resize`'s
    // init-failure branch, UD356).

    /// PORTS: `surface.zig:2378-2429` `propMouseShape`. Set the widget cursor from the
    /// current `mouse-shape` property. Shared by `on_notify_mouse_shape` and
    /// `on_notify_mouse_hidden` (which re-runs the shape logic on un-hide).
    pub(super) fn apply_mouse_shape_cursor(&self) {
        // If our mouse should be hidden currently then we don't do anything
        // (`:2385-2387`).
        if self.imp().mouse_hidden.get() {
            return;
        }
        // Map the typed shape to a CSS cursor name and set it (`:2482-2520`).
        let name = mouse_shape_cursor_name(MouseShape::from(self.imp().mouse_shape.get()));
        self.upcast_ref::<gtk4::Widget>()
            .set_cursor_from_name(Some(name));
    }

    /// PORTS: `surface.zig:1500-1507` `close`. Emits `close-request`; the window
    /// tier decides what to do. The apprt seam's `close(process_alive)` discards
    /// `process_alive` and calls this (`Surface.zig:35-38`).
    pub fn close(&self) {
        self.emit_by_name::<()>("close-request", &[]);
    }

    /// Focus this surface. This properly focuses the input part of
    /// our surface.
    ///
    /// PORTS: `surface.zig:1742-1745` `grabFocus`. Focus the input part of the
    /// surface — the inner `RenderSurface`, NOT the outer `adw::Bin`. The owner
    /// tiers (`window.rs`, `split_tree.rs`, `application.rs`) call
    /// `surface.grab_focus()`; this inherent method shadows `WidgetExt::grab_focus`
    /// so those calls target the RenderSurface explicitly, exactly as the
    /// reference does (rather than relying on `adw::Bin`'s implicit focus-descent).
    pub fn grab_focus(&self) {
        let _ = self
            .imp()
            .render_surface
            .upcast_ref::<gtk4::Widget>()
            .grab_focus();
    }

    /// PORTS: `surface.zig:2231-2248` `setSearchActive`. Toggle the
    /// [`SearchOverlay`]'s `active` property (which drives its `visible` binding);
    /// on activation, seed a non-empty `needle` into the entry and grab focus. The
    /// `active` property setter re-emits `search-changed`, and
    /// [`SearchOverlay::set_search_contents`] re-emits it with `needle`, so the
    /// core search is driven through the surface's `search_changed` signal handler.
    pub fn set_search_active(&self, active: bool, needle: &[u8]) {
        let overlay = &*self.imp().search_overlay;
        // `gobject.Object.setProperty(search_overlay, "active", active)` (`:2233-2238`).
        overlay.set_active(active);

        // `if (!std.mem.eql(u8, needle, "")) search_overlay.setSearchContents(needle)`
        // (`:2240-2242`).
        if !needle.is_empty() {
            overlay.set_search_contents(&String::from_utf8_lossy(needle));
        }

        // `if (active) search_overlay.grabFocus()` (`:2244-2246`).
        if active {
            overlay.grab_focus();
        }
    }

    /// PORTS: `surface.zig:2250-2252` `setSearchTotal`. Forward the total match
    /// count to the search overlay's counter.
    pub fn set_search_total(&self, total: Option<usize>) {
        self.imp().search_overlay.set_search_total(total);
    }

    /// PORTS: `surface.zig:2254-2256` `setSearchSelected`. Forward the selected
    /// match index to the search overlay's counter.
    pub fn set_search_selected(&self, selected: Option<usize>) {
        self.imp().search_overlay.set_search_selected(selected);
    }

    /// Handle a key sequence action from the apprt.
    ///
    /// PORTS: `surface.zig:916-950` `keySequenceAction`. Handle a key-sequence
    /// action from the apprt, updating the owned `key_sequence` (which backs the
    /// `key-sequence` property the key-state overlay binds, revealing pending
    /// leader-key feedback).
    pub fn key_sequence_action(&self, value: rustty_apprt_api::action::KeySequence) {
        use rustty_apprt_api::action::KeySequence;
        let imp = self.imp();

        // `freezeNotify` / `defer thawNotify` (`:923-924`): the guard thaws on
        // drop, coalescing the notify below.
        let _guard = self.freeze_notify();
        // Notify unconditionally (`:925`) — queued before the (possibly
        // early-returning) switch, exactly like the reference.
        self.notify("key-sequence");

        match value {
            KeySequence::Trigger(trigger) => {
                // Convert the trigger to a human-readable label (`:930-934`).
                let mut label = String::new();
                if !key::label_from_trigger(&mut label, trigger) {
                    // `if (!success) return` (`:933`).
                    return;
                }
                // Copy and append (`:937-942`). A label can't contain a NUL, so
                // `CString::new` only fails on a malformed label; drop it if so.
                if let Ok(duped) = CString::new(label) {
                    imp.key_sequence.borrow_mut().push(duped);
                }
            }
            KeySequence::End => {
                // Free all the stored strings and clear
                // Free all stored strings and clear (`:944-948`).
                imp.key_sequence.borrow_mut().clear();
            }
        }
    }

    /// Handle a key table action from the apprt.
    ///
    /// PORTS: `surface.zig:953-981` `keyTableAction`. Handle a key-table action,
    /// maintaining the owned `key_tables` stack (which backs the `key-table`
    /// property the key-state overlay binds).
    pub fn key_table_action(&self, value: rustty_apprt_api::action::KeyTable<'_>) {
        use rustty_apprt_api::action::KeyTable;
        let imp = self.imp();

        let _guard = self.freeze_notify();
        self.notify("key-table");

        match value {
            KeyTable::Activate(name) => {
                // Duplicate the name string and push onto stack
                // Duplicate the name and push onto the stack (`:965-969`).
                if let Ok(duped) = CString::new(name) {
                    imp.key_tables.borrow_mut().push(duped);
                }
            }
            KeyTable::Deactivate => {
                // Pop and free the top table (`:971-973`).
                imp.key_tables.borrow_mut().pop();
            }
            KeyTable::DeactivateAll => {
                // Free all tables and clear (`:975-978`).
                imp.key_tables.borrow_mut().clear();
            }
        }
    }

    /// Set the current progress report state.
    ///
    /// PORTS: `surface.zig:1030-1113` `setProgressReport`. Drive the
    /// `progress_bar_overlay` (a `gtk.ProgressBar` in `surface.blp`) visible +
    /// fraction/pulse from an OSC 9;4 progress report, with a 15s watchdog timer
    /// that removes bars stalled by bad-actor programs.
    pub fn set_progress_report(&self, value: rustty_vt::osc::ProgressReport) {
        use rustty_vt::osc::ProgressReportState;
        let imp = self.imp();

        // No matter what, we stop the timer because if we're removing
        // then we're done and otherwise we restart it.
        // Stop the timer no matter what — if removing we're done, otherwise we
        // restart it below (`:1036-1043`).
        remove_source_id(imp.progress_bar_timer.take(), "progress bar timer");

        // Config gate: `progress-style` off => hide + return (`:1045-1051`).
        if let Some(config) = imp.config.borrow().as_ref() {
            if !config.get().progress_style {
                // log.debug("progress_report action blocked by config", .{});
                //
                // S54-F4: this was dropped entirely. It is the only signal a
                // user gets that a `progress_report` OSC was ignored on purpose.
                log::debug!(target: LOG_SCOPE, "progress_report action blocked by config");
                imp.progress_bar_overlay.set_visible(false);
                return;
            }
        }

        let progress_bar = &*imp.progress_bar_overlay;
        match value.state {
            // Remove the progress bar (`:1055-1059`).
            ProgressReportState::Remove => {
                progress_bar.set_visible(false);
                return;
            }
            // Set the progress bar to a fixed value if one was provided, otherwise pulse.
            // Remove the `error` CSS class so that the progress bar shows as normal.
            // Fixed value if provided, otherwise pulse; clear the `error` class
            // (`:1063-1070`).
            ProgressReportState::Set => {
                progress_bar.remove_css_class("error");
                match value.progress {
                    Some(progress) => progress_bar.set_fraction(compute_fraction(progress)),
                    None => progress_bar.pulse(),
                }
            }
            // Set the progress bar to a fixed value if one was provided, otherwise pulse.
            // Set the `error` CSS class so that the progress bar shows as an error color.
            // As `.set`, but with the `error` class (`:1074-1081`).
            ProgressReportState::Error => {
                progress_bar.add_css_class("error");
                match value.progress {
                    Some(progress) => progress_bar.set_fraction(compute_fraction(progress)),
                    None => progress_bar.pulse(),
                }
            }
            // The state of progress is unknown, so pulse the progress bar to
            // indicate that things are still happening.
            // Unknown progress — pulse (`:1085-1087`).
            ProgressReportState::Indeterminate => progress_bar.pulse(),
            // If a progress value was provided, set the progress bar to that value.
            // Don't pulse the progress bar as that would indicate that things were
            // happening. Otherwise this is mainly used to keep the progress bar on
            // screen instead of timing out.
            // Keep the bar on screen; set a fixed value if provided, but don't
            // pulse (`:1093-1097`).
            ProgressReportState::Pause => {
                if let Some(progress) = value.progress {
                    progress_bar.set_fraction(compute_fraction(progress));
                }
            }
        }

        // Assume all states lead to visibility
        // All non-remove states lead to visibility (`:1100-1102`).
        progress_bar.set_visible(true);

        // Start our timer to remove bad actor programs that stall
        // the progress bar.
        // Start the watchdog to remove bad-actor programs that stall the bar
        // (`:1104-1112`). 15 seconds, mirroring `progress_bar_timeout_seconds`.
        let id = glib::timeout_add_local(
            Duration::from_secs(15),
            glib::clone!(
                #[weak(rename_to = this)]
                self,
                #[upgrade_or]
                glib::ControlFlow::Break,
                move || this.progress_bar_timer()
            ),
        );
        imp.progress_bar_timer.replace(Some(id));
    }

    /// The progress bar hasn't been updated by the TUI recently, remove it.
    ///
    /// PORTS: `surface.zig:1116-1122` `progressBarTimer`. The bar hasn't been
    /// updated by the TUI recently; remove it. Nulls the field first (before the
    /// re-entrant `setProgressReport(.remove)`) so its timer-removal branch is a
    /// no-op on the source currently firing (`:1119`).
    fn progress_bar_timer(&self) -> glib::ControlFlow {
        self.imp().progress_bar_timer.replace(None);
        self.set_progress_report(rustty_vt::osc::ProgressReport {
            state: rustty_vt::osc::ProgressReportState::Remove,
            progress: None,
        });
        glib::ControlFlow::Break
    }

    /// PORTS: `surface.zig:1509-1530` `childExited`. Set the `child-exited`
    /// property (which reveals `child_exited_overlay` via the `visible: bind
    /// template.child-exited` in `surface.blp`) and populate the banner from the
    /// exit payload. Always returns `true`: the `SurfaceChildExitedNoop` path that
    /// returns `false` (forcing the text-based message) does not exist in this
    /// Adwaita-1.3+ port (`:1521-1526`).
    pub fn child_exited_show(&self, value: rustty_apprt_api::surface::ChildExited) -> bool {
        // Even if we don't support the overlay, we still keep our property
        // up to date for anyone listening.
        // Keep the property up to date for anyone listening (`:1513-1519`).
        self.set_child_exited(true);

        // If we have the noop child exited overlay then we don't do anything
        // for child exited. The false return will force librustty to show
        // the normal text-based message.
        // PORT NOTE: this port has no noop child-exited overlay (`surface.zig:1541-1543`
        // `@hasDecl(ChildExited, "noop")`), so that early `return false` has no arm here.
        // Populate the banner (`:1528`).
        self.imp()
            .child_exited_overlay
            .set_child_exited_data(Some(value));
        true
    }

    /// PORTS: `surface.zig:2296-2316` `setBellRinging`. Named `_inner` because the
    /// `bell-ringing` property's generated setter owns `set_bell_ringing`; that
    /// generated setter is the reference's `privateShallowFieldAccessor` (a plain
    /// write), and THIS is the reference method every bell path calls.
    ///
    /// CORRECTION (task 20, S55-F2 + S54-F2): there was no such method. Callers
    /// used the generated setter, which writes and notifies unconditionally, and
    /// `ring_bell` carried the `setBellRinging(true)` half without the freeze. So
    /// `notify::bell-ringing` fired on every focus-in, mouse press and key press
    /// while the value did not change, and the notifications raised by `bell`
    /// handlers and the ring-bell actions were not coalesced around the write. The
    /// property's own contract (`surface.zig:62-68`) promises a notify only on a
    /// full state change.
    pub fn set_bell_ringing_inner(&self, ringing: bool) {
        // Prevent duplicate change notifications if the signals we emit
        // in this function cause this state to change again.
        let _thaw = self.freeze_notify();

        // Logic around bell reaction happens on every event even if we're
        // already in the ringing state.
        if ringing {
            self.ring_bell();
            // focus clears ringing state, so we should not change state to
            // ringing if we're already focused
            if self.focused() {
                return;
            }
        }

        // Property change only happens on actual state change
        let imp = self.imp();
        if imp.bell_ringing.get() == ringing {
            return;
        }
        imp.bell_ringing.set(ringing);
        self.notify_bell_ringing();
    }

    /// Handle bell features that need to happen every time a BEL is received
    /// Currently this is audio and system but this could change in the future.
    ///
    /// PORTS: `surface.zig:2561-2574` `ringBell` (B7 end state, f0f3f4d8d): emit
    /// the `bell` signal, then fan out to the `tab.`/`win.`/`app.ring-bell`
    /// widget actions (system beep, title bell, urgency — `window.rs`/`tab.rs` —
    /// and the application-owned audio bell, `application.rs` `action_ring_bell`).
    ///
    /// CORRECTION (task 20): this used to also host the focused guard and the
    /// `bell-ringing` write, with a PORT NOTE saying "rustty keeps
    /// `set_bell_ringing` as the plain property write and hosts the full bell
    /// here". Those belong to `setBellRinging` ([`Self::set_bell_ringing_inner`]),
    /// which calls this; `ringBell` is private in the reference.
    fn ring_bell(&self) {
        // Emit the signal
        // `signals.bell.impl.emit(...)` — fire any bell subscriber BEFORE the
        // action fan-out.
        self.emit_by_name::<()>("bell", &[]);

        // Activate actions if they exist
        // `tab.ring-bell` / `win.ring-bell` do the system beep, title bell, and
        // attention/urgency (`window.rs`, `tab.rs`); `app.ring-bell` plays the
        // application-owned audio bell (B7, `application.rs`). They bubble up from
        // the surface widget.
        let _ = self.activate_action("tab.ring-bell", None);
        let _ = self.activate_action("win.ring-bell", None);
        let _ = self.activate_action("app.ring-bell", None);
    }

    /// Request that this terminal come to the front and become focused.
    /// It is up to the embedding widget to react to this.
    ///
    /// PORTS: `surface.zig:1126-1133` `present`. Request that this terminal come to
    /// the front and become focused. It is up to the embedding widget (the owning
    /// [`Window`](super::Window)) to react to this via the `present-request` signal
    /// (`window.rs` `surface_present_request`).
    pub fn present(&self) {
        self.emit_by_name::<()>("present-request", &[]);
    }

    /// PORTS: `surface.zig:862-869` `toggleFullscreen`. Emits `toggle-fullscreen`;
    /// the owning window reacts (`window.rs` `surface_toggle_fullscreen`).
    pub fn toggle_fullscreen(&self) {
        self.emit_by_name::<()>("toggle-fullscreen", &[]);
    }

    /// PORTS: `surface.zig:871-878` `toggleMaximize`. Emits `toggle-maximize`;
    /// the owning window reacts (`window.rs` `surface_toggle_maximize`).
    pub fn toggle_maximize(&self) {
        self.emit_by_name::<()>("toggle-maximize", &[]);
    }

    /// Estimate and set the initial window size from config and font metrics.
    /// This can be called before the core surface exists to set up the window
    /// size before presenting. This is an estimate because it does not take
    /// into account any padding that may need to be added to the window.
    ///
    /// PORTS: `surface.zig:2095-2138` `estimateInitialSize`. Estimate the window's
    /// initial pixel size from `window-width`/`window-height` × the font-grid cell
    /// size, so the window manager can position it before the surface initializes;
    /// stores it in the `default-size` property (read back by
    /// `Application::init_and_show_window`).
    pub fn estimate_initial_size(&self) {
        // `const config = priv.config orelse return;` (`:2097-2099`).
        let config_ref = self.imp().config.borrow();
        let Some(config_obj) = config_ref.as_ref() else {
            return;
        };
        let config = config_obj.get();

        // Both dimensions must be configured (`:2101-2102`).
        if config.window_height == 0 || config.window_width == 0 {
            return;
        }

        // Get content scale and compute DPI
        // Content scale -> DPI (`:2104-2113`). `default_dpi` = 72 on macOS else 96
        // (`rustty-font-shaper/face/mod.rs` `default_dpi`).
        let content_scale = self.get_content_scale();
        let default_dpi: f32 = if cfg!(target_os = "macos") {
            72.0
        } else {
            96.0
        };
        // `.xdpi = @intFromFloat(@round(x_dpi))` (`2f0b65346`).
        let font_size = DesiredSize {
            points: config.font_size,
            xdpi: (content_scale.x * default_dpi).round() as u16,
            ydpi: (content_scale.y * default_dpi).round() as u16,
        };

        // Get font grid for cell metrics
        // Get the font grid for cell metrics (`:2115-2126`). Reach the core app's
        // `font_grid_set` the way `delete_surface` reaches the core app (above).
        let app = Application::default();
        let Some(core_app) = app.imp().core_app.get() else {
            return;
        };
        let core_app = core_app.as_ptr().cast::<rustty_core::app::App<App>>();
        let derived = rustty_core::surface::font_shared_grid_set_derived_config(config);
        // SAFETY: a bound core app owns a live `App<App>`; `font_grid_set` is
        // main-thread-only and this runs on the main thread. `ref` is balanced by
        // the `deref` below (the reference `defer font_grid_set.deref(key)`).
        let (font_grid_key, font_grid) =
            match unsafe { (*core_app).font_grid_set.r#ref(&derived, font_size) } {
                Ok(pair) => pair,
                Err(_) => return,
            };
        let cell = unsafe { (*font_grid).cell_size() };
        unsafe { (*core_app).font_grid_set.deref(font_grid_key) };

        // Cells -> pixels -> logical size (`:2128-2137`). Min 10 cols × 4 rows
        // (`CoreSurface.min_window_{width,height}_cells`).
        let width = config.window_width.max(10) * cell.width;
        let height = config.window_height.max(4) * cell.height;
        let final_width = (width as f32 / content_scale.x).ceil() as u32;
        let final_height = (height as f32 / content_scale.y).ceil() as u32;

        // `self.setDefaultSize(...)` (`:2137`).
        self.set_default_size(Some(Size {
            width: final_width,
            height: final_height,
        }));
    }

    /// PORTS: `surface.zig:1539-1583` `getContentScale`. The (fractional)
    /// GdkSurface scale multiplied by the `gtk-xft-dpi` font-DPI scale.
    #[must_use]
    pub fn get_content_scale(&self) -> ContentScale {
        let imp = self.imp();
        let widget = imp.render_surface.upcast_ref::<gtk4::Widget>();

        // The GdkSurface scale (`:1543-1550`, fractional since `45013faf8`).
        let surface_scale = scale::widget_surface_scale(widget);
        let gtk_scale: f32 = if surface_scale <= 0.0 {
            // log.warn("widget surface scale was non-positive: {d}", .{surface_scale});
            log::warn!(target: LOG_SCOPE, "widget surface scale was non-positive: {surface_scale}");
            1.0
        } else {
            surface_scale as f32
        };

        // Also scale using font-specific DPI, which is often exposed to the user
        // via DE accessibility settings (see https://docs.gtk.org/gtk4/class.Settings.html).
        //
        // gtk-xft-dpi is font DPI multiplied by 1024. See
        // https://docs.gtk.org/gtk4/property.Settings.gtk-xft-dpi.html
        //
        // The gtk-xft-dpi font-DPI scale (`:1548-1572`). gtk-xft-dpi is the font
        // DPI × 1024; divide by 1024 then by the 96 default to derive a scale.
        let xft_dpi_scale: f32 = match gsettings::get_c_int(gsettings::Key::GtkXftDpi) {
            None => {
                log::warn!(target: LOG_SCOPE, "gtk-xft-dpi was not set, using default value");
                1.0
            }
            // Use a value of 1.0 for the XFT DPI scale if the setting is <= 0
            // See:
            // https://gitlab.gnome.org/GNOME/libadwaita/-/commit/a7738a4d269bfdf4d8d5429ca73ccdd9b2450421
            // https://gitlab.gnome.org/GNOME/libadwaita/-/commit/9759d3fd81129608dd78116001928f2aed974ead
            Some(dpi) if dpi <= 0 => {
                // -1 is a valid value which specifies default scale.
                // https://docs.gtk.org/gtk4/property.Settings.gtk-xft-dpi.html
                if dpi != -1 {
                    log::warn!(target: LOG_SCOPE, "gtk-xft-dpi has invalid value ({dpi}), using default");
                }
                1.0
            }
            // As noted above gtk-xft-dpi is multiplied by 1024, so we divide by
            // 1024, then divide by the default value (96) to derive a scale. Note
            // gtk-xft-dpi can be fractional, so we use floating point math here.
            Some(dpi) => (dpi as f32 / 1024.0) / 96.0,
        };

        let scale = gtk_scale * xft_dpi_scale;
        ContentScale { x: scale, y: scale }
    }

    /// PORTS: `surface.zig:1578-1586` `getSize`. Returns the CACHED size, not a
    /// live `gtk_widget_get_width` — this is VERY important because it avoids the
    /// pty having an incorrect initial size. We cache it in
    /// [`Surface::render_surface_resize`]
    /// and only build the core surface once we are in a widget tree with proper
    /// dimensions (**invariant 3**).
    #[must_use]
    pub fn get_size(&self) -> SurfaceSize {
        // By the time this is called, we should be in a widget tree.
        // This should not be called before that. We ensure this by initializing
        // the surface in `renderSurfaceResize`. This is VERY important because
        // it avoids the pty having an incorrect initial size.
        self.imp().size.get()
    }

    /// PORTS: `surface.zig:1588-1590` `getCursorPos`.
    #[must_use]
    pub fn get_cursor_pos(&self) -> CursorPos {
        self.imp().cursor_pos.get()
    }

    // `surface.zig:2104-2152`, the remaining property accessors. PORT NOTE: each is
    // the `glib::Properties`-generated getter/setter on the field of the same name
    // (`imp::Surface`), which sets the field and notifies as the reference does:
    //
    // /// Set the title for this surface, copies the value. This should always
    // /// be the title as set by the terminal program, not any manually set
    // /// title. For manually set titles see `setTitleOverride`.
    // pub fn setTitle(self: *Self, title: ?[:0]const u8) void {
    //
    // /// Overridden title. This will be generally be shown over the title
    // /// unless this is unset (null).
    // pub fn setTitleOverride(self: *Self, title: ?[:0]const u8) void {
    //
    // /// Returns the pwd property without a copy.
    // pub fn getPwd(self: *Self) ?[:0]const u8 {
    //
    // /// Set the pwd for this surface, copies the value.
    // pub fn setPwd(self: *Self, pwd: ?[:0]const u8) void {
    //
    // /// Returns the focus state of this surface.
    // pub fn getFocused(self: *Self) bool {
    //
    // /// Returns true if the RenderSurface of this surface is mapped.
    // pub fn getMapped(self: *Self) bool {
    //
    // /// Change the configuration for this surface.
    // pub fn setConfig(self: *Self, config: *Config) void {
    //
    // /// Return the default size, if set.
    // pub fn getDefaultSize(self: *Self) ?*Size {
    //
    // /// Set the default size for a window that contains this surface.
    // /// This is up to the embedding widget to respect this. Generally, only
    // /// the first surface in a window respects this.
    // pub fn setDefaultSize(self: *Self, size: Size) void {
    //
    // /// Return the min size, if set.
    // pub fn getMinSize(self: *Self) ?*Size {
    //
    // /// Set the min size for a window that contains this surface.
    // /// This is up to the embedding widget to respect this. Generally, only
    // /// the first surface in a window respects this.
    // pub fn setMinSize(self: *Self, size: Size) void {
    /// Returns the title property without a copy.
    ///
    /// PORTS: `surface.zig:1994-1996` `getTitle`. The stored surface title (the
    /// terminal-set title, before the user override).
    #[must_use]
    pub fn get_title(&self) -> Option<String> {
        self.imp().title.borrow().clone()
    }

    /// Returns the effective title: the user-overridden title if set,
    /// otherwise the terminal-set title.
    ///
    /// PORTS: `surface.zig:1998-2004` `getEffectiveTitle`. The user override wins
    /// over the terminal-set title.
    #[must_use]
    pub fn get_effective_title(&self) -> Option<String> {
        let imp = self.imp();
        // `return priv.title_override orelse priv.title` (`:2003`).
        imp.title_override
            .borrow()
            .clone()
            .or_else(|| imp.title.borrow().clone())
    }

    /// Copies the effective title to the clipboard.
    ///
    /// PORTS: `surface.zig:2006-2019` `copyTitleToClipboard`. Copies the effective
    /// title to the standard clipboard.
    pub fn copy_title_to_clipboard(&self) -> bool {
        // `const title = self.getEffectiveTitle() orelse return false` (`:2007`).
        let Some(title) = self.get_effective_title() else {
            return false;
        };
        // `if (title.len == 0) return false` (`:2008`).
        if title.is_empty() {
            return false;
        }
        // `self.setClipboard(.standard, &.{.{ .mime = "text/plain", .data = title }},
        // false)` (`:2009-2013`). Reuses the Phase-A clipboard WRITE path.
        self.set_clipboard(
            Clipboard::Standard,
            &[ClipboardContent {
                mime: b"text/plain",
                data: title.as_bytes(),
            }],
            false,
        );
        true
    }

    /// Prompt for a manual title change for the surface.
    ///
    /// PORTS: `surface.zig:1466-1478` `promptTitle` + `titleDialogSet`
    /// (`:3543-3551`). Mirrors [`super::Tab::prompt_tab_title`], but targets the
    /// surface: the initial value is `title_override orelse title` (`:1468`), and
    /// on the dialog's `set` signal we apply the empty-string → `None`
    /// normalization via `setTitleOverride` (`:3550`).
    pub fn prompt_title(&self) {
        use super::super::title_dialog::{Target, TitleDialog};

        // PORTS: `surface.zig:1468` `priv.title_override orelse priv.title`.
        let initial = self.title_override().or_else(|| self.get_title());
        let dialog = TitleDialog::new(Target::Surface, initial.as_deref());

        // PORTS: `surface.zig:1469-1476` — connect `titleDialogSet` to `set`.
        dialog.connect_local(
            "set",
            false,
            glib::clone!(
                #[weak(rename_to = this)]
                self,
                #[upgrade_or]
                None,
                move |args| {
                    // arg 0 is the emitter; arg 1 is the string param.
                    let title: String = args[1].get().expect("set arg is a string");
                    // PORTS: `surface.zig:3549-3550` `titleDialogSet`.
                    this.set_title_override(if title.is_empty() { None } else { Some(title) });
                    None
                }
            ),
        );

        // PORTS: `surface.zig:1478`.
        dialog.present(self.upcast_ref::<gtk4::Widget>());
    }

    /// PORTS: `surface.zig:1747-1778` `sendDesktopNotification`. Build a
    /// `gio.Notification` and send it via the application. The `present-surface`
    /// default action targets this surface's core id, so activating the
    /// notification focuses this surface.
    pub fn send_desktop_notification(&self, title: &[u8], body: &[u8]) {
        use gio::prelude::ApplicationExt as _;

        let app = Application::default();
        let imp = self.imp();

        // `const core_surface = priv.core_surface orelse { warn; return; }`
        // (`:1751-1754`).
        let core = imp.core_surface.load(Ordering::Acquire);
        if core.is_null() {
            log::warn!(target: LOG_SCOPE, "can't send notification because there is no core surface");
            return;
        }
        // SAFETY: a non-null pointer is a live, heap-stable `CoreSurface` (the pin;
        // published by `init_surface`, cleared only after the render/io threads are
        // joined in finalize). This runs on the main thread; `id` is a plain u64.
        let core_id = unsafe { (*core).id };

        // Set a default title if we don't already have one (`:1756-1759`). PORT
        // NOTE (R2 identity): the reference's literal `"Ghostty"` becomes the
        // product display name [`rustty_identity::APP_NAME_TITLECASE`].
        let title_owned;
        let title_str: &str = if title.is_empty() {
            rustty_identity::APP_NAME_TITLECASE
        } else {
            title_owned = String::from_utf8_lossy(title);
            &title_owned
        };
        let body_str = String::from_utf8_lossy(body);

        // Build the notification (`:1761-1774`).
        let notification = gio::Notification::new(title_str);
        notification.set_body(Some(&body_str));

        // `gio.ThemedIcon.new("com.mitchellh.ghostty")` (`:1765`). PORT NOTE (R2):
        // the themed-icon name IS the application id ([`build_info::BASE_APPLICATION_ID`],
        // the single source of truth).
        let icon = gio::ThemedIcon::new(crate::gtk::build_info::BASE_APPLICATION_ID);
        notification.set_icon(&icon);

        // `notification.setDefaultActionAndTargetValue("app.present-surface",
        // glib.Variant.newUint64(core_surface.id))` (`:1768-1772`).
        notification.set_default_action_and_target_value(
            "app.present-surface",
            Some(&core_id.to_variant()),
        );

        // We set the notification ID to the body content. If the content is the
        // same, this notification may replace a previous notification
        // We set the notification ID to the body content so an identical follow-up
        // replaces the prior notification (`:1774-1777`).
        app.upcast_ref::<gio::Application>()
            .send_notification(Some(&body_str), &notification);
    }

    /// Set the scrollbar state for this surface. This will setup the
    /// properties for our Gtk.Scrollable interface properly.
    ///
    /// PORTS: `surface.zig:988-1031` `setScrollbar`. Update the `Gtk.Scrollable`
    /// vadjustment in place from the core's scroll state.
    pub fn set_scrollbar(&self, scrollbar: rustty_vt::pagelist::grow::Scrollbar) {
        let imp = self.imp();

        // Update existing adjustment in-place. If we don't have an
        // adjustment then we do nothing because we're not part of a
        // scrolled window.
        // `const vadj = self.getVAdjustment() orelse return` (`:993`) — reads
        // `priv.vadj`. Clone the GObject so we don't hold the `RefCell` borrow
        // across `configure` (which may synchronously emit `value-changed`). No
        // adjustment => we're not part of a scrolled window, so nothing to do.
        let vadj = {
            let borrow = imp.vadjustment.borrow();
            match borrow.as_ref() {
                Some(adj) => adj.clone(),
                None => return,
            }
        };

        // Check if values match existing adjustment and skip update if so
        // Target values (`:996-999`).
        let value = scrollbar.offset as f64;
        let upper = scrollbar.total as f64;
        let page_size = scrollbar.len as f64;

        // Skip the update if the adjustment already matches (`:1001-1007`,
        // `approxEqAbs` with a 0.001 tolerance).
        const EPS: f64 = 0.001;
        if (vadj.value() - value).abs() < EPS
            && (vadj.upper() - upper).abs() < EPS
            && (vadj.page_size() - page_size).abs() < EPS
        {
            return;
        }

        // If we have a vadjustment we MUST have the signal group since
        // it is setup in the prop handler.
        //
        // During manual scrollbar changes from Rustty core we don't
        // want to emit value-changed signals so we block them. This would
        // cause a waste of resources at best and infinite loops at worst.
        //
        // If we have a vadjustment we MUST have the signal group since it is
        // setup in the prop handler (`:1009-1011`, `const group =
        // priv.vadj_signal_group.?`). rustty's `on_notify_vadjustment` builds the
        // group whenever a vadjustment is set, so it is present here; we tolerate
        // `None` (configure directly) rather than assert, since there is then no
        // connected handler to loop.
        //
        // During manual scrollbar changes from Ghostty core we don't want to emit
        // value-changed signals so we block them. This would cause a waste of
        // resources at best and infinite loops at worst (`:1013-1019`). The
        // scope-end `SignalGroupBlock` guard is the reference's `defer
        // group.unblock()`. The typed binding is behind a glib feature this build
        // does not enable, so block/unblock go through the libgobject-2.0 symbols.
        let _block_guard = imp
            .vadj_signal_group
            .borrow()
            .clone()
            .map(SignalGroupBlock::new);

        // vadj.configure(
        //     value, // value: current scroll position
        //     0, // lower: minimum value
        //     upper, // upper: maximum value (total scrollable area)
        //     1, // step_increment: amount to scroll on arrow click
        //     page_size, // page_increment: amount to scroll on page up/down
        //     page_size, // page_size: size of visible area
        // );
        // `vadj.configure(value, 0, upper, 1, page_size, page_size)` (`:1024-1031`).
        vadj.configure(value, 0.0, upper, 1.0, page_size, page_size);
    }

    /// PORTS: `surface.zig:1135-1202` `commandFinished`. Decide whether a
    /// command-finished notification is warranted (config + the one-shot
    /// `notify-on-next-command-finish` override + duration gate) and, if so, ring
    /// the bell and/or send a desktop notification.
    pub fn command_finished(&self, value: rustty_apprt_api::action::CommandFinished) -> bool {
        use gio::prelude::ActionGroupExt as _;
        use rustty_config::value_types::NotifyOnCommandFinish;

        let imp = self.imp();

        // Read + reset the one-shot `notify-on-next-command-finish` action state
        // (`:1139-1150`). Absent group / absent-or-non-boolean state => false.
        let notify_next_command_finish = {
            let group_ref = imp.action_group.borrow();
            match group_ref.as_ref() {
                None => false,
                Some(group) => {
                    let ag = group.upcast_ref::<gio::ActionGroup>();
                    match ag.action_state("notify-on-next-command-finish") {
                        // `state.isOfType(bool)` + `state.getBoolean()` (`:1145-1146`);
                        // `get::<bool>()` yields `None` for a non-boolean variant.
                        Some(state) => match state.get::<bool>() {
                            Some(notify) => {
                                // Reset to false (`:1147`).
                                ag.change_action_state(
                                    "notify-on-next-command-finish",
                                    &false.to_variant(),
                                );
                                notify
                            }
                            None => false,
                        },
                        None => false,
                    }
                }
            }
        };

        // `const config = priv.config orelse return false` (`:1152`).
        let config_ref = imp.config.borrow();
        let Some(config) = config_ref.as_ref() else {
            return false;
        };
        let cfg = config.get();

        // Config gates (`:1156-1159`), skipped when the one-shot override is set.
        if !notify_next_command_finish {
            if cfg.notify_on_command_finish == NotifyOnCommandFinish::Never {
                return true;
            }
            if cfg.notify_on_command_finish == NotifyOnCommandFinish::Unfocused && self.focused() {
                return true;
            }
        }

        // Duration gate (`:1161`).
        if value.duration.lte(cfg.notify_on_command_finish_after) {
            return true;
        }

        let action = cfg.notify_on_command_finish_action;

        // `if (action.bell) self.setBellRinging(true)` (`:1190`).
        //
        // CORRECTION (task 20, S54-F2): this called `self.ring_bell()`, whose note
        // said "In rustty the full bell lives in [`Surface::ring_bell`]"; the write
        // there was unconditional and unfrozen. See
        // [`Surface::set_bell_ringing_inner`].
        if action.bell {
            self.set_bell_ringing_inner(true);
        }

        // `if (action.notify) { ... sendDesktopNotification(title, body) }`
        // (`:1167-1200`).
        if action.notify {
            // Title (`:1170-1178`), PORTING the reference's `i18n._(...)` wrapping
            // of all three msgids via [`crate::gtk::i18n::tr`].
            let title = match value.exit_code {
                None => tr(c"Command Finished"),
                Some(0) => tr(c"Command Succeeded"),
                Some(_) => tr(c"Command Failed"),
            };

            // Body (`:1180-1196`): `"Command took {f}."` / `"Command took {f} and
            // exited with code {d}."`, the duration rounded to the millisecond and
            // rendered via `Duration`'s human formatter (the `{f}` format).
            // `std.time.ns_per_ms` == 1_000_000.
            let rounded = value.duration.round(1_000_000);
            let mut dur_buf: Vec<u8> = Vec::new();
            rounded.format(&mut dur_buf);
            let dur = String::from_utf8_lossy(&dur_buf);
            let body = match value.exit_code {
                None => format!("Command took {dur}."),
                Some(code) => format!("Command took {dur} and exited with code {code}."),
            };

            self.send_desktop_notification(title.as_bytes(), body.as_bytes());
        }

        true
    }

    /// Notify anyone interested that the readonly status has changed.
    ///
    /// PORTS: `surface.zig:1204-1208` `setReadonly`. The `readonly` property is a
    /// computed read of the core surface's flag (which core has already toggled by
    /// the time this fires); notifying re-evaluates the `reveal-child: bind
    /// template.readonly` binding in `surface.blp`, showing/hiding the read-only
    /// overlay. The `value` is ignored exactly as in the reference (`:1204`).
    pub fn set_readonly(&self, _value: rustty_apprt_api::action::Readonly) -> bool {
        // `self.as(gobject.Object).notifyByPspec(properties.readonly...)` (`:1206`).
        self.notify("readonly");
        true
    }

    // `surface.zig:934-938`:
    // /// Redraw our inspector, if there is one associated with this surface.
    // pub fn redrawInspector(self: *Self) void {
    // PORT NOTE: ported as `crate::gtk::surface::Surface::redraw_inspector`.
    /// The [`InspectorWindow`] currently attached to this surface, if any,
    /// upgraded from the weak backref (`surface.zig:891` `priv.inspector`). The
    /// backing store is a weak ref (`gtk/class/surface.rs:396`); the weak-notify
    /// (`:899`, `inspectorWeakNotify`) nulls it once the window is finalized.
    pub(crate) fn inspector(&self) -> Option<InspectorWindow> {
        self.imp()
            .inspector
            .borrow()
            .as_ref()
            .and_then(glib::object::WeakRef::upgrade)
            .and_then(|obj| obj.downcast::<InspectorWindow>().ok())
    }

    /// PORTS: `surface.zig:885-907` `controlInspector`. Show, hide, or toggle the
    /// terminal inspector window attached to this surface.
    pub fn control_inspector(&self, value: rustty_apprt_api::action::Inspector) -> bool {
        use rustty_apprt_api::action::Inspector;

        // Let's see if we have an inspector already (`:889-891`).
        if let Some(inspector) = self.inspector() {
            match value {
                Inspector::Show => {}
                // Our weak ref will set our private value to null (`:893-894`).
                Inspector::Toggle | Inspector::Hide => {
                    inspector.upcast::<gtk4::Window>().destroy();
                }
            }
        } else {
            match value {
                // `:896-901`.
                Inspector::Toggle | Inspector::Show => {
                    let inspector = InspectorWindow::new(self);
                    inspector.present();

                    // `inspector.as(gobject.Object).weakRef(inspectorWeakNotify,
                    // self)` (`:899`; the notify at `:2618-2625` nulls the
                    // backref). Discarding the returned handle keeps the notify
                    // registered (fire-once on finalize) rather than
                    // disconnecting it.
                    let self_weak = self.downgrade();
                    inspector.add_weak_ref_notify_local(move || {
                        if let Some(this) = self_weak.upgrade() {
                            *this.imp().inspector.borrow_mut() = None;
                        }
                    });

                    // `priv.inspector = inspector` (`:900`), held as a weak
                    // backref (`gtk/class/surface.rs:396`).
                    *self.imp().inspector.borrow_mut() =
                        Some(inspector.upcast_ref::<glib::Object>().downgrade());
                }

                Inspector::Hide => {}
            }
        }

        true
    }

    /// PORTS: `Surface.zig:40-42` `cgroup` → `self.surface.cgroupPath()`.
    ///
    /// **Placeholder-leaf (L6, invariant 8):** the systemd cgroup single-instance
    /// work owns this; kept inhabited returning `None`.
    #[must_use]
    pub fn cgroup_path(&self) -> Option<String> {
        None
    }
}
