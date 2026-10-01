//! Tests for `class::application` (the GTK `Application` subclass).

use super::gio_actions::bind_failed_label;
use super::*;

/// The `config` property getter returns `None` on a freshly-built skeleton
/// instance (before `new` sets the config). Non-display: building the bare
/// subclass and reading a nullable property needs no `gtk_init`/display — but a
/// `GhosttyApplication` is an `adw::Application` (a `GApplication`), whose
/// construction gtk4-rs guards with `assert_initialized_main_thread!` on some
/// paths; to stay in the headless gate we do NOT instantiate here. Instead this
/// compile-touches the accessor types (a `refAllDecls` analogue) so the skeleton
/// API surface is asserted without a display.
#[test]
fn application_accessor_types() {
    // Type-level witnesses that the accessors exist with the intended
    // signatures; no instance is constructed (headless gate).
    let _default: fn() -> Application = Application::default;
    let _alloc: fn(&Application) -> allocator_api2::alloc::Global = Application::allocator;
    let _get_config: fn(&Application) -> Option<Config> = Application::get_config;
}

/// N311: `{}` of `reloadConfig`'s error is the Zig error name, the text the
/// reference's caller line prints (`application.zig:1602-1606`). Pure: no
/// display.
#[test]
fn reload_config_error_prints_the_zig_error_name() {
    use super::action::ReloadConfigError;
    assert_eq!(
        ReloadConfigError::Load(rustty_config::config_struct::LoadError::FileNotFound).to_string(),
        "error.FileNotFound"
    );
    assert_eq!(
        ReloadConfigError::Load(rustty_config::config_struct::LoadError::OutOfMemory).to_string(),
        "error.OutOfMemory"
    );
    assert_eq!(
        ReloadConfigError::Core(rustty_core::app::Error::Surface(
            rustty_core::surface::Error::InitChangeConfig(
                rustty_renderer::message::InitChangeConfigError
            )
        ))
        .to_string(),
        "error.OutOfMemory"
    );
}

/// N311: a failed reload logs ONCE, at the caller, with the error name —
/// `application.zig:1792-1794` `log.warn("error reloading config err={}",
/// .{err})` for the `app.reload-config` action — and `reloadConfig` itself
/// logs nothing (it `try`s `Config.load` and `updateConfig`,
/// `:2945-2982`). The app-target `updateConfig` is made to fail with the
/// core's `error.OutOfMemory` through the bound thunk.
#[test]
#[ignore = "requires a display; run via xvfb-run -a cargo test -p rustty-apprt --features gtk -- --ignored"]
fn reload_config_failure_logs_once_at_the_caller_with_the_error_name() {
    crate::gtk::init_for_display_test(
        "reload_config_failure_logs_once_at_the_caller_with_the_error_name",
    );
    let _ = libadwaita::init();

    let mut core_app: Box<CoreApp<Double>> = CoreApp::<Double>::create(Global).expect("core app");
    let core_ptr: *mut CoreApp<Double> = &mut *core_app;
    let app = build_test_app(core_ptr.cast());

    let reached = std::rc::Rc::new(std::cell::Cell::new(0usize));
    let reached_in = reached.clone();
    *app.imp().update_config_thunk.borrow_mut() = Some(Box::new(move |_config| {
        reached_in.set(reached_in.get() + 1);
        Err(rustty_core::app::Error::Surface(
            rustty_core::surface::Error::InitChangeConfig(
                rustty_renderer::message::InitChangeConfigError,
            ),
        ))
    }));

    // `reloadConfig` alone: the error comes back, nothing is logged.
    let cap = crate::log_capture::start();
    let result = action::reload_config(
        &app,
        Target::<AppTargetHandle>::App,
        rustty_apprt_api::action::ReloadConfig { soft: true },
    );
    let inner = cap.messages();
    drop(cap);
    assert_eq!(
        reached.get(),
        1,
        "the app-target updateConfig must have run"
    );
    assert_eq!(
        result.map_err(|err| err.to_string()),
        Err("error.OutOfMemory".to_string())
    );
    assert_eq!(
        Vec::<(log::Level, String, String)>::new(),
        inner,
        "reloadConfig propagates its error with `try` and logs nothing"
    );

    // The `app.reload-config` action handler: exactly one line, with the name.
    let cap = crate::log_capture::start();
    let gaction = gio::SimpleAction::new("reload-config", None);
    action_reload_config(&gaction, None, &app);
    // The hard reload's `Config.load` may log under its own scopes (which
    // files it read); only this file's scope is under test.
    let messages: Vec<_> = cap
        .messages()
        .into_iter()
        .filter(|(_, target, _)| target == LOG_SCOPE)
        .collect();
    drop(cap);
    assert_eq!(reached.get(), 2, "the action must reach updateConfig");
    assert_eq!(
        vec![(
            log::Level::Warn,
            LOG_SCOPE.to_string(),
            "error reloading config err=error.OutOfMemory".to_string()
        )],
        messages
    );
}

// ---- display-dependent lifecycle tests --------------------------------
//
// These construct a real `GhosttyApplication` and drive its
// startup/activate/dispose vfuncs, so they need an initialized GTK + a
// display. Marked `#[ignore]`; run via
//   xvfb-run -a cargo test -p rustty-apprt --features gtk -- --ignored
// GTK binds its "main thread" to whichever thread first calls `gtk::init`;
// the cargo harness runs each test on a worker thread, so at most ONE GTK
// test's body executes per multi-threaded run. Run each individually to
// exercise its body.
//
// CORRECTION (task 20): this used to say "the others `gtk::init().err()` and
// skip — the established pattern, see `ext/actions.rs`
// `adding_actions_to_an_object`". A skip that reports `1 passed` is not a
// pattern to follow: every one of these now calls
// `crate::gtk::init_for_display_test`, so the others FAIL, by name.

/// PORTS: `application.zig:201,564,1390,2445-2451` `systemd_ready`
/// (`4eb308831`, P7-W8 T3). A reload triggered before startup completes —
/// e.g. syncing the color scheme during startup — must send NO systemd
/// notify. With `Type=notify-reload`, systemd 262+ refuses to start a
/// unit whose reload signal has no handler if it observes
/// `RELOADING=1`/`READY=1` that early (upstream's motivating bug,
/// #14398). Drives a real [`action::reload_config`] through a fake
/// `NOTIFY_SOCKET` (a bound `AF_UNIX` `SOCK_DGRAM`, exactly what
/// `rustty_os::systemd::notify::send` connects to) and asserts the
/// socket receives nothing. `register()` runs the `startup` vfunc
/// synchronously but never enters the hand-rolled run loop, so
/// `systemd_ready` is exactly as it is "before startup completes": still
/// false, no `READY=1` sent yet.
///
/// SAFETY: this test mutates the process-global `NOTIFY_SOCKET`
/// environment variable. Like every GTK test in this module it must run
/// alone (`xvfb-run -a cargo test -p rustty-apprt --features gtk --
/// --ignored --exact reload_config_before_startup_completes_sends_no_systemd_notify`;
/// see the module doc above), so no concurrent test observes the
/// mutation.
#[test]
#[ignore = "requires a display; run via xvfb-run -a cargo test -p rustty-apprt --features gtk -- --ignored --exact"]
fn reload_config_before_startup_completes_sends_no_systemd_notify() {
    use gio::prelude::ApplicationExt as _;
    use std::io::ErrorKind;
    use std::os::unix::net::UnixDatagram;

    crate::gtk::init_for_display_test(
        "reload_config_before_startup_completes_sends_no_systemd_notify",
    );
    let _ = libadwaita::init();

    // A fake systemd notify socket: a bound AF_UNIX SOCK_DGRAM, exactly
    // what `sd_notify(3)` (and `rustty_os::systemd::notify::send`)
    // connects to.
    let dir = std::env::temp_dir().join(format!("rustty-tg-systemd-ready-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let sock_path = dir.join("notify.sock");
    let _ = std::fs::remove_file(&sock_path);
    let socket = UnixDatagram::bind(&sock_path).expect("bind fake notify socket");
    socket
        .set_read_timeout(Some(std::time::Duration::from_millis(200)))
        .expect("set read timeout");

    // SAFETY: see the doc comment above — this GTK test runs alone.
    unsafe { std::env::set_var("NOTIFY_SOCKET", &sock_path) };

    let mut core_app: Box<CoreApp<Double>> = CoreApp::<Double>::create(Global).expect("core app");
    let core_ptr: *mut CoreApp<Double> = &mut *core_app;
    let app = build_test_app(core_ptr.cast());

    // `register` runs `startup` synchronously (installs signals, the
    // style manager, actions, ...) but the run loop — and its
    // `systemd.notify.ready(); systemd_ready = true;` — never executes.
    app.register(gio::Cancellable::NONE).expect("register");
    assert!(
        !app.imp().systemd_ready.get(),
        "setup: systemd_ready must still be false before the run loop's \
         first notify::ready()"
    );

    let result = action::reload_config(
        &app,
        Target::<AppTargetHandle>::App,
        rustty_apprt_api::action::ReloadConfig { soft: true },
    );
    assert!(result.is_ok(), "reload_config: {result:?}");

    let mut buf = [0u8; 256];
    let outcome = socket.recv(&mut buf);
    // SAFETY: see the doc comment above — this GTK test runs alone.
    unsafe { std::env::remove_var("NOTIFY_SOCKET") };
    let _ = std::fs::remove_file(&sock_path);
    let _ = std::fs::remove_dir_all(&dir);
    match outcome {
        Ok(n) => panic!(
            "reload_config before startup completed must send NO systemd \
             notify, but the fake socket received {:?}",
            String::from_utf8_lossy(&buf[..n])
        ),
        Err(e) if e.kind() == ErrorKind::WouldBlock || e.kind() == ErrorKind::TimedOut => {}
        Err(e) => panic!("unexpected recv error: {e}"),
    }
}

/// S49-F3: `loadCustomCss` (`application.zig:1119-1199`) against real files.
///
/// - A directory in `gtk-custom-css` opens and then fails its first read
///   (`error.IsDir`, measured with the reference's own open + read code on the
///   VM). That is not `FileTooBig`, so the WHOLE load stops: a stylesheet
///   listed after it is not loaded, one listed before it stays loaded, and the
///   caller (`propConfig`, `:1309-1314`) logs `failed to load custom CSS, no
///   custom CSS applied, err=...`.
/// - The provider is fed the file's BYTES (`loadFromBytes`): a stylesheet
///   whose comment holds one Latin-1 byte parses to nothing in GTK (measured,
///   GTK 4.22.4), where its U+FFFD-substituted text would parse to its rule.
#[test]
#[ignore = "requires a display; run via cargo test -p rustty-apprt --features gtk,x11,wayland --lib load_custom_css -- --ignored"]
fn load_custom_css_stops_on_a_read_error_and_loads_raw_bytes() {
    use rustty_config::path::Path as CfgPath;

    crate::gtk::init_for_display_test("load_custom_css_stops_on_a_read_error_and_loads_raw_bytes");
    let _ = libadwaita::init();

    let dir = std::env::temp_dir().join(format!("rustty-s49f3-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("a-directory.css")).unwrap();
    let ok_css = dir.join("ok.css");
    std::fs::write(&ok_css, b"label { color: red; }").unwrap();
    let latin1_css = dir.join("latin1.css");
    std::fs::write(&latin1_css, b"/* caf\xe9 */ label { color: red; }").unwrap();
    let cfg_path = |p: &std::path::Path| {
        use std::os::unix::ffi::OsStrExt;
        CfgPath::Required(p.as_os_str().as_bytes().to_vec())
    };
    let provider_text = |provider: &gtk4::CssProvider| -> Vec<u8> {
        use glib::translate::ToGlibPtr;
        // SAFETY: GTK returns a newly allocated NUL-terminated string we free.
        unsafe {
            let p = gtk4::ffi::gtk_css_provider_to_string(provider.to_glib_none().0);
            let bytes = std::ffi::CStr::from_ptr(p).to_bytes().to_vec();
            glib::ffi::g_free(p.cast());
            bytes
        }
    };

    let app = build_test_app(std::ptr::null_mut());
    let set_css = |paths: Vec<CfgPath>| {
        let mut core = rustty_config::Config::default_config();
        core.gtk_custom_css.value = paths;
        app.imp().config.replace(Some(Config::new(&core)));
    };

    // [dir, ok]: the directory's read error stops the load before ok.css.
    set_css(vec![
        cfg_path(&dir.join("a-directory.css")),
        cfg_path(&ok_css),
    ]);
    let err = app
        .imp()
        .load_custom_css()
        .expect_err("a directory's read error must abort the load");
    assert_eq!(err.kind(), std::io::ErrorKind::IsADirectory);
    assert_eq!(
        app.imp().custom_css_providers.borrow().len(),
        0,
        "nothing after the failing entry is loaded"
    );

    // [ok, dir]: the entry BEFORE the failure stays loaded (positive: the loop
    // ran and loaded a provider before it stopped).
    set_css(vec![
        cfg_path(&ok_css),
        cfg_path(&dir.join("a-directory.css")),
    ]);
    assert!(app.imp().load_custom_css().is_err());
    assert_eq!(app.imp().custom_css_providers.borrow().len(), 1);

    // The caller logs the reference's warning with the error.
    set_css(vec![cfg_path(&dir.join("a-directory.css"))]);
    let cap = crate::log_capture::start();
    app.imp().prop_config(&app);
    let messages = cap.messages();
    drop(cap);
    let failed: Vec<_> = messages
        .iter()
        .filter(|(_, _, m)| m.starts_with("failed to load custom CSS, no custom CSS applied, err="))
        .collect();
    assert_eq!(failed.len(), 1, "one caller warning; got {messages:?}");
    assert_eq!(failed[0].0, log::Level::Warn);
    assert_eq!(failed[0].1, LOG_SCOPE);

    // Raw bytes: the Latin-1 comment makes GTK parse nothing; the control
    // (pure ASCII, same rule) parses to its rule.
    set_css(vec![cfg_path(&latin1_css), cfg_path(&ok_css)]);
    app.imp().load_custom_css().expect("both files load");
    let providers = app.imp().custom_css_providers.borrow().clone();
    assert_eq!(providers.len(), 2);
    assert_eq!(
        provider_text(&providers[0]),
        b"",
        "GTK is handed the raw bytes (a lossy U+FFFD copy would parse to the rule)"
    );
    assert!(
        provider_text(&providers[1]).starts_with(b"label {"),
        "control: the ASCII stylesheet parses to its rule"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

// `parse_overrides_trims_ascii_whitespace_only` (S50-F2) moved to
// [`super::overrides`]'s own test module, 9fc8d9ebd — it exercises
// [`super::overrides::parse_overrides`] directly, which now lives there.

/// S50-F1: `gotoWindow`'s walk, pinned on plain integers so it needs no display.
/// Each case lists the nodes `gotoWindowMaybe` was asked about, in order. The
/// "accept nothing" predicate makes the whole visiting order observable.
#[test]
fn goto_window_walk_matches_the_reference_list_walk() {
    use rustty_apprt_api::action::GotoWindow;

    fn visits(n: usize, active: Option<usize>, dir: GotoWindow) -> Vec<usize> {
        let list: Vec<usize> = (0..n).collect();
        let mut seen = Vec::new();
        let found = action::goto_window_walk(
            &list,
            dir,
            |&i| Some(i) == active,
            |&i| {
                seen.push(i);
                false
            },
        );
        assert!(!found);
        seen
    }

    // Anchored at the active node, which is VISITED first (gotoWindowMaybe is
    // what rejects it), then to the end, then from the head up to the anchor.
    assert_eq!(visits(4, Some(2), GotoWindow::Next), [2, 3, 0, 1]);
    // Backwards to the head, then from the tail down to just past the anchor.
    assert_eq!(visits(4, Some(2), GotoWindow::Previous), [2, 1, 0, 3]);
    // Nothing active: anchored at the head.
    assert_eq!(visits(3, None, GotoWindow::Next), [0, 1, 2]);
    assert_eq!(visits(3, None, GotoWindow::Previous), [0, 2, 1]);
    // Empty list, in BOTH directions: `.previous` is the arm whose wrap-around
    // dereferences the null list in the reference (UPSTREAM-DEFECTS UD4), and the
    // one whose slicing would panic here without the empty-list guard.
    assert_eq!(visits(0, None, GotoWindow::Next), Vec::<usize>::new());
    assert_eq!(visits(0, None, GotoWindow::Previous), Vec::<usize>::new());

    // The audit's case (1): one window, nothing active. The reference presents
    // it; the port used to skip the anchor and return false.
    let list = [0usize];
    assert!(action::goto_window_walk(
        &list,
        GotoWindow::Next,
        |_| false,
        |_| true
    ));
}

use allocator_api2::alloc::Global;
use rustty_apprt_api as apprt;
use rustty_core::app::{App as CoreApp, Message};
use rustty_core::surface::Surface as CoreSurface;

/// A minimal apprt `App` double: enough to parametrize `CoreApp<Double>` so a
/// real core-app mailbox exists for the `activate` push to land in. Mirrors
/// `rustty-core`'s in-crate `TestApp`/`TestSurface` (app.rs).
struct Double {
    config: rustty_config::Config,
}

impl apprt::App for Double {
    type Surface = DoubleSurface;

    fn perform_action<C: apprt::CoreSurfaceHandle>(
        &self,
        _target: apprt::Target<'_, C>,
        _action: apprt::Action<'_>,
    ) -> Result<bool, apprt::ApprtError> {
        Ok(true)
    }

    fn perform_ipc(
        &self,
        _target: apprt::ipc::Target<'_>,
        _action: apprt::ipc::Action<'_>,
    ) -> Result<bool, apprt::ipc::Errors> {
        unimplemented!("test double: not exercised")
    }

    fn wakeup(&self) {}

    fn config(&self) -> &rustty_config::Config {
        &self.config
    }

    fn keyboard_layout(&self) -> rustty_input::keyboard::Layout {
        rustty_input::keyboard::Layout::Unknown
    }
}

struct DoubleSurface;

impl apprt::Surface for DoubleSurface {
    type App = Double;
    type Core = CoreSurface<Double>;

    fn core(&self) -> *mut Self::Core {
        std::ptr::null_mut()
    }
    fn rt_app(&self) -> *mut Double {
        std::ptr::null_mut()
    }
    unsafe fn close(_self: *const Self, _process_alive: bool) {}
    fn deinit(&mut self) {}
    fn get_content_scale(&self) -> Result<apprt::ContentScale, apprt::ApprtError> {
        unimplemented!("test double: not exercised")
    }
    fn get_size(&self) -> Result<apprt::SurfaceSize, apprt::ApprtError> {
        unimplemented!("test double: not exercised")
    }
    fn get_title(&self) -> Option<&std::ffi::CStr> {
        None
    }
    fn supports_clipboard(&self, _clipboard_type: apprt::Clipboard) -> bool {
        unimplemented!("test double: not exercised")
    }
    fn clipboard_request(
        &self,
        _clipboard_type: apprt::Clipboard,
        _state: apprt::ClipboardRequest,
    ) -> Result<apprt::ClipboardReadResult, apprt::ApprtError> {
        unimplemented!("test double: not exercised")
    }
    fn set_clipboard(
        &self,
        _clipboard_type: apprt::Clipboard,
        _contents: &[apprt::ClipboardContent<'_>],
        _confirm: bool,
    ) -> Result<(), apprt::ApprtError> {
        unimplemented!("test double: not exercised")
    }
    fn get_cursor_pos(&self) -> Result<apprt::CursorPos, apprt::ApprtError> {
        unimplemented!("test double: not exercised")
    }
    fn default_termio_env(&self) -> Result<rustty_global::EnvMap, apprt::ApprtError> {
        unimplemented!("test double: not exercised")
    }
}

/// Build a `GhosttyApplication` for a lifecycle test WITHOUT going through
/// [`Application::new`]: `new` calls `set_gtk_env`, which `debug_assert!`s GTK
/// is not yet initialized (`application.zig:2850`) and so can run at most once
/// per process — unusable across multiple constructing tests. This builds the
/// subclass directly (the same `glib::Object::builder` shape `new` uses at
/// `:1178-1182`, minus the resource pin) and fills the two private fields the
/// startup/activate helpers read (`config`, `core_app`). Flags force
/// `NON_UNIQUE` so `register` stays local (no session-bus dependency).
fn build_test_app(core_app: *mut std::ffi::c_void) -> Application {
    let app: Application = glib::Object::builder()
        .property("application-id", "io.airely.rustty.LifecycleTest")
        .property("flags", gio::ApplicationFlags::NON_UNIQUE)
        .build();
    let imp = app.imp();
    imp.config
        .replace(Some(Config::new(&rustty_config::Config::default_config())));
    imp.core_app.set(NonNull::new(core_app));
    app
}

/// PORTS: `application.zig:1294-1330` `startup` + `:1411-1429`
/// `startupActionMap`. Registering the app runs the startup vfunc, which must
/// install us as the default `GApplication` (`:1303`) and register the seven
/// application actions (`:1418-1426`).
#[test]
#[ignore = "requires a display; run via xvfb-run -a cargo test -p rustty-apprt --features gtk -- --ignored"]
fn application_startup_sets_default_and_actions() {
    use gio::prelude::{ActionGroupExt, ApplicationExt};

    // CORRECTION (task 20): this was `if gtk4::init().is_err() { return; }`,
    // which reports `1 passed` for a test that never ran when there is no display.
    crate::gtk::init_for_display_test("application_startup_sets_default_and_actions");
    let _ = libadwaita::init();

    let app = build_test_app(std::ptr::null_mut());

    // `register` emits `startup` synchronously on the primary instance.
    app.register(gio::Cancellable::NONE).expect("register");

    // We are the default application (`:1303`).
    let default = gio::Application::default().expect("a default GApplication exists");
    assert_eq!(
        default.downcast::<Application>().ok().as_ref(),
        Some(&app),
        "startup must set our application as the process-global default"
    );

    // All nine actions are present (`:1483-1492` — B5's `new-tab` and B7's
    // `ring-bell` joined the original seven).
    let names: Vec<String> = app.list_actions().into_iter().map(Into::into).collect();
    for expected in [
        "new-window",
        "new-window-command",
        "new-tab",
        "open-config",
        "present-surface",
        "quit",
        "reload-config",
        "toggle-quick-terminal",
        "ring-bell",
    ] {
        assert!(
            names.iter().any(|n| n == expected),
            "startup must register the '{expected}' action; got {names:?}"
        );
    }
}

/// PORTS: `application.zig:1655-1663` `globalShortcutTrigger`. A `trigger`
/// from the GlobalShortcuts portal performs the action on the core app via
/// `performAllAction`. Requires a display only because `build_test_app`
/// constructs a real `GtkApplication`.
#[test]
#[ignore = "requires a display; run via xvfb-run -a cargo test -p rustty-apprt --features gtk -- --ignored"]
fn application_global_shortcut_trigger_performs_action() {
    // F2: NOT `if gtk4::init().is_err() { return; }`. This is the test whose
    // own vacuity taught this stage the lesson; a silent skip here is
    // indistinguishable from a pass, and a reviewer hit exactly that.
    crate::gtk::init_for_display_test("application_global_shortcut_trigger_performs_action");
    let _ = libadwaita::init();

    let mut core_app: Box<CoreApp<Double>> = CoreApp::<Double>::create(Global).expect("core app");
    let core_ptr: *mut CoreApp<Double> = &mut *core_app;

    let app = build_test_app(core_ptr.cast());
    assert!(
        app.imp().perform_all_action_thunk.borrow().is_none(),
        "unbound core app installs no dispatch thunk"
    );

    // task-14d-3: the `rt_app` backref MUST be bound, and BEFORE
    // `bind_core_app`. The thunk opens with
    // `let Some(rt_app) = rt_app else { return; }`, and `bind_core_app`
    // SNAPSHOTS `imp.rt_app` into the closure at install time
    // (`let rt_app = self.imp().rt_app.get()...` — application.rs:2913), so
    // binding it afterwards has no effect on an already-installed thunk.
    // With it unbound the thunk returns BEFORE `performAllAction` and this
    // test never reaches the dispatch it claims to prove. Caught by running
    // the S50-F4 negative control on Linux: re-adding the removed success
    // emission left the test GREEN, which is the definition of a vacuous
    // assertion.
    let mut rt_app = Double {
        config: rustty_config::Config::default_config(),
    };
    let rt_ptr: *mut Double = &raw mut rt_app;
    app.bind_rt_app(rt_ptr.cast());
    assert!(
        app.imp().rt_app.get().is_some(),
        "bind_rt_app must publish the rt-app backref the thunk dereferences"
    );

    app.bind_core_app::<Double>();
    assert!(
        app.imp().perform_all_action_thunk.borrow().is_some(),
        "bind_core_app must install the performAllAction thunk"
    );

    // S50-F4 (task-14d-3): the SUCCESS path must emit NOTHING. The port
    // used to log `global shortcut action performed ok` here; the reference
    // (`application.zig:1741-1749`) is a bare `catch` with no success arm,
    // and an emission the reference does not make is as much a parity
    // defect as a missing one. Asserting the FULL captured record vector is
    // empty is what makes the regression fail — a `.any()`-style assertion
    // could not see an EXTRA record at all.
    let reached_before = PERFORM_ALL_ACTION_REACHED.load(std::sync::atomic::Ordering::Relaxed);
    let cap = crate::log_capture::start();
    app.imp()
        .global_shortcut_trigger(&ActionBoxed(rustty_input::binding::Action::NewWindow));
    let messages = cap.messages();
    drop(cap);

    assert_eq!(
        app.imp().global_shortcut_trigger_count.get(),
        1,
        "the trigger handler must reach the core dispatch exactly once"
    );
    assert_eq!(
        PERFORM_ALL_ACTION_REACHED.load(std::sync::atomic::Ordering::Relaxed) - reached_before,
        1,
        "the thunk must have reached performAllAction — without this the \
         empty-capture assertion below is vacuous, which is exactly what the \
         S50-F4 negative control caught on Linux"
    );
    assert_eq!(
        Vec::<(log::Level, String, String)>::new(),
        messages,
        "globalShortcutTrigger's SUCCESS path logs nothing in the reference"
    );
}

/// PORTS: `application.zig:1700-1719` — the notification body assembly. Pure
/// string work: no display, no D-Bus, no compositor, so NOT `#[ignore]`d. This
/// is where the user-visible content of the whole `bind-failed` channel lives.
#[test]
fn application_bind_failed_notification_body() {
    use rustty_input::binding::{Trigger, TriggerKey};
    use rustty_vt::input::key_mods::Mods;

    let trigger = Trigger {
        key: TriggerKey::Unicode('q' as u32),
        mods: Mods {
            super_: true,
            ..Default::default()
        },
    };

    // A non-empty compositor message is used verbatim (`:1708`).
    let (title, body) = bind_failed_notification(&BindFailed {
        trigger,
        action: rustty_input::binding::Action::NewWindow,
        message: b"already held by vicinae".to_vec(),
        revoked: false,
    })
    .expect("fits the 512-byte body buffer");
    assert_eq!(title, "Global keybind unavailable");
    assert_eq!(body, b"Super+Q: already held by vicinae");

    // Empty message + denied -> the "denied" fallback (`:1710-1712`).
    let (_, body) = bind_failed_notification(&BindFailed {
        trigger,
        action: rustty_input::binding::Action::NewWindow,
        message: Vec::new(),
        revoked: false,
    })
    .expect("fits the 512-byte body buffer");
    assert_eq!(body, b"Super+Q: The keybind was denied by the system.");

    // Empty message + revoked -> the "revoked" fallback (`:1710-1711`). This
    // pair is the reason the `revoked` flag is carried at all: a test of only
    // one arm passes with the flag ignored.
    let (_, body) = bind_failed_notification(&BindFailed {
        trigger,
        action: rustty_input::binding::Action::NewWindow,
        message: Vec::new(),
        revoked: true,
    })
    .expect("fits the 512-byte body buffer");
    assert_eq!(body, b"Super+Q: The keybind was revoked by the system.");

    // An unformattable trigger degrades to "?" rather than dropping the
    // notification (`:1701-1705`).
    let (_, body) = bind_failed_notification(&BindFailed {
        trigger: Trigger {
            key: TriggerKey::CatchAll,
            mods: Mods::default(),
        },
        action: rustty_input::binding::Action::NewWindow,
        message: b"nope".to_vec(),
        revoked: false,
    })
    .expect("fits the 512-byte body buffer");
    assert_eq!(body, b"?: nope");
}

/// S49-F4 / S50-F3: `globalShortcutBindFailed`'s two fixed buffers
/// (`application.zig:1758-1778`). The body is `bufPrintZ` into 512 bytes, so it
/// fits at 511 bytes plus the NUL and at 512 the reference returns WITHOUT a
/// notification; the label is written into 128 bytes and one byte more turns it
/// into `"?"`. The message bytes are measured raw (a non-UTF-8 byte is ONE byte,
/// not the three of a U+FFFD).
#[test]
fn application_bind_failed_notification_buffer_limits() {
    use rustty_input::binding::{Trigger, TriggerKey};
    use rustty_vt::input::key_mods::Mods;

    let trigger = Trigger {
        key: TriggerKey::Unicode('q' as u32),
        mods: Mods {
            super_: true,
            ..Default::default()
        },
    };
    let notify = |message: Vec<u8>| {
        bind_failed_notification(&BindFailed {
            trigger,
            action: rustty_input::binding::Action::NewWindow,
            message,
            revoked: false,
        })
    };
    // "Super+Q: " is 9 bytes.
    let prefix = b"Super+Q: ".len();

    // 511 bytes of body: the largest that fits `[512]u8` with its sentinel.
    let (_, body) = notify(vec![b'x'; 511 - prefix]).expect("511 bytes fit");
    assert_eq!(body.len(), 511);
    // 512 bytes: `bufPrintZ` fails and the reference raises nothing.
    assert!(
        notify(vec![b'x'; 512 - prefix]).is_none(),
        "a 512-byte body does not fit the reference's buffer"
    );
    // Measured in raw bytes: 501 x's + one 0xE9 is 511 bytes (fits); decoded
    // lossily it would be 513 (and be dropped).
    let mut msg = vec![b'x'; 511 - prefix - 1];
    msg.push(0xE9);
    let (_, body) = notify(msg).expect("511 raw bytes fit");
    assert_eq!(
        body.last(),
        Some(&0xE9),
        "the message bytes are passed through raw"
    );

    // The 128-byte label buffer.
    assert_eq!(bind_failed_label(true, "k".repeat(128)), "k".repeat(128));
    assert_eq!(bind_failed_label(true, "k".repeat(129)), "?");
    assert_eq!(bind_failed_label(false, "k".to_owned()), "?");
}

/// PORTS: `application.zig:1475-1481` — the `bind-failed` connection. Emitting
/// the signal on the app's own `GlobalShortcuts` must reach
/// `globalShortcutBindFailed`. Asserts the HANDLER FIRED (the counter), not
/// merely that emitting did not error.
#[test]
#[ignore = "requires a display; run via xvfb-run -a cargo test -p rustty-apprt --features gtk -- --ignored"]
fn application_bind_failed_signal_reaches_the_handler() {
    use rustty_input::binding::{Trigger, TriggerKey};
    use rustty_vt::input::key_mods::Mods;

    // CORRECTION (task 20): this was `if gtk4::init().is_err() { return; }`,
    // which reports `1 passed` for a test that never ran when there is no display.
    crate::gtk::init_for_display_test("application_bind_failed_signal_reaches_the_handler");
    let _ = libadwaita::init();

    use gio::prelude::ApplicationExt as _;

    let app = build_test_app(std::ptr::null_mut());
    // `startup` is what runs `startupGlobalShortcuts` and makes the connection.
    app.register(gio::Cancellable::NONE).expect("register");

    let gs: GlobalShortcuts = app
        .imp()
        .global_shortcuts
        .borrow()
        .clone()
        .expect("startup created the GlobalShortcuts object")
        .downcast()
        .expect("it is a GhosttyGlobalShortcuts");

    assert_eq!(app.imp().global_shortcut_bind_failed_count.get(), 0);

    gs.emit_bind_failed(&BindFailed {
        trigger: Trigger {
            key: TriggerKey::Unicode('q' as u32),
            mods: Mods {
                super_: true,
                ..Default::default()
            },
        },
        action: rustty_input::binding::Action::NewWindow,
        message: Vec::new(),
        revoked: false,
    });

    assert_eq!(
        app.imp().global_shortcut_bind_failed_count.get(),
        1,
        "the bind-failed signal must reach globalShortcutBindFailed exactly once"
    );
}

/// PORTS: `application.zig:1459-1473` `activate`. Activating queues exactly one
/// `Message::NewWindow` onto the core app mailbox — the app's only
/// window-creation entry (`:1463-1466`).
#[test]
#[ignore = "requires a display; run via xvfb-run -a cargo test -p rustty-apprt --features gtk -- --ignored"]
fn application_activate_pushes_new_window() {
    use gio::subclass::prelude::ApplicationImpl;

    // CORRECTION (task 20): this was `if gtk4::init().is_err() { return; }`,
    // which reports `1 passed` for a test that never ran when there is no display.
    crate::gtk::init_for_display_test("application_activate_pushes_new_window");
    let _ = libadwaita::init();

    // A real core app so the mailbox exists; `create` gives a heap-stable
    // pointer (the `Box` address is stable for the app's lifetime).
    let mut core_app: Box<CoreApp<Double>> = CoreApp::<Double>::create(Global).expect("core app");
    let core_ptr: *mut CoreApp<Double> = &mut *core_app;

    let app = build_test_app(core_ptr.cast());
    // Supply the concrete apprt `App` type so the `activate` thunk is
    // monomorphized (the Rust analogue of Zig's comptime `*CoreApp`).
    app.bind_core_app::<Double>();

    // Drive the `activate` vfunc directly (no g_application_activate / no
    // registration needed for the mailbox push).
    <imp::Application as ApplicationImpl>::activate(app.imp());

    // Drain: exactly one NewWindow.
    let mut new_windows = 0usize;
    let mut total = 0usize;
    while let Some(msg) = core_app.mailbox.pop() {
        total += 1;
        if matches!(msg, Message::NewWindow(_)) {
            new_windows += 1;
        }
    }
    assert_eq!(new_windows, 1, "activate must push exactly one NewWindow");
    assert_eq!(total, 1, "activate must push nothing but the NewWindow");
}

/// S50-F5: `app.open-config` pushes `.open_config = target` onto the core
/// mailbox (`application.zig:2102`) instead of performing it synchronously.
/// Drives the real handler with each target string plus a malformed one, then
/// drains the mailbox: exactly the three pushes, in order, and nothing else.
#[test]
#[ignore = "requires a display; run via cargo test -p rustty-apprt --features gtk,x11,wayland --lib open_config_pushes -- --ignored"]
fn open_config_pushes_onto_the_core_mailbox() {
    use glib::prelude::ToVariant;
    use rustty_core::app::OpenConfig;

    crate::gtk::init_for_display_test("open_config_pushes_onto_the_core_mailbox");
    let _ = libadwaita::init();

    let mut core_app: Box<CoreApp<Double>> = CoreApp::<Double>::create(Global).expect("core app");
    let core_ptr: *mut CoreApp<Double> = &mut *core_app;
    let app = build_test_app(core_ptr.cast());
    app.bind_core_app::<Double>();

    let action = gio::SimpleAction::new("open-config", Some(glib::VariantTy::STRING));
    for target in ["new-window", "os-open", "not-a-target"] {
        action_open_config(&action, Some(&target.to_variant()), &app);
    }

    let mut pushed = Vec::new();
    while let Some(msg) = core_app.mailbox.pop() {
        match msg {
            Message::OpenConfig(v) => pushed.push(v),
            _ => panic!("open-config pushed a message that is not OpenConfig"),
        }
    }
    assert_eq!(
        pushed,
        [
            OpenConfig::NewWindow,
            OpenConfig::OsOpen,
            OpenConfig::OsOpen
        ]
    );
}

/// PORTS: `application.zig:1475-1493` `dispose` + `:1400-1408` `startupSignals`.
/// Startup installs the SIGUSR2 source (`:1403`); dispose removes it and clears
/// the field (invariant 11, `:1482-1487`).
#[test]
#[ignore = "requires a display; run via xvfb-run -a cargo test -p rustty-apprt --features gtk -- --ignored"]
fn application_dispose_removes_signal_source() {
    use gio::prelude::ApplicationExt;

    // CORRECTION (task 20): this was `if gtk4::init().is_err() { return; }`,
    // which reports `1 passed` for a test that never ran when there is no display.
    crate::gtk::init_for_display_test("application_dispose_removes_signal_source");
    let _ = libadwaita::init();

    let app = build_test_app(std::ptr::null_mut());

    // `register` → `startup` → `startupSignals` installs the source.
    app.register(gio::Cancellable::NONE).expect("register");
    assert!(
        app.imp().signal_source.borrow().is_some(),
        "startup must install the SIGUSR2 signal source"
    );

    // `run_dispose` invokes the `dispose` vfunc. `unsafe`: it may run while
    // other references to the object exist; here `app` is the sole owner and
    // is not used for GObject operations after this, so it is sound.
    unsafe { app.run_dispose() };
    assert!(
        app.imp().signal_source.borrow().is_none(),
        "dispose must remove the signal source and clear the field"
    );
}

/// PORTS: `application.zig:1565-1583` (the GTK ≥ 4.20 `prefers-color-scheme`
/// push inside `handleStyleManagerDark`) — the RUNTIME KAT for the opt-in
/// `gtk-v4_20` feature. Ghostty's branch is comptime-folded below 4.20 and
/// runtime-guarded above; rustty mirrors that as `#[cfg(feature =
/// "gtk-v4_20")]` + `gtk_version::at_least(4, 20, 0)`. This test only exists
/// when the feature is on, and only proves anything on a ≥ 4.20 GTK runtime
/// (validated 2026-07-19 on GTK 4.22.4): forcing the Adwaita color scheme
/// must synchronously drive `handle_style_manager_dark` (via the
/// `notify::dark` connection made in `startup_style_manager`,
/// `application.zig:1386-1396`) and push the matching
/// `InterfaceColorScheme` onto the startup-installed `css_provider`
/// (`application.zig:1570-1582`).
#[cfg(feature = "gtk-v4_20")]
#[test]
#[ignore = "requires a display; run via xvfb-run -a cargo test -p rustty-apprt --features gtk,gtk-v4_20 -- --ignored"]
fn application_v4_20_color_scheme_css_push() {
    use gio::prelude::ApplicationExt;
    use libadwaita::prelude::AdwApplicationExt;

    // CORRECTION (task 20): this was `if gtk4::init().is_err() { <print>; return; }`,
    // which reports `1 passed` for a test that never ran when there is no display.
    crate::gtk::init_for_display_test("application_v4_20_color_scheme_css_push");
    let _ = libadwaita::init();
    assert!(
        crate::gtk::gtk_version::at_least(4, 20, 0),
        "the gtk-v4_20 KAT needs a GTK >= 4.20 shared library at runtime \
         (found {}.{}.{})",
        gtk4::major_version(),
        gtk4::minor_version(),
        gtk4::micro_version()
    );

    // A real core app so `build_test_app` has a heap-stable pointer (same
    // shape as `application_activate_pushes_new_window`).
    let mut core_app: Box<CoreApp<Double>> = CoreApp::<Double>::create(Global).expect("core app");
    let core_ptr: *mut CoreApp<Double> = &mut *core_app;
    let app = build_test_app(core_ptr.cast());
    // `register` emits `startup` synchronously → `startup_style_manager`
    // connects `notify::dark` (`application.zig:1386-1396`).
    app.register(gio::Cancellable::NONE).expect("register");
    // The shared `css_provider` is installed by `Application::new`
    // (`application.zig:401` → `new` at `:1844`), which `build_test_app`
    // bypasses (it builds the bare NON_UNIQUE test instance to avoid the
    // primary-instance/session-bus hazard). Install a bare provider on the
    // imp directly — the unit under test is `handle_style_manager_dark`'s
    // ≥ 4.20 push into whatever provider the imp holds, not `new`'s
    // installation (exercised by the real binary).
    let provider = gtk4::CssProvider::new();
    app.imp().css_provider.replace(Some(provider.clone()));

    // `notify::dark` only fires on a real transition, and the session may
    // already be dark (or light) — so force the OPPOSITE of the current
    // state first, then flip back: two guaranteed transitions, each driving
    // `handleStyleManagerDark`'s ≥ 4.20 push, independent of session theme.
    let style = app.style_manager();
    let started_dark = style.is_dark();
    let (first, first_expect, second, second_expect) = if started_dark {
        (
            libadwaita::ColorScheme::ForceLight,
            gtk4::InterfaceColorScheme::Light,
            libadwaita::ColorScheme::ForceDark,
            gtk4::InterfaceColorScheme::Dark,
        )
    } else {
        (
            libadwaita::ColorScheme::ForceDark,
            gtk4::InterfaceColorScheme::Dark,
            libadwaita::ColorScheme::ForceLight,
            gtk4::InterfaceColorScheme::Light,
        )
    };

    style.set_color_scheme(first);
    while glib::MainContext::default().iteration(false) {}
    assert_eq!(
        provider.property::<gtk4::InterfaceColorScheme>("prefers-color-scheme"),
        first_expect,
        "the first forced transition must push the matching \
         InterfaceColorScheme onto the css_provider (application.zig:1570-1582)"
    );

    style.set_color_scheme(second);
    while glib::MainContext::default().iteration(false) {}
    assert_eq!(
        provider.property::<gtk4::InterfaceColorScheme>("prefers-color-scheme"),
        second_expect,
        "the second forced transition must push the matching \
         InterfaceColorScheme onto the css_provider"
    );
}

/// PORTS: `application.zig:474-592` `run` + `:1286-1289` `wakeup` + `:639-665`
/// `quitNow`. The L5.1 gate KAT for the hand-rolled loop: with no initial
/// window and `quit-after-last-window-closed`, `run` must acquire → register →
/// (skip activate) → loop `{ iteration(true); tick; check-quit }`, exit cleanly
/// when `quit_now` sets `running = false`, and have ticked the core at least
/// once. A background thread exercises the `wakeup` bridge (breaking the
/// blocking iteration), and a main-thread timeout routes to `quit_now`.
#[test]
#[ignore = "requires a display; run via xvfb-run -a cargo test -p rustty-apprt --features gtk -- --ignored"]
fn application_run_loop_ticks_then_quits() {
    // CORRECTION (task 20): this was `if gtk4::init().is_err() { return; }`,
    // which reports `1 passed` for a test that never ran when there is no display.
    crate::gtk::init_for_display_test("application_run_loop_ticks_then_quits");
    let _ = libadwaita::init();

    // A real apprt `App` double + core app so the tick thunk dereferences live
    // backrefs (both heap/stack pointers are stable for the test's duration).
    let mut rt = Double {
        config: rustty_config::Config::default_config(),
    };
    let rt_ptr: *mut Double = &mut rt;
    let mut core_app: Box<CoreApp<Double>> = CoreApp::<Double>::create(Global).expect("core app");
    let core_ptr: *mut CoreApp<Double> = &mut *core_app;

    let app = build_test_app(core_ptr.cast());
    app.imp().rt_app.set(NonNull::new(rt_ptr.cast()));

    // No initial window (so `activate`/window-creation stays dormant — no real
    // renderer here), quit after last window closed on. `quit_now` is what
    // actually ends this test's loop.
    let mut cfg = rustty_config::Config::default_config();
    cfg.initial_window = false;
    cfg.quit_after_last_window_closed = true;
    app.imp().config.replace(Some(Config::new(&cfg)));

    // Supply the concrete apprt `App` type so the tick / new-window thunks are
    // monomorphized (the Rust analogue of Zig's comptime `*CoreApp`).
    app.bind_core_app::<Double>();

    // Route a main-thread quit through `quit_now` after the loop has ticked a
    // few times (a glib timeout fires on the acquired context inside `run`).
    let app_weak = app.downgrade();
    glib::timeout_add_local(std::time::Duration::from_millis(120), move || {
        if let Some(app) = app_weak.upgrade() {
            app.quit_now();
        }
        glib::ControlFlow::Break
    });

    // Exercise the libghostty→GTK `wakeup` bridge from another thread: break
    // the blocking `iteration(true)` so the inline tick runs promptly.
    std::thread::spawn(|| {
        for _ in 0..8 {
            std::thread::sleep(std::time::Duration::from_millis(10));
            glib::MainContext::default().wakeup();
        }
    });

    let start = std::time::Instant::now();
    app.run().expect("run loop exits cleanly");
    assert!(
        start.elapsed() < std::time::Duration::from_secs(10),
        "run loop must exit within the deadline"
    );
    assert!(
        app.imp().tick_count.get() >= 1,
        "the inline core tick must run at least once (got {})",
        app.imp().tick_count.get()
    );
    assert!(
        !app.imp().running.get(),
        "quit_now must have cleared the running flag"
    );
}

/// F-011 KAT: `saved_language` reports the launch-time `LANG` stored in
/// `priv.saved_language` (`application.zig:475-478`) — not a hardcoded `None`
/// (the regressed placeholder), which made `defaultTermioEnv`
/// (`surface.zig:1602-1606`) strip `LANG` from every default child
/// environment.
#[test]
#[ignore = "requires a display; run via xvfb-run -a cargo test -p rustty-apprt --features gtk -- --ignored"]
fn saved_language_reads_the_captured_launch_lang() {
    // CORRECTION (task 20): this was `if gtk4::init().is_err() { return; }`,
    // which reports `1 passed` for a test that never ran when there is no display.
    crate::gtk::init_for_display_test("saved_language_reads_the_captured_launch_lang");
    let _ = libadwaita::init();

    let app = build_test_app(std::ptr::null_mut());

    // `LANG` unset at launch → `None` (the consumer then removes `LANG`).
    app.imp().saved_language.replace(None);
    assert_eq!(app.saved_language(), None);

    // Captured at launch → returned verbatim (a copy of the stored bytes).
    let lang = CString::new("en_US.UTF-8").unwrap();
    app.imp().saved_language.replace(Some(lang.clone()));
    assert_eq!(app.saved_language(), Some(lang));
}

/// Recursively find a presented [`CloseConfirmationDialog`] in a widget tree
/// (presented unparented, the dialog floats inside its own toplevel host).
fn find_close_confirmation(widget: &gtk4::Widget) -> Option<CloseConfirmationDialog> {
    use gtk4::prelude::WidgetExt as _;
    if let Some(dialog) = widget.downcast_ref::<CloseConfirmationDialog>() {
        return Some(dialog.clone());
    }
    let mut child = widget.first_child();
    while let Some(c) = child {
        if let Some(found) = find_close_confirmation(&c) {
            return Some(found);
        }
        child = c.next_sibling();
    }
    None
}

/// F-009 KAT (`application.zig:603-644` `quit` + `:1552-1558`
/// `handleCloseConfirmation`): when the core needs quit confirmation, `quit`
/// must NOT stop the loop; it must present a `CloseConfirmationDialog(.app)`
/// whose `close-request` routes to `quit_now`. Against the pre-fix
/// placeholder (a stderr warning, no dialog) the toplevel scan finds nothing
/// and this fails.
#[test]
#[ignore = "requires a display; run via xvfb-run -a cargo test -p rustty-apprt --features gtk -- --ignored"]
fn quit_needing_confirmation_presents_dialog_and_close_request_quits() {
    use libadwaita::prelude::AdwDialogExt as _;

    // CORRECTION (task 20): this was `if gtk4::init().is_err() { return; }`,
    // which reports `1 passed` for a test that never ran when there is no display.
    crate::gtk::init_for_display_test(
        "quit_needing_confirmation_presents_dialog_and_close_request_quits",
    );
    let _ = libadwaita::init();
    crate::gtk::register_resources();

    let app = build_test_app(std::ptr::null_mut());
    app.imp().running.set(true);
    app.imp()
        .needs_confirm_quit_thunk
        .replace(Some(Box::new(|| true)));

    app.quit();
    while glib::MainContext::default().iteration(false) {}

    // Not confirmed yet → the loop keeps running (`:609` comment: we must
    // NOT quit without confirmation).
    assert!(
        app.imp().running.get(),
        "quit must not stop the loop before the dialog is confirmed"
    );

    // The dialog was presented (unparented here: no active window exists).
    let dialog = gtk4::Window::list_toplevels()
        .iter()
        .find_map(find_close_confirmation)
        .expect("quit presents a CloseConfirmationDialog");
    assert_eq!(dialog.target(), DialogTarget::App);

    // Confirming — the dialog's `close-request` — routes to `quit_now`
    // (`handleCloseConfirmation`, `application.zig:1552-1558`).
    dialog.emit_by_name::<()>("close-request", &[]);
    assert!(
        !app.imp().running.get(),
        "close-request must route to quit_now"
    );

    dialog.close();
    while glib::MainContext::default().iteration(false) {}
}

/// F-010 KAT (`application.zig:646-666` `quitNow`): destroys the toplevels
/// that are OUR `Window` subclass (forcing the ordered widget-teardown
/// cascade), spares foreign toplevels (the fcitx-IME double-free guard,
/// `:652-656`), then trips the run-loop exit flag. Against the pre-fix
/// placeholder (`let _ = window;`) the `Window` survives and this fails.
#[test]
#[ignore = "requires a display; run via xvfb-run -a cargo test -p rustty-apprt --features gtk -- --ignored"]
fn quit_now_destroys_our_windows_and_spares_foreign_toplevels() {
    use gio::prelude::ApplicationExt as _;
    use gtk4::prelude::GtkWindowExt as _;

    // CORRECTION (task 20): this was `if gtk4::init().is_err() { return; }`,
    // which reports `1 passed` for a test that never ran when there is no display.
    crate::gtk::init_for_display_test("quit_now_destroys_our_windows_and_spares_foreign_toplevels");
    let _ = libadwaita::init();
    crate::gtk::register_resources();

    // A registered app: `Window::new` binds `application`, and `startup`
    // installs us as the default `GApplication` for the template machinery.
    let app = build_test_app(std::ptr::null_mut());
    app.register(gio::Cancellable::NONE)
        .expect("register the application");
    app.imp().running.set(true);

    // One of OUR windows, and a foreign toplevel we must NOT destroy (the
    // fcitx-IME stand-in).
    let ours = Window::new(&app, None);
    let foreign = gtk4::Window::new();
    let foreign_widget = foreign.upcast_ref::<gtk4::Widget>().clone();
    assert!(
        gtk4::Window::list_toplevels()
            .iter()
            .any(|w| w.downcast_ref::<Window>().is_some()),
        "precondition: our Window is a live toplevel"
    );

    app.quit_now();

    let toplevels = gtk4::Window::list_toplevels();
    assert!(
        !toplevels
            .iter()
            .any(|w| w.downcast_ref::<Window>().is_some()),
        "quit_now destroys our Window toplevels (application.zig:657-660)"
    );
    assert!(
        toplevels.contains(&foreign_widget),
        "foreign toplevels are spared (the fcitx guard, application.zig:652-656)"
    );
    assert!(
        !app.imp().running.get(),
        "quit_now clears the running flag (application.zig:664)"
    );

    foreign.destroy();
    drop(ours);
}

/// PORTS: `application.zig:823-846` `startQuitTimer` + `:849-864`
/// `stopQuitTimer`. Arming twice must not leak a source (the second arm cancels
/// the first), and stopping is idempotent — every path returns the timer to
/// `.off` (invariant 11).
#[test]
#[ignore = "requires a display; run via xvfb-run -a cargo test -p rustty-apprt --features gtk -- --ignored"]
fn quit_timer_start_stop_is_idempotent() {
    use rustty_config::value_types::Duration;

    // CORRECTION (task 20): this was `if gtk4::init().is_err() { return; }`,
    // which reports `1 passed` for a test that never ran when there is no display.
    crate::gtk::init_for_display_test("quit_timer_start_stop_is_idempotent");
    let _ = libadwaita::init();

    let app = build_test_app(std::ptr::null_mut());

    // A delay is configured so `start_quit_timer` arms an `.active` source
    // (the leak-prone path) rather than short-circuiting to `.expired`.
    let mut cfg = rustty_config::Config::default_config();
    cfg.quit_after_last_window_closed = true;
    // 10s — long enough it never fires during the test.
    cfg.quit_after_last_window_closed_delay = Some(Duration {
        duration: 10 * 1_000_000_000,
    });
    app.imp().config.replace(Some(Config::new(&cfg)));

    // Arm once → `.active`.
    app.imp().start_quit_timer();
    assert!(
        matches!(*app.imp().quit_timer.borrow(), imp::QuitTimer::Active(_)),
        "start_quit_timer with a delay must arm an active source"
    );

    // Arm again → still `.active`, and the previous source was cancelled (no
    // leak): `start_quit_timer` calls `stop_quit_timer` first.
    app.imp().start_quit_timer();
    assert!(
        matches!(*app.imp().quit_timer.borrow(), imp::QuitTimer::Active(_)),
        "re-arming must leave exactly one active source"
    );

    // Stop → `.off`.
    app.imp().stop_quit_timer();
    assert!(
        matches!(*app.imp().quit_timer.borrow(), imp::QuitTimer::Off),
        "stop_quit_timer must return the timer to off"
    );

    // Stop again → still `.off` (idempotent, no double-remove).
    app.imp().stop_quit_timer();
    assert!(
        matches!(*app.imp().quit_timer.borrow(), imp::QuitTimer::Off),
        "stop_quit_timer must be idempotent"
    );
}

/// PORTS: `application.zig:1286-1289` `wakeup`. A `MainContext::wakeup` from
/// another thread must break a blocking `iteration(true)` on the owning thread
/// — the mechanism the run loop relies on to interleave the inline core tick.
#[test]
#[ignore = "requires a display; run via xvfb-run -a cargo test -p rustty-apprt --features gtk -- --ignored"]
fn wakeup_breaks_blocking_iteration() {
    // CORRECTION (task 20): this was `if gtk4::init().is_err() { return; }`,
    // which reports `1 passed` for a test that never ran when there is no display.
    crate::gtk::init_for_display_test("wakeup_breaks_blocking_iteration");
    let _ = libadwaita::init();

    // `Application::wakeup` delegates to `MainContext::default().wakeup()`; build
    // an app to cover the method itself (calling it here is a harmless no-op
    // when nothing is blocking).
    let app = build_test_app(std::ptr::null_mut());
    app.wakeup();

    let ctx = glib::MainContext::default();
    let _acquire = ctx.acquire().expect("acquire the default main context");

    // Another thread wakes us after a short delay.
    std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_millis(50));
        glib::MainContext::default().wakeup();
    });

    // Blocks until the cross-thread wakeup; must return rather than hang.
    let start = std::time::Instant::now();
    let _ = ctx.iteration(true);
    assert!(
        start.elapsed() < std::time::Duration::from_secs(10),
        "wakeup must break the blocking iteration"
    );
}

/// V14 (`.port/phase-2/VERIFICATION_DEBT.md`): drive
/// [`imp::Application::signal_css_parsing_error`] ITSELF, not just the
/// `CssSection::bytes()` binding it calls.
///
/// PORTS the acceptance surface of `application.zig:1261-1274`: on a >= 4.16
/// build box the handler logs the OFFENDING CSS SOURCE BYTES after the
/// location/domain/code/message; on the 4.14 floor the `if (comptime
/// gtk_version.atLeast(4, 16, 0))` arm folds away and the log carries only the
/// fallback line (`:1275-1280`).
///
/// This is a LIB test because `signal_css_parsing_error` is `pub(super)` and
/// therefore unreachable from `tests/gtk_css_parse_error.rs`, which can only
/// prove the binding.
///
/// The assertion is a DIFFERENTIAL, not a one-sided claim: the expected
/// presence of the bytes is `cfg!(feature = "gtk-v4_16")`, so the same test
/// body asserts PRESENT under `--features gtk,gtk-v4_16` and ABSENT under
/// `--features gtk`. Either arm fails if the handler's gate does not match its
/// feature.
///
/// G-GTKINIT: this reuses the lib-test binary's ONE `gtk4::init()` the same way
/// every other headed test in this module does (soft-skip when GTK is already
/// bound to a different libtest thread) — it does not add a new init to a
/// `tests/` binary.
#[test]
#[ignore = "headed GTK. run: \
            cargo test -p rustty-apprt --features gtk[,gtk-v4_16] --lib \
            css_parsing_error -- --ignored"]
fn signal_css_parsing_error_byte_dump_follows_the_feature_ceiling() {
    // CORRECTION (task 20): this was `if gtk4::init().is_err() { <print>; return; }`,
    // which reports `1 passed` for a test that never ran when there is no display.
    crate::gtk::init_for_display_test(
        "signal_css_parsing_error_byte_dump_follows_the_feature_ceiling",
    );
    let _ = libadwaita::init();

    // The runtime half of the two-part gate: with the feature on but a < 4.16
    // shared library the arm self-disables and there is nothing to prove.
    #[cfg(feature = "gtk-v4_16")]
    assert!(
        crate::gtk::gtk_version::at_least(4, 16, 0),
        "this KAT needs a >= 4.16 GTK runtime; the byte-dump arm self-disables below it"
    );

    const BAD_CSS: &str = "window { color: ###notacolor; }";

    let app = build_test_app(std::ptr::null_mut());

    // The production wiring shape (`:1201-1207` / `Application::new`): a real
    // `CssProvider::parsing-error` emission, forwarded straight into the
    // handler under test with the section and error GTK itself produced.
    let calls = std::rc::Rc::new(std::cell::Cell::new(0usize));
    let provider = gtk4::CssProvider::new();
    provider.connect_parsing_error(glib::clone!(
        #[weak]
        app,
        #[strong]
        calls,
        move |_provider, section, error| {
            calls.set(calls.get() + 1);
            app.imp().signal_css_parsing_error(section, error);
        }
    ));

    // task-14d-3: this used to dup(2)-redirect fd 2 and grep the bytes,
    // because the handler wrote through a raw `eprintln!`. It now goes
    // through the `log::` facade, so the records are read from the capture
    // logger instead — which is strictly better evidence (level and scope
    // are pinned too) and removes this test's `--nocapture` dependency for
    // the log half. The `capture_stderr` fd-2 helper had no other caller
    // and is deleted with it.
    let cap = crate::log_capture::start();
    provider.load_from_string(BAD_CSS);
    let logged = cap.messages();
    drop(cap);

    assert!(
        calls.get() > 0,
        "a malformed stylesheet must raise parsing-error and reach the handler"
    );
    assert!(
        !logged.is_empty(),
        "the handler emitted NOTHING — the assertions below would be vacuous"
    );

    println!(
        "V14 EVIDENCE (gtk-v4_16 = {}): {logged:?}",
        cfg!(feature = "gtk-v4_16"),
    );

    // Both arms log the fallback fields (`:1275-1280`), at warn, under this
    // file's own reference scope.
    assert!(
        logged.iter().any(|(level, target, msg)| {
            *level == log::Level::Warn
                && target == LOG_SCOPE
                && msg.starts_with("css parsing failed at")
        }),
        "the handler must log the fallback line in BOTH arms, at warn under \
         {LOG_SCOPE}; got {logged:?}"
    );

    // The differential: the raw source bytes appear IFF the ceiling is raised.
    let has_bytes = logged.iter().any(|(_, _, msg)| msg.contains(BAD_CSS));
    assert_eq!(
        has_bytes,
        cfg!(feature = "gtk-v4_16"),
        "the `application.zig:1261-1274` byte dump must be present IFF \
         `gtk-v4_16` is on (feature on = {}); logged: {logged:?}",
        cfg!(feature = "gtk-v4_16"),
    );
}
