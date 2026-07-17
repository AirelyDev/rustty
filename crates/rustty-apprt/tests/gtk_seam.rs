//! Compile-time KAT for the GTK apprt `App` seam (L5.1 T11).
//!
//! PORTS the `apprt.runtime.App` interface contract for the NATIVE gtk4-rs
//! backend: `gtk::App` must satisfy [`rustty_apprt_api::App`], and its
//! `MUST_DRAW_FROM_APP_THREAD` const must be `true` — the faithful Rust model of
//! `App.zig:20-23` `pub const must_draw_from_app_thread = true` (GTK's `GLArea`
//! cannot draw off the app thread, so the renderer sends `redraw_surface`
//! instead, `renderer/Thread.zig:22-30`). The embedded backend does NOT declare
//! it, so its const stays the trait default `false` (R5: byte-identical).
//!
//! R5: gated `all(target_os = "linux", feature = "gtk")` — off the gate the
//! `gtk` module (and the gtk4-rs bindings it uses) is compiled out entirely.
//! This is a pure compile-and-const-assert KAT: it touches no GTK runtime and
//! needs no display, so it runs in the normal (headless) gate.
#![cfg(all(target_os = "linux", feature = "gtk"))]

/// Compiles only if `gtk::App` satisfies the seam with the GTK-specific const,
/// and if the embedded backend keeps the default `false`.
#[test]
fn gtk_app_satisfies_seam_with_must_draw_const() {
    fn needs_app<A: rustty_apprt_api::App>() {}
    needs_app::<rustty_apprt::gtk::App>();

    const _: () =
        assert!(<rustty_apprt::gtk::App as rustty_apprt_api::App>::MUST_DRAW_FROM_APP_THREAD);
    const _: () =
        assert!(!<rustty_apprt::embedded::App as rustty_apprt_api::App>::MUST_DRAW_FROM_APP_THREAD);
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
