//! Compile-time KAT for the GTK apprt `App` seam (L5.1 T11).
//!
//! PORTS the `apprt.runtime.App` interface contract for the NATIVE gtk4-rs
//! backend: `gtk::App` must satisfy [`rustty_apprt_api::App`]. (P7-W7 T2:
//! `2f0b65346` deleted `must_draw_from_app_thread`, which this KAT also pinned.)
//!
//! R5: gated `all(any(linux, freebsd), feature = "gtk")` — off the gate the
//! `gtk` module (and the gtk4-rs bindings it uses) is compiled out entirely.
//! This is a pure compile-and-const-assert KAT: it touches no GTK runtime and
//! needs no display, so it runs in the normal (headless) gate.
#![cfg(all(any(target_os = "linux", target_os = "freebsd"), feature = "gtk"))]

/// Compiles only if `gtk::App` satisfies the seam.
#[test]
fn gtk_app_satisfies_seam() {
    fn needs_app<A: rustty_apprt_api::App>() {}
    needs_app::<rustty_apprt::gtk::App>();
}

/// KAT for the THIRD member of the required comptime apprt API, `resourcesDir`
/// (W16-GTK-02).
///
/// `apprt/gtk.zig:4` is `pub const resourcesDir = @import("gtk/flatpak.zig")
/// .resourcesDir;` — the gtk runtime's Flatpak-aware OVERRIDE of the plain
/// `internal_os.resourcesDir` that `none`/`browser`/`embedded` alias
/// (`none.zig:6`, `browser.zig:2`, `embedded.zig:28`). Its ONE consumer is
/// `global.zig:214` `self.resources_dir = try apprt.runtime.resourcesDir(...)`.
/// Rustty's `apprt.runtime` selector is the `cfg` that declares `mod gtk`, so
/// this KAT pins both halves of that routing:
///
///   1. the export exists on the `gtk` module with the apprt-wide signature
///      (before W16-GTK-02 `flatpak.rs` was ported but never re-exported, so
///      nothing could reach it);
///   2. `rustty_apprt::init` publishes into `global_state().resources_dir`
///      exactly what the GTK override returns — under this build's
///      `build_config.flatpak == false` both sides fold to
///      `internal_os.resourcesDir` (`flatpak.zig:8,28`), which is precisely why
///      the routing regressed silently; the assertion is the contract that keeps
///      the two sides tied once the Flatpak arm goes live.
#[test]
fn gtk_exports_resources_dir_and_global_init_routes_through_it() {
    // (1) `pub const resourcesDir = ...` — same signature every apprt exports.
    let runtime_resources_dir: fn() -> Result<
        rustty_os::ResourcesDir,
        rustty_os::resourcesdir::ResourcesDirError,
    > = rustty_apprt::gtk::resources_dir;

    // (2) `global.zig:214` — global init's value IS the runtime's value.
    rustty_apprt::init().expect("initialize rustty global state");
    let expected = runtime_resources_dir().expect("gtk resourcesDir");
    let published = &rustty_core::global_state().resources_dir;
    assert_eq!(published.app(), expected.app(), "app path");
    assert_eq!(published.host(), expected.host(), "host path");
}

/// Compile-time KAT for the GTK apprt **`Surface`** seam (L5.2 T17): the GTK
/// `Surface` shim must satisfy [`rustty_apprt_api::Surface`], and the two halves
/// of the seam must refer to each other (`seam.rs:73,128` — `App::Surface = S`
/// and `S::App = App`), exactly as the reference runtime types do
/// (`App.zig:14` / `Surface.zig:7-9`). Pure compile-and-const-assert; no display.
#[allow(dead_code)]
fn _assert_surface_seam() {
    fn needs_surface<S: rustty_apprt_api::Surface>() {}
    needs_surface::<rustty_apprt::gtk::Surface>();

    // The two halves of the seam refer to each other (`seam.rs:73,128`).
    fn needs_pair<A, S>()
    where
        A: rustty_apprt_api::App<Surface = S>,
        S: rustty_apprt_api::Surface<App = A>,
    {
    }
    needs_pair::<rustty_apprt::gtk::App, rustty_apprt::gtk::Surface>();
}
