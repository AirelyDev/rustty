//! PORTS: ghostty src/apprt/gtk/class/application.zig — the file-local free
//! functions between `Application.Private` and the public API: `handleSigusr2`,
//! the `globalShortcutBindFailed` string assembly, the nine `app.*` gio action
//! callbacks, `deinitPrivate`, `glibLogWriterFunction`, `setGtkEnv`, the
//! `run` error set and cleanup guard, and `findActiveWindow`.

use super::*;

// SIGUSR2 signal handler via g_unix_signal_add
//
// PORTS: `application.zig:1507-1524` `handleSigusr2`. The SIGUSR2 handler
// installed by [`imp::Application::startup_signals`]. `g_unix_signal_add` calls
// it on the main loop when SIGUSR2 is delivered; returning `G_SOURCE_CONTINUE`
// keeps the handler installed.
pub(super) unsafe extern "C" fn handle_sigusr2(ud: glib::ffi::gpointer) -> glib::ffi::gboolean {
    // `const self: *Self = @ptrCast(@alignCast(ud orelse return CONTINUE))` (`:1508`).
    // The userdata is the `Application` GObject pointer (`startup_signals`); a null
    // means the source outlived the app, so keep the handler installed and bail.
    if ud.is_null() {
        return glib::ffi::GTRUE;
    }

    log::info!(target: LOG_SCOPE, "received SIGUSR2, reloading configuration");

    // `Action.reloadConfig(self, .app, .{})` (`:1512-1521`). Reach the app singleton
    // (rustty's blessed `self`-equivalent accessor; the `ud` pointer is that same
    // object) and drive an `.app`-target reload directly — the reference calls the
    // `Action` helper straight, NOT through `core.performAction`. On failure we log;
    // "we should show another GUI" (`:1516-1519`) is future work.
    let app = Application::default();
    if let Err(err) = action::reload_config(
        &app,
        Target::<AppTargetHandle>::App,
        rustty_apprt_api::action::ReloadConfig::default(),
    ) {
        // If we fail to reload the configuration, then we want the
        // user to know it. For now we log but we should show another
        // GUI.
        //
        // log.warn("error reloading config: {}", .{err});
        // (task-14d-3: the error VALUE was dropped — the `.is_err()` discarded it.)
        // `{}` of the error is its Zig name (`ReloadConfigError`'s Display, N311).
        log::warn!(target: LOG_SCOPE, "error reloading config: {err}");
    }

    // `return @intFromBool(glib.SOURCE_CONTINUE)` (`:1523`).
    glib::ffi::GTRUE
}

/// PORTS: `application.zig:1700-1719` — the pure string-assembly half of
/// `globalShortcutBindFailed`, split out so it is testable with no GApplication, no
/// D-Bus and no compositor. Returns `(title, body)`.
///
/// The body is `"<trigger label>: <detail>"`, where the detail is the
/// compositor-supplied `message` when it is non-empty and one of two translated
/// fallbacks otherwise (`:1707-1713`). The trigger label falls back to `"?"` when
/// the trigger cannot be rendered (`:1701-1705`).
///
/// Returns `None` where the reference returns without raising a notification: the
/// body does not fit its 512-byte buffer (`:1773-1778`, `bufPrintZ(...) catch
/// return`).
///
/// The body is BYTES, as the reference's is: `failure.message` is compositor wire
/// data (`[*:0]const u8`), and the 512-byte limit is a limit on those bytes. It is
/// decoded only where it becomes display text (`action::desktop_notification`).
///
/// CORRECTION (task T-D, S49-F4 / S50-F3): this returned `(String, String)` under
/// a PORT NOTE reading "rustty's `String` grows, so the notification is always
/// raised; a notification silently dropped for length is a defect, not behaviour
/// worth reproducing", and it decoded `failure.message` lossily before measuring
/// anything. Neither buffer limit was reproduced. A dropped notification is not a
/// crash, leak, hang or UB, so `PORT_RULES.md` R13 does not apply and R1 does: both
/// limits are reproduced ([`BIND_FAILED_LABEL_BUF_LEN`], [`BIND_FAILED_BODY_BUF_LEN`])
/// and the silent drop is recorded upstream as a latent defect
/// (`UPSTREAM-DEFECTS.md` UD174).
pub(crate) fn bind_failed_notification(failure: &BindFailed) -> Option<(String, Vec<u8>)> {
    // var label_buf: [128]u8 = undefined;
    // var writer: std.Io.Writer = .fixed(&label_buf);
    // const label: []const u8 = label: {
    //     const ok = key.labelFromTrigger(&writer, failure.trigger) catch false;
    //     break :label if (ok) writer.buffered() else "?";
    // };
    let label = {
        let mut buf = String::new();
        let ok = crate::gtk::key::label_from_trigger(&mut buf, failure.trigger);
        bind_failed_label(ok, buf)
    };

    // const detail: [*:0]const u8 = detail: {
    //     if (failure.message[0] != 0) break :detail failure.message;
    //     break :detail if (failure.revoked) ... else ...;
    // };
    //
    // `[*:0]`: the message ends at its first NUL.
    let message = failure
        .message
        .split(|&b| b == 0)
        .next()
        .unwrap_or_default();
    let detail: &[u8] = if message.is_empty() {
        if failure.revoked {
            tr(c"The keybind was revoked by the system.").as_bytes()
        } else {
            tr(c"The keybind was denied by the system.").as_bytes()
        }
    } else {
        message
    };

    // var body_buf: [512]u8 = undefined;
    // const body = std.fmt.bufPrintZ(&body_buf, "{s}: {s}", .{ label, detail }) catch return;
    let body = bind_failed_body(label.as_bytes(), detail)?;

    Some((tr(c"Global keybind unavailable").to_owned(), body))
}

/// `compat_file.ReadToEndAllocError` as `loadCustomCss` distinguishes it:
/// `error.FileTooBig` versus every other read error.
#[derive(Debug)]
pub(crate) enum ReadToEndAllocError {
    /// `error.FileTooBig`: more than `max_bytes` were read.
    FileTooBig,
    /// Any other read error (`else => |e| return e`).
    Io(std::io::Error),
}

/// PORTS: `src/lib/compat/file.zig:15-31` `readToEndAlloc`: read from the file's
/// current position to end of stream, `error.FileTooBig` once more than
/// `max_bytes` have been read. A STREAMING limit, as the reference's is (it never
/// consults the file's size), so a file whose reported size is not its content
/// (a `/proc` file, a FIFO) is measured by what it yields.
pub(crate) fn read_to_end_alloc(
    file: std::fs::File,
    max_bytes: usize,
) -> Result<Vec<u8>, ReadToEndAllocError> {
    use std::io::Read;
    let mut contents = Vec::new();
    // Reading at most `max_bytes + 1` is enough to tell "fits" from "too big":
    // the reference fails as soon as its running total would pass `max_bytes`.
    file.take(max_bytes as u64 + 1)
        .read_to_end(&mut contents)
        .map_err(ReadToEndAllocError::Io)?;
    if contents.len() > max_bytes {
        return Err(ReadToEndAllocError::FileTooBig);
    }
    Ok(contents)
}

/// `var label_buf: [128]u8` (`application.zig:1758`).
const BIND_FAILED_LABEL_BUF_LEN: usize = 128;
/// `var body_buf: [512]u8` (`application.zig:1773`).
const BIND_FAILED_BODY_BUF_LEN: usize = 512;

/// The label half of `globalShortcutBindFailed`: `labelFromTrigger` into a
/// `.fixed(&label_buf)` writer fails with `WriteFailed` once the label exceeds
/// the 128-byte buffer, and a failure (of either kind) becomes `"?"`.
pub(super) fn bind_failed_label(ok: bool, label: String) -> String {
    if ok && label.len() <= BIND_FAILED_LABEL_BUF_LEN {
        label
    } else {
        "?".to_owned()
    }
}

/// The body half: `bufPrintZ(&body_buf, "{s}: {s}", ...)` needs the formatted
/// bytes PLUS the NUL sentinel to fit 512 bytes, so a body of 512 bytes or more
/// is `error.NoSpaceLeft`, which the reference turns into `return` (no
/// notification).
fn bind_failed_body(label: &[u8], detail: &[u8]) -> Option<Vec<u8>> {
    let mut body = Vec::with_capacity(label.len() + 2 + detail.len());
    body.extend_from_slice(label);
    body.extend_from_slice(b": ");
    body.extend_from_slice(detail);
    (body.len() < BIND_FAILED_BODY_BUF_LEN).then_some(body)
}

/// A zero-sized [`CoreSurfaceHandle`] used only to name the `C` type parameter of
/// [`Target::App`] from the non-generic reload entry points ([`handle_sigusr2`] and
/// [`action_reload_config`]). The `.app` target never dereferences a surface handle
/// (the reference's `.app` arms take no `core`), so this handle is never read — it
/// exists purely to satisfy the generic `Target<'_, C>`.
pub(super) struct AppTargetHandle;

impl CoreSurfaceHandle for AppTargetHandle {
    fn rt_surface_ptr(&self) -> *mut c_void {
        std::ptr::null_mut()
    }
}

// `std.posix.SIG.USR2` (`application.zig:1468-1469`), which resolves per target.
//
// CORRECTION (task 20, S49-F1): this was `const SIGUSR2: core::ffi::c_int = 12;`
// under a comment saying "SIGUSR2 on Linux (all glibc targets rustty builds for —
// aarch64/x86_64) is 12 ... `libc` is not a dependency here, so the value is
// inlined". Both halves were false: this module also builds for FreeBSD
// (`lib.rs`'s `any(target_os = "linux", target_os = "freebsd")` gate), where 12 is
// SIGSYS and SIGUSR2 is 31, and `libc` IS a dependency of this crate
// (`Cargo.toml`, the WS4 SIGPIPE sigaction). `libc::SIGUSR2` follows the target.
pub(super) const SIGUSR2: core::ffi::c_int = libc::SIGUSR2;

// `g_unix_signal_add` is a GLib macro over `g_unix_signal_add_full`; the gtk4-rs
// bindings wrap neither, so the real symbol is declared here and resolved at link
// time against the already-linked `libglib-2.0` (glib-sys). Signature per
// `glib-unix.h`. (`application.zig:1403` `glib.unixSignalAdd`.)
extern "C" {
    pub(super) fn g_unix_signal_add_full(
        priority: core::ffi::c_int,
        signum: core::ffi::c_int,
        function: glib::ffi::GSourceFunc,
        data: glib::ffi::gpointer,
        notify: glib::ffi::GDestroyNotify,
    ) -> core::ffi::c_uint;
}

// PORTS: the nine `app.*` action callbacks (`application.zig:1854-2202`; B5's
// `new-tab` and B7's `ring-bell` joined the original seven). Each is
// the `gio.SimpleAction::activate` handler `ext::actions` wires with a WEAK ref to
// the target `Application` (see `ext/actions.rs`). Their bodies dispatch through
// the core app / action surface (`core_app.performAction`, `Action.newWindow`,
// `core().mailbox.push`), which reaches the type-erased `core_app`/`core()` and
// the `performAction` dispatch (Task 23) — placeholder-leaves until then, but
// REGISTERED verbatim so the action map is complete (the `startup` KAT asserts all
// nine names).

/// Handle `app.new-window` and `app.new-window-command` GTK actions
///
/// PORTS: `application.zig:1899-1936` `actionNewWindow` (`new-window` /
/// `new-window-command`; B5 end state — the argv parse is factored into
/// [`parse_overrides`]). Parses the optional `as` argv override, then requests
/// a new window.
pub(super) fn action_new_window(
    _action: &gio::SimpleAction,
    parameter: Option<&glib::Variant>,
    app: &Application,
) {
    // `log.debug("received new window action")` (`:1904`).
    log::debug!(target: LOG_SCOPE, "received new window action");

    // `const overrides = overrides: { ... }` (`:1911-1930`) — parse the
    // optional `as` parameter; a parse failure collapses to null (`catch
    // null`, `:1929`).
    let overrides: Option<TabOverrides> = 'overrides: {
        // were we given a parameter? (`:1913`.)
        let Some(arguments) = parameter else {
            break 'overrides None;
        };

        // ensure that the supplied parameter is an array of strings
        // (`:1918-1925`).
        if !arguments.is_type(glib::VariantTy::STRING_ARRAY) {
            log::warn!(target: LOG_SCOPE, "parameter is of type '{}', not 'as'",
                arguments.type_().as_str(),);
            break 'overrides None;
        }

        parse_overrides(arguments.iter())
    };

    // `Action.newWindow(self, null, if (overrides) |o| .{ ... } else .none)`
    // (`:1932-1944`). Unlike `activate` (which queues a `.new_window` on the
    // core mailbox), the received action opens the window directly so the
    // overrides are honored; `parent` is null → `.app` target.
    if let Err(err) = action::new_window(
        app,
        Target::<AppTargetHandle>::App,
        overrides.unwrap_or_default(),
    ) {
        log::warn!(target: LOG_SCOPE, "unable to create new window: {err:?}");
    }
}

/// Handle `app.new-tab` GTK action
///
/// PORTS: `application.zig:1946-2011` `actionNewTab` (B5, 034506f14): the
/// `(tas)` parameter carries the target surface ID (0 == focused) and the CLI
/// argument array. The surface lookup + `Action.newTab`/`Action.newWindow`
/// fan-out needs the typed core app, so it lives in the monomorphized
/// `new_tab_thunk` installed by [`Application::bind_core_app`] (the
/// `present_surface_thunk` precedent); unbound core → no-op
/// (placeholder-leaf doctrine).
pub(super) fn action_new_tab(
    _action: &gio::SimpleAction,
    parameter: Option<&glib::Variant>,
    app: &Application,
) {
    // `log.debug("received new tab action")` (`:1951`).
    log::debug!(target: LOG_SCOPE, "received new tab action");

    // `const surface_id_, const overrides = result: { ... }` (`:1959-1993`).
    let (surface_id, overrides) = {
        // were we given a parameter? (`:1961-1964`.)
        let Some(parameter) = parameter else {
            log::warn!(target: LOG_SCOPE, "app.new-tab did not receive a parameter");
            return;
        };

        // `log.warn("got parameter: {s}", .{parameter.getTypeString()})`
        // (`:1966` — the reference logs this unconditionally).
        log::warn!(target: LOG_SCOPE, "got parameter: {}", parameter.type_().as_str());

        // ensure that the supplied parameter is an array of strings
        // (`:1968-1978` — the `(tas)` tuple check).
        let Ok(tas) = glib::VariantTy::new("(tas)") else {
            return;
        };
        if !parameter.is_type(tas) {
            log::warn!(target: LOG_SCOPE, "parameter is of type '{}', not '(tas)'",
                parameter.type_().as_str(),);
            return;
        }

        // `parameter.get("(tas)", &surface_id, &arguments_it_)` (`:1980-1986`).
        let surface_id = parameter.child_value(0).get::<u64>().unwrap_or(0);
        let arguments = parameter.child_value(1);

        // `const overrides = parseOverrides(alloc, arguments_it) catch return;`
        // (`:1988`).
        let Some(overrides) = parse_overrides(arguments.iter()) else {
            return;
        };

        // `break :result .{ if (surface_id == 0) null else surface_id,
        // overrides };` (`:1990`).
        (
            if surface_id == 0 {
                None
            } else {
                Some(surface_id)
            },
            overrides,
        )
    };

    // The surface lookup + dispatch (`:1995-2010`), monomorphized in
    // [`imp::Application::new_tab_thunk`].
    if let Some(thunk) = app.imp().new_tab_thunk.borrow().as_ref() {
        thunk(surface_id, overrides);
    }
}

// `parseOverrides` (`application.zig:2013-2098` pre-refactor) moved to its own
// file, 9fc8d9ebd: [`super::overrides::parse_overrides`] (imported above), and
// its `TabOverrides` (formerly this file's `tab::TabOverrides`) with it.

/// PORTS: `application.zig:2114-2148` `actionOpenConfig` (`open-config`;
/// 0a183c923 — the string parameter selects the `os-open` / `new-window`
/// target).
pub(super) fn action_open_config(
    _action: &gio::SimpleAction,
    parameter: Option<&glib::Variant>,
    app: &Application,
) {
    use rustty_core::app::OpenConfig;

    // `const target: CoreApp.Message.OpenConfig = target: { ... }` (`:2070-
    // 2100`): the `StaticStringMap` lookup over the two detailed-action target
    // strings, defaulting to `.os_open` on every malformed-parameter path.
    let target: OpenConfig = 'target: {
        let Some(parameter) = parameter else {
            log::warn!(target: LOG_SCOPE, "no parameter provided to app.open-config action");
            break 'target OpenConfig::OsOpen;
        };

        if !parameter.is_type(glib::VariantTy::STRING) {
            log::warn!(target: LOG_SCOPE, "parameter to app.open-config action is of type '{}' not 's'",
                parameter.type_().as_str(),);
            break 'target OpenConfig::OsOpen;
        }

        let Some(str) = parameter.str() else {
            break 'target OpenConfig::OsOpen;
        };

        // `target_map.get(str) orelse { warn; break :target .os_open; }`
        // (`:2120-2123,2141-2145`).
        match str {
            "os-open" => OpenConfig::OsOpen,
            "new-window" => OpenConfig::NewWindow,
            _ => {
                log::warn!(target: LOG_SCOPE, "'{str}' is not configured as a target for the \
                     app.open-config action");
                OpenConfig::OsOpen
            }
        }
    };

    // _ = self.core().mailbox.push(global.io(), .{ .open_config = target }, .forever);
    //
    // CORRECTION (task 20, S50-F5): this used to call `action::open_config(app,
    // target)` directly, under a note arguing the mailbox hop "is drained to
    // `rt_app.performAction(.app, .open_config, <target>)` — a pure passthrough".
    // The end state is the same, but the reference performs it on the NEXT core
    // tick, after anything already queued, and the drain emits the core's
    // `mailbox message=open_config` log line. Both are observable, so the push is
    // restored through the bound thunk ([`imp::Application::open_config_thunk`]).
    if let Some(thunk) = app.imp().open_config_thunk.borrow().as_ref() {
        thunk(target);
    }
}

/// PORTS: `application.zig:2170-2202` `actionRingBell` (B7, f0f3f4d8d — the
/// application-owned audio bell; the surface's `ringBell` activates
/// `app.ring-bell`, `surface.rs` `ring_bell`).
pub(super) fn action_ring_bell(
    _action: &gio::SimpleAction,
    _parameter: Option<&glib::Variant>,
    app: &Application,
) {
    use std::os::unix::ffi::OsStrExt;

    let Some(config) = app.get_config() else {
        return;
    };
    let config = config.get();

    // Do our sound
    // `if (config.@"bell-features".audio) audio: { ... }` (`:2178-2201`).
    if !config.bell_features.audio {
        return;
    }

    // `const config_path = config.@"bell-audio-path" orelse break :audio;`
    // (`:2179-2183`): the `Path` union carries the optional/required flag.
    let Some(config_path) = config.bell_audio_path.as_ref() else {
        return;
    };
    let (path_bytes, required) = match config_path {
        rustty_config::path::Path::Optional(p) => (p.as_slice(), false),
        rustty_config::path::Path::Required(p) => (p.as_slice(), true),
    };

    // `std.math.clamp(config.@"bell-audio-volume", 0.0, 1.0)` (`:2185-2189`).
    let volume = config.bell_audio_volume.clamp(0.0, 1.0);

    let path = std::path::Path::new(std::ffi::OsStr::from_bytes(path_bytes));

    // Reuse one MediaFile per application (rebuilt only when the path
    // changes) so each bell replays the same pipeline instead of
    // leaking a fresh one. Assign unconditionally: bellMediaFile frees
    // any stale MediaFile and returns the current slot value (possibly
    // null if the path is now inaccessible), so priv.bell_media never
    // dangles. (`:2191-2200`.)
    let imp = app.imp();
    let current = imp.bell_media.borrow_mut().take();
    let media_file = crate::gtk::media::bell_media_file(current, path, required);
    if let Some(media_file) = &media_file {
        crate::gtk::media::play_bell(media_file, volume);
    }
    imp.bell_media.replace(media_file);
}

/// PORTS: `application.zig:1819-1850` `actionPresentSurface` (`present-surface`,
/// `t`).
pub(super) fn action_present_surface(
    _action: &gio::SimpleAction,
    parameter: Option<&glib::Variant>,
    app: &Application,
) {
    // `const parameter = parameter_ orelse return;` (`:1824`).
    let Some(parameter) = parameter else {
        return;
    };

    // Make sure that we've received a u64 from the system (`:1826-1832`). Zig
    // builds a `VariantType.newFor(u64)` and checks `Variant.isOfType`; the glib
    // Rust binding folds both into a typed `get::<u64>()` that returns `None`
    // when the variant is not of type `t` (`"t"`, the u64 type string).
    let Some(surface_id) = parameter.get::<u64>() else {
        return;
    };

    // Convert the u64 to a core surface by using it as a surface ID.
    // A value of zero means that there was no target surface for the
    // notification so we don't focus any surface.
    //
    // (`:1834-1838`.)
    if surface_id == 0 {
        return;
    }

    // `self.core().findSurfaceByID(surface_id)` + push a `.surface_message`/
    // `.present_surface` envelope (`:1839-1849`). The monomorphized lookup+push
    // lives in the [`imp::Application::present_surface_thunk`], installed by
    // [`Application::bind_core_app`]; unbound core → no-op (placeholder-leaf).
    if let Some(thunk) = app.imp().present_surface_thunk.borrow().as_ref() {
        thunk(surface_id);
    }
}

/// PORTS: `application.zig:1687-1696` `actionQuit` (`quit`).
pub(super) fn action_quit(
    _action: &gio::SimpleAction,
    _parameter: Option<&glib::Variant>,
    app: &Application,
) {
    // `priv.core_app.performAction(self.rt(), .quit)` (`:1693`). The core `.quit`
    // action is a pure passthrough back to the apprt (`App.zig:425`), so it
    // collapses to the direct `Application::quit` the `Action::Quit` performAction
    // arm takes (cf. [`action_reload_config`]).
    app.quit();
}

/// PORTS: `application.zig:1665-1674` `actionReloadConfig` (`reload-config`).
pub(super) fn action_reload_config(
    _action: &gio::SimpleAction,
    _parameter: Option<&glib::Variant>,
    app: &Application,
) {
    // `priv.core_app.performAction(self.rt(), .reload_config)` (`:1671`). The core
    // app's `.reload_config` scoped action is a pure passthrough back to the apprt
    // (`App.zig:428`, `rt_app.performAction(.app, .reload_config, .{})`), so it
    // collapses to the same `.app`-target reload the SIGUSR2 handler performs. Route
    // it directly. On failure, log (`:1672`; `application.zig:1792-1794` at the
    // pin). The core passthrough `try`s the apprt's error, so `err` is
    // `reloadConfig`'s own error and the line prints its Zig name (N311).
    if let Err(err) = action::reload_config(
        app,
        Target::<AppTargetHandle>::App,
        rustty_apprt_api::action::ReloadConfig::default(),
    ) {
        // priv.core_app.performAction(self.rt(), .reload_config) catch |err| {
        //     log.warn("error reloading config err={}", .{err});
        // };
        // (task-14d-3: the error VALUE was dropped — the `.is_err()` discarded it.)
        log::warn!(target: LOG_SCOPE, "error reloading config err={err}");
    }
}

/// PORTS: `application.zig:1676-1685` `actionToggleQuickTerminal`
/// (`toggle-quick-terminal`).
pub(super) fn action_toggle_quick_terminal(
    _action: &gio::SimpleAction,
    _parameter: Option<&glib::Variant>,
    app: &Application,
) {
    // `priv.core_app.performAction(self.rt(), .toggle_quick_terminal)` (`:1682`).
    // The core `.toggle_quick_terminal` action is a pure passthrough back to the
    // apprt (`application.zig:762` → `Action.toggleQuickTerminal(self)`), so it
    // collapses to the same direct `action::toggle_quick_terminal` route the
    // `Action::ToggleQuickTerminal` performAction arm takes (cf.
    // [`action_reload_config`]). On failure the toggle just returns `false`.
    let _ = action::toggle_quick_terminal(app);
}

/// PORTS: `application.zig:435-459` `deinit` (the field-clearing body). Shared by
/// [`Application::deinit`] (the run loop's explicit force-teardown) and
/// [`imp::Application`]'s `Drop` (the GObject finalize hook). Idempotent: each
/// clear is a no-op once the field is already cleared.
pub(super) fn deinit_private(priv_: &imp::Application) {
    // `priv.config.unref()` (`application.zig:438`) — clearing drops the ref.
    priv_.config.replace(None);

    // `priv.winproto.deinit()` (`application.zig:439`).
    priv_.winproto.borrow_mut().deinit();

    // `priv.open_uri.deinit()` (`:440`) — cancels the cleanup timer, unsubscribes
    // every in-flight `Response`, drops the entries and releases the D-Bus
    // connection. Idempotent (its own `alive` flag).
    priv_.open_uri.deinit();

    // `priv.global_shortcuts.unref()` (`:441`) — clearing the field drops the ref.
    priv_.global_shortcuts.replace(None);

    // Drop the queued-window / tick / confirm-quit thunks (rustty-only seams) so
    // they release their captured `core_app`/`rt_app` pointers; the pointers are
    // BACKREFs (never owned here).
    priv_.new_window_thunk.replace(None);
    priv_.new_tab_thunk.replace(None);
    priv_.open_config_thunk.replace(None);
    priv_.tick_thunk.replace(None);
    priv_.needs_confirm_quit_thunk.replace(None);
    priv_.update_config_thunk.replace(None);
    priv_.color_scheme_thunk.replace(None);
    priv_.perform_all_action_thunk.replace(None);

    // `if (priv.saved_language) |language| alloc.free(language)` (`:442`) —
    // clearing drops the `CString`.
    priv_.saved_language.replace(None);

    // Remove the css provider and each custom provider from the display
    // (`application.zig:443-455`).
    if let Some(display) = gdk4::Display::default() {
        if let Some(provider) = priv_.css_provider.borrow().as_ref() {
            gtk4::style_context_remove_provider_for_display(&display, provider);
        }
        for provider in priv_.custom_css_providers.borrow().iter() {
            gtk4::style_context_remove_provider_for_display(&display, provider);
        }
    }

    // `priv.css_provider.unref()` (`:456`).
    priv_.css_provider.replace(None);

    // Each custom provider `unref` + the list `deinit` (`:457-458`) — clearing the
    // Vec drops (unrefs) each element.
    priv_.custom_css_providers.borrow_mut().clear();
}

/// Function used to funnel GLib/GObject/GTK log messages into Zig's logging
/// system rather than just getting dumped directly to stderr.
///
/// PORTS: `application.zig:48-110` `glibLogWriterFunction`. Funnels
/// GLib/GObject/GTK log messages through rustty's logging — the `log::` facade
/// under the reference's own `glib` scope (`GLIB_LOG_SCOPE`) — rather than
/// letting GLib dump them straight to stderr unstructured. Registered once via
/// [`glib::log_set_writer_func`] in
/// [`Application::new`] (`application.zig:251`).
///
/// The reference scans the structured-log fields for `MESSAGE` / `GLIB_DOMAIN`
/// and dispatches on the level flags (`application.zig:60-109`); gtk4-rs hands us
/// a decoded [`glib::LogLevel`] and typed [`glib::LogField`]s, so both port
/// directly.
pub(super) fn glib_log_writer(
    level: glib::LogLevel,
    fields: &[glib::LogField<'_>],
) -> glib::LogWriterOutput {
    let mut message: Option<&str> = None;
    let mut domain: Option<&str> = None;
    for field in fields {
        let Some(value) = field.value_str() else {
            continue;
        };
        match field.key() {
            // `if (std.mem.eql(u8, k, "MESSAGE"))` (`application.zig:71`).
            "MESSAGE" => message = Some(value),
            // `if (std.mem.eql(u8, k, "GLIB_DOMAIN"))` (`application.zig:75`).
            "GLIB_DOMAIN" => domain = Some(value),
            _ => continue,
        }
    }

    // `const message = message_ orelse return .unhandled;` (`application.zig:81`).
    let Some(message) = message else {
        return glib::LogWriterOutput::Unhandled;
    };
    // `const domain = domain_ orelse "«unknown»";` (`application.zig:82`).
    let domain = domain.unwrap_or("«unknown»");

    // The level dispatch (`application.zig:84-109`). gtk4-rs decodes the level
    // flags to the highest-severity `LogLevel`, so a single `match` reproduces
    // the reference's ordered flag checks.
    match level {
        // error + critical -> err (`application.zig:84-91`).
        glib::LogLevel::Error => {
            log::error!(target: GLIB_LOG_SCOPE, "ERROR: {domain}: {message}");
        }
        glib::LogLevel::Critical => {
            log::error!(target: GLIB_LOG_SCOPE, "CRITICAL: {domain}: {message}");
        }
        // warning -> warn (`application.zig:92-95`).
        glib::LogLevel::Warning => {
            log::warn!(target: GLIB_LOG_SCOPE, "WARNING: {domain}: {message}");
        }
        // message + info -> info (`application.zig:96-103`).
        glib::LogLevel::Message => {
            log::info!(target: GLIB_LOG_SCOPE, "MESSAGE: {domain}: {message}");
        }
        glib::LogLevel::Info => {
            log::info!(target: GLIB_LOG_SCOPE, "INFO: {domain}: {message}");
        }
        // debug -> debug (`application.zig:104-107`).
        glib::LogLevel::Debug => {
            log::debug!(target: GLIB_LOG_SCOPE, "DEBUG: {domain}: {message}");
        }
    }
    glib::LogWriterOutput::Handled
}

/// This sets various GTK-related environment variables as necessary
/// given the runtime environment or configuration.
///
/// This must be called BEFORE GTK initialization.
///
/// PORTS: `application.zig:3285-3339` `setGtkEnv` (`851cd4dc1` + `202fa8e39`
/// at `12752b2ac`). With the GtkGLArea gone GTK's GL context is no longer
/// Rustty's, so the version-gated `gl-disable-gles`/`gl-no-fractional`/
/// `vulkan-disable`/`gles-api`/`color-mgmt` flags are removed: `GDK_DEBUG` is
/// only `opengl` (when requested) and `GDK_DISABLE` only `vulkan`. Must run
/// before `adw::init()` (`application.zig:3286` asserts GTK is not yet
/// initialized).
///
/// PORT NOTE: the reference builds the comma-joined value into a fixed
/// `[1024]u8` and can return `error.WriteFailed` on overflow; rustty joins into
/// a growable `String`, so it cannot overflow and is infallible — the
/// reference's only error arm is therefore unreachable and folds away.
pub(super) fn set_gtk_env(config: &CoreConfig) {
    // `assert(gtk.isInitialized() == 0);`
    debug_assert!(
        !gtk4::is_initialized(),
        "setGtkEnv must run before GTK is initialized"
    );

    // const gdk_debug: struct {
    //     /// Output OpenGL debug information,
    //     /// `gtk-opengl-debug` dumps logs directly to stderr so both must be true
    //     /// to enable OpenGL debugging.
    //     opengl: bool = false,
    // } = .{
    //     .opengl = global.logging().stderr and config.@"gtk-opengl-debug",
    // };
    //
    // `state.logging.stderr` reads the process global state; unset (e.g. in a
    // test) reads as `false`.
    let stderr_logging = rustty_core::STATE.get().is_some_and(|s| s.logging.stderr);
    let gdk_debug_opengl = stderr_logging && config.gtk_opengl_debug;

    // const gdk_disable: struct {
    //     // Even though we don't use GTK's GL context anymore, there can still
    //     // occasionally be conflicts when Vulkan and OpenGL are used together.
    //     // Disabling Vulkan also saves hundreds of milliseconds of initialization
    //     // time on certain systems.
    //     vulkan: bool = true,
    // } = .{};
    let gdk_disable_vulkan = true;

    // Build + set GDK_DEBUG. Field order matches the reference struct's.
    {
        let mut parts: Vec<&str> = Vec::new();
        if gdk_debug_opengl {
            parts.push("opengl");
        }
        let value = parts.join(",");
        // log.warn("setting GDK_DEBUG={s}", .{value[0 .. value.len - 1]});
        log::warn!(target: LOG_SCOPE, "setting GDK_DEBUG={value}");
        if let Ok(v) = CString::new(value) {
            env_setenv(c"GDK_DEBUG", &v);
        }
    }

    // Build + set GDK_DISABLE.
    {
        let mut parts: Vec<&str> = Vec::new();
        if gdk_disable_vulkan {
            parts.push("vulkan");
        }
        let value = parts.join(",");
        // log.warn("setting GDK_DISABLE={s}", .{value[0 .. value.len - 1]});
        log::warn!(target: LOG_SCOPE, "setting GDK_DISABLE={value}");
        if let Ok(v) = CString::new(value) {
            env_setenv(c"GDK_DISABLE", &v);
        }
    }

    // Sync environ after altering system env
    //
    // (`global.syncEnviron()`: [`env_setenv`] re-syncs after each `setenv`, so
    // both variables are already synced here.)
}

/// PORTS: the three error arms of `application.zig:477` `run` (`!void`): the
/// `error.ContextAcquireFailed` (`:483`), `error.ApplicationRegisterFailed`
/// (`:514`), and the propagated `core_app.tick` error (`:552`, `try`). The Zig
/// error set is anonymous; the port names it so [`Application::run`] has a
/// concrete return type.
#[derive(Debug)]
pub enum RunError {
    /// `glib.MainContext.acquire(ctx) == 0` (`application.zig:483`) — another
    /// thread already owns the default main context.
    ContextAcquireFailed,
    /// `self.as(gio.Application).register(...) == 0` (`application.zig:502-514`) —
    /// GApplication registration failed (the warning is logged at the call site).
    ApplicationRegisterFailed,
    /// The inline `core_app.tick(rt_app)` returned an error (`application.zig:552`,
    /// `try`).
    Tick(rustty_core::app::Error),
}

impl std::fmt::Display for RunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RunError::ContextAcquireFailed => f.write_str("failed to acquire the main context"),
            RunError::ApplicationRegisterFailed => {
                f.write_str("failed to register the application")
            }
            RunError::Tick(err) => write!(f, "core app tick failed: {err}"),
        }
    }
}

impl std::error::Error for RunError {}

/// The `defer`-as-`Drop` guard for [`Application::run`]'s always-runs cleanup
/// (`application.zig:486-498`). Its `Drop` runs the reference's four deferred
/// steps in order — stop the quit timer, sync settings, drain the loop
/// non-blocking — and then the paired `MainContextAcquireGuard` (declared before
/// this guard in `run`, so it drops AFTER) releases the context. This split is
/// the faithful port of the nested `defer glib.MainContext.release(ctx)`
/// (`:497`), which the reference runs last.
pub(super) struct RunCleanup<'a> {
    pub(super) app: &'a Application,
    /// A second handle to the SAME default context (glib contexts are refcounted
    /// singletons) so the drain can iterate without borrowing `run`'s acquire
    /// guard — Rust cannot hold both the context and a guard borrowing it in one
    /// struct.
    pub(super) ctx: glib::MainContext,
}

impl Drop for RunCleanup<'_> {
    fn drop(&mut self) {
        // Ensure our timer source is removed (`application.zig:488`).
        self.app.imp().stop_quit_timer();

        // Sync any remaining settings (`application.zig:491`).
        gio::Settings::sync();

        // Clear out the event loop, don't block.
        //
        // (`application.zig:494`.)
        while self.ctx.iteration(false) {}

        // Release the context so something else can use it.
        //
        // The context release (`application.zig:497`) is the paired
        // `MainContextAcquireGuard`'s `Drop`, which runs after this guard's — see
        // the struct doc.
    }
}

/// PORTS: `application.zig:2953-2960` `findActiveWindow`. Finds the active
/// toplevel window (the one to parent a dialog on). The reference is a
/// `gtk.Window.listToplevels().findCustom` compare function returning 0 on the
/// active window; the behavioral port iterates the toplevels and returns the
/// first active `gtk.Window` (`window.isActive() != 0`).
pub(super) fn find_active_window() -> Option<gtk4::Window> {
    use gtk4::prelude::GtkWindowExt as _;
    gtk4::Window::list_toplevels()
        .into_iter()
        .filter_map(|w| w.downcast::<gtk4::Window>().ok())
        // Confusingly, `isActive` returns 1 when active,
        // but we want to return 0 to indicate equality.
        //
        // (The reference's `findCustom` compare function,
        // `application.zig:2956-2959`; here a plain `find`.)
        .find(|w| w.is_active())
}
