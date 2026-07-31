//! X11 WINPROTO KAT — `Window::add_subprocess_env` publishes `WINDOWID`.
//!
//! PORTS the verification of ghostty `src/apprt/gtk/winproto/x11.zig:320-329`
//! `Window.addSubprocessEnv`, which writes the window's XID in DECIMAL as
//! `WINDOWID` into the child environment.
//!
//! `WINDOWID` is NOT product-namespaced (R2): the reference spells it `WINDOWID`
//! because that is the long-standing X11 ecosystem convention, not a Ghostty name.
//!
//! ONE HEADED TEST PER BINARY: see the module docs of `gtk_x11_winproto.rs`.
//!
//! RUN (Fedora VM, live GNOME session, X server via rootless XWayland):
//!
//! ```sh
//! export XDG_RUNTIME_DIR=/run/user/$(id -u)
//! export DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/$(id -u)/bus
//! export DISPLAY=:0
//! export XAUTHORITY=$(ls /run/user/$(id -u)/.mutter-Xwaylandauth.* | head -1)
//! export GDK_BACKEND=x11 LIBGL_ALWAYS_SOFTWARE=1
//! cargo test -p rustty-apprt --features gtk,x11 --test gtk_x11_windowid -- --ignored
//! ```
//!
//! R5: gated `all(target_os = "linux", feature = "x11")`.
#![cfg(all(target_os = "linux", feature = "x11"))]
#![allow(non_snake_case)]

use gtk4::prelude::*;

use rustty_apprt::gtk::winproto::{App, Protocol, Window as WinprotoWindow};
use rustty_config::Config as CoreConfig;

/// `addSubprocessEnv` (`x11.zig:320-329`): the X11 arm publishes the window's XID as
/// `WINDOWID`, in decimal, into the child environment. Verified against the SAME XID
/// GDK reports for the realized surface.
///
/// NEGATIVE CONTROL (what makes this falsifiable): the map handed in is EMPTY, so
/// `env.get("WINDOWID")` is `None` unless the arm inserted it — the pre-WS16 code
/// had no `add_subprocess_env` at all and this file did not compile. And the
/// expected value is not a constant: it is read independently from
/// `X11Surface::xid()`, so an arm that inserted a wrong/stale/zero id still fails.
#[test]
#[ignore = "headed GTK + a live X server; run under GDK_BACKEND=x11 — see the module docs"]
fn x11_window_add_subprocess_env_publishes_windowid() {
    gtk4::init().expect("gtk init");
    let display = gtk4::gdk::Display::default().expect("a default GDK display");
    let config = CoreConfig::default_config();
    let app = App::init(&display, "io.airely.rustty", &config)
        .expect("winproto App::init must not error on a live display");
    assert_eq!(app.protocol(), Protocol::X11, "must be on the X11 arm");

    // A realized toplevel is required: the XID only exists once GDK has a surface.
    let window = gtk4::Window::new();
    window.present();
    // Pump the main loop until the surface exists (bounded, never a spin-forever).
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while window.surface().is_none() && std::time::Instant::now() < deadline {
        while glib::MainContext::default().iteration(false) {}
    }
    let surface = window
        .surface()
        .expect("the presented window must have a surface");
    let xid = surface
        .downcast_ref::<gdk4_x11::X11Surface>()
        .expect("under GDK_BACKEND=x11 the surface is an X11Surface")
        .xid();
    assert!(xid != 0, "a realized X11 surface must have a non-zero XID");

    let mut wp = WinprotoWindow::init(&app, &window).expect("winproto Window::init");
    // P3-W7 byte-typed the subprocess env map (`EnvMap` is Zig byte-slices, so
    // rustty spells it `HashMap<Vec<u8>, Vec<u8>>`); this `x11`-feature-gated
    // test was not updated with it and has not compiled since. Found and fixed
    // in P3-W8 T5 while building the GTK arm on the Linux VM.
    let mut env: std::collections::HashMap<Vec<u8>, Vec<u8>> = std::collections::HashMap::new();
    wp.add_subprocess_env(&mut env)
        .expect("add_subprocess_env must succeed on the X11 arm");

    assert_eq!(
        env.get(b"WINDOWID".as_slice()).map(Vec::as_slice),
        Some(xid.to_string().as_bytes()),
        "WINDOWID must be the window's XID in decimal (x11.zig:320-329)"
    );
    eprintln!("KAT: WINDOWID = {xid} (matches X11Surface::xid())");

    wp.deinit();
    window.destroy();
}
