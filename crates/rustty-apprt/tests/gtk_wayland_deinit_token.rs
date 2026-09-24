//! UD105 (R13, task R13-D): a window torn down while its `xdg_activation` token
//! is pending must not receive the token's `done`.
//!
//! The reference's `wayland.zig` `Window.deinit` (`:216-221`) destroys the
//! background-effect, decoration and slide proxies but not a pending
//! `activation_token`, whose listener userdata is the `*Window` itself
//! (`setUrgent`, `:267-273`). A `done` that arrives after the window is gone is
//! dispatched to `onActivationTokenEvent` with a dangling `self`, and the proxy
//! leaks. The port's `deinit` destroys the token, so libwayland drops the reply.
//!
//! ONE HEADED TEST PER BINARY (see `gtk_wayland_set_urgent.rs`): `gtk4::init()`
//! binds GTK to one thread per process.
#![cfg(all(any(target_os = "linux", target_os = "freebsd"), rustty_gtk_wayland))]

use gtk4::glib;
use gtk4::prelude::*;

use rustty_apprt::gtk::winproto::{App, Protocol, Window};
use rustty_config::Config as CoreConfig;

fn activation_done_count(app: &App) -> u32 {
    match app {
        App::Wayland(wp) => wp.activation_done_count(),
        _ => panic!("expected the Wayland winproto arm"),
    }
}

fn drain_main_context(ctx: &glib::MainContext) {
    while ctx.iteration(false) {}
}

/// Pump for up to `ms` milliseconds, or until `done` holds.
fn pump_until(ctx: &glib::MainContext, ms: u64, mut done: impl FnMut() -> bool) -> bool {
    for _ in 0..ms / 5 {
        drain_main_context(ctx);
        if done() {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    false
}

#[test]
#[ignore = "headed GTK + a live Wayland compositor; run (do NOT set GDK_BACKEND=x11): \
            cargo test -p rustty-apprt --features gtk,wayland --test gtk_wayland_deinit_token \
            -- --ignored --exact a_pending_activation_token_is_destroyed_with_its_window"]
fn a_pending_activation_token_is_destroyed_with_its_window() {
    gtk4::init().expect("gtk init");
    let display = gtk4::gdk::Display::default().expect("a default GDK display");
    let config = CoreConfig::default_config();
    let app = App::init(&display, "io.airely.rustty", &config)
        .expect("winproto App::init must not error on a live display");
    assert_eq!(
        app.protocol(),
        Protocol::Wayland,
        "this test requires the Wayland arm"
    );

    let window = gtk4::Window::new();
    window.set_default_size(240, 120);
    window.present();
    let ctx = glib::MainContext::default();
    assert!(
        pump_until(&ctx, 3000, || window.surface().is_some()),
        "the test window never acquired a Wayland surface within ~3s"
    );

    // Positive control: with the window alive, the compositor answers the token.
    let mut win = Window::init(&app, &window).expect("wayland Window::init on the live surface");
    let before = activation_done_count(&app);
    win.set_urgent(true).expect("set_urgent(true)");
    assert!(
        pump_until(&ctx, 10_000, || activation_done_count(&app) > before),
        "precondition: the compositor answers an activation token"
    );

    // The same request, but the window is torn down before the reply is pumped.
    let before = activation_done_count(&app);
    win.set_urgent(true).expect("set_urgent(true)");
    win.deinit();
    // Twice the time the positive control needed at most, bounded.
    let arrived = pump_until(&ctx, 3000, || activation_done_count(&app) > before);
    assert!(
        !arrived,
        "a `done` reached the token of a window that was already torn down \
         (before={before}, after={})",
        activation_done_count(&app)
    );

    window.destroy();
}
