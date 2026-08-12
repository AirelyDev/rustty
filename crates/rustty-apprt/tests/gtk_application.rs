//! Integration KAT for the `GhosttyApplication` subclass skeleton (L5.1 T7).
//!
//! PORTS the reference's process-global singleton contract: `Application.default()`
//! is `gobject.ext.cast(Self, gio.Application.getDefault().?)`
//! (`src/apprt/gtk/class/application.zig:230-233`) — it asserts a default
//! `GApplication` exists AND that it is a `GhosttyApplication`. This KAT builds an
//! instance, installs it as the process-global default, and proves `default()`
//! round-trips back to the very same GObject.
//!
//! R5: gated `all(any(linux, freebsd), feature = "gtk")` — off the gate the module
//! (and the gtk4-rs bindings it uses) is compiled out. Instantiating a
//! `GhosttyApplication` (an `adw::Application` subclass) touches GTK, so per the L5
//! test policy it is `#[ignore]`d out of the headless default gate and run
//! explicitly:
//!   xvfb-run -a cargo test -p rustty-apprt --features gtk --test gtk_application -- --ignored
#![cfg(all(any(target_os = "linux", target_os = "freebsd"), feature = "gtk"))]

use std::ffi::c_void;

use gio::prelude::*;

use rustty_apprt::gtk::build_info;
use rustty_apprt::gtk::class::application::Application;
use rustty_apprt::gtk::winproto;

/// PORTS `application.zig:230-233` `default`. Build a `GhosttyApplication`, set it
/// as the process-global default `GApplication`, and assert `Application::default()`
/// resolves back to the identical instance (GObject pointer identity).
#[test]
#[ignore = "instantiates an adw::Application subclass (touches GTK); run via the --ignored GTK lane"]
fn application_default_round_trips() {
    // The skeleton's heavyweight `new` (application.zig:243) is a Task 8 leaf, so
    // build the bare subclass instance directly with a valid application-id (the
    // R2 identity, never a hardcoded bundle id). This registers the
    // `GhosttyApplication` GType and runs its class-init (which registers the
    // compiled gresources).
    let app: Application = glib::Object::builder()
        .property("application-id", build_info::APPLICATION_ID)
        .build();

    // `gio.Application.getDefault()` returns null until a default is set, and the
    // reference's `default()` unconditionally `.?`-unwraps it — so install ours.
    // gio-rs binds `g_application_set_default` as `ApplicationExt::set_default(&self)`.
    let gio_app = app.upcast_ref::<gio::Application>();
    gio_app.set_default();

    // `Application.default()` must hand back the SAME GObject (pointer identity),
    // downcast to `GhosttyApplication`.
    let got = Application::default();
    assert_eq!(
        app.upcast_ref::<glib::Object>(),
        got.upcast_ref::<glib::Object>(),
        "Application::default() must round-trip to the installed default instance"
    );

    // Invariant 2 witness: the one canonical allocator is reachable off the
    // singleton (application.zig:464-466). In rustty the seam is the global
    // allocator; the accessor exists and returns it.
    let _alloc: allocator_api2::alloc::Global = got.allocator();

    // The nullable `config` property is `None` on a bare skeleton instance (no
    // `new` ran), witnessing the `getConfig` accessor (application.zig:1208).
    assert!(
        got.get_config().is_none(),
        "a skeleton Application has no config until Task 8's `new` sets it"
    );
}

/// PORTS: the `noop`-fallthrough contract (`winproto/noop.zig:13-20` +
/// `winproto.zig:46`). Display-free — [`winproto::noop::App::init`] takes no
/// display and always returns `Ok(None)`, and the union's `Default` is the
/// unconditional `.none` — so a noop-only build (this task: no `wayland`/`x11`
/// feature) always lands on [`winproto::Protocol::None`] without ever touching a
/// `gdk::Display`. Runs in the normal `--features gtk` gate (no `#[ignore]`).
#[test]
fn winproto_noop_init_falls_through() {
    let config = rustty_config::Config::default_config();

    // `noop::App::init` always falls through (`winproto/noop.zig:13-20`).
    let probed = winproto::noop::App::init("io.airely.rustty", &config);
    assert!(
        matches!(probed, Ok(None)),
        "noop::App::init must always return Ok(None)"
    );

    // The union's unconditional fallback arm is `.none` (`winproto.zig:46`).
    let app = winproto::App::default();
    assert_eq!(app.protocol(), winproto::Protocol::None);
    assert!(matches!(app, winproto::App::None(_)));
    assert!(
        !app.supports_quick_terminal(),
        "the noop backend never supports the quick terminal"
    );

    // The `Window` union's fallback arm reports CSD enabled (`winproto/noop.zig:65-68`).
    let win = winproto::Window::None(winproto::noop::Window);
    assert!(win.client_side_decoration_enabled());
}

/// PORTS `application.zig:243-428` `new` + `:435-459` `deinit`. Build a real
/// `GhosttyApplication` through the full `new` path, assert the windowing
/// protocol resolved to `.none` (noop-only build), that it installs as the
/// process-global default, and that `deinit` tears the OWNED state down cleanly.
///
/// `#[ignore]`: `new` runs `adw::init()` / `gdk::Display::default()` / CSS
/// provider registration, so it touches GTK and needs a display — run via the
/// `--ignored` GTK lane:
///   xvfb-run -a cargo test -p rustty-apprt --features gtk --test gtk_application -- --ignored
#[test]
#[ignore = "Application::new touches GTK (adw::init + display); run via the --ignored GTK lane"]
fn application_new_then_deinit() {
    // `rt_app`/`core_app` are opaque BACKREF pointers (PREPORT_GTK §1.1: "never
    // freed here") that neither `new` nor `deinit` dereferences — the allocator
    // seam is the global allocator, not `core_app.alloc`. A faithful
    // `*ApprtApp`/`*CoreApp` needs `gtk::App` (Task 11), which does not exist yet,
    // so this KAT passes stable non-null placeholders.
    let mut rt_app_slot: u8 = 0;
    let mut core_app_slot: u8 = 0;
    let rt_app = std::ptr::addr_of_mut!(rt_app_slot).cast::<c_void>();
    let core_app = std::ptr::addr_of_mut!(core_app_slot).cast::<c_void>();

    let app = Application::new(rt_app, core_app);

    // A noop-only build always lands on `.none` (`winproto.zig:46`).
    assert_eq!(
        app.winproto_protocol(),
        winproto::Protocol::None,
        "a noop-only build must select the .none windowing protocol"
    );

    // `new` sets the config, so the nullable `config` property is now `Some`.
    assert!(
        app.get_config().is_some(),
        "Application::new must set the config"
    );

    // Install as the process-global default and prove `default()` round-trips
    // (`application.zig:230-233`).
    app.upcast_ref::<gio::Application>().set_default();
    assert_eq!(
        app.upcast_ref::<glib::Object>(),
        Application::default().upcast_ref::<glib::Object>(),
        "Application::default() must resolve to the constructed instance"
    );

    // Force teardown (`application.zig:435-459`) must run clean and clear the
    // OWNED rows — witnessed by the config being unset afterward.
    app.deinit();
    assert!(
        app.get_config().is_none(),
        "deinit must clear the config (application.zig:438)"
    );
}
