//! X11 WINPROTO KATs — the live ground truth for the `gtk/winproto/x11.rs` arm.
//!
//! Reaching `Protocol::X11` proves the whole FFI chain worked against a real X
//! server: the downcast of the GDK display to `gdk4_x11::X11Display`, the
//! `xdisplay()` hinge, `g_set_prgname` + `gdk_x11_display_set_program_class`, the
//! XKB extension init, and the atom interning. If any step failed or crashed,
//! `App::init` would land on `.none`/`Err` instead of the X11 arm.
//!
//! RUN (on the Fedora VM, inside its live GNOME session; XWayland provides the X
//! server — `GDK_BACKEND=x11` is REQUIRED, it is what makes GTK open a
//! `GdkX11Display` instead of the Wayland one):
//!
//! ```sh
//! export XDG_RUNTIME_DIR=/run/user/$(id -u)
//! export DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/$(id -u)/bus
//! export DISPLAY=:0
//! export XAUTHORITY=$(ls /run/user/$(id -u)/.mutter-Xwaylandauth.* | head -1)
//! export GDK_BACKEND=x11 LIBGL_ALWAYS_SOFTWARE=1
//! cargo test -p rustty-apprt --features gtk,x11 --test gtk_x11_winproto -- --ignored
//! ```
//!
//! NEGATIVE CONTROL (what makes these KATs falsifiable): run the same binary with
//! `GDK_BACKEND` unset against the native Wayland display and every test here
//! FAILS at its `Protocol::X11` assertion with `left: None` — the union falls
//! through to the noop arm. So a pass is evidence the X11 arm really ran.
//!
//! R5: gated `all(target_os = "linux", feature = "x11")` (the `x11` feature implies
//! `gtk`).
#![cfg(all(target_os = "linux", feature = "x11"))]
#![allow(non_snake_case)]

use gtk4::prelude::*;

use rustty_apprt::gtk::winproto::{App, Protocol};
use rustty_config::Config as CoreConfig;

#[test]
#[ignore = "headed GTK + a live X server; run under GDK_BACKEND=x11 — see the module docs"]
fn x11_app_init_selects_x11_arm() {
    gtk4::init().expect("gtk init");
    let display = gtk4::gdk::Display::default().expect("a default GDK display");
    let config = CoreConfig::default_config();
    let app = App::init(&display, "io.airely.rustty", &config)
        .expect("winproto App::init must not error on a live display");

    assert_eq!(
        app.protocol(),
        Protocol::X11,
        "under GDK_BACKEND=x11 the winproto union must select the X11 arm — proving \
         the X11Display downcast + xdisplay() + set_program_class + atom intern all \
         succeeded against the real X server without crashing"
    );
}

/// The XKB extension really initialized: `x11.zig:71-101` runs `XkbQueryExtension`
/// and `XkbSelectEventDetails` in `App.init` and stores the returned
/// `base_event_code`. A real X server always assigns a non-zero base event code
/// (event codes 0 and 1 are reserved for errors/replies), so a non-zero value here
/// is proof the query actually ran and succeeded — not that the field defaulted.
#[test]
#[ignore = "headed GTK + a live X server; run under GDK_BACKEND=x11 — see the module docs"]
fn x11_app_init_initializes_xkb() {
    gtk4::init().expect("gtk init");
    let display = gtk4::gdk::Display::default().expect("a default GDK display");
    let config = CoreConfig::default_config();
    let app = App::init(&display, "io.airely.rustty", &config)
        .expect("winproto App::init must not error on a live display");

    assert_eq!(app.protocol(), Protocol::X11, "must be on the X11 arm");

    let base = app
        .x11_xkb_base_event_code()
        .expect("the X11 arm must expose its XKB base event code");
    assert!(
        base > 1,
        "XkbQueryExtension must have assigned a real base event code, got {base}"
    );
}
