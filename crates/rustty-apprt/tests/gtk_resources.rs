//! Integration KAT: the `build.rs` asset pipeline (blueprint-compiler ->
//! gresource XML -> `glib-compile-resources` -> `cc`) must produce a gresource
//! blob that, once registered at class-init, resolves the compiled `.ui`
//! resources by their `build_info::RESOURCE_PATH`-rooted paths.
//!
//! PORTS the reference's class-init registration + lookup contract: the
//! generated `ghostty_get_resource()` is registered via `gio.resourcesRegister`
//! (`src/apprt/gtk/class/application.zig:1867-1881`) and the compiled UI is then
//! reachable at `gresource.blueprint(...)` (`build/gresource.zig:103-123`).
//!
//! R5: gated `all(any(linux, freebsd), feature = "gtk")` — off the gate the
//! module (and the `gio` binding it uses) is compiled out. Because it registers
//! a live gresource against the process-global gio registry it needs no display,
//! so it is `#[ignore]`d out of the normal gate and run explicitly:
//!   xvfb-run -a cargo test -p rustty-apprt --features gtk -- --ignored
//! (it does not actually need a display; `#[ignore]` keeps GTK-touching tests
//! out of the headless default gate per the L5 test policy).
#![cfg(all(any(target_os = "linux", target_os = "freebsd"), feature = "gtk"))]

/// Registering the compiled resources makes every blueprint `.ui` resolvable at
/// its resource path — the end-to-end proof that `build.rs` compiled the `.blp`,
/// embedded the `.ui` under the identity resource path, and linked the blob.
#[test]
#[ignore = "registers a process-global gresource; run via the --ignored GTK lane"]
fn gresource_registers_and_looks_up_ui() {
    // The class-init helper Task 7 will call (idempotent `Once`).
    rustty_apprt::gtk::register_resources();

    // "/<resource_path>/ui/1.2/surface.ui"
    let p = rustty_apprt::gtk::gresource::blueprint(1, 2, "surface");
    assert!(
        gio::resources_lookup_data(p, gio::ResourceLookupFlags::NONE).is_ok(),
        "compiled surface.ui must be resolvable at {p}"
    );

    // A second, independently-compiled blueprint from a different version dir.
    let w = rustty_apprt::gtk::gresource::blueprint(1, 5, "window");
    assert!(
        gio::resources_lookup_data(w, gio::ResourceLookupFlags::NONE).is_ok(),
        "compiled window.ui must be resolvable at {w}"
    );
}
