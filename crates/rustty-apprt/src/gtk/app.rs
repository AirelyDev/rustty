//! rustty-apprt::gtk::app
//!
//! PORTS: ghostty `src/apprt/gtk/App.zig` (96 loc) — the apprt entrypoint the
//! program initializes to start the application. It is a thin wrapper over the
//! [`Application`](super::class::application::Application) GObject: it OWNS the
//! sole created ref (`App.zig:32` `app: *Application`) and forwards the
//! `apprt.runtime.App` interface (`wakeup`/`performAction`/`performIpc`) plus the
//! lifecycle (`init`/`run`/`terminate`/`redrawInspector`) onto it.
//!
//! This is the NATIVE gtk4-rs backend's half of the `rustty_apprt_api` seam: a
//! sibling of [`crate::embedded::App`]/[`crate::none`] that `impl`s
//! [`rustty_apprt_api::App`] directly against the GObject application, NOT through
//! the libghostty C ABI (AD-1).
//!
//! # Invariant 1 — `MUST_DRAW_FROM_APP_THREAD`
//!
//! `App.zig:20-23` declares `pub const must_draw_from_app_thread = true` because
//! GTK's `GLArea` cannot draw off the app thread — the renderer must send a
//! `redraw_surface` message so `drawFrame` runs on the app thread instead
//! (`renderer/Thread.zig:22-30`). Rust models the reference's `@hasDecl` probe as
//! the [`rustty_apprt_api::App::MUST_DRAW_FROM_APP_THREAD`] trait const, defaulted
//! `false`; this backend overrides it to `true` (PREPORT_GTK §2 invariant 1).

use std::ffi::c_void;

use rustty_apprt_api::{ipc, Action, App as ApprtApp, ApprtError, CoreSurfaceHandle, Target};
use rustty_input::keyboard::Layout;

use super::class::application::{Application, RunError};
use super::surface::Surface;

/// PORTS: `App.zig:1-3` `const App = @This()` — the apprt entrypoint. Wraps the
/// process-wide [`Application`] GObject and OWNS the sole created ref.
pub struct App {
    /// PORTS: `App.zig:32` `app: *Application` — the OWNED row (holds the sole
    /// created ref). gtk4-rs's wrapper `Clone`/`Drop` manage the refcount, so the
    /// strong ref is released when this field drops (unless [`App::terminate`]
    /// force-`deinit`s it first).
    app: Application,
}

/// PORTS: the error union of `App.zig:34-47` `init` (`Allocator.Error!void`). The
/// reference's ONLY failure mode is early OOM ("can't be recovered from",
/// mirrored at `class/application.zig:240-241`), which in Rust aborts inside the
/// global allocator rather than surfacing an `AllocError`. So `init` is in fact
/// infallible; this uninhabited error type keeps the `Result<Self, _>` signature
/// faithful to the reference's `!` return while making the `Err` arm unreachable.
#[derive(Debug)]
pub enum InitError {}

impl std::fmt::Display for InitError {
    fn fmt(&self, _f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {}
    }
}

impl std::error::Error for InitError {}

impl App {
    /// PORTS: `App.zig:34-47` `init`. Creates the [`Application`] GObject that
    /// backs the app (`App.zig:43` `Application.new(self, core_app)`) and returns
    /// the wrapper that owns it.
    ///
    /// PORT NOTE (rt_app): the reference is an out-param `init(self: *App,
    /// core_app)` and passes `self` — the App's already-stable address — as the
    /// Application's `rt_app` backref (`App.zig:43`). Rust returns `Self` by
    /// value, so the App's final address is not known until the caller places it
    /// (the run loop / `main`, L5.5). `rt_app` is therefore bound in a later step,
    /// at the App's stable-address site (via [`Application::bind_core_app`] plus
    /// the `rt_app` pointer), exactly as the T10/T11 tests drive it manually. This
    /// keeps Task 11 the pure shim: `init` only constructs. `core_app` (a BACKREF
    /// libghostty-core owns) is a stable heap pointer already, so it is stored
    /// now. Both pointers stay `*mut c_void` (the Application GObject is
    /// non-generic and cannot name `App<A>`; see its field notes).
    pub fn init(core_app: *mut c_void) -> Result<Self, InitError> {
        // `const app: *Application = try .new(self, core_app);` (`App.zig:43`).
        // rt_app is null until the stable-address bind step runs (PORT NOTE).
        // `errdefer app.unref()` (`App.zig:44`) is discharged by Rust drop glue:
        // an early return past this point would drop `app` (unref) — but there is
        // no fallible step after construction, so the guard is vacuous here.
        let app = Application::new(std::ptr::null_mut(), core_app);
        Ok(App { app })
    }

    /// The owned [`Application`] GObject (`App.zig:32` `app: *Application`). The
    /// run leg and the first-light KAT reach the singleton through this to bind the
    /// backrefs and parent the initial window.
    #[must_use]
    pub fn application(&self) -> &Application {
        &self.app
    }

    /// PORTS: `App.zig:49-51` `run`. Enters the hand-rolled event loop
    /// (`class/application.zig:474-592`).
    pub fn run(&self) -> Result<(), RunError> {
        // Bind the `rt_app` BACKREF to THIS pinned `App` and monomorphize the
        // core-app thunks — the DEFERRED half of `init` (`App.zig:43` passes `self`
        // as `rt_app`; the reference binds `core_app` at comptime). This is the
        // App's stable-address site: `&self` is live for the whole `run`, and the
        // surface renderer/io threads that read `rt_app` are torn down (in
        // `terminate`/`destroy`, after `run` returns) before it drops. Without this
        // the run loop's `tick` thunk is unbound (the core never ticks) and every
        // `Surface::init` bails at the unbound-`rt_app` guard — the wiring the
        // first-light KAT exposes.
        self.app
            .bind_rt_app(std::ptr::from_ref(self).cast_mut().cast::<c_void>());
        self.app.bind_core_app::<App>();

        // `try self.app.run();` (`App.zig:50`).
        self.app.run()
    }

    /// PORTS: `App.zig:53-58` `terminate`. Force-deinitializes the app. We do NOT
    /// unref because other things tend to have a reference at this point, so this
    /// just forces the disposal now.
    pub fn terminate(&self) {
        // `self.app.deinit();` (`App.zig:57`).
        self.app.deinit();
    }

    /// PORTS: `App.zig:92-95` `redrawInspector`. Redraws the inspector for the
    /// given surface.
    pub fn redraw_inspector(&self, surface: &Surface) {
        // `surface.redrawInspector();` (`App.zig:94`).
        surface.redraw_inspector();
    }

    /// PORTS: `App.zig:80-90` `performIpc`. Sends the given IPC to a running
    /// Rustty. Returns `true` if the action was performed, `false` otherwise.
    ///
    /// PORT NOTE: the reference is a STATIC fn (no `self`) — it is called from a
    /// CLI process that is not the full Rustty app, so there is no live apprt App
    /// to use (`App.zig:76-79`). The seam routes the instance method
    /// [`ApprtApp::perform_ipc`] here for shape; both dispatch on the action.
    pub fn perform_ipc(
        target: ipc::Target<'_>,
        action: ipc::Action<'_>,
    ) -> Result<bool, ipc::Errors> {
        // `switch (action) { ... }` (`App.zig:86-89`).
        match action {
            // `.new_window => return try ipcNewWindow(alloc, target, value)`
            // (`App.zig:87`).
            ipc::Action::NewWindow(v) => ipc_new_window(target, v),
            // `.toggle_quick_terminal => return try ipcToggleQuickTerminal(alloc,
            // target)` (`App.zig:88`).
            ipc::Action::ToggleQuickTerminal => ipc_toggle_quick_terminal(target),
        }
    }
}

/// Resolve the `(bus_name, object_path)` for an IPC target and validate them.
/// PORTS: `ipc/DBus.zig` `init`'s target switch: `.detect` uses the build-info app
/// id / object path; `.class` derives the object path from the class name
/// (`"/" + class`, then `.`→`/` and `-`→`_`).
fn ipc_bus_target(target: &ipc::Target<'_>) -> Result<(String, String), ipc::Errors> {
    let (bus_name, object_path) = match target {
        ipc::Target::Detect => (
            super::build_info::APPLICATION_ID.to_string(),
            super::build_info::OBJECT_PATH.to_string(),
        ),
        ipc::Target::Class(class) => {
            let class = String::from_utf8_lossy(class).into_owned();
            let object_path = format!("/{class}").replace('.', "/").replace('-', "_");
            (class, object_path)
        }
    };
    if !gio::Application::id_is_valid(&bus_name) {
        eprintln!("D-Bus bus name is not valid: {bus_name}");
        return Err(ipc::Errors::IPCFailed);
    }
    if !glib::Variant::is_object_path(&object_path) {
        eprintln!("D-Bus object path is not valid: {object_path}");
        return Err(ipc::Errors::IPCFailed);
    }
    Ok((bus_name, object_path))
}

/// Send an `org.gtk.Actions.Activate` D-Bus call to the running instance.
/// PORTS: `ipc/DBus.zig` `send` — builds the `(sava{sv})` payload (action name,
/// an `av` of parameters, empty `a{sv}` platform data) and `callSync`s it.
fn ipc_send(
    target: &ipc::Target<'_>,
    action: &str,
    args: Option<&[&[u8]]>,
) -> Result<bool, ipc::Errors> {
    use gio::prelude::*;
    let (bus_name, object_path) = ipc_bus_target(target)?;

    let conn = match gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE) {
        Ok(c) => c,
        Err(err) => {
            eprintln!("Unable to establish connection to D-Bus session bus: {err}");
            return Err(ipc::Errors::IPCFailed);
        }
    };

    // parameters: `av`. For `new-window-command`, a single element that is the
    // `as` array of CLI arguments (`ipc/new_window.zig:33-59`).
    let params: Vec<glib::Variant> = match args {
        Some(args) => {
            let arg_strings: Vec<String> = args
                .iter()
                .map(|a| String::from_utf8_lossy(a).into_owned())
                .collect();
            let as_variant = arg_strings.to_variant();
            vec![glib::Variant::from_variant(&as_variant)]
        }
        None => vec![],
    };
    let params_v = glib::Variant::array_from_iter_with_type(glib::VariantTy::VARIANT, params);
    let platform_v = std::collections::HashMap::<String, glib::Variant>::new().to_variant();
    let payload = glib::Variant::tuple_from_iter([action.to_variant(), params_v, platform_v]);

    if let Err(err) = conn.call_sync(
        Some(&bus_name),
        &object_path,
        "org.gtk.Actions",
        "Activate",
        Some(&payload),
        None,
        gio::DBusCallFlags::NONE,
        -1,
        gio::Cancellable::NONE,
    ) {
        eprintln!("D-Bus method call returned an error err={err}");
        return Err(ipc::Errors::IPCFailed);
    }
    Ok(true)
}

/// PORTS: `ipc/new_window.zig` `newWindow` — the `new-window` (or
/// `new-window-command` when CLI args are present) D-Bus IPC.
fn ipc_new_window(target: ipc::Target<'_>, value: ipc::NewWindow<'_>) -> Result<bool, ipc::Errors> {
    let action = if value.arguments.is_none() {
        "new-window"
    } else {
        "new-window-command"
    };
    ipc_send(&target, action, value.arguments)
}

/// PORTS: `ipc/toggle_quick_terminal.zig` `toggleQuickTerminal` — the
/// `toggle-quick-terminal` D-Bus IPC.
fn ipc_toggle_quick_terminal(target: ipc::Target<'_>) -> Result<bool, ipc::Errors> {
    ipc_send(&target, "toggle-quick-terminal", None)
}

impl ApprtApp for App {
    /// PORTS: `App.zig:20-23` `pub const must_draw_from_app_thread = true`
    /// (invariant 1): GTK's `GLArea` cannot draw off the app thread, so the
    /// renderer sends a `redraw_surface` message instead of drawing inline. This
    /// overrides the seam's default `false`.
    const MUST_DRAW_FROM_APP_THREAD: bool = true;

    /// PORTS: `App.zig:14` `const Surface = @import("Surface.zig")` — the GTK
    /// per-surface backend (the placeholder [`Surface`], filled by L5.1 T18).
    type Surface = Surface;

    /// PORTS: `App.zig:65-72` `performAction`. Dispatches an apprt action onto a
    /// target via the backing [`Application`].
    ///
    /// Forwards EVERY action to the backing [`Application`]'s big `performAction`
    /// switch (`self.app.performAction(target, action, value)`, `App.zig:71` →
    /// `application.zig:668-799`), [`Application::perform_action`].
    ///
    /// The `.render` action is the **T16 render contract**'s app-thread leg: the
    /// core app, draining a `redraw_surface` mailbox message (invariant 1,
    /// `MUST_DRAW_FROM_APP_THREAD`), calls
    /// `rt_app.performAction(.{ .surface = core }, .render, {})`
    /// (`rustty_core::app::redrawSurface`, `app.rs:474-479`), which lands here,
    /// reaches the switch's `.render` arm → `action_render` → `Surface::redraw()` →
    /// `gl_area.queue_render()`. GTK then emits `render` on the app thread →
    /// `glareaRender` → `drawFrame(true)`.
    fn perform_action<C: CoreSurfaceHandle>(
        &self,
        target: Target<'_, C>,
        action: Action<'_>,
    ) -> Result<bool, ApprtError> {
        // `self.app.performAction(target, action, value)` (`App.zig:71`).
        self.app.perform_action(target, action)
    }

    /// PORTS: `App.zig:80-90` `performIpc` — routed to the static
    /// [`App::perform_ipc`] (the reference `performIpc` is itself static).
    fn perform_ipc(
        &self,
        target: ipc::Target<'_>,
        action: ipc::Action<'_>,
    ) -> Result<bool, ipc::Errors> {
        App::perform_ipc(target, action)
    }

    /// PORTS: `App.zig:60-63` `wakeup`. Wakes the event loop
    /// (`class/application.zig:1286-1289`).
    fn wakeup(&self) {
        // `self.app.wakeup();` (`App.zig:62`).
        self.app.wakeup();
    }

    /// The runtime's configuration — `Application::getConfig`
    /// (`class/application.zig:1208-1210`).
    ///
    /// PORT NOTE (borrow): the seam hands core a `&rustty_config::Config` read
    /// synchronously on the app thread (`rt_app.config...`, `seam.rs` note). The
    /// GTK config lives behind the shared [`Config`](super::class::config::Config)
    /// GObject, whose wrapped `rustty_config::Config` sits at a stable heap
    /// address for as long as the [`Application`] holds its strong ref. We read
    /// that address and tie the borrow to `&self`: sound because this `App` owns
    /// the `Application`, which holds the config ref and does not reassign it while
    /// the returned borrow is live — the same interior-shared-ref contract
    /// `embedded::App::config` uses.
    fn config(&self) -> &rustty_config::Config {
        let config = self
            .app
            .get_config()
            .expect("gtk::App::config called before the Application set its config");
        let ptr: *const rustty_config::Config = config.get();
        // `config` (a strong-ref clone) drops here; the `Application`'s own strong
        // ref keeps the pointee alive and pinned for `&self`'s lifetime.
        // SAFETY: see the method's PORT NOTE (borrow) — stable heap address held
        // alive by the owned `Application`, read on the app thread.
        unsafe { &*ptr }
    }

    /// PORTS: the `keyboardLayout` probe. GTK/Linux has no keyboard-layout
    /// detection (the reference only implements it on macOS — `embedded.zig:429`
    /// early-returns `.unknown` off macOS), so this reports [`Layout::Unknown`].
    fn keyboard_layout(&self) -> Layout {
        Layout::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The `gtk::App` shim's API surface exists with the intended signatures.
    /// Non-display: a `GhosttyApplication` is an `adw::Application` subclass whose
    /// construction touches GTK (`Application::new` even `debug_assert!`s GTK is
    /// not yet initialized and can run at most once per process), so — like the
    /// `Application::application_accessor_types` KAT — this compile-touches the
    /// method types as fn-pointer witnesses (a `refAllDecls` analogue) rather than
    /// constructing an instance, keeping it in the headless gate.
    #[test]
    fn app_shim_api_surface() {
        // Inherent lifecycle methods (`App.zig:34-58,92-95`).
        let _init: fn(*mut c_void) -> Result<App, InitError> = App::init;
        let _run: fn(&App) -> Result<(), RunError> = App::run;
        let _terminate: fn(&App) = App::terminate;
        let _redraw: fn(&App, &Surface) = App::redraw_inspector;

        // The static IPC dispatch (`App.zig:80-90`).
        let _ipc: fn(ipc::Target<'_>, ipc::Action<'_>) -> Result<bool, ipc::Errors> =
            App::perform_ipc;

        // The seam methods that are not generic (`perform_action` is generic over
        // the surface handle, so it is exercised by the `gtk_seam` trait-bound KAT
        // instead of a fn-pointer witness here).
        let _wakeup: fn(&App) = <App as ApprtApp>::wakeup;
        let _config: fn(&App) -> &rustty_config::Config = <App as ApprtApp>::config;
        let _layout: fn(&App) -> Layout = <App as ApprtApp>::keyboard_layout;
    }

    /// The GTK backend's `MUST_DRAW_FROM_APP_THREAD` is `true` (invariant 1,
    /// `App.zig:20-23`), overriding the seam's default `false`. Asserted in a
    /// `const` block (compile-time), matching the `gtk_seam` integration KAT's
    /// `const _: () = assert!(...)` — duplicated here as a unit witness so the
    /// invariant is checked alongside the shim itself.
    #[test]
    fn must_draw_from_app_thread_is_true() {
        const { assert!(<App as ApprtApp>::MUST_DRAW_FROM_APP_THREAD) };
    }

    /// With no running instance on the session bus, both IPC actions must fail
    /// with `IPCFailed` — the PORTED behavior (`ipc/DBus.zig:170-183`: ghostty's
    /// `.detect` target does NO NameHasOwner pre-check; `send()` calls
    /// `org.gtk.Actions.Activate` on the app id and maps any D-Bus error to
    /// `error.IPCFailed` after logging it). This replaced the stale L6
    /// placeholder pin (`Ok(false)`) that predated the real IPC port.
    ///
    /// Environment-sensitive by nature (it talks to the real session bus), so it
    /// SKIPS when a live rustty currently owns the bus name (the call would then
    /// legitimately succeed with `Ok(true)`).
    #[test]
    fn perform_ipc_no_instance_fails_with_ipc_failed() {
        use glib::prelude::ToVariant;
        // Skip if a live instance owns the name (dev instance running).
        if let Ok(conn) = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE) {
            let owned = conn
                .call_sync(
                    Some("org.freedesktop.DBus"),
                    "/org/freedesktop/DBus",
                    "org.freedesktop.DBus",
                    "NameHasOwner",
                    Some(&(crate::gtk::build_info::APPLICATION_ID,).to_variant()),
                    None,
                    gio::DBusCallFlags::NONE,
                    -1,
                    gio::Cancellable::NONE,
                )
                .ok()
                .and_then(|v| v.child_value(0).get::<bool>())
                .unwrap_or(false);
            if owned {
                eprintln!("skip: a live rustty owns the bus name — IPC would succeed");
                return;
            }
        }

        let new_window = ipc::Action::NewWindow(ipc::NewWindow { arguments: None });
        assert!(
            matches!(
                App::perform_ipc(ipc::Target::Detect, new_window),
                Err(ipc::Errors::IPCFailed)
            ),
            "new-window IPC with no running instance must map the D-Bus error to \
             IPCFailed (ipc/DBus.zig:182)"
        );
        assert!(
            matches!(
                App::perform_ipc(ipc::Target::Detect, ipc::Action::ToggleQuickTerminal),
                Err(ipc::Errors::IPCFailed)
            ),
            "toggle-quick-terminal IPC with no running instance must map the D-Bus \
             error to IPCFailed (ipc/DBus.zig:182)"
        );
    }
}
