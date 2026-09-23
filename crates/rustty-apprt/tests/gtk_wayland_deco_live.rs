//! WAYLAND LIVE DECORATION-MODE KAT — a compositor decoration-mode change made AFTER
//! the window exists must be honoured (WS19 T9, audit row O7).
//!
//! # The bug this pins
//!
//! `winproto/wayland.rs`'s `Window` used to SNAPSHOT `Globals.state.default_deco_mode`
//! (and the KDE-decoration-manager presence, and the blur capability) in `init`. The
//! reference does not: it holds `globals: *Globals` and dereferences it at every
//! `getDecorationMode` (`wayland.zig:303`), `clientSideDecorationEnabled` (`:222`) and
//! `syncBlur` (`:252`) call. Since `WlQueueSource` landed, a live
//! `org_kde_kwin_server_decoration_manager.default_mode` event IS dispatched into
//! `Globals` — it was simply never read again.
//!
//! # What is live here, and what is injected — stated plainly
//!
//! This test runs against a REAL compositor (nested KWin 6.7.3) with a REAL realized
//! GTK window and a REAL `winproto::wayland::Window` over the actual protocol:
//!
//!   * the initial mode is read from the wire — KWin advertises
//!     `org_kde_kwin_server_decoration_manager` with `default_mode = Server`, and the
//!     test asserts the port reports server-side decorations as a result;
//!   * the mode CHANGE is injected by writing the exact field the `default_mode`
//!     dispatch handler writes (`App::test_inject_default_deco_mode`, taking the raw
//!     protocol uint), then the same read path is re-exercised on the live window.
//!
//! **Why the change is injected rather than provoked on the wire.** MEASURED, not
//! assumed: no compositor available on the verification host can re-advertise
//! `default_mode` after bind.
//!
//!   * KWin 6.7.3 sends it exactly once per bound resource. The test still PERFORMS the
//!     real provocation (rewrite `kwinrc` to name a missing decoration plugin, then
//!     `org.kde.KWin.reconfigure` over D-Bus) and reports what happened; under
//!     `WAYLAND_DEBUG=1` the trace shows exactly two `default_mode(2)` events, both at
//!     bind time, and none after the reconfigure.
//!   * sway/wlroots calls `wlr_server_decoration_manager_set_default_mode` once at
//!     startup.
//!   * mutter advertises no KDE decoration manager at all.
//!
//! So the RE-READ (what audit row O7 is about) is exercised end-to-end; the
//! compositor-originated *transport* of a post-bind change is the residual, recorded in
//! `.port/phase-2/VERIFICATION_DEBT.md` with a runnable recipe. This comment is the
//! honest boundary — do not upgrade it to "verified on the wire".
//!
//! # Falsifiability
//!
//! * The initial assertion is `csd == false`, which under `window-decoration=auto` is
//!   reachable ONLY when the KDE decoration manager is present AND its default mode is
//!   `Server` (`wayland.zig:219-223`). On mutter — no KDE manager — the initial value is
//!   `true` and the test fails at the first assertion instead of passing vacuously. So
//!   the test also proves it is running somewhere the distinction exists.
//! * DEMONSTRATED FAIL ARM: restoring the snapshot fields (the pre-WS19-T9 code) leaves
//!   `csd == false` after the change and the test FAILS. Recorded in the WS19 T9 commit
//!   message.
//! * The injection is not self-fulfilling: it writes `Globals`, while the assertion
//!   reads `Window::client_side_decoration_enabled()`. Those are connected only by the
//!   live re-read this task added — which is precisely what is under test.
//! * The final assertion is bounded, so a never-honoured change FAILS rather than hangs.
//!
//! ONE HEADED TEST PER BINARY: its own file (G-GTKINIT; see the module docs of
//! `gtk_wayland_set_urgent.rs`).
//!
//! RUN: needs a NESTED KWin, never the primary session. See
//! `.port/tools/gates/ws19_wayland_deco_live_gate.sh`, which brings up
//! `kwin_wayland --socket wayland-kwinkat`, points `XDG_CONFIG_HOME` at a throwaway
//! `kwinrc`, runs this test, and tears the compositor down.
//!
//! R5: gated `all(any(linux, freebsd), rustty_gtk_wayland)` (wayland implies gtk).
#![cfg(all(any(target_os = "linux", target_os = "freebsd"), rustty_gtk_wayland))]

use gtk4::glib;
use gtk4::prelude::*;

use rustty_apprt::gtk::winproto::{App, Protocol, Window};
use rustty_config::Config as CoreConfig;

/// Drain every currently-ready GLib source (GDK's Wayland source AND our
/// `WlQueueSource`) without blocking.
fn drain(ctx: &glib::MainContext) {
    while ctx.iteration(false) {}
}

/// Pump until `done` or the bound elapses. Returns whether it became true, so a
/// timeout surfaces as a failed assertion and never as a silent pass or a hang.
fn pump_until(ctx: &glib::MainContext, ms: u64, mut done: impl FnMut() -> bool) -> bool {
    let start = std::time::Instant::now();
    while start.elapsed() < std::time::Duration::from_millis(ms) {
        drain(ctx);
        if done() {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    done()
}

/// Ask the nested KWin to re-read its config. The gate has already rewritten `kwinrc`.
fn kwin_reconfigure() {
    let out = std::process::Command::new("dbus-send")
        .args([
            "--session",
            "--type=method_call",
            "--dest=org.kde.KWin",
            "/KWin",
            "org.kde.KWin.reconfigure",
        ])
        .output()
        .expect("dbus-send must be installed");
    assert!(
        out.status.success(),
        "org.kde.KWin.reconfigure failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Rewrite the throwaway `kwinrc` so KWin loses its decoration plugin, which is what
/// makes it advertise `default_mode = Client`.
fn point_kwin_at_a_missing_decoration_plugin() {
    let cfg = std::env::var("XDG_CONFIG_HOME").expect("the gate sets XDG_CONFIG_HOME");
    let path = std::path::Path::new(&cfg).join("kwinrc");
    let prior = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        !prior.contains("library=rustty.no.such.decoration"),
        "kwinrc already names the missing plugin: a stale config would make the \
         'before' state wrong and the test vacuous"
    );
    std::fs::write(
        &path,
        format!("{prior}\n[org.kde.kdecoration2]\nlibrary=rustty.no.such.decoration\ntheme=\n"),
    )
    .expect("write kwinrc");
}

#[test]
#[ignore = "headed GTK + a NESTED KWin; run via .port/tools/gates/ws19_wayland_deco_live_gate.sh"]
fn wayland_window_honours_a_live_decoration_mode_change() {
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
        pump_until(&ctx, 3_000, || window.surface().is_some()),
        "the test window never acquired a Wayland surface within ~3s"
    );

    let win = Window::init(&app, &window).expect("wayland Window::init on the live surface");

    // BEFORE. `false` here means: the KDE decoration manager is present AND its
    // default mode is Server, i.e. the compositor draws the decorations
    // (`wayland.zig:219-223`). This is the assertion that proves the test is running
    // somewhere the distinction exists — it fails on mutter.
    let before = win.client_side_decoration_enabled();
    assert!(
        !before,
        "expected server-side decorations before the change (KDE decoration manager \
         present, default_mode=Server). Got csd={before} — either this is not KWin, or \
         KWin has no decoration plugin, and the change below would be unobservable"
    );

    // (1) The REAL provocation, performed and REPORTED (not asserted): ask KWin to drop
    // its decoration plugin and re-read its config. On KWin 6.7.3 this does not
    // re-broadcast `default_mode` — see the module docs. Recorded so that if a future
    // compositor DOES re-broadcast, this run prints it and the residual can be closed.
    point_kwin_at_a_missing_decoration_plugin();
    kwin_reconfigure();
    let after_reconfigure = pump_until(&ctx, 3_000, || win.client_side_decoration_enabled());
    eprintln!(
        "compositor provocation: after kwinrc rewrite + org.kde.KWin.reconfigure, \
         csd={after_reconfigure} (KWin 6.7.3 is expected NOT to re-advertise default_mode; \
         if this is true, the wire half of the residual is now covered)"
    );

    // (2) THE RE-READ UNDER TEST: write `Globals.state.default_deco_mode` exactly as the
    // `default_mode` dispatch handler would, with the raw protocol uint for Client (1),
    // then re-exercise the live window's read path.
    match &app {
        App::Wayland(wp) => wp.test_inject_default_deco_mode(1),
        _ => unreachable!("asserted Wayland above"),
    }

    let flipped = pump_until(&ctx, 5_000, || win.client_side_decoration_enabled());
    assert!(
        flipped,
        "the decoration-mode change was never honoured: csd is still {before} after \
         `Globals.state.default_deco_mode` became Client. `Window` is still reading an \
         init-time snapshot (audit row O7)"
    );

    // And the converse, so the assertion cannot be satisfied by a stuck `true`: put the
    // mode back to Server (2) and the port must report server-side decorations again.
    match &app {
        App::Wayland(wp) => wp.test_inject_default_deco_mode(2),
        _ => unreachable!(),
    }
    let restored = pump_until(&ctx, 5_000, || !win.client_side_decoration_enabled());
    assert!(
        restored,
        "csd did not go back to false when the mode returned to Server: the read is not \
         tracking `Globals`, it is latching"
    );

    eprintln!("KAT PASS: live decoration-mode re-read honoured (csd false -> true -> false)");

    let mut win = win;
    win.deinit();
    window.destroy();
}
