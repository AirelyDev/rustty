//! WAYLAND `compute-size` MONITOR-CLAMP KAT — the **geometry effect** of
//! `004c88e41` (`.port/phase-2/VERIFICATION_DEBT.md` row **V20b**).
//!
//! WHAT IS UNDER TEST. `class::window::imp::toplevel_compute_size`, connected to
//! the realized `gdk::Toplevel`'s `compute-size` signal with `G_CONNECT_AFTER`
//! (`window.rs` `realize`). GTK's own `compute-size` handler clamps the window's
//! requested size to the **compositor-reported bounds** — the usable area of the
//! monitor the toplevel is on. Those bounds go stale the moment the window moves
//! to a different head, so the ported handler runs AFTER GTK's and re-asserts the
//! surface's CURRENT size whenever it exceeds the bounds. The net user-visible
//! effect: a window that has moved to a SMALLER monitor is not silently shrunk to
//! that monitor's size.
//!
//! WHY THIS NEEDS TWO HEADS — and why they can be virtual. The predicate only
//! fires when `w > bounds_w || h > bounds_h`, i.e. when the bounds SHRINK below
//! the window's live size. One fixed-size head can never produce that: the window
//! is clamped on its first map and thereafter always fits. TWO heads of
//! DIFFERENT sizes do, and a **headless sway** (`WLR_BACKENDS=headless`) supplies
//! exactly that with no DRM device, no seat, and its own `WAYLAND_DISPLAY` — so
//! the primary GNOME session is never touched (the discipline row V4 established).
//! `swaymsg` then moves the window between heads from OUTSIDE the client, which is
//! the point: the shrink decision must come from a real compositor, not from us.
//!
//! THE TWO ARMS (driven by `RUSTTY_V20B_ARM`, both run by
//! `.port/tools/gates/v20b_compute_size_clamp_gate.sh`):
//!
//! * `exceeds` — the window is realized at a size that FITS the big head, then
//!   moved to the small head, and the big head is then switched off. `w > bounds`
//!   afterwards, the handler re-asserts, and the surface must still be LARGER than
//!   the small head. Deleting the re-assertion makes this arm fail (negative
//!   control, run and recorded in the row: the surface comes back 800x600).
//! * `fits` — the window is realized SMALL enough to fit the small head, then
//!   moved there. `w <= bounds`, so the predicate returns false and the handler
//!   does nothing; the surface must keep its own size and must NOT be inflated to
//!   the head. This is the contrasting arm: without it, "the window is not the
//!   monitor's size" could just mean the handler never runs at all.
//!
//! ONE HEADED TEST PER BINARY — `gtk4::init()` binds GTK to the calling thread and
//! libtest runs each `#[test]` on its own thread, so a second init in the same
//! process panics. Both arms are the SAME `#[test]`, selected by env var, run in
//! separate processes. See `gtk_wayland_set_urgent.rs` for the full rationale.
//!
//! R5: gated `all(any(linux, freebsd), feature = "wayland")` (wayland implies gtk).
#![cfg(all(any(target_os = "linux", target_os = "freebsd"), feature = "wayland"))]

use std::time::{Duration, Instant};

use gio::prelude::*;
use gtk4::glib;
use gtk4::prelude::*;

use allocator_api2::alloc::Global;
use rustty_apprt::gtk::class::application::Application;
use rustty_apprt::gtk::class::window::Window;
use rustty_apprt::gtk::{self};
use rustty_core::app::App as CoreApp;

/// Pump the default main context until `pred` holds or `secs` elapse. Bounded so a
/// dead seam FAILS rather than hangs.
fn pump_until(secs: u64, mut pred: impl FnMut() -> bool) -> bool {
    let ctx = glib::MainContext::default();
    let deadline = Instant::now() + Duration::from_secs(secs);
    loop {
        while ctx.iteration(false) {}
        if pred() {
            return true;
        }
        if Instant::now() > deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Pump for a fixed wall-clock duration (used to settle after a compositor-side
/// move, where "nothing changed" is itself the observation).
fn pump_for(ms: u64) {
    let ctx = glib::MainContext::default();
    let deadline = Instant::now() + Duration::from_millis(ms);
    while Instant::now() < deadline {
        while ctx.iteration(false) {}
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Ask sway to move the focused window's workspace onto `output`, from OUTSIDE the
/// client. Returns sway's raw reply for the evidence log.
fn swaymsg(args: &[&str]) -> String {
    let out = std::process::Command::new("swaymsg")
        .args(args)
        .output()
        .expect("swaymsg must be on PATH for this KAT");
    format!(
        "swaymsg {:?} -> rc={:?} {}{}",
        args,
        out.status.code(),
        String::from_utf8_lossy(&out.stdout).trim(),
        String::from_utf8_lossy(&out.stderr).trim()
    )
}

/// One line of `name active WxH` per sway output — the raw JSON is hundreds of lines
/// and would bury the geometry this KAT is about.
fn sway_outputs_summary() -> String {
    let out = std::process::Command::new("sh")
        .arg("-c")
        .arg(
            "swaymsg -t get_outputs --raw | python3 -c \"import sys,json; \
             print(' '.join('%s:%s:%dx%d' % (o['name'], o['active'], \
             o['rect']['width'], o['rect']['height']) for o in json.load(sys.stdin)))\"",
        )
        .output()
        .expect("swaymsg/python3");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// THE V20b GEOMETRY KAT. See the module doc.
#[test]
#[ignore = "headed GTK + a live headless sway with TWO differently-sized outputs; \
            run via .port/tools/gates/v20b_compute_size_clamp_gate.sh"]
fn compute_size_reasserts_size_on_a_smaller_monitor() {
    let arm = std::env::var("RUSTTY_V20B_ARM").expect("RUSTTY_V20B_ARM must be set (exceeds|fits)");
    // The requested logical window size, supplied by the gate so the two arms
    // differ ONLY in this number (and so the value in play is visible in the log
    // rather than assumed).
    let want_w: i32 = std::env::var("RUSTTY_V20B_W")
        .expect("RUSTTY_V20B_W")
        .parse()
        .unwrap();
    let want_h: i32 = std::env::var("RUSTTY_V20B_H")
        .expect("RUSTTY_V20B_H")
        .parse()
        .unwrap();
    let small_w: i32 = std::env::var("RUSTTY_V20B_SMALL_W")
        .expect("small w")
        .parse()
        .unwrap();
    let small_h: i32 = std::env::var("RUSTTY_V20B_SMALL_H")
        .expect("small h")
        .parse()
        .unwrap();
    let small_out = std::env::var("RUSTTY_V20B_SMALL_OUTPUT").expect("small output name");
    let big_out = std::env::var("RUSTTY_V20B_BIG_OUTPUT").expect("big output name");
    eprintln!(
        "V20B arm={arm} requested={want_w}x{want_h} small_head={small_w}x{small_h} ({small_out})"
    );

    // ---- The real core/apprt App pair, exactly as `gtk_window_live.rs` stands it
    //      up: the ported `Window::realize` reaches `Application::default()` for
    //      the winproto, so the product wiring has to be real. ----
    rustty_apprt::init().expect("initialize rustty global state");
    let mut core_app: Box<CoreApp<gtk::App>> =
        CoreApp::<gtk::App>::create(Global).expect("core App::create");
    let core_ptr: *mut CoreApp<gtk::App> = &mut *core_app;
    let app = match gtk::App::init(core_ptr.cast()) {
        Ok(rt) => rt,
        Err(err) => match err {},
    };
    let application: Application = app.application().clone();
    application.upcast_ref::<gio::Application>().set_default();
    application
        .register(gio::Cancellable::NONE)
        .expect("register the application");
    let app_ptr: *mut std::ffi::c_void = std::ptr::from_ref(&app).cast_mut().cast();
    application.bind_rt_app(app_ptr);
    application.bind_core_app::<gtk::App>();
    gtk::register_resources();

    // Report what the compositor advertises, so a mis-provisioned head is caught
    // here rather than silently producing a green arm.
    let display = gtk4::gdk::Display::default().expect("a default GDK display");
    let monitors = display.monitors();
    let mut heads = Vec::new();
    for i in 0..monitors.n_items() {
        let m = monitors
            .item(i)
            .unwrap()
            .downcast::<gtk4::gdk::Monitor>()
            .unwrap();
        let g = m.geometry();
        heads.push((
            m.connector().map(|s| s.to_string()).unwrap_or_default(),
            g.width(),
            g.height(),
        ));
    }
    eprintln!("V20B heads advertised by the compositor: {heads:?}");
    assert!(
        heads.len() >= 2,
        "this KAT needs TWO heads; the compositor advertised {heads:?}"
    );

    // ---- The product path: the real ported `Window`, presented so `realize` runs
    //      and installs the G_CONNECT_AFTER `compute-size` trampoline. ----
    let window = Window::new(&application, Some("rustty-v20b"));
    window.set_default_size(want_w, want_h);
    window.present();

    let _keepalive =
        glib::timeout_add_local(Duration::from_millis(50), || glib::ControlFlow::Continue);

    assert!(
        pump_until(20, || window
            .surface()
            .is_some_and(|s| s.width() > 0 && s.height() > 0)),
        "the window never acquired a sized Wayland surface"
    );
    // Let the initial configure/ack settle.
    pump_for(1500);
    let surface = window.surface().expect("realized surface");
    let (w0, h0) = (surface.width(), surface.height());
    eprintln!("V20B before move: surface={w0}x{h0}");

    // ---- The compositor-side move onto the SMALL head. This is what makes the
    //      bounds shrink; it is issued from OUTSIDE the process, by the
    //      compositor, which is the whole point of the row. ----
    eprintln!(
        "V20B {}",
        swaymsg(&["move", "workspace", "to", "output", &small_out])
    );
    pump_for(1500);
    // …and the big head then LEAVES. On this stack (sway 1.11 / wlroots) the
    // compositor does not send `xdg_toplevel.configure_bounds`, so the bounds GDK
    // hands the handler are its fallback: the bounding box of the attached
    // monitors. Merely moving the window therefore does not shrink them — the
    // larger head has to go away for the bounds the window is judged against to
    // become the small head's. The window has still travelled from a 1920x1200
    // head to an 800x600 one, under compositor control, which is exactly the
    // stale-bounds condition the handler exists for.
    eprintln!("V20B {}", swaymsg(&["output", &big_out, "disable"]));
    pump_for(1500);
    eprintln!("V20B outputs now: {}", sway_outputs_summary());

    // A size request AFTER the move — the reference's "resize past the smaller
    // head's dimensions". Without one, GDK has no reason to re-run
    // `compute-size`, so the stale-bounds clamp would never even be attempted and
    // both arms would pass vacuously. The +1 makes the request differ from the
    // current default size, so GTK cannot short-circuit it.
    window.set_default_size(want_w + 1, want_h + 1);
    window.queue_resize();
    pump_for(3000);

    let surface = window.surface().expect("still realized");
    let (w1, h1) = (surface.width(), surface.height());
    eprintln!(
        "V20B after move to {small_out} ({small_w}x{small_h}) + re-request \
         {}x{}: surface={w1}x{h1}",
        want_w + 1,
        want_h + 1
    );

    match arm.as_str() {
        // The re-assertion must beat GTK's clamp: the window stays bigger than the
        // head it was just moved to.
        "exceeds" => {
            assert!(
                w0 > small_w || h0 > small_h,
                "arm `exceeds` is mis-provisioned: the pre-move surface {w0}x{h0} already \
                 fits the small head {small_w}x{small_h}, so no clamp could ever fire"
            );
            assert!(
                w1 > small_w || h1 > small_h,
                "CLAMP NOT DEFEATED: after moving to the {small_w}x{small_h} head the surface \
                 is {w1}x{h1}, i.e. GTK's clamp to the stale monitor bounds won. \
                 `toplevel_compute_size` did not re-assert the current size."
            );
        }
        // The contrasting arm: within bounds, the predicate returns false, the
        // handler is a no-op, and GTK's default sizing keeps the small window
        // small. It must NOT be inflated to the head.
        "fits" => {
            assert!(
                w0 <= small_w && h0 <= small_h,
                "arm `fits` is mis-provisioned: the pre-move surface {w0}x{h0} does not fit \
                 the small head {small_w}x{small_h}"
            );
            assert!(
                w1 <= small_w && h1 <= small_h,
                "the within-bounds window was INFLATED to {w1}x{h1} on a \
                 {small_w}x{small_h} head; the predicate should have been false"
            );
        }
        other => panic!("unknown RUSTTY_V20B_ARM {other:?}"),
    }
    eprintln!("V20B arm `{arm}` OK");
}
