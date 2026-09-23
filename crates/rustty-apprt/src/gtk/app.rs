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
        // V13 — INSTALL THE `rt_*` HOOKS. The reference resolves
        // `@hasDecl(apprt.runtime, "pre_exec")` / `"post_fork"` at COMPTIME
        // against whichever apprt the build selected (`Command.zig:139-155`,
        // `termio/Exec.zig:1034-1037`); THIS module is that apprt, and it is the
        // one that declares both. Rust cannot let `rustty-termio` name us (R4:
        // rustty-apprt -> rustty-core -> rustty-termio already exists, so the
        // reverse edge would cycle), so the selection is handed DOWN here
        // instead — see the seam PORT NOTE above
        // `rustty_termio::command::RtPreExecInfo`. `App::init` is the earliest
        // point at which "the GTK apprt is the runtime" is settled, and it
        // strictly precedes any surface (hence any `Command`). Installing the
        // same pair twice is a no-op, so a second `App::init` is fine.
        rustty_termio::command::install_rt_hooks(rustty_termio::command::RtHooks {
            pre_exec: Some(super::pre_exec::rt_pre_exec),
            post_fork: Some(super::post_fork::rt_post_fork),
        });

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
        // `switch (action) { ... }` (`App.zig:86-90`).
        match action {
            // `.new_window => return try ipcNewWindow(alloc, target, value)`
            // (`App.zig:87`).
            ipc::Action::NewWindow(v) => ipc_new_window(target, v),
            // `.new_tab => return try ipcNewTab(alloc, target, value)`
            // (`App.zig:88`).
            ipc::Action::NewTab(v) => ipc_new_tab(target, v),
            // `.toggle_quick_terminal => return try ipcToggleQuickTerminal(alloc,
            // target)` (`App.zig:89`).
            ipc::Action::ToggleQuickTerminal => ipc_toggle_quick_terminal(target),
        }
    }
}

/// Resolve the `(bus_name, object_path)` for an IPC target and validate them.
/// PORTS: `ipc/DBus.zig` `init`'s target switch: `.detect` uses the build-info app
/// id / object path; `.class` derives the object path from the class name
/// (`"/" + class`, then `.`→`/` and `-`→`_`).
///
/// Both names are BYTES, as the reference's `[:0]const u8` are, and the validity
/// checks and the diagnostics see the user's bytes. CORRECTION (task T-D, S59-F2):
/// the class was `String::from_utf8_lossy`'d first, so an invalid name was printed
/// with U+FFFD in place of the bytes the user passed to `--class`. `stderr` is the
/// reference's `stderr_writer` (`DBus.zig:37-39`); production passes the process
/// stderr.
fn ipc_bus_target(
    target: &ipc::Target<'_>,
    stderr: &mut impl std::io::Write,
) -> Result<(Vec<u8>, Vec<u8>), ipc::Errors> {
    let (bus_name, object_path): (Vec<u8>, Vec<u8>) = match target {
        ipc::Target::Detect => (
            super::build_info::APPLICATION_ID.as_bytes().to_vec(),
            super::build_info::OBJECT_PATH.as_bytes().to_vec(),
        ),
        ipc::Target::Class(class) => {
            // const object_path = try std.fmt.allocPrintSentinel(alloc, "/{s}", .{class}, 0);
            // std.mem.replaceScalar(u8, object_path, '.', '/');
            // std.mem.replaceScalar(u8, object_path, '-', '_');
            let mut object_path = Vec::with_capacity(class.len() + 1);
            object_path.push(b'/');
            object_path.extend(class.iter().map(|&b| match b {
                b'.' => b'/',
                b'-' => b'_',
                b => b,
            }));
            (class.to_vec(), object_path)
        }
    };
    // if (gio.Application.idIsValid(bus_name.ptr) == 0) {
    //     try stderr.print("D-Bus bus name is not valid: {s}\n", .{bus_name});
    if !c_predicate(&bus_name, gio::ffi::g_application_id_is_valid) {
        let _ = stderr.write_all(b"D-Bus bus name is not valid: ");
        let _ = stderr.write_all(&bus_name);
        let _ = stderr.write_all(b"\n");
        let _ = stderr.flush();
        return Err(ipc::Errors::IPCFailed);
    }
    // if (glib.Variant.isObjectPath(object_path.ptr) == 0) {
    //     try stderr.print("D-Bus object path is not valid: {s}\n", .{object_path});
    if !c_predicate(&object_path, glib::ffi::g_variant_is_object_path) {
        let _ = stderr.write_all(b"D-Bus object path is not valid: ");
        let _ = stderr.write_all(&object_path);
        let _ = stderr.write_all(b"\n");
        let _ = stderr.flush();
        return Err(ipc::Errors::IPCFailed);
    }
    Ok((bus_name, object_path))
}

/// Call a GLib `gboolean f(const gchar *)` predicate on `bytes` the way the
/// reference passes `slice.ptr`: C reads the bytes up to the first NUL.
fn c_predicate(
    bytes: &[u8],
    f: unsafe extern "C" fn(*const std::ffi::c_char) -> glib::ffi::gboolean,
) -> bool {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    let c = std::ffi::CString::new(&bytes[..end]).expect("no interior NUL before `end`");
    // SAFETY: `c` is a valid NUL-terminated string that outlives the call; both
    // predicates only read it.
    unsafe { f(c.as_ptr()) != glib::ffi::GFALSE }
}

/// Build an `as` of CLI arguments for `new-window-command` / `new-tab`.
///
/// PORTS: `ipc/new_window.zig:33-56` and `ipc/new_tab.zig:44-61`: each argument's
/// raw bytes become an `"s"` via `glib.Variant.newFromBytes(s, bytes, trusted =
/// true)`. For UTF-8 arguments the port builds the byte-identical `s` values.
///
/// PORT NOTE: reference defect (R13), `UPSTREAM-DEFECTS.md` UD170. `newFromBytes`
/// with `trusted = true` does not validate, so a non-UTF-8 argument (a Latin-1
/// file name after `+new-window -e`) builds an `s` that is not UTF-8, and GLib then
/// ABORTS the process when it serializes the D-Bus message:
/// `GLib-GIO:ERROR:../gio/gdbusmessage.c:2691:append_value_to_blob: assertion
/// failed: (g_utf8_validate (v, -1, &end) && (end == v + len))`, SIGABRT —
/// MEASURED on GLib 2.88.2 (Fedora, the VM) by a C probe that builds the payload
/// exactly as `newWindow` does and calls `g_dbus_message_to_blob`. The port does
/// not send such an argument. It fails the IPC with `IPCFailed` before the call,
/// which is the reference's own failure return for this function
/// (`apprt.ipc.Errors.IPCFailed`); the CLI caller documents that the apprt has
/// already "printed a more specific error message" for it (`cli/new_window.zig:
/// 249-252`), so one line naming the argument's bytes is printed. That line is
/// NEW text (the reference has none for this case). CORRECTION: the port used to
/// `String::from_utf8_lossy` each argument, which forwarded U+FFFD-mangled
/// arguments to the running instance and reported success (S59-F1).
fn ipc_arguments_variant(
    arguments: &[&[u8]],
    stderr: &mut impl std::io::Write,
) -> Result<glib::Variant, ipc::Errors> {
    use glib::prelude::*;
    let mut strings: Vec<glib::Variant> = Vec::with_capacity(arguments.len());
    for argument in arguments {
        let Ok(s) = std::str::from_utf8(argument) else {
            let _ = stderr.write_all(b"D-Bus IPC argument is not valid UTF-8: ");
            let _ = stderr.write_all(argument);
            let _ = stderr.write_all(b"\n");
            let _ = stderr.flush();
            return Err(ipc::Errors::IPCFailed);
        };
        strings.push(s.to_variant());
    }
    Ok(glib::Variant::array_from_iter_with_type(
        glib::VariantTy::STRING,
        strings,
    ))
}

/// Send an `org.gtk.Actions.Activate` D-Bus call to the running instance.
/// PORTS: `ipc/DBus.zig` `send` — builds the `(sava{sv})` payload (action name,
/// an `av` of parameters, empty `a{sv}` platform data) and `callSync`s it.
///
/// `param` builds the action's parameter. It runs AFTER the target checks and the
/// session-bus connection, where the reference's `newWindow`/`newTab` build their
/// parameter (after `DBus.init`), so the diagnostics come in the reference's order.
fn ipc_send(
    target: &ipc::Target<'_>,
    action: &str,
    param: impl FnOnce(&mut std::io::Stderr) -> Result<Option<glib::Variant>, ipc::Errors>,
) -> Result<bool, ipc::Errors> {
    use gio::prelude::*;
    let mut stderr = std::io::stderr();
    let (bus_name, object_path) = ipc_bus_target(target, &mut stderr)?;
    // Both passed the GLib validity checks above, which accept ASCII only.
    let bus_name = String::from_utf8(bus_name).expect("a valid bus name is ASCII");
    let object_path = String::from_utf8(object_path).expect("a valid object path is ASCII");

    let conn = match gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE) {
        Ok(c) => c,
        Err(err) => {
            eprintln!("Unable to establish connection to D-Bus session bus: {err}");
            return Err(ipc::Errors::IPCFailed);
        }
    };

    let param = param(&mut stderr)?;

    // parameters: `av`. Each ipc file builds its own parameter variant
    // (`DBus.zig` `addParameter`): `new-window-command` an `as` of CLI
    // arguments (`ipc/new_window.zig:33-59`), `new-tab` a `(tas)` tuple
    // (`ipc/new_tab.zig:31-67`).
    let params: Vec<glib::Variant> = match param {
        Some(p) => vec![glib::Variant::from_variant(&p)],
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
    // The `as` array of CLI arguments (`ipc/new_window.zig:33-59`); no
    // parameter at all for the bare `new-window` action.
    ipc_send(&target, action, |stderr| {
        value
            .arguments
            .map(|args| ipc_arguments_variant(args, stderr))
            .transpose()
    })
}

// Use a D-Bus method call to open a new tab on GTK.
// See: https://wiki.gnome.org/Projects/GLib/GApplication/DBusAPI
//
// `rustty +new-tab` is equivalent to the following command (on a release build):
//
// ```
// gdbus call --session --dest io.airely.rustty --object-path /io/airely/rustty --method org.gtk.Actions.Activate new-tab '[<@(tas) (0, [])>]' []
// ```
//
// `rustty +new-tab -e echo hello` would be equivalent to the following command (on a release build):
//
// ```
// gdbus call --session --dest io.airely.rustty --object-path /io/airely/rustty --method org.gtk.Actions.Activate new-tab '[<@(tas) (0, ["-e" "echo" "hello"])>]' []
// ```
/// PORTS: `ipc/new_tab.zig` `newTab` — the `new-tab` D-Bus IPC. The parameter
/// is always present: a `(tas)` tuple of the target surface ID and the CLI
/// argument array (empty when no arguments were specified).
fn ipc_new_tab(target: ipc::Target<'_>, value: ipc::NewTab<'_>) -> Result<bool, ipc::Errors> {
    ipc_send(&target, "new-tab", |stderr| {
        ipc_new_tab_param(value, stderr).map(Some)
    })?;

    // return true;
    Ok(true)
}

/// The `(tas)` parameter of `new-tab` (`ipc/new_tab.zig:31-67`).
fn ipc_new_tab_param(
    value: ipc::NewTab<'_>,
    stderr: &mut impl std::io::Write,
) -> Result<glib::Variant, ipc::Errors> {
    use glib::prelude::*;
    // {
    //     // Add the target surface ID to the parameter.
    //     const t = glib.Variant.newUint64(value.surface_id);
    //     parameter.addValue(t);
    // }
    // {
    //     // If any arguments were specified on the command line, this value is an
    //     // array of strings that contain the arguments. They will be sent to the
    //     // main Rustty instance and interpreted as CLI arguments.
    //     ...
    // }
    let args = ipc_arguments_variant(value.arguments.unwrap_or(&[]), stderr)?;
    // `(tas)`: the VariantBuilder init/addValue/end sequence collapses to a
    // two-element tuple (u64 -> `t`, the `as` above).
    Ok(glib::Variant::tuple_from_iter([
        value.surface_id.to_variant(),
        args,
    ]))
}

/// PORTS: `ipc/toggle_quick_terminal.zig` `toggleQuickTerminal` — the
/// `toggle-quick-terminal` D-Bus IPC.
fn ipc_toggle_quick_terminal(target: ipc::Target<'_>) -> Result<bool, ipc::Errors> {
    ipc_send(&target, "toggle-quick-terminal", |_| Ok(None))
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

    /// The reference's own construction of an `as` of CLI arguments
    /// (`ipc/new_window.zig:45-54`, `ipc/new_tab.zig:44-61`): one
    /// `glib.Variant.newFromBytes("s", Bytes(arg ++ NUL), trusted = true)` per
    /// argument, added to an `as` builder. Built through the same C calls so the
    /// port's variant can be compared against it byte for byte.
    fn reference_as(arguments: &[&[u8]]) -> glib::Variant {
        use glib::translate::{FromGlibPtrNone, ToGlibPtr};
        unsafe {
            let as_ty = glib::VariantTy::new("as").unwrap();
            let builder = glib::ffi::g_variant_builder_new(as_ty.to_glib_none().0);
            for argument in arguments {
                let mut with_nul = argument.to_vec();
                with_nul.push(0);
                let bytes = glib::ffi::g_bytes_new(with_nul.as_ptr().cast(), with_nul.len());
                let s = glib::ffi::g_variant_new_from_bytes(
                    glib::VariantTy::STRING.to_glib_none().0,
                    bytes,
                    glib::ffi::GTRUE,
                );
                glib::ffi::g_bytes_unref(bytes);
                glib::ffi::g_variant_builder_add_value(builder, s);
            }
            let v = glib::ffi::g_variant_builder_end(builder);
            glib::ffi::g_variant_builder_unref(builder);
            glib::Variant::from_glib_none(v)
        }
    }

    /// S59-F1: for UTF-8 arguments the port's `as` is the reference's, byte for
    /// byte (type, serialized data, and `g_variant_equal`), including non-ASCII
    /// UTF-8 and an empty argument. Display-free, no D-Bus.
    #[test]
    fn ipc_arguments_variant_matches_the_reference_bytes() {
        let args: &[&[u8]] = &[b"-e", b"echo", "h\u{e9}llo".as_bytes(), b""];
        let mut stderr = Vec::new();
        let Ok(port) = ipc_arguments_variant(args, &mut stderr) else {
            panic!("UTF-8 arguments must build");
        };
        let reference = reference_as(args);
        assert_eq!(port.type_().as_str(), "as");
        assert_eq!(port, reference);
        assert_eq!(&*port.data_as_bytes(), &*reference.data_as_bytes());
        assert!(stderr.is_empty());

        // `new-tab`'s `(tas)`, with and without arguments.
        for arguments in [Some(args), None] {
            let Ok(param) = ipc_new_tab_param(
                ipc::NewTab {
                    surface_id: 42,
                    arguments,
                },
                &mut stderr,
            ) else {
                panic!("UTF-8 arguments must build");
            };
            assert_eq!(param.type_().as_str(), "(tas)");
            assert_eq!(param.child_value(0).get::<u64>(), Some(42));
            assert_eq!(param.child_value(1), reference_as(arguments.unwrap_or(&[])));
        }
    }

    /// S59-F1 / UD170 (R13): a non-UTF-8 argument fails the IPC with `IPCFailed`
    /// and a diagnostic carrying the argument's own bytes, instead of being
    /// U+FFFD-mangled and sent (the port's old behaviour) or reaching GLib's
    /// `append_value_to_blob` UTF-8 assertion, which aborts (the reference's,
    /// measured on GLib 2.88.2). Both callers (`new-window-command`, `new-tab`) go
    /// through this function.
    #[test]
    fn ipc_arguments_variant_rejects_non_utf8() {
        let args: &[&[u8]] = &[b"-e", b"caf\xe9"];
        let mut stderr = Vec::new();
        let result = ipc_arguments_variant(args, &mut stderr);
        assert!(
            matches!(result, Err(ipc::Errors::IPCFailed)),
            "a non-UTF-8 argument must fail the IPC"
        );
        assert_eq!(stderr, b"D-Bus IPC argument is not valid UTF-8: caf\xe9\n");

        let mut stderr = Vec::new();
        let result = ipc_new_tab_param(
            ipc::NewTab {
                surface_id: 0,
                arguments: Some(args),
            },
            &mut stderr,
        );
        assert!(matches!(result, Err(ipc::Errors::IPCFailed)));
        assert_eq!(stderr, b"D-Bus IPC argument is not valid UTF-8: caf\xe9\n");
    }

    /// S59-F2: `--class` is validated and printed as the user's BYTES
    /// (`ipc/DBus.zig:42-66`). An invalid class names its raw bytes in the
    /// diagnostic; a valid one yields the reference's derived object path.
    #[test]
    fn ipc_bus_target_uses_the_class_bytes() {
        let mut stderr = Vec::new();
        let result = ipc_bus_target(&ipc::Target::Class(b"caf\xe9.app"), &mut stderr);
        assert!(matches!(result, Err(ipc::Errors::IPCFailed)));
        assert_eq!(stderr, b"D-Bus bus name is not valid: caf\xe9.app\n");

        let mut stderr = Vec::new();
        let Ok((bus_name, object_path)) =
            ipc_bus_target(&ipc::Target::Class(b"com.example.my-app"), &mut stderr)
        else {
            panic!("a valid class must resolve");
        };
        assert_eq!(bus_name, b"com.example.my-app");
        assert_eq!(object_path, b"/com/example/my_app");
        assert!(stderr.is_empty());
    }

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
