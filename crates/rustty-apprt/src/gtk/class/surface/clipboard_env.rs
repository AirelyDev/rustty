//! PORTS: ghostty src/apprt/gtk/class/surface.zig — `clipboardRequest`,
//! `setClipboard`, the clipboard-confirmation dialog flow, and
//! `defaultTermioEnv` (with its env baseline).

use super::*;

impl Surface {
    /// Request data from the clipboard (read the clipboard). A started
    /// request completes asynchronously and will call the
    /// `completeClipboardRequest` core surface API when done.
    ///
    /// PORTS: `surface.zig` `clipboardRequest` → delegates to
    /// `Clipboard.request`.
    pub fn clipboard_request(
        &self,
        clipboard_type: Clipboard,
        state: ClipboardRequest,
    ) -> Result<ClipboardReadResult, ApprtError> {
        // The GTK apprt doesn't support the Kitty clipboard protocol
        // yet.
        // if (state == .kitty_read or state == .kitty_write or state == .list)
        //     return .unsupported;
        if matches!(
            state,
            ClipboardRequest::KittyRead(_)
                | ClipboardRequest::KittyWrite(_)
                | ClipboardRequest::List(_)
        ) {
            return Ok(ClipboardReadResult::Unsupported);
        }

        // Get our requested clipboard (`surface.zig:3861-3864`).
        let clipboard = self.clipboard_for(clipboard_type);

        // For paste requests, check if the clipboard has a text format available.
        // This is a synchronous check that allows performable keybinds to pass
        // through when the clipboard contains non-text content, e.g. images
        // (`surface.zig:3869-3875`).
        if matches!(state, ClipboardRequest::Paste(_))
            && !clipboard.formats().contains_type(String::static_type())
        {
            log::debug!(target: LOG_SCOPE, "clipboard has no text format, not starting paste request");
            // return .unavailable;
            return Ok(ClipboardReadResult::Unavailable);
        }

        // Important: we ref self here so that we can't free memory
        // while we have an outstanding clipboard read.
        //
        // PORT NOTE: `self.ref()` into the `Request` userdata, whose doc reads
        // (`surface.zig:4392-4396`):
        //   /// The request we send as userdata to the clipboard read.
        //   const Request = struct {
        //       /// "Self" is reffed so we can't dispose it until the clipboard
        //       /// read is complete. Callers must unref when done.
        // Here: we ref `self` (clone the GObject — a cheap refcount bump) into
        // the async closure so the surface can't be freed while a clipboard read is
        // outstanding. The reference `self.ref()`s into the `Request` userdata
        // (`surface.zig:3882-3886`); the gtk4-rs closure captures the clone instead.
        let this = self.clone();

        // Read (`surface.zig:3890-3894`). The completion mirrors `clipboardReadText`
        // (`surface.zig:4042-4105`).
        clipboard.read_text_async(gio::Cancellable::NONE, move |result| {
            let text = match result {
                Ok(Some(text)) => text,
                // No text on the clipboard — nothing to deliver.
                Ok(None) => return,
                Err(err) => {
                    log::warn!(target: LOG_SCOPE, "failed to read clipboard err={err}");
                    return;
                }
            };

            // Reach the core the same way the other handlers do (a single
            // atomic load returning `*mut CoreSurface`).
            let Some(core) = this.core() else {
                return;
            };

            // `data: [:0]const u8` -> raw bytes (`&[u8]`, never a lossy String
            // — R3). `completeClipboardRequest` takes `&mut self`; mirror the
            // `&mut *core` deref used by the sizeCallback/occlusion handlers.
            // (`0ce9054bf` end state: the read result travels as a single
            // text/plain representation in the CompleteClipboard payload.)
            match unsafe { &mut *core }.completeClipboardRequest(
                state,
                rustty_core::surface::CompleteClipboard {
                    contents: &[rustty_vt::clipboard::Content {
                        mime: b"text/plain",
                        data: text.as_bytes(),
                    }],
                    ..rustty_core::surface::CompleteClipboard::default()
                },
            ) {
                Ok(()) => {
                    // `Surface.signals.@"clipboard-read".impl.emit(...)`
                    // (`surface.zig:4099-4104`).
                    this.emit_by_name::<()>("clipboard-read", &[]);
                }

                // error.UnsafePaste, error.UnauthorizedPaste => confirmation
                // (`surface.zig:4079-4088`). DEFERRED to Phase B.
                Err(CoreSurfaceError::UnsafePaste | CoreSurfaceError::UnauthorizedPaste) => {
                    this.show_clipboard_confirmation(state, &text);
                }

                Err(err) => {
                    log::warn!(target: LOG_SCOPE, "failed to complete clipboard request err={err}");
                }
            }
        });

        // return .started; (`surface.zig:3896`)
        Ok(ClipboardReadResult::Started)
    }

    /// `surface.zig:4042`: A namespace for our clipboard-related functions so Surface isn't SO large.
    /// PORT NOTE: the `Clipboard` namespace's functions are methods on [`Surface`]
    /// here (`set_clipboard`, `clipboard_request`, `paste`, `clipboard_for`, ...).
    ///
    /// PORTS: `surface.zig:1726-1738` `setClipboard` → delegates to `Clipboard.set`
    /// (`surface.zig:3760-3847`). Set the clipboard contents (a WRITE).
    pub fn set_clipboard(
        &self,
        clipboard_type: Clipboard,
        contents: &[ClipboardContent<'_>],
        confirm: bool,
    ) {
        // Grab our plaintext content for use in confirmation dialogs
        // and signals. We always expect one to exist. (`surface.zig:3768-3774`)
        let Some(text_content) = contents.iter().find(|c| c.mime == b"text/plain") else {
            return;
        };
        // `const text: [:0]const u8 = ... break content.data;` — the payload's
        // BYTES. CORRECTION (task T-D, S56-F3): this was
        // `String::from_utf8_lossy(text_content.data)`, so a non-UTF-8 OSC 52
        // payload reached the `clipboard-write` signal with U+FFFD in place of its
        // bytes. The signal now carries the bytes ([`raw_string_value`]); the
        // confirmation dialog's copy is the one R13 exception (below).
        let text: &[u8] = text_content.data;

        // If no confirmation is necessary, set the clipboard (`surface.zig:3777`).
        if !confirm {
            let clipboard = self.clipboard_for(clipboard_type);

            // Build one `ContentProvider` per content, then union them all
            // (`surface.zig:3784-3826`).
            // Note: we don't need to unref the individual providers
            // because new_union takes ownership of them.
            //
            // The reference falls back to `clipboard.setText(text)` only on an
            // allocation failure:
            // If we fail to alloc, we can at least set the text content.
            // Rust's `Vec` aborts on OOM, so the fallback path is unreachable here.
            let mut providers: Vec<gdk4::ContentProvider> = Vec::with_capacity(contents.len());
            for content in contents {
                let bytes = glib::Bytes::from(content.data);
                if content.mime == b"text/plain" {
                    // Add an explicit UTF-8 encoding parameter to the
                    // text/plain type. The default charset when there is
                    // none is ASCII, and lots of things look for UTF-8
                    // specifically.
                    // The specs are not clear about the order here, but
                    // some clients apparently pick the first match in the
                    // order we set here then garble up bare 'text/plain'
                    // with non-ASCII UTF-8 content, so offer UTF-8 first.
                    //
                    // Note that under X11, GTK automatically adds the
                    // UTF8_STRING atom when this is present.
                    // (`surface.zig:3792-3817`)
                    let text_providers = [
                        gdk4::ContentProvider::for_bytes("text/plain;charset=utf-8", &bytes),
                        gdk4::ContentProvider::for_bytes("text/plain", &bytes),
                    ];
                    providers.push(gdk4::ContentProvider::new_union(&text_providers));
                } else {
                    let mime = String::from_utf8_lossy(content.mime);
                    providers.push(gdk4::ContentProvider::for_bytes(&mime, &bytes));
                }
            }

            let all = gdk4::ContentProvider::new_union(&providers);
            // _ = clipboard.setContent(all);
            //
            // PORT NOTE (task tail-s1b, N203): the reference discards the result
            // and logs nothing; this used to warn `failed to set clipboard
            // content` on gdk4-rs's `Err` arm, a diagnostic the reference does not
            // emit.
            let _ = clipboard.set_content(Some(&all));

            // Surface.signals.@"clipboard-write".impl.emit(
            //     self,
            //     null,
            //     .{ clipboard_type, text.ptr },
            //     null,
            // );
            // (`surface.zig:4117-4122`): the first param as the
            // `GhosttyApprtClipboard` enum the signal declares, the second as the
            // payload's C string, raw.
            self.emit_by_name_with_values(
                "clipboard-write",
                &[
                    ClipboardEnum::from(clipboard_type).to_value(),
                    raw_string_value(text),
                ],
            );

            return;
        }

        // showClipboardConfirmation(self, .{ .osc_52_write = clipboard_type }, text);
        // (`surface.zig:4127-4131`).
        //
        // PORT NOTE: reference defect (R13), `UPSTREAM-DEFECTS.md` UD172. The
        // reference inserts the payload's raw bytes into the dialog's
        // `GtkTextBuffer` (`showClipboardConfirmation`, `insertAtCursor(str,
        // len)`), whose precondition is valid UTF-8. For a non-UTF-8 OSC 52 payload
        // (any program can send one) GTK rejects the insert —
        // `Gtk-CRITICAL: gtk_text_buffer_emit_insert: assertion 'g_utf8_validate
        // (text, len, NULL)' failed`, MEASURED on GTK 4.22.4 — the preview is
        // EMPTY, and confirming writes that empty buffer text to the clipboard
        // (`clipboardConfirmationConfirm` reads the buffer's `text`); in a GTK
        // built with `G_DISABLE_CHECKS` the invalid bytes reach the text btree,
        // which assumes UTF-8 (UNVERIFIED). The port keeps the conservative
        // handling it already had for this one copy: the preview shows the payload
        // with each invalid sequence replaced by U+FFFD, and confirming writes that
        // text, so the user confirms exactly what is written.
        self.show_clipboard_confirmation(
            ClipboardRequest::Osc52Write(clipboard_type),
            &String::from_utf8_lossy(text),
        );
    }

    /// Get the specific type of clipboard for a widget.
    ///
    /// PORTS: `surface.zig:3934-3943` `Clipboard.get`. Get the specific GDK
    /// clipboard for a widget: `.standard` → the widget's default clipboard,
    /// `.selection`/`.primary` → the primary (selection) clipboard.
    ///
    /// The reference returns `?*gdk.Clipboard` (nullable), but gtk4-rs's
    /// `WidgetExt::{clipboard,primary_clipboard}` always yield a `gdk::Clipboard`
    /// (the display always has one), so the `orelse return` guards in the
    /// reference callers are statically unreachable here.
    fn clipboard_for(&self, clipboard: Clipboard) -> gdk4::Clipboard {
        let widget = self.imp().render_surface.upcast_ref::<gtk4::Widget>();
        match clipboard {
            Clipboard::Standard => widget.clipboard(),
            Clipboard::Selection | Clipboard::Primary => widget.primary_clipboard(),
        }
    }

    /// PORTS: `surface.zig:3945-3984` `Clipboard.showClipboardConfirmation`.
    /// Build a `TextBuffer` for the contents, construct the
    /// [`ClipboardConfirmationDialog`], wire `confirm`/`deny`, and present it.
    pub(super) fn show_clipboard_confirmation(&self, req: ClipboardRequest, text: &str) {
        // Build a text buffer for our contents (`surface.zig:3950-3953`). The
        // dialog takes its own ref via the `clipboard-contents` property; the
        // reference `defer`-unrefs its local afterwards, which the gtk4-rs
        // refcount handles automatically when this local drops.
        let contents_buf = gtk4::TextBuffer::new(None);
        contents_buf.insert_at_cursor(text);

        // `gobject.ext.newInstance(ClipboardConfirmationDialog, ...)`
        // (`surface.zig:3956-3966`) — construct, then set the three properties.
        // `can-remember` is true for OSC 52 read/write, false for paste.
        let dialog = ClipboardConfirmationDialog::new();
        let can_remember = matches!(
            req,
            ClipboardRequest::Osc52Read(_) | ClipboardRequest::Osc52Write(_)
        );
        dialog.set_clipboard_contents(Some(contents_buf));
        dialog.set_can_remember(can_remember);
        // Set `request` LAST: it fires `notify::request`, which sets the dialog's
        // heading/body from the variant.
        dialog.set_request(Some(ClipboardRequestBoxed(req)));
        // TEST-ONLY observation point (compiled out of non-test builds): the UD172
        // test reads back the preview buffer of the dialog this call built.
        #[cfg(test)]
        LAST_CLIPBOARD_CONFIRMATION.with(|last| last.replace(Some(dialog.clone())));

        // Wire confirm/deny (`surface.zig:3968-3981`). The signals carry the
        // "remember" flag as their single bool param (`args[1]`). We ref `self`
        // (a cheap GObject refcount bump) into each closure so the surface can't be
        // freed while the dialog is open — mirroring the reference's `self` capture.
        dialog.connect_local(
            "confirm",
            false,
            glib::clone!(
                #[weak(rename_to = this)]
                self,
                #[weak]
                dialog,
                #[upgrade_or]
                None,
                move |args| {
                    let remember = args[1].get::<bool>().unwrap_or(false);
                    this.clipboard_confirmation_confirm(&dialog, remember);
                    None
                }
            ),
        );
        dialog.connect_local(
            "deny",
            false,
            glib::clone!(
                #[weak(rename_to = this)]
                self,
                #[weak]
                dialog,
                #[upgrade_or]
                None,
                move |args| {
                    let remember = args[1].get::<bool>().unwrap_or(false);
                    this.clipboard_confirmation_deny(&dialog, remember);
                    None
                }
            ),
        );

        // `dialog.present(self.as(gtk.Widget))` (`surface.zig:3983`).
        dialog.present(Some(self.upcast_ref::<gtk4::Widget>()));
    }

    /// PORTS: `surface.zig:3986-4022` `clipboardConfirmationConfirm`. On confirm,
    /// optionally persist the "remember" choice to the core surface config, then
    /// complete the clipboard request with `confirmed = true`.
    fn clipboard_confirmation_confirm(&self, dialog: &ClipboardConfirmationDialog, remember: bool) {
        // `priv.core_surface orelse return` (`:3991-3992`).
        let Some(core) = self.core() else {
            return;
        };
        // `dialog.getRequest() orelse return` (`:3993`).
        let Some(req) = dialog.get_request() else {
            return;
        };

        // Handle remember (`:3995-4000`). The core config's `clipboard_read`/
        // `clipboard_write` are directly reachable and mutable here (`&mut *core`),
        // so we port the persistence faithfully (no defer).
        if remember {
            match req.0 {
                ClipboardRequest::Osc52Read(_) => {
                    unsafe { &mut *core }.config.clipboard_read = ClipboardAccess::Allow;
                }
                ClipboardRequest::Osc52Write(_) => {
                    unsafe { &mut *core }.config.clipboard_write = ClipboardAccess::Allow;
                }
                // .paste, .list, .kitty_read, .kitty_write => {},
                ClipboardRequest::Paste(_)
                | ClipboardRequest::List(_)
                | ClipboardRequest::KittyRead(_)
                | ClipboardRequest::KittyWrite(_) => {}
            }
        }

        // Get our text from the dialog's clipboard-contents buffer (`:4002-4014`).
        let Some(buffer) = dialog.get_clipboard_contents() else {
            return;
        };
        let text = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false);

        // surface.completeClipboardRequest(req.*, .{
        //     .contents = &.{.{ .mime = "text/plain", .data = text }},
        //     .confirmed = true,
        // }) catch |err| { ... };
        // Raw bytes, never a lossy String (R3).
        if let Err(err) = unsafe { &mut *core }.completeClipboardRequest(
            req.0,
            rustty_core::surface::CompleteClipboard {
                contents: &[rustty_vt::clipboard::Content {
                    mime: b"text/plain",
                    data: text.as_bytes(),
                }],
                confirmed: true,
                ..rustty_core::surface::CompleteClipboard::default()
            },
        ) {
            log::warn!(target: LOG_SCOPE, "failed to complete clipboard request err={err}");
        }
    }

    /// PORTS: `surface.zig:4025-4040` `clipboardConfirmationDeny`. On deny, only
    /// the "remember" persistence happens (the request is simply left uncompleted).
    fn clipboard_confirmation_deny(&self, dialog: &ClipboardConfirmationDialog, remember: bool) {
        // `priv.core_surface orelse return` (`:4030-4031`).
        let Some(core) = self.core() else {
            return;
        };
        // `dialog.getRequest() orelse return` (`:4032`).
        let Some(req) = dialog.get_request() else {
            return;
        };

        // Handle remember (`:4034-4039`).
        if remember {
            match req.0 {
                ClipboardRequest::Osc52Read(_) => {
                    unsafe { &mut *core }.config.clipboard_read = ClipboardAccess::Deny;
                }
                ClipboardRequest::Osc52Write(_) => {
                    unsafe { &mut *core }.config.clipboard_write = ClipboardAccess::Deny;
                }
                // PORT NOTE: the reference `@panic("request should not be able
                // to be remembered")`; these requests can never set the
                // remember switch to reach here. In Rust we log instead of
                // aborting.
                ClipboardRequest::Paste(_)
                | ClipboardRequest::List(_)
                | ClipboardRequest::KittyRead(_)
                | ClipboardRequest::KittyWrite(_) => {
                    log::warn!(target: LOG_SCOPE, "request should not be able to be remembered");
                }
            }
        }
    }

    /// The baseline environment `defaultTermioEnv` starts from
    /// (`surface.zig:1596-1599`, oracle `b988efcfe`): under Flatpak the child env
    /// starts EMPTY (`std.process.Environ.Map.init(alloc)`) so the sandbox's own
    /// environment is not leaked to the host command; otherwise it is the
    /// inherited process env (`global.environMap()`).
    ///
    /// Extracted with the predicate as an argument so it is testable without a
    /// live Flatpak sandbox.
    #[must_use]
    pub(super) fn termio_env_baseline(is_flatpak: bool) -> rustty_global::EnvMap {
        if is_flatpak {
            rustty_global::EnvMap::default()
        } else {
            // PORT NOTE (P3-W7 T4): `internal_os.getEnvMap(alloc)` ->
            // `global.environMap()` (`src/os/env.zig` was DELETED by `e8525c0fd`).
            rustty_global::environ_map()
        }
    }

    /// PORTS: `surface.zig:1593-1645` `defaultTermioEnv`. The default environment
    /// for the surface's termio subprocess: the Flatpak-aware baseline env
    /// (`:1596-1599`) with the language override applied and the GTK /
    /// desktop-activation vars stripped so they don't leak to the child.
    ///
    /// CORRECTION (task 20, S55-F1): this doc used to carry "PORT NOTE (deferred):
    /// the snap-path filtering (`:1625-1630`, comptime `build_config.snap` — false
    /// in this build) lands with its own tier". `snap` is a real build option
    /// (`rustty_build_config::snap()`, set by `snap/snapcraft.yaml`'s build), so a
    /// shipped snap leaked `SNAP_*` and snap-prefixed paths to child shells.
    /// [`filter_snap_paths`] is the port, called below.
    #[must_use]
    pub fn default_termio_env(&self) -> rustty_global::EnvMap {
        let app = Application::default();
        // `var env = if (internal_os.isFlatpak()) std.process.Environ.Map.init(alloc)
        // else try global.environMap();` (`:1596-1599`).
        let mut env = Self::termio_env_baseline(rustty_os::flatpak::isFlatpak());

        // Language override (`:1602-1606`).
        match app.saved_language() {
            Some(language) => {
                env.put(b"LANG", language.as_bytes());
            }
            None => {
                env.removev(b"LANG");
            }
        }

        // (`:1608-1621`)
        for key in [
            // Don't leak these GTK environment variables to child processes.
            b"GDK_DEBUG".as_slice(),
            b"GDK_DISABLE",
            b"GSK_RENDERER",
            // Remove some environment variables that are set when Rustty is launched
            // from a `.desktop` file, by D-Bus activation, or systemd.
            b"GIO_LAUNCHED_DESKTOP_FILE",
            b"GIO_LAUNCHED_DESKTOP_FILE_PID",
            b"DBUS_STARTER_ADDRESS",
            b"DBUS_STARTER_BUS_TYPE",
            b"INVOCATION_ID",
            b"JOURNAL_STREAM",
            b"NOTIFY_SOCKET",
        ] {
            env.removev(key);
        }

        // Unset environment varies set by snaps if we're running in a snap.
        // This allows Ghostty to further launch additional snaps.
        // if (comptime build_config.snap) { if (env.get("SNAP") != null) try filterSnapPaths(alloc, &env); }
        if rustty_build_config::snap() && env.hasv(b"SNAP") {
            filter_snap_paths(&mut env);
        }

        // `if (ext.getAncestor(Window, self.as(gtk.Widget))) |window| { try
        // window.winproto().addSubprocessEnv(&env); if (window.isQuickTerminal())
        // try env.put("GHOSTTY_QUICK_TERMINAL", "1"); }` (`surface.zig:1636-1642`).
        //
        // This is a hack because it ties ourselves (optionally) to the
        // Window class. The right solution we should do is emit a signal
        // here where the handler can modify our EnvMap, but boxing the
        // EnvMap is a bit annoying so I'm punting it.
        if let Some(window) = crate::gtk::ext::ancestor::<crate::gtk::class::window::Window>(self) {
            if let Err(err) = window.winproto_mut().add_subprocess_env(&mut env) {
                // log.warn("winproto failed to add subprocess env err={}", .{err});
                log::warn!(
                    target: LOG_SCOPE,
                    "winproto failed to add subprocess env err={err:?}"
                );
            }

            if window.is_quick_terminal() {
                // R2: product-namespaced, unlike `WINDOWID` above.
                env.put(
                    format!("{}_QUICK_TERMINAL", rustty_identity::ENV_PREFIX).as_bytes(),
                    b"1",
                );
            }
        }

        env
    }
}
