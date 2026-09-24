//! X11 WINPROTO KAT — `App::event_mods` on an IDLE X event queue.
//!
//! PORTS the verification of ghostty `src/apprt/gtk/winproto/x11.zig:114-159`
//! `App.eventMods`, whose first act is the `XEventsQueued(QueuedAfterReading) == 0`
//! guard that returns null (`x11.zig:134-137`).
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
//! cargo test -p rustty-apprt --features gtk,x11 --test gtk_x11_event_mods -- --ignored
//! ```
//!
//! R5: gated `all(any(linux, freebsd), rustty_gtk_x11)`.
#![cfg(all(any(target_os = "linux", target_os = "freebsd"), rustty_gtk_x11))]
#![allow(non_snake_case)]

use rustty_apprt::gtk::winproto::{App, Protocol};
use rustty_config::Config as CoreConfig;

/// `eventMods` on an IDLE queue: `x11.zig:134-137` returns null when
/// `XEventsQueued(QueuedAfterReading)` is 0, so the caller falls back to the plain
/// GDK modifier translation.
///
/// NEGATIVE CONTROL (what makes this falsifiable, and it is NOT the return value):
/// this KAT's load-bearing assertion is that the call **terminates**. `XPeekEvent`
/// BLOCKS FOREVER on an empty queue, so dropping the `XEventsQueued` guard turns
/// this test into an infinite hang that the harness's `timeout` kills — a hard,
/// observable failure. (Confirmed by construction: the guard is the only thing
/// standing between this call and a blocking `XPeekEvent`.) The returned-value half
/// pins that an idle queue still yields the `key::translate_mods` fallback, i.e.
/// WS16 changed no behavior when nothing is pending.
///
/// The behavior-CHANGING half (`Some(mods)` on a pending `XkbStateNotify`) cannot be
/// synthesized without racing the X server, so it is verified by the live keypress
/// check recorded in the WS16 plan (Task 3 Step 6 / Task 6).
#[test]
#[ignore = "headed GTK + a live X server; run under GDK_BACKEND=x11 — see the module docs"]
fn x11_event_mods_returns_none_on_idle_queue() {
    gtk4::init().expect("gtk init");
    let display = gtk4::gdk::Display::default().expect("a default GDK display");
    let config = CoreConfig::default_config();
    let app = App::init(&display, "io.airely.rustty", &config)
        .expect("winproto App::init must not error on a live display");
    assert_eq!(app.protocol(), Protocol::X11, "must be on the X11 arm");

    // If the `XEventsQueued` guard were missing, this call would block forever.
    let mods = app.event_mods(None, gtk4::gdk::ModifierType::empty());
    assert_eq!(
        mods,
        rustty_vt::input::key_mods::Mods::default(),
        "with no pending XkbStateNotify the arm returns None and the dispatch falls \
         back to key::translate_mods, which maps an empty ModifierType to no mods"
    );
    eprintln!("KAT: event_mods on an idle queue returned and did not block");
}
