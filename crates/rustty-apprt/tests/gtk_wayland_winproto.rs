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

use gtk4::prelude::*;

use rustty_apprt::gtk::winproto::{App, Protocol};
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
