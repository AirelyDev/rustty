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
//!
//! ONE HEADED TEST PER BINARY (structural, verified empirically on the VM):
//! `gtk4::init()` binds GTK to the calling thread, and libtest runs every `#[test]`
//! on its own spawned thread — even under `--test-threads=1`. A second
//! `gtk4::init()` in the same process therefore panics with
//! `"Attempted to initialize GTK from two different threads"` (or
//! `"Failed to acquire default main context"` when the two race). WS16 T2
//! established this and split the four x11 KATs one-per-binary at `a7fa3b7`; this
//! wayland file was MISSED and carried TWO `gtk4::init()` tests, so its second
//! test could never pass (audit row N3). WS19 T4 split
//! `wayland_set_urgent_dispatches_activation_token` out into
//! `tests/gtk_wayland_set_urgent.rs`.
//!
//! **DO NOT merge the two files back together** — that reintroduces the defect.
//! One `gtk4::init()` per test binary, always. `VERIFICATION_DEBT.md` V4's recipe
//! names the new binary.
//!
//! R5: gated `all(any(linux, freebsd), feature = "wayland")` (wayland implies gtk).
#![cfg(all(any(target_os = "linux", target_os = "freebsd"), feature = "wayland"))]
#![allow(non_snake_case)]

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
