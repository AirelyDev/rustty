//! CSI 8 t ON GTK — the live KAT for `219cca932` (`Window.resizeSurface`,
//! `Action.resizeWindow`), P7-W6 T2.
//!
//! WHAT THIS PROVES, through the real `performAction` dispatch with a
//! `.surface` target on a live terminal hosted in the real `GhosttyWindow`:
//!   (a) TILED: under sway's tiling the compositor owns the size, the toplevel
//!       reports `GDK_TOPLEVEL_STATE_TILED`, and `.resize_window` is refused
//!       (`false`) without touching the window.
//!   (b) FLOATING: once sway floats the window, `.resize_window` for the
//!       current surface size plus 100x60 points is performed (`true`) and the
//!       surface ends up that size. The resize goes through `setDefaultSize`
//!       and the `resize_requested` one-shot in `toplevelComputeSize`.
//!   (b2) UD374: the same at a sway output scale of 1.25, where GTK's integer
//!       scale factor is 2; the surface still ends up the requested size.
//!   (c) UD372: a request of `u32::MAX` x `u32::MAX` points, which panics the
//!       reference's `resizeDelta` (`@intFromFloat` to `c_int`), is clamped: the
//!       action is performed, nothing panics, and the window stays within the
//!       head.
//!   (d) the `resize_requested` one-shot: shrinking the window that now fills
//!       the head works. NEGATIVE CONTROL (rvm, 2026-09-30): with the one-shot
//!       disabled in `toplevel_compute_size`, the compute-size re-assertion
//!       wins and the surface stays 1280x631; the test fails.
//!
//! Run through the canonical launcher (a TILING headless sway):
//!   .port/tools/gtk_live_headless_sway.sh cargo test -p rustty-apprt \
//!     --features gtk --test gtk_resize_window_live -- --ignored --nocapture
//!
//! R5: gated `all(any(linux, freebsd), feature = "gtk")`.
#![cfg(all(any(target_os = "linux", target_os = "freebsd"), feature = "gtk"))]

use std::time::{Duration, Instant};

use allocator_api2::alloc::Global;
use gio::prelude::*;
use gtk4::prelude::*;

use rustty_apprt::gtk::class::application::Application;
use rustty_apprt::gtk::class::overrides::TabOverrides;
use rustty_apprt::gtk::class::surface::{CoreSurface, Surface};
use rustty_apprt::gtk::class::window::Window;
use rustty_apprt::gtk::{self, register_resources};
use rustty_apprt_api::action::ResizeWindow;
use rustty_apprt_api::{Action, Target};
use rustty_config::command::Command;
use rustty_core::app::App as CoreApp;

#[path = "common/gtk_live.rs"]
mod gtk_live;

/// The window and tab title sway's criteria select the window by.
const TITLE: &str = "rustty-resize-window-live";

fn swaymsg(cmd: &str) -> String {
    let out = std::process::Command::new("swaymsg")
        .arg(cmd)
        .output()
        .expect("swaymsg must be on PATH (run via gtk_live_headless_sway.sh)");
    format!(
        "swaymsg {cmd:?} -> rc={:?} {}{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout).trim(),
        String::from_utf8_lossy(&out.stderr).trim()
    )
}

fn toplevel_tiled(window: &Window) -> bool {
    window
        .surface()
        .and_then(|s| s.downcast::<gtk4::gdk::Toplevel>().ok())
        .is_some_and(|t| t.state().contains(gtk4::gdk::ToplevelState::TILED))
}

/// Dispatch `.resize_window` with a `.surface` target, as core's
/// `Surface.resizeWindow` does (`rt_app.performAction(.{ .surface = self }, ...)`).
fn resize_window(
    application: &Application,
    core: *mut CoreSurface,
    width: u32,
    height: u32,
) -> bool {
    // SAFETY: `core` is the live core surface of the window's only terminal; the
    // dispatch runs synchronously on the main thread.
    let target = unsafe { Target::surface_ptr(core) };
    application
        .perform_action(target, Action::ResizeWindow(ResizeWindow { width, height }))
        .expect("resize_window dispatch does not error")
}

#[test]
#[ignore = "headed GTK + a live headless sway; run via \
            .port/tools/gtk_live_headless_sway.sh cargo test -p rustty-apprt \
            --features gtk --test gtk_resize_window_live -- --ignored"]
fn resize_window_refuses_tiled_resizes_floating_and_clamps_ud372() {
    // ---- The real core/apprt App pair, as `gtk_window_live.rs` stands it up. ----
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
    register_resources();

    let window = Window::new(&application, Some(TITLE));
    application
        .bind_property("config", &window, "config")
        .build();
    window.new_tab_for_window(
        None,
        TabOverrides {
            command: Some(Command::Direct(vec![b"/bin/sh".to_vec()])),
            shell_integration: None,
            working_directory: None,
            title: Some(TITLE.to_string()),
        },
    );
    let surface: Surface = window
        .get_active_surface()
        .expect("the window hosts an active Surface");
    window.present();

    let render_threads = gtk_live::RenderThreadGuard::new(core_ptr, [&surface]);
    let ctx = glib::MainContext::default();
    let keepalive =
        glib::timeout_add_local(Duration::from_millis(50), || glib::ControlFlow::Continue);
    let pump_until = |secs: u64, pred: &dyn Fn() -> bool| -> bool {
        let deadline = Instant::now() + Duration::from_secs(secs);
        while Instant::now() < deadline {
            gtk_live::iterate(&ctx, core_ptr, &app);
            if pred() {
                return true;
            }
        }
        pred()
    };
    let pump_for = |ms: u64| {
        let deadline = Instant::now() + Duration::from_millis(ms);
        while Instant::now() < deadline {
            gtk_live::iterate(&ctx, core_ptr, &app);
        }
    };

    assert!(
        pump_until(30, &|| surface.core().is_some()),
        "the core surface was never built (is LIBGL_ALWAYS_SOFTWARE=1 set?)"
    );
    let core = surface.core().expect("core surface");
    pump_for(1000);

    // ---- (a) Tiled: refused, nothing changes. ----
    assert!(
        pump_until(10, &|| toplevel_tiled(&window)),
        "sway never reported the window tiled; is this the TILING launcher?"
    );
    let before = surface.get_size();
    let default_before = window.default_size();
    let tiled = resize_window(&application, core, 100, 100);
    eprintln!("RESIZE tiled: size={before:?} -> performed={tiled}");
    assert!(!tiled, "a tiled window is sized by the compositor: refused");
    assert_eq!(
        window.default_size(),
        default_before,
        "a refused resize leaves the default size alone"
    );

    // ---- (b) Floating: the surface becomes the requested size. ----
    eprintln!(
        "RESIZE {}",
        swaymsg(&format!("[title=\"^{TITLE}$\"] floating enable"))
    );
    assert!(
        pump_until(10, &|| !toplevel_tiled(&window)),
        "the window is still tiled after `floating enable`"
    );
    pump_for(1000);
    let scale = surface.get_content_scale();
    let cur = surface.get_size();
    let want_w = (cur.width as f32 / scale.x).round() as u32 + 100;
    let want_h = (cur.height as f32 / scale.y).round() as u32 + 60;
    let expect_w = (want_w as f32 * scale.x).round() as u32;
    let expect_h = (want_h as f32 * scale.y).round() as u32;
    let performed = resize_window(&application, core, want_w, want_h);
    eprintln!(
        "RESIZE floating: size={cur:?} scale={scale:?} request={want_w}x{want_h} pt \
         -> performed={performed}"
    );
    assert!(performed, "a lone floating terminal resizes");
    let reached = pump_until(10, &|| {
        let s = surface.get_size();
        s.width.abs_diff(expect_w) <= 1 && s.height.abs_diff(expect_h) <= 1
    });
    let after = surface.get_size();
    eprintln!("RESIZE floating: now {after:?}, expected {expect_w}x{expect_h}");
    assert!(
        reached,
        "the surface is {after:?}, not the requested {expect_w}x{expect_h} device px"
    );

    // ---- (b2) UD374: the same at a FRACTIONAL output scale (1.25). The
    //      reference divides the device-pixel change by the integer
    //      `getScaleFactor()` (2 at 1.25), so the window moved by 1.25/2 of
    //      the request. The window no longer fits the smaller logical head
    //      (1024x576 at 1.25), so the compositor caps its height; the shrink
    //      is sized so that the requested window fits again. NEGATIVE
    //      CONTROL (rvm, 2026-09-30, the reference's integer factor): from a
    //      1125x713 surface a 820x450 pt request left it 1063x608, not
    //      1025x563 (the width moved 62 of 100 device px). ----
    eprintln!("RESIZE {}", swaymsg("output * scale 1.25"));
    let surface_scale =
        || rustty_apprt::gtk::scale::widget_surface_scale(window.upcast_ref::<gtk4::Widget>());
    assert!(
        pump_until(10, &|| (surface_scale() - 1.25).abs() < 1e-6),
        "the window never reached surface scale 1.25 (now {})",
        surface_scale()
    );
    // Let the render surface and the content scale follow the new scale.
    pump_for(2000);
    let scale = surface.get_content_scale();
    let cur = surface.get_size();
    let want_w = (cur.width as f32 / scale.x).round() as u32 - 80;
    let want_h = (cur.height as f32 / scale.y).round() as u32 - 120;
    let expect_w = (want_w as f32 * scale.x).round() as u32;
    let expect_h = (want_h as f32 * scale.y).round() as u32;
    let performed = resize_window(&application, core, want_w, want_h);
    eprintln!(
        "RESIZE 1.25: size={cur:?} scale={scale:?} surface_scale={} \
         scale_factor={} request={want_w}x{want_h} pt -> performed={performed}",
        surface_scale(),
        window.scale_factor()
    );
    assert!(performed, "a lone floating terminal resizes at 1.25");
    let reached = pump_until(10, &|| {
        let s = surface.get_size();
        s.width.abs_diff(expect_w) <= 2 && s.height.abs_diff(expect_h) <= 2
    });
    let after = surface.get_size();
    eprintln!("RESIZE 1.25: now {after:?}, expected {expect_w}x{expect_h}");
    assert!(
        reached,
        "at scale 1.25 the surface is {after:?}, not the requested \
         {expect_w}x{expect_h} device px (from {cur:?})"
    );
    eprintln!("RESIZE {}", swaymsg("output * scale 1"));
    assert!(
        pump_until(10, &|| (surface_scale() - 1.0).abs() < 1e-6),
        "the window never returned to surface scale 1"
    );
    pump_for(1000);

    // ---- (c) UD372: the largest request is clamped, not a panic. ----
    let head = {
        let display = gtk4::gdk::Display::default().expect("display");
        let monitor = display
            .monitors()
            .item(0)
            .and_then(|m| m.downcast::<gtk4::gdk::Monitor>().ok())
            .expect("a monitor");
        monitor.geometry()
    };
    let huge = resize_window(&application, core, u32::MAX, u32::MAX);
    eprintln!(
        "RESIZE u32::MAX: performed={huge} default_size={:?}",
        window.default_size()
    );
    assert!(
        huge,
        "the oversize request is performed (clamped), not refused"
    );
    assert_eq!(
        window.default_size(),
        (i32::MAX, i32::MAX),
        "both deltas saturated to i32::MAX"
    );
    pump_for(3000);
    // The window widget's size, not the GdkSurface's: with client-side
    // decorations the surface also holds the shadow margins (on rvm, 25 px a
    // side: a 1280x720 window is a 1330x770 surface).
    let gdk_surface = window.surface().expect("still realized");
    let (win_w, win_h) = (window.width(), window.height());
    eprintln!(
        "RESIZE u32::MAX: window {win_w}x{win_h} (GdkSurface {}x{}), head {}x{}",
        gdk_surface.width(),
        gdk_surface.height(),
        head.width(),
        head.height()
    );
    assert!(
        win_w <= head.width() && win_h <= head.height(),
        "the window grew past the head"
    );

    // ---- (d) Shrinking a window that fills the head. Its surface (with the
    //      shadow) now exceeds the compositor bounds, so `toplevelComputeSize`
    //      would re-assert the current size; the `resize_requested` one-shot
    //      makes it step aside. ----
    let cur = surface.get_size();
    let scale = surface.get_content_scale();
    // UD374: the fractional surface scale, not the integer `scale_factor()`.
    let factor = surface_scale() as f32;
    let (old_w, old_h) = (window.width(), window.height());
    // `resizeDelta`'s arithmetic, recomputed here from the same inputs, so the
    // default size the port set is checked to the pixel.
    let dw = ((400.0 * scale.x - cur.width as f32) / factor).round() as i32;
    let dh = ((300.0 * scale.y - cur.height as f32) / factor).round() as i32;
    let performed = resize_window(&application, core, 400, 300);
    assert!(performed, "the shrink request is performed");
    assert_eq!(
        window.default_size(),
        (old_w + dw, old_h + dh),
        "setDefaultSize(widget width + dw, widget height + dh) from a \
         {old_w}x{old_h} window and a {cur:?} surface"
    );
    // The width is exact. The height may fall short by the chrome drift only:
    // the reference resizes the window by the surface's size change, which
    // assumes the headerbar and tab bar keep their height. At 400 px wide they
    // grow a little (on rvm the surface came out 400x294 for a 400x389
    // window), a property of 219cca932's delta method, not of the port. So
    // the shortfall is one-sided and bounded by the measured amount.
    let shrunk = pump_until(10, &|| {
        let s = surface.get_size();
        s.width.abs_diff(400) <= 1 && (0..=10).contains(&(300 - i64::from(s.height)))
    });
    let after = surface.get_size();
    eprintln!(
        "RESIZE shrink from full head: window {old_w}x{old_h} -> default {:?} \
         (dw {dw}, dh {dh}); surface {cur:?} -> {after:?}, expected 400x300 \
         (height up to 10 short)",
        window.default_size()
    );
    assert!(
        shrunk,
        "the screen-filling window did not shrink: surface {after:?} (the \
         compute-size re-assertion won over the requested resize)"
    );

    // ---- Teardown, as `gtk_window_live.rs`. ----
    // SAFETY: `core` is the live core surface; its shell was started by init.
    let shell_pid = unsafe { gtk_live::shell_pid(core) };
    keepalive.remove();
    gtk_live::close_surface(&ctx, window, surface, shell_pid);
    gtk_live::close_other_windows(&ctx);
    drop(render_threads);
    app.terminate();
    core_app.destroy();
}
