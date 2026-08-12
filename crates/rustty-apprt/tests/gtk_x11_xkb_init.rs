//! X11 WINPROTO KAT — the XKB extension really initialized in `App::init`.
//!
//! PORTS the verification of ghostty `src/apprt/gtk/winproto/x11.zig:71-101`, which
//! runs `XkbQueryExtension` + `XkbSelectEventDetails` in `App.init` and keeps the
//! returned `base_event_code` (`x11.zig:28`).
//!
//! ONE HEADED TEST PER BINARY: see the module docs of `gtk_x11_winproto.rs` —
//! `gtk4::init()` binds GTK to the calling thread and libtest gives each `#[test]`
//! its own thread, so a second headed test in the same process panics.
//!
//! RUN (Fedora VM, live GNOME session, X server via rootless XWayland):
//!
//! ```sh
//! export XDG_RUNTIME_DIR=/run/user/$(id -u)
//! export DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/$(id -u)/bus
//! export DISPLAY=:0
//! export XAUTHORITY=$(ls /run/user/$(id -u)/.mutter-Xwaylandauth.* | head -1)
//! export GDK_BACKEND=x11 LIBGL_ALWAYS_SOFTWARE=1
//! cargo test -p rustty-apprt --features gtk,x11 --test gtk_x11_xkb_init -- --ignored
//! ```
//!
//! R5: gated `all(any(linux, freebsd), feature = "x11")`.
#![cfg(all(any(target_os = "linux", target_os = "freebsd"), feature = "x11"))]
#![allow(non_snake_case)]

use rustty_apprt::gtk::winproto::{App, Protocol};
use rustty_config::Config as CoreConfig;

/// The XKB extension really initialized: `x11.zig:71-101` runs `XkbQueryExtension`
/// and `XkbSelectEventDetails` in `App.init` and stores the returned
/// `base_event_code`. A real X server always assigns a base event code above the
/// reserved low codes (0 = error, 1 = reply), so a value `> 1` here is proof the
/// query actually ran and succeeded — NOT that the field kept its `0` initializer.
///
/// NEGATIVE CONTROL (what makes this falsifiable): with the `XkbQueryExtension`
/// call removed, `base_event_code` stays at its `0` initializer and this assertion
/// fails with `got 0`. With `GDK_BACKEND` unset (native Wayland) the arm is not
/// selected at all and the `Protocol::X11` assertion fails with `left: None`.
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
    eprintln!("KAT: XkbQueryExtension base_event_code = {base}");
}
