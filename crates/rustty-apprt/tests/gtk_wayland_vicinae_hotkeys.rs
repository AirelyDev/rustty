//! `P3-W11 T4` (F-W3/F-W4) — the `vicinae_hotkey_v1` winproto arm under a live
//! compositor.
//!
//! `vicinae_hotkey_v1` is an unratified, testing-phase protocol authored by the
//! vicinae launcher project. No mainstream compositor — mutter, KWin, wlroots —
//! implements it today, so on every host available to this project the registry
//! never advertises `vicinae_hotkey_manager_v1`. What this KAT asserts is therefore
//! the GRACEFUL-ABSENCE half, and it asserts it as a POSITIVE fact about the
//! fallback, not as "nothing crashed":
//!
//!   * the manager global is absent, so
//!   * [`App::bind_global_shortcuts`] returns **false** — which is exactly what
//!     makes `GlobalShortcuts::refresh` fall through to the XDG desktop portal, and
//!   * no hotkey entries exist afterwards (`hotkeys_summary() == (0, 0)`).
//!
//! If any compositor ever DOES advertise the global, the first assertion flips and
//! this test fails loudly — which is the correct outcome, because the wire half
//! would then be live and would need the real gate below.
//!
//! # VERIFICATION DEBT (deliberate, with a recipe)
//!
//! The positive path — `bound` / `pressed` / `denied` / `revoked` over the wire —
//! is NOT verified by anything here, and is not pretended to be. The recipe, from
//! `P3-W8 T7 Step 4`: stand up a minimal fake compositor in a `#[test]`-local
//! `wayland-server` (0.31, the sibling of the vendored `wayland-client`)
//! advertising `vicinae_hotkey_manager_v1` on its own socket, then point the client
//! at it with `WAYLAND_DISPLAY`. That is genuinely runnable and is ~a day of
//! scaffolding; it is scheduled as its OWN task rather than folded in here. The
//! `#[ignore]`d marker test below keeps the debt visible in the test list.
//!
//! ONE HEADED TEST PER BINARY: its own file (G-GTKINIT).
//!
//! R5: gated `all(any(linux, freebsd), rustty_gtk_wayland)`.
#![cfg(all(any(target_os = "linux", target_os = "freebsd"), rustty_gtk_wayland))]

use rustty_apprt::gtk::class::global_shortcuts::GlobalShortcuts;
use rustty_apprt::gtk::winproto::{App, Protocol};
use rustty_config::Config as CoreConfig;

#[test]
#[ignore = "headed GTK + a live Wayland session; run with XDG_RUNTIME_DIR/WAYLAND_DISPLAY set"]
fn vicinae_hotkeys_absent_compositor_falls_back_to_the_portal() {
    gtk4::init().expect("gtk init");
    let display = gtk4::gdk::Display::default().expect("a default GDK display");

    let mut config = CoreConfig::default_config();
    config
        .keybind
        .set
        .parse_and_put(b"global:super+q=new_window")
        .expect("keybind parses");

    let mut app = App::init(&display, "io.airely.rustty", &config).expect("winproto App::init");
    assert_eq!(
        app.protocol(),
        Protocol::Wayland,
        "this test requires the Wayland arm"
    );

    let shortcuts: GlobalShortcuts = GlobalShortcuts::new();

    // The load-bearing assertion: the arm reports "no native mechanism", which is
    // what routes `GlobalShortcuts::refresh` to the XDG portal. A test that only
    // checked "did not panic" would pass with `bind_global_shortcuts` hard-wired to
    // `true` and every global keybind silently dead.
    let bound = app.bind_global_shortcuts(&shortcuts, &config);
    assert!(
        !bound,
        "no compositor implements vicinae_hotkey_v1 today, so the winproto arm must \
         decline and the portal path must run. If this now FAILS, a compositor \
         started advertising vicinae_hotkey_manager_v1 and the wire half needs the \
         fake-compositor gate (see the module docs)."
    );

    let (entries, live) = match &app {
        App::Wayland(wp) => wp.hotkeys_summary(),
        _ => unreachable!("asserted Wayland above"),
    };
    eprintln!("vicinae hotkeys: bound={bound} entries={entries} live={live}");
    assert_eq!(
        (entries, live),
        (0, 0),
        "declining to bind must leave no hotkey entries behind"
    );

    // `clear_global_shortcuts` on a never-bound arm is a no-op, and `close()` calls
    // it unconditionally on every refresh — so it must be safe here.
    app.clear_global_shortcuts();
    app.deinit();
}

// The wire half's verification-debt marker lives in the LIBRARY's test list
// (`src/gtk/winproto/wayland/hotkeys.rs`,
// `hotkeys_wire_path_bound_pressed_denied_revoked`), not here: this file is a
// headed binary run with `--ignored`, and a second `#[ignore]`d test in it would be
// executed (and fail) by that very invocation.
