//! WAYLAND WINPROTO KAT — verifies `winproto::App::init` selects the Wayland arm on a
//! live Wayland compositor. This is the ground-truth check for the wayland winproto
//! port: reaching `Protocol::Wayland` proves the whole FFI chain worked on the real
//! compositor — downcast of the GDK display to `WaylandDisplay`, `wl_display_raw()`,
//! wrapping it in a `wayland-backend` foreign-display Backend + Connection, binding a
//! fresh `wl_registry`, and the two-phase roundtrip that enumerates/binds globals
//! (`wl_compositor` + `xdg_activation_v1` are present on GNOME/mutter; the KDE globals
//! are gracefully absent). If any step failed or crashed, `App::init` would return
//! `.none`/`Err` rather than the Wayland arm.
//!
//! R5: gated `all(target_os = "linux", feature = "wayland")` (wayland implies gtk).
#![cfg(all(target_os = "linux", feature = "wayland"))]
#![allow(non_snake_case)]

use gtk4::glib;
use gtk4::prelude::*;

use rustty_apprt::gtk::winproto::{App, Protocol, Window};
use rustty_config::Config as CoreConfig;

#[test]
#[ignore = "headed GTK + a live Wayland compositor; run (do NOT set GDK_BACKEND=x11): \
            cargo test -p rustty-apprt --features gtk,wayland --test gtk_wayland_winproto \
            -- --ignored --exact wayland_app_init_selects_wayland_arm"]
fn wayland_app_init_selects_wayland_arm() {
    gtk4::init().expect("gtk init");
    let display = gtk4::gdk::Display::default().expect("a default GDK display");
    let config = CoreConfig::default_config();
    let app = App::init(&display, "io.airely.rustty", &config)
        .expect("winproto App::init must not error on a live display");

    assert_eq!(
        app.protocol(),
        Protocol::Wayland,
        "on a live Wayland session, winproto must select the Wayland arm — proving the \
         WaylandDisplay downcast + wl_display_raw + foreign-backend registry bind + \
         two-phase roundtrip all succeeded on the real compositor without crashing"
    );
}

/// Read the wayland arm's post-init dispatch counter (`App::activation_done_count`),
/// the number of `xdg_activation_token_v1.done` replies the [`WlQueueSource`] GSource
/// has cranked. Panics if the arm is not Wayland (the caller asserts that first).
fn activation_done_count(app: &App) -> u32 {
    match app {
        App::Wayland(wp) => wp.activation_done_count(),
        _ => panic!("expected the Wayland winproto arm"),
    }
}

/// Drain every currently-ready GLib source (GDK's Wayland event source that reads the
/// shared `wl_display` fd AND our `WlQueueSource`) without blocking.
fn drain_main_context(ctx: &glib::MainContext) {
    while ctx.iteration(false) {}
}

/// RUNTIME EVIDENCE FOR THE `set_urgent` ASYNC-TOKEN DISPATCH SEAM.
///
/// `winproto/wayland/queue_source.rs`'s `WlQueueSource` integrates the registry
/// [`Connection`]'s PRIVATE libwayland queue into the GLib main loop so that events
/// arriving AFTER `App::init`'s roundtrip still dispatch. The one with a user-visible
/// effect is `xdg_activation_token_v1.done`: the reply carrying the token that
/// `Window::set_urgent(true)` needs to call `xdg_activation_v1.activate` and flash the
/// taskbar (ghostty gets this for free because its registry rides GDK's default
/// `wl_display` queue, `Globals.zig:82`; the wayland-rs port must crank its own queue).
///
/// This KAT realizes a top-level window on the live compositor, requests urgency, then
/// pumps the main loop and asserts the compositor's token `done` actually dispatched
/// through the GSource (observed via `App::activation_done_count`). That is direct
/// runtime proof the whole seam executes end-to-end: GDK reads the socket → libwayland
/// demarshals the `done` onto our private queue → our `GSource` runs `dispatch_pending`
/// → the token handler fires `activate`. Compositors issue `xdg_activation` tokens on
/// request (verified on headless sway/wlroots; mutter implements the same protocol), so
/// the `done` is observable here even though the taskbar flash itself is governed by
/// the compositor's focus policy (`activate` is best-effort by design).
#[test]
#[ignore = "headed GTK + a live Wayland compositor; run (do NOT set GDK_BACKEND=x11): \
            cargo test -p rustty-apprt --features gtk,wayland --test gtk_wayland_winproto \
            -- --ignored --exact wayland_set_urgent_dispatches_activation_token"]
fn wayland_set_urgent_dispatches_activation_token() {
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

    // A realized top-level so the GDK `wl_surface` (and hence the winproto Window)
    // exists. A plain gtk Window is enough: `set_urgent` needs only the bound
    // `xdg_activation` global + this surface, not the rustty ApprtWindow config.
    let window = gtk4::Window::new();
    window.set_default_size(240, 120);
    window.present();

    let ctx = glib::MainContext::default();

    // Pump until the window acquires its Wayland surface (bounded — never hang).
    let mut realized = false;
    for _ in 0..600 {
        drain_main_context(&ctx);
        if window.surface().is_some() {
            realized = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(
        realized,
        "the test window never acquired a Wayland surface within ~3s"
    );

    let mut win = Window::init(&app, &window).expect("wayland Window::init on the live surface");

    let before = activation_done_count(&app);

    // Request urgency: create the activation token, set the surface, commit, flush.
    // The compositor replies asynchronously with `done` — which only dispatches if
    // the WlQueueSource cranks the private queue (the seam under test).
    win.set_urgent(true).expect("set_urgent(true)");

    // Pump until the token `done` dispatches (bounded ~10s so a broken seam FAILS
    // rather than hangs). Compositors reply promptly (sway: <1s).
    let mut after = before;
    for _ in 0..2000 {
        drain_main_context(&ctx);
        after = activation_done_count(&app);
        if after > before {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }

    assert!(
        after > before,
        "the xdg_activation token `done` reply never dispatched: the WlQueueSource GSource \
         did not crank the private wayland queue, so Window::set_urgent's activation would \
         never complete (before={before}, after={after})"
    );

    win.deinit();
    window.destroy();
}
