//! Integration KAT for the `GhosttyApplication` subclass skeleton (L5.1 T7).
//!
//! PORTS the reference's process-global singleton contract: `Application.default()`
//! is `gobject.ext.cast(Self, gio.Application.getDefault().?)`
//! (`src/apprt/gtk/class/application.zig:230-233`) — it asserts a default
//! `GApplication` exists AND that it is a `GhosttyApplication`. This KAT builds an
//! instance, installs it as the process-global default, and proves `default()`
//! round-trips back to the very same GObject.
//!
//! R5: gated `all(target_os = "linux", feature = "gtk")` — off the gate the module
//! (and the gtk4-rs bindings it uses) is compiled out. Instantiating a
//! `GhosttyApplication` (an `adw::Application` subclass) touches GTK, so per the L5
//! test policy it is `#[ignore]`d out of the headless default gate and run
//! explicitly:
//!   xvfb-run -a cargo test -p rustty-apprt --features gtk --test gtk_application -- --ignored
#![cfg(all(target_os = "linux", feature = "gtk"))]

use gio::prelude::*;

use rustty_apprt::gtk::build_info;
use rustty_apprt::gtk::class::application::Application;

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
