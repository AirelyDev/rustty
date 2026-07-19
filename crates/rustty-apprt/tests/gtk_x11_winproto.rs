//! X11 WINPROTO KAT — verifies `winproto::App::init` selects the X11 arm on a
//! live X11 (XWayland) display. This is the ground-truth check for the wayland winproto
//! port: reaching `Protocol::X11` proves the whole FFI chain worked on the real
//! compositor — downcast of the GDK display to `WaylandDisplay`, `wl_display_raw()`,
//! wrapping it in a `wayland-backend` foreign-display Backend + Connection, binding a
//! fresh `wl_registry`, and the two-phase roundtrip that enumerates/binds globals
//! (`wl_compositor` + `xdg_activation_v1` are present on GNOME/mutter; the KDE globals
//! are gracefully absent). If any step failed or crashed, `App::init` would return
//! `.none`/`Err` rather than the X11 arm.
//!
//! R5: gated `all(target_os = "linux", feature = "x11")` (wayland implies gtk).
#![cfg(all(target_os = "linux", feature = "x11"))]
#![allow(non_snake_case)]

use gtk4::prelude::*;

use rustty_apprt::gtk::winproto::{App, Protocol};
use rustty_config::Config as CoreConfig;

#[test]
#[ignore = "headed GTK + a live X11 (XWayland) display; run (do NOT set GDK_BACKEND=x11): \
            cargo test -p rustty-apprt --features gtk,x11 --test gtk_wayland_winproto \
            -- --ignored --exact x11_app_init_selects_x11_arm"]
fn x11_app_init_selects_x11_arm() {
    gtk4::init().expect("gtk init");
    let display = gtk4::gdk::Display::default().expect("a default GDK display");
    let config = CoreConfig::default_config();
    let app = App::init(&display, "io.airely.rustty", &config)
        .expect("winproto App::init must not error on a live display");

    assert_eq!(
        app.protocol(),
        Protocol::X11,
        "on a live X11 (XWayland) session, winproto must select the X11 arm — proving the \
         WaylandDisplay downcast + wl_display_raw + foreign-backend registry bind + \
         two-phase roundtrip all succeeded on the real compositor without crashing"
    );
}
