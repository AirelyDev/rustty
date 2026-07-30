//! WS17 V3 — KDE Wayland globals really bind under a live KWin.
//!
//! `VERIFICATION_DEBT.md` V3 recorded that the KDE-only Wayland globals — the
//! server-decoration manager (CSD/SSD) and the slide manager (quick-terminal slide
//! animation) — had never been exercised against a real KWin, because no KDE was
//! available. This KAT closes that: it asserts the port's registry binding
//! (`winproto/wayland/globals.rs`, PORTS `Globals.zig`) actually acquires them from a
//! live KWin, and reports the whole bound set for the ledger.
//!
//! # Negative control (this is the part that makes it mean something)
//!
//! The SAME test on GNOME/mutter must FAIL, because mutter advertises neither KDE
//! global. `.port/tools/gates/ws17_kde_globals_gate.sh` runs both arms and requires the
//! mutter arm to fail — so a test that somehow passed everywhere would be caught.
//!
//! # A ledger correction this KAT encodes
//!
//! V3's text claimed KWin blur rides `org_kde_kwin_blur`. It does not: the oracle uses
//! `ext_background_effect_v1` (`ghostty/src/apprt/gtk/winproto/wayland.zig:156`) and so
//! does rustty (`Tag::ExtBackgroundEffect`). `org_kde_kwin_blur` appears nowhere in
//! either tree. The blur assertion below is therefore about `ext_background_effect`, and
//! it is deliberately REPORTED rather than required: whether a given KWin build
//! advertises that (fairly new) protocol is a property of the compositor, not of the
//! port, and hard-requiring it would make this KAT fail for the wrong reason.
//!
//! ONE HEADED TEST PER BINARY: its own file (G-GTKINIT).
//!
//! RUN via `.port/tools/gates/ws17_kde_globals_gate.sh`.
//!
//! R5: gated `all(target_os = "linux", feature = "wayland")`.
#![cfg(all(target_os = "linux", feature = "wayland"))]

use rustty_apprt::gtk::winproto::{App, Protocol};
use rustty_config::Config as CoreConfig;

#[test]
#[ignore = "headed GTK + a live KWin; run via .port/tools/gates/ws17_kde_globals_gate.sh"]
fn kde_globals_bind_under_kwin() {
    gtk4::init().expect("gtk init");
    let display = gtk4::gdk::Display::default().expect("a default GDK display");
    let config = CoreConfig::default_config();
    let app = App::init(&display, "io.airely.rustty", &config).expect("winproto App::init");
    assert_eq!(
        app.protocol(),
        Protocol::Wayland,
        "this test requires the Wayland arm"
    );

    let (kde_deco, kde_slide, ext_bg_effect, blur_cap, xdg_activation, default_mode) = match &app {
        App::Wayland(wp) => wp.bound_globals_summary(),
        _ => unreachable!("asserted Wayland above"),
    };

    // Reported verbatim so the gate can copy it into the evidence file.
    eprintln!(
        "bound globals: kde_decoration_manager={kde_deco} kde_slide_manager={kde_slide} \
         ext_background_effect={ext_bg_effect} blur_capability={blur_cap} \
         xdg_activation={xdg_activation} default_deco_mode_raw={default_mode:?}"
    );

    // REQUIRED: the decoration manager. This is the CSD/SSD path V3's "decorations
    // polish" is about, and it is exactly what mutter cannot satisfy.
    assert!(
        kde_deco,
        "org_kde_kwin_server_decoration_manager did not bind: either this is not KWin, \
         or the port's registry binding missed it"
    );

    // REPORTED, NOT REQUIRED: `org_kde_kwin_slide_manager` (the quick-terminal slide
    // animation). MEASURED on KWin 6.7.3 run bare/nested: it is NOT advertised, because
    // the global is registered by the `slidingpopups` builtin effect, and a nested
    // kwin_wayland outside a Plasma session cannot load it —
    // `org.kde.kwin.Effects.loadEffect("slidingpopups")` returns `false`. Requiring it
    // here would make this KAT fail for a compositor-configuration reason that says
    // nothing about the port, so the residual is tracked in VERIFICATION_DEBT.md V3
    // instead. If a full Plasma session ever runs this, `kde_slide_manager=true` in the
    // line above closes it.
    if kde_slide {
        eprintln!(
            "NOTE: org_kde_kwin_slide_manager DID bind — the V3 slide-manager residual \
             can now be closed"
        );
    } else {
        eprintln!(
            "NOTE: org_kde_kwin_slide_manager did NOT bind (expected on a bare nested \
             kwin_wayland: the slidingpopups effect that registers it cannot load); V3's \
             slide residual stands"
        );
    }

    // Anti-vacuity: a compositor advertising the KDE decoration manager MUST also have
    // sent `default_mode` (the manager emits it on bind, `server-decoration.xml`), so a
    // bound manager with no mode would mean the event handler never ran.
    assert!(
        default_mode.is_some(),
        "the decoration manager bound but default_mode never arrived: the bind succeeded \
         and the event dispatch did not, which would leave decoration_mode() on its \
         Client fallback forever"
    );

    // xdg_activation is NOT KDE-specific; every modern compositor has it. Asserting it
    // here proves the registry walk is healthy in general, so a failure of the two KDE
    // assertions above cannot be blamed on a wholly broken bind path.
    assert!(
        xdg_activation,
        "xdg_activation_v1 did not bind — the registry walk itself is broken, so this \
         run says nothing about the KDE globals"
    );

    eprintln!(
        "KAT PASS: org_kde_kwin_server_decoration_manager bound under a live KWin \
         (default_mode={default_mode:?}), ext_background_effect={ext_bg_effect}"
    );
}
